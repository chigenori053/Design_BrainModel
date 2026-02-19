use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const MAGIC: [u8; 8] = *b"HVSTORE0";
const VERSION: u32 = 1;
const HEADER_SIZE: u64 = 8 + 4 + 8;

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryEntry {
    pub id: u64,
    pub depth: usize,
    pub timestamp: u64,
    pub vector: Vec<f64>,
}

#[derive(Debug)]
pub struct HolographicVectorStore {
    path: PathBuf,
    dimension: u32,
}

impl HolographicVectorStore {
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
        Ok(Self { path, dimension })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn dimension(&self) -> u32 {
        self.dimension
    }

    pub fn append(&self, entry: &MemoryEntry) -> io::Result<()> {
        if entry.vector.len() != self.dimension as usize {
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

    pub fn entries(&self) -> io::Result<Vec<MemoryEntry>> {
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

    pub fn entry_count(&self) -> io::Result<u64> {
        let _lock = FileLockGuard::acquire(&self.path)?;
        let mut file = self.open_rw()?;
        let count = Self::read_count(&mut file)?;
        Ok(count)
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

    use super::{HolographicVectorStore, MemoryEntry};

    #[test]
    fn store_appends_and_reads_entries() {
        let path = std::env::temp_dir().join(format!(
            "holographic_store_test_{}.bin",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let store = HolographicVectorStore::open(&path, 4).expect("open");
        store
            .append(&MemoryEntry {
                id: 1,
                depth: 2,
                timestamp: 3,
                vector: vec![0.1, 0.2, 0.3, 0.4],
            })
            .expect("append");
        let items = store.entries().expect("read");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, 1);
        assert_eq!(items[0].depth, 2);
        assert_eq!(items[0].vector.len(), 4);
        let _ = std::fs::remove_file(path);
    }
}
