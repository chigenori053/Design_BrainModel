use code_ir::{CodeIr, IrFunction, IrOp, IrStep};
use crate::{
    error::CodegenError,
    scope::{validate_steps, ScopeStack},
    spec::LanguageSpec,
    type_render::render_type,
};

// ── Program / Function generation (Step4) ────────────────────────────────────

/// Validate and render a single `IrFunction` to source text.
pub fn generate_function(
    func: &IrFunction,
    spec: &LanguageSpec,
) -> Result<String, CodegenError> {
    // Validation
    if func.name.is_empty() {
        return Err(CodegenError::EmptyFunctionName);
    }
    {
        let mut seen = std::collections::HashSet::new();
        for p in &func.params {
            if p.name.is_empty() {
                return Err(CodegenError::EmptyParamName { function: func.name.clone() });
            }
            if !seen.insert(&p.name) {
                return Err(CodegenError::DuplicateParam {
                    function: func.name.clone(),
                    param: p.name.clone(),
                });
            }
        }
    }

    // Scope resolution
    let mut scope = ScopeStack::new();
    for p in &func.params {
        scope.declare(&p.name, p.ty.clone()).map_err(|_| CodegenError::DuplicateParam {
            function: func.name.clone(),
            param: p.name.clone(),
        })?;
    }
    let scope_errors = validate_steps(&func.body, &mut scope);
    if let Some(e) = scope_errors.into_iter().next() {
        return Err(e);
    }

    // Render
    let param_list = render_params(func, spec)?;
    let body_gen = StructuredCodeGenerator::new(spec.clone());
    let body_str = body_gen.emit_indented(&func.body, 1);

    let out = match spec.name.as_str() {
        "python" => {
            let ret_ann = match &func.return_type {
                Some(ty) => format!(" -> {}", render_type(ty, spec)?),
                None => String::new(),
            };
            format!("def {}({}){}:\n{}", func.name, param_list, ret_ann, body_str)
        }
        _ => {
            // Rust (default)
            let ret_ann = match &func.return_type {
                Some(ty) if *ty != code_ir::IrType::Void => {
                    format!(" -> {}", render_type(ty, spec)?)
                }
                _ => String::new(),
            };
            format!("fn {}({}){} {{\n{}}}\n", func.name, param_list, ret_ann, body_str)
        }
    };

    Ok(out)
}

/// Render the full `CodeIr.functions` list to a single source file.
pub fn generate_program(ir: &CodeIr, spec: &LanguageSpec) -> Result<String, CodegenError> {
    let mut out = String::new();
    for func in &ir.functions {
        out.push_str(&generate_function(func, spec)?);
        out.push('\n');
    }
    Ok(out)
}

fn render_params(func: &IrFunction, spec: &LanguageSpec) -> Result<String, CodegenError> {
    let parts: Result<Vec<String>, CodegenError> = func
        .params
        .iter()
        .map(|p| {
            if spec.name == "python" {
                Ok(p.name.clone())
            } else {
                // Rust-style: name: Type
                match &p.ty {
                    Some(ty) => Ok(format!("{}: {}", p.name, render_type(ty, spec)?)),
                    None => Ok(p.name.clone()),
                }
            }
        })
        .collect();
    Ok(parts?.join(", "))
}

/// Emits a list of `IrStep`s to a source string according to a `LanguageSpec`.
///
/// Rules:
/// - Indentation is `spec.formatting.indent` repeated `indent_level` times.
/// - Block start increments level by 1; Block end decrements by 1.
/// - No heuristic whitespace adjustment — output is fully deterministic.
pub struct StructuredCodeGenerator {
    pub spec: LanguageSpec,
}

impl StructuredCodeGenerator {
    pub fn new(spec: LanguageSpec) -> Self {
        Self { spec }
    }

    pub fn emit_steps(&self, steps: &[IrStep]) -> String {
        self.emit_indented(steps, 0)
    }

