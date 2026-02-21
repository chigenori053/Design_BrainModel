use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use hybrid_vm::{
    DesignHypothesis, DesignProjection, HybridVM, L1Id, L1RequirementRole, L2Config, L2Mode,
    MeaningLayerSnapshot, RequirementKind,
};
use semantic_dhm::{L1Snapshot, L2Snapshot, SnapshotDiff, compare_snapshots};

fn main() {
    if let Err(err) = run(std::env::args().skip(1).collect()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err(help_text());
    }
    match args[0].as_str() {
        "l1" => run_l1(&args[1..]),
        "l2" => run_l2(&args[1..]),
        "snapshot" => run_snapshot(&args[1..]),
        "projection" => run_projection(&args[1..]),
        "design" => run_design(&args[1..]),
        _ => Err(help_text()),
    }
}

fn run_l1(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) != Some("inspect") {
        return Err(help_text());
    }
    let text = required_flag_value(args, "--text")?;
    let mut vm = init_vm()?;
    let _ = vm.analyze_text(&text).map_err(|e| e.to_string())?;
    let snapshot = vm.snapshot();
    println!("{}", l1_snapshot_json(&snapshot));
    Ok(())
}

fn run_l2(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("inspect") => {
            let text = required_flag_value(args, "--text")?;
            let threshold = optional_flag_value(args, "--threshold")
                .map(|v| parse_threshold(&v))
                .transpose()?;
            let experimental = has_flag(args, "--experimental");

            let mut vm = init_vm()?;
            let _ = vm.analyze_text(&text).map_err(|e| e.to_string())?;
            if experimental {
                let th = threshold
                    .ok_or("invalid threshold: --threshold is required with --experimental")?;
                vm.rebuild_l2_from_l1_with_mode(L2Mode::Experimental(L2Config {
                    similarity_threshold: th,
                    algorithm_version: 2,
                }))
                .map_err(|e| e.to_string())?;
            } else {
                vm.rebuild_l2_from_l1().map_err(|e| e.to_string())?;
            }
            println!("{}", l2_snapshot_json(&vm.snapshot()));
            Ok(())
        }
        Some("rebuild") => {
            let text = required_flag_value(args, "--text")?;
            let mut vm = init_vm()?;
            let _ = vm.analyze_text(&text).map_err(|e| e.to_string())?;
            let before = vm.snapshot();
            vm.rebuild_l2_from_l1().map_err(|e| e.to_string())?;
            let after = vm.snapshot();
            let diff = compare_snapshots(&before, &after);
            println!("{}", diff_json(&diff));
            Ok(())
        }
        Some("simulate-threshold") => {
            let text = required_flag_value(args, "--text")?;
            let threshold = parse_threshold(&required_flag_value(args, "--threshold")?)?;
            let mut vm = init_vm()?;
            let _ = vm.analyze_text(&text).map_err(|e| e.to_string())?;
            let stable = vm.snapshot();
            vm.rebuild_l2_from_l1_with_config(L2Config {
                similarity_threshold: threshold,
                algorithm_version: stable.algorithm_version.saturating_add(1),
            })
            .map_err(|e| e.to_string())?;
            let experimental = vm.snapshot();
            let diff = compare_snapshots(&stable, &experimental);
            println!("{}", diff_json(&diff));
            Ok(())
        }
        _ => Err(help_text()),
    }
}

fn run_snapshot(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("create") => {
            let text = required_flag_value(args, "--text")?;
            let mut vm = init_vm()?;
            let _ = vm.analyze_text(&text).map_err(|e| e.to_string())?;
            println!("{}", full_snapshot_json(&vm.snapshot()));
            Ok(())
        }
        Some("compare") => {
            if args.len() < 2 {
                return Err("snapshot compare requires snapshot file path".to_string());
            }
            let path = PathBuf::from(&args[1]);
            let text = required_flag_value(args, "--text")?;
            let expected_raw = fs::read_to_string(path)
                .map_err(|e| format!("failed to read snapshot file: {e}"))?;
            let expected = parse_snapshot_json(&expected_raw)?;

            let mut vm = init_vm()?;
            let _ = vm.analyze_text(&text).map_err(|e| e.to_string())?;
            let actual = vm.snapshot();
            let diff = compare_snapshots(&expected, &actual);
            println!("{}", diff_json(&diff));
            if !diff.identical {
                return Err("snapshot mismatch".to_string());
            }
            Ok(())
        }
        _ => Err(help_text()),
    }
}

