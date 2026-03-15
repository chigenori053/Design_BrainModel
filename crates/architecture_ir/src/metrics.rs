use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ComponentMetrics {
    pub loc: u32,
    pub cyclomatic_complexity: u32,
    pub fan_in: u32,
    pub fan_out: u32,
}
