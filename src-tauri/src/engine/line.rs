//! Zdarzenie → wiersz historii. **Tutaj powstaje „czysty terminal", nie w CSS**
//! (niezmiennik 15, `docs/ARCHITECTURE.md` §6).
//!
//! Czternaście rodzajów wiersza [T2 §7.2] i pięć reguł zwijania [T2 §7.3] mieszkają w tym
//! pliku i nigdzie indziej. Który wiersz w ogóle istnieje, co mówi i czy jest zwinięty,
//! rozstrzyga [`Curator`]. Cicha wersja złamania nie wygląda jak zły wiersz — wygląda jak
//! [`Line`] niosący surowy `JSON` „na wszelki wypadek" i front decydujący, co pokazać: wtedy
//! czysty widok da się zepsuć arkuszem stylów, więc nie jest czysty.
//!
//! # Czego kurator NIE dostaje z [`AgentEvent`]
//!
//! Ten enum jest **świadomie stratny** [T1 §8.2] i to jest jego zaleta wszędzie poza tym
//! plikiem: [`AgentEvent::ToolStart`] niesie `id` i etykietę po ludzku,
//! [`AgentEvent::ToolEnd`] niesie **jednolinijkowe** podsumowanie. Kuracji to nie wystarcza
//! w trzech konkretnych miejscach:
//!
//! - wybór wariantu wiersza potrzebuje **rodziny narzędzia** (`Read` to nie `Edit`),
//! - [`Line::Read`] potrzebuje **pełnej ścieżki**, bo rozwinięcie wiersza pokazuje pliki,
//! - reguła 3 potrzebuje **pełnego wyjścia**, bo bez niego nie ma z czego wziąć ostatnich
//!   dwudziestu linii.
//!
//! Dlatego kurator dostaje [`Seen`]: zdarzenie neutralne **plus** [`Tool`] — te same fakty,
//! wyjęte z tej samej linii drutu przez `stream::decode`. To jest zarazem szew dla T-10:
//! taksonomia Codeksa niesie dokładnie je (`file_change.changes[].path`,
//! `command_execution.aggregated_output`) [T2 §9.3], więc drugi vendor wypełnia [`Seen`],
//! a nie przepisuje kuracji.
//!
//! # Stan tego pliku: SZKIELET (2026-08-16)
//!
//! Ciała zwracają **świadomie złą wartość** i są tak oznaczone komentarzem `SZKIELET`. To jest
//! wymagany kształt fazy, w której powstają kryteria: test ma się skompilować i paść **w czasie
//! wykonania, na braku ZACHOWANIA** — test, który się nie kompiluje, niczego nie uruchomił
//! (`AGENTS.md` §2a p. 5). `todo!()` tu nie stoi, bo `todo` jest `deny`
//! w `[workspace.lints.clippy]`; pusty zwrot pada tak samo, a bramka go przepuszcza.

use serde::Serialize;

use super::drivers::AgentEvent;

/// Rodzaj wiersza. Czternaście i ani jednego więcej [T2 §7.2].
///
/// Wariant [`LineKind::Thinking`] istnieje, ale kurator **nigdy** nie dokłada takiego wiersza
/// do historii (reguła 5): „Thinking…" jest stałym slotem na dole ekranu, nadpisywanym. Ta
/// jedna reguła usuwa większość wrażenia ściany tekstu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Nagłówek całego biegu.
    Run,
    /// Przerwa sekcyjna z etykietą; zaczepienie paska planu.
    Step,
    /// Agent dołączył albo skończył. Nigdy paplanina.
    Agent,
    /// Stały slot na dole ekranu. **Nigdy w historii.**
    Thinking,
    /// Czytanie plików, sklejane w licznik.
    Read,
    /// Szukanie w plikach.
    Search,
    /// Zmiana pliku; klik otwiera panel zmian, nie wiersz.
    Edit,
    /// Uruchomiona komenda — blok Warpa: udało się albo nie, jak długo, wyjście za klikiem.
    Ran,
    /// Proza agenta. Jedyna proza w widoku.
    Note,
    /// Pytanie do człowieka. Przyklejone, bo blokuje bieg.
    Asked,
    /// Przekazanie między agentami.
    Handoff,
    /// Ślad pamięci w biegu.
    Memory,
    /// Coś nie wyszło.
    Problem,
    /// Koniec tury.
    Done,
}

