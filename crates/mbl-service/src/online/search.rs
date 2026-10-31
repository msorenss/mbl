//! Provider configuration and relevance signals; ranking is never playback approval.
use super::*;

pub(crate) fn validate_settings(settings: &serde_json::Value) -> Result<()> {
    provider(settings).map(|_| ())
}

pub(super) fn provider(settings: &serde_json::Value) -> Result<(&'static str, Url)> {
    match settings.get("searchProvider").and_then(serde_json::Value::as_str).unwrap_or("mwmbl") {
        "mwmbl" => Ok(("Mwmbl", Url::parse(SEARCH_URL).map_err(Error::internal)?)),
        "searxng" => {
            let raw = settings.get("searxngUrl").and_then(serde_json::Value::as_str).unwrap_or("");
            let mut url = public_url(raw)?;
            if url.scheme() != "https" || url.query().is_some() || raw.contains('#') {
                return Err(Error::Validation(
                    "SearXNG needs a public HTTPS base URL without query or fragment".into(),
                ));
            }
            let path = format!("{}/search", url.path().trim_end_matches('/'));
            url.set_path(&path);
            Ok(("SearXNG", url))
        }
        _ => Err(Error::Validation("Unknown online search provider".into())),
    }
}

pub(super) fn normalized(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| !matches!(c, '\'' | '’'))
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn matches(source: &OnlineSource, query: &str) -> bool {
    let haystack =
        normalized(&format!("{} {}", source.title, source.series_title.as_deref().unwrap_or("")));
    let query = normalized(query);
    !query.is_empty()
        && query
            .split_whitespace()
            .all(|token| haystack.split_whitespace().any(|word| word == token))
}

pub(super) fn rank(source: &OnlineSource, query: &str) -> i32 {
    let text = normalized(&format!(
        "{} {} {}",
        source.title,
        source.url,
        source.summary.as_deref().unwrap_or("")
    ));
    let mut score = if matches(source, query) { 100 } else { 0 };
    score += match source.availability {
        Availability::MediaDetected => 40,
        Availability::Restricted | Availability::Unavailable => -30,
        _ => 0,
    };
    if matches!(source.kind.as_str(), "series" | "season" | "episode" | "movie") {
        score += 20;
    }
    for word in ["watch", "stream", "streaming", "episode", "episodes", "video"] {
        if text.split_whitespace().any(|w| w == word) {
            score += 3;
        }
    }
    for word in ["recap", "review", "reviews", "news", "rumors", "judges", "instagram"] {
        if text.split_whitespace().any(|w| w == word) {
            score -= 8;
        }
    }
    score
}
