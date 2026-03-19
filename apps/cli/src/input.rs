use std::io::{self, BufRead, Write};

#[derive(Debug, PartialEq, Eq)]
pub enum InputState {
    Line(String),
    Eof,
}

pub fn read_input<R, W>(reader: &mut R, writer: &mut W) -> io::Result<InputState>
where
    R: BufRead,
    W: Write,
{
    print_prompt(writer)?;
    let mut input = String::new();
    let bytes = reader.read_line(&mut input)?;
    if bytes == 0 {
        return Ok(InputState::Eof);
    }
    Ok(InputState::Line(input.trim().to_string()))
}

pub fn print_prompt<W: Write>(writer: &mut W) -> io::Result<()> {
    write!(writer, "> ")?;
    writer.flush()
}
