use architecture_ir::{
    ArchitectureIR, ComponentMetrics, ComponentType, ComponentUnit, DependencyEdge, DependencyType,
    Layer, NodeId, Visibility,
};
use memory_space_phase14::{DesignIntentRecord, DesignMemorySpace, TemplateRecord, TopologyType};

use crate::intent::normalize_key;
use crate::template::{ArchitectureTemplate, TemplateSelection, Topology, builtin_templates};
use crate::{IntentModel, SearchSpace, SearchState};

#[derive(Clone, Debug, Default)]
pub struct ArchitectureTemplateEngine {
    templates: Vec<ArchitectureTemplate>,
}

impl ArchitectureTemplateEngine {
    pub fn with_builtin_library() -> Self {
        Self {
            templates: builtin_templates(),
        }
    }

    pub fn select_templates(&self, intent: &IntentModel) -> TemplateSelection {
        self.select_templates_internal(intent, None)
    }

    pub fn select_templates_with_memory(
        &self,
        intent: &IntentModel,
        memory: &DesignMemorySpace,
    ) -> TemplateSelection {
        self.select_templates_internal(intent, Some(memory))
    }

    fn select_templates_internal(
        &self,
        intent: &IntentModel,
        memory: Option<&DesignMemorySpace>,
    ) -> TemplateSelection {
        let recalled = memory
            .map(|memory| memory.recall_templates_for_intent(&intent_record(intent), 3))
            .unwrap_or_default();
        let mut templates = self.templates.clone();
        for record in recalled {
            if !templates
                .iter()
                .any(|template| template.template_id == record.template_id)
            {
                templates.push(template_from_record(&record));
            }
        }
        let mut scored = self
            .templates_for_scoring(templates)
            .iter()
            .cloned()
            .map(|template| {
                let score = template_match_score(intent, &template)
                    + memory_boost(memory, &template.template_id);
                (score, template)
            })
            .collect::<Vec<_>>();
        scored.sort_by(|(ls, lt), (rs, rt)| {
            rs.cmp(ls).then_with(|| lt.template_id.cmp(&rt.template_id))
        });

        let mut ordered = scored
            .into_iter()
            .map(|(_, template)| template)
            .collect::<Vec<_>>();
        let selected = ordered.first().cloned().unwrap_or_else(|| {
            builtin_templates()
                .into_iter()
                .next()
                .expect("template library")
        });
        let alternatives = ordered.drain(1..ordered.len().min(3)).collect::<Vec<_>>();
        TemplateSelection {
            selected,
            alternatives,
        }
    }

    fn templates_for_scoring(
        &self,
        templates: Vec<ArchitectureTemplate>,
    ) -> Vec<ArchitectureTemplate> {
        templates
    }

    pub fn expand_template(
        &self,
        template: &ArchitectureTemplate,
        space: &SearchSpace,
    ) -> SearchState {
        let mut state = SearchState::default();
        let mut ir = ArchitectureIR::default();
        ir.constraints.extend(template.constraints.clone());
        ir.constraints.extend(space.constraints.clone());

        for (index, layer) in template.layer_structure.iter().enumerate() {
            ir.layers.push(Layer {
                id: index as u64 + 1,
                name: layer.name.clone(),
                level: layer.level,
                components: Vec::new(),
                allowed_dependencies: Vec::new(),
            });
        }

        for slot in &template.component_slots {
            if slot.optional && !space.component_catalog.contains(&slot.slot_type) {
                continue;
            }
            let component_id = ir.components.len() as u64 + 1;
            let layer_id = ir
                .layers
                .iter()
                .find(|layer| normalize_key(&layer.name) == normalize_key(&slot.layer))
                .map(|layer| layer.id);
            ir.components.push(ComponentUnit {
                id: component_id,
                name: slot.slot_name.clone(),
                component_type: slot.slot_type.clone(),
                layer: layer_id,
                interfaces: Vec::new(),
                properties: vec![architecture_ir::ComponentProperty {
                    key: "template".to_string(),
                    value: template.template_id.clone(),
                }],
                structures: Vec::new(),
                visibility: Visibility::Public,
                metrics: ComponentMetrics::default(),
            });
            if let Some(layer_id) = layer_id
                && let Some(layer) = ir.layers.iter_mut().find(|layer| layer.id == layer_id)
            {
                layer.components.push(component_id);
            }
        }

        for rule in &template.dependency_rules {
            let sources = ir
                .components
                .iter()
                .filter(|component| component.component_type == rule.from)
                .map(|component| component.id)
                .collect::<Vec<_>>();
            let targets = ir
                .components
                .iter()
                .filter(|component| component.component_type == rule.to)
                .map(|component| component.id)
                .collect::<Vec<_>>();
            for source in &sources {
                for target in &targets {
                    if source != target {
                        ir.dependencies.push(DependencyEdge {
                            source: NodeId::Component(*source),
                            target: NodeId::Component(*target),
                            dependency_type: DependencyType::Use,
                            interface: None,
                        });
                    }
                }
            }
        }

        state.architecture = ir;
        state
    }

