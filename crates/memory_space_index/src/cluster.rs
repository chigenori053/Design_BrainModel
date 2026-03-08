#[derive(Clone, Debug, PartialEq)]
pub struct ClusterPlan {
    pub enabled: bool,
    pub cluster_count: usize,
}

impl Default for ClusterPlan {
    fn default() -> Self {
        Self {
            enabled: false,
            cluster_count: 1,
        }
    }
}
