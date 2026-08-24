//! Pliki przekazań: zapis, odczyt i skanowanie katalogu biegu.
//!
//! Jedyne miejsce w repo, które składa front-matter przekazania. Reguła, z której bierze się
//! całe to zadanie, stoi w `docs/ARCHITECTURE.md` §8 i w [T6 §10.2]: **front-matter pisze
//! Loadout, agent daje tylko treść.** Agent, który wymyśla własne metadane, zmyśli je.
//!
//! Z tego wynika kolejność, która nie jest kosmetyczna: metadane są **nadpisywane**, nigdy
//! scalane z tym, co przyszło w ciele. Scalanie wygląda identycznie w diffie i w UI, a różni
//! się tym, że `status`, `reads` i `id` zaczynają pochodzić od modelu. Sfałszowany blok
//! **zostaje w ciele** — kasowanie go ukryłoby próbę przed człowiekiem, który jako jedyny
//! może na nią zareagować.
//!
//! Czego tu nie ma:
//! - `Connection` — ten moduł zwraca strukturę, wiersz wkłada `store::writer` (niezmiennik 2);
//! - `#[cfg(unix)]` — ścieżki składamy `PathBuf`em, uprawnień nie tykamy (niezmiennik 3);
//! - ścieżki ani treści w argv — ciało jedzie do następnego kroku przez stdin (niezmiennik 9).

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use super::{Error, FrontMatter, Result, est_tokens, slugify};

/// Twardy limit ciała **po normalizacji**, w bajtach [T6 §10.2, §4 „Context bloat"].
///
/// 8 KB ≈ 2 000 jednostek długości, czyli ~1% okna 200k. Liczba jest tu z powodem, a nie
/// dlatego, że ładnie wygląda: Anthropic mierzy 15× więcej długości w systemach
/// wieloagentowych niż w czacie [T6 §3.3], a cap na granicy agenta jest jedyną obroną,
/// która działa bez współpracy modelu. Ryzyko odwrotne — „cap ucina to jedno zdanie, dla
/// którego przekazanie powstało" — jest nazwane w [T6 §11.2] i dlatego cięcie idzie po
/// granicy sekcji, a pełny tekst zawsze ląduje w `attachments/`.
pub const BODY_CAP: usize = 8192;

/// Katalog obok `handoffs/`, w którym ląduje ORYGINAŁ przekazania uciętego na [`BODY_CAP`].
///
/// Nazwa stoi w jednym miejscu, bo dwa rozjechałyby się po cichu: [`write_inner`] tworzy tu plik
/// i wpisuje do ciała wskaźnik, a `commands::run` musi dać następnemu krokowi prawo ten plik
/// otworzyć. Gdyby każde z tych miejsc składało nazwę osobno, pierwsza jej zmiana dałaby
/// wskaźnik prowadzący tam, gdzie nikt nie otworzył drzwi — czyli odnośnik bez handlera
/// (niezmiennik 16). Ten sam powód stoi nad `drivers::claude::Transcript`.
pub const ATTACHMENTS_DIR: &str = "attachments";

/// Trzy sekcje o stałych nazwach i stałej kolejności [T6 §10.2].
///
/// `Answer` to jest to, czego potrzebuje następny agent; `Evidence` to `plik:linia` albo URL,
/// bo twierdzenie bez dowodu jest twierdzeniem; `Open` to nierozstrzygnięte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Answer,
    Evidence,
    Open,
}

/// Kolejność jest kontraktem, więc mieszka w jednej tablicy, a nie w trzech pętlach.
const SECTIONS: [Section; 3] = [Section::Answer, Section::Evidence, Section::Open];

impl Section {
    /// Nazwa sekcji — ta sama w nagłówku pliku, w prompcie kroku i w `run.json`.
    ///
    /// 2026-08-23 — `pub` z tego samego powodu, co [`Kind::name`]: `commands::run` zapisuje
    /// w `run.json`, które sekcje musiał dopisać za agenta. Druga kopia tej tabeli byłaby drugim
    /// miejscem, w którym mieszka odpowiedź na pytanie „jak ta sekcja się nazywa" — i przy
    /// pierwszej zmianie nazwy plik biegu mówiłby co innego niż plik przekazania.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Answer => "Answer",
            Self::Evidence => "Evidence",
            Self::Open => "Open",
        }
    }
}

/// Zamknięty zbiór siedmiu wartości plus wariant „coś nowego albo cudzego".
///
/// [`Kind::Other`] jest niezmiennikiem 5 zapisanym w typie: starszy albo nowszy Loadout,
/// ręczna edycja pliku, wpis z gałęzi, której jeszcze nie ma. Skan katalogu biegu nie ma
/// prawa się na tym przewrócić — jeden nieczytelny plik zamieniłby listę w UI w pustkę.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Brief,
    Findings,
    Plan,
    PatchSummary,
    Question,
    Answer,
    Review,
    Other(String),
}

