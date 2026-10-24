use crate::Candidate;

/// Lowercase, alphanumerics only, single spaces, leading article removed, `&` → `and`.
pub fn normalize_title(s: &str) -> String {
    let lowered = s.to_lowercase().replace('&', " and ");
    let cleaned: String =
        lowered.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let words = match words.first() {
        Some(&("the" | "a" | "an")) if words.len() > 1 => &words[1..],
        _ => &words[..],
    };
    words.join(" ")
}

/// Confidence in `0.0..=1.0` that `candidate` is the item we parsed as `title`/`year`.
pub fn score(title: &str, year: Option<i32>, candidate: &Candidate) -> f64 {
    let want = normalize_title(title);
    let names = std::iter::once(&candidate.title).chain(candidate.original_title.as_ref());
    let similarity =
        names.map(|n| strsim::jaro_winkler(&want, &normalize_title(n))).fold(0.0, f64::max);
    let year_penalty = match (year, candidate.year) {
        (Some(a), Some(b)) if a == b => 0.0,
        (Some(a), Some(b)) if (a - b).abs() == 1 => 0.05,
        (Some(_), Some(_)) => 0.3,
        _ => 0.02,
    };
    (similarity - year_penalty).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(title: &str, year: Option<i32>) -> Candidate {
        Candidate { tmdb_id: 1, title: title.into(), original_title: None, year }
    }

    #[test]
    fn normalizes() {
        assert_eq!(normalize_title("The Matrix"), "matrix");
        assert_eq!(normalize_title("Fast & Furious"), "fast and furious");
        assert_eq!(normalize_title("WALL·E"), "wall e");
        assert_eq!(normalize_title("The"), "the");
    }

    #[test]
    fn scores() {
        assert!(score("Heat", Some(1995), &cand("Heat", Some(1995))) > 0.99);
        assert!(score("Heat", Some(1995), &cand("Heat", Some(1996))) >= 0.9);
        assert!(score("Heat", Some(1995), &cand("Heat", Some(2010))) < 0.85);
        assert!(score("The Matrix", None, &cand("Matrix", Some(1999))) > 0.95);
        assert!(score("Big Test", Some(2020), &cand("Completely Different", Some(2020))) < 0.7);
        let with_original = Candidate {
            tmdb_id: 1,
            title: "Spirited Away".into(),
            original_title: Some("千と千尋の神隠し".into()),
            year: Some(2001),
        };
        assert!(score("千と千尋の神隠し", Some(2001), &with_original) > 0.99);
    }
}
