use crate::nl::types::SupportedLanguage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LanguageDetectionResult {
    pub detected_language: SupportedLanguage,
    pub mixed_language: bool,
}

pub fn detect_language(input: &str) -> LanguageDetectionResult {
    let mut japanese_units = 0usize;
    let mut english_units = 0usize;

    for ch in input.chars() {
        if is_japanese_script(ch) {
            japanese_units += 1;
        } else if ch.is_ascii_alphabetic() {
            english_units += 1;
        }
    }

    let mixed_language = japanese_units > 0 && english_units > 0;
    let detected_language = match (japanese_units, english_units) {
        (0, 0) => SupportedLanguage::Unknown,
        (0, _english) => SupportedLanguage::English,
        (_japanese, 0) => SupportedLanguage::Japanese,
        (japanese, english) if japanese >= english => SupportedLanguage::Japanese,
        _ => SupportedLanguage::English,
    };

    LanguageDetectionResult {
        detected_language,
        mixed_language,
    }
}

fn is_japanese_script(ch: char) -> bool {
    matches!(
        ch,
        '\u{3040}'..='\u{309F}'
            | '\u{30A0}'..='\u{30FF}'
            | '\u{31F0}'..='\u{31FF}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_japanese_input() {
        let detected = detect_language("修正して検証して");
        assert_eq!(detected.detected_language, SupportedLanguage::Japanese);
        assert!(!detected.mixed_language);
    }

    #[test]
    fn detects_english_input() {
        let detected = detect_language("fix planner routing and validate");
        assert_eq!(detected.detected_language, SupportedLanguage::English);
        assert!(!detected.mixed_language);
    }

    #[test]
    fn detects_mixed_language_input() {
        let detected = detect_language("planner routing を fix して");
        assert!(detected.mixed_language);
        assert_eq!(detected.detected_language, SupportedLanguage::English);
    }
}
