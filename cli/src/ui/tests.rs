use super::{App, DetailTab, Drag, Pane, Tab};
use crate::i18n::Lang;
use crate::input::Input;
use crate::settings::{LangPref, Settings, ThemePref};
use crate::theme::{self, Mode};
use ev_core::{Inventory, NewNode};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::style::Color;
use ratatui::{Terminal, backend::TestBackend};
use ratatui_image::picker::Picker;

fn with_prefs(inv: Inventory, language: LangPref, theme: ThemePref) -> App {
    let mut app = App::new(inv).unwrap();
    // Tests do not depend on the COLORFGBG of whoever runs them.
    app.detected = None;
    app.set_prefs(Settings {
        language,
        theme,
        ..Default::default()
    })
    .unwrap();
    app
}

/// The older screen tests read Turkish words, so they run with Turkish chosen.
fn app_tr(inv: Inventory) -> App {
    with_prefs(inv, LangPref::Fixed(Lang::Tr), ThemePref::Auto)
}

fn home() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    inv.add(NewNode {
        name: "Ev".into(),
        kind: "home".into(),
        ..Default::default()
    })
    .unwrap();
    (dir, inv)
}

#[test]
fn english_is_shown_when_english_is_chosen() {
    let (_dir, inv) = home();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    // Tall enough for the whole sidebar.
    let mut term = Terminal::new(TestBackend::new(140, 26)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("1 Layout") && s.contains("0 Settings"), "{s}");
    // Only the tabs the node has something for: a bare home has its summary alone.
    assert!(s.contains("Summary") && !s.contains("Grid"), "{s}");
    assert!(s.contains("│home"), "{s}");
    assert!(!s.contains("Ayrıntı"), "{s}");
}

#[test]
fn the_settings_tab_switches_language_and_appearance_and_saves_them() {
    let (dir, inv) = home();
    let path = dir.path().join("settings.json");
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    app.settings_path = Some(path.clone());
    let mut term = Terminal::new(TestBackend::new(140, 26)).unwrap();

    press(&mut app, KeyCode::Char('0'));
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("Language: English"), "{s}");
    assert!(s.contains("Appearance: Automatic"), "{s}");

    // Enter moves English to Türkçe, and the whole screen follows at once.
    press(&mut app, KeyCode::Enter);
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("Dil: Türkçe") && s.contains("0 Ayarlar"), "{s}");
    assert!(s.contains("ayarlar kaydedildi"), "{s}");
    assert_eq!(
        Settings::load_from(&path).language,
        LangPref::Fixed(Lang::Tr)
    );

    // ← goes back; the appearance row moves Automatic to Dark.
    press(&mut app, KeyCode::Left);
    assert_eq!(app.prefs.language, LangPref::Fixed(Lang::En));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.prefs.theme, ThemePref::Fixed(Mode::Dark));
    assert_eq!(
        Settings::load_from(&path).theme,
        ThemePref::Fixed(Mode::Dark)
    );

    // The last row turns reopening where I left off off, and back on.
    press(&mut app, KeyCode::Down);
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("Reopen where I left off: On"));
    press(&mut app, KeyCode::Enter);
    assert!(!app.prefs.resume);
    assert!(!Settings::load_from(&path).resume);
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("Reopen where I left off: Off"));
    press(&mut app, KeyCode::Enter);
    assert!(Settings::load_from(&path).resume);
}

#[test]
fn the_palette_follows_the_terminal_while_running() {
    let (_dir, inv) = home();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let mut term = Terminal::new(TestBackend::new(120, 20)).unwrap();
    let brand_bg = |term: &Terminal<TestBackend>| term.backend().buffer()[(1, 0)].bg;

    term.draw(|f| app.draw(f)).unwrap();
    assert_eq!(brand_bg(&term), Color::Cyan);

    // The terminal reports a switch to light (mode 2031): no restart needed.
    app.handle(Input::Appearance {
        mode: Mode::Light,
        notified: true,
    })
    .unwrap();
    assert_eq!(theme::mode(), Mode::Light);
    term.draw(|f| app.draw(f)).unwrap();
    assert_eq!(brand_bg(&term), Color::Rgb(0, 110, 140));

    app.handle(Input::Appearance {
        mode: Mode::Dark,
        notified: true,
    })
    .unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    assert_eq!(brand_bg(&term), Color::Cyan);

    // A fixed appearance ignores the terminal.
    app.set_prefs(Settings {
        language: LangPref::Fixed(Lang::En),
        theme: ThemePref::Fixed(Mode::Light),
        ..Default::default()
    })
    .unwrap();
    app.handle(Input::Appearance {
        mode: Mode::Dark,
        notified: true,
    })
    .unwrap();
    assert_eq!(theme::mode(), Mode::Light);
}

#[test]
fn the_picture_protocol_comes_from_the_answers_read_by_ev_ui() {
    use crate::input::Graphics;
    use ratatui_image::picker::ProtocolType;
    let (_dir, inv) = home();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    app.picker = Some(Picker::halfblocks());
    app.probe = Some(super::ImageProbe::default());
    for g in [
        Graphics::Kitty,
        Graphics::Attributes { sixel: false },
        Graphics::CellSize {
            width: 10,
            height: 20,
        },
    ] {
        app.handle(Input::Graphics(g)).unwrap();
    }
    // Nothing changes until the status report says the answers are complete.
    assert_eq!(
        app.picker.as_ref().unwrap().protocol_type(),
        ProtocolType::Halfblocks
    );
    app.handle(Input::Graphics(Graphics::Done)).unwrap();
    let p = app.picker.as_ref().unwrap();
    if std::env::var_os("WEZTERM_EXECUTABLE").is_none()
        && std::env::var_os("KONSOLE_VERSION").is_none()
    {
        assert_eq!(p.protocol_type(), ProtocolType::Kitty);
    }
    assert!(app.probe.is_none());
}

#[test]
fn it_reopens_on_the_tree_node_it_was_on_and_on_the_top_when_that_is_gone() {
    let (_dir, inv) = led_drawer();
    let led = inv.resolve("Kırmızı LED 10 mm", false).unwrap();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    // A new session starts on a node inside a collapsed box.
    app.resume_at(led).unwrap();
    assert_eq!(app.selected_id(), Some(led));
    // The tree's position is what is kept, even when another tab is open at exit.
    app.switch(Tab::Search).unwrap();
    assert_eq!(app.tree_position(), Some(led));
    // A node deleted since leaves the first row selected rather than nothing.
    app.resume_at(999_999).unwrap();
    assert_eq!(app.state.selected(), Some(0));
}

