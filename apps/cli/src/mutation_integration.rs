use std::collections::BTreeSet;

use mutation_engine::{AnalyzeContext, DependencyEdge};

use crate::analyze_engine::{AnalyzeEngineOutput, StructureEdgeKind};

pub fn mutation_context_from_analyze(output: &AnalyzeEngineOutput) -> AnalyzeContext {
    let nodes = output
        .graph
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();
    let dependencies = output
        .graph
        .edges
        .iter()
        .filter(|edge| {
            matches!(
                edge.kind,
                StructureEdgeKind::DependsOn | StructureEdgeKind::Uses | StructureEdgeKind::Calls
            )
        })
        .map(|edge| DependencyEdge {
            from: edge.from.clone(),
            to: edge.to.clone(),
        })
        .collect();
    let public_api_symbols = output
        .ast_modules
        .iter()
        .flat_map(|module| {
            module
                .structs
                .iter()
                .chain(&module.enums)
                .chain(&module.traits)
                .chain(&module.functions)
        })
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let design_intent = output
        .semantic_structure
        .components
        .iter()
        .map(|component| component.responsibility.clone())
        .filter(|intent| !intent.trim().is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    AnalyzeContext {
        nodes,
        dependencies,
        responsibility_boundaries: Vec::new(),
        public_api_symbols,
        design_intent,
    }
}
