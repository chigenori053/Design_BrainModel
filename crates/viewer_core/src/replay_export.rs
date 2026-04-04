use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::benchmark::{ReplayBenchmarkReport, benchmark_replay};
use crate::model::{StructureSnapshot, StructureViewIR};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReplayExportManifest {
    pub export_dir: String,
    pub files: Vec<String>,
}

pub fn export_demo_replay(
    ir: &StructureViewIR,
    snapshots: &[StructureSnapshot],
    export_dir: &Path,
) -> Result<ReplayExportManifest, String> {
    fs::create_dir_all(export_dir)
        .map_err(|err| format!("failed to create {}: {err}", export_dir.display()))?;

    let mut files = Vec::new();
    write_json(export_dir, "scene_3d.json", &ir.scene_3d)?;
    files.push("scene_3d.json".to_string());

    write_json(export_dir, "timeline.delta", snapshots)?;
    files.push("timeline.delta".to_string());

    let camera = ir.scene_3d.as_ref().map(|scene| &scene.camera);
    write_json(export_dir, "camera_preset.json", &camera)?;
    files.push("camera_preset.json".to_string());

    let telemetry = ir.scene_3d.as_ref().map(|scene| &scene.overlays.telemetry);
    write_json(export_dir, "telemetry.json", &telemetry)?;
    files.push("telemetry.json".to_string());

    let benchmark: ReplayBenchmarkReport = benchmark_replay(snapshots);
    write_json(export_dir, "benchmark.json", &benchmark)?;
    files.push("benchmark.json".to_string());

    Ok(ReplayExportManifest {
        export_dir: export_dir.display().to_string(),
        files,
    })
}

fn write_json<T: Serialize + ?Sized>(dir: &Path, file_name: &str, value: &T) -> Result<(), String> {
    let path = dir.join(file_name);
    let body = serde_json::to_string_pretty(value).map_err(|err| err.to_string())?;
    fs::write(&path, body).map_err(|err| format!("failed to write {}: {err}", path.display()))
}
