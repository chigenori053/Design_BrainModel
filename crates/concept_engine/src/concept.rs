use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConceptId(pub u64);

impl ConceptId {
    pub fn from_name(name: &str) -> Self {
        Self(fnv1a64(normalize_concept_name(name).as_bytes()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConceptCategory {
    Component,
    Action,
    Property,
    Constraint,
    Domain,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Concept {
    pub id: ConceptId,
    pub name: String,
    pub embedding: Vec<f32>,
    pub category: ConceptCategory,
}

pub fn normalize_concept_name(text: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = false;

    for ch in text.trim().chars() {
        let up = ch.to_ascii_uppercase();
        if up.is_ascii_alphanumeric() {
            out.push(up);
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }

    out.trim_matches('_').to_string()
}

pub fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concept_id_is_deterministic() {
        let a = ConceptId::from_name("query optimization");
        let b = ConceptId::from_name("QUERY OPTIMIZATION");
        assert_eq!(a, b);
    }

    #[test]
    fn normalization_uses_upper_snake() {
        assert_eq!(normalize_concept_name("optimize query"), "OPTIMIZE_QUERY");
    }
}
