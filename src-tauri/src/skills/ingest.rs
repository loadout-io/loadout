//! Wciąganie umiejętności z linku: nieufna treść, wykrycie wstrzyknięcia, dowód, że działa.
//!
//! Tu jest cała prawdziwa trudność tej funkcji [T5 §5, ARCHITECTURE §9]: **umiejętność jest
//! z definicji zbiorem instrukcji, które agent wykona**, więc wklejony link to wstrzyknięcie
//! z gotowym kanałem dostarczenia, a dołączone `scripts/` to dodatkowo wektor uruchomienia kodu.
//!
//! **Kolejność potoku jest częścią kontraktu: dekoduj → normalizuj → skanuj → zapisz to samo,
//! co skanowałeś.** Cicha porażka numer jeden bierze się z odwrócenia dwóch środkowych kroków:
//! skan biegnie na tekście surowym, a na dysk idzie tekst znormalizowany. Wtedy `ig<ZWJ>nore all
//! previous instructions` nie pasuje do żadnej reguły przed usunięciem zero-width joinera
//! i pasuje do wszystkich po — skaner mówi „czysto", a plik, który dostanie model, zawiera atak.
//!
//! Cicha porażka numer dwa jest lustrzana i tak samo kosztowna: skaner zapalający się na słowie
//! `instructions` zamienia ostrzeżenie w tło. Po trzech fałszywych alarmach człowiek klika
//! „Add" bez czytania i mechanizm przestaje istnieć — dlatego reguły opisują KSZTAŁT linii,
//! nigdy worek słów, i dlatego waga zależy od tego, czy linia stoi w bloku kodu.
//!
//! Trzecia: brak skanera renderowany jako „no problems found", czyli nieobecność dowodu
//! zamieniona w dowód nieobecności. Dlatego [`DeepScan::Unavailable`] nigdy nie daje
//! [`Verdict::Clean`].
//!
//! Niezmiennik 23 rządzi układem tego pliku: reguły R1–R5 żyją w JEDNEJ funkcji nad tekstem,
//! a `oxidized-agentic-audit` jest adapterem, który znaleziska **dokłada** i nigdy ich nie
//! zastępuje. „Skaner to załatwia" jest tym, jak przy pierwszym biegu bez binarki nie zostaje
//! żadna reguła — tak umarło skanowanie sekretów w meetnotes (PR #535).
//!
//! Sieci tu nie ma jako biblioteki: `src-tauri/Cargo.toml` nie należy do T-19, więc bajty
//! pobiera `curl` przez [`build_fetch_command`]. Flagi narzędzia **nie są dowodem**
//! (niezmiennik 20, blizna z raportu 06: `--sandbox workspace-write` w komentarzu przy żywym
//! `danger-full-access`) — każdy limit jest sprawdzany jeszcze raz u siebie, po fakcie, na tym,
//! co faktycznie przyszło: [`follow_policy`], [`read_capped`], [`total_within`].

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::place::Discovery;
use super::{BundledFile, Skill, SkillDoc, place};

/// Nazwa pliku, od którego zaczyna się każda umiejętność [T5 §2.2].
const SKILL_FILE: &str = "SKILL.md";

// ── Rdzeń reguł: pięć id, dwie wagi ────────────────────────────────────────────────────────
//
// Id są łańcuchami, a nie wariantami enuma, z jednego powodu: adapter skanera przynosi własne
// id z JSON-a i ma je nieść dalej bez tłumaczenia (niezmiennik 5 — nieznana reguła to
// znalezisko, nie panika). Jeden typ [`Finding`] dla obu źródeł znaczy, że karta przeglądu
// ma jedną listę do pokazania, a nie dwie, które trzeba scalić na ekranie.

/// R1. Komentarz HTML, znak zero-width (`200B–200D`, `FEFF`, `2060`), sterujące bidi
/// (`202A–202E`, `2066–2069`), mieszanka pism (homoglif).
///
/// DLACZEGO `Block`, a nie `Warn`: tekst, który renderuje się inaczej, niż się parsuje, jest
/// całą grą w tej klasie ataków [T5 §5.4]. Człowiek czytający kartę przeglądu widzi zdanie,
/// którego w pliku nie ma — więc jego zgoda dotyczy czegoś innego niż to, co dostanie model.
pub const HIDDEN_TEXT: &str = "hidden-text";

/// R2. „ignore/disregard/forget" + „previous/prior/above/all" + „instructions/rules/prompt"
/// w jednej linii.
///
/// DLACZEGO `Block` poza blokiem kodu: to jest dosłowna treść ataku, wykonywalna przez model
/// bez żadnego dalszego kroku. DLACZEGO tylko `Warn` w bloku kodu albo w cytacie: umiejętność
/// **o obronie** przed wstrzyknięciem cytuje tę linię jako przykład i musi dać się
/// zainstalować, bo inaczej po trzecim fałszywym alarmie nikt już karty nie czyta.
pub const INSTRUCTION_OVERRIDE: &str = "instruction-override";

/// R3. Linia wysyłająca (`curl`/`wget`/`nc`/`scp`/`git push`) razem ze źródłem sekretu
/// (`.env`, `~/.ssh`, `id_rsa`, `*_API_KEY`, `credentials`, `$(cat …)`).
///
/// DLACZEGO oba człony naraz, a nie samo `curl`: dokumentacja API jest pełna `curl -X POST`
/// i skaner zapalający się na samym poleceniu jest bezwartościowy po trzecim imporcie.
/// DLACZEGO `Block`: sekret wychodzi z maszyny raz i nie da się go cofnąć.
pub const EXFILTRATION: &str = "exfiltration";

/// R4. `<system>`, `system:`, `assistant:` jako znacznik tury, „you are now".
///
/// DLACZEGO `Block`: to jest próba przejęcia ramki rozmowy, czyli tego jedynego miejsca,
/// w którym rozstrzyga się, czyje instrukcje model uzna za swoje.
pub const ROLE_MANIPULATION: &str = "role-manipulation";

/// R5. Front-matter z `allowed-tools` albo `hooks` w imporcie.
///
/// DLACZEGO tylko `Warn`: `hooks` zdejmuje emiter T-18, więc do pliku na dysku to pole i tak
/// nie dojedzie. Fakt zostaje w znalezisku, bo umiejętność, która PRÓBOWAŁA przynieść własny
/// hak, mówi coś o swoim autorze.
pub const ESCALATION: &str = "escalation";