    pub fn mutate_template(
        &self,
        template: &ArchitectureTemplate,
        intent: &IntentModel,
    ) -> ArchitectureTemplate {
        let mut mutated = template.clone();
        if intent
            .requirements
            .iter()
            .any(|req| normalize_key(req).contains("cache"))
            && !mutated
                .component_slots
                .iter()
                .any(|slot| slot.slot_name == "CacheAdapter")
        {
            mutated
                .component_slots
                .push(crate::template::ComponentSlot {
                    layer: "Infrastructure".to_string(),
                    slot_name: "CacheAdapter".to_string(),
                    slot_type: ComponentType::Adapter,
                    optional: true,
                });
        }
        if intent
            .requirements
            .iter()
            .any(|req| normalize_key(req).contains("auth"))
            && mutated.topology == Topology::Layered
            && !mutated
                .component_slots
                .iter()
                .any(|slot| slot.slot_name == "AuthService")
        {
            mutated
                .component_slots
                .push(crate::template::ComponentSlot {
                    layer: "Application".to_string(),
                    slot_name: "AuthService".to_string(),
                    slot_type: ComponentType::Service,
                    optional: true,
                });
        }
        mutated
    }
}

fn template_match_score(intent: &IntentModel, template: &ArchitectureTemplate) -> u32 {
    let system = normalize_key(&intent.system_type);
    let mut score = template.ranking.historical_success + template.ranking.pattern_stability;
    score = score.saturating_sub(template.ranking.complexity);

    match template.topology {
        Topology::Layered if system.contains("api") || system.contains("web") => score += 10,
        Topology::Hexagonal if system.contains("api") || system.contains("domain") => score += 8,
        Topology::Pipeline if system.contains("pipeline") || system.contains("data") => score += 10,
        Topology::EventDriven if system.contains("event") || system.contains("stream") => {
            score += 9
        }
        Topology::Microservice if system.contains("microservice") || system.contains("saas") => {
            score += 8
        }
        _ => {}
    }

    for req in &intent.requirements {
        let req = normalize_key(req);
        if req.contains("cache")
            && matches!(template.topology, Topology::Layered | Topology::Hexagonal)
        {
            score += 2;
        }
        if req.contains("event") && template.topology == Topology::EventDriven {
            score += 3;
        }
    }
    score
}

fn memory_boost(memory: Option<&DesignMemorySpace>, template_id: &str) -> u32 {
    memory
        .and_then(|memory| memory.template_memory.get(template_id))
        .map(|record| {
            let score = record.metadata.average_score.max(0.0) * 10.0;
            let usage = record.metadata.usage_count.min(5) as f32;
            (score + usage) as u32
        })
        .unwrap_or(0)
}

fn intent_record(intent: &IntentModel) -> DesignIntentRecord {
    DesignIntentRecord {
        intent_id: format!(
            "{}:{}",
            normalize_key(&intent.system_type),
            intent
                .requirements
                .iter()
                .map(|req| normalize_key(req))
                .collect::<Vec<_>>()
                .join("-")
        ),
        system_type: intent.system_type.clone(),
        requirements: intent.requirements.clone(),
        constraints: intent.constraints.architecture.iter().cloned().collect(),
    }
}

pub fn template_record_from_template(template: &ArchitectureTemplate) -> TemplateRecord {
    TemplateRecord {
        template_id: template.template_id.clone(),
        topology: match template.topology {
            Topology::Layered => TopologyType::Layered,
            Topology::Hexagonal => TopologyType::Hexagonal,
            Topology::Microservice => TopologyType::Microservice,
            Topology::EventDriven => TopologyType::EventDriven,
            Topology::Pipeline => TopologyType::Pipeline,
        },
        layers: template
            .layer_structure
            .iter()
            .enumerate()
            .map(|(index, layer)| Layer {
                id: index as u64 + 1,
                name: layer.name.clone(),
                level: layer.level,
                components: Vec::new(),
                allowed_dependencies: Vec::new(),
            })
            .collect(),
        dependency_rules: template
            .dependency_rules
            .iter()
            .map(|rule| memory_space_phase14::DependencyRuleRecord {
                from: rule.from.clone(),
                to: rule.to.clone(),
            })
            .collect(),
        constraints: template.constraints.clone(),
        metadata: memory_space_phase14::TemplateMetadata::default(),
    }
}

fn template_from_record(record: &TemplateRecord) -> ArchitectureTemplate {
    ArchitectureTemplate {
        template_id: record.template_id.clone(),
        topology: match &record.topology {
            TopologyType::Layered => Topology::Layered,
            TopologyType::Hexagonal => Topology::Hexagonal,
            TopologyType::Microservice => Topology::Microservice,
            TopologyType::EventDriven => Topology::EventDriven,
            TopologyType::Pipeline => Topology::Pipeline,
            TopologyType::Custom(_) => Topology::Layered,
        },
        layer_structure: record
            .layers
            .iter()
            .map(|layer| crate::template::TemplateLayer {
                name: layer.name.clone(),
                level: layer.level,
            })
            .collect(),
        component_slots: Vec::new(),
        dependency_rules: record
            .dependency_rules
            .iter()
            .map(|rule| crate::DependencyRule {
                from: rule.from.clone(),
                to: rule.to.clone(),
            })
            .collect(),
        constraints: record.constraints.clone(),
        ranking: crate::template::TemplateRanking {
            historical_success: (record.metadata.success_rate * 10.0) as u32,
            pattern_stability: (record.metadata.average_score * 10.0) as u32,
            complexity: record.layers.len() as u32,
        },
    }
}
