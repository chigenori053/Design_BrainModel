fn main() {
    let domain = "Compiler pipeline";
    let components = ["lexer", "parser", "semantic", "ir_builder", "optimizer"];
    println!("domain={};components={};status=ok", domain, components.len());
}
