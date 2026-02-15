use interface_ui::{UiEvent, UserInterface, VmBridge};

struct CliUi {
    bridge: VmBridge,
}

impl CliUi {
    fn new() -> Self {
        Self {
            bridge: VmBridge::new(),
        }
    }
}

impl UserInterface for CliUi {
    fn render(&mut self) {
        println!("tick={}", self.bridge.current_tick());
    }

    fn handle_input(&mut self, input: UiEvent) {
        if let UiEvent::Tick = input {
            self.bridge.tick();
        }
    }
}

fn main() {
    let mut ui = CliUi::new();
    ui.render();
    ui.handle_input(UiEvent::Tick);
    ui.render();
}
