//! Notatki: model, skan plików, filtr statusu, blok „What you know", budżety, promocja.
//!
//! Jedno zdanie trzyma cały ten plik: **notatka `suggested` nigdy nie trafia do żadnego
//! promptu** [`00-SYNTHESIS` §2.2: „only 'in use' notes go into a prompt"]. Cicha porażka,
//! przed którą stoi [`what_you_know`], jest banalna: filtr po statusie stoi w jednym miejscu
//! (lista do wyświetlenia), a przy składaniu bloku ktoś dokleja „a na końcu jeszcze kandydatki,
//! żeby model miał kontekst". Wszystkie testy dalej są zielone, bo sprawdzają `note.status`,
//! a nie **zmontowany tekst**. Od tej chwili jedna halucynacja agenta jest trwałym prawem
//! projektu [`00-SYNTHESIS` §2.1].
//!
//! Dlatego [`what_you_know`] jest **jedynym** wyjściem do promptu i filtruje sama: gdyby
//! wołający musiał podać już przefiltrowaną listę, filtr istniałby w dwóch miejscach, a drugie
//! miejsce jest dokładnie tym, do którego ktoś dopisuje „a na końcu jeszcze kandydatki".
//!
//! **Dwa stany, promuje wyłącznie człowiek** [ARCHITECTURE §2 pyt. 5]. To unieważnia cykl
//! z czterema stanami i auto-promocję przy drugim wystąpieniu z [T6 §5.3]: powtórzenie
//! podbija `occurrences` i nic poza tym. Powód jest zmierzony, nie estetyczny — arXiv
//! 2608.11095 na 1867 repozytoriach: nieobsługiwana akrecja instrukcji to sama choroba,
//! a nie ma podstaw sądzić, że agenci Loadouta będą lepszymi kuratorami niż ludzie, którzy
//! te pliki utrzymywali [T6 §5.3].
//!
//! Czego tu nie ma i nie będzie:
//! - `Connection` — `notes.rs` czyta i pisze **pliki**, wiersz do `SQLite` wkłada
//!   `store::writer` i nikt inny (niezmiennik 2). `UPDATE memory SET status=…` w promocji,
//!   „bo to przecież jedna linijka", jest tym, jak ten moduł przestaje działać po `rm loadout.db`;
//! - zegara — moment działania człowieka przychodzi w [`Actor::You`], a moment zgłoszenia
//!   w [`NoteDraft::at`]. Funkcja, która sama czyta zegar, nie da się sprawdzić na równość
//!   bajtową, a AC-4 pyta dokładnie o to;
//! - `#[cfg(…)]` — ścieżki składamy `PathBuf`em (niezmiennik 3).
//!
//! Format pliku jest płaski i czytany wspólnym czytnikiem z [`super::FrontMatter`] — jeden
//! parser na oba rodzaje pamięci, bo dwa rozjeżdżają się w tydzień (niezmiennik 23):
//!
//! ```text
//! ---
//! scope: this-project
//! kind: fact
//! title: The tenant is resolved before the guard
//! rule: An unresolved tenant surfaces as 401, not 400.
//! because: run 7f3a step 2 reproduced it
//! status: suggested
//! occurrences: 1
//! modified: 2026-08-15T10:31:02Z
//! last_used_at: null
//! ---
//!
//! How to apply: read the middleware before blaming the guard.
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::FrontMatter;

/// Katalog notatek wewnątrz korzenia. Jedna nazwa, w jednym miejscu.
const NOTES_DIR: &str = "notes";

/// Nagłówek bloku — te same trzy słowa, które człowiek widzi w sekcji Pamięć
/// [`00-SYNTHESIS` §2.2]. Prompt i ekran mówią o tym samym zbiorze tym samym zdaniem,
/// więc pytanie „co model o tym wie" ma jedną odpowiedź, nie dwie.
const HEADING: &str = "What you know";

/// Klucze, które ta wersja rozumie. Wszystko poza nimi jedzie do [`Note::extra`] i wraca
/// na dysk nietknięte — plik od nowszego Loadouta nie ma prawa stracić pola przy zapisie,
/// którego to pole nie dotyczyło (niezmiennik 5).
const KNOWN: [&str; 9] = [
    "scope",
    "kind",
    "title",
    "rule",
    "because",
    "status",
    "occurrences",
    "modified",
    "last_used_at",
];

/// Dwa stany i ani jeden trzeci [ARCHITECTURE §2 pyt. 5].
///
/// Trzeci stan (`confirmed`, `corroborated`, `archived`, `replaced`) jest świadomie poza
/// zakresem: poprzedni prototyp ma trzy stany i korroboratora, którym w praktyce nie ma kto być.
/// Na dysku: `status: suggested` i `status: in-use`. Na ekranie: `Suggested` i `In use`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Suggested,
    InUse,
}

