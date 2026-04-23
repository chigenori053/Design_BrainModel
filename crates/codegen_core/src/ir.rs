use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrOp {
    Assign,
    Call,
}

impl fmt::Display for IrOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrOp::Assign => write!(f, "Assign"),
            IrOp::Call => write!(f, "Call"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IrStep {
    pub op: IrOp,
    pub target: Option<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub condition: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CodeIr {
    pub steps: Vec<IrStep>,
}

impl CodeIr {
    pub fn new(steps: Vec<IrStep>) -> Self {
        Self { steps }
    }
}
