use regenerated_system::designbrain_summary;
fn main() {
    let (name, modules, deps) = designbrain_summary();
    println!("system={name};modules={modules};deps={deps}");
}