/// Trzy zakresy, które mają własny budżet [T6 §6]. Czwarty („This run") to przekazania,
/// czyli [`super::handoff`], nie notatki.
///
/// **Zakres mieszka we front-matterze notatki, nie w katalogu, w którym plik leży.** Powód
/// jest mechaniczny: [`promote`] i [`scan_notes`] znają dokładnie jeden korzeń, więc zakres
/// wyprowadzony z położenia pliku byłby dla nich niepoznawalny. Powód drugi jest ważniejszy:
/// notatka przeniesiona albo skopiowana między korzeniami nie ma prawa po cichu zmienić
/// limitu, przeciw któremu się liczy (niezmiennik 4 — plik mówi, czym jest).
///
/// Na dysku: `everywhere`, `this-project`, `this-agent`. Nieczytelna albo brakująca wartość
/// czyta się jako [`Scope::ThisProject`] i **nigdy** jako [`Scope::Everywhere`]: notatki,
/// której nie umiemy przeczytać, nie awansujemy na regułę obowiązującą we wszystkich
/// projektach (niezmiennik 5 mówi „wczytaj dalej", nie „zgadnij szerzej").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Everywhere,
    ThisProject,
    ThisAgent,
}

impl Scope {
    /// Sufit **zbioru w użyciu** dla tego zakresu, w jednostkach długości [T6 §5.3].
    ///
    /// Liczby są z raportu, nie z gustu: 1000 / 1500 / 800. Cytat, który za nimi stoi:
    /// „The budget is the real anti-bloat mechanism. Each scope has a hard cap on the *active*
    /// set" oraz „When a promotion would exceed the cap, Loadout does not silently trim —
    /// it shows a forced choice" [T6 §5.3]. Stąd [`Error::MemoryFull`] zamiast cichego
    /// przycięcia: przycięcie wygląda w UI identycznie jak sukces, a różni się tym, że
    /// notatka, którą człowiek zatwierdził, przestaje docierać do modelu.
    ///
    /// To jest sufit **wyliczony z plików**, nie licznik trzymany gdziekolwiek indziej —
    /// suma [`Note::est_tokens`] notatek `InUse` w tym zakresie.
    #[must_use]
    pub const fn cap(self) -> usize {
        match self {
            // Wszędzie: najwęższy budżet, bo ten tekst jedzie do KAŻDEGO promptu w każdym
            // projekcie — 1000 jednostek na wszystko, co ma być prawdą zawsze.
            Self::Everywhere => 1000,
            // Ten projekt: 1500, bo fakty o konkretnym kodzie są najliczniejsze i najbardziej
            // konkretne, a jadą tylko tutaj.
            Self::ThisProject => 1500,
            // Ten agent: 800. Blok agenta jedzie w każdym jego biegu OBOK dwóch powyższych,
            // więc jest doliczany do nich, a nie zamiast nich.
            Self::ThisAgent => 800,
        }
    }
}

/// `fact | rule | pitfall` [T6 §10.3] plus wariant „coś nowego albo cudzego".
///
/// [`Kind::Other`] to niezmiennik 5 zapisany w typie. Plik od nowszego Loadouta, plik po
/// ręcznej edycji, plik z gałęzi, której jeszcze nie ma — żaden z nich nie ma prawa przewrócić
/// skanu, bo strict parser zamienia jeden taki plik w pustą sekcję Pamięć po aktualizacji.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Fact,
    Rule,
    Pitfall,
    Other(String),
}

/// Tożsamość notatki: **nazwa jej pliku bez rozszerzenia**.
///
/// Nie ma linii `id:` we front-matterze i to jest decyzja: identyfikator w pliku obok nazwy
/// pliku to dwa źródła jednej prawdy, a rozjazd między nimi widać dopiero wtedy, gdy ktoś
/// zmieni nazwę pliku w Finderze. Nazwa pliku jest funkcją znormalizowanego `title`
/// ([`super::slugify`]), więc ta sama kandydatka zgłoszona dwa razy trafia w ten sam plik —
/// i stąd bierze się `occurrences`, bez żadnego dopasowywania po treści.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoteId(pub String);

