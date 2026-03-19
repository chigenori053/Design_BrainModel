#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkMode {
    Disabled,
    Allowlist(Vec<String>),
    FullAccess,
}

#[derive(Clone, Debug)]
pub struct NetworkGuard {
    pub mode: NetworkMode,
}

impl Default for NetworkGuard {
    fn default() -> Self {
        Self {
            mode: NetworkMode::Disabled,
        }
    }
}

impl NetworkGuard {
    pub fn validate_command(&self, command: &[String]) -> Result<(), String> {
        let is_network_command = command
            .first()
            .map(|program| matches!(program.as_str(), "curl" | "wget" | "ping" | "nc"))
            .unwrap_or(false);
        if !is_network_command {
            return Ok(());
        }
        match &self.mode {
            NetworkMode::Disabled => Err("network guard blocked external access".to_string()),
            NetworkMode::Allowlist(allowed) => {
                let joined = command.join(" ");
                if allowed.iter().any(|host| joined.contains(host)) {
                    Ok(())
                } else {
                    Err("network guard blocked non-allowlisted access".to_string())
                }
            }
            NetworkMode::FullAccess => Ok(()),
        }
    }
}
