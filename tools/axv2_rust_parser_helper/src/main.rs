use std::env;
use std::fs;
use std::process;

use syn::visit::{self, Visit};
use syn::{ExprCall, ExprMethodCall, File, ImplItem, Item, ItemImpl, ItemStruct, ItemTrait, UseTree};

fn main() {
    let path = match env::args().nth(1) {
        Some(path) => path,
        None => {
            eprintln!("missing source path");
            process::exit(1);
        }
    };

    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("failed to read {path}: {err}");
            process::exit(1);
        }
    };

    let ast: File = match syn::parse_file(&source) {
        Ok(ast) => ast,
        Err(err) => {
            eprintln!("failed to parse {path}: {err}");
            process::exit(1);
        }
    };

    let module = std::path::Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module")
        .to_string();

    let mut structs = Vec::new();
    let mut traits = Vec::new();
    let mut uses = Vec::new();
    let mut calls = Vec::new();
    let mut types = Vec::new();

    for item in ast.items {
        match item {
            Item::Struct(ItemStruct { ident, .. }) => {
                structs.push(format!("{module}::{ident}"));
            }
            Item::Trait(ItemTrait { ident, .. }) => {
                traits.push(format!("{module}::{ident}"));
            }
            Item::Use(item_use) => {
                collect_use_tree(&item_use.tree, String::new(), &mut uses);
            }
            Item::Impl(item_impl) => collect_impl(item_impl, &module, &mut calls, &mut types),
            _ => {}
        }
    }

    for value in structs {
        println!("STRUCT {value}");
    }
    for value in traits {
        println!("TRAIT {value}");
    }
    for value in uses {
        println!("USE {value}");
    }
    for (src, dst) in calls {
        println!("CALL {src}|{dst}");
    }
    for (src, dst) in types {
        println!("TYPE {src}|{dst}");
    }
}

fn collect_use_tree(tree: &UseTree, prefix: String, out: &mut Vec<String>) {
    match tree {
        UseTree::Path(path) => {
            let next = if prefix.is_empty() {
                path.ident.to_string()
            } else {
                format!("{prefix}::{}", path.ident)
            };
            collect_use_tree(&path.tree, next, out);
        }
        UseTree::Name(name) => {
            if prefix.is_empty() {
                out.push(name.ident.to_string());
            } else {
                out.push(format!("{prefix}::{}", name.ident));
            }
        }
        UseTree::Rename(rename) => {
            if prefix.is_empty() {
                out.push(rename.ident.to_string());
            } else {
                out.push(format!("{prefix}::{}", rename.ident));
            }
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_use_tree(item, prefix.clone(), out);
            }
        }
        UseTree::Glob(_) => {
            out.push(prefix);
        }
    }
}

fn collect_impl(item_impl: ItemImpl, module: &str, calls: &mut Vec<(String, String)>, types: &mut Vec<(String, String)>) {
    let self_ty = type_name(&item_impl.self_ty);
    let self_name = self_ty.rsplit("::").next().unwrap_or(&self_ty).to_string();
    let self_module_name = to_snake_case(&self_name);
    if let Some((_, path, _)) = &item_impl.trait_ {
        if let Some(segment) = path.segments.last() {
            types.push((format!("{module}::{self_name}"), format!("{module}::{}", segment.ident)));
        }
    }

    for item in item_impl.items {
        if let ImplItem::Fn(fun) = item {
            let src = format!("{self_module_name}::{}", fun.sig.ident);
            let mut visitor = CallVisitor {
                current_src: src,
                calls,
            };
            visitor.visit_block(&fun.block);
        }
    }
}

struct CallVisitor<'a> {
    current_src: String,
    calls: &'a mut Vec<(String, String)>,
}

impl<'ast> Visit<'ast> for CallVisitor<'_> {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let syn::Expr::Path(path) = node.func.as_ref() {
            if let Some(segment) = path.path.segments.last() {
                self.calls.push((self.current_src.clone(), segment.ident.to_string()));
            }
        }
        visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if let syn::Expr::Call(inner_call) = node.receiver.as_ref() {
            if let syn::Expr::Path(path) = inner_call.func.as_ref() {
                let segments = path.path.segments.iter().collect::<Vec<_>>();
                let target_segment = if segments.len() >= 2 && segments.last().map(|s| s.ident == "new").unwrap_or(false) {
                    segments.get(segments.len() - 2)
                } else {
                    segments.last()
                };
                if let Some(segment) = target_segment {
                    self.calls.push((
                        self.current_src.clone(),
                        format!("{}::{}", to_snake_case(&segment.ident.to_string()), node.method),
                    ));
                }
            }
        }
        visit::visit_expr_method_call(self, node);
    }
}

fn type_name(ty: &Box<syn::Type>) -> String {
    match ty.as_ref() {
        syn::Type::Path(type_path) => type_path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_else(|| "Unknown".to_string()),
        _ => "Unknown".to_string(),
    }
}

fn to_snake_case(value: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if ch.is_uppercase() && idx > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}
