use std::mem;

use crate::{ExecutionMode, HybridVm, RuntimeContext};

pub struct TestRuntimeContext {
    runtime: RuntimeContext,
}

impl Default for TestRuntimeContext {
    fn default() -> Self {
        Self::new()
    }
}

impl TestRuntimeContext {
    pub fn new() -> Self {
        Self {
            runtime: RuntimeContext::new(),
        }
    }

    pub fn runtime(&mut self) -> &mut RuntimeContext {
        &mut self.runtime
    }
}

impl Drop for TestRuntimeContext {
    fn drop(&mut self) {
        self.runtime.release_completed_task_memory();
        self.runtime.force_clear_all();
    }
}

pub fn with_test_vm<R>(
    runtime: &mut RuntimeContext,
    mode: ExecutionMode,
    body: impl FnOnce(&mut HybridVm) -> R,
) -> R {
    runtime.release_completed_task_memory();
    runtime.force_clear_all();
    let mut vm = HybridVm::with_context(mode, mem::take(runtime));
    let result = body(&mut vm);
    *runtime = vm.into_context();
    result
}

#[macro_export]
macro_rules! dbm_test {
    ($name:ident, $runtime:ident, $body:block) => {
        #[test]
        fn $name() {
            let mut __ctx = $crate::test_support::TestRuntimeContext::new();
            let $runtime = __ctx.runtime();
            $body
        }
    };
    ($name:ident, $(#[$attr:meta])+ , $runtime:ident, $body:block) => {
        #[test]
        $(#[$attr])*
        fn $name() {
            let mut __ctx = $crate::test_support::TestRuntimeContext::new();
            let $runtime = __ctx.runtime();
            $body
        }
    };
}