/// Jeden wiersz historii — **jedyna** rzecz, którą dostaje widok.
///
/// Reguła 1: jedna czynność, jeden wiersz; treść siedzi ZA wierszem, nigdy w nim. Dlatego
/// [`Line::text`] nie zawiera `\n`, a wszystko, co ma ciało, jedzie przez `detail`
/// i `detail_id`.
///
/// Warianty, których strumień nie produkuje ([`Line::Run`], [`Line::Step`], [`Line::Handoff`],
/// [`Line::Memory`]), są tu dlatego, że enum ma być kompletny wobec [T2 §7.2] — konstruuje je
/// planista (T-02) i pamięć (T-16).
#[derive(Debug, Clone, Serialize)]
pub enum Line {
    /// `▶ Fix the login bug · Research → Plan → Build`
    Run {
        /// Kto to zrobił.
        agent: String,
        /// Tekst wiersza, gotowy na ekran.
        text: String,
    },
    /// `── Planning`
    Step {
        /// Kto to zrobił.
        agent: String,
        /// Tekst wiersza, gotowy na ekran.
        text: String,
    },
    /// `Researcher 2 joined`
    Agent {
        /// Kto to zrobił.
        agent: String,
        /// Tekst wiersza, gotowy na ekran.
        text: String,
    },
    /// `Thinking…` — wariant istnieje, do historii nie wchodzi (reguła 5).
    Thinking {
        /// Kto myśli.
        agent: String,
    },
    /// `Read 6 files` — sklejone w oknie 2 s (reguła 4).
    Read {
        /// Kto to zrobił.
        agent: String,
        /// Tekst wiersza, gotowy na ekran.
        text: String,
        /// Ile czynności skleiło się w ten wiersz.
        count: u32,
        /// Ścieżki w kolejności czytania — to jest treść rozwinięcia.
        paths: Vec<String>,
        /// Klucz do pełnej treści w indeksie (T-06).
        detail_id: Option<u64>,
    },
    /// `Searched for "auth token" — 12 matches`
    Search {
        /// Kto to zrobił.
        agent: String,
        /// Tekst wiersza, gotowy na ekran.
        text: String,
        /// Ile trafień.
        count: u32,
        /// Pliki, w których coś było.
        paths: Vec<String>,
        /// Klucz do pełnej treści w indeksie (T-06).
        detail_id: Option<u64>,
    },
    /// `Edited src/auth.rs  +12 −4`
    Edit {
        /// Kto to zrobił.
        agent: String,
        /// Tekst wiersza, gotowy na ekran.
        text: String,
        /// Ile zmian skleiło się w ten wiersz.
        count: u32,
        /// Zmienione ścieżki w kolejności.
        paths: Vec<String>,
        /// Ile linii przybyło.
        added: u32,
        /// Ile linii ubyło.
        removed: u32,
        /// Klucz do panelu zmian (T-08).
        detail_id: Option<u64>,
    },
    /// `Ran tests — ok · 2.4s` / `Ran build — didn't work`
    Ran {
        /// Kto to zrobił.
        agent: String,
        /// Tekst wiersza, gotowy na ekran.
        text: String,
        /// Czy się udało. To, i tylko to, rozwija wiersz samo (reguła 3).
        ok: bool,
        /// Początek wyjścia, przycięty do 2 KB [T2 §6.3, obrona 2]. Reszta zostaje na dysku.
        preview: String,
        /// Ostatnie 20 linii wyjścia — **tylko** przy porażce. To jedyne miejsce, w którym
        /// ściana tekstu jest pożądana.
        detail: Vec<String>,
        /// Klucz do pełnego wyjścia w indeksie (T-06).
        detail_id: Option<u64>,
    },
    /// Proza agenta.
    Note {
        /// Kto to zrobił.
        agent: String,
        /// Tekst wiersza, gotowy na ekran.
        text: String,
    },
    /// `Needs your answer: which database?`
    Asked {
        /// Kto pyta.
        agent: String,
        /// Pytanie, gotowe na ekran.
        text: String,
        /// Odpowiedzi do wyboru; front rysuje je jako przyciski.
        options: Vec<String>,
    },
    /// `Planner → Implementer`
    Handoff {
        /// Kto to zrobił.
        agent: String,
        /// Tekst wiersza, gotowy na ekran.
        text: String,
    },
    /// `Saved a note — api-conventions.md`
    Memory {
        /// Kto to zrobił.
        agent: String,
        /// Tekst wiersza, gotowy na ekran.
        text: String,
        /// Plik pamięci, którego to dotyczy.
        path: String,
    },
    /// `Couldn't reach the API`
    Problem {
        /// Kto to zgłasza.
        agent: String,
        /// Zdanie po angielsku, gotowe na ekran.
        text: String,
        /// Kiedy limit u dostawcy wraca, w sekundach epoki uniksowej — **przepisane z drutu**.
        /// Godzinę lokalną renderuje front; to jest formatowanie, nie kuracja [T7 §7.2].
        resets_at: Option<i64>,
    },
    /// `Done · 2 turns · 6.2s · $0.15`
    Done {
        /// Kto skończył.
        agent: String,
        /// Tekst wiersza, gotowy na ekran.
        text: String,
        /// Ile tur agent wykonał — przepisane, nie przeliczone.
        turns: u32,
        /// Ile to trwało według vendora, w milisekundach — przepisane, nie przeliczone.
        duration_ms: u64,
        /// Koszt tury. `None`, kiedy vendor go nie podał: zero jest liczbą i sumuje się
        /// w rachunek, którego nikt nie zamawiał.
        cost_usd: Option<f64>,
    },
}

