use crate::{
    ParseError, ParseErrorKind,
    ast::{
        AstBlock, AstExpression, AstFunction, AstLiteral, AstLoop, AstLoopKind, AstModule,
        AstStatement,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockEnd {
    Close,
    Else,
    Eof,
}

pub fn parse_module(module_name: &str, source: &str) -> Result<AstModule, ParseError> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut imports = Vec::new();
    let mut functions = Vec::new();
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty() {
            index += 1;
            continue;
        }
        if is_unsupported_toplevel(line) {
            return Err(ParseError::new(
                ParseErrorKind::UnsupportedSyntax,
                format!("unsupported rust syntax: {line}"),
                Some(index + 1),
            ));
        }
        if line.starts_with("use ") {
            imports.push(line.trim_end_matches(';').to_string());
            index += 1;
            continue;
        }
        if is_function_header(line) {
            let (function, next_index) = parse_function(&lines, index)?;
            functions.push(function);
            index = next_index;
            continue;
        }
        return Err(ParseError::new(
            ParseErrorKind::ParseError,
            format!("unrecognized rust line: {line}"),
            Some(index + 1),
        ));
    }

    Ok(AstModule {
        name: module_name.to_string(),
        imports,
        functions,
    })
}

fn parse_function(lines: &[&str], start: usize) -> Result<(AstFunction, usize), ParseError> {
    let header = lines[start].trim();
    let signature = header
        .strip_suffix('{')
        .ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::ParseError,
                "expected function body",
                Some(start + 1),
            )
        })?
        .trim();
    let signature = signature
        .trim_start_matches("pub ")
        .trim_start_matches("async ")
        .trim();
    let signature = signature.strip_prefix("fn ").ok_or_else(|| {
        ParseError::new(ParseErrorKind::ParseError, "expected fn", Some(start + 1))
    })?;
    let (name, rest) = signature.split_once('(').ok_or_else(|| {
        ParseError::new(ParseErrorKind::ParseError, "expected (", Some(start + 1))
    })?;
    let (params_str, tail) = rest.rsplit_once(')').ok_or_else(|| {
        ParseError::new(ParseErrorKind::ParseError, "expected )", Some(start + 1))
    })?;
    let return_type = tail
        .trim()
        .strip_prefix("->")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "Void");
    let params = parse_rust_params(params_str, start + 1)?;
    let (body, next_index, end) = parse_block(lines, start + 1)?;
    if end == BlockEnd::Else {
        return Err(ParseError::new(
            ParseErrorKind::AmbiguousConstruct,
            "dangling else at function scope",
            Some(next_index),
        ));
    }
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
    lines: &[&str],
    mut index: usize,
) -> Result<(AstBlock, usize, BlockEnd), ParseError> {
    let mut statements = Vec::new();
    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty() {
            index += 1;
            continue;
        }
        if line == "}" {
            return Ok((AstBlock { statements }, index + 1, BlockEnd::Close));
        }
        if line == "} else {" {
            return Ok((AstBlock { statements }, index + 1, BlockEnd::Else));
        }
        if line.starts_with("if ") && line.ends_with('{') {
            let condition = parse_expression(
                line.trim_start_matches("if ").trim_end_matches('{').trim(),
                index + 1,
            )?;
            let (then_block, next_index, end) = parse_block(lines, index + 1)?;
            let (else_block, final_index) = if end == BlockEnd::Else {
                let (else_block, final_index, final_end) = parse_block(lines, next_index)?;
                if final_end == BlockEnd::Else {
                    return Err(ParseError::new(
                        ParseErrorKind::AmbiguousConstruct,
                        "nested else chain is unsupported",
                        Some(final_index),
                    ));
                }
                (else_block, final_index)
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
        if line.starts_with("while ") && line.ends_with('{') {
            let condition = parse_expression(
                line.trim_start_matches("while ")
                    .trim_end_matches('{')
                    .trim(),
                index + 1,
            )?;
            let (body, next_index, end) = parse_block(lines, index + 1)?;
            if end != BlockEnd::Close {
                return Err(ParseError::new(
                    ParseErrorKind::AmbiguousConstruct,
                    "while block must end with }",
                    Some(next_index),
                ));
            }
            statements.push(AstStatement::Loop(AstLoop {
                kind: AstLoopKind::While,
                iterator: condition,
                body,
            }));
            index = next_index;
            continue;
        }
        if line.starts_with("for ") && line.ends_with('{') {
            let spec = line
                .trim_start_matches("for ")
                .trim_end_matches('{')
                .trim()
                .to_string();
            let (body, next_index, end) = parse_block(lines, index + 1)?;
            if end != BlockEnd::Close {
                return Err(ParseError::new(
                    ParseErrorKind::AmbiguousConstruct,
                    "for block must end with }",
                    Some(next_index),
                ));
            }
            statements.push(AstStatement::Loop(AstLoop {
                kind: AstLoopKind::For,
                iterator: AstExpression::Variable(spec),
                body,
            }));
            index = next_index;
            continue;
        }
        statements.push(parse_statement(line, index + 1)?);
        index += 1;
    }

    Ok((AstBlock { statements }, index, BlockEnd::Eof))
}

fn parse_statement(line: &str, line_no: usize) -> Result<AstStatement, ParseError> {
    if line.starts_with("return") {
        let rest = line
            .trim_end_matches(';')
            .trim_start_matches("return")
            .trim();
        if rest.is_empty() {
            return Ok(AstStatement::Return(None));
        }
        return Ok(AstStatement::Return(Some(parse_expression(rest, line_no)?)));
    }
    if let Some(rest) = line.strip_prefix("let ") {
        let (target, value) = rest.trim_end_matches(';').split_once('=').ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::ParseError,
                "invalid assignment",
                Some(line_no),
            )
        })?;
        return Ok(AstStatement::Assign {
            target: target.trim().trim_start_matches("mut ").trim().to_string(),
            value: parse_expression(value.trim(), line_no)?,
        });
    }
    if let Some((target, value)) = line.trim_end_matches(';').split_once('=') {
        if !target.contains("==") {
            return Ok(AstStatement::Assign {
                target: target.trim().to_string(),
                value: parse_expression(value.trim(), line_no)?,
            });
        }
    }
    if line.ends_with(");") {
        return Ok(AstStatement::Call(parse_expression(
            line.trim_end_matches(';'),
            line_no,
        )?));
    }
    Err(ParseError::new(
        ParseErrorKind::ParseError,
        format!("unsupported rust statement: {line}"),
        Some(line_no),
    ))
}

