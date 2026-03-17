#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Constraint {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct DesignMetadata {
    pub language_hint: Option<String>,
    pub constraints: Vec<Constraint>,
    pub annotations: Vec<String>,
}
