use std::collections::BTreeSet;
use std::env;
use std::path::PathBuf;

use hybrid_vm::{ActionType, ConceptId, HybridVM, VmDecisionWeights};

#[derive(Debug, PartialEq)]
enum Commands {
    Analyze {
        text: String,
    },
    Explain {
        concept_id: String,
    },
    Compare {
        left_id: String,
        right_id: String,
    },
    Multi {
        concept_ids: Vec<String>,
    },
    Report {
        concept_ids: Vec<String>,
    },
    Recommend {
        concept_id: String,
        top_k: usize,
    },
    Decide {
        concept_ids: Vec<String>,
        weights: VmDecisionWeights,
    },
}

#[derive(Debug, PartialEq)]
struct ParsedCommand {
    command: Commands,
    json: bool,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match parse_command(&args).and_then(run) {
        Ok(out) => println!("{out}"),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}

fn run(parsed: ParsedCommand) -> Result<String, String> {
    let mut vm = HybridVM::for_cli_storage(cli_store_dir()).map_err(|e| e.to_string())?;
    let report_top_k = 3usize;

    match parsed.command {
        Commands::Analyze { text } => {
            let concept = vm.analyze_text(&text).map_err(|e| e.to_string())?;
            let concept_id = concept_id_string(concept.id);
            let abstraction = round2(concept.a);
            let abstraction_label = abstraction_phrase(concept.a);
            if parsed.json {
                Ok(format!(
                    "{{\n    \"type\": \"analyze\",\n    \"concept_id\": \"{}\",\n    \"abstraction\": {},\n    \"abstraction_label\": \"{}\"\n}}",
                    concept_id,
                    json_num(abstraction),
                    escape_json(abstraction_label),
                ))
            } else {
                Ok(format!(
                    "[Concept Created]\nID: {}\nAbstraction: {:.2} ({})",
                    concept_id, abstraction, abstraction_label,
                ))
            }
        }
        Commands::Explain { concept_id } => {
            let id = parse_concept_id(&concept_id)?;
            let Some(concept) = vm.get_concept(id) else {
                return Err("Concept not found".to_string());
            };
            let explanation = HybridVM::recomposer().explain_concept(&concept);
            if parsed.json {
                Ok(format!(
                    "{{\n    \"type\": \"explain\",\n    \"concept_id\": \"{}\",\n    \"summary\": \"{}\",\n    \"reasoning\": \"{}\",\n    \"abstraction_note\": \"{}\"\n}}",
                    concept_id_string(concept.id),
                    escape_json(&explanation.summary),
                    escape_json(&explanation.reasoning),
                    escape_json(&explanation.abstraction_note),
                ))
            } else {
                Ok(format!(
                    "Summary:\n{}\n\nReasoning:\n{}\n\nAbstraction:\n{}",
                    explanation.summary, explanation.reasoning, explanation.abstraction_note,
                ))
            }
        }
        Commands::Compare { left_id, right_id } => {
            let left = parse_concept_id(&left_id)?;
            let right = parse_concept_id(&right_id)?;
            let report = vm.compare(left, right).map_err(|e| e.to_string())?;
            if parsed.json {
                Ok(format!(
                    "{{\n    \"type\": \"compare\",\n    \"concept_a\": \"{}\",\n    \"concept_b\": \"{}\",\n    \"semantic_similarity\": {},\n    \"structural_similarity\": {},\n    \"abstraction_difference\": {},\n    \"alignment_label\": \"{}\"\n}}",
                    concept_id_string(report.c1),
                    concept_id_string(report.c2),
                    json_num(round2(report.v_sim)),
                    json_num(round2(report.s_sim)),
                    json_num(round2(report.a_diff)),
                    escape_json(alignment_phrase(report.score)),
                ))
            } else {
                let explanation = HybridVM::recomposer().explain_resonance(&report);
                Ok(format!(
                    "Summary:\n{}\n\nReasoning:\n{}\n\nAbstraction:\n{}",
                    explanation.summary, explanation.reasoning, explanation.abstraction_note,
                ))
            }
        }
        Commands::Multi { concept_ids } => {
            let ids = dedup_parsed_ids(&concept_ids)?;
            if ids.len() < 2 {
                return Err("multi requires at least 2 unique concept ids".to_string());
            }
            let out = vm.explain_multiple(&ids).map_err(|e| e.to_string())?;
            if parsed.json {
                let mean_resonance = parse_metric(&out.metrics.pairwise_mean_r)?;
                let mean_abstraction = parse_metric(&out.metrics.mean_abstraction)?;
                let concept_ids_json = ids
                    .iter()
                    .map(|id| format!("\"{}\"", concept_id_string(*id)))
                    .collect::<Vec<_>>()
                    .join(", ");
                Ok(format!(
                    "{{\n    \"type\": \"multi\",\n    \"concept_ids\": [{}],\n    \"mean_resonance\": {},\n    \"mean_abstraction\": {},\n    \"conflict_pairs\": {},\n    \"coherence_label\": \"{}\",\n    \"abstraction_label\": \"{}\"\n}}",
                    concept_ids_json,
                    json_num(mean_resonance),
                    json_num(mean_abstraction),
                    out.metrics.conflict_pairs,
                    escape_json(coherence_phrase(mean_resonance)),
                    escape_json(abstraction_tendency_phrase(mean_abstraction)),
                ))
            } else {
                Ok(format!(
                    "Summary:\n{}\n\nStructural Analysis:\n{}\n\nAbstraction Analysis:\n{}\n\nConflict Analysis:\n{}",
                    out.summary,
                    out.structural_analysis,
                    out.abstraction_analysis,
                    out.conflict_analysis,
                ))
            }
        }
        Commands::Recommend { concept_id, top_k } => {
            let id = parse_concept_id(&concept_id)?;
            let report = vm.recommend(id, top_k).map_err(|e| e.to_string())?;
            if parsed.json {
                let recommendations = report
                    .recommendations
                    .iter()
                    .map(|rec| {
                        format!(
                            "        {{\n            \"target\": \"{}\",\n            \"action\": \"{}\",\n            \"score\": {}\n        }}",
                            concept_id_string(rec.target),
                            action_label(rec.action),
                            json_num(round2(rec.score)),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",\n");
                Ok(format!(
                    "{{\n    \"type\": \"recommend\",\n    \"query\": \"{}\",\n    \"recommendations\": [\n{}\n    ],\n    \"summary\": \"{}\"\n}}",
                    concept_id_string(id),
                    recommendations,
                    escape_json(&report.summary),
                ))
            } else {
                let mut out = String::from("[Recommendations]\n");
                if report.recommendations.is_empty() {
                    out.push_str("No candidates available.");
                    return Ok(out);
                }
                for (idx, rec) in report.recommendations.iter().enumerate() {
                    let line = match rec.action {
                        ActionType::Merge => {
                            format!(
                                "{}. Merge with {} (R={:.2})",
                                idx + 1,
                                concept_id_string(rec.target),
                                round2(rec.score)
                            )
                        }
                        ActionType::Refine => format!(
                            "{}. Refine with {} (R={:.2})",
                            idx + 1,
                            concept_id_string(rec.target),
                            round2(rec.score)
                        ),
                        ActionType::ApplyPattern => {
                            format!(
                                "{}. ApplyPattern from {}",
                                idx + 1,
                                concept_id_string(rec.target)
                            )
                        }
                        ActionType::ResolveDirectionalConflict => {
                            format!(
                                "{}. ResolveDirectionalConflict with {}",
                                idx + 1,
                                concept_id_string(rec.target)
                            )
                        }
                        ActionType::Separate => {
                            format!(
                                "{}. Separate from {} (R={:.2})",
                                idx + 1,
                                concept_id_string(rec.target),
                                round2(rec.score)
                            )
                        }
                    };
                    out.push_str(&line);
                    if idx + 1 != report.recommendations.len() {
                        out.push('\n');
                    }
                }
                Ok(out)
            }
        }

        Commands::Decide {
            concept_ids,
            weights,
        } => {
            let ids = dedup_parsed_ids(&concept_ids)?;
            if ids.is_empty() {
                return Err("decide requires at least 1 concept id".to_string());
            }
            let report = vm.decide(&ids, weights).map_err(|e| e.to_string())?;
            if parsed.json {
                let warning = match report.warning {
                    Some(w) => format!(
                        ",
    \"warning\": \"{}\"",
                        escape_json(&w)
                    ),
                    None => String::new(),
                };
                Ok(format!(
                    "{{
    \"decision_score\": {},
    \"weights\": {{
        \"coherence\": {},
        \"stability\": {},
        \"conflict\": {},
        \"tradeoff\": {}
    }},
    \"interpretation\": \"{}\"{}
}}",
                    json_num(round2(report.decision_score)),
                    json_num(round2(report.weights.coherence)),
                    json_num(round2(report.weights.stability)),
                    json_num(round2(report.weights.conflict)),
                    json_num(round2(report.weights.tradeoff)),
                    escape_json(&report.interpretation),
                    warning,
                ))
            } else {
                let mut out = format!(
                    "[Decision]
Score: {:.2}
Interpretation: {}",
                    round2(report.decision_score),
                    report.interpretation
                );
                if let Some(w) = report.warning {
                    out.push_str(&format!(
                        "
Warning: {w}"
                    ));
                }
                Ok(out)
            }
        }
        Commands::Report { concept_ids } => {
            let ids = dedup_parsed_ids(&concept_ids)?;
            if ids.is_empty() {
                return Err("report requires at least 1 concept id".to_string());
            }
            let report = vm
                .design_report(&ids, report_top_k)
                .map_err(|e| e.to_string())?;
            if parsed.json {
                let tradeoff_items = report
                    .consistency
                    .tradeoffs
                    .iter()
                    .map(|t| {
                        format!(
                            "            {{\n                \"pair\": [\"{}\", \"{}\"],\n                \"tension\": {}\n            }}",
                            concept_id_string(t.pair.0),
                            concept_id_string(t.pair.1),
                            json_num(round2(t.tension)),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",\n");
                let rec_items = report
                    .recommendations
                    .recommendations
                    .iter()
                    .map(|rec| {
                        format!(
                            "            {{\n                \"target\": \"{}\",\n                \"action\": \"{}\",\n                \"score\": {}\n            }}",
                            concept_id_string(rec.target),
                            action_label(rec.action),
                            json_num(round2(rec.score)),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",\n");
                Ok(format!(
                    "{{\n    \"summary\": \"{}\",\n    \"abstraction_mean\": {},\n    \"abstraction_variance\": {},\n    \"consistency\": {{\n        \"directional_conflicts\": {},\n        \"structural_conflicts\": {},\n        \"tradeoffs\": [\n{}\n        ],\n        \"stability\": {}\n    }},\n    \"global_coherence\": {},\n    \"recommendations\": {{\n        \"summary\": \"{}\",\n        \"items\": [\n{}\n        ]\n    }}\n}}",
                    escape_json(&report.summary),
                    json_num(round2(report.abstraction_mean)),
                    json_num(round2(report.abstraction_variance)),
                    report.consistency.directional_conflicts,
                    report.consistency.structural_conflicts,
                    tradeoff_items,
                    json_num(round2(report.consistency.stability_score)),
                    json_num(round2(report.global_coherence)),
                    escape_json(&report.recommendations.summary),
                    rec_items,
                ))
            } else {
                let mut out = format!(
                    "=== Design Report ===\n\nSummary:\n{}\n\nAbstraction:\nMean: {:.2}\nVariance: {:.2}\n\nConsistency:\nDirectional conflicts: {}\nStructural conflicts: {}\nTradeoffs: {}\nStability: {:.2}\n\nGlobal coherence: {:.2}\n\nTop Recommendations:\n",
                    report.summary,
                    round2(report.abstraction_mean),
                    round2(report.abstraction_variance),
                    report.consistency.directional_conflicts,
                    report.consistency.structural_conflicts,
                    report.consistency.tradeoffs.len(),
                    round2(report.consistency.stability_score),
                    round2(report.global_coherence),
                );
                for t in &report.consistency.tradeoffs {
                    out.push_str(&format!(
                        "\n - {} ↔ {} (tension={:.2})",
                        concept_id_string(t.pair.0),
                        concept_id_string(t.pair.1),
                        round2(t.tension),
                    ));
                }
                out.push('\n');
                if report.recommendations.recommendations.is_empty() {
                    out.push_str("1. No recommendation candidates available.");
                } else {
                    for (idx, rec) in report.recommendations.recommendations.iter().enumerate() {
                        out.push_str(&format!("{}. {}", idx + 1, rec.rationale));
                        if idx + 1 != report.recommendations.recommendations.len() {
                            out.push('\n');
                        }
                    }
                }
                Ok(out)
            }
        }
    }
}

fn parse_command(args: &[String]) -> Result<ParsedCommand, String> {
    let json = args.iter().any(|arg| arg == "--json");
    let filtered = args
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .cloned()
        .collect::<Vec<_>>();

    let Some(cmd) = filtered.first() else {
        return Err(help_text());
    };

    let command = match cmd.as_str() {
        "analyze" => {
            if filtered.len() < 2 {
                return Err("analyze requires text".to_string());
            }
            Commands::Analyze {
                text: filtered[1..].join(" "),
            }
        }
        "explain" => {
            if filtered.len() != 2 {
                return Err("explain requires one concept id".to_string());
            }
            Commands::Explain {
                concept_id: filtered[1].clone(),
            }
        }
        "compare" => {
            if filtered.len() != 3 {
                return Err("compare requires two concept ids".to_string());
            }
            Commands::Compare {
                left_id: filtered[1].clone(),
                right_id: filtered[2].clone(),
            }
        }
        "multi" => {
            if filtered.len() < 3 {
                return Err("multi requires at least 2 concept ids".to_string());
            }
            Commands::Multi {
                concept_ids: filtered[1..].to_vec(),
            }
        }
        "report" => {
            if filtered.len() < 2 {
                return Err("report requires at least 1 concept id".to_string());
            }
            Commands::Report {
                concept_ids: filtered[1..].to_vec(),
            }
        }
        "recommend" => parse_recommend_command(&filtered)?,
        "decide" => parse_decide_command(&filtered)?,
        _ => return Err(help_text()),
    };

    Ok(ParsedCommand { command, json })
}

fn parse_recommend_command(args: &[String]) -> Result<Commands, String> {
    if args.len() < 2 {
        return Err("recommend requires concept id".to_string());
    }

    let concept_id = args[1].clone();
    let mut top_k = 3usize;

    let mut i = 2usize;
    while i < args.len() {
        match args[i].as_str() {
            "--top" => {
                if i + 1 >= args.len() {
                    return Err("--top requires a number".to_string());
                }
                top_k = args[i + 1]
                    .parse::<usize>()
                    .map_err(|_| "--top must be a positive integer".to_string())?;
                i += 2;
            }
            unknown => return Err(format!("unknown option for recommend: {unknown}")),
        }
    }

    Ok(Commands::Recommend { concept_id, top_k })
}

fn parse_decide_command(args: &[String]) -> Result<Commands, String> {
    if args.len() < 2 {
        return Err("decide requires at least 1 concept id".to_string());
    }

    let mut concept_ids = Vec::new();
    let mut weights = VmDecisionWeights::default();

    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--weights" => {
                i += 1;
                while i < args.len() {
                    if args[i].starts_with("--") {
                        break;
                    }
                    let (k, v) = args[i]
                        .split_once('=')
                        .ok_or_else(|| format!("invalid weight: {}", args[i]))?;
                    let value = v
                        .parse::<f32>()
                        .map_err(|_| format!("invalid weight value: {v}"))?;
                    match k {
                        "coherence" => weights.coherence = value,
                        "stability" => weights.stability = value,
                        "conflict" => weights.conflict = value,
                        "tradeoff" => weights.tradeoff = value,
                        _ => return Err(format!("unknown weight key: {k}")),
                    }
                    i += 1;
                }
                continue;
            }
            value if value.starts_with("--") => {
                return Err(format!("unknown option for decide: {value}"));
            }
            value => concept_ids.push(value.to_string()),
        }
        i += 1;
    }

