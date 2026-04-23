use crate::policy_model::SearchPolicy;

/// Versioned policy store with last-stable-policy tracking.
/// Supports replay: retrieving the policy at any past version.
#[derive(Clone, Debug, Default)]
pub struct PolicyStore {
    history: Vec<SearchPolicy>,
    last_stable_version: Option<u64>,
}

impl PolicyStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Save a policy. If it is marked stable, record it as the last stable checkpoint.
    pub fn save(&mut self, policy: SearchPolicy, is_stable: bool) {
        if is_stable {
            self.last_stable_version = Some(policy.version);
        }
        self.history.push(policy);
    }

    pub fn latest(&self) -> Option<&SearchPolicy> {
        self.history.last()
    }

    /// Retrieve the last policy that was explicitly marked stable.
    pub fn last_stable(&self) -> Option<&SearchPolicy> {
        let stable_ver = self.last_stable_version?;
        self.history.iter().rev().find(|p| p.version == stable_ver)
    }

    /// Retrieve the policy at an exact version (for replay).
    pub fn at_version(&self, version: u64) -> Option<&SearchPolicy> {
        self.history.iter().find(|p| p.version == version)
    }

    pub fn version_count(&self) -> usize {
        self.history.len()
    }
}
