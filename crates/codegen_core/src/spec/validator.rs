use super::{EmitCondition, EmitNode, FeatureFlags, IrOp, LanguageSpec};

#[derive(Debug, PartialEq)]
pub enum ValidationError {
    /// A defined IrOp has no corresponding Pattern in the spec.
    MissingPattern(IrOp),
    /// An EmitCondition::FeatureEnabled references an unknown flag name.
    UnknownFeatureFlag(String),
    /// An EmitNode tree exceeds the recursion depth limit.
    RecursionLimitExceeded,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::MissingPattern(op) => {
                write!(f, "missing pattern for IrOp::{op:?}")
            }
            ValidationError::UnknownFeatureFlag(name) => {
                write!(f, "unknown feature flag \"{name}\" in FeatureEnabled")
            }
            ValidationError::RecursionLimitExceeded => {
                write!(f, "EmitNode tree exceeds maximum recursion depth")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Maximum allowed nesting depth for EmitNode trees.
const MAX_DEPTH: usize = 64;

/// Validate a LanguageSpec and return all errors found.
/// Returns `Ok(())` only when the spec is fully valid.
pub fn validate(spec: &LanguageSpec) -> Result<(), Vec<ValidationError>> {
    let mut errors: Vec<ValidationError> = Vec::new();

    // Section 6: every defined IrOp must have a Pattern.
    for op in IrOp::all() {
        if !spec.patterns.contains_key(op) {
            errors.push(ValidationError::MissingPattern(op.clone()));
        }
    }

    // Validate each pattern's emit tree.
    for pattern in spec.patterns.values() {
        for node in &pattern.emit {
            validate_node(node, 0, &spec.features, &mut errors);
        }
    }

    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

fn validate_node(
    node: &EmitNode,
    depth: usize,
    features: &FeatureFlags,
    errors: &mut Vec<ValidationError>,
) {
    if depth > MAX_DEPTH {
        errors.push(ValidationError::RecursionLimitExceeded);
        return;
    }

    match node {
        EmitNode::Text(_) | EmitNode::Placeholder(_) | EmitNode::NewLine | EmitNode::Join { .. } => {}

        EmitNode::Sequence(nodes) => {
            for n in nodes {
                validate_node(n, depth + 1, features, errors);
            }
        }

        EmitNode::Optional { condition, node } => {
            validate_condition(condition, errors);
            validate_node(node, depth + 1, features, errors);
        }

        EmitNode::Indent(inner) => {
            validate_node(inner, depth + 1, features, errors);
        }
    }
}

/// Valid FeatureFlags field names that FeatureEnabled may reference.
const KNOWN_FEATURE_FLAGS: &[&str] = &[
    "requires_semicolon",
    "has_type_annotations",
    "supports_block_scope",
];

fn validate_condition(condition: &EmitCondition, errors: &mut Vec<ValidationError>) {
    if let EmitCondition::FeatureEnabled(name) = condition {
        if !KNOWN_FEATURE_FLAGS.contains(&name.as_str()) {
            errors.push(ValidationError::UnknownFeatureFlag(name.clone()));
        }
    }
}
