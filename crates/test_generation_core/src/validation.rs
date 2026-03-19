use std::collections::BTreeSet;

use crate::model::test_case::TestKind;
use crate::model::test_suite::TestSuite;
use code_language_core::stable_v03::GenerationContext;
use unified_design_ir::ImplementationUnit;

pub fn validate_test_suite(
    suite: &TestSuite,
    unit: &ImplementationUnit,
    ctx: &GenerationContext,
) -> Result<(), String> {
    if suite.module_name != unit.module_name {
        return Err("suite module name does not match implementation unit".to_string());
    }
    if suite.test_cases.is_empty() {
        return Err("test suite must contain at least one test case".to_string());
    }
    if suite
        .test_cases
        .iter()
        .any(|case| case.name.trim().is_empty() || case.code.trim().is_empty())
    {
        return Err("test suite contains an empty test case".to_string());
    }

    let covered = suite
        .test_cases
        .iter()
        .filter(|case| {
            matches!(
                case.kind,
                TestKind::InterfaceExistence | TestKind::SignatureValidation
            )
        })
        .map(|case| case.target.clone())
        .collect::<BTreeSet<_>>();
    let expected = unit
        .public_interfaces
        .iter()
        .map(|interface| interface.name.clone())
        .collect::<BTreeSet<_>>();
    if !expected.is_subset(&covered) {
        return Err("not every public interface has structural coverage".to_string());
    }

    if ctx.framework_profile.is_some()
        && !suite
            .test_cases
            .iter()
            .any(|case| matches!(case.kind, TestKind::EndpointAvailability))
    {
        return Err("framework-aware modules require an endpoint availability test".to_string());
    }

    Ok(())
}
