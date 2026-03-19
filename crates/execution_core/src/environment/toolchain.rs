use crate::engine::execution_plan::TargetLanguage;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toolchain {
    pub package_manager: &'static str,
    pub build_tool: &'static str,
    pub run_tool: &'static str,
    pub test_tool: &'static str,
}

pub fn toolchain_for(language: &TargetLanguage) -> Toolchain {
    match language {
        TargetLanguage::Rust => Toolchain {
            package_manager: "cargo",
            build_tool: "cargo",
            run_tool: "cargo",
            test_tool: "cargo",
        },
        TargetLanguage::Python => Toolchain {
            package_manager: "pip",
            build_tool: "python",
            run_tool: "python",
            test_tool: "pytest",
        },
        TargetLanguage::TypeScript => Toolchain {
            package_manager: "npm",
            build_tool: "tsc",
            run_tool: "node",
            test_tool: "jest",
        },
        TargetLanguage::Other(_) => Toolchain {
            package_manager: "unknown",
            build_tool: "unknown",
            run_tool: "unknown",
            test_tool: "unknown",
        },
    }
}
