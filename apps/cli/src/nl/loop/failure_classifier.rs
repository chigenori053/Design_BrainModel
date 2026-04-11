use super::state::FailureClass;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureClassifierInput<'a> {
    pub diagnostics: &'a [String],
    pub changed_files_count: usize,
    pub previous_signature: Option<&'a str>,
}

pub struct FailureClassifier;

impl FailureClassifier {
    pub fn classify(input: FailureClassifierInput<'_>) -> Option<FailureClass> {
        let joined = input.diagnostics.join("\n").to_lowercase();

        if input.changed_files_count == 0 {
            return Some(FailureClass::NoImprovement);
        }
        if joined.contains("unresolved import") || joined.contains("cannot find") {
            return Some(FailureClass::MissingImport);
        }
        if joined.contains("trait bound") || joined.contains("the trait") {
            return Some(FailureClass::TraitMismatch);
        }
        if joined.contains("borrowed") || joined.contains("cannot borrow") {
            return Some(FailureClass::BorrowConflict);
        }
        if joined.contains("lifetime") {
            return Some(FailureClass::LifetimeConflict);
        }
        if joined.contains("test failed") || joined.contains("failures:") {
            return Some(FailureClass::TestFailure);
        }
        if joined.contains("unsafe") && joined.contains("pub ") {
            return Some(FailureClass::UnsafeExpansion);
        }
        if joined.contains("error[") || joined.contains("could not compile") {
            return Some(FailureClass::CompileError);
        }

        if let Some(signature) = input.previous_signature
            && !signature.is_empty()
            && joined.contains(signature)
        {
            return Some(FailureClass::NoImprovement);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_import_maps_to_missing_import() {
        let diagnostics = vec![String::from("error[E0432]: unresolved import `foo::bar`")];
        let classified = FailureClassifier::classify(FailureClassifierInput {
            diagnostics: &diagnostics,
            changed_files_count: 1,
            previous_signature: None,
        });
        assert_eq!(classified, Some(FailureClass::MissingImport));
    }
}
