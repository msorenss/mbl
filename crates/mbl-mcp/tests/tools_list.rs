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

    // Verify all tools have Read scope (17 M2 + 2 M4 + 2 M5: search_scenes, get_spoiler_free_recap)
    assert_eq!(tools.len(), 21);
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

    // Read (21) + Library (2: scan_library, refresh_metadata) = 23
    assert_eq!(tools.len(), 23);
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

    // All tools = 42 (21 read + 2 library + 2 playback + 2 hw/opt + 6 users + 4 remote + 5 settings/backup/maintenance)
    assert_eq!(tools.len(), 42);
    insta::assert_json_snapshot!("tools_list_admin", tools);
}
