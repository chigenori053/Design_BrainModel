#[derive(Debug, Clone, PartialEq)]
pub struct Tensor {
    values: Vec<f64>,
    shape: Vec<usize>,
}

impl Tensor {
    pub fn new(values: Vec<f64>, shape: Vec<usize>) -> Self {
        Self { values, shape }
    }

    pub fn values(&self) -> &[f64] {
        &self.values
    }

    pub fn shape(&self) -> &[usize] {
        &self.shape
    }
}

pub struct PhaseModule;

impl PhaseModule {
    pub fn compute(input: &Tensor) -> Tensor {
        input.clone()
    }
}

#[derive(Debug, Default)]
pub struct TensorEngine;

impl TensorEngine {
    pub fn run(&self, input: &Tensor) -> Tensor {
        PhaseModule::compute(input)
    }
}
