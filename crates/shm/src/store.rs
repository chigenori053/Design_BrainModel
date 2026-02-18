use std::io;

use memory_store::{FileStore, InMemoryStore, Store};

#[derive(Debug)]
pub struct ShmStore<S>
where
    S: Store<String, String>,
{
    inner: S,
}

impl<S> ShmStore<S>
where
    S: Store<String, String>,
{
    pub fn new(inner: S) -> Self {
        Self { inner }
    }

    pub fn put_rule_snapshot(&self, rule_id: &str, value: &str) -> io::Result<()> {
        self.inner.put(rule_id.to_string(), value.to_string())
    }

    pub fn get_rule_snapshot(&self, rule_id: &str) -> io::Result<Option<String>> {
        self.inner.get(&rule_id.to_string())
    }
}

pub type InMemoryShmStore = ShmStore<InMemoryStore<String, String>>;
pub type FileShmStore = ShmStore<FileStore<String, String>>;

#[cfg(test)]
mod tests {
    use super::ShmStore;

    #[test]
    fn shm_store_roundtrip() {
        let store = ShmStore::new(memory_store::InMemoryStore::<String, String>::new());
        store.put_rule_snapshot("r1", "payload").expect("put");
        let out = store.get_rule_snapshot("r1").expect("get");
        assert_eq!(out.as_deref(), Some("payload"));
    }
}
