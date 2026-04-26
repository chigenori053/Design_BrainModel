pub mod applier;
pub mod patch_generator;
pub mod validator;

pub use applier::{ApplyError, apply_patch, apply_patch_to_files, rollback};
pub use patch_generator::{
    Patch, PatchError, TextEdit, generate_patch, generate_patch_for_backend,
};
pub use validator::{
    SafetyError, SemanticValidationError, ensure_safe_changes, validate_patch_semantics,
};