fn parse_rust_params(
    params: &str,
    line_no: usize,
) -> Result<Vec<(String, Option<String>)>, ParseError> {
    if params.trim().is_empty() {
        return Ok(vec![]);
    }
    params
        .split(',')
        .map(|param| {
            let (name, ty) = param.split_once(':').ok_or_else(|| {
                ParseError::new(
                    ParseErrorKind::ParseError,
                    format!("expected typed parameter: {param}"),
                    Some(line_no),
                )
            })?;
            Ok((name.trim().to_string(), Some(ty.trim().to_string())))
        })
        .collect()
}

fn parse_expression(value: &str, line_no: usize) -> Result<AstExpression, ParseError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(AstExpression::Literal(AstLiteral::Void));
    }
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        return Ok(AstExpression::Literal(AstLiteral::String(
            value[1..value.len() - 1].to_string(),
        )));
    }
    if let Ok(number) = value.parse::<i64>() {
        return Ok(AstExpression::Literal(AstLiteral::Int(number)));
    }
    if value == "true" {
        return Ok(AstExpression::Literal(AstLiteral::Bool(true)));
    }
    if value == "false" {
        return Ok(AstExpression::Literal(AstLiteral::Bool(false)));
    }
    if value.ends_with(')') && value.contains('(') {
        let (function, args) = split_call(value, line_no)?;
        return Ok(AstExpression::Call {
            function,
            args: parse_args(&args, line_no)?,
        });
    }
    Ok(AstExpression::Variable(value.to_string()))
}

fn parse_args(args: &str, line_no: usize) -> Result<Vec<AstExpression>, ParseError> {
    if args.trim().is_empty() {
        return Ok(vec![]);
    }
    split_arguments(args)
        .into_iter()
        .map(|arg| parse_expression(&arg, line_no))
        .collect()
}

fn split_call(value: &str, line_no: usize) -> Result<(String, String), ParseError> {
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
    Ok((
        value[..open].trim().to_string(),
        value[open + 1..close].trim().to_string(),
    ))
}

fn split_arguments(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0isize;
    let mut in_string = false;
    for ch in value.chars() {
        match ch {
            '"' => {
                in_string = !in_string;
                current.push(ch);
            }
            '(' if !in_string => {
                depth += 1;
                current.push(ch);
            }
            ')' if !in_string => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !in_string && depth == 0 => {
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

fn is_function_header(line: &str) -> bool {
    (line.starts_with("fn ")
        || line.starts_with("pub fn ")
        || line.starts_with("async fn ")
        || line.starts_with("pub async fn "))
        && line.ends_with('{')
}

fn is_unsupported_toplevel(line: &str) -> bool {
    line.starts_with("match ")
        || line.starts_with("impl ")
        || line.starts_with("struct ")
        || line.starts_with("enum ")
        || line.starts_with("trait ")
}
