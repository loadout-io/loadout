//! Czat z orchestratorem: rozmowa, która **nigdy nie uruchamia biegu**.
//!
//! # Po co to istnieje
//!
//! Rozstrzygnięcie właściciela 2026-08-19: „ten czat nadrzędny powinien być jak z orchiestratorem,
//! czyli sobie piszemy/zmieniamy coś itp, a sztywne flow dopiero po komendzie". Do tego dnia górny
//! wiersz miał dwa stany i oba były ubogie: przy żywym biegu dopowiadał zdanie pracującemu
//! agentowi, a przy pustym ekranie nie miał komu nic doręczyć. Nie było **z kim** rozmawiać
//! o tym, co dopiero ma się stać.
//!
//! # Czego ten moduł NIE MA i to jest jego główna własność
//!
//! Nie ma ani jednej drogi do uruchomienia biegu. Właściciel rozstrzygnął to wprost tym samym
//! zdaniem — „tylko komendy determinują akcje workflow" — i nie jest to prośba zapisana
//! w promptcie systemowym, którą model mógłby zignorować: ten plik nie zna [`super::RunDeps`],
//! nie importuje `super::run`, nie widzi [`super::RunControl`] i nie ma dostępu do bazy biegów.
//! Orchestrator może czytać projekt, radzić i **przygotowywać** pliki; żeby cokolwiek ruszyło,
//! człowiek musi napisać `/run`. Zdanie w promptcie systemowym jest tu wyłącznie uprzejmością
//! wobec modelu, żeby nie obiecywał czegoś, czego nie zrobi.
//!
//! # Dlaczego to nie jest bieg
//!
//! Bo bieg ma plan, kroki, limit miejsc, katalog `runs/<ts>__<id>/` i dowód śmierci grupy na
//! końcu. Rozmowa nie ma nic z tego i udawanie, że ma, kosztowałoby wpis w historii biegów za
//! każde „siema". To jest jedna sesja, jeden proces, tyle tur, ile człowiek napisze.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::Drivers;
use crate::engine::drivers::{
    AgentDriver, AgentHandle, DecodedEvent, Policy, RunSpec, ToAgent, Voice,
};
use crate::engine::line::{Curator, Line, Seen};
use crate::engine::supervisor::GroupProof;
use crate::ipc::LineSink;
use crate::library::agents::Agent;

/// Pod jaką nazwą orchestrator mówi w strumieniu.
///
/// To samo słowo, którym nazywa go właściciel, i to samo, które trafia w pole `agent` każdego
/// jego wiersza — czyli w podpis, który widać na ekranie. Nazwa kroku biegu nie może z nim
/// kolidować, bo rozmowa i bieg nie stoją w jednym strumieniu w tej samej chwili.
pub const LEAD: &str = "Lead";

/// Ile zdarzeń mieści się w kanale sesji rozmowy.
///
/// Mniej niż bieg, bo tu pracuje jeden agent, a nie ośmiu — ale z zapasem: pełny kanał
/// zatrzymałby pętlę czytającą model, czyli mierzylibyśmy własny przyrząd.
const EVENTS: usize = 128;

/// Prompt systemowy orchestratora.
///
/// Mówi trzy rzeczy i każda ma powód. Że jest do rozmowy — bo inaczej model zachowuje się jak
/// wykonawca zadania i zaczyna pisać kod na pierwsze zdanie. Że **nie uruchamia biegów** — bo
/// model, który obiecuje „już odpalam", zostawia człowieka czekającego na coś, co nie nadejdzie.
/// I że praca zaczyna się od `/run` — bo odmowa bez nazwania następnego ruchu zostawia człowieka
/// tam, gdzie był (DESIGN §8).
pub const BRIEF: &str = "\
You are the orchestrator in Loadout, a desktop app where a person configures agents and \
workflows. You are talking to that person in a chat, not executing a job.

Your part: talk things through, look at the project when it helps, and help shape what the \
workflow should do. You may read files and write draft files when asked.

You cannot start a workflow run, and you must never claim you have started one or that you are \
about to. Only the person can start work, by typing /run in the input line. If they ask you to \
run something, say plainly that they start it with /run, and offer what you can prepare first.

Answer in the language the person writes in. Keep answers short unless they ask for depth.";