/// Znalezisko, którego nie ma w tabeli reguł: głęboki skan nie pobiegł.
///
/// Nieobecność dowodu nie jest dowodem nieobecności. To jedyny powód, dla którego ten łańcuch
/// istnieje jako id: żeby brak skanera był POZYCJĄ NA LIŚCIE, a nie brakiem pozycji.
pub const DEEP_SCAN_UNAVAILABLE: &str = "deep-scan-unavailable";

/// Dwie wagi i ani jednej więcej. Trzecia („info") jest tym, jak lista znalezisk przestaje
/// być czytana.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    /// Człowiek ma to zobaczyć. Nie zatrzymuje instalacji.
    Warn,
    /// Instalacja czeka, aż człowiek to przeczyta (`acknowledge`).
    Block,
}

/// Skąd wzięło się znalezisko. Pole istnieje wyłącznie po to, żeby dało się DOWIEŚĆ
/// niezmiennika 23: zbiór znalezisk rdzenia jest ten sam, kiedy skaner jest, i kiedy go nie ma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Pięć reguł z tego pliku.
    Rules,
    /// Adapter `oxidized-agentic-audit`.
    DeepScan,
}

/// Jedno znalezisko: która reguła, jak ciężko, w której linii i co dokładnie tam stało.
///
/// Jedno znalezisko na parę (reguła, linia). Dwa zapisy tej samej reguły w tej samej linii to
/// dwa wiersze na karcie mówiące o jednej rzeczy — a karta, która się powtarza, uczy przewijać.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Id reguły: jedna z pięciu stałych wyżej, [`DEEP_SCAN_UNAVAILABLE`], albo cokolwiek
    /// przyniósł skaner.
    pub rule: String,
    pub weight: Weight,
    /// Numer linii w ciele, które zapisujemy, liczony od 1. `None`, kiedy znalezisko nie
    /// dotyczy żadnej konkretnej linii (brak skanera).
    pub line: Option<usize>,
    /// Linia zacytowana dosłownie. Człowiek ma przeczytać atak, nie jego opis.
    pub quoted: String,
    /// Tekst, który został ZDJĘTY z ciała — treść komentarza HTML albo napis odzyskany po
    /// usunięciu znaków niewidzialnych.
    ///
    /// `Some` wyłącznie dla [`HIDDEN_TEXT`]: pozostałe cztery reguły niczego nie usuwają,
    /// bo skasowanie linii ataku ukryłoby atak przed człowiekiem.
    pub recovered: Option<String>,
    pub source: Source,
}

/// Trzy stany, w jakich może być import. Nie ma czwartego i nie ma „prawie czysto".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Zero znalezisk.
    Clean,
    /// Same [`Weight::Warn`].
    Concerns,
    /// Co najmniej jedno [`Weight::Block`].
    Blocked,
}

/// Treść po przejściu potoku i wszystko, co po drodze o niej zauważyliśmy.
#[derive(Debug, Clone)]
pub struct Reviewed {
    /// Ciało dokładnie takie, jakie pójdzie na dysk — i dokładnie to, które przeskanowaliśmy.
    /// To jest cała treść zdania „zapisz to samo, co skanowałeś".
    pub body: String,
    pub findings: Vec<Finding>,
    pub verdict: Verdict,
}

/// Dekoduj → normalizuj → skanuj. Zwraca ciało do zapisania razem ze znaleziskami, które
/// padły NA TYM ciele.
///
/// Jedna funkcja, bo rozdzielenie „znormalizuj" od „przeskanuj" na dwa publiczne wywołania
/// jest zaproszeniem do wywołania ich w złej kolejności — a zła kolejność jest tu cichą
/// porażką numer jeden, nie literówką.
///
/// Znaki niewidzialne i komentarze HTML **znikają** z ciała, a ich treść wraca w polu
/// [`Finding::recovered`]. Linie ataku pozostałych czterech reguł zostają w ciele
/// **dosłownie**: usunięcie ich ukryłoby atak przed jedynym czytelnikiem, który może go
/// rozpoznać.
#[must_use]
pub fn review(raw: &str) -> Reviewed {
    // KROK 1 i 2 — normalizacja. Najpierw komentarze, potem znaki niewidzialne: komentarz
    // rozbity zero-width joinerem (`<!\u{200b}--`) nie jest komentarzem dla przeglądarki,
    // ale jest nim dla oka, więc kolejność odwrotna zostawiałaby tę parę bez znaleziska.
    let (uncommented, comments) = strip_comments(raw);
    let (body, invisible) = strip_invisible(&uncommented);

    // KROK 3 — skan. Biegnie po `body`, czyli po tym, co za chwilę pójdzie na dysk. Skan na
    // `raw` z zapisem `body` jest cichą porażką numer jeden i cały ten porządek istnieje po to,
    // żeby nie dało się jej napisać przez pomyłkę.
    let mut findings = hidden_text(raw, &body, comments, invisible);
    findings.extend(instruction_rules(raw, &body));

    // Po linii, potem po regule: karta czyta się wtedy z góry na dół tak, jak plik.
    findings.sort_by(|left, right| {
        (left.line.unwrap_or(0), left.rule.as_str())
            .cmp(&(right.line.unwrap_or(0), right.rule.as_str()))
    });

    let verdict = verdict_of(&findings);
    Reviewed {
        body,
        findings,
        verdict,
    }
}

/// Zero znalezisk to [`Verdict::Clean`], jeden [`Weight::Block`] to [`Verdict::Blocked`],
/// wszystko pomiędzy to [`Verdict::Concerns`].
///
/// Jedna funkcja, bo werdykt liczy się dwa razy: raz nad rdzeniem, drugi raz po dołożeniu
/// znalezisk skanera. Dwie kopie tej reguły to dwa różne znaczenia słowa „czysto" — jedno
/// w rdzeniu i jedno w adapterze.
fn verdict_of(findings: &[Finding]) -> Verdict {
    if findings
        .iter()
        .any(|finding| finding.weight == Weight::Block)
    {
        Verdict::Blocked
    } else if findings.is_empty() {
        Verdict::Clean
    } else {
        Verdict::Concerns
    }
}

// ── Normalizacja ───────────────────────────────────────────────────────────────────────────
//
// Obie funkcje niżej trzymają JEDEN niezmiennik, na którym stoi cała numeracja linii:
// **normalizacja nigdy nie usuwa znaku nowej linii.** Dzięki temu linia numer N w tekście
// surowym, w tekście zapisanym i w znalezisku jest tą samą linią, a człowiek czytający kartę
// może otworzyć plik i zobaczyć to samo miejsce. Komentarz rozpięty na trzech liniach zostawia
// po sobie trzy puste linie i to jest cena, którą płacimy świadomie.

