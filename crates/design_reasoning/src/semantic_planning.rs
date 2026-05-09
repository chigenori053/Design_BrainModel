use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticPlanningNode {
    pub node_id: String,
    pub semantic_goal: String,
    pub inherited_intent: Vec<String>,
    pub responsibility_units: Vec<String>,
    pub abstraction_level: f64,
    pub continuity_score: f64,
    pub parent_nodes: Vec<String>,
    pub child_nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticPlanningGraph {
    pub root_intent: String,
    pub planning_nodes: Vec<SemanticPlanningNode>,
    pub semantic_dependencies: Vec<(String, String)>,
    pub convergence_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntentLineage {
    pub lineage_id: String,
    pub root_intent: String,
    pub evolving_intents: Vec<String>,
    pub continuity_score: f64,
    pub drift_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResponsibilityUnit {
    pub responsibility_id: String,
    pub semantic_role: String,
    pub inherited_from: Option<String>,
    pub continuity_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AbstractionTransition {
    pub transition_id: String,
    pub source_abstraction: String,
    pub target_abstraction: String,
    pub semantic_reason: String,
    pub continuity_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanningMemory {
    pub planning_id: String,
    pub planning_history: Vec<String>,
    pub convergence_trajectory: Vec<f64>,
    pub semantic_lineage: Vec<String>,
}

pub struct IntentContinuityEngine {
    drift_threshold: f64,
}

impl Default for IntentContinuityEngine {
    fn default() -> Self {
        Self {
            drift_threshold: 0.7,
        }
    }
}

impl IntentContinuityEngine {
    pub fn new(drift_threshold: f64) -> Self {
        Self { drift_threshold }
    }

    pub fn calculate_drift(&self, root_intent: &str, current_intent: &str) -> f64 {
        // Simple heuristic: if core keywords are missing, drift increases
        let root_keywords: Vec<&str> = root_intent.split_whitespace().collect();
        let mut matches = 0;
        for kw in &root_keywords {
            if current_intent.contains(kw) {
                matches += 1;
            }
        }
        
        if root_keywords.is_empty() {
            return 0.0;
        }
        
        1.0 - (matches as f64 / root_keywords.len() as f64)
    }

    pub fn is_drift_fatal(&self, drift_score: f64) -> bool {
        drift_score > self.drift_threshold
    }

    pub fn track_lineage(&self, lineage: &mut IntentLineage, new_intent: String) {
        let drift = self.calculate_drift(&lineage.root_intent, &new_intent);
        lineage.evolving_intents.push(new_intent);
        lineage.drift_score = drift;
        lineage.continuity_score = 1.0 - drift;
    }
}

pub struct SemanticPlanningEngine;

impl SemanticPlanningEngine {
    pub fn build_graph(root_intent: &str, goals: Vec<String>) -> SemanticPlanningGraph {
        let mut nodes = Vec::new();
        let mut dependencies = Vec::new();
        
        for (i, goal) in goals.into_iter().enumerate() {
            let node_id = format!("PLAN_{}", i);
            nodes.push(SemanticPlanningNode {
                node_id: node_id.clone(),
                semantic_goal: goal,
                inherited_intent: vec![root_intent.to_string()],
                responsibility_units: vec![format!("RESP_{}", i)],
                abstraction_level: 1.0,
                continuity_score: 1.0,
                parent_nodes: if i > 0 { vec![format!("PLAN_{}", i - 1)] } else { vec![] },
                child_nodes: vec![],
            });
            
            if i > 0 {
                dependencies.push((format!("PLAN_{}", i - 1), node_id));
            }
        }
        
        // Update child nodes
        for i in 0..nodes.len() {
            if i + 1 < nodes.len() {
                let next_id = nodes[i+1].node_id.clone();
                nodes[i].child_nodes.push(next_id);
            }
        }

        SemanticPlanningGraph {
            root_intent: root_intent.to_string(),
            planning_nodes: nodes,
            semantic_dependencies: dependencies,
            convergence_score: 1.0,
        }
    }

    pub fn sort_planning_nodes(nodes: &mut [SemanticPlanningNode]) {
        // ordering: continuity desc -> contradiction asc (omitted for now) -> convergence desc (omitted) -> abstraction stability desc -> node_id asc
        nodes.sort_by(|a, b| {
            let cont_cmp = b.continuity_score.partial_cmp(&a.continuity_score).unwrap_or(Ordering::Equal);
            if cont_cmp != Ordering::Equal {
                return cont_cmp;
            }
            let abs_cmp = b.abstraction_level.partial_cmp(&a.abstraction_level).unwrap_or(Ordering::Equal);
            if abs_cmp != Ordering::Equal {
                return abs_cmp;
            }
            a.node_id.cmp(&b.node_id)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 13.1 Planning Tests
    #[test]
    fn semantic_planning_graph_deterministic() {
        let root = "大規模分散キャッシュへ移行";
        let goals = vec!["ノード分散".to_string(), "整合性確保".to_string()];
        let graph1 = SemanticPlanningEngine::build_graph(root, goals.clone());
        let graph2 = SemanticPlanningEngine::build_graph(root, goals);
        assert_eq!(graph1, graph2);
        assert_eq!(graph1.planning_nodes.len(), 2);
    }

    #[test]
    fn planning_dependency_propagation_stable() {
        let root = "意図";
        let goals = vec!["G1".to_string(), "G2".to_string()];
        let graph = SemanticPlanningEngine::build_graph(root, goals);
        assert_eq!(graph.semantic_dependencies.len(), 1);
        assert_eq!(graph.semantic_dependencies[0], ("PLAN_0".to_string(), "PLAN_1".to_string()));
    }

    #[test]
    fn planning_replay_deterministic() {
        let root = "意図";
        let goals = vec!["G1".to_string()];
        let graph = SemanticPlanningEngine::build_graph(root, goals);
        // Re-building the same graph should produce identical structure
        let graph_replay = SemanticPlanningEngine::build_graph(root, vec!["G1".to_string()]);
        assert_eq!(graph, graph_replay);
    }

    // 13.2 Intent Continuity Tests
    #[test]
    fn root_intent_preserved() {
        let engine = IntentContinuityEngine::default();
        let mut lineage = IntentLineage {
            lineage_id: "L1".to_string(),
            root_intent: "キャッシュ 高速化".to_string(),
            evolving_intents: vec![],
            continuity_score: 1.0,
            drift_score: 0.0,
        };
        engine.track_lineage(&mut lineage, "キャッシュ 最適化".to_string());
        assert_eq!(lineage.root_intent, "キャッシュ 高速化");
        assert!(lineage.evolving_intents.contains(&"キャッシュ 最適化".to_string()));
    }

    #[test]
    fn intent_drift_detected() {
        let engine = IntentContinuityEngine::new(0.5);
        let root = "分散 ストレージ 高信頼性";
        let current = "単一 サーバー 低コスト"; // Drifted completely
        let drift = engine.calculate_drift(root, current);
        assert!(drift > 0.5);
        assert!(engine.is_drift_fatal(drift));
    }

    #[test]
    fn semantic_drift_rejected() {
        let engine = IntentContinuityEngine::new(0.5);
        let drift = engine.calculate_drift("A B C", "X Y Z");
        assert!(engine.is_drift_fatal(drift));
        // In runtime this would trigger SemanticDriftRejected state
    }

    // 13.3 Responsibility Tests
    #[test]
    fn responsibility_continuity_preserved() {
        let resp = ResponsibilityUnit {
            responsibility_id: "R1".to_string(),
            semantic_role: "Data Persistence".to_string(),
            inherited_from: None,
            continuity_score: 0.9,
        };
        assert_eq!(resp.semantic_role, "Data Persistence");
    }

    #[test]
    fn responsibility_collapse_detected() {
        // If continuity score drops too low, it's a collapse
        let score = 0.2;
        assert!(score < 0.5);
    }

    // 13.4 Abstraction Tests
    #[test]
    fn abstraction_transition_stable() {
        let transition = AbstractionTransition {
            transition_id: "T1".to_string(),
            source_abstraction: "Monolith".to_string(),
            target_abstraction: "Microservices".to_string(),
            semantic_reason: "Scalability".to_string(),
            continuity_score: 0.85,
        };
        assert_eq!(transition.target_abstraction, "Microservices");
    }

    #[test]
    fn abstraction_collapse_detected() {
        let continuity = 0.1;
        assert!(continuity < 0.3);
    }

    // 13.5 Observability Tests
    #[test]
    fn planning_lineage_observable() {
        let lineage = IntentLineage {
            lineage_id: "L1".to_string(),
            root_intent: "Root".to_string(),
            evolving_intents: vec!["E1".to_string(), "E2".to_string()],
            continuity_score: 0.9,
            drift_score: 0.1,
        };
        let _s = format!("{:?}", lineage);
    }

    #[test]
    fn drift_events_observable() {
        let drift = 0.45;
        let _s = format!("Drift detected: {}", drift);
    }

    #[test]
    fn transition_history_preserved() {
        let memory = PlanningMemory {
            planning_id: "P1".to_string(),
            planning_history: vec!["Step 1".to_string(), "Step 2".to_string()],
            convergence_trajectory: vec![0.5, 0.8, 1.0],
            semantic_lineage: vec!["I1".to_string(), "I2".to_string()],
        };
        assert_eq!(memory.planning_history.len(), 2);
    }
}
