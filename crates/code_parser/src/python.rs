use crate::{
    ParseError, ParseErrorKind,
    ast::{
        AstBlock, AstExpression, AstFunction, AstLiteral, AstLoop, AstLoopKind, AstModule,
        AstStatement,
    },
};

#[derive(Clone, Debug)]
struct Line<'a> {
    number: usize,
    indent: usize,
    text: &'a str,
}

pub fn parse_module(module_name: &str, source: &str) -> Result<AstModule, ParseError> {
    let lines = source
        .lines()
        .enumerate()
        .filter_map(|(index, raw)| {
            if raw.trim().is_empty() {
                None
            } else {
                Some(Line {
                    number: index + 1,
                    indent: raw.chars().take_while(|ch| *ch == ' ').count(),
                    text: raw.trim(),
                })
            }
        })
        .collect::<Vec<_>>();

    let mut imports = Vec::new();
    let mut functions = Vec::new();
    let mut index = 0usize;

    while index < lines.len() {
        let line = &lines[index];
        if line.indent != 0 {
            return Err(ParseError::new(
                ParseErrorKind::ParseError,
                "top-level python statements must not be indented",
                Some(line.number),
            ));
        }
        if is_unsupported_toplevel(line.text) {
            return Err(ParseError::new(
                ParseErrorKind::UnsupportedSyntax,
                format!("unsupported python syntax: {}", line.text),
                Some(line.number),
            ));
        }
        if line.text.starts_with("import ") || line.text.starts_with("from ") {
            imports.push(line.text.to_string());
            index += 1;
            continue;
        }
        if line.text.starts_with("def ") {
            let (function, next_index) = parse_function(&lines, index)?;
            functions.push(function);
            index = next_index;
            continue;
        }
        return Err(ParseError::new(
            ParseErrorKind::ParseError,
            format!("unrecognized python line: {}", line.text),
            Some(line.number),
        ));
    }

    Ok(AstModule {
        name: module_name.to_string(),
        imports,
        functions,
    })
}

fn parse_function(lines: &[Line<'_>], start: usize) -> Result<(AstFunction, usize), ParseError> {
    let line = &lines[start];
    let signature = line
        .text
        .strip_prefix("def ")
        .and_then(|value| value.strip_suffix(':'))
        .ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::ParseError,
                "invalid function header",
                Some(line.number),
            )
        })?;
    let (name, rest) = signature.split_once('(').ok_or_else(|| {
        ParseError::new(ParseErrorKind::ParseError, "expected (", Some(line.number))
    })?;
    let (params_str, tail) = rest.rsplit_once(')').ok_or_else(|| {
        ParseError::new(ParseErrorKind::ParseError, "expected )", Some(line.number))
    })?;
    let return_type = tail
        .trim()
        .strip_prefix("->")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "Void");
    let params = parse_python_params(params_str, line.number)?;
    let block_indent = next_indent(lines, start + 1).ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::ParseError,
            "python function requires an indented body",
            Some(line.number),
        )
    })?;
    let (body, next_index) = parse_block(lines, start + 1, block_indent)?;
    Ok((
        AstFunction {
            name: name.trim().to_string(),
            params,
            return_type,
            body,
        },
        next_index,
    ))
}

fn parse_block(
    lines: &[Line<'_>],
    mut index: usize,
    indent: usize,
) -> Result<(AstBlock, usize), ParseError> {
    let mut statements = Vec::new();
    while index < lines.len() {
        let line = &lines[index];
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(ParseError::new(
                ParseErrorKind::AmbiguousConstruct,
                "unexpected indentation",
                Some(line.number),
            ));
        }
        if line.text == "pass" {
            index += 1;
            continue;
        }
        if line.text.starts_with("if ") && line.text.ends_with(':') {
            let condition = parse_expression(
                line.text
                    .trim_start_matches("if ")
                    .trim_end_matches(':')
                    .trim(),
                line.number,
            )?;
            let then_indent = next_indent(lines, index + 1).ok_or_else(|| {
                ParseError::new(
                    ParseErrorKind::ParseError,
                    "if requires body",
                    Some(line.number),
                )
            })?;
            let (then_block, next_index) = parse_block(lines, index + 1, then_indent)?;
            let (else_block, final_index) = if next_index < lines.len()
                && lines[next_index].indent == indent
                && lines[next_index].text == "else:"
            {
                let else_indent = next_indent(lines, next_index + 1).ok_or_else(|| {
                    ParseError::new(
                        ParseErrorKind::ParseError,
                        "else requires body",
                        Some(lines[next_index].number),
                    )
                })?;
                parse_block(lines, next_index + 1, else_indent)?
            } else {
                (AstBlock::default(), next_index)
            };
            statements.push(AstStatement::If {
                condition,
                then_block,
                else_block,
            });
            index = final_index;
            continue;
        }
        if line.text.starts_with("while ") && line.text.ends_with(':') {
            let condition = parse_expression(
                line.text
                    .trim_start_matches("while ")
                    .trim_end_matches(':')
                    .trim(),
                line.number,
            )?;
            let body_indent = next_indent(lines, index + 1).ok_or_else(|| {
                ParseError::new(
                    ParseErrorKind::ParseError,
                    "while requires body",
                    Some(line.number),
                )
            })?;
            let (body, next_index) = parse_block(lines, index + 1, body_indent)?;
            statements.push(AstStatement::Loop(AstLoop {
                kind: AstLoopKind::While,
                iterator: condition,
                body,
            }));
            index = next_index;
            continue;
        }
        if line.text.starts_with("for ") && line.text.ends_with(':') {
            let spec = line
                .text
                .trim_start_matches("for ")
                .trim_end_matches(':')
                .trim()
                .to_string();
            let body_indent = next_indent(lines, index + 1).ok_or_else(|| {
                ParseError::new(
                    ParseErrorKind::ParseError,
                    "for requires body",
                    Some(line.number),
                )
            })?;
            let (body, next_index) = parse_block(lines, index + 1, body_indent)?;
            statements.push(AstStatement::Loop(AstLoop {
                kind: AstLoopKind::For,
                iterator: AstExpression::Variable(spec),
                body,
            }));
            index = next_index;
            continue;
        }
        statements.push(parse_statement(line)?);
        index += 1;
    }
    Ok((AstBlock { statements }, index))
}

