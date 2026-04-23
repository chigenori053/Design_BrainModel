use crate::error::CodegenError;
use crate::ir::IrStep;
use crate::spec::EmitNode;

use super::context::EmitContext;
use super::placeholder_resolver::resolve_placeholder;

pub fn emit_node(
    node: &EmitNode,
    step: &IrStep,
    ctx: &mut EmitContext<'_>,
) -> Result<String, CodegenError> {
    match node {
        EmitNode::Text(s) => Ok(s.clone()),

        EmitNode::Placeholder(p) => resolve_placeholder(step, p),

        EmitNode::Sequence(nodes) => {
            let mut out = String::new();
            for n in nodes {
                out.push_str(&emit_node(n, step, ctx)?);
            }
            Ok(out)
        }

        EmitNode::Join { items, separator } => {
            let parts: Result<Vec<String>, CodegenError> =
                items.iter().map(|n| emit_node(n, step, ctx)).collect();
            Ok(parts?.join(separator))
        }

        EmitNode::Optional { condition, node } => {
            let should_emit = match condition.as_str() {
                "condition" => step.condition.is_some(),
                _ => false,
            };
            if should_emit { emit_node(node, step, ctx) } else { Ok(String::new()) }
        }

        EmitNode::Indent(node) => {
            ctx.indent_level += 1;
            let prefix = ctx.current_indent();
            let inner = emit_node(node, step, ctx)?;
            ctx.indent_level -= 1;
            Ok(format!("{}{}", prefix, inner))
        }

        EmitNode::NewLine => Ok(ctx.spec.formatting.newline.clone()),
    }
}