impl Line {
    /// Rodzaj wiersza.
    #[must_use]
    pub fn kind(&self) -> LineKind {
        match self {
            Self::Run { .. } => LineKind::Run,
            Self::Step { .. } => LineKind::Step,
            Self::Agent { .. } => LineKind::Agent,
            Self::Thinking { .. } => LineKind::Thinking,
            Self::Read { .. } => LineKind::Read,
            Self::Search { .. } => LineKind::Search,
            Self::Edit { .. } => LineKind::Edit,
            Self::Ran { .. } => LineKind::Ran,
            Self::Note { .. } => LineKind::Note,
            Self::Asked { .. } => LineKind::Asked,
            Self::Handoff { .. } => LineKind::Handoff,
            Self::Memory { .. } => LineKind::Memory,
            Self::Problem { .. } => LineKind::Problem,
            Self::Done { .. } => LineKind::Done,
        }
    }

    /// Kto ten wiersz wyprodukował. Wchodzi w klucz grupy sklejania: dwa agenty czytające
    /// pliki w tej samej sekundzie to dwa wiersze, nie jeden.
    #[must_use]
    pub fn agent(&self) -> &str {
        match self {
            Self::Run { agent, .. }
            | Self::Step { agent, .. }
            | Self::Agent { agent, .. }
            | Self::Thinking { agent }
            | Self::Read { agent, .. }
            | Self::Search { agent, .. }
            | Self::Edit { agent, .. }
            | Self::Ran { agent, .. }
            | Self::Note { agent, .. }
            | Self::Asked { agent, .. }
            | Self::Handoff { agent, .. }
            | Self::Memory { agent, .. }
            | Self::Problem { agent, .. }
            | Self::Done { agent, .. } => agent,
        }
    }

