//! Command-line contract of spec §6 and §11.1.

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

struct Ev {
    _dir: TempDir,
    db: std::path::PathBuf,
    config: std::path::PathBuf,
}

impl Ev {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ev.db");
        let config = dir.path().join("settings.json");
        Self {
            _dir: dir,
            db,
            config,
        }
    }

    fn run(&self, args: &[&str]) -> (i32, Value, String) {
        let out = Command::cargo_bin("ev")
            .unwrap()
            .env_remove("EV_DB")
            .env("EV_CONFIG", &self.config)
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
fn text_is_asked_for_through_a_pipe() {
    let ev = seeded();
    let out = Command::cargo_bin("ev")
        .unwrap()
        .env_remove("EV_DB")
        .env("EV_CONFIG", &ev.config)
        .args(["--db"])
        .arg(&ev.db)
        .args(["--text", "show", "Salon"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    // Readable, not JSON: no braces, the name and its path in words.
    assert!(!stdout.trim_start().starts_with('{'), "{stdout}");
    assert!(stdout.contains("Salon"), "{stdout}");
    // --text and --json together is a usage error.
    let (code, _, _) = ev.run(&["--text", "--json", "show", "Salon"]);
    assert_eq!(code, 2);
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
fn several_records_are_edited_from_stdin_all_or_none() {
    let ev = seeded();
    ev.ok(&["edit", "Salon", "note=eski not"]);
    let run = |lines: &str| {
        Command::cargo_bin("ev")
            .unwrap()
            .env_remove("EV_DB")
            .env("EV_CONFIG", &ev.config)
            .args(["--db"])
            .arg(&ev.db)
            .args(["edit", "--stdin"])
            .write_stdin(lines.to_string())
            .output()
            .unwrap()
    };
    let out = run(concat!(
        "{\"ref\":\"Salon\",\"set\":{\"note\":null,\"tags\":[\"+oda\",\"+ana\"]}}\n",
        "\n",
        "{\"ref\":\"Ev\",\"set\":{\"address\":\"Ankara, Çankaya\"}}\n",
    ));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["edited"].as_array().unwrap().len(), 2);
    let salon = ev.ok(&["show", "Salon"]);
    assert!(salon["node"]["note"].is_null());
    assert_eq!(salon["node"]["tags"], serde_json::json!(["ana", "oda"]));
    // Two tags set in one line read as one change in the history.
    let h = ev.ok(&["history", "Salon"]);
    let last = h["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["data"]["tags"]["before"], serde_json::json!([]));
    assert_eq!(
        last["data"]["tags"]["after"],
        serde_json::json!(["ana", "oda"])
    );
    // A bad second line changes nothing, and says which line.
    let out = run(concat!(
        "{\"ref\":\"Ev\",\"set\":{\"address\":\"İzmir\"}}\n",
        "{\"ref\":\"Yok\",\"set\":{\"note\":\"x\"}}\n",
    ));
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("line 2:"));
    assert_eq!(ev.ok(&["show", "Ev"])["node"]["address"], "Ankara, Çankaya");
    // Without --stdin a reference and an assignment are needed.
    assert_eq!(ev.run(&["edit", "Ev"]).0, 2);
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

/// A drawer `D` with a 3×2 grid, two boxes (A1, B1–C1) and a photo of it on disk.
fn drawer() -> (Ev, std::path::PathBuf) {
    let ev = seeded();
    ev.ok(&[
        "add",
        "Çekmece",
        "--kind",
        "container",
        "--in",
        "Salon",
        "--code",
        "D",
    ]);
    ev.ok(&[
        "add",
        "Kutu",
        "--kind",
        "container",
        "--in",
        "D",
        "--code",
        "D-A1",
    ]);
    ev.ok(&[
        "add",
        "Kutu",
        "--kind",
        "container",
        "--in",
        "D",
        "--code",
        "D-B1",
    ]);
    ev.ok(&["add", "Röle", "--kind", "item", "--in", "D-A1"]);
    ev.ok(&["grid", "D", "--cols", "3", "--rows", "2"]);
    ev.ok(&["cell", "D-A1=A1", "D-B1=B1-C1"]);
    let photo = ev._dir.path().join("drawer.png");
    image::RgbImage::from_pixel(300, 200, image::Rgb([200, 200, 200]))
        .save(&photo)
        .unwrap();
    (ev, photo)
}

const CORNERS: &str = "0.1,0.1,0.9,0.1,0.9,0.9,0.1,0.9";

#[test]
fn a_grid_cut_previews_then_cuts_every_box_and_keeps_its_corners() {
    let (ev, photo) = drawer();
    let p = photo.to_str().unwrap();
    // --preview with no note is a flag; with one it also asks ev ui to show it. Either way
    // nothing is attached.
    let v = ev.ok(&[
        "photo",
        "cut",
        p,
        "--place",
        "D",
        "--grid",
        CORNERS,
        "--preview",
    ]);
    assert_eq!(v["framed"], 2);
    assert!(std::path::Path::new(v["preview"].as_str().unwrap()).is_file());
    let v = ev.ok(&[
        "photo",
        "cut",
        p,
        "--place",
        "D",
        "--grid",
        CORNERS,
        "--preview",
        "bak",
    ]);
    assert_eq!(v["shown"]["note"], "bak");
    assert_eq!(
        ev.ok(&["photo", "list", "D"])["photos"],
        serde_json::json!([])
    );
    // The cut: the drawer whole, each box a crop.
    let v = ev.ok(&[
        "photo", "cut", p, "--place", "D", "--grid", CORNERS, "--note", "son",
    ]);
    assert_eq!(v["attached"].as_array().unwrap().len(), 3);
    // The corners were kept, so cells are marked by name.
    let v = ev.ok(&["photo", "mark", "D", "1=A1", "2 → B1=B1-C1"]);
    assert_eq!(v["marks"][1]["label"], "2 → B1");
    assert!(std::path::Path::new(v["marked"].as_str().unwrap()).is_file());
    // A cell on a plain file, or a cell outside the grid, is a usage error.
    let (code, _, _) = ev.run(&["photo", "mark", p, "1=A1"]);
    assert_eq!(code, 2);
    let (code, _, _) = ev.run(&["photo", "mark", "D", "1=D9"]);
    assert_eq!(code, 2);
}

#[test]
fn marked_pictures_go_to_ev_ui_together_with_a_note() {
    let (ev, photo) = drawer();
    let p = photo.to_str().unwrap();
    let a = ev.ok(&["photo", "mark", p, "1 → A1=0.1,0.1,0.2,0.2"])["marked"]
        .as_str()
        .unwrap()
        .to_string();
    let v = ev.ok(&[
        "focus",
        "--file",
        &a,
        "--file",
        p,
        "--note",
        "parçalar → A1",
    ]);
    assert_eq!(v["focus"]["files"].as_array().unwrap().len(), 2);
    assert_eq!(v["focus"]["note"], "parçalar → A1");
    // --show on mark does the same for one picture.
    let v = ev.ok(&["photo", "mark", p, "1=0.1,0.1,0.2,0.2", "--show", "tek"]);
    assert_eq!(v["shown"]["note"], "tek");
    // A note needs a file; a missing file is not found.
    let (code, _, _) = ev.run(&["focus", "--note", "x"]);
    assert_eq!(code, 2);
    let (code, _, _) = ev.run(&["focus", "--file", "/no/such.jpg"]);
    assert_eq!(code, 3);
}

#[test]
fn ids_tags_contents_history_facets_and_observations_from_the_command_line() {
    let (ev, _) = drawer();
    // #id resolves anywhere a reference goes.
    let id = ev.ok(&["show", "D-A1"])["node"]["id"].as_i64().unwrap();
    let v = ev.ok(&["show", &format!("#{id}")]);
    assert_eq!(v["node"]["code"], "D-A1");
    // A tag alone lists everything tagged with it.
    ev.ok(&["edit", "Röle", "tags=+3d yazıcı"]);
    let v = ev.ok(&["find", "--tag", "3d yazıcı"]);
    assert_eq!(v["results"][0]["name"], "Röle");
    let (code, _, _) = ev.run(&["find"]);
    assert_eq!(code, 2);
    // A place's history with what was added in it.
    let v = ev.ok(&["history", "D-A1", "--contents"]);
    let added = v["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["relation"] == "added" && e["item"]["name"] == "Röle");
    assert!(added, "{v}");
    // Facets.
    ev.ok(&["facet", "add", "modül", "--words", "modül, kart"]);
    let v = ev.ok(&["facet", "list"]);
    assert_eq!(v["facets"][0]["name"], "modül");
    ev.ok(&["facet", "remove", "modül"]);
    assert_eq!(ev.ok(&["facet", "list"])["facets"], serde_json::json!([]));
    // An observation, and taking it back, both stay in history.
    let v = ev.ok(&["observe", "D", "röleler masada bekliyor"]);
    let obs = v["observations"][0]["id"].as_i64().unwrap();
    ev.ok(&["unobserve", &obs.to_string()]);
    let types: Vec<String> = ev.ok(&["history", "D"])["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap().to_string())
        .collect();
    assert!(
        types.ends_with(&["observe".to_string(), "unobserve".to_string()]),
        "{types:?}"
    );
}

#[test]
fn a_box_added_after_the_drawer_photo_asks_for_a_new_one() {
    let (ev, photo) = drawer();
    let p = photo.to_str().unwrap();
    ev.ok(&["photo", "cut", p, "--place", "D", "--grid", CORNERS]);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    ev.ok(&[
        "add",
        "Kutu",
        "--kind",
        "container",
        "--in",
        "D",
        "--code",
        "D-A2",
    ]);
    ev.ok(&["add", "Pil", "--kind", "item", "--in", "D-A2"]);
    ev.ok(&["cell", "D-A2=A2"]);
    let v = ev.ok(&["todo"]);
    let codes: Vec<&str> = v["photos"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["code"].as_str())
        .collect();
    assert!(codes.contains(&"D") && codes.contains(&"D-A2"), "{codes:?}");
    // Touring it is refused until the photos are current.
    let (code, _, _) = ev.run(&["review", "D", "--as", "toured"]);
    assert_eq!(code, 5);
}

#[test]
fn a_record_is_split_by_name_and_count_from_the_command_line() {
    let (ev, _) = drawer();
    ev.ok(&["edit", "Röle", "qty=6"]);
    let v = ev.ok(&[
        "split",
        "Röle",
        "Açılı header=3",
        "Yedek",
        "--rename",
        "Düz header",
        "--qty",
        "3",
    ]);
    assert_eq!(v["node"]["name"], "Düz header");
    assert_eq!(v["node"]["qty"], 3);
    assert_eq!(v["into"][0]["name"], "Açılı header");
    assert_eq!(v["into"][0]["qty"], 3);
    // A part without `=` has no count.
    assert_eq!(v["into"][1]["name"], "Yedek");
    assert!(v["into"][1]["qty"].is_null());
    let id = v["into"][0]["id"].to_string();
    let h = ev.ok(&["history", &id]);
    assert!(
        h["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "split_from")
    );
    // A count that is not a number is a usage error.
    let (code, _, _) = ev.run(&["split", "Düz header", "Kablo=üç"]);
    assert_eq!(code, 2);
}

#[test]
fn a_kit_is_recorded_linked_and_counted_from_the_command_line() {
    let (ev, _) = drawer();
    let v = ev.ok(&[
        "kit",
        "add",
        "Proje seti",
        "--copies",
        "2",
        "--part",
        "Röle modülü",
        "--part",
        "Kablo=3",
    ]);
    assert_eq!(v["parts"][1]["expected"], 6);
    ev.ok(&["kit", "part", "Proje seti", "HC-06"]);
    let v = ev.ok(&["kit", "link", "Proje seti", "1", "Röle"]);
    assert_eq!(v["parts"][0]["found"], 1);
    assert_eq!(v["parts"][0]["open"], 1);
    assert_eq!(v["parts"][2]["text"], "HC-06");
    let v = ev.ok(&["kit", "list"]);
    assert_eq!(v["kits"][0]["counts"]["expected"], 10);
    assert_eq!(v["kits"][0]["counts"]["found"], 1);
    let v = ev.ok(&["show", "Röle"]);
    assert_eq!(v["kits"][0]["kit"], "Proje seti");
    let (code, _, _) = ev.run(&["kit", "add", "Başka", "--part", "Kablo=iki"]);
    assert_eq!(code, 2);
}

#[test]
fn rooms_are_placed_beside_each_other_and_from_lines_on_the_command_line() {
    let ev = seeded();
    ev.ok(&["add", "Mutfak", "--kind", "room", "--in", "Ev"]);
    ev.ok(&["add", "Banyo", "--kind", "room", "--in", "Ev"]);
    ev.ok(&["sketch", "Salon", "--at", "0,0", "--size", "400,300"]);
    let v = ev.ok(&[
        "sketch",
        "Mutfak",
        "--size",
        "300,500",
        "--right-of",
        "Salon",
        "--offset",
        "-50",
    ]);
    assert_eq!(
        (v["sketch"]["x"].clone(), v["sketch"]["y"].clone()),
        (400.0.into(), (-50.0).into())
    );
    // One place at a time: beside and at together is a usage error.
    let (code, _, _) = ev.run(&["sketch", "Banyo", "--at", "0,0", "--below", "Salon"]);
    assert_eq!(code, 2);
    // Many from lines, in order.
    let out = Command::cargo_bin("ev")
        .unwrap()
        .env_remove("EV_DB")
        .env("EV_CONFIG", &ev.config)
        .arg("--db")
        .arg(&ev.db)
        .args(["sketch", "--stdin"])
        .write_stdin(
            "{\"ref\": \"Banyo\", \"points\": [[0, 300], [200, 300], [200, 450], [0, 450]]}\n",
        )
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = ev.ok(&["sketch", "Banyo"]);
    assert_eq!(v["sketch"]["y"], 300.0);
    assert_eq!(ev.ok(&["map"])["tiles"].as_array().unwrap().len(), 3);
}

#[test]
fn a_room_is_sketched_and_a_stack_is_mapped_from_the_command_line() {
    let ev = seeded();
    ev.ok(&["add", "Dolap", "--kind", "furniture", "--in", "Salon"]);
    ev.ok(&["add", "Raf", "--kind", "furniture", "--in", "Salon"]);
    for r in ["D-1", "D-2"] {
        ev.ok(&[
            "add",
            "Bölme",
            "--code",
            r,
            "--kind",
            "container",
            "--in",
            "Dolap",
        ]);
    }
    // One grid for several compartments at once.
    let v = ev.ok(&["grid", "D-1", "D-2", "--cols", "1", "--rows", "2"]);
    assert_eq!(v["grids"].as_array().unwrap().len(), 2);
    ev.ok(&["grid", "Dolap", "--cols", "2", "--rows", "1"]);
    ev.ok(&["cell", "D-1=A1", "D-2=B1"]);
    ev.ok(&["sketch", "Salon", "--size", "400,300"]);
    ev.ok(&["sketch", "Dolap", "--at", "0,0", "--size", "120,40"]);
    let v = ev.ok(&["sketch", "Raf", "--on", "Dolap"]);
    assert_eq!(v["sketch"]["on"], v["node"]["id"].as_i64().unwrap() - 1);
    let v = ev.ok(&["sketch", "Dolap"]);
    assert_eq!(v["sketch"]["w"], 120.0);
    // The room: a sketch with the stack under the cupboard's tile.
    let v = ev.ok(&["map", "Salon"]);
    assert_eq!(v["layout"], "sketch");
    assert_eq!(v["tiles"][0]["stacked"][0]["name"], "Raf");
    // The cupboard: the stack front on, the shelf on top.
    let v = ev.ok(&["map", "Dolap"]);
    assert_eq!(v["layout"], "stack");
    assert_eq!(v["bands"][0]["name"], "Raf");
    let (code, _, err) = ev.run(&["sketch", "Dolap", "--on", "Raf"]);
    assert_eq!(code, 5, "{err}");
    let (code, _, _) = ev.run(&["sketch", "Dolap", "--size", "yüz"]);
    assert_eq!(code, 2);
}

#[test]
fn settings_are_shown_changed_and_checked_without_a_database() {
    let ev = Ev::new();
    let (code, v, _) = ev.run(&["settings"]);
    assert_eq!(code, 0);
    assert_eq!(v["language"]["setting"], "auto");
    assert_eq!(v["theme"]["setting"], "auto");
    assert!(!ev.db.exists(), "settings must not create the database");

    let (code, v, _) = ev.run(&["settings", "language", "tr"]);
    assert_eq!(code, 0);
    assert_eq!(v["language"]["setting"], "tr");
    assert_eq!(v["language"]["effective"], "tr");
    let (_, v, _) = ev.run(&["settings", "theme", "light"]);
    assert_eq!(v["theme"]["setting"], "light");
    assert_eq!(v["language"]["setting"], "tr");
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&ev.config).unwrap()).unwrap();
    assert_eq!(saved["language"], "tr");
    assert_eq!(saved["theme"], "light");

    let (code, _, err) = ev.run(&["settings", "language", "klingon"]);
    assert_eq!(code, 2, "{err}");
    let (code, _, _) = ev.run(&["settings", "colour", "red"]);
    assert_eq!(code, 2);

    // Reopening where ev ui was left is on until turned off.
    assert_eq!(ev.ok(&["settings"])["resume"], true);
    assert_eq!(ev.ok(&["settings", "resume", "off"])["resume"], false);
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&ev.config).unwrap()).unwrap();
    assert_eq!(saved["resume"], false);
    let (code, _, _) = ev.run(&["settings", "resume", "maybe"]);
    assert_eq!(code, 2);
}
