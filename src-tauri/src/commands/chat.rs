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
use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::Drivers;
use crate::bridge::Role as BridgeRole;
use crate::bridge::host::Bridge;
use crate::bridge::library::{Desk as BridgeLibrary, Waiting as AskWaiting};
use crate::engine::drivers::claude::{no_such_tools, tool_surface};
use crate::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason, Policy,
    RunSpec, ToAgent, ValidatedImages, Voice,
};
use crate::engine::line::{Curator, Line, Seen, suggested};
use crate::engine::supervisor::GroupProof;
use crate::evidence::{
    ConversationMetadata, ConversationVendor, EvidenceFailureKind, EvidenceTarget, ImageFact,
    SafeInputManifest, TurnCounters,
};
use crate::ipc::LineSink;
use crate::library::agents::{Agent, Tools, effort_level, policy_of};

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

/// Ile czekamy, aż po dowodzie `Dead` czytnik opróżni już zakolejkowane zdarzenia i `flush()`.
const READER_DRAIN: Duration = Duration::from_secs(1);

/// Katalog agentów w bibliotece człowieka: `~/.loadout/agents/` (`docs/ARCHITECTURE.md` §8).
///
/// # Dlaczego własna stała, a nie import z [`super::agents`]
///
/// Bo tamta jest prywatna — i to nie jest przeoczenie, tylko kształt, którym ten fakt stoi w tym
/// drzewie już cztery razy: `commands::agents` i `commands::run` (agenci), `commands::workflows`
/// i `ipc` (workflow). Jedna odpowiedź mieszka w §8, a każdy moduł, który składa ścieżkę, trzyma
/// swoją kopię z odnośnikiem do niej.
///
/// Ta kopia niesie więc dokładnie ten sam obowiązek, co tamte cztery: dzień, w którym §8 zmieni
/// nazwę któregoś z tych dwóch katalogów, jest dniem, w którym `"agents"` i `"workflows"` trzeba
/// przeszukać po całym `src-tauri/src`. Rozjazd nie wygląda tu na literówkę — wygląda na lidera,
/// który „nie widzi" workflow leżącego na dysku.
///
/// Piąte trafienie tego gerpu jest przy tym FAŁSZYWE i dlatego stoi tu wymienione:
/// `inherit::wire::SUBAGENTS_DIR` to `agents/` w repo **gospodarza** (`.claude/agents`), czyli
/// inny fakt o tej samej nazwie. Zmiana §8 nie ma go dotknąć.
const AGENTS_DIR: &str = "agents";

/// Katalog workflow w tej samej bibliotece: `~/.loadout/workflows/` (§8 tamże, ten sam powód
/// i ten sam obowiązek, co przy [`AGENTS_DIR`]).
const WORKFLOWS_DIR: &str = "workflows";

/// Prompt systemowy orchestratora.
///
/// Mówi trzy rzeczy i każda ma powód. Że jest do rozmowy — bo inaczej model zachowuje się jak
/// wykonawca zadania i zaczyna pisać kod na pierwsze zdanie. Że **nie uruchamia biegów** — bo
/// model, który obiecuje „już odpalam", zostawia człowieka czekającego na coś, co nie nadejdzie.
///
/// # 2026-08-30 — LIDER UMIE JUŻ ZACZĄĆ PRACĘ, WIĘC BRIEF PRZESTAŁ TWIERDZIĆ, ŻE NIE UMIE
///
/// Do tego dnia stało tu „You cannot start a workflow run… Only the person can start work, by
/// typing /run". Było to prawdą i było ostrożnością: rozmowa nie miała ŻADNEJ drogi do biegu.
///
/// Rozstrzygnięcie właściciela z 2026-08-30 („rusza samo") tę drogę otwiera —
/// `crate::bridge::verbs` daje liderowi czasownik `start_workflow`. Prompt musiał pójść za tym
/// **tego samego dnia**: model, który ma narzędzie i zdanie mówiące, że go nie ma, jest najgorszą
/// z możliwych kombinacji. Albo nie sięgnie po nie ani razu, albo sięgnie i zaprzeczy sam sobie
/// w tej samej odpowiedzi.
///
/// **Co z tamtej ostrożności zostaje, dosłownie:** zakaz obiecywania startu, którego nie było.
/// Brzmi teraz „never say you have started something unless a tool told you it went" — bo
/// narzędzie odpowiada, czy poszło, a proza nie.
///
/// # Czego w tym prompcie NIE MA i mieć nie może
///
/// Ani jednego zdania każącego zadać pytanie. Wymaganie właściciela z 2026-08-30, dosłownie:
/// „nie chcę też aby na sztywno było żeby agent lub ktokolwiek zadawał 2-3 pytania, wszystko
/// zależy od analiz i potrzeb". Ceremonia wpisana w prompt jest ceremonią, której nie da się
/// wyłączyć konfiguracją — czyli dokładnie tym, czego zabrania D7.
///
/// # Skąd lider wie, jak idzie bieg
///
/// Z plików, nie z drugiego kanału. Jego katalog roboczy to folder projektu, katalog biegu leży
/// w środku, `run.json` jest przepisywany po każdej zmianie księgi, a `Read`/`Glob` ma na każdym
/// szczeblu dialu — więc jedyne, czego brakowało, to zdanie mówiące, GDZIE patrzeć. Tym samym
/// domyka się punkt 5 z D6 („orchestrator widzi, co wyprodukowali pozostali"), jedyny z pięciu,
/// który nie miał do dziś ani jednej linii kodu.
pub const BRIEF: &str = "\
You are the orchestrator in Loadout, a desktop app where a person configures agents and \
workflows. You are talking to that person in a chat, not executing a job.

Your part: talk things through, look at the project when it helps, and help shape what the \
workflow should do. You may read files and write draft files when asked.

Loadout gives you its own tools. Use list_workflows and list_agents to see what this person has \
actually built, and start_workflow to start one of them. Always look before you start, and use \
the name exactly as the list gave it, so you start something they really have.

Never say you have started something unless a tool told you it went. The run appears in the \
stream this person is watching, and so does the reason if it could not start.

While a run is going, the truth about it is on disk inside the folder you are working in: \
.loadout/runs has one directory per run, newest last, and each one holds run.json with the steps \
and their state, handoffs with what each step passed on, and logs. Read them when this person \
asks how the work is going. Do not guess at progress you have not read.

Answer in the language the person writes in. Keep answers short unless they ask for depth.";

/// Zdanie [`BRIEF`] o plikach — **znak w znak to, które w nim stoi**.
///
/// Wyjęte do stałej, bo przy `look only` staje się nieprawdą i musi dać się wymienić. Jeżeli
/// ktoś przepisze brief innymi słowami, podmiana niżej nie znajdzie tej frazy i wersja dla
/// `ReadOnly` zostanie z obietnicą zapisu — a to jest czerwień `brief_matches_the_policy`,
/// nie ciche przejście. Kryterium jest tu strażnikiem, bo `replace` sam z siebie nic nie mówi.
const MAY_WRITE_DRAFTS: &str = "You may read files and write draft files when asked.";

/// To samo zdanie przy [`Policy::ReadOnly`].
///
/// Model nie ma skąd wiedzieć, że mu nie wolno: dial jedzie do vendora osobno, flagami, a prompt
/// systemowy mówi swoje. Lider, który obieca plik i go nie zapisze, zostawia człowieka czekającego
/// na coś, co nie powstanie — więc zamiast obietnicy dostaje tu ruch, który MOŻE wykonać.
const LOOK_ONLY: &str = "You may read files. You cannot write anything, not even a rough one: \
     this lead was set to look only, so put what you would have saved into your answer instead of \
     promising a file.";

/// To samo zdanie przy [`Policy::EditInFolder`].
///
/// Nazywa granicę, bo to ona jest treścią tej pozycji dialu: „przygotować szkic" znaczy zapisać
/// go **w folderze, w którym człowiek pracuje**, a nie gdziekolwiek.
const MAY_WRITE_DRAFTS_HERE: &str = "You may read files and write draft files when asked, inside \
     the folder this person is working in.";

/// To samo zdanie przy [`Policy::Unrestricted`].
const MAY_WRITE_DRAFTS_ANYWHERE: &str = "You may read files and write draft files when asked, and \
     you are not held to the folder this person is working in.";

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
    /// Bezpiecznego receiptu nie dało sie zapisac, wiec tura nie pojechala.
    CouldNotRecord,
    /// Sesja zeszła i nie przyjmuje już tur.
    StoppedListening,
    /// Sterownik odmówił tury, a eskalacja nie dowiodła jeszcze śmierci procesu.
    StillRunning,
    /// Nikt nie jest wskazany na lidera.
    ///
    /// **Odmowa, nigdy cichy powrót do zaszytego vendora**, i to jest cała treść tego wariantu.
    /// Powrót jest tu gorszy niż odmowa: rozmowa idzie, płaci i odpowiada — tylko nie ten agent,
    /// którego człowiek wybrał. Nie ma przy tym żadnego sygnału, po którym dałoby się odróżnić
    /// lidera, którego wybrał, od lidera, którego dostał.
    NobodyIsTheLead,
    /// Wskazano lidera, którego w bibliotece nie ma.
    ///
    /// Osobny wariant, nie [`ChatError::NobodyIsTheLead`], bo **czynność naprawcza jest inna**:
    /// brak wskazania naprawia wybranie kogokolwiek, a wskazanie na nieistniejącego — wybranie
    /// kogoś INNEGO. Jedno zdanie na dwa stany zostawiałoby połowę ludzi przy niedziałającej
    /// instrukcji.
    NoSuchLead(String),
    /// Biblioteki agentów nie dało się przeczytać — zdanie z `library::agents` w środku.
    ///
    /// Przezroczyste, bo tamten typ nazywa PLIK (T4 §10), a „popraw ten plik" jest wykonalne
    /// tylko wtedy, kiedy widać który.
    CouldNotReadTheLibrary(String),
    /// Okno nie otworzyło jeszcze strumienia tego zakresu.
    ///
    /// Wątek bez kanału jest wątkiem, którego wierszy nikt nie odbiera — czyli rozmową, która
    /// płaci u dostawcy i nie ma jak nic pokazać. Kolejność jest odwrotna i tak ją woła okno:
    /// najpierw `open_chat`, potem pierwsze zdanie.
    NotWatchingThatFolder,
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
            Self::CouldNotRecord => write!(
                f,
                "Loadout could not save this conversation, so it did not send the message."
            ),
            Self::StoppedListening => write!(
                f,
                "The lead agent stopped listening. Write again and it will start a fresh \
                 conversation."
            ),
            Self::StillRunning => write!(
                f,
                "The lead agent stopped accepting messages, but it is still running after \
                 Loadout tried to stop it. Loadout is still tracking it; close this terminal to \
                 try stopping it again."
            ),
            // Nazywa NASTĘPNY RUCH, bo odmowa bez niego zostawia człowieka tam, gdzie był
            // (DESIGN §8) — a tu jest gdzie odesłać: kontrolka lidera stoi w pasku pracy.
            Self::NobodyIsTheLead => write!(
                f,
                "Pick a lead agent first: Loadout will not guess who you are talking to. Choose \
                 one in the work screen, or save one in Agents if the list is empty."
            ),
            Self::NoSuchLead(who) => write!(
                f,
                "The lead agent you picked is not in your library any more ({who}). Choose \
                 another one in the work screen."
            ),
            Self::CouldNotReadTheLibrary(said) => {
                write!(f, "Loadout could not read your saved agents: {said}")
            }
            Self::NotWatchingThatFolder => write!(
                f,
                "The lead agent is not ready in this folder yet. Reopen the work screen and try \
                 again."
            ),
        }
    }
}

impl std::error::Error for ChatError {}