/// Znak, którego człowiek nie zobaczy: zero-width i sterujące bidi.
///
/// DLACZEGO lista, a nie `char::is_control`: `\n` i `\t` też są sterujące, a są treścią pliku.
/// Ta piętnastka to znaki, których jedynym zastosowaniem w umiejętności jest sprawienie, żeby
/// tekst renderował się inaczej, niż się parsuje.
fn is_invisible(character: char) -> bool {
    matches!(character,
        '\u{200b}'..='\u{200d}' | '\u{feff}' | '\u{2060}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}')
}

/// Co zostało zdjęte i z której linii.
struct Taken {
    line: usize,
    /// Tekst, który człowiek ma zobaczyć w znalezisku, bo w ciele już go nie ma.
    recovered: String,
}

/// Zdejmuje komentarze HTML, zostawiając ich znaki nowej linii.
///
/// Komentarz bez zamknięcia połyka resztę pliku — i to jest kształt ataku, nie literówka:
/// `<!--` w połowie dokumentu ukrywa przed człowiekiem wszystko, co po nim, a model dostaje
/// całość. Dlatego brak `-->` znaczy „komentarz do końca tekstu", a nie „to nie był komentarz".
fn strip_comments(text: &str) -> (String, Vec<Taken>) {
    let mut out = String::with_capacity(text.len());
    let mut taken = Vec::new();
    let mut rest = text;
    let mut line = 1usize;

    while let Some(at) = rest.find("<!--") {
        let (before, from) = rest.split_at(at);
        out.push_str(before);
        line += before.matches('\n').count();

        let after_open = &from["<!--".len()..];
        let (inner, tail) = match after_open.find("-->") {
            Some(end) => (&after_open[..end], &after_open[end + "-->".len()..]),
            None => (after_open, ""),
        };

        taken.push(Taken {
            line,
            recovered: inner.trim().to_owned(),
        });
        let breaks = inner.matches('\n').count();
        for _ in 0..breaks {
            out.push('\n');
        }
        line += breaks;
        rest = tail;
    }

    out.push_str(rest);
    (out, taken)
}

/// Zdejmuje znaki niewidzialne, zostawiając linie na swoich miejscach.
///
/// `split_inclusive` zamiast `lines`, bo `lines` gubi informację o tym, czy plik kończył się
/// znakiem nowej linii — a ciało różniące się od wejścia jednym bajtem na końcu nie jest
/// „bajt w bajt" i przewraca AC-2 w miejscu, w którym nikt nie szuka.
fn strip_invisible(text: &str) -> (String, Vec<Taken>) {
    let mut out = String::with_capacity(text.len());
    let mut taken = Vec::new();

    for (index, line) in text.split_inclusive('\n').enumerate() {
        if line.chars().any(is_invisible) {
            let cleaned: String = line.chars().filter(|c| !is_invisible(*c)).collect();
            taken.push(Taken {
                line: index + 1,
                recovered: cleaned.trim().to_owned(),
            });
            out.push_str(&cleaned);
        } else {
            out.push_str(line);
        }
    }

    (out, taken)
}

