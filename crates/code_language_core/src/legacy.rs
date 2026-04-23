//! Internal legacy codegen bridge — crate-private, accessed only via `generate_legacy`.

use code_ir::CodeIr;

pub(crate) fn generate(ir: &CodeIr) -> Vec<(String, String)> {
    codegen_core_old::generate(ir)
}