impl Kind {
    /// Nazwa w pliku i **trzeci człon nazwy pliku**, więc bez `_`: podkreślenie jest
    /// separatorem pól w `<NN>__<from>__<kind>.md` i pojedyncze `patch_summary` czytałoby się
    /// jako granicę pola przy ręcznym oglądaniu katalogu.
    ///
    /// 2026-08-18 — `pub`, bo `commands::handoffs` wysyła ten napis na drut. To samo słowo,
    /// jedno miejsce (niezmiennik 13): tabela „wariant → napis" przepisana w warstwie komend
    /// rozjechałaby się z nazwą pliku przy pierwszym nowym rodzaju, i to po cichu.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Brief => "brief",
            Self::Findings => "findings",
            Self::Plan => "plan",
            Self::PatchSummary => "patch-summary",
            Self::Question => "question",
            Self::Answer => "answer",
            Self::Review => "review",
            Self::Other(raw) => raw.as_str(),
        }
    }

    /// Nieznana wartość jest **niesiona**, nie odrzucana (niezmiennik 5).
    fn parse(raw: &str) -> Self {
        match raw {
            "brief" => Self::Brief,
            "findings" => Self::Findings,
            "plan" => Self::Plan,
            "patch-summary" => Self::PatchSummary,
            "question" => Self::Question,
            "answer" => Self::Answer,
            "review" => Self::Review,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// Przekazania są niezmienne. Korekta to nowy plik, nie edycja starego [T6 §9].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Current,
    Superseded,
}

impl Status {
    /// Słowo, które stoi w pliku — i to samo, które jedzie na drut do okna.
    ///
    /// 2026-08-18 — `pub` z tego samego powodu, co [`Kind::name`]: `commands::handoffs`
    /// potrzebuje tego napisu, a druga jego kopia byłaby drugim miejscem, w którym mieszka
    /// odpowiedź na pytanie „czy to przekazanie jest jeszcze aktualne".
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Superseded => "superseded",
        }
    }

    /// Wszystko, co nie mówi wprost „zastąpione", jest bieżące. Ten kierunek domyślności jest
    /// wybrany: plik po ręcznej edycji ma zostać w obiegu, a nie zniknąć z listy po cichu.
    fn parse(raw: &str) -> Self {
        if raw.trim() == Self::Superseded.name() {
            Self::Superseded
        } else {
            Self::Current
        }
    }
}

/// Werdykt sędziego pętli: czy robota przeszła.
///
/// `Fail` jest **domyślne** i to jest cała treść tego typu. Nie ma tu wariantu „nie wiem": brak
/// werdyktu i werdykt odmowny prowadzą do tego samego — jeszcze jedna runda albo koniec biegu.
/// Trzeci wariant zmuszałby każdego wołającego do wybrania, co z nim zrobić, a jedyna bezpieczna
/// odpowiedź jest tą, którą daje `Fail`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Sędzia napisał, że robota przeszła. Pętla się domyka.
    Pass,
    /// Wszystko inne, **łącznie z brakiem werdyktu**.
    #[default]
    Fail,
}

/// Znacznik, którym sędzia pętli zapisuje werdykt. Musi być CAŁYM wierszem.
///
/// Wielkość liter i odstępy nie mają znaczenia; miejsce w wierszu ma. Powód stoi przy
/// [`verdict_in`] i jest zmierzony na tym, jak modele naprawdę odpowiadają.
const VERDICT_MARK: &str = "outcome:";

/// Werdykt z ciała przekazania.
///
/// JEDEN WYJĄTEK OD „CIAŁA NIE PARSUJEMY", nazwany i wąski. `commands::run::Live::hand_over`
/// składa front-matter sam i mówi wprost, że ani jedno jego pole nie pochodzi z tekstu modelu.
/// Dlatego sędzia pętli nie ma jak zapisać werdyktu tam, gdzie pierwotnie zaprojektowano
/// (spec §3) — jego jedynym kanałem jest ciało. Wyjątek jest jednym wierszem o sztywnym
/// kształcie, a nie furtką: nic innego z ciała nie jedzie do żadnego pola.
///
/// POLE W `## Answer` WYGRYWA Z LINIĄ PROZY. Od T-100 sędzia dostaje umówione pole `outcome`,
/// więc ten mocniejszy nośnik rozstrzyga także wtedy, gdy stary fallback na końcu odpowiedzi
/// mówi co innego. Przy braku pola zostaje dokładnie dotychczasowa reguła: decyduje ostatni
/// znacznik, bo modele powtarzają instrukcję, zanim zaczną pracować („napiszę OUTCOME: PASS,
/// jeśli testy przejdą"), a wniosek stawiają na końcu.
///
/// ZNACZNIK MUSI BYĆ CAŁYM WIERSZEM. Szukanie go w tekście przez `contains` zamyka pętlę na
/// zdaniu „once the tests are green I will write OUTCOME: PASS" — czyli nad czerwonymi testami,
/// na obietnicy werdyktu wziętej za werdykt.
#[must_use]
pub fn verdict_in(body: &str) -> Verdict {
    // 2026-08-25 (T-100) — pole jest jawną odpowiedzią na umowę z promptu, a końcowy wiersz
    // tylko zgodnościowym zapasem. Ta kolejność jest polityką: odwrócenie jej ponownie
    // uzależniłoby bieg od jednej literalnej linii prozy i zgubiło ustrukturyzowany werdykt.
    outcome_field_in(body).unwrap_or_else(|| fallback_verdict_in(body))
}

/// Umówione pole `outcome` z sekcji odpowiedzi, jeśli sędzia je podał.
fn outcome_field_in(body: &str) -> Option<Verdict> {
    let answer = heading_at(body, "Answer")?;
    let content = body[answer..]
        .find('\n')
        .map_or(body.len(), |offset| answer + offset + 1);
    let end = [heading_at(body, "Evidence"), heading_at(body, "Open")]
        .into_iter()
        .flatten()
        .filter(|at| *at > content)
        .min()
        .unwrap_or(body.len());

    body[content..end]
        .lines()
        .filter_map(verdict_on_line)
        .next_back()
}

/// Zgodnościowy wiersz `outcome: …` z prozy; jak dotąd rozstrzyga ostatni.
fn fallback_verdict_in(body: &str) -> Verdict {
    body.lines()
        .filter_map(verdict_on_line)
        .next_back()
        .unwrap_or_default()
}

fn verdict_on_line(line: &str) -> Option<Verdict> {
    let line = line.trim().to_ascii_lowercase();
    let rest = line.strip_prefix(VERDICT_MARK)?;
    match rest.trim() {
        "pass" => Some(Verdict::Pass),
        // Wiersz, który zaczyna się znacznikiem i mówi coś innego, jest werdyktem odmownym,
        // nie brakiem werdyktu: sędzia się wypowiedział, tylko nie przepuścił.
        _ => Some(Verdict::Fail),
    }
}