/// Co poszło nie tak w rozmowie.
///
/// Osobny typ, nie `anyhow`: każda z tych rzeczy ma inne zdanie dla człowieka i inną czynność
/// naprawczą, a `anyhow` w tym miejscu oddawałby napis od vendora.
#[derive(Debug)]
pub enum ChatError {
    /// Człowiek nacisnął Enter na pustym polu.
    NothingToSay,
    /// Nie ma folderu, więc rozmowa nie ma gdzie patrzeć.
    ///
    /// 2026-08-19 — WARIANT ISTNIEJE I NIE MA WOŁAJĄCEGO, i jest to zgłoszenie, nie przeoczenie.
    /// `AppState::project_for(None)` nie odmawia: bierze katalog, pod którym wstała aplikacja, więc
    /// rozmowa bez wybranego zakresu patrzy tam. Czy to jest poprawne, jest pytaniem o produkt —
    /// bieg w tej samej sytuacji ODMAWIA i odsyła do bocznego menu (`launch.ts`, `NO_FOLDER`).
    /// Zdanie stoi tu gotowe na dzień, w którym człowiek to rozstrzygnie; użycie go dzisiaj
    /// zmieniłoby zachowanie, o które nikt nie prosił.
    NoFolder,
    /// Sesji nie udało się wystartować — zdanie od sterownika w środku.
    CouldNotStart(String),
    /// Sesja zeszła i nie przyjmuje już tur.
    StoppedListening,
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NothingToSay => write!(f, "Write something first, then press Enter."),
            Self::NoFolder => write!(
                f,
                "Pick a workspace first: the lead agent looks at the folder you are working in."
            ),
            Self::CouldNotStart(said) => write!(f, "The lead agent could not start: {said}"),
            Self::StoppedListening => write!(
                f,
                "The lead agent stopped listening. Write again and it will start a fresh \
                 conversation."
            ),
        }
    }
}

impl std::error::Error for ChatError {}

/// Żywa sesja rozmowy.
struct Session {
    /// Głos do niej — klonowalny nadajnik, bez `&mut` (`engine::drivers::Voice`).
    voice: Voice,
    /// Uchwyt sesji. Trzymany, bo jego porzucenie jest końcem procesu.
    handle: Box<dyn AgentHandle>,
    /// Zadanie zamieniające zdarzenia na wiersze. Kończy się razem z kanałem sesji.
    reader: JoinHandle<()>,
}

/// Rozmowa z orchestratorem: strumień do okna i sesja, która powstaje przy pierwszym zdaniu.
pub struct Chat {
    /// Tędy wiersze rozmowy idą do okna — uchwyt WYMIENNY, wspólny z zadaniem czytającym.
    ///
    /// # Dlaczego to nie jest zwykłe pole
    ///
    /// Zmierzone 2026-08-19 w dzienniku aplikacji: `open_chat` wołane drugi raz (a woła je każdy
    /// montaż ekranu pracy i każde przeładowanie okna przez HMR) zamykało CAŁĄ rozmowę — więc
    /// wyjście na Agentów i powrót gubiło wątek. To jest wprost sprzeczne z tym, po co ta rozmowa
    /// istnieje („sobie piszemy/zmieniamy coś itp").
    ///
    /// Podmiana samego pola nic by nie dała: zadanie czytające trzyma nadajnik od chwili startu
    /// sesji, więc pisałoby dalej w kanał, którego nikt już nie słucha. Dlatego jeden uchwyt na
    /// dwóch — okno podmienia zawartość, zadanie czyta ją przy każdym wierszu.
    ///
    /// `std::sync::Mutex` i nigdy trzymany przez `await` (niezmiennik 8): `LineSink::send` jest
    /// synchroniczny, a każde wzięcie tego zamka mieści się w jednym wyrażeniu.
    lines: Arc<Mutex<LineSink>>,
    /// Sesja. `None`, dopóki człowiek nic nie napisał.
    ///
    /// LENIWIE, i to jest decyzja o pieniądzach: sesja wystartowana przy montażu ekranu płaci za
    /// pierwszą turę u dostawcy, choć nikt jeszcze o nic nie zapytał. Pierwsze zdanie człowieka
    /// JEST pierwszą turą.
    live: Option<Session>,
}

/* RĘCZNIE, bo `Box<dyn AgentHandle>` nie jest `Debug` i nie ma być: sterownik nie ma obowiązku
 * opisywać swojego procesu. Pokazujemy JEDEN fakt, który cokolwiek znaczy w dzienniku — czy sesja
 * już stoi. Ten sam zabieg i ten sam powód stoi przy `ipc::AppState`. */
impl std::fmt::Debug for Chat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chat")
            .field("live", &self.is_live())
            .finish_non_exhaustive()
    }
}

