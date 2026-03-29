use std::ffi::OsString;
use std::path::PathBuf;

use crate::app;

pub fn run(
    path: PathBuf,
    json: bool,
    out: Option<PathBuf>,
    report_md: Option<PathBuf>,
) -> Result<(), String> {
    let mut forwarded = vec![
        OsString::from("design_cli"),
        OsString::from("analyze"),
        path.into_os_string(),
    ];

    if json {
        forwarded.push(OsString::from("--json"));
    }
    if let Some(out) = out {
        forwarded.push(OsString::from("--out"));
        forwarded.push(out.into_os_string());
    }
    if let Some(report_md) = report_md {
        forwarded.push(OsString::from("--report-md"));
        forwarded.push(report_md.into_os_string());
    }

    app::run_with_args(forwarded)
}
