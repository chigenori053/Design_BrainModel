use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactorType {
    Why,
    What,
    How,
    Constraint,
    Risk,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignFactor {
    pub id: String,
    pub factor_type: FactorType,
    pub depends_on: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScsInputs {
    pub completeness: f64,
    pub ambiguity_mean: f64,
    pub inconsistency: f64,
    pub dependency_consistency: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DependencyConsistencyMetrics {
    pub dependency_consistency: f64,
    pub connectivity: f64,
    pub cyclicity: f64,
    pub orphan_rate: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SanityStats {
    pub empty_id_fixes: usize,
    pub duplicate_id_fixes: usize,
    pub unknown_dependency_drops: usize,
}

#[derive(Clone, Default)]
pub struct Phase1Engine;

impl Phase1Engine {
    pub fn compute_dependency_consistency(&self, factors: &[DesignFactor]) -> f64 {
        compute_dependency_consistency(factors)
    }

    pub fn compute_dependency_consistency_metrics(
        &self,
        factors: &[DesignFactor],
    ) -> DependencyConsistencyMetrics {
        compute_dependency_consistency_metrics(factors)
    }

    pub fn sanitize_factors(&self, factors: &[DesignFactor]) -> (Vec<DesignFactor>, SanityStats) {
        sanitize_factors(factors)
    }

    pub fn compute_scs_v1_1(&self, inputs: ScsInputs) -> f64 {
        compute_scs_v1_1(inputs)
    }
}

pub fn compute_dependency_consistency(factors: &[DesignFactor]) -> f64 {
    compute_dependency_consistency_metrics(factors).dependency_consistency
}

pub fn compute_dependency_consistency_metrics(factors: &[DesignFactor]) -> DependencyConsistencyMetrics {
    if factors.is_empty() {
        return DependencyConsistencyMetrics {
            dependency_consistency: 0.5,
            connectivity: 0.5,
            cyclicity: 0.0,
            orphan_rate: 0.5,
        };
    }
    if is_unmeasurable_graph(factors) {
        return DependencyConsistencyMetrics {
            dependency_consistency: 0.5,
            connectivity: 0.5,
            cyclicity: 0.0,
            orphan_rate: 0.5,
        };
    }

    let connectivity = compute_connectivity(factors);
    let cyclicity = compute_cyclicity(factors);
    let orphan_rate = compute_orphan_rate(factors);
    let dependency_consistency =
        clamp01(0.50 * connectivity + 0.30 * (1.0 - cyclicity) + 0.20 * (1.0 - orphan_rate));

    DependencyConsistencyMetrics {
        dependency_consistency,
        connectivity,
        cyclicity,
        orphan_rate,
    }
}

pub fn compute_scs_v1_1(inputs: ScsInputs) -> f64 {
    let completeness = clamp01(inputs.completeness);
    let ambiguity_mean = clamp01(inputs.ambiguity_mean);
    let inconsistency = clamp01(inputs.inconsistency);
    let dependency_consistency = clamp01(inputs.dependency_consistency);

    clamp01(
        0.40 * completeness
            + 0.25 * (1.0 - ambiguity_mean)
            + 0.20 * dependency_consistency
            + 0.15 * (1.0 - inconsistency),
    )
}

fn compute_connectivity(factors: &[DesignFactor]) -> f64 {
    let total_nodes = factors.len();
    if total_nodes == 0 {
        return 0.5;
    }

    let id_set = factors
        .iter()
        .map(|f| f.id.as_str())
        .collect::<HashSet<_>>();
    let mut graph = HashMap::<&str, Vec<&str>>::new();
    for factor in factors {
        let deps = factor
            .depends_on
            .iter()
            .map(String::as_str)
            .filter(|dep| id_set.contains(dep))
            .collect::<Vec<_>>();
        graph.insert(factor.id.as_str(), deps);
    }

    let roots = factors
        .iter()
        .filter(|f| f.factor_type == FactorType::Why)
        .map(|f| f.id.as_str())
        .collect::<Vec<_>>();
    if roots.is_empty() {
        return 0.0;
    }

    let mut stack = roots;
    let mut visited = HashSet::<&str>::new();
    while let Some(cur) = stack.pop() {
        if !visited.insert(cur) {
            continue;
        }
        if let Some(next) = graph.get(cur) {
            stack.extend(next.iter().copied());
        }
    }
    visited.len() as f64 / total_nodes as f64
}

fn compute_cyclicity(factors: &[DesignFactor]) -> f64 {
    let id_set = factors
        .iter()
        .map(|f| f.id.as_str())
        .collect::<HashSet<_>>();
    let mut graph = HashMap::<&str, Vec<&str>>::new();
    let mut total_edges = 0usize;
    for factor in factors {
        let deps = factor
            .depends_on
            .iter()
            .map(String::as_str)
            .filter(|dep| id_set.contains(dep))
            .collect::<Vec<_>>();
        total_edges += deps.len();
        graph.insert(factor.id.as_str(), deps);
    }
    if total_edges == 0 {
        return 0.0;
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Visiting,
        Done,
    }

    let mut marks = HashMap::<&str, Mark>::new();
    let mut cycle_edges = 0usize;

    fn dfs<'a>(
        cur: &'a str,
        graph: &HashMap<&'a str, Vec<&'a str>>,
        marks: &mut HashMap<&'a str, Mark>,
        cycle_edges: &mut usize,
    ) {
        marks.insert(cur, Mark::Visiting);
        if let Some(next) = graph.get(cur) {
            for &to in next {
                match marks.get(to).copied() {
                    Some(Mark::Visiting) => {
                        *cycle_edges += 1;
                    }
                    Some(Mark::Done) => {}
                    None => dfs(to, graph, marks, cycle_edges),
                }
            }
        }
        marks.insert(cur, Mark::Done);
    }

    for factor in factors {
        let id = factor.id.as_str();
        if marks.contains_key(id) {
            continue;
        }
        dfs(id, &graph, &mut marks, &mut cycle_edges);
    }

    (cycle_edges as f64 / total_edges as f64).min(1.0)
}

fn compute_orphan_rate(factors: &[DesignFactor]) -> f64 {
    if factors.is_empty() {
        return 0.5;
    }

    let id_set = factors
        .iter()
        .map(|f| f.id.as_str())
        .collect::<HashSet<_>>();
    let mut incoming = HashMap::<&str, usize>::new();
    for factor in factors {
        incoming.entry(factor.id.as_str()).or_insert(0);
        for dep in &factor.depends_on {
            let dep = dep.as_str();
            if id_set.contains(dep) {
                *incoming.entry(dep).or_insert(0) += 1;
            }
        }
    }

    let orphan_nodes = factors
        .iter()
        .filter(|f| {
            f.factor_type != FactorType::Why
                && f.depends_on.is_empty()
                && incoming.get(f.id.as_str()).copied().unwrap_or(0) == 0
        })
        .count();

    orphan_nodes as f64 / factors.len() as f64
}

fn is_unmeasurable_graph(factors: &[DesignFactor]) -> bool {
    let mut ids = HashSet::<&str>::new();
    for factor in factors {
        if factor.id.trim().is_empty() {
            return true;
        }
        if !ids.insert(factor.id.as_str()) {
            return true;
        }
    }
    false
}

pub fn sanitize_factors(factors: &[DesignFactor]) -> (Vec<DesignFactor>, SanityStats) {
    let mut stats = SanityStats::default();
    let mut out = Vec::with_capacity(factors.len());
    let mut used = HashSet::<String>::new();

    for (idx, factor) in factors.iter().enumerate() {
        let mut base_id = factor.id.trim().to_string();
        if base_id.is_empty() {
            stats.empty_id_fixes += 1;
            base_id = format!("auto_{idx}");
        }

        let mut id = base_id.clone();
        let mut suffix = 1usize;
        while used.contains(&id) {
            stats.duplicate_id_fixes += 1;
            id = format!("{base_id}_{suffix}");
            suffix += 1;
        }
        used.insert(id.clone());
        out.push(DesignFactor {
            id,
            factor_type: factor.factor_type,
            depends_on: factor.depends_on.clone(),
        });
    }

    let valid_ids = out.iter().map(|f| f.id.clone()).collect::<HashSet<_>>();
    for factor in &mut out {
        let before = factor.depends_on.len();
        factor.depends_on.retain(|dep| valid_ids.contains(dep));
        stats.unknown_dependency_drops += before.saturating_sub(factor.depends_on.len());
    }

    (out, stats)
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}