impl Chat {
    /// Świeża rozmowa: jest gdzie pisać wiersze, nie ma jeszcze z kim rozmawiać.
    #[must_use]
    pub fn new(lines: LineSink) -> Self {
        Self {
            lines: Arc::new(Mutex::new(lines)),
            live: None,
        }
    }

    /// Okno otwarło się na nowo: wiersze idą odtąd TAM, a rozmowa zostaje.
    ///
    /// To jest cała naprawa „przejście na inną sekcję gubi rozmowę". Sesja u dostawcy nic o tym
    /// nie wie i nie ma powodu wiedzieć — zmienia się wyłącznie to, komu jej wiersze są pokazywane.
    pub fn lines_go_to(&self, lines: LineSink) {
        *self.lines.lock().unwrap_or_else(PoisonError::into_inner) = lines;
    }

    /// Czy rozmowa ma już żywą sesję.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    /// Mówi coś orchestratorowi — pierwsze zdanie zakłada sesję, każde następne jest kolejną turą.
    ///
    /// # Kolejność, która jest treścią
    ///
    /// Wiersz z **twoim** zdaniem wchodzi do strumienia dopiero wtedy, gdy zdanie naprawdę
    /// pojechało. Odwrotna kolejność pokazywałaby na ekranie rozmowę, której druga strona nie
    /// usłyszała — ten sam powód i ta sama reguła, co w [`super::run::say_to_agent_inner`].
    pub async fn say(
        &mut self,
        driver: &dyn AgentDriver,
        cwd: PathBuf,
        text: &str,
    ) -> Result<(), ChatError> {
        let said = text.trim();
        if said.is_empty() {
            return Err(ChatError::NothingToSay);
        }

        if let Some(session) = self.live.as_ref() {
            // Sesja stoi: zdanie jest kolejną turą i jedzie głosem, bez `&mut` na uchwycie.
            session
                .voice
                .send(ToAgent::Turn(said.to_owned()))
                .await
                .map_err(|_| ChatError::StoppedListening)?;
        } else {
            /* Strumień KLONUJEMY przed `await`, i to nie jest kosmetyka. `begin` nie bierze
             * `&self`, bo `&Chat` nie jest `Send`: uchwyt sesji (`Box<dyn AgentHandle>`) jest
             * `Send`, ale nie `Sync`, a `&T: Send` wymaga `T: Sync`. Pożyczka `self` przeżywająca
             * `await` czyni całą komendę nie-`Send`, czego Tauri nie przyjmuje — i słusznie,
             * bo to zadanie może wznowić się na innym wątku. */
            let session = begin(driver, cwd, Arc::clone(&self.lines), said).await?;
            self.live = Some(session);
        }

        /* TWOJE ZDANIE W STRUMIENIU. Wynik świadomie porzucony: pełna kolejka do okna jest
         * normalnym stanem (`ipc::Sent`), a zdanie i tak POSZŁO — odmowa w tym miejscu mówiłaby,
         * że tura nie doszła, kiedy doszła. */
        let _ = self.say_in_the_stream(Line::Told {
            agent: LEAD.to_owned(),
            text: said.to_owned(),
        });
        Ok(())
    }

    /// Wpisuje wiersz do strumienia rozmowy; `false`, kiedy nie dojechał.
    ///
    /// Klon POD zamkiem, wysyłka NAD nim: `send` jest synchroniczny, ale trzymanie zamka przez
    /// cudzy kod jest tym, z czego robi się zakleszczenie, którego nikt nie umie odtworzyć.
    fn say_in_the_stream(&self, line: Line) -> bool {
        let sink = self
            .lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        sink.send(line) == crate::ipc::Sent::Queued
    }

    /// Kończy rozmowę: sesja schodzi, zadanie czytające razem z nią.
    ///
    /// Wołane przy zamykaniu okna — proces rozmowy jest procesem jak każdy inny i po śmierci
    /// Loadouta przeszedłby pod PID 1 (`recovery.rs`, nagłówek), czyli dokładnie ten defekt,
    /// który 2026-08-19 naprawiono dla biegów.
    pub async fn close(&mut self) {
        let Some(mut session) = self.live.take() else {
            return;
        };
        let _ = session.handle.close().await;
        /* Zadanie czytające kończy się na zamkniętym kanale zdarzeń, więc po `close()` nie ma na
         * co czekać — ale porzucony `JoinHandle` zostawiłby zadanie, o którym nikt nie wie. */
        session.reader.abort();
    }
}

