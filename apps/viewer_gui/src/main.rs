use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use design_cli::viewer::nl_dispatch::{NlContext, dispatch_nl};
use viewer_core::model::{ActionRequest, ValidationOverlay, ViewMode};
use viewer_core::native::{LaunchRequest, launch_native_viewer};
use viewer_gui::ipc::{self, DispatchResult};

#[derive(Parser, Debug)]
#[command(name = "dbm_viewer", version, about = "Native DBM structure viewer")]
struct Args {
    #[arg(long, value_parser = ["2d", "3d"])]
    mode: String,
    #[arg(long)]
    ir: PathBuf,
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long)]
    cli: Option<PathBuf>,
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    let mode = if args.mode == "3d" {
        ViewMode::ThreeD
    } else {
        ViewMode::TwoD
    };
    let ir_path = args.ir;
    let root = args.root.unwrap_or_else(|| {
        ir_path
            .parent()
            .and_then(|path| path.parent())
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf()
    });
    let cli = args.cli.unwrap_or_else(resolve_cli_path);
    let dispatch_root = root.clone();
    let nl_root = root.clone();
    let request = LaunchRequest {
        mode,
        ir_path,
        root,
        diagnostics: ValidationOverlay::default(),
        dispatch_action: Arc::new(move |request: ActionRequest| {
            let action_path = ipc::write_action(
                &dispatch_root,
                &ipc::ActionRequest {
                    target: request.target,
                    node: request.node,
                    selected_nodes: request.selected_nodes,
                    mode: match request.mode {
                        viewer_core::model::GuiActionMode::Preview => {
                            design_cli::refactor::GuiActionMode::Preview
                        }
                        viewer_core::model::GuiActionMode::Apply => {
                            design_cli::refactor::GuiActionMode::Apply
                        }
                    },
                },
            )?;
            let result: DispatchResult = ipc::dispatch_action(&cli, &dispatch_root, &action_path)?;
            Ok(result.stdout)
        }),
        source_path_for_node: Arc::new(|_| None),
        dispatch_nl: Arc::new(move |prompt: &str, selected_node: Option<&str>| {
            let ctx = NlContext {
                prompt: prompt.to_string(),
                selected_node: selected_node.map(str::to_string),
                root: nl_root.clone(),
            };
            let result = dispatch_nl(&ctx);
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }),
    };
    launch_native_viewer(request).map(|_| ())
}

fn resolve_cli_path() -> PathBuf {
    if let Ok(path) = std::env::var("DESIGN_CLI_BIN") {
        return PathBuf::from(path);
    }
    if let Ok(current) = std::env::current_exe() {
        let sibling = current.with_file_name(if cfg!(target_os = "windows") {
            "design_cli.exe"
        } else {
            "design_cli"
        });
        if sibling.exists() {
            return sibling;
        }
    }
    PathBuf::from("design_cli")
}