fn parse_statement(line: &Line<'_>) -> Result<AstStatement, ParseError> {
    if line.text.starts_with("return") {
        let rest = line.text.trim_start_matches("return").trim();
        if rest.is_empty() {
            return Ok(AstStatement::Return(None));
        }
        return Ok(AstStatement::Return(Some(parse_expression(
            rest,
            line.number,
        )?)));
    }
    if let Some((target, value)) = line.text.split_once('=') {
        if !target.trim_end().ends_with('!')
            && !target.contains("==")
            && !target.contains(">=")
            && !target.contains("<=")
        {
            return Ok(AstStatement::Assign {
                target: target.trim().to_string(),
                value: parse_expression(value.trim(), line.number)?,
            });
        }
    }
    if line.text.ends_with(')') {
        return Ok(AstStatement::Call(parse_expression(
            line.text,
            line.number,
        )?));
    }
    Err(ParseError::new(
        ParseErrorKind::ParseError,
        format!("unsupported python statement: {}", line.text),
        Some(line.number),
    ))
}

fn parse_python_params(
    params: &str,
    line_no: usize,
) -> Result<Vec<(String, Option<String>)>, ParseError> {
    if params.trim().is_empty() {
        return Ok(vec![]);
    }
    params
        .split(',')
        .map(|param| {
            let param = param.trim();
            if let Some((name, ty)) = param.split_once(':') {
                Ok((name.trim().to_string(), Some(ty.trim().to_string())))
            } else {
                Ok((param.to_string(), None))
            }
        })
        .collect::<Result<Vec<_>, ParseError>>()
        .map_err(|_| {
            ParseError::new(
                ParseErrorKind::ParseError,
                "invalid parameter",
                Some(line_no),
            )
        })
}

fn parse_expression(value: &str, line_no: usize) -> Result<AstExpression, ParseError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(AstExpression::Literal(AstLiteral::Void));
    }
    if ((value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\'')))
        && value.len() >= 2
    {
        return Ok(AstExpression::Literal(AstLiteral::String(
            value[1..value.len() - 1].to_string(),
        )));
    }
    if let Ok(number) = value.parse::<i64>() {
        return Ok(AstExpression::Literal(AstLiteral::Int(number)));
    }
    if value == "True" || value == "true" {
        return Ok(AstExpression::Literal(AstLiteral::Bool(true)));
    }
    if value == "False" || value == "false" {
        return Ok(AstExpression::Literal(AstLiteral::Bool(false)));
    }
    if value.ends_with(')') && value.contains('(') {
        let open = value.find('(').ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::ParseError,
                "expected ( in call",
                Some(line_no),
            )
        })?;
        let close = value.rfind(')').ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::ParseError,
                "expected ) in call",
                Some(line_no),
            )
        })?;
        return Ok(AstExpression::Call {
            function: value[..open].trim().to_string(),
            args: split_arguments(&value[open + 1..close])
                .into_iter()
                .map(|arg| parse_expression(&arg, line_no))
                .collect::<Result<Vec<_>, _>>()?,
        });
    }
    Ok(AstExpression::Variable(value.to_string()))
}

fn split_arguments(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0isize;
    let mut in_string: Option<char> = None;
    for ch in value.chars() {
        match ch {
            '\'' | '"' => {
                if in_string == Some(ch) {
                    in_string = None;
                } else if in_string.is_none() {
                    in_string = Some(ch);
                }
                current.push(ch);
            }
            '(' if in_string.is_none() => {
                depth += 1;
                current.push(ch);
            }
            ')' if in_string.is_none() => {
                depth -= 1;
                current.push(ch);
            }
            ',' if in_string.is_none() && depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

fn next_indent(lines: &[Line<'_>], index: usize) -> Option<usize> {
    lines.get(index).map(|line| line.indent)
}

fn is_unsupported_toplevel(line: &str) -> bool {
    line.starts_with("class ")
        || line.starts_with("match ")
        || line.starts_with("try:")
        || line.starts_with("@")
}
