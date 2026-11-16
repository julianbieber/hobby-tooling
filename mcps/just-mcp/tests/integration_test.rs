use hobby_just_mcp::JustMcpServer;
use hobby_just_mcp::config::Config;
use rmcp::{ServiceExt, model::CallToolRequestParam, transport::IntoTransport};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Create a connected in-memory server/client pair (no real stdio).
async fn create_server_client() -> (
    rmcp::service::RunningService<
        rmcp::service::RoleServer,
        impl rmcp::service::Service<rmcp::service::RoleServer>,
    >,
    rmcp::service::RunningService<rmcp::service::RoleClient, ()>,
) {
    let (client_side, server_side) = tokio::io::duplex(8192);
    let (server_read, server_write) = tokio::io::split(server_side);
    let (client_read, client_write) = tokio::io::split(client_side);

    let server = JustMcpServer::new(Config::default());
    let (server_result, client_result) = tokio::join!(
        server.serve((server_read, server_write).into_transport()),
        ().serve((client_read, client_write).into_transport())
    );

    (
        server_result.expect("server should start"),
        client_result.expect("client should connect"),
    )
}

/// Write a fixture project tree under a unique temp directory and return its
/// root. The tree has a top-level justfile plus a nested submodule.
fn write_fixture(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("just_mcp_test_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("frontend")).unwrap();

    std::fs::write(
        root.join("justfile"),
        "mod frontend\n\
         \n\
         # Greet someone\n\
         greet name greeting=\"hello\":\n    \
             @echo \"{{greeting}} {{name}}\"\n\
         \n\
         # Sum some numbers\n\
         sum +nums:\n    \
             @echo \"{{nums}}\"\n\
         \n\
         _private:\n    \
             @echo secret\n",
    )
    .unwrap();

    std::fs::write(
        root.join("frontend/mod.just"),
        "# Build the frontend\n\
         build target=\"dev\":\n    \
             @echo \"build {{target}}\"\n",
    )
    .unwrap();

    root
}

fn tool_text(result: &rmcp::model::CallToolResult) -> String {
    match &result.content.first().expect("content").raw {
        rmcp::model::RawContent::Text(t) => t.text.clone(),
        other => panic!("expected text content, got {other:?}"),
    }
}

#[tokio::test]
async fn lists_four_tools() {
    let (_server, client) = create_server_client().await;
    let tools = client.list_tools(None).await.expect("list tools");
    let names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_ref()).collect();
    assert_eq!(tools.tools.len(), 4, "expected 4 tools, got {names:?}");
    for expected in ["just_projects", "just_recipes", "just_show", "just_run"] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
    client.cancel().await.ok();
}

