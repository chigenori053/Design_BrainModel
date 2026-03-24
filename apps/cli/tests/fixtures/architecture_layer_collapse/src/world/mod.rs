use crate::adapter;

#[derive(Clone)]
pub struct WorldState {
    pub name: String,
}

pub fn state() -> WorldState {
    adapter::load()
}
