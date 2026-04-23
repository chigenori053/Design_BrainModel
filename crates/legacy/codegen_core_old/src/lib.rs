//! Legacy code generation pipeline — isolated reference implementation.
//!
//! Do NOT depend on this crate directly. Access only via `code_language_core`
//! with the `legacy_codegen` feature enabled.

use code_ir::CodeIr;

pub fn generate(ir: &CodeIr) -> Vec<(String, String)> {
    ir.modules
        .iter()
        .map(|module| {
            let file_name = format!("{}.rs", module.name.to_lowercase());
            let source = format!("// [legacy-generated]\npub struct {};\n", module.name);
            (file_name, source)
        })
        .collect()
}