/// Linia tak, jak ma ją zacytować karta: z tekstu SUROWEGO, ale bez znaków niewidzialnych.
///
/// DLACZEGO z surowego: przy komentarzu zapisana linia jest już pusta, a cytat z pustej linii
/// nie mówi człowiekowi nic. DLACZEGO bez niewidzialnych: cytat jedzie na ekran, a sterujący
/// bidi wpuszczony do karty przestawia na niej tekst, którego atak nawet nie dotyczył.
fn quote_from(raw: &str, line: usize) -> String {
    raw.split_inclusive('\n')
        .nth(line.saturating_sub(1))
        .map(|text| {
            text.chars()
                .filter(|c| !is_invisible(*c))
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .unwrap_or_default()
}

// ── R1: to, czego nie widać ────────────────────────────────────────────────────────────────

/// Znaleziska [`HIDDEN_TEXT`]: zdjęte komentarze, zdjęte znaki niewidzialne i homoglify.
///
/// Jedno znalezisko na linię, choćby powodów było trzy. Dwa wiersze na karcie mówiące o jednej
/// linii uczą przewijać, a przewijana karta jest kartą nieprzeczytaną.
fn hidden_text(raw: &str, body: &str, comments: Vec<Taken>, invisible: Vec<Taken>) -> Vec<Finding> {
    let mut per_line: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for item in comments.into_iter().chain(invisible) {
        per_line.entry(item.line).or_default().push(item.recovered);
    }
    // Homoglif niczego nie zdejmuje z ciała (zdjęcie przepisałoby cudze słowo), więc szuka się
    // go w tekście ZAPISANYM i wraca jako numer znaku, nie jako tekst.
    for (index, line) in body.split_inclusive('\n').enumerate() {
        if let Some(said) = mixed_scripts(line) {
            per_line.entry(index + 1).or_default().push(said);
        }
    }

    per_line
        .into_iter()
        .map(|(line, said)| Finding {
            rule: HIDDEN_TEXT.to_owned(),
            weight: Weight::Block,
            line: Some(line),
            quoted: quote_from(raw, line),
            recovered: Some(said.join(" ")),
            source: Source::Rules,
        })
        .collect()
}

/// Słowo pisane dwoma alfabetami naraz — `аdmin` z cyrylicznym `а` czyta się jak `admin`
/// i jest innym napisem.
///
/// Zwraca zdanie z numerem znaku, bo bez numeru komunikat brzmi „coś tu jest nie tak z literą,
/// która wygląda dobrze" i nie da się na jego podstawie niczego zrobić.
fn mixed_scripts(line: &str) -> Option<String> {
    for word in line.split(|c: char| !c.is_alphabetic()) {
        let latin = word.chars().any(|c| c.is_ascii_alphabetic());
        let other = word
            .chars()
            .find(|c| matches!(c, '\u{0370}'..='\u{03ff}' | '\u{0400}'..='\u{04ff}'));
        if let (true, Some(character)) = (latin, other) {
            return Some(format!(
                "{word}: {character} is U+{:04X}",
                u32::from(character)
            ));
        }
    }
    None
}

// ── R2–R5: kształt linii, nie worek słów ───────────────────────────────────────────────────
//
// Cztery reguły w jednym przejściu po zapisanym ciele. Jedna funkcja, bo to jest JEDNA
// polityka (niezmiennik 23): dołożenie piątej reguły ma być dopisaniem gałęzi tutaj, a nie
// nowym miejscem, w którym „też się coś sprawdza".
//
// Każda z nich pyta o KSZTAŁT: trzy człony naraz w jednej linii, polecenie wysyłające RAZEM
// ze źródłem sekretu, znacznik tury na początku linii. Reguła zapalająca się na jednym słowie
// (`instructions`, `curl`) daje trzy fałszywe alarmy, po których człowiek klika „Add" bez
// czytania — i wtedy mechanizm przeglądu przestaje istnieć, choć kod dalej stoi w repo.

/// Znaleziska reguł R2–R5 dla całego ciała.
fn instruction_rules(raw: &str, body: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut fenced = false;
    // Front-matter zaczyna się WYŁĄCZNIE w pierwszej linii pliku. `---` w środku dokumentu
    // jest poziomą kreską i pole `hooks:` pod nią nie jedzie do żadnego vendora.
    let mut in_front_matter = body.starts_with("---\n") || body.starts_with("---\r\n");

    for (index, line) in body.lines().enumerate() {
        let number = index + 1;
        let trimmed = line.trim_start();

        if index > 0 && in_front_matter && trimmed.trim_end() == "---" {
            in_front_matter = false;
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }

        // Cztery spacje wcięcia to blok kodu w każdym dialekcie Markdowna, a `>` to cytat.
        let quoting = fenced || line.starts_with("    ") || trimmed.starts_with('>');
        let mut hit = |rule: &str, weight: Weight| {
            findings.push(Finding {
                rule: rule.to_owned(),
                weight,
                line: Some(number),
                quoted: quote_from(raw, number),
                recovered: None,
                source: Source::Rules,
            });
        };

        if says_ignore_the_rules(line) {
            // Waga, nie odmowa. Ta sama linia w prozie jest treścią ataku, a w bloku kodu jest
            // cytatem z ataku — i skaner, który nie umie ich rozróżnić, blokuje umiejętność
            // o obronie przed wstrzyknięciem.
            hit(
                INSTRUCTION_OVERRIDE,
                if quoting { Weight::Warn } else { Weight::Block },
            );
        }
        if sends_a_secret_out(line) {
            hit(EXFILTRATION, Weight::Block);
        }
        if wears_a_turn_marker(trimmed) {
            hit(ROLE_MANIPULATION, Weight::Block);
        }
        if in_front_matter && asks_for_more(trimmed) {
            hit(ESCALATION, Weight::Warn);
        }
    }

    findings
}

/// Wyrazy linii, małymi literami — tylko litery i cyfry, reszta jest granicą.
///
/// DLACZEGO wyrazy, a nie `contains`: `all` siedzi w `install`, `allowed` i `actually`, więc
/// reguła oparta na zawieraniu zapala się na zdaniu „install the allowed tools" i po trzech
/// takich alarmach nikt już karty nie czyta.
fn words(line: &str) -> Vec<String> {
    line.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// R2. Trzy człony naraz: „przestań słuchać" + „tego, co było" + „instrukcji".
///
/// Każdy z osobna jest zwykłą angielszczyzną (`Follow these instructions in order.`,
/// `Ignore files under node_modules/.`) i dopiero komplet w JEDNEJ linii jest tym zdaniem,
/// które model potrafi wykonać.
fn says_ignore_the_rules(line: &str) -> bool {
    let words = words(line);
    // Przedrostki, bo odmiana jest częścią tej samej próby: `ignoring`, `disregarded`.
    let stop = ["ignor", "disregard", "forget", "forgot", "overrid"];
    let past = ["previous", "prior", "above", "all", "earlier", "preceding"];
    let orders = [
        "instruction",
        "instructions",
        "rule",
        "rules",
        "prompt",
        "prompts",
    ];

    words
        .iter()
        .any(|word| stop.iter().any(|stem| word.starts_with(stem)))
        && words.iter().any(|word| past.contains(&word.as_str()))
        && words.iter().any(|word| orders.contains(&word.as_str()))
}

/// R3. Polecenie wysyłające RAZEM ze źródłem sekretu.
///
/// Oba człony naraz, bo dokumentacja API jest zbudowana z `curl -X POST` i reguła zapalająca
/// się na samym poleceniu jest bezwartościowa po pierwszym imporcie.
fn sends_a_secret_out(line: &str) -> bool {
    let words = words(line);
    let sends = ["curl", "wget", "nc", "ncat", "scp", "sftp", "rsync"]
        .iter()
        .any(|command| words.iter().any(|word| word == command))
        || line.contains("git push");

    let secret = [
        ".env",
        "~/.ssh",
        "id_rsa",
        "id_ed25519",
        "credentials",
        "$(cat",
    ]
    .iter()
    .any(|source| line.contains(source))
        || words
            .iter()
            .any(|word| word.ends_with("_api_key") || word.ends_with("_secret"));

    sends && secret
}

/// R4. Próba przejęcia ramki rozmowy.
///
/// Znacznik tury liczy się na POCZĄTKU linii: `system:` w środku zdania („the system: a note")
/// jest interpunkcją, a na początku linii jest udawaniem, że rozmowa właśnie zmieniła mówcę.
fn wears_a_turn_marker(trimmed: &str) -> bool {
    let lower = trimmed.to_lowercase();
    lower.contains("<system>")
        || lower.contains("</system>")
        || lower.contains("<|im_start|>")
        || ["system:", "assistant:", "human:", "user:"]
            .iter()
            .any(|marker| lower.starts_with(marker))
        || lower.contains("you are now")
}

/// R5. Front-matter proszący o własne narzędzia albo o własny hak.
fn asks_for_more(trimmed: &str) -> bool {
    let key = trimmed
        .split_once(':')
        .map(|(key, _)| key.trim().to_lowercase())
        .unwrap_or_default();
    key == "allowed-tools" || key == "hooks"
}

// ── Adapter głębokiego skanu ───────────────────────────────────────────────────────────────

/// Wynik `oxidized-agentic-audit` [T5 §5.4].
#[derive(Debug, Clone)]
pub enum DeepScan {
    /// Skaner pobiegł i to jest wszystko, co zgłosił. Pusta lista znaczy „nic nie znalazł",
    /// a nie „nie sprawdzał".
    Ran { findings: Vec<Finding> },
    /// Nie pobiegł: nie ma binarki, wyszedł błędem, albo odpowiedział czymś, czego nie umiemy
    /// przeczytać. **To nie jest czysty rachunek** — [`with_deep_scan`] dokłada wtedy
    /// [`DEEP_SCAN_UNAVAILABLE`].
    ///
    /// Powód jest `&'static str`, jak [`Discovery::Unknown`] w T-18: zdanie dla człowieka,
    /// nie kod błędu do rozgałęziania.
    Unavailable(&'static str),
}

/// Uruchamia skaner nad katalogiem umiejętności i czyta jego JSON.
///
/// Adapter ma pięć linii treści i ani jednej reguły (niezmiennik 23). Parsowanie jest
/// permisywne: nieznany klucz w znalezisku to `extra`, nieznana waga to [`Weight::Warn`],
/// a niczytelna odpowiedź to [`DeepScan::Unavailable`] — nigdy panika i nigdy cisza
/// (niezmiennik 5). Odpowiedź bez tablicy `findings` jest brakiem odpowiedzi, nie odpowiedzią
/// „zero": tak samo jak zdarzenie bez `skills` w [`Discovery`], bo vendorzy zmieniają kształt
/// wyjścia po cichu i co tydzień.
///
/// Kody wyjścia narzędzia: `0` czysto, `1` **są znaleziska**, `2` błąd wykonania [T5 §5.4].
/// Zero i jedynka znaczą więc „pobiegł"; wszystko inne to [`DeepScan::Unavailable`], nawet
/// kiedy na wyjściu stoi poprawny JSON — skaner, który padł, nie wystawia czystego rachunku.
///
/// Skryptów umiejętności nie uruchamiamy nigdy — ani tutaj, ani przy instalacji [T5 §5.4].
/// Skaner CZYTA katalog; jedynym uruchamianym plikiem jest on sam.
#[must_use]
pub fn deep_scan(dir: &Path, bin: &Path) -> DeepScan {
    todo!(
        "uruchom {} nad {} i przeczytaj JSON permisywnie",
        bin.display(),
        dir.display()
    )
}

/// Dokłada znaleziska skanera do przeglądu rdzenia i przelicza werdykt.
///
/// **Dokłada, nigdy nie zastępuje** (niezmiennik 23). Zbiór znalezisk z [`Source::Rules`]
/// jest po tej operacji identyczny co do jednego wpisu — i to jest jedyna rzecz, która
/// odróżnia adapter od drugiego rdzenia polityki.
#[must_use]
pub fn with_deep_scan(reviewed: Reviewed, deep: &DeepScan) -> Reviewed {
    todo!(
        "dołóż znaleziska skanera do {} znalezisk rdzenia; {deep:?} nieobecny to Concerns, \
         nigdy Clean",
        reviewed.findings.len()
    )
}

// ── Polityka adresu i limity pobrania ──────────────────────────────────────────────────────

/// Hosty, z których wolno pobierać [T5 §5.1].
///
/// Lista, nie wzorzec. `https://github.com.evil.tld/o/r` zawiera `github.com` i nie jest
/// GitHubem; porównanie musi być równością CAŁEGO hosta, nie zawieraniem.
pub const ALLOWED_HOSTS: [&str; 4] = [
    "github.com",
    "raw.githubusercontent.com",
    "gist.github.com",
    "gist.githubusercontent.com",
];

/// 1 MB na plik [T5 §5.2].
pub const FILE_CAP: u64 = 1_048_576;

/// 5 MB na całą umiejętność [T5 §5.2].
pub const TOTAL_CAP: u64 = 5_242_880;

/// Najwyżej trzy przekierowania [T5 §5.2].
pub const MAX_REDIRECTS: usize = 3;

/// Dwadzieścia sekund na pobranie [T5 §5.2].
pub const FETCH_TIMEOUT_SECONDS: u64 = 20;

/// Co się kryje pod adresem [T5 §5.1].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Adres kończy się na `/SKILL.md` — bierzemy wprost.
    RawFile,
    /// Podkatalog repozytorium: `github.com/{owner}/{repo}/tree/{ref}/{path}`.
    Folder {
        owner: String,
        repo: String,
        /// Gałąź albo tag. `ref` jest słowem kluczowym Rusta, stąd `git_ref`.
        git_ref: String,
        path: String,
    },
    /// Gist.
    Gist,
}

/// Odmowy pobrania. Jeden enum na całą drogę bajtów, bo to jest jedna polityka
/// (niezmiennik 23), a nie pięć osobnych.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// `http://` bez `s`. Treść, którą agent wykona, nie jedzie kanałem, w który można wpisać
    /// cudzy tekst po drodze.
    #[error("Loadout reads skills over https only")]
    NotHttps,

    /// Host spoza [`ALLOWED_HOSTS`] — także taki, który tylko wygląda jak z listy.
    #[error("Loadout reads skills from GitHub and gist.github.com only")]
    HostNotAllowed,

    /// Więcej niż [`MAX_REDIRECTS`] przeskoków.
    #[error("this link moved more than {MAX_REDIRECTS} times, so Loadout stopped following it")]
    TooManyRedirects,

    /// Jeden plik przekroczył limit. Limit jest w komunikacie, bo „file too big" bez liczby
    /// nie mówi, o ile za duży.
    #[error("this file is larger than the {limit} byte limit")]
    FileTooBig { limit: u64 },

    /// Suma plików przekroczyła limit.
    #[error("this skill is larger than the {limit} byte limit altogether")]
    TotalTooBig { limit: u64 },

    /// Bajty nie są tekstem w UTF-8 [T5 §5.2, krok 3].
    #[error("this file is not text Loadout can read")]
    NotText,

    /// Host jest z listy, a adres nie wskazuje na umiejętność: link do zgłoszenia, do
    /// pojedynczego `reference.md`, do strony ustawień repozytorium.
    ///
    /// Osobna odmowa, bo [`HostNotAllowed`](Self::HostNotAllowed) w tym miejscu byłoby
    /// nieprawdą o hoście, a człowiek poprawiałby wtedy nie to, co trzeba. Dowolna strona
    /// docs jest świadomie poza v1 [T5 §5.3]: gdyby kiedyś weszła, to jest jej miejsce.
    #[error("Loadout could not tell which skill this link points at")]
    NotASkillLink,

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Adres → co to właściwie jest, albo odmowa [T5 §5.1].
///
/// Czysta funkcja: nic nie pobiera i nie rozwiązuje nazw. Dzięki temu polityka adresu jest
/// testowalna bez sieci, a bramka, która wymaga internetu, jest bramką czerwieniejącą od
/// cudzych awarii.
pub fn resolve_url(url: &str) -> Result<Target, FetchError> {
    let host = host_of(url)?;
    let path = path_of(url);
    let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();

    if host == "gist.github.com" || host == "gist.githubusercontent.com" {
        return Ok(Target::Gist);
    }
    // `.../SKILL.md` bierzemy wprost [T5 §5.1]. To jest też jedyny kształt, jaki umiemy
    // rozpoznać na `raw.githubusercontent.com`: tam nie ma nic poza ścieżką do pliku.
    if segments.last().is_some_and(|last| *last == SKILL_FILE) {
        return Ok(Target::RawFile);
    }

    if host == "github.com" {
        return match segments.as_slice() {
            // `github.com/{owner}/{repo}` — korzeń repozytorium [T5 §5.1]. `HEAD` zamiast
            // zgadywanej nazwy gałęzi: GitHub rozwiązuje go do gałęzi domyślnej, a wpisany
            // na sztywno `main` przestaje działać przy pierwszym repo z gałęzią `master`.
            [owner, repo] => Ok(Target::Folder {
                owner: (*owner).to_owned(),
                repo: (*repo).to_owned(),
                git_ref: "HEAD".to_owned(),
                path: String::new(),
            }),
            [owner, repo, "tree", git_ref, rest @ ..] => Ok(Target::Folder {
                owner: (*owner).to_owned(),
                repo: (*repo).to_owned(),
                git_ref: (*git_ref).to_owned(),
                path: rest.join("/"),
            }),
            _ => Err(FetchError::NotASkillLink),
        };
    }

    Err(FetchError::NotASkillLink)
}

/// Host adresu, w całości i małymi literami — albo odmowa.
///
/// Trzy rzeczy, które odcinamy, i każda z nich jest osobnym sposobem na to, żeby porównanie
/// z listą wypadło inaczej niż połączenie: `user@` przed hostem (przeglądarka i `curl` łączą
/// się z tym PO małpie), port po dwukropku i wszystko od pierwszego `/`, `?` albo `#`.
fn host_of(url: &str) -> Result<String, FetchError> {
    let after_scheme = match url.get(..8) {
        Some(head) if head.eq_ignore_ascii_case("https://") => &url[8..],
        _ => return Err(FetchError::NotHttps),
    };

    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();

    // Równość CAŁEGO hosta. `contains("github.com")` przepuszcza `github.com.evil.tld`
    // (nasza nazwa jako prefiks cudzej domeny) i `evil.tld/x?u=github.com/o/r` (nasza nazwa
    // w parametrze) — czyli dokładnie te dwa adresy, po których poznaje się tę pomyłkę.
    if ALLOWED_HOSTS.contains(&host.as_str()) {
        Ok(host)
    } else {
        Err(FetchError::HostNotAllowed)
    }
}

/// Ścieżka adresu, bez zapytania i bez kotwicy.
fn path_of(url: &str) -> &str {
    let after_scheme = url.get(8..).unwrap_or_default();
    let from_slash = after_scheme.find('/').map_or("", |at| &after_scheme[at..]);
    from_slash.split(['?', '#']).next().unwrap_or(from_slash)
}

/// Czy łańcuch odwiedzonych adresów wolno było przejść do końca.
///
/// `chain[0]` to adres, o który poprosiliśmy; każdy następny to jeden przeskok. Sprawdzamy
/// **każde ogniwo**, nie tylko pierwsze: przekierowanie jest tym, jak dozwolony host oddaje
/// treść z niedozwolonego, a `--max-redirs` w argv curla jest deklaracją narzędzia, nie naszym
/// dowodem (niezmiennik 20).
pub fn follow_policy(chain: &[&str]) -> Result<(), FetchError> {
    for (hop, url) in chain.iter().enumerate() {
        // Sprawdzenie w kolejności, w jakiej działy się przeskoki: host najpierw, bo o tym
        // dowiadujemy się, dochodząc do ogniwa, a liczba dopiero po nim.
        host_of(url)?;
        if hop > MAX_REDIRECTS {
            return Err(FetchError::TooManyRedirects);
        }
    }
    Ok(())
}

/// Czyta najwyżej `limit` bajtów i odmawia, kiedy źródło ma ich więcej.
///
/// Czyta o jeden bajt za dużo NAUMYŚLNIE: „przeczytaj dokładnie limit i przestań" nie odróżnia
/// pliku dokładnie na limicie od pliku uciętego w połowie. Sprawdzenie u siebie, po fakcie,
/// zamiast zaufania `--max-filesize`.
pub fn read_capped(source: impl Read, limit: u64) -> Result<Vec<u8>, FetchError> {
    let mut bytes = Vec::new();
    source
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;

    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(FetchError::FileTooBig { limit });
    }
    Ok(bytes)
}

/// Suma rozmiarów wszystkich plików umiejętności wobec limitu całości.
///
/// Osobno od [`read_capped`], bo to jest inny atak: pięć plików mieszczących się w limicie
/// pojedynczym, których suma zapycha dysk.
pub fn total_within(sizes: &[u64], limit: u64) -> Result<u64, FetchError> {
    // `checked_add`, nie `+`: suma rozmiarów przychodzi z sieci, a przepełnienie u64 zawija się
    // do liczby MNIEJSZEJ od limitu, czyli zamienia odmowę w zgodę.
    let total = sizes
        .iter()
        .try_fold(0u64, |sum, size| sum.checked_add(*size))
        .ok_or(FetchError::TotalTooBig { limit })?;

    if total > limit {
        return Err(FetchError::TotalTooBig { limit });
    }
    Ok(total)
}

/// Komenda pobrania — JEDNO miejsce, w którym powstaje wywołanie `curl`.
///
/// Flagi (`--proto '=https'`, `--max-redirs 3`, `--max-filesize`, `--max-time 20`) są pierwszą
/// linią obrony i **nie są dowodem**: dokładnie tak umarło `--sandbox workspace-write`
/// w spreadsheecie, gdzie flaga stała w komentarzu, a żywa brzmiała `danger-full-access`
/// (raport 06, niezmiennik 20). Każdy z tych limitów jest sprawdzany drugi raz u siebie, na
/// tym, co faktycznie przyszło.
#[must_use]
pub fn build_fetch_command(url: &str) -> Command {
    let mut command = Command::new("curl");
    command
        .arg("--proto")
        .arg("=https")
        .arg("--max-redirs")
        .arg(MAX_REDIRECTS.to_string())
        .arg("--max-filesize")
        .arg(FILE_CAP.to_string())
        .arg("--max-time")
        .arg(FETCH_TIMEOUT_SECONDS.to_string())
        .arg("--location")
        .arg("--fail")
        .arg("--silent")
        .arg("--show-error")
        // Adres NIE stoi w argv (niezmiennik 9). `curl` czyta go ze stdinu w składni swojego
        // pliku konfiguracyjnego, bo link do umiejętności potrafi nieść token w zapytaniu,
        // a argv widzi w `ps` każdy proces na tej maszynie i każdy log, który je zapisuje.
        .arg("--config")
        .arg("-");

    // `env_clear` plus jawna lista (niezmiennik 9). PATH przepuszczamy, bo bez niego nie da
    // się znaleźć samego `curl`-a; `http_proxy`, `CURL_CA_BUNDLE` i reszta środowiska nie ma
    // prawa zmieniać tego, skąd i po czym przyjdzie treść, którą wykona agent.
    command.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    match config_on_stdin(url) {
        Ok(reader) => command.stdin(reader),
        // Bez rury nie ma adresu, a `curl` bez adresu kończy się błędem i nie pobiera niczego.
        // Zapasowe wpisanie URL-a do argv byłoby cichym cofnięciem całej ostrożności wyżej.
        Err(_) => command.stdin(Stdio::null()),
    };
    command
}

/// Jedna linia konfiguracji `curl`-a — adres — w rurze gotowej do podstawienia pod stdin.
///
/// Rura, a nie plik tymczasowy: plik z adresem przeżyłby bieg i dałby się przeczytać
/// (niezmiennik 9 zakazuje obu). Adres ma kilkadziesiąt bajtów, więc zapis mieści się w buforze
/// rury i `write_all` nie ma na co czekać — dziecko jeszcze nie istnieje.
fn config_on_stdin(url: &str) -> std::io::Result<std::io::PipeReader> {
    let (reader, mut writer) = std::io::pipe()?;
    let quoted = url.replace('\\', "\\\\").replace('"', "\\\"");
    writer.write_all(format!("url = \"{quoted}\"\n").as_bytes())?;
    Ok(reader)
}

// ── Import z katalogu ──────────────────────────────────────────────────────────────────────

/// Pobrana umiejętność, przejrzana i gotowa do pokazania człowiekowi — jeszcze przed
/// walidacją i przed pierwszym zapisem w katalogu vendora.
#[derive(Debug, Clone)]
pub struct Import {
    /// Kanoniczna umiejętność, którą dostanie [`super::place::plan`].
    pub skill: Skill,
    /// Ciało do zapisania i wszystko, co o nim wiemy.
    pub reviewed: Reviewed,
    /// Ile dołączonych skryptów niesie umiejętność — liczba, którą dostaje karta przeglądu.
    ///
    /// Zdanie na karcie brzmi „Includes N scripts — these will not run unless an agent chooses
    /// to run them." [T5 §8.3]. Liczba jest LICZONA z tego, co przyszło; wpisana na sztywno
    /// mówiłaby to samo o umiejętności, która nie niesie żadnego.
    pub scripts: usize,
}

/// Katalog z pobraną umiejętnością → umiejętność kanoniczna plus przegląd jej treści.
///
/// **Niczego nie uruchamia.** Ani tutaj, ani przy instalacji: dołączony `scripts/run.sh` jest
/// dla nas plikiem do skopiowania i policzenia, nigdy do wykonania [T5 §5.4]. Bit
/// wykonywalności zostaje — zdejmowanie go zamieniłoby cudzą działającą umiejętność
/// w zepsutą — a to, że nikt tego pliku nie odpala, jest własnością KODU, nie uprawnień.
pub fn from_folder(dir: &Path) -> Result<Import, FetchError> {
    // Limit pojedynczego pliku sprawdzony u siebie, na bajtach, które faktycznie przyszły —
    // `--max-filesize` w argv curla jest deklaracją narzędzia (niezmiennik 20).
    let raw = read_capped(fs::File::open(dir.join(SKILL_FILE))?, FILE_CAP)?;
    let raw = String::from_utf8(raw).map_err(|_| FetchError::NotText)?;

    let files = bundled_files(dir)?;
    let mut sizes = vec![u64::try_from(raw.len()).unwrap_or(u64::MAX)];
    for file in &files {
        sizes.push(fs::symlink_metadata(&file.source)?.len());
    }
    total_within(&sizes, TOTAL_CAP)?;

    // Potok w jednej kolejności: znormalizuj i przeskanuj CAŁY plik, dopiero potem rozbij na
    // pola. Parsowanie przed skanem znaczyłoby, że skan ogląda ciało, a front-matter
    // z `hooks:` przejeżdża bokiem.
    let reviewed = review(&raw);
    let skill = skill_from(parse_doc(&reviewed.body), files);
    let scripts = skill
        .files
        .iter()
        .filter(|file| file.relative.starts_with("scripts"))
        .count();

    Ok(Import {
        skill,
        reviewed,
        scripts,
    })
}

/// Wszystko obok `SKILL.md`, rekurencyjnie, ścieżkami względnymi.
///
/// Dowiązania są pomijane, a nie kopiowane: `fs::copy` idzie po dowiązaniu do końca, więc
/// `references/keys -> ~/.ssh` w cudzej umiejętności zaciągnąłby zawartość katalogu, którego
/// nikt nie pobierał. Katalog docelowy ma zawierać to, co przyszło z sieci, i nic spoza niego.
fn bundled_files(dir: &Path) -> Result<Vec<BundledFile>, FetchError> {
    let mut found = Vec::new();
    walk_into(dir, Path::new(""), &mut found)?;
    // Kolejność z systemu plików nie jest ustalona; plan instalacji czyta człowiek.
    found.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(found)
}

fn walk_into(dir: &Path, prefix: &Path, found: &mut Vec<BundledFile>) -> Result<(), FetchError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let source = entry.path();
        let relative = prefix.join(entry.file_name());
        let kind = fs::symlink_metadata(&source)?;

        if kind.is_dir() {
            walk_into(&source, &relative, found)?;
        } else if kind.is_file() && relative != Path::new(SKILL_FILE) {
            if kind.len() > FILE_CAP {
                return Err(FetchError::FileTooBig { limit: FILE_CAP });
            }
            found.push(BundledFile { relative, source });
        }
    }
    Ok(())
}

