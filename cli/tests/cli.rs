//! Command-line contract of spec §6 and §11.1.

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

struct Ev {
    _dir: TempDir,
    db: std::path::PathBuf,
}

impl Ev {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ev.db");
        Self { _dir: dir, db }
    }

    fn run(&self, args: &[&str]) -> (i32, Value, String) {
        let out = Command::cargo_bin("ev")
            .unwrap()
            .env_remove("EV_DB")
            .arg("--db")
            .arg(&self.db)
            .args(args)
            .output()
            .unwrap();
        let stdout = String::from_utf8(out.stdout).unwrap();
        let json = if stdout.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&stdout).unwrap()
        };
        (
            out.status.code().unwrap(),
            json,
            String::from_utf8(out.stderr).unwrap(),
        )
    }

    fn ok(&self, args: &[&str]) -> Value {
        let (code, v, err) = self.run(args);
        assert_eq!(code, 0, "{args:?}: {err}");
        v
    }
}

fn seeded() -> Ev {
    let ev = Ev::new();
    ev.ok(&["add", "Ev", "--kind", "home", "--address", "Ankara"]);
    ev.ok(&["add", "Salon", "--kind", "room", "--in", "Ev"]);
    ev.ok(&["add", "Anten", "--kind", "item", "--in", "Salon"]);
    ev.ok(&["add", "Anten", "--kind", "item", "--in", "Salon"]);
    ev
}

#[test]
fn stdout_is_json_when_piped() {
    let ev = seeded();
    let v = ev.ok(&["find", "anten"]);
    assert_eq!(v["results"].as_array().unwrap().len(), 2);
    assert_eq!(v["results"][0]["path_text"], "Ev › Salon › Anten");
}

#[test]
fn errors_go_to_stderr_with_fixed_codes() {
    let ev = seeded();

    let (code, out, err) = ev.run(&["show", "drone"]);
    assert_eq!((code, out), (3, Value::Null));
    let e: Value = serde_json::from_str(err.trim()).unwrap();
    assert_eq!(e["error"]["kind"], "not_found");

    let (code, _, err) = ev.run(&["show", "anten"]);
    assert_eq!(code, 4);
    let e: Value = serde_json::from_str(err.trim()).unwrap();
    assert_eq!(e["error"]["candidates"].as_array().unwrap().len(), 2);

    let (code, _, _) = ev.run(&[
        "add",
        "Kutu",
        "--kind",
        "container",
        "--in",
        "Salon",
        "--code",
        "12",
    ]);
    assert_eq!(code, 5);

    let (code, _, _) = ev.run(&["add", "Kutu", "--kind", "box", "--in", "Salon"]);
    assert_eq!(code, 2);

    let (code, _, _) = ev.run(&[
        "add",
        "Kutu",
        "--kind",
        "container",
        "--in",
        "Salon",
        "--fill",
        "101",
    ]);
    assert_eq!(code, 2);
}

#[test]
fn batch_from_stdin_with_keys() {
    let ev = seeded();
    let lines = concat!(
        "{\"key\":\"s\",\"name\":\"Ses ve kablo\",\"kind\":\"container\",\"in\":\"Salon\",\"code\":\"S5-01\",\"fill\":20}\n",
        "{\"name\":\"Belkin çoklayıcı\",\"kind\":\"item\",\"in\":\"@s\"}\n",
        "{\"name\":\"Ses adaptörü\",\"kind\":\"item\",\"in\":\"@s\",\"qty\":3}\n",
    );
    let out = Command::cargo_bin("ev")
        .unwrap()
        .env_remove("EV_DB")
        .args(["--db"])
        .arg(&ev.db)
        .args(["add", "--stdin"])
        .write_stdin(lines)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = ev.ok(&["show", "s5-01"]);
    assert_eq!(v["children"].as_array().unwrap().len(), 2);
    assert_eq!(v["node"]["fill"], 20);
}

#[test]
fn db_flag_wins_over_env() {
    let ev = seeded();
    let other = tempfile::tempdir().unwrap();
    let out = Command::cargo_bin("ev")
        .unwrap()
        .env("EV_DB", other.path().join("other.db"))
        .arg("--db")
        .arg(&ev.db)
        .args(["find", "anten"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["results"].as_array().unwrap().len(), 2);
}
