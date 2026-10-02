//! `ev mcp` (spec/mcp.md), driven by an MCP client over a child process, as an agent's client
//! would start it.

use rmcp::RoleClient;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use serde_json::{Value, json};
use tempfile::TempDir;

type Client = RunningService<RoleClient, ()>;

/// A server on a fresh inventory, in English whatever the machine's language.
async fn start() -> (TempDir, Client) {
    let dir = tempfile::tempdir().unwrap();
    let (db, config) = (dir.path().join("ev.db"), dir.path().join("settings.json"));
    let cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_ev")).configure(|c| {
        c.arg("mcp").env("EV_DB", &db).env("EV_CONFIG", &config);
    });
    let client = ().serve(TokioChildProcess::new(cmd).unwrap()).await.unwrap();
    let r = call(
        &client,
        "ev",
        json!({ "args": ["settings", "language", "en"] }),
    )
    .await;
    assert_ne!(r.is_error, Some(true), "{r:?}");
    (dir, client)
}

async fn call(client: &Client, tool: &'static str, args: Value) -> CallToolResult {
    let Value::Object(args) = args else {
        panic!("arguments are an object")
    };
    client
        .call_tool(CallToolRequestParams::new(tool).with_arguments(args))
        .await
        .unwrap()
}

/// The text of a result's first block.
fn text(r: &CallToolResult) -> String {
    r.content[0]
        .as_text()
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn the_server_says_what_it_is_in_at_most_2048_characters_and_lists_its_tools() {
    let (_d, client) = start().await;
    let info = client.peer_info().unwrap();
    let instructions = info.instructions.clone().unwrap();
    assert!(
        instructions.chars().count() <= 2048,
        "{}",
        instructions.len()
    );
    assert!(instructions.contains("`next`"), "{instructions}");
    let tools = client.list_all_tools().await.unwrap();
    let ev = tools.iter().find(|t| t.name == "ev").unwrap();
    let hints = ev.annotations.clone().unwrap();
    assert_eq!(hints.read_only_hint, Some(false));
    assert_eq!(hints.destructive_hint, Some(true));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn the_ev_tool_runs_a_command_takes_input_and_says_why_it_refuses() {
    let (_d, client) = start().await;
    // `input` is what `--stdin` reads; the JSON comes back structured too.
    let r = call(
        &client,
        "ev",
        json!({
            "args": ["add", "--stdin"],
            "input": "{\"name\":\"Ev\",\"kind\":\"home\"}\n{\"name\":\"Oda\",\"kind\":\"room\",\"in\":\"Ev\"}\n",
            "format": "json",
        }),
    )
    .await;
    assert_ne!(r.is_error, Some(true), "{r:?}");
    assert_eq!(r.structured_content.unwrap()["created"][1]["name"], "Oda");
    // The readable text by default, with #ids.
    let r = call(&client, "ev", json!({ "args": ["show", "Oda"] })).await;
    assert!(text(&r).contains("#2"), "{}", text(&r));
    // A failed command is a tool error the agent can read.
    let r = call(&client, "ev", json!({ "args": ["show", "Mutfak"] })).await;
    assert_eq!(r.is_error, Some(true));
    assert!(text(&r).contains("no node matches"), "{}", text(&r));
    // What cannot run inside the server is refused with the reason.
    for (args, why) in [
        (json!(["ui"]), "terminal"),
        (json!(["mcp"]), "this server"),
        (json!(["show", "Oda", "--db", "/tmp/x.db"]), "one inventory"),
        (json!(["add", "--stdin"]), "`input`"),
    ] {
        let r = call(&client, "ev", json!({ "args": args })).await;
        assert_eq!(r.is_error, Some(true), "{args}");
        assert!(text(&r).contains(why), "{args}: {}", text(&r));
    }
    // Help comes back as the result.
    let r = call(&client, "ev", json!({ "args": ["find", "--help"] })).await;
    assert_ne!(r.is_error, Some(true));
    assert!(text(&r).contains("Usage: ev find"), "{}", text(&r));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn each_read_tool_answers_as_the_command_it_stands_for() {
    let (_d, client) = start().await;
    let seed = "{\"name\":\"Ev\",\"kind\":\"home\"}\n\
{\"name\":\"Oda\",\"kind\":\"room\",\"in\":\"Ev\"}\n\
{\"name\":\"Çekmece\",\"kind\":\"container\",\"in\":\"Oda\",\"code\":\"D\"}\n\
{\"name\":\"Kırmızı LED 5 mm\",\"kind\":\"item\",\"in\":\"D\",\"tags\":[\"led\"]}\n";
    call(
        &client,
        "ev",
        json!({ "args": ["add", "--stdin"], "input": seed }),
    )
    .await;
    let tools = client.list_all_tools().await.unwrap();
    for (tool, args, same) in [
        ("next", json!({}), json!(["next"])),
        ("todo", json!({}), json!(["todo"])),
        (
            "find",
            json!({ "text": "led", "kind": "item" }),
            json!(["find", "--kind", "item", "led"]),
        ),
        (
            "find",
            json!({ "tag": "led" }),
            json!(["find", "--tag", "led"]),
        ),
        ("show", json!({ "ref": "D" }), json!(["show", "D"])),
        (
            "suggest",
            json!({ "text": "kırmızı led" }),
            json!(["suggest", "kırmızı", "led"]),
        ),
        (
            "suggest",
            json!({ "for": "#4" }),
            json!(["suggest", "--for", "#4"]),
        ),
        (
            "history",
            json!({ "ref": "D", "contents": true }),
            json!(["history", "D", "--contents"]),
        ),
        (
            "tree",
            json!({ "ref": "Oda", "depth": 1 }),
            json!(["tree", "Oda", "--depth", "1"]),
        ),
    ] {
        let hints = tools
            .iter()
            .find(|t| t.name == tool)
            .unwrap()
            .annotations
            .clone()
            .unwrap();
        assert_eq!(hints.read_only_hint, Some(true), "{tool}");
        let got = call(&client, tool, args.clone()).await;
        let want = call(&client, "ev", json!({ "args": same })).await;
        assert_ne!(got.is_error, Some(true), "{tool} {args}: {}", text(&got));
        assert_eq!(text(&got), text(&want), "{tool} {args}");
    }
    client.cancel().await.unwrap();
}
