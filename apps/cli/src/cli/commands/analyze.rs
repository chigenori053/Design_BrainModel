use std::ffi::OsString;
use std::path::PathBuf;

use crate::app;

pub fn run(
    path: PathBuf,
    detailed: bool,
    report: bool,
    design: bool,
    lang: String,
    intent: Option<String>,
    json: bool,
    design_json: bool,
    out: Option<PathBuf>,
    report_md: Option<PathBuf>,
) -> Result<(), String> {
    let mut forwarded = vec![
        OsString::from("design_cli"),
        OsString::from("analyze"),
        path.into_os_string(),
    ];

    if detailed {
        forwarded.push(OsString::from("--detailed"));
    }
    if report {
        forwarded.push(OsString::from("--report"));
    }
    if design {
        forwarded.push(OsString::from("--design"));
    }
    forwarded.push(OsString::from("--lang"));
    forwarded.push(OsString::from(lang));
    if let Some(intent) = intent {
        forwarded.push(OsString::from("--intent"));
        forwarded.push(OsString::from(intent));
    }
    if json {
        forwarded.push(OsString::from("--json"));
    }
    if design_json {
        forwarded.push(OsString::from("--design-json"));
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
