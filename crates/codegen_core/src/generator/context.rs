use crate::spec::LanguageSpec;

pub struct EmitContext<'a> {
    pub indent_level: usize,
    pub in_block: bool,
    pub spec: &'a LanguageSpec,
}

impl<'a> EmitContext<'a> {
    pub fn new(spec: &'a LanguageSpec) -> Self {
        Self { indent_level: 0, in_block: false, spec }
    }

    pub fn current_indent(&self) -> String {
        self.spec.formatting.indent.repeat(self.indent_level)
    }
}
