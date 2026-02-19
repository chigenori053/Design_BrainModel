use std::collections::BTreeSet;

use language_dhm::EMBEDDING_DIM;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub struct MeaningStructure {
    pub root: NodeId,
    pub nodes: Vec<MeaningNode>,
    pub edges: Vec<MeaningEdge>,
    pub abstraction_score: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeaningNode {
    pub id: NodeId,
    pub role: RoleType,
    pub token_span: (usize, usize),
    pub semantic_vector: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleType {
    Subject,
    Action,
    Object,
    Modifier,
    Constraint,
    Condition,
    Abstraction,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeaningEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub relation: RelationType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationType {
    AgentOf,
    ActsOn,
    Modifies,
    Causes,
    DependsOn,
    IsAbstractOf,
}

#[derive(Default)]
pub struct MeaningExtractor;

impl MeaningExtractor {
    pub fn extract(&self, text: &str, embedding: &[f32]) -> MeaningStructure {
        let tokens = tokenize(text);
        if tokens.is_empty() {
            return MeaningStructure {
                root: NodeId(1),
                nodes: vec![MeaningNode {
                    id: NodeId(1),
                    role: RoleType::Abstraction,
                    token_span: (0, 0),
                    semantic_vector: Vec::new(),
                }],
                edges: Vec::new(),
                abstraction_score: 0.0,
            };
        }

        let mut roles = infer_roles(&tokens);
        promote_subject_object_roles(&mut roles);

        let abstraction_score = compute_abstraction_score(&tokens);

        let mut nodes = build_nodes(&tokens, &roles, embedding);
        let mut edges = build_edges(&tokens, &roles, &nodes);

        let root = select_root(&nodes);
        ensure_no_isolated_nodes(root, &nodes, &mut edges);

        nodes.sort_by(|l, r| l.id.cmp(&r.id));
        edges.sort_by(|l, r| {
            l.from
                .cmp(&r.from)
                .then_with(|| l.to.cmp(&r.to))
                .then_with(|| relation_rank(l.relation).cmp(&relation_rank(r.relation)))
        });

        MeaningStructure {
            root,
            nodes,
            edges,
            abstraction_score,
        }
    }
}

#[derive(Clone, Debug)]
struct Token {
    surface: String,
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut buf = String::new();

    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' || is_japanese(ch) {
            buf.push(ch);
            continue;
        }
        if !buf.is_empty() {
            out.push(Token {
                surface: std::mem::take(&mut buf),
            });
        }
    }
    if !buf.is_empty() {
        out.push(Token { surface: buf });
    }
    out
}

fn is_japanese(ch: char) -> bool {
    ('\u{3040}'..='\u{30ff}').contains(&ch) || ('\u{4e00}'..='\u{9faf}').contains(&ch)
}

fn infer_roles(tokens: &[Token]) -> Vec<RoleType> {
    tokens
        .iter()
        .map(|t| {
            let w = t.surface.to_ascii_lowercase();
            if is_condition(&w) {
                RoleType::Condition
            } else if is_constraint(&w) {
                RoleType::Constraint
            } else if is_action(&w) {
                RoleType::Action
            } else if is_modifier(&w) {
                RoleType::Modifier
            } else if is_abstract_word(&w) {
                RoleType::Abstraction
            } else {
                RoleType::Object
            }
        })
        .collect()
}

fn promote_subject_object_roles(roles: &mut [RoleType]) {
    let action_idx = roles.iter().position(|r| *r == RoleType::Action);
    for (idx, role) in roles.iter_mut().enumerate() {
        if *role != RoleType::Object {
            continue;
        }
        if let Some(a_idx) = action_idx {
            if idx < a_idx {
                *role = RoleType::Subject;
            } else {
                *role = RoleType::Object;
            }
        } else {
            *role = RoleType::Subject;
        }
    }
}

fn build_nodes(tokens: &[Token], roles: &[RoleType], embedding: &[f32]) -> Vec<MeaningNode> {
    let mut nodes: Vec<MeaningNode> = Vec::new();
    let mut next_id = 1u64;

    let mut idx = 0usize;
    while idx < tokens.len() {
        let role = roles[idx];
        let span = if role == RoleType::Object
            && idx > 0
            && roles[idx - 1] == RoleType::Modifier
            && !nodes
                .iter()
                .any(|n| n.token_span == (idx - 1, idx))
        {
            (idx - 1, idx + 1)
        } else {
            (idx, idx + 1)
        };

        nodes.push(MeaningNode {
            id: NodeId(next_id),
            role,
            token_span: span,
            semantic_vector: Vec::new(),
        });
        next_id = next_id.saturating_add(1);
        idx += 1;
    }

    assign_semantic_vectors(&mut nodes, embedding);
    nodes
}

fn assign_semantic_vectors(nodes: &mut [MeaningNode], embedding: &[f32]) {
    if nodes.is_empty() {
        return;
    }
    let dim = if embedding.is_empty() {
        EMBEDDING_DIM
    } else {
        embedding.len()
    };
    let chunk = (dim / nodes.len()).max(1);

    let n_nodes = nodes.len();
    for (i, node) in nodes.iter_mut().enumerate() {
        let start = i.saturating_mul(chunk).min(dim);
        let end = if i == n_nodes - 1 {
            dim
        } else {
            (start + chunk).min(dim)
        };
        let raw = if embedding.is_empty() {
            vec![0.0; end.saturating_sub(start)]
        } else {
            embedding[start..end].to_vec()
        };
        node.semantic_vector = normalize_l2(&raw);
    }
}

fn build_edges(tokens: &[Token], roles: &[RoleType], nodes: &[MeaningNode]) -> Vec<MeaningEdge> {
    let mut edges = Vec::new();

    let subject = nodes.iter().find(|n| n.role == RoleType::Subject).map(|n| n.id);
    let action = nodes.iter().find(|n| n.role == RoleType::Action).map(|n| n.id);
    let object = nodes.iter().find(|n| n.role == RoleType::Object).map(|n| n.id);

    if let (Some(s), Some(a)) = (subject, action) {
        edges.push(MeaningEdge {
            from: s,
            to: a,
            relation: RelationType::AgentOf,
        });
    }
    if let (Some(a), Some(o)) = (action, object) {
        edges.push(MeaningEdge {
            from: a,
            to: o,
            relation: RelationType::ActsOn,
        });
    }

    for node in nodes.iter().filter(|n| n.role == RoleType::Modifier) {
        if let Some(target) = nearest_right_by_roles(node, nodes, &[RoleType::Object, RoleType::Subject]) {
            edges.push(MeaningEdge {
                from: node.id,
                to: target.id,
                relation: RelationType::Modifies,
            });
        }
    }

    for node in nodes.iter().filter(|n| n.role == RoleType::Condition) {
        if let Some(target) = action.and_then(|id| nodes.iter().find(|n| n.id == id)) {
            edges.push(MeaningEdge {
                from: node.id,
                to: target.id,
                relation: RelationType::DependsOn,
            });
        }
    }

    for node in nodes.iter().filter(|n| n.role == RoleType::Abstraction) {
        let target = nodes
            .iter()
            .find(|n| matches!(n.role, RoleType::Subject | RoleType::Object | RoleType::Action));
        if let Some(target) = target {
            edges.push(MeaningEdge {
                from: node.id,
                to: target.id,
                relation: RelationType::IsAbstractOf,
            });
        }
    }

    for i in 0..tokens.len().saturating_sub(1) {
        if roles[i] == RoleType::Subject && roles[i + 1] == RoleType::Subject {
            let from = nodes.get(i).map(|n| n.id);
            let to = nodes.get(i + 1).map(|n| n.id);
            if let (Some(from), Some(to)) = (from, to) {
                edges.push(MeaningEdge {
                    from,
                    to,
                    relation: RelationType::DependsOn,
                });
            }
        }
    }

    dedup_edges(edges)
}

fn dedup_edges(edges: Vec<MeaningEdge>) -> Vec<MeaningEdge> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for e in edges {
        let key = (e.from, e.to, relation_rank(e.relation));
        if seen.insert(key) {
            out.push(e);
        }
    }
    out
}

