pub fn designbrain_summary() -> (&'static str, usize, usize) {
    ("python_worker_optional", 256, 256)
}

#[cfg(test)]
mod tests {
    use super::designbrain_summary;
    #[test]
    fn generated_summary_is_non_empty() {
        let (name, modules, deps) = designbrain_summary();
        assert!(!name.is_empty());
        assert!(modules >= 1);
        assert!(deps <= modules * modules);
    }
}
