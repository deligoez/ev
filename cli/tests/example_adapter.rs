//! The worked adapter in `examples/purchases/` imports, links and brings as its docstring says,
//! so the example an agent copies from never rots.

use assert_cmd::Command;
use std::path::Path;

fn ev(db: &Path, args: &[&str], stdin: Option<&[u8]>) -> serde_json::Value {
    let mut cmd = Command::cargo_bin("ev").unwrap();
    cmd.env_remove("EV_DB")
        .env("EV_CONFIG", db.with_file_name("settings.json"))
        .arg("--db")
        .arg(db)
        .arg("--json")
        .args(args);
    if let Some(s) = stdin {
        cmd.write_stdin(s);
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn the_example_adapter_imports_and_brings_what_it_documents() {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/purchases/example-shop.py");
    let Ok(out) = std::process::Command::new("python3").arg(&script).output() else {
        eprintln!("python3 not found; skipped");
        return;
    };
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ev.db");
    let v = ev(&db, &["buy", "import", "--stdin"], Some(&out.stdout));
    assert_eq!(v["imported"]["new"], 1, "{v}");
    assert_eq!(v["imported"]["skipped"], 1, "the bag is used up: {v}");
    assert_eq!(v["imported"]["attachments"], 3, "{v}");
    assert_eq!(v["imported"]["document_links"], 1, "{v}");
    ev(&db, &["add", "Ev", "--kind", "home"], None);
    ev(
        &db,
        &["add", "Matkap", "--kind", "item", "--in", "Ev"],
        None,
    );
    ev(&db, &["buy", "link", "1", "Matkap"], None);
    let b = ev(&db, &["buy", "bring", "1", "Matkap"], None);
    assert_eq!(
        b["brought_types"],
        serde_json::json!({"link": 1, "coverage": 1, "image": 1}),
        "{b}"
    );
}
