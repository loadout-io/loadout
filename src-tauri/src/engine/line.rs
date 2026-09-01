//! Zdarzenie → wiersz historii. **Tutaj powstaje „czysty terminal", nie w CSS**
//! (niezmiennik 15, `docs/ARCHITECTURE.md` §6).
//!
//! Rodzaje wiersza (czternaście z [T2 §7.2] plus `stepState`) i pięć reguł zwijania [T2 §7.3]
//! mieszkają w tym pliku i nigdzie indziej. Który wiersz w ogóle istnieje, co mówi i czy jest
//! zwinięty, rozstrzyga [`Curator`]. Cicha wersja złamania nie wygląda jak zły wiersz — wygląda jak
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
//! # Co ten plik rozstrzyga, a czego nie
//!
//! Rozstrzyga: **czy wiersz w ogóle istnieje** (`system/init` nie istnieje),
//! **co mówi** (jedno zdanie po angielsku, bez żargonu — niezmiennik 14) i **czy jest
//! zwinięty** (reguły 1–3). Nie rozstrzyga: jak wiersz wygląda, jaką ma wysokość i o której
//! lokalnej godzinie wraca limit — to jest formatowanie i mieszka w widoku (T-08).
//!
//! Granica biegnie dokładnie tędy, bo tylko tak „czysty widok" nie da się zepsuć arkuszem
//! stylów. Wiersz, którego tu nie ma, nie pojawi się nigdzie; wiersz, który tu jest, pojawi
//! się zawsze.

use std::collections::HashMap;
use std::fmt::Write as _;

use serde::Serialize;

use super::drivers::{AgentEvent, FinishReason, Outcome, is_unknown_price_notice};

