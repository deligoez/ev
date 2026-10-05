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
fn json_is_one_line() {
    let ev = seeded();
    let out = Command::cargo_bin("ev")
        .unwrap()
        .env_remove("EV_DB")
        .env("EV_CONFIG", &ev.config)
        .arg("--db")
        .arg(&ev.db)
        .args(["show", "Salon"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    assert!(!stdout.contains(": "), "{stdout}");
}

#[test]
fn a_mistyped_argument_is_a_json_usage_error_and_help_stays_text() {
    let ev = seeded();
    let (code, out, err) = ev.run(&["show", "Salon", "--no-such-flag"]);
    assert_eq!((code, out), (2, Value::Null));
    let e: Value = serde_json::from_str(err.trim()).unwrap();
    assert_eq!(e["error"]["kind"], "usage");
    assert!(
        e["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("unexpected argument '--no-such-flag'"),
        "{e}"
    );
    let help = Command::cargo_bin("ev")
        .unwrap()
        .args(["show", "--help"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("Usage: ev show")
    );
}

#[test]
fn an_edit_answers_with_what_changed_not_the_whole_record() {
    let ev = seeded();
    ev.ok(&["edit", "Salon", "note=uzun bir not"]);
    let v = ev.ok(&["edit", "Salon", "note=+ikinci satır", "code=SL"]);
    assert_eq!(v["node"]["path_text"], "Ev › SL");
    assert_eq!(v["changed"]["note"]["before"], "uzun bir not");
    assert_eq!(v["changed"]["note"]["after"], "uzun bir not\nikinci satır");
    assert!(v["changed"]["code"].get("before").is_none(), "{v}");
    assert!(
        v["node"].get("note").is_none() && v.get("children").is_none(),
        "{v}"
    );
}

#[test]
fn a_tree_node_says_lost_only_when_it_is() {
    let ev = seeded();
    let home = &ev.ok(&["tree"])["tree"][0];
    assert!(home.get("lost").is_none(), "{home}");
    assert!(home["children"][0].get("lost").is_none(), "{home}");
}

#[test]
fn a_row_names_its_place_once_and_the_node_keeps_its_ancestors() {
    let ev = seeded();
    let row = &ev.ok(&["find", "anten"])["results"][0];
    assert_eq!(row["path_text"], "Ev › Salon › Anten");
    assert!(
        row.get("path").is_none() && row.get("lost").is_none(),
        "{row}"
    );
    let v = ev.ok(&["show", "Salon"]);
    assert_eq!(v["node"]["path"][0]["name"], "Ev");
    assert!(v["children"][0].get("path").is_none(), "{v}");
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
        .env("EV_CONFIG", &ev.config)
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
        .env("EV_CONFIG", &ev.config)
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
    let v = ev.ok(&[
        "photo",
        "mark",
        "D",
        "1=A1",
        "2 → B1=B1-C1",
        "--keep-numbers",
    ]);
    assert_eq!(v["marks"][1]["label"], "2 → B1");
    assert!(std::path::Path::new(v["marked"].as_str().unwrap()).is_file());
    // A cell on a plain file, or a cell outside the grid, is a usage error.
    let (code, _, _) = ev.run(&["photo", "mark", p, "1=A1"]);
    assert_eq!(code, 2);
    let (code, _, _) = ev.run(&["photo", "mark", "D", "1=D9"]);
    assert_eq!(code, 2);
}

#[test]
fn a_cut_numbers_what_it_recognised_and_show_sends_it_to_ev_ui() {
    let (ev, photo) = drawer();
    let p = photo.to_str().unwrap();
    // A crop named by hand is 1; the grid's boxes follow.
    let pieces = [
        "photo",
        "cut",
        p,
        "Röle=0.1,0.1,0.2,0.2",
        "--place",
        "D",
        "--grid",
        CORNERS,
    ];
    let v = ev.ok(&[&pieces[..], &["--preview", "bak", "--show"]].concat());
    let legend = v["legend"].as_array().unwrap();
    assert_eq!(legend.len(), 3);
    assert_eq!(legend[0]["n"], 1);
    assert_eq!(legend[0]["ref"]["name"], "Röle");
    assert_eq!(legend[0]["crop"], "0.1000,0.1000,0.2000,0.2000");
    assert_eq!(legend[1]["ref"]["code"], "D-A1");
    assert_eq!(legend[2]["ref"]["code"], "D-B1");
    // A preview's note shows the preview and the numbered photo together.
    assert_eq!(v["shown"]["note"], "bak");
    assert_eq!(v["shown"]["files"].as_array().unwrap().len(), 2);
    // A cut shows its numbered photo by default; --no-show sends nothing, and the numbered
    // photo is there all the same.
    let v = ev.ok(&pieces);
    assert_eq!(v["shown"]["files"].as_array().unwrap().len(), 1);
    let v = ev.ok(&[&pieces[..], &["--no-show"]].concat());
    assert!(v["shown"].is_null());
    assert!(std::path::Path::new(v["marked"].as_str().unwrap()).is_file());
    assert_eq!(v["legend"][2]["ref"]["code"], "D-B1");
    // --show titles it with --note, else with what each number is. A new series numbers from 1.
    let other = ev._dir.path().join("other.png");
    std::fs::copy(&photo, &other).unwrap();
    let o = other.to_str().unwrap();
    ev.ok(&["focus", "--clear"]);
    let v = ev.ok(&["photo", "cut", o, "Röle=0.5,0.5,0.2,0.2", "--show"]);
    assert_eq!(v["shown"]["note"], "1 Röle");
    let files = v["shown"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(
        std::fs::canonicalize(files[0].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(v["marked"].as_str().unwrap()).unwrap()
    );
    let v = ev.ok(&[
        "photo",
        "cut",
        o,
        "Röle=0,0,0.2,0.2",
        "--note",
        "röleler",
        "--show",
    ]);
    assert_eq!(v["shown"]["note"], "röleler");
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
    assert_eq!(v["part"]["found"], 1);
    assert_eq!(v["part"]["open"], 1);
    let v = ev.ok(&["kit", "show", "Proje seti"]);
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

#[test]
fn a_record_id_is_taken_as_ev_prints_it_with_a_hash_or_bare() {
    let ev = Ev::new();
    ev.ok(&["add", "Ev", "--kind", "home"]);
    let id = ev.ok(&["task", "add", "Say", "--why", "x"])["id"]
        .as_i64()
        .unwrap();
    let v = ev.ok(&["task", "done", &format!("#{id}")]);
    assert_eq!(v["status"], "done");
    let v = ev.ok(&["task", "reopen", &id.to_string()]);
    assert_eq!(v["status"], "open");
    let (code, _, err) = ev.run(&["task", "done", "#x"]);
    assert_eq!(code, 2);
    assert!(err.contains("such as 12 or #12"), "{err}");
}

#[test]
fn a_reader_that_stops_early_is_no_error() {
    let ev = Ev::new();
    let mut lines = String::from("{\"name\":\"Ev\",\"kind\":\"home\"}\n");
    for i in 0..400 {
        lines.push_str(&format!(
            "{{\"name\":\"Kutu {i}\",\"kind\":\"container\",\"in\":\"Ev\"}}\n"
        ));
    }
    Command::cargo_bin("ev")
        .unwrap()
        .env_remove("EV_DB")
        .env("EV_CONFIG", &ev.config)
        .arg("--db")
        .arg(&ev.db)
        .args(["add", "--stdin"])
        .write_stdin(lines)
        .assert()
        .success();
    // More than a pipe holds, read a little, then the pipe is closed, as `| head` does.
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("ev"))
        .env_remove("EV_DB")
        .env("EV_CONFIG", &ev.config)
        .arg("--db")
        .arg(&ev.db)
        .arg("tree")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = [0u8; 16];
    std::io::Read::read_exact(child.stdout.as_mut().unwrap(), &mut first).unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("panicked"), "{err}");
    assert!(out.status.success(), "{err}");
}

#[test]
fn a_thing_used_up_leaves_as_used_and_neither_used_nor_merged_is_set_aside() {
    let ev = Ev::new();
    ev.ok(&["add", "Ev", "--kind", "home"]);
    ev.ok(&["add", "Şerit kaset", "--kind", "item", "--in", "Ev"]);
    ev.ok(&["add", "Kalem", "--kind", "item", "--in", "Ev"]);
    let (code, _, _) = ev.run(&["dispose", "Şerit kaset", "--as", "used"]);
    assert_eq!(code, 2);
    let (code, _, _) = ev.run(&["gone", "Kalem", "--as", "merged"]);
    assert_eq!(code, 2);
    let v = ev.ok(&[
        "gone",
        "Şerit kaset",
        "--as",
        "used",
        "--why",
        "üç etiket bastı",
    ]);
    assert_eq!(v["node"]["state"], "gone");
    assert_eq!(v["node"]["disposition"], "used");
}

#[test]
fn focus_list_reads_the_series_marks_built_and_clear_starts_a_new_one() {
    let (ev, photo) = drawer();
    let p = photo.to_str().unwrap();
    assert!(ev.ok(&["focus", "--list"])["series"].is_null());
    let v = ev.ok(&["photo", "mark", p, "1=0.1,0.1,0.2,0.2"]);
    assert_eq!(v["marks"][0]["label"], "1");
    let other = ev._dir.path().join("other.png");
    std::fs::copy(&photo, &other).unwrap();
    let o = other.to_str().unwrap();
    let v = ev.ok(&["photo", "mark", o, "1=0.1,0.1,0.2,0.2", "2=0.5,0.5,0.2,0.2"]);
    assert_eq!(v["marks"][1]["label"], "3");
    assert_eq!(v["shown"]["series"], 2);
    let s = &ev.ok(&["focus", "--list"])["series"];
    assert_eq!(s["next"], 4);
    assert_eq!(s["pictures"][1]["frames"][1]["n"], 3);
    assert_eq!(s["pictures"][1]["frames"][1]["at"], "0.5,0.5,0.2,0.2");
    // A mark that points at frames already numbered keeps its numbers.
    let v = ev.ok(&["photo", "mark", p, "3=0.2,0.2,0.1,0.1", "--keep-numbers"]);
    assert_eq!(v["marks"][0]["label"], "3");
    ev.ok(&["focus", "--clear"]);
    let v = ev.ok(&["photo", "mark", o, "1=0.1,0.1,0.2,0.2"]);
    assert_eq!(v["marks"][0]["label"], "1");
}

#[test]
fn an_attached_photo_joins_the_series_unframed_unless_no_show() {
    let (ev, photo) = drawer();
    let p = photo.to_str().unwrap();
    let v = ev.ok(&["photo", "add", "D-A1", p, "--note", "son hali"]);
    assert_eq!(v["shown"]["note"], "son hali");
    assert_eq!(v["shown"]["series"], 1);
    let s = &ev.ok(&["focus", "--list"])["series"];
    assert_eq!(s["pictures"][0]["frames"], serde_json::json!([]));
    // Without a note it is titled with the record; --no-show sends nothing.
    let other = ev._dir.path().join("other.png");
    std::fs::copy(&photo, &other).unwrap();
    let v = ev.ok(&["photo", "add", "D-B1", other.to_str().unwrap(), "--whole"]);
    assert_eq!(v["shown"]["note"], "D-B1");
    let v = ev.ok(&["photo", "add", "D-B1", p, "--whole", "--no-show"]);
    assert!(v["shown"].is_null());
}

#[test]
fn pictures_sent_together_each_take_a_note_of_their_own() {
    let (ev, photo) = drawer();
    let other = ev._dir.path().join("other.png");
    std::fs::copy(&photo, &other).unwrap();
    let a = format!("{}=üst çekmece", photo.to_str().unwrap());
    let b = other.to_str().unwrap();
    ev.ok(&["focus", "--file", &a, "--file", b, "--note", "son hali"]);
    let s = &ev.ok(&["focus", "--list"])["series"];
    assert_eq!(s["pictures"][0]["note"], "üst çekmece");
    assert_eq!(s["pictures"][1]["note"], "son hali");
}

#[test]
fn f_names_a_picture_of_the_series_and_its_unmarked_photo() {
    let (ev, photo) = drawer();
    let p = photo.to_str().unwrap();
    ev.ok(&["photo", "mark", p, "1=0.1,0.1,0.2,0.2", "--show", "çekmece"]);
    let s = &ev.ok(&["focus", "--list"])["series"];
    assert_eq!(s["pictures"][0]["f"], "f1");
    let source = std::fs::canonicalize(s["pictures"][0]["source"].as_str().unwrap()).unwrap();
    assert_eq!(source, std::fs::canonicalize(&photo).unwrap());
    // `f1` is that photo, unmarked, to a command that takes one.
    let v = ev.ok(&["photo", "cut", "f1", "Röle=0.1,0.1,0.2,0.2", "--no-show"]);
    assert_eq!(v["legend"][0]["ref"]["name"], "Röle");
    // `ev focus f1` shows the picture again; there is no `f9`.
    let v = ev.ok(&["focus", "f1"]);
    assert_eq!(v["focus"]["note"], "çekmece");
    let (code, _, _) = ev.run(&["focus", "f9"]);
    assert_eq!(code, 3);
}

#[test]
fn a_flag_given_in_edits_key_value_form_is_named_in_the_error() {
    let ev = seeded();
    let (code, _, err) = ev.run(&["add", "Objektif", "make=Optika", "--kind", "item"]);
    assert_eq!(code, 2);
    assert!(err.contains("--make Optika"), "{err}");
    // A key no flag has keeps the usual error.
    let (code, _, err) = ev.run(&["add", "Objektif", "renk=siyah"]);
    assert_eq!(code, 2);
    assert!(err.contains("unexpected argument"), "{err}");
}

#[test]
fn series_pictures_are_attached_by_f_number_several_at_once() {
    let (ev, _photo) = drawer();
    let mut pics = Vec::new();
    for (i, note) in ["kutu önden", "kutu yandan", "pil"].iter().enumerate() {
        let p = ev._dir.path().join(format!("p{i}.png"));
        image::RgbImage::from_pixel(8, 8, image::Rgb([i as u8, 2, 3]))
            .save(&p)
            .unwrap();
        pics.push(format!("{}={note}", p.to_str().unwrap()));
    }
    ev.ok(&[
        "focus", "--file", &pics[0], "--file", &pics[1], "--file", &pics[2],
    ]);
    // Two pictures to one record: each keeps the note it was sent with, none is sent again.
    let v = ev.ok(&["photo", "add", "D-A1", "f1", "f2"]);
    assert_eq!(v["added"].as_array().unwrap().len(), 2, "{v}");
    assert!(v["shown"].is_null());
    let photos = &ev.ok(&["photo", "list", "D-A1"])["photos"];
    assert_eq!(photos[0]["note"], "kutu önden", "{photos}");
    assert_eq!(
        ev.ok(&["focus", "--list"])["series"]["pictures"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // Many records at once from standard input; one wrong record attaches nothing.
    let run = |input: &str| {
        Command::cargo_bin("ev")
            .unwrap()
            .env_remove("EV_DB")
            .env("EV_CONFIG", &ev.config)
            .arg("--db")
            .arg(&ev.db)
            .args(["photo", "add", "--stdin"])
            .write_stdin(input.to_string())
            .output()
            .unwrap()
    };
    let bad = run("{\"ref\":\"D-B1\",\"photo\":\"f3\"}\n{\"ref\":\"Yok\",\"photo\":\"f3\"}\n");
    assert!(!bad.status.success());
    assert!(
        ev.ok(&["photo", "list", "D-B1"])["photos"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let good = run("{\"ref\":\"D-B1\",\"photo\":\"f3\",\"note\":\"pil, üstten\"}\n");
    assert!(good.status.success());
    assert_eq!(
        ev.ok(&["photo", "list", "D-B1"])["photos"][0]["note"],
        "pil, üstten"
    );
}
