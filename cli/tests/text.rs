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
        s.contains("\n  task #1: Çekmeceyi say (order 1)  (via #3)\n"),
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
    // Places not counted yet get their photo on their tour, so they are only counted here.
    assert!(
        s.contains("\n3 more places get their photo on their tour\n"),
        "{s}"
    );
    assert!(!s.contains("Photo of the current state needed"), "{s}");
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
fn layout_lists_spread_kinds_and_drafts_a_layout_with_its_moves() {
    let h = Home::new();
    h.run(
        &["add", "--stdin"],
        Some(
            r#"{"name":"Raf","kind":"furniture","in":"Oda","code":"K2"}
{"name":"Çekmece","kind":"container","in":"K2","code":"K2-A"}
{"name":"Çekmece","kind":"container","in":"K2","code":"K2-B"}
{"name":"Çekmece","kind":"container","in":"K2","code":"K2-C"}
{"name":"Kablo, USB-C","kind":"item","in":"K2-A"}
{"name":"Kablo, HDMI","kind":"item","in":"K2-A"}
{"name":"Pil AA","kind":"item","in":"K2-B"}
{"name":"Kablo, Lightning","kind":"item","in":"K2-B"}
{"name":"Pil 9V","kind":"item","in":"K2-C"}
{"name":"Pil, AAA","kind":"item","in":"K2-C"}
"#,
        ),
    );
    let s = h.text(&["layout", "K2", "--propose"]);
    assert!(s.starts_with("K2  Raf: 3 places, 6 things\n"), "{s}");
    assert!(
        s.contains("\nKinds spread over several places:\n  kablo (3): K2-A 2, K2-B 1\n"),
        "{s}"
    );
    assert!(
        s.contains("\n  K2-A  kablo (3 things, 1 to bring)\n    #"),
        "{s}"
    );
    assert!(s.contains(" Kablo, Lightning  from K2-B\n"), "{s}");
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

#[test]
fn show_lists_make_model_and_serial_before_the_note() {
    let h = Home::new();
    h.run(
        &[
            "edit",
            "Aktif buzzer",
            "make=Murata",
            "model=PKM17EPPH4001",
            "note=kart üstü",
        ],
        None,
    );
    let s = h.text(&["show", "Aktif buzzer"]);
    assert!(
        s.contains("\n  make: Murata\n  model: PKM17EPPH4001\n  note: kart üstü\n"),
        "{s}"
    );
    assert!(!s.contains("serial"), "{s}");
}

#[test]
fn doc_add_names_the_document_its_stored_copy_and_what_it_belongs_to() {
    let h = Home::new();
    let pdf = h.dir.path().join("fatura.pdf");
    std::fs::write(&pdf, "%PDF").unwrap();
    let s = h.text(&[
        "doc",
        "add",
        pdf.to_str().unwrap(),
        "--kind",
        "invoice",
        "--for",
        "Aktif buzzer",
        "--issued",
        "2024-05-03",
        "--issuer",
        "Robotistan",
        "--number",
        "RBT-1",
    ]);
    assert!(
        s.starts_with("#1 invoice  2024-05-03  Robotistan  no RBT-1  fatura.pdf\n  file: "),
        "{s}"
    );
    assert!(s.contains("/docs/"), "{s}");
    assert!(
        s.ends_with("  → #7 Ev › Oda › D › D-B1 › Aktif buzzer\n"),
        "{s}"
    );
    assert_eq!(
        h.text(&["doc", "list"]),
        "#1 invoice  2024-05-03  Robotistan  no RBT-1  fatura.pdf  → #7\n"
    );
    let show = h.text(&["show", "Aktif buzzer"]);
    assert!(
        show.contains("\n  document: #1 invoice  2024-05-03  Robotistan  no RBT-1  fatura.pdf\n"),
        "{show}"
    );
}

#[test]
fn buy_add_shows_the_line_its_order_and_the_thing_it_is_linked_to() {
    let h = Home::new();
    let s = h.text(&[
        "buy",
        "add",
        "Aktif buzzer 5V",
        "--shop",
        "Robotistan",
        "--date",
        "2021-12-24",
        "--paid",
        "12,50",
        "--currency",
        "TRY",
        "--order",
        "TS-1",
        "--for",
        "Aktif buzzer",
    ]);
    assert_eq!(
        s,
        "#1  ordered 2021-12-24  Robotistan  Aktif buzzer 5V ×1  12.50 TRY  [linked]\n  \
         order: TS-1\n  → #7 Ev › Oda › D › D-B1 › Aktif buzzer ×1\n"
    );
    let show = h.text(&["show", "Aktif buzzer"]);
    assert!(
        show.contains(
            "\n  bought: #1  ordered 2021-12-24  Robotistan  Aktif buzzer 5V ×1  12.50 TRY\n"
        ),
        "{show}"
    );
    assert_eq!(h.text(&["buy", "list", "--open"]), "(no purchases)\n");
}

#[test]
fn buy_for_ranks_the_lines_with_their_reasons_and_add_offers_them() {
    let h = Home::new();
    h.run(
        &["buy", "import", "--stdin"],
        Some(
            r#"{"source":"s","key":"1","shop":"Shop","brand":"Murata","name":"Murata PKM17EPPH4001 Buzzer","delivered_at":"2021-12-24","paid":"12.50","currency":"TRY"}"#,
        ),
    );
    let s = h.text(&[
        "add",
        "Buzzer, Murata",
        "--kind",
        "item",
        "--in",
        "D-B1",
        "--model",
        "PKM17EPPH4001",
    ]);
    assert!(s.contains("\n  Could be one of these purchases:\n  1. #1  delivered 2021-12-24  Shop  Murata PKM17EPPH4001 Buzzer ×1  12.50 TRY  ("), "{s}");
    let f = h.text(&["buy", "for", "Buzzer, Murata"]);
    assert!(
        f.starts_with("#9 Ev › Oda › D › D-B1 › Buzzer, Murata\n  1. #1 "),
        "{f}"
    );
    assert!(f.contains("model pkm17epph4001 60"), "{f}");
    assert!(f.contains("brand murata 12"), "{f}");
}

#[test]
fn buy_for_toured_numbers_each_thing_with_its_path_then_the_line_and_its_reasons() {
    let h = Home::new();
    h.run(
        &["buy", "import", "--stdin"],
        Some(
            r#"{"source":"s","key":"1","shop":"Shop","brand":"Murata","name":"Murata PKM17EPPH4001 Buzzer","delivered_at":"2021-12-24","paid":"12.50","currency":"TRY"}"#,
        ),
    );
    h.run(&["edit", "Aktif buzzer", "model=PKM17EPPH4001"], None);
    // Nothing is toured yet: nothing is asked.
    assert_eq!(
        h.text(&["buy", "for", "--toured"]),
        "(none of 0 unlinked things in toured places could be a purchase)\n"
    );
    for place in ["D", "D-A1", "D-B1"] {
        h.run(&["photo", "current", place], None);
    }
    h.run(&["review", "D", "--as", "toured"], None);
    let s = h.text(&["buy", "for", "--toured"]);
    assert!(
        s.starts_with(
            "1 of 5 unlinked things in toured places could be a purchase, best first:\n  1. #7 Ev › Oda › D › D-B1 › Aktif buzzer\n     #1  delivered 2021-12-24  Shop  Murata PKM17EPPH4001 Buzzer ×1  12.50 TRY  ("
        ),
        "{s}"
    );
    assert!(s.contains("model pkm17epph4001 60"), "{s}");
    assert_eq!(s.lines().count(), 3, "{s}");
}

#[test]
fn cover_add_states_the_term_and_status_and_show_lists_it() {
    let h = Home::new();
    let s = h.text(&[
        "cover",
        "add",
        "Aktif buzzer",
        "--kind",
        "manufacturer",
        "--term",
        "lifetime",
        "--from",
        "2020-01-01",
        "--issuer",
        "Murata",
    ]);
    assert_eq!(
        s,
        "#1 manufacturer warranty  Murata  lifetime  active, lifetime\n  \
         → #7 Ev › Oda › D › D-B1 › Aktif buzzer\n"
    );
    let show = h.text(&["show", "Aktif buzzer"]);
    assert!(
        show.contains(
            "\n  coverage: #1 manufacturer warranty  Murata  lifetime  active, lifetime\n"
        ),
        "{show}"
    );
    h.run(&["track", "D-A1", "value", "no", "--why", "LED'ler"], None);
    let led = h.text(&["show", "Kırmızı LED 5 mm"]);
    assert!(
        led.contains("\n  value: not tracked — LED'ler  (via #4)\n"),
        "{led}"
    );
    let settings = h.text(&["settings", "inventory"]);
    assert!(
        settings.contains("valuable_threshold: 1000  (default)\n"),
        "{settings}"
    );
}

#[test]
fn money_imports_the_index_and_a_bought_thing_shows_its_price_in_todays_money() {
    let h = Home::new();
    h.run(
        &[
            "buy",
            "add",
            "Aktif buzzer 5V",
            "--date",
            "2024-05-16",
            "--paid",
            "500.00",
            "--currency",
            "TRY",
            "--for",
            "Aktif buzzer",
        ],
        None,
    );
    let imported = h.run(
        &["--text", "money", "import", "--stdin"],
        Some(
            "{\"type\":\"index\",\"series\":\"eurostat:TR\",\"period\":\"2024-05\",\"value\":71.67}\n\
             {\"type\":\"index\",\"series\":\"eurostat:TR\",\"period\":\"2026-08\",\"value\":134.76}\n",
        ),
    );
    assert_eq!(imported, "2 index values, 0 rates imported\n");
    let show = h.text(&["show", "Aktif buzzer"]);
    assert!(
        show.contains("500.00 TRY  ≈ 940.14 TRY in 2026-08 money\n"),
        "{show}"
    );
    let status = h.text(&["money", "status"]);
    assert!(
        status.starts_with("eurostat:TR index: 2 periods, latest 2026-08"),
        "{status}"
    );
}

#[test]
fn value_and_link_show_on_the_thing_with_the_latest_value_first() {
    let h = Home::new();
    h.run(
        &[
            "value",
            "Aktif buzzer",
            "40",
            "--at",
            "2025-01-10",
            "--source",
            "ilan",
        ],
        None,
    );
    let v = h.text(&["value", "Aktif buzzer", "55", "--at", "2026-09-01"]);
    assert_eq!(
        v,
        "#7 Ev › Oda › D › D-B1 › Aktif buzzer\n  #2  2026-09-01  55.00 TRY\n  \
         #1  2025-01-10  40.00 TRY  ilan\n"
    );
    let l = h.text(&[
        "link",
        "add",
        "Aktif buzzer",
        "https://example.com/buzzer",
        "--archive",
        "https://web.archive.org/web/2024/https://example.com/buzzer",
    ]);
    assert!(
        l.contains("  #1 info page  https://example.com/buzzer  archive: https://web.archive.org/"),
        "{l}"
    );
    let show = h.text(&["show", "Aktif buzzer"]);
    assert!(
        show.contains("\n  value: #2  2026-09-01  55.00 TRY  (+1 earlier)\n"),
        "{show}"
    );
    assert!(
        show.contains("\n  link: #1 info page  https://example.com/buzzer"),
        "{show}"
    );
}

#[test]
fn photo_mark_lists_each_label_and_where_the_numbered_copy_is() {
    let h = Home::new();
    let file = h.dir.path().join("drawer.png");
    image::RgbImage::from_pixel(200, 100, image::Rgb([120, 120, 120]))
        .save(&file)
        .unwrap();
    let file = file.to_string_lossy().to_string();
    let s = h.text(&[
        "photo",
        "mark",
        &file,
        "1=0.1,0.1,0.3,0.3",
        "2=0.5,0.5,0.2,0.2",
    ]);
    assert!(
        s.starts_with("1  0.1,0.1,0.3,0.3\n2  0.5,0.5,0.2,0.2\nNumbered photo: "),
        "{s}"
    );
    assert!(!s.contains('{'), "{s}");
}

#[test]
fn focus_says_what_ev_ui_was_asked_to_show_or_that_the_request_is_cleared() {
    let h = Home::new();
    assert_eq!(h.text(&["focus", "D"]), "Sent to ev ui: #3\n");
    let file = h.dir.path().join("marked.png");
    image::RgbImage::from_pixel(20, 10, image::Rgb([0, 0, 0]))
        .save(&file)
        .unwrap();
    let file = file.to_string_lossy().to_string();
    assert_eq!(
        h.text(&["focus", "--file", &file]),
        "Sent to ev ui: 1 picture(s), 1 in the series\n"
    );
    assert_eq!(
        h.text(&["focus", "--clear"]),
        "The request to ev ui is cleared.\n"
    );
}

#[test]
fn money_needs_says_the_index_and_lists_the_missing_rates() {
    let h = Home::new();
    assert_eq!(
        h.text(&["money", "needs"]),
        "eurostat:TR index from -; home currency TRY, country TR\nNo exchange rate missing.\n"
    );
    h.run(
        &[
            "buy",
            "add",
            "Aktif buzzer 5V",
            "--date",
            "2026-03-04",
            "--paid",
            "10",
            "--currency",
            "USD",
        ],
        None,
    );
    assert_eq!(
        h.text(&["money", "needs"]),
        "eurostat:TR index from 2026-03; home currency TRY, country TR\n\
         Exchange rates missing (1):\n  USD 2026-03-04\n"
    );
}

#[test]
fn a_thing_in_several_places_shows_its_units_in_all_and_where_the_rest_are() {
    let h = Home::new();
    h.run(&["edit", "Kırmızı LED 5 mm", "qty=10"], None);
    h.run(
        &[
            "move",
            "Kırmızı LED 5 mm",
            "--qty",
            "3",
            "--to",
            "Aktif buzzer",
        ],
        None,
    );
    let s = h.text(&["show", "#6"]);
    assert!(
        s.contains(
            "\n  thing: ×10 in 2 places · in use 3 · spare 7\n    elsewhere: #9 Ev › Oda › D › \
             D-B1 › Aktif buzzer › Kırmızı LED 5 mm ×3 (in use)\n"
        ),
        "{s}"
    );
}

#[test]
fn find_shows_a_thing_in_several_places_together_under_one_line() {
    let h = Home::new();
    h.run(&["edit", "Kırmızı LED 5 mm", "qty=10"], None);
    h.run(
        &[
            "move",
            "Kırmızı LED 5 mm",
            "--qty",
            "3",
            "--to",
            "Aktif buzzer",
        ],
        None,
    );
    let s = h.text(&["find", "kırmızı led 5 mm"]);
    assert!(
        s.starts_with(
            "Kırmızı LED 5 mm ×10 in 2 places · in use 3 · spare 7\n  #6 Ev › Oda › D › D-A1 › \
             Kırmızı LED 5 mm  x7\n  #9 Ev › Oda › D › D-B1 › Aktif buzzer › Kırmızı LED 5 mm  x3\n"
        ),
        "{s}"
    );
}

#[test]
fn a_purchase_names_its_dates_the_order_first_then_the_delivery() {
    let h = Home::new();
    h.run(
        &["buy", "import", "--stdin"],
        Some(
            "{\"source\":\"s\",\"key\":\"1\",\"name\":\"Etiket makinesi\",\"shop\":\"Dükkan\",\
             \"ordered_at\":\"2024-12-06\",\"delivered_at\":\"2024-12-09\",\"paid\":\"999\",\
             \"currency\":\"TRY\"}\n",
        ),
    );
    let s = h.text(&["buy", "show", "1"]);
    assert!(
        s.starts_with("#1  ordered 2024-12-06 · delivered 2024-12-09  Dükkan  Etiket makinesi"),
        "{s}"
    );
}

#[test]
fn buy_bring_says_what_it_brought_and_what_it_left_and_why() {
    let h = Home::new();
    h.run(
        &["buy", "import", "--stdin"],
        Some(
            "{\"source\":\"s\",\"key\":\"1\",\"name\":\"Aktif buzzer\",\"paid\":\"99\"}\n\
             {\"type\":\"valuation\",\"source\":\"s\",\"purchase\":\"1\",\"amount\":\"120\"}\n\
             {\"type\":\"link\",\"source\":\"s\",\"purchase\":\"1\",\
              \"url\":\"https://shop.example/buzzer\"}\n",
        ),
    );
    h.run(&["buy", "link", "1", "Aktif buzzer"], None);
    let s = h.text(&[
        "buy",
        "bring",
        "1",
        "Aktif buzzer",
        "--only",
        "1,2",
        "--type",
        "link",
    ]);
    assert!(
        s.starts_with("Brought from purchase #1: link ×1\n  left, of another type: #1 value\n\n"),
        "{s}"
    );
    let again = h.text(&["buy", "bring", "1", "Aktif buzzer", "--type", "link"]);
    assert!(
        again.starts_with("Nothing brought from purchase #1.\n  already brought: #2 link\n"),
        "{again}"
    );
}

#[test]
fn buy_bring_all_lists_each_line_it_brought_from_with_the_thing() {
    let h = Home::new();
    h.run(
        &["buy", "import", "--stdin"],
        Some(
            "{\"source\":\"s\",\"key\":\"1\",\"name\":\"Aktif buzzer\",\"paid\":\"99\"}\n\
             {\"type\":\"link\",\"source\":\"s\",\"purchase\":\"1\",\
              \"url\":\"https://shop.example/buzzer\"}\n",
        ),
    );
    h.run(&["buy", "link", "1", "Aktif buzzer"], None);
    let s = h.text(&["buy", "bring", "--all", "--type", "image,link"]);
    let id = h.run(&["show", "Aktif buzzer"], None);
    let id: serde_json::Value = serde_json::from_str(&id).unwrap();
    assert_eq!(
        s,
        format!(
            "Brought from 1 purchases: link ×1\n  #1 → #{} Aktif buzzer: link ×1\n",
            id["node"]["id"]
        )
    );
    // Run again, it says why nothing came: the one line's link is brought already.
    assert_eq!(
        h.text(&["buy", "bring", "--all"]),
        "Nothing brought: 1 linked lines checked, 1 carry that type, 1 of it brought already.\n"
    );
}

#[test]
fn grid_lists_each_box_with_its_cells_its_code_and_its_name() {
    let h = Home::new();
    let s = h.text(&["grid", "D"]);
    assert!(s.contains("  A1      D-A1  Kutu\n"), "{s}");
    assert!(s.contains("  B1      D-B1  Kutu\n"), "{s}");
}

#[test]
fn photo_mark_codes_puts_each_placed_boxs_code_on_its_cells() {
    let h = Home::new();
    let photo = h.dir.path().join("d.png");
    image::RgbImage::from_pixel(400, 200, image::Rgb([120, 120, 120]))
        .save(&photo)
        .unwrap();
    h.run(&["photo", "add", "D", photo.to_str().unwrap()], None);
    let out = h.run(
        &[
            "--json",
            "photo",
            "mark",
            "D",
            "--codes",
            "--grid",
            "0.1,0.1,0.9,0.1,0.9,0.9,0.1,0.9",
        ],
        None,
    );
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        v["marks"],
        serde_json::json!([{"label": "D-A1", "at": "A1"}, {"label": "D-B1", "at": "B1"}])
    );
}

#[test]
fn a_line_added_to_a_note_shows_only_that_line() {
    let h = Home::new();
    h.text(&["edit", "D-B1", "note=uzun bir not, ilk satır"]);
    let s = h.text(&["edit", "D-B1", "note=+ikinci satır"]);
    assert!(s.contains("+ ikinci satır"), "{s}");
    assert!(!s.contains("ilk satır"), "{s}");
    // A theme written out longer is replaced, not added to: shown before and after.
    h.text(&["edit", "D-B1", "theme=Dremel"]);
    let s = h.text(&["edit", "D-B1", "theme=Dremel ve parçaları"]);
    assert!(s.contains("Dremel → Dremel ve parçaları"), "{s}");
}