    /// Tekst wiersza — **jedna linia**, bez `\n` (reguła 1). Pusty tam, gdzie wiersz nic nie
    /// mówi sam z siebie.
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Thinking { .. } => "",
            Self::Run { text, .. }
            | Self::Step { text, .. }
            | Self::Agent { text, .. }
            | Self::Read { text, .. }
            | Self::Search { text, .. }
            | Self::Edit { text, .. }
            | Self::Ran { text, .. }
            | Self::Note { text, .. }
            | Self::Asked { text, .. }
            | Self::Handoff { text, .. }
            | Self::Memory { text, .. }
            | Self::Problem { text, .. }
            | Self::Done { text, .. } => text,
        }
    }

    /// Ile czynności skleiło się w ten wiersz. Rodzaje, które się nie sklejają, mówią 1.
    #[must_use]
    pub fn count(&self) -> u32 {
        match self {
            Self::Read { count, .. } | Self::Search { count, .. } | Self::Edit { count, .. } => {
                *count
            }
            _ => 1,
        }
    }

    /// Ścieżki, których wiersz dotyczy, w kolejności zdarzeń. Puste tam, gdzie nie ma plików.
    #[must_use]
    pub fn paths(&self) -> &[String] {
        match self {
            Self::Read { paths, .. } | Self::Search { paths, .. } | Self::Edit { paths, .. } => {
                paths
            }
            _ => &[],
        }
    }

    /// Czy wiersz jest rozwinięty od razu (reguły 2 i 3).
    ///
    /// To jest **wyliczane z rodzaju**, nigdy pole zapisane przy budowie: gdyby stało w polu,
    /// tabelę reguł mógłby nadpisać dowolny wołający, a „czysty widok" znowu zależałby od
    /// warstwy wyżej (niezmiennik 15).
    #[must_use]
    pub fn expanded(&self) -> bool {
        // SZKIELET (2026-08-16): tabela reguły 2 i wyjątek reguły 3 są całą treścią AC-4 (c)
        // i (b), więc tutaj ich nie ma. Odpowiedź `true` pada wyłącznie dla rodzaju, który
        // NIGDY nie wchodzi do historii, więc każdy wiersz historii jest tu zwinięty — łącznie
        // z prozą, pytaniem, błędem i strukturą, czyli dokładnie tymi, na których AC-4 pada.
        matches!(self.kind(), LineKind::Thinking)
    }

    /// Początek wyjścia, przycięty do 2 KB [T2 §6.3, obrona 2]. Pusty tam, gdzie nie ma wyjścia.
    #[must_use]
    pub fn preview(&self) -> &str {
        match self {
            Self::Ran { preview, .. } => preview,
            _ => "",
        }
    }

    /// Linie, które wiersz pokazuje **bez klikania** — czyli ostatnie 20 linii wyjścia, kiedy
    /// coś nie wyszło (reguła 3). Wszędzie indziej puste: reszta siedzi za `detail_id`.
    #[must_use]
    pub fn detail(&self) -> &[String] {
        match self {
            Self::Ran { detail, .. } => detail,
            _ => &[],
        }
    }

    /// Klucz do pełnej treści w indeksie. `Some` wszędzie tam, gdzie coś zostało na dysku.
    #[must_use]
    pub fn detail_id(&self) -> Option<u64> {
        match self {
            Self::Read { detail_id, .. }
            | Self::Search { detail_id, .. }
            | Self::Edit { detail_id, .. }
            | Self::Ran { detail_id, .. } => *detail_id,
            _ => None,
        }
    }
}

/// Stały slot na dole ekranu — jedyne miejsce, w którym widać myślenie (reguła 5).
///
/// Jeden fakt, jedno miejsce (niezmiennik 13): to jest **stan**, nadpisywany, nigdy wiersz
/// dokładany do historii.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Agent myśli.
    Thinking,
}

/// Rodzina czynności narzędzia — to, czego [`AgentEvent::ToolStart`] nie niesie.
///
/// Nie jest tym samym co [`LineKind`] i nie ma prawa być: rodzin jest tyle, ile ich rozróżnia
/// **kuracja**, a rodzajów wiersza tyle, ile rozróżnia **widok**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// `Read`, `Glob`, `NotebookRead`.
    Read,
    /// `Grep` i szukanie w sieci.
    Search,
    /// `Edit`, `Write`, `NotebookEdit`.
    Edit,
    /// `Bash` i wywołania serwerów narzędzi.
    Ran,
    /// Pytanie do człowieka.
    Asked,
    /// Uruchomienie podagenta.
    Agent,
}

