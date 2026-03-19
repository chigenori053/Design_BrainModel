use crate::model::test_case::TestCase;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestSuite {
    pub module_name: String,
    pub test_cases: Vec<TestCase>,
}
