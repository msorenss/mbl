use serde::{Deserialize, Serialize};

/// Opaque pagination cursor. Its encoding is owned by the repository that issues it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(transparent)]
pub struct Cursor(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Paginated<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<Cursor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

impl<T> Paginated<T> {
    pub fn last_page(items: Vec<T>) -> Self {
        Self { items, next_cursor: None, total: None }
    }

    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Paginated<U> {
        Paginated {
            items: self.items.into_iter().map(f).collect(),
            next_cursor: self.next_cursor,
            total: self.total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_camel_case_and_skips_missing_total() {
        let page =
            Paginated { items: vec![1, 2], next_cursor: Some(Cursor("abc".into())), total: None };
        assert_eq!(serde_json::to_string(&page).unwrap(), r#"{"items":[1,2],"nextCursor":"abc"}"#);
    }

    #[test]
    fn map_keeps_cursor() {
        let page =
            Paginated { items: vec![1, 2], next_cursor: Some(Cursor("c".into())), total: Some(9) };
        let mapped = page.map(|n| n * 10);
        assert_eq!(mapped.items, vec![10, 20]);
        assert_eq!(mapped.next_cursor, Some(Cursor("c".into())));
        assert_eq!(mapped.total, Some(9));
    }
}