#[test]
fn a_grid_holder_shows_its_map_and_a_box_its_cells() {
    let (dir, mut inv) = home();
    let mut boxes: Vec<(String, &str, String, Option<String>)> = vec![
        ("Oda".into(), "room", "Ev".into(), None),
        (
            "Çekmece".into(),
            "container",
            "Oda".into(),
            Some("D".into()),
        ),
        ("Kutu".into(), "container", "D".into(), Some("D-B1".into())),
    ];
    // Enough boxes and photos that anything below the fields would be off screen.
    for i in 0..25 {
        boxes.push((format!("Dolgu {i}"), "container", "D".into(), None));
    }
    for (name, kind, parent, code) in boxes {
        inv.add(NewNode {
            name,
            kind: kind.into(),
            parent: Some(parent),
            code,
            ..Default::default()
        })
        .unwrap();
    }
    let photo = dir.path().join("p.png");
    image::RgbImage::from_pixel(16, 16, image::Rgb([9, 9, 9]))
        .save(&photo)
        .unwrap();
    for i in 0..8 {
        inv.photo_add("D", &photo, None, Some(&format!("{i}")))
            .unwrap();
    }
    inv.grid_set("D", 3, 2).unwrap();
    inv.cells_set(&[("D-B1".into(), "B1-C1".into())]).unwrap();
    let drawer = inv.resolve("D", false).unwrap();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    app.picker = Some(Picker::halfblocks());
    app.reveal(drawer).unwrap();
    // `L` steps from the summary through the photos to the grid.
    press(&mut app, KeyCode::Char('L'));
    assert_eq!(app.shown_detail_tab(), DetailTab::Photos);
    press(&mut app, KeyCode::Char('L'));
    let mut term = Terminal::new(TestBackend::new(140, 40)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("3×2 grid, row 1 at the back"), "{s}");
    assert!(s.contains(" 1    ·  │ B1        │"), "{s}");
    assert!(s.contains("┌───────────┐"), "{s}");
    assert!(s.contains("free (4): A1 A2 B2 C2"), "{s}");

    // On the drawer's own grid a box opens with a click, on its cells or on the frame
    // inside it; a free cell does nothing.
    let (x0, y0) = (app.details_area.x + 1, app.details_area.y + 1 + 5);
    let cell = |col: u16, line: u16| (x0 + 3 + col * 6 + 2, y0 + line);
    let (x, y) = cell(0, 1);
    click(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
    assert_eq!(app.selected_id(), Some(drawer));
    let (x, y) = cell(2, 1);
    click(&mut app, MouseEventKind::Down(MouseButton::Left), x - 2, y);
    let bx = app.selected_id().unwrap();
    assert_eq!(app.snap.label[&bx], "D-B1  Kutu");
    // On the box, its drawer's grid still opens boxes; a free cell stays put.
    term.draw(|f| app.draw(f)).unwrap();
    assert!(app.grid_hit.is_some());
    let (x, y) = cell(0, 1);
    click(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
    assert_eq!(app.selected_id(), Some(bx));
}

#[test]
fn a_grid_click_finds_the_box_under_it_frames_included() {
    use super::grid_box_at;
    // Row 1: A1 alone, B1–C1 one box. Row 2: A2 free, B2–C2 another box.
    let map = vec![
        vec![Some(1), Some(2), Some(2)],
        vec![None, Some(3), Some(3)],
    ];
    // Cell lines are odd, frame lines even; columns start after the 3-column row label.
    assert_eq!(grid_box_at(&map, 3 + 2, 1), Some(1));
    assert_eq!(
        grid_box_at(&map, 3 + 6, 1),
        None,
        "the frame between A1 and B1"
    );
    assert_eq!(
        grid_box_at(&map, 3 + 12, 1),
        Some(2),
        "the frame inside B1–C1"
    );
    assert_eq!(
        grid_box_at(&map, 3 + 8, 2),
        None,
        "the frame between rows 1 and 2"
    );
    assert_eq!(grid_box_at(&map, 3 + 2, 3), None, "a free cell");
    assert_eq!(grid_box_at(&map, 1, 1), None, "the row label");
}

/// `ev sketch <r> [--at] [--size] [--on] [--points]`.
fn sketch(
    inv: &mut Inventory,
    r: &str,
    at: Option<&str>,
    size: Option<&str>,
    on: Option<&str>,
    points: Option<&str>,
) -> ev_core::Result<serde_json::Value> {
    inv.sketch_set(&ev_core::SketchChange {
        reference: r.into(),
        at: at.map(|s| ev_core::parse_pair(s, "--at")).transpose()?,
        size: size.map(|s| ev_core::parse_pair(s, "--size")).transpose()?,
        on: on.map(Into::into),
        points: points.map(ev_core::parse_points).transpose()?,
        ..Default::default()
    })
}

fn add(inv: &mut Inventory, name: &str, kind: &str, parent: &str, code: Option<&str>) {
    inv.add(NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: Some(parent.into()),
        code: code.map(Into::into),
        ..Default::default()
    })
    .unwrap();
}

/// A drawer `D` in a 2×1 grid: a red LED box, and a buzzer box with a red LED in it.
fn led_drawer() -> (tempfile::TempDir, Inventory) {
    let (dir, mut inv) = home();
    add(&mut inv, "Oda", "room", "Ev", None);
    add(&mut inv, "Çekmece", "container", "Oda", Some("D"));
    add(&mut inv, "Kutu", "container", "D", Some("D-A1"));
    add(&mut inv, "Kutu", "container", "D", Some("D-B1"));
    inv.edit("D-A1", &["theme=Kırmızı LED".into(), "size=1x1x1".into()])
        .unwrap();
    inv.edit("D-B1", &["theme=Buzzer".into(), "fill=50".into()])
        .unwrap();
    for (name, parent) in [
        ("Kırmızı LED 5 mm", "D-A1"),
        ("Kırmızı LED 3 mm", "D-A1"),
        ("Aktif buzzer", "D-B1"),
        ("Pasif buzzer", "D-B1"),
        ("Kırmızı LED 10 mm", "D-B1"),
    ] {
        add(&mut inv, name, "item", parent, None);
    }
    inv.grid_set("D", 2, 1).unwrap();
    inv.cells_set(&[("D-A1".into(), "A1".into()), ("D-B1".into(), "B1".into())])
        .unwrap();
    (dir, inv)
}

#[test]
fn m_opens_the_map_walks_it_goes_in_and_out_and_t_shows_the_tile_in_the_tree() {
    let (_dir, mut inv) = led_drawer();
    add(&mut inv, "Raf", "furniture", "Oda", None);
    sketch(&mut inv, "Raf", None, None, Some("D"), None).unwrap();
    let id = |app: &App, r: &str| app.inv.resolve(r, false).unwrap();
    let mut app = app_tr(inv);
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let mut draw = |app: &mut App| {
        term.draw(|f| app.draw(f)).unwrap();
        screen(&term)
    };
    let sel = |app: &App| app.map_view.as_ref().and_then(|m| m.selected());
    // From a box in the tree the map opens on the home, the room on the way chosen.
    app.reveal(id(&app, "D-A1")).unwrap();
    press(&mut app, KeyCode::Char('M'));
    let s = draw(&mut app);
    assert!(s.contains(" Ev "), "{s}");
    assert_eq!(sel(&app), Some(id(&app, "Oda")));
    // Enter after Enter follows the way down: the drawer, then the box.
    press(&mut app, KeyCode::Enter);
    assert_eq!(sel(&app), Some(id(&app, "D")));
    press(&mut app, KeyCode::Enter);
    let s = draw(&mut app);
    assert!(s.contains("Ev › Oda › D"), "{s}");
    assert!(s.contains(" Raf "), "{s}");
    assert!(s.contains("D-B1"), "{s}");
    assert!(s.contains("Enter içine gir"), "{s}");
    // A box's tile says what it is for and what is in it.
    assert!(s.contains("Buzzer"), "{s}");
    assert!(s.contains("Aktif buzzer · Kırmızı LED 10 mm"), "{s}");
    assert_eq!(sel(&app), Some(id(&app, "D-A1")));
    press(&mut app, KeyCode::Right);
    assert_eq!(sel(&app), Some(id(&app, "D-B1")));
    // Into the box, and back out onto it.
    press(&mut app, KeyCode::Enter);
    let s = draw(&mut app);
    assert!(s.contains("Ev › Oda › D › D-B1"), "{s}");
    assert!(s.contains("Pasif buzzer"), "{s}");
    press(&mut app, KeyCode::Enter);
    assert!(draw(&mut app).contains("içinde bir şey yok"));
    press(&mut app, KeyCode::Backspace);
    assert_eq!(sel(&app), Some(id(&app, "D-B1")));
    // The room: the drawer's tile says what stands on it.
    press(&mut app, KeyCode::Backspace);
    let s = draw(&mut app);
    assert!(s.contains("üstünde Raf"), "{s}");
    assert_eq!(sel(&app), Some(id(&app, "D")));
    // t closes the map on that tile in the tree.
    press(&mut app, KeyCode::Char('t'));
    assert!(app.map_view.is_none());
    assert_eq!(app.selected_id(), Some(id(&app, "D")));
    assert!(!app.quit);
}

#[test]
fn a_home_with_a_plan_draws_its_rooms_in_their_shapes_and_a_click_picks_one() {
    let (_dir, mut inv) = home();
    add(&mut inv, "Salon", "room", "Ev", None);
    add(&mut inv, "Mutfak", "room", "Ev", None);
    add(&mut inv, "Kupa", "item", "Mutfak", None);
    sketch(
        &mut inv,
        "Salon",
        None,
        None,
        None,
        Some("0,0 600,0 600,400 0,400"),
    )
    .unwrap();
    sketch(
        &mut inv,
        "Mutfak",
        None,
        None,
        None,
        Some("620,0 900,0 900,400 620,400"),
    )
    .unwrap();
    let mut app = app_tr(inv);
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    press(&mut app, KeyCode::Char('M'));
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    // Rooms are floors of their own shape, named inside, not frames.
    assert!(s.contains("Salon") && s.contains("Mutfak"), "{s}");
    assert!(!s.contains("┌ Salon"), "{s}");
    // The chosen room's floor is lit, the other's is not.
    let buf = term.backend().buffer().clone();
    let bg_of = |name: &str| {
        let (row, line) = s.lines().enumerate().find(|l| l.1.contains(name)).unwrap();
        let col = line.split(name).next().unwrap().chars().count();
        (col as u16, row as u16, buf[(col as u16, row as u16)].bg)
    };
    let (_, _, salon) = bg_of("Salon");
    let (mx, my, mutfak) = bg_of("Mutfak");
    assert_eq!(salon, theme::pal().room_chosen);
    assert_ne!(mutfak, salon);
    // A click on the kitchen's floor picks it; the line under the plan says what is in it.
    click(&mut app, MouseEventKind::Down(MouseButton::Left), mx, my);
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    // A room gone through on its own says how far it is counted.
    assert!(s.contains("Mutfak 1 eşya  sayılmadı  · Kupa"), "{s}");
}

#[test]
fn the_tree_lists_lost_things_under_unknown_place_and_says_how_far_places_are_counted() {
    let (_dir, mut inv) = home();
    add(&mut inv, "Oda", "room", "Ev", None);
    add(&mut inv, "Kutu", "container", "Oda", Some("K-1"));
    add(&mut inv, "Kalem", "item", "K-1", None);
    add(&mut inv, "Silgi", "item", "K-1", None);
    inv.mark_lost("Kalem").unwrap();
    let mut app = app_tr(inv);
    app.reveal(app.inv.resolve("K-1", false).unwrap()).unwrap();
    press(&mut app, KeyCode::Right);
    let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    // The box says it is not counted; the lost pen is under its own heading, with where
    // it was last seen, and only once.
    assert!(s.contains("K-1") && s.contains("[sayılmadı]"), "{s}");
    assert!(s.contains("Yeri bilinmiyor (1)"), "{s}");
    assert!(s.contains("son görüldüğü yer: K-1"), "{s}");
    assert_eq!(s.matches("Kalem").count(), 1, "{s}");
}

#[test]
fn going_to_a_lost_thing_opens_the_collapsed_unknown_place_heading() {
    let (_dir, mut inv) = home();
    add(&mut inv, "Oda", "room", "Ev", None);
    add(&mut inv, "Kalem", "item", "Oda", None);
    inv.mark_lost("Kalem").unwrap();
    let mut app = app_tr(inv);
    let kalem = app.inv.resolve("Kalem", false).unwrap();
    // The heading closed by hand, then the pen asked for (a search, `t`, `ev focus`).
    app.toggle_section(super::LOST_SECTION).unwrap();
    assert!(!app.rows.iter().any(|r| r.id == kalem));
    app.reveal(kalem).unwrap();
    assert_eq!(app.selected_id(), Some(kalem));
}

#[test]
fn a_drawer_on_the_map_is_its_grid_plate_with_free_cells_and_cell_names() {
    let (_dir, mut inv) = led_drawer();
    // Two boxes in a 3×2 plate: four cells free.
    inv.grid_set("D", 3, 2).unwrap();
    let mut app = app_tr(inv);
    app.reveal(app.inv.resolve("D-A1", false).unwrap()).unwrap();
    press(&mut app, KeyCode::Char('M'));
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("ızgara 3×2"), "{s}");
    let letters = s
        .lines()
        .find(|l| l.split_whitespace().collect::<Vec<_>>() == ["A", "B", "C"]);
    assert!(letters.is_some(), "{s}");
    // The free cells are dots: C1 beside the boxes, and the whole of row 2.
    let row = |n: char| s.lines().find(|l| l.starts_with(n)).unwrap_or_default();
    assert_eq!(row('2').matches('·').count(), 3, "{s}");
    let after_boxes = row('1').rsplit('│').next().unwrap_or_default();
    assert_eq!(after_boxes.matches('·').count(), 1, "{s}");
    assert!(s.contains("D-A1") && s.contains("D-B1"), "{s}");
}

