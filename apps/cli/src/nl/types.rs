use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentType {
    Analyze,
    Coding,
    Validate,
    StructureView,
    StructureEdit,
    StructureDiff,
    StructureDispatch,
    StructureUndo,
    StructureRedo,
    Run,
    Rules,
    Memory,
    GitCommit,
    GitPR,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTarget {
    pub path: PathBuf,
    pub node: Option<String>,
    pub scope: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodingOptions {
    pub safe: bool,
    pub check: bool,
}

impl Default for CodingOptions {
    fn default() -> Self {
        Self {
            safe: true,
            check: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlannedStep {
    Analyze(PathBuf),
    Coding(PathBuf, CodingOptions),
    Validate(PathBuf),
    StructureView(PathBuf),
    StructureEdit(PathBuf),
    StructureDiff(PathBuf, Option<String>),
    StructureUndo(PathBuf),
    StructureRedo(PathBuf),
    Run(PathBuf),
    Rules,
    Memory(PathBuf),
    GitCommit(PathBuf),
    GitPR(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CommandPlan {
    pub steps: Vec<PlannedStep>,
}
