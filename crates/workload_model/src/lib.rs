#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Distribution {
    Uniform,
    Bursty,
    QueueHeavy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkloadModel {
    pub request_rate: f64,
    pub concurrency: usize,
    pub request_distribution: Distribution,
}

impl Default for WorkloadModel {
    fn default() -> Self {
        Self {
            request_rate: 10.0,
            concurrency: 4,
            request_distribution: Distribution::Uniform,
        }
    }
}
