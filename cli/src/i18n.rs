//! The words people read, in English or Turkish.
//!
//! The English text is the key: `t("Layout")` returns it unchanged in English and looks up the
//! translation otherwise, falling back to English when one is missing. `tf` fills `{}`
//! placeholders in order, after translating, so a language may move words around a value but
//! keeps the same number of placeholders (a test checks that). Another language is one more
//! table and one more `Lang` variant.

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Tr,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Tr => "tr",
        }
    }

    /// The name of the language in itself, as a language menu shows it.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Tr => "Türkçe",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        match s {
            "en" => Some(Lang::En),
            "tr" => Some(Lang::Tr),
            _ => None,
        }
    }

    /// A BCP 47 tag such as `tr-TR` or `en-TR`: only the language part counts, so an English
    /// system set to the Turkish region stays English.
    pub fn from_tag(tag: &str) -> Lang {
        let lang = tag
            .split(['-', '_', '.'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        Lang::from_code(&lang).unwrap_or(Lang::En)
    }
}

/// The computer's preferred language, English when it is not one ev speaks. Read once.
///
/// `sys-locale` asks the system: on macOS the first of System Settings' preferred languages
/// (a terminal's `LANG` is often `en_US.UTF-8` whatever the system says), elsewhere the
/// locale variables.
pub fn system_lang() -> Lang {
    static CACHE: OnceLock<Lang> = OnceLock::new();
    // Tests choose their language; asking macOS costs seconds there (it reads the folder the
    // program is in, and a test program's holds tens of thousands of files), in every test.
    *CACHE.get_or_init(|| {
        if cfg!(test) {
            Lang::En
        } else {
            asked_system_lang()
        }
    })
}

fn asked_system_lang() -> Lang {
    sys_locale::get_locale()
        .map(|tag| Lang::from_tag(&tag))
        .unwrap_or(Lang::En)
}

thread_local! {
    static LANG: Cell<Lang> = const { Cell::new(Lang::En) };
}

pub fn set_lang(l: Lang) {
    LANG.with(|c| c.set(l));
}

pub fn lang() -> Lang {
    LANG.with(Cell::get)
}

fn turkish() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| TR.iter().copied().collect())
}