/// `SKILL.md` → front-matter i ciało, permisywnie (niezmiennik 5).
///
/// Nieznany klucz nie jest błędem: ląduje w [`Skill::extras`], a emiter T-18 decyduje, czy ma
/// przenośny odpowiednik. Front-matter bez zamknięcia nie jest front-matterem — `---` w
/// pierwszej linii pliku, który nigdy się nie domyka, to pozioma kreska, a nie nagłówek.
fn parse_doc(text: &str) -> SkillDoc {
    let whole = || SkillDoc {
        fields: Vec::new(),
        body: text.to_owned(),
    };
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return whole();
    };

    let mut fields: Vec<(String, String)> = Vec::new();
    let mut consumed = 0usize;
    let mut closed = false;

    for line in rest.split_inclusive('\n') {
        consumed += line.len();
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim_end() == "---" {
            closed = true;
            break;
        }
        if line.starts_with([' ', '\t']) {
            // Wcięcie to ciąg dalszy poprzedniego klucza (`metadata:` i jego pary). Surowy
            // tekst wystarczy: rozbiera go [`nested`], a emiter i tak pisze mapę po swojemu.
            if let Some((_, value)) = fields.last_mut() {
                value.push('\n');
                value.push_str(content);
            }
        } else if let Some((key, value)) = content.split_once(':') {
            fields.push((key.trim().to_owned(), unquote(value.trim())));
        }
    }

    if closed {
        SkillDoc {
            fields,
            body: rest[consumed..].to_owned(),
        }
    } else {
        whole()
    }
}

