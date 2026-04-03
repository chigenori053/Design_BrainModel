use std::path::PathBuf;

use crate::app::{ViewerApp, ViewerAppConfig};
use crate::model::{
    DispatchAction, DispatchNl, SourcePathResolver, ValidationOverlay, ViewMode,
    ViewerLoopTelemetry,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LaunchResult {
    pub url: String,
    pub launched: bool,
    pub telemetry: ViewerLoopTelemetry,
}

#[derive(Clone)]
pub struct LaunchRequest {
    pub mode: ViewMode,
    pub ir_path: PathBuf,
    pub root: PathBuf,
    pub diagnostics: ValidationOverlay,
    pub dispatch_action: DispatchAction,
    pub source_path_for_node: SourcePathResolver,
    pub dispatch_nl: DispatchNl,
}

pub fn launch_native_viewer(request: LaunchRequest) -> Result<LaunchResult, String> {
    let descriptor = format!(
        "embedded://viewer_core?mode={}&ir={}&root={}",
        request.mode.as_str(),
        request.ir_path.display(),
        request.root.display()
    );

    if std::env::var("DBM_VIEWER_SKIP_OPEN").as_deref() == Ok("1") {
        return Ok(LaunchResult {
            url: descriptor,
            launched: false,
            telemetry: ViewerLoopTelemetry::default(),
        });
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1360.0, 900.0]),
        ..Default::default()
    };
    let title = "DBM Embedded Structure Viewer".to_string();
    eframe::run_native(
        &title,
        native_options,
        Box::new(|cc| {
            Ok(Box::new(ViewerApp::new(
                cc,
                ViewerAppConfig {
                    mode: request.mode,
                    ir_path: request.ir_path.clone(),
                    root: request.root.clone(),
                    diagnostics: request.diagnostics.clone(),
                    dispatch_action: request.dispatch_action.clone(),
                    source_path_for_node: request.source_path_for_node.clone(),
                    dispatch_nl: request.dispatch_nl.clone(),
                },
            )))
        }),
    )
    .map_err(|err| format!("failed to launch embedded viewer: {err}"))?;

    Ok(LaunchResult {
        url: descriptor,
        launched: true,
        telemetry: ViewerLoopTelemetry::default(),
    })
}