    // ── internal ─────────────────────────────────────────────────────────

    fn indent(&self, level: usize) -> String {
        self.spec.formatting.indent.repeat(level)
    }

    fn emit_indented(&self, steps: &[IrStep], level: usize) -> String {
        steps.iter().map(|s| self.emit_step(s, level)).collect()
    }

    fn emit_step(&self, step: &IrStep, level: usize) -> String {
        match step.op {
            IrOp::Assign => self.emit_assign(step, level),
            IrOp::Call => self.emit_call(step, level),
            IrOp::Return => self.emit_return(step, level),
            IrOp::Branch => self.emit_branch(step, level),
            IrOp::Block => self.emit_block_scope(step, level),
            IrOp::Loop => self.emit_loop(step, level),
        }
    }

    fn emit_assign(&self, step: &IrStep, level: usize) -> String {
        let out = step.outputs.first().map(|v| v.name.as_str()).unwrap_or("_");
        let inp = step.inputs.first().map(|v| v.name.as_str()).unwrap_or("()");
        format!("{}let {} = {};\n", self.indent(level), out, inp)
    }

    fn emit_call(&self, step: &IrStep, level: usize) -> String {
        let func = step.outputs.first().map(|v| v.name.as_str()).unwrap_or("f");
        let args: Vec<&str> = step.inputs.iter().map(|v| v.name.as_str()).collect();
        format!("{}{}({});\n", self.indent(level), func, args.join(", "))
    }

    fn emit_return(&self, step: &IrStep, level: usize) -> String {
        let val = step.inputs.first().map(|v| v.name.as_str()).unwrap_or("()");
        format!("{}return {};\n", self.indent(level), val)
    }

    fn emit_branch(&self, step: &IrStep, level: usize) -> String {
        let pfx = self.indent(level);
        let cond = step.condition.as_ref().map(|e| e.text.as_str()).unwrap_or("true");
        let body = step.body.as_deref().unwrap_or(&[]);
        let body_str = self.emit_indented(body, level + 1);

        let mut out = String::new();
        if self.spec.formatting.use_braces {
            out.push_str(&format!("{}if {} {{\n", pfx, cond));
            out.push_str(&body_str);
            match step.else_body.as_deref() {
                Some(else_steps) => {
                    let else_str = self.emit_indented(else_steps, level + 1);
                    out.push_str(&format!("{}}} else {{\n", pfx));
                    out.push_str(&else_str);
                    out.push_str(&format!("{}}}\n", pfx));
                }
                None => out.push_str(&format!("{}}}\n", pfx)),
            }
        } else {
            out.push_str(&format!("{}if {}:\n", pfx, cond));
            out.push_str(&body_str);
            if let Some(else_steps) = step.else_body.as_deref() {
                let else_str = self.emit_indented(else_steps, level + 1);
                out.push_str(&format!("{}else:\n", pfx));
                out.push_str(&else_str);
            }
        }
        out
    }

    fn emit_block_scope(&self, step: &IrStep, level: usize) -> String {
        let pfx = self.indent(level);
        let body = step.body.as_deref().unwrap_or(&[]);
        let body_str = self.emit_indented(body, level + 1);

        if self.spec.formatting.use_braces {
            format!("{}{{\n{}{}}}\n", pfx, body_str, pfx)
        } else {
            body_str
        }
    }

    fn emit_loop(&self, step: &IrStep, level: usize) -> String {
        let pfx = self.indent(level);
        let iter_expr = step.inputs.first().map(|v| v.name.as_str()).unwrap_or("item in items");
        let body = step.body.as_deref().unwrap_or(&[]);
        let body_str = self.emit_indented(body, level + 1);

        if self.spec.formatting.use_braces {
            format!("{}for {} {{\n{}{}}}\n", pfx, iter_expr, body_str, pfx)
        } else {
            format!("{}for {}:\n{}", pfx, iter_expr, body_str)
        }
    }
}
