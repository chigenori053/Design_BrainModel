#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Layer {
    Ui,
    Service,
    Repository,
    Database,
}

impl Layer {
    pub fn order(self) -> usize {
        match self {
            Self::Ui => 0,
            Self::Service => 1,
            Self::Repository => 2,
            Self::Database => 3,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ui => "UI",
            Self::Service => "Service",
            Self::Repository => "Repository",
            Self::Database => "Database",
        }
    }

    pub fn infer_from_name(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        if lower.contains("controller") || lower.contains("ui") {
            Self::Ui
        } else if lower.contains("repository") {
            Self::Repository
        } else if lower.contains("database") || lower.contains("store") {
            Self::Database
        } else {
            Self::Service
        }
    }
}
