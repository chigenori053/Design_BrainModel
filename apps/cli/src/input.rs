use std::io::{self, BufRead, Write};

use crate::state::State;

#[derive(Debug, PartialEq, Eq)]
pub enum InputState {
    Line(String),
    Eof,
}

pub fn read_input<R, W>(reader: &mut R, writer: &mut W, state: State) -> io::Result<InputState>
where
    R: BufRead,
    W: Write,
{
    print_prompt(writer, state)?;
    let mut input = String::new();
    let bytes = reader.read_line(&mut input)?;
    if bytes == 0 {
        return Ok(InputState::Eof);
    }
    Ok(InputState::Line(input.trim().to_string()))
}

pub fn read_input_with_label<R, W>(
    reader: &mut R,
    writer: &mut W,
    state: State,
    label: Option<&str>,
) -> io::Result<InputState>
where
    R: BufRead,
    W: Write,
{
    print_prompt_with_label(writer, state, label)?;
    let mut input = String::new();
    let bytes = reader.read_line(&mut input)?;
    if bytes == 0 {
        return Ok(InputState::Eof);
    }
    Ok(InputState::Line(input.trim().to_string()))
}

pub fn print_prompt<W: Write>(writer: &mut W, state: State) -> io::Result<()> {
    print_prompt_with_label(writer, state, None)
}

pub fn print_prompt_with_label<W: Write>(
    writer: &mut W,
    state: State,
    label: Option<&str>,
) -> io::Result<()> {
    let indicator = match state {
        State::Idle | State::Completed => "DBM",
        State::Running | State::Planning => "DBM..",
        State::Error => "DBM!",
        State::Ready => "DBM?",
    };
    match label {
        Some(label) if !label.is_empty() => write!(writer, "{indicator}[{label}] > ")?,
        _ => write!(writer, "{indicator} > ")?,
    }
    writer.flush()
}
