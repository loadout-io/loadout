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

/// Dokąd odchodzi notatka, której człowiek nie chciał (2026-08-23, T-92).
///
/// **Nic nie jest twardo usuwane** [T6 §5.3]: zdanie skasowane z dysku jest zdaniem, którego
/// nikt nie umie ani odzyskać, ani wytłumaczyć następnemu, kto zaproponuje je drugi raz.
/// Katalog stoi obok [`NOTES_DIR`], a nie w nim, bo [`scan_notes`] czyta płasko i wyłącznie
/// `.md` — odrzucona notatka zostawiona wśród notatek wróciłaby ze skanu jako kandydatka.
pub const DISCARDED_DIR: &str = "discarded";

/// Nagłówek bloku — te same trzy słowa, które człowiek widzi w sekcji Pamięć
/// [`00-SYNTHESIS` §2.2]. Prompt i ekran mówią o tym samym zbiorze tym samym zdaniem,
/// więc pytanie „co model o tym wie" ma jedną odpowiedź, nie dwie.
const HEADING: &str = "What you know";

/// Klucze, które ta wersja rozumie. Wszystko poza nimi jedzie do [`Note::extra`] i wraca
/// na dysk nietknięte — plik od nowszego Loadouta nie ma prawa stracić pola przy zapisie,
/// którego to pole nie dotyczyło (niezmiennik 5).
const KNOWN: [&str; 11] = [
    "scope",
    // 2026-08-22 (T-80): z jakiego projektu ta notatka przyjechała. W kontrakcie, bo czyta to
    // ekran (`src/sections/memory/note-row.tsx`) — a to samo zdanie przywiezione z dwóch
    // projektów bez tej linii wygląda jak dwa fakty.
    "from",
    // 2026-08-22 (T-80): czyja jest ta notatka. Klucz dołożony do kontraktu, a nie zostawiony
    // w [`Note::extra`] — odpowiedź na pytanie „czyja to wiedza" nie ma prawa mieszkać w worku
    // rzeczy, których ta wersja nie rozumie, bo wtedy każdy czytelnik musi wiedzieć, że akurat
    // tego jednego klucza ma tam szukać.
    "agent",
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
    /// Z jakiego projektu ta notatka przyjechała. `None` znaczy „stąd" — zdanie napisane w tym
    /// Loadoucie nie ma pochodzenia do pokazania i nie ma go udawać (2026-08-22, T-80).
    pub from: Option<String>,
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
    /// **W SEKUNDACH, nigdy drobniej** — i to nie jest gust. To pole jest jedynym, które ten
    /// moduł porządkuje leksykograficznie ([`promote`]), a `2026-08-23T10:00:00.500Z` sortuje
    /// się PRZED `2026-08-23T10:00:00Z`, bo `.` stoi w ASCII przed `Z`. Ułamek dopisany do
    /// części wartości odwracałby więc kolejność względem każdej wartości wpisanej ręcznie
    /// albo przywiezionej z importu — czyli dokładnie tam, gdzie ta kolejność jest treścią.
    ///
    /// Pisze je [`mark_used`], wołane przez bieg w chwili, w której prompty z tym zdaniem są
    /// już złożone i bieg rusza (`commands::run`). Nie ten moduł i nie [`what_you_know`]:
    /// funkcja składająca blok nie widzi dysku i o to chodzi — inaczej samo pytanie „co model
    /// o tym wie" zmieniałoby odpowiedź na następne.
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

    /// Notatka o zakresie [`Scope::ThisAgent`], która nie umie powiedzieć, czyja jest.
    ///
    /// Odmowa, nie cicha degradacja do [`Scope::ThisProject`] (2026-08-22, T-80). Notatka, która
    /// miała jechać do jednego agenta, a pojechała do wszystkich kroków w projekcie, wygląda na
    /// ekranie identycznie jak zapisana i różni się wyłącznie tym, do ilu promptów wchodzi.
    /// Ten sam kierunek błędu, co przy [`scope_from`]: schodzimy do węższego zakresu, nigdy do
    /// szerszego — a tutaj węższego już nie ma, więc zostaje odmowa.
    ///
    /// Zdanie nazywa brakującą rzecz słowem, którym nazywa ją plik (`agent:`), i pyta o nią
    /// wprost: człowiek, któremu powiedziano wyłącznie „nie udało się zapisać", klika drugi raz
    /// (niezmiennik 14).
    #[error("This note is for one agent. Which agent is it for?")]
    NoAgentNamed,

    /// Odrzucenie notatki, która wchodzi do promptu (2026-08-23, T-92).
    ///
    /// Odmowa, nie ciche odstawienie po drodze. „Odrzuć" i „przestań używać" to dwie różne
    /// decyzje człowieka i mają zostać dwiema: notatka, która najpierw sama wyszła z promptu,
    /// a potem zniknęła z listy, znika w jednym kliknięciu z miejsca, w którym człowiek jej
    /// właśnie szukał — a on prosił o jedno.
    ///
    /// Zdanie mówi, CO ZROBIĆ, a nie czego nie wolno: „nie można odrzucić notatki w użyciu"
    /// zostawia człowieka przed przyciskiem, który odmawia, i bez drugiego, który by pomógł
    /// (niezmiennik 14).
    #[error("This note is in use. Stop using it first.")]
    StillInUse,
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
    record(root, draft, agent, None, None)
}

