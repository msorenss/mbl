//! FFmpeg `libavfilter` path escaper and filter-graph structural validator.

use mbl_core::{Error, Result};

/// Characters that hold syntactic meaning in FFmpeg's `libavfilter` graph and option parsers.
const SPECIAL_CHARS: &[char] = &['\\', ':', '\'', '[', ']', ',', ';'];

/// Escape a filesystem path so it can be embedded safely as a filter option value
/// (for example `subtitles=filename=<escaped>:si=0`) without breaking the filter graph.
///
/// Handles `\ : ' [ ] , ;` and Windows drive letters (`C:\...`). Rejects NUL and newline
/// characters that cannot appear in valid media paths.
pub fn escape_filter_path(path: &str) -> Result<String> {
    if path.is_empty() {
        return Err(Error::Validation("filter path must not be empty".into()));
    }
    if path.chars().any(|c| matches!(c, '\0' | '\r' | '\n')) {
        return Err(Error::Validation(
            "filter path must not contain NUL or newline characters".into(),
        ));
    }

    let mut out = String::with_capacity(path.len() + 16);
    for ch in path.chars() {
        if SPECIAL_CHARS.contains(&ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    Ok(out)
}

/// Unescape a single filter option value produced by [`escape_filter_path`].
pub fn unescape_filter_value(escaped: &str) -> Result<String> {
    let mut out = String::with_capacity(escaped.len());
    let mut chars = escaped.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let next = chars
                .next()
                .ok_or_else(|| Error::Validation("dangling backslash in filter value".into()))?;
            out.push(next);
        } else {
            out.push(ch);
        }
    }
    Ok(out)
}

/// Split `input` on an unescaped `delimiter` character, preserving backslash escapes in each piece.
pub fn split_unescaped(input: &str, delimiter: char) -> Result<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let next = chars
                .next()
                .ok_or_else(|| Error::Validation("dangling backslash in filter graph".into()))?;
            current.push('\\');
            current.push(next);
        } else if ch == '\'' {
            return Err(Error::Validation("unescaped single quote in filter graph".into()));
        } else if ch == delimiter {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    parts.push(current);
    Ok(parts)
}

/// Validate that `graph` is a well-formed FFmpeg filter graph with no unescaped quotes,
/// no dangling backslashes, balanced link labels `[...]`, and non-empty filters.
pub fn validate_filter_graph(graph: &str) -> Result<()> {
    if graph.is_empty() {
        return Err(Error::Validation("filter graph must not be empty".into()));
    }
    let chains = split_unescaped(graph, ';')?;
    for chain in chains {
        if chain.is_empty() {
            return Err(Error::Validation("empty filter chain in graph".into()));
        }
        let filters = split_unescaped(&chain, ',')?;
        for filter in filters {
            validate_single_filter(&filter)?;
        }
    }
    Ok(())
}

fn validate_single_filter(filter: &str) -> Result<()> {
    if filter.is_empty() {
        return Err(Error::Validation("empty filter in chain".into()));
    }
    let mut in_bracket = false;
    let mut bracket_len = 0usize;
    let mut body = String::new();
    let mut chars = filter.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let next = chars
                .next()
                .ok_or_else(|| Error::Validation("dangling backslash in filter".into()))?;
            if in_bracket {
                return Err(Error::Validation("escaped character inside link label".into()));
            }
            body.push('\\');
            body.push(next);
        } else if ch == '\'' {
            return Err(Error::Validation("unescaped single quote in filter".into()));
        } else if ch == '[' {
            if in_bracket {
                return Err(Error::Validation("nested '[' in filter link label".into()));
            }
            in_bracket = true;
            bracket_len = 0;
        } else if ch == ']' {
            if !in_bracket || bracket_len == 0 {
                return Err(Error::Validation(
                    "unmatched or empty ']' in filter link label".into(),
                ));
            }
            in_bracket = false;
        } else if in_bracket {
            bracket_len += 1;
        } else {
            body.push(ch);
        }
    }
    if in_bracket {
        return Err(Error::Validation("unclosed '[' in filter link label".into()));
    }
    if body.is_empty() {
        return Err(Error::Validation("filter missing name".into()));
    }
    if let Some((_, args)) = body.split_once('=') {
        let opts = split_unescaped(args, ':')?;
        if opts.iter().any(String::is_empty) {
            return Err(Error::Validation("empty filter option in filter args".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn escapes_windows_drive_letters_and_special_chars() {
        let escaped =
            escape_filter_path(r"C:\Movies\Mission: Impossible [2023]\Part 1, 'Cut';Final.mkv")
                .unwrap();
        assert_eq!(
            escaped,
            r"C\:\\Movies\\Mission\: Impossible \[2023\]\\Part 1\, \'Cut\'\;Final.mkv"
        );
        let graph = format!("subtitles=filename={escaped}:si=0,scale=w=1920:h=-2,format=yuv420p");
        validate_filter_graph(&graph).unwrap();
    }

    #[test]
    fn rejects_empty_and_control_characters() {
        assert!(escape_filter_path("").is_err());
        assert!(escape_filter_path("/media/bad\0file.mkv").is_err());
        assert!(escape_filter_path("/media/bad\nfile.mkv").is_err());
        assert!(escape_filter_path("/media/bad\rfile.mkv").is_err());
    }

    proptest! {
        #[test]
        fn escaped_path_never_breaks_filter_graph(
            raw in "[^\\x00\\r\\n]{1,120}"
        ) {
            let escaped = escape_filter_path(&raw).unwrap();
            let graph = format!(
                "[0:v:0]yadif=mode=send_frame:parity=auto:deint=all,subtitles=filename={escaped}:si=2,scale=w=1280:h=-2,format=yuv420p[vout]"
            );
            validate_filter_graph(&graph).unwrap();

            // Verify the subtitles filter options split into exactly `filename=<escaped>` and `si=2`
            // and unescaping recovers `raw`.
            let chains = split_unescaped(&graph, ';').unwrap();
            prop_assert_eq!(chains.len(), 1);
            let filters = split_unescaped(&chains[0], ',').unwrap();
            prop_assert_eq!(filters.len(), 4);
            let sub_filter = &filters[1];
            let (_, sub_args) = sub_filter.split_once('=').unwrap();
            let opts = split_unescaped(sub_args, ':').unwrap();
            prop_assert_eq!(opts.len(), 2);
            let filename_val = opts[0].strip_prefix("filename=").unwrap();
            let recovered = unescape_filter_value(filename_val).unwrap();
            prop_assert_eq!(recovered, raw);
            prop_assert_eq!(&opts[1], "si=2");
        }
    }
}
