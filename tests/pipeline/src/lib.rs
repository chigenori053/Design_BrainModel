use std::fs;
use std::path::PathBuf;

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tests dir")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

pub fn read_workspace_file(relative: &str) -> String {
    fs::read_to_string(workspace_root().join(relative)).expect("read workspace file")
}

pub fn extract_fn_body(source: &str, signature: &str) -> String {
    let start = source.find(signature).expect("function signature exists");
    let body_start = source[start..]
        .find('{')
        .map(|offset| start + offset)
        .expect("function body starts");
    let mut depth = 0usize;
    let mut end = body_start;
    for (offset, ch) in source[body_start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = body_start + offset + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    source[body_start..end].to_string()
}
