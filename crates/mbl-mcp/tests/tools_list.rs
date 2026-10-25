//! Snapshot tests for `tools/list` per scope set (`read`, `read+library`, `admin`).
#![allow(clippy::unwrap_used)]

use mbl_auth::tokens::{Scope, ScopeSet};
use mbl_core::{Actor, LibraryAccess, UserId};
use mbl_mcp::{Caller, MblMcp};

fn dummy_caller(scopes: Vec<Scope>, is_admin: bool) -> Caller {
    Caller {
        actor: Actor { user_id: UserId(1), is_admin, library_access: LibraryAccess::All },
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

    // Verify all tools have Read scope
    assert_eq!(tools.len(), 17);
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

    // Read (17) + Library (2: scan_library, refresh_metadata) = 19
    assert_eq!(tools.len(), 19);
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

    // All tools = 21 (17 read + 2 library + 2 playback)
    assert_eq!(tools.len(), 21);
    insta::assert_json_snapshot!("tools_list_admin", tools);
}