fn relation_rank(r: RelationType) -> u8 {
    match r {
        RelationType::AgentOf => 1,
        RelationType::ActsOn => 2,
        RelationType::Modifies => 3,
        RelationType::Causes => 4,
        RelationType::DependsOn => 5,
        RelationType::IsAbstractOf => 6,
    }
}

fn nearest_right_by_roles<'a>(
    base: &MeaningNode,
    nodes: &'a [MeaningNode],
    roles: &[RoleType],
) -> Option<&'a MeaningNode> {
    nodes
        .iter()
        .filter(|n| n.token_span.0 >= base.token_span.1 && roles.contains(&n.role))
        .min_by(|l, r| {
            l.token_span
                .0
                .cmp(&r.token_span.0)
                .then_with(|| l.id.cmp(&r.id))
        })
}

fn select_root(nodes: &[MeaningNode]) -> NodeId {
    nodes
        .iter()
        .find(|n| n.role == RoleType::Action)
        .or_else(|| nodes.first())
        .map(|n| n.id)
        .unwrap_or(NodeId(1))
}

fn ensure_no_isolated_nodes(root: NodeId, nodes: &[MeaningNode], edges: &mut Vec<MeaningEdge>) {
    let mut incident = BTreeSet::new();
    for e in edges.iter() {
        incident.insert(e.from);
        incident.insert(e.to);
    }

    for node in nodes {
        if node.id == root {
            continue;
        }
        if !incident.contains(&node.id) {
            edges.push(MeaningEdge {
                from: root,
                to: node.id,
                relation: RelationType::DependsOn,
            });
        }
    }

    if nodes.len() > 1 && edges.is_empty() {
        for node in nodes.iter().filter(|n| n.id != root) {
            edges.push(MeaningEdge {
                from: root,
                to: node.id,
                relation: RelationType::DependsOn,
            });
        }
    }
}