/// Fakty o narzędziu, których [`AgentEvent`] świadomie nie niesie [T1 §8.2].
///
/// Wypełnia to `stream::decode` z tej samej linii drutu, z której powstało zdarzenie — i to
/// jest cały szew, w który T-10 wpina Codeksa.
#[derive(Debug, Clone)]
pub enum Tool {
    /// Czynność ruszyła: co to za rodzina i czego dotyczy (pełna ścieżka, wzorzec, komenda).
    Started {
        /// Rodzina czynności.
        action: Action,
        /// Czego dotyczy — **pełna** ścieżka, nie sama nazwa pliku.
        target: String,
    },
    /// Czynność się skończyła: pełne wyjście, nieprzycięte. Przycinanie jest kuracją i dzieje
    /// się w [`Curator`], nie po drodze.
    Ended {
        /// Pełne wyjście narzędzia.
        output: String,
    },
}

/// Jedno zdarzenie, tak jak widzi je kurator.
///
/// Czas przychodzi **argumentem**, nigdy z zegara czytanego w środku: kurator z własnym
/// zegarem nie da się przetestować bez `sleep`, a test z `sleep` mierzy planistę systemu
/// operacyjnego, nie okno sklejania.
#[derive(Debug, Clone, Copy)]
pub struct Seen<'a> {
    /// Kto to zrobił. Wchodzi w klucz grupy sklejania.
    pub agent: &'a str,
    /// Kiedy, w milisekundach od startu biegu.
    pub at_ms: u64,
    /// Zdarzenie neutralne wobec vendora.
    pub event: &'a AgentEvent,
    /// To, czego zdarzenie nie niesie, a kuracja potrzebuje. `None` dla zdarzeń, które
    /// z narzędziem nie mają nic wspólnego.
    pub tool: Option<&'a Tool>,
}

/// Maszyna stanu pięciu reguł zwijania [T2 §7.3].
///
/// Zwraca wiersze, które **właśnie się domknęły**, a nie wiersz na zdarzenie: grupa sklejania
/// może jeszcze urosnąć, więc dopóki żyje, nie ma czego wysyłać. Otwartą grupę zamyka
/// [`Curator::flush`] — woła je koniec strumienia i tik pompy z T-07.
#[derive(Debug, Default)]
pub struct Curator {
    /// Wiersze grupy, która jeszcze może urosnąć.
    open: Vec<Line>,
    /// Stały slot na dole ekranu.
    status: Option<Status>,
}

impl Curator {
    /// Świeży kurator, przed pierwszym zdarzeniem.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Wpuszcza jedno zdarzenie i oddaje wiersze, które przez nie się domknęły.
    ///
    /// Pusty wektor jest **normalną odpowiedzią**: tak wygląda myślenie, `init`, hak sesji
    /// i każde zdarzenie, które tylko dokłada się do otwartej grupy.
    pub fn observe(&mut self, seen: Seen<'_>) -> Vec<Line> {
        // SZKIELET (2026-08-16): tabela zdarzenie→wiersz (`ARCHITECTURE.md` §6) i pięć reguł
        // zwijania są całą treścią AC-1..AC-4, więc tutaj ich nie ma. Zdarzenie jest czytane
        // i porzucane, historia zostaje pusta — każda asercja o DŁUGOŚCI historii pada wtedy
        // na braku zachowania, a nie na braku typu.
        tracing::debug!(
            agent = seen.agent,
            at_ms = seen.at_ms,
            "SZKIELET: an event reached the curator and was dropped"
        );
        self.open.clear();
        Vec::new()
    }

    /// Zamyka otwartą grupę sklejania i oddaje jej wiersz.
    ///
    /// Bez tego ostatnia grupa biegu nie wyszłaby nigdy, a użytkownik zobaczyłby o jeden
    /// wiersz mniej niż się wydarzyło — najgorszy możliwy rodzaj zgubienia, bo cichy.
    pub fn flush(&mut self) -> Vec<Line> {
        std::mem::take(&mut self.open)
    }

    /// Co stoi w slocie na dole ekranu. `None` znaczy „nic się teraz nie dzieje".
    #[must_use]
    pub fn status(&self) -> Option<Status> {
        self.status
    }
}
