use semantic_dhm::{ConceptId, ConceptUnit};

#[derive(Clone, Debug, PartialEq)]
pub struct ResonanceReport {
    pub c1: ConceptId,
    pub c2: ConceptId,
    pub score: f32,
    pub v_sim: f32,
    pub s_sim: f32,
    pub a_diff: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explanation {
    pub summary: String,
    pub reasoning: String,
    pub abstraction_note: String,
}

#[derive(Default)]
pub struct Recomposer;

impl Recomposer {
    pub fn explain_concept(&self, c: &ConceptUnit) -> Explanation {
        let abs = abstraction_phrase(c.a);
        Explanation {
            summary: format!("This concept represents a {abs}."),
            reasoning: format!("Abstraction score = {:.2}.", round2(c.a)),
            abstraction_note: format!("Abstraction level: {abs}."),
        }
    }

    pub fn explain_resonance(&self, report: &ResonanceReport) -> Explanation {
        let align = alignment_phrase(report.score);
        let mut abstraction_note = String::new();
        if report.s_sim >= 0.6 {
            abstraction_note.push_str("with significant structural similarity");
        }
        if report.a_diff >= 0.4 {
            if !abstraction_note.is_empty() {
                abstraction_note.push_str("; ");
            }
            abstraction_note.push_str("at different abstraction levels");
        }
        if abstraction_note.is_empty() {
            abstraction_note.push_str("no additional structural or abstraction note");
        }
        Explanation {
            summary: format!("Concept A is {align} with Concept B."),
            reasoning: format!(
                "Semantic similarity = {:.2}, structural similarity = {:.2}, abstraction difference = {:.2}.",
                round2(report.v_sim),
                round2(report.s_sim),
                round2(report.a_diff),
            ),
            abstraction_note,
        }
    }
}

fn abstraction_phrase(a: f32) -> &'static str {
    if a < 0.30 {
        "concrete design element"
    } else if a < 0.70 {
        "mid-level structural concept"
    } else {
        "high-level architectural abstraction"
    }
}

fn alignment_phrase(score: f32) -> &'static str {
    if score >= 0.75 {
        "strongly aligned"
    } else if score >= 0.40 {
        "moderately aligned"
    } else if score >= 0.10 {
        "weakly aligned"
    } else if score > -0.10 {
        "structurally neutral"
    } else {
        "structurally conflicting"
    }
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use semantic_dhm::{ConceptQuery, SemanticDhm};

    use super::{Recomposer, ResonanceReport, abstraction_phrase, alignment_phrase};

    fn sample_query(a: f32) -> ConceptQuery {
        ConceptQuery {
            v: vec![1.0; 384],
            a,
            s: vec![0.5; 384],
        }
    }

    #[test]
    fn abstraction_boundary_test() {
        assert_eq!(abstraction_phrase(0.29), "concrete design element");
        assert_eq!(abstraction_phrase(0.30), "mid-level structural concept");
        assert_eq!(abstraction_phrase(0.70), "high-level architectural abstraction");
    }

    #[test]
    fn resonance_classification_test() {
        assert_eq!(alignment_phrase(0.80), "strongly aligned");
        assert_eq!(alignment_phrase(0.50), "moderately aligned");
        assert_eq!(alignment_phrase(0.20), "weakly aligned");
        assert_eq!(alignment_phrase(0.00), "structurally neutral");
        assert_eq!(alignment_phrase(-0.20), "structurally conflicting");
    }

    #[test]
    fn deterministic_output_test() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id = dhm.insert_query(&sample_query(0.45));
        let c = dhm.get(id).expect("concept");
        let r = Recomposer;
        let e1 = r.explain_concept(&c);
        let e2 = r.explain_concept(&c);
        assert_eq!(e1, e2);

        let rep = ResonanceReport {
            c1: c.id,
            c2: c.id,
            score: 0.42,
            v_sim: 0.51,
            s_sim: 0.61,
            a_diff: 0.10,
        };
        let x1 = r.explain_resonance(&rep);
        let x2 = r.explain_resonance(&rep);
        assert_eq!(x1, x2);
    }

    #[test]
    fn non_mutation_test() {
        let path = std::env::temp_dir().join(format!(
            "recomposer_non_mutation_{}.bin",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut dhm = SemanticDhm::file(&path).expect("open");
        let id = dhm.insert_query(&sample_query(0.33));
        let before = dhm.get(id).expect("before");

        let rc = Recomposer;
        let _ = rc.explain_concept(&before);

        let q = sample_query(0.33);
        let score = dhm
            .recall(&q, 1)
            .first()
            .map(|(_, s)| *s)
            .unwrap_or(0.0);

        let rep = ResonanceReport {
            c1: id,
            c2: id,
            score,
            v_sim: 1.0,
            s_sim: 1.0,
            a_diff: 0.0,
        };
        let _ = rc.explain_resonance(&rep);

        let after = dhm.get(id).expect("after");
        assert_eq!(before, after);
        let _ = std::fs::remove_file(path);
    }
}