fn run_projection(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) != Some("phase-a") {
        return Err(help_text());
    }
    let text = required_flag_value(args, "--text")?;
    let mut vm = init_vm()?;
    let _ = vm.analyze_text(&text).map_err(|e| e.to_string())?;
    let projection = vm.project_phase_a();
    println!("{}", projection_json(&projection));
    Ok(())
}

fn run_design(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) != Some("evaluate") {
        return Err(help_text());
    }
    let text = required_flag_value(args, "--text")?;
    let mut vm = init_vm()?;
    let _ = vm.analyze_text(&text).map_err(|e| e.to_string())?;
    let projection = vm.project_phase_a();
    let hypothesis = vm.evaluate_hypothesis(&projection);
    println!("{}", hypothesis_json(&hypothesis));
    Ok(())
}

fn parse_threshold(raw: &str) -> Result<f64, String> {
    let v = raw
        .parse::<f64>()
        .map_err(|_| "invalid threshold".to_string())?;
    if !v.is_finite() || !(-1.0..=1.0).contains(&v) {
        return Err("invalid threshold".to_string());
    }
    Ok(v)
}

fn required_flag_value(args: &[String], flag: &str) -> Result<String, String> {
    optional_flag_value(args, flag).ok_or_else(|| format!("{flag} is required"))
}

fn optional_flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == flag && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
        i += 1;
    }
    None
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn init_vm() -> Result<HybridVM, String> {
    HybridVM::for_cli_storage(cli_store_dir())
        .map_err(|e| format!("failed to initialize HybridVM: {e}"))
}

fn cli_store_dir() -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("design_cli_store_{}_{}", std::process::id(), ts))
}