/// Żywa sesja rozmowy.
struct Session {
    /// Głos do niej, kiedy vendor trzyma jeden dwukierunkowy proces. `None` nie znaczy, ze
    /// rozmowy nie ma: Codex konczy proces po turze i wznawia ja przez [`AgentHandle::send`].
    voice: Option<Voice>,
    /// Uchwyt sesji. Trzymany, bo jego porzucenie jest końcem procesu.
    handle: Box<dyn AgentHandle>,
    /// Zadanie zamieniające zdarzenia na wiersze. Kończy się razem z kanałem sesji.
    reader: JoinHandle<()>,
    /// Prywatny receipt tej rozmowy. `None` tylko w legacy `Chat`, bez produkcyjnego wolacza.
    evidence: Option<EvidenceTarget>,
    /// Liczba rozpoczętych prób, także tej, której transport odmówił przed dostarczeniem.
    attempts: usize,
}

/// Uchwyt jest już przyjęty przez sterownik, ale odpowiedzi nie mają jeszcze drogi na ekran.
///
/// Ten stan trwa dokładnie do zapisania pierwszego [`Line::Told`]. Dopiero potem [`Self::listen`]
/// uruchamia czytnik. Zdarzenia, które szybki vendor zdążył wysłać w `start`, bezpiecznie
/// czekają w `inbox` i nie mogą wyprzedzić pytania człowieka.
struct ReadySession {
    voice: Option<Voice>,
    handle: Box<dyn AgentHandle>,
    inbox: mpsc::Receiver<DecodedEvent>,
    evidence: Option<EvidenceTarget>,
    attempts: usize,
}

impl ReadySession {
    fn listen(self, lines: Arc<Mutex<LineSink>>) -> Session {
        let reader = tokio::spawn(read_along(self.inbox, lines, self.evidence.clone()));
        Session {
            voice: self.voice,
            handle: self.handle,
            reader,
            evidence: self.evidence,
            attempts: self.attempts,
        }
    }
}

/// Rozmowa z orchestratorem: strumień do okna i sesja, która powstaje przy pierwszym zdaniu.
///
/// # 2026-08-20 (T-71) — TEN TYP NIE MA JUŻ PRODUKCYJNEGO WOŁAJĄCEGO, I JEST TO ZGŁOSZENIE
///
/// `ipc::AppState` trzymał go w polu `chat` jako JEDNĄ rozmowę na całą aplikację; pole zniknęło,
/// bo żywa droga idzie dziś przez [`Threads`], po jednym wątku na terminal. Konstruują ten typ
/// wyłącznie dwa pliki testowe (`tests/it/chat_never_starts_a_run.rs`,
/// `tests/flow_lead_agent_chat.rs`), więc jest to dokładnie ten kształt, na który to repo ma
/// osobne sprawdzenie: mechanizm z testem i bez wołającego (`checks/quick-wired.sh`, nagłówek).
/// Sprawdzenie nie świeci, bo sądzi wyłącznie `pub fn` na poziomie modułu DOPISANE przez gałąź.
///
/// Skasowania nie robię tutaj i to nie jest wygoda: `chat_never_starts_a_run` jest kryterium
/// sprzed tego zadania i dowodzi na tym typie rzeczy, której nie dowodzi nic innego — że rozmowa
/// nie ma **żadnej** drogi do uruchomienia biegu. Zabranie mu podmiotu jest zmianą cudzego
/// kryterium, a nie porządkami (AGENTS.md §7). Ten akapit jest po to, żeby dzień, w którym ktoś
/// przepisze tamto kryterium na [`Threads`], był zarazem dniem, w którym ten typ znika.
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

        if let Some(session) = self.live.as_mut() {
            let delivered = next_turn(session, said).await;
            if delivered.is_err() {
                /* „Fresh conversation" wolno obiecać dopiero po dowodzie `Dead`. `Alive` po
                 * pełnej eskalacji nadal jest żywym, płatnym procesem: uchwyt wraca wtedy do
                 * rejestru zamiast wypaść z niego przy wyjściu z tej funkcji (niezmiennik 6). */
                if let Some(mut failed) = self.live.take() {
                    match failed.handle.cancel().await {
                        GroupProof::Dead { status } => {
                            finish_dead_session(failed, status.and_then(|status| status.code()))
                                .await;
                        }
                        GroupProof::Alive { .. } => {
                            self.live = Some(failed);
                            return Err(ChatError::StillRunning);
                        }
                    }
                }
            }
            delivered?;
        } else {
            /* Strumień KLONUJEMY przed `await`, i to nie jest kosmetyka. `begin` nie bierze
             * `&self`, bo `&Chat` nie jest `Send`: uchwyt sesji (`Box<dyn AgentHandle>`) jest
             * `Send`, ale nie `Sync`, a `&T: Send` wymaga `T: Sync`. Pożyczka `self` przeżywająca
             * `await` czyni całą komendę nie-`Send`, czego Tauri nie przyjmuje — i słusznie,
             * bo to zadanie może wznowić się na innym wątku. */
            let ready = begin(driver, spec_hard_wired(cwd, said)).await?;
            /* CZYTNIK JESZCZE NIE ISTNIEJE. Vendor przyjął prompt, więc wiersz jest uczciwy;
             * odpowiedź może ruszyć na ekran dopiero po nim. */
            let _ = self.say_in_the_stream(Line::Told {
                agent: LEAD.to_owned(),
                text: said.to_owned(),
            });
            self.live = Some(ready.listen(Arc::clone(&self.lines)));
            return Ok(());
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
    pub fn pointed_at(library: &Path, who: Option<&str>) -> Result<Self, ChatError> {
        // Pusty napis jest tym samym faktem, co brak wskazania: tak wygląda „człowiek jeszcze
        // nie wybierał" po przejściu przez okno (`src/sections/run/lead.ts` trzyma `''`).
        // Rozróżnianie ich tutaj dałoby drugie zdanie o jednym stanie.
        let who = who
            .map(str::trim)
            .filter(|who| !who.is_empty())
            .ok_or(ChatError::NobodyIsTheLead)?;

        // Przez `list_agents_inner`, nie przez własny spacer po katalogu: gdzie leżą agenci
        // i jak się czyta ich plik, wie `commands::agents` razem z `library::agents` (T-11).
        // Druga odpowiedź na „gdzie leży ten agent" jest tą, która przestanie się zgadzać
        // przy pierwszej zmianie reguły nazwy pliku (niezmiennik 23).
        let saved = super::agents::list_agents_inner(library)
            .map_err(|error| ChatError::CouldNotReadTheLibrary(error.to_string()))?;

        saved
            .into_iter()
            .find(|agent| agent.id.to_string() == who)
            // Nie „pierwszy z katalogu": lider, którego nikt nie wskazał, wygląda na ekranie
            // dokładnie jak wskazany, a odpowiada nie tym, czym miał.
            .map(|agent| Self { agent })
            .ok_or_else(|| ChatError::NoSuchLead(who.to_owned()))
    }

    /// Co temu liderowi wolno zrobić z plikami.
    ///
    /// TĄ SAMĄ tabelą `FileAccess` → [`Policy`], którą czyta bieg (`commands::run::policy_of`),
    /// nigdy drugą jej kopią (niezmiennik 23). Druga kopia tej tabeli to sposób, w jaki w repo
    /// źródłowym po cichu umarło skanowanie sekretów: obie wyglądają poprawnie, a podpięta jest
    /// zawsze starsza.
    ///
    /// # 2026-08-20 (T-63) — ZDANIE WYŻEJ ZNOWU JEST PRAWDĄ, I TO JEST CAŁA TREŚĆ AC-4
    ///
    /// Do tego dnia stała tu **druga, ręcznie napisana** kopia tamtej tabeli, bo tamta była
    /// prywatna, a T-60 nie posiadało `run.rs`. Kopia nie była zepsuta — oba dopasowania oddawały
    /// to samo, więc każda asercja o wartościach przechodziła dla obu. Rozjechać się mogła dokładnie
    /// jedna rzecz: **przecelowanie istniejącego ramienia** w jednym z dwóch miejsc, i tego nie
    /// widziało żadne sprawdzenie w tym repo. Lider, któremu wolno pisać, choć człowiek ustawił
    /// „look only", nie wygląda na awarię — wygląda na lidera, który zapisał plik.
    ///
    /// Dlatego tu nie ma ani jednego ramienia po dialu, i to jest **mierzone**, nie obiecane:
    /// `one_table_for_policy.rs` liczy pliki pod `src/`, w których to odwzorowanie jest zapisane,
    /// i wymaga dokładnie jednego.
    ///
    /// Tabela stoi przy dialu ([`crate::library::agents::policy_of`]), a nie w module biegu, i to
    /// nie jest wybór estetyczny: rozmowa **nie ma prawa** zależeć od `commands::run`, bo brak tej
    /// zależności jest jedynym mechanizmem, którym nie może zacząć biegu (`chat_never_starts_a_run`
    /// asertuje to na źródle tego pliku). Powód pełny stoi przy definicji tamtej funkcji.
    #[must_use]
    pub fn policy(&self) -> Policy {
        policy_of(self.agent.file_access)
    }

    /// Ile ten lider ma myśleć — **tą samą tabelą**, którą czyta krok biegu
    /// ([`crate::library::agents::effort_level`]).
    ///
    /// Ten sam powód, co przy [`Lead::policy`] linię wyżej: tabela stoi przy szczeblu, a nie
    /// w module biegu, bo rozmowa nie ma prawa zależeć od `commands::run`. Druga kopia, choćby
    /// dziś odpowiadała tak samo, rozjeżdża się w dniu, w którym ktoś przeceluje jedno ramię —
    /// i wtedy lider myśli inaczej niż krok tego samego agenta, a nic tego nie mówi.
    #[must_use]
    pub fn effort(&self) -> &'static str {
        effort_level(self.agent.thinking)
    }

    /// Prompt systemowy tego lidera: brief dopasowany do jego polityki **plus** jego instrukcje.
    ///
    /// Razem, nie zamiast: [`BRIEF`] mówi, czego lider nie umie zrobić (zaczynać biegów) i czym
    /// się to robi (`/run`), a instrukcje mówią, kim on jest. Lider bez pierwszego zdania obieca
    /// start, którego nie wykona; lider bez drugiego jest agentem, którego nikt nie konfigurował.
    ///
    /// Z briefu wymieniane jest DOKŁADNIE jedno zdanie — to o plikach ([`MAY_WRITE_DRAFTS`]) —
    /// bo dokładnie ono jedno zależy od dialu. Trzy osobne kopie całego promptu byłyby trzema
    /// miejscami, w których mieszka zdanie „biegów nie zaczynasz", i pierwszym, które by się
    /// rozjechało (niezmiennik 13).
    #[must_use]
    pub fn brief(&self) -> String {
        let about_files = match self.policy() {
            Policy::ReadOnly => LOOK_ONLY,
            Policy::EditInFolder => MAY_WRITE_DRAFTS_HERE,
            Policy::Unrestricted => MAY_WRITE_DRAFTS_ANYWHERE,
        };
        let brief = BRIEF.replace(MAY_WRITE_DRAFTS, about_files);
        // Puste `instructions` znaczą „nie mam zdania", a nie „dopisz pustkę": dwie puste linie
        // na końcu promptu systemowego to ten sam artefakt, którym `some_text` w biegu odmawia
        // być (`commands::run`).
        let says = self.agent.instructions.trim();
        if says.is_empty() {
            return brief;
        }
        format!("{brief}\n\n{says}")
    }
}