/// The text in the current language.
pub fn t(en: &'static str) -> &'static str {
    match lang() {
        Lang::En => en,
        Lang::Tr => turkish().get(en).copied().unwrap_or(en),
    }
}

/// The text in the current language with its `{}` placeholders filled in order.
pub fn tf(en: &'static str, args: &[&dyn Display]) -> String {
    let template = t(en);
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut rest = template;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(a) = args.next() {
            out.push_str(&a.to_string());
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

/// An error with an id in the current language (spec/error-ids.md): its Turkish sentence by
/// the id, else the English one, filled with its values, with where it happened in front.
pub fn error_sentence(s: &ev_core::Said) -> String {
    let template = match lang() {
        Lang::Tr => ERRORS_TR
            .iter()
            .find(|(id, _)| *id == s.id)
            .map(|(_, t)| *t),
        Lang::En => None,
    }
    .or_else(|| ev_core::template(s.id))
    .unwrap_or(s.id);
    // A value that is a word of ev's own (a kind, a bucket, why a line was dismissed) is said
    // in the reader's language too.
    let mut values = s.values.clone();
    if lang() == Lang::Tr {
        for key in ["kind", "bucket", "as", "status"] {
            if let Some(serde_json::Value::String(word)) = values.get(key)
                && let Some(tr) = turkish().get(word.as_str())
            {
                values.insert(key.into(), serde_json::json!(tr));
            }
        }
    }
    let mut out = ev_core::fill(template, &values);
    for a in &s.at {
        let at = match &a["line"] {
            serde_json::Value::Number(n) => tf("line {}", &[n]),
            _ => ev_core::at_text(a),
        };
        out = format!("{at}: {out}");
    }
    out
}

/// The Turkish sentence of every error with an id, by the id; the placeholders are the English
/// template's, by name.
static ERRORS_TR: &[(&str, &str)] = &[
    (
        "no_record_matches",
        "`{ref}` ile eşleşen bir kayıt yok; `ev find` ile ara, sonra id ile yeniden dene",
    ),
    ("no_record_with_id", "{id} numaralı bir kayıt yok"),
    (
        "ref_matches_several",
        "`{ref}` {count} kayıtla eşleşiyor; id ile yeniden dene",
    ),
    (
        "record_gone",
        "#{id} artık burada değil; `ev show {id} --include-gone` ya da `ev history {id}` onu yine bulur",
    ),
    (
        "record_joined",
        "#{id}, #{into} kaydına katıldı; artık orada sayılıyor",
    ),
    (
        "split_holds_things",
        "{node} içinde {inside} şey var; içindekileri böl, önce dışarı taşı ya da --take ile boş birimleri ayır",
    ),
    (
        "split_takes_all",
        "parçalar {node} kaydının {had} biriminden {take} tanesini alıyor; geriye bir şey kalmaz: bunun yerine bir parçayı --rename ve --qty ile asıl kayıt olarak tut",
    ),
    (
        "already_candidate",
        "{node} zaten aday; `ev gone` ya da `ev restore` kullan",
    ),
    (
        "restore_gone_by_correction",
        "#{id} artık burada değil; `ev restore {id} --correction \"neden\"` bunu geri alır",
    ),
    ("not_a_candidate", "{node} aday değil"),
    (
        "gone_needs_how",
        "{node} hâlâ evde; nasıl gittiğini --as trash|give|sell|trade|used|digitize|left|stolen|unknown ile söyle",
    ),
    ("already_lost", "{node} zaten kayıp"),
    ("home_cannot_be_lost", "ev kaybolamaz"),
    (
        "vehicle_cannot_be_lost",
        "araç kaybolamaz; çalınan araç elden çıkmıştır (--as stolen)",
    ),
    (
        "coverage_edit_nothing",
        "neyin düzeltileceğini söyleyin: --for, --off ya da issuer=, number=, term=, ends=, premium=, deductible=, currency=, scope=, note=",
    ),
    (
        "coverage_edit_field_bad",
        "`{field}` alan=değer biçiminde değil",
    ),
    (
        "coverage_edit_field_unknown",
        "`{field}` düzeltilemez; şunlardan birini kullanın: {fields}",
    ),
    (
        "coverage_on_nothing",
        "{id} numaralı kapsam hiçbir kayda bağlı kalmazdı; bir hataysa ev cover remove ile kaldırın",
    ),
    (
        "purchase_edit_not_manual",
        "{id} numaralı alım {source} kaynağından geldi; orada düzeltip yeniden içe aktarın",
    ),
    (
        "purchase_edit_nothing",
        "neyin düzeltileceğini söyleyin: name=, date=, paid=, currency=, shop=, brand=, order= ya da qty=",
    ),
    (
        "purchase_edit_field_bad",
        "`{field}` alan=değer biçiminde değil",
    ),
    (
        "purchase_edit_field_unknown",
        "`{field}` düzeltilemez; şunlardan birini kullanın: {fields}",
    ),
    (
        "purchase_qty_below_linked",
        "{id} numaralı alımın {linked} birimi eşyalara bağlı; {qty} adet bundan az",
    ),
    (
        "place_is_a_household",
        "{place} için {count} iş var: orası başka bir hane, bizim eski evimiz değil",
    ),
    (
        "home_leaves_moved_or_sold",
        "bir ev taşınılarak (--as moved), satılarak (--as sell) ya da kayıt hatası olarak bırakılır, {way} olarak değil",
    ),
    (
        "moved_is_a_homes",
        "--as moved evler içindir; taşınırken geride kalan bir {kind} için --as left kullanın",
    ),
    (
        "leaving_still_holds",
        "{node} odaları ya da bölmeleri dışında hâlâ {count} kayıt tutuyor; önce her birini gideceği yere taşıyın ya da geride kaldığını söyleyin (--as left)",
    ),
    (
        "vehicle_inside",
        "araç evlerin yanında en üstte durur, hiçbir kaydın içine konamaz",
    ),
    ("not_lost", "{node} kayıp değil"),
    (
        "never_seen",
        "{node} hiçbir yerde görülmedi; nerede çıktığını `ev found <ref> --in <yer>` ile söyle",
    ),
    ("no_pending_move", "{node} için bekleyen bir taşıma yok"),
    (
        "already_there",
        "{node} zaten {place} içinde; onun içindeki bir yer (bir göz) ya bir ızgara hücresidir (`ev grid`, `ev cell`) ya da kendi başına bir kap",
    ),
    (
        "move_already_pending",
        "{node} için zaten bekleyen bir taşıma var; önce onu iptal et",
    ),
    (
        "code_only_digits",
        "`{code}` kodu yalnız rakamlardan oluşuyor, id gibi okunur",
    ),
    ("code_in_use", "`{code}` kodu zaten kullanılıyor"),
    ("home_inside_another", "ev başka bir kaydın içine konamaz"),
    (
        "needs_a_place",
        "bir {kind} için yer gerekir: --in ver, yeri bilinmiyorsa --lost",
    ),
    ("holder_gone", "{node} artık burada değil, bir şey tutamaz"),
    (
        "room_inside_wrong_kind",
        "oda yalnız bir evin ya da başka bir odanın içinde olabilir, bir {kind} içinde değil",
    ),
    (
        "into_itself",
        "bir kayıt kendi içine ya da içindeki bir şeyin içine taşınamaz",
    ),
    ("address_only_home", "yalnız evin adresi olur"),
    (
        "still_holds",
        "{node} içinde hâlâ {count} kayıt var; önce onları taşı ya da elden çıkar",
    ),
    (
        "digitize_needs_copy",
        "{node} için henüz bir kopya yok; dijitale geçmeden önce `ev photo add` ya da `ev doc add --for` ile ekle",
    ),
    ("not_gone", "{node} gitmiş değil"),
    (
        "left_with",
        "{node}, {holder} ile birlikte gitti; önce onu geri getir",
    ),
    (
        "purchase_dismissed",
        "{id} numaralı alım {as} olarak kapatılmış; önce bunu kaldır",
    ),
    (
        "purchase_not_enough_open",
        "{id} numaralı alımda bağlanacak {open} birim kaldı, {qty} değil",
    ),
    (
        "purchase_join_itself",
        "{id} numaralı alım kendisine birleştirilemez",
    ),
    (
        "purchase_join_to_joined",
        "{into} numaralı alım zaten {kept} ile birleşik; {kept} ile birleştirin",
    ),
    (
        "purchase_join_linked",
        "{id} numaralı alım bir eşyaya bağlı; önce bağı kaldırın ya da diğer satırı buna birleştirin",
    ),
    (
        "purchase_join_kept",
        "başka satırlar {id} numaralı alımla birleşik; onları diğer satıra birleştirin",
    ),
    (
        "purchase_never_a_thing",
        "{id} numaralı alım bir {bucket} alımı; evde hiçbir zaman bir eşya olmaz",
    ),
    (
        "purchase_nothing_open",
        "{id} numaralı alımda bağlanacak bir şey kalmadı",
    ),
    (
        "purchase_already_bucket",
        "{id} numaralı alım zaten {bucket}",
    ),
    (
        "purchase_linked_unlink_first",
        "{id} numaralı alım bir eşyaya bağlı; önce `ev buy unlink` ile ayır",
    ),
    (
        "purchase_not_linked_to",
        "{id} numaralı alım #{node} kaydına bağlı değil",
    ),
    (
        "line_linked_unlink_first",
        "{id} numaralı satır #{node} kaydına bağlı; önce `ev buy unlink {id} {node}`",
    ),
    (
        "line_not_declined",
        "{id} numaralı satır #{node} için reddedilmemişti",
    ),
    (
        "left_holds_no_purchase",
        "#{node} {how} olarak gitti; bir alımı olmaz",
    ),
    (
        "bought_after_left",
        "alım {bought} tarihinde yapılmış, #{node} gittikten sonra ({left})",
    ),
    (
        "grid_none",
        "buranın ızgarası yok; `ev grid <ref> --cols N --rows M` ile kur",
    ),
    (
        "cells_outside_grid",
        "{cells}, {cols}×{rows} ızgaranın dışında",
    ),
    (
        "grid_too_small",
        "yerleştirilmiş {count} kutu {cols}×{rows} ızgaranın dışında kalır",
    ),
    (
        "face_needs_grid",
        "ızgarası yok; --cols ve --rows ile bir ızgara ver",
    ),
    (
        "grid_has_boxes",
        "bu ızgarada {count} kutu yerleştirilmiş; önce hücrelerini boşalt",
    ),
    ("not_inside_anything", "{node} hiçbir şeyin içinde değil"),
    (
        "holder_has_no_grid",
        "{holder} kaydının ızgarası yok; `ev grid <holder> --cols N --rows M` ile kur",
    ),
    (
        "cells_do_not_fit",
        "{cells}, {cols}×{rows} ızgaraya sığmıyor (sütunlar A–{last_col}, satırlar 1–{rows})",
    ),
    (
        "cells_shared",
        "{a} ({a_cells}) ile {b} ({b_cells}) aynı hücreleri paylaşır",
    ),
    (
        "spread_items_only",
        "{node}: yalnız eşyalar birkaç yerde tutulur",
    ),
    (
        "spread_serial_one_unit",
        "{node}: seri numaralı bir kayıt tek birimdir",
    ),
    ("spread_lost", "{node}: kayıp; önce bul"),
    ("spread_lent", "{node}: ödünçte; önce geri al"),
    (
        "spread_pending",
        "{node}: zaten bekleyen bir taşıması var; önce iptal et",
    ),
    (
        "spread_holds_things",
        "{node}: içinde şeyler var; önce içindekileri taşı",
    ),
    (
        "not_that_many",
        "{node} kaydında {have} var; alınacak {qty} yok",
    ),
    (
        "portion_not_an_item",
        "{node} bir {kind}, yalnız eşyalar birkaç yerde tutulur; birkaç kutuyu tutan tek kaydı ikiye ayırmak için `ev split <box> <name>=<n> --take` ile bir kısmını al",
    ),
    (
        "portion_has_serial",
        "{node} kaydının seri numarası var: tek birimdir, birkaç yerde tutulmaz",
    ),
    (
        "join_all_gone",
        "hepsi gitmiş; birleştirmek için hâlâ burada olan biri gerekir",
    ),
    (
        "join_differ",
        "{field} alanında farklılar ({seen}); tek bir şeyse önce hepsine aynı {field} değerini ver",
    ),
    ("not_in_several_places", "{node} birkaç yerde tutulmuyor"),
    (
        "portion_field_apart",
        "{node}, birkaç yerde tutulan bir şeyin bir parçası; {field} onu ayırır: önce `ev unjoin`",
    ),
    (
        "review_unchanged",
        "#{id} zaten {status}; o zamandan beri bir şey değişmedi",
    ),
    (
        "task_done_reopen_first",
        "{id} numaralı iş bitmiş; bitmediyse önce `ev task reopen {id}`",
    ),
    (
        "task_already",
        "{id} numaralı iş zaten {status}; değişecek bir şey yok",
    ),
    (
        "task_not_closed",
        "{id} numaralı iş kapalı değil; yeniden açılacak bir şey yok",
    ),
    (
        "task_dropped_reopen_first",
        "{id} numaralı iş bırakılmış; bittiyse önce `ev task reopen {id}`",
    ),
    (
        "task_closed_reopen_first",
        "{id} numaralı iş kapalı; önce `ev task reopen {id}`",
    ),
    (
        "task_closed_no_move",
        "{id} numaralı iş kapalı; yerini değiştirmeden önce yeniden aç",
    ),
    (
        "no_photo_to_call_current",
        "#{id} kaydının güncel sayılacak bir fotoğrafı yok; `ev photo add {id} <file>`",
    ),
    (
        "no_code_no_label",
        "#{id} kaydının kodu yok, basılacak bir etiketi de yok",
    ),
    (
        "empty_not_a_box",
        "{ref} bir {kind}, kutu değil: yalnız bir kap boş sayılır",
    ),
    (
        "empty_has_records",
        "{ref} içinde {count} kayıt var; boşsa önce onları çıkar",
    ),
    ("not_broken", "#{id} bozuk olarak işaretli değil"),
    (
        "not_for_sale",
        "#{id} satılmak üzere ayrılmamış; önce `ev dispose {id} --as sell`",
    ),
    ("need_closed", "{id} numaralı ihtiyaç zaten kapalı"),
    (
        "gone_fields_only",
        "#{id} artık burada değil; yalnız {fields} değişebilir, `{given}` değil",
    ),
    (
        "address_clear_first",
        "yalnız evin adresi olur; önce adresi kaldır",
    ),
    (
        "holds_rooms",
        "{node} içinde odalar var, ev ya da oda olarak kalmalı",
    ),
    (
        "has_not_left",
        "#{id} gitmedi; ne zaman gittiğini `ev gone` söyler",
    ),
    (
        "waits_for_itself",
        "{node} kendisini ya da içindeki bir şeyi bekleyemez",
    ),
    ("not_ours_to_lend", "{node} bizim değil; ödünç verilemez"),
    ("place_name_taken", "`{name}` zaten bir yerin adı"),
    ("places_already_one", "iki ad da zaten aynı yeri gösteriyor"),
    ("already_lent_to", "{node} zaten {to} kimsesinde"),
    ("not_lent_out", "{node} ödünçte değil"),
    (
        "alias_names_another",
        "`{alias}` zaten başka bir yerin adı; `ev place merge` kullan",
    ),
    (
        "beside_needs_size",
        "başka bir şeyin yanına koymak için önce --size ver",
    ),
    (
        "beside_other_place",
        "yalnız aynı yerdeki bir şeyin yanına konabilir",
    ),
    ("beside_unplaced", "ötekinin henüz yeri yok; önce onu çiz"),
    (
        "stands_on_itself",
        "bir şey kendi üstünde ya da üstünde duranın üstünde duramaz",
    ),
    (
        "outside_holder",
        "{x},{y} konumunda ve {w}×{d} cm boyunda, {pw}×{pd} cm olan kabının dışına taşar",
    ),
    (
        "coverage_no_line_to_clear",
        "{id} numaralı güvencenin kaldırılacak bir alım satırı yok",
    ),
    (
        "coverage_line_dismissed",
        "{line} numaralı satır kapatılmış ({as}); önce `ev buy dismiss {line} --clear`",
    ),
    (
        "coverage_line_linked",
        "{line} numaralı satır bir eşyaya bağlı; güvencenin satırıysa önce `ev buy unlink` ile ayır",
    ),
    (
        "coverage_line_taken",
        "{line} numaralı satır zaten {coverage} numaralı güvencenin",
    ),
    (
        "not_sold",
        "#{id} satılarak gitmedi; önce `ev gone {id} --as sell` (hâlâ buradaysa `ev dispose {id} --as sell`)",
    ),
    (
        "trade_not_left",
        "#{id} gitmedi; önce `ev gone {id} --as trade`",
    ),
    (
        "trade_wrong_leaving",
        "#{id} {how} olarak gitti; yalnız verilen, satılan ya da takas edilen bir şey takas olabilir",
    ),
    (
        "already_traded_for",
        "#{id} zaten #{for} karşılığında takas edilmiş; değişecek bir şey yok",
    ),
    (
        "already_traded",
        "#{id} zaten takas edilmiş; değişecek bir şey yok",
    ),
    ("kit_name_taken", "`{name}` adlı bir set zaten var"),
    (
        "kit_part_linked",
        "{part}. parçaya ({text}) bağlı {count} kayıt var; önce `ev kit unlink` ile ayır",
    ),
    (
        "kit_part_not_linked",
        "#{node}, {kit} setinin {part}. parçasına bağlı değil",
    ),
    (
        "photo_whole_elsewhere",
        "bu fotoğraf zaten {count} başka kayda bütün olarak eklenmiş; #{id} kaydını gösteren parçayı --crop ile ekle, bütün görünüm kastediliyorsa --whole ver",
    ),
    (
        "photo_whole_elsewhere_batch",
        "bu fotoğraf zaten {count} başka kayda bütün olarak eklenmiş",
    ),
    (
        "mark_needs_grid_photo",
        "ızgara köşelerini tutan bir fotoğrafı yok; --grid ver ya da bir sonrakini --grid ile kes",
    ),
    (
        "mark_needs_whole_photo",
        "işaretlenecek bütün bir fotoğrafı yok",
    ),
    (
        "document_not_linked",
        "{id} numaralı belge #{node} kaydına bağlı değil",
    ),
    (
        "no_decline_to_clear",
        "#{id} için geri alınacak reddedilmiş bir taşıma yok",
    ),
    (
        "bring_not_linked",
        "{id} numaralı alım {ref} kaydına bağlı değil; önce ev buy link ile bağla",
    ),
    (
        "layout_too_few_units",
        "{ref} içinde kendi başına gezilen {count} yer var; bir yerleşim en az iki yeri karşılaştırır",
    ),
    (
        "batch_key_outside_batch",
        "`@key` başvuruları yalnızca bir toplu girişte çalışır",
    ),
    (
        "batch_key_unknown",
        "bilinmeyen toplu giriş anahtarı `{key}`",
    ),
    (
        "batch_key_duplicate",
        "`{key}` toplu giriş anahtarı iki kez verilmiş",
    ),
    (
        "search_text_empty",
        "arama metni boş; bir metin ver ya da listelemek için --tag / --kind / --empty kullan",
    ),
    ("edit_no_lines", "düzenlenecek satır yok"),
    ("edit_nothing_to_set", "değiştirilecek bir şey yok"),
    ("split_no_parts", "ayırmak için en az bir <name>=<qty> ver"),
    ("split_part_needs_name", "ayrılan parçanın bir adı olmalı"),
    (
        "upkeep_kind_unknown",
        "`{kind}` bir bakım türü değil: {kinds} olabilir",
    ),
    ("upkeep_work_empty", "ne yapıldığını söyle (--work)"),
    ("upkeep_at_empty", "ne zaman yapıldığını söyle"),
    (
        "upkeep_next_bad",
        "vade tarihi YYYY-AA-GG ya da YYYY-AA olmalı, gelen: `{date}`",
    ),
    ("upkeep_km_bad", "kilometre tam sayı olmalı, gelen: `{km}`"),
    ("upkeep_doc_unknown", "{doc} numaralı belge yok"),
    ("upkeep_not_found", "{id} numaralı bakım kaydı yok"),
    ("upkeep_edit_nothing", "en az bir alan=değer ver"),
    (
        "upkeep_edit_field_bad",
        "`{field}` alan=değer biçiminde değil",
    ),
    (
        "upkeep_edit_field_unknown",
        "`{field}` bir bakım alanı değil: kind, work, at, km, by, next_at, next_km, doc ya da note",
    ),
    (
        "unobserve_nothing",
        "bir gözlem numarası ver ya da tüm gözlemlerini silmek için --on ile bir yer",
    ),
    (
        "guess_nothing",
        "tahmin olarak işaretlenecek en az bir kayıt ver",
    ),
    (
        "guess_field_unknown",
        "`{field}` tahmin olabilecek bir alan değil: {fields} ya da cover:<id>",
    ),
    (
        "guess_cover_not_its",
        "{cover} numaralı kapsam #{id} kaydının kapsamlarından biri değil",
    ),
    (
        "split_part_moves_and_leaves",
        "`{name}` parçasına hem gideceği bir yer hem bir çıkış verilmiş: biri yeter",
    ),
    (
        "split_take_with_qty",
        "--take asıl kaydın sayısını kendisi belirler; --qty verme",
    ),
    (
        "split_take_needs_counts",
        "--take için hem asıl kayıtta hem her parçada bir sayı gerekir",
    ),
    ("codes_none_given", "en az bir <ref>=<code> ver"),
    ("node_given_twice", "{node} iki kez verilmiş"),
    ("code_given_twice", "`{code}` kodu iki kez verilmiş"),
    ("join_needs_two", "birleştirmek için en az iki kayıt söyle"),
    (
        "nothing_to_use_up",
        "tükenmeyi bekleyen bir şey yok; tükendiğinde `ev gone --as used` ile kaydet",
    ),
    (
        "nothing_set_aside",
        "{way} için ayrılmış bir şey yok; `ev gone --as {way}` ile kaydet",
    ),
    (
        "mistake_not_set_aside",
        "yanlış girilmiş bir kayıt kenara ayrılmaz; `ev gone --as mistake --why` ile kapat",
    ),
    (
        "mistake_needs_why",
        "kaydın neden yanlış olduğunu --why ile söyle",
    ),
    ("reference_empty", "başvuru boş"),
    (
        "code_series_malformed",
        "`{code}`: bir seri, bir önek ve ardından tek bir `*` ile yazılır, GF1x1-* gibi",
    ),
    ("code_empty", "kod boş"),
    ("qty_below_one", "adet en az 1 olmalı"),
    (
        "purchase_qty_not_a_count",
        "adet {qty} tam bir sayı değil; 2 gibi bir sayı gönderin",
    ),
    ("fill_out_of_range", "doluluk 0 ile 100 arasında olmalı"),
    ("tag_empty", "etiket boş"),
    ("photo_path_empty", "fotoğraf yolu boş"),
    ("photo_path_invalid", "fotoğraf yolu `{path}`: {error}"),
    (
        "past_with_of",
        "geçmişteki bir şey tek başına eklenir: `gone` ile `of` birlikte olmaz",
    ),
    (
        "leaving_details_without_gone",
        "--at ve --where bir şeyin nasıl gittiğini söyler: onu --gone ile ekle",
    ),
    ("name_empty", "ad boş"),
    (
        "leaving_way_unknown",
        "`{way}` bir gidiş biçimi değil; sell, give, trash, used, trade, return, left, stolen ya da unknown kullan",
    ),
    (
        "past_way_not_allowed",
        "geçmişteki bir şey {way} olarak eklenmez: nasıl gittiğini söyle (sell, give, trash, used, trade, return, left, stolen ya da unknown)",
    ),
    (
        "past_in_a_place",
        "geçmişteki bir şey tek başına eklenir: --in, --lost, ev ya da oda olmaz",
    ),
    (
        "past_no_place_fields",
        "geçmişteki bir şeyin --to, --temporary ya da --code değeri olmaz: artık burada değil",
    ),
    (
        "past_where_is_a_place",
        "--where bir kaydı değil, bir yeri (eski bir evi) adlandırır",
    ),
    (
        "traded_for_without_trade",
        "traded_for yalnızca bir takasla verilir: `--gone trade`",
    ),
    ("say_where_with_in", "nerede olduklarını --in ile söyle"),
    (
        "shred_not_for_way",
        "--shred çöpe gidenler içindir (trash, digitize), `{way}` için değil",
    ),
    (
        "merged_not_a_way",
        "`merged` bir gidiş biçimi değil: bir parça başka birine katıldığında bunu ev kendisi koyar",
    ),
    (
        "restore_needs_why",
        "kaydın neden aslında gitmediğini söyle",
    ),
    (
        "purchase_not_an_amount",
        "`{amount}` 1234.56 gibi bir tutar değil",
    ),
    (
        "not_a_date",
        "`{date}` YYYY-MM-DD biçiminde bir tarih değil",
    ),
    ("no_purchase_with_id", "{id} numaralı alım yok"),
    ("field_required", "`{field}` gerekli"),
    (
        "purchase_status_unknown",
        "durum `{status}`; delivered, returned ya da cancelled kullan",
    ),
    (
        "purchase_import_bucket_unknown",
        "sınıf `{bucket}`; {buckets} kullan (bağdaştırıcının sarf satırları alınmaz)",
    ),
    ("line_not_json", "JSON değil: {error}"),
    (
        "purchase_line_type_unknown",
        "bilinmeyen satır türü `{type}`",
    ),
    ("date_still_to_come", "`{date}` henüz gelmedi"),
    (
        "purchase_consumable_not_recorded",
        "sarf malzemesi alım olarak kaydedilmez; {buckets} kullan",
    ),
    (
        "purchase_manual_cancelled",
        "elle girilen bir alım iptal edilemez",
    ),
    (
        "purchase_bucket_unknown",
        "sınıf `{bucket}`; {buckets} kullan",
    ),
    (
        "purchase_reason_unknown",
        "`{reason}` bir gerekçe değil; {reasons} kullan",
    ),
    ("purchase_pack_below_one", "paket en az 1 olmalı"),
    (
        "purchase_pack_too_small",
        "{id} numaralı alımın {linked} birimi bağlı; {pack}'li paket {units} birim bırakır",
    ),
    (
        "purchase_link_to_place",
        "#{node} bir yer; alım bir eşyaya bağlanır",
    ),
    (
        "money_currency_unknown",
        "`{currency}` EUR, USD ya da TRY gibi bir para birimi kodu değil",
    ),
    ("money_not_positive", "`{field}` pozitif bir sayı olmalı"),
    (
        "money_period_malformed",
        "dönem `{period}`: YYYY-MM ya da YYYY",
    ),
    ("money_day_malformed", "gün `{day}`: YYYY-MM-DD"),
    (
        "money_line_type_unknown",
        "satır türü {type}; index ya da rate kullan",
    ),
    ("not_web_address", "`{url}` bir web adresi değil"),
    (
        "attachment_link_kind_unknown",
        "`{kind}` bir bağlantı türü değil",
    ),
    (
        "attachment_coverage_kind_unknown",
        "`{kind}` bir güvence türü değil",
    ),
    ("attachment_unknown", "bilinmeyen ek `{attachment}`"),
    (
        "attachment_not_carried",
        "{id} numaralı alımda {attachments} eki yok; ev buy show {id} hepsini listeler",
    ),
    (
        "attachment_type_unknown",
        "'{type}' ek türü şunlardan biri değil: {types}",
    ),
    ("valuation_not_positive", "değer sıfırdan büyük olmalı"),
    ("valuation_no_such_id", "{id} numaralı değer yok"),
    (
        "link_kind_unknown",
        "`{kind}` bir bağlantı türü değil; şunlardan birini kullan: {kinds}",
    ),
    (
        "link_archive_neither",
        "arşiv `{archive}`: ne web adresi ne dosya",
    ),
    ("link_archive_unreadable", "{archive}: {error}"),
    ("link_no_such_id", "{id} numaralı bağlantı yok"),
    (
        "doc_kind_unknown",
        "`{kind}` bir belge türü değil; şunlardan birini kullan: {kinds}",
    ),
    (
        "doc_date_malformed",
        "`{date}` bir tarih değil; YYYY-MM-DD, YYYY-MM ya da YYYY kullan",
    ),
    ("doc_no_such_id", "{id} numaralı belge yok"),
    ("doc_no_such_file", "{file}: böyle bir dosya yok"),
    ("doc_file_unreadable", "{file}: {error}"),
    (
        "coverage_bad_term",
        "`{term}` bir süre değil; örneğin 2y, 18m, 6w, 90d ya da lifetime kullan",
    ),
    ("coverage_not_found", "{id} numaralı bir güvence yok"),
    (
        "coverage_setting_unknown",
        "`{setting}` bir envanter ayarı değil; şunlardan birini kullan: {settings}",
    ),
    (
        "coverage_setting_bad_value",
        "`{value}`, {setting} için geçerli bir değer değil",
    ),
    (
        "coverage_covers_nothing",
        "kapsadığı en az bir eşyayı söyle",
    ),
    (
        "coverage_kind_unknown",
        "`{kind}` bir güvence türü değil; şunlardan birini kullan: {kinds}",
    ),
    (
        "coverage_bad_after",
        "`{after}`: after:<güvence id> biçiminde ver",
    ),
    (
        "coverage_needs_term",
        "bir --term (2y, 18m, lifetime) ya da bir --ends tarihi ver",
    ),
    (
        "coverage_insurance_lifetime",
        "bir sigortanın süresi biter: lifetime değil, bir --ends tarihi ya da yıl, ay, hafta veya gün olarak bir süre ver",
    ),
    (
        "coverage_track_unknown",
        "`{subject}` izlenmiyor; value ya da coverage kullan",
    ),
    (
        "coverage_track_decision",
        "`{decision}` anlaşılmadı; no, later ya da yes kullan",
    ),
    ("task_not_found", "{id} numaralı bir görev yok"),
    (
        "task_bad_due",
        "bitiş tarihi YYYY-MM-DD biçiminde olmalı, gelen: `{due}`",
    ),
    ("plan_field_empty", "{field} boş"),
    ("plan_goal_unknown", "hedef şunlardan biri olmalı: {goals}"),
    ("node_has_no_photo", "#{id} kaydının {n}. fotoğrafı yok"),
    (
        "plan_series_has_no",
        "işaretli fotoğraf dizisinde f{n} yok ({count} resim)",
    ),
    ("plan_names_no_picture", "en az bir resim söyle"),
    ("no_such_file", "{file} diye bir dosya yok"),
    ("plan_no_observation", "{id} numaralı bir gözlem yok"),
    (
        "review_thing_not_place",
        "#{id} bir yer değil, bir eşya; içinde durduğu yeri gözden geçir",
    ),
    (
        "review_status_unknown",
        "gözden geçirme durumu counting, toured, kept ya da raw olmalı",
    ),
    (
        "progress_thing_not_place",
        "#{id} bir yer değil, bir eşya: içinde durduğu mobilyayı ya da odayı ver",
    ),
    (
        "task_status_unknown",
        "durum şunlardan biri olmalı: {states}",
    ),
    (
        "mark_bad_use_by",
        "`{date}` YYYY-MM-DD ya da YYYY-MM biçiminde değil",
    ),
    ("need_not_found", "{id} numaralı bir ihtiyaç yok"),
    ("mark_empty_names_none", "boş olan kutuları söyle"),
    (
        "mark_condition_unknown",
        "`{condition}` bir durum değil; şunlardan birini kullan: {conditions}",
    ),
    (
        "mark_sale_state_unknown",
        "`{state}` bir satış durumu değil; listed ya da reserved kullan",
    ),
    ("need_text_empty", "ihtiyacın metni boş"),
    (
        "kit_not_found",
        "`{kit}` diye bir set yok; `ev kit list` setleri gösterir",
    ),
    ("kit_part_needs_name", "setin bir parçasının adı olmalı"),
    (
        "kit_part_qty_too_small",
        "`{part}`: bir parça en az bir kez gelir",
    ),
    (
        "kit_part_missing",
        "{n}. parça yok; sette {count} parça var (`ev kit show` hepsini listeler)",
    ),
    (
        "kit_part_missing_numbered",
        "{n}. parça yok; sette {last} numarasına kadar numaralanmış {count} parça var (`ev kit show` hepsini listeler)",
    ),
    ("kit_needs_name", "bir setin adı olmalı"),
    ("kit_copies_too_few", "bir set en az bir kez alınır"),
    ("kit_needs_parts", "en az bir parça ver"),
    ("kit_link_needs_records", "bu parça olan kayıtları ver"),
    ("portion_qty_too_small", "--qty en az 1 olmalı"),
    (
        "sketch_points_bad",
        "--points santimetre cinsinden üç ya da daha çok köşedir, örneğin `0,0 400,0 400,300`; gelen: `{points}`",
    ),
    (
        "sketch_pair_bad",
        "{what} santimetre cinsinden iki sayıdır, örneğin 120,40; gelen: `{value}`",
    ),
    (
        "sketch_nothing_given",
        "--size w,d, --at x,y, --points, --on <ref>, --right-of/--left-of/--above/--below <ref> ya da --clear ver",
    ),
    ("sketch_clear_alone", "--clear yanına başka bir şey almaz"),
    (
        "sketch_place_once",
        "yer bir kez verilir: --at, --points ya da başka bir şeyin yanı",
    ),
    (
        "sketch_outline_too_few",
        "dış çizgi üç ya da daha çok köşedir",
    ),
    (
        "sketch_outline_or_size",
        "dış çizginin kendi boyutu var; ya --points ya --size ver",
    ),
    (
        "sketch_size_bad",
        "boyut santimetre cinsinden iki pozitif sayıdır",
    ),
    (
        "sketch_size_has_outline",
        "boyutunu dış çizgisi veriyor; bunun yerine yeni --points ver",
    ),
    ("sketch_at_bad", "yer santimetre cinsinden iki sayıdır"),
    (
        "sketch_offset_alone",
        "--offset, --right-of, --left-of, --above ya da --below ile birlikte verilir",
    ),
    (
        "map_no_home",
        "henüz ev yok; `ev add <name> --kind home` ile bir tane ekle",
    ),
    ("map_place_gone", "#{id} artık burada değil"),
    (
        "past_date_bad",
        "`{date}` bir tarih değil: bir yıl (2016), bir ay (2016-06) ya da bir gün (2016-06-14) ver",
    ),
    (
        "past_came_after_left",
        "{came} tarihinde gelmiş ama {left} tarihinde gitmiş; tarihlerden biri yanlış",
    ),
    (
        "past_where_not_place",
        "--where bir yeri (eski bir evi) adıyla söyler, bir kaydı ya da id'yi değil",
    ),
    (
        "past_sale_price_zero",
        "satış sıfırdan fazlasını getirmiş olmalı",
    ),
    ("trade_for_itself", "bir şey kendisiyle takas edilmez"),
    (
        "trade_for_place",
        "#{other} bir yer; bir şey ancak bir şeyle takas edilir",
    ),
    (
        "trade_other_left_before",
        "#{other} takastan ({left}) önce, {other_left} tarihinde gitmiş; takasta gelmiş olamaz",
    ),
    (
        "trade_other_came_before",
        "#{other} takastan ({left}) önce, {came} tarihinde gelmiş; zaten bizimdi",
    ),
    (
        "trade_other_came_years_after",
        "#{other} takastan ({left}) yıllar sonra, {came} tarihinde gelmiş; tarihlerden biri yanlış",
    ),
    ("past_year_bad", "`{year}` geriye bakılacak bir yıl değil"),
    (
        "photo_not_an_image",
        "{file} ev'in okuyabildiği bir resim değil (JPEG ya da PNG fotoğraf)",
    ),
    (
        "photo_cut_nothing",
        "en az bir <ref>=x,y,w,h ya da --place <ref> ver",
    ),
    (
        "photo_grid_needs_place",
        "--grid, --place ızgarasındaki kutuları okur; --place de ver",
    ),
    (
        "photo_preview_nothing",
        "önizlenecek bir şey yok: <ref>=x,y,w,h ya da --grid ile --place ver",
    ),
    (
        "series_ref_bad",
        "`{text}` bir dizi resmi değil: f12 ya da f16..f31 gibi bir aralık ver",
    ),
    ("series_is_empty", "işaretli fotoğraf dizisinde resim yok"),
    (
        "photo_mark_nothing",
        "en az bir <label>=x,y,w,h ya da <label>=<cell> ver",
    ),
    (
        "photo_mark_cell_on_file",
        "`{spec}` bir hücre: bir dosyayı değil, ızgarası olan bir yeri (koduyla) işaretle",
    ),
    (
        "photo_number_missing",
        "{n}. fotoğraf yok; {count} fotoğrafı var",
    ),
    (
        "photo_crop_not_numbers",
        "kırpma `{crop}` dört sayı (x,y,w,h) değil",
    ),
    (
        "photo_crop_not_four",
        "kırpma `{crop}` tam dört sayı (x,y,w,h) ister",
    ),
    (
        "photo_crop_outside",
        "kırpma `{crop}` fotoğrafın içinde kalmalı: 0–1 arası kesirler, x+w ≤ 1 ve y+h ≤ 1",
    ),
    (
        "photo_turn_bad",
        "fotoğrafı saat yönünde 90, 180 ya da 270 derece döndür, {degrees} değil",
    ),
    (
        "edit_size_bad",
        "boyut WxDxH ya da WxD biçimindedir, örneğin 1x2x0.5; gelen: `{size}`",
    ),
    (
        "edit_not_integer",
        "{field} tam sayı olmalı, gelen: `{value}`",
    ),
    (
        "edit_not_assignment",
        "`{assignment}` alan=değer biçiminde değil",
    ),
    ("edit_name_empty", "ad boş olamaz"),
    ("edit_note_add_empty", "note=+ eklenecek metni ister"),
    (
        "edit_not_boolean",
        "{field} true ya da false alır, gelen: `{value}`",
    ),
    (
        "edit_left_in_not_place",
        "left_in bir yeri (eski bir evi) adıyla söyler, bir kaydı ya da id'yi değil",
    ),
    (
        "edit_field_unknown",
        "bilinmeyen ya da salt okunur alan `{field}`; değiştirilebilenler: name, code, kind, address, qty, note, theme, fill, size, tags, photos, to, owner, with, temporary, waits_for, make, model, serial, came, gitmiş bir kayıtta da left ve left_in (bir yerin ne kadar sayıldığı `ev review` ile değişir)",
    ),
    (
        "edit_not_plus_minus",
        "{field} +değer ya da -değer alır, gelen: `{value}`",
    ),
    ("cell_bad", "`{cell}` A3 gibi bir hücre değil"),
    (
        "grid_corners_bad",
        "ızgara köşeleri `{corners}` 0–1 arası sekiz kesirdir: arka-sol x,y, arka-sağ x,y, ön-sağ x,y, ön-sol x,y",
    ),
    (
        "grid_size_bad",
        "ızgara 1–{max_cols} sütun ve 1–{max_rows} satırdır",
    ),
    (
        "grid_face_bad",
        "ızgaraya üstten (`above`) ya da önden (`front`) bakılır; gelen: `{face}`",
    ),
    ("cell_nothing_given", "en az bir <ref>=<cells> ver"),
    ("cell_ref_twice", "`{ref}` iki kez verilmiş"),
    (
        "placement_no_live_node",
        "{ref} diye evde duran bir kayıt yok",
    ),
    (
        "placement_nothing_described",
        "yerleştirilecek şeyi anlat (3+ harfli bir kelime ya da parça kodu) ya da --for ver",
    ),
    ("placement_no_holder_to_stay", "içinde kalacağı bir kap yok"),
    (
        "vocab_facet_name_bad",
        "türün adında aranabilir bir kelime (3+ harf) olmalı",
    ),
    ("vocab_no_facet", "{name} diye bir tür yok"),
    (
        "vocab_synonyms_too_few",
        "virgülle ayrılmış iki ya da daha çok kelime veya söz öbeği ver, her birinde aranabilir bir kelime olsun",
    ),
    (
        "vocab_no_synonym_group",
        "{id} numaralı bir eşanlamlı grubu yok",
    ),
    ("place_name_empty", "yer adı boş"),
    (
        "place_no_such",
        "`{name}` adında bir yer yok; `ev place list` hepsini gösterir",
    ),
    ("audit_rule_empty", "kural metni boş"),
    ("audit_no_rule", "{id} numaralı bir kural yok"),
    ("label_required", "{what} gerekli"),
    (
        "label_unknown",
        "bilinmeyen {what} `{value}`; şunlardan biri olmalı: {choices}",
    ),
];

/// English → Turkish. Keys are exactly the strings passed to `t` and `tf`.
static TR: &[(&str, &str)] = &[
    // Tabs
    ("Layout", "Yerleşim"),
    ("Pending", "Bekleyen"),
    ("Leaving", "Çıkış"),
    ("Lost", "Kayıp"),
    ("Errands", "Götür/İade"),
    ("Search", "Ara"),
    ("To do", "Yapılacak"),
    ("Settings", "Ayarlar"),
    ("Statistics", "İstatistik"),
    ("Past", "Gidenler"),
    // Words for stored values
    ("trash", "çöp"),
    ("give", "ver"),
    ("sell", "sat"),
    ("return", "iade"),
    ("record error", "kayıt hatası"),
    ("photograph, then throw out", "fotoğrafla, sonra at"),
    ("  (gone, copy kept)", "  (gitti, kopyası var)"),
    ("  (shred)", "  (parçalanarak)"),
    ("shred first", "önce parçala"),
    ("yes", "evet"),
    ("scan", "kopya (kağıdı atıldı)"),
    ("warning: {}", "uyarı: {}"),
    ("home", "ev"),
    ("room", "oda"),
    ("furniture", "mobilya"),
    ("container", "kap"),
    ("item", "eşya"),
    ("vehicle", "araç"),
    ("take", "götür"),
    ("collect", "geri al"),
    ("listed", "ilanda"),
    ("reserved", "ayrıldı"),
    ("organize", "düzenleme"),
    ("track", "yalnız kayıt"),
    // Markers on a line
    ("  [lost]", "  [kayıp]"),
    ("  [owner: {}]", "  [sahibi: {}]"),
    ("  [with {}]", "  [{}'de]"),
    ("  (+{} parts)", "  (+{} parça)"),
    ("never known", "hiç bilinmiyor"),
    ("  (last seen: {})", "  (son görüldüğü: {})"),
    ("  {} items", "  {} eşya"),
    // To do tab
    (
        " To do · {}/{} counted · {} tasks · {} moves ",
        " Yapılacak · {}/{} sayıldı · {} iş · {} taşıma ",
    ),
    ("TASKS", "İŞLER"),
    ("MOVES", "TAŞIMALAR"),
    ("TAKE / RETURN", "GÖTÜR / İADE"),
    ("LEAVING", "ÇIKIŞ"),
    ("LABELS TO PRINT", "ETİKET BASILACAK"),
    ("TO GET", "ALINACAK"),
    ("  (print / make)", "  (bas / yap)"),
    ("  last seen: {}", "  son görüldüğü: {}"),
    ("  (never known)", "  (hiç bilinmiyor)"),
    ("REPAIRS", "TAMİR"),
    ("USE-BY", "SON KULLANMA"),
    ("  {} · past", "  {} · geçti"),
    ("  {} · {} days", "  {} · {} gün"),
    ("LOST", "KAYIP"),
    ("NOT COUNTED YET", "HENÜZ SAYILMADI"),
    ("CHANGED SINCE COUNTED", "SAYILDIKTAN SONRA DEĞİŞTİ"),
    ("PHOTO NEEDED", "FOTOĞRAF GEREKLİ"),
    ("  no photo at all", "  hiç fotoğrafı yok"),
    (
        "  photo marked out of date",
        "  fotoğrafı eski olarak işaretli",
    ),
    (
        "  changed after the photo ({})",
        "  fotoğraftan sonra değişti ({})",
    ),
    (
        "same whole photo on {} records  ",
        "{} kayıtta aynı tam fotoğraf  ",
    ),
    ("UNCUT SHARED PHOTO", "KESİLMEMİŞ ORTAK FOTOĞRAF"),
    ("UNCLEAR RECORDS", "BELİRSİZ KAYITLAR"),
    // Settings tab
    ("Language", "Dil"),
    ("Appearance", "Görünüm"),
    ("Automatic", "Otomatik"),
    ("Automatic ({})", "Otomatik ({})"),
    ("system: {}", "sistem: {}"),
    ("terminal: {}", "terminal: {}"),
    ("dark", "koyu"),
    ("light", "açık"),
    ("Dark", "Koyu"),
    ("Light", "Açık"),
    ("not reported yet", "henüz bildirmedi"),
    (
        "Automatic follows the computer's language, and English when that language is not available.",
        "Otomatik, bilgisayarın dilini izler; o dil yoksa İngilizce gösterir.",
    ),
    (
        "Automatic follows the terminal's light or dark background and switches with it while ev ui is open.",
        "Otomatik, terminalin açık ya da koyu arka planını izler ve ev ui açıkken onunla birlikte değişir.",
    ),
    (
        "This terminal does not announce appearance changes, so ev ui asks it every few seconds.",
        "Bu terminal görünüm değişimini kendiliğinden bildirmiyor; ev ui birkaç saniyede bir soruyor.",
    ),
    (
        "Enter or → picks the next option, ← the previous one.",
        "Enter ya da → sonraki seçeneği, ← öncekini seçer.",
    ),
    ("Saved in {}", "Kaydedildiği yer: {}"),
    (
        "From the command line: ev settings language en|tr|auto, ev settings theme dark|light|auto, ev settings resume on|off",
        "Komut satırından: ev settings language en|tr|auto, ev settings theme dark|light|auto, ev settings resume on|off",
    ),
    ("Reopen where I left off", "Kaldığım yerden aç"),
    ("On", "Açık"),
    ("Off", "Kapalı"),
    (
        "On: ev ui opens on the node that was selected in the tree when it last closed, for each database on its own. Off: it opens at the top.",
        "Açık: ev ui, en son kapandığında Yerleşim ağacında seçili olan düğümde açılır; her veritabanı için ayrı. Kapalı: en baştan açılır.",
    ),
    ("settings saved", "ayarlar kaydedildi"),
    (
        "could not save the settings: {}",
        "ayarlar kaydedilemedi: {}",
    ),
    (
        "shown in {}; the computer's language is {}",
        "gösterilen: {}; bilgisayarın dili: {}",
    ),
    // Status line, titles, help
    ("updated {}", "güncellendi {}"),
    ("showing: {}", "gösteriliyor: {}"),
    ("about {}, the series {}", "konu {}, seri {}"),
    ("\"{}\": {} results", "\"{}\": {} sonuç"),
    ("search cleared", "arama temizlendi"),
    (
        "(this terminal cannot show pictures — press O to open it outside)",
        "(bu terminal resim gösteremiyor — O ile dışarıda aç)",
    ),
    (
        "[ ] ← → step · r/R rotate · O open outside · Esc/o/click close",
        "[ ] ← → teker gez · r/R döndür · O dışarıda aç · Esc/o/tık kapat",
    ),
    (
        " Search: \"{}\" · ✕ clear (x) ",
        " Ara: \"{}\" · ✕ temizle (x) ",
    ),
    (" Photo {}/{} ", " Fotoğraf {}/{} "),
    (" Product image {}/{} ", " Ürün görseli {}/{} "),
    ("Photos ({})", "Fotoğraflar ({})"),
    ("Product images ({})", "Ürün görselleri ({})"),
    (
        "([ ] step · r rotate · o full screen · O open outside) ",
        "([ ] gez · r döndür · o tam ekran · O dışarıda aç) ",
    ),
    (" Details ", " Ayrıntı "),
    (" J/K scroll ", " J/K kaydır "),
    ("just now", "az önce"),
    ("{} min ago", "{} dk önce"),
    ("{} h ago", "{} sa önce"),
    ("Suggestions (ev regroup)", "Öneriler (ev regroup)"),
    (
        "(every place with things in it has a theme)",
        "(içinde eşya olan her yerin teması var)",
    ),
    ("words: {}", "kelimeler: {}"),
    ("(no facets)", "(tür yok)"),
    ("(no kits)", "(set yok)"),
    ("grid {}×{}", "ızgara {}×{}"),
    ("sketch {}×{} cm", "kroki {}×{} cm"),
    ("stack, front on, top first", "üst üste, önden, en üst önce"),
    ("tiles (no layout yet)", "karolar (henüz yerleşim yok)"),
    ("  (with {} on it)", "  (üstünde {})"),
    ("nothing inside", "içinde bir şey yok"),
    ("this is the top", "en üstteyiz"),
    (" Not on the map yet ", " Haritada yeri yok "),
    ("(nothing here yet)", "(burada henüz bir şey yok)"),
    ("←↑↓→ move", "←↑↓→ gez"),
    ("Enter go in", "Enter içine gir"),
    ("Backspace go up", "Backspace üste çık"),
    ("t show in tree", "t ağaçta göster"),
    ("click: select, again: go in", "tık: seç, bir daha: gir"),
    ("Esc close", "Esc kapat"),
    ("temporary place", "geçici yer"),
    ("{} on it", "üstünde {}"),
    ("M map", "M harita"),
    ("sketched", "krokiye işlendi"),
    (
        "Declined, left where they are:",
        "Reddedildi, yerinde kalıyor:",
    ),
    ("move declined", "taşıma reddedildi"),
    ("decline taken back", "ret geri alındı"),
    ("stays in {}", "{} içinde kalıyor"),
    (
        "regroup may propose moving it again",
        "regroup onu yeniden taşımak için önerebilir",
    ),
    (
        "{}×{} grid seen from the front, row 1 at the top",
        "{}×{} ızgara, önden, 1. satır üstte",
    ),
    ("grid seen from", "ızgaranın bakışı"),
    ("the front", "önden"),
    ("above", "yukarıdan"),
    ("sketch removed", "krokiden çıkarıldı"),
    (
        "(the unplaced ones are listed last)",
        "(yeri belirsiz olanlar en sonda)",
    ),
    ("(no sketch)", "(kroki yok)"),
    ("at x {} · y {} cm", "x {} · y {} cm'de"),
    ("{}×{} cm", "{}×{} cm"),
    ("on #{}", "#{} üstünde"),
    ("  [temporary place]", "  [geçici yer]"),
    ("  (temporary place)", "  (geçici yer)"),
    ("Waiting for a final place", "Nihai yerini bekleyenler"),
    ("  (parked in {})", "  ({} içinde bekliyor)"),
    ("  (not counted)", "  (sayılmadı)"),
    (
        "Parking places, not offered as a final place:",
        "Geçici yerler, nihai yer olarak önerilmez:",
    ),
    (
        "Boxes whose name says a size their size field does not:",
        "Adında yazan boyutu size alanında olmayan kutular:",
    ),
    ("kit", "set"),
    ("waits for", "bekliyor"),
    ("waited for by", "bunu bekleyen"),
    (
        "{} waited for this: where do they go now?",
        "{} bunu bekliyordu: şimdi nereye gidecekler?",
    ),
    ("  waits for #{} {}", "  bekliyor: #{} {}"),
    (
        "{} of {} found · {} lost · {} still missing",
        "{} / {} bulundu · {} kayıp · {} eksik",
    ),
    ("{} lost", "{} kayıp"),
    ("linked to kit", "sete bağlandı"),
    ("unlinked from kit", "setten çıkarıldı"),
    ("(no holder tagged yet)", "(henüz etiketli kap yok)"),
    ("Facet: {}", "Tür: {}"),
    (
        "Kept out, another facet:",
        "Başka türde olduğu için önerilmeyenler:",
    ),
    ("score {}", "puan {}"),
    ("reads like: {}", "benzediği: {}"),
    ("contents: {}", "içindekiler: {}"),
    ("No theme yet (ev themes)", "Henüz teması yok (ev themes)"),
    ("words: ", "kelimeler: "),
    ("reads like: ", "benzediği: "),
    ("  (a guess)", "  (tahmin)"),
    (
        "Sharing no word with anything in its holder (a guess; look before moving):",
        "Kabındaki hiçbir şeyle kelime paylaşmıyor (tahmin; taşımadan önce bak):",
    ),
    (
        "mixed: half or more fit better elsewhere",
        "karışık: yarısı ya da fazlası başka yere daha iyi uyar",
    ),
    (
        "Search: {}▏  (Enter search · Esc clear/cancel · Ctrl+U clear)",
        "Ara: {}▏  (Enter ara · Esc sil/vazgeç · Ctrl+U sil)",
    ),
    (
        "↑↓ move · Enter/→ next option · ← previous option · Tab pane · q quit    {}",
        "↑↓ gez · Enter/→ sonraki seçenek · ← önceki seçenek · Tab bölme · q çık    {}",
    ),
    // The sidebar (spec/ui-sidebar.md): its title, its headings, its keys.
    (" Lists ", " Listeler "),
    ("HOME", "EVDE"),
    ("HISTORY", "GEÇMİŞ"),
    ("INSIGHT", "İÇGÖRÜ"),
    ("↑↓ choose a list", "↑↓ liste seç"),
    ("Enter go to the list", "Enter listeye geç"),
    ("Tab next pane", "Tab sonraki bölme"),
    ("b sidebar", "b kenar çubuğu"),
    // The key hints under the panes, one part each so the line fits the width.
    ("↑↓ move", "↑↓ gez"),
    (
        " → ← open/close · d two levels · e/c all below · C close all ",
        " → ← aç/kapat · d iki seviye aç · e/c altı tümden · C hepsini kapat ",
    ),
    ("Enter show in tree", "Enter ağaçta göster"),
    ("Enter open/close section", "Enter bölümü aç/kapat"),
    ("x clear", "x temizle"),
    ("/ search", "/ ara"),
    (": go to or do", ": git ya da yap"),
    (
        " Go to a list, a place, a thing or #id · > a command ",
        " Git: bir liste, bir yer, bir eşya ya da #id · > bir komut ",
    ),
    // Repairs and maintenance (spec/repairs.md).
    ("Removed: {}", "Silindi: {}"),
    ("Upkeep due", "Vadesi gelen bakımlar"),
    ("Upkeep", "Bakım"),
    ("by {}", "yapan: {}"),
    ("document #{}", "belge #{}"),
    ("in {} km", "{} km sonra"),
    ("inspection", "muayene"),
    ("next: {}", "sonraki: {}"),
    ("repair", "onarım"),
    ("service", "bakım"),
    ("upkeep", "bakım kaydı"),
    ("{} days past", "{} gün geçti"),
    ("{} km", "{} km"),
    ("{} km past", "{} km geçti"),
    // A place just emptied (spec/emptied-place.md).
    (
        "{} is empty now; what it says is about what was in it:",
        "{} artık boş; üzerinde yazanlar içinde olanlarla ilgili:",
    ),
    ("clear: ev edit {} theme=", "temizle: ev edit {} theme="),
    (
        "clear: ev unobserve --on {}",
        "temizle: ev unobserve --on {}",
    ),
    (
        "Removed {} observation(s) from:",
        "{} gözlem silindi, şuradan:",
    ),
    // What is only a guess (spec/guesses.md).
    ("(end is a guess)", "(bitişi tahmin)"),
    ("Only a guess", "Yalnızca tahmin"),
    (
        "Still a guess here: ask whether each was seen",
        "Burada hâlâ tahmin olanlar: her biri görüldü mü, sor",
    ),
    ("a guess", "tahmin"),
    ("guess", "tahmin"),
    ("guessed", "tahmini alan"),
    ("a guess: {}", "tahmin: {}"),
    ("check: {}", "kontrol et: {}"),
    ("no guess", "tahmin yok"),
    ("{} is a guess", "{} tahmin"),
    ("{} is a guess: {}", "{} tahmin: {}"),
    // The screen's commands, as `:` offers them (ui/commands.rs).
    ("Search everything", "Her yerde ara"),
    ("Map of the home", "Evin haritası"),
    (
        "Show or hide the sidebar",
        "Kenar çubuğunu göster ya da gizle",
    ),
    ("Narrower list", "Listeyi daralt"),
    ("Wider list", "Listeyi genişlet"),
    (
        "Widen the details, or put them back",
        "Ayrıntıyı genişlet ya da geri al",
    ),
    (
        "Copy: the #id and name, or the document",
        "Kopyala: #id ve adı, ya da belgeyi",
    ),
    (
        "Show or hide the empty fields",
        "Boş alanları göster ya da gizle",
    ),
    ("Next details tab", "Sonraki ayrıntı sekmesi"),
    ("Previous details tab", "Önceki ayrıntı sekmesi"),
    ("Next place of this thing", "Bu eşyanın bir sonraki yeri"),
    (
        "Filter: open, linked, dismissed",
        "Süz: açık, bağlı, kapatılmış",
    ),
    ("Open everything below", "Altındaki her şeyi aç"),
    ("Close everything below", "Altındaki her şeyi kapat"),
    ("Open two levels", "İki seviye aç"),
    ("Close the whole tree", "Bütün ağacı kapat"),
    ("Photo full screen", "Fotoğraf tam ekran"),
    ("Open the photo outside", "Fotoğrafı dışarıda aç"),
    ("Rotate the photo right", "Fotoğrafı sağa döndür"),
    ("Rotate the photo left", "Fotoğrafı sola döndür"),
    ("Smaller photo", "Fotoğrafı küçült"),
    ("Larger photo", "Fotoğrafı büyüt"),
    (
        "Reopen the marked photo series",
        "İşaretli fotoğraf dizisini yeniden aç",
    ),
    (
        "Close the marked photo series",
        "İşaretli fotoğraf dizisini kapat",
    ),
    ("Quit", "Çık"),
    ("nothing found", "bir şey bulunamadı"),
    ("    Esc ‹ {}", "    Esc ‹ {}"),
    ("#{} left this list", "#{} bu listeden çıktı"),
    // The purchase lists (spec/ui-sidebar.md, phase 3).
    ("All", "Tümü"),
    ("Durable", "Dayanıklı"),
    ("Clothing", "Giyim"),
    ("Digital", "Dijital"),
    ("Service", "Hizmet"),
    ("all", "hepsi"),
    ("open", "açık"),
    ("linked", "bağlı"),
    ("dismissed", "kapatılmış"),
    (" {} · {} lines · {} · {} ", " {} · {} satır · {} · {} "),
    ("· \"{}\" ", "· \"{}\" "),
    ("  order {} · {} lines · {}", "  sipariş {} · {} satır · {}"),
    (" Purchase #{} ", " Alım #{} "),
    (
        "this line is linked to no thing here",
        "bu satır buradaki hiçbir eşyaya bağlı değil",
    ),
    (
        "Filter: {}▏  (the list follows as you type · Enter keep · Esc clear)",
        "Süz: {}▏  (liste yazdıkça değişir · Enter tut · Esc sil)",
    ),
    ("Enter the thing in the tree", "Enter eşyayı ağaçta göster"),
    ("f open/linked/dismissed", "f açık/bağlı/kapatılmış"),
    ("/ filter", "/ süz"),
    // Where an error happened (spec/error-ids.md).
    ("line {}", "satır {}"),
    (
        "Enter on › the list behind a figure",
        "› olanda Enter: sayının ardındaki liste",
    ),
    ("H/L details tabs", "H/L ayrıntı sekmesi"),
    ("J/K scroll", "J/K kaydır"),
    ("[ ] o photos", "[ ] o fotoğraf"),
    (
        "Tab pane · 0-9 lists · b sidebar",
        "Tab bölme · 0-9 liste · b kenar çubuğu",
    ),
    ("< > { } or drag: resize", "< > { } ya da sürükle: boyut"),
    ("q quit", "q çık"),
    (
        "#{} is no longer in the tree (gone)",
        "#{} artık ağaçta yok (gitti)",
    ),
    (" Marked photo ", " İşaretli fotoğraf "),
    ("Esc/o hide", "Esc/o gizle"),
    ("[ ] ← → step", "[ ] ← → gez"),
    ("m shows it again", "m yeniden gösterir"),
    ("X close series", "X seriyi kapat"),
    ("g grid", "g ızgara"),
    ("g single", "g tek resim"),
    ("f12 go to · Home/End", "f12 git · Home/End"),
    (" Marked photo series ", " İşaretli foto serisi "),
    (
        "← ↑ → ↓ move · Enter or click open",
        "← ↑ → ↓ gez · Enter ya da tıkla aç",
    ),
    ("+/- size", "+/- boyut"),
    ("the series has no f{}", "seride f{} yok"),
    (
        "marked photo series closed",
        "işaretli foto serisi kapatıldı",
    ),
    ("r/R rotate", "r/R döndür"),
    ("O open outside", "O dışarıda aç"),
    ("m marked photo series", "m işaretli foto serisi"),
    (
        "no marked photo series to show",
        "gösterilecek işaretli foto serisi yok",
    ),
    ("Photos", "Fotoğraflar"),
    ("crop", "kesit"),
    ("whole", "tam"),
    // Details tabs
    ("Summary", "Özet"),
    ("Contents", "İçindekiler"),
    ("Suggestions", "Öneriler"),
    ("History", "Geçmiş"),
    (" H/L tabs ", " H/L sekme "),
    (" H/L tabs · J/K scroll ", " H/L sekme · J/K kaydır "),
    // History tab
    ("Today", "Bugün"),
    ("Yesterday", "Dün"),
    ("created", "oluşturuldu"),
    ("moved", "taşındı"),
    ("plan dropped", "plan bırakıldı"),
    ("moved as planned", "planıyla taşındı"),
    ("move planned", "taşıma planlandı"),
    ("plan cancelled", "plan iptal edildi"),
    ("changed", "değişti"),
    ("photo added", "fotoğraf eklendi"),
    ("photo removed", "fotoğraf çıkarıldı"),
    ("split into", "bölündü"),
    ("split from", "şuradan bölündü"),
    ("(a crop)", "(kesit)"),
    ("observed", "gözlem"),
    ("observation removed", "gözlem kaldırıldı"),
    ("reviewed", "tur"),
    ("toured", "sayıldı"),
    ("counting", "sayılıyor"),
    ("kept", "olduğu gibi bırakıldı"),
    ("raw", "sayılmadı"),
    ("not counted", "sayılmadı"),
    ("being counted", "sayılıyor"),
    ("counted", "sayıldı"),
    ("left as is", "olduğu gibi"),
    ("Unknown place", "Yeri bilinmiyor"),
    ("  (last seen in {})", "  (son görüldüğü yer: {})"),
    ("  (never seen anywhere)", "  (hiç görülmedi)"),
    ("set aside", "ayrıldı"),
    ("gone", "gitti"),
    ("restored", "geri alındı"),
    ("grid set", "ızgara"),
    ("found", "bulundu"),
    ("returned", "geri geldi"),
    ("event", "olay"),
    ("added here", "buraya eklendi"),
    ("planned to come", "gelecek (plan)"),
    ("came in", "geldi"),
    ("went out", "çıktı"),
    ("cleared", "silindi"),
    ("name", "ad"),
    ("(the photo could not be opened)", "(fotoğraf açılamadı)"),
    ("(opening the photo…)", "(fotoğraf açılıyor…)"),
    ("(empty)", "(boş)"),
    // Details pane fields
    ("kind", "tür"),
    ("code", "kod"),
    ("qty", "adet"),
    ("state", "durum"),
    ("candidate ({})", "aday ({})"),
    ("gone ({})", "gitti ({})"),
    ("lost", "kayıp"),
    ("empty", "boş"),
    ("  [empty]", "  [boş]"),
    ("empty (counted)", "boş (sayıldı)"),
    ("last seen: {}", "son görüldüğü: {}"),
    ("moving to", "gidecek"),
    ("to take to", "götürülecek"),
    ("owner", "sahibi"),
    ("lent to", "ödünçte"),
    ("theme", "tema"),
    ("note", "not"),
    ("make", "marka"),
    ("model", "model"),
    ("serial", "seri no"),
    // Documents
    ("invoice", "fatura"),
    ("warranty certificate", "garanti belgesi"),
    ("manual", "kılavuz"),
    ("service form", "servis formu"),
    ("appraisal", "ekspertiz"),
    ("policy", "poliçe"),
    ("other document", "diğer belge"),
    ("no {}", "no {}"),
    (
        "(already in the store; only new links were added)",
        "(zaten depoda; yalnız yeni bağlar eklendi)",
    ),
    ("file", "dosya"),
    ("(no documents)", "(belge yok)"),
    ("document", "belge"),
    // Purchases
    ("[dismissed: {}]", "[kapatıldı: {}]"),
    ("[{} open]", "[{} açık]"),
    ("[linked]", "[bağlı]"),
    ("[bought as kit {}]", "[{} kiti olarak alındı]"),
    ("the set: kit {}", "setin tamamı: {} kiti"),
    ("[returned]", "[iade]"),
    ("({} units each)", "(her biri {} adet)"),
    ("seller", "satıcı"),
    ("order", "sipariş"),
    ("order page", "sipariş sayfası"),
    ("product page", "ürün sayfası"),
    ("why", "neden"),
    (
        "{} new, {} updated, {} unchanged, {} skipped; {} document links, {} documents skipped; {} attachments; {} joined to another source's line",
        "{} yeni, {} güncellendi, {} aynı, {} atlandı; {} belge bağı, {} belge atlandı; {} ek; {} satır başka kaynağın satırına bağlandı",
    ),
    (
        "Fields ev does not read, left out: {}",
        "ev'nin okumadığı alanlar, alınmadı: {}",
    ),
    ("(no purchases)", "(alım yok)"),
    ("bought", "alım"),
    (
        "(no purchase could be this)",
        "(bu olabilecek bir alım yok)",
    ),
    (
        "Could be one of these purchases:",
        "Şu alımlardan biri olabilir:",
    ),
    (
        "{} of {} unlinked things in toured places could be a purchase, best first:",
        "Sayılmış yerlerde alıma bağlanmamış şeylerden {} tanesi bir alım olabilir ({} şeyden), en iyisi önce:",
    ),
    (
        "(none of {} unlinked things in toured places could be a purchase)",
        "(sayılmış yerlerde alıma bağlanmamış {} şeyden hiçbiri bir alım olamaz)",
    ),
    // Coverage
    ("Coverage ending", "Biten güvenceler"),
    ("Coverage not asked", "Garantisi sorulmamış"),
    (
        "valuable things (from {}) with no warranty or insurance recorded; the dearest:",
        "garanti ya da sigortası kayıtlı olmayan değerli eşyalar ({} ve üstü); en pahalılar:",
    ),
    ("active, lifetime", "geçerli, ömür boyu"),
    ("active until {}", "{} tarihine kadar geçerli"),
    (
        "ends {} ({} days left)",
        "{} tarihinde bitiyor ({} gün kaldı)",
    ),
    ("ended {}", "{} tarihinde bitti"),
    (
        "undetermined: no start known",
        "belirsiz: başlangıç bilinmiyor",
    ),
    ("statutory warranty", "yasal garanti"),
    ("manufacturer warranty", "üretici garantisi"),
    ("extended warranty", "uzatılmış garanti"),
    ("store warranty", "mağaza garantisi"),
    ("insurance", "sigorta"),
    ("+{} days in repair", "+{} gün tamirde"),
    ("scope", "kapsam"),
    ("premium", "prim"),
    ("(no coverage)", "(güvence yok)"),
    ("coverage #{} removed", "güvence #{} silindi"),
    ("(default)", "(varsayılan)"),
    ("coverage", "güvence"),
    (
        "proposed: statutory warranty until {} (2 years from delivery {}), not recorded",
        "öneri: {} tarihine kadar yasal garanti (teslimden {} itibaren 2 yıl), kaydedilmedi",
    ),
    ("value", "değer"),
    ("not now", "şimdilik değil"),
    ("not tracked", "takip edilmiyor"),
    // Condition at sale
    ("condition", "durum"),
    ("new", "yeni"),
    ("like new", "yeni gibi"),
    ("used", "kullanılmış"),
    // Attachments and joined lines
    ("the same purchase as #{}", "#{} ile aynı alım"),
    ("also seen as #{}", "#{} olarak da görüldü"),
    ("brought to #{}", "#{} eşyasına aktarıldı"),
    ("to bring", "aktarılacak"),
    ("attachment", "ek"),
    (
        "Brought from purchase #{}: {}",
        "#{} alımından aktarıldı: {}",
    ),
    (
        "Nothing brought: purchase #{} carries nothing to bring.",
        "Hiçbir şey aktarılmadı: #{} alımında aktarılacak bir şey yok.",
    ),
    (
        "Nothing brought from purchase #{}.",
        "#{} alımından hiçbir şey aktarılmadı.",
    ),
    ("already brought: {}", "zaten aktarılmış: {}"),
    (
        "left, of another type: {}",
        "başka türde olduğu için bırakıldı: {}",
    ),
    (
        "Nothing brought: {} linked lines checked, {} carry that type, {} of it brought already.",
        "Hiçbir şey aktarılmadı: {} bağlı satıra bakıldı, {} tanesinde bu türden ek var, {} ek zaten aktarılmış.",
    ),
    ("Brought from {} purchases: {}", "{} alımdan aktarıldı: {}"),
    ("#{} {} is gone", "#{} {} gitmiş"),
    ("linked to several things", "birden çok eşyaya bağlı"),
    (
        "left #{}: {}, {} waiting; ev buy bring {} <ref> brings it",
        "#{} bırakıldı: {}, {} ek bekliyor; ev buy bring {} <eşya> ile aktarılır",
    ),
    // Values and links
    ("  (+{} earlier)", "  (+{} önceki)"),
    ("(no links)", "(bağlantı yok)"),
    ("(no value recorded)", "(değer kaydı yok)"),
    ("Purchases to link", "Bağlanacak alımlar"),
    ("Value not asked", "Değeri sorulmamış"),
    ("about {}", "yaklaşık {}"),
    ("archive: {}", "arşiv: {}"),
    (
        "bought things with no value recorded; the dearest:",
        "değeri kaydedilmemiş alınmış eşyalar; en pahalıları:",
    ),
    ("driver", "sürücü"),
    (
        "durable purchase lines not linked to a thing yet: ev buy list --open --bucket durable",
        "henüz bir eşyaya bağlanmamış kalıcı alım satırları: ev buy list --open --bucket durable",
    ),
    ("info page", "ürün sayfası"),
    ("link", "bağlantı"),
    ("other", "diğer"),
    ("support", "destek"),
    // Money over time
    ("≈ {} in {} money", "≈ {} ({} parasıyla)"),
    // The details pane
    ("Documents", "Belgeler"),
    ("Money", "Para"),
    ("Coverage", "Güvence"),
    ("Note", "Not"),
    ("label to print", "etiket basılacak"),
    ("last seen", "son görüldü"),
    (
        "Documents ({}) · Links ({})",
        "Belgeler ({}) · Bağlantılar ({})",
    ),
    (
        "→ Documents tab (H/L), O opens",
        "→ Belgeler sekmesi (H/L), O açar",
    ),
    (
        "{} more on the places it is in",
        "bulunduğu yerlerde {} görev daha",
    ),
    ("opened {}", "açıldı: {}"),
    ("could not open {}: {}", "açılamadı: {} ({})"),
    ("Documents ({})", "Belgeler ({})"),
    ("Links ({})", "Bağlantılar ({})"),
    ("  (purchase #{})", "  (alım #{})"),
    ("  · archived", "  · arşivli"),
    (
        "[ ] pick · O or a click opens it",
        "[ ] seç · O ya da tıklama açar",
    ),
    ("lent", "ödünç verildi"),
    ("plan imported", "plan içe alındı"),
    ("not", "değil"),
    ("not this purchase", "bu alım değil"),
    ("purchase offered again", "alım yeniden önerilir"),
    ("[ ] O open a document", "[ ] O belge aç"),
    ("copied: {}", "kopyalandı: {}"),
    ("could not copy: {}", "kopyalanamadı: {}"),
    (
        "E empty · y copy · + wide",
        "E boşlar · y kopyala · + geniş",
    ),
    ("{} · {} rooms", "{} · {} oda"),
    ("fixed", "tamir edildi"),
    ("linked to purchase", "alıma bağlandı"),
    ("unlinked from purchase", "alımdan ayrıldı"),
    ("document added", "belge eklendi"),
    ("document removed", "belge çıkarıldı"),
    ("coverage added", "güvence eklendi"),
    ("coverage removed", "güvence silindi"),
    ("decided", "karar"),
    ("tracked again", "yeniden takipte"),
    ("{} ×{}", "{} ×{}"),
    ("(stale: fetch again)", "(eski: yeniden çek)"),
    (
        "{} index: {} periods, latest {}{}; {} rates, {} missing; home currency {}",
        "{} endeksi: {} dönem, son {}{}; {} kur, {} eksik; ev para birimi {}",
    ),
    (
        "{} index values, {} rates imported",
        "{} endeks değeri, {} kur alındı",
    ),
    ("address", "adres"),
    ("fill", "doluluk"),
    ("{}%", "%{}"),
    ("tags", "etiketler"),
    ("photo", "foto"),
    ("Attached {} photo(s):", "{} fotoğraf eklendi:"),
    (
        "Series grid: pictures {} cells wide",
        "Seri ızgarası: resimler {} hücre genişliğinde",
    ),
    ("photos", "fotoğraflar"),
    ("label", "etiket"),
    ("to print", "basılacak"),
    ("printed", "basıldı"),
    ("broken", "bozuk"),
    ("awaiting repair {}", "tamir bekliyor {}"),
    ("use-by", "son kullanma"),
    ("sale", "satış"),
    ("to get", "alınacak"),
    ("  (via {})", "  ({} üzerinden)"),
    ("task", "görev"),
    ("updated", "güncellendi"),
    ("Contents ({})", "İçindekiler ({})"),
    // Command-line text
    ("  (candidate: {})", "  (aday: {})"),
    ("  (gone: {})", "  (gitti: {})"),
    ("  (lost)", "  (kayıp)"),
    ("  (to: {})", "  (götürülecek: {})"),
    ("  (owner: {})", "  (sahibi: {})"),
    ("  (with: {})", "  (ödünçte: {})"),
    ("return (theirs)", "iade (onların)"),
    ("collect (lent)", "geri al (ödünç verilen)"),
    ("  [{} items]", "  [{} eşya]"),
    ("[counted, changed since]", "[sayıldı, sonra değişti]"),
    ("[counted]", "[sayıldı]"),
    ("counted, changed since", "sayıldı, sonra değişti"),
    ("[left as is]", "[olduğu gibi]"),
    ("[being counted]", "[sayılıyor]"),
    ("[not counted]", "[sayılmadı]"),
    (" (in progress)", " (sürüyor)"),
    (" (done)", " (bitti)"),
    (" (dropped)", " (bırakıldı)"),
    ("why: {}", "neden: {}"),
    (
        "{} places: {} counted, {} left as is, {} being counted, {} not counted; {} changed since counted",
        "{} yer: {} sayıldı, {} olduğu gibi, {} sayılıyor, {} sayılmadı; {} sayıldıktan sonra değişti",
    ),
    ("(not set)", "(belirlenmedi)"),
    (" (make)", " (yapılacak)"),
    ("  for {}", "  {} için"),
    ("Goal: {}", "Hedef: {}"),
    ("Tasks", "İşler"),
    ("Moves", "Taşımalar"),
    ("Labels to print", "Basılacak etiketler"),
    ("Broken", "Bozuk"),
    ("Use-by soon", "Son kullanma yaklaşan"),
    ("Not counted yet", "Henüz sayılmadı"),
    ("Changed since counted", "Sayıldıktan sonra değişti"),
    ("Unclear records", "Belirsiz kayıtlar"),
    (
        "Photo of the current state needed",
        "Son hâlinin fotoğrafı gerekli",
    ),
    ("  {} ({} days)", "  {} ({} gün)"),
    ("  (no photo)", "  (fotoğraf yok)"),
    (
        "  (photo marked out of date)",
        "  (fotoğrafı eski olarak işaretli)",
    ),
    (
        "{} more places get their photo on their tour",
        "{} yer daha fotoğrafını kendi turunda alacak",
    ),
    // `ev next`: due dates, what else to do in a place, notes on the order
    ("today", "bugün"),
    ("{} days overdue", "{} gün gecikti"),
    ("in {} days", "{} gün kaldı"),
    ("due {} ({})", "son tarih {} ({})"),
    ("due {}", "son tarih {}"),
    (
        "(first because it is due)",
        "(son tarihi geldiği için önde)",
    ),
    ("On the order:", "Sıra üzerine:"),
    ("settles {} planned moves", "{} planlı taşımayı tamamlar"),
    ("while there:", "oradayken:"),
    ("photo: {}", "fotoğraf: {}"),
    ("label: {}", "etiket: {}"),
    ("unclear: {}", "belirsiz: {}"),
    ("waiting for its place: {}", "yerini bekliyor: {}"),
    ("take along: {} → {}", "yanında götür: {} → {}"),
    ("leaving the home ({}): {}", "evden çıkıyor ({}): {}"),
    (
        "lost, last seen here: {}",
        "kayıp, en son burada görüldü: {}",
    ),
    ("ask about its warranty: {}", "garantisini sor: {}"),
    ("ask its value: {}", "değerini sor: {}"),
    ("  (changed {})", "  (değişti {})"),
    (
        "Whole photo shared by several records",
        "Birden çok kayıtta aynı tam fotoğraf",
    ),
    ("To get", "Alınacaklar"),
    ("(no labels to print)", "(basılacak etiket yok)"),
    ("(nothing to get)", "(alınacak bir şey yok)"),
    ("Next task ({} open):", "Sıradaki iş ({} açık):"),
    ("observed: {}", "gözlem: {}"),
    ("arriving: {}", "gelecek: {}"),
    ("(no open task)", "(açık iş yok)"),
    (
        "Raw places no task covers ({}):",
        "Hiçbir işin kapsamadığı gezilmemiş yerler ({}):",
    ),
    ("(no tasks)", "(iş yok)"),
    ("pending move → {}", "bekleyen taşıma → {}"),
    ("review: {} ({})", "sayım: {} ({})"),
    ("observed #{}: {}", "gözlem #{}: {}"),
    ("  (via #{})", "  (#{} üzerinden)"),
    ("Rules:", "Kurallar:"),
    // Placement
    ("(no synonyms)", "(eşanlamlı yok)"),
    ("room ({}% full)", "yer var (%{} dolu)"),
    ("little room ({}% full)", "az yer var (%{} dolu)"),
    ("full ({}%)", "dolu (%{})"),
    ("fill unknown", "doluluk bilinmiyor"),
    (", changed since", ", o zamandan beri değişti"),
    ("For: {}", "Yerleştirilecek: {}"),
    ("Words: {}", "Kelimeler: {}"),
    ("Synonyms added: {}", "Eklenen eşanlamlılar: {}"),
    (
        "No holder matches what this thing is: it likely needs a new group.",
        "Bu şeyin ne olduğuna uyan bir kap yok: büyük ihtimalle yeni bir grup gerekiyor.",
    ),
    (
        "Empty boxes to start it in:",
        "Başlatılabilecek boş kaplar:",
    ),
    ("  (same room)", "  (aynı oda)"),
    ("Best matches:", "En iyi eşleşmeler:"),
    ("score {} · covers {} · {}", "puan {} · kapsama {} · {}"),
    ("matched: {}", "eşleşen: {}"),
    ("a thing's name", "içindeki eşyanın adı"),
    ("a thing's note", "içindeki eşyanın notu"),
    ("a thing's tag", "içindeki eşyanın etiketi"),
    ("bought before for {}", "daha önce {} için alındı"),
    ("model {}", "model {}"),
    ("serial {}", "seri no {}"),
    ("part of the model {}", "modelin bir kısmı {}"),
    ("code {}", "kod {}"),
    ("brand {}", "marka {}"),
    ("words {}", "kelimeler {}"),
    (
        "words beside what it is {}",
        "ne olduğunun dışındaki kelimeler {}",
    ),
    ("{} differs", "{} farklı"),
    ("charge", "kapasite"),
    ("current", "akım"),
    ("frequency", "frekans"),
    ("length", "uzunluk"),
    ("mass", "ağırlık"),
    ("power", "güç"),
    ("storage", "depolama"),
    ("voltage", "gerilim"),
    ("volume", "hacim"),
    (
        "Scoring: a holder's theme ×3, name ×2.5, note ×1; the things inside it: name ×2, tags ×1.5, note ×1. Rare words weigh more, repeats less; * marks a word rare enough to say what the thing is.",
        "Puanlama: kabın teması ×3, adı ×2,5, notu ×1; içindekilerin adı ×2, etiketleri ×1,5, notu ×1. Nadir kelimeler daha ağır, tekrarlar daha hafif sayılır; * şeyin ne olduğunu söyleyecek kadar nadir kelimeyi işaretler.",
    ),
    (
        "{} of {} things are already in their best place.",
        "{} / {} şey zaten en uygun yerinde.",
    ),
    ("Would fit better elsewhere:", "Başka yere daha iyi uyar:"),
    ("Taken back from the thing: {}", "Eşyadan geri alındı: {}"),
    ("Part {} taken off: {}", "{}. parça listeden çıkarıldı: {}"),
    (
        "Not counted yet, left out of the draft: {}",
        "Henüz sayılmadı, taslak dışında: {}",
    ),
    (
        "Task #{} is still open on {}: {} (done, or drop it?)",
        "#{} görevi {} üzerinde hâlâ açık: {} (yapıldı mı, düşürülsün mü?)",
    ),
    ("Left on the thing: {} ({})", "Eşyada bırakıldı: {} ({})"),
    ("{}: {} places, {} things", "{}: {} yer, {} eşya"),
    (
        "Kinds spread over several places:",
        "Birkaç yere dağılmış türler:",
    ),
    ("Places that read alike:", "Birbirine benzeyen yerler:"),
    (
        "Nearly empty, could join another:",
        "Neredeyse boş, bir başkasına katılabilir:",
    ),
    ("({} things; shared: {})", "({} eşya; ortak: {})"),
    (
        "Full or mixed, could be split:",
        "Dolu ya da karışık, bölünebilir:",
    ),
    ("full", "dolu"),
    ("mixed", "karışık"),
    ("{} things", "{} eşya"),
    (
        "Draft layout (nothing is moved):",
        "Taslak düzen (hiçbir şey taşınmadı):",
    ),
    (
        "{} ({} things, {} to bring)",
        "{} ({} eşya, {} getirilecek)",
    ),
    ("from {}", "{} yerinden"),
    (
        "No place left for these, they stay where they are: {}",
        "Bunlara yer kalmadı, oldukları yerde kalır: {}",
    ),
    (
        "Named for another box's theme (worth a look):",
        "Adı başka bir kutunun temasına uyuyor (bakılmaya değer):",
    ),
    ("Full:", "Dolu:"),
    (
        "no bigger spare box recorded",
        "kayıtlı daha büyük boş kap yok",
    ),
    ("no free cell for it", "ona yetecek boş hücre yok"),
    ("fits at {}", "sığdığı yer: {}"),
    ("bigger spare: {} {} · {}", "daha büyük boş kap: {} {} · {}"),
    ("Nearly empty:", "Neredeyse boş:"),
    ("could merge into {}", "şuna birleşebilir: {}"),
    ("no similar box with room", "yeri olan benzer kutu yok"),
    (
        "Mixed (half or more fit better elsewhere):",
        "Karışık (yarısı ya da fazlası başka yere daha iyi uyar):",
    ),
    (
        "Fill unknown or out of date:",
        "Doluluk bilinmiyor ya da eskimiş:",
    ),
    ("size", "boyut"),
    (
        "All {} places that can hold something:",
        "Bir şey alabilecek {} yerin hepsi:",
    ),
    ("{} items", "{} eşya"),
    (
        "Alike things in several places:",
        "Birden çok yere dağılmış benzer şeyler:",
    ),
    ("Holders without a theme:", "Teması olmayan kaplar:"),
    (
        "Items lying loose in a room or on furniture:",
        "Odada ya da mobilya üstünde açıkta duran eşyalar:",
    ),
    (
        "(nothing to take, return or collect)",
        "(götürülecek, iade edilecek ya da geri alınacak bir şey yok)",
    ),
    (
        "take {} · return {} · collect {}",
        "götür {} · iade {} · geri al {}",
    ),
    ("(none)", "(yok)"),
    ("(nothing changed)", "(değişen bir şey yok)"),
    ("(unchanged)", "(değişmedi)"),
    // Statistics
    ("OVERVIEW", "GENEL"),
    ("{} records of things, {} units", "{} eşya kaydı, {} adet"),
    (
        "{} rooms · {} pieces of furniture · {} containers",
        "{} oda · {} mobilya · {} kap",
    ),
    (
        "{} things kept in several places",
        "{} eşya birden çok yerde",
    ),
    (
        "{} records with a photo · {} documents",
        "{} kaydın fotoğrafı var · {} belge",
    ),
    ("WHAT IT COST", "NEYE MAL OLDU"),
    (
        "What the things here cost, by their linked purchases: {}",
        "Evdeki eşyaların bağlı alımlara göre tutarı: {}",
    ),
    ("known for {} of {} records", "{} / {} kayıt için biliniyor"),
    (
        "in today's money: {} ({} of {} lines)",
        "bugünün parasıyla: {} ({} / {} satır)",
    ),
    (
        "latest values recorded: {} things, {}",
        "son kaydedilen değerler: {} eşya, {}",
    ),
    ("ROOMS", "ODALAR"),
    (
        "{} rooms with nothing recorded yet",
        "{} odada henüz kayıt yok",
    ),
    (" (today {})", " (bugün {})"),
    ("no date", "tarihsiz"),
    (
        "{}: {} records, {} units, {} holders · {}",
        "{}: {} kayıt, {} adet, {} kap · {}",
    ),
    ("COUNTING", "SAYIM"),
    (
        "{} of {} places counted · {} being counted · {} not counted",
        "{} / {} yer sayıldı · {} sayılıyor · {} sayılmadı",
    ),
    (
        "{} changed since they were counted",
        "{} yer sayıldıktan sonra değişti",
    ),
    (
        "{} of {} records are in counted places ({}%)",
        "{} / {} kayıt sayılmış yerlerde (%{})",
    ),
    ("PURCHASES", "ALIMLAR"),
    (
        "{} lines · {} linked · {} settled · {} durable still open",
        "{} satır · {} bağlı · {} kapatıldı · {} kalıcı eşya satırı açık",
    ),
    ("  {}: {} lines · {}", "  {}: {} satır · {}"),
    ("LAST 30 DAYS", "SON 30 GÜN"),
    (
        "{} added · {} moves · {} photos",
        "{} eklendi · {} taşıma · {} fotoğraf",
    ),
    ("left the home: {}", "evden çıkan: {}"),
    ("busiest day: {} ({} events)", "en yoğun gün: {} ({} olay)"),
    ("BOXES", "KAPLAR"),
    (
        "{} containers · {} known to be empty · {} with nothing recorded, never counted",
        "{} kap · {} boş olduğu biliniyor · {} içinde kayıt yok ama hiç sayılmadı",
    ),
    (
        "{} with a fill, {}% on average · {} full",
        "{} kabın doluluğu var, ortalama %{} · {} dolu",
    ),
    ("  {} records  {}", "  {} kayıt  {}"),
    ("COVERAGE", "GÜVENCE"),
    (
        "{} active of {} · {} ending soon",
        "{} / {} geçerli · {} yakında bitiyor",
    ),
    ("TAGS", "ETİKETLER"),
    ("BOUGHT LONGEST AGO", "EN ESKİ ALINANLAR"),
    ("Empty, on the person's word:", "Kişinin sözüyle boş:"),
    (
        "Nothing recorded in these, but never counted: empty is not known",
        "Bunlarda kayıtlı bir şey yok ama hiç sayılmadılar: boş oldukları bilinmiyor",
    ),
    ("(no pending moves)", "(bekleyen taşıma yok)"),
    ("(nothing lost)", "(kayıp bir şey yok)"),
    ("holds {}", "içinde: {}"),
    ("photo {}, crop {}", "fotoğraf {}, kesit {}"),
    ("photo {}, whole", "fotoğraf {}, tam"),
    ("Preview: {}", "Önizleme: {}"),
    ("Numbered photo: {}", "Numaralı fotoğraf: {}"),
    ("Contact sheet: {}", "Kesit sayfası: {}"),
    ("Sent to ev ui.", "ev ui'a gönderildi."),
    ("joined another portion", "başka bir porsiyona katıldı"),
    ("used up", "kullanılıp bitti"),
    ("left behind", "geride bırakıldı"),
    ("moved out", "taşınıldı"),
    ("made from the place", "şu yerden yapıldı"),
    ("stolen", "çalındı"),
    ("left, how not known", "gitti, nasıl bilinmiyor"),
    ("came", "geldi"),
    ("left", "gitti"),
    ("[gone]", "[gitti]"),
    ("in {}", "{} içinde"),
    ("sold", "satıldı"),
    ("thrown out", "atıldı"),
    ("given away", "verildi"),
    ("returned to the shop", "iade edildi"),
    ("photographed, then thrown out", "fotoğraflanıp atıldı"),
    ("how not known", "nasıl bilinmiyor"),
    ("trade", "takas"),
    ("things", "eşya"),
    ("photo {} of {}", "fotoğraf {}/{}"),
    (
        "#{} is back to open, not finished",
        "#{} yeniden açık, bitmedi",
    ),
    ("was in {}", "{} içindeydi"),
    ("photo turned", "fotoğraf döndürüldü"),
    ("{}° clockwise", "saat yönünde {}°"),
    ("part moved out", "bir kısmı çıktı"),
    ("part came in", "bir kısmı geldi"),
    ("portion joined", "bir porsiyon katıldı"),
    ("more of", "aynısından"),
    ("joined to a thing", "bir şeye katıldı"),
    ("taken from a thing", "bir şeyden ayrıldı"),
    ("found empty", "boş bulundu"),
    ("lifetime", "ömür boyu"),
    ("year", "yıl"),
    ("years", "yıl"),
    ("month", "ay"),
    ("months", "ay"),
    ("week", "hafta"),
    ("weeks", "hafta"),
    ("day", "gün"),
    ("days", "gün"),
    ("still to get", "alınacak"),
    ("got", "alındı"),
    ("dropped", "vazgeçildi"),
    ("photo still current", "fotoğraf hâlâ güncel"),
    ("photo out of date", "fotoğraf eskidi"),
    ("with", "ödünçte"),
    ("temporary", "geçici"),
    ("left from", "gittiği yer"),
    ("consumed", "tüketildi"),
    ("given", "verildi"),
    ("not mine", "benim değil"),
    ("a duplicate", "mükerrer"),
    ("[bought as coverage {}]", "[güvence {} olarak alındı]"),
    ("bought as", "satın alındığı satır"),
    (
        "Not shown on the final photo yet ({} of {}): say which is which",
        "Son fotoğrafta yeri henüz gösterilmeyen ({} / {}): hangisi hangisi, söyle",
    ),
    ("coverage bought as", "güvencenin satırı"),
    ("kit bought as", "setin satırı"),
    ("line", "satır"),
    ("taken back", "geri alındı"),
    ("clothing", "giyim"),
    ("digital", "dijital"),
    ("services", "hizmet"),
    ("traded", "takas edildi"),
    ("for {}", "{} karşılığında"),
    ("for #{} {}", "#{} {} karşılığında"),
    ("for #{}", "#{} karşılığında"),
    ("came in a trade for", "takasla geldi, karşılığında giden"),
    ("via {}", "{} üzerinden"),
    ("(nothing past)", "(giden bir şey yok)"),
    ("PAST", "GİDENLER"),
    ("when not known", "ne zaman bilinmiyor"),
    ("Remembered ({})", "Hatırlananlar ({})"),
    ("Left the inventory ({})", "Envanterden çıkanlar ({})"),
    ("Still not counted in {}:", "{} içinde hâlâ sayılmamış:"),
    (
        "Still not counted elsewhere in {}:",
        "{} içinde başka yerlerde hâlâ sayılmamış:",
    ),
    ("in no task", "hiçbir görevde değil"),
    (
        "Not counted in the same furniture, outside this task ({}):",
        "Aynı mobilyada bu görevin dışında sayılmamış ({}):",
    ),
    (
        "{} things that were ours: {}",
        "bir zamanlar bizim olan {} şey: {}",
    ),
    ("paid for them: {}", "onlara ödenen: {}"),
    ("got for them: {}", "onlardan alınan: {}"),
    ("{} left", "{} gitti"),
    ("paid {}", "ödenen {}"),
    ("got {}", "alınan {}"),
    ("Homes and vehicles ({})", "Evler ve araçlar ({})"),
    ("since {}", "{} tarihinden beri"),
    ("ours now", "şu an bizde"),
    ("{} today", "bugün {}"),
    ("came {}", "geldi {}"),
    ("left {}", "gitti {}"),
    ("ours in {}: {}", "{} yılında bizimdi: {}"),
    (
        "{} more are not counted: nothing says when they came, or when they left",
        "ne zaman geldiği ya da gittiği bilinmeyen {} şey bu listede yok",
    ),
    (
        "thing: ×{} in {} places · in use {} · spare {}",
        "eşya: ×{}, {} yerde · kullanımda {} · yedek {}",
    ),
    (" · lost {}", " · kayıp {}"),
    ("elsewhere: #{} {} ×{}", "başka yerde: #{} {} ×{}"),
    (" (in use)", " (kullanımda)"),
    ("in all", "toplam"),
    ("product image", "ürün görseli"),
    ("ordered {} · delivered {}", "sipariş {} · teslim {}"),
    ("ordered {}", "sipariş {}"),
    ("delivered {}", "teslim {}"),
    ("kept in one place", "tek yerde duruyor"),
    ("p next place of this thing", "p bu eşyanın sonraki yeri"),
    (
        "{} ×{} in {} places · in use {} · spare {}",
        "{} ×{}, {} yerde · kullanımda {} · yedek {}",
    ),
    ("  (on #{})", "  (#{} üzerinde)"),
    ("bought {}", "alınan {}"),
    ("here {}", "burada {}"),
    ("gone: {}", "giden: {}"),
    ("{} unaccounted for", "{} hesapta yok"),
    ("{} more than bought", "alınandan {} fazla"),
    ("accounted: {}", "hesap: {}"),
    (
        "The same name in more than one place, one thing? `ev join`:",
        "Aynı ad birden çok yerde, tek eşya mı? `ev join`:",
    ),
    (
        "×{} in {} places · in use {} · spare {}",
        "×{}, {} yerde · kullanımda {} · yedek {}",
    ),
    ("elsewhere", "başka yerde"),
    ("task #{}: {}", "görev #{}: {}"),
    ("(order {})", "(sıra {})"),
    (
        "{} index from {}; home currency {}, country {}",
        "{} endeksi, {} itibarıyla; ev para birimi {}, ülke {}",
    ),
    ("No exchange rate missing.", "Eksik döviz kuru yok."),
    ("Exchange rates missing ({}):", "Eksik döviz kurları ({}):"),
    (
        "Sent to ev ui: #{}, photo {}",
        "ev ui'a gönderildi: #{}, {}. fotoğraf",
    ),
    ("Sent to ev ui: #{}", "ev ui'a gönderildi: #{}"),
    (
        "Sent to ev ui: {} picture(s), {} in the series",
        "ev ui'a gönderildi: {} resim, seride {}",
    ),
    (
        "No marked photo series in ev ui.",
        "ev ui'da işaretli foto serisi yok.",
    ),
    ("Next number: {}", "Sıradaki numara: {}"),
    (
        "The request to ev ui is cleared.",
        "ev ui'a gönderilen istek silindi.",
    ),
    ("  (no photos)", "  (fotoğraf yok)"),
    ("  crop {}", "  kesit {}"),
    (
        "turned {}° clockwise, on:",
        "saat yönünde {}° döndürüldü, şunlarda:",
    ),
    ("  (file missing)", "  (dosya yok)"),
    // Grids
    (
        "{}×{} grid, row 1 at the back",
        "{}×{} ızgara, 1. satır arkada",
    ),
    ("free ({}): {}", "boş ({}): {}"),
    ("not placed in a cell: {}", "hücreye yerleştirilmemiş: {}"),
    ("(no grid)", "(ızgarası yok)"),
    ("cells", "hücreler"),
    ("Grid", "Izgara"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every string literal passed to `t(` or `tf(` in these sources.
    fn keys_in(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for start in ["t(\"", "tf(\"", "t(\n", "tf(\n"] {
            let mut rest = src;
            while let Some(i) = rest.find(start) {
                let before = rest[..i].chars().last();
                rest = &rest[i + start.len()..];
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let body = if start.ends_with('\n') {
                    let trimmed = rest.trim_start();
                    match trimmed.strip_prefix('"') {
                        Some(b) => b,
                        None => continue,
                    }
                } else {
                    rest
                };
                let mut key = String::new();
                let mut chars = body.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => match chars.next() {
                            Some('n') => key.push('\n'),
                            Some(other) => key.push(other),
                            None => break,
                        },
                        '"' => break,
                        c => key.push(c),
                    }
                }
                out.push(key);
            }
        }
        out
    }

    #[test]
    fn every_error_id_has_a_turkish_sentence_with_the_same_values() {
        for (id, en) in ev_core::ERRORS {
            let tr = super::ERRORS_TR
                .iter()
                .find(|(i, _)| i == id)
                .map(|(_, t)| *t);
            let Some(tr) = tr else {
                panic!("no Turkish for the error `{id}`");
            };
            let mut a = ev_core::placeholders(en);
            let mut b = ev_core::placeholders(tr);
            a.sort_unstable();
            b.sort_unstable();
            a.dedup();
            b.dedup();
            assert_eq!(a, b, "{id}");
        }
        for (id, _) in super::ERRORS_TR {
            assert!(
                ev_core::template(id).is_some(),
                "`{id}` is no error of core"
            );
        }
    }

    #[test]
    fn every_text_in_the_code_has_a_turkish_translation() {
        // Every file that shows words; the ui's own were missed for a while when it was split.
        let sources = [
            include_str!("ui.rs"),
            include_str!("ui/details.rs"),
            include_str!("ui/draw.rs"),
            include_str!("ui/buys.rs"),
            include_str!("ui/commands.rs"),
            include_str!("ui/events.rs"),
            include_str!("ui/palette.rs"),
            include_str!("ui/prefs.rs"),
            include_str!("ui/rows.rs"),
            include_str!("render.rs"),
            include_str!("history.rs"),
            include_str!("mapview.rs"),
            include_str!("settings.rs"),
        ];
        let map = turkish();
        let mut missing: Vec<String> = sources
            .iter()
            .flat_map(|s| keys_in(s))
            .filter(|k| !map.contains_key(k.as_str()))
            .collect();
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "no Turkish for: {missing:#?}");
    }

    #[test]
    fn translations_keep_their_placeholders_and_keys_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for (en, tr) in TR {
            assert!(seen.insert(*en), "duplicate key {en:?}");
            assert_eq!(
                en.matches("{}").count(),
                tr.matches("{}").count(),
                "{en:?} → {tr:?}"
            );
        }
    }

    #[test]
    fn text_follows_the_language_and_falls_back_to_english() {
        set_lang(Lang::Tr);
        assert_eq!(t("Layout"), "Yerleşim");
        assert_eq!(tf("  [with {}]", &[&"Annemler"]), "  [Annemler'de]");
        assert_eq!(t("no such text"), "no such text");
        set_lang(Lang::En);
        assert_eq!(tf("{}%", &[&40]), "40%");
        set_lang(Lang::Tr);
        assert_eq!(tf("{}%", &[&40]), "%40");
        set_lang(Lang::En);
    }

    #[test]
    fn the_system_language_is_one_ev_speaks() {
        // Whatever this machine prefers, it maps onto English or Turkish, never fails. The one
        // test that asks the system itself.
        assert!(matches!(super::asked_system_lang(), Lang::En | Lang::Tr));
        assert_eq!(system_lang(), Lang::En);
    }
    #[test]
    fn only_the_language_part_of_a_tag_counts() {
        assert_eq!(Lang::from_tag("tr-TR"), Lang::Tr);
        assert_eq!(Lang::from_tag("en-TR"), Lang::En);
        assert_eq!(Lang::from_tag("tr_TR.UTF-8"), Lang::Tr);
        assert_eq!(Lang::from_tag("de-DE"), Lang::En);
    }
}
