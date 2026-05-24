use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::memory_entry::MemoryEntry;

const MAGIC: [u8; 8] = *b"HVSTORE0";
const VERSION: u32 = 1;
const HEADER_SIZE: u64 = 8 + 4 + 8;

pub trait MemoryStore {
    fn append(&self, entry: &MemoryEntry) -> io::Result<()>;
    fn entries(&self) -> io::Result<Vec<MemoryEntry>>;
    fn entry_count(&self) -> io::Result<u64>;
}

#[derive(Debug)]
pub struct FileMemoryStore {
    path: PathBuf,
    dimensions: usize,
}

impl FileMemoryStore {
    pub fn open(path: impl AsRef<Path>, dimension: u32) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let _lock = FileLockGuard::acquire(&path)?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        let len = file.metadata()?.len();
        if len == 0 {
            Self::write_header(&mut file, 0)?;
        } else {
            Self::validate_header(&mut file)?;
        }
        Ok(Self {
            path,
            dimensions: dimension as usize,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn dimension(&self) -> u32 {
        self.dimensions as u32
    }

    fn open_rw(&self) -> io::Result<File> {
        OpenOptions::new().read(true).write(true).open(&self.path)
    }

    fn validate_header(file: &mut File) -> io::Result<()> {
        file.seek(SeekFrom::Start(0))?;
        let mut magic = [0u8; 8];
        file.read_exact(&mut magic)?;
        if magic != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid magic"));
        }
        let version = read_u32(file)?;
        if version != VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid version",
            ));
        }
        let _ = read_u64(file)?;
        Ok(())
    }

    fn write_header(file: &mut File, entry_count: u64) -> io::Result<()> {
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&MAGIC)?;
        file.write_all(&VERSION.to_le_bytes())?;
        file.write_all(&entry_count.to_le_bytes())?;
        file.flush()?;
        Ok(())
    }

    fn read_count(file: &mut File) -> io::Result<u64> {
        file.seek(SeekFrom::Start(12))?;
        read_u64(file)
    }

    fn write_count(file: &mut File, count: u64) -> io::Result<()> {
        file.seek(SeekFrom::Start(12))?;
        file.write_all(&count.to_le_bytes())?;
        file.flush()?;
        Ok(())
    }
}

impl MemoryStore for FileMemoryStore {
    fn append(&self, entry: &MemoryEntry) -> io::Result<()> {
        if entry.vector.len() != self.dimensions {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "vector dimension mismatch",
            ));
        }
        let _lock = FileLockGuard::acquire(&self.path)?;
        let mut file = self.open_rw()?;
        let count = Self::read_count(&mut file)?;
        file.seek(SeekFrom::End(0))?;
        file.write_all(&entry.id.to_le_bytes())?;
        file.write_all(&(entry.depth as u64).to_le_bytes())?;
        file.write_all(&entry.timestamp.to_le_bytes())?;
        file.write_all(&(entry.vector.len() as u32).to_le_bytes())?;
        for value in &entry.vector {
            file.write_all(&value.to_le_bytes())?;
        }
        Self::write_count(&mut file, count.saturating_add(1))?;
        Ok(())
    }

    fn entries(&self) -> io::Result<Vec<MemoryEntry>> {
        let _lock = FileLockGuard::acquire(&self.path)?;
        let mut file = self.open_rw()?;
        Self::validate_header(&mut file)?;
        let count = Self::read_count(&mut file)? as usize;
        file.seek(SeekFrom::Start(HEADER_SIZE))?;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let id = read_u64(&mut file)?;
            let depth = read_u64(&mut file)? as usize;
            let timestamp = read_u64(&mut file)?;
            let dim = read_u32(&mut file)? as usize;
            let mut vector = Vec::with_capacity(dim);
            for _ in 0..dim {
                vector.push(read_f64(&mut file)?);
            }
            out.push(MemoryEntry {
                id,
                depth,
                timestamp,
                vector,
            });
        }
        Ok(out)
    }

    fn entry_count(&self) -> io::Result<u64> {
        let _lock = FileLockGuard::acquire(&self.path)?;
        let mut file = self.open_rw()?;
        let count = Self::read_count(&mut file)?;
        Ok(count)
    }
}