/// Wątki lidera: po jednym na TERMINAL, wszystkie w jednym miejscu.
///
/// # Po co to istnieje obok [`Chat`]
///
/// Bo [`Chat`] jest JEDNĄ rozmową i jego własny komentarz zapowiada ten dzień: „jedna na
/// aplikację, nie jedna na zakres — i to jest do przemyślenia, kiedy zakresy dostaną własne
/// sesje". Skutek tamtego stanu widział człowiek: `Chat::say` używa `cwd` **wyłącznie przy
/// zakładaniu sesji**, więc rozmowa o projekcie A, po przełączeniu na B, odpowiada dalej o A —
/// bez ani jednego zdania ostrzeżenia, z żywego procesu siedzącego w folderze sprzed
/// przełączenia.
///
/// # 2026-08-20 (T-71) — KLUCZEM JEST TERMINAL, NIE FOLDER
///
/// T-60 kluczowało zakresem, bo zakres był najdrobniejszą rzeczą, jaką okno umiało nazwać: karta
/// BYŁA folderem (`src/sections/run/tabs/store.ts`, „w jednym zakresie może stać najwyżej jedna
/// karta"). Od T-71 karta jest terminalem z własną tożsamością, więc dwie rozmowy w jednym
/// projekcie są zwykłym stanem — a rejestr kluczowany folderem oddaje im JEDEN wątek: człowiek
/// pisze w lewej karcie, a odpowiedź pojawia mu się w prawej.
///
/// **Rejestr jest jeden, dróg do niego dwie** (niezmiennik 13). Droga po folderze zostaje, bo
/// woła ją kryterium sprzed tego zadania, i nazywa wtedy DOMYŚLNY terminal tego zakresu
/// ([`key_of`]). Drugi rejestr obok byłby drugim domem dla odpowiedzi „gdzie mieszka ta rozmowa"
/// i rozjechałby się przy pierwszym zamknięciu okna: [`Threads::close`] widziałaby jeden z nich,
/// a drugi zostawiał żywe procesy pod PID 1.
///
/// Wpis w `ThreadRegistry::lines` powstaje, kiedy okno pierwszy raz na ten terminal patrzy; actor
/// w `ThreadRegistry::live` dopiero przy pierwszym zdaniu — sesja wystartowana przy montażu
/// ekranu płaci za turę, o którą nikt nie zapytał, i to jest ten sam powód, który stoi
/// przy [`Chat::live`].
#[derive(Default)]
pub struct Threads {
    /// Krótki rejestr, nigdy trzymany przez `await` (niezmiennik 8).
    ///
    /// Każdy żywy uchwyt należy do osobnego actora. Dzięki temu Codex czekający na koniec
    /// tury w terminalu A nie trzyma zamka aplikacji i nie zatrzymuje wiadomości ani Stopu
    /// terminalu B. `std::sync::Mutex` jest tutaj celowy: pod nim są wyłącznie lookup, clone
    /// i podmiana `LineSink`; cudzy kod oraz sterownik stoją po drugiej stronie kanału actora.
    state: Mutex<ThreadRegistry>,
}

#[derive(Default)]
struct ThreadRegistry {
    /// Kanał wierszy tego terminalu. Podmieniany przy każdym otwarciu ekranu, nigdy zamykany:
    /// zamknięcie cudzej rozmowy przy przełączeniu byłoby zgubieniem wątku, o który chodzi
    /// cała ta zmiana.
    lines: HashMap<String, Arc<Mutex<LineSink>>>,
    /// Actor tego terminalu. Tylko on posiada `Session`, więc `wait -> send` nie może zostać
    /// przeplecione drugą turą, a Stop może przerwać `wait` i przejść przez `cancel`.
    live: HashMap<String, Conversation>,
    /// Gdzie leży biblioteka tego człowieka — `~/.loadout`, powiedziane przez okno.
    ///
    /// JEDNA na wszystkie zakresy, a nie jedna na wątek, bo biblioteka jest globalna
    /// (`docs/ARCHITECTURE.md` §8): agenci i workflow są tym, co się przenosi między projektami,
    /// i to jest cały powód, dla którego leżą poza repo.
    ///
    /// `None` znaczy „okno jeszcze nie powiedziało" i daje liderowi dokładnie to, co miał przed
    /// tym zadaniem — sam folder zakresu. Ścieżka zgadnięta tutaj z `HOME` byłaby gorsza od braku:
    /// każdy test rozmawiałby wtedy z prawdziwą biblioteką człowieka (ten sam wybór i ten sam
    /// powód, co przy [`super::agents::list_agents_inner`] i przy `RunDeps::home`).
    library: Option<PathBuf>,
    /// Most tego terminalu — gniazdo, przez które lider sięga po czasowniki Loadouta.
    ///
    /// Jeden na terminal, nie jeden na aplikację: tożsamością wołającego jest samo gniazdo, więc
    /// dwie rozmowy w jednym projekcie nie mają jak odpowiedzieć sobie nawzajem na wywołanie.
    /// Powstaje przy PIERWSZYM zdaniu, razem z sesją, i z tego samego powodu — most założony przy
    /// montażu ekranu byłby gniazdem otwartym dla rozmowy, której nikt nie zaczął.
    ///
    /// `Arc`, bo tę samą wartość trzyma zadanie przyjmujące połączenia; porzucenie ostatniej
    /// kopii zamyka nasłuch i kasuje plik.
    bridges: HashMap<String, Arc<Bridge>>,
    /// Pytanie tego terminalu, które czeka na człowieka — najwyżej jedno naraz.
    ///
    /// Trzymane TUTAJ, a nie tylko w biurku, bo odpowiedź przychodzi z okna i musi kogoś znaleźć.
    /// Ten sam `Arc` widzi biurko, więc to jest jedno miejsce na jeden fakt (niezmiennik 13),
    /// oglądane z dwóch stron.
    waiting: HashMap<String, Arc<AskWaiting>>,
}

/// Sufit kolejki jednego terminalu. Kolejka porządkuje tury, nie uruchamia ich równolegle.
const THREAD_COMMANDS: usize = 16;
const THREAD_IDLE: u8 = 0;
const THREAD_ACTIVE: u8 = 1;
const THREAD_CLOSING: u8 = 2;
const THREAD_CLOSED: u8 = 3;

/// Klamka do actora jednego terminalu.
///
/// Dwa kanały są rozdzielone celowo. Gdy Codex czeka w `handle.wait()`, zwykłe tury zostają
/// w swojej kolejce, a Stop ma osobne pasmo i może przerwać oczekiwanie przez `tokio::select!`.
/// Jeden kanał FIFO stawiałby Stop za dowolną liczbą wiadomości, czyli człowiek naciskałby
/// Stop dokładnie wtedy, gdy nie ma on kiedy zadziałać.
#[derive(Clone)]
struct Conversation {
    inner: Arc<ConversationInner>,
}

struct ConversationInner {
    turns: mpsc::Sender<TurnRequest>,
    stops: mpsc::Sender<StopRequest>,
    /// Obserwowalny cień stanu actora; nie jest tokenem anulowania ani autorytetem procesu.
    ///
    /// Jedynym autorytetem śmierci pozostaje `GroupProof`. Atom służy wyłącznie temu, by
    /// synchroniczne `is_live_at` nie musiało zaglądać do uchwytu należącego do zadania.
    status: Arc<AtomicU8>,
}

struct TurnRequest {
    driver: Arc<dyn AgentDriver>,
    spec: RunSpec,
    text: String,
    images: ValidatedImages,
    done: oneshot::Sender<Result<(), ChatError>>,
}

struct StopRequest {
    done: oneshot::Sender<Option<GroupProof>>,
}

enum FollowUp {
    Delivered(Result<(), ChatError>),
    Stop(Option<StopRequest>),
}

impl Conversation {
    fn new(lines: Arc<Mutex<LineSink>>) -> Self {
        let (turns, turn_inbox) = mpsc::channel(THREAD_COMMANDS);
        let (stops, stop_inbox) = mpsc::channel(1);
        let status = Arc::new(AtomicU8::new(THREAD_IDLE));
        tokio::spawn(conversation_actor(
            turn_inbox,
            stop_inbox,
            Arc::clone(&status),
            lines,
        ));
        Self {
            inner: Arc::new(ConversationInner {
                turns,
                stops,
                status,
            }),
        }
    }

    fn is_live(&self) -> bool {
        matches!(
            self.inner.status.load(Ordering::Acquire),
            THREAD_ACTIVE | THREAD_CLOSING
        )
    }

