use runtime_vm::RuntimeVm;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    Tick,
    Exit,
    Noop,
}

pub trait UserInterface {
    fn render(&mut self);
    fn handle_input(&mut self, input: UiEvent);
}

#[derive(Debug, Default)]
pub struct VmBridge {
    vm: RuntimeVm,
}

impl VmBridge {
    pub fn new() -> Self {
        Self {
            vm: RuntimeVm::new(),
        }
    }

    pub fn tick(&mut self) {
        let _ = self.vm.tick();
    }

    pub fn current_tick(&self) -> u64 {
        self.vm.state().tick
    }
}