fn read_u32(file: &mut File) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64(file: &mut File) -> io::Result<u64> {
    let mut buf = [0u8; 8];
    file.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

fn read_f64(file: &mut File) -> io::Result<f64> {
    let mut buf = [0u8; 8];
    file.read_exact(&mut buf)?;
    Ok(f64::from_le_bytes(buf))
}

struct FileLockGuard {
    lock_path: PathBuf,
}

impl FileLockGuard {
    fn acquire(path: &Path) -> io::Result<Self> {
        let lock_path = path.with_extension("lock");
        let mut retries = 0usize;
        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(_) => return Ok(Self { lock_path }),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists && retries < 200 => {
                    retries += 1;
                    thread::sleep(Duration::from_millis(5));
                }
                Err(err) => return Err(err),
            }
        }
    }
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{FileMemoryStore, MemoryStore};
    use crate::memory_entry::MemoryEntry;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{name}_{}.bin",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn adapter_opens_and_preserves_entries() {
        let path = temp_path("adapter_preserves_entries");
        {
            let adapter = FileMemoryStore::open(&path, 4).expect("open adapter");
            adapter
                .append(&MemoryEntry {
                    id: 7,
                    depth: 3,
                    timestamp: 11,
                    vector: vec![0.1, 0.2, 0.3, 0.4],
                })
                .expect("append");
        }

        let reopened = FileMemoryStore::open(&path, 4).expect("reopen adapter");
        let entries = reopened.entries().expect("entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, 7);
        assert_eq!(entries[0].depth, 3);
        assert_eq!(entries[0].timestamp, 11);
        assert_eq!(entries[0].vector, vec![0.1, 0.2, 0.3, 0.4]);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn file_memory_store_replaces_holographic_adapter() {
        let path = temp_path("adapter_legacy_roundtrip");
        let entry = MemoryEntry {
            id: 1,
            depth: 2,
            timestamp: 3,
            vector: vec![0.5, 0.6, 0.7, 0.8],
        };

        let adapter = FileMemoryStore::open(&path, 4).expect("open adapter");
        adapter.append(&entry).expect("append");

        let entries = adapter.entries().expect("entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], entry);
        assert_eq!(adapter.entry_count().expect("count"), 1);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn canonical_memory_store_api_is_public() {
        fn assert_implements_memory_store<T: MemoryStore>() {}
        assert_implements_memory_store::<FileMemoryStore>();
        let _store_type: Option<FileMemoryStore> = None;
    }

    #[test]
    fn adapter_roundtrip_after_backend_split() {
        let path = temp_path("adapter_roundtrip_split");
        let adapter = FileMemoryStore::open(&path, 4).expect("open");
        let entry = MemoryEntry {
            id: 42,
            depth: 5,
            timestamp: 100,
            vector: vec![1.0, 2.0, 3.0, 4.0],
        };
        adapter.append(&entry).expect("append");
        let entries = adapter.entries().expect("entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], entry);
        assert_eq!(adapter.entry_count().expect("count"), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn adapter_restart_consistency_after_backend_split() {
        let path = temp_path("adapter_restart_split");
        let entry = MemoryEntry {
            id: 99,
            depth: 7,
            timestamp: 200,
            vector: vec![0.1, 0.2, 0.3, 0.4],
        };
        {
            let adapter = FileMemoryStore::open(&path, 4).expect("open first");
            adapter.append(&entry).expect("append");
        }
        {
            let adapter = FileMemoryStore::open(&path, 4).expect("reopen");
            let entries = adapter.entries().expect("entries");
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0], entry);
        }
        let _ = std::fs::remove_file(path);
    }
}
