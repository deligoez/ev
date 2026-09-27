use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Folds text for comparison (spec §5, §11.12): Turkish lowercasing, dotless `ı` to `i`,
/// every combining mark removed, surrounding whitespace trimmed.
pub fn fold(s: &str) -> String {
    let mut lower = String::with_capacity(s.len());
    for c in s.trim().chars() {
        match c {
            // Turkish casing: I -> ı and İ -> i; ı then folds to i.
            'I' | 'İ' | 'ı' => lower.push('i'),
            _ => lower.extend(c.to_lowercase()),
        }
    }
    lower.nfd().filter(|c| !is_combining_mark(*c)).collect()
}

#[cfg(test)]
mod tests {
    use super::fold;

    #[test]
    fn spec_table() {
        for (input, want) in [
            ("Çekmece", "cekmece"),
            ("IŞIK", "isik"),
            ("İğne", "igne"),
            ("Kör", "kor"),
            ("Şarj", "sarj"),
            ("Üst", "ust"),
            ("Hâlâ", "hala"),
            ("ışık", "isik"),
            ("  K4x4-07-Ü ", "k4x4-07-u"),
        ] {
            assert_eq!(fold(input), want, "fold({input:?})");
        }
    }
}
