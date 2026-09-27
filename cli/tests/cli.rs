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