#[test]
fn an_empty_room_with_an_outline_opens_on_its_floor() {
    let (_dir, mut inv) = home();
    add(&mut inv, "Salon", "room", "Ev", None);
    sketch(
        &mut inv,
        "Salon",
        None,
        None,
        None,
        Some("0,0 400,0 400,300 0,300"),
    )
    .unwrap();
    let mut app = app_tr(inv);
    let mut term = Terminal::new(TestBackend::new(80, 30)).unwrap();
    press(&mut app, KeyCode::Char('M'));
    press(&mut app, KeyCode::Enter);
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    // Nothing is recorded in it, yet Enter opens it: its floor is what there is to see.
    assert!(s.contains("Ev › Salon"), "{s}");
    assert!(!s.contains("içinde bir şey yok"), "{s}");
    assert_eq!(
        term.backend().buffer()[(40, 15)].bg,
        theme::pal().floor,
        "{s}"
    );
}

fn shown(app: &mut App, reference: &str, w: u16, h: u16) -> String {
    let id = app.inv.resolve(reference, false).unwrap();
    app.reveal(id).unwrap();
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    screen(&term)
}

#[test]
fn a_box_shows_its_drawer_map_size_room_and_what_would_fit_better_elsewhere() {
    let (_dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let s = shown(&mut app, "D-B1", 150, 40);
    // Room from the fill, not only the percentage; no database id among the fields.
    assert!(s.contains("▮▮▯▯  room (50% full)"), "{s}");
    assert!(!s.contains("id: "), "{s}");
    // The tabs count what they hold: three things inside, one suggested move.
    assert!(s.contains("Contents 3 · Suggestions 1"), "{s}");
    // The drawer's map, with this box among the others.
    app.detail_tab = DetailTab::Grid;
    let s = shown(&mut app, "D-B1", 150, 40);
    assert!(s.contains("2×1 grid, row 1 at the back"), "{s}");
    assert!(s.contains(" 1 │ A1  │ B1  │"), "{s}");
    // A neighbour on it opens with a click, from the box as from the drawer.
    let (x, y) = (app.details_area.x + 1 + 5, app.details_area.y + 1 + 6);
    click(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
    let a1 = app.inv.resolve("D-A1", false).unwrap();
    assert_eq!(app.selected_id(), Some(a1));
    // The grid stays open, so the drawer can be walked box by box.
    assert_eq!(app.shown_detail_tab(), DetailTab::Grid);
    // The stray LED, with where it would fit better.
    app.detail_tab = DetailTab::Suggestions;
    let s = shown(&mut app, "D-B1", 150, 40);
    assert!(s.contains("Suggestions (ev regroup)"), "{s}");
    assert!(s.contains("→ D-A1  Kırmızı LED 10 mm"), "{s}");
    // A box with nothing to suggest shows its summary, and the choice is kept.
    let s = shown(&mut app, "D-A1", 150, 40);
    assert!(
        s.lines()
            .any(|l| l.contains("size ") && l.contains(" 1x1x1")),
        "{s}"
    );
    assert_eq!(app.detail_tab, DetailTab::Suggestions);
}

fn click(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    app.handle(Input::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
}

#[test]
fn the_divider_drags_resets_on_a_double_click_and_steps_with_keys() {
    let (_dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    // The sidebar hidden, so the list and the details share the whole width.
    app.sidebar = Some(false);
    let _ = shown(&mut app, "D-B1", 100, 30);
    // The list ends at column 54 of 100; the right side starts at 55.
    assert_eq!(app.right_area.x, 55);
    click(&mut app, MouseEventKind::Down(MouseButton::Left), 55, 10);
    assert_eq!(app.drag, Some(Drag::Columns));
    click(&mut app, MouseEventKind::Drag(MouseButton::Left), 39, 12);
    click(&mut app, MouseEventKind::Up(MouseButton::Left), 39, 12);
    assert_eq!((app.split, app.drag), (40, None));
    let s = shown(&mut app, "D-B1", 100, 30);
    assert_eq!(app.right_area.x, 40, "{s}");
    // Too far is held back, so neither side disappears.
    click(&mut app, MouseEventKind::Down(MouseButton::Left), 40, 10);
    click(&mut app, MouseEventKind::Drag(MouseButton::Left), 2, 10);
    assert_eq!(app.split, 20);
    click(&mut app, MouseEventKind::Up(MouseButton::Left), 2, 10);
    // A double click on the divider puts it back.
    let _ = shown(&mut app, "D-B1", 100, 30);
    let x = app.right_area.x;
    app.divider_click = None;
    click(&mut app, MouseEventKind::Down(MouseButton::Left), x, 10);
    click(&mut app, MouseEventKind::Up(MouseButton::Left), x, 10);
    click(&mut app, MouseEventKind::Down(MouseButton::Left), x, 10);
    assert_eq!(app.split, 55);
    press(&mut app, KeyCode::Char('<'));
    assert_eq!(app.split, 50);
    // The layout is what `ui-state.json` keeps, and a stored one comes back clamped.
    assert_eq!(app.layout_json()["split"], 50);
    app.apply_layout(&serde_json::json!({"split": 5, "details": "history"}));
    assert_eq!((app.split, app.detail_tab), (20, DetailTab::History));
}

#[test]
fn the_history_tab_tells_what_happened_here_newest_first() {
    let (_dir, mut inv) = led_drawer();
    inv.move_to("Kırmızı LED 10 mm", "D-A1", false).unwrap();
    inv.edit("D-A1", &["theme=LED".into()]).unwrap();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    app.detail_tab = DetailTab::History;
    let s = shown(&mut app, "D-A1", 150, 40);
    assert!(s.contains("Today"), "{s}");
    let at = |text: &str| s.find(text).unwrap_or_else(|| panic!("{text} in {s}"));
    // Its own change, what came in and from where, what was added: newest first.
    assert!(at("changed  theme: Kırmızı LED → LED") < at("came in  Kırmızı LED 10 mm  ← D-B1"));
    assert!(at("came in") < at("added here  Kırmızı LED 3 mm"));
    assert!(s.contains("created  D"), "{s}");
    // Clicking a title opens that tab; the summary is first.
    let (row, hits) = app.detail_tab_hits.clone();
    click(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        hits[0].0,
        row,
    );
    assert_eq!(app.detail_tab, DetailTab::Summary);
}

#[test]
fn photos_history_and_contents_lines_open_what_they_name_with_a_click() {
    let (dir, mut inv) = led_drawer();
    let photo = dir.path().join("p.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([9, 9, 9]))
        .save(&photo)
        .unwrap();
    for note in ["eski", "yeni"] {
        inv.photo_add("D-B1", &photo, None, Some(note)).unwrap();
    }
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    app.detail_tab = DetailTab::Photos;
    let s = shown(&mut app, "D-B1", 150, 40);
    // Newest first, the one shown marked; a click on the older shows it.
    let (new, old) = (s.find("yeni").unwrap(), s.find("eski").unwrap());
    assert!(new < old && s.contains("▶  2"), "{s}");
    let (x, first) = (app.details_area.x + 5, app.details_area.y + 1 + 3);
    click(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        x,
        first + 1,
    );
    assert_eq!(app.photo_idx, 0);
    // The contents open in the tree.
    app.detail_tab = DetailTab::Contents;
    let _ = shown(&mut app, "D-B1", 150, 40);
    click(&mut app, MouseEventKind::Down(MouseButton::Left), x, first);
    let opened = app.selected_id().unwrap();
    assert_eq!(
        app.snap.parent[&opened],
        app.inv.resolve("D-B1", false).unwrap()
    );
    // And so does a thing named in a drawer's history.
    app.detail_tab = DetailTab::History;
    let s = shown(&mut app, "D-A1", 150, 40);
    let line = s
        .lines()
        .position(|l| l.contains("added here  Kırmızı LED 5 mm"))
        .unwrap() as u16;
    click(&mut app, MouseEventKind::Down(MouseButton::Left), x, line);
    assert_eq!(
        app.snap.label[&app.selected_id().unwrap()],
        "Kırmızı LED 5 mm"
    );
}

#[test]
fn a_history_line_naming_something_gone_says_so_instead_of_opening_it() {
    let (_dir, mut inv) = led_drawer();
    let led = inv.resolve("Kırmızı LED 5 mm", false).unwrap();
    inv.gone(&led.to_string(), Some(ev_core::Disposition::Trash))
        .unwrap();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    app.detail_tab = DetailTab::History;
    let s = shown(&mut app, "D-A1", 150, 40);
    let line = s
        .lines()
        .position(|l| l.contains("added here  Kırmızı LED 5 mm"))
        .unwrap_or_else(|| panic!("{s}")) as u16;
    let (before, x) = (app.selected_id(), app.details_area.x + 5);
    click(&mut app, MouseEventKind::Down(MouseButton::Left), x, line);
    assert_eq!(app.selected_id(), before);
    assert_eq!(
        app.status,
        format!("#{led} is no longer in the tree (gone)")
    );
}

#[test]
fn a_marked_photo_sent_from_another_process_shows_full_screen_until_closed() {
    let (dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let marked = dir.path().join("marked.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([230, 30, 30]))
        .save(&marked)
        .unwrap();
    // Another process asks; the running UI picks it up without restarting.
    let drawer = dir.path().join("drawer.png");
    std::fs::copy(&marked, &drawer).unwrap();
    app.inv
        .focus_file(&[marked.clone(), drawer.clone()], Some("1 → A6"))
        .unwrap();
    app.apply_focus().unwrap();
    let mut term = Terminal::new(TestBackend::new(100, 20)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(
        s.contains("f1/2 · 1 → A6") && s.contains("Esc/o hide · [ ] ← → step"),
        "{s}"
    );
    // A stray click does not close it.
    click(&mut app, MouseEventKind::Down(MouseButton::Left), 10, 5);
    assert!(app.overlay.is_some());
    // Sent together, they are stepped through; the end holds.
    press(&mut app, KeyCode::Char(']'));
    press(&mut app, KeyCode::Char(']'));
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("f2/2 · 1 → A6"));
    assert_eq!(
        app.shown_picture(),
        Some(drawer.to_string_lossy().into_owned())
    );
    // Esc closes it; the same request is not shown again, but `m` opens it, and the key
    // hints say so.
    press(&mut app, KeyCode::Esc);
    app.apply_focus().unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(
        s.contains("Summary") && s.contains("m marked photo series"),
        "{s}"
    );
    assert!(app.overlay.is_none());
    press(&mut app, KeyCode::Char('m'));
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("f2/2 · 1 → A6"));
    // A restarted `ev ui` treats the request as seen, and still opens it with `m`.
    let inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    let mut again = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    again.apply_focus().unwrap();
    assert!(again.overlay.is_none());
    press(&mut again, KeyCode::Char('m'));
    assert_eq!(
        again.shown_picture(),
        Some(marked.to_string_lossy().into_owned())
    );
}

#[test]
fn the_key_hints_fit_the_width_dropping_the_least_useful_first() {
    let (_dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let _ = shown(&mut app, "D-B1", 150, 40);
    let wide = app.help_line(300, true);
    assert!(
        wide.contains("J/K scroll") && wide.contains("resize"),
        "{wide}"
    );
    // No photo, no photo keys.
    assert!(!wide.contains("photos"), "{wide}");
    let narrow = app.help_line(60, true);
    assert!(narrow.chars().count() <= 60, "{narrow}");
    assert!(
        narrow.starts_with("↑↓ move") && narrow.ends_with("q quit"),
        "{narrow}"
    );
    assert!(!narrow.contains("resize"), "{narrow}");
    // A status message stays.
    app.status = "saved".into();
    assert!(app.help_line(40, true).ends_with("    saved"));
}

#[test]
fn the_details_pane_scrolls_to_contents_below_its_edge() {
    let (_dir, mut inv) = home();
    add(&mut inv, "Oda", "room", "Ev", None);
    add(&mut inv, "Çekmece", "container", "Oda", Some("D"));
    for i in 0..40 {
        add(&mut inv, &format!("Parça {i:02}"), "item", "D", None);
    }
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    app.detail_tab = DetailTab::Contents;
    let s = shown(&mut app, "D", 150, 24);
    assert!(!s.contains("Parça 39"), "{s}");
    assert!(s.contains("H/L tabs · J/K scroll"), "{s}");
    for _ in 0..20 {
        press(&mut app, KeyCode::Char('J'));
    }
    let mut term = Terminal::new(TestBackend::new(150, 24)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("Parça 39"), "{s}");
    // Another node starts at the top again.
    let s = shown(&mut app, "Oda", 150, 24);
    assert!(s.contains(" H/L tabs ─"), "{s}");
}

#[test]
fn a_place_without_a_theme_shows_what_its_contents_share() {
    let (_dir, mut inv) = home();
    add(&mut inv, "Oda", "room", "Ev", None);
    add(&mut inv, "Kutu", "container", "Oda", Some("K"));
    add(&mut inv, "RP-SMA çubuk anten", "item", "K", None);
    add(&mut inv, "U.FL anten kablosu", "item", "K", None);
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    app.detail_tab = DetailTab::Suggestions;
    let s = shown(&mut app, "K", 150, 30);
    assert!(s.contains("No theme yet (ev themes)"), "{s}");
    assert!(s.contains("words: anten (2)"), "{s}");
    // A themed place shows no hints.
    app.inv.edit("K", &["theme=Antenler".into()]).unwrap();
    let s = shown(&mut app, "Oda", 150, 30);
    let s2 = shown(&mut app, "K", 150, 30);
    assert!(
        !s.contains("No theme yet") && !s2.contains("No theme yet"),
        "{s2}"
    );
}

#[test]
fn long_rows_end_in_an_ellipsis_and_times_read_as_ago() {
    crate::i18n::set_lang(Lang::En);
    let spans = vec![
        ratatui::text::Span::raw("abcdef"),
        ratatui::text::Span::raw("ghij"),
    ];
    let cut: String = super::fit(spans, 6)
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert_eq!(cut, "abcde…");
    assert_eq!(super::fill_bar(50), "▮▮▯▯");
    assert_eq!(super::fill_bar(95), "▮▮▮▮");
    let at = chrono::DateTime::parse_from_rfc3339("2026-09-29T09:00:00Z")
        .unwrap()
        .timestamp();
    assert_eq!(super::when("2026-09-29T09:00:00Z", at + 30), "just now");
    assert_eq!(super::when("2026-09-29T09:00:00Z", at + 7200), "2 h ago");
    let old = super::when("2026-09-29T09:00:00Z", at + 3 * 86_400);
    // Local date and time: the day holds for any zone within nine hours of UTC.
    assert!(old.starts_with("2026-09-29 "), "{old}");
}

#[test]
fn a_settings_change_made_elsewhere_shows_up_without_restarting() {
    let (dir, inv) = home();
    let path = dir.path().join("settings.json");
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    app.settings_path = Some(path.clone());
    app.reload_settings().unwrap();
    assert_eq!(crate::i18n::lang(), Lang::En);
    Settings {
        language: LangPref::Fixed(Lang::Tr),
        theme: ThemePref::Auto,
        ..Default::default()
    }
    .save_to(&path)
    .unwrap();
    app.reload_settings().unwrap();
    assert_eq!(crate::i18n::lang(), Lang::Tr);
    assert_eq!(Tab::Tree.title(), "Yerleşim");
}

fn screen(term: &Terminal<TestBackend>) -> String {
    let buf = term.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn the_selected_node_shows_its_photo_panel() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    inv.add(NewNode {
        name: "Ev".into(),
        kind: "home".into(),
        ..Default::default()
    })
    .unwrap();
    let img = image::RgbImage::from_pixel(64, 32, image::Rgb([200, 50, 50]));
    let photo = dir.path().join("p.png");
    img.save(&photo).unwrap();
    inv.photo_add("Ev", &photo, None, None).unwrap();
    let mut app = app_tr(inv);
    app.picker = Some(Picker::halfblocks());
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("Fotoğraf 1/1"), "{s}");
    assert!(
        s.contains("Özet · Fotoğraflar 1") && !s.contains("Izgara"),
        "{s}"
    );
}

#[test]
fn a_photo_is_decoded_off_the_loop_and_shown_when_it_is_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    inv.add(NewNode {
        name: "Ev".into(),
        kind: "home".into(),
        ..Default::default()
    })
    .unwrap();
    let photo = dir.path().join("p.png");
    image::RgbImage::from_pixel(64, 32, image::Rgb([200, 50, 50]))
        .save(&photo)
        .unwrap();
    inv.photo_add("Ev", &photo, None, None).unwrap();
    let mut app = app_tr(inv);
    app.picker = Some(Picker::halfblocks());
    let (wake, woken) = std::sync::mpsc::channel();
    app.decoder = Some(crate::ui::Decoder::start(wake));
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("fotoğraf açılıyor"));
    woken
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    app.take_decoded();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(!s.contains("fotoğraf açılıyor"), "{s}");
    assert_eq!(app.decoded.len(), 1);
}

#[test]
fn the_newest_photo_and_its_note_show_first_under_the_nodes_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    inv.add(NewNode {
        name: "Ev".into(),
        kind: "home".into(),
        ..Default::default()
    })
    .unwrap();
    let photo = dir.path().join("p.png");
    image::RgbImage::from_pixel(64, 32, image::Rgb([200, 50, 50]))
        .save(&photo)
        .unwrap();
    inv.photo_add("Ev", &photo, None, Some("before the tour"))
        .unwrap();
    inv.photo_add("Ev", &photo, None, Some("final state"))
        .unwrap();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    app.picker = Some(Picker::halfblocks());
    let mut term = Terminal::new(TestBackend::new(140, 40)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("Photo 2/2"), "{s}");
    assert!(s.contains("final state"), "{s}");
    assert!(s.contains("#1  Ev"), "{s}");
    // A step back reaches the older one.
    press(&mut app, KeyCode::Char('['));
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("before the tour"));
}

fn press(app: &mut App, c: KeyCode) {
    app.key(KeyEvent::new(c, KeyModifiers::NONE)).unwrap();
}

#[test]
fn o_opens_the_photo_full_screen_and_esc_closes_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    inv.add(NewNode {
        name: "Ev".into(),
        kind: "home".into(),
        ..Default::default()
    })
    .unwrap();
    let photo = dir.path().join("p.png");
    image::RgbImage::from_pixel(64, 32, image::Rgb([200, 50, 50]))
        .save(&photo)
        .unwrap();
    inv.photo_add("Ev", &photo, None, Some("ilk")).unwrap();
    inv.photo_add("Ev", &photo, None, Some("ikinci")).unwrap();
    let mut app = app_tr(inv);
    app.picker = Some(Picker::halfblocks());
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();

    press(&mut app, KeyCode::Char('o'));
    press(&mut app, KeyCode::Char(']'));
    press(&mut app, KeyCode::Char(']'));
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("Ev · Fotoğraf 2/2 · ikinci"), "{s}");
    assert!(!s.contains("Özet"), "{s}");

    press(&mut app, KeyCode::Esc);
    assert!(!app.quit);
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("Özet"));
}

