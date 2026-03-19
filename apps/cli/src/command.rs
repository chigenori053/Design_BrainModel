#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Exit,
    Reset,
    None,
}

pub fn parse_command(input: &str) -> Command {
    match input.trim() {
        "/exit" | "/quit" | "exit" | "quit" => Command::Exit,
        "/reset" | "reset" => Command::Reset,
        _ => Command::None,
    }
}
