use design_search_engine::RankedCandidate;

/// `DesignUnitId` や `ComponentId` の数値表現から安全なMermaidノードIDを生成する。
fn node_id(raw: &str) -> String {
    raw.replace([' ', '-', '.', '/'], "_")
}

/// 1候補の `ArchitectureState` をMermaid `graph TD` 記法に変換する。
pub fn candidate_to_mermaid(candidate: &RankedCandidate) -> String {
    let arch = &candidate.state.architecture_state;
    let mut out = String::from("graph TD\n");

    for dep in &arch.dependencies {
        let from = node_id(&format!("{:?}", dep.from));
        let to = node_id(&format!("{:?}", dep.to));
        out.push_str(&format!("  {from} --> {to}\n"));
    }

    if arch.dependencies.is_empty() {
        for comp in &arch.components {
            let id = node_id(&format!("{:?}", comp.id));
            out.push_str(&format!("  {id}\n"));
        }
    }

    out
}
