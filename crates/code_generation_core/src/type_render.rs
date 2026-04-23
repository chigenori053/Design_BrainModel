use code_ir::IrType;
use crate::{error::CodegenError, spec::LanguageSpec};

/// Renders an `IrType` to the target language's type syntax.
pub fn render_type(ty: &IrType, spec: &LanguageSpec) -> Result<String, CodegenError> {
    match spec.name.as_str() {
        "rust" => Ok(match ty {
            IrType::Int => "i64".to_string(),
            IrType::Float => "f64".to_string(),
            IrType::Bool => "bool".to_string(),
            IrType::Str => "String".to_string(),
            IrType::Void => "()".to_string(),
            IrType::Custom(name) => name.clone(),
        }),
        "python" => Ok(match ty {
            IrType::Int => "int".to_string(),
            IrType::Float => "float".to_string(),
            IrType::Bool => "bool".to_string(),
            IrType::Str => "str".to_string(),
            IrType::Void => "None".to_string(),
            IrType::Custom(name) => name.clone(),
        }),
        lang => Err(CodegenError::UnsupportedTypeRendering {
            ty: format!("{ty:?}"),
            language: lang.to_string(),
        }),
    }
}