fn compute_abstraction_score(tokens: &[Token]) -> f32 {
    if tokens.is_empty() {
        return 0.0;
    }
    let abs = tokens
        .iter()
        .filter(|t| is_abstract_word(&t.surface.to_ascii_lowercase()))
        .count();
    abs as f32 / tokens.len() as f32
}

fn is_condition(w: &str) -> bool {
    matches!(w, "if" | "when" | "unless" | "while" | "because" | "ifthen" | "もし" | "なら")
}

fn is_constraint(w: &str) -> bool {
    matches!(w, "must" | "should" | "shall" | "limit" | "constraint" | "制約" | "必須")
}

fn is_action(w: &str) -> bool {
    matches!(
        w,
        "is" | "are" | "be" | "improves" | "improve" | "optimize" | "optimizes" | "design" | "build"
    ) || w.ends_with("ed")
        || w.ends_with("ize")
        || w.ends_with("izes")
        || w.ends_with("ise")
        || w.ends_with("ises")
}

fn is_modifier(w: &str) -> bool {
    matches!(w, "structural" | "semantic" | "causal" | "dynamic" | "static")
        || w.ends_with("al")
        || w.ends_with("ive")
        || w.ends_with("ous")
}

fn is_abstract_word(w: &str) -> bool {
    matches!(
        w,
        "構造"
            | "責務"
            | "設計"
            | "最適化"
            | "structure"
            | "responsibility"
            | "design"
            | "optimization"
    )
}

fn normalize_l2(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| x / norm).collect()
}

#[cfg(test)]
mod tests {
    use super::{MeaningExtractor, RelationType, RoleType};

    #[test]
    fn basic_sentence_test() {
        let text = "DesignBrainModel improves structural reasoning.";
        let embedding = vec![1.0f32; 384];
        let extractor = MeaningExtractor;
        let structure = extractor.extract(text, &embedding);

        assert!(structure.nodes.iter().any(|n| n.role == RoleType::Subject));
        assert!(structure.nodes.iter().any(|n| n.role == RoleType::Action));
        assert!(structure.nodes.iter().any(|n| n.role == RoleType::Object));

        assert!(
            structure
                .edges
                .iter()
                .any(|e| e.relation == RelationType::AgentOf)
        );
        assert!(
            structure
                .edges
                .iter()
                .any(|e| e.relation == RelationType::ActsOn)
        );
    }

    #[test]
    fn abstraction_score_test() {
        let text = "構造 設計 最適化 structure design optimization";
        let embedding = vec![0.5f32; 384];
        let extractor = MeaningExtractor;
        let structure = extractor.extract(text, &embedding);
        assert!(structure.abstraction_score > 0.5);
    }

    #[test]
    fn edge_integrity_test() {
        let text = "DesignBrainModel improves structural reasoning";
        let embedding = vec![0.3f32; 384];
        let extractor = MeaningExtractor;
        let structure = extractor.extract(text, &embedding);

        assert!(!structure.nodes.is_empty());
        assert!(structure.nodes.iter().any(|n| n.id == structure.root));

        for node in &structure.nodes {
            if node.id == structure.root {
                continue;
            }
            let connected = structure
                .edges
                .iter()
                .any(|e| e.from == node.id || e.to == node.id);
            assert!(connected);
        }
    }
}