#[tokio::test]
async fn discovers_projects() {
    let root = write_fixture("discover");
    let (_server, client) = create_server_client().await;

    let result = client
        .call_tool(CallToolRequestParam {
            name: "just_projects".into(),
            arguments: Some(
                json!({ "root": root.to_str().unwrap() })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        })
        .await
        .expect("call just_projects");

    assert!(!result.is_error.unwrap_or(false), "tool errored");
    let parsed: Value = serde_json::from_str(&tool_text(&result)).unwrap();
    assert_eq!(parsed["count"], 1);
    // 2 top-level (greet, sum) + 1 module recipe (frontend::build); _private counts too.
    assert_eq!(parsed["projects"][0]["recipe_count"], 4);
    assert_eq!(parsed["projects"][0]["default_recipe"], "greet");

    let _ = std::fs::remove_dir_all(&root);
    client.cancel().await.ok();
}

#[tokio::test]
async fn lists_recipes_with_params_and_hides_private() {
    let root = write_fixture("recipes");
    let (_server, client) = create_server_client().await;

    let result = client
        .call_tool(CallToolRequestParam {
            name: "just_recipes".into(),
            arguments: Some(
                json!({ "justfile": root.to_str().unwrap() })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        })
        .await
        .expect("call just_recipes");

    let parsed: Value = serde_json::from_str(&tool_text(&result)).unwrap();
    let recipes = parsed["recipes"].as_array().unwrap();
    let names: Vec<&str> = recipes
        .iter()
        .map(|r| r["namepath"].as_str().unwrap())
        .collect();

    assert!(names.contains(&"greet"));
    assert!(names.contains(&"sum"));
    assert!(names.contains(&"frontend::build"), "module recipe missing");
    assert!(
        !names.contains(&"_private"),
        "private recipe should be hidden"
    );

    let greet = recipes.iter().find(|r| r["name"] == "greet").unwrap();
    let params = greet["parameters"].as_array().unwrap();
    assert_eq!(params[0]["name"], "name");
    assert_eq!(params[0]["required"], true);
    assert_eq!(params[1]["name"], "greeting");
    assert_eq!(params[1]["required"], false);
    assert_eq!(params[1]["default"], "hello");

    let sum = recipes.iter().find(|r| r["name"] == "sum").unwrap();
    assert_eq!(sum["parameters"][0]["variadic"], true);

    let _ = std::fs::remove_dir_all(&root);
    client.cancel().await.ok();
}

#[tokio::test]
async fn runs_recipe_with_named_args() {
    let root = write_fixture("run_named");
    let (_server, client) = create_server_client().await;

    let result = client
        .call_tool(CallToolRequestParam {
            name: "just_run".into(),
            arguments: Some(
                json!({
                    "justfile": root.to_str().unwrap(),
                    "recipe": "greet",
                    "args": { "name": "world", "greeting": "hi" }
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        })
        .await
        .expect("call just_run");

    let parsed: Value = serde_json::from_str(&tool_text(&result)).unwrap();
    assert_eq!(parsed["exit_code"], 0, "run failed: {parsed}");
    assert_eq!(parsed["stdout"].as_str().unwrap().trim(), "hi world");

    let _ = std::fs::remove_dir_all(&root);
    client.cancel().await.ok();
}

#[tokio::test]
async fn runs_variadic_recipe_and_dry_run() {
    let root = write_fixture("run_variadic");
    let (_server, client) = create_server_client().await;

    // Variadic args as an array.
    let run = client
        .call_tool(CallToolRequestParam {
            name: "just_run".into(),
            arguments: Some(
                json!({
                    "justfile": root.to_str().unwrap(),
                    "recipe": "sum",
                    "args": { "nums": ["1", "2", "3"] }
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        })
        .await
        .expect("call just_run variadic");
    let parsed: Value = serde_json::from_str(&tool_text(&run)).unwrap();
    assert_eq!(parsed["stdout"].as_str().unwrap().trim(), "1 2 3");
    let cmd = parsed["command"].as_array().unwrap();
    let cmd: Vec<&str> = cmd.iter().map(|c| c.as_str().unwrap()).collect();
    assert!(cmd.ends_with(&["sum", "1", "2", "3"]), "argv was {cmd:?}");

    // Dry run should not execute (no stdout from echo) but still succeed.
    let dry = client
        .call_tool(CallToolRequestParam {
            name: "just_run".into(),
            arguments: Some(
                json!({
                    "justfile": root.to_str().unwrap(),
                    "recipe": "greet",
                    "positional": ["there"],
                    "dry_run": true
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        })
        .await
        .expect("call just_run dry");
    let parsed: Value = serde_json::from_str(&tool_text(&dry)).unwrap();
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["exit_code"], 0);

    let _ = std::fs::remove_dir_all(&root);
    client.cancel().await.ok();
}

#[tokio::test]
async fn missing_required_arg_errors() {
    let root = write_fixture("run_missing");
    let (_server, client) = create_server_client().await;

    let result = client
        .call_tool(CallToolRequestParam {
            name: "just_run".into(),
            arguments: Some(
                json!({
                    "justfile": root.to_str().unwrap(),
                    "recipe": "greet",
                    "args": { "greeting": "hi" }
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        })
        .await
        .expect("call just_run");

    assert!(
        result.is_error.unwrap_or(false),
        "expected a tool error for missing required param"
    );
    assert!(tool_text(&result).contains("missing required parameter"));

    let _ = std::fs::remove_dir_all(&root);
    client.cancel().await.ok();
}

#[tokio::test]
async fn shows_recipe_source() {
    let root = write_fixture("show");
    let (_server, client) = create_server_client().await;

    let result = client
        .call_tool(CallToolRequestParam {
            name: "just_show".into(),
            arguments: Some(
                json!({ "justfile": root.to_str().unwrap(), "recipe": "greet" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        })
        .await
        .expect("call just_show");

    let parsed: Value = serde_json::from_str(&tool_text(&result)).unwrap();
    assert!(parsed["source"].as_str().unwrap().contains("greet name"));

    let _ = std::fs::remove_dir_all(&root);
    client.cancel().await.ok();
}
