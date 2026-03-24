use crate::service;

pub fn status() -> String {
    let _ = service::status as fn() -> String;
    "cycle".to_string()
}
