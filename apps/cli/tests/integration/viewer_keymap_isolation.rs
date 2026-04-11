use design_cli::viewer::keymap::{
    REPL_RESERVED_REVIEW_KEYS, active_viewer_bindings, viewer_supports_binding,
    viewer_uses_repl_reserved_key,
};

#[test]
fn viewer_does_not_bind_repl_review_keys() {
    for key in REPL_RESERVED_REVIEW_KEYS {
        assert!(viewer_uses_repl_reserved_key(key));
        assert!(
            !viewer_supports_binding(key),
            "{key} must stay reserved for REPL review"
        );
    }
}

#[test]
fn viewer_keeps_navigation_zoom_and_source_jump_bindings() {
    let labels = active_viewer_bindings()
        .into_iter()
        .map(|binding| binding.label())
        .collect::<Vec<_>>();
    assert!(labels.contains(&"ArrowUp"));
    assert!(labels.contains(&"ArrowDown"));
    assert!(labels.contains(&"ArrowLeft"));
    assert!(labels.contains(&"ArrowRight"));
    assert!(labels.contains(&"MouseNavigation"));
    assert!(labels.contains(&"ZoomIn"));
    assert!(labels.contains(&"ZoomOut"));
    assert!(labels.contains(&"SourceJump"));
}
