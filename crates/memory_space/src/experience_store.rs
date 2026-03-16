use causal_domain::CausalGraph;
use design_domain::{Architecture, Layer};
use semantic_domain::MeaningGraph;
use architecture_ir::{
    ArchitectureIR, ComponentMetrics, ComponentType, ComponentUnit, DependencyEdge, DependencyType,
    NodeId, Visibility,
};

#[derive(Clone, Debug, PartialEq)]
pub struct DesignExperience {
    pub semantic_context: MeaningGraph,
    pub inferred_semantics: MeaningGraph,
    pub architecture: Architecture,
    pub architecture_hash: u64,
    pub causal_graph: CausalGraph,
    pub dependency_edges: Vec<(u64, u64)>,
    pub layer_sequence: Vec<Layer>,
    pub score: f64,
    pub search_depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExperienceStore {
    high_score_threshold: f64,
    experiences: Vec<DesignExperience>,
}

impl ExperienceStore {
    pub fn new(high_score_threshold: f64) -> Self {
        Self {
            high_score_threshold,
            experiences: Vec::new(),
        }
    }

    pub fn update_experience(&mut self, experience: DesignExperience) -> bool {
        if experience.score < self.high_score_threshold {
            return false;
        }
        self.experiences.push(experience);
        self.experiences.sort_by(|lhs, rhs| {
            rhs.score
                .total_cmp(&lhs.score)
                .then_with(|| lhs.architecture_hash.cmp(&rhs.architecture_hash))
        });
        true
    }

    pub fn experiences(&self) -> &[DesignExperience] {
        &self.experiences
    }
}

impl Default for ExperienceStore {
    fn default() -> Self {
        Self::new(0.65)
    }
}

impl DesignExperience {
    pub fn to_architecture_ir(&self) -> ArchitectureIR {
        let mut ir = ArchitectureIR::default();
        for layer in &self.layer_sequence {
            let id = ir.layers.len() as u64 + 1;
            ir.layers.push(architecture_ir::Layer {
                id,
                name: layer.as_str().to_string(),
                level: (self.layer_sequence.len() - ir.layers.len() + 1) as u32,
                components: Vec::new(),
                allowed_dependencies: Vec::new(),
            });
        }

        let units = self.architecture.design_units_by_id();
        for unit in units.values() {
            let component_id = unit.id.0;
            let layer_name = unit.layer.as_str();
            let layer_id = ir
                .layers
                .iter()
                .find(|layer| layer.name == layer_name)
                .map(|layer| layer.id);
            ir.components.push(ComponentUnit {
                id: component_id,
                name: unit.name.clone(),
                component_type: component_type_for_layer(unit.layer),
                layer: layer_id,
                interfaces: Vec::new(),
                properties: Vec::new(),
                structures: Vec::new(),
                visibility: Visibility::Public,
                metrics: ComponentMetrics::default(),
            });
            if let Some(layer_id) = layer_id {
                if let Some(layer) = ir.layers.iter_mut().find(|layer| layer.id == layer_id) {
                    layer.components.push(component_id);
                }
            }
        }

        for (from, to) in &self.dependency_edges {
            ir.dependencies.push(DependencyEdge {
                source: NodeId::Component(*from),
                target: NodeId::Component(*to),
                dependency_type: DependencyType::Use,
                interface: None,
            });
        }

        ir
    }
}

fn component_type_for_layer(layer: Layer) -> ComponentType {
    match layer {
        Layer::Ui => ComponentType::Controller,
        Layer::Service => ComponentType::Service,
        Layer::Repository | Layer::Database => ComponentType::Repository,
    }
}
