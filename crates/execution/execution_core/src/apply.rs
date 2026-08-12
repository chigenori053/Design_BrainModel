use code_ir::{CodeIr, ControlFlowEdge, DataFlowEdge, DependencyIr, InterfaceDirection, ModuleIr};

use crate::types::{AppliedChange, ChangeKind};

pub struct ExecutionStep {
    pub step_id: usize,
    pub ir_module_id: u64,
    pub description: String,
    pub kind: ChangeKind,
}

pub struct ApplyEngine {
    dry_run: bool,
}

impl ApplyEngine {
    pub fn new(dry_run: bool) -> Self {
        Self { dry_run }
    }

    pub fn apply(&self, step: &ExecutionStep) -> Option<AppliedChange> {
        if !self.dry_run {
            // Apply mode: in production would write files / transform AST.
            // Intentionally not implemented — apply is a future concern.
        }
        // Dry-run (default): record intent without side-effects.
        Some(AppliedChange {
            step_id: step.step_id,
            ir_module_id: step.ir_module_id,
            description: step.description.clone(),
            kind: step.kind.clone(),
        })
    }
}

pub fn decompose_plan(plan: &CodeIr, max_steps: usize) -> Vec<ExecutionStep> {
    let mut steps: Vec<ExecutionStep> = Vec::new();

    // Each module becomes a FileChange step (IR module → source file)
    for module in &plan.modules {
        if steps.len() >= max_steps {
            break;
        }
        steps.push(module_to_step(steps.len(), module));
    }

    // Each interface produces an AstTransform step
    for iface in &plan.interfaces {
        if steps.len() >= max_steps {
            break;
        }
        steps.push(interface_to_step(steps.len(), iface));
    }

    // Dependencies become DependencyUpdate steps
    for dep in &plan.dependencies {
        if steps.len() >= max_steps {
            break;
        }
        steps.push(dependency_to_step(steps.len(), dep));
    }

    // Control-flow edges refine structure
    for edge in &plan.control_flow {
        if steps.len() >= max_steps {
            break;
        }
        steps.push(control_flow_to_step(steps.len(), edge));
    }

    // Data-flow edges validate payload routing
    for edge in &plan.data_flow {
        if steps.len() >= max_steps {
            break;
        }
        steps.push(data_flow_to_step(steps.len(), edge));
    }

    steps
}

fn module_to_step(id: usize, module: &ModuleIr) -> ExecutionStep {
    ExecutionStep {
        step_id: id,
        ir_module_id: module.id,
        description: format!("apply module '{}' (layer {:?})", module.name, module.layer),
        kind: ChangeKind::FileChange,
    }
}

fn interface_to_step(id: usize, iface: &code_ir::InterfaceIr) -> ExecutionStep {
    let direction = match iface.direction {
        InterfaceDirection::Input => "input",
        InterfaceDirection::Output => "output",
    };
    ExecutionStep {
        step_id: id,
        ir_module_id: iface.module_id,
        description: format!("transform interface '{}' ({direction})", iface.name),
        kind: ChangeKind::AstTransform,
    }
}

fn dependency_to_step(id: usize, dep: &DependencyIr) -> ExecutionStep {
    ExecutionStep {
        step_id: id,
        ir_module_id: dep.from,
        description: format!("update dependency {}→{} ({:?})", dep.from, dep.to, dep.kind),
        kind: ChangeKind::DependencyUpdate,
    }
}

fn control_flow_to_step(id: usize, edge: &ControlFlowEdge) -> ExecutionStep {
    ExecutionStep {
        step_id: id,
        ir_module_id: edge.from,
        description: format!(
            "refactor control flow {}→{} ({})",
            edge.from, edge.to, edge.label
        ),
        kind: ChangeKind::StructureRefactor,
    }
}

fn data_flow_to_step(id: usize, edge: &DataFlowEdge) -> ExecutionStep {
    ExecutionStep {
        step_id: id,
        ir_module_id: edge.from,
        description: format!(
            "validate data flow {}→{} payload={}",
            edge.from, edge.to, edge.payload
        ),
        kind: ChangeKind::AstTransform,
    }
}