/// Czy sędzia w ogóle się wypowiedział — bez pytania o to, JAK.
///
/// OSOBNO OD [`verdict_in`], i to jest cała treść tej funkcji. Dla sterowania biegiem „nie
/// przepuścił" i „nic nie powiedział" są tym samym i tak zostaje (powód stoi przy [`Verdict`]).
/// DLA CZŁOWIEKA to dwie zupełnie różne historie: pierwsza to robota do poprawki, druga to
/// zepsuty kontrakt — i to właśnie ta druga przewróciła osiem biegów właściciela, w których
/// wiersz `outcome:` nie padł ani razu na 80 przekazaniach.
///
/// Znacznik czytany jest tym samym warunkiem, co w [`verdict_in`] (`VERDICT_MARK` na początku
/// przyciętego wiersza): dwa różne pytania o tę samą rzecz rozjechałyby się przy pierwszej
/// poprawce brzmienia, a rozjazd znaczyłby tu „powiedział, ale mówimy, że nie".
#[must_use]
pub fn said_an_outcome(body: &str) -> bool {
    body.lines()
        .any(|line| line.trim().to_ascii_lowercase().starts_with(VERDICT_MARK))
}

/// Co podaje wołający. Siedem pól — reszta front-mattera jest wyliczana przez Loadout
/// i wołający nie ma jak jej podać, właśnie o to chodzi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaDraft {
    pub run: String,
    pub step: u32,
    pub from: String,
    pub to: Vec<String>,
    pub kind: Kind,
    pub title: String,
    /// Lista tego, co Loadout **faktycznie wstrzyknął** w prompt tego kroku — nie to, co agent
    /// twierdzi, że przeczytał. Pochodzenie, o którym nie da się skłamać [T6 §10.2].
    pub reads: Vec<String>,
}

/// Trzynaście pól kontraktu plus worek na to, czego kontrakt nie zna.
///
/// Wszystkie trzynaście musi dać się odczytać z samego pliku (niezmiennik 4). Pole, które
/// mieszka wyłącznie w wierszu `SQLite`, znika razem z `loadout.db` — a wtedy przekazanie
/// oznaczone jako zastąpione wraca do obiegu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub id: String,
    pub run: String,
    pub step: u32,
    pub from: String,
    pub to: Vec<String>,
    pub kind: Kind,
    pub title: String,
    pub status: Status,
    pub supersedes: Option<String>,
    pub reads: Vec<String>,
    pub created: String,
    /// Długość **zapisanego** ciała, tak jak stoi w pliku. Przy odczycie cudzego pliku bywa
    /// nieprawdą i wtedy jest to fakt do zaraportowania, nie do wygładzenia — patrz
    /// [`Handoff::bytes_mismatch`].
    pub bytes: usize,
    pub est_tokens: usize,
    /// Klucze spoza kontraktu, w kolejności z pliku. Niezmiennik 5 po stronie odczytu:
    /// `serde(deny_unknown_fields)` zamieniłby jeden ręcznie doklejony wiersz w pustą listę
    /// w UI. Klucz z ciała agenta tu **nie trafia** — ciało nigdy nie jest parsowane.
    pub extra: BTreeMap<String, String>,
}

/// Trzynaście nazw kontraktu, w kolejności zapisu. Wszystko poza tą listą jest `extra`.
const FIELDS: [&str; 13] = [
    "id",
    "run",
    "step",
    "from",
    "to",
    "kind",
    "title",
    "status",
    "supersedes",
    "reads",
    "created",
    "bytes",
    "est_tokens",
];

/// Przekazanie odczytane z dysku.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handoff {
    pub path: PathBuf,
    pub meta: Meta,
    /// Ciało bez bloku front-mattera i bez wiersza separatora.
    pub body: String,
    /// Faktyczna długość ciała, policzona przy odczycie. Osobne pole, bo `meta.bytes` jest
    /// **deklaracją z pliku**, a te dwie liczby mają prawo się różnić.
    pub actual_bytes: usize,
}

impl Handoff {
    /// Czy plik kłamie o własnej długości.
    ///
    /// Cudzy plik (starszy Loadout, ręczna edycja, ucięty zapis) ma prawo tu trafić i nie jest
    /// błędem — ale przeliczenie `bytes` po cichu z zawartości zabrałoby jedyny sygnał, że coś
    /// się rozjechało.
    #[must_use]
    pub fn bytes_mismatch(&self) -> bool {
        self.meta.bytes != self.actual_bytes
    }

    /// Pełna kopia wskazana przez to przekazanie — w katalogu tego samego biegu.
    ///
    /// 2026-08-24 (T-114) — trwały wiersz zostaje względny, więc przeniesienie całego katalogu
    /// biegu nie psuje pliku. Bezwzględny adres składa dopiero czytelnik promptu z bieżącego
    /// położenia przekazania; nazwa musi zgadzać się z tą, którą nadał [`write_inner`].
    #[must_use]
    pub fn attachment(&self) -> Option<PathBuf> {
        let stem = self.path.file_stem()?.to_str()?;
        let name = format!("{stem}__full.md");
        let pointer = format!("Moved to {ATTACHMENTS_DIR}/{name}");
        let run_dir = self.path.parent()?.parent()?;
        self.body
            .lines()
            .any(|line| line == pointer)
            .then(|| run_dir.join(ATTACHMENTS_DIR).join(name))
    }

    /// Czy po normalizacji wszystkie trzy sekcje są faktycznie puste.
    #[must_use]
    pub fn left_nothing(&self) -> bool {
        sections_are_empty(&self.body)
    }
}

/// Co powstało na dysku i co z tego wynika dla kroku.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub path: PathBuf,
    /// `Some` **wyłącznie** wtedy, gdy w ciele stoi wskaźnik prowadzący do tego pliku
    /// (niezmiennik 21). Pełny tekst „na wszelki wypadek" przy ciele, którego nikt nie uciął,
    /// to artefakt, którego żaden skrypt nie czyta.
    pub attachment: Option<PathBuf>,
    /// Sekcje, których agent nie napisał, a Loadout je wstawił. Pusta lista znaczy, że ciało
    /// przyszło w umówionym kształcie — i to jest licznik, który warto oglądać [T6 §11.1].
    pub repaired: Vec<Section>,
    pub truncated: bool,
    /// `true`, kiedy znormalizowane sekcje nie niosą ani jednego znaku treści.
    pub left_nothing: bool,
}

