use code_ir::{IrStep, IrOp, IrType};
use crate::error::CodegenError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub name: String,
    pub ty: Option<IrType>,
    pub depth: usize,
}

/// A stack of scope frames. Depth 0 = function scope (params).
pub struct ScopeStack {
    frames: Vec<Vec<Binding>>,
}

impl ScopeStack {
    pub fn new() -> Self {
        Self { frames: vec![vec![]] }
    }

    pub fn push_scope(&mut self) {
        self.frames.push(vec![]);
    }

    pub fn pop_scope(&mut self) {
        self.frames.pop();
    }

    pub fn depth(&self) -> usize {
        self.frames.len() - 1
    }

    /// Declare a name in the current (innermost) scope.
    /// Returns an error if the name already exists at this exact depth.
    pub fn declare(
        &mut self,
        name: impl Into<String>,
        ty: Option<IrType>,
    ) -> Result<(), CodegenError> {
        let name = name.into();
        let depth = self.depth();
        let frame = self.frames.last_mut().expect("scope stack is never empty");
        if frame.iter().any(|b| b.name == name) {
            return Err(CodegenError::DuplicateBinding { name, depth });
        }
        frame.push(Binding { name, ty, depth });
        Ok(())
    }

    /// Look up a name from innermost to outermost scope.
    pub fn resolve(&self, name: &str) -> Option<&Binding> {
        for frame in self.frames.iter().rev() {
            if let Some(b) = frame.iter().find(|b| b.name == name) {
                return Some(b);
            }
        }
        None
    }

    pub fn is_resolved(&self, name: &str) -> bool {
        self.resolve(name).is_some()
    }
}

/// Language keywords / boolean literals that are never variable references.
const LITERALS: &[&str] = &[
    "true", "false", "None", "null", "nil", "self", "Self",
    "undefined", "True", "False",
];

/// Returns `true` if `s` looks like a plain identifier (variable reference).
/// Expression-like strings (spaces, `(`, `.`, `::`, operators) or known literals
/// are excluded from variable checking.
fn is_identifier(s: &str) -> bool {
    if s.is_empty() || LITERALS.contains(&s) {
        return false;
    }
    // Numbers are literals too
    if s.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        return false;
    }
    // If it contains non-identifier chars it's an expression
    s.chars().all(|c| c.is_alphanumeric() || c == '_')
        && s.chars().next().map_or(false, |c| c.is_alphabetic() || c == '_')
}

/// Walk `steps` under `scope`, collecting binding/resolution errors.
pub fn validate_steps(
    steps: &[IrStep],
    scope: &mut ScopeStack,
) -> Vec<CodegenError> {
    let mut errors = vec![];

    for step in steps {
        match step.op {
            IrOp::Assign => {
                // Check that inputs (RHS expression) are resolved if they look like identifiers.
                for v in &step.inputs {
                    if is_identifier(&v.name) && !scope.is_resolved(&v.name) {
                        errors.push(CodegenError::UnresolvedVariable { name: v.name.clone() });
                    }
                }
                // Declare each output into the current scope.
                for v in &step.outputs {
                    if let Err(e) = scope.declare(&v.name, None) {
                        errors.push(e);
                    }
                }
            }
            IrOp::Call | IrOp::Return => {
                for v in &step.inputs {
                    if is_identifier(&v.name) && !scope.is_resolved(&v.name) {
                        errors.push(CodegenError::UnresolvedVariable { name: v.name.clone() });
                    }
                }
            }
            IrOp::Branch => {
                // Validate then-body in a child scope.
                if let Some(body) = &step.body {
                    scope.push_scope();
                    errors.extend(validate_steps(body, scope));
                    scope.pop_scope();
                }
                if let Some(else_body) = &step.else_body {
                    scope.push_scope();
                    errors.extend(validate_steps(else_body, scope));
                    scope.pop_scope();
                }
            }
            IrOp::Block | IrOp::Loop => {
                if let Some(body) = &step.body {
                    scope.push_scope();
                    errors.extend(validate_steps(body, scope));
                    scope.pop_scope();
                }
            }
        }
    }

    errors
}