impl std::fmt::Display for NoteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Notatka odczytana z pliku.
///
/// `status`, `because`, `occurrences` i `modified` są we **front-matterze** (niezmiennik 4).
/// Trzymanie statusu wyłącznie w kolumnie `SQLite`, „bo szybciej filtrować", znaczy, że po
/// `rm loadout.db` Loadout nie wie, co zatwierdziłeś — i przy odbudowie indeksu wszystko wraca
/// jako `suggested` albo, gorzej, jako `in use`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub id: NoteId,
    pub scope: Scope,
    /// Czyja to wiedza — nazwa agenta z front-mattera, `None` dla notatki niczyjej.
    ///
    /// 2026-08-22 (T-80): [`Scope::ThisAgent`] istnieje od T-17 i do dziś nie umiał powiedzieć,
    /// **którego** agenta dotyczy — filtrowanie po agencie nie miało po czym filtrować, więc
    /// trzeci zakres nie wchodził do żadnego promptu (`commands::run::what_the_agents_know`).
    /// Pole jest `Option`, bo notatka o zakresie `everywhere` albo `this-project` nie ma
    /// właściciela i nie ma udawać, że ma.
    ///
    /// Wartość jest tym, co w pliku napisał **człowiek** (`agent: backend-dev`), a nie
    /// identyfikatorem z biblioteki: plik jest prawdą (niezmiennik 4), a uuid w pliku, który
    /// człowiek otwiera w edytorze, jest polem, którego nie da się ani napisać, ani przeczytać.
    pub agent: Option<String>,
    pub kind: Kind,
    /// Zdanie, po którym człowiek poznaje notatkę na liście. Nie jedzie do promptu.
    pub title: String,
    /// Jedna linia, która jedzie do promptu — i jedyna rzecz z notatki, która tam jedzie
    /// [T6 §10.3: `rule TEXT NOT NULL -- one line`].
    pub rule: String,
    /// Uzasadnienie. **Obowiązkowe**: „no because, no memory" [T6 §10.3].
    ///
    /// arXiv 2608.11095: uzasadnienie instrukcji rozpada się szybciej niż sama instrukcja,
    /// a instrukcja bez uzasadnienia jest nieusuwalna — kasowanie kosztuje `O(2^|D|)`, bo
    /// trzeba od nowa wyprowadzić jej interakcje z każdą inną [T6 §5.1]. To jest jedyny powód,
    /// dla którego [`record_candidate`] odmawia zapisu notatki bez tego pola.
    pub because: String,
    pub status: Status,
    /// W ilu **osobnych** zgłoszeniach ta kandydatka się pojawiła.
    ///
    /// Liczba, nie bramka: auto-promocja przy drugim wystąpieniu z [T6 §5.3] jest świadomie
    /// nieobecna (ARCHITECTURE §2 pyt. 5). Powtórzenie jest sygnałem dla człowieka, nie
    /// decyzją za niego.
    pub occurrences: u32,
    /// Ostatnia zmiana **treści albo stanu**, ISO 8601 UTC. Przesuwa ją [`promote`]
    /// i [`record_candidate`], zawsze na czas podany przez wołającego.
    pub modified: String,
    /// Kiedy ta notatka ostatnio weszła do promptu. `None` = jeszcze nigdy.
    ///
    /// Porządek leksykograficzny ISO 8601 UTC **jest** porządkiem chronologicznym, więc
    /// sortowanie po tym polu nie potrzebuje ani parsera daty, ani zależności na `time`.
    /// `None` sortuje się przed każdą datą i to jest właściwy kierunek: notatka nieużyta
    /// nigdy jest „najdawniej użyta" i schodzi z listy pierwsza.
    ///
    /// Pole zapisuje krok składania promptu (T-15), nie ten moduł: [`what_you_know`] nie
    /// widzi dysku i o to chodzi.
    pub last_used_at: Option<String>,
    /// Ile jednostek długości ta notatka zabiera z budżetu zakresu.
    ///
    /// **Liczone przy odczycie** z długości `rule` przez [`super::est_tokens`], nigdy czytane
    /// z pliku. To jest różnica wobec `handoff::Meta::bytes`, gdzie deklaracja z pliku jest
    /// dowodem uciętego zapisu i dlatego zostaje kłamstwem — tutaj nikt tej liczby nie pisze,
    /// więc pole w pliku mogłoby wyłącznie kłamać.
    ///
    /// Liczy się `rule`, bo `rule` to jedyna część notatki, która trafia do promptu. Nagłówek
    /// bloku i myślniki są ramą o stałej długości i nie są obciążane zakresowi.
    pub est_tokens: usize,
    pub path: PathBuf,
    /// Klucze spoza kontraktu, w kolejności z pliku (niezmiennik 5).
    pub extra: BTreeMap<String, String>,
}

/// Co podaje zgłaszający kandydatkę. Reszta pliku jest funkcją Loadouta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteDraft {
    pub title: String,
    pub rule: String,
    pub because: String,
    pub scope: Scope,
    pub kind: Kind,
    /// Status **zadeklarowany** przez zgłaszającego — czytany i wyrzucany.
    ///
    /// Pole istnieje po to, żeby deklaracja agenta miała gdzie wylądować i **nie została
    /// uhonorowana**: notatka powstaje jako [`Status::Suggested`] niezależnie od tego, co tu
    /// stoi. Gdyby pola nie było, ta sama deklaracja przyjechałaby w `title` albo w ciele
    /// i nikt by nie zauważył, że w ogóle padła.
    pub status: Status,
    /// Kiedy to zgłoszono, ISO 8601 UTC. Podaje wołający — ten moduł nie ma zegara.
    pub at: String,
}

/// Kto wykonuje działanie. Do `in use` prowadzi **wyłącznie** [`Actor::You`].
///
/// `at` jest `String`em w ISO 8601 UTC, nie `OffsetDateTime`: `time` ani `chrono` nie są
/// zależnościami tego repo, a `src-tauri/Cargo.toml` nie należy do T-17, więc dołożenie ich
/// jest pytaniem do człowieka (`AGENTS.md` §7), nie dopiskiem. Ten sam kształt ma `created`
/// w przekazaniach [T6 §10.2].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    You { at: String },
    Agent(String),
    Loadout,
}

