use crate::service;

pub fn tick() -> String {
    service::status()
}
