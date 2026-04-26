use architecture_reasoner::ReverseArchitectureReasoner;
use code_language_core::{CodeLanguageCore, ParsedSourceFile, architecture_from_code_ir};
use design_domain::{Architecture, Dependency, DependencyKind, DesignUnit, DesignUnitId};

fn sample_architecture() -> Architecture {
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::new(1, "ApiController"));
    architecture.add_design_unit(DesignUnit::new(2, "UserService"));
    architecture.add_design_unit(DesignUnit::new(3, "UserRepository"));
    architecture.dependencies = vec![
        Dependency {
            from: DesignUnitId(1),
            to: DesignUnitId(2),
            kind: DependencyKind::Calls,
        },
        Dependency {
            from: DesignUnitId(2),
            to: DesignUnitId(3),
            kind: DependencyKind::Calls,
        },
    ];
    architecture.graph.edges = vec![(1, 2), (2, 3)];
    architecture
}

#[test]
fn test1_architecture_to_code_generation_quality() {
    let architecture = sample_architecture();
    let core = CodeLanguageCore::default();
    let graph = ReverseArchitectureReasoner
        .infer_from_code_ir(&core.architecture_to_code_ir(&architecture));
    let generated = core.generate_code(&graph);
    let quality = core.evaluate_generation_quality(&graph);

    assert_eq!(generated.len(), 3);
    assert!(
        generated
            .iter()
            .all(|(_, source)| source.contains("pub struct "))
    );
    assert!(
        generated
            .iter()
            .any(|(_, source)| source.contains("use crate::user_service::UserService;"))
    );
    assert!(quality >= 0.95, "quality={quality}");
}

#[test]
fn test2_code_to_architecture_analyzes_tokio_axum_serde_usage() {
    let files = vec![
        ParsedSourceFile {
            path: "src/api_controller.rs".into(),
            source: r#"
use axum::{Json, Router};
use crate::user_service::UserService;

pub struct ApiController;

pub async fn route() -> Router {
    Router::new()
}
"#
            .into(),
        },
        ParsedSourceFile {
            path: "src/user_service.rs".into(),
            source: r#"
use tokio::task::JoinHandle;
use crate::user_dto::UserDto;

pub struct UserService;

pub async fn fetch_user() -> JoinHandle<()> {
    tokio::spawn(async {})
}
"#
            .into(),
        },
        ParsedSourceFile {
            path: "src/user_dto.rs".into(),
            source: r#"
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct UserDto {
    pub id: u64,
}
"#
            .into(),
        },
    ];

    let core = CodeLanguageCore::default();
    let ir = core.parse_sources(&files);
    let graph = core.reverse_architecture(&files);

    assert_eq!(ir.modules.len(), 3);
    assert!(
        ir.interfaces
            .iter()
            .any(|interface| interface.name == "Router")
    );
    assert!(
        ir.interfaces
            .iter()
            .any(|interface| interface.name == "Json")
    );
    assert!(
        ir.interfaces
            .iter()
            .any(|interface| interface.name == "JoinHandle")
    );
    assert!(
        ir.interfaces
            .iter()
            .any(|interface| interface.name == "Serialize")
    );
    assert!(graph.nodes.iter().any(|node| node.name == "ApiController"));
    assert!(graph.nodes.iter().any(|node| node.name == "UserService"));
    assert!(graph.nodes.iter().any(|node| node.name == "UserDto"));
    assert_eq!(graph.dependency_edges().count(), 2);
}

#[test]
fn test3_round_trip_architecture_consistency_rate() {
    let architecture = sample_architecture();
    let core = CodeLanguageCore::default();
    let report = core.evaluate_roundtrip_consistency(&architecture);
    let regenerated = core.roundtrip_from_architecture(&architecture);
    let recovered = architecture_from_code_ir(&core.architecture_to_code_ir(&architecture));

    assert_eq!(regenerated.len(), 3);
    assert_eq!(recovered.dependencies.len(), 2);
    assert!(
        report.node_recall >= 1.0,
        "node_recall={}",
        report.node_recall
    );
    assert!(
        report.dependency_recall >= 1.0,
        "dependency_recall={}",
        report.dependency_recall
    );
    assert!(
        report.consistency_rate >= 1.0,
        "consistency_rate={}",
        report.consistency_rate
    );
}