/// Blok „What you know" i **pełny rachunek z tego, co się z notatkami stało**.
///
/// `used` i `dropped` są po to, żeby UI mogło powiedzieć prawdę o tym, co pojechało do modelu.
/// Nie są dowodem: dowodem jest `text`. Implementacja, która dokleja „Also suggested: …" na
/// końcu, ma poprawne `used` i skłamany `text` — dlatego AC-1 asertuje na łańcuchu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Dokładnie ten tekst, który T-15 wkleja w prompt kroku. Dla zbioru bez ani jednej
    /// notatki `InUse` jest **pusty** (`""`), a nie „nagłówek bez treści": nagłówek nad
    /// pustką uczy model, że ta sekcja bywa pusta, i kosztuje długość za nic.
    pub text: String,
    /// Notatki, które są w `text`, w kolejności, w której tam stoją.
    pub used: Vec<NoteId>,
    /// Notatki `InUse` tego zakresu, które nie zmieściły się w budżecie.
    ///
    /// W normalnym biegu pusta, bo [`promote`] nie pozwala przekroczyć limitu. Niepusta staje
    /// się wtedy, gdy pliki przyszły skądinąd (repozytorium kolegi, ręczna edycja) — i wtedy
    /// UI ma powiedzieć, czego model nie dostał, zamiast milczeć.
    pub dropped: Vec<NoteId>,
}

/// Budżet jednego zakresu. Nosi zakres, bo [`what_you_know`] filtruje **sam**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub scope: Scope,
    pub cap: usize,
}

impl Budget {
    /// Budżet zakresu z limitem z [`Scope::cap`].
    #[must_use]
    pub const fn of(scope: Scope) -> Self {
        Self {
            scope,
            cap: scope.cap(),
        }
    }
}

/// Błędy notatek.
///
/// Osobny enum, nie warianty dopisane do [`super::Error`]: `memory/mod.rs` należy do T-16
/// i T-17 ma w nim dopisać jeden wiersz `pub mod notes;`, nic więcej (`AGENTS.md` §7).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// Cokolwiek, co zgłosił wspólny czytnik front-mattera z `memory/mod.rs`.
    #[error("{0}")]
    Memory(#[from] super::Error),

    /// „no because, no memory" [T6 §10.3].
    ///
    /// Zdanie jest po angielsku i nazywa brakującą rzecz słowem, które człowiek zna:
    /// pytanie „dlaczego to jest prawda" da się odpowiedzieć, a „pole `because` jest puste"
    /// nie mówi, po co to pole istnieje (niezmiennik 14).
    #[error("Every note needs a reason. Why is this true?")]
    NoBecause,

    /// Promocja czymkolwiek, co nie jest człowiekiem [ARCHITECTURE §2 pyt. 5].
    #[error("Only you can put a note to use.")]
    OnlyYouCanDoThat,

    /// Zakres jest pełny — wymuszony wybór, nie ciche przycięcie [T6 §5.3].
    #[error("Memory is full. Keep this one and stop using something else?")]
    MemoryFull {
        /// O ile jednostek długości ta promocja przekroczyłaby limit zakresu.
        over_by: usize,
        /// Kandydatki do odstawienia, **najdawniej użyte pierwsze**, i na tyle długa lista,
        /// żeby odstawienie jej prefiksu pokryło `over_by`. Kolejność jest treścią tej listy:
        /// wybór postawiony przed człowiekiem ma zaczynać się od notatki, której model
        /// najdawniej potrzebował.
        retire: Vec<NoteId>,
    },

    /// W tym korzeniu nie ma notatki o tym identyfikatorze.
    #[error("nothing here has the id {0}")]
    NoSuchNote(NoteId),
}

/// Skrót używany przez cały moduł notatek.
pub type Result<T> = std::result::Result<T, Error>;

/// Czyta `root/notes/*.md` bez bazy i bez zaufania do tego, kto te pliki pisał.
///
/// Kolejność wynikowa to kolejność identyfikatorów (czyli nazw plików) rosnąco. Musi być
/// niezależna od zegara i od kolejności, w jakiej system plików oddaje wpisy — inaczej dwa
/// skany tego samego katalogu dają dwa różne prompty, a AC-4 pyta wprost o równość bajtową.
///
/// Jeden nieczytelny plik nie zabiera ze sobą całej listy (niezmiennik 5): ślad zostaje
/// w dzienniku, bo cicha strata notatki jest gorsza niż głośna.
pub fn scan_notes(root: &Path) -> Result<Vec<Note>> {
    let dir = root.join(NOTES_DIR);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        // Korzeń, w którym nikt jeszcze niczego nie zapisał, ma zero notatek, a nie błąd.
        // Pusta sekcja Pamięć w nowym projekcie jest prawdą; czerwony pasek nie jest.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };

    // Płasko, nie rekurencyjnie, i wyłącznie `.md`: katalog notatek bywa też miejscem, w którym
    // ktoś trzyma załącznik albo `.DS_Store`, a spacer po drzewie zwróciłby je jako notatki.
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    // Sortujemy ścieżki, a nie wynik: kolejność, w jakiej system plików oddaje wpisy, nie jest
    // niczyją obietnicą, a AC-4 pyta wprost o równość bajtową dwóch skanów tego samego drzewa.
    paths.sort();

    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        match read_note(&path) {
            Ok(note) => out.push(note),
            // Niezmiennik 5: jeden nieczytelny plik nie zabiera ze sobą całej listy. Ślad
            // zostaje w dzienniku, bo cicha strata notatki jest gorsza niż głośna — a dziennik
            // jest jedynym miejscem, w którym wolno go zostawić: skan jest ODCZYTEM i plik
            // odłożony obok notatek byłby drugim miejscem, w którym mieszka status.
            Err(error) => tracing::warn!("{} is not a readable note: {error}", path.display()),
        }
    }
    Ok(out)
}

