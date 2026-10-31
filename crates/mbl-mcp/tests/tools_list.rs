//! Snapshot tests for `tools/list` per scope set (`read`, `read+library`, `admin`).
#![allow(clippy::unwrap_used)]

use mbl_auth::tokens::{Scope, ScopeSet};
use mbl_core::{Actor, LibraryAccess, UserId};
use mbl_mcp::{Caller, MblMcp};

fn dummy_caller(scopes: Vec<Scope>, is_admin: bool) -> Caller {
    Caller {
        actor: Actor {
            user_id: UserId(1),
            is_admin,
            library_access: LibraryAccess::All,
            is_managed: false,
            content_rating_ceiling: None,
        },
        scopes: ScopeSet::new(scopes),
        client: "test".into(),
        token_id: Some(1),
    }
}

#[tokio::test]
async fn snapshot_tools_list_read_scope() {
    let caller = dummy_caller(vec![Scope::Read], false);
    let tools: Vec<_> = MblMcp::registry()
        .into_iter()
        .filter(|t| caller.allows(t.scope))
        .map(|t| t.definition)
        .collect();

    // 30 baseline read tools plus online discovery, inspection and history.
    assert_eq!(tools.len(), 33);
    insta::assert_json_snapshot!("tools_list_read", tools);
}

#[tokio::test]
async fn snapshot_tools_list_read_library_scope() {
    let caller = dummy_caller(vec![Scope::Read, Scope::Library], false);
    let tools: Vec<_> = MblMcp::registry()
        .into_iter()
        .filter(|t| caller.allows(t.scope))
        .map(|t| t.definition)
        .collect();

    // Read (33) plus five library-management tools.
    assert_eq!(tools.len(), 38);
    insta::assert_json_snapshot!("tools_list_read_library", tools);
}

#[tokio::test]
async fn snapshot_tools_list_admin_scope() {
    let caller = dummy_caller(vec![Scope::Admin], true);
    let tools: Vec<_> = MblMcp::registry()
        .into_iter()
        .filter(|t| caller.allows(t.scope))
        .map(|t| t.definition)
        .collect();

    // 75 baseline tools plus online discovery, inspection and history.
    assert_eq!(tools.len(), 78);
    insta::assert_json_snapshot!("tools_list_admin", tools);
}
