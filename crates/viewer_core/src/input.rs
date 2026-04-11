#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerBinding {
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    MouseNavigation,
    ZoomIn,
    ZoomOut,
    SourceJump,
}

impl ViewerBinding {
    pub fn label(self) -> &'static str {
        match self {
            Self::ArrowUp => "ArrowUp",
            Self::ArrowDown => "ArrowDown",
            Self::ArrowLeft => "ArrowLeft",
            Self::ArrowRight => "ArrowRight",
            Self::MouseNavigation => "MouseNavigation",
            Self::ZoomIn => "ZoomIn",
            Self::ZoomOut => "ZoomOut",
            Self::SourceJump => "SourceJump",
        }
    }
}

pub const REPL_RESERVED_REVIEW_KEYS: [&str; 7] = ["E", "C", "J", "K", "A", "D", "R"];

pub fn active_viewer_bindings() -> Vec<ViewerBinding> {
    vec![
        ViewerBinding::ArrowUp,
        ViewerBinding::ArrowDown,
        ViewerBinding::ArrowLeft,
        ViewerBinding::ArrowRight,
        ViewerBinding::MouseNavigation,
        ViewerBinding::ZoomIn,
        ViewerBinding::ZoomOut,
        ViewerBinding::SourceJump,
    ]
}

pub fn viewer_supports_binding(label: &str) -> bool {
    active_viewer_bindings()
        .into_iter()
        .any(|binding| binding.label() == label)
}

pub fn viewer_uses_repl_reserved_key(label: &str) -> bool {
    REPL_RESERVED_REVIEW_KEYS.contains(&label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_bindings_exclude_repl_review_keys() {
        for key in REPL_RESERVED_REVIEW_KEYS {
            assert!(!viewer_supports_binding(key));
            assert!(viewer_uses_repl_reserved_key(key));
        }
    }
}