/// **Jedyne wyjście do promptu.** Bierze notatki, oddaje blok „What you know".
///
/// Filtruje po zakresie z `budget` i po [`Status::InUse`] — sam, w jednym miejscu. Nie widzi
/// dysku i nie ma jak niczego zapisać: funkcja, która składa prompt, nie ma prawa zmieniać
/// tego, co powie następne złożenie.
///
/// Ponad budżet nie przycina po cichu treści notatki: notatki, które się nie mieszczą,
/// wychodzą w [`Block::dropped`] w całości. Pół zdania w prompcie jest gorsze niż brak
/// zdania — model nie ma jak poznać, że czegoś nie doczytał.
#[must_use]
pub fn what_you_know(notes: &[Note], budget: Budget) -> Block {
    // Porządek jest identyfikatorem rosnąco, ustalony TUTAJ, a nie odziedziczony po wołającym.
    // AC-4 pyta o równość bajtową bloku sprzed i po odmowie promocji, a wołający podaje raz
    // wynik skanu, raz listę złożoną w pamięci — dwa różne porządki na tych samych faktach
    // dają dwa różne prompty i różnicy nie widać nigdzie poza rachunkiem za długość.
    let mut live: Vec<&Note> = notes
        .iter()
        .filter(|note| note.scope == budget.scope && note.status == Status::InUse)
        .collect();
    live.sort_by(|left, right| left.id.cmp(&right.id));

    let mut block = Block {
        text: String::new(),
        used: Vec::new(),
        dropped: Vec::new(),
    };

    let mut spent = 0;
    let mut lines = String::new();
    for note in live {
        // Notatka, która się nie mieści, wychodzi CAŁA. Pół zdania w prompcie jest gorsze niż
        // brak zdania: model nie ma jak poznać, że czegoś nie doczytał, więc czyta ucięte
        // zdanie jako całe. Mniejsza notatka za nią wchodzi dalej — zakres, który stanął na
        // jednej długiej regule, milczałby o wszystkim, co po niej.
        if spent + note.est_tokens > budget.cap {
            block.dropped.push(note.id.clone());
            continue;
        }
        spent += note.est_tokens;
        block.used.push(note.id.clone());
        lines.push_str("- ");
        lines.push_str(&note.rule);
        lines.push('\n');
    }

    // Pusto znaczy pusto. Nagłówek nad niczym uczy model, że ta sekcja bywa pusta, i kosztuje
    // długość za nic — a to jest dokładnie ten kształt, do którego ktoś dopisuje „a na końcu
    // jeszcze kandydatki, żeby model miał kontekst".
    if !block.used.is_empty() {
        block.text = format!("{HEADING}\n{lines}");
    }
    block
}

/// Zapisuje kandydatkę **niczyją** — całą treść ma [`record_candidate_for`].
///
/// Podpis zostaje nietknięty, bo pinują go kryteria zadań, których ta gałąź nie posiada
/// (`AGENTS.md` §7): pole dołożone do [`NoteDraft`] wywróciłoby każdy literał tej struktury
/// w cudzych plikach testowych, czyli zamieniłoby kryterium tego zadania w czerwień dwóch
/// innych. Nazwa agenta jedzie więc osobnym argumentem, o jedną funkcję niżej.
pub fn record_candidate(root: &Path, draft: NoteDraft) -> Result<Note> {
    record_candidate_for(root, draft, None)
}

