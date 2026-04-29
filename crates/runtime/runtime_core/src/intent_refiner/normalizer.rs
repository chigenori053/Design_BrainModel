#[derive(Clone, Debug, Default)]
pub struct Normalizer;

impl Normalizer {
    pub fn normalize(&self, input: &str) -> String {
        input
            .split_whitespace()
            .map(canonicalize_token)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn canonicalize_token(token: &str) -> String {
    let cleaned = token
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect::<String>()
        .to_ascii_lowercase();
    match cleaned.as_str() {
        "postgresql" | "pgsql" | "postgres" => "postgres".to_string(),
        "database" => "db".to_string(),
        "backend" => "service".to_string(),
        "frontend" => "web".to_string(),
        "restful" => "rest".to_string(),
        other => other.to_string(),
    }
}
