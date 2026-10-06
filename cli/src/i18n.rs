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
    *CACHE.get_or_init(|| {
        sys_locale::get_locale()
            .map(|tag| Lang::from_tag(&tag))
            .unwrap_or(Lang::En)
    })
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
    ("at {},{} cm", "{},{} cm'de"),
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
        "↑↓ move · Enter/→ next option · ← previous option · Tab/1-0 tabs · q quit    {}",
        "↑↓ gez · Enter/→ sonraki seçenek · ← önceki seçenek · Tab/1-0 sekme · q çık    {}",
    ),
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
    ("H/L details tabs", "H/L ayrıntı sekmesi"),
    ("J/K scroll", "J/K kaydır"),
    ("[ ] o photos", "[ ] o fotoğraf"),
    ("Tab/1-0 tabs", "Tab/1-0 sekme"),
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
    ("#{} ×{}", "#{} ×{}"),
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
    ("stolen", "çalındı"),
    ("left, how not known", "gitti, nasıl bilinmiyor"),
    ("came", "geldi"),
    ("left", "gitti"),
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
    fn every_text_in_the_code_has_a_turkish_translation() {
        let sources = [
            include_str!("ui.rs"),
            include_str!("render.rs"),
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
        // Whatever this machine prefers, it maps onto English or Turkish, never fails.
        assert!(matches!(system_lang(), Lang::En | Lang::Tr));
    }
    #[test]
    fn only_the_language_part_of_a_tag_counts() {
        assert_eq!(Lang::from_tag("tr-TR"), Lang::Tr);
        assert_eq!(Lang::from_tag("en-TR"), Lang::En);
        assert_eq!(Lang::from_tag("tr_TR.UTF-8"), Lang::Tr);
        assert_eq!(Lang::from_tag("de-DE"), Lang::En);
    }
}