/// Zapisuje kandydatkę jako plik i oddaje ją odczytaną z powrotem.
///
/// Cztery rzeczy, które robi i które są całą jej treścią:
/// 1. **Odmawia bez uzasadnienia** ([`Error::NoBecause`]) — i odmawia **przed** pierwszym
///    zapisem, więc listing katalogu `notes/` przed i po jest identyczny. Walidacja po
///    zapisie zostawia plik, którego nikt nie chciał, i wygląda w teście tak samo.
/// 2. **Ignoruje `draft.status`.** Notatka powstaje jako [`Status::Suggested`], choćby draft
///    deklarował `in use` (ARCHITECTURE §2 pyt. 5).
/// 3. **Ta sama kandydatka to ten sam plik.** Znormalizowany `title` daje nazwę pliku, więc
///    drugie zgłoszenie podbija `occurrences` i przesuwa `modified`, a `status` zostaje
///    nietknięty — auto-promocja przy drugim wystąpieniu [T6 §5.3] jest świadomie nieobecna.
/// 4. **Zapisuje, czyja jest ta notatka** (2026-08-22, T-80). `agent` ma trzy stany, nie dwa:
///    - `Some(nazwa)` — notatka należy do tego agenta i jego nazwa ląduje we front-matterze;
///    - `None` przy zakresie innym niż [`Scope::ThisAgent`] — notatka jest niczyja i tak ma być;
///    - `None` przy [`Scope::ThisAgent`] — **odmowa zapisu**. Nie cicha degradacja do zakresu
///      projektu: notatka, która miała jechać do jednego agenta, a pojechała do wszystkich
///      kroków w projekcie, jest dokładnie tym cichym rozszerzeniem zasięgu, przed którym stoi
///      [`scope_from`] („nie awansujemy notatki, której nie umiemy przeczytać").
pub fn record_candidate_for(root: &Path, draft: NoteDraft, agent: Option<&str>) -> Result<Note> {
    // SZKIELET T-80. Dwie nowe drogi — zapis nazwy agenta i odmowa dla notatki, która nie ma
    // czyja być — nie mają jeszcze ciała. Stara droga (notatka niczyja, zakres inny niż
    // `this-agent`) biegnie niżej nietknięta, bo pinują ją kryteria spoza tego zadania.
    if agent.is_some() || draft.scope == Scope::ThisAgent {
        todo!("T-80: a note has to be able to say whose it is, and to refuse when it cannot")
    }

    // Draft rozbieramy na pola w pierwszej linii, bo dzięki temu deklarowany status ma jedno
    // widoczne miejsce, w którym jest czytany i wyrzucany. Gdyby stał dalej jako `draft.status`,
    // dopisanie go do pliku byłoby o jedno słowo od prawdy — a to jest dokładnie ta zmiana,
    // po której dwa stany są ozdobą (ARCHITECTURE §2 pyt. 5).
    let NoteDraft {
        title,
        rule,
        because,
        scope,
        kind,
        status: _ignored_declaration,
        at,
    } = draft;

    // PRZED dotknięciem dysku. Walidacja po zapisie odmawia równie głośno i zostawia plik,
    // którego nikt nie chciał — a listing katalogu jest jedyną rzeczą, która te dwie
    // implementacje rozróżnia [T6 §10.3: „no because, no memory"].
    if because.trim().is_empty() {
        return Err(Error::NoBecause);
    }

    let id = NoteId(super::slugify(&title));
    let dir = root.join(NOTES_DIR);
    let path = dir.join(format!("{id}.md"));

    match fs::read_to_string(&path) {
        // Ta sama kandydatka drugi raz: podbijamy licznik i przesuwamy moment, i NIC POZA TYM.
        // Nie `status` — auto-promocja przy drugim wystąpieniu [T6 §5.3] jest świadomie
        // nieobecna (ARCHITECTURE §2 pyt. 5). Nie `rule` ani `because` — plik mógł przejść
        // przez ręce człowieka, a zgłoszenie agenta nie ma prawa nadpisać cudzej redakcji
        // (niezmiennik 4: plik jest prawdą, także wtedy, gdy prawdę dopisał człowiek).
        Ok(raw) => {
            let (mut front, body_at) = FrontMatter::split(&raw)?;
            let seen = front
                .get("occurrences")
                .and_then(|value| value.trim().parse::<u32>().ok())
                .unwrap_or(1);
            front.set("occurrences", &seen.saturating_add(1).to_string());
            front.set("modified", &one_line(&at));
            write_note(&path, &front, &raw[body_at..])?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(&dir)?;
            let mut front = FrontMatter::default();
            // Kolejność kluczy jest kontraktem [T6 §10.2] i jest tą samą, którą czyta człowiek
            // w edytorze: czym to jest, o czym to jest, co z tego wynika, skąd to wiemy.
            front.set("scope", scope_word(scope));
            front.set("kind", kind_word(&kind));
            front.set("title", &one_line(&title));
            front.set("rule", &one_line(&rule));
            front.set("because", &one_line(&because));
            // Kandydatka powstaje jako `suggested`, choćby zgłaszający deklarował `in use`:
            // deklaracja została wyrzucona wyżej i nie ma stąd drogi z powrotem.
            front.set("status", "suggested");
            front.set("occurrences", "1");
            front.set("modified", &one_line(&at));
            front.set("last_used_at", "null");
            write_note(&path, &front, "")?;
        }
        Err(error) => return Err(error.into()),
    }

    // Wracamy z tym, co LEŻY NA DYSKU, a nie z tym, co przed chwilą złożyliśmy w pamięci.
    // Wołający dostaje wtedy dokładnie to, co przeczyta następny skan — i nie ma jak zobaczyć
    // notatki, której zapis po cichu nie doszedł.
    read_note(&path)
}

