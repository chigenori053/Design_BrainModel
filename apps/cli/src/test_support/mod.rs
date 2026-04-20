pub mod ir_assert;
pub mod resource_guard;

use std::sync::{Mutex, MutexGuard, OnceLock};

/// Global lock for serializing tests that modify `DBM_GH_BIN`.
/// Both `coding` and `autonomous_execute` tests must acquire this
/// before setting `DBM_GH_BIN` to prevent env-var races under parallel
/// test execution.
pub fn gh_bin_env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}
