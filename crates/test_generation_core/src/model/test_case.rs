#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestCase {
    pub name: String,
    pub kind: TestKind,
    pub target: String,
    pub code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TestKind {
    InterfaceExistence,
    SignatureValidation,
    Serialization,
    DependencyWiring,
    EndpointAvailability,
}
