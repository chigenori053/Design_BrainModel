use std::collections::BTreeMap;

use shm::RuleId;

#[derive(Clone, Debug, PartialEq)]
pub struct CausalEdge {
    pub from_rule: RuleId,
    pub to_rule: RuleId,
    pub strength: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Chm {
    pub rule_graph: BTreeMap<RuleId, Vec<CausalEdge>>,
}

impl Chm {
    pub fn new(rule_graph: BTreeMap<RuleId, Vec<CausalEdge>>) -> Self {
        Self { rule_graph }
    }

    pub fn insert_edge(&mut self, from_rule: RuleId, to_rule: RuleId, strength: f64) {
        if from_rule == to_rule {
            return;
        }

        let clamped = clamp_strength(strength);
        let edges = self.rule_graph.entry(from_rule).or_default();

        if let Some(edge) = edges.iter_mut().find(|edge| edge.to_rule == to_rule) {
            edge.strength = clamped;
            return;
        }

        edges.push(CausalEdge {
            from_rule,
            to_rule,
            strength: clamped,
        });
    }

    pub fn related_rules(&self, rule_id: RuleId) -> Vec<RuleId> {
        self.rule_graph
            .get(&rule_id)
            .map(|edges| edges.iter().map(|edge| edge.to_rule).collect())
            .unwrap_or_default()
    }

    pub fn update_strength(&mut self, from: RuleId, to: RuleId, delta: f64) {
        if from == to {
            return;
        }

        let edges = self.rule_graph.entry(from).or_default();
        if let Some(edge) = edges.iter_mut().find(|edge| edge.to_rule == to) {
            edge.strength = clamp_strength(edge.strength + delta);
            return;
        }

        edges.push(CausalEdge {
            from_rule: from,
            to_rule: to,
            strength: clamp_strength(delta),
        });
    }
}

fn clamp_strength(value: f64) -> f64 {
    value.clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use memory_space::Uuid;

    use crate::Chm;

    #[test]
    fn edge_insertion() {
        let mut chm = Chm::default();
        let r1 = Uuid::from_u128(1);
        let r2 = Uuid::from_u128(2);

        chm.insert_edge(r1, r2, 0.4);

        let edges = chm.rule_graph.get(&r1).expect("edge list must exist");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to_rule, r2);
        assert_eq!(edges[0].strength, 0.4);
    }

    #[test]
    fn strength_update_clamping() {
        let mut chm = Chm::default();
        let r1 = Uuid::from_u128(1);
        let r2 = Uuid::from_u128(2);

        chm.insert_edge(r1, r2, 0.8);
        chm.update_strength(r1, r2, 0.7);

        let edge = &chm.rule_graph.get(&r1).expect("edge list must exist")[0];
        assert_eq!(edge.strength, 1.0);

        chm.update_strength(r1, r2, -2.5);
        let edge = &chm.rule_graph.get(&r1).expect("edge list must exist")[0];
        assert_eq!(edge.strength, -1.0);
    }

    #[test]
    fn related_rule_lookup() {
        let mut chm = Chm::default();
        let r1 = Uuid::from_u128(1);
        let r2 = Uuid::from_u128(2);
        let r3 = Uuid::from_u128(3);

        chm.insert_edge(r1, r2, 0.2);
        chm.insert_edge(r1, r3, -0.1);

        let related = chm.related_rules(r1);
        assert_eq!(related, vec![r2, r3]);
    }
}
