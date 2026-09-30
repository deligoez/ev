//! The readable output (`--text`) the agent reads every day, one test per shape.

use assert_cmd::Command;
use tempfile::TempDir;

/// A drawer `D` in a 2×1 grid: a red LED box and a buzzer box with a stray red LED in it, and
/// a task to count the drawer.
const SEED: &str = r#"{"name":"Ev","kind":"home"}
{"name":"Oda","kind":"room","in":"Ev","key":"o"}
{"name":"Çekmece","kind":"container","in":"@o","code":"D","key":"d"}
{"name":"Kutu","kind":"container","in":"@d","code":"D-A1","theme":"Kırmızı LED"}
{"name":"Kutu","kind":"container","in":"@d","code":"D-B1","theme":"Buzzer"}
{"name":"Kırmızı LED 5 mm","kind":"item","in":"D-A1"}
{"name":"Aktif buzzer","kind":"item","in":"D-B1"}
{"name":"Kırmızı LED 10 mm","kind":"item","in":"D-B1"}
"#;

struct Home {
    dir: TempDir,
}

impl Home {
    /// The seed, in English whatever the machine's language.
    fn new() -> Home {
        let home = Home {
            dir: tempfile::tempdir().unwrap(),
        };
        home.run(&["settings", "language", "en"], None);
        home.run(&["add", "--stdin"], Some(SEED));
        home.run(&["grid", "D", "--cols", "2", "--rows", "1"], None);
        home.run(&["cell", "D-A1=A1", "D-B1=B1"], None);
        home.run(
            &[
                "task",
                "add",
                "Çekmeceyi say",
                "--why",
                "hiç sayılmadı",
                "--on",
                "D",
            ],
            None,
        );
        home
    }

    fn run(&self, args: &[&str], stdin: Option<&str>) -> String {
        let mut cmd = Command::cargo_bin("ev").unwrap();
        cmd.env_remove("EV_DB")
            .env("EV_CONFIG", self.dir.path().join("settings.json"))
            .arg("--db")
            .arg(self.dir.path().join("ev.db"))
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
        String::from_utf8(out.stdout).unwrap()
    }

    /// The readable output of a command. A JSON field the text reads but the command does not
    /// send shows up as `null`, which no shape ever means to print.
    fn text(&self, args: &[&str]) -> String {
        let mut all = vec!["--text"];
        all.extend_from_slice(args);
        let s = self.run(&all, None);
        assert!(!s.contains("null"), "{s}");
        s
    }
}

#[test]
fn show_names_a_box_by_id_and_path_with_its_theme_cells_task_and_contents() {
    let h = Home::new();
    let s = h.text(&["show", "D-B1"]);
    assert!(s.starts_with("#5 Ev › Oda › D › D-B1  [Kutu]\n"), "{s}");
    assert!(s.contains("\n  theme: Buzzer\n"), "{s}");
    assert!(s.contains("\n  cells: B1\n"), "{s}");
    // The task is on the drawer; the box shows it through the drawer.
    assert!(
        s.contains("\n  task 1. #1 Çekmeceyi say  (via #3)\n"),
        "{s}"
    );
    assert!(
        s.contains("\n  └ #8 Ev › Oda › D › D-B1 › Kırmızı LED 10 mm\n"),
        "{s}"
    );
}

#[test]
fn next_says_the_goal_how_far_the_home_is_and_the_task_with_its_places() {
    let h = Home::new();
    let s = h.text(&["next"]);
    assert!(s.starts_with("Goal: (not set)\n"), "{s}");
    assert!(
        s.contains("2 places: 0 counted, 0 left as is, 0 being counted, 2 not counted"),
        "{s}"
    );
    assert!(
        s.contains("Next task (1 open):\n1. #1 Çekmeceyi say\n"),
        "{s}"
    );
    assert!(s.contains("\n  Ev › Oda › D [not counted]\n"), "{s}");
    assert!(
        s.contains("\n    └ #5 Ev › Oda › D › D-B1  [Kutu]\n"),
        "{s}"
    );
    assert!(s.contains("Raw places no task covers (2):"), "{s}");
}