/// Składa front-matter, naprawia sekcje, pilnuje limitu i zapisuje plik w `run_dir/handoffs/`.
///
/// `agent_body` jest **danymi niezaufanymi**. Jedyne, co się z nim dzieje, to normalizacja
/// nowych linii, uzupełnienie brakujących nagłówków sekcji i ewentualne cięcie na granicy
/// sekcji. Nic z niego nie wpływa na ani jedno pole front-mattera.
pub fn write_handoff(run_dir: &Path, draft: MetaDraft, agent_body: &str) -> Result<Written> {
    write_inner(run_dir, draft, agent_body, None)
}

/// Odczytuje jeden plik przekazania. Nieznany klucz i nieznany `kind` nie są błędem.
pub fn read_handoff(path: &Path) -> Result<Handoff> {
    let text = fs::read_to_string(path)?;
    let (front, body_at) = FrontMatter::split(&text).map_err(|error| match error {
        // Parser dostał sam tekst, więc nie miał czym wypełnić ścieżki. Tu wiemy.
        Error::NoFrontMatter { .. } => Error::NoFrontMatter {
            path: path.to_owned(),
        },
        other => other,
    })?;

    let body = text[body_at..].to_owned();
    Ok(Handoff {
        path: path.to_owned(),
        meta: meta_from(&front),
        actual_bytes: body.len(),
        body,
    })
}

/// Czyta `run_dir/handoffs/` bez bazy i bez zaufania do tego, kto te pliki pisał.
///
/// Kolejność wynikowa jest kolejnością nazw plików, bo prefiks `NN` jest numerem kroku —
/// to jedyne uporządkowanie, które przeżywa skasowanie `loadout.db` (niezmiennik 4).
pub fn scan_run_dir(run_dir: &Path) -> Result<Vec<Handoff>> {
    let dir = run_dir.join("handoffs");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        // Bieg, w którym jeszcze nikt niczego nie przekazał, ma zero przekazań, a nie błąd.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };

    // Płasko, nie rekurencyjnie: `attachments/` trzyma pliki `.md`, które przekazaniami nie
    // są, a spacer po drzewie zwróciłby je jako kolejne rekordy i nikt by nie zauważył.
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();

    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        match read_handoff(&path) {
            Ok(handoff) => out.push(handoff),
            // Niezmiennik 5: jeden nieczytelny plik nie zabiera ze sobą całej listy. Ślad
            // zostaje w dzienniku, bo cicha strata rekordu jest gorsza niż głośna.
            Err(error) => tracing::warn!("{} is not a readable handoff: {error}", path.display()),
        }
    }
    Ok(out)
}

/// Korekta: **nowy** plik z `supersedes: <old_id>`, a w starym zmienia się jedna linia.
///
/// Nadpisanie starego pliku w miejscu zabiera bieg historii i nikt tego nie zauważy, bo plik
/// dalej wygląda poprawnie [T6 §9]. Druga korekta tego samego `id` jest odmawiana
/// ([`super::Error::AlreadySuperseded`]) i nie zostawia po sobie ani jednego zapisu.
pub fn supersede(run_dir: &Path, old_id: &str, draft: MetaDraft, body: &str) -> Result<Written> {
    // Obie odmowy padają PRZED pierwszym zapisem. Wywołanie, które przewraca się w połowie,
    // zostawia bieg w stanie, którego nie produkuje żadna poprawna ścieżka kodu.
    let old = scan_run_dir(run_dir)?
        .into_iter()
        .find(|handoff| handoff.meta.id == old_id)
        .ok_or_else(|| Error::NoSuchHandoff {
            id: old_id.to_owned(),
        })?;

    if old.meta.status == Status::Superseded {
        return Err(Error::AlreadySuperseded {
            id: old_id.to_owned(),
        });
    }

    let written = write_inner(run_dir, draft, body, Some(old_id))?;
    flip_status(&old.path)?;
    Ok(written)
}

// ── zapis ─────────────────────────────────────────────────────────────────────────────────

fn write_inner(
    run_dir: &Path,
    draft: MetaDraft,
    agent_body: &str,
    supersedes: Option<&str>,
) -> Result<Written> {
    let dir = run_dir.join("handoffs");
    fs::create_dir_all(&dir)?;

    let stem = format!(
        "{:02}__{}__{}",
        draft.step,
        slugify(&draft.from),
        slugify(draft.kind.name())
    );
    let (path, mut file) = claim(&dir, &stem)?;

    // Nazwa attachmentu jest nazwą przekazania, do którego należy — para daje się rozpoznać
    // bez otwierania żadnego z dwóch plików.
    let name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(stem.as_str());
    let attachment_name = format!("{name}__full.md");
    let pointer = format!("Moved to {ATTACHMENTS_DIR}/{attachment_name}");

    let normalized = normalize(agent_body);
    let (shaped, repaired) = reshape(&normalized);
    let left_nothing = sections_are_empty(&shaped);
    let (body, truncated) = cap(&shaped, &pointer);

    let attachment = if truncated {
        // Niezmiennik 21: plik powstaje TYLKO wtedy, gdy w ciele stoi wskaźnik, który do
        // niego prowadzi. Trzyma **oryginał** agenta, nie to, co zostało po cięciu — inaczej
        // zgubionego zdania nie ma nigdzie [T6 §11.2].
        let attachments = run_dir.join(ATTACHMENTS_DIR);
        fs::create_dir_all(&attachments)?;
        let at = attachments.join(&attachment_name);
        fs::write(&at, normalized.as_bytes())?;
        Some(at)
    } else {
        None
    };

    let mut out = front_matter(draft, supersedes, body.len()).render();
    out.push('\n');
    out.push_str(&body);
    file.write_all(out.as_bytes())?;

    Ok(Written {
        path,
        attachment,
        repaired,
        truncated,
        left_nothing,
    })
}