/// Przestawia notatkę na [`Status::InUse`] — **wyłącznie** działaniem człowieka.
///
/// Kolejność jest częścią kontraktu: wszystkie trzy odmowy padają **przed** pierwszym zapisem.
/// Implementacja, która zapisuje plik, a dopiero potem zwraca błąd, przechodzi każde
/// `assert!(… .is_err())` i zostawia na dysku notatkę w użyciu, której nikt nie zatwierdził.
///
/// 1. `by` inne niż [`Actor::You`] → [`Error::OnlyYouCanDoThat`];
/// 2. `because` puste albo same białe znaki (np. wyczyszczone ręcznie na dysku) →
///    [`Error::NoBecause`] — „no because, no memory" obowiązuje też notatkę, która już leży;
/// 3. suma [`Note::est_tokens`] notatek `InUse` tego zakresu plus ta notatka ponad
///    [`Scope::cap`] → [`Error::MemoryFull`] z wymuszonym wyborem [T6 §5.3].
///
/// Przy `Ok`: w pliku zmienia się `status` i `modified` (na `at` z [`Actor::You`]) i nic
/// poza tym.
pub fn promote(root: &Path, id: &NoteId, by: Actor) -> Result<Note> {
    // 1. Wyłącznie człowiek — i to jest pierwsza linia, więc żadne inne wywołanie nie zdąży
    //    otworzyć pliku do zapisu [ARCHITECTURE §2 pyt. 5].
    let Actor::You { at } = by else {
        return Err(Error::OnlyYouCanDoThat);
    };

    let path = root.join(NOTES_DIR).join(format!("{id}.md"));
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::NoSuchNote(id.clone()));
        }
        Err(error) => return Err(error.into()),
    };
    let (mut front, body_at) = FrontMatter::split(&raw)?;
    let note = note_from(&path, &front);

    // 2. „no because, no memory" obowiązuje też notatkę, która już leży: inaczej skasowanie
    //    jednej linii w edytorze jest całą drogą dookoła tej reguły [T6 §5.1].
    if note.because.trim().is_empty() {
        return Err(Error::NoBecause);
    }

    // Notatka już w użyciu: nie ma czego przestawiać, więc plik zostaje nietknięty. Stempel
    // `modified` za kliknięcie, które niczego nie zmieniło, jest kłamstwem o tym, kiedy ta
    // notatka ostatnio się zmieniła — a to pole czyta człowiek, żeby wiedzieć, co jest świeże.
    if note.status == Status::InUse {
        return Ok(note);
    }

    // 3. Budżet zakresu, liczony Z PLIKÓW — nie z licznika trzymanego gdziekolwiek indziej.
    //    Licznik, którego nie da się odtworzyć z plików, jest polem łamiącym niezmiennik 4.
    let mut in_use: Vec<Note> = scan_notes(root)?
        .into_iter()
        .filter(|other| {
            other.scope == note.scope && other.status == Status::InUse && other.id != note.id
        })
        .collect();
    let spent: usize = in_use.iter().map(|other| other.est_tokens).sum();
    let cap = note.scope.cap();

    if spent + note.est_tokens > cap {
        // Najdawniej użyte pierwsze. Kolejność JEST treścią tej listy: wybór postawiony przed
        // człowiekiem ma zaczynać się od notatki, której model najdawniej potrzebował, a nie od
        // tej, która akurat stała pierwsza w katalogu. `None` (nigdy nieużyta) sortuje się
        // przed każdą datą i to jest właściwy kierunek.
        in_use.sort_by(|left, right| {
            (&left.last_used_at, &left.id).cmp(&(&right.last_used_at, &right.id))
        });
        return Err(Error::MemoryFull {
            over_by: spent + note.est_tokens - cap,
            // Cały zbiór w użyciu, nie sam prefiks pokrywający deficyt: wymuszony wybór, w
            // którym jest dokładnie jedna pozycja, nie jest wyborem [T6 §5.3]. Prefiks tej
            // listy pokrywa deficyt zawsze, kiedy pokryć go w ogóle można — a notatka dłuższa
            // niż cały limit zakresu nie zmieści się choćby po odstawieniu wszystkiego i wtedy
            // ta lista jest wszystkim, co uczciwie da się pokazać.
            retire: in_use.into_iter().map(|other| other.id).collect(),
        });
    }

    // Dwie linie w pliku i ani jedna więcej. Złożenie front-mattera od nowa z tego, co ta
    // funkcja wie, przepisałoby pola, o które nikt jej nie pytał — razem z kluczami, których
    // ta wersja Loadouta nie zna (niezmiennik 5).
    front.set("status", "in-use");
    front.set("modified", &one_line(&at));
    write_note(&path, &front, &raw[body_at..])?;
    read_note(&path)
}

// ── odczyt i zapis pliku ──────────────────────────────────────────────────────────────────

/// Notatka spod tej ścieżki. Tożsamością jest **nazwa pliku bez rozszerzenia**.
fn read_note(path: &Path) -> Result<Note> {
    let raw = fs::read_to_string(path)?;
    let (front, _) = FrontMatter::split(&raw)?;
    Ok(note_from(path, &front))
}