#[test]
fn r_and_shift_r_rotate_the_full_screen_photo_for_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    inv.add(NewNode {
        name: "Ev".into(),
        kind: "home".into(),
        ..Default::default()
    })
    .unwrap();
    let photo = dir.path().join("p.png");
    image::RgbImage::from_pixel(64, 32, image::Rgb([200, 50, 50]))
        .save(&photo)
        .unwrap();
    inv.photo_add("Ev", &photo, None, None).unwrap();
    let mut app = app_tr(inv);
    app.picker = Some(Picker::halfblocks());
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();

    press(&mut app, KeyCode::Char('o'));
    let path = app.current_photo().unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    press(&mut app, KeyCode::Char('r'));
    assert_eq!(app.rotation.get(&path), Some(&1));
    term.draw(|f| app.draw(f)).unwrap();
    assert!(matches!(&app.shown, Some((_, 1, _, _))));

    press(&mut app, KeyCode::Char('R'));
    press(&mut app, KeyCode::Char('R'));
    assert_eq!(app.rotation.get(&path), Some(&3));
    term.draw(|f| app.draw(f)).unwrap();
    assert!(matches!(&app.shown, Some((_, 3, _, _))));
    assert!(screen(&term).contains("r/R döndür"));

    // Closing and reopening keeps the turn for the rest of the session.
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('o'));
    assert_eq!(app.rotation.get(&path), Some(&3));
}

