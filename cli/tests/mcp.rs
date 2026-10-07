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
        (
            "find",
            json!({ "any": ["led", "çekmece"] }),
            json!(["find", "--any", "led", "çekmece"]),
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

#[tokio::test]
async fn text_results_follow_the_language_setting_even_when_it_changes_during_the_session() {
    let (_d, client) = start().await;
    call(
        &client,
        "ev",
        json!({ "args": ["add", "Ev", "--kind", "home"] }),
    )
    .await;
    call(
        &client,
        "ev",
        json!({ "args": ["settings", "language", "tr"] }),
    )
    .await;
    // Through the general tool and through a read tool alike.
    let r = call(&client, "ev", json!({ "args": ["show", "Ev"] })).await;
    assert!(text(&r).contains("tür: ev"), "{}", text(&r));
    let r = call(&client, "show", json!({ "ref": "Ev" })).await;
    assert!(text(&r).contains("tür: ev"), "{}", text(&r));
    call(
        &client,
        "ev",
        json!({ "args": ["settings", "language", "en"] }),
    )
    .await;
    let r = call(&client, "show", json!({ "ref": "Ev" })).await;
    assert!(text(&r).contains("kind: home"), "{}", text(&r));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn a_result_too_long_is_cut_as_text_and_refused_as_json_instead_of_cut_unparseable() {
    let (_d, client) = start().await;
    let mut seed = String::from("{\"name\":\"Ev\",\"kind\":\"home\"}\n");
    for i in 0..1500 {
        seed.push_str(&format!(
            "{{\"name\":\"Kutu numarası {i} ve uzunca bir ad\",\"kind\":\"container\",\"in\":\"Ev\"}}\n"
        ));
    }
    let r = call(
        &client,
        "ev",
        json!({ "args": ["add", "--stdin"], "input": seed }),
    )
    .await;
    assert_ne!(r.is_error, Some(true), "{}", text(&r));
    let r = call(&client, "tree", json!({})).await;
    assert_ne!(r.is_error, Some(true));
    assert!(text(&r).ends_with("a tag or a kind)"), "{}", text(&r));
    let r = call(&client, "tree", json!({ "format": "json" })).await;
    assert_eq!(r.is_error, Some(true));
    assert!(r.structured_content.is_none());
    assert!(text(&r).contains("narrow the call"), "{}", text(&r));
    client.cancel().await.unwrap();
}

/// The width and height of a result's image block, decoded.
fn image_size(r: &CallToolResult, at: usize) -> (u32, u32) {
    let img = r.content[at].as_image().expect("an image block");
    assert_eq!(img.mime_type, "image/jpeg");
    let bytes =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &img.data).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    (decoded.width(), decoded.height())
}

#[tokio::test]
async fn a_photo_and_a_numbered_photo_come_back_as_images_the_agent_can_show() {
    let (dir, client) = start().await;
    let file = dir.path().join("drawer.png");
    image::RgbImage::from_pixel(2000, 1000, image::Rgb([120, 120, 120]))
        .save(&file)
        .unwrap();
    let file = file.to_string_lossy().to_string();
    let seed = "{\"name\":\"Ev\",\"kind\":\"home\"}\n{\"name\":\"Çekmece\",\"kind\":\"container\",\"in\":\"Ev\",\"code\":\"D\"}\n";
    call(
        &client,
        "ev",
        json!({ "args": ["add", "--stdin"], "input": seed }),
    )
    .await;
    call(
        &client,
        "ev",
        json!({ "args": ["photo", "add", "D", file] }),
    )
    .await;
    // A node's photo, cut down to 1,568 pixels on its long side.
    let r = call(&client, "photo", json!({ "ref": "D" })).await;
    assert_ne!(r.is_error, Some(true), "{}", text(&r));
    assert!(text(&r).contains("photo 1 of 1"), "{}", text(&r));
    assert_eq!(image_size(&r, 1), (1568, 784));
    let r = call(&client, "photo", json!({ "ref": "D", "n": 2 })).await;
    assert_eq!(r.is_error, Some(true));
    // A numbered photo made by a command comes back after its text.
    let r = call(
        &client,
        "ev",
        json!({ "args": ["photo", "mark", file, "1=0.1,0.1,0.4,0.4"] }),
    )
    .await;
    assert_ne!(r.is_error, Some(true), "{}", text(&r));
    assert_eq!(image_size(&r, 1).0, 1568);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn the_skill_is_a_prompt_and_a_resource_and_the_reference_a_resource() {
    let (_d, client) = start().await;
    let skill = include_str!("../../skills/ev/SKILL.md");
    let reference = include_str!("../../REFERENCE.md");
    let prompts = client.list_all_prompts().await.unwrap();
    assert_eq!(
        prompts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["ev"]
    );
    let got = client
        .get_prompt(rmcp::model::GetPromptRequestParams::new("ev"))
        .await
        .unwrap();
    let rmcp::model::ContentBlock::Text(t) = &got.messages[0].content else {
        panic!("a text message")
    };
    assert_eq!(t.text, skill);
    let resources = client.list_all_resources().await.unwrap();
    assert_eq!(
        resources.iter().map(|r| r.uri.as_str()).collect::<Vec<_>>(),
        ["ev://skill", "ev://reference"]
    );
    for (uri, want) in [("ev://skill", skill), ("ev://reference", reference)] {
        let read = client
            .read_resource(rmcp::model::ReadResourceRequestParams::new(uri))
            .await
            .unwrap();
        let rmcp::model::ResourceContents::TextResourceContents { text, .. } = &read.contents[0]
        else {
            panic!("text contents")
        };
        assert_eq!(text, want, "{uri}");
    }
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn a_batch_file_named_as_standard_input_reads_the_calls_input() {
    let (_d, client) = start().await;
    let r = call(
        &client,
        "ev",
        json!({
            "args": ["add", "--batch", "/dev/stdin"],
            "input": "{\"name\":\"Ev\",\"kind\":\"home\"}\n",
            "format": "json",
        }),
    )
    .await;
    assert_ne!(r.is_error, Some(true), "{r:?}");
    assert_eq!(r.structured_content.unwrap()["created"][0]["name"], "Ev");
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn a_call_with_no_command_is_an_error() {
    let (_d, client) = start().await;
    let r = call(&client, "ev", json!({ "args": [] })).await;
    assert_eq!(r.is_error, Some(true), "{r:?}");
    client.cancel().await.unwrap();
}
