/// Indentation and brace style for a target language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formatting {
    pub indent: String,
    pub use_braces: bool,
}

impl Formatting {
    pub fn braced(indent: impl Into<String>) -> Self {
        Self { indent: indent.into(), use_braces: true }
    }

    pub fn colon(indent: impl Into<String>) -> Self {
        Self { indent: indent.into(), use_braces: false }
    }
}

/// Declarative description of how a language emits control structures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageSpec {
    pub name: String,
    pub formatting: Formatting,
}

impl LanguageSpec {
    pub fn rust() -> Self {
        Self {
            name: "rust".to_string(),
            formatting: Formatting::braced("    "),
        }
    }

    pub fn python() -> Self {
        Self {
            name: "python".to_string(),
            formatting: Formatting::colon("    "),
        }
    }

    // ── helpers ───────────────────────────────────────────────────────────

    pub fn branch_open(&self) -> &str {
        if self.formatting.use_braces { " {" } else { ":" }
    }

    pub fn branch_close(&self) -> Option<&str> {
        if self.formatting.use_braces { Some("}") } else { None }
    }

    pub fn else_keyword(&self) -> &str {
        if self.formatting.use_braces { "} else {" } else { "else:" }
    }

    pub fn loop_open(&self) -> &str {
        if self.formatting.use_braces { " {" } else { ":" }
    }

    pub fn loop_close(&self) -> Option<&str> {
        if self.formatting.use_braces { Some("}") } else { None }
    }

    pub fn block_open(&self) -> Option<&str> {
        if self.formatting.use_braces { Some("{") } else { None }
    }

    pub fn block_close(&self) -> Option<&str> {
        if self.formatting.use_braces { Some("}") } else { None }
    }
}
