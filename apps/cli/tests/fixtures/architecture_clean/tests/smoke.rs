#[test]
fn smoke() {
    assert_eq!(architecture_clean::app::run(), "ok");
    assert_eq!(architecture_clean::r#loop::tick(), "ok");
}
