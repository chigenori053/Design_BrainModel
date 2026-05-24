use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uuid(u128);

impl Uuid {
    pub const fn from_u128(value: u128) -> Self {
        Self(value)
    }

    pub const fn as_u128(&self) -> u128 {
        self.0
    }
}

impl fmt::Debug for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Uuid({:032x})", self.0)
    }
}

pub type NodeId = Uuid;
pub type StateId = Uuid;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
}

#[cfg(test)]
mod tests {
    use super::{Uuid, Value};

    #[test]
    fn value_is_clone_and_debug_compatible() {
        let value = Value::Text("sample".to_string());
        let cloned = value.clone();
        let debugged = format!("{cloned:?}");
        assert!(debugged.contains("sample"));
    }

    #[test]
    fn uuid_is_deterministic_value_type() {
        let id = Uuid::from_u128(42);
        assert_eq!(id.as_u128(), 42);
        assert_eq!(id, Uuid::from_u128(42));
    }
}