/// Wartość YAML-a bez cudzysłowu, jeśli w cudzysłowie przyszła.
///
/// Lustro `place::scalar`, które cytuje przy zapisie. Bez tego `description: "a: b"` wraca
/// z cudzysłowami w środku pola i po drugim „Update" umiejętność ma tytuł w trzech warstwach
/// znaków cytowania.
fn unquote(value: &str) -> String {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return value[1..value.len() - 1]
                .replace("\\\"", "\"")
                .replace("\\\\", "\\");
        }
    }
    value.to_owned()
}

/// Dokument → umiejętność kanoniczna.
fn skill_from(doc: SkillDoc, files: Vec<BundledFile>) -> Skill {
    let mut skill = Skill {
        body: doc.body,
        files,
        ..Skill::default()
    };
    for (key, value) in doc.fields {
        match key.as_str() {
            "name" => skill.name = value,
            "description" => skill.description = value,
            "license" => skill.license = Some(value),
            "compatibility" => skill.compatibility = Some(value),
            "allowed-tools" => skill.allowed_tools = Some(value),
            "metadata" => skill.metadata = nested(&value),
            // Wszystko inne — łącznie z `hooks` — zostaje jako fakt o imporcie. Zdejmuje je
            // emiter T-18 i to on mówi, co dokładnie zniknęło.
            _ => {
                skill.extras.insert(key, value);
            }
        }
    }
    skill
}

