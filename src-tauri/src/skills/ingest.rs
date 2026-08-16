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

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::Skill;
use super::place::Discovery;

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
    todo!("normalizuj, potem skanuj to, co znormalizowane, i zwróć oba wyniki razem: {raw:?}")
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

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Adres → co to właściwie jest, albo odmowa [T5 §5.1].
///
/// Czysta funkcja: nic nie pobiera i nie rozwiązuje nazw. Dzięki temu polityka adresu jest
/// testowalna bez sieci, a bramka, która wymaga internetu, jest bramką czerwieniejącą od
/// cudzych awarii.
pub fn resolve_url(url: &str) -> Result<Target, FetchError> {
    todo!("porównaj CAŁY host z ALLOWED_HOSTS, potem rozpoznaj kształt ścieżki: {url}")
}

/// Czy łańcuch odwiedzonych adresów wolno było przejść do końca.
///
/// `chain[0]` to adres, o który poprosiliśmy; każdy następny to jeden przeskok. Sprawdzamy
/// **każde ogniwo**, nie tylko pierwsze: przekierowanie jest tym, jak dozwolony host oddaje
/// treść z niedozwolonego, a `--max-redirs` w argv curla jest deklaracją narzędzia, nie naszym
/// dowodem (niezmiennik 20).
pub fn follow_policy(chain: &[&str]) -> Result<(), FetchError> {
    todo!("każde ogniwo na liście hostów, najwyżej {MAX_REDIRECTS} przeskoki: {chain:?}")
}

/// Czyta najwyżej `limit` bajtów i odmawia, kiedy źródło ma ich więcej.
///
/// Czyta o jeden bajt za dużo NAUMYŚLNIE: „przeczytaj dokładnie limit i przestań" nie odróżnia
/// pliku dokładnie na limicie od pliku uciętego w połowie. Sprawdzenie u siebie, po fakcie,
/// zamiast zaufania `--max-filesize`.
pub fn read_capped(source: impl Read, limit: u64) -> Result<Vec<u8>, FetchError> {
    let _ = source;
    todo!("czytaj do limitu {limit} i odmów przy pierwszym bajcie ponad")
}

/// Suma rozmiarów wszystkich plików umiejętności wobec limitu całości.
///
/// Osobno od [`read_capped`], bo to jest inny atak: pięć plików mieszczących się w limicie
/// pojedynczym, których suma zapycha dysk.
pub fn total_within(sizes: &[u64], limit: u64) -> Result<u64, FetchError> {
    todo!("zsumuj {} rozmiarów i porównaj z {limit}", sizes.len())
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
    todo!("zbuduj `curl` z limitami i adresem {url}")
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
    todo!("wczytaj SKILL.md i pliki obok niego z {}", dir.display())
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
