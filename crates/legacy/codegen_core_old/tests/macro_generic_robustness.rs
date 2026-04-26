use code_language_core::{CodeLanguageCore, ParsedSourceFile};
use std::collections::BTreeSet;

#[test]
fn test10_macro_generic_robustness() {
    let files = vec![ParsedSourceFile {
        path: "src/user.rs".into(),
        source: r#"
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct User<T: Clone + Send> {
    pub id: T,
}

pub trait Repository<T: Clone + Send>: Send + Sync {
    fn save(&self, user: User<T>);
}

macro_rules! make_user {
    () => {};
}

pub async fn persist<T: Clone + Send>(repo: impl Repository<T>) {}
"#
        .into(),
    }];

    let core = CodeLanguageCore::default();
    let ir = core.parse_sources(&files);
    let module = &ir.modules[0];
    let interfaces = ir
        .interfaces
        .iter()
        .map(|interface| interface.name.clone())
        .collect::<BTreeSet<_>>();
    let semantics = module
        .responsibilities
        .iter()
        .map(|entry| entry.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let generic_hits = [
        semantics.iter().any(|entry| entry.contains("generic t")),
        interfaces.contains("Serialize"),
        interfaces.contains("Deserialize"),
    ]
    .into_iter()
    .filter(|hit| *hit)
    .count();
    let trait_hits = [interfaces.contains("Clone"), interfaces.contains("Send")]
        .into_iter()
        .filter(|hit| *hit)
        .count();
    let generic_structure_recall = generic_hits as f64 / 3.0;
    let trait_dependency_accuracy = trait_hits as f64 / 2.0;

    println!(
        "Test10 Macro / Generic\ngeneric_structure_recall: {:.2}\ntrait_dependency_accuracy: {:.2}",
        generic_structure_recall, trait_dependency_accuracy
    );

    assert!(
        generic_structure_recall >= 0.7,
        "generic_structure_recall={generic_structure_recall}"
    );
    assert!(
        trait_dependency_accuracy >= 0.7,
        "trait_dependency_accuracy={trait_dependency_accuracy}"
    );
}