    if concept_ids.is_empty() {
        return Err("decide requires at least 1 concept id".to_string());
    }

    Ok(Commands::Decide {
        concept_ids,
        weights,
    })
}

fn help_text() -> String {
    "Usage: design <command>\n  analyze <text> [--json]\n  explain <ConceptId> [--json]\n  compare <ConceptId> <ConceptId> [--json]\n  multi <ConceptId> <ConceptId> [ConceptId ...] [--json]\n  report <ConceptId> [ConceptId ...] [--json]\n  recommend <ConceptId> [--top N] [--json]
  decide <ConceptId> [ConceptId ...] [--weights coherence=0.4 stability=0.3 conflict=0.2 tradeoff=0.1] [--json]".to_string()
}

fn cli_store_dir() -> PathBuf {
    match std::env::var("DESIGN_STORE_DIR") {
        Ok(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => PathBuf::from(".design_store"),
    }
}

fn parse_concept_id(raw: &str) -> Result<ConceptId, String> {
    let trimmed = raw.trim();
    let numeric = trimmed
        .strip_prefix('C')
        .or_else(|| trimmed.strip_prefix('c'))
        .unwrap_or(trimmed);
    let parsed = numeric
        .parse::<u64>()
        .map_err(|_| format!("invalid concept id: {raw}"))?;
    Ok(ConceptId(parsed))
}

fn dedup_parsed_ids(raw_ids: &[String]) -> Result<Vec<ConceptId>, String> {
    let mut set = BTreeSet::new();
    for raw in raw_ids {
        set.insert(parse_concept_id(raw)?);
    }
    Ok(set.into_iter().collect())
}

fn concept_id_string(id: ConceptId) -> String {
    format!("C{}", id.0)
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

fn coherence_phrase(r_mean: f32) -> &'static str {
    if r_mean >= 0.60 {
        "globally coherent"
    } else if r_mean >= 0.30 {
        "moderately coherent"
    } else if r_mean >= 0.0 {
        "loosely connected"
    } else {
        "structurally conflicting"
    }
}

fn abstraction_tendency_phrase(a_mean: f32) -> &'static str {
    if a_mean < 0.30 {
        "primarily concrete"
    } else if a_mean < 0.70 {
        "mixed abstraction levels"
    } else {
        "primarily high-level"
    }
}