fn full_snapshot_json(snapshot: &MeaningLayerSnapshot) -> String {
    let mut l1_lines = Vec::new();
    for s in &snapshot.l1 {
        l1_lines.push(format!(
            "        {{\"id\": {}, \"role\": \"{}\", \"polarity\": {}, \"abstraction\": {:.3}, \"vector_hash\": {}}}",
            s.id.0,
            role_to_str(s.role),
            s.polarity,
            quantize3(s.abstraction),
            s.vector_hash
        ));
    }
    let mut l2_lines = Vec::new();
    for s in &snapshot.l2 {
        let refs = s
            .l1_refs
            .iter()
            .map(|id| id.0.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        l2_lines.push(format!(
            "        {{\"id\": {}, \"l1_refs\": [{}], \"integrated_vector_hash\": {}}}",
            s.id.0, refs, s.integrated_vector_hash
        ));
    }
    format!(
        "{{\n    \"algorithm_version\": {},\n    \"l1\": [\n{}\n    ],\n    \"l2\": [\n{}\n    ]\n}}",
        snapshot.algorithm_version,
        l1_lines.join(",\n"),
        l2_lines.join(",\n")
    )
}

fn l1_snapshot_json(snapshot: &MeaningLayerSnapshot) -> String {
    let mut l1_lines = Vec::new();
    for s in &snapshot.l1 {
        l1_lines.push(format!(
            "        {{\"id\": {}, \"role\": \"{}\", \"polarity\": {}, \"abstraction\": {:.3}, \"vector_hash\": {}}}",
            s.id.0,
            role_to_str(s.role),
            s.polarity,
            quantize3(s.abstraction),
            s.vector_hash
        ));
    }
    format!(
        "{{\n    \"algorithm_version\": {},\n    \"l1\": [\n{}\n    ]\n}}",
        snapshot.algorithm_version,
        l1_lines.join(",\n")
    )
}

fn l2_snapshot_json(snapshot: &MeaningLayerSnapshot) -> String {
    let mut l2_lines = Vec::new();
    for s in &snapshot.l2 {
        let refs = s
            .l1_refs
            .iter()
            .map(|id| id.0.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        l2_lines.push(format!(
            "        {{\"id\": {}, \"l1_refs\": [{}], \"integrated_vector_hash\": {}}}",
            s.id.0, refs, s.integrated_vector_hash
        ));
    }
    format!(
        "{{\n    \"algorithm_version\": {},\n    \"l2\": [\n{}\n    ]\n}}",
        snapshot.algorithm_version,
        l2_lines.join(",\n")
    )
}

fn diff_json(diff: &SnapshotDiff) -> String {
    format!(
        "{{\n    \"identical\": {},\n    \"algorithm_version_changed\": {},\n    \"l1_changed\": {},\n    \"l2_changed\": {}\n}}",
        diff.identical, diff.algorithm_version_changed, diff.l1_changed, diff.l2_changed
    )
}

fn projection_json(proj: &DesignProjection) -> String {
    let ids = proj
        .source_l2_ids
        .iter()
        .map(|id| id.0.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let mut derived = proj.derived.clone();
    derived.sort_by(|l, r| format!("{:?}", l.kind).cmp(&format!("{:?}", r.kind)));
    let derived_lines = derived
        .iter()
        .map(|d| {
            format!(
                "        {{\"kind\": \"{:?}\", \"strength\": {:.3}}}",
                d.kind,
                quantize3(d.strength)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");

    format!(
        "{{\n    \"source_l2_ids\": [{}],\n    \"derived\": [\n{}\n    ]\n}}",
        ids, derived_lines
    )
}

fn hypothesis_json(h: &DesignHypothesis) -> String {
    let dominant = h
        .dominant_requirement()
        .map(requirement_kind_label)
        .unwrap_or("None");
    format!(
        "{{\n    \"total_score\": {:.3},\n    \"normalized_score\": {:.3},\n    \"constraint_violation\": {},\n    \"dominant_requirement\": \"{}\"\n}}",
        h.total_score, h.normalized_score, h.constraint_violation, dominant
    )
}

fn requirement_kind_label(kind: RequirementKind) -> &'static str {
    match kind {
        RequirementKind::Performance => "Performance",
        RequirementKind::Memory => "Memory",
        RequirementKind::Security => "Security",
        RequirementKind::NoCloud => "NoCloud",
        RequirementKind::Reliability => "Reliability",
    }
}

fn quantize3(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

fn role_to_str(role: L1RequirementRole) -> &'static str {
    match role {
        L1RequirementRole::Goal => "Goal",
        L1RequirementRole::Constraint => "Constraint",
        L1RequirementRole::Optimization => "Optimization",
        L1RequirementRole::Prohibition => "Prohibition",
    }
}

fn parse_snapshot_json(raw: &str) -> Result<MeaningLayerSnapshot, String> {
    let mut p = JsonParser::new(raw);
    let root = p.parse_value()?;
    let obj = root.as_object()?;
    let algorithm_version = obj
        .get("algorithm_version")
        .ok_or("missing algorithm_version")?
        .as_u32()?;

    let l1 = obj
        .get("l1")
        .ok_or("missing l1")?
        .as_array()?
        .iter()
        .map(|v| {
            let o = v.as_object()?;
            let role = parse_role(o.get("role").ok_or("missing role")?.as_str()?)?;
            Ok(L1Snapshot {
                id: L1Id(o.get("id").ok_or("missing id")?.as_u128()?),
                role,
                polarity: o.get("polarity").ok_or("missing polarity")?.as_i8()?,
                abstraction: o
                    .get("abstraction")
                    .ok_or("missing abstraction")?
                    .as_f32()?,
                vector_hash: o
                    .get("vector_hash")
                    .ok_or("missing vector_hash")?
                    .as_u64()?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let l2 = obj
        .get("l2")
        .ok_or("missing l2")?
        .as_array()?
        .iter()
        .map(|v| {
            let o = v.as_object()?;
            let refs = o
                .get("l1_refs")
                .ok_or("missing l1_refs")?
                .as_array()?
                .iter()
                .map(JsonValue::as_u128)
                .map(|r| r.map(L1Id))
                .collect::<Result<Vec<_>, String>>()?;
            Ok(L2Snapshot {
                id: hybrid_vm::ConceptId(o.get("id").ok_or("missing id")?.as_u64()?),
                l1_refs: refs,
                integrated_vector_hash: o
                    .get("integrated_vector_hash")
                    .ok_or("missing integrated_vector_hash")?
                    .as_u64()?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(MeaningLayerSnapshot {
        algorithm_version,
        l1,
        l2,
    })
}

fn parse_role(raw: &str) -> Result<L1RequirementRole, String> {
    match raw {
        "Goal" => Ok(L1RequirementRole::Goal),
        "Constraint" => Ok(L1RequirementRole::Constraint),
        "Optimization" => Ok(L1RequirementRole::Optimization),
        "Prohibition" => Ok(L1RequirementRole::Prohibition),
        _ => Err("invalid role".to_string()),
    }
}

fn help_text() -> String {
    "Usage:\n  cli l1 inspect --text <TEXT>\n  cli l2 inspect --text <TEXT> [--threshold <T>] [--experimental]\n  cli l2 rebuild --text <TEXT>\n  cli l2 simulate-threshold --text <TEXT> --threshold <T>\n  cli snapshot create --text <TEXT>\n  cli snapshot compare <SNAPSHOT_JSON> --text <TEXT>\n  cli projection phase-a --text <TEXT>\n  cli design evaluate --text <TEXT>".to_string()
}

#[derive(Debug, Clone)]
enum JsonValue {
    Obj(BTreeMap<String, JsonValue>),
    Arr(Vec<JsonValue>),
    Str(String),
    Num(String),
}

impl JsonValue {
    fn as_object(&self) -> Result<&BTreeMap<String, JsonValue>, String> {
        if let Self::Obj(v) = self {
            Ok(v)
        } else {
            Err("expected object".to_string())
        }
    }
    fn as_array(&self) -> Result<&Vec<JsonValue>, String> {
        if let Self::Arr(v) = self {
            Ok(v)
        } else {
            Err("expected array".to_string())
        }
    }
    fn as_str(&self) -> Result<&str, String> {
        if let Self::Str(v) = self {
            Ok(v)
        } else {
            Err("expected string".to_string())
        }
    }
    fn as_u64(&self) -> Result<u64, String> {
        if let Self::Num(v) = self {
            v.parse::<u64>().map_err(|_| "expected u64".to_string())
        } else {
            Err("expected number".to_string())
        }
    }
    fn as_u32(&self) -> Result<u32, String> {
        if let Self::Num(v) = self {
            v.parse::<u32>().map_err(|_| "expected u32".to_string())
        } else {
            Err("expected number".to_string())
        }
    }
    fn as_u128(&self) -> Result<u128, String> {
        if let Self::Num(v) = self {
            v.parse::<u128>().map_err(|_| "expected u128".to_string())
        } else {
            Err("expected number".to_string())
        }
    }
    fn as_i8(&self) -> Result<i8, String> {
        if let Self::Num(v) = self {
            v.parse::<i8>().map_err(|_| "expected i8".to_string())
        } else {
            Err("expected number".to_string())
        }
    }
    fn as_f32(&self) -> Result<f32, String> {
        self.as_num().map(|n| n as f32)
    }
    fn as_num(&self) -> Result<f64, String> {
        if let Self::Num(v) = self {
            v.parse::<f64>().map_err(|_| "expected number".to_string())
        } else {
            Err("expected number".to_string())
        }
    }
}

struct JsonParser<'a> {
    src: &'a [u8],
    i: usize,
}

impl<'a> JsonParser<'a> {
    fn new(raw: &'a str) -> Self {
        Self {
            src: raw.as_bytes(),
            i: 0,
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.ws();
        let Some(&b) = self.src.get(self.i) else {
            return Err("unexpected eof".to_string());
        };
        match b {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' => self.parse_string().map(JsonValue::Str),
            b'-' | b'0'..=b'9' => self.parse_number().map(JsonValue::Num),
            _ => Err("invalid json".to_string()),
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        let mut map = BTreeMap::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(JsonValue::Obj(map));
        }
        loop {
            let key = self.parse_string()?;
            self.ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            map.insert(key, value);
            self.ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b'}') => {
                    self.i += 1;
                    break;
                }
                _ => return Err("invalid object".to_string()),
            }
        }
        Ok(JsonValue::Obj(map))
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect(b'[')?;
        let mut out = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(JsonValue::Arr(out));
        }
        loop {
            out.push(self.parse_value()?);
            self.ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b']') => {
                    self.i += 1;
                    break;
                }
                _ => return Err("invalid array".to_string()),
            }
        }
        Ok(JsonValue::Arr(out))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        while let Some(&b) = self.src.get(self.i) {
            self.i += 1;
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = *self.src.get(self.i).ok_or("invalid escape")?;
                    self.i += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        _ => return Err("unsupported escape".to_string()),
                    }
                }
                _ => out.push(b as char),
            }
        }
        Err("unterminated string".to_string())
    }

    fn parse_number(&mut self) -> Result<String, String> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        let s = std::str::from_utf8(&self.src[start..self.i]).map_err(|_| "invalid num")?;
        if s.is_empty() || s == "-" || s.ends_with('.') {
            return Err("invalid number".to_string());
        }
        Ok(s.to_string())
    }

    fn expect(&mut self, ch: u8) -> Result<(), String> {
        self.ws();
        if self.peek() == Some(ch) {
            self.i += 1;
            Ok(())
        } else {
            Err("unexpected token".to_string())
        }
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.i).copied()
    }
}
