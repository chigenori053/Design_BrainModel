use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub trait Codec: Sized {
    fn encode(&self) -> Vec<u8>;
    fn decode(bytes: &[u8]) -> io::Result<Self>;
}

impl Codec for String {
    fn encode(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        String::from_utf8(bytes.to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }
}

impl Codec for Vec<u8> {
    fn encode(&self) -> Vec<u8> {
        self.clone()
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        Ok(bytes.to_vec())
    }
}

pub trait Store<K, V>: Send + Sync
where
    K: Clone + Ord + Codec,
    V: Clone + Codec,
{
    fn put(&self, key: K, value: V) -> io::Result<()>;
    fn get(&self, key: &K) -> io::Result<Option<V>>;
    fn entries(&self) -> io::Result<Vec<(K, V)>>;
    fn replace_all(&self, entries: Vec<(K, V)>) -> io::Result<()>;
}

#[derive(Debug, Default)]
pub struct InMemoryStore<K, V>
where
    K: Clone + Ord + Codec,
    V: Clone + Codec,
{
    inner: RwLock<BTreeMap<K, V>>,
}

impl<K, V> InMemoryStore<K, V>
where
    K: Clone + Ord + Codec,
    V: Clone + Codec,
{
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(BTreeMap::new()),
        }
    }
}

impl<K, V> Store<K, V> for InMemoryStore<K, V>
where
    K: Clone + Ord + Codec + Send + Sync + 'static,
    V: Clone + Codec + Send + Sync + 'static,
{
    fn put(&self, key: K, value: V) -> io::Result<()> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| io::Error::other("in-memory store poisoned"))?;
        guard.insert(key, value);
        Ok(())
    }

    fn get(&self, key: &K) -> io::Result<Option<V>> {
        let guard = self
            .inner
            .read()
            .map_err(|_| io::Error::other("in-memory store poisoned"))?;
        Ok(guard.get(key).cloned())
    }

    fn entries(&self) -> io::Result<Vec<(K, V)>> {
        let guard = self
            .inner
            .read()
            .map_err(|_| io::Error::other("in-memory store poisoned"))?;
        Ok(guard.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }

    fn replace_all(&self, entries: Vec<(K, V)>) -> io::Result<()> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| io::Error::other("in-memory store poisoned"))?;
        guard.clear();
        for (k, v) in entries {
            guard.insert(k, v);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct FileStore<K, V>
where
    K: Clone + Ord + Codec,
    V: Clone + Codec,
{
    path: PathBuf,
    _marker: PhantomData<(K, V)>,
}

impl<K, V> FileStore<K, V>
where
    K: Clone + Ord + Codec,
    V: Clone + Codec,
{
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)?;
            file.write_all(&0u64.to_le_bytes())?;
        }
        Ok(Self {
            path,
            _marker: PhantomData,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn read_map(&self) -> io::Result<BTreeMap<K, V>> {
        let mut file = OpenOptions::new().read(true).open(&self.path)?;
        let mut raw = Vec::new();
        file.read_to_end(&mut raw)?;
        if raw.len() < 8 {
            return Ok(BTreeMap::new());
        }
        let mut idx = 0usize;
        let count = read_u64(&raw, &mut idx)? as usize;
        let mut out = BTreeMap::new();
        for _ in 0..count {
            let k_len = read_u32(&raw, &mut idx)? as usize;
            let key_end = idx.saturating_add(k_len);
            if key_end > raw.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "corrupt key length",
                ));
            }
            let key = K::decode(&raw[idx..key_end])?;
            idx = key_end;

            let v_len = read_u32(&raw, &mut idx)? as usize;
            let value_end = idx.saturating_add(v_len);
            if value_end > raw.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "corrupt value length",
                ));
            }
            let value = V::decode(&raw[idx..value_end])?;
            idx = value_end;

            out.insert(key, value);
        }
        Ok(out)
    }

    fn write_map(&self, map: &BTreeMap<K, V>) -> io::Result<()> {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(&(map.len() as u64).to_le_bytes());
        for (k, v) in map {
            let kb = k.encode();
            let vb = v.encode();
            encoded.extend_from_slice(&(kb.len() as u32).to_le_bytes());
            encoded.extend_from_slice(&kb);
            encoded.extend_from_slice(&(vb.len() as u32).to_le_bytes());
            encoded.extend_from_slice(&vb);
        }

        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&self.path)?;
        file.write_all(&encoded)?;
        file.flush()?;
        Ok(())
    }
}

impl<K, V> Store<K, V> for FileStore<K, V>
where
    K: Clone + Ord + Codec + Send + Sync + 'static,
    V: Clone + Codec + Send + Sync + 'static,
{
    fn put(&self, key: K, value: V) -> io::Result<()> {
        let mut map = self.read_map()?;
        map.insert(key, value);
        self.write_map(&map)
    }

    fn get(&self, key: &K) -> io::Result<Option<V>> {
        let map = self.read_map()?;
        Ok(map.get(key).cloned())
    }

    fn entries(&self) -> io::Result<Vec<(K, V)>> {
        let map = self.read_map()?;
        Ok(map.into_iter().collect())
    }

    fn replace_all(&self, entries: Vec<(K, V)>) -> io::Result<()> {
        let map = entries.into_iter().collect::<BTreeMap<_, _>>();
        self.write_map(&map)
    }
}

fn read_u32(raw: &[u8], idx: &mut usize) -> io::Result<u32> {
    if idx.saturating_add(4) > raw.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "u32"));
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&raw[*idx..*idx + 4]);
    *idx += 4;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64(raw: &[u8], idx: &mut usize) -> io::Result<u64> {
    if idx.saturating_add(8) > raw.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "u64"));
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&raw[*idx..*idx + 8]);
    *idx += 8;
    Ok(u64::from_le_bytes(buf))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{FileStore, InMemoryStore, Store};

    #[test]
    fn in_memory_store_roundtrip() {
        let store = InMemoryStore::<String, String>::new();
        store
            .put("alpha".to_string(), "one".to_string())
            .expect("put");
        let out = store.get(&"alpha".to_string()).expect("get");
        assert_eq!(out.as_deref(), Some("one"));
        assert_eq!(store.entries().expect("entries").len(), 1);
    }

    #[test]
    fn file_store_survives_restart() {
        let path = std::env::temp_dir().join(format!(
            "memory_store_restart_{}.bin",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        {
            let store = FileStore::<String, String>::open(&path).expect("open write");
            store.put("k1".to_string(), "v1".to_string()).expect("put");
        }
        {
            let store = FileStore::<String, String>::open(&path).expect("open read");
            let out = store.get(&"k1".to_string()).expect("get");
            assert_eq!(out.as_deref(), Some("v1"));
            assert_eq!(store.entries().expect("entries").len(), 1);
        }
        let _ = std::fs::remove_file(path);
    }
}
