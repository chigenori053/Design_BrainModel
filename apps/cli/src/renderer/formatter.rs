pub fn join_sections(sections: &[String]) -> String {
    sections
        .iter()
        .filter(|section| !section.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn format_score(value: f64) -> String {
    format!("{value:.2}")
}

pub fn confidence_label(value: f64) -> &'static str {
    if value >= 0.85 {
        "High"
    } else if value >= 0.60 {
        "Medium"
    } else {
        "Low"
    }
}
