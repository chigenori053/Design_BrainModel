use design_cli::dbm::analyzer;
use std::fs;

#[test]
fn controller_replay_analyzer_semantic_alignment() {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path();

    fs::create_dir_all(root_path.join("controller")).unwrap();
    fs::create_dir_all(root_path.join("replay")).unwrap();
    fs::create_dir_all(root_path.join("controller_replay_interface")).unwrap();

    fs::write(
        root_path.join("controller_replay_interface/mod.rs"),
        "pub trait ControllerReplayInterface {\n    fn replay(&self) -> bool;\n}\n",
    )
    .unwrap();

    fs::write(
        root_path.join("controller/mod.rs"),
        "use crate::controller_replay_interface::ControllerReplayInterface;\nstruct Controller;\n",
    )
    .unwrap();

    fs::write(
        root_path.join("replay/mod.rs"),
        "use crate::controller_replay_interface::ControllerReplayInterface;\nstruct Replay;\n",
    )
    .unwrap();

    let result = analyzer::analyze_project(root_path.to_str().unwrap()).unwrap();
    assert!(result.dependencies.iter().any(|edge| {
        edge.from == "controller"
            && edge.to == "controller_replay_interface"
            && edge.edge_type == analyzer::DependencyEdgeType::Mediated
    }));
    assert!(result.dependencies.iter().any(|edge| {
        edge.from == "replay"
            && edge.to == "controller_replay_interface"
            && edge.edge_type == analyzer::DependencyEdgeType::Mediated
    }));
    assert!(result.dependencies.iter().any(|edge| {
        edge.from == "controller"
            && edge.to == "replay"
            && edge.edge_type == analyzer::DependencyEdgeType::Mediated
    }));
    assert!(!result.dependencies.iter().any(|edge| {
        edge.from == "controller"
            && edge.to == "replay"
            && edge.edge_type == analyzer::DependencyEdgeType::Direct
    }));
    assert!(!result.dependencies.iter().any(|edge| {
        edge.from == "replay"
            && edge.to == "controller"
            && edge.edge_type == analyzer::DependencyEdgeType::Direct
    }));
}

#[test]
fn renderer_world_analyzer_semantic_alignment() {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path();

    fs::create_dir_all(root_path.join("renderer")).unwrap();
    fs::create_dir_all(root_path.join("world")).unwrap();
    fs::create_dir_all(root_path.join("renderer_world_interface")).unwrap();
    fs::create_dir_all(root_path.join("world_renderer_interface")).unwrap();

    fs::write(
        root_path.join("renderer/mod.rs"),
        "use crate::world::World;\nuse crate::renderer_world_interface::RendererWorldInterface;\npub struct Renderer;\n",
    )
    .unwrap();
    fs::write(
        root_path.join("world/mod.rs"),
        "use crate::renderer::Renderer;\nuse crate::world_renderer_interface::WorldRendererInterface;\npub struct World;\n",
    )
    .unwrap();
    fs::write(
        root_path.join("renderer_world_interface/mod.rs"),
        "pub trait RendererWorldInterface {}\n",
    )
    .unwrap();
    fs::write(
        root_path.join("world_renderer_interface/mod.rs"),
        "pub trait WorldRendererInterface {}\n",
    )
    .unwrap();

    let result = analyzer::analyze_project(root_path.to_str().unwrap()).unwrap();

    assert!(
        result.dependencies.iter().any(|edge| {
            edge.from == "renderer"
                && edge.to == "renderer_world_interface"
                && edge.edge_type == analyzer::DependencyEdgeType::Mediated
        }),
        "{:?}",
        result.dependencies
    );
    assert!(result.dependencies.iter().any(|edge| {
        edge.from == "world"
            && edge.to == "world_renderer_interface"
            && edge.edge_type == analyzer::DependencyEdgeType::Mediated
    }));
    assert!(!result.dependencies.iter().any(|edge| {
        ((edge.from == "renderer" && edge.to == "world")
            || (edge.from == "world" && edge.to == "renderer"))
            && edge.edge_type == analyzer::DependencyEdgeType::Direct
    }));
}

#[test]
fn renderer_world_production_tree_analyzer_semantic_alignment() {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path();

    fs::create_dir_all(root_path.join("apps/cli/src")).unwrap();

    fs::write(
        root_path.join("apps/cli/src/renderer.rs"),
        "use crate::world::World;\nuse crate::renderer_world_interface::RendererWorldInterface;\npub struct Renderer;\n",
    )
    .unwrap();
    fs::write(
        root_path.join("apps/cli/src/world.rs"),
        "use crate::renderer::Renderer;\nuse crate::world_renderer_interface::WorldRendererInterface;\npub struct World;\n",
    )
    .unwrap();
    fs::write(
        root_path.join("apps/cli/src/renderer_world_interface.rs"),
        "pub trait RendererWorldInterface {}\n",
    )
    .unwrap();
    fs::write(
        root_path.join("apps/cli/src/world_renderer_interface.rs"),
        "pub trait WorldRendererInterface {}\n",
    )
    .unwrap();

    let result = analyzer::analyze_project(root_path.to_str().unwrap()).unwrap();

    assert!(
        result
            .modules
            .iter()
            .any(|module| module.name == "renderer")
    );
    assert!(result.modules.iter().any(|module| module.name == "world"));
    assert!(
        result
            .modules
            .iter()
            .any(|module| module.name == "renderer_world_interface")
    );
    assert!(
        result
            .modules
            .iter()
            .any(|module| module.name == "world_renderer_interface")
    );
    assert!(result.dependencies.iter().any(|edge| {
        edge.from == "renderer"
            && edge.to == "renderer_world_interface"
            && edge.edge_type == analyzer::DependencyEdgeType::Mediated
    }));
    assert!(result.dependencies.iter().any(|edge| {
        edge.from == "world"
            && edge.to == "world_renderer_interface"
            && edge.edge_type == analyzer::DependencyEdgeType::Mediated
    }));
    assert!(!result.dependencies.iter().any(|edge| {
        ((edge.from == "renderer" && edge.to == "world")
            || (edge.from == "world" && edge.to == "renderer"))
            && edge.edge_type == analyzer::DependencyEdgeType::Direct
    }));
}
