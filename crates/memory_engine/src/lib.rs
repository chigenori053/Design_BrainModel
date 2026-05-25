//! Runtime memory engine crate.
//!
//! This crate is the future extraction target for
//! `memory_space_phase14::stable_v03` memory engine APIs.

pub const MEMORY_ENGINE_CRATE_READY: bool = true;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn memory_engine_crate_scaffold_is_available() {
        assert!(MEMORY_ENGINE_CRATE_READY);
    }
}
