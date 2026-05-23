/// A blake3-based 32-byte deterministic checksum.
///
/// Ordering-dependent: the byte sequence fed to the hasher is part of
/// the checksum contract.  Same bytes in same order → same checksum.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Checksum([u8; 32]);

impl Checksum {
    /// Hash arbitrary bytes.
    pub fn of(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }

    /// Hash a UTF-8 string.
    pub fn of_str(s: &str) -> Self {
        Self::of(s.as_bytes())
    }

    /// Build from a raw 32-byte array (e.g. from a stored value).
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Raw bytes of the checksum.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lower-hex string representation.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl std::fmt::Display for Checksum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl std::fmt::Debug for Checksum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Checksum({})", self.to_hex())
    }
}

/// Incrementally build a checksum by feeding multiple byte slices in order.
///
/// The hash depends on the ordering of all `update()` calls, satisfying
/// the "stable serialization + ordering-dependent" requirement.
pub struct ChecksumBuilder {
    hasher: blake3::Hasher,
}

impl ChecksumBuilder {
    pub fn new() -> Self {
        Self {
            hasher: blake3::Hasher::new(),
        }
    }

    /// Feed bytes in order.
    pub fn update(mut self, data: &[u8]) -> Self {
        self.hasher.update(data);
        self
    }

    /// Feed a string slice in order.
    pub fn update_str(self, s: &str) -> Self {
        self.update(s.as_bytes())
    }

    /// Feed a `u64` in little-endian order.
    pub fn update_u64(self, v: u64) -> Self {
        self.update(&v.to_le_bytes())
    }

    /// Feed a `bool` (0x00 / 0x01).
    pub fn update_bool(self, v: bool) -> Self {
        self.update(&[v as u8])
    }

    /// Consume the builder and produce the final checksum.
    pub fn finish(self) -> Checksum {
        Checksum::from_bytes(*self.hasher.finalize().as_bytes())
    }
}

impl Default for ChecksumBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_input_same_checksum() {
        let a = Checksum::of(b"hello world");
        let b = Checksum::of(b"hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn different_input_different_checksum() {
        let a = Checksum::of(b"hello");
        let b = Checksum::of(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn ordering_matters() {
        let ab = ChecksumBuilder::new()
            .update(b"alpha")
            .update(b"beta")
            .finish();
        let ba = ChecksumBuilder::new()
            .update(b"beta")
            .update(b"alpha")
            .finish();
        assert_ne!(ab, ba, "ordering must affect the checksum");
    }

    #[test]
    fn hex_roundtrip_length() {
        let c = Checksum::of(b"test");
        assert_eq!(c.to_hex().len(), 64);
    }
}