// ── KIM JEST LIDER I GDZIE MIESZKA JEGO WĄTEK ──────────────────────────────────────────────
//
// 2026-08-20 — SZKIELET T-60. Ciała są `todo!()`, więc kryteria padają w czasie wykonania,
// a nie na kompilacji: test, który się nie zbudował, nie uruchomił niczego (AGENTS.md §2a p. 5).
// `clippy::todo = deny` w `Cargo.toml` pilnuje, żeby ani jedno z nich nie przeżyło do pełnej
// bramki. Podkreślenia przy nazwach parametrów są częścią tej samej tymczasowości: ciało, które
// ich nie czyta, dawałoby `unused_variables` — implementacja zdejmuje je razem z `todo!()`.

/// Kim jest lider tej rozmowy — jego zapisana definicja i nic obok niej.
///
/// # Dlaczego to jest typ, a nie sam [`Agent`]
///
/// Bo pytanie „co z definicji dojeżdża do sesji" ma mieć JEDNĄ odpowiedź w jednym miejscu
/// (niezmiennik 13): vendor wybiera sterownik, `model` jedzie do [`RunSpec::model`],
/// `file_access` przechodzi TĄ SAMĄ tabelą, którą czyta bieg, a `instructions` doklejają się do
/// briefu. Kopia któregokolwiek z tych czterech pól, trzymana obok definicji — w stanie okna,
/// w polu struktury, w stałej — jest pierwszą rzeczą, która się rozjedzie, i rozjedzie się po
/// cichu: lider odpowiadający innym modelem niż wybrany wygląda dokładnie jak lider, który się
/// myli.
#[derive(Debug, Clone)]
pub struct Lead {
    /// Definicja, znak w znak taka, jak leży w bibliotece.
    pub agent: Agent,
}

impl Lead {
    /// Wskazany lider → jego zapisana definicja.
    ///
    /// `who` jest **identyfikatorem** zapisanego agenta, nie nazwą pliku i nie nazwą vendora —
    /// ten sam wybór, co przy [`super::skills::draft_skill_inner`], i z tego samego powodu:
    /// `id` przeżywa zmianę nazwy (T4 §5.1).
    ///
    /// `None` znaczy „nikt nie jest wskazany" i jest **odmową nazywającą następny ruch**, nigdy
    /// cichym powrotem do zaszytego vendora. Cichy powrót jest tu gorszy niż odmowa, bo nie ma
    /// żadnego sygnału, po którym człowiek mógłby odróżnić lidera, którego wybrał, od lidera,
    /// którego dostał — a jedyną rzeczą, która się zmieniła, był jego własny klik.
    pub fn pointed_at(_library: &Path, _who: Option<&str>) -> Result<Self, ChatError> {
        todo!("T-60 AC-1: wskazany lider -> jego zapisana definicja, brak wskazania -> odmowa")
    }

    /// Co temu liderowi wolno zrobić z plikami.
    ///
    /// TĄ SAMĄ tabelą `FileAccess` → [`Policy`], którą czyta bieg (`commands::run::policy_of`),
    /// nigdy drugą jej kopią (niezmiennik 23). Druga kopia tej tabeli to sposób, w jaki w repo
    /// źródłowym po cichu umarło skanowanie sekretów: obie wyglądają poprawnie, a podpięta jest
    /// zawsze starsza.
    #[must_use]
    pub fn policy(&self) -> Policy {
        todo!("T-60 AC-1: tabela z commands::run::policy_of, nie druga kopia")
    }

    /// Prompt systemowy tego lidera: brief dopasowany do jego polityki **plus** jego instrukcje.
    ///
    /// Razem, nie zamiast: [`BRIEF`] mówi, czego lider nie umie zrobić (zaczynać biegów) i czym
    /// się to robi (`/run`), a instrukcje mówią, kim on jest. Lider bez pierwszego zdania obieca
    /// start, którego nie wykona; lider bez drugiego jest agentem, którego nikt nie konfigurował.
    #[must_use]
    pub fn brief(&self) -> String {
        todo!("T-60 AC-3: brief dla polityki + instrukcje z definicji")
    }
}

