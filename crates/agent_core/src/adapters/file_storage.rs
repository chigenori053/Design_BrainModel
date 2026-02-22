use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use core_types::ObjectiveVector;

use crate::domain::DomainError;

pub fn append_raw_objectives(
    path: &Path,
    depth: usize,
    candidates: &[ObjectiveVector],
) -> Result<(), DomainError> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| DomainError::PortError(format!("failed to open raw trace file: {e}")))?;

    if depth == 1 {
        writeln!(
            file,
            "depth,candidate_id,objective_0,objective_1,objective_2,objective_3_shape"
        )
        .map_err(|e| DomainError::PortError(format!("failed to write raw trace header: {e}")))?;
    }

    for (i, obj) in candidates.iter().enumerate() {
        writeln!(
            file,
            "{},{},{},{},{},{}",
            depth, i, obj.f_struct, obj.f_field, obj.f_risk, obj.f_shape
        )
        .map_err(|e| DomainError::PortError(format!("failed to append raw trace row: {e}")))?;
    }

    Ok(())
}
