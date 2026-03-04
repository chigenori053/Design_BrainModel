use std::collections::BTreeMap;

use crate::domain::hash::compute_hash;
use crate::domain::history::{SessionHistory, SessionSnapshot};
use crate::domain::transaction::{
    ActiveTransaction, ProposedDiff, TransactionEngine, TxError, TxStatus,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeState {
    pub dispatch_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedDesignState {
    pub nodes: BTreeMap<String, String>,
    pub dependencies: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub node_id_states: BTreeMap<String, NodeIdState>,
    #[serde(default)]
    pub node_origins: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeIdState {
    #[default]
    Temporary,
    PendingPromotion,
    Global,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromotionError {
    TransactionInProgress,
    RootNodeNotFound(String),
    EmptyOriginId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromotionReport {
    pub root_node: String,
    pub promotion_unit: Vec<String>,
    pub promoted_count: usize,
    pub kept_global_count: usize,
    pub closure_size_warning: bool,
}

impl UnifiedDesignState {
    fn node_state_or_default(&self, key: &str) -> NodeIdState {
        self.node_id_states
            .get(key)
            .copied()
            .unwrap_or(NodeIdState::Temporary)
    }

    fn dependency_closure(&self, root: &str) -> Result<Vec<String>, PromotionError> {
        if !self.nodes.contains_key(root) {
            return Err(PromotionError::RootNodeNotFound(root.to_string()));
        }

        let mut stack = vec![root.to_string()];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(cur) = stack.pop() {
            if !visited.insert(cur.clone()) {
                continue;
            }
            if let Some(nexts) = self.dependencies.get(&cur) {
                for next in nexts {
                    if self.nodes.contains_key(next) {
                        stack.push(next.clone());
                    }
                }
            }
        }

        Ok(visited.into_iter().collect())
    }

    fn promote_dependency_closure(
        &mut self,
        root: &str,
        new_origin_id: &str,
        closure_warning_threshold: usize,
    ) -> Result<PromotionReport, PromotionError> {
        let origin = new_origin_id.trim();
        if origin.is_empty() {
            return Err(PromotionError::EmptyOriginId);
        }

        let promotion_unit = self.dependency_closure(root)?;
        let closure_size_warning = promotion_unit.len() > closure_warning_threshold;

        let mut promoted_count = 0_usize;
        let mut kept_global_count = 0_usize;

        for key in &promotion_unit {
            match self.node_state_or_default(key) {
                NodeIdState::Global => {
                    kept_global_count += 1;
                }
                NodeIdState::Temporary | NodeIdState::PendingPromotion => {
                    self.node_id_states
                        .insert(key.clone(), NodeIdState::Global);
                    self.node_origins.insert(key.clone(), origin.to_string());
                    promoted_count += 1;
                }
            }
        }

        Ok(PromotionReport {
            root_node: root.to_string(),
            promotion_unit,
            promoted_count,
            kept_global_count,
            closure_size_warning,
        })
    }

    fn apply_diff(&mut self, diff: &ProposedDiff) -> Result<(), TxError> {
        match diff {
            ProposedDiff::UpsertNode { key, value } => {
                self.nodes.insert(key.clone(), value.clone());
                self.node_id_states
                    .entry(key.clone())
                    .or_insert(NodeIdState::Temporary);
            }
            ProposedDiff::RemoveNode { key } => {
                if self.nodes.remove(key).is_none() {
                    return Err(TxError::MissingNode(key.clone()));
                }
                self.dependencies.remove(key);
                self.node_id_states.remove(key);
                self.node_origins.remove(key);
                for deps in self.dependencies.values_mut() {
                    deps.retain(|dep| dep != key);
                }
            }
            ProposedDiff::SetDependencies { key, dependencies } => {
                if !self.nodes.contains_key(key) {
                    return Err(TxError::MissingNode(key.clone()));
                }
                for dep in dependencies {
                    if !self.nodes.contains_key(dep) {
                        return Err(TxError::MissingDependency(dep.clone()));
                    }
                }
                let mut sorted = dependencies.clone();
                sorted.sort();
                sorted.dedup();
                self.dependencies.insert(key.clone(), sorted);
            }
            ProposedDiff::RemoveDependencies { key } => {
                if self.dependencies.remove(key).is_none() {
                    return Err(TxError::MissingDependency(key.clone()));
                }
            }
            ProposedDiff::SplitHighOutDegreeNode { key } => {
                if !self.nodes.contains_key(key) {
                    return Err(TxError::MissingNode(key.clone()));
                }
                let deps = self
                    .dependencies
                    .get(key)
                    .cloned()
                    .ok_or_else(|| TxError::MissingDependency(key.clone()))?;

                let mut normalized = deps;
                normalized.sort();
                normalized.dedup();
                if normalized.len() < 3 {
                    return Err(TxError::InvalidSplitCandidate(key.clone()));
                }

                let split_at = normalized.len() / 2;
                if split_at == 0 || split_at >= normalized.len() {
                    return Err(TxError::InvalidSplitCandidate(key.clone()));
                }

                let kept = normalized[..split_at].to_vec();
                let moved = normalized[split_at..].to_vec();
                let new_key = self.next_split_node_key(key);
                let new_value = self.nodes.get(key).cloned().unwrap_or_default();

                let cycle_before = cyclic_penalty_ratio(self);
                let mut candidate = self.clone();

                candidate.nodes.insert(new_key.clone(), new_value);
                candidate.dependencies.insert(new_key.clone(), moved);

                let mut owner_deps = kept;
                owner_deps.push(new_key.clone());
                owner_deps.sort();
                owner_deps.dedup();
                candidate.dependencies.insert(key.clone(), owner_deps);

                let cycle_after = cyclic_penalty_ratio(&candidate);
                if cycle_after > cycle_before + 1e-9 {
                    return Err(TxError::CycleIncreaseRejected(key.clone()));
                }

                *self = candidate;
            }
            ProposedDiff::RewireHighImpactEdge { key, from, to } => {
                if !self.nodes.contains_key(key) {
                    return Err(TxError::MissingNode(key.clone()));
                }
                if !self.nodes.contains_key(from) || !self.nodes.contains_key(to) {
                    return Err(TxError::MissingDependency(format!("{from}->{to}")));
                }
                if key == to {
                    return Err(TxError::InvalidRewireCandidate(format!(
                        "self-loop: {key}->{to}"
                    )));
                }

                let deps = self
                    .dependencies
                    .get(key)
                    .cloned()
                    .ok_or_else(|| TxError::MissingDependency(key.clone()))?;
                let mut normalized = deps;
                normalized.sort();
                normalized.dedup();
                if !normalized.iter().any(|d| d == from) {
                    return Err(TxError::InvalidRewireCandidate(format!(
                        "missing edge: {key}->{from}"
                    )));
                }
                if normalized.iter().any(|d| d == to) {
                    return Err(TxError::InvalidRewireCandidate(format!(
                        "edge exists: {key}->{to}"
                    )));
                }

                let cycle_before = cyclic_penalty_ratio(self);
                let mut candidate = self.clone();
                if let Some(owner_deps) = candidate.dependencies.get_mut(key) {
                    owner_deps.retain(|d| d != from);
                    owner_deps.push(to.clone());
                    owner_deps.sort();
                    owner_deps.dedup();
                }

                if candidate.path_exists(to, key) {
                    return Err(TxError::CycleIncreaseRejected(format!("{key}->{to}")));
                }

                let cycle_after = cyclic_penalty_ratio(&candidate);
                if cycle_after > cycle_before + 1e-9 {
                    return Err(TxError::CycleIncreaseRejected(format!("{key}->{to}")));
                }

                *self = candidate;
            }
            ProposedDiff::TwoStep { first, second } => {
                let mut candidate = self.clone();
                candidate.apply_diff(first)?;
                candidate.apply_diff(second)?;
                *self = candidate;
            }
        }

        Ok(())
    }

    fn next_split_node_key(&self, base: &str) -> String {
        let mut idx = 1_usize;
        loop {
            let key = format!("{base}__split{idx}");
            if !self.nodes.contains_key(&key) {
                return key;
            }
            idx = idx.saturating_add(1);
        }
    }

    fn path_exists(&self, start: &str, goal: &str) -> bool {
        if start == goal {
            return true;
        }
        let mut stack = vec![start.to_string()];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(cur) = stack.pop() {
            if !visited.insert(cur.clone()) {
                continue;
            }
            if cur == goal {
                return true;
            }
            if let Some(nexts) = self.dependencies.get(&cur) {
                for next in nexts {
                    if self.nodes.contains_key(next) {
                        stack.push(next.clone());
                    }
                }
            }
        }
        false
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignScoreVector {
    pub consistency: u32,
    pub structural_integrity: u32,
    pub dependency_soundness: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StateVector {
    pub consistency: f64,
    pub propagation_quality: f64,
    pub cycle_quality: f64,
    pub modularity: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeltaVector {
    pub d_consistency: f64,
    pub d_prop_quality: f64,
    pub d_cycle_quality: f64,
    pub d_modularity: f64,
}

impl StateVector {
    pub fn delta(&self, other: &StateVector) -> DeltaVector {
        DeltaVector {
            d_consistency: other.consistency - self.consistency,
            d_prop_quality: other.propagation_quality - self.propagation_quality,
            d_cycle_quality: other.cycle_quality - self.cycle_quality,
            d_modularity: other.modularity - self.modularity,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppState {
    pub uds: UnifiedDesignState,
    pub evaluation: DesignScoreVector,
    pub tx_engine: TransactionEngine,
    pub session_history: SessionHistory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalError {
    TransactionInProgress,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnalyzeError {
    TransactionInProgress,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SuggestError {
    TransactionInProgress,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationResult {
    pub delta: DeltaVector,
    pub score: f64,
    pub accepted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParetoResult {
    pub frontier_indices: Vec<usize>,
    pub scores: Vec<DesignScoreVector>,
}

impl AppState {
    pub fn new(uds: UnifiedDesignState) -> Self {
        let evaluation = evaluate_lightweight(&uds);
        let snapshot = SessionSnapshot {
            version_id: 0,
            uds_hash: compute_hash(&uds),
            uds,
            evaluation: evaluation.clone(),
        };

        Self {
            uds: snapshot.uds.clone(),
            evaluation,
            tx_engine: TransactionEngine::new(),
            session_history: SessionHistory::with_initial(snapshot, 100),
        }
    }

    pub fn begin_tx(&mut self) -> Result<(), TxError> {
        if self.tx_engine.current_tx.is_some() {
            return Err(TxError::ActiveTransactionExists);
        }

        let snapshot_before = self.make_snapshot(self.current_version_id());
        self.tx_engine.current_tx = Some(ActiveTransaction {
            snapshot_before,
            diffs: Vec::new(),
            status: TxStatus::Pending,
        });

        Ok(())
    }

    pub fn from_persisted(
        version_id: u64,
        uds_hash: u64,
        uds: UnifiedDesignState,
        evaluation: DesignScoreVector,
    ) -> Self {
        let snapshot = SessionSnapshot {
            version_id,
            uds_hash,
            uds,
            evaluation: evaluation.clone(),
        };

        Self {
            uds: snapshot.uds.clone(),
            evaluation,
            tx_engine: TransactionEngine::new(),
            session_history: SessionHistory::with_initial(snapshot, 100),
        }
    }

    pub fn promote_node_with_dependency_closure(
        &mut self,
        root_node: &str,
        new_origin_id: &str,
        closure_warning_threshold: usize,
    ) -> Result<PromotionReport, PromotionError> {
        if self.tx_engine.current_tx.is_some() {
            return Err(PromotionError::TransactionInProgress);
        }

        self.begin_tx().map_err(|_| PromotionError::TransactionInProgress)?;
        let mut candidate = self.uds.clone();
        let report = match candidate.promote_dependency_closure(
            root_node,
            new_origin_id,
            closure_warning_threshold,
        ) {
            Ok(r) => r,
            Err(err) => {
                let _ = self.abort_tx();
                return Err(err);
            }
        };

        if self.replace_uds(candidate).is_err() {
            let _ = self.abort_tx();
            return Err(PromotionError::TransactionInProgress);
        }
        if self.commit_tx().is_err() {
            let _ = self.abort_tx();
            return Err(PromotionError::TransactionInProgress);
        }

        Ok(report)
    }

    pub fn apply_diff(&mut self, diff: ProposedDiff) -> Result<(), TxError> {
        let snapshot_before = match self.tx_engine.current_tx.as_ref() {
            Some(tx) => tx.snapshot_before.clone(),
            None => return Err(TxError::NoActiveTransaction),
        };

        {
            let tx = self
                .tx_engine
                .current_tx
                .as_mut()
                .ok_or(TxError::NoActiveTransaction)?;

            if !matches!(tx.status, TxStatus::Pending | TxStatus::Applied) {
                return Err(TxError::InvalidTransactionState {
                    expected: TxStatus::Pending,
                    actual: tx.status.clone(),
                });
            }

            tx.diffs.push(diff.clone());
        }

        if let Err(err) = self.uds.apply_diff(&diff) {
            self.rollback_internal(&snapshot_before);
            return Err(err);
        }

        self.evaluation = evaluate_lightweight(&self.uds);

        if let Some(tx) = self.tx_engine.current_tx.as_mut() {
            tx.status = TxStatus::Applied;
        }

        Ok(())
    }

    pub fn replace_uds(&mut self, new_uds: UnifiedDesignState) -> Result<(), TxError> {
        let tx = self
            .tx_engine
            .current_tx
            .as_mut()
            .ok_or(TxError::NoActiveTransaction)?;

        if !matches!(tx.status, TxStatus::Pending | TxStatus::Applied) {
            return Err(TxError::InvalidTransactionState {
                expected: TxStatus::Pending,
                actual: tx.status.clone(),
            });
        }

        self.uds = new_uds;
        self.evaluation = evaluate_lightweight(&self.uds);
        tx.status = TxStatus::Applied;
        Ok(())
    }

    pub fn commit_tx(&mut self) -> Result<(), TxError> {
        let status = self
            .tx_engine
            .current_tx
            .as_ref()
            .map(|tx| tx.status.clone())
            .ok_or(TxError::NoActiveTransaction)?;

        if status != TxStatus::Applied {
            return Err(TxError::InvalidTransactionState {
                expected: TxStatus::Applied,
                actual: status,
            });
        }

        let next_version_id = self.current_version_id().saturating_add(1);
        let snapshot = self.make_snapshot(next_version_id);
        self.session_history.push(snapshot);

        if let Some(tx) = self.tx_engine.current_tx.as_mut() {
            tx.status = TxStatus::Committed;
        }

        self.tx_engine.current_tx = None;
        Ok(())
    }

    pub fn abort_tx(&mut self) -> Result<(), TxError> {
        let snapshot_before = self
            .tx_engine
            .current_tx
            .as_ref()
            .map(|tx| tx.snapshot_before.clone())
            .ok_or(TxError::NoActiveTransaction)?;

        self.restore_snapshot(&snapshot_before);
        self.tx_engine.current_tx = None;
        Ok(())
    }

    pub fn undo(&mut self) -> Result<(), TxError> {
        if self.tx_engine.current_tx.is_some() {
            return Err(TxError::TransactionInProgress);
        }

        let snapshot = self.session_history.undo().ok_or(TxError::UndoUnavailable)?;
        self.restore_snapshot(&snapshot);
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), TxError> {
        if self.tx_engine.current_tx.is_some() {
            return Err(TxError::TransactionInProgress);
        }

        let snapshot = self.session_history.redo().ok_or(TxError::RedoUnavailable)?;
        self.restore_snapshot(&snapshot);
        Ok(())
    }

    pub fn evaluate_now(&mut self) -> Result<(), EvalError> {
        if self.tx_engine.current_tx.is_some() {
            return Err(EvalError::TransactionInProgress);
        }
        self.evaluation = evaluate_lightweight(&self.uds);
        Ok(())
    }

    pub fn compute_state_vector(&self) -> StateVector {
        compute_state_vector_from(&self.uds, &self.evaluation)
    }

    pub fn evaluate_diff(&self, diff: &ProposedDiff) -> Option<EvaluationResult> {
        const ALPHA: f64 = 3.0;
        const BETA: f64 = 3.0;
        const GAMMA: f64 = 1.0;
        const ETA: f64 = 0.02;
        const DELTA_MOD: f64 = 0.8;

        let mut candidate_uds = self.uds.clone();
        if candidate_uds.apply_diff(diff).is_err() {
            return None;
        }

        let candidate_eval = evaluate_lightweight(&candidate_uds);
        let before = compute_state_vector_from(&self.uds, &self.evaluation);
        let after = compute_state_vector_from(&candidate_uds, &candidate_eval);
        let delta = before.delta(&after);
        let complexity_delta = delta_complexity(&self.uds, &candidate_uds);
        let score = delta.d_consistency
            + GAMMA * delta.d_prop_quality.max(0.0)
            - ALPHA * (-delta.d_prop_quality).max(0.0)
            - BETA * (-delta.d_cycle_quality).max(0.0)
            - ETA * complexity_delta.max(0.0)
            + DELTA_MOD * delta.d_modularity.max(0.0);

        Some(EvaluationResult {
            delta,
            score,
            accepted: score > 0.0,
        })
    }

    pub fn analyze_pareto(&self) -> Result<ParetoResult, AnalyzeError> {
        if self.tx_engine.current_tx.is_some() {
            return Err(AnalyzeError::TransactionInProgress);
        }

        let scores = self.build_pareto_candidates();
        let mut frontier_indices = Vec::new();

        for (i, score_i) in scores.iter().enumerate() {
            let dominated = scores
                .iter()
                .enumerate()
                .any(|(j, score_j)| i != j && dominates(score_j, score_i));
            if !dominated {
                frontier_indices.push(i);
            }
        }

        Ok(ParetoResult {
            frontier_indices,
            scores,
        })
    }

    pub fn suggest_diffs_from_analysis(
        &self,
        _pareto: &ParetoResult,
    ) -> Result<Vec<ProposedDiff>, SuggestError> {
        if self.tx_engine.current_tx.is_some() {
            return Err(SuggestError::TransactionInProgress);
        }

        let baseline = evaluate_lightweight(&self.uds);
        let candidates = self.build_candidate_diffs(&baseline);

        let accepted = candidates
            .into_iter()
            .filter(|diff| self.evaluate_diff(diff).map(|r| r.accepted).unwrap_or(false))
            .collect::<Vec<_>>();

        Ok(accepted)
    }

    fn build_candidate_diffs(&self, baseline: &DesignScoreVector) -> Vec<ProposedDiff> {
        let mut candidates = self.build_single_step_candidates(baseline);
        if let Some(two_step) = self.build_best_two_step_candidate(baseline, &candidates) {
            candidates.push(two_step);
        }
        candidates
    }

    fn build_single_step_candidates(&self, baseline: &DesignScoreVector) -> Vec<ProposedDiff> {
        let mut candidates = Vec::new();
        if baseline.consistency < 80 {
            for (key, value) in &self.uds.nodes {
                if value.trim().is_empty() {
                    candidates.push(ProposedDiff::UpsertNode {
                        key: key.clone(),
                        value: "auto-filled".to_string(),
                    });
                    if self.uds.nodes.len() > 1 {
                        candidates.push(ProposedDiff::RemoveNode { key: key.clone() });
                    }
                }
            }
        }

        if baseline.structural_integrity < 75 {
            for key in self.uds.dependencies.keys() {
                if !self.uds.nodes.contains_key(key) {
                    candidates.push(ProposedDiff::RemoveDependencies { key: key.clone() });
                }
            }
        }

        if baseline.dependency_soundness < 85 {
            for (key, deps) in &self.uds.dependencies {
                let filtered = deps
                    .iter()
                    .filter(|dep| *dep != key && self.uds.nodes.contains_key(*dep))
                    .cloned()
                    .collect::<Vec<_>>();
                if &filtered != deps {
                    candidates.push(ProposedDiff::SetDependencies {
                        key: key.clone(),
                        dependencies: filtered,
                    });
                }
            }
        }

        for key in self.split_candidate_keys() {
            let diff = ProposedDiff::SplitHighOutDegreeNode { key };
            if self.preview_passes_guard(baseline, &diff) {
                candidates.push(diff);
            }
        }

        for diff in self.rewire_candidate_diffs() {
            if self.preview_passes_guard(baseline, &diff) {
                candidates.push(diff);
            }
        }

        candidates
    }

    fn build_best_two_step_candidate(
        &self,
        _baseline: &DesignScoreVector,
        first_step_candidates: &[ProposedDiff],
    ) -> Option<ProposedDiff> {
        const TOP_K: usize = 3;

        let mut first_scored = first_step_candidates
            .iter()
            .filter_map(|diff| self.evaluate_diff(diff).map(|r| (diff.clone(), r.score)))
            .collect::<Vec<_>>();
        if first_scored.is_empty() {
            return None;
        }

        first_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let best_single_score = first_scored[0].1;

        let mut best_two_step: Option<(ProposedDiff, f64)> = None;
        for (first_diff, _) in first_scored.into_iter().take(TOP_K) {
            let mut after_first = self.uds.clone();
            if after_first.apply_diff(&first_diff).is_err() {
                continue;
            }

            let mut temp = self.clone();
            temp.uds = after_first.clone();
            temp.evaluation = evaluate_lightweight(&after_first);
            let second_candidates = temp.build_single_step_candidates(&temp.evaluation);

            for second_diff in second_candidates {
                let mut after_second = after_first.clone();
                if after_second.apply_diff(&second_diff).is_err() {
                    continue;
                }

                let candidate = ProposedDiff::TwoStep {
                    first: Box::new(first_diff.clone()),
                    second: Box::new(second_diff),
                };
                let two_step_score = self
                    .evaluate_diff(&candidate)
                    .map(|r| r.score)
                    .unwrap_or(0.0);
                if two_step_score <= 0.0 {
                    continue;
                }

                match &best_two_step {
                    Some((_, best)) if *best >= two_step_score => {}
                    _ => best_two_step = Some((candidate, two_step_score)),
                }
            }
        }

        match best_two_step {
            Some((candidate, score)) if score > best_single_score => Some(candidate),
            _ => None,
        }
    }

    fn split_candidate_keys(&self) -> Vec<String> {
        const SPLIT_OUT_DEGREE_MIN: usize = 3;
        const IMPACT_TOP_PERCENTILE: f64 = 0.30;
        const LAMBDA: f64 = 0.60;

        let keys = self.uds.nodes.keys().cloned().collect::<Vec<_>>();
        if keys.is_empty() {
            return Vec::new();
        }

        let index = keys
            .iter()
            .enumerate()
            .map(|(idx, key)| (key.clone(), idx))
            .collect::<BTreeMap<_, _>>();

        let mut adjacency = vec![Vec::<usize>::new(); keys.len()];
        for (owner, deps) in &self.uds.dependencies {
            let Some(&from) = index.get(owner) else {
                continue;
            };
            for dep in deps {
                let Some(&to) = index.get(dep) else {
                    continue;
                };
                if from != to {
                    adjacency[from].push(to);
                }
            }
        }
        for edges in &mut adjacency {
            edges.sort_unstable();
            edges.dedup();
        }

        let mut scored = Vec::new();
        for key in &keys {
            let out_degree = self
                .uds
                .dependencies
                .get(key)
                .map(|deps| {
                    let mut d = deps.clone();
                    d.sort();
                    d.dedup();
                    d.len()
                })
                .unwrap_or(0);
            if out_degree < SPLIT_OUT_DEGREE_MIN {
                continue;
            }

            let Some(&idx) = index.get(key) else {
                continue;
            };
            let impact = propagation_sum_from(idx, &adjacency, LAMBDA);
            scored.push((key.clone(), impact));
        }

        if scored.is_empty() {
            return Vec::new();
        }

        let mut impact_values = scored.iter().map(|(_, impact)| *impact).collect::<Vec<_>>();
        impact_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let rank = ((impact_values.len() as f64) * (1.0 - IMPACT_TOP_PERCENTILE)).floor() as usize;
        let threshold = impact_values[rank.min(impact_values.len().saturating_sub(1))];

        scored
            .into_iter()
            .filter(|(_, impact)| *impact >= threshold)
            .map(|(key, _)| key)
            .collect::<Vec<_>>()
    }

    fn rewire_candidate_diffs(&self) -> Vec<ProposedDiff> {
        const EDGE_TOP_PERCENTILE: f64 = 0.30;
        const LAMBDA: f64 = 0.60;

        let keys = self.uds.nodes.keys().cloned().collect::<Vec<_>>();
        if keys.len() < 3 {
            return Vec::new();
        }

        let index = keys
            .iter()
            .enumerate()
            .map(|(idx, key)| (key.clone(), idx))
            .collect::<BTreeMap<_, _>>();

        let mut adjacency = vec![Vec::<usize>::new(); keys.len()];
        let mut indegree = vec![0_usize; keys.len()];
        for (owner, deps) in &self.uds.dependencies {
            let Some(&from) = index.get(owner) else {
                continue;
            };
            for dep in deps {
                let Some(&to) = index.get(dep) else {
                    continue;
                };
                if from == to {
                    continue;
                }
                adjacency[from].push(to);
            }
        }
        for edges in &mut adjacency {
            edges.sort_unstable();
            edges.dedup();
            for &to in edges.iter() {
                indegree[to] = indegree[to].saturating_add(1);
            }
        }

        let mut node_impact = vec![0.0_f64; keys.len()];
        for (i, impact) in node_impact.iter_mut().enumerate() {
            *impact = propagation_sum_from(i, &adjacency, LAMBDA);
        }

        let mut edge_scores = Vec::<(usize, usize, f64)>::new();
        for (from, tos) in adjacency.iter().enumerate() {
            for &to in tos {
                let score = node_impact[from] * (1.0 + indegree[to] as f64);
                edge_scores.push((from, to, score));
            }
        }
        if edge_scores.is_empty() {
            return Vec::new();
        }

        let mut score_values = edge_scores.iter().map(|(_, _, s)| *s).collect::<Vec<_>>();
        score_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let rank = ((score_values.len() as f64) * (1.0 - EDGE_TOP_PERCENTILE)).floor() as usize;
        let threshold = score_values[rank.min(score_values.len().saturating_sub(1))];

        let mut candidates = Vec::new();
        for (from, to, score) in edge_scores {
            if score < threshold {
                continue;
            }
            let owner = &keys[from];
            let old_dep = &keys[to];

            let mut best: Option<(String, f64)> = None;
            for w_idx in 0..keys.len() {
                if w_idx == from || w_idx == to {
                    continue;
                }
                let w = &keys[w_idx];
                if adjacency[from].contains(&w_idx) {
                    continue;
                }
                let dist = shortest_distance(from, w_idx, &adjacency).unwrap_or(keys.len() + 1);
                let indegree_gain = indegree[to] as f64 - indegree[w_idx] as f64;
                let rewiring_score = (dist as f64) + indegree_gain;
                match &best {
                    Some((_, best_score)) if *best_score >= rewiring_score => {}
                    _ => best = Some((w.clone(), rewiring_score)),
                }
            }

            if let Some((new_dep, _)) = best {
                candidates.push(ProposedDiff::RewireHighImpactEdge {
                    key: owner.clone(),
                    from: old_dep.clone(),
                    to: new_dep,
                });
            }
        }

        candidates
    }

    fn preview_passes_guard(
        &self,
        _baseline_eval: &DesignScoreVector,
        diff: &ProposedDiff,
    ) -> bool {
        self.evaluate_diff(diff).map(|r| r.accepted).unwrap_or(false)
    }

    fn rollback_internal(&mut self, snapshot_before: &SessionSnapshot) {
        self.restore_snapshot(snapshot_before);
        if let Some(tx) = self.tx_engine.current_tx.as_mut() {
            tx.status = TxStatus::Aborted;
        }
    }

    fn restore_snapshot(&mut self, snapshot: &SessionSnapshot) {
        self.uds = snapshot.uds.clone();
        self.evaluation = snapshot.evaluation.clone();
    }

    fn make_snapshot(&self, version_id: u64) -> SessionSnapshot {
        SessionSnapshot {
            version_id,
            uds_hash: compute_hash(&self.uds),
            uds: self.uds.clone(),
            evaluation: self.evaluation.clone(),
        }
    }

    pub fn current_version_id(&self) -> u64 {
        self.session_history
            .current()
            .map(|snapshot| snapshot.version_id)
            .unwrap_or(0)
    }

    fn build_pareto_candidates(&self) -> Vec<DesignScoreVector> {
        let mut scores = vec![evaluate_lightweight(&self.uds)];

        let keys = self.uds.nodes.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            let mut candidate = self.uds.clone();
            candidate.nodes.remove(&key);
            candidate.dependencies.remove(&key);
            for deps in candidate.dependencies.values_mut() {
                deps.retain(|dep| dep != &key);
            }
            scores.push(evaluate_lightweight(&candidate));
        }

        scores
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(UnifiedDesignState::default())
    }
}

fn evaluate_lightweight(uds: &UnifiedDesignState) -> DesignScoreVector {
    let node_count = uds.nodes.len() as u32;

    let non_empty_nodes = uds
        .nodes
        .values()
        .filter(|value| !value.split_whitespace().collect::<String>().is_empty())
        .count() as u32;

    let consistency = if node_count == 0 {
        100
    } else {
        (non_empty_nodes * 100) / node_count
    };

    // Phase A objective vector:
    // F = (consistency, -propagation_cost, -cyclic_penalty)
    // Existing DesignScoreVector is maximization-oriented, so we store
    // the two minimization objectives as inverse quality scores.
    let propagation_cost = propagation_cost_ratio(uds);
    let cyclic_penalty = cyclic_penalty_ratio(uds);
    let structural_integrity = inverse_ratio_score(propagation_cost);
    let dependency_soundness = inverse_ratio_score(cyclic_penalty);

    DesignScoreVector {
        consistency,
        structural_integrity,
        dependency_soundness,
    }
}

fn dominates(a: &DesignScoreVector, b: &DesignScoreVector) -> bool {
    let a_dims = [a.consistency, a.structural_integrity, a.dependency_soundness];
    let b_dims = [b.consistency, b.structural_integrity, b.dependency_soundness];
    let mut strictly_better = false;

    for (lhs, rhs) in a_dims.iter().zip(b_dims.iter()) {
        if lhs < rhs {
            return false;
        }
        if lhs > rhs {
            strictly_better = true;
        }
    }

    strictly_better
}

fn compute_state_vector_from(uds: &UnifiedDesignState, eval: &DesignScoreVector) -> StateVector {
    StateVector {
        consistency: eval.consistency as f64 / 100.0,
        propagation_quality: -propagation_cost_ratio(uds),
        cycle_quality: -cyclic_penalty_ratio(uds),
        modularity: modularity_score(uds),
    }
}

fn delta_complexity(before: &UnifiedDesignState, after: &UnifiedDesignState) -> f64 {
    after.nodes.len() as f64 - before.nodes.len() as f64
}

fn inverse_ratio_score(ratio: f64) -> u32 {
    ((1.0 - ratio).clamp(0.0, 1.0) * 100.0).round() as u32
}

fn propagation_cost_ratio(uds: &UnifiedDesignState) -> f64 {
    const LAMBDA: f64 = 0.60;
    propagation_cost_ratio_with_lambda(uds, LAMBDA)
}

fn propagation_cost_ratio_with_lambda(uds: &UnifiedDesignState, lambda: f64) -> f64 {
    let n = uds.nodes.len();
    if n <= 1 {
        return 0.0;
    }

    let keys = uds.nodes.keys().cloned().collect::<Vec<_>>();
    let index = keys
        .iter()
        .enumerate()
        .map(|(idx, key)| (key.clone(), idx))
        .collect::<BTreeMap<_, _>>();

    let mut adjacency = vec![Vec::<usize>::new(); n];
    for (owner, deps) in &uds.dependencies {
        let Some(&from) = index.get(owner) else {
            continue;
        };
        for dep in deps {
            let Some(&to) = index.get(dep) else {
                continue;
            };
            if from != to {
                adjacency[from].push(to);
            }
        }
    }
    for edges in &mut adjacency {
        edges.sort_unstable();
        edges.dedup();
    }
    const KAPPA: f64 = 0.8;

    let mut total_impact = 0.0_f64;
    for start in 0..n {
        let s = propagation_sum_from(start, &adjacency, lambda);
        total_impact += (KAPPA * s).ln_1p();
    }

    let normalizer = ((n - 1) as f64 * (1.0 + KAPPA).ln()).max(1e-12);
    (total_impact / (n as f64 * normalizer)).clamp(0.0, 1.0)
}

fn propagation_sum_from(start: usize, adjacency: &[Vec<usize>], lambda: f64) -> f64 {
    let mut dist = vec![usize::MAX; adjacency.len()];
    let mut queue = std::collections::VecDeque::new();
    dist[start] = 0;
    queue.push_back(start);

    while let Some(cur) = queue.pop_front() {
        let next_dist = dist[cur].saturating_add(1);
        for &next in &adjacency[cur] {
            if dist[next] == usize::MAX {
                dist[next] = next_dist;
                queue.push_back(next);
            }
        }
    }

    let mut sum = 0.0_f64;
    for (idx, d) in dist.iter().enumerate() {
        if idx == start || *d == usize::MAX {
            continue;
        }
        sum += lambda.powi(*d as i32);
    }
    sum
}

fn shortest_distance(start: usize, goal: usize, adjacency: &[Vec<usize>]) -> Option<usize> {
    if start == goal {
        return Some(0);
    }
    let mut dist = vec![usize::MAX; adjacency.len()];
    let mut queue = std::collections::VecDeque::new();
    dist[start] = 0;
    queue.push_back(start);
    while let Some(cur) = queue.pop_front() {
        let next_dist = dist[cur].saturating_add(1);
        for &next in &adjacency[cur] {
            if dist[next] == usize::MAX {
                dist[next] = next_dist;
                if next == goal {
                    return Some(next_dist);
                }
                queue.push_back(next);
            }
        }
    }
    None
}

fn modularity_score(uds: &UnifiedDesignState) -> f64 {
    let n = uds.nodes.len();
    if n == 0 {
        return 0.0;
    }

    let keys = uds.nodes.keys().cloned().collect::<Vec<_>>();
    let index = keys
        .iter()
        .enumerate()
        .map(|(idx, key)| (key.clone(), idx))
        .collect::<BTreeMap<_, _>>();

    let mut adjacency = vec![Vec::<usize>::new(); n];
    for (owner, deps) in &uds.dependencies {
        let Some(&from) = index.get(owner) else {
            continue;
        };
        for dep in deps {
            let Some(&to) = index.get(dep) else {
                continue;
            };
            if from != to {
                adjacency[from].push(to);
            }
        }
    }
    for edges in &mut adjacency {
        edges.sort_unstable();
        edges.dedup();
    }

    let sccs = tarjan_scc(&adjacency);
    let mut cluster_of = vec![usize::MAX; n];
    for (cid, component) in sccs.iter().enumerate() {
        for &node in component {
            cluster_of[node] = cid;
        }
    }

    let mut cohesion_sum = 0.0_f64;
    for from in 0..n {
        let mut cross = 0_usize;
        for &to in &adjacency[from] {
            if cluster_of[from] != cluster_of[to] {
                cross = cross.saturating_add(1);
            }
        }
        cohesion_sum += 1.0 / (1.0 + cross as f64);
    }

    (cohesion_sum / n as f64).clamp(0.0, 1.0)
}

fn cyclic_penalty_ratio(uds: &UnifiedDesignState) -> f64 {
    let n = uds.nodes.len();
    if n == 0 {
        return 0.0;
    }

    let keys = uds.nodes.keys().cloned().collect::<Vec<_>>();
    let index = keys
        .iter()
        .enumerate()
        .map(|(idx, key)| (key.clone(), idx))
        .collect::<BTreeMap<_, _>>();

    let mut adjacency = vec![Vec::<usize>::new(); n];
    let mut self_loops = vec![false; n];

    for (owner, deps) in &uds.dependencies {
        let Some(&from) = index.get(owner) else {
            continue;
        };
        for dep in deps {
            if dep == owner {
                self_loops[from] = true;
            }
            let Some(&to) = index.get(dep) else {
                continue;
            };
            if from != to {
                adjacency[from].push(to);
            }
        }
    }
    for edges in &mut adjacency {
        edges.sort_unstable();
        edges.dedup();
    }
    let edge_count = adjacency.iter().map(|edges| edges.len()).sum::<usize>();
    let max_edges = n * (n - 1);
    let graph_density = if max_edges == 0 {
        0.0
    } else {
        edge_count as f64 / max_edges as f64
    };

    let sccs = tarjan_scc(&adjacency);
    let mut penalty = 0.0_f64;

    for component in sccs {
        if component.len() > 1 {
            let cycle_intensity = internal_cycle_intensity(&component, &adjacency);
            penalty += (1.0 / component.len() as f64) * cycle_intensity;
        } else {
            // Optional self-loop penalty for singleton SCC.
            let idx = component[0];
            if self_loops[idx] {
                penalty += 1.0;
            }
        }
    }

    // Normalize by node count, then blend with graph density
    // to keep monotonicity against dense cyclic wiring.
    let normalized_cycle = (penalty / n as f64).clamp(0.0, 1.0);
    (0.7 * normalized_cycle + 0.3 * graph_density).clamp(0.0, 1.0)
}

fn tarjan_scc(adjacency: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct Tarjan<'a> {
        adjacency: &'a [Vec<usize>],
        index: usize,
        indices: Vec<Option<usize>>,
        lowlink: Vec<usize>,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        components: Vec<Vec<usize>>,
    }

    impl<'a> Tarjan<'a> {
        fn new(adjacency: &'a [Vec<usize>]) -> Self {
            let n = adjacency.len();
            Self {
                adjacency,
                index: 0,
                indices: vec![None; n],
                lowlink: vec![0; n],
                stack: Vec::new(),
                on_stack: vec![false; n],
                components: Vec::new(),
            }
        }

        fn run(mut self) -> Vec<Vec<usize>> {
            for v in 0..self.adjacency.len() {
                if self.indices[v].is_none() {
                    self.strong_connect(v);
                }
            }
            self.components
        }

        fn strong_connect(&mut self, v: usize) {
            let v_index = self.index;
            self.indices[v] = Some(v_index);
            self.lowlink[v] = v_index;
            self.index += 1;
            self.stack.push(v);
            self.on_stack[v] = true;

            for &w in &self.adjacency[v] {
                if self.indices[w].is_none() {
                    self.strong_connect(w);
                    self.lowlink[v] = self.lowlink[v].min(self.lowlink[w]);
                } else if self.on_stack[w] {
                    let w_index = self.indices[w].unwrap_or(v_index);
                    self.lowlink[v] = self.lowlink[v].min(w_index);
                }
            }

            if self.lowlink[v] == v_index {
                let mut component = Vec::new();
                loop {
                    let w = self.stack.pop().expect("tarjan stack should not be empty");
                    self.on_stack[w] = false;
                    component.push(w);
                    if w == v {
                        break;
                    }
                }
                self.components.push(component);
            }
        }
    }

    Tarjan::new(adjacency).run()
}

fn internal_cycle_intensity(component: &[usize], adjacency: &[Vec<usize>]) -> f64 {
    if component.len() <= 1 {
        return 0.0;
    }

    let mut in_component = vec![false; adjacency.len()];
    for &idx in component {
        in_component[idx] = true;
    }

    let mut edge_count = 0_usize;
    for &from in component {
        for &to in &adjacency[from] {
            if in_component[to] {
                edge_count += 1;
            }
        }
    }

    let size = component.len();
    if size == 0 {
        0.0
    } else {
        edge_count as f64 / size as f64
    }
}

#[cfg(test)]
mod tests {
    use super::{AppState, NodeIdState, ParetoResult, PromotionError, UnifiedDesignState};
    use crate::domain::hash::compute_hash;
    use crate::domain::transaction::ProposedDiff;

    fn sample_uds() -> UnifiedDesignState {
        let mut uds = UnifiedDesignState::default();
        uds.nodes.insert("A".to_string(), "alpha".to_string());
        uds.nodes.insert("B".to_string(), "beta".to_string());
        uds.nodes.insert("C".to_string(), "gamma".to_string());
        uds.dependencies
            .insert("A".to_string(), vec!["B".to_string()]);
        uds
    }

    fn sample_valid_diff() -> ProposedDiff {
        ProposedDiff::UpsertNode {
            key: "TX_NODE".to_string(),
            value: "transactional value".to_string(),
        }
    }

    #[test]
    fn abort_restores_original_state() {
        let mut app = AppState::new(sample_uds());

        let original_uds = app.uds.clone();
        let original_eval = app.evaluation.clone();
        let original_hash = compute_hash(&app.uds);

        app.begin_tx().expect("begin_tx should succeed");
        app.apply_diff(sample_valid_diff())
            .expect("apply_diff should succeed");
        app.abort_tx().expect("abort should succeed");

        assert_eq!(app.uds, original_uds);
        assert_eq!(app.evaluation, original_eval);
        assert_eq!(compute_hash(&app.uds), original_hash);

        let current_snapshot = app
            .session_history
            .current()
            .expect("initial snapshot should exist");
        assert_eq!(current_snapshot.uds, app.uds);
        assert_eq!(compute_hash(&current_snapshot.uds), compute_hash(&app.uds));
    }

    #[test]
    fn commit_then_undo_restores_initial_state_and_hash() {
        let mut app = AppState::new(sample_uds());
        let initial_uds = app.uds.clone();
        let initial_hash = compute_hash(&app.uds);

        app.begin_tx().expect("begin_tx should succeed");
        app.apply_diff(sample_valid_diff())
            .expect("apply_diff should succeed");
        app.commit_tx().expect("commit should succeed");
        app.undo().expect("undo should succeed");

        assert_eq!(app.uds, initial_uds);
        assert_eq!(compute_hash(&app.uds), initial_hash);
    }

    #[test]
    fn abort_does_not_pollute_snapshot_or_future_transactions() {
        let mut app = AppState::new(sample_uds());
        let baseline_snapshot = app
            .session_history
            .current()
            .expect("initial snapshot should exist")
            .clone();
        let baseline_hash = compute_hash(&baseline_snapshot.uds);

        app.begin_tx().expect("begin_tx should succeed");
        let tx_snapshot_before = app
            .tx_engine
            .current_tx()
            .expect("active tx should exist")
            .snapshot_before
            .clone();
        app.apply_diff(sample_valid_diff())
            .expect("apply_diff should succeed");
        app.abort_tx().expect("abort should succeed");

        assert_eq!(app.uds, tx_snapshot_before.uds);
        assert_eq!(compute_hash(&app.uds), compute_hash(&tx_snapshot_before.uds));
        assert_eq!(compute_hash(&baseline_snapshot.uds), baseline_hash);

        app.begin_tx().expect("begin_tx should succeed");
        app.apply_diff(ProposedDiff::UpsertNode {
            key: "after-abort".to_string(),
            value: "safe".to_string(),
        })
        .expect("apply_diff should succeed");
        app.commit_tx().expect("commit should succeed");

        assert_eq!(compute_hash(&baseline_snapshot.uds), baseline_hash);
        assert_ne!(compute_hash(&app.uds), baseline_hash);
    }

    #[test]
    fn multi_stage_commit_commit_abort_then_undo_redo_is_consistent() {
        let mut app = AppState::new(sample_uds());

        app.begin_tx().expect("begin_tx should succeed");
        app.apply_diff(ProposedDiff::UpsertNode {
            key: "A-commit".to_string(),
            value: "one".to_string(),
        })
        .expect("apply_diff should succeed");
        app.commit_tx().expect("commit should succeed");
        let hash_after_a = compute_hash(&app.uds);
        let eval_after_a = app.evaluation.clone();

        app.begin_tx().expect("begin_tx should succeed");
        app.apply_diff(ProposedDiff::UpsertNode {
            key: "B-commit".to_string(),
            value: "two".to_string(),
        })
        .expect("apply_diff should succeed");
        app.commit_tx().expect("commit should succeed");
        let hash_after_b = compute_hash(&app.uds);
        let eval_after_b = app.evaluation.clone();

        app.begin_tx().expect("begin_tx should succeed");
        app.apply_diff(ProposedDiff::UpsertNode {
            key: "C-abort".to_string(),
            value: "three".to_string(),
        })
        .expect("apply_diff should succeed");
        app.abort_tx().expect("abort should succeed");
        assert_eq!(compute_hash(&app.uds), hash_after_b);

        app.undo().expect("undo should succeed");
        assert_eq!(compute_hash(&app.uds), hash_after_a);
        assert_eq!(app.evaluation, eval_after_a);

        app.redo().expect("redo should succeed");
        assert_eq!(compute_hash(&app.uds), hash_after_b);
        assert_eq!(app.evaluation, eval_after_b);
    }

    #[test]
    fn history_limit_of_100_is_enforced() {
        let mut state = AppState::default();

        for idx in 0..101 {
            state.begin_tx().expect("begin_tx should succeed");
            state
                .apply_diff(ProposedDiff::UpsertNode {
                    key: format!("node-{idx}"),
                    value: format!("value-{idx}"),
                })
                .expect("apply_diff should succeed");
            state.commit_tx().expect("commit should succeed");
        }

        assert_eq!(state.session_history.max_size(), 100);
        assert_eq!(state.session_history.len(), 100);
        assert_eq!(state.session_history.current_index(), 99);
    }

    #[test]
    fn hash_is_stable_for_equivalent_structure_with_different_build_order() {
        let mut uds_a = UnifiedDesignState::default();
        uds_a.nodes.insert("n2".to_string(), "second".to_string());
        uds_a
            .nodes
            .insert("n1".to_string(), "  hello   world ".to_string());
        uds_a.nodes.insert("n3".to_string(), "third".to_string());
        uds_a
            .dependencies
            .insert("n1".to_string(), vec!["n2".to_string(), "n3".to_string(), "n2".to_string()]);
        uds_a
            .dependencies
            .insert("n3".to_string(), vec!["n1".to_string()]);

        let mut uds_b = UnifiedDesignState::default();
        uds_b.nodes.insert("n3".to_string(), "third".to_string());
        uds_b
            .nodes
            .insert("n1".to_string(), "hello world".to_string());
        uds_b.nodes.insert("n2".to_string(), "second".to_string());
        uds_b
            .dependencies
            .insert("n1".to_string(), vec!["n3".to_string(), "n2".to_string()]);
        uds_b
            .dependencies
            .insert("n3".to_string(), vec!["n1".to_string()]);

        let hash_a = compute_hash(&uds_a);
        let hash_b = compute_hash(&uds_b);
        assert_eq!(hash_a, hash_b);
    }

    #[test]
    fn replace_uds_without_transaction_returns_error() {
        let mut app = AppState::default();
        let mut new_uds = UnifiedDesignState::default();
        new_uds
            .nodes
            .insert("standalone".to_string(), "value".to_string());

        let err = app.replace_uds(new_uds).expect_err("replace_uds must fail");
        assert!(matches!(err, crate::domain::transaction::TxError::NoActiveTransaction));
    }

    #[test]
    fn evaluate_now_updates_only_evaluation_without_touching_history_or_hash() {
        let mut app = AppState::default();
        app.begin_tx().expect("begin tx");
        app.apply_diff(ProposedDiff::UpsertNode {
            key: "x".to_string(),
            value: "".to_string(),
        })
        .expect("apply diff");
        app.commit_tx().expect("commit");

        let hash_before = compute_hash(&app.uds);
        let history_before = app.session_history.len();
        let version_before = app.current_version_id();
        app.evaluation.consistency = 777;

        app.evaluate_now().expect("analyze should succeed");

        assert_eq!(compute_hash(&app.uds), hash_before);
        assert_eq!(app.session_history.len(), history_before);
        assert_eq!(app.current_version_id(), version_before);
        assert_ne!(app.evaluation.consistency, 777);
    }

    #[test]
    fn evaluate_now_is_idempotent_for_hash_history_and_version() {
        let mut app = AppState::new(sample_uds());
        let hash_before = compute_hash(&app.uds);
        let history_before = app.session_history.len();
        let version_before = app.current_version_id();
        let eval_first = app.evaluation.clone();

        app.evaluate_now().expect("first evaluate_now");
        let eval_after_first = app.evaluation.clone();
        app.evaluate_now().expect("second evaluate_now");
        let eval_after_second = app.evaluation.clone();

        assert_eq!(compute_hash(&app.uds), hash_before);
        assert_eq!(app.session_history.len(), history_before);
        assert_eq!(app.current_version_id(), version_before);
        assert_eq!(eval_after_first, eval_after_second);
        assert_eq!(eval_after_first, eval_first);
    }

    #[test]
    fn analyze_pareto_is_non_destructive() {
        let mut app = AppState::new(sample_uds());
        app.begin_tx().expect("begin");
        app.apply_diff(ProposedDiff::UpsertNode {
            key: "D".to_string(),
            value: "".to_string(),
        })
        .expect("apply");
        app.commit_tx().expect("commit");

        let uds_before = app.uds.clone();
        let hash_before = compute_hash(&app.uds);
        let history_before = app.session_history.len();
        let eval_before = app.evaluation.clone();

        let result = app.analyze_pareto().expect("pareto analyze should succeed");
        assert!(!result.scores.is_empty());
        assert!(!result.frontier_indices.is_empty());

        assert_eq!(app.uds, uds_before);
        assert_eq!(compute_hash(&app.uds), hash_before);
        assert_eq!(app.session_history.len(), history_before);
        assert_eq!(app.evaluation, eval_before);
    }

    #[test]
    fn analyze_pareto_rejects_active_transaction() {
        let mut app = AppState::new(sample_uds());
        app.begin_tx().expect("begin tx");
        let err = app.analyze_pareto().expect_err("analyze should fail in tx");
        assert!(matches!(err, super::AnalyzeError::TransactionInProgress));
    }

    #[test]
    fn suggest_diffs_is_non_destructive() {
        let mut uds = sample_uds();
        uds.nodes.insert("EMPTY".to_string(), String::new());
        let app = AppState::new(uds);
        let pareto = app.analyze_pareto().expect("pareto");

        let uds_before = app.uds.clone();
        let hash_before = compute_hash(&app.uds);
        let history_before = app.session_history.len();
        let eval_before = app.evaluation.clone();

        let _suggestions = app
            .suggest_diffs_from_analysis(&pareto)
            .expect("suggest should succeed");

        assert_eq!(app.uds, uds_before);
        assert_eq!(compute_hash(&app.uds), hash_before);
        assert_eq!(app.session_history.len(), history_before);
        assert_eq!(app.evaluation, eval_before);
    }

    #[test]
    fn suggest_diffs_rejects_active_transaction() {
        let mut app = AppState::new(sample_uds());
        app.begin_tx().expect("begin");
        let pareto = ParetoResult {
            frontier_indices: vec![0],
            scores: vec![app.evaluation.clone()],
        };
        let err = app
            .suggest_diffs_from_analysis(&pareto)
            .expect_err("suggest should fail in tx");
        assert!(matches!(err, super::SuggestError::TransactionInProgress));
    }

    #[test]
    fn strict_guard_filters_non_improving_candidates() {
        let app = AppState::new(sample_uds());
        let pareto = app.analyze_pareto().expect("pareto");
        let suggestions = app
            .suggest_diffs_from_analysis(&pareto)
            .expect("suggest should succeed");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn strict_guard_accepts_consistency_improving_candidate() {
        let mut uds = sample_uds();
        uds.nodes.insert("EMPTY".to_string(), " ".to_string());
        let app = AppState::new(uds);
        let pareto = app.analyze_pareto().expect("pareto");
        let suggestions = app
            .suggest_diffs_from_analysis(&pareto)
            .expect("suggest should succeed");
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn uds_key_stability_test() {
        let mut app = AppState::new(sample_uds());

        app.begin_tx().expect("begin remove tx");
        app.apply_diff(ProposedDiff::RemoveNode {
            key: "A".to_string(),
        })
        .expect("remove existing key");
        app.commit_tx().expect("commit remove tx");
        assert!(!app.uds.nodes.contains_key("A"));

        app.begin_tx().expect("begin reinsert tx");
        app.apply_diff(ProposedDiff::UpsertNode {
            key: "A".to_string(),
            value: "alpha-reused".to_string(),
        })
        .expect("reinsert same key");
        app.commit_tx().expect("commit reinsert tx");

        assert_eq!(
            app.uds.nodes.get("A").map(String::as_str),
            Some("alpha-reused")
        );
    }

    #[test]
    fn uds_key_order_independence() {
        let mut left = UnifiedDesignState::default();
        left.nodes.insert("k2".to_string(), "v2".to_string());
        left.nodes.insert("k1".to_string(), "v1".to_string());
        left.nodes.insert("k3".to_string(), "v3".to_string());

        let mut right = UnifiedDesignState::default();
        right.nodes.insert("k3".to_string(), "v3".to_string());
        right.nodes.insert("k1".to_string(), "v1".to_string());
        right.nodes.insert("k2".to_string(), "v2".to_string());

        let left_keys = left.nodes.keys().cloned().collect::<Vec<_>>();
        let right_keys = right.nodes.keys().cloned().collect::<Vec<_>>();
        assert_eq!(left_keys, right_keys);
        assert_eq!(compute_hash(&left), compute_hash(&right));
    }

    #[test]
    fn uds_dependency_key_integrity() {
        let mut app = AppState::new(sample_uds());

        app.begin_tx().expect("begin dependency tx");
        let err = app
            .apply_diff(ProposedDiff::SetDependencies {
                key: "A".to_string(),
                dependencies: vec!["MISSING".to_string()],
            })
            .expect_err("missing dependency key must fail");
        assert!(matches!(
            err,
            crate::domain::transaction::TxError::MissingDependency(dep) if dep == "MISSING"
        ));
        app.abort_tx().expect("abort dependency tx");

        app.begin_tx().expect("begin remove tx");
        app.apply_diff(ProposedDiff::RemoveNode {
            key: "B".to_string(),
        })
        .expect("remove B");
        app.commit_tx().expect("commit remove tx");

        let deps_of_a = app.uds.dependencies.get("A").cloned().unwrap_or_default();
        assert!(!deps_of_a.iter().any(|dep| dep == "B"));
    }

    #[test]
    fn promotion_uses_dependency_closure_unit() {
        let mut uds = UnifiedDesignState::default();
        uds.nodes.insert("A".to_string(), "a".to_string());
        uds.nodes.insert("B".to_string(), "b".to_string());
        uds.nodes.insert("C".to_string(), "c".to_string());
        uds.nodes.insert("D".to_string(), "d".to_string());
        uds.dependencies
            .insert("A".to_string(), vec!["B".to_string()]);
        uds.dependencies
            .insert("B".to_string(), vec!["C".to_string()]);
        uds.dependencies
            .insert("D".to_string(), vec!["C".to_string()]);

        let mut app = AppState::new(uds);
        let report = app
            .promote_node_with_dependency_closure("A", "origin-1", 1000)
            .expect("promotion should succeed");

        assert_eq!(report.promotion_unit, vec!["A", "B", "C"]);
        assert_eq!(
            app.uds.node_id_states.get("A"),
            Some(&NodeIdState::Global)
        );
        assert_eq!(
            app.uds.node_id_states.get("B"),
            Some(&NodeIdState::Global)
        );
        assert_eq!(
            app.uds.node_id_states.get("C"),
            Some(&NodeIdState::Global)
        );
        assert_eq!(
            app.uds.node_id_states.get("D"),
            None
        );
        assert_eq!(app.uds.node_origins.get("D"), None);
    }

    #[test]
    fn promotion_keeps_existing_global_origin_unchanged() {
        let mut uds = UnifiedDesignState::default();
        uds.nodes.insert("A".to_string(), "a".to_string());
        uds.nodes.insert("B".to_string(), "b".to_string());
        uds.dependencies
            .insert("A".to_string(), vec!["B".to_string()]);
        uds.node_id_states
            .insert("A".to_string(), NodeIdState::PendingPromotion);
        uds.node_id_states
            .insert("B".to_string(), NodeIdState::Global);
        uds.node_origins
            .insert("B".to_string(), "existing-origin".to_string());

        let mut app = AppState::new(uds);
        let report = app
            .promote_node_with_dependency_closure("A", "new-origin", 1000)
            .expect("promotion should succeed");

        assert_eq!(report.promoted_count, 1);
        assert_eq!(report.kept_global_count, 1);
        assert_eq!(
            app.uds.node_origins.get("A").map(String::as_str),
            Some("new-origin")
        );
        assert_eq!(
            app.uds.node_origins.get("B").map(String::as_str),
            Some("existing-origin")
        );
    }

    #[test]
    fn promotion_warns_for_large_closure() {
        let mut uds = UnifiedDesignState::default();
        uds.nodes.insert("A".to_string(), "a".to_string());
        uds.nodes.insert("B".to_string(), "b".to_string());
        uds.dependencies
            .insert("A".to_string(), vec!["B".to_string()]);

        let mut app = AppState::new(uds);
        let report = app
            .promote_node_with_dependency_closure("A", "origin-1", 1)
            .expect("promotion should succeed");

        assert!(report.closure_size_warning);
    }

    #[test]
    fn promotion_rolls_back_on_missing_root() {
        let mut app = AppState::new(sample_uds());
        let before = app.uds.clone();
        let before_hash = compute_hash(&before);

        let err = app
            .promote_node_with_dependency_closure("MISSING", "origin-1", 1000)
            .expect_err("missing root must fail");

        assert!(matches!(err, PromotionError::RootNodeNotFound(node) if node == "MISSING"));
        assert_eq!(app.uds, before);
        assert_eq!(compute_hash(&app.uds), before_hash);
    }
}
