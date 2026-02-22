use std::path::PathBuf;

use core_types::ObjectiveVector;

#[derive(Clone, Debug, PartialEq)]
pub enum AgentEvent {
    RequestSearch { query: String },
    PersistMemory { key: String, value: Vec<u8> },
    WriteRawObjectives {
        path: PathBuf,
        depth: usize,
        objectives: Vec<ObjectiveVector>,
    },
    EmitTelemetry(TelemetryEvent),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelemetryEvent {
    pub name: String,
    pub value: String,
}