/// Zajmuje ścieżkę w `handoffs/` i oddaje otwarty plik.
///
/// `create_new`, nie „sprawdź i zapisz": sprawdzenie i zapis to dwa wywołania, a między nimi
/// mieści się drugi krok biegu piszący tę samą nazwę. Kolizja idzie licznikiem `-2`, `-3`, …
/// — sufiks zaszyty na sztywno przechodzi drugi zapis i **nadpisuje** przy trzecim.
fn claim(dir: &Path, stem: &str) -> Result<(PathBuf, fs::File)> {
    let root = fs::canonicalize(dir)?;
    let mut nth = 1usize;
    loop {
        let name = if nth == 1 {
            format!("{stem}.md")
        } else {
            format!("{stem}-{nth}.md")
        };
        let path = dir.join(&name);

        // AC-6: pytanie brzmi „gdzie ten plik naprawdę leży", a odpowiada na nie wyłącznie
        // system plików. `starts_with` porównuje tekst, więc przechodzi na ścieżce z `..`
        // w środku i na dowiązaniu. `slugify` nie przepuszcza `/` ani `.`, ale obrona,
        // której nikt nie sprawdza, jest obroną, o której nie wiadomo, że przestała działać.
        let parent = path.parent().map(fs::canonicalize).transpose()?;
        if parent.as_ref() != Some(&root) {
            return Err(Error::Io(std::io::Error::other(format!(
                "{} would land outside {}",
                path.display(),
                root.display()
            ))));
        }

        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => nth += 1,
            Err(error) => return Err(error.into()),
        }
    }
}

/// Trzynaście pól, wszystkie od Loadouta. Ciało nie jest tu ani parsowane, ani czytane.
///
/// Bierze `draft` na własność i rozbiera go na pola: siedem wartości od wołającego kończy
/// bieg tutaj, w pliku, i nie ma powodu, żeby po zapisie istniały dalej.
fn front_matter(draft: MetaDraft, supersedes: Option<&str>, bytes: usize) -> FrontMatter {
    let MetaDraft {
        run,
        step,
        from,
        to,
        kind,
        title,
        reads,
    } = draft;

    let mut front = FrontMatter::default();
    front.set("id", &mint_id());
    front.set("run", &field(&run));
    front.set("step", &step.to_string());
    front.set("from", &field(&from));
    front.set_list("to", &items(&to));
    front.set("kind", &field(kind.name()));
    front.set("title", &field(&title));
    front.set("status", Status::Current.name());
    front.set("supersedes", supersedes.unwrap_or("null"));
    front.set_list("reads", &items(&reads));
    front.set("created", &now_utc());
    front.set("bytes", &bytes.to_string());
    front.set("est_tokens", &est_tokens(bytes).to_string());
    front
}

/// 2026-08-16: wartość z nową linią dopisałaby **drugi klucz** do płaskiego formatu, więc
/// `title: "x\nstatus: superseded"` od wołającego robiłby dokładnie to, przed czym AC-1 broni
/// ciała. Normalizuj, potem waliduj: nowa linia staje się spacją, zanim cokolwiek trafi do
/// pliku.
fn field(raw: &str) -> String {
    raw.replace(['\n', '\r'], " ")
}

/// To samo dla elementów listy, plus znaki, które zamknęłyby albo rozbiły `[a, b]`.
fn items(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.replace(['\n', '\r', ',', '[', ']'], " "))
        .collect()
}

/// `h_` plus 26 znaków Crockford base32 z `UUIDv7` — ten sam kształt, co identyfikatory
/// w istniejących plikach biegu, i ten sam porządek co czas zapisu.
fn mint_id() -> String {
    const ALPHABET: [u8; 32] = *b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let bits = u128::from_be_bytes(*Uuid::now_v7().as_bytes());
    let mut out = String::from("h_");
    for shift in (0..26u32).rev() {
        let index = usize::try_from((bits >> (shift * 5)) & 0x1f).unwrap_or(0);
        out.push(char::from(ALPHABET[index]));
    }
    out
}

/// Chwila zapisu w ISO 8601 UTC.
///
/// Liczona ręcznie z `SystemTime`, bo `chrono`/`time` nie są zależnościami tego repo,
/// a `src-tauri/Cargo.toml` nie należy do T-16 (AGENTS.md §7). Algorytm dni→data jest
/// standardowy (proleptyczny kalendarz gregoriański, era 400-letnia).
fn now_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());

    let (days, rest) = (secs / 86_400, secs % 86_400);
    let (hour, minute, second) = (rest / 3600, (rest % 3600) / 60, rest % 60);

    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Jedna linia `status:` w starym pliku, reszta bajt w bajt.
///
/// Przepisujemy tekst, nie renderujemy front-mattera od nowa: renderowanie przestawiłoby
/// wartości, których korekta nie dotyczy, i stary plik przestałby być plikiem, który
/// naprawdę powstał [T6 §9].
fn flip_status(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let (_, body_at) = FrontMatter::split(&text).map_err(|error| match error {
        Error::NoFrontMatter { .. } => Error::NoFrontMatter {
            path: path.to_owned(),
        },
        other => other,
    })?;

    let mut out = String::with_capacity(text.len());
    let mut done = false;
    for line in text[..body_at].split_inclusive('\n') {
        if !done && line.trim_end().starts_with("status:") {
            out.push_str("status: ");
            out.push_str(Status::Superseded.name());
            out.push('\n');
            done = true;
        } else {
            out.push_str(line);
        }
    }
    out.push_str(&text[body_at..]);

    fs::write(path, out)?;
    Ok(())
}

// ── kształt ciała ─────────────────────────────────────────────────────────────────────────