/// Rodzaj wiersza. Czternaście z [T2 §7.2] plus [`LineKind::StepState`].
///
/// **Dwa z nich nie są wpisem w historii** i nie stają się nim przez to, że kurator je
/// wypuszcza: [`LineKind::Thinking`] rysuje stały slot na dole ekranu, a [`LineKind::StepState`]
/// przestawia blok paska loadoutu i chip na kafelku agenta. Dokąd wiersz idzie, rozstrzyga
/// **jedno** miejsce — rejestr rodzajów po stronie okna (`src/sections/run/feed/kinds.ts`,
/// pole `route`) — i to jest cała treść reguły 5 w wersji, którą da się wykonać: „nigdy nie
/// powstaje" znaczyłoby, że dół ekranu jest martwy, a sześć z siedmiu stanów kroku
/// nieosiągalnych (`docs/ARCHITECTURE.md` §5, §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Nagłówek całego biegu.
    Run,
    /// Przerwa sekcyjna z etykietą; zaczepienie paska planu.
    Step,
    /// Krok zmienił stan. **Nigdy w historii** — przestawia pasek loadoutu w miejscu.
    StepState,
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
    /// Proza agenta. Jedyna proza AGENTA w widoku.
    Note,
    /// Proza człowieka — jego tura wpisana w wiersz wejścia. Powód przy [`Line::Told`].
    Told,
    /// Lider proponuje bieg: proza plus gotowa komenda. Powód przy [`Line::Suggested`].
    Suggested,
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
///
/// # Kształt na drucie: `{"kind":"read","agent":"builder","detailId":1,…}`
///
/// `rename_all_fields` jest tu **jedyną** rzeczą, która stoi między nami a błędem z meetnotes:
/// bez niego `detail_id`, `duration_ms`, `cost_usd` i `resets_at` jadą na front pod nazwami,
/// których on nie zna, widok wywraca się na `undefined`, a pierwsze sześć poprawek idzie
/// w złą warstwę, bo objaw jest w widoku, a przyczyna w derive [FOUNDATIONS §3].
/// Wariant jest **wewnętrznym** znacznikiem `kind`, żeby front dostał płaski obiekt zamiast
/// `{"Read":{…}}` — jeden rodzaj, jedno pole, żadnego rozpakowywania po stronie widoku.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
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
    /// Krok przeszedł w nowy stan: `{"kind":"stepState","agent":"Build","stepId":"build",
    /// "state":"running"}`.
    ///
    /// 2026-08-18 — OSOBNY WARIANT, NIE POLE DOŁOŻONE DO [`Line::Step`], i wymusza to lustro po
    /// stronie okna (`src/ipc/types.ts`, `parseLine`): ono porównuje zestaw kluczy **co do
    /// jednego**, więc pole dopisane do istniejącego rodzaju kazałoby froncie PORZUCAĆ każdy
    /// wiersz `step` do chwili, w której obie strony granicy zmienią się w tym samym commicie.
    /// Nowy rodzaj jest addytywny w obie strony: starszy front porzuca go w ciszy, starszy Rust
    /// go po prostu nie wysyła.
    ///
    /// Do tego dnia na drucie NIE BYŁO NOŚNIKA tego faktu — [`Line::Step`] niesie sam tekst —
    /// więc pasek loadoutu stał na obrysach przez cały bieg, a kafelek agenta, który właśnie
    /// edytował pliki, mówił „waiting". Sześć z siedmiu stanów z `docs/ARCHITECTURE.md` §5 było
    /// nieosiągalnych.
    StepState {
        /// Nazwa kroku — **ten sam podpis**, którym ten krok mówi w każdym innym wierszu, żeby
        /// szyna agentów nie dostała dwóch nazw dla jednego kafelka (niezmiennik 13).
        agent: String,
        /// Klucz kroku **z pliku workflow** (`node_key`), bo to po nim okno rozpoznaje swój
        /// kafelek: plan paska powstaje z pliku, zanim Rust powie pierwsze słowo
        /// (`src/state/run.ts`, `withStepStates` porównuje `step.id === line.stepId`).
        /// Świeży uuid biegu byłby tu identyfikatorem, którego okno nigdy nie widziało.
        step_id: String,
        /// Jedno z siedmiu: `pending`, `ready`, `running`, `succeeded`, `failed`, `cancelled`,
        /// `skipped`.
        ///
        /// Napis, a nie [`super::step::StepState`], bo lustro po stronie okna czyta to pole jako
        /// zwykły tekst i **samo** odrzuca wartość spoza siódemki (`src/state/run.ts`,
        /// `STEP_STATES`) — enum na drucie zmuszałby je do znajomości naszego `serde`. Wartość
        /// składa `StepState::name`, więc te siedem słów stoi w drzewie raz.
        state: String,
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
        /// Tekst wiersza, gotowy na ekran. W biegu jest NAGŁÓWKIEM, nie całą prozą.
        text: String,
        /// Cała proza, wiersz po wierszu — pusta, kiedy wiersz niesie ją w całości.
        ///
        /// # Po co to pole istnieje
        ///
        /// Reguła 1 mówi „treść siedzi ZA wierszem, nigdy w nim", i to pole jest jedyną drogą,
        /// którą proza może za ten wiersz trafić. Do 2026-08-30 jej nie było, więc odpowiedź
        /// agenta nie miała gdzie pójść i szła do `text` — zmierzone na zrzucie właściciela
        /// z biegu `20260830-191440`: **78 wierszy w jednym wierszu strumienia**, zasłaniające
        /// cały bieg. Skarga brzmiała „nie podoba mi się ta ściana tekstu".
        ///
        /// # Dlaczego CAŁA proza, a nie „reszta bez nagłówka"
        ///
        /// Bo to jest ta sama rzecz do przeczytania, a nie dwie. Ciało zaczynające się od
        /// drugiego zdania czyta się jak tekst, któremu ucięto początek — i zmusza człowieka
        /// do składania jednej wypowiedzi z dwóch miejsc na ekranie.
        ///
        /// # Dlaczego w linii, a nie za `detail_id`
        ///
        /// Bo `detail_id` **nie ma dokąd prowadzić**: kurator mintuje te numery, a nic nie
        /// zapisuje odwzorowania numer→treść, więc żaden czytelnik nie miałby czego przeczytać.
        /// To pole idzie tą samą drogą, co `detail` przy [`Line::Ran`] — jedyną, która w tym
        /// produkcie naprawdę dowozi treść za wiersz.
        body: Vec<String>,
    },
    /// Co powiedział CZŁOWIEK — jedyny wiersz historii, którego nie napisał agent.
    ///
    /// # Po co ten wariant istnieje
    ///
    /// Zgłoszenie właściciela 2026-08-19: „tak samo nadal jak coś piszę w terminal np siema, to
    /// agent nie odpisuje i to się wgl nie wysyła", a chwilę potem rozstrzygające zdanie:
    /// „a może odpisuje on, ale na pewno nie widać moich wiadomości". I to była dokładna
    /// diagnoza. Droga do żywej sesji już działała (`commands::run::say_to_agent_inner`,
    /// `engine::drivers::Voice`), ale **tura człowieka nie miała nośnika na drucie**: [`Note`]
    /// jest opisany jako „jedyna proza w widoku" i należy do agenta, więc zdanie wpisane
    /// w wiersz wejścia znikało bez śladu. Człowiek widział strumień, w którym agent
    /// odpowiada na pytanie, którego nie widać — czyli wiersz wejścia wyglądał na martwy
    /// niezależnie od tego, czy działał.
    ///
    /// # Dlaczego to jedzie drutem, a nie dopisuje się w oknie
    ///
    /// Bo tura człowieka JEST zdarzeniem tego biegu, a nie stanem widoku. Wiersz dopisany
    /// lokalnie ginie przy przeładowaniu okna i nie ma go w `run.json` — a wtedy plik nie
    /// potrafi wyjaśnić, dlaczego agent w połowie kroku zrobił coś, o co nikt go w pliku
    /// workflow nie prosił (niezmiennik 4).
    ///
    /// Zestaw pól jest **taki sam** jak w [`Note`] i to jest celowe: lustro po stronie okna
    /// porównuje klucze co do jednego, więc nowy rodzaj o znanym kształcie jest addytywny
    /// w obie strony — starszy front porzuca go w ciszy, starszy Rust go nie wysyła.
    Told {
        /// Do KOGO to poszło — nazwa kroku, ta sama, którą niesie każdy inny wiersz tego kroku.
        ///
        /// Nie „człowiek": pole `agent` odpowiada w każdym wierszu na pytanie „czyj to kafelek",
        /// a ta linia należy do rozmowy z tym właśnie krokiem. Że mówi człowiek, niesie rodzaj.
        agent: String,
        /// Zdanie człowieka, słowo w słowo — bez skracania i bez streszczania.
        text: String,
    },
    /// Lider proponuje bieg: `/run easy Make the flaky login test pass`.
    ///
    /// # Po co ten wariant istnieje
    ///
    /// Rozstrzygnięcie właściciela 2026-08-20, wariant A: lider **podaje gotową komendę**, a nie
    /// startuje bieg sam. Wartość jest w jednym kliknięciu zamiast przepisywania: lider patrzy na
    /// projekt, więc umie powiedzieć „to jest robota dla Easy, z takim zadaniem", a człowiek nie
    /// musi pamiętać nazw plików workflow ani przepisywać zdania. Uruchomienie zostaje przy
    /// człowieku — `commands::chat` nie zna `RunDeps`, nie importuje `run` i nie widzi bazy
    /// biegów, więc ta propozycja jest **tekstem**.
    ///
    /// # Dlaczego to jest rodzaj wiersza, a nie robota okna
    ///
    /// Bo wiersz strumienia jest decyzją podjętą tutaj, w mapowaniu zdarzenie → linia
    /// (niezmiennik 15, decyzja D4). Okno, które samo szuka `/run` w prozie agenta i dorysowuje
    /// przycisk, jest kuracją w CSS-ie: da się ją zepsuć arkuszem stylów, nie da się jej sprawdzić
    /// bez przeglądarki i nie ma jej w `run.json`.
    ///
    /// Cicha porażka, przed którą stoi ten wariant: propozycja rozpoznana u KAŻDEGO agenta.
    /// Krok w środku biegu, który napisze w prozie `/run …`, dostałby wtedy przycisk startujący
    /// DRUGI bieg — a silnik prowadzi dziś jeden (`AppState::begin_run` podmienia uchwyt, więc
    /// pierwszy zostałby osierocony, niezmiennik 6). Dlatego rozpoznanie jest własnością
    /// rozmowy: robi je [`suggested`], którego woła wyłącznie `commands::chat::read_along`,
    /// a nie [`Curator::observe`], przez które idą wiersze biegu.
    ///
    /// Zestaw pól jest addytywny w obie strony — starszy front porzuca ten rodzaj w ciszy,
    /// starszy Rust go po prostu nie wysyła — a nowy rodzaj, nie pole dołożone do [`Note`],
    /// bo lustro po stronie okna porównuje klucze **co do jednego** (`src/ipc/types.ts`).
    Suggested {
        /// Kto to zaproponował. Ten sam podpis, którym ten agent mówi w każdym innym wierszu.
        agent: String,
        /// Tekst wiersza: **cała** proza lidera, nie sama komenda.
        ///
        /// Człowiek ma przeczytać, DLACZEGO lider to proponuje, zanim kliknie. Wiersz niosący
        /// samą komendę jest formularzem z jednym polem, a nie zdaniem w rozmowie.
        text: String,
        /// Czy okno ma tę komendę **uruchomić samo**, bez czekania na kliknięcie.
        ///
        /// # Rozstrzygnięcie właściciela 2026-08-30
        ///
        /// Na pytanie „po rozmowie z liderem — klikasz przycisk, czy bieg rusza sam?" odpowiedź
        /// brzmiała **„rusza samo"**. Cofa to część rozstrzygnięcia z 2026-08-19 („tylko komendy
        /// determinują akcje workflow") — tę i tylko tę, która mówiła, KTO może zacząć bieg.
        ///
        /// # Co z tamtego rozstrzygnięcia zostaje
        ///
        /// Proza bez ukośnika dalej nie uruchamia niczego. `true` stoi tu **wyłącznie** wtedy, gdy
        /// lider wywołał czasownik `start_workflow` — czyli podjął jawną decyzję, a nie napisał
        /// zdania, które przypadkiem wygląda jak komenda. Wiersz rozpoznany z prozy
        /// ([`suggested`]) niesie `false` i dostaje przycisk, tak jak dotąd. To jest dokładnie ta
        /// awaria, którą właściciel odrzucił w sierpniu („jak piszę bez komendy… to się na nowo
        /// całe workflow odpala"), i ona nie wraca.
        ///
        /// # Dlaczego wiersz, a nie osobny kanał do okna
        ///
        /// Bo start ma być **widoczny**. Kanał obok strumienia byłby biegiem, który zaczyna się
        /// bez śladu na ekranie — a przy „rusza samo" jedyną ochroną człowieka jest to, że widzi,
        /// co ruszyło, w tej samej sekundzie, w której to ruszyło.
        auto: bool,
        /// Komenda, znak w znak taka, jak lider ją napisał.
        ///
        /// OSOBNE POLE, a nie wycinek z `text`, i to jest cały powód, dla którego ten wariant
        /// ma trzy pola zamiast dwóch: `text` jest sklejony do jednej linii (reguła 1,
        /// [`one_line`]), więc granica między komendą a powodem, dla którego lider ją podaje,
        /// jest po tej stronie granicy nieodtwarzalna. Okno, które składa komendę z powrotem
        /// z prozy, jest tym samym oknem, które samo szuka `/run` — tylko o jeden krok dalej.
        command: String,
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
        /// Świeże wejście tej tury — **przepisane z drutu**, nie policzone.
        ///
        /// 2026-08-24 (T-97) — TRZY POLA SĄ NOWE i powstały z kroku, który nie miał na ekranie
        /// ani jednej cyfry. `Outcome::tokens` niesie te liczby od T-06, a wiersz zamykający je
        /// wyrzucał: u Claude'a nie było tego widać, bo obok stał koszt, ale Codex kwoty nie
        /// podaje (`cost_usd` zostaje `None` i ma zostać), więc jego kroki pokazywały pustkę
        /// i liczyły się jako zero w każdym podsumowaniu.
        ///
        /// Zero, a nie `Option`, i to jest wybór za źródłem: [`Outcome::tokens`] jest
        /// [`super::drivers::Tokens`] z trzema `u64`, więc `Option` tutaj wymyślałby rozróżnienie,
        /// którego drut nie niesie. Pustkę na ekranie rozstrzyga suma: bieg, w którym wszystkie
        /// trzy są zerem, nie ma czego pokazać i nie pokazuje nic.
        input_tokens: u64,
        /// Wyjście modelu w tej turze — przepisane, nie policzone.
        output_tokens: u64,
        /// Wejście przeczytane z cache'u. To ta liczba mówi, czy izolacja kontekstu działa
        /// [T1 §3.3, korekta 4].
        cached_tokens: u64,
        /// Jak się skończyło — **osobnym polem, nie do wyczytania z `text`**.
        ///
        /// 2026-08-22 — POLE JEST NOWE i powstało z wady widocznej na zrzucie właściciela:
        /// kafelek agenta pokazywał `Done · 26 turns · 6m 27s · $2.33` i pod spodem `working`.
        /// Szyna zna stan kroku wyłącznie z linii `StepState`, a kiedy ten stan do niej nie
        /// dojedzie, jedyne, co jej zostawało, to zgadywać — i zgadywała „pracuje", nad
        /// agentem, który skończył kilkanaście minut wcześniej.
        ///
        /// Wyczytanie tego z `text` byłoby parsowaniem prozy po stronie okna: `Done`,
        /// `Didn't work` i `Stopped` są zdaniami dla człowieka i wolno je zmienić, nie pytając
        /// nikogo o zgodę. Enum z drutu na ekran nie trafia (niezmiennik 14) — trafia decyzja,
        /// którą on niesie.
        ended: Ended,
    },
}

