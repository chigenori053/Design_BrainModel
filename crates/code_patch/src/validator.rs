use std::collections::BTreeMap;

use code_diff::{ChangeSet, IrChange, replay_changes};
use code_ir::program_v1::{BackendLanguage, Function, Program, Statement};
use code_parser::{ParseError, SupportedLanguage, parse_source_to_ir};

use crate::patch_generator::Patch;

pub fn ensure_safe_changes(old: &Program, changes: &ChangeSet) -> Result<(), SafetyError> {
    for change in &changes.changes {
        match change {
            IrChange::RenameSymbol { .. } => {}
            IrChange::ModifyFunction {
                module,
                old_name,
                new_function,
            } => {
                let old_function = find_function(old, module, old_name).ok_or_else(|| {
                    SafetyError::InvalidChange(format!("function {} not found", old_name))
                })?;
                if old_function.inputs != new_function.inputs
                    || old_function.outputs != new_function.outputs
                {
                    return Err(SafetyError::UnsafeTypeChange {
                        module: module.clone(),
                        function: old_name.clone(),
                    });
                }
                if old_function.body != new_function.body {
                    return Err(SafetyError::UnsafeStatementChange {
                        module: module.clone(),
                        function: old_name.clone(),
                    });
                }
            }
            IrChange::ModifyStatement {
                module,
                function_name,
                new_statement,
                ..
            } => {
                if changes_condition(new_statement) {
                    return Err(SafetyError::UnsafeConditionChange {
                        module: module.clone(),
                        function: function_name.clone(),
                    });
                }
                return Err(SafetyError::UnsafeStatementChange {
                    module: module.clone(),
                    function: function_name.clone(),
                });
            }
            IrChange::AddFunction { module, function } => {
                return Err(SafetyError::UnsafeFunctionAddition {
                    module: module.clone(),
                    function: function.name.clone(),
                });
            }
            IrChange::RemoveFunction {
                module,
                function_name,
            } => {
                return Err(SafetyError::UnsafeFunctionRemoval {
                    module: module.clone(),
                    function: function_name.clone(),
                });
            }
            IrChange::AddStatement {
                module,
                function_name,
                ..
            }
            | IrChange::RemoveStatement {
                module,
                function_name,
                ..
            } => {
                return Err(SafetyError::UnsafeStatementChange {
                    module: module.clone(),
                    function: function_name.clone(),
                });
            }
        }
    }

    Ok(())
}

pub fn validate_patch_semantics(
    language: SupportedLanguage,
    old_ir: &Program,
    patch: &Patch,
    expected_changes: &ChangeSet,
) -> Result<Program, SemanticValidationError> {
    let expected =
        replay_changes(old_ir, expected_changes).map_err(SemanticValidationError::Replay)?;
    let backend = backend_for_language(language);
    let original_files = old_ir
        .render_canonical_source_tree(backend.clone())
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let patched_files = crate::applier::apply_patch_to_files(&original_files, patch)
        .map_err(SemanticValidationError::Apply)?;
    let actual = parse_program(language, &patched_files).map_err(SemanticValidationError::Parse)?;
    if actual != expected {
        return Err(SemanticValidationError::Mismatch { expected, actual });
    }
    Ok(actual)
}

fn parse_program(
    language: SupportedLanguage,
    files: &BTreeMap<String, String>,
) -> Result<Program, ParseError> {
    let mut program = Program::new("patched");
    program.modules.clear();
    for (path, source) in files {
        let module_name = path
            .rsplit('/')
            .next()
            .unwrap_or(path.as_str())
            .split('.')
            .next()
            .unwrap_or(path.as_str());
        let parsed = parse_source_to_ir(language, module_name, source)?;
        program.metadata = parsed.metadata;
        program.dependencies = parsed.dependencies;
        program.generation_strategy = parsed.generation_strategy;
        program.build_validation = parsed.build_validation;
        program.modules.extend(parsed.modules);
    }
    program.modules.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    Ok(program)
}

fn backend_for_language(language: SupportedLanguage) -> BackendLanguage {
    match language {
        SupportedLanguage::Rust => BackendLanguage::Rust,
        SupportedLanguage::Python => BackendLanguage::Python,
    }
}

fn find_function<'a>(
    program: &'a Program,
    module_name: &str,
    function_name: &str,
) -> Option<&'a Function> {
    program
        .modules
        .iter()
        .find(|module| module.name == module_name)
        .and_then(|module| {
            module
                .functions
                .iter()
                .find(|function| function.name == function_name)
        })
}

fn changes_condition(statement: &Statement) -> bool {
    matches!(statement, Statement::If(_) | Statement::Loop(_))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SafetyError {
    UnsafeConditionChange { module: String, function: String },
    UnsafeTypeChange { module: String, function: String },
    UnsafeStatementChange { module: String, function: String },
    UnsafeFunctionAddition { module: String, function: String },
    UnsafeFunctionRemoval { module: String, function: String },
    InvalidChange(String),
}

impl std::fmt::Display for SafetyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsafeConditionChange { module, function } => {
                write!(f, "unsafe condition change in {module}::{function}")
            }
            Self::UnsafeTypeChange { module, function } => {
                write!(f, "unsafe type change in {module}::{function}")
            }
            Self::UnsafeStatementChange { module, function } => {
                write!(f, "unsafe statement change in {module}::{function}")
            }
            Self::UnsafeFunctionAddition { module, function } => {
                write!(f, "unsafe function addition in {module}::{function}")
            }
            Self::UnsafeFunctionRemoval { module, function } => {
                write!(f, "unsafe function removal in {module}::{function}")
            }
            Self::InvalidChange(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for SafetyError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticValidationError {
    Replay(code_diff::ir_diff::ReplayError),
    Apply(crate::applier::ApplyError),
    Parse(ParseError),
    Mismatch { expected: Program, actual: Program },
}

impl std::fmt::Display for SemanticValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Replay(err) => write!(f, "{err}"),
            Self::Apply(err) => write!(f, "{err}"),
            Self::Parse(err) => write!(f, "{err}"),
            Self::Mismatch { .. } => write!(f, "semantic mismatch"),
        }
    }
}

impl std::error::Error for SemanticValidationError {}
