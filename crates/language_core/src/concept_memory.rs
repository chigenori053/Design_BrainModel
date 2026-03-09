#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ConceptId(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub struct Concept {
    pub concept_id: ConceptId,
    pub label: String,
    pub attributes: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConceptMemory {
    concepts: Vec<Concept>,
}

impl ConceptMemory {
    pub fn seeded() -> Self {
        Self {
            concepts: vec![
                concept(1, "api", &["interface", "endpoint", "api", "apis"]),
                concept(2, "rest", &["http", "resource", "restful"]),
                concept(3, "database", &["storage", "repository"]),
                concept(4, "authentication", &["security", "auth"]),
                concept(
                    5,
                    "microservice",
                    &["service", "distributed", "microservicio", "microservices"],
                ),
                concept(
                    6,
                    "scalable",
                    &[
                        "scale",
                        "performance",
                        "escalable",
                        "scalability",
                        "scalable",
                        "スケーラブル",
                    ],
                ),
                concept(7, "controller", &["ui", "entrypoint"]),
                concept(8, "service", &["domain", "business"]),
                concept(9, "repository", &["data", "persistence"]),
                concept(
                    10,
                    "build",
                    &["create", "design", "construct", "construir", "構築", "設計"],
                ),
                concept(11, "stateless", &["cacheless", "sans-state"]),
                concept(12, "http", &["web", "protocol"]),
                concept(13, "client_server", &["client-server", "request-response"]),
                concept(14, "layered_architecture", &["layered", "n-tier"]),
                concept(15, "service_discovery", &["discovery", "registry"]),
                concept(16, "api_gateway", &["gateway", "edge"]),
                concept(17, "containerization", &["container", "docker"]),
            ],
        }
    }

    pub fn concepts(&self) -> &[Concept] {
        &self.concepts
    }

    pub fn resolve_text(&self, text: &str) -> Vec<Concept> {
        let lower = text.to_ascii_lowercase();
        let mut out = self
            .concepts
            .iter()
            .filter(|concept| {
                lower.contains(&concept.label)
                    || concept.attributes.iter().any(|attr| lower.contains(attr))
            })
            .cloned()
            .collect::<Vec<_>>();
        out.sort_by_key(|concept| concept.concept_id);
        out
    }
}

fn concept(id: u64, label: &str, attributes: &[&str]) -> Concept {
    Concept {
        concept_id: ConceptId(id),
        label: label.to_string(),
        attributes: attributes.iter().map(|attr| attr.to_string()).collect(),
    }
}