/// Czym skończyła się praca agenta — trzy stany, te same trzy, które rozróżnia [`done_line`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ended {
    /// Skończył i wyszło.
    Well,
    /// Skończył i nie wyszło.
    Badly,
    /// Zatrzymany przez człowieka. Anulowanie jest wartością, nie błędem (niezmiennik 7).
    Stopped,
}

impl Line {
    /// Rodzaj wiersza.
    #[must_use]
    pub fn kind(&self) -> LineKind {
        match self {
            Self::Run { .. } => LineKind::Run,
            Self::Step { .. } => LineKind::Step,
            Self::StepState { .. } => LineKind::StepState,
            Self::Agent { .. } => LineKind::Agent,
            Self::Thinking { .. } => LineKind::Thinking,
            Self::Read { .. } => LineKind::Read,
            Self::Search { .. } => LineKind::Search,
            Self::Edit { .. } => LineKind::Edit,
            Self::Ran { .. } => LineKind::Ran,
            Self::Note { .. } => LineKind::Note,
            Self::Told { .. } => LineKind::Told,
            Self::Suggested { .. } => LineKind::Suggested,
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
            | Self::StepState { agent, .. }
            | Self::Agent { agent, .. }
            | Self::Thinking { agent }
            | Self::Read { agent, .. }
            | Self::Search { agent, .. }
            | Self::Edit { agent, .. }
            | Self::Ran { agent, .. }
            | Self::Note { agent, .. }
            | Self::Told { agent, .. }
            | Self::Suggested { agent, .. }
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
            // Oba rodzaje spoza historii mówią stanem, nie zdaniem: „Thinking…" rysuje stały
            // slot, a stan kroku przestawia blok paska. Tekst dorobiony tutaj byłby drugim
            // brzmieniem tego samego faktu, i to tym, którego nikt nie tłumaczy.
            Self::Thinking { .. } | Self::StepState { .. } => "",
            Self::Run { text, .. }
            | Self::Step { text, .. }
            | Self::Agent { text, .. }
            | Self::Read { text, .. }
            | Self::Search { text, .. }
            | Self::Edit { text, .. }
            | Self::Ran { text, .. }
            | Self::Note { text, .. }
            | Self::Told { text, .. }
            | Self::Suggested { text, .. }
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
        match self {
            // Reguła 3 stoi PRZED tabelą reguły 2 i tylko dlatego działa: `ran` jest mechaniką,
            // więc reguła 2 zwija go zawsze. Porażka jest jedynym miejscem, w którym ściana
            // tekstu jest pożądana — a człowiek, który musi kliknąć, żeby dowiedzieć się,
            // dlaczego build się wywalił, nie kliknie.
            Self::Ran { ok, .. } => !ok,
            // Reguła 2: proza, pytania, błędy i struktura są widoczne; mechanika nie jest.
            // `thinking` jest po tej stronie, bo stały slot na dole ekranu jest widoczny —
            // ale to jedyny rodzaj, którego kurator nigdy nie dokłada do historii (reguła 5),
            // więc ta odpowiedź nie dotyczy żadnego wiersza, który ktokolwiek przewinie.
            Self::Read { .. } | Self::Search { .. } | Self::Edit { .. } | Self::Memory { .. } => {
                false
            }
            Self::Run { .. }
            | Self::Step { .. }
            | Self::StepState { .. }
            | Self::Agent { .. }
            | Self::Thinking { .. }
            | Self::Note { .. }
            // Zdanie człowieka jest prozą i jest widoczne od razu, jak proza agenta. Zwinięte
            // byłoby jedynym wierszem w historii, który człowiek musi rozwinąć, żeby przeczytać
            // to, co sam napisał.
            | Self::Told { .. }
            // Propozycja zwinięta jest propozycją, której nie widać — a wiersz, który trzeba
            // najpierw rozwinąć, żeby zobaczyć w nim przycisk, jest przyciskiem schowanym.
            | Self::Suggested { .. }
            | Self::Asked { .. }
            | Self::Handoff { .. }
            | Self::Problem { .. }
            | Self::Done { .. } => true,
        }
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

// ── PROPOZYCJA BIEGU: ROZPOZNANIE, KTÓRE NALEŻY DO ROZMOWY ─────────────────────────────────

/// Komenda, którą zaczyna się bieg — **to samo słowo**, które przyjmuje wiersz wejścia
/// (`src/sections/run/run-command.ts`) i które makieta obiecuje pod polem („`/plan · /run ·
/// or just say what you want`").
///
/// Napis, nie wyrażenie regularne: wzorzec pisany osobno łapie przy okazji `/runner` i `/runs`,
/// a to są inne komendy, które tylko zaczynają się tymi samymi literami.
const RUN: &str = "/run";

/// Wiersz rozmowy, w którym proza lidera jest propozycją biegu — albo ten sam wiersz.
///
/// # Dlaczego to jest funkcja obok [`Curator`], a nie gałąź w środku
///
/// Bo [`Curator`] jest kuratorem **biegu** i jego wierszy nie wolno tym dotknąć. Propozycja
/// rozpoznana w kuratorze byłaby rozpoznana u każdego agenta, a krok w środku biegu, który
/// napisze w prozie `/run …`, dostałby wtedy przycisk startujący DRUGI bieg — powód w całości
/// stoi przy [`Line::Suggested`]. Tę funkcję woła dokładnie jedno miejsce
/// (`commands::chat::read_along`), i to jest cała treść zdania „rozpoznanie jest własnością
/// rozmowy, nie kuratora biegu".
///
/// # Dlaczego zdarzenie, a nie sam wiersz
///
/// Bo [`Line::Note`] jest już sklejony do jednej linii (reguła 1, [`one_line`]), a granica
/// między komendą i powodem, dla którego lider ją podaje, biegnie po **znaku nowej linii**.
/// Wiersz jest tu więc tym, co ma się zmienić, a zdarzenie — jedynym miejscem, w którym
/// surowa proza jeszcze istnieje. Zdarzenie, które prozą nie jest, nie może być propozycją
/// i oddaje wiersz bez zmiany.
///
/// Oddaje wiersz, nie [`Option`], żeby wołający nie musiał pisać gałęzi: jedno wywołanie,
/// jedna wartość, żadnego `unwrap_or` po drodze (niezmiennik 23 — polityka ma jedno miejsce,
/// a wołający nie ma prawa mieć własnego zdania na jej temat).
#[must_use]
pub fn suggested(line: Line, event: &AgentEvent) -> Line {
    // Zdarzenie, które prozą nie jest, nie może być propozycją: `/run` w wyjściu komendy albo
    // w czytanym pliku jest tekstem, na który agent PATRZY, a nie zdaniem, które mówi.
    let AgentEvent::Said { text } = event else {
        return line;
    };
    let Some(command) = command_in(text) else {
        return line;
    };
    match line {
        /* WYŁĄCZNIE WIERSZ PROZY, i to nie jest ostrożność na zapas: `Curator::observe` oddaje
         * przy jednym zdarzeniu `Said` także wiersz grupy sklejania, którą to zdanie zamknęło
         * (`Read 3 files` sprzed zdania) — a wołający pyta tą samą funkcją o KAŻDY wiersz
         * z tego wektora (`commands::chat::read_along`). Rozpoznanie patrzące na samo zdarzenie
         * zamieniłoby więc w propozycję cudzy wiersz, i to ten, który o niczym nie mówi. */
        /* TYLKO PROZA BEZ CIAŁA, i strażnik jest tu treścią, nie ostrożnością.
         *
         * Ta funkcja woła się wyłącznie z rozmowy (`commands::chat::read_along`), a kurator
         * rozmowy prozy nie dzieli — jej `body` jest z definicji puste, więc strażnik dziś nie
         * odrzuca niczego. Stoi dlatego, że [`Line::Suggested`] **nie ma pola na ciało**: proza
         * z ciałem zamieniona w propozycję zgubiłaby je bez śladu. Z tym strażnikiem taki wiersz
         * spada do `other => other` i zostaje prozą — z całą swoją treścią i bez przycisku.
         *
         * Bez przycisku, bo to jest właściwy wybór przy tym rozjeździe: propozycja, której nikt
         * nie zobaczy, kosztuje jedno kliknięcie mniej, a zgubiona odpowiedź agenta kosztuje
         * całą jego turę. */
        Line::Note { agent, text, body } if body.is_empty() => Line::Suggested {
            agent,
            /* PROZA NIGDY NIE URUCHAMIA SAMA. Ten wiersz powstał z rozpoznania zdania, a nie
             * z decyzji lidera — więc dostaje przycisk, dokładnie jak dotąd. */
            auto: false,
            // Tekst zostaje TAKI, JAKI ZŁOŻYŁ GO KURATOR — cała proza w jednej linii (reguła 1).
            // Sama komenda jedzie osobnym polem, bo człowiek ma przeczytać, DLACZEGO lider to
            // proponuje, zanim kliknie; wiersz z samą komendą jest formularzem z jednym polem.
            text,
            command,
        },
        other => other,
    }
}

/// Komenda z tej prozy — pierwsza linia, która **jest** poleceniem — albo `None`.
///
/// LINIA, NIE WYSTĄPIENIE NAPISU, i na tej różnicy stoi całe to rozpoznanie. „Zrobiłbym to
/// przez /run easy" jest zdaniem O poleceniu, a nie poleceniem: przycisk pod opisem startuje
/// bieg, o który nikt nie prosił, a lider opisujący drogę do celu robi to w co drugim zdaniu.
///
/// Linię przycinamy z obu stron i to jest świadome wobec „znak w znak": komenda ma pojechać
/// dalej dokładnie w tej postaci, w której da się ją uruchomić, a wcięcie i biała spacja na
/// końcu nie należą do niej — po tej samej stronie granicy przycina je `startFromLine`.
fn command_in(prose: &str) -> Option<String> {
    prose
        .lines()
        .map(str::trim)
        .find(|line| names_a_workflow(line))
        .map(str::to_owned)
}

/// Czy ta linia jest poleceniem uruchomienia, które **nazywa** workflow.
///
/// Nazwa jest wymagana, bo przycisk pod tym wierszem ma powiedzieć, CO uruchomi — samo „Run"
/// nie mówi, a to jest ta jedna czynność, po której zaczynają pracować agenci i zaczynają się
/// pieniądze. Zadania nie wymagamy: `/run easy` jest kompletnym poleceniem, w którym każdy krok
/// robi to, co stoi w pliku workflow (`readRunLine` oddaje wtedy `task: null`).
fn names_a_workflow(line: &str) -> bool {
    let Some(rest) = line.strip_prefix(RUN) else {
        return false;
    };
    // Biała spacja po komendzie, bo bez niej `/runner easy` czytałoby się jako `/run` z nazwą
    // `ner easy` — czyli propozycja z nazwy, której nikt nie napisał.
    rest.starts_with(char::is_whitespace) && rest.split_whitespace().next().is_some()
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
    /// Grupa sklejania, która jeszcze może urosnąć. Najwyżej jedna: reguła 4 skleja wiersze
    /// **sąsiednie**, a sąsiadem jest dokładnie ten ostatni.
    open: Option<Group>,
    /// Komendy, które czekają na swój wynik, w kolejności startu.
    ///
    /// Wiersz `ran` nie może powstać na starcie: dopiero wynik mówi, czy się udało i co
    /// wypisał, a bez tego reguła 3 nie ma czego rozwinąć. Wektor, nie pojedyncze pole, bo
    /// model wysyła kilka bloków `tool_use` w jednej wiadomości i wtedy dwie komendy biegną
    /// naraz — przy jednym polu druga nadpisywałaby pierwszą i jeden wiersz znikałby cicho.
    pending: Vec<Pending>,
    /// Stały slot na dole ekranu.
    status: Option<Status>,
    /// Brak ceny czeka na końcowy wiersz TEGO SAMEGO kroku. Mapa, nie jedno pole, bo kurator
    /// może dostać przeplecione zdarzenia dwóch równoległych kroków o tej samej nazwie.
    unknown_prices: HashMap<String, String>,
    /// Skąd biorą się `detail_id`. Rośnie monotonicznie w obrębie jednego biegu.
    minted: u64,
    /// Czy proza tego kuratora zachowuje przełamania wierszy.
    ///
    /// `false` w biegu (reguła 1: jedna linia na zdanie), `true` w rozmowie. Jedno pole i jedno
    /// ramię, bo to jest JEDNA różnica między dwoma produktami stojącymi w tym samym widoku:
    /// rozmowę się CZYTA, strumień pracy się PRZEGLĄDA. Domyślne `false` jest tu treścią,
    /// nie oszczędnością — `Curator::default()` zostaje kuratorem biegu, co do bajtu.
    keeps_line_breaks: bool,
}

/// Grupa sklejania: sąsiednie czynności tego samego rodzaju, tego samego agenta, w oknie 2 s.
#[derive(Debug)]
struct Group {
    /// Czyja jest — klucz grupy zawiera agenta, więc dwa agenty czytające w tej samej
    /// sekundzie to dwa wiersze, nie jeden.
    agent: String,
    /// Rodzaj wiersza, który z niej powstanie. Tylko [`LineKind::Read`], [`LineKind::Search`]
    /// i [`LineKind::Edit`] — reszta niesie treść, której nie da się zsumować w licznik.
    kind: LineKind,
    /// Chwila **pierwszego** wiersza grupy. Okno biegnie od niej i to jest cała reguła 4:
    /// liczone od ostatniego, okno nigdy się nie zamyka, a agent czytający plik na sekundę
    /// przez pięć minut daje jeden puchnący wiersz `Read 300 files`.
    first_at_ms: u64,
    /// Ile czynności się skleiło.
    count: u32,
    /// Czego dotyczyły, w kolejności.
    targets: Vec<String>,
    /// Identyfikatory wywołań, których wyniki jeszcze do tej grupy należą.
    ids: Vec<String>,
    /// Ile trafień zgłosiły wyniki (tylko [`LineKind::Search`]).
    matches: u32,
    /// Czy jakikolwiek wynik już doszedł — bez tego „0 matches" znaczyłoby „nie wiemy".
    answered: bool,
    /// Klucz do treści, której wiersz nie niesie.
    detail_id: Option<u64>,
}

/// Komenda, która ruszyła i czeka na swój wynik.
#[derive(Debug)]
struct Pending {
    /// Identyfikator wywołania — po nim wynik trafia do swojego wiersza.
    id: String,
    /// Kto ją uruchomił.
    agent: String,
    /// Co uruchomił; z tego powstaje tekst wiersza.
    subject: String,
}

/// Ile milisekund grupa przyjmuje kolejne czynności, licząc od pierwszej (reguła 4).
const WINDOW_MS: u64 = 2_000;

/// Sufit podglądu wyjścia na granicy z widokiem [T2 §6.3, obrona 2]. 200 KB wyniku narzędzia
/// ma kosztować 2 KB w wiadomości do frontu; reszta zostaje na dysku i za kliknięciem.
const PREVIEW_LIMIT: usize = 2_048;

/// Ile ostatnich linii wyjścia pokazuje wiersz, który się nie udał (reguła 3).
const TAIL_LINES: usize = 20;

/// Sufit tekstu, który przepisujemy z drutu do wiersza. Ta sama obrona co [`PREVIEW_LIMIT`],
/// o rząd wielkości niżej: komenda bywa heredokiem na pięć tysięcy znaków, a wiersz ma być
/// wierszem (reguła 1).
const SUBJECT_LIMIT: usize = 120;

impl Curator {
    /// Świeży kurator, przed pierwszym zdarzeniem.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Kurator ROZMOWY: ten sam kurator, jedna różnica — proza zachowuje przełamania wierszy.
    ///
    /// # Dlaczego osobny konstruktor, a nie argument w [`Curator::new`]
    ///
    /// Bo `new()` woła w tym drzewie wielu i każdy z nich sądzi BIEG. Szew addytywny zostawia
    /// ich zachowanie co do bajtu i nie zamienia jednej różnicy w trzydzieści zmienionych
    /// plików — ten sam ruch i ten sam powód, co przy `AgentDriver::configured`.
    ///
    /// # Co ten tryb naprawia (2026-08-30)
    ///
    /// Zgłoszenie właściciela z 2026-08-23: „ten tekst niech też będzie jakoś fajnie i ładnie
    /// formatowany". Poprawka weszła wtedy w CSS (`feed/line.tsx`, `whitespace-pre-line`)
    /// i była poprawna — ale [`one_line`] kasowało przełamania TUTAJ, warstwę wcześniej, więc
    /// do arkusza stylów nie dojeżdżał ani jeden przełam do zachowania. Kryterium frontowe tego
    /// nie widziało, bo sądziło wiersz rodzaju `step`, a takiego agent nie pisze nigdy.
    #[must_use]
    pub fn talking() -> Self {
        Self {
            keeps_line_breaks: true,
            ..Self::default()
        }
    }

    /// Wpuszcza jedno zdarzenie i oddaje wiersze, które przez nie się domknęły.
    ///
    /// Pusty wektor jest **normalną odpowiedzią**: tak wygląda myślenie, `init`, hak sesji
    /// i każde zdarzenie, które tylko dokłada się do otwartej grupy.
    pub fn observe(&mut self, seen: Seen<'_>) -> Vec<Line> {
        self.observe_with_step_key(seen, None)
    }

    /// Wpuszcza zdarzenie z opcjonalnym, stabilnym kluczem kroku.
    ///
    /// Żywy bieg podaje indeks węzła, bo nazwa wyświetlana nie jest identyfikatorem i dwa
    /// równoległe kroki mogą ją współdzielić. Odczyt starych transkryptów nie zna indeksu,
    /// więc zachowuje dotychczasowe grupowanie po nazwie przez [`Self::observe`].
    pub fn observe_with_step_key(&mut self, seen: Seen<'_>, step_key: Option<&str>) -> Vec<Line> {
        let unknown_price_key = step_key.unwrap_or(seen.agent);
        match seen.event {
            // Reguła 5. Myślenie MUSI dojść — inaczej dół ekranu jest martwy, kiedy agent
            // pracuje — i nie ma prawa wejść do historii, bo wirtualizowana lista mierzy
            // każdy wiersz, także pusty.
            AgentEvent::Thinking => {
                self.status = Some(Status::Thinking);
                // NIC W STRUMIENIU, i to jest cytat, nie interpretacja: `docs/ARCHITECTURE.md`
                // linia 178 daje dla `thinking` i `thinking_tokens` wprost „*nic w strumieniu* —
                // stały slot na dole, nadpisywany", a §6 reguła 5 powtarza „nigdy nie wchodzi
                // do historii".
                //
                // 2026-08-18 — TU BYŁA PRÓBA ODWROTNA I ZOSTAŁA WYCOFANA. Rozumowanie za nią
                // było niegłupie („reguła mówi »nie do historii«, a nie »nie na ekran«, a rejestr
                // po stronie okna kieruje ten rodzaj na trasę `now`, więc do historii i tak nie
                // wejdzie") i front rzeczywiście nie dokłada go do historii
                // (`src/sections/run/feed/model.ts`, gałąź `route === 'now'` robi `continue`).
                // Przewróciła jednak CZTERY kryteria w dwóch plikach, z których jedno przepuszcza
                // przez PRAWDZIWĄ pompę złotą fiksturę szesnastu zdarzeń i wymaga dokładnie
                // trzech wierszy — a przede wszystkim kłóciła się z linią 178, której żadne
                // z tych kryteriów nie napisało samo.
                //
                // ZOSTAJE WIĘC LUKA I JEST ZGŁOSZONA, nie zaklajstrowana: slot „Thinking…" nie
                // ma dziś ŻADNEGO nośnika. Jedynym śladem myślenia jest [`Curator::status`],
                // którego w produkcji nikt nie czyta, więc dolna strefa ekranu jest martwa także
                // wtedy, gdy agent myśli minutami. Domknięcie wymaga jednej z dwóch rzeczy,
                // i obie są decyzją człowieka, nie tego pliku: albo osobnej drogi dla statusu
                // (emit poza strumieniem wierszy), albo zmiany linii 178 w architekturze.
                Vec::new()
            }
            // ZDARZENIA WIDZIANE I ŚWIADOMIE NIEBĘDĄCE WIERSZEM. Stoją w jednej gałęzi, bo
            // „co nie ma prawa dołożyć wiersza" jest tu listą, którą się czyta — z szesnastu
            // zdarzeń prawdziwego biegu trzynaście nie zostawia śladu i to jest cała teza
            // produktu, nie przeoczenie. Każde ma własny powód:
            //
            // - `Started` (`system/init`) to 9 929 bajtów i 42% strumienia [T7 §4.3]. Widać po
            //   nim dokładnie jedno: kropka agenta robi się aktywna (`ARCHITECTURE` §6).
            AgentEvent::Started { .. } => Vec::new(),
            // `FileEdit` przychodzi od sterownika Claude **razem** z `ToolEnd` tego samego
            // wywołania, a wiersz `edit` powstał już na `ToolStart` (to on niesie pełną ścieżkę).
            // Drugi wiersz z tego samego faktu podwajałby KAŻDĄ zmianę pliku w widoku.
            // Czytelnika ten wariant ma w T-06, w indeksie zmienionych plików.
            AgentEvent::FileEdit { path } => self.file_edit(seen, path),
            AgentEvent::Said { text } => {
                /* JEDYNE MIEJSCE, W KTÓRYM ROZMOWA RÓŻNI SIĘ OD BIEGU.
                 *
                 * Rozmowa zachowuje akapity i listy w wierszu, bo się ją CZYTA — to jest ta
                 * rzecz, po którą człowiek przyszedł, i schowanie jej za kliknięciem byłoby
                 * schowaniem odpowiedzi na jego własne pytanie.
                 *
                 * Bieg oddaje nagłówek, a prozę stawia ZA wierszem (reguła 1). Sześciu agentów
                 * piszących akapitami jest ścianą — i to nie jest przewidywanie, tylko pomiar:
                 * zrzut właściciela z biegu `20260830-191440`, jedna odpowiedź na 78 wierszy
                 * zasłaniająca komplet dziewięciu kroków. */
                let line = if self.keeps_line_breaks {
                    Line::Note {
                        agent: seen.agent.to_owned(),
                        text: paragraphs(text),
                        body: Vec::new(),
                    }
                } else {
                    let (text, body) = headline_and_body(text);
                    Line::Note {
                        agent: seen.agent.to_owned(),
                        text,
                        body,
                    }
                };
                self.close_then(line)
            }
            AgentEvent::ToolStart { id, label } => self.tool_start(seen, id, label),
            AgentEvent::ToolEnd { id, ok, summary } => self.tool_end(seen, id, *ok, summary),
            AgentEvent::RateLimit {
                status, resets_at, ..
            } => self.rate_limit(seen, status, *resets_at),
            AgentEvent::Notice { text } => {
                if is_unknown_price_notice(text) {
                    self.unknown_prices
                        .insert(unknown_price_key.to_owned(), text.to_owned());
                    return Vec::new();
                }
                let line = Line::Problem {
                    agent: seen.agent.to_owned(),
                    text: one_line(text),
                    resets_at: None,
                };
                self.close_then(line)
            }
            AgentEvent::Finished(outcome) => {
                let unknown_price = self.unknown_prices.remove(unknown_price_key);
                let line = done_line(seen.agent, outcome, unknown_price.as_deref());
                self.close_then(line)
            }
        }
    }

    /// Zamyka otwartą grupę sklejania i oddaje jej wiersz.
    ///
    /// Bez tego ostatnia grupa biegu nie wyszłaby nigdy, a użytkownik zobaczyłby o jeden
    /// wiersz mniej niż się wydarzyło — najgorszy możliwy rodzaj zgubienia, bo cichy.
    ///
    /// Domyka też komendy, których wynik nigdy nie przyszedł. Strumień urwany w połowie
    /// komendy to komenda, która **się nie udała** z punktu widzenia człowieka: proces wyszedł
    /// i nie powiedział, co zrobił [T1 §8.5]. Wiersz, który o tym mówi, jest gorszy od wiersza
    /// prawdziwego i lepszy od braku wiersza, bo brak wiersza jest niewidoczny.
    pub fn flush(&mut self) -> Vec<Line> {
        let mut out: Vec<Line> = std::mem::take(&mut self.pending)
            .into_iter()
            .map(|pending| Line::Ran {
                agent: pending.agent,
                text: ran_text(&pending.subject, false),
                ok: false,
                preview: String::new(),
                detail: Vec::new(),
                detail_id: None,
            })
            .collect();
        out.extend(self.close_group());
        out
    }

    /// Co stoi w slocie na dole ekranu. `None` znaczy „nic się teraz nie dzieje".
    #[must_use]
    pub fn status(&self) -> Option<Status> {
        self.status
    }

    /// Czynność ruszyła: albo dokłada się do grupy, albo otwiera własny wiersz.
    fn tool_start(&mut self, seen: Seen<'_>, id: &str, label: &str) -> Vec<Line> {
        // Bez faktów o narzędziu nie da się wybrać wariantu wiersza: samo `ToolStart` niesie
        // etykietę i `id`, a etykieta nie mówi, czy to było czytanie, czy komenda [T1 §8.2].
        // Wiersz zgadnięty z etykiety byłby wierszem zmyślonym, więc go nie ma.
        let Some(Tool::Started { action, target }) = seen.tool else {
            tracing::debug!(id, label, "a tool started without the facts a row needs");
            return Vec::new();
        };

        match action {
            Action::Read | Action::Search | Action::Edit => {
                self.coalesce(seen, kind_of(*action), id, target)
            }
            Action::Ran => {
                let closed = self.close_group();
                self.status = None;
                self.pending.push(Pending {
                    id: id.to_owned(),
                    agent: seen.agent.to_owned(),
                    subject: clamp(&one_line(target), SUBJECT_LIMIT),
                });
                closed
            }
            Action::Asked => {
                let line = Line::Asked {
                    agent: seen.agent.to_owned(),
                    text: one_line(target),
                    // Warianty odpowiedzi siedzą w `tool_use.input.options`, a szew
                    // `Tool::Started` niesie jedną wartość. Front rysuje przyciski z tego,
                    // co dostanie; pusta lista znaczy „odpowiedz własnymi słowami", nie
                    // „pytanie bez treści".
                    options: Vec::new(),
                };
                self.close_then(line)
            }
            Action::Agent => {
                let line = Line::Agent {
                    agent: seen.agent.to_owned(),
                    text: one_line(target),
                };
                self.close_then(line)
            }
        }
    }

    /// Zmiana pliku ogłoszona **bez** zapowiedzi czynności — czyli jedyny kanał, jaki ma Codex.
    ///
    /// # 2026-08-24 (T-97) — DLACZEGO TA GAŁĄŹ ISTNIEJE, SKORO OBOK STOI „ŚWIADOMIE NIEBĘDĄCE
    /// WIERSZEM"
    ///
    /// Bo to są dwa różne strumienie mówiące tym samym zdarzeniem dwie różne rzeczy, a odróżnia
    /// je **obecność faktu o czynności**, nie nazwa vendora (niezmiennik 23 — nazwa vendora
    /// w kuratorze byłaby trzecią kopią polityki).
    ///
    /// * **Claude** wypuszcza `FileEdit` obok `ToolEnd` tego samego wywołania, a wiersz `edit`
    ///   powstał już na `ToolStart`. `stream::decode` przypina [`Tool`] wyłącznie do
    ///   `ToolStart`/`ToolEnd`, więc tutaj `seen.tool` jest `None` i wiersza nie ma — dokładnie
    ///   jak przed tą zmianą, bo drugi wiersz podwajałby każdą zmianę pliku.
    /// * **Codex** nie ogłasza zmiany pliku jako czynności w ogóle: `item.completed` typu
    ///   `file_change` daje po jednym `FileEdit` na plik i **żadnego** `ToolStart`. Do tego dnia
    ///   kończyło się to tutaj ciszą, więc krok, który przepisał trzy pliki, wyglądał jak krok,
    ///   który tylko o nich opowiedział.
    ///
    /// Grupa jedzie po ŚCIEŻCE jako identyfikatorze, bo drugiej strony tej czynności nie ma —
    /// `tool_end` nigdy po nią nie przyjdzie, a dwie zmiany tego samego pliku w jednym oknie
    /// mają się zwinąć w jeden wiersz z licznikiem, tak samo jak u sąsiada.
    fn file_edit(&mut self, seen: Seen<'_>, path: &std::path::Path) -> Vec<Line> {
        let Some(Tool::Started {
            action: Action::Edit,
            target,
        }) = seen.tool
        else {
            return Vec::new();
        };
        self.coalesce(seen, LineKind::Edit, &path.display().to_string(), target)
    }

    /// Wynik czynności. **Nigdy nie tworzy własnego wiersza** [T2 §9.3] — domyka ten, który
    /// już stoi, albo dokłada do niego fakty.
    fn tool_end(&mut self, seen: Seen<'_>, id: &str, ok: bool, summary: &str) -> Vec<Line> {
        let output = match seen.tool {
            Some(Tool::Ended { output }) => output.as_str(),
            // Wynik bez pełnego wyjścia zdarza się u vendora, który go nie przysyła. Zostaje
            // jednolinijkowe podsumowanie ze zdarzenia — mniej, niż chcemy, ale nie kłamstwo.
            _ => summary,
        };

        if let Some(index) = self.pending.iter().position(|pending| pending.id == id) {
            let pending = self.pending.remove(index);
            self.status = None;
            return vec![Line::Ran {
                agent: pending.agent,
                text: ran_text(&pending.subject, ok),
                ok,
                preview: clamp(output, PREVIEW_LIMIT),
                // Reguła 3: ostatnie dwadzieścia linii i tylko przy porażce. Pierwsze
                // dwadzieścia linii wyjścia builda to zawsze banner, nigdy przyczyna.
                detail: if ok { Vec::new() } else { tail(output) },
                detail_id: (!output.is_empty()).then(|| self.mint()),
            }];
        }

        // Wynik czytania, szukania albo zmiany: wiersz grupy już istnieje i ma rosnąć dalej,
        // bo prawdziwy strumień przeplata `tool_use` z `tool_result` — grupa zamknięta na
        // pierwszym wyniku nigdy nie doczekałaby się drugiego pliku i `Read 3 files` nie
        // powstałoby ani razu.
        let mine = self
            .open
            .as_ref()
            .is_some_and(|group| group.ids.iter().any(|open| open == id));
        if mine {
            // Klucz bijemy PRZED pożyczeniem grupy i tylko dla szukania: wiersze `read`
            // i `edit` niosą całą swoją treść w `paths`, więc klucz do treści, której nie ma,
            // byłby kluczem do niczego (niezmiennik 21).
            let searched = self.open.as_ref().is_some_and(|group| {
                group.kind == LineKind::Search && group.detail_id.is_none() && !output.is_empty()
            });
            let minted = searched.then(|| self.mint());
            let matches = count_matches(output);

            if let Some(group) = self.open.as_mut() {
                group.answered = true;
                if group.kind == LineKind::Search {
                    group.matches += matches;
                    group.detail_id = group.detail_id.or(minted);
                    /* TEMAT DOKŁADANY Z LINII, KTÓRA GO ZNA (2026-08-24, T-97).
                     *
                     * Claude nazywa szukanie w chwili startu i ta gałąź go nie dotyczy — jego
                     * temat nigdy nie jest pusty, bo `action_of` bez niego nie oddaje faktu.
                     * Codex nazywa je **dopiero na końcu**: `item.started` typu `web_search`
                     * niesie `query: ""` (zmierzone na `codex-stream-live.jsonl`). Bez tych
                     * czterech linii jego wiersz brzmiałby „Searched" i nie mówił, czego —
                     * czyli byłby wierszem, po którym trzeba otworzyć surowy strumień.
                     *
                     * Uzupełniamy WYŁĄCZNIE puste miejsce: nadpisanie tematu, który już stoi,
                     * zamieniłoby wynik szukania w jego treść. */
                    if let Some(slot) = group
                        .ids
                        .iter()
                        .position(|open| open == id)
                        .and_then(|at| group.targets.get_mut(at))
                        && slot.is_empty()
                    {
                        *slot = one_line(output);
                    }
                }
            }
            return Vec::new();
        }

        tracing::debug!(
            id,
            ok,
            "a tool result arrived for a row that is already closed"
        );
        Vec::new()
    }

    /// Reguła 4 w całości: dołóż do grupy albo zamknij ją i otwórz następną.
    fn coalesce(&mut self, seen: Seen<'_>, kind: LineKind, id: &str, target: &str) -> Vec<Line> {
        let fits = self.open.as_ref().is_some_and(|group| {
            group.kind == kind
                && group.agent == seen.agent
                // Okno od PIERWSZEGO wiersza grupy, nie od ostatniego (przypadek b z AC-3).
                && seen.at_ms.saturating_sub(group.first_at_ms) < WINDOW_MS
        });

        let closed = if fits { Vec::new() } else { self.close_group() };
        let group = self.open.get_or_insert_with(|| Group {
            agent: seen.agent.to_owned(),
            kind,
            first_at_ms: seen.at_ms,
            count: 0,
            targets: Vec::new(),
            ids: Vec::new(),
            matches: 0,
            answered: false,
            detail_id: None,
        });
        group.count += 1;
        group.targets.push(target.to_owned());
        group.ids.push(id.to_owned());
        self.status = None;
        closed
    }

    /// `rate_limit_event` → wiersz albo cisza.
    ///
    /// `allowed` znaczy „vendor mówi, że wszystko gra". Wiersz na to jest bannerem, który
    /// krzyczy w każdym biegu — a po drugim takim nikt nie czyta już żadnego.
    ///
    /// Pola `pause_run` tu **nie ma** i to jest celowe: czyta je T-21 i nikt poza nim
    /// (niezmiennik 21). O tym, czy wiersz istnieje, rozstrzyga `status`.
    fn rate_limit(&mut self, seen: Seen<'_>, status: &str, resets_at: i64) -> Vec<Line> {
        // Pytamy rdzeń, nie własną stałą (niezmiennik 23). Druga kopia tej reguły stała tu
        // do 2026-08-19 i rozjechała się z bramą dokładnie na `allowed_warning`: bieg stawał,
        // a wiersz obok niego mówił „Hit the usage limit", choć niczego nie osiągnięto.
        if super::limits::is_allowed(status) {
            return Vec::new();
        }
        let line = Line::Problem {
            agent: seen.agent.to_owned(),
            // Godziny tu nie ma, i to nie jest niedopatrzenie: `resets_at` jedzie obok,
            // a lokalną godzinę renderuje widok [T7 §7.2]. Zdanie z godziną wpisaną w tekst
            // pokazywałoby czas maszyny, która akurat parsowała strumień.
            text: "Hit the usage limit — waiting for it to reset".to_owned(),
            resets_at: Some(resets_at),
        };
        self.close_then(line)
    }

    /// Domyka otwartą grupę i dokłada za nią gotowy wiersz — w kolejności, w jakiej się zdarzyły.
    fn close_then(&mut self, line: Line) -> Vec<Line> {
        let mut out = self.close_group();
        out.push(line);
        // Prawdziwy wiersz wylądował, więc slot na dole gaśnie: kółko kręcące się po tym, jak
        // agent już się odezwał, mówi, że bieg pracuje, kiedy on nie pracuje.
        self.status = None;
        out
    }

    /// Zamyka grupę, jeśli jakaś jest otwarta.
    fn close_group(&mut self) -> Vec<Line> {
        self.open.take().map(Group::into_line).into_iter().collect()
    }

    /// Kolejny klucz do treści, która została poza wierszem.
    fn mint(&mut self) -> u64 {
        self.minted += 1;
        self.minted
    }
}

impl Group {
    /// Zamknięta grupa → jeden wiersz historii.
    fn into_line(self) -> Line {
        let count = self.count;
        match self.kind {
            LineKind::Search => Line::Search {
                agent: self.agent,
                text: search_text(&self.targets, count, self.matches, self.answered),
                // Licznik wiersza `search` to **trafienia**, nie wywołania: tak stoi w polu
                // i tak czyta to człowiek. Ile razy szukaliśmy, mówi tekst.
                count: self.matches,
                // Których plików dotyczyły, nie wiemy: rozbiór wyjścia grepa po stronie
                // kuratora byłby zgadywaniem formatu vendora w miejscu, które ma być wobec
                // vendorów neutralne. Pełne wyjście stoi za `detail_id`.
                paths: Vec::new(),
                detail_id: self.detail_id,
            },
            LineKind::Edit => Line::Edit {
                agent: self.agent,
                text: edit_text(&self.targets, count),
                count,
                paths: self.targets,
                // 2026-08-16 — ZERA SĄ UCZCIWE. `+N −M` da się policzyć wyłącznie z
                // `old_string`/`new_string`, a szew `Tool::Started` niesie akcję i ścieżkę.
                // Liczba zgadnięta z rozmiaru pliku byłaby liczbą zmyśloną; prawdziwą różnicę
                // pokazuje panel zmian (T-08), który czyta dysk, nie ten strumień.
                added: 0,
                removed: 0,
                detail_id: self.detail_id,
            },
            // Read jest domyślną gałęzią, bo grupę otwiera wyłącznie `kind_of`, a ono zna
            // dokładnie te trzy rodzaje.
            _ => Line::Read {
                agent: self.agent,
                text: read_text(&self.targets, count),
                count,
                paths: self.targets,
                // Rozwinięcie wiersza `read` pokazuje ŚCIEŻKI, a te jadą w `paths`. Klucz do
                // treści, której nie ma, byłby kluczem do niczego (niezmiennik 21).
                detail_id: None,
            },
        }
    }
}

/// Rodzina czynności → rodzaj wiersza. Tylko dla tych trzech, które się sklejają.
fn kind_of(action: Action) -> LineKind {
    match action {
        Action::Search => LineKind::Search,
        Action::Edit => LineKind::Edit,
        _ => LineKind::Read,
    }
}

/// Tekst do jednej linii (reguła 1).
///
/// Proza agenta bywa akapitem; wiersz o nieprzewidywalnej wysokości psuje wirtualizowaną listę,
/// która mierzy każdy z nich. Zdania **sklejamy**, nie ucinamy: pierwsza linia akapitu to nie
/// jest jego treść, a `Line` jest jedyną rzeczą, którą dostaje widok.
/// Dzieli prozę na nagłówek do wiersza i całe ciało za wierszem.
///
/// # Reguła
///
/// Proza, która **mieści się w wierszu**, zostaje w wierszu i nie dostaje ciała: kontrolka
/// rozwijająca przy dwuzdaniowej uwadze jest krokiem do zrobienia po nic (niezmiennik 16 czytany
/// od strony kosztu). Dopiero proza, która się nie mieści, oddaje nagłówek i chowa się za nim.
///
/// „Mieści się" znaczy: jeden wiersz i nie dłuższa niż [`SUBJECT_LIMIT`]. Ten sam sufit, co przy
/// przepisywaniu komendy do wiersza, i z tego samego powodu — wiersz ma być wierszem.
///
/// # Co jest nagłówkiem
///
/// PIERWSZY NIEPUSTY WIERSZ prozy, nie jej pierwsze `N` znaków. Model otwiera odpowiedź zdaniem
/// podsumowującym („Implementation is complete and all gates are green."), więc pierwszy wiersz
/// jest streszczeniem napisanym przez tego, kto wie najwięcej. Ucięcie po znakach dałoby w tym
/// samym miejscu „Implementation is complete and all gates are green. ## Answer Tasks and Re…" —
/// czyli nagłówek z doklejonym początkiem nagłówka markdownowego.
fn headline_and_body(text: &str) -> (String, Vec<String>) {
    let head = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let flat = one_line(text);

    if text.lines().filter(|line| !line.trim().is_empty()).count() <= 1
        && flat.chars().count() <= SUBJECT_LIMIT
    {
        return (flat, Vec::new());
    }

    /* CIAŁO NIESIE CAŁĄ PROZĘ, razem z wierszem, który stoi w nagłówku. Ciało zaczynające się od
     * drugiego zdania czyta się jak tekst, któremu ucięto początek, i każe składać jedną
     * wypowiedź z dwóch miejsc na ekranie. */
    (
        clamp(&one_line(head), SUBJECT_LIMIT),
        text.lines().map(str::to_owned).collect(),
    )
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Jak [`one_line`], ale zostawia przełamania wierszy — proza rozmowy, nie strumienia pracy.
///
/// Zwija ciągi białych znaków WEWNĄTRZ wiersza, z tego samego powodu, dla którego okno wybrało
/// `pre-line` zamiast `pre` (`src/sections/run/feed/line.tsx`): wcięcia z modelu robiłyby schody
/// w wąskiej kolumnie. Ciąg pustych wierszy zwija do jednego, bo trzy puste wiersze w strumieniu
/// są dziurą, nie akapitem — a wiodące i końcowe znikają, bo model kończy odpowiedź przełamem
/// częściej, niż nie.
fn paragraphs(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut blank_before = false;
    for line in text.lines() {
        let tidy = one_line(line);
        if tidy.is_empty() {
            // Pusty wiersz przed pierwszą treścią nie jest akapitem, tylko wcięciem od góry.
            blank_before = !out.is_empty();
            continue;
        }
        if blank_before {
            out.push(String::new());
            blank_before = false;
        }
        out.push(tidy);
    }
    out.join("\n")
}

/// Przycina do `limit` bajtów, nie rozcinając znaku.
fn clamp(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// Ostatnie [`TAIL_LINES`] linii wyjścia.
fn tail(output: &str) -> Vec<String> {
    let body = output.strip_suffix('\n').unwrap_or(output);
    let lines: Vec<&str> = body.split('\n').collect();
    let from = lines.len().saturating_sub(TAIL_LINES);
    lines[from..]
        .iter()
        .map(|line| line.trim_end_matches('\r').to_owned())
        .collect()
}

/// Ile trafień zgłosiło wyjście szukania: niepuste linie.
fn count_matches(output: &str) -> u32 {
    let found = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    u32::try_from(found).unwrap_or(u32::MAX)
}

/// `Read sample.txt` / `Read 3 files`.
///
/// Przy jednym pliku widać nazwę, bo to jest informacja; przy wielu — licznik, bo lista nazw
/// w wierszu jest tym samym, co brak wiersza. Pełne ścieżki niesie `paths` i pokazuje je
/// rozwinięcie (pełne za kliknięciem, skrót w widoku — ta sama reguła co przy SHA-256).
fn read_text(targets: &[String], count: u32) -> String {
    match targets {
        [only] => format!("Read {}", file_name(only)),
        _ => format!("Read {count} files"),
    }
}

/// `Edited src/auth.rs` / `Edited 3 files`.
fn edit_text(targets: &[String], count: u32) -> String {
    match targets {
        [only] => format!("Edited {}", file_name(only)),
        _ => format!("Edited {count} files"),
    }
}

/// `Searched for "auth token" — 12 matches`.
fn search_text(targets: &[String], count: u32, matches: u32, answered: bool) -> String {
    let head = match targets {
        [only] => format!("Searched for \"{}\"", clamp(&one_line(only), SUBJECT_LIMIT)),
        _ => format!("Searched {count} times"),
    };
    if !answered {
        return head;
    }
    let plural = if matches == 1 { "match" } else { "matches" };
    format!("{head} — {matches} {plural}")
}

/// `Ran npm test — ok` / `Ran npm test — didn't work`.
///
/// Podmiotem jest **komenda**, nie etykieta, którą model pisze sobie sam w `description`:
/// tamta jest frazą czasownikową („Running the tests"), a ten wiersz ma kształt
/// `Ran <co> — <jak poszło>` [T2 §7.2 poz. 8] i potrzebuje rzeczownika. Komenda jest przy tym
/// jedyną wartością w tym wierszu, której nikt nie wymyślił.
fn ran_text(subject: &str, ok: bool) -> String {
    let outcome = if ok { "ok" } else { "didn't work" };
    format!("Ran {subject} — {outcome}")
}

/// Nazwa pliku ze ścieżki; cała ścieżka, kiedy nazwy nie da się wyjąć.
fn file_name(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

/// Linia zamykająca turę: `Done · 2 turns · 6.2s · $0.15`.
///
/// Liczby w zdaniu są zaokrąglone **do wyświetlenia**, a pola obok niosą wartości surowe —
/// na tym polega różnica między formatowaniem a utratą. `cost_usd` przepisany co do bitu,
/// bo `Line` jest jedyną rzeczą, którą dostaje widok: `$0,15` nie da się zamienić z powrotem
/// w `0.14836290000000002`, a suma biegu byłaby wtedy krzywa na zawsze i bez śladu.
fn done_line(agent: &str, outcome: &Outcome, unknown_price: Option<&str>) -> Line {
    let duration_ms = u64::try_from(outcome.took.as_millis()).unwrap_or(u64::MAX);
    let ended = match (&outcome.reason, outcome.ok) {
        // Anulowanie jest wartością, nie błędem (niezmiennik 7). Krok, który ktoś zatrzymał
        // celowo, nie ma prawa czytać się jak krok, który się zepsuł.
        (FinishReason::Cancelled, _) => Ended::Stopped,
        (_, true) => Ended::Well,
        (_, false) => Ended::Badly,
    };
    // ZDANIE I DECYZJA POWSTAJĄ Z JEDNEGO ROZSTRZYGNIĘCIA. Dwa `match`e na te same trzy stany
    // rozjechałyby się przy pierwszej zmianie brzmienia — a wtedy kafelek mówiłby co innego niż
    // wiersz strumienia obok niego.
    let head = match ended {
        Ended::Stopped => "Stopped",
        Ended::Well => "Done",
        Ended::Badly => "Didn't work",
    };

    let turns = outcome.turns;
    let plural = if turns == 1 { "turn" } else { "turns" };
    let mut text = format!("{head} · {turns} {plural} · {}", took_text(duration_ms));
    if let Some(cost) = outcome.cost_usd {
        // `write!` do `String`, nie `push_str(&format!(…))`: ten drugi alokuje bufor
        // pośredni tylko po to, żeby go zaraz skopiować i wyrzucić (clippy
        // `format_push_string`). Zapis do `String` jest nieomylny — `fmt::Error` może
        // zwrócić tylko sam formatter — więc wynik idzie do `let _`, a nie do `unwrap()`,
        // który w tym drzewie jest `deny`.
        let _ = write!(text, " · ${cost:.2}");
    }
    if let Some(notice) = unknown_price {
        let _ = write!(text, " · {notice}");
    }
    /* POWÓD PORAŻKI, czyli druga rzecz, którą to zdanie ma powiedzieć.
     *
     * Do 2026-08-23 `match` wyżej czytał z `reason` WYŁĄCZNIE wariant `Cancelled` — po to, by
     * odróżnić świadomy Stop od awarii. `Failed(why)` wpadał w `(_, false)`, a `why` nie było
     * czytane nigdy i przez nikogo. Na ekranie stawało więc `Didn't work · 15 turns · 203.4s`
     * i to było wszystko, co człowiek dostawał o kroku, który padł.
     *
     * Że to jest do naprawienia tutaj, dowodzi sąsiad: sterownik Codeksa emituje powód osobnym
     * `Notice` (`drivers/codex.rs`), więc jego porażki miały zdanie, a Claude'a nie — ta sama
     * awaria czytała się inaczej zależnie od vendora, czyli dokładnie ten rozjazd, przed którym
     * broni niezmiennik 23.
     *
     * PIERWSZY WIERSZ I SUFIT, bo `why` bywa zrzutem stosu. Wiersz strumienia rozciągnięty na
     * ekran przewijany w bok jest wierszem, którego nikt nie przeczyta — a reszta i tak leży
     * w `logs/`, dokąd ten skrót ma wysłać, nie zastąpić.
     */
    if let FinishReason::Failed(why) = &outcome.reason
        && let Some(said) = shortened(why)
    {
        let _ = write!(text, " — {said}");
    }

    Line::Done {
        agent: agent.to_owned(),
        text,
        turns,
        duration_ms,
        cost_usd: outcome.cost_usd,
        // PRZEPISANE, NIE POLICZONE — tak samo jak `turns` i `duration_ms` obok. Vendor bez
        // cennika (Codex) podaje wyłącznie te trzy liczby, więc to one są jedynym, co ten krok
        // ma do powiedzenia o swoim rozmiarze.
        input_tokens: outcome.tokens.input,
        output_tokens: outcome.tokens.output,
        cached_tokens: outcome.tokens.cached,
        ended,
    }
}

/// Pierwszy niepusty wiersz powodu, przycięty do jednego wiersza ekranu.
///
/// `None`, kiedy powód jest pusty albo z samych odstępów: myślnik bez zdania za nim jest
/// gorszy niż jego brak, bo wygląda na uciętą treść.
fn shortened(why: &str) -> Option<String> {
    /// Ile znaków powodu wchodzi do wiersza strumienia. Reszta zostaje w `logs/`.
    const MOST: usize = 160;

    let said = why.lines().map(str::trim).find(|one| !one.is_empty())?;
    if said.chars().count() <= MOST {
        return Some(said.to_owned());
    }
    // Po ZNAKACH, nie po bajtach: `&said[..MOST]` panikuje w środku znaku wielobajtowego,
    // a powody przychodzą od vendora, więc mogą być w dowolnym języku.
    let cut: String = said.chars().take(MOST).collect();
    Some(format!("{}…", cut.trim_end()))
}

/// `6.2s` do minuty, `4m 12s` powyżej.
///
/// Liczone na liczbach całkowitych, bo `ms as f64` jest rzutowaniem stratnym, a pełna bramka
/// woła clippy z `-D warnings` — i słusznie: to jedyne miejsce w tym pliku, w którym ktoś
/// mógłby przypadkiem zamienić przepisaną liczbę na przeliczoną.
fn took_text(ms: u64) -> String {
    if ms < 60_000 {
        let mut whole = ms / 1_000;
        let mut tenths = (ms % 1_000 + 50) / 100;
        if tenths == 10 {
            whole += 1;
            tenths = 0;
        }
        return format!("{whole}.{tenths}s");
    }
    let seconds = ms / 1_000;
    format!("{}m {}s", seconds / 60, seconds % 60)
}