/// Pary spod wciętego klucza (`metadata:`), płasko.
fn nested(raw: &str) -> BTreeMap<String, String> {
    raw.lines()
        .filter_map(|line| line.trim().split_once(':'))
        .map(|(key, value)| (key.trim().to_owned(), unquote(value.trim())))
        .collect()
}

// ── Samotest po instalacji ─────────────────────────────────────────────────────────────────

/// Tier 1 [T5 §6.3]: czy plik jest napisany poprawnie. Offline, natychmiast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecCheck {
    /// UI: „The skill is written correctly."
    Valid,
    /// Komunikaty z [`super::place::validate_strict`], jeden na przyczynę.
    Invalid { messages: Vec<String> },
}

/// Tier 2 [T5 §6.3]: czy pliki NAPRAWDĘ leżą tam, gdzie miały.
///
/// Liczone przez ponowny odczyt i ponowne sparsowanie **z dysku**. Samotest zbudowany
/// z [`super::place::InstallPlan`], który przed chwilą wykonaliśmy, przechodzi zawsze — i jest
/// dokładnie tym ptaszkiem „`fs::write` zwróciło `Ok`", przed którym stoi całe T-18 i T-19.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Ile katalogów docelowych ma czytelną, poprawną umiejętność.
    pub ok: usize,
    /// Ile ich było.
    pub of: usize,
    /// Katalogi docelowe, które się nie wczytały — ścieżkami, żeby zgłoszenie mówiło GDZIE,
    /// a nie ile. `ok + broken.len() == of`: katalog, który nie jest ani jednym, ani drugim,
    /// jest katalogiem, w który nikt nigdy nie zajrzy.
    pub broken: Vec<PathBuf>,
}

