mod model;
mod view;
mod event;
mod vm_client;
mod app;
mod tui;

use crate::app::AppRoot;
use simplelog::*;
use std::fs::File;

fn main() -> std::io::Result<()> {
    // Initialize Logging
    let _ = WriteLogger::init(
        LevelFilter::Info,
        Config::default(),
        File::create("client.log").unwrap(),
    );

    let mut app = AppRoot::new();
    app.run()
}
