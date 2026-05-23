#[derive(Clone, Debug, PartialEq)]
pub struct MemoryEntry {
    pub id: u64,
    pub depth: usize,
    pub timestamp: u64,
    pub vector: Vec<f64>,
}