/// Front-matter na notatkę. Nie zwraca [`Result`], bo **żadne** pole nie jest tu powodem do
/// odrzucenia pliku: brak, literówka i wartość od nowszego Loadouta czytają się na wartość
/// domyślną, a nie na błąd (niezmiennik 5). Jedyną porażką odczytu jest plik, który wcale nie
/// ma nagłówka — i tę zgłasza [`FrontMatter::split`].
fn note_from(path: &Path, front: &FrontMatter) -> Note {
    let rule = front.get("rule").unwrap_or_default().to_owned();
    let extra = front
        .keys()
        .into_iter()
        .filter(|key| !KNOWN.contains(key))
        .map(|key| {
            (
                key.to_owned(),
                front.get(key).unwrap_or_default().to_owned(),
            )
        })
        .collect();

    Note {
        id: NoteId(
            path.file_stem()
                .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned()),
        ),
        scope: scope_from(front.get("scope").unwrap_or_default()),
        // SZKIELET T-80. Klucz agenta stoi w pliku i wraca na dysk (niezmiennik 5 niesie go
        // przez `extra`), ale skan go jeszcze nie CZYTA — i dopóki go nie czyta, notatka
        // `this-agent` nie umie powiedzieć, czyja jest. Wypełnia to implementacja T-80.
        agent: None,
        kind: kind_from(front.get("kind").unwrap_or_default()),
        title: front.get("title").unwrap_or_default().to_owned(),
        because: front.get("because").unwrap_or_default().to_owned(),
        status: status_from(front.get("status").unwrap_or_default()),
        // Brak licznika czyta się jako jedno wystąpienie: plik istnieje, więc ktoś tę notatkę
        // zgłosił co najmniej raz. Zero mówiłoby, że nie zgłosił jej nikt.
        occurrences: front
            .get("occurrences")
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(1),
        modified: front.get("modified").unwrap_or_default().to_owned(),
        last_used_at: front
            .get("last_used_at")
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "null")
            .map(ToOwned::to_owned),
        // Liczone przy odczycie z długości `rule`, nigdy czytane z pliku: tej liczby nikt nie
        // zapisuje, więc pole w pliku mogłoby wyłącznie kłamać.
        est_tokens: super::est_tokens(rule.len()),
        rule,
        path: path.to_owned(),
        extra,
    }
}

/// Nagłówek plus ciało. Pusty separator należy do nagłówka, tak jak przy przekazaniach
/// [T6 §10.2] — dzięki temu plik odczytany i zapisany bez zmian jest tym samym plikiem.
fn write_note(path: &Path, front: &FrontMatter, body: &str) -> Result<()> {
    let mut out = front.render();
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body);
    }
    fs::write(path, out)?;
    Ok(())
}

// ── słowa z pliku ─────────────────────────────────────────────────────────────────────────

/// Nieczytelna albo brakująca wartość to [`Scope::ThisProject`] i **nigdy**
/// [`Scope::Everywhere`]: notatki, której nie umiemy przeczytać, nie awansujemy na regułę
/// obowiązującą we wszystkich projektach.
fn scope_from(raw: &str) -> Scope {
    match raw.trim() {
        "everywhere" => Scope::Everywhere,
        "this-agent" => Scope::ThisAgent,
        _ => Scope::ThisProject,
    }
}

const fn scope_word(scope: Scope) -> &'static str {
    match scope {
        Scope::Everywhere => "everywhere",
        Scope::ThisProject => "this-project",
        Scope::ThisAgent => "this-agent",
    }
}

/// Nieznany rodzaj jedzie dalej jako [`Kind::Other`] i wraca do pliku niezmieniony. Brak
/// rodzaju to `fact`: notatka bez etykiety dalej jest czymś, co ktoś uznał za prawdę.
fn kind_from(raw: &str) -> Kind {
    match raw.trim() {
        "rule" => Kind::Rule,
        "pitfall" => Kind::Pitfall,
        "" | "fact" => Kind::Fact,
        other => Kind::Other(other.to_owned()),
    }
}

fn kind_word(kind: &Kind) -> &str {
    match kind {
        Kind::Fact => "fact",
        Kind::Rule => "rule",
        Kind::Pitfall => "pitfall",
        Kind::Other(raw) => raw,
    }
}

/// Do `in use` prowadzi **wyłącznie** dosłowne `in-use`. Każda inna wartość — literówka, pole
/// od nowszego Loadouta, pusty wiersz — czyta się jako `suggested`. Kierunek błędu jest tu
/// wybrany: notatka, której statusu nie rozumiemy, nie wchodzi do promptu.
fn status_from(raw: &str) -> Status {
    if raw.trim() == "in-use" {
        Status::InUse
    } else {
        Status::Suggested
    }
}

/// Wartość, która na pewno zmieści się w jednym wierszu front-mattera.
///
/// Nowa linia w `rule` albo w `because` rozcięłaby nagłówek na pół: wszystko za nią przestałoby
/// być nagłówkiem, a notatka wróciłaby ze skanu bez uzasadnienia — czyli jako notatka, której
/// nie da się promować. Tekst przychodzi od agenta, więc to nie jest przypadek hipotetyczny.
fn one_line(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}
