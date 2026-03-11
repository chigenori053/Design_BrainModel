use code_language_core::{CodeLanguageCore, ParsedSourceFile};
use std::collections::BTreeSet;

#[test]
fn test11_cross_language_architecture() {
    let core = CodeLanguageCore::default();
    let rust_graph = core.reverse_architecture(&rust_fixture());
    let python_graph = core.reverse_architecture(&python_fixture());
    let go_graph = core.reverse_architecture(&go_fixture());

    let rust_python = architecture_similarity(&rust_graph, &python_graph);
    let rust_go = architecture_similarity(&rust_graph, &go_graph);
    let python_go = architecture_similarity(&python_graph, &go_graph);
    let similarity = (rust_python + rust_go + python_go) / 3.0;

    println!(
        "Test11 Cross Language\narchitecture_similarity: {:.2}",
        similarity
    );

    assert!(similarity >= 0.7, "architecture_similarity={similarity}");
}

fn rust_fixture() -> Vec<ParsedSourceFile> {
    vec![
        ParsedSourceFile {
            path: "src/api_gateway.rs".into(),
            source: "use crate::user_service::UserService;\npub struct ApiGateway;\npub async fn route() {}\n".into(),
        },
        ParsedSourceFile {
            path: "src/user_service.rs".into(),
            source: "use crate::user_repository::UserRepository;\npub struct UserService;\npub async fn load_user() {}\n".into(),
        },
        ParsedSourceFile {
            path: "src/user_repository.rs".into(),
            source: "pub struct UserRepository;\npub fn fetch() {}\n".into(),
        },
    ]
}

fn python_fixture() -> Vec<ParsedSourceFile> {
    vec![
        ParsedSourceFile {
            path: "src/api_gateway.py".into(),
            source: "from user_service import UserService\n\nclass ApiGateway:\n    def route(self):\n        return UserService()\n".into(),
        },
        ParsedSourceFile {
            path: "src/user_service.py".into(),
            source: "from user_repository import UserRepository\n\nclass UserService:\n    def load_user(self):\n        return UserRepository()\n".into(),
        },
        ParsedSourceFile {
            path: "src/user_repository.py".into(),
            source: "class UserRepository:\n    def fetch(self):\n        return 1\n".into(),
        },
    ]
}

fn go_fixture() -> Vec<ParsedSourceFile> {
    vec![
        ParsedSourceFile {
            path: "src/api_gateway.go".into(),
            source: "package gateway\nimport UserService \"user_service\"\ntype ApiGateway struct {}\nfunc Route() {}\n".into(),
        },
        ParsedSourceFile {
            path: "src/user_service.go".into(),
            source: "package service\nimport UserRepository \"user_repository\"\ntype UserService struct {}\nfunc LoadUser() {}\n".into(),
        },
        ParsedSourceFile {
            path: "src/user_repository.go".into(),
            source: "package repository\ntype UserRepository struct {}\nfunc Fetch() {}\n".into(),
        },
    ]
}

fn architecture_similarity(
    left: &architecture_reasoner::ArchitectureGraph,
    right: &architecture_reasoner::ArchitectureGraph,
) -> f64 {
    let left_nodes = left
        .nodes
        .iter()
        .map(|node| normalize_name(&node.name))
        .collect::<BTreeSet<_>>();
    let right_nodes = right
        .nodes
        .iter()
        .map(|node| normalize_name(&node.name))
        .collect::<BTreeSet<_>>();
    let node_score = if left_nodes.is_empty() && right_nodes.is_empty() {
        1.0
    } else {
        let intersection = left_nodes.intersection(&right_nodes).count() as f64;
        let union = left_nodes.union(&right_nodes).count() as f64;
        intersection / union.max(1.0)
    };
    let left_edges = left
        .dependency_edges()
        .map(|edge| {
            (
                normalize_name(
                    &left
                        .nodes
                        .iter()
                        .find(|node| node.id == edge.from)
                        .unwrap()
                        .name,
                ),
                normalize_name(
                    &left
                        .nodes
                        .iter()
                        .find(|node| node.id == edge.to)
                        .unwrap()
                        .name,
                ),
            )
        })
        .collect::<BTreeSet<_>>();
    let right_edges = right
        .dependency_edges()
        .map(|edge| {
            (
                normalize_name(
                    &right
                        .nodes
                        .iter()
                        .find(|node| node.id == edge.from)
                        .unwrap()
                        .name,
                ),
                normalize_name(
                    &right
                        .nodes
                        .iter()
                        .find(|node| node.id == edge.to)
                        .unwrap()
                        .name,
                ),
            )
        })
        .collect::<BTreeSet<_>>();
    let edge_score = if left_edges.is_empty() && right_edges.is_empty() {
        1.0
    } else {
        let intersection = left_edges.intersection(&right_edges).count() as f64;
        let union = left_edges.union(&right_edges).count() as f64;
        intersection / union.max(1.0)
    };
    ((node_score + edge_score) / 2.0).clamp(0.0, 1.0)
}

fn normalize_name(name: &str) -> String {
    name.to_ascii_lowercase()
}
