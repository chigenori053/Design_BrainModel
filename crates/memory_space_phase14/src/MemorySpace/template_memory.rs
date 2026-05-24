use std::collections::BTreeMap;

use architecture_ir::{ArchitectureConstraint, ComponentType, Layer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopologyType {
    Layered,
    Hexagonal,
    Microservice,
    EventDriven,
    Pipeline,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyRuleRecord {
    pub from: ComponentType,
    pub to: ComponentType,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TemplateMetadata {
    pub usage_count: u64,
    pub success_rate: f32,
    pub average_score: f32,
    pub created_from_architecture: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateRecord {
    pub template_id: String,
    pub topology: TopologyType,
    pub layers: Vec<Layer>,
    pub dependency_rules: Vec<DependencyRuleRecord>,
    pub constraints: Vec<ArchitectureConstraint>,
    pub metadata: TemplateMetadata,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TemplateMemoryDomain {
    records: BTreeMap<String, TemplateRecord>,
}

impl TemplateMemoryDomain {
    pub fn upsert(&mut self, record: TemplateRecord) {
        self.records.insert(record.template_id.clone(), record);
    }

    pub fn get(&self, template_id: &str) -> Option<&TemplateRecord> {
        self.records.get(template_id)
    }

    pub fn all(&self) -> Vec<&TemplateRecord> {
        self.records.values().collect()
    }
}