#[test]
fn todo_lists_each_kind_of_waiting_work_under_its_count() {
    let h = Home::new();
    let s = h.text(&["todo"]);
    assert!(s.contains("0 being counted, 2 not counted"), "{s}");
    assert!(s.contains("\nTasks (1)\n  1. #1 Çekmeceyi say\n"), "{s}");
    // Codes set at add time wait for their labels.
    assert!(s.contains("\nLabels to print (3)\n"), "{s}");
    assert!(s.contains("\nNot counted yet (2)\n"), "{s}");
    assert!(s.contains("\n  Ev › Oda › D  (no photo)\n"), "{s}");
}

#[test]
fn suggest_ranks_the_best_holder_first_and_says_what_matched_and_why() {
    let h = Home::new();
    let s = h.text(&["suggest", "--for", "Kırmızı LED 10 mm"]);
    assert!(
        s.starts_with("For: Ev › Oda › D › D-B1 › Kırmızı LED 10 mm\n"),
        "{s}"
    );
    // The stray LED's best place is the LED box, on its theme, with the rare words starred.
    assert!(
        s.contains("\n  1. D-A1  Kutu  (not counted)  score "),
        "{s}"
    );
    assert!(s.contains("matched: led* "), "{s}");
    assert!(
        s.contains("\nAll 4 places that can hold something:\n"),
        "{s}"
    );
    assert!(s.contains("\nScoring: "), "{s}");
}

#[test]
fn regroup_names_the_stray_with_where_it_fits_better_and_the_unknown_fills() {
    let h = Home::new();
    let s = h.text(&["regroup", "D"]);
    assert!(s.starts_with("D  Çekmece\n"), "{s}");
    assert!(
        s.contains("2 of 3 things are already in their best place."),
        "{s}"
    );
    assert!(
        s.contains("\n  #8 Kırmızı LED 10 mm\n    D-B1 → D-A1 ("),
        "{s}"
    );
    assert!(s.contains("\nFill unknown or out of date:\n"), "{s}");
}

#[test]
fn tree_nests_the_home_and_lists_the_lost_apart_with_where_they_were_last_seen() {
    let h = Home::new();
    h.run(&["lost", "Aktif buzzer"], None);
    let s = h.text(&["tree"]);
    assert!(s.starts_with("Ev  #1 (home)  [2 items]\n"), "{s}");
    assert!(
        s.contains("\n      D-A1  Kutu  #4 (container)  (not counted)  [1 items]\n"),
        "{s}"
    );
    // The lost buzzer is under its own heading, not in its box.
    assert!(
        s.contains("\nUnknown place\n  #7 Aktif buzzer  (last seen in D-B1)\n"),
        "{s}"
    );
    assert_eq!(s.matches("Aktif buzzer").count(), 1, "{s}");
}

#[test]
fn photo_add_and_list_name_the_node_then_each_photo_by_number_with_its_note() {
    let h = Home::new();
    let file = h.dir.path().join("kutu.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([200, 30, 30]))
        .save(&file)
        .unwrap();
    let file = file.to_str().unwrap();
    h.run(&["photo", "add", "D-B1", file, "--note", "ön yüz"], None);
    let s = h.text(&["photo", "add", "D-B1", file, "--crop", "0,0,0.5,1"]);
    assert!(s.starts_with("#5 Ev › Oda › D › D-B1  [Kutu]\n"), "{s}");
    assert!(s.contains(".png  — ön yüz\n"), "{s}");
    assert!(
        s.contains("\n  2. ") && s.contains("  crop 0.0000,0.0000,0.5000,1.0000\n"),
        "{s}"
    );
    assert!(!s.contains('{'), "{s}");
    assert_eq!(s, h.text(&["photo", "list", "D-B1"]));
}
