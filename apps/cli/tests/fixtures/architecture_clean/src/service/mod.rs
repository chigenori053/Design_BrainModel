use crate::world;

pub mod dto {
    pub struct AnalyzeResultDTO {
        pub summary: String,
    }
}

pub fn status() -> String {
    world::status()
}
