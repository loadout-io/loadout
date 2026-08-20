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
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::Drivers;
use crate::engine::drivers::{
    AgentDriver, AgentHandle, DecodedEvent, Policy, RunSpec, ToAgent, Voice,
};
use crate::engine::line::{Curator, Line, Seen, suggested};
use crate::engine::supervisor::GroupProof;
use crate::ipc::LineSink;
use crate::library::agents::{Agent, policy_of};

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
    /// Sesja zeszła i nie przyjmuje już tur.
    StoppedListening,
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
            Self::StoppedListening => write!(
                f,
                "The lead agent stopped listening. Write again and it will start a fresh \
                 conversation."
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
            let session =
                begin(driver, spec_hard_wired(cwd, said), Arc::clone(&self.lines)).await?;
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
    pub fn lines_go_to(&mut self, cwd: PathBuf, lines: LineSink) {
        match self.lines.entry(cwd) {
            /* PODMIENIAMY ZAWARTOŚĆ UCHWYTU, nie sam wpis w mapie, i to jest cała naprawa
             * „wyjście na inną sekcję gubi rozmowę". Zadanie czytające trzyma ten `Arc` od chwili
             * startu wątku, więc wstawienie w to miejsce NOWEGO uchwytu zostawiłoby je piszące
             * w kanał, którego nikt już nie słucha — powód i pomiar stoją przy [`Chat::lines`]. */
            Entry::Occupied(open) => {
                *open.get().lock().unwrap_or_else(PoisonError::into_inner) = lines;
            }
            // Pierwszy raz na tym zakresie: sam widok, jeszcze bez wątku. Sesja wstaje przy
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
    /// # 2026-08-20 — SZKIELET T-70
    ///
    /// Ciało jest `todo!()`, żeby kryteria padały w czasie wykonania, a nie na kompilacji: test,
    /// który się nie zbudował, nie uruchomił niczego (`AGENTS.md` §2a p. 5). `clippy::todo = deny`
    /// w `Cargo.toml` pilnuje, żeby nie przeżyło do pełnej bramki, a podkreślenie przy nazwie
    /// parametru jest częścią tej samej tymczasowości — ciało, które go nie czyta, dawałoby
    /// `unused_variables`.
    pub fn library_is(&mut self, _library: PathBuf) {
        todo!("T-70: katalogi biblioteki mają dojechać do RunSpec każdej rozmowy tego okna")
    }

    /// Czy w tym zakresie stoi wątek.
    ///
    /// Pytanie zadawane o zakres, nie o aplikację: to na nim stoi asercja „sesja zakresu B żyje
    /// dalej, kiedy okno patrzy na A".
    #[must_use]
    pub fn is_live_in(&self, cwd: &Path) -> bool {
        self.live.contains_key(cwd)
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
        drivers: &Drivers,
        lead: &Lead,
        cwd: PathBuf,
        text: &str,
    ) -> Result<(), ChatError> {
        let said = text.trim();
        if said.is_empty() {
            return Err(ChatError::NothingToSay);
        }

        if let Some(thread) = self.live.get(&cwd) {
            /* WĄTEK TEGO ZAKRESU STOI: zdanie jest jego kolejną turą i jedzie głosem, bez `&mut`
             * na uchwycie. To ten punkt odróżnia „wątek na zakres" od „wątek na turę":
             * implementacja startująca proces na każde zdanie płaci zimny start za każdym razem
             * i gubi rozmowę, bo model nie słyszał poprzedniego zdania. */
            thread
                .voice
                .send(ToAgent::Turn(said.to_owned()))
                .await
                .map_err(|_| ChatError::StoppedListening)?;
        } else {
            /* Uchwyt strumienia KLONUJEMY przed `await` — powód (a `&Chat` nie jest `Send`) stoi
             * przy [`Chat::say`] i dotyczy tu tego samego uchwytu sesji. */
            let lines = self
                .lines
                .get(&cwd)
                .map(Arc::clone)
                .ok_or(ChatError::NotWatchingThatFolder)?;
            /* STEROWNIK WYBIERA FABRYKA, PO VENDORZE Z DEFINICJI. Zaszyty vendor nie znika przez
             * dołożenie odczytu definicji obok — zostaje jako gałąź domyślna, a gałąź domyślna
             * jest tym, czego konfiguracją nie da się wyłączyć. Tutaj nie ma ani jednej gałęzi:
             * jest jedna wartość z pliku i jedno wywołanie fabryki. */
            let driver = drivers(lead.agent.runs_with);
            let session = begin(driver.as_ref(), spec_for(lead, cwd.clone(), said), lines).await?;
            self.live.insert(cwd.clone(), session);
        }

        /* TWOJE ZDANIE W STRUMIENIU TEGO ZAKRESU. Wynik świadomie porzucony z tego samego powodu,
         * co w [`Chat::say`]: pełna kolejka do okna jest stanem normalnym, a zdanie i tak POSZŁO. */
        let _ = self.say_in_the_stream(
            &cwd,
            Line::Told {
                agent: LEAD.to_owned(),
                text: said.to_owned(),
            },
        );
        Ok(())
    }

    /// Wpisuje wiersz do strumienia TEGO zakresu; `false`, kiedy nie dojechał.
    ///
    /// Klon POD zamkiem, wysyłka NAD nim — ten sam zabieg i ten sam powód, co przy
    /// [`Chat::say_in_the_stream`].
    fn say_in_the_stream(&self, cwd: &Path, line: Line) -> bool {
        let Some(sink) = self
            .lines
            .get(cwd)
            .map(|open| open.lock().unwrap_or_else(PoisonError::into_inner).clone())
        else {
            return false;
        };
        sink.send(line) == crate::ipc::Sent::Queued
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
        /* ZDJĘTE Z MAPY PRZED PIERWSZYM `await`, i to nie jest kosmetyka: `Drain` trzymany przez
         * całą eskalację zabijania pożyczałby mapę mutowalnie przez sekundy, a `is_live_in`
         * pytane w tym czasie odpowiadałoby o wątkach, które już schodzą. Po tej linii nie ma
         * ani jednego wątku, o którym to okno jeszcze wie. */
        let closing: Vec<(PathBuf, Session)> = self.live.drain().collect();
        let mut proofs = Vec::with_capacity(closing.len());
        for (_, mut session) in closing {
            /* `cancel`, nie `close`, i to jest wymóg niezmiennika 6: `close` oddaje KOD WYJŚCIA,
             * a nie dowód, więc „zamknięte" znaczyłoby wtedy „wysłałem sygnał". Łaska nie ginie —
             * trzystopniowa eskalacja (przerwanie w paśmie, SIGTERM, SIGKILL) siedzi w środku
             * `cancel` u sterownika, razem z powodem, dla którego nie wolno jej skracać. */
            proofs.push(session.handle.cancel().await);
            /* Zadanie czytające kończy się na zamkniętym kanale zdarzeń, ale porzucony
             * `JoinHandle` zostawiłby zadanie, o którym nikt nie wie — jak w [`Chat::close`]. */
            session.reader.abort();
        }
        proofs
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

/// Startuje sesję rozmowy z pierwszym zdaniem człowieka jako pierwszą turą.
///
/// Wolna funkcja, nie metoda, i powód jest twardy: `&Chat` nie jest `Send`, bo uchwyt sesji jest
/// `Send` ale nie `Sync`, a `&T: Send` wymaga `T: Sync`. Pożyczka `self` przeżywająca `await`
/// uczyniłaby całą komendę nie-`Send`, czego Tauri nie przyjmuje.
async fn begin(
    driver: &dyn AgentDriver,
    spec: RunSpec,
    lines: Arc<Mutex<LineSink>>,
) -> Result<Session, ChatError> {
    let (events, inbox) = mpsc::channel::<DecodedEvent>(EVENTS);
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

/// Specyfikacja sesji **zaszytego** lidera — ta, którą startuje [`Chat`].
///
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
fn spec_for(lead: &Lead, cwd: PathBuf, first: &str) -> RunSpec {
    RunSpec {
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
        /* 2026-08-20 (T-63) — LISTA NARZĘDZI LIDERA WCIĄŻ TU NIE DOJEŻDŻA I JEST TO ZGŁOSZENIE,
         * NIE PRZEOCZENIE. Bieg kroku bierze ją od tego dnia z definicji agenta
         * (`commands::run::what_this_step_may_use`), więc `Agent.tools` przestało być martwą
         * kontrolką TAM — a tutaj nie: lider z zawężoną listą dostaje dalej cały sufit swojej
         * polityki, dokładnie jak przed tym zadaniem.
         *
         * Czego brakuje, dosłownie: `claude::tool_surface(lead.policy(), …)` oddaje też `refused`,
         * a `refused` nie ma tu gdzie pojechać — ta funkcja zwraca `RunSpec`, nie `Result`.
         * Przycięcie listy po cichu jest wykluczone (to najdroższa wersja tej wady: agent, któremu
         * po cichu zabrano narzędzie, wygląda jak agent, który „nie umiał"), więc wpięcie znaczy
         * nowy wariant [`ChatError`] i sygnatura `Result<RunSpec, ChatError>`. Oba pliki są
         * w bloku OWNS tego zadania, więc to nie jest bariera techniczna — decyzja jest
         * produktowa i należy do człowieka: czy lider z listą ponad swoim dialem ma ODMÓWIĆ
         * ROZMOWY. Odmowa startu biegu i odmowa rozmowy nie ważą tyle samo, a żadne kryterium
         * T-63 tego nie sądzi. */
        tools: None,
        extra_dirs: reaches,
        resume: None,
    }
}