/// Wątki lidera: po jednym na zakres, wszystkie w jednym miejscu.
///
/// # Po co to istnieje obok [`Chat`]
///
/// Bo [`Chat`] jest JEDNĄ rozmową i jego własny komentarz zapowiada ten dzień: „jedna na
/// aplikację, nie jedna na zakres — i to jest do przemyślenia, kiedy zakresy dostaną własne
/// sesje" (`ipc::AppState::chat`). Skutek dzisiejszego stanu widzi człowiek: `Chat::say` używa
/// `cwd` **wyłącznie przy zakładaniu sesji**, więc rozmowa o projekcie A, po przełączeniu na B,
/// odpowiada dalej o A — bez ani jednego zdania ostrzeżenia, z żywego procesu siedzącego
/// w folderze sprzed przełączenia.
///
/// Zakres jest kluczem, bo zakres jest tym, co człowiek przełącza. Wpis w [`Threads::lines`]
/// powstaje, kiedy okno pierwszy raz na ten zakres patrzy; wpis w [`Threads::live`] dopiero przy
/// pierwszym zdaniu — sesja wystartowana przy montażu ekranu płaci za turę, o którą nikt nie
/// zapytał, i to jest ten sam powód, który stoi przy [`Chat::live`].
#[derive(Default)]
pub struct Threads {
    /// Kanał wierszy tego zakresu. Podmieniany przy każdym otwarciu ekranu, nigdy zamykany:
    /// zamknięcie cudzej rozmowy przy przełączeniu byłoby zgubieniem wątku, o który chodzi
    /// cała ta zmiana.
    lines: HashMap<PathBuf, Arc<Mutex<LineSink>>>,
    /// Sesja tego zakresu. Osobny wpis na zakres, bo to jest jedyna rzecz, która czyni zdanie
    /// „wątek należy do zakresu" prawdziwym, a nie zadeklarowanym.
    live: HashMap<PathBuf, Session>,
}

/* RĘCZNIE, z tego samego powodu, co przy [`Chat`]: `Box<dyn AgentHandle>` nie jest `Debug`
 * i nie ma być. Pokazujemy dwie liczby, które cokolwiek znaczą w dzienniku — na ile zakresów
 * okno patrzyło i ile wątków naprawdę stoi. */
impl std::fmt::Debug for Threads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Threads")
            .field("watched", &self.lines.len())
            .field("live", &self.live.len())
            .finish_non_exhaustive()
    }
}

impl Threads {
    /// Ani jednego wątku i ani jednego widoku — stan aplikacji, która właśnie wstała.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Okno patrzy na ten zakres: jego wiersze idą odtąd TAM, a wątek zostaje.
    ///
    /// Wołane przy każdym montażu ekranu pracy i przy każdym przeładowaniu okna, więc **nie może**
    /// niczego kończyć — powód i pomiar stoją przy [`Chat::lines_go_to`].
    pub fn lines_go_to(&mut self, _cwd: PathBuf, _lines: LineSink) {
        todo!("T-60 AC-2: strumień per zakres, wątek nietknięty")
    }

    /// Czy w tym zakresie stoi wątek.
    ///
    /// Pytanie zadawane o zakres, nie o aplikację: to na nim stoi asercja „sesja zakresu B żyje
    /// dalej, kiedy okno patrzy na A".
    #[must_use]
    pub fn is_live_in(&self, _cwd: &Path) -> bool {
        todo!("T-60 AC-2: wątek jest własnością zakresu")
    }

    /// Mówi zdanie liderowi w TYM zakresie — pierwsze zdanie zakłada jego wątek, każde następne
    /// jest kolejną turą tego samego wątku.
    ///
    /// Sterownik wybiera **fabryka**, po vendorze z definicji lidera, i dlatego jedzie tu
    /// [`Drivers`], a nie gotowy sterownik: wybór po vendorze jest jedną z rzeczy, których to
    /// zadanie dowodzi, a wybór zrobiony u wołającego byłby wyborem, którego żaden test bez okna
    /// nie widzi (dziś robi go `ipc::AppState::chat_driver`, na sztywno).
    pub async fn say(
        &mut self,
        _drivers: &Drivers,
        _lead: &Lead,
        _cwd: PathBuf,
        _text: &str,
    ) -> Result<(), ChatError> {
        todo!("T-60 AC-1/AC-2: vendor, model, polityka i brief z definicji; wątek per zakres")
    }