fn action_label(action: ActionType) -> &'static str {
    match action {
        ActionType::Merge => "Merge",
        ActionType::Refine => "Refine",
        ActionType::ResolveDirectionalConflict => "ResolveDirectionalConflict",
        ActionType::ApplyPattern => "ApplyPattern",
        ActionType::Separate => "Separate",
    }
}

fn escape_json(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn parse_metric(raw: &str) -> Result<f32, String> {
    raw.parse::<f32>()
        .map(round2)
        .map_err(|_| format!("invalid metric value: {raw}"))
}

fn json_num(v: f32) -> String {
    format!("{:.2}", round2(v))
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::{
        Commands, ParsedCommand, dedup_parsed_ids, parse_command, parse_concept_id, parse_metric,
    };

    #[test]
    fn concept_id_parses_prefix() {
        assert_eq!(parse_concept_id("C42").expect("id").0, 42);
        assert_eq!(parse_concept_id("42").expect("id").0, 42);
    }

    #[test]
    fn dedup_ids_works() {
        let ids = dedup_parsed_ids(&["C9".to_string(), "c9".to_string(), "C10".to_string()])
            .expect("ids");
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0].0, 9);
        assert_eq!(ids[1].0, 10);
    }

    #[test]
    fn command_shape_is_stable() {
        let parsed = parse_command(&["analyze".to_string(), "hello".to_string()]).expect("cmd");
        assert_eq!(
            parsed,
            ParsedCommand {
                command: Commands::Analyze {
                    text: "hello".to_string()
                },
                json: false,
            }
        );
    }

    #[test]
    fn json_flag_is_global() {
        let parsed = parse_command(&[
            "recommend".to_string(),
            "C1".to_string(),
            "--top".to_string(),
            "3".to_string(),
            "--json".to_string(),
        ])
        .expect("parsed");

        assert_eq!(
            parsed,
            ParsedCommand {
                command: Commands::Recommend {
                    concept_id: "C1".to_string(),
                    top_k: 3,
                },
                json: true,
            }
        );
    }

    #[test]
    fn metric_rounding_works() {
        assert_eq!(parse_metric("0.416").expect("metric"), 0.42);
    }

    #[test]
    fn decide_command_parses_weights() {
        let parsed = parse_command(&[
            "decide".to_string(),
            "C1".to_string(),
            "C2".to_string(),
            "--weights".to_string(),
            "coherence=0.7".to_string(),
            "stability=0.1".to_string(),
            "conflict=0.1".to_string(),
            "tradeoff=0.1".to_string(),
        ])
        .expect("parsed");

        match parsed.command {
            Commands::Decide {
                concept_ids,
                weights,
            } => {
                assert_eq!(concept_ids, vec!["C1".to_string(), "C2".to_string()]);
                assert_eq!(weights.coherence, 0.7);
                assert_eq!(weights.stability, 0.1);
                assert_eq!(weights.conflict, 0.1);
                assert_eq!(weights.tradeoff, 0.1);
            }
            _ => panic!("unexpected command"),
        }
    }
}
