use crate::nl::language_detection::detect_language;
use crate::nl::types::SupportedLanguage;

pub fn detect_runtime_language(input: &str) -> SupportedLanguage {
    let detected = detect_language(input);
    if detected.mixed_language && contains_japanese_script(input) {
        SupportedLanguage::Japanese
    } else {
        detected.detected_language
    }
}

pub fn language_label(language: SupportedLanguage) -> &'static str {
    match language {
        SupportedLanguage::Japanese => "JA",
        SupportedLanguage::English => "EN",
        SupportedLanguage::Unknown => "UNKNOWN",
    }
}

fn contains_japanese_script(input: &str) -> bool {
    input.chars().any(|ch| {
        matches!(
            ch,
            '\u{3040}'..='\u{309F}'
                | '\u{30A0}'..='\u{30FF}'
                | '\u{31F0}'..='\u{31FF}'
                | '\u{3400}'..='\u{4DBF}'
                | '\u{4E00}'..='\u{9FFF}'
                | '\u{F900}'..='\u{FAFF}'
        )
    })
}