/// Trzy poziomy dowodu [T5 §6.3] plus zdania, którymi mówi o nich karta.
#[derive(Debug, Clone)]
pub struct SelfTest {
    /// Tier 1 — poprawność zapisu.
    pub valid: SpecCheck,
    /// Tier 2 — obecność na dysku, odczytana z dysku.
    pub installed: Installed,
    /// Tier 3 — czy vendor to widzi. Werdykt bierze T-18
    /// ([`super::place::discovery_from_init`]); brak CLI to [`Discovery::Unknown`], pokazywane
    /// jako `not installed`, **nigdy** jako porażka.
    pub discovered: Discovery,
    /// Zdania dla karty, LICZONE z trzech pól wyżej.
    ///
    /// Komplet brzmi „Installed for 2 tools." [T5 §8.3] — liczba jest liczbą MIEJSC, tak samo
    /// jak [`Installed::of`] — i przy choćby jednej zepsutej kopii tego zdania tu nie ma.
    /// Zdanie o komplecie postawione obok niepełnej instalacji jest gorsze niż brak zdania:
    /// człowiek przestaje sprawdzać coś, co właśnie nie zadziałało.
    ///
    /// Tier 3 mówi tu `not installed`, nigdy „failed": brak CLI jest faktem o świecie, nie
    /// werdyktem o umiejętności.
    pub summary: Vec<String>,
}

/// Trzy tiery nad umiejętnością, która przed chwilą została zainstalowana.
///
/// `wrote` to ścieżki katalogów docelowych — stamtąd, i tylko stamtąd, bierze się Tier 2:
/// **ponowny odczyt i ponowne sparsowanie z dysku**. `init_line` jest szwem offline dla
/// Tiera 3: pusty łańcuch znaczy „CLI nigdy nie wystartowało".
#[must_use]
pub fn self_test(skill: &Skill, wrote: &[PathBuf], init_line: &str) -> SelfTest {
    todo!(
        "przeczytaj {} katalogów z dysku dla umiejętności {:?}; init: {init_line:?}",
        wrote.len(),
        skill.name
    )
}
