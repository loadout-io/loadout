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
//! Ciała są jeszcze `todo!()`. Szkielet ma się **skompilować** i paść w czasie wykonania:
//! test, który się nie kompiluje, niczego nie uruchomił (`AGENTS.md` §2a). `clippy::todo`
//! stoi w `Cargo.toml` na `deny`, więc żaden z nich nie przeżyje do pełnej bramki.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
    todo!("T-17 AC-5: notatki z {root:?}/notes, nieznany klucz i nieznany kind niesione dalej")
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
    todo!(
        "T-17 AC-1: wyłącznie InUse z zakresu {budget:?}, z {} notatek",
        notes.len()
    )
}

/// Zapisuje kandydatkę jako plik i oddaje ją odczytaną z powrotem.
///
/// Trzy rzeczy, które robi i które są całą jej treścią:
/// 1. **Odmawia bez uzasadnienia** ([`Error::NoBecause`]) — i odmawia **przed** pierwszym
///    zapisem, więc listing katalogu `notes/` przed i po jest identyczny. Walidacja po
///    zapisie zostawia plik, którego nikt nie chciał, i wygląda w teście tak samo.
/// 2. **Ignoruje `draft.status`.** Notatka powstaje jako [`Status::Suggested`], choćby draft
///    deklarował `in use` (ARCHITECTURE §2 pyt. 5).
/// 3. **Ta sama kandydatka to ten sam plik.** Znormalizowany `title` daje nazwę pliku, więc
///    drugie zgłoszenie podbija `occurrences` i przesuwa `modified`, a `status` zostaje
///    nietknięty — auto-promocja przy drugim wystąpieniu [T6 §5.3] jest świadomie nieobecna.
pub fn record_candidate(root: &Path, draft: NoteDraft) -> Result<Note> {
    todo!("T-17 AC-2/AC-3: kandydatka {draft:?} w {root:?}, zawsze Suggested")
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
    todo!("T-17 AC-2/AC-4: {id} w {root:?} na wniosek {by:?}, odmowy przed zapisem")
}