#[test]
fn the_plan_tab_lists_tasks_with_progress() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Kutu", "container", Some("Oda")),
    ] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: parent.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    }
    inv.task_add("Kutuyu aç", "hiç açılmadı", &["Kutu".into()], None)
        .unwrap();
    let mut app = app_tr(inv);
    press(&mut app, KeyCode::Char('2'));
    let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("0/1 sayıldı"), "{s}");
    // Every place is on the not-counted list until counted, so it starts collapsed.
    assert!(s.contains("▸ HENÜZ SAYILMADI"), "{s}");
    assert!(s.contains("İŞLER"), "{s}");
    assert!(s.contains("1. Kutuyu aç"), "{s}");
    // The task's place is shown on the right: its name, and the place it is in.
    assert!(s.contains("Kutu") && s.contains("Ev › Oda"), "{s}");
}

#[test]
fn ev_focus_from_another_process_shows_the_node_and_photo() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ev.db");
    let mut inv = Inventory::open(&db).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Kutu", "container", Some("Oda")),
        ("Çekiç", "item", Some("Kutu")),
    ] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: parent.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    }
    let photo = dir.path().join("p.png");
    image::RgbImage::from_pixel(16, 16, image::Rgb([9, 9, 9]))
        .save(&photo)
        .unwrap();
    inv.photo_add("Çekiç", &photo, None, Some("yakın")).unwrap();
    let mut app = app_tr(inv);
    press(&mut app, KeyCode::Char('3'));
    assert!(!app.fullscreen);

    // The agent points at the hammer from its own connection; the request is a file beside the
    // database, read on the UI's next tick.
    let mut agent = Inventory::open(&db).unwrap();
    agent.focus(Some("Çekiç"), None).unwrap();
    app.apply_focus().unwrap();
    assert!(app.tab == Tab::Tree);
    let hammer = agent.resolve("Çekiç", false).unwrap();
    assert_eq!(app.selected_id(), Some(hammer));
    assert!(app.fullscreen);

    // Shown once: closing it does not bring it back on the next tick or refresh.
    press(&mut app, KeyCode::Esc);
    agent.observe("Kutu", "x", None).unwrap();
    app.refresh_if_changed().unwrap();
    app.apply_focus().unwrap();
    assert!(!app.fullscreen);
}

#[test]
fn a_search_can_be_cleared_without_quitting() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    inv.add(NewNode {
        name: "Ev".into(),
        kind: "home".into(),
        ..Default::default()
    })
    .unwrap();
    let mut app = app_tr(inv);
    let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
    press(&mut app, KeyCode::Char('/'));
    press(&mut app, KeyCode::Char('e'));
    press(&mut app, KeyCode::Char('v'));
    press(&mut app, KeyCode::Enter);
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("✕ temizle"));

    press(&mut app, KeyCode::Esc);
    assert!(!app.quit);
    assert!(app.query.is_empty() && app.rows.is_empty());
    term.draw(|f| app.draw(f)).unwrap();
    assert!(!screen(&term).contains("✕ temizle"));
    // With nothing left to clear, Esc quits as before.
    press(&mut app, KeyCode::Esc);
    assert!(app.quit);
}