/// Zapisuje właścicielską kandydatkę razem z pełnym ciałem źródłowego Markdownu.
///
/// Szkielet T-124 jest celowo wykonywalny i czerwony: targety akceptacyjne mają dojść do
/// zachowania, którego jeszcze nie ma, zamiast zatrzymać się na brakującym symbolu podczas
/// kompilacji. Implementacja zastąpi `todo!()` atomowym zapisem w katalogu celu.
pub fn record_candidate_for_with_body(
    _root: &Path,
    _draft: NoteDraft,
    _agent: &str,
    _body: &str,
) -> Result<Note> {
    todo!("T-124 owner-aware full-body note write")
}

/// Zapisuje kandydatkę, którą zgłosił **bieg** — z jego identyfikatorem w polu `from`.
///
/// 2026-08-23 (T-92). Osobne wejście, a nie czwarty argument [`record_candidate_for`] i nie pole
/// w [`NoteDraft`]: tamten podpis i tamta struktura są konstruowane literałem w czterech plikach
/// spoza tego zadania, więc dopisanie pola zamieniłoby to kryterium w czerwień trzech innych
/// (`AGENTS.md` §7). Ten sam powód i ten sam kształt, co przy [`record_imported`].
///
/// **`from` niesie tu bieg, nie projekt**, i to jest jedyne miejsce w tym module, w którym tak
/// jest. Powód: notatka zaproponowana po biegu jest zdaniem, którego nikt jeszcze nie sprawdził,
/// a jedyną drogą do sprawdzenia jest transkrypt tego biegu. Roszczenie, do którego nie ma drogi
/// powrotnej, jest roszczeniem, którego nie da się później wycofać [T6 §5.1] — a wycofanie jest
/// całą różnicą między pamięcią a akrecją instrukcji.
pub fn record_candidate_from_run(root: &Path, draft: NoteDraft, run: &str) -> Result<Note> {
    record(root, draft, None, Some(run), None)
}

/// Skąd wzięła się notatka, której **nikt tutaj nie napisał** (2026-08-22, T-80).
///
/// Cztery pola, bo tyle trzeba, żeby zdanie dało się później sprawdzić i wycofać [T6 §5.1]:
/// notatka bez pochodzenia jest zdaniem, o którym nie wiadomo, czy projekt, z którego przyszło,
/// dalej tak uważa. Nazwy pól są nazwami kluczy we front-matterze i to jest jedyne miejsce
/// w drzewie, w którym te słowa stoją wypisane — format notatki ma jednego pisarza.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// Projekt, z którego to przyjechało. **Jedyne z czterech, które widzi człowiek** — resztę
    /// czyta ktoś, kto pyta „czy tamten projekt dalej tak uważa".
    pub from: String,
    /// Plik w tamtym projekcie, ścieżką względną wobec jego korzenia.
    pub source: PathBuf,
    /// Odcisk tamtego pliku w chwili skanu.
    pub source_hash: String,
    /// Z czyjego katalogu to wzięliśmy — dwie aplikacje trzymają pamięć w dwóch miejscach
    /// i to samo zdanie potrafi stać w obu.
    pub app: String,
}

