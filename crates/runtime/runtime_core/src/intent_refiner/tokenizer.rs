#[derive(Clone, Debug, Default)]
pub struct Tokenizer;

impl Tokenizer {
    pub fn tokenize(&self, input: &str) -> Vec<String> {
        input
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .map(|token| token.to_ascii_lowercase())
            .collect()
    }
}