#[test]
fn the_tree_reopens_with_its_nodes_open_and_headings_closed_as_left() {
    let (dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let led = app.inv.resolve("Kırmızı LED 10 mm", false).unwrap();
    // The drawer and its buzzer box opened down to the LED, and the Unknown place heading
    // closed; then ev ui left.
    app.reveal(led).unwrap();
    app.collapsed.insert(super::LOST_SECTION);
    let mut kept = app.tree_json();
    kept["expanded"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!(999_999));
    // The next session opens the same way; a node gone since is dropped.
    let inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    let mut again = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    assert!(!again.rows.iter().any(|r| r.id == led));
    again.apply_tree(&kept).unwrap();
    assert!(again.rows.iter().any(|r| r.id == led));
    assert!(again.collapsed.contains(&super::LOST_SECTION));
    assert!(!again.expanded.contains(&999_999));
}

#[test]
fn each_kind_has_its_own_mark_before_it_in_the_tree_and_the_search() {
    let (_dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let led = app.inv.resolve("Aktif buzzer", false).unwrap();
    app.reveal(led).unwrap();
    let mut term = Terminal::new(TestBackend::new(120, 20)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    // A box and the thing in it read apart, and so do the home and its room.
    for row in [
        "⌂ Ev",
        "◫ Oda",
        "□ D  Çekmece",
        "□ D-B1  Kutu",
        "· Aktif buzzer",
    ] {
        assert!(s.contains(row), "{row} in {s}");
    }
    press(&mut app, KeyCode::Char('/'));
    for c in "buzzer".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    press(&mut app, KeyCode::Enter);
    term.draw(|f| app.draw(f)).unwrap();
    assert!(
        screen(&term).contains("· Ev › Oda › D › D-B1 › Aktif buzzer"),
        "{}",
        screen(&term)
    );
}

/// The colour the screen shows `text` in, at its first cell.
fn fg_of(term: &Terminal<TestBackend>, text: &str) -> Option<Color> {
    let buf = term.backend().buffer();
    for y in 0..buf.area.height {
        let row: Vec<String> = (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect();
        if let Some(at) = row.concat().find(text) {
            let x = row.concat()[..at].chars().count() as u16;
            return Some(buf[(x, y)].fg);
        }
    }
    None
}

#[test]
fn a_counted_box_with_nothing_waiting_turns_green_and_its_holders_wait_for_all_of_it() {
    let (_dir, mut inv) = led_drawer();
    inv.label(&["D".into(), "D-A1".into(), "D-B1".into()], true)
        .unwrap();
    let tour = |inv: &mut Inventory| {
        for b in ["D", "D-A1", "D-B1"] {
            photo_now(inv, b);
        }
        for b in ["D-A1", "D-B1"] {
            inv.review(b, "toured", None).unwrap();
        }
    };
    tour(&mut inv);
    inv.move_to("Aktif buzzer", "D-A1", true).unwrap();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let settled = |app: &App, r: &str| {
        app.snap
            .settled
            .contains(&app.inv.resolve(r, false).unwrap())
    };
    // What stays put in a counted box is done; the thing on its way is not, nor either box
    // it moves between, nor anything holding them.
    assert!(settled(&app, "Pasif buzzer") && settled(&app, "Kırmızı LED 5 mm"));
    for r in ["Aktif buzzer", "D-A1", "D-B1", "D", "Oda", "Ev"] {
        assert!(!settled(&app, r), "{r}");
    }
    let pasif = app.inv.resolve("Pasif buzzer", false).unwrap();
    app.reveal(pasif).unwrap();
    // The selected row is drawn in the selection's colours; select the box they are in.
    let bx = app.inv.resolve("D-B1", false).unwrap();
    app.reveal(bx).unwrap();
    let mut term = Terminal::new(TestBackend::new(120, 20)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let done = theme::pal().done;
    assert_eq!(fg_of(&term, "Pasif buzzer"), Some(done));
    assert_ne!(fg_of(&term, "Aktif buzzer"), Some(done));
    // Moved, and both boxes counted again: everything up to the home is done.
    app.inv.done("Aktif buzzer").unwrap();
    tour(&mut app.inv);
    // Its own writes do not move the database's data version, so read it again here.
    app.snap = super::Snapshot::load(&app.inv).unwrap();
    app.rebuild().unwrap();
    for r in ["Aktif buzzer", "D-A1", "D-B1", "D", "Oda", "Ev"] {
        assert!(settled(&app, r), "{r}");
    }
    // The whole row turns green, the code with the name.
    term.draw(|f| app.draw(f)).unwrap();
    assert_eq!(fg_of(&term, "D-A1"), Some(done));
}

#[test]
fn a_counted_place_takes_no_word_and_one_changed_since_says_so() {
    let (_dir, mut inv) = led_drawer();
    inv.label(&["D".into(), "D-A1".into(), "D-B1".into()], true)
        .unwrap();
    for b in ["D", "D-A1", "D-B1"] {
        photo_now(&mut inv, b);
    }
    inv.review("D-A1", "toured", None).unwrap();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let bx = app.inv.resolve("D-B1", false).unwrap();
    app.reveal(bx).unwrap();
    let mut term = Terminal::new(TestBackend::new(120, 20)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    // Counted is the normal state: a green name, no word. The box not counted says so.
    assert!(!s.contains("[counted"), "{s}");
    assert!(s.contains("[not counted]"), "{s}");
    assert_eq!(fg_of(&term, "D-A1"), Some(theme::pal().done), "{s}");
    // Something new lands in the counted box: now it says it changed since.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    app.inv
        .add(NewNode {
            name: "Direnç".into(),
            kind: "item".into(),
            parent: Some("D-A1".into()),
            ..Default::default()
        })
        .unwrap();
    app.snap = super::Snapshot::load(&app.inv).unwrap();
    app.rebuild().unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("[counted, changed since]"), "{s}");
    assert_ne!(fg_of(&term, "Direnç"), Some(theme::pal().done));
}

#[test]
fn e_and_c_open_and_close_all_below_and_capital_c_closes_the_tree() {
    let (_dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let shown = |app: &App, r: &str| {
        let id = app.inv.resolve(r, false).unwrap();
        app.rows.iter().any(|row| row.id == id)
    };
    let first = app.rows.len();
    assert!(!shown(&app, "Pasif buzzer"));
    // `e` on the drawer opens it and every box in it, down to the things.
    let d = app.inv.resolve("D", false).unwrap();
    app.reveal(d).unwrap();
    press(&mut app, KeyCode::Char('e'));
    assert!(shown(&app, "Pasif buzzer") && shown(&app, "Kırmızı LED 5 mm"));
    assert_eq!(app.selected_id(), Some(d));
    // `c` closes it all again: nothing below the drawer shows, and it stays selected.
    press(&mut app, KeyCode::Char('c'));
    assert!(!shown(&app, "D-A1") && !shown(&app, "Pasif buzzer"));
    assert_eq!(app.selected_id(), Some(d));
    // `C` closes everything but the home: the room shows, closed, and the selection moves up
    // to it from the thing that is hidden now.
    app.reveal(d).unwrap();
    press(&mut app, KeyCode::Char('e'));
    assert!(app.rows.len() > first);
    let pasif = app.inv.resolve("Pasif buzzer", false).unwrap();
    app.reveal(pasif).unwrap();
    press(&mut app, KeyCode::Char('C'));
    let oda = app.inv.resolve("Oda", false).unwrap();
    assert!(shown(&app, "Oda") && !shown(&app, "D"));
    assert_eq!(app.selected_id(), Some(oda));
    // The keys are on the tree's bottom edge, as H/L are on the details'.
    let mut term = Terminal::new(TestBackend::new(200, 20)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    let edge = s.lines().rev().nth(1).unwrap_or_default();
    // At the tree's right end, against its corner.
    assert!(edge.contains("e/c all below · C close all ┘"), "{s}");
}

#[test]
fn d_opens_two_levels_and_no_further() {
    let (_dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let shown = |app: &App, r: &str| {
        let id = app.inv.resolve(r, false).unwrap();
        app.rows.iter().any(|row| row.id == id)
    };
    // From the room: the drawer and the boxes in it show, the things in the boxes do not.
    let oda = app.inv.resolve("Oda", false).unwrap();
    app.reveal(oda).unwrap();
    press(&mut app, KeyCode::Char('c'));
    assert!(!shown(&app, "D"));
    press(&mut app, KeyCode::Char('d'));
    assert!(shown(&app, "D") && shown(&app, "D-A1") && shown(&app, "D-B1"));
    assert!(!shown(&app, "Pasif buzzer"));
    assert_eq!(app.selected_id(), Some(oda));
}

#[test]
fn a_bought_thing_reads_in_sections_and_its_documents_open_from_their_own_tab() {
    let (dir, mut inv) = home();
    inv.add(NewNode {
        name: "Matkap".into(),
        kind: "item".into(),
        parent: Some("Ev".into()),
        note: Some("Şarj aleti ayrı kutuda".into()),
        ..Default::default()
    })
    .unwrap();
    inv.buy_add(
        &serde_json::json!({"name": "Bosch GSB 13 RE Darbeli Matkap 600 W", "shop": "Hırdavatçı",
                            "ordered_at": "2024-05-03", "paid": "1999.5", "currency": "TRY",
                            "order_url": "https://shop.example/orders/1"}),
        Some("Matkap"),
    )
    .unwrap();
    let pdf = dir.path().join("fatura.pdf");
    std::fs::write(&pdf, "%PDF qa").unwrap();
    inv.doc_add(
        &pdf,
        &ev_core::NewDoc {
            kind: "invoice".into(),
            ..Default::default()
        },
        &["Matkap".into()],
    )
    .unwrap();
    let mut app = app_tr(inv);
    let s = shown(&mut app, "Matkap", 150, 40);
    // Sections in the order a person asks; money the Turkish way; no empty tabs.
    let (money, note) = (s.find("Para ─").unwrap(), s.find("Not ─").unwrap());
    assert!(money < note, "{s}");
    assert!(s.contains("1.999,50 TL"), "{s}");
    assert!(s.contains("Belgeler 2") && !s.contains("Izgara"), "{s}");
    // The documents and the purchase's order page, the first one picked.
    app.detail_tab = DetailTab::Documents;
    let s = shown(&mut app, "Matkap", 150, 40);
    assert!(s.contains("▶ fatura"), "{s}");
    assert!(s.contains("↗ sipariş sayfası · Hırdavatçı"), "{s}");
    press(&mut app, KeyCode::Char(']'));
    assert_eq!(app.document_targets()[app.doc_idx], super::Target::Link(0));
    // `E` shows the identity still to fill; `+` widens the details and goes back.
    app.detail_tab = DetailTab::Summary;
    press(&mut app, KeyCode::Char('E'));
    let s = shown(&mut app, "Matkap", 150, 40);
    assert!(
        s.lines().any(|l| l.contains("marka") && l.contains("—")),
        "{s}"
    );
    let before = app.split;
    press(&mut app, KeyCode::Char('+'));
    assert_eq!(app.split, 20);
    press(&mut app, KeyCode::Char('+'));
    assert_eq!(app.split, before);
}

#[test]
fn a_thing_in_several_places_shows_them_and_p_goes_from_one_place_to_the_next() {
    let (_dir, mut inv) = home();
    for (name, kind, parent, qty) in [
        ("Kutu", "container", "Ev", None),
        ("El feneri", "item", "Ev", None),
        ("Eneloop pil", "item", "Kutu", Some(10)),
    ] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: Some(parent.into()),
            qty,
            ..Default::default()
        })
        .unwrap();
    }
    inv.move_qty("Eneloop pil", "El feneri", false, Some(3))
        .unwrap();
    let mut app = app_tr(inv);
    let s = shown(&mut app, "#4", 150, 40);
    assert!(s.contains("×10, 2 yerde"), "{s}");
    assert!(s.contains("başka yerde"), "{s}");
    let id = |app: &App| app.details.as_ref().unwrap()["node"]["id"].as_i64();
    assert_eq!(id(&app), Some(4));
    press(&mut app, KeyCode::Char('p'));
    assert_eq!(id(&app), Some(5));
    press(&mut app, KeyCode::Char('p'));
    assert_eq!(id(&app), Some(4));
}

#[test]
fn product_images_show_on_the_photos_tab_after_the_persons_photos_never_as_one() {
    let (dir, mut inv) = home();
    inv.add(NewNode {
        name: "Matkap".into(),
        kind: "item".into(),
        parent: Some("Ev".into()),
        ..Default::default()
    })
    .unwrap();
    let photo = dir.path().join("p.png");
    image::RgbImage::from_pixel(64, 32, image::Rgb([200, 50, 50]))
        .save(&photo)
        .unwrap();
    inv.photo_add("Matkap", &photo, None, Some("rafta"))
        .unwrap();
    let picture = dir.path().join("urun.png");
    image::RgbImage::from_pixel(32, 32, image::Rgb([1, 2, 3]))
        .save(&picture)
        .unwrap();
    inv.doc_add(
        &picture,
        &ev_core::NewDoc {
            kind: "image".into(),
            note: Some("kutu önü".into()),
            ..Default::default()
        },
        &["Matkap".into()],
    )
    .unwrap();
    let mut app = app_tr(inv);
    app.picker = Some(Picker::halfblocks());
    let s = shown(&mut app, "Matkap", 150, 40);
    // The person's photo is the one shown; the tab counts the two apart; no Documents tab.
    assert!(s.contains("Fotoğraf 1/1 · rafta"), "{s}");
    assert!(
        s.contains("Fotoğraflar 1+1") && !s.contains("Belgeler"),
        "{s}"
    );
    app.detail_tab = DetailTab::Photos;
    let s = shown(&mut app, "Matkap", 150, 40);
    assert!(
        s.contains("Fotoğraflar (1)") && s.contains("Ürün görselleri (1)"),
        "{s}"
    );
    // `]` steps on to the product image, shown inside ev ui and full screen with `o`.
    press(&mut app, KeyCode::Char(']'));
    let mut term = Terminal::new(TestBackend::new(150, 40)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("Ürün görseli 1/1 · kutu önü"), "{s}");
    press(&mut app, KeyCode::Char('o'));
    assert!(app.fullscreen);
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("Matkap · Ürün görseli 1/1"), "{s}");
}

#[test]
fn eight_opens_the_statistics_with_a_section_per_heading() {
    let (_dir, inv) = led_drawer();
    let mut app = app_tr(inv);
    press(&mut app, KeyCode::Char('8'));
    assert!(app.tab == Tab::Stats);
    let mut term = Terminal::new(TestBackend::new(150, 50)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("8 İstatistik"), "{s}");
    assert!(s.contains("GENEL") && s.contains("SAYIM"), "{s}");
    assert!(s.contains("eşya kaydı"), "{s}");
    // The first heading closes with Enter, and its lines go.
    let first = app.rows.iter().position(|r| r.id < 0).unwrap();
    app.select(first).unwrap();
    press(&mut app, KeyCode::Enter);
    term.draw(|f| app.draw(f)).unwrap();
    assert!(!screen(&term).contains("eşya kaydı"));
}

/// A small picture on disk, as `photo mark` leaves one.
fn picture(dir: &tempfile::TempDir, name: &str) -> std::path::PathBuf {
    let p = dir.path().join(name);
    image::RgbImage::from_pixel(8, 8, image::Rgb([230, 30, 30]))
        .save(&p)
        .unwrap();
    p
}

#[test]
fn marked_photos_join_the_series_and_x_ends_it() {
    let (dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let (a, b) = (picture(&dir, "a.png"), picture(&dir, "b.png"));
    app.inv
        .focus_file(std::slice::from_ref(&a), Some("parts"))
        .unwrap();
    app.apply_focus().unwrap();
    app.inv.focus_file(&[b], Some("drawer")).unwrap();
    app.apply_focus().unwrap();
    // The second joins the first and is the one shown, under its own note.
    let mut term = Terminal::new(TestBackend::new(100, 20)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(
        s.contains("f2/2 · drawer") && s.contains("X close series"),
        "{s}"
    );
    press(&mut app, KeyCode::Char('['));
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("f1/2 · parts"));
    // Esc only hides it: `m` brings the whole series back.
    press(&mut app, KeyCode::Esc);
    assert!(app.overlay.is_none());
    press(&mut app, KeyCode::Char('m'));
    assert_eq!(app.overlay.as_ref().unwrap().files.len(), 2);
    // X ends it, on screen and in the request, and what comes next starts another.
    press(&mut app, KeyCode::Char('X'));
    assert!(app.overlay.is_none() && app.last_overlay.is_none());
    assert!(app.inv.focus_request().unwrap().is_null());
    assert_eq!(app.status, "marked photo series closed");
    app.inv.focus_file(&[a], Some("next")).unwrap();
    app.apply_focus().unwrap();
    assert_eq!(app.overlay.as_ref().unwrap().files.len(), 1);
}

#[test]
fn a_series_the_agent_ends_leaves_the_screen() {
    let (dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    app.inv
        .focus_file(&[picture(&dir, "a.png")], Some("parts"))
        .unwrap();
    app.apply_focus().unwrap();
    assert!(app.overlay.is_some());
    app.inv.focus(None, None).unwrap();
    app.apply_focus().unwrap();
    assert!(app.overlay.is_none() && app.last_overlay.is_none());
    assert!(!app.fullscreen);
}

#[test]
fn g_shows_the_series_as_a_grid_moved_through_by_arrows_and_f_numbers() {
    let (dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let files: Vec<(std::path::PathBuf, Option<String>)> = (1..=5)
        .map(|i| {
            (
                picture(&dir, &format!("p{i}.png")),
                Some(format!("kutu {i}")),
            )
        })
        .collect();
    app.inv.focus_noted(&files, None).unwrap();
    app.apply_focus().unwrap();
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Char('g'));
    assert!(app.series_grid);
    // At 100 columns and 28-cell pictures, three a row; every tile titled `f… · note`.
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(
        s.contains("f1 · kutu 1") && s.contains("f5 · kutu 5"),
        "{s}"
    );
    assert_eq!(app.grid_cols, 3);
    let at = |app: &App| app.overlay.as_ref().unwrap().at;
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Down);
    assert_eq!(at(&app), 4);
    press(&mut app, KeyCode::Home);
    assert_eq!(at(&app), 0);
    // `f`, digits, Enter: straight to that picture.
    for k in [KeyCode::Char('f'), KeyCode::Char('4'), KeyCode::Enter] {
        press(&mut app, k);
    }
    assert_eq!(at(&app), 3);
    // `+` widens the pictures for now; Enter opens the one selected on its own.
    press(&mut app, KeyCode::Char('+'));
    assert_eq!(app.tile, Some(32));
    press(&mut app, KeyCode::Enter);
    assert!(!app.series_grid);
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("f4/5 · kutu 4"));
}

#[test]
fn a_click_on_the_series_grid_opens_the_picture_under_it() {
    let (dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let files: Vec<(std::path::PathBuf, Option<String>)> = (1..=5)
        .map(|i| {
            (
                picture(&dir, &format!("p{i}.png")),
                Some(format!("kutu {i}")),
            )
        })
        .collect();
    app.inv.focus_noted(&files, None).unwrap();
    app.apply_focus().unwrap();
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Char('g'));
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    // The wheel moves a row: three pictures a row, from f1 to f4.
    click(&mut app, MouseEventKind::ScrollDown, 50, 10);
    assert_eq!(app.overlay.as_ref().unwrap().at, 3);
    // A click on the second row's second picture (f5) opens it on its own.
    let (r, _) = app.grid_hits[4];
    click(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        r.x + 2,
        r.y + 1,
    );
    assert!(!app.series_grid);
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("f5/5 · kutu 5"));
}

#[test]
fn seven_opens_the_past_by_year_apart_from_the_inventory() {
    let (_dir, mut inv) = home();
    inv.add(NewNode {
        name: "Oyun konsolu".into(),
        kind: "item".into(),
        gone: Some("sell".into()),
        at: Some("2019-05".into()),
        came: Some("2016".into()),
        place: Some("Eski ev".into()),
        ..Default::default()
    })
    .unwrap();
    inv.sold("Oyun konsolu", "1500", None, None, None, None)
        .unwrap();
    let mut app = app_tr(inv);
    // Never in the tree.
    let mut term = Terminal::new(TestBackend::new(150, 40)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(
        s.contains("7 Gidenler") && !s.contains("Oyun konsolu"),
        "{s}"
    );
    press(&mut app, KeyCode::Char('7'));
    assert!(app.tab == Tab::Past);
    let thing = app.rows.iter().position(|r| r.id > 0).unwrap();
    app.select(thing).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("Hatırlananlar (1)"), "{s}");
    assert!(s.contains("2019") && s.contains("1 gitti"), "{s}");
    assert!(s.contains("Oyun konsolu") && s.contains("Eski ev"), "{s}");
    // The details say when it came and how it left.
    assert!(
        s.contains("geldi  2016") && s.contains("1.500,00 TL"),
        "{s}"
    );
    // Enter on it stays here: a past thing is in no tree.
    press(&mut app, KeyCode::Enter);
    assert!(app.tab == Tab::Past);
    // The year closes with Enter, and its things go.
    let year = app.rows.iter().position(|r| r.id < 0).unwrap();
    app.select(year).unwrap();
    press(&mut app, KeyCode::Enter);
    assert!(app.rows.iter().all(|r| r.id <= 0));
}

/// A whole photo of `place` taken now, so a tour of it finds its photo current.
fn photo_now(inv: &mut Inventory, place: &str) {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("now.png");
    // A picture of its own for each place, as a camera gives.
    let shade = place
        .bytes()
        .fold(0u8, |a, b| a.wrapping_mul(31).wrapping_add(b));
    image::RgbImage::from_pixel(8, 8, image::Rgb([shade, 2, 3]))
        .save(&png)
        .unwrap();
    inv.photo_add_with(place, &png, None, None, true).unwrap();
}

#[test]
fn the_sidebar_lists_every_list_under_its_heading_with_digit_and_count() {
    let (_dir, mut inv) = led_drawer();
    inv.mark_lost("Aktif buzzer").unwrap();
    inv.mark_lost("Pasif buzzer").unwrap();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let mut term = Terminal::new(TestBackend::new(140, 24)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    // The sidebar's own text on the line that holds `label`.
    let side = |label: &str| {
        let l = s.lines().find(|l| l.contains(label)).unwrap_or_default();
        l.split('│').nth(1).unwrap_or_default().to_string()
    };
    for heading in ["HOME", "HISTORY", "INSIGHT"] {
        assert!(s.contains(heading), "{s}");
    }
    // The count is the list's own length, at the sidebar's right edge.
    assert!(side(" 5 Lost").ends_with(" 2"), "{s}");
    assert!(side(" 3 Pending").ends_with(" 0"), "{s}");
    // The tree has no single count.
    assert!(side(" 1 Layout").trim_end().ends_with("Layout"), "{s}");
    assert!(s.contains("/ Search") && s.contains("0 Settings"), "{s}");
    // No tab bar: the top line says where you are.
    assert!(s.lines().next().unwrap().contains("HOME › Layout"), "{s}");
    press(&mut app, KeyCode::Char('5'));
    term.draw(|f| app.draw(f)).unwrap();
    assert!(
        screen(&term)
            .lines()
            .next()
            .unwrap()
            .contains("HOME › Lost · 2")
    );
}

#[test]
fn tab_moves_the_keys_between_the_panes_and_the_sidebar_opens_lists_as_it_moves() {
    let (_dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let mut term = Terminal::new(TestBackend::new(140, 24)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    assert_eq!(app.pane, Pane::List);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.pane, Pane::Details);
    // Esc in the details goes back to the list, it does not quit.
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.pane, Pane::List);
    assert!(!app.quit);
    press(&mut app, KeyCode::BackTab);
    assert_eq!(app.pane, Pane::Sidebar);
    // Moving in the sidebar opens the next list at once; the ends hold.
    press(&mut app, KeyCode::Down);
    assert!(app.tab == Tab::Plan);
    press(&mut app, KeyCode::Char('k'));
    press(&mut app, KeyCode::Char('k'));
    assert!(app.tab == Tab::Tree);
    press(&mut app, KeyCode::End);
    assert!(app.tab == Tab::Settings);
    // Enter goes into the list, and its keys work there again.
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.pane, Pane::List);
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("Language:"));
}

#[test]
fn the_sidebar_narrows_to_a_rail_hides_opens_as_a_drawer_and_a_phone_sees_one_pane() {
    let (_dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let draw = |app: &mut App, w: u16| {
        let mut term = Terminal::new(TestBackend::new(w, 24)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        screen(&term)
    };
    // Under 120 columns: digits and counts only.
    let s = draw(&mut app, 100);
    assert_eq!(app.sidebar_area.width, 7);
    assert!(!s.contains("Layout │") && !s.contains(" Lists "), "{s}");
    // Under 90: hidden, and `b` lays it over the list until a list is chosen.
    draw(&mut app, 80);
    assert_eq!(app.sidebar_area.width, 0);
    press(&mut app, KeyCode::Char('b'));
    let s = draw(&mut app, 80);
    assert_eq!((app.sidebar_area.width, app.pane), (24, Pane::Sidebar));
    assert!(s.contains(" 4 Leaving"), "{s}");
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    draw(&mut app, 80);
    assert!(app.tab == Tab::Plan);
    assert_eq!((app.sidebar_area.width, app.pane), (0, Pane::List));
    // Under 70, as on a phone: the focused pane alone, the whole width.
    draw(&mut app, 60);
    assert_eq!((app.list_area.width, app.right_area.width), (60, 0));
    press(&mut app, KeyCode::Tab);
    draw(&mut app, 60);
    assert_eq!((app.list_area.width, app.right_area.width), (0, 60));
    press(&mut app, KeyCode::Tab);
    let s = draw(&mut app, 60);
    assert_eq!(app.sidebar_area.width, 60);
    assert!(s.contains(" 2 To do"), "{s}");
}

#[test]
fn b_hides_the_sidebar_and_ui_state_keeps_the_choice() {
    let (_dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let mut term = Terminal::new(TestBackend::new(140, 24)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    assert_eq!(app.layout_json()["sidebar"], serde_json::Value::Null);
    press(&mut app, KeyCode::Char('b'));
    term.draw(|f| app.draw(f)).unwrap();
    assert_eq!(app.sidebar_area.width, 0);
    assert_eq!(app.layout_json()["sidebar"], false);
    // Tab passes a hidden sidebar by.
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.pane, Pane::List);
    // The next `ev ui` starts with it hidden.
    app.sidebar = None;
    app.apply_layout(&serde_json::json!({"sidebar": false}));
    assert_eq!(app.sidebar, Some(false));
}

fn type_in(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

#[test]
fn colon_finds_a_list_without_turkish_letters_and_esc_comes_back() {
    let (_dir, mut inv) = led_drawer();
    inv.mark_lost("Aktif buzzer").unwrap();
    let mut app = app_tr(inv);
    let mut term = Terminal::new(TestBackend::new(140, 24)).unwrap();
    press(&mut app, KeyCode::Char(':'));
    type_in(&mut app, "kayip");
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains(": kayip") && s.contains("EVDE › Kayıp"), "{s}");
    press(&mut app, KeyCode::Enter);
    assert!(app.tab == Tab::Lost && app.palette.is_none());
    term.draw(|f| app.draw(f)).unwrap();
    // The top line says where Esc goes.
    assert!(
        screen(&term)
            .lines()
            .next()
            .unwrap()
            .contains("Esc ‹ Yerleşim")
    );
    press(&mut app, KeyCode::Esc);
    assert!(app.tab == Tab::Tree && !app.quit);
}

#[test]
fn colon_finds_a_thing_by_name_or_id_and_opens_it_in_the_tree() {
    let (_dir, inv) = led_drawer();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    let buzzer = app.inv.resolve("Pasif buzzer", false).unwrap();
    press(&mut app, KeyCode::Char('3'));
    // By a word inside the name, with the place it is in beside it.
    press(&mut app, KeyCode::Char(':'));
    type_in(&mut app, "pasif");
    let hits = &app.palette.as_ref().unwrap().hits;
    assert!(
        hits[0].1.contains("Pasif buzzer") && hits[0].1.contains("D-B1"),
        "{hits:?}"
    );
    press(&mut app, KeyCode::Enter);
    assert!(app.tab == Tab::Tree);
    assert_eq!(app.selected_id(), Some(buzzer));
    // Esc goes back to the list it was opened from.
    press(&mut app, KeyCode::Esc);
    assert!(app.tab == Tab::Pending);
    // By its #id; Esc closes the palette without going anywhere.
    press(&mut app, KeyCode::Char(':'));
    type_in(&mut app, &format!("#{buzzer}"));
    assert_eq!(
        app.palette.as_ref().unwrap().hits[0].0,
        super::palette::Goal::Node(buzzer)
    );
    press(&mut app, KeyCode::Esc);
    assert!(app.palette.is_none() && app.tab == Tab::Pending);
}

#[test]
fn a_record_that_leaves_the_list_hands_the_selection_to_its_neighbour_and_says_so() {
    let (dir, mut inv) = led_drawer();
    inv.move_to("Aktif buzzer", "D-A1", true).unwrap();
    inv.move_to("Pasif buzzer", "D-A1", true).unwrap();
    let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    press(&mut app, KeyCode::Char('3'));
    press(&mut app, KeyCode::End);
    let last = app.selected_id().unwrap();
    // The agent makes that move from its own connection.
    let mut agent = Inventory::open(&dir.path().join("ev.db")).unwrap();
    agent.done(&format!("#{last}")).unwrap();
    app.refresh_if_changed().unwrap();
    assert!(app.tab == Tab::Pending);
    assert_eq!(app.rows.len(), 1);
    assert_eq!(app.state.selected(), Some(0));
    assert_eq!(app.status, format!("#{last} left this list"));
    assert_eq!(app.counts[&Tab::Pending], 1);
}

/// One order of two durable lines, the first linked to a LED; a digital line on its own.
fn with_purchases() -> (tempfile::TempDir, App) {
    let (dir, mut inv) = led_drawer();
    let lines = [
        serde_json::json!({"type": "purchase", "source": "shop", "key": "o1:a", "shop": "Shop",
            "order": "o1", "name": "LED seti", "ordered_at": "2024-05-01", "qty": 1,
            "paid": "1999.00", "currency": "TRY"}),
        serde_json::json!({"type": "purchase", "source": "shop", "key": "o1:b", "shop": "Shop",
            "order": "o1", "name": "Buzzer", "ordered_at": "2024-05-01", "qty": 2,
            "paid": "800.00", "currency": "TRY"}),
        serde_json::json!({"type": "purchase", "source": "shop", "key": "o2:c", "shop": "Store",
            "order": "o2", "name": "Bir oyun", "ordered_at": "2024-06-01", "qty": 1,
            "paid": "99.00", "currency": "TRY", "bucket": "digital"}),
    ]
    .iter()
    .map(serde_json::Value::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    inv.buy_import(&lines).unwrap();
    let set = inv
        .buy_list_matching(false, None, None, None, Some("LED seti"))
        .unwrap();
    let id = set["purchases"][0]["id"].as_i64().unwrap();
    inv.buy_link(id, "Kırmızı LED 5 mm", None).unwrap();
    let app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
    (dir, app)
}

#[test]
fn nine_lists_every_purchase_line_with_an_order_under_its_heading_and_the_totals() {
    let (_dir, mut app) = with_purchases();
    let mut term = Terminal::new(TestBackend::new(170, 30)).unwrap();
    press(&mut app, KeyCode::Char('9'));
    assert!(app.tab == Tab::Buys);
    term.draw(|f| app.draw(f)).unwrap();
    let s = screen(&term);
    assert!(s.contains("All · 3 lines · 2898.00 TRY · all"), "{s}");
    // The order's two lines under one heading with its total; the other line on its own.
    assert!(s.contains("order o1 · 2 lines · 2799.00 TRY"), "{s}");
    assert!(s.contains("Store  Bir oyun  99.00 TRY"), "{s}");
    assert!(
        s.contains("LED seti  1999.00 TRY  → Kırmızı LED 5 mm"),
        "{s}"
    );
    // Each bucket is a list of its own, counted in the sidebar.
    assert_eq!(
        (app.counts[&Tab::Buys], app.counts[&Tab::BuysDurable]),
        (3, 2)
    );
    assert_eq!(app.counts[&Tab::BuysDigital], 1);
    // The details are the line's, as `ev buy show` writes them.
    let line = app.rows.iter().position(|r| r.id > 0).unwrap();
    app.select(line).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    assert!(screen(&term).contains("Purchase #"));
}

#[test]
fn f_and_slash_filter_a_purchase_list_and_one_esc_clears_both() {
    let (_dir, mut app) = with_purchases();
    press(&mut app, KeyCode::Char('9'));
    let lines = |app: &App| app.rows.iter().filter(|r| r.id > 0).count();
    // `f`: the lines still open (a digital line has nothing to link), then those linked.
    press(&mut app, KeyCode::Char('f'));
    assert_eq!(lines(&app), 1);
    assert!(app.purchase_title.contains("open"));
    press(&mut app, KeyCode::Char('f'));
    assert_eq!(lines(&app), 1);
    assert!(app.purchase_title.contains("linked"));
    press(&mut app, KeyCode::Char('f'));
    press(&mut app, KeyCode::Char('f'));
    // `/`: the list follows the words as they are typed, the global search is untouched.
    press(&mut app, KeyCode::Char('/'));
    type_in(&mut app, "buz");
    assert_eq!(lines(&app), 1);
    press(&mut app, KeyCode::Enter);
    assert!(app.tab == Tab::Buys && app.query.is_empty());
    press(&mut app, KeyCode::Char('f'));
    assert_eq!(lines(&app), 1);
    // One Esc clears both filters; the next goes back.
    press(&mut app, KeyCode::Esc);
    assert_eq!(lines(&app), 3);
    assert!(app.tab == Tab::Buys);
    press(&mut app, KeyCode::Esc);
    assert!(app.tab == Tab::Tree && !app.quit);
}

#[test]
fn enter_on_a_linked_purchase_line_opens_its_thing_in_the_tree() {
    let (_dir, mut app) = with_purchases();
    let led = app.inv.resolve("Kırmızı LED 5 mm", false).unwrap();
    press(&mut app, KeyCode::Char('9'));
    let at = |app: &App, name: &str| {
        app.rows
            .iter()
            .position(|r| r.spans.iter().any(|s| s.content.contains(name)))
            .unwrap()
    };
    // A line linked to nothing stays, and says so.
    app.select(at(&app, "Bir oyun")).unwrap();
    press(&mut app, KeyCode::Enter);
    assert!(app.tab == Tab::Buys);
    assert_eq!(app.status, "this line is linked to no thing here");
    app.select(at(&app, "LED seti")).unwrap();
    press(&mut app, KeyCode::Enter);
    assert!(app.tab == Tab::Tree);
    assert_eq!(app.selected_id(), Some(led));
    press(&mut app, KeyCode::Esc);
    assert!(app.tab == Tab::Buys);
}

#[test]
fn enter_on_a_shops_figure_opens_its_purchase_lines_and_esc_comes_back() {
    let (_dir, mut app) = with_purchases();
    press(&mut app, KeyCode::Char('8'));
    let shop = app
        .rows
        .iter()
        .position(|r| r.spans.iter().any(|s| s.content.starts_with("Store:")))
        .unwrap();
    // The figure says it has a list behind it.
    assert!(app.rows[shop].spans.iter().any(|s| s.content.contains('›')));
    app.select(shop).unwrap();
    press(&mut app, KeyCode::Enter);
    assert!(app.tab == Tab::Buys);
    let lines: Vec<i64> = app.rows.iter().map(|r| r.id).filter(|&id| id > 0).collect();
    assert_eq!(lines.len(), 1);
    assert!(
        app.purchase_title.contains("· Store "),
        "{}",
        app.purchase_title
    );
    // Esc goes straight back to the figure, and the shop narrows nothing any more.
    press(&mut app, KeyCode::Esc);
    assert!(app.tab == Tab::Stats);
    assert!(app.buy_shop.is_none());
    press(&mut app, KeyCode::Char('9'));
    assert_eq!(app.rows.iter().filter(|r| r.id > 0).count(), 3);
}
