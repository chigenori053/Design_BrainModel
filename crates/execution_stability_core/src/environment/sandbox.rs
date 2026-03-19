#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sandbox {
    pub process_isolation: bool,
    pub working_directory_isolation: bool,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self {
            process_isolation: true,
            working_directory_isolation: true,
        }
    }
}
