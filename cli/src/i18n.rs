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
    // Words for stored values
    ("trash", "çöp"),
    ("give", "ver"),
    ("sell", "sat"),
    ("return", "iade"),
    ("record error", "kayıt hatası"),
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
    (" {} · Photo {}/{} ", " {} · Fotoğraf {}/{} "),
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
        "↑↓ move · Enter/→ next option · ← previous option · Tab/1-8 tabs · q quit    {}",
        "↑↓ gez · Enter/→ sonraki seçenek · ← önceki seçenek · Tab/1-8 sekme · q çık    {}",
    ),
    // The key hints under the panes, one part each so the line fits the width.
    ("↑↓ move", "↑↓ gez"),
    ("→ ← open/close", "→ ← aç/kapat"),
    ("Enter show in tree", "Enter ağaçta göster"),
    ("Enter open/close section", "Enter bölümü aç/kapat"),
    ("x clear", "x temizle"),
    ("/ search", "/ ara"),
    ("H/L details tabs", "H/L ayrıntı sekmesi"),
    ("J/K scroll", "J/K kaydır"),
    ("[ ] o photos", "[ ] o fotoğraf"),
    ("Tab/1-8 tabs", "Tab/1-8 sekme"),
    ("< > { } or drag: resize", "< > { } ya da sürükle: boyut"),
    ("q quit", "q çık"),
    (
        "#{} is no longer in the tree (gone)",
        "#{} artık ağaçta yok (gitti)",
    ),
    (" Marked photo ", " İşaretli fotoğraf "),
    ("Esc/o close", "Esc/o kapat"),
    ("[ ] ← → step", "[ ] ← → gez"),
    ("m opens it again later", "kapatınca m yeniden açar"),
    ("r/R rotate", "r/R döndür"),
    ("O open outside", "O dışarıda aç"),
    ("m marked photos", "m işaretli foto"),
    (
        "no marked photos to show",
        "gösterilecek işaretli fotoğraf yok",
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
    ("(empty)", "(boş)"),
    // Details pane fields
    ("kind", "tür"),
    ("code", "kod"),
    ("qty", "adet"),
    ("state", "durum"),
    ("candidate ({})", "aday ({})"),
    ("gone ({})", "gitti ({})"),
    ("lost", "kayıp"),
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
    ("address", "adres"),
    ("fill", "doluluk"),
    ("{}%", "%{}"),
    ("tags", "etiketler"),
    ("photo", "foto"),
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
    ("review: {} ({})", "gezme: {} ({})"),
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
    ("Best matches:", "En iyi eşleşmeler:"),
    ("score {} · covers {} · {}", "puan {} · kapsama {} · {}"),
    ("matched: {}", "eşleşen: {}"),
    (
        "Scoring: a holder's theme ×3, name ×2.5, note ×1; the things inside it: name ×2, tags ×1.5, note ×1. Rare words weigh more, repeats less; * marks a word rare enough to say what the thing is.",
        "Puanlama: kabın teması ×3, adı ×2,5, notu ×1; içindekilerin adı ×2, etiketleri ×1,5, notu ×1. Nadir kelimeler daha ağır, tekrarlar daha hafif sayılır; * şeyin ne olduğunu söyleyecek kadar nadir kelimeyi işaretler.",
    ),
    (
        "{} of {} things are already in their best place.",
        "{} / {} şey zaten en uygun yerinde.",
    ),
    ("Would fit better elsewhere:", "Başka yere daha iyi uyar:"),
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
    ("(no pending moves)", "(bekleyen taşıma yok)"),
    ("(nothing lost)", "(kayıp bir şey yok)"),
    ("holds {}", "içinde: {}"),
    ("photo {}, crop {}", "fotoğraf {}, kesit {}"),
    ("photo {}, whole", "fotoğraf {}, tam"),
    ("  (no photos)", "  (fotoğraf yok)"),
    ("  crop {}", "  kesit {}"),
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
        assert_eq!(tf("  [with {}]", &[&"Mahmutlar"]), "  [Mahmutlar'de]");
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
