use std::collections::BTreeSet;
use std::env;
use std::path::PathBuf;

use hybrid_vm::{ActionType, ConceptId, HybridVM};

#[derive(Debug, PartialEq, Eq)]
enum Commands {
    Analyze { text: String },
    Explain { concept_id: String },
    Compare { left_id: String, right_id: String },
    Multi { concept_ids: Vec<String> },
    Recommend { concept_id: String, top_k: usize },
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

fn run(command: Commands) -> Result<String, String> {
    let mut vm = HybridVM::for_cli_storage(cli_store_dir()).map_err(|e| e.to_string())?;

    match command {
        Commands::Analyze { text } => {
            let concept = vm.analyze_text(&text).map_err(|e| e.to_string())?;
            let level = abstraction_phrase(concept.a);
            Ok(format!(
                "[Concept Created]\nID: C{}\nAbstraction: {:.2} ({level})",
                concept.id.0,
                round2(concept.a),
            ))
        }
        Commands::Explain { concept_id } => {
            let id = parse_concept_id(&concept_id)?;
            let Some(concept) = vm.get_concept(id) else {
                return Err("Concept not found".to_string());
            };
            let explanation = HybridVM::recomposer().explain_concept(&concept);
            Ok(format!(
                "Summary:\n{}\n\nReasoning:\n{}\n\nAbstraction:\n{}",
                explanation.summary, explanation.reasoning, explanation.abstraction_note,
            ))
        }
        Commands::Compare { left_id, right_id } => {
            let left = parse_concept_id(&left_id)?;
            let right = parse_concept_id(&right_id)?;
            let report = vm.compare(left, right).map_err(|e| e.to_string())?;
            let explanation = HybridVM::recomposer().explain_resonance(&report);
            Ok(format!(
                "Summary:\n{}\n\nReasoning:\n{}\n\nAbstraction:\n{}",
                explanation.summary, explanation.reasoning, explanation.abstraction_note,
            ))
        }
        Commands::Multi { concept_ids } => {
            let ids = dedup_parsed_ids(&concept_ids)?;
            if ids.len() < 2 {
                return Err("multi requires at least 2 unique concept ids".to_string());
            }
            let out = vm.explain_multiple(&ids).map_err(|e| e.to_string())?;
            Ok(format!(
                "Summary:\n{}\n\nStructural Analysis:\n{}\n\nAbstraction Analysis:\n{}\n\nConflict Analysis:\n{}",
                out.summary,
                out.structural_analysis,
                out.abstraction_analysis,
                out.conflict_analysis,
            ))
        }
        Commands::Recommend { concept_id, top_k } => {
            let id = parse_concept_id(&concept_id)?;
            let report = vm.recommend(id, top_k).map_err(|e| e.to_string())?;
            let mut out = String::from("[Recommendations]\n");
            if report.recommendations.is_empty() {
                out.push_str("No candidates available.");
                return Ok(out);
            }
            for (idx, rec) in report.recommendations.iter().enumerate() {
                let line = match rec.action {
                    ActionType::Merge => {
                        format!(
                            "{}. Merge with C{} (R={:.2})",
                            idx + 1,
                            rec.target.0,
                            round2(rec.score)
                        )
                    }
                    ActionType::Refine => format!(
                        "{}. Refine with C{} (R={:.2})",
                        idx + 1,
                        rec.target.0,
                        round2(rec.score)
                    ),
                    ActionType::ApplyPattern => {
                        format!("{}. ApplyPattern from C{}", idx + 1, rec.target.0)
                    }
                    ActionType::Separate => {
                        format!(
                            "{}. Separate from C{} (R={:.2})",
                            idx + 1,
                            rec.target.0,
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
}

fn parse_command(args: &[String]) -> Result<Commands, String> {
    let Some(cmd) = args.first() else {
        return Err(help_text());
    };

    match cmd.as_str() {
        "analyze" => {
            if args.len() < 2 {
                return Err("analyze requires text".to_string());
            }
            Ok(Commands::Analyze {
                text: args[1..].join(" "),
            })
        }
        "explain" => {
            if args.len() != 2 {
                return Err("explain requires one concept id".to_string());
            }
            Ok(Commands::Explain {
                concept_id: args[1].clone(),
            })
        }
        "compare" => {
            if args.len() != 3 {
                return Err("compare requires two concept ids".to_string());
            }
            Ok(Commands::Compare {
                left_id: args[1].clone(),
                right_id: args[2].clone(),
            })
        }
        "multi" => {
            if args.len() < 3 {
                return Err("multi requires at least 2 concept ids".to_string());
            }
            Ok(Commands::Multi {
                concept_ids: args[1..].to_vec(),
            })
        }
        "recommend" => parse_recommend_command(args),
        _ => Err(help_text()),
    }
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

fn help_text() -> String {
    "Usage: design <command>\n  analyze <text>\n  explain <ConceptId>\n  compare <ConceptId> <ConceptId>\n  multi <ConceptId> <ConceptId> [ConceptId ...]\n  recommend <ConceptId> [--top N]".to_string()
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

fn abstraction_phrase(a: f32) -> &'static str {
    if a < 0.30 {
        "concrete design element"
    } else if a < 0.70 {
        "mid-level structural concept"
    } else {
        "high-level architectural abstraction"
    }
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::{Commands, dedup_parsed_ids, parse_command, parse_concept_id};

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
        let cmd = parse_command(&["analyze".to_string(), "hello".to_string()]).expect("cmd");
        match cmd {
            Commands::Analyze { text } => assert_eq!(text, "hello"),
            _ => panic!("unexpected variant"),
        }
    }
}