    /// Zamknięcie okna: schodzą WSZYSTKIE wątki i każdy oddaje dowód śmierci swojej grupy.
    ///
    /// Dowód, nie „wysłałem sygnał" (niezmiennik 6): rozmowa porzucona żywa przechodzi pod PID 1
    /// i pracuje dalej (`recovery.rs`, nagłówek), a odzyskiwanie po niej nie posprząta, bo
    /// rozmowa nie ma wpisu w indeksie biegów. Osierocony agent pali limit w tle — to jest błąd
    /// finansowy, nie higieniczny.
    ///
    /// Oddaje po jednym dowodzie na wątek, bo bilans jest kompletny tylko wtedy, kiedy widać
    /// KAŻDY z nich: jeden `Alive` wśród pięciu `Dead` jest dokładnie tym stanem, o którym nikt
    /// się nie dowie z liczby „zamknięto pięć".
    pub async fn close(&mut self) -> Vec<GroupProof> {
        todo!("T-60 AC-2: wszystkie wątki schodzą, każdy z dowodem")
    }
}

/// Zdarzenia sesji rozmowy → wiersze na ekran.
///
/// Kuracja mieszka w [`Curator`] i tylko tam (niezmiennik 15): ta pętla nie decyduje, który wiersz
/// istnieje ani co mówi. To ten sam mechanizm, którym idą wiersze biegu — druga tabela nazw
/// czynności obok tamtej byłaby tą, o której nikt by nie pamiętał (niezmiennik 23).
///
/// Czas liczymy od pierwszego zdarzenia, nie od zegara ściennego: [`Seen::at_ms`] służy oknu
/// sklejania, a nie datowaniu, i kurator z własnym zegarem nie dałby się sprawdzić bez `sleep`.
async fn read_along(mut inbox: mpsc::Receiver<DecodedEvent>, lines: Arc<Mutex<LineSink>>) {
    let mut curator = Curator::new();
    let began = std::time::Instant::now();
    while let Some(DecodedEvent { event, tool }) = inbox.recv().await {
        let at_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
        let seen = Seen {
            agent: LEAD,
            at_ms,
            event: &event,
            tool: tool.as_ref(),
        };
        for line in curator.observe(seen) {
            /* Uchwyt czytany PRZY KAŻDYM wierszu, nie raz na starcie: okno mogło się w międzyczasie
             * przeładować i odtąd wiersze mają iść do nowego kanału. */
            let sink = lines.lock().unwrap_or_else(PoisonError::into_inner).clone();
            let _ = sink.send(line);
        }
    }
    /* Koniec strumienia zamyka otwartą grupę sklejania. Bez tego ostatnie zdanie rozmowy zostaje
     * w kuratorze i nie dochodzi nigdy — czyli odpowiedź, na którą człowiek czeka, przepada. */
    for line in curator.flush() {
        let sink = lines.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let _ = sink.send(line);
    }
}

/// Startuje sesję rozmowy z pierwszym zdaniem człowieka jako pierwszą turą.
///
/// Wolna funkcja, nie metoda, i powód jest twardy: `&Chat` nie jest `Send`, bo uchwyt sesji jest
/// `Send` ale nie `Sync`, a `&T: Send` wymaga `T: Sync`. Pożyczka `self` przeżywająca `await`
/// uczyniłaby całą komendę nie-`Send`, czego Tauri nie przyjmuje.
async fn begin(
    driver: &dyn AgentDriver,
    cwd: PathBuf,
    lines: Arc<Mutex<LineSink>>,
    first: &str,
) -> Result<Session, ChatError> {
    let (events, inbox) = mpsc::channel::<DecodedEvent>(EVENTS);
    let spec = RunSpec {
        run_id: Uuid::now_v7(),
        cwd,
        prompt: first.to_owned(),
        model: None,
        system_append: Some(BRIEF.to_owned()),
        /* PISZE W SWOIM FOLDERZE, bo „przygotowywać" znaczy móc zapisać szkic — a rozmowa,
         * która umie tylko czytać, odpowiada „napisz to sobie sam". `EditInFolder`, nie
         * `Unrestricted`: rozmowa nie ma powodu dotykać niczego poza folderem, w którym
         * człowiek pracuje. Uruchomienia biegu to nie dotyczy i nie ma jak dotyczyć — biegi
         * zaczyna komenda, a tej nie ma w żadnym narzędziu, które ten proces widzi. */
        policy: Policy::EditInFolder,
        extra_dirs: Vec::new(),
        resume: None,
    };

    let handle = driver
        .start(spec, events)
        .await
        .map_err(|error| ChatError::CouldNotStart(error.to_string()))?;
    let voice = handle.voice().ok_or(ChatError::StoppedListening)?;
    let reader = tokio::spawn(read_along(inbox, lines));
    Ok(Session {
        voice,
        handle,
        reader,
    })
}
