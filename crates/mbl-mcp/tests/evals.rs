//! Automated evaluation scenario runner for Model Context Protocol (MCP) tools (M10.8).
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

use mbl_mcp::MblMcp;

#[test]
fn run_mcp_eval_scenarios() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Try both ../../evals/mcp and ../evals/mcp
    let evals_dir = if manifest_dir.join("../../evals/mcp").exists() {
        manifest_dir.join("../../evals/mcp")
    } else {
        manifest_dir.join("../evals/mcp")
    };

    assert!(evals_dir.exists(), "evals directory does not exist: {:?}", evals_dir);

    let registry = MblMcp::registry();
    let tool_names: Vec<String> = registry.iter().map(|t| t.definition.name.clone()).collect();

    let mut entries: Vec<_> = fs::read_dir(&evals_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "yaml" || ext == "yml"))
        .collect();
    entries.sort_by_key(|e| e.path());

    assert!(!entries.is_empty(), "no eval scenarios found in {:?}", evals_dir);

    for entry in entries {
        let content = fs::read_to_string(entry.path()).unwrap();
        let filename = entry.file_name().to_string_lossy().to_string();

        let mut scenario_name = String::new();
        let mut expected_tools = Vec::new();
        let mut guard_tool = None;

        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(name) = trimmed.strip_prefix("name: ") {
                scenario_name = name.trim_matches('"').to_string();
            } else if trimmed.starts_with("- tool: ") {
                let tool = trimmed.strip_prefix("- tool: ").unwrap().trim_matches('"');
                expected_tools.push(tool.to_string());
            } else if trimmed.starts_with("tool: ") {
                let tool = trimmed.strip_prefix("tool: ").unwrap().trim_matches('"');
                guard_tool = Some(tool.to_string());
            }
        }

        println!("Running MCP Eval: {scenario_name} ({filename})");

        // 1. Verify expected tool calls map to existing registered tools
        for tool in &expected_tools {
            assert!(
                tool_names.contains(tool),
                "Scenario '{scenario_name}' expects tool '{tool}' which is not in MblMcp registry"
            );
        }

        // 2. Verify destructive / guard tool annotations
        if let Some(tool_name) = &guard_tool {
            let tool = registry
                .iter()
                .find(|t| &t.definition.name == tool_name)
                .unwrap_or_else(|| panic!("guard tool '{tool_name}' not found"));

            let annotations = tool
                .definition
                .annotations
                .as_ref()
                .expect("destructive tool must have annotations");
            assert_eq!(
                annotations.destructive_hint,
                Some(true),
                "Tool '{tool_name}' in safety guard must have destructive_hint = true"
            );
        }
    }
}
