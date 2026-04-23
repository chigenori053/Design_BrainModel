use crate::error::CodegenError;
use crate::ir::CodeIr;
use crate::spec::LanguageSpec;

use super::context::EmitContext;
use super::emitter::emit_node;

/// Generates a code string from an IR and a language spec.
/// Pure function – no side effects, 100% deterministic.
pub fn generate_code(ir: &CodeIr, spec: &LanguageSpec) -> Result<String, CodegenError> {
    let mut output = String::new();
    for step in &ir.steps {
        let pattern = spec
            .find_pattern(&step.op)
            .ok_or_else(|| CodegenError::MissingPattern(step.op.clone()))?;
        let mut ctx = EmitContext::new(spec);
        output.push_str(&emit_node(&pattern.emit, step, &mut ctx)?);
    }
    Ok(output)
}