/// Zapisuje notatkę **przywiezioną z cudzego projektu**, z całym pochodzeniem.
///
/// Osobne wejście, a nie czwarty argument [`record_candidate_for`]: tamten podpis pinuje
/// kryterium AC-1 tego zadania, a notatka zgłoszona przez agenta w biegu pochodzenia nie ma
/// i mieć nie będzie. Cała reszta zachowania jest ta sama — łącznie z tym, że notatka powstaje
/// jako [`Status::Suggested`]: import jest przywiezieniem zdania, nie zgodą na nie
/// (ARCHITECTURE §2 pyt. 5).
pub fn record_imported(
    root: &Path,
    draft: NoteDraft,
    agent: Option<&str>,
    origin: &Origin,
) -> Result<Note> {
    record(root, draft, agent, None, Some(origin))
}

fn record(
    root: &Path,
    draft: NoteDraft,
    agent: Option<&str>,
    from: Option<&str>,
    origin: Option<&Origin>,
) -> Result<Note> {
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

    // Nazwa z samych białych znaków to nie jest nazwa: `agent: ` w pliku wraca ze skanu jako
    // `None`, więc przepuszczona tutaj dałaby notatkę, która przy zapisie ma właściciela,
    // a przy odczycie nie ma. Jedno miejsce, jedna odpowiedź.
    let owner = agent.map(str::trim).filter(|name| !name.is_empty());

    // Też PRZED dotknięciem dysku i z tego samego powodu, co `because`. Nie ma trzeciej
    // odpowiedzi: albo notatka mówi, czyja jest, albo jej nie ma.
    if scope == Scope::ThisAgent && owner.is_none() {
        return Err(Error::NoAgentNamed);
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
            // Właściciela DOPISUJEMY, nigdy nie nadpisujemy (2026-08-22, T-80). Plik mógł przejść
            // przez ręce człowieka, a zgłoszenie agenta nie ma prawa przepisać cudzej redakcji —
            // dokładnie ta sama reguła, która wyżej trzyma `rule` i `because` nietknięte. Bez tej
            // linii notatka zapisana kiedyś bez właściciela nie miałaby go nigdy dostać; z
            // nadpisaniem drugie zgłoszenie po cichu przenosiłoby notatkę między agentami.
            if let Some(name) = owner.filter(|_| front.get("agent").is_none()) {
                front.set("agent", &one_line(name));
            }
            // Ta sama reguła dla biegu: notatka mówi o PIERWSZYM, który ją zgłosił. Drugie
            // zgłoszenie podbija `occurrences` i to jest cały jego ślad — przepisanie `from`
            // zabrałoby drogę powrotną do transkryptu, w którym to zdanie w ogóle powstało.
            if let Some(run) = from {
                add_missing(&mut front, "from", run);
            }
            // Ta sama reguła dla pochodzenia: dopisujemy brakujące, nie przepisujemy cudzego.
            // Notatka, która leży w bibliotece i już mówi, skąd jest, mówi to o PIERWSZYM
            // projekcie, który ją przywiózł — a drugi import tego nie unieważnia.
            if let Some(origin) = origin {
                add_missing(&mut front, "from", &origin.from);
                add_missing(&mut front, "source", &origin.source.to_string_lossy());
                add_missing(&mut front, "source_hash", &origin.source_hash);
                add_missing(&mut front, "app", &origin.app);
            }
            write_note(&path, &front, &raw[body_at..])?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(&dir)?;
            let mut front = FrontMatter::default();
            // Kolejność kluczy jest kontraktem [T6 §10.2] i jest tą samą, którą czyta człowiek
            // w edytorze: czym to jest, o czym to jest, co z tego wynika, skąd to wiemy.
            front.set("scope", scope_word(scope));
            // Właściciel stoi tuż pod zakresem, bo to jest jedna para: zakres mówi, jak daleko
            // ta notatka sięga, a ta linia — do kogo. Notatka niczyja nie dostaje pustego klucza:
            // pole, które w połowie plików znaczy „nie wiem", a w połowie „nikt", nie znaczy nic.
            if let Some(name) = owner {
                front.set("agent", &one_line(name));
            }
            // Bieg, który to zgłosił, stoi w tym samym miejscu i w tym samym kluczu co projekt,
            // z którego notatka przyjechała: oba odpowiadają na pytanie „skąd to zdanie", a dwa
            // klucze na jedno pytanie znaczyłyby, że czytelnik musi wiedzieć, którego szukać.
            if let Some(run) = from {
                front.set("from", &one_line(run));
            }
            // Pochodzenie stoi zaraz za właścicielem, bo odpowiada na to samo pytanie z drugiej
            // strony: kto tego używa i skąd to wzięliśmy. Notatka napisana tutaj nie dostaje ani
            // jednego z tych kluczy — pusty `from:` znaczyłby „przyjechała znikąd".
            if let Some(origin) = origin {
                front.set("from", &one_line(&origin.from));
                front.set("source", &one_line(&origin.source.to_string_lossy()));
                front.set("source_hash", &one_line(&origin.source_hash));
                front.set("app", &one_line(&origin.app));
            }
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
    let raw = read_or_missing(&path, id)?;
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

/// Odstawia notatkę: zostaje na liście i przestaje wchodzić do promptu.
///
/// 2026-08-23 (T-92) — DRUGI KIERUNEK JEDNEGO PRZEŁĄCZNIKA WRACA OBOK PIERWSZEGO. Od T-17 do
/// dziś mieszkał w `commands::memory::stop_using_note_inner`, a nagłówek tamtego modułu nazywał
/// to długiem wprost: „przy pierwszej okazji ma się przenieść do `memory::notes` obok
/// [`promote`], żeby oba kierunki jednego przełącznika mieszkały w jednym pliku" (niezmiennik
/// 23). Cena tamtego rozdzielenia była wąska i mierzalna — słowo `suggested` stało wypisane
/// w dwóch plikach — ale rosła: [`discard`] jest trzecim wejściem, które musi wiedzieć, co
/// znaczy „ta notatka nie wchodzi do promptu", i trzecia kopia tej wiedzy to już nie kopia,
/// tylko drugi zestaw reguł.
///
/// Nie pyta o [`Actor`] i to jest ta sama decyzja, co w warstwie komend: reguła „tylko człowiek"
/// broni WEJŚCIA do promptu (ARCHITECTURE §2 pyt. 5), a wyjście z niego nie jest uprawnieniem,
/// którego trzeba pilnować. Budżetu też nie sprawdza — zbiór w użyciu tylko maleje.
///
/// Notatka, która już nie jest w użyciu, zostaje **nietknięta**: stempel `modified` za
/// kliknięcie, które niczego nie zmieniło, jest kłamstwem o tym, kiedy ta notatka ostatnio się
/// zmieniła. Ta sama decyzja stoi po drugiej stronie przełącznika, w [`promote`].
pub fn stop_using(root: &Path, id: &NoteId, at: &str) -> Result<Note> {
    let path = root.join(NOTES_DIR).join(format!("{id}.md"));
    let raw = read_or_missing(&path, id)?;
    let (mut front, body_at) = FrontMatter::split(&raw)?;
    let note = note_from(&path, &front);

    // Notatka, która już nie jest w użyciu: plik zostaje NIETKNIĘTY, i to jest ta sama decyzja,
    // co po drugiej stronie przełącznika ([`promote`] przy notatce już `in-use`).
    if note.status == Status::Suggested {
        return Ok(note);
    }

    // Dwie linie w pliku i ani jedna więcej — dokładnie jak w [`promote`]. Złożenie
    // front-mattera od nowa przepisałoby klucze, o które nikt tej funkcji nie pytał, razem
    // z tymi, których ta wersja Loadouta nie zna (niezmiennik 5).
    front.set("status", "suggested");
    front.set("modified", &one_line(at));
    write_note(&path, &front, &raw[body_at..])?;
    read_note(&path)
}

/// Stempluje `last_used_at`: **ta notatka właśnie weszła do promptu**.
///
/// # Po co to istnieje (zmierzone 2026-08-23, T-92)
///
/// Pole jest w kontrakcie od T-17 i jego opis mówił, że „zapisuje je krok składania promptu
/// (T-15)". **Nie zapisywał.** Po 23 biegach właściciela każda notatka w tym repo dalej twierdziła,
/// że nie była użyta nigdy — wartość szła na dysk raz, jako `null`, i nie ruszała się z miejsca.
///
/// Skutek ma jednego adresata i nie jest kosmetyczny. Kiedy zakres jest pełny, [`promote`] odmawia
/// i pokazuje człowiekowi wymuszony wybór „najdawniej użyte pierwsze" [T6 §5.3]. Ta lista sortuje
/// się po tym polu — a skoro wszędzie stało `null`, sortowała się po identyfikatorze, czyli po
/// NAZWIE PLIKU. Człowiekowi, który ma zdecydować, co przestaje docierać do modelu, pokazywaliśmy
/// zdania ułożone alfabetycznie i mówiliśmy, że są ułożone po tym, jak dawno były potrzebne.
///
/// # Ścieżką, nie identyfikatorem
///
/// Jedyna funkcja w tym module, która tak robi, i powód jest wąski: wołający — bieg — trzyma
/// rachunek z tego, co naprawdę pojechało w promptach, a w rachunku stoją ŚCIEŻKI (`run.json`,
/// `MemoryRecord::reference`). Droga przez identyfikator znaczyłaby rozłożenie ścieżki na nazwę
/// pliku po to, żeby złożyć z niej z powrotem tę samą ścieżkę — czyli drugie miejsce, w którym
/// mieszka odpowiedź na pytanie, gdzie leży ten plik.
///
/// **Jedna linia w pliku i ani jedna więcej**, jak przy zmianie `status` w [`promote`].
/// `modified` zostaje NIETKNIĘTE: wejście do promptu nie jest zmianą treści ani stanu notatki,
/// a to pole czyta człowiek, żeby wiedzieć, co ktoś ostatnio poprawiał.
///
/// Zwraca `()`, a nie odczytaną notatkę — jedyne wyłamanie z reguły „wracamy z tym, co leży na
/// dysku", i też z powodu: tej wartości nie ma kto przeczytać. Bieg stempluje i idzie dalej,
/// więc odczyt po zapisie byłby odczytem dla nikogo (niezmiennik 21).
pub fn mark_used(path: &Path, at: &str) -> Result<()> {
    let raw = fs::read_to_string(path)?;
    let (mut front, body_at) = FrontMatter::split(&raw)?;
    front.set("last_used_at", &one_line(at));
    write_note(path, &front, &raw[body_at..])
}

/// Odrzuca kandydatkę: plik odchodzi do `<root>/discarded/`, **nie znika**.
///
/// Cztery rzeczy, które robi i które są całą jej treścią:
/// 1. **Wyłącznie [`Actor::You`]** ([`Error::OnlyYouCanDoThat`]). Kurator jest jeden i jest nim
///    człowiek — ta sama reguła, która trzyma [`promote`], czytana od drugiej strony. Agent,
///    który umie skasować cudzą notatkę, umie skasować tę, która opisuje jego własny błąd.
/// 2. **Notatka `in-use` to odmowa** ([`Error::StillInUse`]), nie ciche odstawienie po drodze.
/// 3. **Przeniesienie, nigdy `remove_file`** [T6 §5.3]. Nazwa pliku w `discarded/` niesie datę
///    podaną przez wołającego, bo ten moduł nie ma zegara (patrz nagłówek pliku) — i dlatego
///    dwie odrzucone kandydatki o tym samym tytule nie nadpisują się nawzajem.
/// 4. **Odmowy padają PRZED pierwszym zapisem.** Implementacja, która przenosi plik i dopiero
///    potem zwraca błąd, przechodzi każde `assert!(… .is_err())` i zostawia człowieka bez
///    notatki, o której powiedziano mu, że jej nie ruszono. Ta sama kolejność co w [`promote`].
///
/// Zwraca ścieżkę, pod którą notatka teraz leży: bez niej „nic nie jest twardo usuwane" jest
/// zdaniem w komentarzu, a nie czymś, co wołający umie pokazać człowiekowi.
pub fn discard(root: &Path, id: &NoteId, by: Actor) -> Result<PathBuf> {
    // 1. Wyłącznie człowiek — pierwsza linia, więc żadne inne wywołanie nie zdąży ruszyć pliku.
    let Actor::You { at } = by else {
        return Err(Error::OnlyYouCanDoThat);
    };

    let path = root.join(NOTES_DIR).join(format!("{id}.md"));
    let raw = read_or_missing(&path, id)?;
    let (front, _) = FrontMatter::split(&raw)?;

    // 2. …i dopiero potem odmowa dla notatki, która wchodzi do promptu. Też PRZED pierwszym
    //    zapisem: implementacja, która przenosi plik i zwraca błąd po fakcie, przechodzi każde
    //    `assert!(… .is_err())` i zostawia człowieka bez notatki, o której powiedziano mu, że
    //    jej nie ruszono.
    if note_from(&path, &front).status == Status::InUse {
        return Err(Error::StillInUse);
    }

    let gone = root.join(DISCARDED_DIR);
    fs::create_dir_all(&gone)?;
    // 3. Przeniesienie, nigdy `remove_file` [T6 §5.3]. Nazwa niesie moment podany przez
    //    wołającego, bo ten moduł nie ma zegara — bez niego druga odrzucona kandydatka o tym
    //    samym tytule nadpisuje pierwszą, czyli „nic nie jest usuwane" przestaje być prawdą
    //    przy drugim kliknięciu, a nie przy pierwszym.
    let landing = gone.join(format!("{id}__{}.md", file_safe(&at)));
    fs::rename(&path, &landing)?;
    Ok(landing)
}

/// Treść pliku notatki spod tej ścieżki — albo [`Error::NoSuchNote`].
///
/// Brak pliku jest tu odpowiedzią o notatce, nie o dysku: „nothing here has the id …" mówi
/// człowiekowi, czego szukał, a `No such file or directory` mówi mu o katalogu, którego nigdy
/// nie widział (niezmiennik 14).
fn read_or_missing(path: &Path, id: &NoteId) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(raw),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(Error::NoSuchNote(id.clone()))
        }
        Err(error) => Err(error.into()),
    }
}

