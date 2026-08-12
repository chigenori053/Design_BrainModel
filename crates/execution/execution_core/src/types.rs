use code_ir::CodeIr;

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub dry_run: bool,
    pub max_steps: usize,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self {
            dry_run: true,
            max_steps: 64,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionInput {
    pub plan: CodeIr,
    pub context: ExecutionContext,
}

impl ExecutionInput {
    pub fn new(plan: CodeIr) -> Self {
        Self {
            plan,
            context: ExecutionContext::default(),
        }
    }

    pub fn with_context(plan: CodeIr, context: ExecutionContext) -> Self {
        Self { plan, context }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    FileChange,
    AstTransform,
    DependencyUpdate,
    StructureRefactor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedChange {
    pub step_id: usize,
    pub ir_module_id: u64,
    pub description: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    pub success: bool,
    pub messages: Vec<String>,
}

impl ValidationResult {
    pub fn ok() -> Self {
        Self {
            success: true,
            messages: Vec::new(),
        }
    }

    pub fn failed(messages: Vec<String>) -> Self {
        Self {
            success: false,
            messages,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RollbackInfo {
    pub steps_applied: usize,
    pub reverted: bool,
    pub reverted_changes: Vec<AppliedChange>,
}

impl RollbackInfo {
    pub fn committed(steps_applied: usize) -> Self {
        Self {
            steps_applied,
            reverted: false,
            reverted_changes: Vec::new(),
        }
    }

    pub fn rolled_back(changes: Vec<AppliedChange>) -> Self {
        let count = changes.len();
        Self {
            steps_applied: count,
            reverted: true,
            reverted_changes: changes,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub applied_changes: Vec<AppliedChange>,
    pub validation_result: ValidationResult,
    pub rollback_info: RollbackInfo,
    pub dry_run: bool,
}

impl ExecutionResult {
    pub fn success(&self) -> bool {
        self.validation_result.success && !self.rollback_info.reverted
    }
}
