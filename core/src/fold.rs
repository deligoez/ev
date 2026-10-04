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

/// Folds a code for comparison (spec/codes.md): as `fold`, and `_` is `-`, and a number loses
/// its leading zeros, so `S05_12` and `S5-12` are one code however the label was printed.
pub fn fold_code(s: &str) -> String {
    let folded = fold(s).replace('_', "-");
    let mut out = String::with_capacity(folded.len());
    let mut digits = String::new();
    let flush = |digits: &mut String, out: &mut String| {
        if !digits.is_empty() {
            let trimmed = digits.trim_start_matches('0');
            out.push_str(if trimmed.is_empty() { "0" } else { trimmed });
            digits.clear();
        }
    };
    for c in folded.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            flush(&mut digits, &mut out);
            out.push(c);
        }
    }
    flush(&mut digits, &mut out);
    out
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
