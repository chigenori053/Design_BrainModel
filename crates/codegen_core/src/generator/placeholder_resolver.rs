use crate::error::CodegenError;
use crate::ir::IrStep;
use crate::spec::Placeholder;

pub fn resolve_placeholder(
    step: &IrStep,
    placeholder: &Placeholder,
) -> Result<String, CodegenError> {
    match placeholder {
        Placeholder::VarName => step
            .outputs
            .first()
            .cloned()
            .ok_or(CodegenError::UnresolvedPlaceholder(Placeholder::VarName)),

        Placeholder::ValueExpr => step
            .inputs
            .first()
            .cloned()
            .ok_or(CodegenError::UnresolvedPlaceholder(Placeholder::ValueExpr)),

        Placeholder::FuncName => step
            .target
            .clone()
            .ok_or(CodegenError::UnresolvedPlaceholder(Placeholder::FuncName)),

        // Join all inputs with ", " for deterministic, ordered output.
        Placeholder::Args => Ok(step.inputs.join(", ")),

        Placeholder::ConditionExpr => step
            .condition
            .clone()
            .ok_or(CodegenError::UnresolvedPlaceholder(Placeholder::ConditionExpr)),
    }
}
