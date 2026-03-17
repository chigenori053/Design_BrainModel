use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Metadata {
    pub attributes: BTreeMap<String, String>,
}

impl Metadata {
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}