    fn same_as(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    async fn say_with_images(
        &self,
        driver: Arc<dyn AgentDriver>,
        spec: RunSpec,
        text: String,
        images: ValidatedImages,
    ) -> Result<(), ChatError> {
        let (done, answer) = oneshot::channel();
        self.inner
            .turns
            .send(TurnRequest {
                driver,
                spec,
                text,
                images,
                done,
            })
            .await
            .map_err(|_| ChatError::StoppedListening)?;
        answer.await.unwrap_or(Err(ChatError::StoppedListening))
    }

    /// Wysyła Stop i oddaje odbiornik dowodu. Rozdzielenie wysłania od czekania pozwala
    /// [`Threads::close`] najpierw obudzić KAŻDEGO actora, a dopiero potem czekać na najwolniejszy.
    async fn ask_to_stop(&self) -> Result<oneshot::Receiver<Option<GroupProof>>, ()> {
        let (done, proof) = oneshot::channel();
        self.inner
            .stops
            .send(StopRequest { done })
            .await
            .map_err(|_| ())?;
        Ok(proof)
    }

    async fn stop(&self) -> Option<GroupProof> {
        if self.inner.status.load(Ordering::Acquire) == THREAD_CLOSED {
            return None;
        }
        let Ok(proof) = self.ask_to_stop().await else {
            /* Zerwany kanał bez otrzymanego `Dead` jest stanem nieznanym. Konserwatywne `Alive`
             * zachowuje wpis w rejestrze i nie zamienia utraty actora w fałszywy dowód śmierci.
             *
             * `group: None` jest tu prawdą, nie zaniedbaniem (2026-08-28): actor był jedynym,
             * kto trzymał `Supervised`, więc razem z jego kanałem znika adres grupy. To jest
             * dokładnie ten najgorszy stan, który wariant `Alive` ma umieć wypowiedzieć —
             * żyje i nie wiadomo, kogo pytać. */
            return Some(GroupProof::Alive { group: None });
        };
        proof
            .await
            .unwrap_or(Some(GroupProof::Alive { group: None }))
    }
}

/// Jedyny właściciel `Session` jednego terminalu.
async fn conversation_actor(
    mut turns: mpsc::Receiver<TurnRequest>,
    mut stops: mpsc::Receiver<StopRequest>,
    status: Arc<AtomicU8>,
    lines: Arc<Mutex<LineSink>>,
) {
    let mut session: Option<Session> = None;
    let mut closing = false;

    loop {
        if closing {
            serve_while_closing(&mut session, &mut turns, &mut stops, &status).await;
            return;
        }

        tokio::select! {
            biased;
            stop = stops.recv() => {
                let Some(stop) = stop else {
                    stop_orphan(session, &status).await;
                    return;
                };
                let proof = cancel_session(&mut session).await;
                let alive = matches!(proof, Some(GroupProof::Alive { .. }));
                status.store(if alive { THREAD_CLOSING } else { THREAD_CLOSED }, Ordering::Release);
                let _ = stop.done.send(proof);
                if alive {
                    closing = true;
                } else {
                    return;
                }
            }
            turn = turns.recv() => {
                let Some(turn) = turn else {
                    stop_orphan(session, &status).await;
                    return;
                };

                status.store(THREAD_ACTIVE, Ordering::Release);
                if session.is_none() {
                    match begin_thread(turn.driver, turn.spec, turn.images).await {
                        Ok(ready) => {
                            /* Najpierw przyjęta tura człowieka, dopiero potem zadanie, które
                             * może przepuścić odpowiedź. `start` mógł już zapełnić `inbox`, ale
                             * bez czytnika żaden z tych wierszy nie wyprzedzi `Told`. */
                            say_to_stream(&lines, &turn.text);
                            session = Some(ready.listen(Arc::clone(&lines)));
                            let _ = turn.done.send(Ok(()));
                        }
                        Err(error) => {
                            status.store(THREAD_IDLE, Ordering::Release);
                            let _ = turn.done.send(Err(error));
                        }
                    }
                    continue;
                }

                let Some(running) = session.as_mut() else {
                    status.store(THREAD_IDLE, Ordering::Release);
                    let _ = turn.done.send(Err(ChatError::StoppedListening));
                    continue;
                };
                let follow_up =
                    interruptible_next_turn(running, &turn.text, turn.images, &mut stops).await;
                match follow_up {
                    FollowUp::Delivered(Ok(())) => {
                        say_to_stream(&lines, &turn.text);
                        let _ = turn.done.send(Ok(()));
                    }
                    FollowUp::Delivered(Err(error)) => {
                        let proof = cancel_session(&mut session).await;
                        if matches!(proof, Some(GroupProof::Alive { .. })) {
                            status.store(THREAD_CLOSING, Ordering::Release);
                            closing = true;
                            let _ = turn.done.send(Err(ChatError::StillRunning));
                        } else {
                            status.store(THREAD_IDLE, Ordering::Release);
                            let reply = if matches!(&error, ChatError::CouldNotRecord) {
                                error
                            } else {
                                ChatError::StoppedListening
                            };
                            let _ = turn.done.send(Err(reply));
                        }
                    }
                    FollowUp::Stop(Some(stop)) => {
                        let proof = cancel_session(&mut session).await;
                        let alive = matches!(proof, Some(GroupProof::Alive { .. }));
                        status.store(if alive { THREAD_CLOSING } else { THREAD_CLOSED }, Ordering::Release);
                        let _ = turn.done.send(Err(if alive {
                            ChatError::StillRunning
                        } else {
                            ChatError::StoppedListening
                        }));
                        let _ = stop.done.send(proof);
                        if alive {
                            closing = true;
                        } else {
                            return;
                        }
                    }
                    FollowUp::Stop(None) => {
                        let _ = turn.done.send(Err(ChatError::StoppedListening));
                        stop_orphan(session, &status).await;
                        return;
                    }
                }
            }
        }
    }
}

/// Po `Alive` actor nie przyjmuje już tur, ale zachowuje uchwyt i obsługuje kolejne próby Stop.
async fn serve_while_closing(
    session: &mut Option<Session>,
    turns: &mut mpsc::Receiver<TurnRequest>,
    stops: &mut mpsc::Receiver<StopRequest>,
    status: &AtomicU8,
) {
    loop {
        tokio::select! {
            biased;
            stop = stops.recv() => {
                let Some(stop) = stop else {
                    stop_orphan(session.take(), status).await;
                    return;
                };
                let proof = cancel_session(session).await;
                let alive = matches!(proof, Some(GroupProof::Alive { .. }));
                status.store(if alive { THREAD_CLOSING } else { THREAD_CLOSED }, Ordering::Release);
                let _ = stop.done.send(proof);
                if !alive {
                    return;
                }
            }
            turn = turns.recv() => {
                let Some(turn) = turn else {
                    stop_orphan(session.take(), status).await;
                    return;
                };
                let _ = turn.done.send(Err(ChatError::StillRunning));
            }
        }
    }
}

/// Kolejna tura, z priorytetowym pasmem Stop podczas każdego długiego `await` sterownika.
async fn interruptible_next_turn(
    session: &mut Session,
    text: &str,
    images: ValidatedImages,
    stops: &mut mpsc::Receiver<StopRequest>,
) -> FollowUp {
    let number = session.attempts + 1;
    let evidence = session.evidence.clone();
    if let Some(evidence) = &evidence {
        let input = safe_input(text, &images);
        if evidence.begin_turn(number, &input).await.is_err() {
            evidence.mark_incomplete();
            return FollowUp::Delivered(Err(ChatError::CouldNotRecord));
        }
    }
    // Od tej chwili numer jest zajęty nawet wtedy, gdy transport odmówi. Następna próba nie
    // może nadpisać pliku, który mówi prawdę o tej odmowie.
    session.attempts = number;

    let delivered = if let Some(voice) = session.voice.clone() {
        if images.is_empty() {
            tokio::select! {
                biased;
                stop = stops.recv() => {
                    cancel_pending_attempt(evidence.as_ref(), number).await;
                    return FollowUp::Stop(stop);
                },
                sent = voice.send(ToAgent::Turn(text.to_owned())) => {
                    sent.map_err(|_| ChatError::StoppedListening)
                }
            }
        } else {
            tokio::select! {
                biased;
                stop = stops.recv() => {
                    cancel_pending_attempt(evidence.as_ref(), number).await;
                    return FollowUp::Stop(stop);
                },
                sent = session.handle.send_with_images(text.to_owned(), images) => {
                    sent.map_err(|_| ChatError::StoppedListening)
                }
            }
        }
    } else {
        let waited = tokio::select! {
            biased;
            stop = stops.recv() => {
                cancel_pending_attempt(evidence.as_ref(), number).await;
                return FollowUp::Stop(stop);
            },
            waited = session.handle.wait() => waited,
        };
        if waited.is_err() {
            fail_pending_attempt(
                evidence.as_ref(),
                number,
                EvidenceFailureKind::DeliveryFailed,
            )
            .await;
            return FollowUp::Delivered(Err(ChatError::StoppedListening));
        }
        tokio::select! {
            biased;
            stop = stops.recv() => {
                cancel_pending_attempt(evidence.as_ref(), number).await;
                return FollowUp::Stop(stop);
            },
            sent = session.handle.send_with_images(text.to_owned(), images) => {
                sent.map_err(|_| ChatError::StoppedListening)
            }
        }
    };
    if delivered.is_ok() {
        if let Some(evidence) = &evidence
            && evidence.accept_turn(number).await.is_err()
        {
            evidence.mark_incomplete();
            return FollowUp::Delivered(Err(ChatError::CouldNotRecord));
        }
    } else {
        fail_pending_attempt(
            evidence.as_ref(),
            number,
            EvidenceFailureKind::DeliveryFailed,
        )
        .await;
    }
    FollowUp::Delivered(delivered)
}

async fn fail_pending_attempt(
    evidence: Option<&EvidenceTarget>,
    number: usize,
    failure: EvidenceFailureKind,
) {
    if let Some(evidence) = evidence
        && evidence.fail_turn(number, failure).await.is_err()
    {
        evidence.mark_incomplete();
    }
}

async fn cancel_pending_attempt(evidence: Option<&EvidenceTarget>, number: usize) {
    fail_pending_attempt(evidence, number, EvidenceFailureKind::Cancelled).await;
}

/// Eskalacja jednego actora. `Dead` opróżnia slot; `Alive` zostawia uchwyt dokładnie tam,
/// gdzie był, żeby następny Stop miał do czego wrócić (niezmiennik 6).
async fn cancel_session(session: &mut Option<Session>) -> Option<GroupProof> {
    let running = session.as_mut()?;
    let proof = running.handle.cancel().await;
    if let GroupProof::Dead { status } = &proof
        && let Some(ended) = session.take()
    {
        finish_dead_session(
            ended,
            status.as_ref().and_then(std::process::ExitStatus::code),
        )
        .await;
    }
    Some(proof)
}

/// `Dead` kończy proces, nie koleję zdarzeń, które zdążył wysłać.
///
/// Najpierw znikają wszystkie nadajniki należące do sesji. Czytnik dostaje wtedy EOF, opróżnia
/// koleję i wykonuje `Curator::flush`. Abort jest wyłącznie bezpiecznikiem na wadliwy adapter,
/// który po dowodzie śmierci nadal trzyma klon nadajnika; po aborcie również odbieramy wynik
/// zadania, żeby nie zostawić porzuconego `JoinHandle`.
async fn finish_dead_session(ended: Session, exit_code: Option<i32>) {
    let Session {
        voice,
        handle,
        mut reader,
        evidence,
        attempts: _,
    } = ended;
    drop(voice);
    drop(handle);
    let drained = match tokio::time::timeout(READER_DRAIN, &mut reader).await {
        Ok(Ok(())) => true,
        Ok(Err(error)) => {
            tracing::warn!(%error, "the lead evidence reader did not finish cleanly");
            false
        }
        Err(_) => {
            reader.abort();
            let _aborted = reader.await;
            false
        }
    };
    if let Some(evidence) = evidence {
        if !drained {
            evidence.mark_incomplete();
        }
        if drained && let Err(error) = evidence.finish_conversation(exit_code, true).await {
            evidence.mark_incomplete();
            tracing::warn!(%error, "the lead conversation receipt stayed incomplete");
        }
    }
}

/// Ostatni `Conversation` zniknął bez jawnego Close (np. podczas gaszenia runtime).
///
/// `Alive` nie może wypuścić uchwytu ze scope. Actor ponawia pełną eskalację i pozostaje
/// jedynym właścicielem sesji tak długo, aż dostanie dowód `Dead`.
async fn stop_orphan(mut session: Option<Session>, status: &AtomicU8) {
    loop {
        match cancel_session(&mut session).await {
            None | Some(GroupProof::Dead { .. }) => {
                status.store(THREAD_CLOSED, Ordering::Release);
                return;
            }
            Some(GroupProof::Alive { .. }) => {
                status.store(THREAD_CLOSING, Ordering::Release);
                tracing::error!(
                    "a lead agent is still alive after losing its terminal; Loadout will retry \
                     stopping it"
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

fn say_to_stream(lines: &Arc<Mutex<LineSink>>, text: &str) {
    let sink = lines.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let _ = sink.send(Line::Told {
        agent: LEAD.to_owned(),
        text: text.to_owned(),
    });
}

/* RĘCZNIE, z tego samego powodu, co przy [`Chat`]: `Box<dyn AgentHandle>` nie jest `Debug`
 * i nie ma być. Pokazujemy dwie liczby, które cokolwiek znaczą w dzienniku — na ile terminali
 * okno patrzyło i ile wątków naprawdę stoi — plus biblioteką, bo `None` w tym polu jest jedynym
 * odróżnieniem lidera odciętego od plików, o których rozmawia, od lidera, który ich nie znalazł. */
impl std::fmt::Debug for Threads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        f.debug_struct("Threads")
            .field("watched", &state.lines.len())
            .field(
                "live",
                &state
                    .live
                    .values()
                    .filter(|thread| thread.is_live())
                    .count(),
            )
            .field("library", &state.library)
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
    ///
    /// Folder nazywa domyślny terminal tego zakresu ([`key_of`]): to jest ta sama czynność, co
    /// [`Threads::terminal_lines_go_to`], tylko zadana pytaniem „ten folder" zamiast „ten
    /// terminal" — i dosłownie tą drugą drogą wykonana, żeby dwa pytania nie mogły dostać dwóch
    /// odpowiedzi (niezmiennik 13).
    pub fn lines_go_to(&self, cwd: PathBuf, lines: LineSink) {
        let terminal = Terminal {
            id: key_of(&cwd),
            folder: cwd,
        };
        self.terminal_lines_go_to(&terminal, lines);
    }

    /// Wiersze tego terminalu idą odtąd tam — jedno ciało dla obu dróg wyżej.
    fn watch(&self, terminal: String, lines: LineSink) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        match state.lines.entry(terminal) {
            /* PODMIENIAMY ZAWARTOŚĆ UCHWYTU, nie sam wpis w mapie, i to jest cała naprawa
             * „wyjście na inną sekcję gubi rozmowę". Zadanie czytające trzyma ten `Arc` od chwili
             * startu wątku, więc wstawienie w to miejsce NOWEGO uchwytu zostawiłoby je piszące
             * w kanał, którego nikt już nie słucha — powód i pomiar stoją przy [`Chat::lines`]. */
            Entry::Occupied(open) => {
                *open.get().lock().unwrap_or_else(PoisonError::into_inner) = lines;
            }
            // Pierwszy raz na tym terminalu: sam widok, jeszcze bez wątku. Sesja wstaje przy
            // pierwszym zdaniu, bo tura wystartowana przy montażu ekranu jest turą, za którą
            // ktoś płaci, choć nikt o nic nie zapytał.
            Entry::Vacant(spot) => {
                spot.insert(Arc::new(Mutex::new(lines)));
            }
        }
    }

    /// Gdzie leży biblioteka tego człowieka — `~/.loadout` (`docs/ARCHITECTURE.md` §8).
    ///
    /// # Po co rozmowa w ogóle o tym wie
    ///
    /// Bo bez tego „przygotuję ci to" jest obietnicą bez pokrycia. Lider startuje w folderze
    /// zakresu i **wyłącznie** w nim, a twoje workflow i twoi agenci leżą poza nim. „Załóż mi
    /// agenta do recenzji" albo „popraw ten krok w workflow" kończy się wtedy instrukcją, jak
    /// zrobić to RĘCZNIE — czyli doradcą odciętym od jedynych plików, o których rozmawiacie.
    ///
    /// Katalogi, nie ich zawartość: biblioteka JEST plikami (niezmiennik 4), więc lider
    /// poprawiający workflow poprawia to samo, co czyta okno, i żaden stan pośredni nie jest do
    /// tego potrzebny.
    ///
    /// **Ścieżka przychodzi argumentem, nigdy z `HOME` czytanego w środku.** Katalog domowy
    /// odczytany tutaj znaczyłby, że każdy test rozmawia z prawdziwą biblioteką — ten sam wybór
    /// i ten sam powód, co przy [`super::agents::list_agents_inner`] i przy `RunDeps::home`.
    ///
    /// # Czego to NIE dosypuje, i to jest granica decyzji, nie przeoczenie
    ///
    /// Kroku biegu. Agent piszący kod w projekcie nie ma powodu przepisywać definicji innych
    /// agentów, a bieg czyta tę definicję RAZ, przy starcie kroku: nadpisana w trakcie nie
    /// przewraca niczego dzisiaj, więc awarii nie widać aż do NASTĘPNEGO biegu, kiedy „ten sam
    /// workflow" robi co innego. Wersja dosypująca katalogi wszystkim wygląda przy tym dokładnie
    /// tak samo jak ta — różnicę widać wyłącznie po stronie kroku.
    ///
    /// Sufit zostaje przy [`Lead::policy`] i tylko tam (niezmiennik 23): to zdanie mówi GDZIE,
    /// nie CO. Lider `look only` bibliotekę czyta — na tym polega cała wartość pytania „jakie mam
    /// workflow?" — a pisze dopiero ten, któremu człowiek dał wyżej.
    ///
    /// Wołane, kiedy okno wie, gdzie ta biblioteka leży, i dotyczy KAŻDEGO wątku tego okna —
    /// także tych, które już stoją. Wątek stojący ma jednak swój proces wystartowany, a `--add-dir`
    /// jedzie w argv przy starcie: rozmowa, która zaczęła się przed tym zdaniem, dostanie zasięg
    /// przy następnej. Wołanie z okna stoi więc przed pierwszym zdaniem (przy `open_chat`), a nie
    /// po nim.
    pub fn library_is(&self, library: PathBuf) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .library = Some(library);
    }

    /// Czy w tym zakresie stoi wątek.
    ///
    /// Pytanie zadawane o zakres, nie o aplikację: to na nim stoi asercja „sesja zakresu B żyje
    /// dalej, kiedy okno patrzy na A". Folder nazywa domyślny terminal tego zakresu ([`key_of`]).
    #[must_use]
    pub fn is_live_in(&self, cwd: &Path) -> bool {
        self.is_live_at(&key_of(cwd))
    }

    /// Mówi zdanie liderowi w TYM zakresie — pierwsze zdanie zakłada jego wątek, każde następne
    /// jest kolejną turą tego samego wątku.
    ///
    /// Sterownik wybiera **fabryka**, po vendorze z definicji lidera, i dlatego jedzie tu
    /// [`Drivers`], a nie gotowy sterownik: wybór po vendorze jest jedną z rzeczy, których to
    /// zadanie dowodzi, a wybór zrobiony u wołającego byłby wyborem, którego żaden test bez okna
    /// nie widzi.
    ///
    /// Folder nazywa domyślny terminal tego zakresu, więc to jest jedno wywołanie
    /// [`Threads::say_in`] i ani jednej decyzji obok.
    pub async fn say(
        &self,
        drivers: &Drivers,
        lead: &Lead,
        cwd: PathBuf,
        text: &str,
    ) -> Result<(), ChatError> {
        let terminal = Terminal {
            id: key_of(&cwd),
            folder: cwd,
        };
        self.say_in(drivers, lead, &terminal, text).await
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
    pub async fn close(&self) -> Vec<GroupProof> {
        let closing: Vec<(String, Conversation)> = self
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .live
            .iter()
            .map(|(terminal, thread)| (terminal.clone(), thread.clone()))
            .collect();

        /* STOP WYSŁANY DO KAŻDEGO PRZED CZEKANIEM NA PIERWSZY DOWÓD. Actor A może być
         * w wielosekundowej eskalacji; terminal B dostaje własny sygnał od razu, a nie dopiero
         * po jej końcu. To jest równoległość kontroli, nie tylko lista actorów. */
        let mut pending = Vec::with_capacity(closing.len());
        for (terminal, thread) in closing {
            let proof = thread.ask_to_stop().await.ok();
            pending.push((terminal, thread, proof));
        }

        let mut proofs = Vec::with_capacity(pending.len());
        for (terminal, thread, pending_proof) in pending {
            let proof = match pending_proof {
                Some(pending_proof) => pending_proof
                    .await
                    .unwrap_or(Some(GroupProof::Alive { group: None })),
                None => Some(GroupProof::Alive { group: None }),
            };
            let stopped = !matches!(proof, Some(GroupProof::Alive { .. }));
            if stopped {
                self.forget(&terminal, &thread);
            }
            if let Some(proof) = proof {
                proofs.push(proof);
            }
        }
        proofs
    }

    fn forget(&self, terminal: &str, thread: &Conversation) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state
            .live
            .get(terminal)
            .is_some_and(|current| current.same_as(thread))
        {
            state.live.remove(terminal);
            /* MOST SCHODZI RAZEM Z WĄTKIEM. Zostawiony, byłby gniazdem bez rozmowy — a plik,
             * pod którym nikt nie odpowiada, to most następnej sesji czekający w nieskończoność
             * na powitanie, które nie przyjdzie. */
            state.bridges.remove(terminal);
            /* Kanał odpowiedzi schodzi razem z wątkiem. Porzucony nadawca zamyka pytanie, które
             * na nim stało, więc lider dostaje zdanie zamiast tury wiszącej bez końca. */
            state.waiting.remove(terminal);
        }
    }
}

// ── TERMINAL, CZYLI JEDNOSTKA DROBNIEJSZA NIŻ ZAKRES ───────────────────────────────────────

/// Tożsamość DOMYŚLNEGO terminalu tego zakresu — czyli klucz rejestru, którym folder nazywa
/// sam siebie.
///
/// # Dlaczego rejestr jest kluczowany napisem, a nie `PathBuf`
///
/// Bo terminal, który wybiło okno, nie jest ścieżką (`src/sections/run/tabs/terminal.ts` oddaje
/// `terminal-1`) — a rejestr ma być JEDEN (niezmiennik 13). Napis jest jedynym typem, w którym
/// obie tożsamości mieszczą się bez odwzorowania między nimi; odwzorowanie byłoby drugą
/// odpowiedzią na pytanie „gdzie mieszka ta rozmowa".
///
/// `to_string_lossy` jest tu bezpieczne w jedną stronę, która ma znaczenie: obie drogi do tego
/// rejestru są nasze, a okno przysyła tę samą ścieżkę, którą tu widzimy, więc klucz zgadza się
/// sam z sobą. Prefiks `terminal-` po tamtej stronie nie zderzy się ze ścieżką bezwzględną
/// nigdy — ta zaczyna się od `/`.
fn key_of(folder: &Path) -> String {
    folder.to_string_lossy().into_owned()
}

/// Który terminal mówi i gdzie stoi.
///
/// # Dlaczego to jest para, a nie sam identyfikator
///
/// Bo zakres pracy zostaje tam, gdzie mieszkał — w magazynie zakresów po stronie okna — a terminal
/// go tylko NIESIE (niezmiennik 13). Wątek potrzebuje obu odpowiedzi naraz i w tej samej chwili:
/// `id` mówi, KTÓRA to rozmowa, `folder` mówi, GDZIE ona patrzy. Dwa osobne argumenty dawałyby
/// wywołanie, w którym da się podać tożsamość jednego terminalu z folderem drugiego, a to jest
/// dokładnie ta pomyłka, której nie widać na ekranie: lider odpowiada o innym projekcie.
///
/// # Co się zmieniło wobec [`Threads`] z T-60
///
/// Klucz. Wątek należał do ZAKRESU, bo zakres był najdrobniejszą rzeczą, jaką okno umiało nazwać —
/// karta była wtedy folderem (`src/sections/run/tabs/store.ts`, „w jednym zakresie może stać
/// najwyżej jedna karta"). Od T-71 karta jest terminalem z własną tożsamością, więc dwie rozmowy
/// w jednym projekcie są zwykłym stanem, a nie stanem, którego nie da się wyrazić.
#[derive(Debug, Clone)]
pub struct Terminal {
    /// Tożsamość terminalu, znak w znak ta, którą wybiło okno.
    pub id: String,
    /// Folder zakresu, w którym ten terminal stoi. Tu startuje sesja lidera i tylko tu patrzy.
    pub folder: PathBuf,
}

impl Threads {
    /// Okno patrzy na ten terminal: jego wiersze idą odtąd TAM, a wątek zostaje.
    ///
    /// Wołane przy każdym montażu ekranu pracy i przy każdym przeładowaniu okna, więc **nie może**
    /// niczego kończyć — powód i pomiar stoją przy [`Chat::lines_go_to`].
    ///
    /// # Jak to się ma do [`Threads::lines_go_to`]
    ///
    /// To jest ta sama czynność, tylko zadana pytaniem, na które da się odpowiedzieć: „ten
    /// terminal", a nie „ten folder". Droga po folderze zostaje, bo woła ją kryterium sprzed tego
    /// zadania, i ma zostać JEDNĄ DROGĄ do jednego rejestru — folder nazywa wtedy domyślny
    /// terminal tego zakresu. Drugi rejestr obok byłby drugim domem dla odpowiedzi „gdzie mieszka
    /// ta rozmowa" (niezmiennik 13) i rozjechałby się przy pierwszym zamknięciu okna.
    pub fn terminal_lines_go_to(&self, terminal: &Terminal, lines: LineSink) {
        self.watch(terminal.id.clone(), lines);
    }

    /// Czy w tym terminalu stoi wątek.
    ///
    /// Pytanie zadawane o terminal, nie o folder: to na nim stoi asercja „zamknięcie jednego
    /// terminalu zostawia drugi", której nie da się wypowiedzieć, dopóki oba mają jeden klucz.
    #[must_use]
    pub fn is_live_at(&self, terminal: &str) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .live
            .get(terminal)
            .is_some_and(Conversation::is_live)
    }

    /// Mówi zdanie liderowi w TYM terminalu — pierwsze zdanie zakłada jego wątek, każde następne
    /// jest kolejną turą tego samego wątku.
    ///
    /// Sterownik wybiera **fabryka**, po vendorze z definicji lidera, i dlatego jedzie tu
    /// [`Drivers`], a nie gotowy sterownik — powód w całości stoi przy [`Threads::say`].
    ///
    /// Powrót do terminalu, w którym rozmowa już stoi, jest kolejną turą i to jest cała różnica
    /// między „wątek na terminal" a „wątek na turę": implementacja startująca sesję przy każdym
    /// zdaniu płaci zimny start za każdym razem i gubi rozmowę, bo model nie słyszał poprzedniego
    /// zdania.
    pub async fn say_in(
        &self,
        drivers: &Drivers,
        lead: &Lead,
        terminal: &Terminal,
        text: &str,
    ) -> Result<(), ChatError> {
        self.say_in_with_images(drivers, lead, terminal, text, ValidatedImages::default())
            .await
    }

    pub async fn say_in_with_images(
        &self,
        drivers: &Drivers,
        lead: &Lead,
        terminal: &Terminal,
        text: &str,
        images: ValidatedImages,
    ) -> Result<(), ChatError> {
        let said = text.trim();
        if said.is_empty() && images.is_empty() {
            return Err(ChatError::NothingToSay);
        }

        let (lines, library) = {
            /* TEN ZAMEK KOŃCZY SIĘ PRZED PIERWSZYM `await` I PRZED PYTANIEM SYSTEMU PLIKÓW.
             * W środku są wyłącznie lookup i clone. Sterownik — w tym długi Codex `wait()` —
             * nigdy go nie widzi (niezmiennik 8). */
            let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            let lines = state
                .lines
                .get(&terminal.id)
                .map(Arc::clone)
                .ok_or(ChatError::NotWatchingThatFolder)?;
            (lines, state.library.clone())
        };
        let reaches = library
            .as_ref()
            .into_iter()
            .flat_map(|library| [AGENTS_DIR, WORKFLOWS_DIR].map(|name| library.join(name)))
            .filter(|folder| folder.is_dir())
            .collect();
        let thread = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            state
                .live
                .entry(terminal.id.clone())
                .or_insert_with(|| Conversation::new(Arc::clone(&lines)))
                .clone()
        };

        /* MOST TEGO TERMINALU — powstaje przy pierwszym zdaniu i żyje tak długo, jak wątek.
         *
         * PO CO: bez niego lider nie ma ŻADNEJ drogi do biblioteki człowieka. Vendor w trybie
         * bez terminala nie daje ani jednego narzędzia, którym dałoby się o nią zapytać
         * (zmierzone 2026-08-29, powód w całości stoi w nagłówku `crate::bridge`).
         *
         * ZAŁOŻENIE MOSTU NIE MOŻE ODMÓWIĆ ROZMOWY. Gniazdo, którego nie da się otworzyć, jest
         * powodem, by lider nie miał czasowników — nigdy powodem, by przestał rozmawiać.
         * Człowiek pyta wtedy o coś innego i dostaje odpowiedź, zamiast ściany. */
        let bridge = self.bridge_for(terminal, library.clone(), &lines).await;

        /* STEROWNIK WYBIERA FABRYKA, PO VENDORZE Z DEFINICJI. Zaszyty vendor nie znika przez
         * dołożenie odczytu definicji obok — zostaje jako gałąź domyślna. Tutaj nie ma ani
         * jednej gałęzi: actor dostaje jedną wartość z pliku i zachowuje kolejność tur. */
        let driver = as_the_step_is_configured(
            drivers(lead.agent.runs_with),
            lead,
            library.as_deref(),
            &terminal.folder,
            bridge.as_deref(),
        )?;
        /* PYTANIE ZADANE PRZED PRZEKAZANIEM STEROWNIKA, bo `say_with_images` bierze go przez
         * wartość. „Czy ten vendor umie zawężać listę" jest faktem o sterowniku i musi zostać
         * odczytane, póki jest co pytać. */
        let narrows = driver.narrows_its_tools();
        let told = spec_for(lead, terminal.folder.clone(), said, reaches, narrows);
        /* ZDANIE IDZIE NA EKRAN, ZANIM RUSZY TURA. Po niej byłoby uwagą o konfiguracji doklejoną
         * pod odpowiedzią, której ta konfiguracja dotyczyła — czyli w miejscu, w którym człowiek
         * już przestał jej szukać. */
        if let Some(problem) = told.said {
            let _ = lines
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .send(Line::Problem {
                    agent: LEAD.to_owned(),
                    text: problem,
                    resets_at: None,
                });
        }
        thread
            .say_with_images(driver, told.spec, said.to_owned(), images)
            .await
    }

    /// Most tego terminalu — jeden na wątek, zakładany przy pierwszym zdaniu.
    ///
    /// `None` znaczy „lider pracuje bez czasowników Loadouta", i jest to **poprawny stan**, nie
    /// awaria: gniazdo, którego nie dało się otworzyć, nie ma prawa uciszyć rozmowy. Człowiek
    /// dostaje wtedy lidera, który umie czytać pliki i rozmawiać, tylko nie widzi biblioteki —
    /// a to jest dokładnie tyle, ile miał przed tym zadaniem.
    ///
    /// # Dlaczego katalog tymczasowy, a nie `.loadout/` projektu
    ///
    /// Bo adres gniazda uniksowego mieści się na macOS w ~104 bajtach RAZEM ze ścieżką, a projekt
    /// bywa położony głęboko — `~/Projects/klient/monorepo/packages/…` wyczerpuje ten budżet bez
    /// ostrzeżenia i z błędem, który nie mówi o długości ani słowa. Gniazdo nie jest przy tym
    /// pracą człowieka: nie ma czego odtwarzać po restarcie, więc niezmiennik 4 go nie dotyczy.
    async fn bridge_for(
        &self,
        terminal: &Terminal,
        library: Option<PathBuf>,
        lines: &Arc<Mutex<LineSink>>,
    ) -> Option<Arc<Bridge>> {
        if let Some(standing) = self
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .bridges
            .get(&terminal.id)
        {
            return Some(Arc::clone(standing));
        }

        /* BIURKO WIDZI TEN SAM STRUMIEŃ, CO ROZMOWA. Bez tego `start_workflow` odmawia — i to
         * jest właściwe zachowanie, nie brak: bieg zaczęty bez śladu na ekranie jest dokładnie
         * tą awarią, przed którą stoi całe „rusza samo". */
        let waiting = Arc::new(AskWaiting::default());
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .waiting
            .insert(terminal.id.clone(), Arc::clone(&waiting));

        let desk = Arc::new(
            BridgeLibrary::at(library, terminal.folder.clone())
                .showing(Arc::clone(lines))
                .hearing(waiting),
        );
        let opened = Bridge::open(&std::env::temp_dir(), BridgeRole::Lead, desk)
            .await
            .map_err(|error| {
                tracing::warn!(%error, "the lead's bridge could not be opened; it talks without \
                                        Loadout's own verbs this time");
            })
            .ok()?;

        let bridge = Arc::new(opened);
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        /* WYŚCIG ROZSTRZYGA WPIS, KTÓRY JUŻ STOI. Dwa pierwsze zdania w jednym terminalu mogą
         * wejść tu naraz; przegrany most jest po prostu porzucany, a jego gniazdo znika razem
         * z nim. Nadpisanie cudzego wpisu zostawiłoby żywy nasłuch bez właściciela. */
        Some(Arc::clone(
            state.bridges.entry(terminal.id.clone()).or_insert(bridge),
        ))
    }

    /// Człowiek odpowiedział na pytanie lidera w tym terminalu.
    ///
    /// `false` znaczy „w tym terminalu nikt na nic nie czekał" — i to jest **odpowiedź, nie
    /// błąd**. Okno woła to przy KAŻDEJ odpowiedzi, także tej na kafelek kontrolny biegu, bo
    /// nie ma jak rozstrzygnąć, do kogo należy przypięte pytanie. Rozstrzyga to strona, która
    /// wie: podpis musi się zgadzać, inaczej cudze pytanie zostaje na miejscu.
    ///
    /// Bez tego rozróżnienia odpowiedź na punkt kontrolny odblokowywałaby przy okazji pytanie
    /// lidera — zdaniem, które go nie dotyczy, i w chwili, w której nikt tego nie widzi.
    pub fn answer_in(&self, terminal: &str, agent: &str, said: String) -> bool {
        let waiting = self
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .waiting
            .get(terminal)
            .map(Arc::clone);
        waiting.is_some_and(|waiting| waiting.answer(agent, said))
    }

    /// Człowiek zamknął ten terminal: jego wątek schodzi i oddaje dowód śmierci swojej grupy.
    ///
    /// `None`, kiedy w tym terminalu nie stała żadna rozmowa — nie ma wtedy czego dowodzić i nie
    /// jest to odmowa. Dowód, nie „wysłałem sygnał" (niezmiennik 6): rozmowa porzucona żywa
    /// przechodzi pod PID 1 i pracuje dalej (`recovery.rs`, nagłówek), a odzyskiwanie po niej nie
    /// posprząta, bo rozmowa nie ma wpisu w indeksie biegów. Osierocony agent pali limit w tle —
    /// to jest błąd finansowy, nie higieniczny.
    ///
    /// Kończy JEDEN wątek i milczy o pozostałych. Zamknięcie karty, w której nic nie chodzi, nie
    /// jest instrukcją o karcie obok — a przy jednym kluczu na folder byłoby nią zawsze.
    pub async fn close_at(&self, terminal: &str) -> Option<GroupProof> {
        let thread = self
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .live
            .get(terminal)
            .cloned()?;
        let proof = thread.stop().await;
        /* `Alive` ZOSTAJE W REJESTRZE razem z jedynym uchwytem. Dopiero `Dead` albo brak sesji
         * pozwala usunąć actora; inaczej kolejne Close nie miałoby już czego zatrzymać. */
        if !matches!(proof, Some(GroupProof::Alive { .. })) {
            self.forget(terminal, &thread);
        }
        proof
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
async fn read_along(
    mut inbox: mpsc::Receiver<DecodedEvent>,
    lines: Arc<Mutex<LineSink>>,
    evidence: Option<EvidenceTarget>,
) {
    /* KURATOR ROZMOWY, NIE BIEGU. Odpowiedź lidera zachowuje akapity i listy, którymi ją
     * napisał; strumień pracy zostaje przy jednej linii na zdanie (reguła 1). Powód w całości
     * stoi przy `Curator::talking` — skarga właściciela z 2026-08-23 dostała wtedy poprawkę
     * w CSS, a spłaszczanie działo się warstwę wcześniej, w kuratorze. */
    let mut curator = Curator::talking();
    let began = std::time::Instant::now();
    let mut attempt = 1_usize;
    while let Some(DecodedEvent { event, tool }) = inbox.recv().await {
        if let AgentEvent::Finished(outcome) = &event {
            if let Some(evidence) = &evidence {
                let counters = TurnCounters {
                    turns: u64::from(outcome.turns),
                    input_tokens: outcome.tokens.input,
                    output_tokens: outcome.tokens.output,
                    cached_tokens: outcome.tokens.cached,
                };
                let cancelled = outcome.reason == FinishReason::Cancelled;
                if evidence
                    .finish_turn(attempt, counters, outcome.ok, cancelled)
                    .await
                    .is_err()
                {
                    evidence.mark_incomplete();
                    tracing::warn!(attempt, "the Lead turn receipt could not be finalized");
                }
            }
            attempt = attempt.saturating_add(1);
        }
        let at_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
        let seen = Seen {
            agent: LEAD,
            at_ms,
            event: &event,
            tool: tool.as_ref(),
        };
        for line in curator.observe(seen) {
            /* JEDNO WYWOŁANIE, I TO ONO ODDZIELA ROZMOWĘ OD BIEGU. Propozycja jest własnością
             * TEJ pętli — wierszy biegu nikt tak nie pyta, bo krok, który napisze w prozie
             * `/run …`, dostałby przycisk startujący DRUGI bieg (powód w całości stoi przy
             * `engine::line::Line::Suggested`). Kuracja zostaje po tamtej stronie: ta linia nie
             * decyduje, który wiersz istnieje ani co mówi (niezmiennik 15). */
            let line = suggested(line, &event);
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

/// Sterownik rozmowy niosący to, co ten lider naprawdę dostaje — tym samym szwem, co krok biegu.
///
/// # 2026-08-24 (T-97) — TĄ SAMĄ DROGĄ JADĄ ZATWIERDZONE CONNECTIONS
///
/// Do tego dnia ta funkcja składała fragment **wyłącznie** ze szczebla „ile myśleć", więc lider,
/// którego agent ma `connections: ["x"]`, rozmawiał bez ani jednego serwera — u obu vendorów.
/// Człowiek zatwierdził połączenie w imporcie, ekran je pokazuje przy agencie, a rozmowa go nie
/// miała: „lider nie umie tego zrobić" jest z zewnątrz nieodróżnialne od „lider nie chciał".
///
/// # Kolejność jest ta sama, co w `Live::run_agent`, i jest WYMUSZONA
///
/// Connections → dziedziczenie → dowody. Każde opakowanie oddaje **klon** sterownika, więc
/// opakowanie założone wcześniej ginie, jeśli późniejsze klonuje sterownik sprzed niego.
/// Connections idą pierwsze, bo `configured` startuje od sterownika prosto z fabryki; dowody
/// ostatnie ([`begin_thread`]), bo tylko wtedy nadajnik dowodów siedzi na sterowniku, który
/// naprawdę pójdzie do `start_conversation`. Odwrócenie jest niewidoczne: wszystko się
/// kompiluje, rozmowa rusza, a znika albo `--mcp-config`, albo plik dowodu.
///
/// # Dlaczego to jest opakowanie, a nie pole w [`RunSpec`]
///
/// `RunSpec` nie ma `Default` i konstruuje go w tym drzewie ponad trzydzieści miejsc, więc nowe
/// pole w literale byłoby trzydziestoma plikami zmienionymi po to, żeby dowieźć jedną flagę.
/// `DriverConfiguration.arguments` jest już kanałem na „gotowy fragment argv tej jednej sesji" —
/// tędy jadą zatwierdzone Connections i tędy jedzie szczebel.
///
/// # Dwie odmowy o różnej wadze, i to jest cała treść typu zwrotnego
///
/// Nierozwiązane połączenie jest **odmową**: to jest zgoda człowieka wyrażona w imporcie, więc
/// rozmowa, która by go nie dostała, ma nie ruszyć — tak samo jak krok biegu. Sterownik bez szwu
/// `configured` odmową **nie jest**: oddaje siebie samego i to jest poprawna odpowiedź atrapy
/// spoza produkcyjnej fabryki. Odmowa rozmowy z powodu USTAWIENIA (szczebla, nie zgody) byłaby
/// liderem, który nie chce rozmawiać, bo ktoś przesunął suwak.
fn as_the_step_is_configured(
    driver: Arc<dyn AgentDriver>,
    lead: &Lead,
    library: Option<&Path>,
    folder: &Path,
    bridge: Option<&Bridge>,
) -> Result<Arc<dyn AgentDriver>, ChatError> {
    let mut configuration = connections_of(lead, library, folder, driver.id(), bridge)?;
    configuration
        .arguments
        .extend(driver.effort_argv(lead.effort()));
    if configuration.arguments.is_empty() {
        // Nic do niesienia i nie ma o co pytać: rozmowa bez połączeń i bez szczebla startuje
        // dokładnie tak, jak startowała — co do bajtu.
        return Ok(driver);
    }
    match driver.configured(&configuration) {
        Some(configured) => Ok(configured),
        // ZATWIERDZONE POŁĄCZENIE MA TYLKO TĘ JEDNĄ DROGĘ, więc vendor, który jej nie zna, nie
        // dostanie go wcale — i wtedy rozmowa nie rusza. Sam szczebel takiej wagi nie ma.
        None if !lead.agent.connections.is_empty() => Err(ChatError::CouldNotStart(
            "this agent app cannot use the approved Connections. Loadout stopped the \
             conversation instead of starting it without them."
                .to_owned(),
        )),
        None => Ok(driver),
    }
}

/// Katalog zatwierdzonych połączeń w bibliotece — ta sama nazwa, którą czyta bieg
/// (`commands::run`, `deps.home.join("connections")`).
const CONNECTIONS: &str = "connections";

/// Jedyny katalog w folderze człowieka, który należy do nas (`docs/ARCHITECTURE.md` §8).
const OURS: &str = ".loadout";

/// Fragment argv z zatwierdzonych Connections tego lidera — albo pusty, kiedy żadnych nie ma.
///
/// Katalog pliku serwerów stoi pod `<folder>/.loadout/`, czyli w JEDYNYM miejscu tego folderu,
/// które należy do nas (`docs/ARCHITECTURE.md` §8) — nigdy w drzewie człowieka. Po identyfikatorze
/// agenta, nie po terminalu: dwie karty tego samego lidera opisują ten sam zestaw serwerów, więc
/// drugi plik byłby drugą kopią jednego faktu.
///
/// Rozwiązywanie nazw i skład argv idą przez `connections::runtime`, tak samo jak w biegu
/// (niezmiennik 23): druga kopia którejkolwiek z tych dwóch rzeczy rozjechałaby się przy pierwszej
/// zmianie kształtu pliku i rozjechałaby się po cichu.
fn connections_of(
    lead: &Lead,
    library: Option<&Path>,
    folder: &Path,
    vendor: &str,
    bridge: Option<&Bridge>,
) -> Result<DriverConfiguration, ChatError> {
    /* MOST JEDZIE JAKO KOLEJNE POŁĄCZENIE, i to jest cała jego droga do argv.
     *
     * `connections::runtime::for_driver` pisze konfigurację obu vendorów i wypełnia
     * `DriverConfiguration::servers`, a z tego pola `mcp__<serwer>` trafia do `--allowedTools`
     * (`drivers/claude.rs`). Most podany tędy dostaje więc całą tę drogę bez zmiany ani jednej
     * linii w sterownikach — a to jest ta sama droga, którą zmierzyłem 2026-08-30 jako działającą
     * na żywym `claude 2.1.251`.
     *
     * Gniazdo, którego nie dało się otworzyć, po prostu nie dokłada połączenia. Rozmowa rusza
     * dalej, tylko bez czasowników. */
    let mut chosen: Vec<crate::connections::Connection> = Vec::new();
    if let Some(bridge) = bridge {
        match bridge.as_connection() {
            Ok(connection) => chosen.push(connection),
            Err(error) => {
                tracing::warn!(%error, "the bridge could not describe itself; the lead talks \
                                        without Loadout's own verbs this time");
            }
        }
    }

    if lead.agent.connections.is_empty() && chosen.is_empty() {
        return Ok(DriverConfiguration::default());
    }
    /* WYMÓG BIBLIOTEKI DOTYCZY WYŁĄCZNIE POŁĄCZEŃ, KTÓRYCH CHCIAŁ AGENT.
     *
     * Most jej nie potrzebuje — jest Loadoutem rozmawiającym sam ze sobą i zna swoją ścieżkę
     * z `current_exe()`. Warunek postawiony nad całą funkcją odmawiałby rozmowy KAŻDEMU liderowi
     * w oknie, które nie zdążyło jeszcze powiedzieć, gdzie leży biblioteka — czyli zamieniałby
     * dodanie czasowników w regresję zabierającą rozmowę. Zmierzone: 20 kryteriów sprzed tego
     * zadania na czerwono, wszystkie z tym jednym zdaniem. */
    if !lead.agent.connections.is_empty() {
        let Some(library) = library else {
            return Err(ChatError::CouldNotStart(
                "this agent needs Connections from your library, and Loadout does not know where \
                 your library is."
                    .to_owned(),
            ));
        };
        chosen.extend(
            crate::connections::runtime::selected(
                &library.join(CONNECTIONS),
                &lead.agent.connections,
            )
            .map_err(|error| ChatError::CouldNotStart(error.to_string()))?,
        );
    }

    let asked_for_connections = !lead.agent.connections.is_empty();
    crate::connections::runtime::for_driver(
        &folder
            .join(OURS)
            .join(CONNECTIONS)
            .join(lead.agent.id.to_string()),
        vendor,
        &chosen,
        |name| std::env::var_os(name),
    )
    .or_else(|error| {
        /* ODMOWA MA WAGĘ TEGO, CO PRZEZ NIĄ PRZEPADA, i to są dwie różne rzeczy.
         *
         * Zatwierdzone połączenie jest zgodą CZŁOWIEKA wyrażoną w imporcie, więc rozmowa, która
         * by go nie dostała, ma nie ruszyć — dokładnie jak krok biegu.
         *
         * Czasowniki Loadouta są czym innym: nikt o nie nie prosił z osobna, a lider bez nich
         * dalej umie czytać pliki i rozmawiać — czyli dostaje dokładnie tyle, ile miał przed tym
         * zadaniem. Uciszenie go tutaj zamieniłoby DODANIE możliwości w zabranie rozmowy,
         * a to jest najgorszy możliwy wynik dla człowieka, który po prostu chciał coś napisać. */
        if asked_for_connections {
            return Err(ChatError::CouldNotStart(error.to_string()));
        }
        tracing::warn!(
            %error,
            "this agent app cannot take Loadout's own verbs; the lead talks without them"
        );
        Ok(DriverConfiguration::default())
    })
}

/// Wolna funkcja, nie metoda, i powód jest twardy: `&Chat` nie jest `Send`, bo uchwyt sesji jest
/// `Send` ale nie `Sync`, a `&T: Send` wymaga `T: Sync`. Pożyczka `self` przeżywająca `await`
/// uczyniłaby całą komendę nie-`Send`, czego Tauri nie przyjmuje.
async fn begin(driver: &dyn AgentDriver, spec: RunSpec) -> Result<ReadySession, ChatError> {
    let (events, inbox) = mpsc::channel::<DecodedEvent>(EVENTS);
    let handle = driver
        .start(spec, events)
        .await
        .map_err(|error| ChatError::CouldNotStart(error.to_string()))?;
    /* Claude daje dwukierunkowy glos do jednego procesu. Codex swiadomie go nie daje: jedna
     * tura to jeden proces, a kolejna jedzie przez `AgentHandle::send` jako `exec resume`.
     * Brak glosu jest wiec zdolnoscia adaptera, nie dowodem martwej rozmowy. */
    let voice = handle.voice();
    Ok(ReadySession {
        voice,
        handle,
        inbox,
        evidence: None,
        attempts: 1,
    })
}

/// Produkcyjny start wątku Lead: prywatny receipt powstaje przed procesem, a sterownik dostaje
/// ten sam target niezależnie od vendora. Codex wybiera tu app-server przez `start_conversation`;
/// workflow dalej woła zwykłe `start` w `commands::run`.
async fn begin_thread(
    driver: Arc<dyn AgentDriver>,
    spec: RunSpec,
    images: ValidatedImages,
) -> Result<ReadySession, ChatError> {
    let conversation = Uuid::now_v7();
    let input = safe_input(&spec.prompt, &images);
    let evidence = EvidenceTarget::lead(&spec.cwd, conversation, input.clone());
    let vendor = conversation_vendor(driver.id());
    evidence
        .begin_conversation(ConversationMetadata {
            vendor,
            model_configured: spec
                .model
                .as_deref()
                .is_some_and(|model| !model.trim().is_empty()),
        })
        .await
        .map_err(|_error| ChatError::CouldNotRecord)?;
    evidence
        .begin_turn(1, &input)
        .await
        .map_err(|_error| ChatError::CouldNotRecord)?;

    let driver = match driver.with_evidence(evidence.clone()) {
        Some(driver) => driver,
        None if vendor != ConversationVendor::Unknown => {
            if evidence
                .fail_turn(1, EvidenceFailureKind::EvidenceIncomplete)
                .await
                .is_err()
            {
                evidence.mark_incomplete();
            }
            return Err(ChatError::CouldNotStart(
                "this agent app cannot preserve its private conversation evidence".to_owned(),
            ));
        }
        // Legacy test doubles do not expose raw vendor bytes. Produkcyjna fabryka nie ma
        // trzeciego ramienia, wiec ta furtka nie jest osiagalna z okna.
        None => driver,
    };
    let (events, inbox) = mpsc::channel::<DecodedEvent>(EVENTS);
    let mut handle = match driver.start_conversation(spec, images, events).await {
        Ok(handle) => handle,
        Err(error) => {
            if evidence
                .fail_turn(1, EvidenceFailureKind::StartFailed)
                .await
                .is_err()
            {
                evidence.mark_incomplete();
            }
            return Err(ChatError::CouldNotStart(error.to_string()));
        }
    };
    if evidence.accept_turn(1).await.is_err() {
        evidence.mark_incomplete();
        stop_unregistered_handle(handle.as_mut()).await;
        return Err(ChatError::CouldNotRecord);
    }
    let voice = handle.voice();
    Ok(ReadySession {
        voice,
        handle,
        inbox,
        evidence: Some(evidence),
        attempts: 1,
    })
}

/// Proces wystartował, ale nie wolno go jeszcze włożyć do rejestru: receipt pierwszej tury nie
/// potwierdził dostarczenia. `Alive` nie jest zgodą na porzucenie ostatniego uchwytu — ten scope
/// pozostaje właścicielem i ponawia pełną eskalację aż do prawdziwego `Dead` (niezmiennik 6).
async fn stop_unregistered_handle(handle: &mut dyn AgentHandle) {
    loop {
        match handle.cancel().await {
            GroupProof::Dead { .. } => return,
            GroupProof::Alive { .. } => {
                tracing::error!(
                    "an unregistered Lead process is still alive; Loadout will retry stopping it"
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

fn conversation_vendor(driver: &str) -> ConversationVendor {
    match driver {
        "claude" => ConversationVendor::Claude,
        "codex" => ConversationVendor::Codex,
        _ => ConversationVendor::Unknown,
    }
}

fn safe_input(text: &str, images: &ValidatedImages) -> SafeInputManifest {
    SafeInputManifest {
        prompt_bytes: text.len(),
        context: Vec::new(),
        images: images
            .as_slice()
            .iter()
            .map(|image| ImageFact {
                mime: image.mime().as_str().to_owned(),
                bytes: image.bytes().len(),
            })
            .collect(),
    }
}

/// Wysyla kolejna ture niezaleznie od ksztaltu procesu vendora.
///
/// Claude przyjmuje koperte od razu przez klonowalny [`Voice`]. Codex musi najpierw zebrac wynik
/// poprzedniego procesu, a potem startuje nowy przez [`AgentHandle::send`]. To rozgalezienie
/// nalezy do granicy sesji, nie do wyboru vendora: nowy adapter bez dwukierunkowego stdinu dostaje
/// ten sam poprawny kontrakt bez kolejnego `if vendor == ...` (niezmiennik 23).
async fn next_turn(session: &mut Session, text: &str) -> Result<(), ChatError> {
    if let Some(voice) = session.voice.as_ref() {
        return voice
            .send(ToAgent::Turn(text.to_owned()))
            .await
            .map_err(|_| ChatError::StoppedListening);
    }

    session
        .handle
        .wait()
        .await
        .map_err(|_| ChatError::StoppedListening)?;
    session
        .handle
        .send(text.to_owned())
        .await
        .map_err(|_| ChatError::StoppedListening)
}

/// Specyfikacja sesji **zaszytego** lidera — ta, którą startuje [`Chat`].
///
/// Sesja lidera plus zdanie, które przy jej składaniu trzeba było powiedzieć człowiekowi.
///
/// # Dlaczego para, a nie sam `RunSpec`
///
/// Bo „lider prosi o narzędzia ponad swoim dialem" jest faktem, który powstaje TUTAJ, a widać go
/// musi CZŁOWIEK (niezmiennik 29). Wersja bez tego pola miała dwa wyjścia i oba złe: odmówić
/// rozmowy (zmierzone: zabiera ją 18 z 29 agentów w bibliotece właściciela) albo przyciąć listę
/// po cichu (agent, któremu po cichu zabrano narzędzie, wygląda jak agent, który „nie umiał").
///
/// `None` jest normalnym stanem i znaczy „nie ma czego mówić".
#[derive(Debug)]
struct Told {
    /// Zdanie na ekran, albo `None`.
    said: Option<String>,
    /// Sesja, którą i tak zaczynamy.
    spec: RunSpec,
}

/// Wolna funkcja obok [`spec_for`], a nie trzecia gałąź w środku: te dwa zestawy wartości mają
/// dwóch różnych właścicieli. Tutaj właścicielem jest to źródło (stała [`BRIEF`], `None` na model,
/// jedna polityka), a tam — zapisana definicja agenta. Zlanie ich w jedną funkcję z warunkiem
/// dałoby dokładnie tę gałąź domyślną, której zniknięcia dowodzi AC-1.
fn spec_hard_wired(cwd: PathBuf, first: &str) -> RunSpec {
    RunSpec {
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
        /* BEZ SIECI, i to jest ta sama decyzja, co przy dialu obok: zaszyty lider nie ma
         * zapisanej definicji, w której człowiek mógłby to wybrać, więc jedyną uczciwą wartością
         * jest ta, o którą nikt nie prosił. Lider WSKAZANY bierze to ze swojej definicji
         * (`spec_for`). */
        reaches_the_web: false,
        policy: Policy::EditInFolder,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Specyfikacja sesji **wskazanego** lidera: cztery pola, wszystkie z jego zapisanej definicji.
///
/// To jest całe miejsce, w którym definicja agenta spotyka sesję, i dlatego jest jedno
/// (niezmiennik 13): `model` jedzie do [`RunSpec::model`] (do dziś było tam zawsze `None`, czyli
/// „co vendor ma domyślnie"), dial przechodzi przez [`Lead::policy`], a `instructions` doklejają
/// się do briefu w [`Lead::brief`]. Vendora nie ma w tej strukturze — on wybrał sterownik jedną
/// linią wyżej, u wołającego.
///
/// `reaches` przychodzi **argumentem**, a nie jest tu składane z katalogu domowego, i to jest ta
/// sama granica, którą pilnuje [`Threads::library`]: ta funkcja nie wie, gdzie leży biblioteka,
/// i nie ma prawa wiedzieć. Wersja czytająca `HOME` w środku znaczyłaby, że każdy test rozmawia
/// z prawdziwą biblioteką człowieka.
fn spec_for(
    lead: &Lead,
    cwd: PathBuf,
    first: &str,
    reaches: Vec<PathBuf>,
    narrows_its_tools: bool,
) -> Told {
    /* LISTA NARZĘDZI PRZECHODZI TĄ SAMĄ TABELĄ, CO KROK BIEGU (niezmiennik 23). Kolejność pytań
     * jest przepisana z `commands::run::what_this_step_may_use` co do jednego, bo to jest jedna
     * reguła zadana dwa razy — a nie dwie reguły o tym samym.
     *
     * Vendor, który nie umie zawężać, dostaje `None`: `--tools` nie istnieje w jego argv, więc
     * nie ma czego wyczyścić ani czego przekroczyć. */
    let mut say_out_loud = None;
    let tools = if narrows_its_tools {
        match &lead.agent.tools {
            /* „Wszystkie narzędzia" jedzie jako `None`, czyli „nie zawężaj" — to jest DOKŁADNIE
             * sufit polityki i dokładnie to argv, które lider dostawał przed tą zmianą. */
            Tools::Everything => None,
            Tools::Only(names) => {
                let surface = tool_surface(lead.policy(), Some(names));
                match surface.refused {
                    None => Some(surface.available),
                    /* LISTA PONAD DIALEM NIE ZABIERA ROZMOWY — ZMIERZONE, DLACZEGO NIE MOŻE.
                     *
                     * Pierwsza wersja tej gałęzi ODMAWIAŁA rozmowy, dla spójności z krokiem
                     * biegu. Sprawdzone 2026-08-30 na bibliotece właściciela: **18 z 29 agentów**
                     * ma listę ponad swoim dialem — sami `claude-code`, bo Codex nie zawęża
                     * w ogóle. Każdy z nich przestałby po tamtej wersji rozmawiać.
                     *
                     * Spójność z biegiem była pozorna: krok ma furtkę, której rozmowa nie ma —
                     * `AgentStep::overrides` podnosi dial na kafelku, więc ten sam agent biega
                     * poprawnie. Lider jest samą definicją i nie ma czym nadpisać.
                     *
                     * Asymetria ma przy tym powód, nie tylko pomiar. Bieg odmawia, bo startuje
                     * sześciu agentów bez nadzoru i kosztuje pieniądze od pierwszej sekundy;
                     * rozmowa to jedna tura z człowiekiem patrzącym na ekran. Odebranie mu
                     * rozmowy zabiera zarazem jedyne miejsce, w którym mógłby zapytać dlaczego.
                     *
                     * Cicho też nie jest: zdanie idzie NA EKRAN (niezmiennik 29) i nazywa OBA
                     * pola do poprawienia. Lider pracuje tymczasem z sufitem swojego dialu —
                     * czyli dokładnie tym, co miał przed tą zmianą. */
                    Some(refused) => {
                        say_out_loud = Some(no_such_tools(&lead.agent.name, &refused));
                        None
                    }
                }
            }
        }
    } else {
        None
    };

    Told {
        said: say_out_loud,
        spec: RunSpec {
            run_id: Uuid::now_v7(),
            cwd,
            prompt: first.to_owned(),
            /* Puste pole w definicji znaczy „nie mam zdania", a nie „ustaw pustkę" — ta sama reguła
             * i ten sam powód, co przy `some_text` w biegu. WPROST, a nie własną funkcją o tej samej
             * nazwie: druga `some_text` w drzewie czytałaby się jak rozjazd do wyśledzenia, a mamy tu
             * jedno miejsce wołania. `None` znaczy dla sterownika „to, co vendor ma domyślnie". */
            model: (!lead.agent.model.trim().is_empty()).then(|| lead.agent.model.clone()),
            system_append: Some(lead.brief()),
            policy: lead.policy(),
            /* Z DEFINICJI AGENTA, tą samą drogą co dial. Rozmowa z liderem do researchu, która nie
             * widzi świata, jest tą samą połową kontrolki, co krok biegu bez sieci. */
            reaches_the_web: lead.agent.reaches_the_web,
            /* 2026-08-30 — LISTA NARZĘDZI Z DEFINICJI, TĄ SAMĄ TABELĄ, CO KROK BIEGU.
             *
             * Do tego dnia stało tu `None`, a `Agent.tools` było dla lidera MARTWĄ KONTROLKĄ:
             * człowiek zawężał listę w formularzu, a rozmowa dostawała cały sufit swojej polityki.
             * Bieg brał ją z definicji od T-63; lider nie brał jej wcale.
             *
             * Wpięcia nie robiło jedno: `tool_surface` oddaje też `refused`, a `refused` nie miał
             * tu gdzie pojechać — ta funkcja zwracała `RunSpec`, nie `Result`. Kod nazywał to
             * zgłoszeniem i zostawiał człowiekowi decyzję: czy lider z listą ponad dialem ma
             * ODMÓWIĆ ROZMOWY.
             *
             * Właściciel rozstrzygnął ją 2026-08-30: „lidera traktujemy jak proces claude/codex…
             * chcę mieć elastyczność". Lider dostaje więc to, co stoi w jego definicji, a lista ponad
             * dialem nie odbiera rozmowy, tylko mówi to na ekranie ([`Told::said`]) —
             * bo przycięcie po cichu jest tą samą wadą, którą bieg nazwał już raz: agent, któremu po
             * cichu zabrano narzędzie, wygląda dokładnie jak agent, który „nie umiał". */
            tools,
            /* 2026-08-20 (T-70) — BIBLIOTEKA W ZASIĘGU ROZMOWY I **TYLKO** ROZMOWY.
             *
             * Do tego dnia stało tu `Vec::new()`, więc lider widział wyłącznie folder zakresu, a twoje
             * workflow i twoi agenci leżą poza nim (`~/.loadout`, `docs/ARCHITECTURE.md` §8). „Załóż
             * mi agenta do recenzji" kończyło się wtedy zdaniem, jak to zrobić RĘCZNIE — czyli doradcą
             * odciętym od jedynych plików, o których rozmawiacie.
             *
             * Czego tu NIE MA i to jest granica decyzji, nie przeoczenie: tej listy nie dostaje krok
             * biegu. Agent piszący kod w projekcie nie ma powodu przepisywać definicji innych agentów,
             * a bieg czyta tę definicję RAZ, przy starcie kroku — nadpisana w trakcie nie przewraca
             * dzisiaj niczego, więc awarii nie widać aż do NASTĘPNEGO biegu, kiedy „ten sam workflow"
             * robi co innego. Prawa kroku składa `commands::run::prompt_for` i po tej zmianie stoi
             * tam dokładnie to, co stało: katalog przekazań i nic ponad to.
             *
             * Sufit zostaje przy `policy` linię wyżej i tylko tam (niezmiennik 23): ta lista mówi
             * GDZIE lider patrzy, nie CO mu wolno. Lider `look only` bibliotekę czyta — na tym polega
             * cała wartość pytania „jakie mam workflow?" — a pisze dopiero ten, któremu człowiek dał
             * wyżej, i to samą tabelą dialu, bez ani jednej flagi dosypanej „żeby mogło działać". */
            extra_dirs: reaches,
            resume: None,
        },
    }
}
