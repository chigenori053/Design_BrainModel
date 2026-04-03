use crate::session::AgentSession;
use std::path::PathBuf;

pub fn resolve_command_path(args: &[String], session: &AgentSession) -> Result<PathBuf, String> {
    eprintln!("TRACE:R1:ENTER");

    if let Some(arg) = args.first() {
        let resolved = PathBuf::from(arg);
        eprintln!("TRACE:R2:LAST={:?}", None::<String>);
        eprintln!("TRACE:R3:CURRENT={:?}", resolved);
        eprintln!("TRACE:R4:RETURN={:?}", resolved);
        return Ok(resolved);
    }

    let last = session.context.last_path.clone();
    eprintln!("TRACE:R2:LAST={:?}", last);

    let last = last.ok_or_else(|| "MissingLastPath: no stored fallback path".to_string())?;
    let current = PathBuf::from(last);
    eprintln!("TRACE:R3:CURRENT={:?}", current);

    if !current.exists() {
        return Err(format!(
            "MissingLastPath: stored fallback path does not exist: {}",
            current.display()
        ));
    }

    let resolved = current.canonicalize().map_err(|err| {
        format!(
            "InvalidFallbackPath: failed to canonicalize stored fallback path {}: {err}",
            current.display()
        )
    })?;
    eprintln!("TRACE:R4:RETURN={:?}", resolved);
    Ok(resolved)
}