/// Nowe linie do `\n` i jedna na końcu. To jedyne, co dzieje się z tekstem agenta, zanim
/// zajmą się nim sekcje — i to jest cała lista.
fn normalize(body: &str) -> String {
    let mut out = body.replace("\r\n", "\n").replace('\r', "\n");
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Offset wiersza, który jest **dokładnie** nagłówkiem `## <name>`.
///
/// Po wierszach, nie po podłańcuchu: `## Answer` zacytowane w środku zdania nie jest
/// nagłówkiem i przesunięcie sekcji na taki cytat pocięłoby treść w losowym miejscu.
fn heading_at(body: &str, name: &str) -> Option<usize> {
    let head = format!("## {name}");
    let mut at = 0;
    while at < body.len() {
        let end = body[at..].find('\n').map_or(body.len(), |i| at + i + 1);
        if body[at..end].trim_end() == head {
            return Some(at);
        }
        at = end;
    }
    None
}

/// Ciało w umówionym kształcie plus lista sekcji, których agent nie napisał.
///
/// Komplet trzech nagłówków we właściwej kolejności przechodzi **nietknięty**. To nie jest
/// optymalizacja: AC-1 wymaga, żeby sfałszowany blok został tam, gdzie agent go postawił,
/// a przepisywanie ciała, którego nie trzeba naprawiać, jest jedynym sposobem, żeby go
/// zgubić po drodze.
fn reshape(body: &str) -> (String, Vec<Section>) {
    let found: Vec<Option<usize>> = SECTIONS
        .iter()
        .map(|section| heading_at(body, section.name()))
        .collect();

    if let [Some(answer), Some(evidence), Some(open)] = found[..]
        && answer < evidence
        && evidence < open
    {
        return (body.to_owned(), Vec::new());
    }

    let mut marks: Vec<usize> = found.iter().flatten().copied().collect();
    marks.sort_unstable();
    let first = marks.first().copied().unwrap_or(body.len());

    let content_of = |start: usize| -> &str {
        let after = body[start..]
            .find('\n')
            .map_or(body.len(), |i| start + i + 1);
        let end = marks
            .iter()
            .copied()
            .find(|mark| *mark > start)
            .unwrap_or(body.len());
        &body[after..end]
    };

    let mut out = String::with_capacity(body.len() + 64);
    let mut repaired = Vec::new();
    for (index, (section, at)) in SECTIONS.iter().zip(found.iter()).enumerate() {
        if at.is_none() {
            repaired.push(*section);
        }
        let own = at.map_or("", |start| content_of(start));
        // Proza bez nagłówka JEST odpowiedzią — to jedyna sekcja, do której może należeć,
        // i najczęstsza rzecz, jaką przyśle model [T6 §11.1].
        let content = if index == 0 {
            format!("{}{own}", &body[..first])
        } else {
            own.to_owned()
        };
        push_section(
            &mut out,
            section.name(),
            &content,
            index + 1 == SECTIONS.len(),
        );
    }

    (out, repaired)
}

/// Nagłówek, treść i **jeden** pusty wiersz przed następnym nagłówkiem.
fn push_section(out: &mut String, name: &str, content: &str, last: bool) {
    out.push_str("## ");
    out.push_str(name);
    out.push('\n');
    out.push_str(content);

    if last {
        if !content.is_empty() && !content.ends_with('\n') {
            out.push('\n');
        }
        return;
    }
    if content.is_empty() {
        out.push('\n');
    } else if content.ends_with("\n\n") {
        // Pusty wiersz już tam stoi — drugi rozjechałby ciało, które nic nie zawiniło.
    } else if content.ends_with('\n') {
        out.push('\n');
    } else {
        out.push_str("\n\n");
    }
}

/// Preambuła i treść trzech sekcji ciała, które przeszło przez [`reshape`].
fn split_sections(body: &str) -> Option<(&str, Vec<&str>)> {
    let mut at = Vec::with_capacity(SECTIONS.len());
    for section in SECTIONS {
        at.push(heading_at(body, section.name())?);
    }

    let mut contents = Vec::with_capacity(at.len());
    for (index, start) in at.iter().enumerate() {
        let after = body[*start..]
            .find('\n')
            .map_or(body.len(), |i| start + i + 1);
        let end = at.get(index + 1).copied().unwrap_or(body.len());
        contents.push(&body[after..end]);
    }
    Some((&body[..at[0]], contents))
}

/// Pustka po normalizacji, nie brak bajtów w surowej odpowiedzi.
///
/// Proza przed pierwszym nagłówkiem też jest treścią: [`reshape`] przypisuje ją do Answer, więc
/// pominięcie preambuły nazwałoby odpowiedź z tekstem pustą tylko dlatego, że agent dopisał
/// później trzy puste nagłówki (T-114, 2026-08-24).
fn sections_are_empty(body: &str) -> bool {
    split_sections(body).is_some_and(|(preamble, contents)| {
        preamble.trim().is_empty() && contents.iter().all(|content| content.trim().is_empty())
    })
}

/// Ostatnia jawna decyzja sędziego pętli, jeśli odpowiedź ją zawiera.
///
/// Cały wiersz i tylko dwie wartości: `outcome:` w prozie albo nieznana wartość nie może stać
/// się decyzją przez przypadek. `trim` dopuszcza jedynie wcięcie i końcowe spacje samego wiersza.
fn last_decision(body: &str) -> Option<&str> {
    body.lines()
        .rev()
        .map(str::trim)
        .find(|line| matches!(*line, "outcome: pass" | "outcome: fail"))
}

/// Zachowuje wskazaną decyzję dokładnie raz w już uciętym ciele.
fn keep_decision_once(body: &mut String, decision: &str) {
    let mut seen = false;
    let mut kept = String::with_capacity(body.len());
    for line in body.split_inclusive('\n') {
        if line.trim() == decision {
            if seen {
                continue;
            }
            seen = true;
        }
        kept.push_str(line);
    }
    if !seen {
        if !kept.ends_with('\n') {
            kept.push('\n');
        }
        kept.push_str(decision);
        kept.push('\n');
    }
    *body = kept;
}

/// Cięcie do [`BODY_CAP`] po granicy sekcji; wewnątrz sekcji — po granicy wiersza.
///
/// 2026-08-16, [T6 §11.2]: jednostką cięcia jest **sekcja**, bo ciało ucięte w połowie zdania
/// na 8192 bajcie przechodzi każdy test na „≤ 8 KB" i gubi dokładnie to jedno zdanie, dla
/// którego przekazanie powstało. Sekcja, która się nie mieści, zostaje nagłówkiem i jednym
/// wierszem wskaźnika — nagłówek zostaje, bo sekcja skasowana razem z nim nie zostawia
/// następnemu agentowi żadnego znaku, że cokolwiek tam było.
///
/// Wewnątrz sekcji tniemy tylko wtedy, gdy nie zachowała się jeszcze żadna treść — inaczej
/// pierwsza sekcja zjadałaby cały budżet. Taka sekcja dostaje **jedną trzecią** limitu, czyli
/// swój udział z trzech: bez tego jedna rozdęta sekcja z góry skazuje dwie pozostałe na sam
/// wskaźnik, nawet gdy miały po dwa wiersze i zmieściłyby się bez trudu.
fn cap(body: &str, pointer: &str) -> (String, bool) {
    if body.len() <= BODY_CAP {
        return (body.to_owned(), false);
    }
    let Some((preamble, contents)) = split_sections(body) else {
        return (body.to_owned(), false);
    };

    let heads: Vec<String> = SECTIONS
        .iter()
        .map(|section| format!("## {}\n", section.name()))
        .collect();
    let line = pointer.len() + 1;
    let costs: Vec<usize> = heads.iter().map(|head| head.len() + line).collect();
    // 2026-08-24 (T-114) — werdykt zwykle stoi na końcu odpowiedzi, czyli dokładnie tam, gdzie
    // cięcie 8 KB go usuwało. Rezerwujemy jego jeden wiersz przed doborem treści sekcji.
    let decision = last_decision(body);
    let content_cap = BODY_CAP.saturating_sub(decision.map_or(0, |said| said.len() + 1));

    // 2026-08-24 (T-114, naprawa po drugiej opinii) — poprawnie ułożone ciało może mieć
    // prozę przed `## Answer`, bo [`reshape`] zostawia taki kształt bez zmian. Preambuła jest
    // treścią, więc dostaje ten sam budżet co sekcje; wcześniej sama mogła przekroczyć limit,
    // zanim dopisaliśmy obowiązkowe nagłówki, wskaźniki i zarezerwowany werdykt.
    let minimum_sections: usize = costs.iter().sum();
    let preamble_budget = content_cap.saturating_sub(minimum_sections);
    let preamble_end = if preamble.len() <= preamble_budget {
        preamble.len()
    } else {
        last_line_boundary(preamble, preamble_budget)
    };
    let preamble_truncated = preamble_end < preamble.len();
    let mut out = String::from(&preamble[..preamble_end]);
    let mut truncated = preamble_truncated;
    let mut kept = !out.trim().is_empty();

    for (index, (head, content)) in heads.iter().zip(contents.iter()).enumerate() {
        let rest: usize = costs.iter().skip(index + 1).sum();
        if !preamble_truncated && out.len() + head.len() + content.len() + rest <= content_cap {
            out.push_str(head);
            out.push_str(content);
            kept = kept || !content.trim().is_empty();
            continue;
        }

        truncated = true;
        out.push_str(head);
        if !kept {
            let room = content_cap.saturating_sub(out.len() + line + rest);
            let budget = room.min(BODY_CAP / SECTIONS.len());
            out.push_str(&content[..last_line_boundary(content, budget)]);
        }
        // Bez pustego wiersza po wskaźniku: wskaźnik jest ostatnim wierszem sekcji i ma
        // stać zaraz za ostatnim zachowanym, żeby było widać, gdzie tekst się urwał.
        out.push_str(pointer);
        out.push('\n');
        for head in heads.iter().skip(index + 1) {
            out.push_str(head);
            out.push_str(pointer);
            out.push('\n');
        }
        break;
    }

    if let Some(decision) = decision {
        keep_decision_once(&mut out, decision);
    }

    (out, truncated)
}

/// Największy offset `<= budget`, który leży zaraz za nową linią. Zero, gdy takiego nie ma.
fn last_line_boundary(content: &str, budget: usize) -> usize {
    let mut cut = 0;
    for (at, byte) in content.bytes().enumerate() {
        if at >= budget {
            break;
        }
        if byte == b'\n' {
            cut = at + 1;
        }
    }
    cut
}

// ── odczyt ────────────────────────────────────────────────────────────────────────────────

/// Trzynaście pól z pliku, reszta do `extra`. Żadna wartość nie jest tu przeliczana —
/// `bytes`, które kłamie, ma zostać kłamstwem, bo to jedyny ślad po uciętym zapisie.
fn meta_from(front: &FrontMatter) -> Meta {
    let text = |key: &str| front.get(key).unwrap_or_default().to_owned();
    let number = |key: &str| {
        front
            .get(key)
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    };

    let extra = front
        .keys()
        .iter()
        .filter(|key| !FIELDS.contains(*key))
        .map(|key| {
            (
                (*key).to_owned(),
                front.get(key).unwrap_or_default().to_owned(),
            )
        })
        .collect();

    Meta {
        id: text("id"),
        run: text("run"),
        step: front
            .get("step")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        from: text("from"),
        to: front.list("to").unwrap_or_default(),
        kind: Kind::parse(front.get("kind").unwrap_or_default()),
        title: text("title"),
        status: Status::parse(front.get("status").unwrap_or_default()),
        // Brak klucza i jawne `null` znaczą to samo: nic. Cokolwiek innego i skan wymyśla
        // łańcuch korekt, którego nikt nigdy nie zapisał.
        supersedes: front
            .get("supersedes")
            .filter(|value| !value.is_empty() && *value != "null")
            .map(ToOwned::to_owned),
        reads: front.list("reads").unwrap_or_default(),
        created: text("created"),
        bytes: number("bytes"),
        est_tokens: number("est_tokens"),
        extra,
    }
}

#[cfg(test)]
mod tests {
    //! Werdykt sędziego pętli: czytany z CIAŁA przekazania, sztywnym znacznikiem, domyślnie `fail`.
    //!
    //! # Dlaczego testy jednostkowe W TYM PLIKU, a nie w `tests/it/`
    //!
    //! Ten sam powód, co przy `commands::run::tests`: [`verdict_in`] jest funkcją czystą od
    //! napisu do decyzji, więc droga do niej z zewnątrz nie dokłada ani jednego faktu, a dokłada
    //! bieg. Do tego `checks/quick-scope.sh` przy ręcznym biegu bez `TASK.md` nie wpuszcza zapisu
    //! do `src-tauri/tests/`, a kryterium ma powstać razem z kodem, nie po nim.
    //!
    //! # Rozbieżność ze spec-em, znaleziona przy wdrożeniu
    //!
    //! Projekt (`docs/superpowers/specs/2026-08-19-petla-z-limitem-tur-design.md` §3) mówił
    //! „tester pisze `outcome: pass` we front-matterze". Tego nie da się zrobić:
    //! `commands::run::Live::hand_over` składa front-matter **sam** i mówi o tym wprost — „ani
    //! jedno z tych pól nie pochodzi z tekstu, który przyszedł od modelu". Agent nie ma do
    //! front-mattera dostępu i mieć nie ma. Jedynym kanałem modelu jest ciało, więc reguła
    //! „ciała nie parsujemy" dostaje jeden wyjątek: wąski, nazwany, jeden wiersz.
    //!
    //! # Co tu jest najważniejsze
    //!
    //! Słabą wersją tych kryteriów jest sprawdzenie samego „PASS daje pass, FAIL daje fail".
    //! Przechodzi ją implementacja szukająca znacznika gdziekolwiek w wierszu — czyli ta, w
    //! której zdanie kończące się na „I will write OUTCOME: PASS" **zamyka pętlę nad czerwonymi
    //! testami**, na obietnicy werdyktu wziętej za werdykt. Dwa razy z rzędu moja własna wersja
    //! tego przypadku była za słaba i mutacja ją przeżyła: raz przez kropkę po znaczniku, raz
    //! przez echo instrukcji wtrącone w zdanie zamiast postawione w osobnym wierszu.

    use super::{Verdict, verdict_in};

    /// Ciało, jakie naprawdę oddaje model: akapit, potem wniosek w osobnym wierszu.
    fn body(lines: &[&str]) -> String {
        lines.join("\n")
    }

    #[test]
    fn a_marker_on_its_own_line_passes() {
        let said = body(&["Ran the suite. 40 rows, no problems.", "", "OUTCOME: PASS"]);

        assert_eq!(verdict_in(&said), Verdict::Pass);
    }

    #[test]
    fn the_same_marker_saying_fail_does_not_pass() {
        let said = body(&[
            "2 tests are red: the parser drops a quote.",
            "OUTCOME: FAIL",
        ]);

        assert_eq!(verdict_in(&said), Verdict::Fail);
    }

    #[test]
    fn no_marker_at_all_is_a_fail() {
        let said = body(&["Looks good to me, shipping it."]);

        assert_eq!(
            verdict_in(&said),
            Verdict::Fail,
            "if a missing verdict passed, the cheapest way through the loop would be to write no \
             verdict — and a model that forgot the line would be indistinguishable from one that \
             judged the work good"
        );
    }

    #[test]
    fn the_last_marker_decides_not_the_first() {
        /* OBA znaczniki są PEŁNYMI wierszami, i to jest cała moc tego przypadku. Model pokazuje
         * format, którego ma użyć, w osobnym wierszu — a potem, po pracy, pisze werdykt. Wersja,
         * w której echo jest wtrącone w zdanie, nie mierzy niczego: taki wiersz odpada już na
         * regule „znacznik jest całym wierszem" i w tekście zostaje jeden znacznik, więc „pierwszy"
         * i „ostatni" to ten sam wiersz. */
        let said = body(&[
            "The format you asked for looks like this:",
            "OUTCOME: PASS",
            "",
            "Now the actual result. The suite is not green: 2 failures in the header parser.",
            "",
            "OUTCOME: FAIL",
        ]);

        assert_eq!(
            verdict_in(&said),
            Verdict::Fail,
            "models restate the instruction they were given before doing the work, so the first \
             marker is an echo of the prompt and not a judgement. The conclusion is at the end."
        );
    }

    #[test]
    fn a_marker_buried_in_a_sentence_is_not_a_verdict() {
        /* Zdanie kończy się DOKŁADNIE znacznikiem, bez ani jednego znaku po nim. To jest kształt,
         * który przepuszcza każda implementacja szukająca znacznika gdziekolwiek w wierszu —
         * a wersja z kropką albo słowem po „PASS" odrzuca się sama i nie mierzy niczego. */
        let said = body(&[
            "They are not green yet. When they are, I will write OUTCOME: PASS",
            "",
            "For now the header parser drops a quote.",
        ]);

        assert_eq!(
            verdict_in(&said),
            Verdict::Fail,
            "THIS is the case the whole file exists for: an implementation searching the text with \
             contains() closes the loop over red tests on a sentence that promises a verdict instead \
             of giving one. The marker has to be the whole line."
        );
    }

    #[test]
    fn spacing_and_case_do_not_change_the_verdict() {
        for said in ["outcome: pass", "  OUTCOME:PASS  ", "Outcome:   Pass"] {
            assert_eq!(
                verdict_in(said),
                Verdict::Pass,
                "the verdict is a decision, not a typing exercise; refusing `outcome: pass` because \
                 it is lower case would send the run round another turn for a reason the person \
                 cannot see. It read: {said:?}"
            );
        }
    }
}
