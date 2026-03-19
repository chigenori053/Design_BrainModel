use crate::framework::api_test::endpoint_tests;
use crate::framework::module_test::{
    dependency_wiring_tests, interface_tests, serialization_tests, signature_tests,
};
use crate::generator::test_generator::TestGenerator;
use crate::model::test_suite::TestSuite;
use code_language_core::stable_v03::GenerationContext;
use unified_design_ir::ImplementationUnit;

#[derive(Clone, Debug, Default)]
pub struct RustStructuralTestGenerator;

impl TestGenerator for RustStructuralTestGenerator {
    fn generate(&self, unit: &ImplementationUnit, ctx: &GenerationContext) -> TestSuite {
        let mut test_cases = interface_tests(unit, ctx);
        test_cases.extend(signature_tests(unit, ctx));
        test_cases.extend(serialization_tests(unit, ctx));
        test_cases.extend(dependency_wiring_tests(unit, ctx));
        test_cases.extend(endpoint_tests(unit, ctx));
        TestSuite {
            module_name: unit.module_name.clone(),
            test_cases,
        }
    }
}
