#![allow(dead_code)]

pub trait Logger {
    fn log(&self, msg: &str);
}

pub struct NoopLogger;

impl Logger for NoopLogger {
    fn log(&self, _msg: &str) {}
}
