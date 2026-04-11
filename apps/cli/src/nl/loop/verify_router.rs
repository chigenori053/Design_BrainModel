use super::state::PatchStrategy;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationRoute {
    pub command: String,
    pub scope: String,
}

pub struct VerifyRouter;

impl VerifyRouter {
    pub fn route(
        strategy: PatchStrategy,
        affected_crates: &[String],
        changed_files: &[String],
    ) -> VerificationRoute {
        if changed_files.iter().any(|file| file.contains("viewer_core")) {
            return VerificationRoute {
                command: "cargo test -p viewer_core --test integration".to_string(),
                scope: "viewer_core integration".to_string(),
            };
        }

        match strategy {
            PatchStrategy::TraitExtraction => VerificationRoute {
                command: format!(
                    "cargo check -p {}",
                    affected_crates.first().map(String::as_str).unwrap_or("design_cli")
                ),
                scope: "trait change".to_string(),
            },
            PatchStrategy::VisibilityFix => VerificationRoute {
                command: "cargo test --workspace".to_string(),
                scope: "public api".to_string(),
            },
            _ => VerificationRoute {
                command: format!(
                    "cargo check -p {}",
                    affected_crates.first().map(String::as_str).unwrap_or("design_cli")
                ),
                scope: "crate-local".to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_core_routes_to_integration_tests() {
        let route = VerifyRouter::route(
            PatchStrategy::CycleCut,
            &[String::from("design_cli")],
            &[String::from("crates/viewer_core/src/lib.rs")],
        );
        assert!(route.command.contains("viewer_core"));
        assert!(route.scope.contains("integration"));
    }
}