/// Chwila z [`Actor::You`] w kształcie, który na pewno przeżyje jako **człon nazwy pliku**.
///
/// Lista dozwolonych, nigdy zakazanych — ten sam wybór, co w [`super::slugify`] i z tego samego
/// powodu. Dwukropki z ISO 8601 są tu jedyną rzeczą, która naprawdę przepada: w Finderze
/// wyświetlają się jako ukośniki, więc nazwa z nimi jest nazwą, której człowiek nie umie
/// przepisać. Data zostaje czytelna, bo po niej ten plik się znajduje.
fn file_safe(at: &str) -> String {
    at.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
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
        // Brak klucza i klucz pusty to jedna odpowiedź: „niczyja". Dwie różne odpowiedzi na to
        // samo pytanie znaczyłyby, że `agent: ` (bez wartości) opisuje agenta o pustej nazwie —
        // czyli kogoś, kogo żaden krok nigdy nie będzie się nazywał.
        agent: front
            .get("agent")
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned),
        from: front
            .get("from")
            .map(str::trim)
            .filter(|project| !project.is_empty())
            .map(ToOwned::to_owned),
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

/// Dopisuje klucz, którego w pliku jeszcze nie ma, i nie rusza tego, który już tam stoi.
fn add_missing(front: &mut FrontMatter, key: &str, value: &str) {
    if front.get(key).is_none() {
        front.set(key, &one_line(value));
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
