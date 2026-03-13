use architecture_domain::ArchitectureState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComplexityClass {
    Constant,
    Linear,
    Linearithmic,
    Quadratic,
    Cubic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComplexityEstimate {
    pub time_complexity: ComplexityClass,
    pub space_complexity: ComplexityClass,
}

impl ComplexityEstimate {
    pub fn score(self) -> f64 {
        let time: f64 = match self.time_complexity {
            ComplexityClass::Constant => 1.0,
            ComplexityClass::Linear => 0.9,
            ComplexityClass::Linearithmic => 0.75,
            ComplexityClass::Quadratic => 0.5,
            ComplexityClass::Cubic => 0.25,
        };
        let space: f64 = match self.space_complexity {
            ComplexityClass::Constant => 1.0,
            ComplexityClass::Linear => 0.85,
            ComplexityClass::Linearithmic => 0.7,
            ComplexityClass::Quadratic => 0.45,
            ComplexityClass::Cubic => 0.2,
        };
        ((time + space) / 2.0).clamp(0.0, 1.0)
    }
}

pub trait ComplexityEstimator {
    fn estimate(&self, architecture: &ArchitectureState) -> ComplexityEstimate;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HeuristicComplexityEstimator;

impl ComplexityEstimator for HeuristicComplexityEstimator {
    fn estimate(&self, architecture: &ArchitectureState) -> ComplexityEstimate {
        let edges = architecture.metrics.dependency_count;
        let nodes = architecture.metrics.component_count.max(1);
        let density = edges as f64 / nodes as f64;
        let time_complexity = if edges <= 1 {
            ComplexityClass::Constant
        } else if density <= 1.0 {
            ComplexityClass::Linear
        } else if density <= 1.5 {
            ComplexityClass::Linearithmic
        } else if density <= 2.5 {
            ComplexityClass::Quadratic
        } else {
            ComplexityClass::Cubic
        };
        let space_complexity = if nodes <= 2 {
            ComplexityClass::Constant
        } else if nodes <= 8 {
            ComplexityClass::Linear
        } else if nodes <= 16 {
            ComplexityClass::Linearithmic
        } else if nodes <= 32 {
            ComplexityClass::Quadratic
        } else {
            ComplexityClass::Cubic
        };
        ComplexityEstimate {
            time_complexity,
            space_complexity,
        }
    }
}
