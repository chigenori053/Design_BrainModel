pub mod ir_assert;
pub mod resource_guard;

use std::path::{Path, PathBuf};
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

pub fn git_guard_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

fn current_dir_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

pub struct CurrentDirGuard<'a> {
    _lock: MutexGuard<'a, ()>,
    previous: PathBuf,
}

impl CurrentDirGuard<'_> {
    pub fn enter(root: &Path) -> Self {
        let lock = current_dir_lock();
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(root).expect("set cwd");
        Self {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for CurrentDirGuard<'_> {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.previous).expect("restore cwd");
    }
}

pub fn with_current_dir<T>(root: &Path, run: impl FnOnce() -> T) -> T {
    let _guard = CurrentDirGuard::enter(root);
    run()
}
