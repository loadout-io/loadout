//! Uruchom, zatrzymaj, wznów. Trzy funkcje domykające pętlę *płótno → plik → silnik → linie*.
//!
//! Nic tutaj nie jest nową zdolnością. Wszystko jest już zbudowane osobno: planista dowiódł
//! równoległości (T-02), nadzór dowiódł śmierci grupy procesów (T-03), walidator dowiódł odmów
//! (T-12), płótno dowiodło zapisu (T-13). Ten plik jest jedynym miejscem, w którym widać, czy
//! te rzeczy do siebie pasują — i dlatego cicha porażka wygląda tu inaczej niż gdziekolwiek
//! indziej: **wszystko działa osobno, a bieg i tak idzie sekwencyjnie**, bo liczba „ile naraz"
//! z UI jest wczytywana, logowana i nigdzie nie podawana. Semafor dostaje `1`, każdy test
//! przechodzi, bo wszyscy agenci naprawdę skończyli, i dokładnie tak przegrał poprzedni prototyp
//! (`docs/handoff.md:144-165`, niezmiennik 11).
//!
//! # Kolejność, której nie wolno odwrócić
//!
//! `docs/ARCHITECTURE.md` §4, czytane od góry: **wczytaj plik → sprawdź go jeszcze raz → dopiero
//! potem cokolwiek utwórz.** Bieg nie ufa UI (T3 §5.2): plik mógł zostać zmergowany gitem między
//! zapisem a naciśnięciem Start, więc odmowa pada **przed** katalogiem biegu i przed pierwszym
//! procesem. Implementacja, która najpierw tworzy katalog i odpala krok, a waliduje po drodze,
//! pali pieniądze na workflow odrzuconym pięć sekund później i zostawia po sobie pusty
//! `runs/<ts>__<id>/`.
//!
//! # Cztery pułapki, każda o jedną linijkę tańsza od wersji poprawnej
//!
//! 1. **`tokio::time::timeout(dur, step)` wokół kroku.** Wygląda na limit czasu i anuluje
//!    **zadanie Rusta, nie proces systemowy** (niezmienniki 6 i 10). Zostawia żywego agenta
//!    palącego limit u dostawcy. Każda ścieżka anulowania przechodzi przez `AgentHandle::cancel`,
//!    bo tylko ona wraca z `GroupProof`, a nie z „wysłałem sygnał".
//! 2. **`Err(Cancelled)`.** Anulowanie jest wartością (niezmiennik 7, [`Outcome::Cancelled`]).
//!    Krok po anulowaniu jest `cancelled`, jego potomkowie też — **nie `skipped`**, bo `skipped`
//!    znaczy „ktoś wyżej padł" i UI kłamałoby o powodzie (`docs/ARCHITECTURE.md` §5). Jedyny
//!    wyjątek to agent, którego śmierci nie udało się dowieść po skończonej serii Stopu: cały
//!    bieg dalej jest anulowany, ale krok jest `failed` z adresem grupy i jawnym powodem.
//! 3. **Instrukcje kroku w argv.** Prompt jedzie wyłącznie stdinem (niezmiennik 9); ta warstwa
//!    nie skleja komendy i nie zna ani jednej flagi vendora — wkłada instrukcje do
//!    `RunSpec::prompt` jako dane i oddaje je sterownikowi.
//! 4. **Referencja zamiast migawki.** `run.json` zapisuje konfigurację **efektywną**
//!    (`library::agents::resolve`) zamrożoną w chwili startu [T4 §5.2 p. 3]. Migawka będąca
//!    referencją zostawia pytanie „dlaczego zeszłotygodniowy bieg zachował się inaczej" bez
//!    odpowiedzi po każdej edycji szablonu [T4 §10, ryzyko 1].
//!
//! # `run.json` — kształt, który czytają dwa zadania
//!
//! Plik leży w `<projekt>/.loadout/runs/<ts>__<id>/run.json` i jest **prawdą** o biegu;
//! `loadout.db` jest jego indeksem i wolno go skasować (niezmiennik 4). Klucze biegu i kroków
//! są dokładnie tymi, które czyta `store::rebuild` — rozjazd znaczy, że po skasowaniu bazy
//! dostaje się co innego, niż się miało. Do tego dwa klucze, których wymaga T-15:
//!
//! ```json
//! {
//!   "id": "…uuid v7…",
//!   "workflow_id": "ship-a-feature",
//!   "workflow_hash": "…",          // ← „czy to był ten sam plan?"
//!   "workflow_snapshot": { … },    // graf JAK BIEGŁ
//!   "title": "Ship a feature",
//!   "status": "running | paused | succeeded | failed | cancelled",
//!   "concurrency": 3,
//!   "steps": [
//!     {
//!       "id": "…uuid v7…", "node_key": "build", "name": "Build", "agent": "claude",
//!       "depends_on": ["plan"], "status": "succeeded", "attempt": 0,
//!       "effective": { "id": "…uuid agenta…", "model": "opus", "thinking": "deep", … }
//!     }
//!   ]
//! }
//! ```
//!
//! `effective` jest **dosłowną** serializacją `library::agents::Agent` po złożeniu nadpisań
//! kroku, więc jego klucze są w camelCase — to jest migawka cudzego kształtu, nie nasz schemat.
//! `status` biegu jest jedynym miejscem, w którym istnieje `paused`: to jest stan **biegu**,
//! nigdy kroku (`docs/ARCHITECTURE.md` §5, to usuwa całą ćwiartkę stanów).
//!
//! # Kto tu z kim rozmawia
//!
//! ```text
//! run_workflow_inner ─ plan_run ─ workflow::{load, check} ─ library::agents::resolve
//!         │                            odmowa pada TUTAJ, przed pierwszym katalogiem
//!         ▼
//!   katalog biegu + run.json          ← plik istnieje, zanim ruszy pierwszy krok
//!         │
//!         ▼
//!   scheduler::execute(graf, token)              ← zależnościami rządzi graf…
//!         │
//!         ├─ krok agenta:   limits::Run::dispatch → miejsce ze WSPÓLNEJ puli aplikacji
//!         │                 (niezmiennik 11: „ile naraz" jest liczbą APLIKACJI, nie biegu)
//!         │                 AgentDriver::start → AgentEvent → Curator → Vec<Line>
//!         └─ krok kontrolny: status biegu = paused, czekaj na „dalej"
//!         │
//!         ▼
//!   run.json (stany końcowe od planisty) → store::rebuild_from   ← indeks Z PLIKÓW
//! ```
//!
//! Ostatnia strzałka jest tu z rozmysłem i jest całym niezmiennikiem 4: do bazy nie idzie ani
//! jedna wartość, której nie ma w katalogu biegu, bo baza powstaje **z tego katalogu**. Wersja
//! zapisująca do bazy po drodze wygląda tak samo przez trzy tygodnie — do pierwszego skasowania
//! `loadout.db`.
//!
//! # Wynik kroku → przekazanie → prompt następnego
//!
//! Szew, dla którego istnieje T-32, i cała jego treść mieści się w dwóch zdaniach. Po udanej
//! turze wynik kroku ląduje w `handoffs/` ([`Live::hand_over`]); prompt kroku, który po nim
//! idzie, niesie **ścieżkę** tego pliku ([`Live::prompt_for`]) i nigdy jego treść.
//! Front-matter składa Loadout, ciałem jest dosłownie to, co oddał agent (`ARCHITECTURE` §8).
//!
//! **Indeks, nie transkrypt** (D6 punkt 5). Wklejenie ciała do promptu jest o linijkę tańsze
//! i w pierwszym biegu wygląda lepiej: krok dostaje wszystko, czego mógłby chcieć, i nie musi
//! otwierać ani jednego pliku. Płaci za to każdy krok po nim — przy czwartym prompt niesie trzy
//! poprzednie tury w całości i jest większy niż praca. [T6 §10.2] każe dostarczać „belt and
//! braces", czyli ciało **i** ścieżkę; to jest świadome odejście od tamtego akapitu, nie
//! przeoczenie, i jest jedynym miejscem, w którym te dwa dokumenty się nie zgadzają.
//!
//! Skoro ścieżka jest jedyną drogą do treści, to musi **działać**: katalog przekazań jedzie do
//! sterownika w `RunSpec::extra_dirs`, bo krok `fresh-copy` stoi w `work/<krok>` i bez tego
//! dostałby odnośnik, którego nie wolno mu otworzyć — czyli kontrolkę bez handlera
//! (niezmiennik 16).
//!
//! Kolejność wpisów bierze się **z grafu**, nigdy z chwili zakończenia: dwa biegi tego samego
//! workflow mają dać ten sam prompt, a to, który agent odpowiedział szybciej, zmienia się
//! z biegu na bieg.
//!
//! **Runda pętli nie jest krokiem, który zaczyna od zera** (2026-08-23, T-87). „Krok przede mną"
//! liczone samą strzałką daje rundzie k+1 jedno zdanie krytyki i nic poza tym — ani planu, od
//! którego pętla ruszyła, ani własnej odpowiedzi, którą ta runda ma poprawić. Dlatego indeks
//! rundy niesie też wejście pętli, wszystkie jej wcześniejsze próby i wszystkie wcześniejsze
//! werdykty sędziego, a każdy wiersz mówi, CZYM jest plik, który wskazuje ([`Live::handed_before`],
//! [`WhatItIs`]). Symetrycznie w drugą stronę: krok ZA pętlą dostaje ostatnie **wyprodukowane**
//! przekazanie każdego kroku jej ciała, bo strzałka na zewnątrz wychodzi z rundy ostatniej —
//! a ta, przy pętli, która przeszła wcześniej, nie biegnie wcale.
//!
//! **Wznowienie niesie przekazania biegu, od którego ruszyło** (2026-08-23, T-88). Skopiowanie
//! plików do katalogu nowego biegu ([`seed_the_handoffs`]) nie jest przekazaniem ich dalej:
//! indeks powstaje z [`Live::handoffs`], czyli z tego, co oddały kroki TEGO biegu, a wycinek
//! zostawia tylko strzałki z obydwoma końcami w środku — więc krok na czele wycinka nie miał ani
//! jednego poprzednika i indeksu nie dostawał wcale. Co przejmuje który krok, rozstrzyga
//! [`what_the_run_before_left`]; wiersz indeksu mówi wprost, że plik jest z tamtego biegu, a pełne
//! teksty odłożone obok (`attachments/`) jadą razem z plikami, które je wołają.
//!
//! # Czego ta warstwa świadomie NIE robi
//!
//! - **Sama nie ogląda surowego strumienia.** `AgentDriver` oddaje już zdarzenie neutralne, więc
//!   surowych bajtów ta warstwa nie widzi ani jednego. NIE ZNACZY TO, ŻE NIKT ICH NIE ZAPISUJE:
//!   od T-34 `logs/agent-<id>.jsonl` powstaje w każdym biegu i pisze go [`crate::evidence`],
//!   któremu ta warstwa daje wyłącznie katalog biegu i identyfikator kroku
//!   ([`Live::evidence_for_agent`]). Do 2026-08-23 stało tu zdanie odwrotne — „katalog `logs/`
//!   powstaje, ale nikt tam nie pisze" — i było nieprawdą w każdym biegu właściciela, czyli
//!   uczyło następnego pisarza szukać szwu, który już istnieje.
//! - Kopiuje pliki projektu przy `fresh-copy` (T-33) — patrz [`copy_project_into`].
//!
//! # Dwie gałęzie schodzą się w JEDNEJ kopii (2026-08-29)
//!
//! Do tego dnia krok „to samo drzewo, co krok przede mną", przed którym stały dwa kroki pracujące
//! w RÓŻNYCH drzewach, był odmową: *„…the steps before it work in 2 different folders"*. Dwie
//! równoległe gałęzie dało się więc narysować i nie dało się na nich pracować — a to jest cały
//! kształt „front i backend osobno, potem ktoś to składa". Od teraz taki krok dostaje **własną,
//! nową kopię**, do której bieg znosi zmiany plikowe WSZYSTKICH rodziców ([`fan_in`]), i dopiero
//! na niej pracuje sterownik.
//!
//! **Składanie porównuje PLIKI, nie różnicę gita**, i to nie jest wygoda implementacji. Plik,
//! o którym git nie wie — `docs/added.txt` w katalogu, którego wcześniej nie było — nie ma
//! w różnicy żadnej reprezentacji, a jest najczęstszym kształtem pracy agenta. Bazą porównania
//! jest **nietknięta kopia składana**: powstaje tym samym przepisem, co kopie rodziców, więc
//! „różni się od bazy" znaczy dokładnie „ten krok to zmienił".
//!
//! **Cicha wygrana jednej strony jest gorsza od zatrzymanego kroku.** Kiedy dwoje rodziców
//! napisało w jednym pliku różne bajty, „ostatni wygrywa" zależy od tego, który agent skończył
//! szybciej — czyli krok poniżej dostawałby kod, którego nikt nie napisał, i kończył się
//! sukcesem. Dlatego niezgoda zatrzymuje krok PRZED sterownikiem ([`Live::fold_what_came_before`]),
//! nie wpisuje do kopii ani jednego bajtu, i zostawia obie kopie rodziców tam, gdzie są.
//! Znacznik konfliktu w pliku byłby tą samą wadą o klasę gorzej: agent pracowałby na nim
//! jak na kodzie.
//!
//! # Kopie kroku są węzłami grafu (2026-08-23, T-90)
//!
//! Do tego dnia stało tu zdanie odwrotne — „krok z `copies: 3` biegnie tu jako jedna sesja" —
//! i było prawdą: człowiek ustawiał liczbę w wierszu „How many at once", walidator pilnował
//! zakresu, plik ją zapisywał, a robota wykonywała się raz. Zmierzone na biegu właściciela: dwa
//! kroki po `copies: 2` dały **22** kroki zamiast 28.
//!
//! Rozwinięcie robi [`crate::workflow::unroll`], tą samą drogą, którą rozwija rundy pętli: graf,
//! który stąd schodzi do planisty, jest dalej bez cykli i ma tylko więcej węzłów. Trzy kopie
//! dzielą KLUCZ KAFELKA — okno rysuje jedną kartę, bo człowiek narysował jeden kafelek — a różnią
//! się kluczem węzła, katalogiem roboczym i podpisem („Build (2 of 3)"). `RunReport::steps` ma
//! od teraz jeden wpis na WĘZEŁ, nie na krok pliku.

mod neighbours;
mod protection;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::fan_in;
use super::history::NotAsked;
use super::isolate;
use super::processes;
use super::settings;
use super::triggers::{self, DeliveryState, TriggerClaim, TriggerDelivery, TriggerOrigin};
use super::{Outcome, Part, RunControl, RunDeps, RunError, RunReport, RunRequest};
use crate::durable_file::{DEFINITION_FILE_MODE, DurableFilePublisher, ModePolicy, PublishError};
use crate::engine::StepId;
use crate::engine::dag::Dag;
use crate::engine::drivers::claude::tool_surface;
use crate::engine::drivers::command::{
    CheckHow, CheckSpec, Checking, CommandDriver, GIVE_UP_AFTER,
};
use crate::engine::drivers::prices::{Prices, WHERE_PRICES_LIVE};
use crate::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DidNotLetGo, DriverConfiguration,
    DriverSetupError, DriverSetupFailure, FinishReason, LoadedFromTheFolder,
    Outcome as DriverOutcome, Policy, RunSpec, THE_MODEL_WITH_NO_NAME, ToAgent,
};
use crate::engine::limits::{self, Limiter};
use crate::engine::line::{Action, Curator, Line, Seen, Status, Tool};
use crate::engine::scheduler;
use crate::engine::step::{StepReport, StepState};
use crate::engine::supervisor::{
    self, GroupId, GroupProof, KeepsLeftovers, Leftover, MissingProgram, PublicationIdentity,
    StepTag,
};
use crate::evidence::{ContextKind, ContextSource, EvidenceTarget, SafeInputManifest};
use crate::inherit::rewrite;
use crate::inherit::wire::{self, BorrowedConcern, Chosen, Inherited, InheritedSourceKind};
use crate::ipc::LineSink;
use crate::library::agents::{
    Agent, Overrides, Thinking, Tools, effort_level, read_agent_directory, resolve,
};
use crate::memory::handoff::{self, Kind, MetaDraft, fields_said_in};
use crate::skills::place::material_of;
use crate::skills::{Moved, NotAsItWas, StepSkills};
use crate::workflow::check::{Level, Note, check_to_run};
use crate::workflow::file::load;
use crate::workflow::unroll::{self, Unrolled};
use crate::workflow::{
    AgentStep, Borrow, CheckOutcome, ConditionalLink, Folder, Handover, HandoverField, Point,
    RouteEvidence, Skills, Step, WhenItFails, WorkflowFile,
};

/// Biblioteka agentów pod katalogiem domowym Loadouta (`docs/ARCHITECTURE.md` §8).
const AGENTS_DIR: &str = "agents";

/// Katalog projektowy, w którym mieszkają biegi.
const PROJECT_DIR: &str = ".loadout";

/// Katalog biegów pod [`PROJECT_DIR`].
const RUNS_DIR: &str = "runs";

/// Katalog, pod którym stają katalogi pluginu z umiejętnościami **kroków** — po jednym na krok.
///
/// PO JEDNYM NA KROK, bo zbiór umiejętności jest własnością kroku, nie biegu: trzy kroki jednego
/// agenta mogą mieć trzy różne zbiory, a jeden wspólny katalog dałby każdemu z nich sumę
/// wszystkich — czyli odznaczenie na kroku przestałoby cokolwiek znaczyć.
///
/// **Pod katalogiem biegu**, bo katalog pluginu jest wyjściem builda (niezmiennik 4) i ma zniknąć
/// razem z biegiem. `$TMPDIR` zostawiałby artefakt biegu poza biegiem (`docs/ARCHITECTURE.md` §8),
/// a katalog roboczy kroku bywa folderem człowieka.
const STEP_SKILLS_DIR: &str = "skills";

/// Katalog, pod którym stoi to, co **kroki** pożyczyły z repozytorium gospodarza — po jednym
/// podkatalogu na krok.
///
/// PO JEDNYM NA KROK z dokładnie tego samego powodu, co przy [`STEP_SKILLS_DIR`]: wybór
/// „pożycz to z tego repozytorium" jest własnością kafelka, a jeden wspólny katalog dałby
/// każdemu krokowi sumę wszystkich wyborów — czyli odznaczenie na kafelku przestałoby cokolwiek
/// znaczyć. Do 2026-08-23 stał tu jeden katalog `plugin/` na cały bieg i było to bez znaczenia
/// tylko dlatego, że wybór był zawsze pusty.
const BORROWED_DIR: &str = "borrowed";

/// WF-13: brak manifestu pozostaje jawnym brakiem, nie pustą listą. Recorded sprawdza,
/// czy wcześniejszy krok oczekiwał skilli; nigdy nie zastępuje None dzisiejszą biblioteką.
#[derive(Debug)]
pub struct SavedSkillBundles {
    pub agent: Option<Vec<crate::skills::bundle::ResolvedSkill>>,
    pub borrowed: Option<Vec<crate::skills::bundle::ResolvedSkill>>,
}

pub fn saved_skill_bundles(run_dir: &Path, node_key: &str) -> io::Result<SavedSkillBundles> {
    let mut components = Path::new(node_key).components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(io::Error::other(
            "The saved skill step address is not valid.",
        ));
    }
    let root = crate::engine::supervisor::PublicationRoot::open(run_dir)?;
    let read =
        |relative: PathBuf| -> io::Result<Option<Vec<crate::skills::bundle::ResolvedSkill>>> {
            match root.entry_identity(&relative) {
                Ok(None) => Ok(None),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Ok(Some((crate::engine::supervisor::PublicationEntryKind::Directory, _))) => {
                    crate::skills::bundle::read_delivered(&run_dir.join(relative)).map(Some)
                }
                Ok(Some(_)) => Err(io::Error::other(
                    "The saved skill folder is no longer a real directory.",
                )),
                Err(error) => Err(error),
            }
        };
    let agent = read(Path::new(STEP_SKILLS_DIR).join(node_key))?;
    let borrowed = read(Path::new(BORROWED_DIR).join(node_key).join("plugin"))?;
    root.validate_path_identity(run_dir)?;
    Ok(SavedSkillBundles { agent, borrowed })
}

/// Opis biegu: bieg, jego kroki i migawki. To jest **prawda** (niezmiennik 4).
const RUN_FILE: &str = "run.json";

/// Bezpieczne zdanie pokazywane i zapisywane, gdy jeden żywy agent powtarza typowaną porażkę.
///
/// Cel i surowy wynik są celowo pominięte: pochodzą z obcego procesu i mogą zawierać prywatne
/// ścieżki albo wartości. Są dowodem do porównania w pamięci, nie tekstem interfejsu.
pub const REPEATED_TOOL_FAILURE_SENTENCE: &str = "The same tool failed with the same error three times, so Loadout stopped this step. Fix the tool or its setup, then run the step again.";

/// Zdanie dla człowieka o turze, którą sufit wydatku przerwał w połowie.
///
/// 2026-09 (Z-13b) — KWOTA JEST W NIM Z ROZMYSŁEM. Bez niej to zdanie jest nie do odróżnienia
/// od każdej innej porażki kroku, a człowiek ma się dowiedzieć, ILE temu krokowi wolno było
/// wydać: to ta liczba mówi mu, czy podnieść sufit, czy podzielić pracę na mniej kroków naraz.
fn over_its_share_sentence(allowed: f64) -> String {
    format!(
        "Stopped: this step reached the ${allowed:.2} it was allowed to spend, so Loadout ended \
         it before it cost more."
    )
}

const REPEATED_TOOL_FAILURE_LIMIT: u8 = 3;

/// Nazwa, pod którą `run.json` powstaje przed przemianowaniem.
///
/// Zapis jest dwustopniowy, bo ten plik czyta ktoś inny **w trakcie** biegu: UI odpytuje o stan,
/// a punkt kontrolny ogłasza pauzę właśnie nim. `fs::write` prosto na `run.json` ma okno, w którym
/// plik jest przycięty do zera — czytelnik dostaje wtedy „to nie jest JSON" i nie ma jak odróżnić
/// tego od uszkodzenia.
const RUN_FILE_WRITING: &str = "run.json.writing";

/// Surowe strumienie agentów, po jednym pliku na krok (`logs/agent-<id>.jsonl`).
const LOGS_DIR: &str = "logs";

/// Podsumowanie rundy, której bieg nie potrzebował.
///
/// Zdanie, nie słowo: ląduje w `run.json` obok stanu `succeeded` i jest jedynym miejscem, które
/// mówi, dlaczego ten krok nie ma ani logu, ani przekazania, ani kosztu. Bez niego historia biegu
/// twierdziłaby, że agent pracował i nic nie powiedział.
const NOT_NEEDED: &str = "Not needed: the work already passed in an earlier try.";

/// Podsumowanie rundy pętli, której ciało w tej rundzie NIE WYKONAŁO SIĘ ANI RAZU.
///
/// Inne zdanie niż [`NOT_NEEDED`], bo inny fakt: tamto mówi „praca już przeszła", to mówi
/// „nie było czego sądzić". Człowiek czytający `run.json` musi je odróżnić, bo drugie znaczy,
/// że sufit wydatku albo cudza porażka zatrzymały implementera przed pierwszym słowem.
const NOTHING_RAN: &str = "Nothing to check: the step before this one never ran.";

/// Katalog, pod którym powstają własne kopie plików dla kroków `fresh-copy`.
const WORK_DIR: &str = "work";

/// Katalog, pod którym leży auto-pamięć kroków TEGO biegu: `<katalog biegu>/mem/<krok>`.
///
/// 2026-08-23 (T-92) — powstał po to, żeby auto-pamięć Claude Code przestała pisać do
/// `~/.claude/projects/<projekt>/memory/`, czyli do katalogu, który człowiek dzieli ze swoimi
/// sesjami interaktywnymi [T6 §10.4]. Pod katalogiem biegu, bo to jest **wyjście biegu**
/// (niezmiennik 4) i ma dać się przeczytać po fakcie razem z resztą jego historii.
///
/// `pub`, bo tę nazwę musi znać także kryterium, które sprawdza, gdzie krok naprawdę pisał —
/// a literał `"mem"` powtórzony w teście jest drugim miejscem, w którym mieszka ta odpowiedź.
pub const STEP_MEMORY_DIR: &str = "mem";

/// Model tury refleksji. **Jedna stała i ani jedno miejsce więcej.**
///
/// Refleksja jest tania z założenia [T6 §5.3: „jedna tania refleksja po biegu"], a „tania"
/// przestaje być prawdą dokładnie w chwili, w której model wybiera wołający: dwa wołania z dwoma
/// modelami to dwa różne rachunki za tę samą rzecz, a różnicy nie widać nigdzie poza fakturą.
/// Model biegu tu nie wchodzi z tego samego powodu — bieg opusem nie ma powodu myśleć opusem
/// o tym, czego się nauczył.
pub const REFLECTION_MODEL: &str = "haiku";

/// Ile minut wolno turze refleksji. Jedna stała, z dokładnie tego samego powodu.
///
/// Krótko, bo to jest jedno pytanie o skończony bieg, a nie praca: tura, która myśli dziesięć
/// minut nad tym, czego się nauczyła, kosztuje więcej niż krok, który to zrobił.
pub const REFLECTION_MINUTES: u64 = 2;

/// PODŁOGA sufitu jednej prywatnej tury Loadouta.
///
/// Nie pochodna budżetu workflow: refleksja jest osobnym, krótkim pytaniem Loadouta, więc nie
/// dziedziczy ani braku limitu, ani reszty limitu ustawionego dla kroków. Tyle dostaje bieg,
/// który sam nic nie kosztował — i tyle wystarczy na przeczytanie jednego przekazania.
///
/// 2026-09 (Z-38) — DO TEGO DNIA BYŁA TO CAŁA ODPOWIEDŹ, ta sama dla każdego biegu. Zmierzone
/// na biegu meetnotes z 2026-09-04: dwie godziny, 57,52 USD, dziewięć przekazań, a tura dostała
/// osiem centów i zeszła na nich po 22 sekundach z `Reached maximum budget ($0.08)`. Sufit
/// liczy dziś [`what_this_run_taught_us`] jako jeden procent tego, co kosztowały kroki —
/// nigdy mniej niż ta liczba i nigdy więcej niż [`REFLECTION_BUDGET_CAP_USD`].
pub const REFLECTION_BUDGET_USD: f64 = 0.08;

/// I twardy sufit tego samego, ile by bieg nie kosztował.
///
/// 2026-09 (Z-38) — SUFIT JEST TU DLATEGO, ŻE PROCENT NIE MA GÓRNEJ GRANICY. Jeden procent
/// z biegu za tysiąc dolarów to dziesięć dolarów na jedno pytanie o to, czego się nauczył —
/// czyli tura droższa niż większość kroków, które streszcza. Refleksja ma zostać tania
/// [T6 §5.3], a „tania" przestaje być prawdą po cichu: widać to wyłącznie na fakturze.
pub const REFLECTION_BUDGET_CAP_USD: f64 = 1.00;

/// Ile kandydatek wolno zostawić po jednym biegu [T6 §5.3: „najwyżej trzy"].
///
/// Sufit JEST całym mechanizmem antyrozrostowym tego podsystemu. Pisarz, który bierze tyle, ile
/// przyszło, zamienia sekcję w listę, której nikt nie czyta — a lista, której nikt nie czyta,
/// czyni z bramki promocji rytuał [T6 §5.1]. Nieobsługiwana akrecja instrukcji jest samą
/// chorobą, nie objawem (arXiv 2608.11095 na 1867 repozytoriach).
const AT_MOST_KEPT: usize = 3;

/// Ile znaków reguły wchodzi do tytułu, czyli do NAZWY PLIKU notatki.
///
/// Tytuł powstaje z reguły, bo model nie pisze osobnego: `record_candidate_for` robi z tytułu
/// slug, a slug jest tożsamością notatki — więc to samo zdanie zgłoszone w dwóch biegach musi
/// trafić w ten sam plik i tylko tak `occurrences` ma się z czego wziąć.
///
/// Sufit jest tu, bo reguła przychodzi od modelu, a nazwa pliku ma twardy limit systemowy
/// (255 bajtów na macOS). Cięcie idzie po granicy słowa i jest deterministyczne: cięcie
/// w środku znaku wielobajtowego panikuje, a cięcie zależne od czegokolwiek poza samą regułą
/// dawałoby dwa pliki dla jednego zdania.
const TITLE_CAP: usize = 120;

/// O co pytamy w jedynej turze refleksji.
///
/// **Kształt odpowiedzi jest w prośbie**, bo to jedyne miejsce, w którym da się go poprosić:
/// para `rule:` / `because:` na osobnych wierszach jest tym, co czyta [`worth_remembering`].
/// Zdanie o `because` stoi tu wprost, bo bez niego para jest odrzucana po cichu i model nie ma
/// jak się dowiedzieć, czego zabrakło („no because, no memory" [T6 §10.3]).
///
/// Prośba mówi też, czego NIE chcemy: podsumowania biegu. Refleksja, która streszcza to, co się
/// przed chwilą stało, produkuje zdania prawdziwe wyłącznie o tym jednym biegu — a notatka jedzie
/// do promptów, których jeszcze nie ma.
///
/// 2026-09 (Z-38) — ZESZŁO STĄD ZDANIE „handoffs/ holds what each step passed on". Kazało
/// modelowi SZUKAĆ tego, co Loadout ma wprost pod ręką: listę zbudował potem
/// [`what_this_run_left_behind`] i dokleja ją tuż za tym akapitem, tym samym otwarciem, którym
/// dostaje ją każdy krok ([`HANDOFF_INDEX_OPENS`]). Zmierzone na biegu z 2026-09-04: sześć tur
/// spędzonych na chodzeniu po katalogu i budżet wydany przed odpowiedzią.
const REFLECTION_ASK: &str = "\
This run has finished. Its directory is your working directory, and run.json holds what happened.

Name at most three things worth remembering for the next run in this project. Fewer is better, \
and none at all is a good answer when nothing here was surprising.

Write each one as exactly two lines, and nothing else around them:

rule: <one sentence, true beyond this run, that a later agent should act on>
because: <where you saw it in this run>

A rule without a `because:` line is dropped, so leave out anything you cannot ground. Do not \
summarise the run, do not describe the workflow, and do not change any file.";

/// Początek zdania o turze, która nie doszła do odpowiedzi.
///
/// 2026-09 (Z-38) — TYMI SAMYMI SŁOWAMI NAZYWA SIĘ KONTROLKA, którą człowiek to włączył
/// (`src/sections/run/reflection/toggle.tsx`, `REFLECTION_LABEL`). Zdanie zaczynające się
/// inaczej — „the private turn", „reflection" — opisuje coś, czego na żadnym ekranie nie ma
/// (niezmiennik 14), a człowiek ma poznać rzecz, którą sam zaznaczył.
///
/// Napis jest po obu stronach granicy: tutaj składa go bieg, który właśnie zszedł, a w panelu
/// historii ten sam bieg otwarty tydzień później (`said.ts`). To nie są dwa żywe regiony jednego
/// faktu (niezmiennik 13) — to jedno zdanie o jednym biegu, którego nigdy nie widać dwa razy
/// naraz: strumień znika razem z biegiem, panel istnieje dopiero po nim.
const REFLECTION_DID_NOT_FINISH: &str = "Learn from this run didn't finish";

/// Trwałe granice kompletności kopii, poza samymi worktree i ich roboczym diffem.
const ISOLATION_MARKERS_DIR: &str = ".isolation";

/// Ile zdarzeń sterownika mieści się w kanale, zanim ten zaczeka.
///
/// Kanał **ograniczony**, nigdy `unbounded_channel`: agent, który mówi szybciej, niż kurator
/// nadąża, ma zaczekać, a nie rosnąć w pamięci do końca biegu.
const EVENT_QUEUE: usize = 256;

/// Ile znaków przepisujemy z ostatniej wypowiedzi agenta do jednolinijkowego podsumowania kroku.
const SUMMARY_LIMIT: usize = 240;

/// Ile znaków ma tytuł przekazania.
///
/// Tytuł jest **jednym wierszem** płaskiego front-mattera (`memory::handoff`), a instrukcja kroku
/// bywa akapitem: `title:` na dwieście znaków czyta się jak plik, który ktoś uszkodził.
const TITLE_LIMIT: usize = 120;

/// Zdanie, po którym w prompcie zaczyna się indeks przekazań.
///
/// Po angielsku, jak wszystko, co czyta agent i człowiek (decyzja D5), i bez ani jednego naszego
/// słowa z drutu: „handoff" i „fan-in" nie znaczą nic dla kogoś, kto właśnie dostał zadanie
/// (niezmiennik 14).
const HANDOFF_INDEX_OPENS: &str = "Steps before this one left what they found in these files:";

/// I zdanie, którym się kończy. Mówi wprost, że treści w prompcie nie ma — inaczej agent, który
/// nie otworzy pliku, uzna brak cytatu za brak materiału.
const HANDOFF_INDEX_CLOSES: &str =
    "Read the ones you need; their contents were not copied into this prompt.";

/// Zdanie, po którym w prompcie **refleksji** zaczyna się to, co kroki powiedziały na koniec.
///
/// 2026-09 (Z-38) — INNA RZECZ NIŻ INDEKS WYŻEJ, i dlatego osobny nagłówek. Tamten wymienia
/// PLIKI i mówi, o co dany krok był poproszony (tytuł przekazania to instrukcja z pliku
/// workflow, `title_of`); ten wymienia jedną linię, którą krok zostawił po sobie, kiedy
/// skończył — czyli to, co naprawdę się stało. Refleksja pytana o naukę z biegu bez tej drugiej
/// listy zna wyłącznie zlecenia i musi otworzyć każdy plik, żeby dowiedzieć się wyniku.
///
/// PO ANGIELSKU I BEZ NASZYCH SŁÓW Z DRUTU, tak jak [`HANDOFF_INDEX_OPENS`] (decyzja D5,
/// niezmiennik 14).
const WHAT_THE_STEPS_SAID_OPENS: &str = "This is what each step said when it finished:";

/// Dopowiedzenie wyłącznie dla sterownika, który nie potrafi przenieść dodatkowego katalogu.
const HANDOFF_PATHS_ARE_OUTSIDE: &str =
    "These files are outside your working directory, so read them at the full paths shown.";

/// Domyślna etykieta wiersza indeksu: plik zostawił krok, po którym ten krok idzie po strzałce.
///
/// 2026-08-23 (T-87) — ZAMKNIĘTA LISTA ETYKIET ZACZYNA SIĘ TUTAJ i ma dokładnie sześć pozycji
/// (szósta doszła w T-88, dla plików przejętych po poprzednim biegu).
/// Do tego dnia wiersz indeksu niósł nazwę kafelka i ścieżkę, i tyle. Od chwili, w której runda
/// trzecia pętli dostaje pięć pozycji — z których trzy pochodzą od dwóch kafelków — sama nazwa
/// przestaje cokolwiek rozróżniać: dwa wiersze `- Work: …` pod rząd nie mówią, który plik jest
/// próbą odrzuconą, a który tą przed nią. A to jest cała różnica między „popraw to, co zostało
/// odrzucone" a „przeczytaj cokolwiek".
///
/// STAŁE, NIE ZDANIE SKŁADANE PRZY WIERSZU. Etykieta pisana per wiersz rośnie z każdą gałęzią
/// kodu, który ją składa, i dwa biegi tego samego pliku czytają się inaczej — czyli przestaje
/// być etykietą, a staje się kolejnym akapitem promptu.
///
/// PO ANGIELSKU I BEZ NASZYCH SŁÓW Z DRUTU, tak jak [`HANDOFF_INDEX_OPENS`] wyżej (decyzja D5,
/// niezmiennik 14): „handoff", „verdict", „judge" i „loop" nie znaczą nic dla kogoś, kto właśnie
/// dostał robotę do zrobienia.
const IS_WHAT_THE_STEP_BEFORE_LEFT: &str = "what the step before left";

/// To samo, kiedy tamten krok **nie przeszedł**, a robota pojechała dalej mimo to.
///
/// Bez tego zdania następny agent buduje na materiale, którego nikt nie przyjął, i nie ma jak się
/// o tym dowiedzieć — a cicha luka w indeksie wygląda dokładnie tak samo jak gałąź, której nigdy
/// nie było (T-87 AC-5).
const IS_WHAT_A_STEP_THAT_FAILED_LEFT: &str = "the step before did not pass; this is what it said";

/// Etykieta wejścia pętli: plik, który dostała jej pierwsza runda.
const IS_WHAT_YOU_STARTED_WITH: &str = "what you were given at the start";

/// Początek etykiety wcześniejszej rundy TEGO kroku. Ogon dopisuje [`WhatItIs::said`].
const IS_YOUR_OWN_EARLIER_ANSWER: &str = "your own earlier answer";

/// Etykieta wcześniejszej próby pracy, którą sędzia porównuje z bieżącą.
const IS_AN_EARLIER_TRY_OF_THE_WORK: &str = "an earlier answer from the work you are checking";

/// Początek etykiety wcześniejszej rundy sędziego. „Tester", bo tak nazywa go człowiek — nasze
/// słowo („judge") nie znaczy nic po drugiej stronie promptu.
const IS_WHAT_THE_TESTER_SAID: &str = "what the tester said last time";

/// Etykieta pliku PRZEJĘTEGO PO POPRZEDNIM BIEGU — tym, od którego ten bieg wznowiono.
///
/// 2026-08-23 (T-88) — bez niej wiersz przejęty stoi w indeksie obok wierszy z tego biegu
/// i wygląda tak samo. To są dwie różne rzeczy: praca, która właśnie powstała obok, i praca
/// sprzed godziny, po której ktoś zdążył poprawić agenta albo instrukcję. Agent, który tego nie
/// wie, czyta odpowiedź na nieaktualne pytanie jak świeży materiał.
///
/// „earlier run", nie nazwa katalogu ani znacznik czasu: wiersz indeksu ma mówić, CZYM ten plik
/// jest, a `20260823-145648__01a0…` nie mówi nic nikomu (niezmiennik 14).
const IS_WHAT_AN_EARLIER_RUN_LEFT: &str = "what an earlier run left here";

/// Zdanie, którym sędzia pętli dostaje SWÓJ JEDYNY KANAŁ na wynik — i którego do 2026-08-23
/// nie dostawał wcale.
///
/// `memory::handoff::verdict_in` czyta wynik z całego wiersza `outcome: pass`, a jego własny
/// komentarz twierdzi: „Sędzia dostaje w prompcie zdanie o tym, jak zapisać werdykt". To zdanie
/// nigdy nie istniało. Kod stał na kontrakcie, którego druga strona nie została napisana.
///
/// ZMIERZONE, NIE PRZECZUTE. Na 80 przekazaniach z ośmiu biegów właściciela wiersz `outcome:`
/// nie pada ANI RAZU. Brak znacznika czyta się jako `Fail` (`Verdict::default()`), więc każda
/// pętla przepalała komplet rund, a jej ostatnia runda dostawała `Failed` — i cały stożek za
/// pętlą schodził jako `Skipped`. W biegu `20260823-011240` sędzia napisał wprost
/// „## Werdykt: **PASS** … przyjąć", a `run.json` zapisał ten krok jako `failed`; pod nim
/// zginęły `Syntezę`, `Design` i `Implementation`, czyli cały produkt biegu.
///
/// DLACZEGO WPROST O SKUTKU BRAKU. Zdanie „napisz wiersz X" bez powiedzenia, co się stanie bez
/// niego, model traktuje jak formalność. Tu brak wiersza jest decyzją — i to najkosztowniejszą
/// z możliwych — więc jest nazwany.
///
/// PO ANGIELSKU I BEZ NASZYCH SŁÓW Z DRUTU, tak jak `HANDOFF_INDEX_OPENS` obok (decyzja D5,
/// niezmiennik 14): „verdict", „loop" i „judge" nie znaczą nic dla kogoś, kto właśnie dostał
/// robotę do sprawdzenia.
/// Zdanie kroku, którego zatwierdzone wymagania nie zostały potwierdzone.
///
/// Konkret stoi obok, w `run.json`, bo wymienia identyfikatory; to zdanie jest nagłówkiem.
const WORK_LEFT_A_REQUIREMENT_OPEN: &str =
    "This work did not confirm every requirement it was given.";

const OUTCOME_ASKED_FOR: &str = "\
End your answer with a line of its own that says exactly `outcome: pass` when the work you \
were given is good enough to build on, or `outcome: fail` when it has to be done again. Put \
nothing else on that line, and write it last — anything after it is read instead of it. If \
you leave the line out, this is taken as `fail` and the work goes round again, so say what \
you mean even when the answer is obvious.";

/// Blok, którym kończy się prompt **każdego** kroku agenta — i którego do 2026-08-23 nie
/// dostawał żaden.
///
/// Loadout ma wobec agenta trzy konkretne oczekiwania i nie mówił mu ani jednego. Ostatnia
/// wypowiedź tury JEST przekazaniem ([`Live::hand_over`]), `memory::handoff::reshape` dopisuje
/// brakujące nagłówki, a wyników nie zapisuje się do plików, bo robi to Loadout.
///
/// ZMIERZONE, NIE PRZECZUTE. W biegu `20260823-145648` **sześć** kroków Claude'a zaczyna
/// podsumowanie od „*Write access is disabled in this session, so I can't create the handoff
/// file — the findings are below*". Agent palił tury na próbę zapisania pliku wyników, bo tak
/// każą mu instrukcje gospodarza, a dial `look-only` to blokuje. Gdyby wiedział, że jego
/// odpowiedź **jest** tym, co przekazuje dalej, nie próbowałby wcale.
///
/// TRZY NAGŁÓWKI SŁOWO W SŁOWO Z `memory::handoff`. `heading_at` przyjmuje wiersz, który jest
/// DOKŁADNIE `## <nazwa>`, i tylko komplet trzech we właściwej kolejności przechodzi nietknięty.
/// Prośba o `## Findings` albo o nagłówek z dopiskiem w tym samym wierszu byłaby umową, której
/// nasza własna strona nie podpisała — i każda tura płaciłaby za naprawę kształtu, który agent
/// oddał dokładnie tak, jak go poproszono.
///
/// BEZ WIERSZA O WYNIKU. Ten blok dostają wszyscy, a o wynik wolno poprosić wyłącznie sędziego
/// pętli ([`Live::ask_for_an_outcome`]): prośba skierowana do kroku, którego odpowiedzi nikt nie
/// sądzi, jest poleceniem bez skutku (niezmiennik 16) i uczy model pisać ten wiersz wszędzie.
///
/// W LICZBIE NIEOKREŚLONEJ („what comes next"), nigdy „the step after yours". Ta pierwsza wersja
/// kłamała w dwóch kształtach, które ten produkt buduje na co dzień: przy rozgałęzieniu krok
/// oddaje robotę TRZEM krokom naraz, a krok ostatni nie oddaje jej żadnemu — czyta go człowiek.
/// Agent, który wierzy w jeden krok po sobie, pisze odpowiedź pod jednego czytelnika i wybiera,
/// pod którego; przy ostatnim kroku pisze pod czytelnika, którego nie ma.
///
/// PO ANGIELSKU I BEZ NASZYCH SŁÓW Z DRUTU, tak jak [`HANDOFF_INDEX_OPENS`] i
/// [`OUTCOME_ASKED_FOR`] obok (decyzja D5, niezmiennik 14): agent czyta „what this step passes
/// on", nigdy „handoff".
const HOW_TO_ANSWER: &str = "\
Your last message is what this step passes on. What comes next reads it and nothing else, so \
leave nothing worth keeping outside it.

Write it under these three headings, each one alone on its line and in this order:

## Answer
what comes next needs to know.

## Evidence
`file:line`, or a link, for every claim above.

## Open
what you could not settle.

Do not write your results to a file. Loadout files your last message for you, and a file you \
write yourself is read by nobody.";

/// Zdanie, którym blok mówi, że ten krok nie ma limitu czasu.
///
/// `0` w `giveUpAfterMinutes` znaczy „bez limitu" (`library::agents::Agent`), więc podstawienie
/// tej liczby dałoby „you have 0 minutes for this step" — polecenie, po którym nie ma nic
/// sensownego do zrobienia, i wygląda ono w kodzie dokładnie tak samo jak każde inne. Milczenie
/// też nie jest tą samą odpowiedzią: agent, któremu nie powiedziano nic, budżetuje pod limit,
/// którego się domyśla, i domyśla się nisko.
const NO_TIME_LIMIT: &str =
    "There is no time limit on this step, so take the time the work really needs.";

/// Zdanie, którym blok „jak odpowiadać" prosi o umówione pola przekazania.
///
/// 2026-08-23 (T-90) — `Handover::Form { fields }` jest w schemacie kroku od T3 §3.1, czyta go
/// import, ma nawet własne `required` — i jedynym użyciem w drzewie było `Handover::default()`.
/// Człowiek opisywał, co ten krok ma oddać, plik to zapisywał, a agent nigdy się o tym nie
/// dowiadywał. Wymaganie, o którym agent nie wie, jest karą, nie umową — dokładnie jak limit
/// czasu, o którym do 2026-08-23 wiedział wyłącznie ten, kto zabija krok.
///
/// KSZTAŁT POKAZANY WPROST, bo agent kopiuje ten, który mu się pokaże, a nasz własny czytnik
/// bierze **cały wiersz** zaczynający się nazwą i dwukropkiem ([`fields_said_in`]). Lista
/// wypisana myślnikami wygląda w prompcie równie porządnie i jest dla tamtego czytnika
/// niewidzialna — czyli byłaby umową, której jedna strona nie podpisała.
const FIELDS_ASKED_FOR: &str = "\
This step also has to hand these back. Put each one on a line of its own, starting with the name \
and a colon, in the shape shown here:";

/// I zdanie o tym, co się stanie bez pola oznaczonego jako potrzebne.
///
/// WPROST O SKUTKU, tak samo jak [`OUTCOME_ASKED_FOR`] obok i z tego samego powodu: prośbę „napisz
/// wiersz X" bez powiedzenia, co się stanie bez niego, model traktuje jak formalność.
const FIELDS_ARE_REQUIRED: &str = "\
The ones marked as needed are not optional: an answer without them does not pass, and whatever \
comes after this step is left without the thing it was promised. The rest you may leave out when \
you have nothing to put in them.";

/// Znacznik, którym blok odróżnia pole wymagane od reszty.
///
/// Po angielsku i bez naszych słów z drutu (decyzja D5, niezmiennik 14): `required` jest kluczem
/// w pliku, a nie słowem, którym mówi się do kogoś, kto właśnie dostał robotę.
const FIELD_IS_NEEDED: &str = " (needed)";

/// Etykieta, którą człowiek widzi nad polem ze ścieżką wyniku (`step-panel/panel.tsx`).
///
/// 2026-08-23 (T-90) — ODMOWA NAZYWA TO, CO STOI NA EKRANIE, nigdy klucza z pliku:
/// `writeResultsTo` nie istnieje na żadnym ekranie (niezmiennik 14), a panel kroku ma dziewięć
/// wierszy — odmowa bez nazwy pola wysyła człowieka szukać, który z nich zatrzymał bieg.
const WRITE_RESULTS_TO: &str = "Write results to";

/// Zdanie o kroku, którego plików kontekstu nie dało się udowodnić.
///
/// Stoi tu, przy pozostałych zdaniach tego modułu, a nie w środku gałęzi, która je wysyła: to jest
/// tekst, który człowiek czyta na karcie kroku, więc czyta się go razem z resztą takich zdań.
const CONTEXT_NOT_PROVEN: &str =
    "Loadout could not prove the context files for this agent, so it did not start the step.";

const PRIVATE_STATE_NOT_READY: &str =
    "Loadout could not create this agent's private state folder, so it did not start the step.";

fn public_start_refusal(vendor: &str, error: &anyhow::Error) -> String {
    if error
        .downcast_ref::<DriverSetupError>()
        .is_some_and(|typed| typed.failure() == DriverSetupFailure::PrivateState)
    {
        // Pełny łańcuch z kontekstem ścieżki zostaje w diagnostyce; na ekran nie wolno
        // wypuścić ani prywatnej ścieżki, ani surowego zdania systemu plików [T-127].
        tracing::debug!(error = ?error, "the agent private state setup was refused");
        return PRIVATE_STATE_NOT_READY.to_owned();
    }
    let executable_was_not_found = error
        .downcast_ref::<io::Error>()
        .and_then(io::Error::get_ref)
        .is_some_and(|source| source.downcast_ref::<MissingProgram>().is_some());
    if executable_was_not_found {
        let cli = match vendor {
            "codex" => Some("Codex CLI"),
            "claude" => Some("Claude Code CLI"),
            _ => None,
        };
        if let Some(cli) = cli {
            tracing::debug!(vendor, error = ?error, "the configured agent CLI was not found");
            return format!(
                "Loadout could not find {cli} on this Mac. Please install it or add it to PATH, \
                 then restart Loadout."
            );
        }
    }
    format!("Loadout could not start this agent: {error}")
}

/// Zdanie, którym kończy się każda ścieżka wyniku wypadająca poza folder kroku.
///
/// Jedno na dwa pytania — o katalog nad plikiem i o sam plik ([`Live::room_for_the_answer`]) — bo
/// człowiekowi wypada z tego ta sama poprawka: wskaż miejsce w folderze, który ten krok dostał.
const LEADS_OUT_OF_THE_FOLDER: &str = "that path leads out of the folder this step works in";

/// Uruchamia workflow z pliku i oddaje jego linie pompie — **linia po linii**.
///
/// Kolejność: wczytaj → sprawdź → katalog biegu → migawka → planista → sterowniki → linie.
/// Odmowa przed pierwszym utworzonym katalogiem; szczegóły w nagłówku modułu.
///
/// `lines` jest [`LineSink`] z T-07, a nie `mpsc::Sender<Vec<Line>>`, i to jest cała zmiana
/// tego zadania. Sklejanie mieszka **po stronie pompy**, bo tam je zmierzono (16 ms / 2000
/// linii, [T8 §5.3]), a `LineSink::send` nigdy nie blokuje producenta: na pełnej kolejce linia
/// jest porzucana i **policzona**. Kanał, który każe czekać pętli czytającej stdout agenta,
/// kasuje dokładnie tę własność, dla której ta pompa powstała.
///
/// [`LineSink`] jedzie stąd w dół jedną drogą i nigdzie się nie rozgałęzia:
/// [`the_whole_run`] → [`Live::lines`] → [`forward`] → [`send_batch`], gdzie paczka kuratora
/// rozsypuje się na pojedyncze `sink.send(line)`. Sklejanie z powrotem robi pompa, po drugiej
/// stronie kolejki — i to jest jedyne miejsce, w którym wolno je zrobić, bo tam je zmierzono.
///
/// **Pulę miejsc robi sobie sam, na ten jeden bieg** — i to jest wada, nie wygoda: dwie karty
/// dają wtedy `2 × limit` agentów naraz, a semafor ma być jeden na całą aplikację
/// (`docs/ARCHITECTURE.md` §6a, niezmiennik 11). Wołający, który ma pulę wspólną, wchodzi
/// [`run_workflow_with_slots`] i podaje ją argumentem; ta droga zostaje dla tego, kto żadnej
/// nie ma i chce bieg sam dla siebie.
pub async fn run_workflow_inner(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
) -> Result<RunReport, RunError> {
    run_workflow_with_prestart_faults(deps, request, lines, Arc::new(NoPrestartFaults)).await
}

/// Granice przygotowania biegu, w których kryterium T-152 może odmówić dalszej pracy.
///
/// To jest szew obserwacyjny wokół jednej produkcyjnej drogi przygotowania, nie alternatywny
/// algorytm. Implementacja ma pytać go dopiero po wykonaniu nazwanej operacji, a przed przejściem
/// do następnej. Odmowa zawsze wraca przez provisional guard, zanim ruszy pierwszy sterownik.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrestartFaultPoint {
    AfterWorktreeAdd,
    AfterFirstIsolation,
    AfterSecondIsolation,
    AfterHandoffSeed,
    AfterBorrow,
    AfterSkills,
    BeforeFirstRunFile,
    AfterFirstRunFile,
    OwnershipTransferred,
}

/// Zasób należący jeszcze do provisional guarda, zgłaszany przy każdej próbie rollbacku.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RolledBackResource {
    GitTree { path: PathBuf, branch: String },
    FileCopy(PathBuf),
    RunFile(PathBuf),
    RunDirectory(PathBuf),
}

/// Produkcyjny szew fault-injection dla transakcji przed pierwszym procesem.
pub trait PrestartFaultInjector: Send + Sync {
    /// Pozwala kryterium odtworzyć stan po awarii, zanim produkcja oceni własność katalogu.
    /// Zwykły bieg niczego tu nie robi; przygotowany stan dalej przechodzi pełną ścieżkę Run.
    fn before_run_directory(&self, _run_dir: &Path) -> io::Result<()> {
        Ok(())
    }

    /// WF-03: błąd po rzeczywistej operacji składania, przypięty wyłącznie do tego biegu.
    /// Obserwator nie wykonuje publikacji ani cleanupu; zwykły bieg ma tutaj noop.
    fn after_fan_in_operation(&self, _into: &Path, _applied: usize) -> io::Result<()> {
        Ok(())
    }

    /// `Err` odtwarza odmowę w nazwanej granicy przygotowania.
    fn check(&self, point: PrestartFaultPoint) -> io::Result<()>;

    /// Obserwacja bounded cleanupu; nie wykonuje sprzątania za produkcję.
    fn rolled_back(&self, resource: &RolledBackResource);
}

#[derive(Debug)]
struct NoPrestartFaults;

impl PrestartFaultInjector for NoPrestartFaults {
    fn check(&self, _point: PrestartFaultPoint) -> io::Result<()> {
        Ok(())
    }

    fn rolled_back(&self, _resource: &RolledBackResource) {}
}

#[derive(Debug, PartialEq, Eq)]
enum ProvisionalResource {
    RunDirectory(PathBuf),
    FileCopy(PathBuf),
    GitTree {
        path: PathBuf,
        branch: String,
        head: String,
        marker_path: PathBuf,
    },
    RunFile {
        path: PathBuf,
        run_id: String,
        exact_bytes: Option<Vec<u8>>,
        identity: Option<PublicationIdentity>,
    },
}

/// Jedyny właściciel artefaktów biegu przed przekazaniem ich do [`Live`].
///
/// 2026-08-28 (T-152): przed pierwszym `run.json` nie istnieje lifecycle, który mógłby
/// posprzątać odmowę po drugiej kopii, seedzie albo materializacji umiejętności. Guard zapisuje
/// wyłącznie zasoby utworzone przez tę próbę i zdejmuje każdy z nich najwyżej raz, w LIFO.
/// 2026-09 (Z-10) — KATALOG PROJEKTU NA WŁASNOŚĆ, nie pożyczka. Całe układanie katalogu biegu
/// jedzie od tego dnia na pulę blokującą (`git worktree add` to pełny checkout), a domknięcie
/// przekazane `spawn_blocking` musi być `'static`. Pożyczka `deps.project` tego nie spełnia
/// i spełnić nie może — jedna dodatkowa kopia ścieżki jest tu ceną za wątek okna.
struct ProvisionalRun {
    project: PathBuf,
    faults: Arc<dyn PrestartFaultInjector>,
    bound_prestart: Option<BoundPrestart>,
    reclaimed_run_directory: Option<PathBuf>,
    parent_cleanup_blocked: bool,
    resources: Vec<ProvisionalResource>,
    armed: bool,
}

impl ProvisionalRun {
    fn new(
        project: PathBuf,
        faults: Arc<dyn PrestartFaultInjector>,
        bound_prestart: Option<BoundPrestart>,
    ) -> Self {
        Self {
            project,
            faults,
            bound_prestart,
            reclaimed_run_directory: None,
            parent_cleanup_blocked: false,
            resources: Vec::new(),
            armed: true,
        }
    }

    fn check(&self, point: PrestartFaultPoint) -> Result<(), RunError> {
        self.faults.check(point).map_err(RunError::Io)
    }

    fn owns_run_directory(&mut self, path: PathBuf) {
        self.push_once(ProvisionalResource::RunDirectory(path));
    }

    fn owns_reclaimed_run_directory(&mut self, path: PathBuf) {
        self.reclaimed_run_directory = Some(path.clone());
        self.owns_run_directory(path);
    }

    fn block_reclaimed_parent_cleanup(&mut self, run_dir: &Path) {
        if self.reclaimed_run_directory.as_deref() == Some(run_dir) {
            self.parent_cleanup_blocked = true;
        }
    }

    fn can_reclaim_bound_directory(&self, run_id: &str, run_dir: &Path) -> bool {
        self.bound_prestart
            .as_ref()
            .is_some_and(|bound| bound.run_id == run_id && bound.run_file == run_dir.join(RUN_FILE))
    }

    fn owns_run_directory_path(&self, path: &Path) -> bool {
        self.resources
            .iter()
            .any(|resource| matches!(resource, ProvisionalResource::RunDirectory(owned) if owned == path))
    }

    fn owns_directory_containing_marker(&self, marker_path: &Path) -> bool {
        marker_path
            .parent()
            .and_then(Path::parent)
            .is_some_and(|run_dir| self.owns_run_directory_path(run_dir))
    }

    fn owns_work_path(&self, path: &Path) -> bool {
        self.resources.iter().any(|resource| match resource {
            ProvisionalResource::FileCopy(owned)
            | ProvisionalResource::GitTree { path: owned, .. } => owned == path,
            ProvisionalResource::RunDirectory(_) | ProvisionalResource::RunFile { .. } => false,
        })
    }

    fn owns_git_tree(&mut self, path: PathBuf, branch: String, head: String, marker_path: PathBuf) {
        self.push_once(ProvisionalResource::GitTree {
            path,
            branch,
            head,
            marker_path,
        });
    }

    fn owns_file_copy(&mut self, path: PathBuf) {
        self.push_once(ProvisionalResource::FileCopy(path));
    }

    fn will_create_run_file(&mut self, path: PathBuf, run_id: String) -> io::Result<()> {
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.push_once(ProvisionalResource::RunFile {
                    path,
                    run_id,
                    exact_bytes: None,
                    identity: None,
                });
                Ok(())
            }
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "the first run file already exists; nothing ran",
            )),
            Err(error) => Err(error),
        }
    }

    fn authenticate_created_run_file(
        &mut self,
        path: &Path,
        run_id: &str,
        exact_bytes: Vec<u8>,
        identity: PublicationIdentity,
    ) -> io::Result<()> {
        let resource = self.resources.iter_mut().find(|resource| {
            matches!(
                resource,
                ProvisionalResource::RunFile {
                    path: owned_path,
                    run_id: owned_run_id,
                    ..
                } if owned_path == path && owned_run_id == run_id
            )
        });
        let Some(ProvisionalResource::RunFile {
            exact_bytes: owned_bytes,
            identity: owned_identity,
            ..
        }) = resource
        else {
            return Err(io::Error::other(
                "the first run file was not registered before publication",
            ));
        };
        *owned_bytes = Some(exact_bytes);
        *owned_identity = Some(identity);
        Ok(())
    }

    fn push_once(&mut self, resource: ProvisionalResource) {
        if !self.resources.contains(&resource) {
            self.resources.push(resource);
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ProvisionalRun {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut children_clean = !self.parent_cleanup_blocked;
        while let Some(resource) = self.resources.pop() {
            let public = match &resource {
                ProvisionalResource::RunDirectory(path) => {
                    RolledBackResource::RunDirectory(path.clone())
                }
                ProvisionalResource::FileCopy(path) => RolledBackResource::FileCopy(path.clone()),
                ProvisionalResource::GitTree { path, branch, .. } => RolledBackResource::GitTree {
                    path: path.clone(),
                    branch: branch.clone(),
                },
                ProvisionalResource::RunFile { path, .. } => {
                    RolledBackResource::RunFile(path.clone())
                }
            };
            self.faults.rolled_back(&public);
            let result = match resource {
                ProvisionalResource::RunDirectory(path) => {
                    if !children_clean {
                        tracing::warn!(
                            ?public,
                            "a provisional run directory was kept because one of its children no longer matched this attempt"
                        );
                        continue;
                    }
                    remove_owned_directory(&path)
                }
                ProvisionalResource::FileCopy(path) => remove_owned_directory(&path),
                ProvisionalResource::GitTree {
                    path,
                    branch,
                    head,
                    marker_path,
                } => {
                    cleanup_provisional_git_tree(&self.project, &path, &branch, &head, &marker_path)
                        .map_err(|error| io::Error::other(error.to_string()))
                }
                ProvisionalResource::RunFile {
                    path,
                    run_id,
                    exact_bytes,
                    identity,
                } => remove_owned_run_file(&path, &run_id, exact_bytes.as_deref(), identity),
            };
            if let Err(error) = result {
                children_clean = false;
                tracing::warn!(?public, %error, "a provisional run resource could not be rolled back");
            }
        }
    }
}

fn remove_owned_directory(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn validate_unstarted_run_file(bytes: &[u8], run_id: &str) -> io::Result<()> {
    let described: Value = serde_json::from_slice(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if described.get("id").and_then(Value::as_str) != Some(run_id) {
        return Err(io::Error::other(
            "the provisional run file belongs to another run; nothing was removed",
        ));
    }
    let steps = described
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            io::Error::other("the provisional run file has no step records; nothing was removed")
        })?;
    if steps.iter().any(|step| {
        step.get("executed").and_then(Value::as_bool) == Some(true)
            || step.get("process_started").and_then(Value::as_bool) == Some(true)
    }) {
        return Err(io::Error::other(
            "the provisional run file says that work started; nothing was removed",
        ));
    }
    Ok(())
}

fn remove_owned_run_file(
    path: &Path,
    run_id: &str,
    exact_bytes: Option<&[u8]>,
    identity: Option<PublicationIdentity>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("the provisional run file has no parent directory"))?;
    // `plan.dir` może zachować ścieżkę workspace'u otwartego przez symlink. Kandydat przeszedł
    // już proof wygenerowanego dziecka; publisher dostaje jego realny root, bo sam poprawnie
    // odmawia przechodzenia przez symlink w kontrolowanej ścieżce.
    let parent = fs::canonicalize(parent)?;
    let relative = Path::new(
        path.file_name()
            .ok_or_else(|| io::Error::other("the provisional run file has no file name"))?,
    );
    DurableFilePublisher::new(&parent)
        .with_publication(|batch| {
            let target = batch.root().target(relative).map_err(PublishError::Io)?;
            let Some(found_identity) = target
                .regular_target_identity()
                .map_err(PublishError::Io)?
            else {
                return Ok(());
            };
            let Some(identity) = identity else {
                return Err(PublishError::Io(io::Error::other(
                    "the published run file was not recorded before cleanup; nothing was removed",
                )));
            };
            if found_identity != identity {
                return Err(PublishError::Io(io::Error::other(
                    "the run file was replaced before cleanup; nothing was removed",
                )));
            }
            let Some(exact_bytes) = exact_bytes else {
                return Err(PublishError::Io(io::Error::other(
                    "the provisional run file was never authenticated after publication; nothing was removed",
                )));
            };
            let found = match batch.root().read_regular(relative, false) {
                Ok(found) => found,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(PublishError::Io(error)),
            };
            validate_unstarted_run_file(&found, run_id).map_err(PublishError::Io)?;
            if found.as_slice() != exact_bytes {
                return Err(PublishError::Io(io::Error::other(
                    "the run file changed before cleanup; nothing was removed",
                )));
            }
            // Wspólny publisher trzyma jeden lifecycle dla tego roota. POSIX nie ma
            // compare-and-unlink wobec obcego procesu, więc ostatnia descriptor-relative
            // kontrola identity pozostaje możliwie wąsko przy samym unlinku.
            if !batch
                .root()
                .remove_regular_file_if_identity(relative, identity)
                .map_err(PublishError::Io)?
            {
                return Err(PublishError::Io(io::Error::other(
                    "the provisional run file changed before removal; nothing was removed",
                )));
            }
            Ok(())
        })
        .map_err(PublishError::into_io)
}

/// Ten sam Run co [`run_workflow_inner`], z kontrolowanymi odmowami w fazie przygotowania.
///
/// Ile miejsca musi zostać na dysku, żeby bieg w ogóle ruszył.
///
/// Jeden gigabajt, i to jest próg BEZPIECZEŃSTWA, nie oszacowanie potrzeb. Bieg nie wie z góry,
/// ile napisze: transkrypt rośnie z liczbą tur, a drzewo robocze z rozmiarem projektu. Liczba
/// ma być na tyle duża, żeby pod nią żaden bieg nie miał sensu, i na tyle mała, żeby nigdy nie
/// odmówiła komuś, kto ma normalnie miejsce.
pub const ROOM_FLOOR_BYTES: u64 = 1_073_741_824;

/// Zdanie odmowy, kiedy na dysku nie ma już miejsca na pracę tego biegu — albo `None`.
///
/// Zdanie niesie OBIE liczby, bo „za mało miejsca" bez nich zostawia człowieka ze zgadywaniem,
/// ile ma zwolnić. Gigabajty, nie bajty: to jest jednostka, w której człowiek patrzy na dysk.
#[must_use]
pub fn no_room_refusal(free: u64) -> Option<Note> {
    (free < ROOM_FLOOR_BYTES).then(|| Note {
        level: Level::Problem,
        step_id: None,
        message: format!(
            "There is not enough room on disk to start: {:.1} GB free, and a run needs at least \
             {:.1} GB. A run that fills the disk halfway through leaves a half-written copy \
             behind. Free some space and start again.",
            as_gigabytes(free),
            as_gigabytes(ROOM_FLOOR_BYTES)
        ),
        fix: None,
    })
}

/// Bajty jako gigabajty dziesiętne — tak, jak liczy je Finder i każdy producent dysku.
fn as_gigabytes(bytes: u64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "zdanie dla człowieka ma jedno miejsce po przecinku; f64 trzyma bajty \
                  dokładnie aż do 9 petabajtów"
    )]
    let bytes = bytes as f64;
    bytes / 1_000_000_000.0
}

/// Szkielet T-152: właściwa implementacja zastąpi `todo!()` po uczciwym czerwonym `before`.
pub async fn run_workflow_with_prestart_faults(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    faults: Arc<dyn PrestartFaultInjector>,
) -> Result<RunReport, RunError> {
    let slots = the_pool_of_this_application(deps, request.how_many_at_once);
    the_whole_workflow_with_prestart(
        deps,
        request,
        lines,
        slots,
        WorkflowRunOptions {
            budget_usd: None,
            lead_start: None,
            reflection_enabled: learn_from_runs(deps),
            before_stamp: None,
            faults,
        },
    )
    .await
}

/// Ustawienie dla dróg, które nie mają żywego paska Run: `/ask`, trigger i szwy akceptacyjne.
fn learn_from_runs(deps: &RunDeps<'_>) -> bool {
    match settings::read_settings_inner(deps.home) {
        Ok(settings) => settings.learn_from_runs,
        Err(error) => {
            // 2026-09 (Z-18): nieczytelny plik nie może po cichu zmienić wcześniejszego
            // domyślnego `on` w `off`; zwykły ekran pokaże osobno odmowę odczytu.
            tracing::error!(%error, "what Loadout learns from runs could not be read, so it stays on");
            true
        }
    }
}

/// Jednorazowy obserwator tekstu zamrożonego w planie przed produkcyjnym stemplem pamięci.
pub type FrozenPromptHook = Arc<dyn Fn(&str) + Send + Sync>;

/// Acceptance wejście zachowujące tę samą drogę budżetu, refleksji i wykonania co zwykły Run.
pub async fn run_workflow_with_snapshot_hook(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    budget_usd: Option<f64>,
    reflection_enabled: bool,
    after_prompt: Option<FrozenPromptHook>,
) -> Result<RunReport, RunError> {
    let slots = the_pool_of_this_application(deps, request.how_many_at_once);
    the_whole_workflow(
        deps,
        request,
        lines,
        slots,
        budget_usd,
        reflection_enabled,
        after_prompt,
    )
    .await
}

/// Zgodnościowy adapter poprzedniego szwu; zwykły Run prowadzi nim do wspólnego rdzenia.
pub async fn run_workflow_with_before_stamp(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    after_prompt: Option<FrozenPromptHook>,
) -> Result<RunReport, RunError> {
    run_workflow_with_snapshot_hook(deps, request, lines, None, true, after_prompt).await
}

/// Ten sam bieg, z **sufitem wydatku** — albo bez niego, kiedy człowiek żadnego nie postawił.
///
/// Sufit jedzie ARGUMENTEM, nie polem [`RunRequest`], i to nie jest szczegół stylu. Zmierzone
/// 2026-08-24: literał tamtej struktury stoi w tym drzewie w **55 plikach**, a typ nie ma
/// `Default` — jedno nowe pole przewraca każdy z nich naraz, w tym pliki kryteriów, których to
/// zadanie nie posiada (`AGENTS.md` §7). Argument psuje wyłącznie prawdziwych wołających,
/// a tych jest kilku.
pub async fn run_workflow_with_budget(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    budget_usd: Option<f64>,
) -> Result<RunReport, RunError> {
    let slots = the_pool_of_this_application(deps, request.how_many_at_once);
    the_whole_workflow(deps, request, lines, slots, budget_usd, true, None).await
}

/// Ten sam bieg z jawnym wyborem, czy po jego końcu Loadout bierze prywatną turę refleksji.
///
/// Sygnatura powstaje w fazie specyfikacji T-126, żeby kryteria kompilowały się i padały na
/// brakującym zachowaniu. Stare wejścia zostają nietknięte: ich domyślne zachowanie i cudze
/// kryteria nie mogą zależeć od nowego argumentu.
pub async fn run_workflow_with_reflection(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    budget_usd: Option<f64>,
    reflection_enabled: bool,
) -> Result<RunReport, RunError> {
    let slots = the_pool_of_this_application(deps, request.how_many_at_once);
    the_whole_workflow(
        deps,
        request,
        lines,
        slots,
        budget_usd,
        reflection_enabled,
        None,
    )
    .await
}

/// WF-07: ten sam executor i globalna pula, dodatkowo ack po trwałym prestarcie.
pub async fn run_workflow_with_lead_start(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    budget_usd: Option<f64>,
    reflection_enabled: bool,
    lead_start: super::lead_start::LeadStart,
) -> Result<RunReport, RunError> {
    let slots = the_pool_of_this_application(deps, request.how_many_at_once);
    the_whole_workflow_with_prestart(
        deps,
        request,
        lines,
        slots,
        WorkflowRunOptions {
            budget_usd,
            reflection_enabled,
            before_stamp: None,
            faults: Arc::new(NoPrestartFaults),
            lead_start: Some(lead_start),
        },
    )
    .await
}

/// Pula miejsc, z której ma brać TEN bieg — jedna na całą aplikację, nie jedna na bieg.
///
/// Uchwyt przychodzi z [`RunDeps`], czyli z tego, co aplikacja wręczyła temu biegowi
/// ([`crate::ipc::AppState::begin_run`]). Do 2026-08-24 stało tu `Limiter::new(…)` i to była
/// wada, nie wygoda: dwa biegi dawały `2 × limit` agentów po ~583 MB, czyli zamrożony laptop
/// zamiast szybszej pracy (`docs/ARCHITECTURE.md` §6a, niezmiennik 11).
///
/// **Suwak przestawia wspólny limit właśnie tutaj**, a nie zakłada drugiej puli obok. W dół nic
/// nie ginie: nadmiar schodzi dopiero przy zwalnianiu miejsc (`engine::limits::Pool::take_back`),
/// więc obniżenie liczby nie dotyka ani jednego kroku, który już pracuje.
fn the_pool_of_this_application(deps: &RunDeps<'_>, how_many_at_once: usize) -> Limiter {
    let slots = deps.control.slots();
    slots.set_at_once(how_many_at_once);
    slots
}

/// Wynik Startu niosącego trwały claim triggera.
#[derive(Debug, Clone)]
pub enum TriggerRunReport {
    /// Ten Start utworzył bieg i doprowadził go zwykłą drogą do wyniku.
    Ran(RunReport),
    /// Ledger po restarcie znalazł już pierwszy `run.json`; drugi agent nie został uruchomiony.
    AlreadyAccepted {
        /// UUID v7 przydzielony jeszcze przy utworzeniu dostawy.
        id: String,
        /// Plik, który jest dowodem trwałej akceptacji.
        run_file: PathBuf,
    },
}

/// Istniejąca droga biegu z jedną różnicą: plan bierze UUID i czas z trwałej dostawy, a pierwszy
/// `run.json` domyka ledger przed pierwszym wywołaniem sterownika.
pub async fn run_triggered_workflow_inner(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    claim: &TriggerClaim,
    lines: LineSink,
) -> Result<TriggerRunReport, RunError> {
    run_triggered_workflow_with_budget(deps, request, claim, lines, None).await
}

/// Ta sama droga triggera, plus **sufit wydatku** tego biegu.
///
/// 2026-08-29 — DOPISANA, A NIE WSTAWIONA W [`run_triggered_workflow_inner`], i to nie jest gust:
/// tamten podpis wołają czterema argumentami dwa cudze pliki kryteriów (`tests/it/trigger_run_is_
/// accepted_once.rs`, `tests/run_evidence_reaches_the_product.rs`), których to zadanie nie posiada
/// (`AGENTS.md` §7). Sufit jedzie ARGUMENTEM, nie polem [`RunRequest`], z tego samego zmierzonego
/// powodu, co przy [`run_workflow_with_budget`]: literał tamtej struktury stoi w tym drzewie
/// w 55 plikach i nie ma `Default`.
pub async fn run_triggered_workflow_with_budget(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    claim: &TriggerClaim,
    lines: LineSink,
    budget_usd: Option<f64>,
) -> Result<TriggerRunReport, RunError> {
    the_triggered_start(
        deps,
        request,
        claim,
        lines,
        Arc::new(NoPrestartFaults),
        budget_usd,
    )
    .await
}

/// Produkcyjna droga triggera z tym samym szwem odmów, którego używa kryterium T-152.
///
/// To nie jest drugi algorytm Startu: fault injector jedzie do wspólnego
/// [`the_planned_run_with_prestart`], a bind, reconcile i release pozostają dokładnie tutaj.
pub async fn run_triggered_workflow_with_prestart_faults(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    claim: &TriggerClaim,
    lines: LineSink,
    faults: Arc<dyn PrestartFaultInjector>,
) -> Result<TriggerRunReport, RunError> {
    the_triggered_start(deps, request, claim, lines, faults, None).await
}

/// Jedno ciało trzech dróg wyżej: zapadka biegu, pula i sufit podany argumentem.
///
/// `deps.control.settle()` musi zostać na KAŻDEJ drodze wyjścia — powód w całości stoi przy
/// [`the_whole_triggered_run`], a od 2026-09 pilnuje tego [`Settling`], nie kolejność linii.
async fn the_triggered_start(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    claim: &TriggerClaim,
    lines: LineSink,
    faults: Arc<dyn PrestartFaultInjector>,
    budget_usd: Option<f64>,
) -> Result<TriggerRunReport, RunError> {
    deps.control.begin();
    let settling = Settling(deps.control.clone());
    deps.control.lines_go_to(lines.clone());
    let slots = the_pool_of_this_application(deps, request.how_many_at_once);
    let report =
        the_whole_triggered_run(deps, request, claim, lines, slots, faults, budget_usd).await;
    drop(settling);
    report
}

/// Ten sam bieg, tylko miejsca bierze ze **wspólnej puli aplikacji** — jednej dla wszystkich
/// kart, nie jednej na bieg.
///
/// [`run_workflow_inner`] podaje `how_many_at_once` prosto do semafora, który planista zakłada
/// per bieg, więc dwie karty dają `2 × limit` agentów naraz. Przy ~583 MB na agenta
/// `[T7 ryzyko 3, V]` to jest zamrożony laptop, a nie szybsza praca — dlatego semafor ma być
/// jeden na całą aplikację (`docs/ARCHITECTURE.md` §6a, niezmiennik 11).
///
/// **Pula wchodzi argumentem.** Bieg, który robi ją sobie sam, jest nie do odróżnienia od biegu,
/// który robi po jednej na kartę — to samo zdanie stoi przy [`crate::workspace::Registry::new`],
/// i to jest dokładnie ten uchwyt, który tamten rejestr wydaje przez `Registry::slots`. Klon
/// [`Limiter`] dzieli tę samą pulę i to jest cały mechanizm (`engine::limits`).
///
/// 2026-08-17 — naturalnym miejscem tego uchwytu jest pole w [`RunDeps`], żeby wszystkie drzwi
/// do biegu miały je bez wyjątku. `commands/mod.rs` nie należy do T-31 (`AGENTS.md` §7), więc
/// pula wchodzi tędy, a nie tamtędy; scalenie obu dróg w jedną należy do tego, kto będzie mógł
/// dotknąć [`RunDeps`].
///
/// **`deps.control.settle()` musi zostać na KAŻDEJ drodze wyjścia**, także po odmowie: to na to
/// zdanie czeka [`stop_run_inner`], żeby móc wrócić z dowodem (niezmiennik 6). Settle wpisany
/// tylko na szczęśliwej ścieżce zawiesza Stop przy każdym biegu, który padł, i wygląda to jak
/// zawieszony agent, nie jak brakująca linijka. Dlatego cały bieg siedzi w [`the_whole_run`]:
/// stamtąd wychodzi się kilkoma `?`, a stąd — dokładnie jednym `return`.
pub async fn run_workflow_with_slots(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    slots: Limiter,
) -> Result<RunReport, RunError> {
    the_whole_workflow(deps, request, lines, slots, None, true, None).await
}

/// Jedno ciało obu dróg wyżej: pula podana argumentem, sufit wydatku podany argumentem.
async fn the_whole_workflow(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    slots: Limiter,
    budget_usd: Option<f64>,
    reflection_enabled: bool,
    before_stamp: Option<FrozenPromptHook>,
) -> Result<RunReport, RunError> {
    the_whole_workflow_with_prestart(
        deps,
        request,
        lines,
        slots,
        WorkflowRunOptions {
            budget_usd,
            lead_start: None,
            reflection_enabled,
            before_stamp,
            faults: Arc::new(NoPrestartFaults),
        },
    )
    .await
}

async fn the_whole_workflow_with_prestart(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    slots: Limiter,
    options: WorkflowRunOptions,
) -> Result<RunReport, RunError> {
    /* „Ruszyliśmy" zapala się PRZED pierwszym `?`, a nie po walidacji, i to jest celowe: bieg
     * odrzucony przez walidator też przechodzi tę funkcję, więc zapali za chwilę `settle()` —
     * a `is_working()` czyta oba znaczniki i odpowie wtedy „nie ma czego zatrzymywać". Zapalenie
     * dopiero po walidacji dałoby okno czasu, w którym bieg już czyta dysk, a zamknięcie okna
     * uznałoby, że nie ma nic do roboty. */
    deps.control.begin();
    /* GWARDIA STOI ZARAZ ZA `begin()` i to jest cała jej treść: między jednym a drugim nie ma ani
     * jednego `await`, więc nie istnieje stan „bieg ruszył i nie ma kto go oddać". */
    let settling = Settling(deps.control.clone());
    /* Strumień oddajemy biegowi DO UCHWYTU, bo tura człowieka przychodzi spoza pętli kroku:
     * komendą z okna, w chwili, w której krok czeka na agenta. Klon, nie przekazanie: pompa ma
     * jednego właściciela, a `LineSink` jest klonowalny właśnie dlatego, że sypie do niej kilku
     * producentów naraz. */
    deps.control.lines_go_to(lines.clone());
    let report = the_whole_run(deps, request, lines, slots, options).await;
    // Jawnie, a nie na końcu ramki: kolejność „najpierw cisza, potem dowód" ma zostać widoczna
    // w tym miejscu, w którym była (`Settling` trzyma ją u siebie).
    drop(settling);
    report
}

/// Rejestr ocalałych widziany przez sterownik JEDNEGO kroku — razem z miejscem tego kroku w puli.
///
/// # Po co to istnieje, skoro `Processes` już implementuje ten trait
///
/// 2026-09 (Z-4) — bo nieudany start vendora zostawia grupę, której permit należy do KROKU, a nie
/// do sterownika. Krok bierze miejsce z puli **przed** `AgentDriver::start` (`Live::step`), więc
/// oddanie ocalałego prosto do [`processes::Processes`] wkładało go tam z `slot: None`: rejestr
/// trzymał właściciela, a miejsce wracało do puli razem z ramką kroku — czyli następny agent po
/// ~583 MB startował obok grupy, która dalej pali limit u dostawcy (niezmiennik 11). Ta wartość
/// jest tym jednym miejscem, w którym permit kroku i ocalały sterownika się spotykają.
///
/// Niesie też adres z powrotem, bo krok musi mieć co zapisać człowiekowi: `pgid` do `run.json`
/// i zdanie o ocalałym do historii. Bez tego z tej jednej drogi człowiek nie dowiedziałby się
/// o żywej grupie ani słowem (niezmiennik 29).
///
/// Oba `std::sync::Mutex` są brane i oddawane w jednej instrukcji i **nigdy przez `await`**
/// (niezmiennik 8): [`KeepsLeftovers::keep_leftover`] jest synchroniczne z definicji.
#[derive(Debug)]
struct StepLeftovers {
    processes: Arc<processes::Processes>,
    /// Miejsce z puli tego kroku, dopóki nikt go stąd nie zabrał.
    seat: Mutex<Option<limits::Slot>>,
    /// Adres grupy, którą sterownik tu zostawił — albo `None`, kiedy niczego nie zostawił.
    left: Mutex<Option<GroupId>>,
}

impl StepLeftovers {
    /// Trzyma miejsce kroku na czas startu sterownika.
    fn holding(processes: Arc<processes::Processes>, seat: Option<limits::Slot>) -> Self {
        Self {
            processes,
            seat: Mutex::new(seat),
            left: Mutex::new(None),
        }
    }

    /// Miejsce wraca do kroku — chyba że zabrał je ocalały, i wtedy tu już nic nie ma.
    fn take_back(&self) -> Option<limits::Slot> {
        self.seat
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Adres grupy, którą sterownik zostawił po nieudanym starcie.
    fn left_behind(&self) -> Option<GroupId> {
        *self.left.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl KeepsLeftovers for StepLeftovers {
    fn keep_leftover(&self, owner: Box<dyn Leftover>, slot: Option<limits::Slot>) {
        let address = owner.address();
        // Miejsce podane przez sterownik wygrywa, a kiedy go nie podał — jedzie miejsce KROKU.
        // Dzisiejszy `codex.rs` nie podaje żadnego i nie ma skąd: permit należy do warstwy wyżej.
        let seat = slot.or_else(|| self.take_back());
        if address.is_some() {
            *self.left.lock().unwrap_or_else(PoisonError::into_inner) = address;
        }
        self.processes.keep_leftover(owner, seat);
    }
}

/// Gwardia, która oddaje bieg NA KAŻDEJ drodze wyjścia: cichnie i zapala dowód zejścia,
/// jakkolwiek by się ta ramka skończyła.
///
/// # Po co to istnieje, skoro te dwie linie już tam stały
///
/// 2026-09 (Z-4) — stały ZA `await`, więc widziały wyłącznie powrót. Panika w zadaniu biegu
/// (indeks, `expect`, cudzy `unwrap` w bibliotece) i każde PORZUCENIE tego future'a przechodziły
/// obok nich: `settled` zostawało niezapalone, [`stop_if_anything_is_going`] czekało na dowód,
/// którego nikt już nie zapali, a zapadka folderu ([`crate::ipc::AppState::begin_run`]) odmawiała
/// każdego następnego Startu — aż do restartu Loadouta. Człowiek widział „A run is already
/// going… Press Stop first", naciskał Stop i okno przestawało odpowiadać.
///
/// KOLEJNOŚĆ JEST TREŚCIĄ i jest dokładnie ta, która była: najpierw porzucenie nadajnika, potem
/// dowód. Pompa kończy się na zamkniętej kolejce, czyli dopiero wtedy, gdy zniknie każdy
/// `LineSink` — a klon biegu siedzi w uchwycie (powód w całości przy `RunControl::lines_go_quiet`).
///
/// `[profile.release] panic = "abort"` (`Cargo.toml`) znaczy, że w wydanej aplikacji panika ubija
/// proces, zanim `Drop` zdąży cokolwiek zrobić — i to jest świadome, profilu nie ruszamy. Ta
/// gwardia broni więc buildu deweloperskiego (`npm run app`) oraz — w OBU profilach — każdej
/// drogi, na której future biegu zostaje porzucone zamiast dokończone.
///
/// Klon uchwytu, nie pożyczka: `RunControl` jest `Arc` w środku, a gwardia z czasem życia
/// zmusiłaby każdą z trzech dróg startu do parametru, którego żadna z nich nie potrzebuje.
#[derive(Debug)]
struct Settling(RunControl);

impl Drop for Settling {
    fn drop(&mut self) {
        self.0.lines_go_quiet();
        self.0.settle();
    }
}

/// Jeden agent, jedno zdanie — żądanie biegu jednokrokowego.
///
/// # Dlaczego to nie jest [`RunRequest`]
///
/// Tamta struktura niesie `workflow: PathBuf`, czyli PLIK, i to jest jej cała treść: bieg nie
/// ufa UI, więc jedyne, co o planie wiadomo na pewno, to gdzie leży. Tutaj planu nie ma —
/// jednostką jest definicja agenta z biblioteki, a nazwa pliku byłaby zmyśleniem, po którym
/// wznowienie szukałoby kiedyś workflow, którego nikt nigdy nie zapisał.
///
/// Osobny typ, a nie pole `Option` w [`RunRequest`], bo `src-tauri/src/commands/mod.rs` nie
/// należy do T-62 (`AGENTS.md` §7) — a i tak byłby to jeden typ z dwoma znaczeniami, czyli
/// para, którą prędzej czy później ktoś zamieni miejscami.
#[derive(Debug, Clone)]
pub struct AskRequest {
    /// Identyfikator agenta z biblioteki — ten sam, którym nazywa go krok pliku workflow.
    ///
    /// Identyfikator, nie nazwa: przeżywa zmianę nazwy (T3 §3.1), a wiersz wejścia i tak
    /// tłumaczy wpisane słowo na identyfikator, zanim tu dojedzie.
    pub agent: String,
    /// Zdanie człowieka — CO ten agent ma zrobić.
    ///
    /// Puste znaczy „nic nie kazano" i jest odmową po stronie wiersza wejścia, nie tutaj:
    /// agent bez polecenia to tura, za którą ktoś płaci, choć nikt o nic nie zapytał.
    pub task: String,
    /// Ile kroków ma **naprawdę** działać naraz — ta sama liczba, co przy biegu z pliku.
    ///
    /// Jest tu, a nie w stałej `1`, bo bieg jednokrokowy bierze miejsce z TEJ SAMEJ puli
    /// (niezmiennik 11). Bieg, który zna swój limit z definicji, jest biegiem, który idzie
    /// obok puli — a wtedy człowiek ustawia trzech i pracuje piątka.
    pub how_many_at_once: usize,
}

/// Uruchamia JEDNEGO agenta z jednym zdaniem — zwykłym biegiem, nie drugą maszynerią.
///
/// Katalog `runs/<ts>__<id>/`, `run.json`, miejsce w puli i dowód śmierci grupy na końcu: to
/// wszystko przychodzi stąd, bo bieg jednokrokowy JEST biegiem. Druga ścieżka wykonania —
/// „lekki tryb bez katalogu" — byłaby dokładnie tym, co `docs/ARCHITECTURE.md` opisuje jako
/// osiem rodzajów autorytetu w repo źródłowym.
///
/// Pulę robi sobie sam, dokładnie jak [`run_workflow_inner`], i z dokładnie tą samą wadą:
/// wołający, który ma pulę wspólną, wchodzi [`run_agent_with_slots`].
pub async fn run_agent_inner(
    deps: &RunDeps<'_>,
    ask: &AskRequest,
    lines: LineSink,
) -> Result<RunReport, RunError> {
    run_agent_with_budget(deps, ask, lines, None).await
}

/// Ten sam bieg jednokrokowy, z sufitem wydatku — albo bez niego.
///
/// `/ask` jest zwykłym biegiem, więc obowiązuje go ten sam sufit i ta sama pula. Bieg
/// jednokrokowy z własnym limitem byłby drugą odpowiedzią na pytanie „ile wolno wydać".
pub async fn run_agent_with_budget(
    deps: &RunDeps<'_>,
    ask: &AskRequest,
    lines: LineSink,
    budget_usd: Option<f64>,
) -> Result<RunReport, RunError> {
    let slots = the_pool_of_this_application(deps, ask.how_many_at_once);
    the_whole_single_agent(deps, ask, lines, slots, budget_usd).await
}

/// Ten sam bieg jednokrokowy, tylko miejsce bierze ze **wspólnej puli aplikacji**.
///
/// Cała treść niezmiennika 11 w tym zadaniu siedzi w tym, że ta funkcja istnieje i że nie
/// zakłada semafora sama: dwa `/ask` przy puli trzech to dalej najwyżej trzech pracujących
/// agentów. Bieg jednokrokowy, który omija limiter, wygląda jak wygoda („to tylko jeden
/// agent") i znaczy, że `atOnce` przestaje być prawdą o maszynie.
///
/// **`deps.control.settle()` musi zostać na KAŻDEJ drodze wyjścia**, także po odmowie — powód
/// w całości stoi przy [`run_workflow_with_slots`] i jest tu dokładnie ten sam: na to zdanie
/// czeka [`stop_run_inner`], żeby móc wrócić z dowodem (niezmiennik 6). Dlatego cały bieg
/// siedzi w [`the_whole_ask`]: stamtąd wychodzi się kilkoma `?`, a stąd jednym `return`.
pub async fn run_agent_with_slots(
    deps: &RunDeps<'_>,
    ask: &AskRequest,
    lines: LineSink,
    slots: Limiter,
) -> Result<RunReport, RunError> {
    the_whole_single_agent(deps, ask, lines, slots, None).await
}

/// Jedno ciało obu dróg wyżej — dokładnie jak [`the_whole_workflow`] przy biegu z pliku.
async fn the_whole_single_agent(
    deps: &RunDeps<'_>,
    ask: &AskRequest,
    lines: LineSink,
    slots: Limiter,
    budget_usd: Option<f64>,
) -> Result<RunReport, RunError> {
    // Kolejność i powód każdej z tych linii stoją przy `run_workflow_with_slots`. Ta sama czwórka,
    // nie jej wariant: uchwyt biegu odpowiada na pytanie „czy jest co zatrzymywać" tak samo dla
    // obu rodzajów biegu, bo Stop nie wie, którym z nich jest ten, który idzie. Od 2026-09 (Z-4)
    // ta sama czwórka znaczy też tę samą gwardię: `/ask`, który przewraca się po drodze, ma zejść
    // dokładnie tak, jak bieg z pliku.
    deps.control.begin();
    let settling = Settling(deps.control.clone());
    deps.control.lines_go_to(lines.clone());
    let report = the_whole_ask(deps, ask, lines, slots, budget_usd).await;
    drop(settling);
    report
}

/// Bieg od wczytania pliku do zamknięcia księgi. Wydzielony z [`run_workflow_inner`], żeby
/// dowód z `settle()` schodził dokładnie raz, niezależnie od tego, którym `?` się stąd wyszło.
async fn the_whole_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    slots: Limiter,
    options: WorkflowRunOptions,
) -> Result<RunReport, RunError> {
    let WorkflowRunOptions {
        budget_usd,
        lead_start,
        reflection_enabled,
        before_stamp,
        faults,
    } = options;
    let plan = if let Some(start) = &lead_start {
        plan_run_with_identity(
            deps,
            request,
            Uuid::now_v7().to_string(),
            now_ms(),
            None,
            Some(start),
        )?
    } else {
        plan_run(deps, request)?
    };
    the_planned_run_with_prestart(
        deps,
        plan,
        lines,
        slots,
        PlannedRunOptions {
            acceptance: None,
            lead_start,
            budget_usd,
            reflection_enabled,
            before_stamp,
        },
        faults,
    )
    .await
}

/// Claim triggera przechodzi przez ten sam plan i wykonanie, a ledger oplata tylko dwie
/// atomowe granice: zwiazanie przed katalogiem i akceptacje po pierwszym `run.json`.
async fn the_whole_triggered_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    claim: &TriggerClaim,
    lines: LineSink,
    slots: Limiter,
    faults: Arc<dyn PrestartFaultInjector>,
    budget_usd: Option<f64>,
) -> Result<TriggerRunReport, RunError> {
    let delivery = triggers::claimed_delivery(deps.home, claim)?;
    let requested = request.workflow.file_name().and_then(|name| name.to_str());
    if requested != Some(claim.workflow.as_str()) {
        return Err(triggers::TriggerError::InvalidClaim.into());
    }

    let run_dir = run_directory(deps.project, &delivery.claim.run_id, delivery.created_at);
    // Ten dowod stoi przed `bind`: symlink przygotowany pod prealokowanym UUID nie moze nawet
    // przejsc ledgera z Pending do Bound, a tym bardziej zapisac czegos poza projektem.
    if let Err(problem) = prove_run_candidate(deps.project, &run_dir) {
        // Bound bez dowiedzionego katalogu nie ma pliku, ktoremu wolno zaufac. Cofamy go tak
        // samo jak odmowe planu, zeby po usunieciu obcego linku ten sam claim/UUID mogl wrocic.
        triggers::release_delivery(deps.home, claim)?;
        return Err(RunError::Io(io::Error::other(problem.to_string())));
    }
    let run_file = run_dir.join(RUN_FILE);
    triggers::bind_delivery(deps.home, claim, &run_file)?;
    // Reconcile czyta `run.json` dopiero po dowodzie sciezki i idempotentnym bindzie. Inaczej
    // stary Bound wskazujacy przez symlink moglby zaakceptowac podrobiony plik poza biegiem.
    match triggers::reconcile_delivery(deps.home, claim, |bound_file| {
        read_and_sync_run_file(deps.project, bound_file)
    })? {
        DeliveryState::Accepted { run_file, .. } => {
            return Ok(TriggerRunReport::AlreadyAccepted {
                id: claim.run_id.clone(),
                run_file,
            });
        }
        DeliveryState::Bound {
            run_file: ref found,
        } if found == &run_file => {}
        DeliveryState::Pending | DeliveryState::Bound { .. } | DeliveryState::Cancelled => {
            return Err(triggers::TriggerError::InvalidClaim.into());
        }
    }
    let plan = match plan_triggered_run(deps, request, &delivery) {
        Ok(plan) => plan,
        Err(error) => {
            triggers::release_delivery(deps.home, claim)?;
            return Err(error);
        }
    };
    let acceptance = TriggerAcceptance {
        home: deps.home.to_path_buf(),
        claim: claim.clone(),
        bound_prestart: BoundPrestart {
            run_id: claim.run_id.clone(),
            run_file: run_file.clone(),
        },
    };
    match the_planned_run_with_prestart(
        deps,
        plan,
        lines,
        slots,
        PlannedRunOptions {
            acceptance: Some(acceptance),
            lead_start: None,
            budget_usd,
            reflection_enabled: learn_from_runs(deps),
            before_stamp: None,
        },
        faults,
    )
    .await
    {
        Ok(report) => Ok(TriggerRunReport::Ran(report)),
        Err(error) if !run_file.exists() => {
            triggers::release_delivery(deps.home, claim)?;
            Err(error)
        }
        Err(error) => Err(error),
    }
}

/// To samo dla biegu jednokrokowego: plan powstaje z definicji agenta, a nie z pliku.
///
/// Dwie linie i ani jednej decyzji więcej — cała różnica między `/ask` i `/run` mieści się
/// w tym, KTO rozpisuje plan. Wszystko, co dalej, jest dosłownie tym samym wykonaniem
/// ([`the_planned_run`]), bo bieg jednokrokowy **jest** biegiem: druga ścieżka wykonania byłaby
/// tym, co `docs/ARCHITECTURE.md` opisuje jako osiem rodzajów autorytetu w repo źródłowym.
async fn the_whole_ask(
    deps: &RunDeps<'_>,
    ask: &AskRequest,
    lines: LineSink,
    slots: Limiter,
    budget_usd: Option<f64>,
) -> Result<RunReport, RunError> {
    the_planned_run(
        deps,
        plan_ask(deps, ask)?,
        lines,
        slots,
        PlannedRunOptions {
            acceptance: None,
            lead_start: None,
            budget_usd,
            reflection_enabled: learn_from_runs(deps),
            before_stamp: None,
        },
    )
    .await
}

struct TriggerAcceptance {
    home: PathBuf,
    claim: TriggerClaim,
    bound_prestart: BoundPrestart,
}

#[derive(Clone)]
struct BoundPrestart {
    run_id: String,
    run_file: PathBuf,
}

struct PlannedRunOptions {
    lead_start: Option<super::lead_start::LeadStart>,
    acceptance: Option<TriggerAcceptance>,
    budget_usd: Option<f64>,
    reflection_enabled: bool,
    before_stamp: Option<FrozenPromptHook>,
}

struct WorkflowRunOptions {
    lead_start: Option<super::lead_start::LeadStart>,
    budget_usd: Option<f64>,
    reflection_enabled: bool,
    before_stamp: Option<FrozenPromptHook>,
    faults: Arc<dyn PrestartFaultInjector>,
}

struct PreparationOptions {
    acceptance: Option<TriggerAcceptance>,
    budget_usd: Option<f64>,
    before_stamp: Option<FrozenPromptHook>,
    faults: Arc<dyn PrestartFaultInjector>,
}

/// Rozpisany plan → katalog, kroki, indeks. **Jedna droga wykonania na oba rodzaje biegu.**
///
/// Wydzielone 2026-08-20 (T-62) z [`the_whole_run`], bez zmiany ani jednej linii w środku:
/// od tego miejsca w dół nie ma jak zapytać, czy plan przyszedł z pliku, czy z jednego zdania
/// w wierszu wejścia — i to jest jedyny sposób, żeby „`/ask` to zwykły bieg" było własnością
/// kodu, a nie zdaniem w komentarzu.
async fn the_planned_run(
    deps: &RunDeps<'_>,
    plan: Plan,
    lines: LineSink,
    slots: Limiter,
    options: PlannedRunOptions,
) -> Result<RunReport, RunError> {
    the_planned_run_with_prestart(
        deps,
        plan,
        lines,
        slots,
        options,
        Arc::new(NoPrestartFaults),
    )
    .await
}

async fn the_planned_run_with_prestart(
    deps: &RunDeps<'_>,
    plan: Plan,
    lines: LineSink,
    slots: Limiter,
    options: PlannedRunOptions,
    faults: Arc<dyn PrestartFaultInjector>,
) -> Result<RunReport, RunError> {
    let PlannedRunOptions {
        acceptance,
        lead_start,
        budget_usd,
        reflection_enabled,
        before_stamp,
    } = options;
    let (live, isolated, dag) = prepare_planned_run(
        deps,
        plan,
        lines,
        slots,
        PreparationOptions {
            acceptance,
            budget_usd,
            before_stamp,
            faults,
        },
    )
    .await?;
    // Dopiero ta granica oznacza trwały run.json oraz przejęte ownership przygotowania.
    // Utrata odpowiedzi mostu nie odbiera AppState uchwytu ani nie odwołuje przyjętego biegu.
    deps.control
        .set_run_address(live.plan.id.clone(), live.plan.dir.clone());
    if let Some(start) = lead_start {
        start.prepared(deps.project, &live.plan.id);
    }
    let cancel = deps.control.cancel_token();
    let outcome = run_planned_graph(Arc::clone(&live), &dag, cancel.clone()).await;
    finish_planned_run(deps, live, isolated, outcome, cancel, reflection_enabled).await
}

/// To, czego przygotowanie biegu potrzebuje z [`RunDeps`] — **na własność**.
///
/// 2026-09 (Z-10): całe przygotowanie biegnie na puli blokującej, a `spawn_blocking` żąda
/// `'static`. Pożyczka `&RunDeps` tego nie spełnia; te trzy pola spełniają, bo każde jest albo
/// ścieżką, albo uchwytem z `Arc` w środku — więc klon nie kopiuje ani jednego bajta stanu.
struct WhatPreparingNeeds {
    project: PathBuf,
    control: RunControl,
    processes: std::sync::Arc<crate::commands::processes::Processes>,
}

async fn prepare_planned_run(
    deps: &RunDeps<'_>,
    plan: Plan,
    lines: LineSink,
    slots: Limiter,
    options: PreparationOptions,
) -> Result<(Arc<Live>, Vec<Isolated>, Dag), RunError> {
    if plan.inputs.configuration.isolate_contexts {
        // Rzeczywisty, niepłatny proces dowodzi zdolności platformy przed pierwszym agentem.
        supervisor::FilesystemFence::new(Vec::new(), Vec::new(), vec![plan.project.clone()])?
            .prove_available()
            .await?;
    }
    /* CAŁE PRZYGOTOWANIE JEDZIE NA PULĘ BLOKUJĄCĄ — RAZEM Z GWARDIĄ (2026-09, Z-10).
     *
     * `lay_out_the_run_dir` zakłada drzewo pracy każdego kroku, a to jest `git worktree add`,
     * czyli PEŁNY CHECKOUT repozytorium — plus kopia plikowa dla projektu bez gita. Stało to
     * dotąd w linii, więc Start trzymał wątek okna od kliknięcia do pierwszego procesu agenta:
     * zmierzone na monorepo właściciela kilka sekund.
     *
     * WCHODZI TU CAŁE CIAŁO, NIE SAM LAYOUT, i to jest treść tej linii. [`ProvisionalRun`] jest
     * uzbrojony przez cały czas przygotowania, a jego `Drop` **sprząta gitem**
     * (`cleanup_provisional_git_tree`, `remove_dir_all`). Wersja, która oddawała uzbrojoną
     * gwardię z powrotem na worker, przenosiła więc całą pracę tylko dla biegu UDANEGO: każde
     * `?` po drodze — nieudany seed, odmowa pożyczki, brak umiejętności, odmowa akceptacji
     * triggera — kasowało drzewo robocze i katalog biegu z powrotem na wątku tokio. Na worker
     * wraca od dziś wyłącznie wynik: albo gotowy bieg (gwardia rozbrojona), albo `RunError`
     * (gwardia zdjęta razem z całym sprzątaniem, po tamtej stronie).
     *
     * `JoinError` znaczy panikę zadania blokującego (`panic` jest w tym drzewie `deny`); gwardia
     * schodzi wtedy przy odwijaniu stosu — dalej na wątku puli. */
    let needs = WhatPreparingNeeds {
        project: deps.project.to_path_buf(),
        control: deps.control.clone(),
        processes: Arc::clone(&deps.processes),
    };
    tokio::task::spawn_blocking(move || {
        everything_before_the_first_process(needs, plan, lines, slots, options)
    })
    .await
    .map_err(|joined| {
        /* PANIKA WRACA PANIKĄ, a nie odmową Startu (2026-09, Z-10). Pula blokująca łapie panikę
         * swojego zadania i oddaje ją jako `JoinError` — zamienienie jej tutaj w `RunError`
         * schowałoby WADĘ KODU za zdaniem „nie udało się zacząć", czyli za czymś, co człowiek
         * czyta jako swoją pomyłkę. Podnosimy ją więc z powrotem na wątku wołającego, dokładnie
         * tam, gdzie wychodziła, zanim ta praca zeszła z workera; gwardia biegu (`Settling`)
         * stoi nad tą ramką, więc `settle()` pada tak samo i Stop dalej odpowiada — mierzy to
         * `a_run_always_settles::a_panic_in_the_run_body_still_settles_so_stop_answers`. */
        if joined.is_panic() {
            std::panic::resume_unwind(joined.into_panic());
        }
        RunError::Io(io::Error::other(joined))
    })?
}

/// Ciało [`prepare_planned_run`], wykonywane w CAŁOŚCI na puli blokującej.
///
/// Synchroniczne od początku do końca i takie ma zostać: gwardia prowizoryczna nie ma prawa
/// przekroczyć granicy wątku w stanie uzbrojonym — powód w całości stoi u wołającego.
fn everything_before_the_first_process(
    needs: WhatPreparingNeeds,
    mut plan: Plan,
    lines: LineSink,
    slots: Limiter,
    options: PreparationOptions,
) -> Result<(Arc<Live>, Vec<Isolated>, Dag), RunError> {
    let WhatPreparingNeeds {
        project,
        control,
        processes,
    } = needs;
    let PreparationOptions {
        acceptance,
        budget_usd,
        before_stamp,
        faults,
    } = options;
    // Ostatnia obrona przed cyklem stoi przed pierwszym artefaktem biegu.
    let dag = Dag::new(plan.steps.len(), &plan.arrows)?;
    faults
        .before_run_directory(&plan.dir)
        .map_err(RunError::Io)?;
    let bound_prestart = acceptance
        .as_ref()
        .map(|acceptance| acceptance.bound_prestart.clone());
    let mut provisional = ProvisionalRun::new(project.clone(), Arc::clone(&faults), bound_prestart);
    let isolated = lay_out_the_run_dir(&mut plan, &project, &mut provisional)?;
    bring_recorded_sources(&mut plan)?;
    bring_project_instructions(&mut plan, &project)?;
    plan.memory_sources.save_to(&plan.dir)?;
    save_context_sources(&plan)?;
    // Wznowienie kopiuje trwałe pliki, potem dopiero buduje z nich indeks promptu. Odwrotna
    // kolejność zostawia pliki w katalogu, ale nie daje do nich drogi żadnemu agentowi.
    seed_the_handoffs(&plan)?;
    provisional.check(PrestartFaultPoint::AfterHandoffSeed)?;
    plan.carried = what_the_run_before_left(&plan);
    say_what_was_left_behind(&lines, &isolated);
    say_the_folder_is_inside_a_repo(&lines, &project, &plan);
    // Pożyczki i umiejętności piszą pod nowym katalogiem biegu; planowanie pozostaje czyste.
    bring_in_what_each_step_borrowed(&mut plan, &project)?;
    // ZARAZ PO PRZEPISANIU, bo dopiero teraz jest o czym mówić: przegląd cudzego tekstu biegnie
    // w tamtej funkcji, a jego ciężkie znalezisko zabrało bieg jeszcze przy planowaniu.
    say_what_the_borrowed_text_carries(&lines, &plan);
    provisional.check(PrestartFaultPoint::AfterBorrow)?;
    hand_the_skills_to_the_steps(&mut plan)?;
    if plan
        .replay
        .as_ref()
        .is_some_and(|replay| replay.mode == super::replay::ReplayMode::Recorded)
    {
        plan.skills = what_this_run_froze(&plan.steps)?;
    }
    provisional.check(PrestartFaultPoint::AfterSkills)?;
    plan.protection = protection::prepare(&mut plan)?;
    control.set_external_messages(plan.inputs.configuration.effects.external_messages);
    let live = Arc::new(Live::new(
        plan,
        lines,
        control,
        slots,
        processes,
        budget_usd,
        Arc::clone(&faults),
    ));
    // Bez pierwszego trwałego zrzutu żaden proces nie rusza (niezmiennik 4).
    provisional.check(PrestartFaultPoint::BeforeFirstRunFile)?;
    let run_file = live.plan.dir.join(RUN_FILE);
    provisional.will_create_run_file(run_file.clone(), live.plan.id.clone())?;
    let (exact_run_file, run_file_identity) = live.open_the_book()?;
    // ID nie jest provenance: cleanup porównuje komplet bajtów i opaque identity pierwszej
    // publikacji; każda niezgodność zachowuje zarówno receipt, jak i jego katalog.
    provisional.authenticate_created_run_file(
        &run_file,
        &live.plan.id,
        exact_run_file,
        run_file_identity,
    )?;
    provisional.check(PrestartFaultPoint::AfterFirstRunFile)?;
    /* P-01 (incydent I-08): SĄSIEDNIE REPO NAZWANE, ZANIM RUSZY PIERWSZY PROCES.
     *
     * Krok pracujący we własnej kopii dostaje `.loadout/runs/<bieg>/work/<krok>`, więc
     * zależność `path = "../murmur-server"` wskazuje mu katalog, którego tam nie ma i nie
     * będzie. Bez tego zdania agent czyta wyłącznie błąd Cargo — prawdziwy i milczący o kopii —
     * i pali tury na ratowanie środowiska zamiast na swoją robotę.
     *
     * ZDANIE, NIE ODMOWA. Loadout nie wie, czy ten krok w ogóle zbuduje ten manifest, a odmowa
     * biegu na podstawie manifestu, którego nikt może nie tknąć, kosztowałaby więcej niż
     * ostrzeżenie. Wciągnięcia sąsiada nie proponujemy jako czynności Loadouta: to jest
     * decyzja człowieka o zakresie odczytu cudzego katalogu. */
    live.say_which_neighbours_will_be_missing();
    if let Some(before_stamp) = before_stamp {
        let (id, job) = live
            .plan
            .steps
            .iter()
            .enumerate()
            .find_map(|(id, step)| match &step.job {
                Job::Agent(job) => Some((id, job.as_ref())),
                Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => None,
            })
            .ok_or_else(|| io::Error::other("the before-stamp workflow has no agent prompt"))?;
        // 2026-08-26 (T-137): używamy dokładnie tego samego kompozytora co późniejszy
        // `RunSpec`; osobne składanie w szwie mogłoby dowodzić promptu, którego sterownik nie dostał.
        let frozen = live
            .prompt_for(id, &job.prompt, &job.context, job.minutes)
            .map_err(|error| io::Error::other(error.to_string()))?;
        before_stamp(&frozen.prompt);
    }
    // Stempel jest uczciwy dopiero po powstaniu biegu, lecz przed pierwszym procesem: bierze
    // dokładnie zestaw notatek już zamrożony w `run.json`. Księga jest właścicielem receipt,
    // bo od T-130 dopisuje do niego fizycznych odbiorców podczas biegu.
    let book = live.book();
    if live.plan.inputs.configuration.effects.publish_memory
        && !live
            .plan
            .replay
            .as_ref()
            .is_some_and(|replay| replay.mode == super::replay::ReplayMode::Recorded)
    {
        stamp_what_this_run_carried(&book.memory);
    }
    drop(book);
    provisional.check(PrestartFaultPoint::OwnershipTransferred)?;
    // Akceptacja jest ostatnią fallible operacją przed przekazaniem ownershipu. Gdyby stała
    // wcześniej, późniejsza odmowa wycofałaby `run.json`, ale ledger zostałby `Accepted` i
    // wskazywał na nieistniejący bieg z zerem startów.
    if let Err(failure) = accept_trigger_after_durable_run(&project, &live, acceptance) {
        if failure.preserve_resources {
            provisional.disarm();
        }
        return Err(failure.error);
    }
    provisional.disarm();
    Ok((live, isolated, dag))
}

fn save_context_sources(plan: &Plan) -> io::Result<()> {
    if let Some(context) = &plan.context_sources {
        // 2026-09-08 (CT-06) — własna kopia musi istnieć przed ogrodzeniem chronionych kroków;
        // późniejszy odczyt biblioteki zamieniałby bieg w ruchomy cel.
        context.save_to(&plan.dir)?;
    }
    Ok(())
}

fn accept_trigger_after_durable_run(
    project: &Path,
    live: &Live,
    acceptance: Option<TriggerAcceptance>,
) -> Result<(), TriggerAcceptanceFailure> {
    let Some(acceptance) = acceptance else {
        return Ok(());
    };
    let run_file = live.plan.dir.join(RUN_FILE);
    // Sam atomowy rename nie wystarcza recovery: fsync pliku i katalogu jest dowodem trwałości.
    match read_and_sync_run_file(project, &run_file) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err(TriggerAcceptanceFailure::rollback(io::Error::new(
                io::ErrorKind::NotFound,
                "the run file disappeared before it could be saved safely",
            )));
        }
        Err(error) => return Err(TriggerAcceptanceFailure::rollback(error)),
    }
    let Err(accept_error) =
        triggers::accept_delivery(&acceptance.home, &acceptance.claim, &run_file, now_ms())
    else {
        return Ok(());
    };
    let state = triggers::reconcile_delivery(&acceptance.home, &acceptance.claim, |_| {
        Ok::<Option<Vec<u8>>, io::Error>(None)
    });
    match state {
        Ok(DeliveryState::Accepted {
            run_file: ref found,
            ..
        }) if found == &run_file => Ok(()),
        Ok(DeliveryState::Bound {
            run_file: ref found,
        }) if found == &run_file => Err(TriggerAcceptanceFailure {
            error: accept_error.into(),
            preserve_resources: false,
        }),
        // Zapis atomowy może zwrócić błąd dopiero po rename. Jeśli późniejszy odczyt nie umie
        // rozstrzygnąć, czy ledger jest Bound czy Accepted, skasowanie receipt stworzyłoby
        // najgorszy stan: Accepted wskazujące na brakujący bieg. Zachowujemy oba artefakty,
        // a istniejący reconcile rozliczy je przy kolejnym wejściu.
        Ok(
            DeliveryState::Pending
            | DeliveryState::Bound { .. }
            | DeliveryState::Accepted { .. }
            | DeliveryState::Cancelled,
        )
        | Err(_) => Err(TriggerAcceptanceFailure {
            error: accept_error.into(),
            preserve_resources: true,
        }),
    }
}

struct TriggerAcceptanceFailure {
    error: RunError,
    preserve_resources: bool,
}

impl TriggerAcceptanceFailure {
    fn rollback(error: io::Error) -> Self {
        Self {
            error: error.into(),
            preserve_resources: false,
        }
    }
}

async fn run_planned_graph(
    live: Arc<Live>,
    dag: &Dag,
    cancel: CancellationToken,
) -> scheduler::Outcome {
    let run_step = {
        let live = Arc::clone(&live);
        move |id: StepId, cancel: CancellationToken, started: scheduler::Started| {
            let live = Arc::clone(&live);
            async move { live.step(id, cancel, started).await }
        }
    };
    let route_after = {
        let live = Arc::clone(&live);
        move |id: StepId, _report: StepReport| live.route_after(id)
    };
    // Semafor planisty celowo nie ogranicza: każdy krok bierze permit ze wspólnej puli
    // aplikacji w `Live::a_slot_for_this_step` (niezmiennik 11). Dlatego droga jest
    // „start-aware" (2026-09, Z-30): permit tego semafora nie jest tu startem i krok, który
    // stoi w kolejce po prawdziwe miejsce, nie ma prawa czytać się jako `running`.
    scheduler::execute_routed_with_start(dag, dag.len(), cancel, run_step, route_after).await
}

async fn finish_planned_run(
    deps: &RunDeps<'_>,
    live: Arc<Live>,
    isolated: Vec<Isolated>,
    outcome: scheduler::Outcome,
    cancel: CancellationToken,
    reflection_enabled: bool,
) -> Result<RunReport, RunError> {
    // Sufit dostawcy znaczy `skipped`, a tylko token człowieka znaczy `cancelled`.
    let mut states = outcome.states;
    live.name_what_the_budget_stopped(&mut states);
    // WF-25: koniec grafu zamyka wyłącznie usługi o czasie życia tego biegu. Window-owned
    // zostają widoczne w Processes; ich lease odracza domknięcie właściwej kopii.
    let stopped = live.processes.stop_run(deps.project, &live.plan.id).await;
    let (proved, warning) = match stopped {
        Ok(report) => {
            let proved = report
                .proofs
                .iter()
                .all(|proof| matches!(proof, GroupProof::Dead { .. }));
            (proved, (!proved).then(|| "A background process could not be confirmed stopped. Its working folder was kept. Use Stop again before removing it.".to_owned()))
        }
        Err(error) => (
            false,
            Some(format!(
                "Loadout could not confirm that this run's background processes stopped: {error}"
            )),
        ),
    };
    live.update(|book| {
        for (at, step) in live.plan.steps.iter().enumerate() {
            if matches!(&step.job, Job::Serve(job) if job.lifetime == crate::workflow::ServiceLifetime::Run)
                && book.steps[at].execution.executed
            {
                book.steps[at].death_proof = proved;
                if let Some(warning) = &warning { book.steps[at].error = Some(warning.clone()); }
            }
        }
    });
    live.close_the_book(&states, outcome.cancelled);
    close_the_trees(deps.project, &isolated, &live).await;
    deps.store.rebuild_from(&live.plan.dir).await?;
    // Auto-pamięć kroków i refleksja czytają skończony, posprzątany i już zindeksowany bieg.
    if live.plan.inputs.configuration.effects.publish_memory {
        what_the_steps_wrote_down(deps, &live.plan);
    }
    let cancelled_before_reflection = outcome.cancelled || cancel.is_cancelled();
    let reflection = if cancelled_before_reflection {
        ReflectionReceipt::not_asked(NotAsked::Stopped)
    } else if reflection_enabled && live.plan.inputs.configuration.effects.publish_memory {
        // Sufit tury liczy się z tego, co bieg NAPRAWDĘ wydał na kroki, a materiał — z tego, co
        // kroki po sobie zostawiły; jedno i drugie jest znane dopiero tutaj, po
        // `close_the_book` (2026-09, Z-38).
        what_this_run_taught_us(
            deps,
            &live.plan,
            &states,
            live.spent_so_far(),
            &live.what_the_steps_said(),
        )
        .await?
    } else {
        ReflectionReceipt::not_asked(NotAsked::TurnedOff)
    };
    say_if_the_reflection_ran_out(&live.lines, &live.plan, &reflection);
    // 2026-09 (Z-18): drugi odczyt zachowuje późny Stop, który padł dopiero na żywej grupie
    // refleksji; pierwszy odczyt wyżej broni przed uruchomieniem jej po Stopie schedulera.
    let cancelled = cancelled_before_reflection || cancel.is_cancelled();
    live.update(|book| {
        book.reflection = reflection;
        if cancelled {
            book.status = RunState::Cancelled;
        }
    });

    // WF-10: wartość z rzeczywistych wyników terminalnych, nie z samego zwrotu zadania.
    // Window-owned Serve świadomie żyje dalej. Każdy wykonany Agent/Check oraz Run-owned
    // Serve musi mieć dowód; odmowa/refleksja/przerwane domknięcie wcześniej zostawia Unknown.
    let groups_proved = {
        let book = live.book.lock().unwrap_or_else(PoisonError::into_inner);
        book.steps.iter().enumerate().all(|(at, step)| {
            !step.execution.executed
                || match &live.plan.steps[at].job {
                    Job::Ask { .. } => true,
                    Job::Serve(job) if job.lifetime == crate::workflow::ServiceLifetime::Window => {
                        true
                    }
                    _ => step.death_proof,
                }
        })
    };
    deps.control.record_stop_proof(groups_proved && proved);

    Ok(RunReport {
        id: live.plan.id.clone(),
        dir: live.plan.dir.clone(),
        outcome: if cancelled {
            Outcome::Cancelled
        } else {
            Outcome::Done
        },
        steps: states,
    })
}

/// Zapisuje w każdej niesionej notatce, że **właśnie weszła do promptu**.
///
/// Jeden odczyt zegara na cały bieg, nie jeden na notatkę: dwie notatki, które pojechały tym
/// samym promptem, były potrzebne w tej samej chwili, a dwa stemple o milisekundę od siebie
/// ustawiałyby je w wymuszonym wyborze w kolejności, której nic nie odpowiada.
///
/// Nieudany zapis **nie przewraca biegu** — ta sama decyzja, co przy zrzucie `run.json` w locie
/// ([`Live::update`]) i przy przekazaniu ([`Live::hand_over`]): bieg za chwilę rusza, a jego
/// wynik jest prawdziwy niezależnie od tego, czy stempel doszedł. Cena stoi w dzienniku wprost,
/// bo bez niej jest to notatka, która wygląda na nieużytą i schodzi z listy pierwsza.
fn stamp_what_this_run_carried(carried: &[MemoryRecord]) {
    if carried.is_empty() {
        return;
    }
    let at = super::now_utc();
    for record in carried.iter().filter(|record| record.carried) {
        // 2026-08-26 (T-128): stempel używa tej samej migawki, z której złożono prompt. Odczyt
        // ścieżki ponownie mógłby ostemplować ręczną redakcję, której model nigdy nie dostał.
        if let Err(error) =
            crate::memory::notes::mark_used_from_snapshot(&record.path, &record.snapshot, &at)
        {
            tracing::debug!(
                note = %record.reference,
                %error,
                "this note went into a prompt and could not be stamped, so it will look unused"
            );
        }
    }
}

/// To, co kroki zapisały w swojej auto-pamięci, staje się kandydatkami tych agentów.
///
/// # Druga połowa przekierowania (2026-08-23, T-92)
///
/// Pierwsza połowa siedzi w [`Live::with_its_own_settings`]: krok pisze do `mem/<krok>` pod
/// katalogiem biegu, zamiast do `~/.claude/projects/<projekt>/memory/`. Sama zamieniłaby katalog
/// dzielony z sesjami człowieka na katalog **zapomniany**: nikt by tam nie zajrzał, a agent dalej
/// nie miałby jak zabrać ze sobą niczego, czego się nauczył. Ta funkcja jest drugą połową i bez
/// niej pierwsza jest tylko sprzątaniem.
///
/// Trzy decyzje, każda z powodem:
///
/// - **`MEMORY.md` nie jest notatką.** Claude Code pisze ten plik sam, jako spis pozostałych, więc
///   jest listą tytułów, a nie wiedzą. Kandydatka bez treści kosztuje człowieka dokładnie ten sam
///   przegląd, co prawdziwa.
/// - **Zakres „ten agent", nie „ten projekt".** To jest zdanie, które JEDEN agent napisał sam
///   sobie: `this-project` wwiózłby nawyk jednego agenta do promptu każdego innego. Dlatego
///   notatka niesie też jego nazwę — bez niej trzeci zakres nie ma po czym filtrować i nie wchodzi
///   do żadnego promptu (T-80).
/// - **Po całym biegu, nie po każdej turze.** Kroki idą równolegle i bywają rundami jednej pętli
///   dzielącymi ten sam kafelek, więc „po turze" znaczyłoby czytanie tego samego katalogu tyle
///   razy, ile rund. Katalog jest kompletny dopiero wtedy, kiedy zszedł ostatni krok.
fn what_the_steps_wrote_down(deps: &RunDeps<'_>, plan: &Plan) {
    let root = super::memory::notes_root(deps.home);
    let at = noticed_now();
    // Rundy jednej pętli dzielą kafelek, więc dzielą katalog pamięci — i mają go dać jeden raz.
    let mut done: BTreeSet<&str> = BTreeSet::new();

    for step in &plan.steps {
        let Job::Agent(job) = &step.job else {
            continue;
        };
        if !done.insert(step.tile_key.as_str()) {
            continue;
        }
        let dir = plan.dir.join(STEP_MEMORY_DIR).join(&step.tile_key);
        for path in what_this_step_left_in(&dir) {
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Some(written) = what_the_agent_wrote(&text) else {
                continue;
            };
            let draft = crate::memory::notes::NoteDraft {
                title: title_from(&written.rule),
                rule: written.rule,
                // POCHODZENIE JEST UZASADNIENIEM, bo innego tu nie ma: agent pisał to sobie,
                // nie nam, więc nikt nie poprosił go o „dlaczego". Zdanie mówi to, co jest
                // prawdą i co da się sprawdzić — kto, przy czym i w którym biegu — a bez drogi
                // powrotnej do tury roszczenia nie da się później wycofać [T6 §5.1].
                because: written.because.unwrap_or_else(|| {
                    format!(
                        "{} left this in its own notes while working on \"{}\" in run {}.",
                        job.agent_name, step.name, plan.id
                    )
                }),
                scope: crate::memory::notes::Scope::ThisAgent,
                kind: crate::memory::notes::Kind::Fact,
                status: crate::memory::notes::Status::Suggested,
                at: at.clone(),
            };
            if let Err(error) = crate::memory::notes::record_candidate_for_with_body(
                &root,
                draft,
                &job.agent_name,
                &text,
            ) {
                tracing::warn!(
                    step = %step.tile_key,
                    %error,
                    "this step wrote something down for itself and it could not be kept"
                );
            }
        }
    }
}

/// Pliki tematyczne z katalogu auto-pamięci jednego kroku, po nazwie — bez indeksu.
///
/// Płasko i wyłącznie `.md`, dokładnie jak `memory::notes::scan_notes` czyta swój katalog:
/// spacer po drzewie zwróciłby to, co CLI trzyma tam obok, jako kolejne zdania agenta.
fn what_this_step_left_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        // Krok, który niczego nie zapisał, ma zero plików, a nie błąd: to jest stan normalny
        // i najczęstszy — vendor bez auto-pamięci nie zakłada nawet katalogu.
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension().is_some_and(|ext| ext == "md")
                && !path
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("MEMORY.md"))
        })
        .collect();
    // Kolejność nazw, nie kolejność systemu plików: dwa biegi tego samego kroku mają zostawić
    // kandydatki w tej samej kolejności, bo inaczej lista przestawia się sama między odczytami.
    found.sort();
    found
}

/// Pierwszy akapit, który agent naprawdę napisał w tym pliku, oraz jawny powód, jeśli istnieje.
///
/// Nagłówek markdown i front-matter są ramą, którą CLI stawia samo — reguła złożona z nich mówi
/// „# Queue" i nie niesie ani jednego faktu. Akapit może być zawinięty na kilka wierszy, więc
/// regułę składamy aż do pustego wiersza; pełny, nietknięty Markdown jedzie osobno do pisarza.
fn what_the_agent_wrote(text: &str) -> Option<AgentNote> {
    let body = crate::memory::FrontMatter::split(text).map_or(0, |(_, at)| at);
    let body = text.get(body..)?;
    let mut paragraph = Vec::new();
    for line in body.lines().map(str::trim) {
        if paragraph.is_empty() {
            if line.is_empty() || line.starts_with('#') || line == "---" {
                continue;
            }
        } else if line.is_empty() {
            break;
        }
        paragraph.push(line);
    }
    let rule = (!paragraph.is_empty()).then(|| paragraph.join(" "))?;
    let because = body.lines().find_map(|line| {
        line.trim()
            .strip_prefix("**Why:**")
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
            .map(ToOwned::to_owned)
    });
    Some(AgentNote { rule, because })
}

struct AgentNote {
    rule: String,
    because: Option<String>,
}

/// Pyta model RAZ, czego ten bieg nauczył, i zostawia z tego najwyżej trzy kandydatki.
///
/// # Po co to istnieje (zmierzone 2026-08-23, po 23 biegach właściciela)
///
/// Podsystem pamięci był zbudowany od strony **czytnika** i nie miał pisarza w produkcie:
/// `~/.loadout/memory/` nie istniało, tabela `memory` miała zero wierszy, a `record_candidate*`
/// miało wołających wyłącznie w testach i w imporcie. Sekcja Pamięć rysowała trzy strefy nad
/// pustym katalogiem, budżety pilnowały zera, wymuszony wybór nie miał czego wybierać. To jest
/// niezmiennik 29 czytany od strony wejścia: mechanizm jest, ekran o nim mówi, nikt nigdy nic
/// do niego nie napisał.
///
/// # Trzy rzeczy, które ta funkcja robi, i po jednym powodzie na każdą
///
/// 1. **Nie pyta biegu, w którym nie pracował żaden model.** Workflow z samych kafelków
///    „sprawdź", „uruchom i zostaw" i pytań do człowieka nie wołał vendora ani razu — a tura
///    refleksji wstawiłaby do niego jedno wywołanie, którego nie ma się o co oprzeć i za które
///    ktoś zapłaci. To jest ta sama granica, którą trzyma `run_check`: krok bez vendora nie ma
///    powodu prosić fabrykę o sterownik.
/// 2. **Nie pyta biegu, po którym nic nie zostało.** Refleksja pracuje z katalogu biegu, a bieg
///    bez ani jednego przekazania nie ma tam wyniku żadnego kroku — więc tura jest zapłacona za
///    przeczytanie pustego folderu.
/// 3. **Pyta dokładnie raz**, nie raz na krok. „Tania" przestaje być prawdą po cichu: katalog
///    notatek wygląda tak samo, a różnica siedzi wyłącznie na rachunku.
/// 4. **Nie przewraca biegu niczym, co się tu nie uda.** Bieg jest skończony i zapisany; jego
///    wynik jest prawdziwy niezależnie od tego, czy vendor odpowiedział. Ta sama decyzja, co
///    przy nieudanym zapisie przekazania ([`Live::hand_over`]) — z dziennikiem zamiast ciszy.
/// 5. **Pyta WŁASNYM SZWEM** ([`crate::engine::drivers::AgentDriver::reflecting`]), a nie
///    sterownikiem, którym jadą kroki. Sterownik, który tego szwu nie podaje, nie ma jak zobaczyć
///    tury, o którą nie prosił żaden kafelek — a to jest jedyny powód, dla którego dołożenie tej
///    tury nie przestawiło ani jednej z 26 specyfikacji liczących wywołania sterownika. Cała
///    cena tej decyzji stoi przy tamtej metodzie.
/// 6. **Płaci za pytanie NA MIARĘ BIEGU, o który pyta** (2026-09, Z-38). Sufit `spent` procent
///    ([`a_ceiling_for`]) zamiast jednej stałej: bieg, który kosztował 57 USD i zostawił
///    dziewięć przekazań, ma ich do przeczytania dziewięć, a ośmiocentowa tura schodzi na
///    suficie po 22 sekundach — zapłacona i bez odpowiedzi. Argument, a nie odczyt księgi
///    w środku: ta funkcja nie zna `Live` i nie ma powodu poznać.
/// 7. **Daje turze MATERIAŁ, zamiast kazać go szukać** (2026-09, Z-38): indeks przekazań
///    ([`what_this_run_left_behind`]) i po jednej linii na krok z tym, co powiedział na koniec
///    ([`how_each_step_ended`]). Bez tej drugiej listy refleksja zna wyłącznie ZLECENIA — tytuł
///    przekazania jest instrukcją z pliku workflow ([`title_of`]) — i żeby dowiedzieć się, co
///    z nich wyszło, musi otworzyć każdy plik po kolei. Za to płaci się jej budżetem.
async fn what_this_run_taught_us(
    deps: &RunDeps<'_>,
    plan: &Plan,
    states: &[StepState],
    spent: f64,
    said: &[SaidByAStep],
) -> Result<ReflectionReceipt, RunError> {
    // 2026-09 (Z-18): „pracował" znaczy tu mocniej „skończył z sukcesem". Sam start po
    // nieudanej albo anulowanej turze nie zostawia wyniku, na którym refleksja może się oprzeć.
    let a_model_worked =
        plan.steps.iter().zip(states).any(|(step, state)| {
            matches!(step.job, Job::Agent(_)) && *state == StepState::Succeeded
        });
    if !a_model_worked {
        return Ok(ReflectionReceipt::not_asked(NotAsked::NoAgentWorked));
    }

    // Zero przekazań to zero powodów, żeby pytać. Czytamy tym samym skanerem, którym czyta je
    // reszta aplikacji (niezmiennik 23) — własne `read_dir` byłoby drugą definicją słowa
    // „przekazanie", a rozjazd widać dopiero na rachunku.
    let Ok(left) = handoff::scan_run_dir(&plan.dir) else {
        // Refleksja nie może twierdzić, że przeczytała wynik, którego skaner produktu nie umie
        // odczytać. Sam bieg pozostaje prawdziwy; prywatna tura po prostu się nie zaczyna.
        return Ok(ReflectionReceipt::not_asked(NotAsked::NothingWasLeft));
    };
    if left.is_empty() || left.iter().all(handoff::Handoff::left_nothing) {
        return Ok(ReflectionReceipt::not_asked(NotAsked::NothingWasLeft));
    }

    let (run, dir) = (plan.id.as_str(), plan.dir.as_path());
    /* MATERIAŁ SKŁADAMY PRZED TURĄ I DAJEMY JEJ W PROMPCIE (2026-09, Z-38): tura, która zna
     * listę, nie płaci za chodzenie po katalogu, żeby ją zbudować. Dwie listy, bo to są dwa
     * różne fakty o tym samym biegu: indeks mówi, jakie pliki zostały i o co był poproszony
     * krok, który je zostawił, a druga lista mówi, co ten krok NAPRAWDĘ powiedział, kiedy
     * skończył. Sam indeks zostawia refleksję z samymi zleceniami. */
    let (index, context) = what_this_run_left_behind(&left, dir);
    let mut told = index;
    if let Some(block) = how_each_step_ended(said) {
        told.push_str("\n\n");
        told.push_str(&block);
    }
    let ceiling = a_ceiling_for(spent);
    let ended = a_short_turn_about(deps, dir, &told, context, ceiling).await?;

    /* RACHUNEK WYPEŁNIA SIĘ NA KAŻDEJ DRODZE, także tej bez odpowiedzi (2026-09, Z-38). Cena
     * jest tu jedyną drogą, którą tura wchodzi do wydatku biegu ([`final_spend_in`]), a tura,
     * która zeszła na sufcie, wydała dokładnie tyle, ile jej dano — do tego dnia ta kwota
     * ginęła razem z `Ok(None)`. Sufit zostaje przy rachunku, bo bez niego zdanie o zejściu na
     * nim nie ma czym się skończyć, a odtworzyć go ze stałej już się nie da. */
    let paid = ReflectionReceipt {
        cost_usd: ended.cost_usd,
        budget_usd: Some(ceiling),
        ..ReflectionReceipt::default()
    };
    if let Some(why) = ended.why {
        return Ok(ReflectionReceipt {
            why: Some(why),
            ..paid
        });
    }

    let (worth, without_reason) = worth_remembering(&ended.text);
    if without_reason > 0 {
        // Policzona, nie zapisana [T6 §10.3]. Para bez uzasadnienia nie staje się plikiem, bo
        // instrukcja bez uzasadnienia jest nieusuwalna: skasowanie kosztuje `O(2^|D|)`, trzeba
        // bowiem od nowa wyprowadzić jej interakcje z każdą inną [T6 §5.1].
        tracing::info!(
            run = %run,
            dropped = without_reason,
            "this run proposed rules with no reason under them, and a rule nobody can ground is \
             one nobody can retire either"
        );
    }

    let kept = keep_reflection_notes(deps, run, worth);
    Ok(ReflectionReceipt {
        ran: true,
        kept: kept.kept,
        discarded_again: kept.discarded_again,
        dropped_without_reason: without_reason,
        ..paid
    })
}

/// Mówi w strumieniu biegu, że prywatna tura nie doszła do odpowiedzi — jednym wierszem.
///
/// # Dlaczego akurat te dwie drogi, a nie każdy powód (2026-09, Z-38)
///
/// Bo tylko po nich człowiek ZAPŁACIŁ i nie dostał nic, i tylko one zostawiają pytanie „to co
/// teraz". Pozostałe powody są odpowiedzią same w sobie: Stop nacisnął on, wyłączone w
/// ustawieniach wybrał on, brak przekazań widzi na ekranie, a tura, która poszła i nie znalazła
/// nic, ma swoje zdanie w panelu historii. Wiersz na ekranie za każdy z nich byłby raportem
/// z pracy, której nikt nie zlecił.
///
/// PODPIS TO TYTUŁ BIEGU, nie nazwa kroku, i to nie jest szczegół: refleksja nie jest krokiem
/// (niezmiennik 27), a wiersz podpisany kafelkiem kazałby szukać wady w kroku, który zrobił
/// swoje. Ta sama droga, co [`say_the_folder_is_inside_a_repo`].
fn say_if_the_reflection_ran_out(lines: &LineSink, plan: &Plan, receipt: &ReflectionReceipt) {
    let Some(text) = ran_out_sentence(receipt) else {
        return;
    };
    // Wynik świadomie porzucony: pełna kolejka do okna jest normalnym stanem (`ipc::Sent`),
    // a bieg nie ma prawa stanąć dlatego, że okno nie nadąża.
    let _ = lines.send(Line::Problem {
        agent: plan.title.clone(),
        text,
        resets_at: None,
    });
}

/// Zdanie o turze, która zeszła na swoim sufcie — albo `None`, kiedy zeszła inaczej.
///
/// KWOTA JEST WYMAGANA, a nie zastąpiona podłogą: sufit skaluje się z biegiem, więc liczba
/// wzięta ze stałej byłaby zdaniem prawdziwym tylko dla najtańszego biegu, a fałszywa kwota na
/// ekranie jest gorsza niż jej brak. Rachunek niesie ją na każdej drodze, na której ta tura
/// w ogóle ruszyła ([`what_this_run_taught_us`]).
fn ran_out_sentence(receipt: &ReflectionReceipt) -> Option<String> {
    match receipt.why? {
        NotAsked::RanOutOfBudget => Some(format!(
            "{REFLECTION_DID_NOT_FINISH}: the note-taker used its ${:.2} before answering.",
            receipt.budget_usd?
        )),
        // BEZ LICZBY MINUT, choć zna ją [`REFLECTION_MINUTES`] tuż obok: to samo zdanie składa
        // panel historii, a tamta strona granicy tej stałej nie zna i nigdy nie pozna — wiersz
        // z drutu jej nie niesie. Dwa zdania o jednym fakcie, różniące się jednym słowem,
        // czytałyby się jak dwa różne fakty (2026-09, Z-38).
        NotAsked::RanOutOfTime => Some(format!(
            "{REFLECTION_DID_NOT_FINISH}: the note-taker ran out of time before answering."
        )),
        _ => None,
    }
}

/// Ile wolno wydać na jedno pytanie o bieg, który kosztował `spent`.
///
/// # Jeden procent, z podłogą i sufitem (2026-09, Z-38)
///
/// Procent, bo pytanie jest **proporcjonalne do biegu**: bieg z dziewięcioma przekazaniami ma
/// dziewięć rzeczy do przeczytania, a bieg z jednym ma jedną. Podłoga [`REFLECTION_BUDGET_USD`],
/// bo bieg tani albo darmowy (same kafelki „sprawdź") nadal ma prawo do jednego pytania. Sufit
/// [`REFLECTION_BUDGET_CAP_USD`], bo procent bez górnej granicy zamienia refleksję po drogim
/// biegu w najdroższą turę tego biegu.
///
/// `is_finite`, a nie samo [`f64::clamp`]: cena przychodzi od vendora, `clamp` na `NaN` PANIKUJE,
/// a panika w silniku zabiera cały bieg (`AGENTS.md` §4). Wartość, której nie da się porównać,
/// dostaje podłogę — tyle, co bieg bez ceny.
///
/// # PEŁNE CENTY, ZAOKRĄGLANE W GÓRĘ, i to jest poprawka z 2026-09-05
///
/// Sufit jedzie stąd w trzy miejsca i wszystkie trzy muszą mówić tę samą kwotę: do argv
/// vendora ([`crate::engine::drivers::claude::budget_argv`]), do rachunku w `run.json`
/// i do zdania na ekranie, które składa `{:.2}`. Bez tej linii bieg z audytu (57,52 USD) dawał
/// sufit `0.5752`: ekran zaokrąglał go do **najbliższego** centa i obiecywał `$0.58`, a vendor
/// dostawał kwotę zaokrągloną w **dół**, czyli 0,57. Człowiek czytał wtedy o cencie, którego
/// proces nigdy nie zobaczył.
///
/// W GÓRĘ, nie do najbliższego: ta kwota jest sufitem, który Loadout sam sobie stawia, więc
/// nadmiarowe pół centa jest ceną za to, że proces nigdy nie dostaje MNIEJ, niż obiecuje ekran.
/// Odwrotny wybór ma [`crate::engine::drivers::claude::budget_argv`] i z odwrotnego powodu:
/// tam kwotą jest reszta budżetu POSTAWIONEGO PRZEZ CZŁOWIEKA i nie wolno jej przekroczyć.
fn a_ceiling_for(spent: f64) -> f64 {
    let one_percent = spent / 100.0;
    if one_percent.is_finite() {
        let whole_cents = (one_percent * 100.0).ceil() / 100.0;
        whole_cents.clamp(REFLECTION_BUDGET_USD, REFLECTION_BUDGET_CAP_USD)
    } else {
        REFLECTION_BUDGET_USD
    }
}

/// Indeks tego, co ten bieg zostawił — i ta sama lista jako źródła kontekstu tury.
///
/// # Tą samą drogą, którą przekazania dostaje krok (2026-09, Z-38)
///
/// To samo otwarcie ([`HANDOFF_INDEX_OPENS`]), ten sam kształt wiersza — **etykieta i ścieżka
/// w jednym wierszu**, bo odnośnik i to, czym on jest, czytane z dwóch osobnych list są dwiema
/// listami do zestawienia w głowie — i to samo zamknięcie ([`HANDOFF_INDEX_CLOSES`]), które mówi
/// wprost, że treści w prompcie nie ma. Powody stoją w całości przy
/// [`Live::index_of_what_came_before`]; tutaj różni się jedno: krok dostaje przekazania swoich
/// poprzedników, a ta tura dostaje **wszystkie**, bo pyta się jej o cały bieg.
///
/// Ścieżki są WZGLĘDNE wobec katalogu biegu, bo katalog biegu jest katalogiem roboczym tej tury
/// — adres bezwzględny byłby tu dłuższy i prawdziwy do pierwszego przeniesienia katalogu.
///
/// Druga zwrócona lista jedzie do `logs/reflection.input.json` jako to, co Loadout naprawdę
/// wstrzyknął (`SafeInputManifest`). Do 2026-09-04 stało tam `context: []` przy turze, której
/// kazano te pliki znaleźć samodzielnie — czyli zapis mówiący prawdę o tym, że nie dostała nic.
/// Jedna linia, którą krok po sobie zostawił: podpis kafelka i to, co powiedział na koniec.
///
/// 2026-09 (Z-38) — DWA POLA Z DWÓCH RÓŻNYCH MIEJSC, i to jest cały powód, dla którego ten typ
/// istnieje. Nazwa jest z planu (niezmienna od startu biegu), a zdanie z księgi (powstaje przy
/// zejściu kroku) — złożone razem dopiero tam, gdzie obie strony są znane ([`Live`]).
struct SaidByAStep {
    name: String,
    said: String,
}

/// Blok promptu z tym, co kroki powiedziały — albo `None`, kiedy nie powiedział nic żaden.
///
/// `None`, A NIE PUSTY NAGŁÓWEK: zdanie „oto co powiedziały kroki" nad zerem wierszy jest
/// zdaniem o niczym i kosztuje dokładnie tyle samo tokenów, co prawdziwa lista (ten sam powód
/// stoi przy [`Live::prompt_for`] dla indeksu przekazań).
fn how_each_step_ended(said: &[SaidByAStep]) -> Option<String> {
    if said.is_empty() {
        return None;
    }
    let mut block = String::from(WHAT_THE_STEPS_SAID_OPENS);
    for one in said {
        // `write!` do `String`, nie `push_str(&format!(…))` — powód przy indeksie przekazań
        // (clippy `format_push_string`), i to samo `let _`.
        let _ = write!(block, "\n- {}: {}", one.name, one.said);
    }
    Some(block)
}

fn what_this_run_left_behind(
    left: &[handoff::Handoff],
    dir: &Path,
) -> (String, Vec<ContextSource>) {
    let mut index = String::from(HANDOFF_INDEX_OPENS);
    let mut context = Vec::with_capacity(left.len());
    for one in left {
        let filed = one
            .path
            .strip_prefix(dir)
            .unwrap_or(&one.path)
            .display()
            .to_string();
        // `write!` do `String`, nie `push_str(&format!(…))`: ten sam powód, co przy indeksie
        // kroku (clippy `format_push_string`), i to samo `let _` — zapis do `String` nie ma jak
        // zawieść.
        let title = one.meta.title.trim();
        if title.is_empty() {
            // Tytuł bywa pusty w cudzym pliku; dwukropek nad niczym czyta się jak plik ucięty
            // przy zapisie, a krok, który to zostawił, jest wtedy jedyną prawdą, którą mamy.
            let _ = write!(index, "\n- {filed} — {}", one.meta.from);
        } else {
            let _ = write!(index, "\n- {filed} — {}: {title}", one.meta.from);
        }
        context.push(ContextSource {
            kind: ContextKind::Handoff,
            reference: filed,
            // Długość POLICZONA przy odczycie, nie zadeklarowana w pliku: `meta.bytes` jest
            // deklaracją, a te dwie liczby mają prawo się różnić (`Handoff::bytes_mismatch`).
            bytes: one.actual_bytes,
        });
    }
    index.push_str("\n\n");
    index.push_str(HANDOFF_INDEX_CLOSES);
    (index, context)
}

fn keep_reflection_notes(
    deps: &RunDeps<'_>,
    run: &str,
    worth: Vec<Remembered>,
) -> KeptReflectionNotes {
    let library_root = super::memory::notes_root(deps.home);
    let project_root = super::memory::project_notes_root(deps.project);
    let at = noticed_now();
    let mut kept = 0;
    let mut discarded_again = 0;
    for one in worth.into_iter().take(AT_MOST_KEPT) {
        let draft = crate::memory::notes::NoteDraft {
            // Tytuł JEST regułą, przyciętą do nazwy pliku: model nie pisze osobnego, a nazwa
            // pliku jest tożsamością notatki — więc to samo zdanie z dwóch biegów trafia w ten
            // sam plik i stąd bierze się `occurrences` (powód w całości przy [`TITLE_CAP`]).
            title: title_from(&one.rule),
            rule: one.rule,
            because: one.because,
            // Czego uczy jeden bieg, jest prawdą o TYM projekcie. `everywhere` wniosłoby to
            // zdanie do każdego innego projektu na tej maszynie, a zakres, którego nikt nie
            // wybrał, jest tym najszerszym, którego nikt nie zauważył.
            scope: crate::memory::notes::Scope::ThisProject,
            kind: crate::memory::notes::Kind::Rule,
            // Czytane i wyrzucane przez `record_candidate*`; stoi tu jawnie, żeby było widać,
            // że deklaracja zgłaszającego nie jest tą, która o czymkolwiek decyduje.
            status: crate::memory::notes::Status::Suggested,
            at: at.clone(),
        };
        match crate::memory::notes::record_project_candidate_from_run(
            &library_root,
            &project_root,
            draft,
            run,
        ) {
            Ok(_) => kept += 1,
            Err(crate::memory::notes::Error::PreviouslyDiscarded(_)) => {
                discarded_again += 1;
            }
            Err(error) => {
                warn_reflection_write_failed(run, &error);
            }
        }
    }
    KeptReflectionNotes {
        kept,
        discarded_again,
    }
}

fn warn_reflection_write_failed(run: &str, error: &crate::memory::notes::Error) {
    use tracing::callsite::Callsite as _;

    // 2026-08-27 (T-133): dwa kończące się równolegle biegi mogą wywołać ten sam callsite z
    // różnych dispatcherów. Bezpośrednia wysyłka do bieżącego dispatchera omija wyłącznie
    // współdzielony cache zainteresowania; filtr aktywnego subscribera nadal rozstrzyga, czy
    // zdarzenie zapisać. Dzięki temu błąd IO nie znika z diagnostyki pod obciążeniem.
    let callsite = tracing::callsite! {
        name: "reflection write failed",
        kind: tracing::metadata::Kind::EVENT,
        target: module_path!(),
        level: tracing::Level::WARN,
        fields: message, run, error,
    };
    let metadata = callsite.metadata();
    tracing::Event::dispatch(
        metadata,
        &tracing::valueset!(
            metadata.fields(),
            message = "this run had something to remember and it could not be written down",
            run = %run,
            error = %error,
        ),
    );
}

struct KeptReflectionNotes {
    kept: usize,
    discarded_again: usize,
}

/// Czym skończyła się prywatna tura: powodem, tekstem i ceną, którą zdążyła nabić.
///
/// 2026-09 (Z-38) — ZASTĄPIŁO `Option<ReflectionTurn>`, i to jest cała treść tej zmiany.
/// `Ok(None)` zlewało cztery rozłączne drogi — nie ma aplikacji agenta, tura padła, zeszła na
/// cenie, zeszła na czasie — w jedno „nic nie wróciło", a po drodze wyrzucało cenę tury, która
/// padła. Bieg meetnotes z 2026-09-04 zapłacił za turę zabitą sufitem i zapisał o niej, że nic
/// nie wróciło.
struct ReflectionEnded {
    /// `None` znaczy „tura odpowiedziała". Każdy inny wariant jest osobnym zdaniem na ekranie.
    why: Option<NotAsked>,
    text: String,
    cost_usd: Option<f64>,
}

impl ReflectionEnded {
    /// Droga bez odpowiedzi i bez ceny: tura, która nigdy nie ruszyła albo nie zdążyła nic
    /// powiedzieć.
    fn nothing(why: NotAsked) -> Self {
        Self {
            why: Some(why),
            text: String::new(),
            cost_usd: None,
        }
    }
}

enum ReflectionEnd {
    Finished(anyhow::Result<DriverOutcome>),
    Stopped,
    TimedOut,
}

async fn wait_for_reflection(
    handle: &mut dyn AgentHandle,
    cancel: CancellationToken,
) -> Result<ReflectionEnded, RunError> {
    let limit = Duration::from_secs(REFLECTION_MINUTES * 60);
    let end = {
        let waiting = handle.wait();
        tokio::pin!(waiting);
        tokio::select! {
            outcome = &mut waiting => ReflectionEnd::Finished(outcome),
            () = cancel.cancelled() => ReflectionEnd::Stopped,
            () = tokio::time::sleep(limit) => ReflectionEnd::TimedOut,
        }
    };
    match end {
        ReflectionEnd::Finished(Ok(outcome)) if outcome.ok => Ok(ReflectionEnded {
            why: None,
            text: outcome.text,
            cost_usd: outcome.cost_usd,
        }),
        ReflectionEnd::Finished(Ok(outcome)) => {
            tracing::debug!(reason = ?outcome.reason, "the reflection turn did not finish");
            /* 2026-09 (Z-38) — SUFIT, W KTÓRY TA TURA MOŻE UDERZYĆ, JEST DOKŁADNIE JEDEN.
             * [`FinishReason::LimitReached`] niesie u tego vendora dwa zdarzenia: sufit tur
             * (`--max-turns`) i sufit ceny (`error_max_budget_usd`). Tura Loadouta nie dostaje
             * sufitu tur ani jedną flagą ([`a_short_turn_about`]), więc pozostaje cena — i to
             * jest ten kod, który 2026-09-04 przyszedł z prawdziwego biegu.
             * Cena jedzie dalej także stąd: tura, która zeszła na sufcie, wydała swoje. */
            let why = if outcome.reason == FinishReason::LimitReached {
                NotAsked::RanOutOfBudget
            } else {
                NotAsked::NothingCameBack
            };
            Ok(ReflectionEnded {
                why: Some(why),
                text: outcome.text,
                cost_usd: outcome.cost_usd,
            })
        }
        ReflectionEnd::Finished(Err(error)) => {
            tracing::debug!(%error, "the reflection turn fell over");
            Ok(ReflectionEnded::nothing(NotAsked::NothingCameBack))
        }
        // Obie ścieżki przechodzą przez dowód śmierci prawdziwej grupy procesu. Samo porzucenie
        // `wait` anulowałoby tylko future Rusta (niezmienniki 6 i 10). Różni je jedno: powód,
        // z którym tura schodzi — Stop człowieka i limit czasu są dla niego dwiema różnymi
        // wiadomościami (2026-09, Z-38).
        ReflectionEnd::Stopped => after_proving_it_is_dead(handle, NotAsked::Stopped).await,
        ReflectionEnd::TimedOut => after_proving_it_is_dead(handle, NotAsked::RanOutOfTime).await,
    }
}

/// Kończy turę dowodem śmierci jej grupy i dopiero wtedy oddaje powód.
///
/// Brak dowodu jest jedynym błędem tej ścieżki i nie wolno go spłaszczyć do braku odpowiedzi:
/// proces może nadal pracować i naliczać koszt (niezmiennik 6).
async fn after_proving_it_is_dead(
    handle: &mut dyn AgentHandle,
    why: NotAsked,
) -> Result<ReflectionEnded, RunError> {
    match handle.cancel().await {
        GroupProof::Dead { .. } => {
            tracing::debug!(?why, "the reflection turn was ended and proven dead");
            Ok(ReflectionEnded::nothing(why))
        }
        GroupProof::Alive { .. } => {
            tracing::error!(
                "the reflection group is still alive after escalation; this run cannot report a \
                 successful Stop"
            );
            Err(RunError::Io(io::Error::other(
                "Loadout could not make sure the agent stopped after learning from this run, so \
                 it may still be running.",
            )))
        }
    }
}

/// Ta jedna tura: własny szew sterownika, polityka tylko-do-odczytu, katalog biegu, jeden model.
///
/// Każda droga bez odpowiedzi wraca WŁASNYM powodem (2026-09, Z-38): brak aplikacji agenta —
/// czyli vendor, który tury Loadouta nie bierze, i vendor, który nie wstał — jest innym faktem
/// niż tura, która poszła i wróciła z niczym, i innym niż tura zabita sufitem. Jedyny błąd to
/// brak dowodu śmierci po Stopie: tego nie wolno spłaszczyć do braku odpowiedzi, bo proces może
/// nadal pracować i naliczać koszt (niezmiennik 6).
///
/// `index` dokleja się za prośbą i jest tym, co ten bieg zostawił ([`what_this_run_left_behind`]);
/// `context` jest tą samą listą dla dowodu, a `ceiling` sufitem tej jednej tury.
async fn a_short_turn_about(
    deps: &RunDeps<'_>,
    dir: &Path,
    index: &str,
    context: Vec<ContextSource>,
    ceiling: f64,
) -> Result<ReflectionEnded, RunError> {
    /* WŁASNYM SZWEM, NIE STEROWNIKIEM KROKÓW ([`AgentDriver::reflecting`], gdzie stoi cała cena
     * tej decyzji). Vendor jest jeden i wybrany: refleksja jest turą LOADOUTA, nie żadnego agenta
     * z grafu, więc vendor wzięty z ostatniego kroku dawałby dwa różne rachunki i dwa różne
     * zachowania za jedno pytanie. Ta sama stała rozstrzyga model ([`REFLECTION_MODEL`]) i to nie
     * przypadek, że jest aliasem tego vendora.
     *
     * Fabryka mówi tu WYŁĄCZNIE, który to vendor; czy on tę turę bierze i czym ją weźmie,
     * rozstrzyga szew. Sterownik, który go nie podaje — a nie podaje go żadna atrapa — nie ma
     * jak zobaczyć tury, o którą nie prosił żaden krok. */
    let prompt = format!("{REFLECTION_ASK}\n\n{index}");
    let Some(driver) = reflection_driver(deps, dir, &prompt, context, ceiling) else {
        return Ok(ReflectionEnded::nothing(NotAsked::NoAgentApp));
    };
    let spec = RunSpec {
        run_id: Uuid::now_v7(),
        // Katalog biegu: to o niego pytamy. Gdziekolwiek indziej jest to tura poproszona
        // o streszczenie czegoś, czego nie widzi.
        cwd: dir.to_path_buf(),
        prompt,
        model: Some(REFLECTION_MODEL.to_owned()),
        system_append: None,
        // Pyta, czego ten bieg nauczył, a nie o zmianę czegokolwiek: tura, której wolno pisać,
        // jest turą mogącą poprawić pracę, którą właśnie streszcza, w katalogu, na który nikt
        // już nie patrzy.
        policy: Policy::ReadOnly,
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    };

    let (events, mut inbox) = mpsc::channel::<DecodedEvent>(EVENT_QUEUE);
    /* Odbiornik stoi PRZED startem i istnieje wyłącznie po to, żeby vendor nie zatrzymał się na
     * pełnym buforze: te zdarzenia nie mają czytelnika, bo bieg już zszedł z ekranu, a wiersze
     * dosypane po jego końcu byłyby wierszami kroku, którego nikt nie zlecił. */
    let drain = tokio::spawn(async move { while inbox.recv().await.is_some() {} });

    let turn = match driver.start(spec, events).await {
        Ok(mut handle) => {
            let said = wait_for_reflection(handle.as_mut(), deps.control.cancel_token()).await;
            // `close()` czeka na samodzielne wyjście procesu. Po `Alive` nie mamy prawa na nim
            // zawisnąć ani zamienić braku dowodu w sukces; uchwyt schodzi dopiero z błędem wyżej.
            if said.is_ok() {
                let _ = handle.close().await;
            }
            said
        }
        Err(error) => {
            tracing::debug!(%error, "no reflection turn could be started after this run");
            Ok(ReflectionEnded::nothing(NotAsked::NoAgentApp))
        }
    };

    // Nadajnik ginie razem z uchwytem, więc dopiero tutaj kolejka jest zamknięta i pętla wyżej
    // ma jak się skończyć.
    if turn.is_err() {
        // Nieudowodniona grupa może nadal trzymać nadajnik. Drenaż nie jest procesem vendora;
        // kończymy tylko lokalnego czytelnika, żeby jawna odmowa mogła dojść do człowieka.
        drain.abort();
    }
    let _ = drain.await;
    turn
}

/// Sterownik tej jednej tury: prywatne ustawienia, własny dowód i sufit ceny.
///
/// `prompt` i `context` idą do manifestu dowodu, bo to jest jedyny zapis tego, co Loadout
/// naprawdę wstrzyknął (2026-09, Z-38): długość PRAWDZIWEGO promptu, nie samej prośby, i lista
/// przekazań, która do 2026-09-04 była pusta.
fn reflection_driver(
    deps: &RunDeps<'_>,
    dir: &Path,
    prompt: &str,
    context: Vec<ContextSource>,
    ceiling: f64,
) -> Option<Arc<dyn AgentDriver>> {
    let driver = (deps.drivers)(crate::library::agents::Vendor::ClaudeCode).reflecting()?;
    let settings = crate::engine::drivers::StepSettings {
        dir: dir.to_path_buf(),
        work_key: "_reflection".to_owned(),
        memory: dir.join(STEP_MEMORY_DIR).join("_reflection"),
        deny: crate::engine::drivers::host::deny_rules(deps.project),
    };
    let driver = match driver.with_settings(&settings) {
        Some(Ok(driver)) => driver,
        Some(Err(error)) => {
            tracing::debug!(%error, "the reflection turn could not get its private settings");
            return None;
        }
        None => {
            tracing::debug!("the reflection turn has no private settings wrapper");
            return None;
        }
    };
    if let Err(error) = fs::create_dir_all(&settings.memory) {
        tracing::debug!(%error, "the reflection memory directory could not be created");
        return None;
    }
    let target = EvidenceTarget::reflection(
        dir.to_path_buf(),
        SafeInputManifest {
            prompt_bytes: prompt.len(),
            context,
            images: Vec::new(),
        },
    );
    let Some(driver) = driver.with_evidence(target) else {
        tracing::debug!("the reflection turn has no private evidence wrapper");
        return None;
    };
    let Some(driver) = driver.with_budget(ceiling) else {
        tracing::debug!("the reflection turn has no price ceiling wrapper");
        return None;
    };
    Some(driver)
}

/// Chwila zgłoszenia kandydatki: ISO 8601 UTC **z milisekundami**.
///
/// Sekundy nie wystarczają, i to jest zmierzone, nie hipotetyczne: `modified` ma się przesunąć
/// przy drugim zgłoszeniu tej samej reguły, bo to jedyny ślad, po którym człowiek pozna, że dwa
/// biegi niezależnie powiedziały to samo. Dwa biegi bywają w jednej sekundzie i wtedy stempel
/// o rozdzielczości sekundy mówi „nic się nie zmieniło" o pliku, który się właśnie zmienił.
///
/// Sekundę bierzemy z [`super::now_utc`], a ułamek z [`now_ms`] — czyli z zegara **tego biegu**,
/// zamiast trzeciej drogi do `SystemTime`. To są dwa odczyty, więc na granicy sekundy ułamek
/// może należeć do sąsiedniej: stempel jest wtedy o mniej niż sekundę wcześniejszy, niż był
/// naprawdę. Kosztu nie ma, bo tego pola nie porządkuje ani jedna linia w drzewie — inaczej niż
/// `last_used_at`, które zostaje przy sekundach dokładnie po to, żeby porównywało się poprawnie
/// z każdą wartością wpisaną ręcznie (`memory::notes::Note::last_used_at`).
fn noticed_now() -> String {
    let second = super::now_utc();
    let inside = now_ms().rem_euclid(1_000);
    format!("{}.{inside:03}Z", second.trim_end_matches('Z'))
}

/// Jedna para `rule:` / `because:`, dokładnie tak, jak napisał ją model.
struct Remembered {
    rule: String,
    because: String,
}

/// Pary z odpowiedzi modelu — i ile ich odpadło, bo nie miały uzasadnienia.
///
/// Czyta WIERSZAMI, a nie akapitami, i pary składa dopiero `because:`. Implementacja przerywająca
/// pętlę na pierwszej złej parze gubi dobre pary stojące za nią tak samo jak taka, która nie
/// sprawdza niczego — a te dwie wady różnią się dla człowieka wszystkim.
///
/// Wiersz, który nie zaczyna się żadnym z dwóch kluczy, jest **pomijany**: model prawie zawsze
/// napisze zdanie wstępu, a odpowiedź odrzucona przez jedno takie zdanie jest turą zapłaconą
/// za nic (ten sam kierunek, co niezmiennik 5 na drucie).
fn worth_remembering(said: &str) -> (Vec<Remembered>, usize) {
    let mut kept: Vec<Remembered> = Vec::new();
    let mut without_reason = 0;
    let mut pending: Option<String> = None;

    for raw in said.lines() {
        // Punktory i pogrubienia zdejmujemy z początku wiersza, bo model pisze listę wtedy, kiedy
        // prosi się go o listę. Wiersz, który po tym nie zaczyna się kluczem, i tak przepada.
        let line = raw.trim().trim_start_matches(['-', '*', '#', ' ']).trim();

        if let Some(rule) = after_key(line, "rule:") {
            // Poprzednia reguła, do której nie doszedł żaden `because:`, odpada dokładnie tutaj:
            // to jest kształt, w którym brak uzasadnienia przychodzi najczęściej — nie pusta
            // wartość, tylko wiersz, którego nie ma wcale.
            if pending.take().is_some() {
                without_reason += 1;
            }
            if rule.is_empty() {
                without_reason += 1;
            } else {
                pending = Some(rule.to_owned());
            }
        } else if let Some(because) = after_key(line, "because:") {
            match pending.take() {
                // Obecność KLUCZA to nie jest obecność uzasadnienia. `because:` bez treści jest
                // drugim kształtem, w którym to przychodzi od modelu, i odpada tak samo.
                Some(rule) if !because.is_empty() => kept.push(Remembered {
                    rule,
                    because: because.to_owned(),
                }),
                Some(_) => without_reason += 1,
                None => {}
            }
        }
    }

    // Reguła stojąca na samym końcu odpowiedzi, bez uzasadnienia pod nią.
    if pending.is_some() {
        without_reason += 1;
    }
    (kept, without_reason)
}

/// Treść wiersza za tym kluczem — albo `None`, kiedy wiersz nie jest tym kluczem.
///
/// Bez rozróżniania wielkości liter: „Rule:" i „rule:" to jedno i to samo zdanie modelu, a para
/// odrzucona za wielką literę jest parą odrzuconą za nic.
fn after_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    if line.len() < key.len() || !line.is_char_boundary(key.len()) {
        return None;
    }
    let (head, rest) = line.split_at(key.len());
    head.eq_ignore_ascii_case(key).then_some(rest.trim())
}

/// Reguła przycięta do tytułu, czyli do nazwy pliku notatki (powód przy [`TITLE_CAP`]).
///
/// Cięcie idzie po granicy ZNAKU, a potem po granicy słowa: pierwsze dlatego, że wycinek
/// w środku znaku wielobajtowego panikuje, a reguła przychodzi od modelu; drugie dlatego, że
/// tytuł urwany w połowie słowa czyta się jak plik uszkodzony przy zapisie.
fn title_from(rule: &str) -> String {
    let trimmed = rule.trim();
    if trimmed.chars().count() <= TITLE_CAP {
        return trimmed.to_owned();
    }
    let cut = trimmed
        .char_indices()
        .nth(TITLE_CAP)
        .map_or(trimmed.len(), |(at, _)| at);
    let end = trimmed[..cut].rfind(char::is_whitespace).unwrap_or(cut);
    trimmed[..end].to_owned()
}

/// Zatrzymuje bieg i wraca po rozstrzygnięciu każdej należącej do niego grupy.
///
/// Zwraca [`Outcome::Cancelled`] jako wartość, nigdy `Err` (niezmiennik 7). `Ok(())` zaraz po
/// wysłaniu sygnału byłoby tym samym błędem, przed którym broni `GroupProof`: wołający
/// przeczytałby „nie żyje" tam, gdzie napisano „wysłałem SIGTERM" (niezmiennik 6). Kiedy trzy
/// pełne eskalacje nie dają dowodu, bieg zachowuje adres grupy i zapisuje krok jako porażkę;
/// samo anulowanie pozostaje wartością całego biegu.
///
/// **Warunek dla wołającego (T-07):** ten `RunControl` ma należeć do biegu, który ruszył albo
/// już zszedł. Dowód zapala [`run_workflow_inner`] na każdej swojej drodze wyjścia, więc bieg
/// zakończony i bieg odrzucony wracają stąd natychmiast — ale uchwyt biegu, którego nikt nigdy
/// nie uruchomił, nie ma czego dowieść i czekanie na niego nie ma końca.
pub async fn stop_run_inner(deps: &RunDeps<'_>) -> Result<Outcome, RunError> {
    let _proof = stop_control(&deps.control).await;
    Ok(Outcome::Cancelled)
}

/// Wspólna droga Stop — to samo anulowanie i czekanie, ale z osobnym wynikiem dowodowym.
pub async fn stop_control(control: &RunControl) -> super::StopProof {
    control.stop();
    // Czekamy na bieg, a nie na siebie. Kroki rozstrzygają swoje grupy procesów same — tylko
    // one znają uchwyt i adres do późniejszego sprzątania — a `settle()` zapala się dopiero,
    // kiedy `run_workflow_inner` naprawdę wróciło.
    control.wait_until_settled().await;
    // Bieg, którego token jest anulowany, melduje `cancelled` także wtedy, gdy ostatni krok
    // zdążył się udać (`scheduler::execute` czyta token na końcu). Dwa różne zdania o jednym
    // biegu byłyby dwoma miejscami, w których mieszka jedna odpowiedź.
    control.stop_proof()
}

/// Stop naciśnięty przez człowieka: zatrzymuje bieg, jeśli jakikolwiek idzie.
///
/// Oddaje `false`, kiedy nie było czego zatrzymywać, i **to jest odpowiedź, nie błąd**:
/// naciśnięcie Stopu nad pustym ekranem nie jest pomyłką.
///
/// # Po co to istnieje osobno od [`stop_run_inner`]
///
/// Zgłoszenie właściciela 2026-08-23, cztery wiersze pod rząd w jednym terminalu: odmowa
/// „A run is already going… Press Stop first", potem `/stop` → **„Nothing is running."**,
/// potem to samo jeszcze raz. Bieg pracował przez cały ten czas.
///
/// Zdanie „nic nie biegnie" mówiło do tego dnia OKNO, z własnej pamięci. Ta pamięć jest ulotna —
/// gubi ją przeładowanie strony — a zapadka biegu jest JEDNA NA APLIKACJĘ i mieszka po tej
/// stronie. Dwie odpowiedzi na jedno pytanie rozjechały się dokładnie tam, gdzie boli: odmowa
/// każe nacisnąć Stop, a Stop twierdzi, że nie ma czego zatrzymywać (niezmiennik 13).
///
/// # Dlaczego to pytanie jest konieczne, a nie uprzejme
///
/// [`stop_run_inner`] czeka na dowód śmierci grupy procesów, a dowód zapala bieg, który przez
/// siebie przeszedł. Zawołane na uchwycie, którego nikt nie wziął, czekałoby **bez końca** —
/// czyli Stop nad pustym ekranem wieszałby aplikację. To samo pytanie i z tego samego powodu
/// stoi w [`stop_before_closing`]; różnica jest taka, że tam kończy się zamknięciem okna,
/// a tutaj zdaniem dla człowieka.
pub async fn stop_if_anything_is_going(deps: &RunDeps<'_>) -> Result<bool, RunError> {
    if !deps.control.is_working() {
        return Ok(false);
    }
    stop_run_inner(deps).await.map(|_| true)
}

/// Okno się zamyka: zatrzymuje bieg **z dowodem**, jeśli jest co zatrzymywać.
///
/// # Po co to istnieje
///
/// Zgłoszenie właściciela 2026-08-19: „co się dzieje jak zamykasz apkę a leci jakiś workflow?
/// on się wyłączy?", a zaraz po nim „odpalałem kilka workflow i apkę zamykałem w trakcie".
/// Nie wyłączał się. W `lib.rs` nie było ani jednego `on_window_event`, `CloseRequested` czy
/// `RunEvent`, więc zamknięcie okna kończyło proces Loadouta i **nic więcej**: agenci przechodzili
/// pod PID 1 i dalej pracowali, dalej pisali po plikach projektu i dalej palili limit u dostawcy,
/// aż ktoś odpalił Loadouta ponownie (`recovery.rs`, nagłówek: „Agenci nie giną razem
/// z Loadoutem"). Odzyskiwanie sprząta to dopiero przy NASTĘPNYM starcie, więc rachunek rósł przez
/// cały czas, w którym aplikacja była zamknięta — czyli dokładnie wtedy, kiedy nikt nie patrzył.
///
/// # Dlaczego to pytanie o `is_working` jest konieczne
///
/// [`stop_run_inner`] czeka na dowód śmierci grupy procesów (niezmiennik 6), a dowód zapala bieg,
/// który naprawdę przez siebie przeszedł. Wywołane na uchwycie biegu, którego nikt nie uruchomił,
/// czekałoby **bez końca** — czyli zamknięcie okna wieszałoby aplikację w najczęstszym przypadku
/// ze wszystkich: kiedy nic nie biegnie.
///
/// # Co ta funkcja świadomie robi wolno
///
/// Wraca dopiero z dowodem, więc zamknięcie okna trwa tyle, ile schodzenie agentów (TERM, potem
/// KILL — `engine::supervisor`). Okno, które zamyka się natychmiast i zostawia procesy, jest
/// szybsze i jest kłamstwem: człowiek czyta zniknięcie okna jako koniec pracy.
pub async fn stop_before_closing(deps: &RunDeps<'_>) -> Result<Outcome, RunError> {
    if !deps.control.is_working() {
        // Nie ma czego zatrzymywać i nie ma na co czekać. `Cancelled` byłoby tu zdaniem
        // o biegu, którego nie ma — a `Done` mówi prawdę: nic nie zostało niedokończone.
        return Ok(Outcome::Done);
    }
    /* SUFIT, i jest tu dla przypadku, którego `is_working` nie łapie. Nagłówek wyżej nazywa
     * czekanie bez końca jako ryzyko i gasi je pytaniem „czy w ogóle coś biegnie" — a to gasi
     * połowę: bieg, który JEST w trakcie i którego zadanie się zacięło, dalej czeka w
     * nieskończoność. `prevent_close` jest już wtedy podniesione, więc człowiek zostaje z oknem,
     * którego nie da się zamknąć, i sięga po jedyne wyjście, jakie mu zostało — ubicie aplikacji
     * z zewnątrz, czyli dokładnie tę drogę, która zostawia sieroty.
     *
     * Sufit stoi WYSOKO NAD uczciwym najgorszym przypadkiem, i tak ma być: schodzenie agentów
     * jest ograniczone co do sekundy (`engine::supervisor::DEFAULT_GRACE` plus dowód po
     * dziewiątce), a kroki schodzą równolegle. Trzydzieści sekund nie skraca ani jednego
     * uczciwego zamknięcia — odróżnia schodzenie od zacięcia. */
    match tokio::time::timeout(HOW_LONG_CLOSING_MAY_WAIT, stop_run_inner(deps)).await {
        Ok(stopped) => stopped,
        Err(_) => Err(RunError::StillGoingAtClose {
            seconds: HOW_LONG_CLOSING_MAY_WAIT.as_secs(),
        }),
    }
}

/// Ile zamknięcie okna czeka na koniec biegu, zanim uzna, że to już nie schodzenie.
///
/// Powód dla tej liczby stoi w ciele [`stop_before_closing`]. W skrócie: uczciwe schodzenie
/// mieści się w kilku sekundach i jest ograniczone przez [`crate::engine::supervisor`], więc ta
/// wartość nie skraca niczego, co naprawdę schodzi.
pub const HOW_LONG_CLOSING_MAY_WAIT: Duration = Duration::from_secs(30);

/// Ile pełnych eskalacji wykonuje żywy Stop, zanim odda aplikację człowiekowi z uczciwą odmową
/// uznania procesu za martwy. Stała należy do produkcji: wołający nie może skrócić dowodu.
const LIVE_STOP_ATTEMPTS: usize = 3;

/// Odstęp między pełnymi eskalacjami, nigdy dodatkowe czekanie po ostatniej próbie.
const LIVE_STOP_RETRY_PAUSE: Duration = Duration::from_secs(1);

/// Jedno zdanie współdzielone przez trwały receipt i istniejący panel historii.
const LIVE_STOP_SURVIVOR_ERROR: &str =
    "This agent survived Loadout's three attempts to stop it and may still be running.";

/// Zdanie dla człowieka o kroku, który **skończył pracę**, a jego grupa procesów nadal odpowiada
/// na sygnał zerowy.
///
/// 2026-08-28 — osobne od [`LIVE_STOP_SURVIVOR_ERROR`], choć obie mówią o ocalałym, bo różnią się
/// tym, czego człowiek szuka dalej: tamto zdanie pada po Stopie, którego ktoś nacisnął, a to po
/// kroku, który wygląda na udany i o którym nikt by nie zapytał. Nazywa więc obie połowy — pracę
/// skończoną i grupę niedowiedzioną — bo bez pierwszej połowy czyta się jak porażka agenta.
const STEP_SURVIVOR_ERROR: &str = "\
This step finished its work, but Loadout could not make sure everything it started had stopped, \
so some of it may still be running.";

/// Zdanie dla kroku, który oddał wynik, ale po zamknięciu wejścia wymagał eskalacji.
///
/// 2026-09 — osobne od awarii dowodów: prywatny zapis nadal jest zdrowy, lecz agent nie spełnił
/// kontraktu normalnego końca. Zdanie stoi w `run.json`, skąd czyta je okno (niezmiennik 29).
const STEP_WOULD_NOT_LET_GO_ERROR: &str = "This step finished its work, but the agent kept going \
after Loadout closed its input, so Loadout stopped it.";

/// To samo o komendzie kroku „sprawdź". Osobne zdanie, bo nazywa KOMENDĘ: w tym kroku nie ma
/// agenta, a „ten krok" bez podmiotu wysyła człowieka szukać wady u kogoś, kogo tam nie było.
const CHECK_SURVIVOR_ERROR: &str = "\
This check ran to the end, but Loadout could not make sure everything it started had stopped, \
so some of it may still be running.";

/// Zdanie o kroku, który zszedł od sygnału, **którego Loadout nie wysłał**.
///
/// # 2026-09 (Z-39) — po co to jest osobnym zdaniem
///
/// Bieg meetnotes `20260901-150035`: lider nie miał czym zatrzymać biegu, więc przeczytał `pgid`
/// z `run.json` i wykonał `kill -TERM` na cztery grupy ręcznie. Loadout zapisał wtedy to, co
/// powiedział vendor — „The agent stopped without ever sending its result" — czyli zdanie
/// o agencie, który przestał mówić. Człowiek czytający historię szukał więc wady u agenta,
/// a agent nie zrobił nic: ktoś go zabił z zewnątrz.
///
/// NUMER SYGNAŁU JEST W ZDANIU, i to nie jest żargon (niezmiennik 14): to jedyna wartość,
/// po której odróżnia się piętnastkę od dziewiątki, czyli grzeczne zatrzymanie od ubicia.
/// Bez niej zdanie mówi „coś się stało" i kończy się tam, gdzie zaczyna się pytanie.
fn stopped_from_outside_sentence(signal: i32) -> String {
    format!("Something outside Loadout stopped this step (signal {signal}).")
}

/// Zdanie na karcie kroku, który pojechał dalej BEZ wyniku poprzednika ubitego z zewnątrz.
///
/// 2026-09 (Z-39) — powód jest ten sam bieg i ta sama godzina: po ubiciu Reaserch A/B Loadout
/// puścił dalej Final Plan (17 minut Codeksa) i Combine (26 minut), a na kartach tych kroków nie
/// stało ani jedno zdanie o tym, że jadą na pustym materiale. Wyglądały dokładnie tak, jak
/// wyglądałby krok, który po prostu tak odpowiedział.
///
/// „runs", nie „ran": ten wiersz staje w strumieniu **w chwili**, w której krok rusza, i zostaje
/// w `run.json` w tej samej postaci — jedno zdanie na dwa miejsca (niezmiennik 13).
fn ran_without_sentence(who: &str) -> String {
    format!("runs without {who}'s result — it was stopped from outside")
}

/// Puszcza bieg dalej z punktu kontrolnego (T3 §6.1 reguła 5).
///
/// Punkt kontrolny zatrzymuje **bieg**, nie krok, i nic za nim nie startuje, dopóki człowiek nie
/// odpowie. Pytanie, które pojawia się na ekranie po tym, jak agent już zrobił swoje, nie jest
/// pytaniem.
pub async fn continue_run_inner(
    deps: &RunDeps<'_>,
    answer: Option<String>,
) -> Result<(), RunError> {
    // Licznik, nie flaga (`RunControl::go_on`): bieg z dwoma punktami kontrolnymi przeszedłby
    // przez drugi bez pytania, gdyby zgoda była flagą, która raz zapalona zostaje zapalona.
    //
    // 2026-08-18 — TREŚĆ ODPOWIEDZI JEDZIE RAZEM ZE ZGODĄ. Do tego dnia ta komenda nie brała
    // żadnego argumentu: człowiek pisał zdanie, pytanie znikało z ekranu, bieg ruszał — i to
    // zdanie nie trafiało ani do promptu następnego kroku, ani na dysk. Kontrolka, która
    // przyjmuje tekst i go wyrzuca, jest gorsza niż jej brak (niezmiennik 16).
    deps.control.go_on_with(answer);
    // Wracamy dopiero, kiedy bieg naprawdę ruszył — tak samo jak Stop wraca dopiero z dowodem.
    // Bez tego ekran wraca do człowieka w chwili, w której bieg **jeszcze stoi**, i pierwsze,
    // co ten człowiek widzi po odpowiedzeniu na pytanie, to dalej „paused". Czekanie kończy się
    // natychmiast, gdy nie było na co odpowiadać, i kończy się także wtedy, gdy bieg w tym
    // czasie zszedł (`RunControl::wait_until_moving`).
    deps.control.wait_until_moving().await;
    Ok(())
}

/// Kompatybilny sygnał dla pauz niebędących pytaniem. Sieć nigdy nie wybiera za człowieka ID.
pub async fn continue_unaddressed(
    deps: &RunDeps<'_>,
    answer: Option<String>,
) -> Result<(), RunError> {
    if !super::checkpoint::list(&deps.control).is_empty() {
        return Err(RunError::QuestionNeedsAddress);
    }
    deps.control.signal_go_on(answer);
    // Sygnał nie zwolni nowego pytania utworzonego po sprawdzeniu; ono ma własny oneshot.
    if super::checkpoint::list(&deps.control).is_empty() {
        deps.control.wait_until_moving().await;
    }
    Ok(())
}

/// Mówi coś agentowi, który **właśnie pracuje** — kolejna tura w jego żywej sesji.
///
/// # Po co to istnieje
///
/// Zgłoszenie właściciela 2026-08-18, dwa razy: „i pisać z nim nie mogę", potem „dalej nie działa
/// pisanie do agenta przez terminal". Droga do żywej sesji nie istniała, i nie z braku komendy:
/// `stdin` był polem uchwytu, więc pisanie wymagało `&mut`, a uchwyt jest pożyczony mutowalnie
/// przez całą turę ([`Live::one_turn`]). Naprawa poszła w przyczynę — potok należy dziś do
/// jednego zadania-pisarza, a bieg trzyma nadajniki pod nazwami kroków
/// ([`RunControl::step_can_hear`]).
///
/// # Dlaczego to stoi TUTAJ, a nie w skorupie komendy
///
/// Bo to jest polityka, a nie transport: cztery różne odmowy, każda innym zdaniem, i wybór
/// adresata przy jednym pracującym agencie. Do 2026-08-18 mieszkało to w całości w
/// `#[tauri::command]` w `ipc.rs` — czterdzieści linii decyzji w skorupie, która ma mieć dwie
/// (niezmiennik 1 i 23). Miało to jeden, konkretny koszt: `State<'_, AppState>` nie da się
/// zbudować bez żywego Tauri, więc na ANI JEDNĄ z tych czterech odmów nie dało się napisać
/// kryterium — a kryterium, którego nie da się napisać, jest zachowaniem, którego nikt nie
/// sprawdził.
///
/// # Adresat
///
/// `agent` jest opcjonalny i to jest wygoda z pomiarem, nie zgadywanie: kiedy pracuje dokładnie
/// jeden krok, nie ma czego wybierać. Przy dwóch i więcej **odmawiamy z listą nazw** — kontrolka,
/// która wysyła tekst do losowego z dwóch agentów, jest gorsza niż odmowa (niezmiennik 16).
pub async fn say_to_agent_inner(
    control: &RunControl,
    agent: Option<&str>,
    text: &str,
) -> Result<(), RunError> {
    if let Some(refusal) = super::step_message::fixed_input_refusal(control, "", "") {
        return Err(RunError::MessageRefused(refusal.said));
    }
    let said = text.trim();
    if said.is_empty() {
        return Err(RunError::NothingToSay);
    }
    /* Lista brana RAZ i po niej rozstrzygamy wszystko: drugi odczyt między wyborem adresata
     * a wzięciem głosu dałby dwie różne odpowiedzi na jedno pytanie „kto pracuje", a wtedy
     * zdanie odmowy mogłoby wymieniać kroki, których w chwili wysyłki już nie ma. */
    let listening = control.who_is_listening();
    let named = agent.map(str::trim).filter(|one| !one.is_empty());

    let to = match (named, listening.as_slice()) {
        (Some(named), _) => named.to_owned(),
        (None, [only]) => only.clone(),
        (None, []) => return Err(RunError::NobodyIsWorking),
        (None, many) => {
            return Err(RunError::SeveralAreWorking {
                names: many.to_vec(),
            });
        }
    };

    let voice = control.voice_of(&to).ok_or_else(|| {
        if listening.is_empty() {
            RunError::ThatOneFinished
        } else {
            RunError::NoSuchAgentWorking {
                name: to.clone(),
                working: listening.clone(),
            }
        }
    })?;

    // Każdy przygotowany bieg ma dokładny adres. Stary adapter nazw dochodzi do tej samej
    // funkcji przyjęcia co Entry i most; poniższa ścieżka bez adresu zostaje dla starych,
    // wewnętrznych uchwytów testowych, które nigdy nie przeszły prepare.
    if let Some(run) = control.run_address() {
        let recipients = super::step_message::recipients(control);
        let node = recipients
            .iter()
            .find(|one| one.agent == to && one.can_receive)
            .ok_or_else(|| RunError::StoppedListening { name: to.clone() })?;
        let reply = super::step_message::send(control, &run.id, &node.node_key, said);
        return if reply.result == super::step_message::StepMessageResult::AcceptedBySession {
            Ok(())
        } else {
            Err(RunError::MessageRefused(reply.said))
        };
    }

    // Kanał, nie uchwyt: nadajnik jest klonowalny i nie wymaga `&mut`, czyli da się nim pisać
    // do sesji, której tura właśnie trwa. Cała naprawa mieści się w tym jednym zdaniu.
    voice
        .send(crate::engine::drivers::ToAgent::Turn(said.to_owned()))
        .await
        .map_err(|_| RunError::StoppedListening { name: to.clone() })?;

    /* DOPIERO TERAZ WIDAĆ TO NA EKRANIE, i kolejność jest tu treścią kryterium.
     *
     * Zgłoszenie właściciela 2026-08-19: „a może odpisuje on, ale na pewno nie widać moich
     * wiadomości". Zdanie dochodziło do modelu i nie zostawiało śladu w strumieniu, bo tura
     * człowieka nie miała nośnika na drucie (powód w całości przy `Line::Told`).
     *
     * PO wysłaniu, nie przed: wiersz dopisany wcześniej pokazywałby w historii zdanie, które za
     * chwilę odbije się o `StoppedListening` — czyli historia twierdziłaby, że agent coś usłyszał,
     * a nie usłyszał. Odwrotna kolejność jest tą, która kłamie w pliku (niezmiennik 4).
     *
     * Wynik świadomie porzucony: pełna kolejka do okna jest normalnym stanem szybkiego agenta
     * (`ipc::Sent`), a zdanie i tak POSZŁO. Odmowa w tym miejscu mówiłaby człowiekowi, że jego
     * tura nie doszła, kiedy doszła. */
    let _ = control.show_in_the_run(crate::engine::line::Line::Told {
        agent: to,
        text: said.to_owned(),
    });
    Ok(())
}

// ── PLAN: wszystko, co da się rozstrzygnąć, ZANIM cokolwiek powstanie ───────────────────────

/// Bieg rozpisany do końca i **jeszcze niczego niedotykający na dysku**.
///
/// Wszystko, co może odmówić — nieczytelny plik, koło w grafie, agent, którego nie ma
/// w bibliotece — odmawia przy budowie tej struktury. Dzięki temu „odmowa nie tworzy katalogu"
/// jest własnością kolejności wywołań, a nie obietnicą powtarzaną w komentarzach.
struct Plan {
    replay: Option<Arc<super::replay::ReplayMaterial>>,
    inputs: super::run_inputs::BoundInputs,
    /// 2026-09-08 (WP-02): fizyczne źródła wersji, policzone raz przed pierwszym procesem.
    work_plan_sources: crate::workflow::work_plan::AuthorMap,
    protection: BTreeMap<StepId, protection::Boundary>,
    lead_origin: Option<super::lead_start::LeadStartOrigin>,
    /// uuid v7 biegu — sortuje się po czasie.
    id: String,
    /// `<projekt>/.loadout/runs/<ts>__<id>/`. Policzony tutaj, tworzony dopiero po planie.
    dir: PathBuf,
    /// WF-01: jeden obraz bazowy, wspólny dla izolacji, fan-in i odtwarzania.
    input_snapshot: Option<super::input_snapshot::InputSnapshot>,
    /// Jawnie adresowane wejścia WF-16, po rzeczywistym kluczu zarządzanej kopii.
    workspace_inputs: super::workspace_inputs::Bound,
    /// ID i odcisk prywatnego pakietu WF-12, odczytywane także przez odtwarzanie i Lab.
    project_instructions: Option<Value>,
    /// WF-23: pełne źródła wybranych notatek, nie gotowe prompty; zapis przed procesami.
    memory_sources: super::memory_sources::Snapshot,
    /// CT-06: prywatny pakiet opracowań, związany z fizycznymi kluczami kroków.
    context_sources: Option<super::context_sources::Snapshot>,
    additional_inputs: Vec<String>,
    /// Dokładny zapisany wynik, od którego zaczyna każda wznowiona kopia (WF-01).
    starting_results: BTreeMap<String, StartingResult>,
    /// Tytuł widoczny w historii.
    title: String,
    /// Który workflow to był.
    workflow_id: String,
    /// Odcisk pliku — druga połowa pytania „czy to był ten sam plan".
    hash: String,
    /// Graf **jak biegł**, dosłownie taki, jaki wczytaliśmy.
    graph: Value,
    /// Krawędzie po numerach kroków, gotowe dla `engine::dag`.
    arrows: Vec<(StepId, StepId)>,
    /// Warunki po numerach rozwiniętych węzłów. Pusty wektor zachowuje zwykły scheduler.
    routes: Vec<PlannedRoute>,
    /// Ile kroków ma naprawdę działać naraz — prosto z żądania.
    concurrency: usize,
    /// O co poproszono TEN bieg. Pusty napis znaczy „nic nie kazano".
    ///
    /// Trzymane w planie, żeby dojechało do `run.json`: do 2026-08-23 zdanie definiujące cały
    /// bieg nie istniało w ŻADNYM pliku, więc po jego końcu nie dało się odpowiedzieć na
    /// pytanie „co ten bieg miał zbudować" inaczej niż zgadując z promptów kroków. Niezmiennik
    /// 4 mówi, że prawdą są pliki — a najważniejszego faktu w nich nie było.
    task: String,
    /// Pętle tego biegu: kto orzeka i ile razy wolno próbować, po jednej pozycji na powrót.
    ///
    /// Klucz KAFELKA, nie węzła: sędzia jest jeden na wszystkie rundy swojej pętli.
    ///
    /// 2026-08-22 — WEKTOR, NIE `Option`. Walidator dopuszcza dziś tyle pętli, ile jest, byle
    /// miały ROZŁĄCZNE ciała (`check::loops_that_cross`), bo dwie gałęzie z osobnym sprawdzeniem
    /// są zwykłym dniem pracy. Kolejność jest kolejnością z `unroll::Unrolled::loops` i to jest
    /// kontrakt: [`Planned::in_loop`] indeksuje tę listę, a `settled_at` ma tyle samo pozycji.
    loops: Vec<Loop>,
    /// Kroki w kolejności z pliku workflow. Ta kolejność jest kontraktem `RunReport::steps`.
    steps: Vec<Planned>,
    /// Bieg, z którego ten przejmuje przekazania na wejściu. `None` dla zwykłego biegu.
    ///
    /// 2026-08-23 — nosi to ponowne odpalenie kroku: krok powtórzony sam jeden nie ma po czym
    /// iść, więc jego wejście musi przyjechać z biegu, w którym poprzednicy naprawdę pracowali.
    seeded_from: Option<PathBuf>,
    /// Co każdy krok przejmuje po tamtym biegu — po jednej pozycji na krok, pusto dla zwykłego.
    ///
    /// 2026-08-23 (T-88) — SAMO SKOPIOWANIE PLIKÓW NIE JEST PRZEKAZANIEM ICH DALEJ.
    /// [`seed_the_handoffs`] kładzie je w katalogu nowego biegu od 2026-08-23, ale indeks promptu
    /// powstaje z [`Live::handoffs`], czyli **wyłącznie** z tego, co oddały kroki TEGO biegu —
    /// więc zasiane pliki nie trafiały do żadnego promptu. Do tego wycinek zostawia tylko
    /// strzałki z obydwoma końcami w środku, więc krok na czele wycinka nie ma ani jednego
    /// poprzednika i indeksu nie dostawał wcale.
    ///
    /// Liczone RAZ, przy starcie biegu ([`what_the_run_before_left`]), a nie przy każdym
    /// prompcie: odpowiedź zależy od plików tamtego biegu i od grafu, jak biegł — a jedno i drugie
    /// jest już zamknięte. Liczenie per krok byłoby tą samą odpowiedzią wyliczaną tyle razy, ile
    /// jest kroków, i tyloma miejscami, w których wolno ją wyliczyć inaczej (niezmiennik 13).
    carried: Vec<Vec<Carried>>,
    /// Korzeń projektu — katalog, w którym pracują agenci tego biegu.
    ///
    /// 2026-08-22 — pole doszło dla pętli: żeby zapytać gita, czy ciało pętli cokolwiek zmieniło,
    /// trzeba znać bazę, od której odbite są drzewa kroków (`isolate::touched`). Wyprowadzanie go
    /// z `dir` przez trzy `parent()` byłoby drugim miejscem z odpowiedzią na „gdzie jest projekt",
    /// zależnym od kształtu ścieżki biegu.
    project: PathBuf,
    /// Co ten bieg wiedział, kiedy ruszał. Policzone RAZ, tutaj, z notatek zamrożonych przed
    /// pierwszym procesem — zrzut przepisany na końcu opisywałby pliki, jakimi są PO biegu.
    memory: Vec<MemoryRecord>,
    /// Po jaki materiał ten bieg sięgnął, kiedy ruszał — po jednej pozycji na umiejętność.
    ///
    /// 2026-08-28 (T-154) — TEN SAM KSZTAŁT I TEN SAM MOMENT, CO [`Plan::memory`] obok: odwołanie,
    /// odcisk i liczba bajtów, policzone przed pierwszym procesem i zapisane w `run.json`.
    /// Umiejętność, która pojechała do agenta i nie zostawiła tu śladu, jest faktem o biegu,
    /// którego nikt później nie odtworzy (niezmiennik 4) — a bez odcisku „ten sam krok jeszcze
    /// raz" nie ma jak zauważyć, że jego materiał w międzyczasie się przesunął.
    skills: Vec<SkillRecord>,
    /// Milisekundy epoki: kiedy ten bieg powstał.
    created_at: i64,
    /// Zrodlo triggera; brak pola w JSON zachowuje doslownie ksztalt recznego biegu.
    trigger_origin: Option<TriggerOrigin>,
    /// Kiedy wstała maszyna. Czytane RAZ, przy planowaniu: ten sam bieg ma nosić jedną
    /// odpowiedź, a nie tyle, ile razy ktoś zapyta system.
    boot_id: Option<String>,
    /// Skąd ten bieg bierze wartości wymagane przez zatwierdzone Connections.
    ///
    /// 2026-09 (Z-23) — POLE DOSZŁO, bo do tego dnia jedynym źródłem było środowisko procesu
    /// okna, a aplikacja uruchomiona z Docka nie dziedziczy powłoki człowieka: ten sam bieg
    /// ruszał po starcie z terminala i odmawiał po kliknięciu w ikonę. Nośnik jest częścią
    /// PLANU, a nie odczytem w chwili startu kroku, z tego samego powodu, co polityka i lista
    /// narzędzi obok — biblioteka jest znana przed pierwszym procesem, a dwa kroki tego samego
    /// biegu mają pytać ten sam plik.
    secrets: crate::connections::secrets::Carrier,
    /// Stawki, którymi ten bieg wycenia tury swoich kroków.
    ///
    /// 2026-09 (Z-44) — CZYTANE RAZ, TUTAJ, dokładnie z tego samego powodu, co nośnik wartości
    /// wyżej: biblioteka jest znana przed pierwszym procesem, a dwa kroki tego samego biegu mają
    /// pytać ten sam plik. Cennik poprawiony w połowie biegu nie ma prawa zmienić ceny kroku,
    /// który już rusza — ani odpowiedzi na pytanie, czy wolno mu było ruszyć.
    prices: Prices,
}

#[derive(Debug, Clone)]
struct PlannedRoute {
    from: StepId,
    to: StepId,
    link: ConditionalLink,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RouteDecision {
    step_id: String,
    to: String,
    evidence: RouteEvidence,
}

/// Wybrana droga razem z trwałym paragonem, jeżeli wybór naprawdę zawęził graf.
struct ChosenRoute {
    route: scheduler::Route,
    decision: Option<RouteDecision>,
}

/// Jedna pętla tego biegu: kto orzeka i ile razy wolno próbować.
struct Loop {
    /// Klucz kafelka kroku, z którego wychodzi powrót. To on pisze werdykt.
    judge: String,
    /// Klucz kafelka kroku, DO którego powrót wraca — czyli tego, którego pracę sędzia ocenia.
    ///
    /// 2026-08-22 — bez tego pola nie da się zapytać „czy jest co sprawdzać": pytanie dotyczy
    /// drzewa implementera, nigdy drzewa sędziego. Sędzia z własną świeżą kopią ma drzewo puste
    /// zawsze, więc pytanie postawione u niego pomijałoby KAŻDĄ weryfikację.
    entry: String,
    /// Ile rund ma pętla. Ostatnia runda to `turns - 1`.
    turns: u8,
    /// Klucze kafelków, które ta pętla powtarza, **w kolejności z pliku**. Oba końce powrotu
    /// należą do ciała.
    ///
    /// 2026-08-23 (T-87) — POLE DOSZŁO DLA KROKU ZA PĘTLĄ. Strzałka z pętli na zewnątrz wychodzi
    /// z rundy OSTATNIEJ (`workflow::unroll`), a runda ostatnia pętli, która przeszła wcześniej,
    /// nie biegnie wcale — więc fan-in wisiał na węźle, który z definicji nic nie napisał.
    /// Odpowiedź na „co ta pętla wyprodukowała" jest pytaniem o CAŁE jej ciało, nie o jeden
    /// węzeł, i liczenie go drugi raz z grafu tutaj byłoby drugą definicją słowa „ciało pętli"
    /// (niezmiennik 13) — dlatego jedzie gotowe z [`crate::workflow::unroll`].
    body: Vec<String>,
}

/// Jeden krok, rozpisany przed startem.
struct Planned {
    /// uuid v7 kroku — klucz wiersza w indeksie.
    id: String,
    /// Stabilny klucz WĘZŁA, unikalny w obrębie biegu.
    ///
    /// Dla kroku spoza pętli jest to dosłownie `id` kroku z pliku. Dla rundy pętli jest to ten sam
    /// klucz z sufiksem rundy (`s_test#1`), i to nie jest ozdoba: `steps` w bazie ma
    /// `UNIQUE (run_id, node_key)` (`store::schema`), więc trzy rundy o jednym kluczu wywróciłyby
    /// odbudowę indeksu — **po** zapłaceniu za cały bieg. Tego ograniczenia nie da się zmigrować:
    /// niezmiennik 25 zabrania przepisywania tabel, a `SQLite` nie zmienia `UNIQUE` inaczej.
    node_key: String,
    /// Klucz KAFELKA, czyli `id` kroku z pliku — ten sam dla wszystkich rund.
    ///
    /// Rozdzielony od [`Planned::node_key`], bo jedno pole robiło dotąd dwie różne rzeczy. Okno
    /// rozpoznaje po nim kafelek (`Line::StepState { step_id }` → `withStepStates`) i po nim
    /// zlewa rundy w jedną kartę — czyli jest to warunek właściciela „nie ma być widać, że
    /// spawnujemy nowych agentów". Wysłanie tam klucza z sufiksem znaczyłoby, że okno nie zna
    /// żadnego z nadesłanych kroków i **po cichu porzuca każdą linię stanu**: pasek stoi pusty
    /// przez cały bieg, a kafelek mówi „waiting" do końca.
    ///
    /// Ten sam klucz wyznacza katalog własnej kopii plików (`work/<klucz>`), i to też jest
    /// treścią: rundy pętli MUSZĄ dzielić folder, bo inaczej runda 2 nie widzi poprawek rundy 1
    /// i pętla przestaje mieć sens w swoim jedynym zadaniu.
    tile_key: String,
    /// Która runda pętli, licząc od zera. Zero dla kroku spoza pętli.
    turn: u8,
    /// Do której pętli planu należy ten węzeł — numer pozycji w [`Plan::loops`].
    ///
    /// 2026-08-22 — POLE JEST NOWE i bez niego dwie pętle naraz są niewyrażalne. Dopóki pętla
    /// była jedna, „należy do pętli" dawało się policzyć z jednego faktu: `turn > 0`. Przy dwóch
    /// runda pierwsza pętli frontowej i runda pierwsza pętli backendowej są rundami DWÓCH różnych
    /// pętli, więc werdykt jednej pomijałby rundy drugiej — czyli praca, której nikt nie sprawdził,
    /// jechałaby dalej jako zrobiona.
    in_loop: Option<usize>,
    /// Nazwa z kafelka. To ona jedzie na ekran jako etykieta wiersza — identyfikator kroku
    /// ani uuid agenta nie mają tam czego szukać (niezmiennik 14).
    name: String,
    /// Co zrobic z robota, kiedy ten krok nie przejdzie — wybor czlowieka z pliku workflow.
    ///
    /// Zamrozone przy planowaniu, jak wszystko inne w tej strukturze: plik poprawiony w trakcie
    /// biegu nie ma prawa zmienic zasad biegu, ktory juz ruszyl.
    when_it_fails: WhenItFails,
    /// Klucze węzłów, po których ten krok idzie.
    depends_on: Vec<String>,
    /// Etykieta vendora, którym poszedł ten krok. Pusta dla kafelka kontrolnego: nie woła
    /// żadnego agenta, a wpisanie mu vendora byłoby wymyśleniem faktu, po którym wznowienie
    /// szukałoby kiedyś sesji, której nigdy nie było.
    vendor: String,
    /// Kopie, których pracę ten krok ma znieść do swojej — pusto dla każdego kroku, który
    /// niczego nie składa (2026-08-29).
    ///
    /// Liczona przy PLANOWANIU, jak wszystko inne w tej strukturze, i z tego samego powodu:
    /// odpowiedź zależy od grafu, a graf jest w tej chwili zamrożony. Policzona per krok przy
    /// starcie byłaby drugim miejscem, w którym wolno ją wyliczyć inaczej niż zrobił to
    /// [`where_it_works`] (niezmiennik 13).
    folds_in: Vec<Folded>,
    /// Co ten krok robi.
    job: Job,
}

/// Co krok robi. Dwa rodzaje wobec vendorów (D6, `ARCHITECTURE` §6b) plus jeden, który vendora
/// nie zna — powód w całości stoi przy [`crate::workflow::Step`].
enum Job {
    /// Krok, który woła agenta.
    Agent(Box<AgentJob>),
    /// Kafelek kontrolny: bieg staje i pyta człowieka (T3 §6.1 reguła 5).
    Ask {
        /// Pytanie z kafelka, gotowe na ekran.
        question: Option<String>,
    },
    /// Uruchom i zostaw: Loadout podnosi proces i ODDAJE GO REJESTROWI, zamiast czekać.
    ///
    /// Krok konczy sie w chwili, w ktorej proces WSTAL. Czekanie na jego koniec zatrzymaloby graf
    /// na zawsze - serwer dev nie konczy sie nigdy i wlasnie o to w nim chodzi.
    Serve(Box<ServeJob>),
    /// Krok „sprawdź": Loadout uruchamia komendę sam i sam orzeka.
    ///
    /// Planista nie wie, że ten krok „jest bramką" — dostaje z niego werdykt i nic więcej.
    /// Ani jeden warunek w tym pliku nie nazywa etapu biegu (niezmiennik 27); to ramię mówi,
    /// **czym** jest kafelek, dokładnie jak dwa ramiona obok.
    Check(Box<CheckJob>),
}

/// Wszystko, czego potrzebuje kafelek „uruchom i zostaw".
///
/// Powod istnienia tego kroku stoi w calosci przy [`crate::workflow::ServeStep`]: zderzenie
/// dwoch POPRAWNYCH regul - proces poboczny nie ma prawa przezyc kroku (niezmiennik 6), a
/// weryfikacja przez pomiar zywej aplikacji wymaga, zeby przezyl.
struct ServeJob {
    /// Wiersz powloki, doslownie z pliku.
    command: String,
    /// Nazwa pola przekazania, z ktorego wziac komende, kiedy nie wpisal jej czlowiek.
    ///
    /// `None` znaczy „komende wpisal czlowiek" — powod w calosci stoi przy
    /// [`crate::workflow::ServeStep::command_from`].
    command_from: Option<crate::workflow::CommandFrom>,
    lifetime: crate::workflow::ServiceLifetime,
    start_when: crate::workflow::ServiceStartWhen,
    /// P-02: czym jest ten cel — dla `native` gotowy port jest połową prawdy.
    kind: crate::workflow::TargetKind,
    /// P-02: ustawienie, którym ta aplikacja przyjmuje testowy katalog danych.
    test_data_env: Option<String>,
    endpoints: Vec<crate::workflow::ServiceEndpointSpec>,
    readiness: Option<crate::workflow::ReadinessSpec>,
    /// Katalog, w ktorym to wstaje. Dla serwera dev jest trescia, nie szczegolem: podaje kod
    /// z TEGO drzewa, wiec weryfikacja w kopii kroku oglada dokladnie te prace.
    cwd: PathBuf,
    /// Czy katalog jest nasz - jak [`AgentJob::ours`].
    ours: bool,
}

/// Wszystko, czego krok „sprawdź" potrzebuje, żeby ruszyć — policzone przed startem biegu.
struct CheckJob {
    proof_mode: crate::engine::drivers::command::ProofMode,
    /// Co uruchomić, po czym poznać i gdzie. Prosto z pliku workflow, bez ani jednego naszego
    /// słowa: komenda jest tym, co człowiek wpisał.
    spec: CheckSpec,
    /// Czy katalog roboczy jest nasz, czyli czy mamy go utworzyć — jak [`AgentJob::ours`].
    ours: bool,
}

/// Czym skończyło się czekanie na turę. Cztery stany, bo `Option` umiał powiedzieć dwa, a od
/// T-35 „skończył się czas" jest czymś innym niż „człowiek nacisnął Stop": pierwsze jest
/// porażką kroku z nazwanym powodem, drugie jest anulowaniem i nie jest niczyją winą.
enum Ended {
    /// Tura wróciła sama — z wynikiem albo z błędem sterownika.
    Turn(anyhow::Result<crate::engine::drivers::Outcome>),
    /// Człowiek nacisnął Stop.
    Stopped,
    /// Krok przekroczył swój limit czasu.
    Overdue,
    /// Żywa tura trzeci raz dostała ten sam typowany błąd jednego narzędzia.
    RepeatedToolFailure,
    /// Żywa tura przebiła udział tego kroku w sufcie wydatku biegu.
    OverItsShare {
        /// Ile temu krokowi wolno było wydać — to jest liczba, którą przeczyta człowiek.
        allowed: f64,
    },
}

/// Dlaczego pompa zdarzeń każe zejść turze, która jeszcze trwa.
///
/// 2026-09 (Z-13b) — MAŁY ENUM ZAMIAST `()`. Do tego dnia kanał niósł sam fakt „przerwij",
/// bo powód był dokładnie jeden. Drugi powód potrzebuje własnego zdania dla człowieka **i**
/// własnej liczby, a kanał bez powodu zamieniłby oba w to samo — czyli krok zatrzymany przez
/// sufit tłumaczyłby się awarią narzędzia.
enum WhyTheTurnMustEnd {
    /// Ten sam typowany błąd jednego celu, trzeci raz z rzędu.
    RepeatedToolFailure,
    /// Vendor bez własnej flagi sufitu doniósł, że wydał już więcej, niż mu przyznano.
    OverItsShare {
        /// Udział, który ta tura przebiła.
        allowed: f64,
    },
}

impl From<WhyTheTurnMustEnd> for Ended {
    fn from(why: WhyTheTurnMustEnd) -> Self {
        match why {
            WhyTheTurnMustEnd::RepeatedToolFailure => Self::RepeatedToolFailure,
            WhyTheTurnMustEnd::OverItsShare { allowed } => Self::OverItsShare { allowed },
        }
    }
}

/// Wszystkie pożyczone wejścia jednej żywej tury, zebrane pod jednym właścicielem wywołania.
///
/// Odbiorniki bariery i bezpiecznika należą do tej samej pompy zdarzeń; `slot` należy do tego
/// samego procesu co `handle`. Jeden kształt utrudnia rozdzielenie tych par przy dokładaniu
/// kolejnej drogi końca tury i nie zmienia właściciela żadnego z zasobów.
struct LiveAgentTurn<'a> {
    id: StepId,
    cancel: &'a CancellationToken,
    runtime_fault: &'a mut mpsc::Receiver<WhyTheTurnMustEnd>,
    finished_event: &'a mut mpsc::Receiver<()>,
    finish_forward: &'a CancellationToken,
    reads: &'a [String],
    evidence: &'a EvidenceTarget,
    slot: &'a mut Option<limits::Slot>,
}

/// Start jednej czynności, zachowany do przyjścia wyniku o tym samym identyfikatorze wywołania.
#[derive(Debug)]
struct PendingToolCall {
    action: Action,
    target: String,
}

/// Ostatnia mierzona porażka jednej dokładnej pary `(Action, cel)`.
#[derive(Debug)]
struct ToolFailureRun {
    action: Action,
    target: String,
    output: String,
    repeats: u8,
}

/// Niezależny od vendora bezpiecznik jednej żywej sesji.
///
/// Liczy wyłącznie `ok: false`, dla którego ta sama para zdarzeń przyniosła zarówno
/// strukturalny start (`Action`, pełny cel), jak i niepuste pełne wyjście. `AgentEvent::summary` nie wchodzi
/// tu nigdy: jest tekstem kuracji i trzy różne awarie mogą mieć to samo jednozdaniowe streszczenie.
#[derive(Debug, Default)]
struct RepeatedToolFailures {
    pending: BTreeMap<String, PendingToolCall>,
    run: Option<ToolFailureRun>,
}

impl RepeatedToolFailures {
    /// `true` dokładnie na trzecim identycznym typowanym błędzie jednego celu.
    fn observe(&mut self, event: &AgentEvent, tool: Option<&Tool>) -> bool {
        match (event, tool) {
            (AgentEvent::ToolStart { id, .. }, Some(Tool::Started { action, target })) => {
                // 2026-09-01 — pusty cel nie identyfikuje wywołania. App Server dopuszcza
                // taki fakt na drucie, więc trzy anonimowe narzędzia mogłyby inaczej zostać
                // uznane za jedną serię i fałszywie zatrzymać żywego agenta.
                if target.trim().is_empty() {
                    self.pending.remove(id);
                    return false;
                }
                self.pending.insert(
                    id.clone(),
                    PendingToolCall {
                        action: *action,
                        target: target.clone(),
                    },
                );
                false
            }
            (AgentEvent::ToolEnd { id, ok, .. }, ended) => {
                let Some(started) = self.pending.remove(id) else {
                    self.run = None;
                    return false;
                };

                // Każde ukończone wywołanie, które nie jest kolejną identyczną typowaną porażką, przerywa
                // serię. Inaczej `fail A, fail B, fail A, fail A` fałszywie wygląda jak trzy A.
                if *ok {
                    self.run = None;
                    return false;
                }
                let Some(Tool::Ended { output }) = ended else {
                    self.run = None;
                    return false;
                };
                let Some(output) = normalized_tool_output(output) else {
                    self.run = None;
                    return false;
                };

                if let Some(run) = self.run.as_mut()
                    && run.action == started.action
                    && run.target == started.target
                    && run.output == output
                {
                    run.repeats = run.repeats.saturating_add(1);
                    run.repeats == REPEATED_TOOL_FAILURE_LIMIT
                } else {
                    self.run = Some(ToolFailureRun {
                        action: started.action,
                        target: started.target,
                        output,
                        repeats: 1,
                    });
                    false
                }
            }
            _ => false,
        }
    }
}

/// Minimalna normalizacja transportowa, nie heurystyka komunikatu.
///
/// CRLF i końcowy znak nowego wiersza zależą od adaptera/potoku, nie od przyczyny błędu.
/// Niczego w samym tekście nie maskujemy: spacja, PID, znacznik czasu albo inna zmienna
/// wartość mają uczciwie dać inną porażkę i nie uruchomić bezpiecznika.
fn normalized_tool_output(output: &str) -> Option<String> {
    if output.trim().is_empty() {
        return None;
    }
    let mut output = output.replace("\r\n", "\n").replace('\r', "\n");
    while output.ends_with('\n') {
        output.pop();
    }
    Some(output)
}

/// Co zostało po turze: gotowy wynik albo porażka, którą wolno rozstrzygnąć dopiero PO tym, jak
/// zdarzenia tej tury przejdą przez kuratora.
///
/// 2026-08-23 (T-87) — TEN TYP ISTNIEJE DLA JEDNEJ KOLEJNOŚCI. Krok, którego tura wróciła błędem,
/// oddaje dalej to, co zdążył powiedzieć ([`Live::hand_on_its_last_words`]), a jego proza jedzie
/// tą samą kolejką, co wiersze na ekran. Rozstrzygnięcie porażki w środku [`Live::one_turn`]
/// wyprzedzało tę kolejkę: plik powstawał, zanim ostatnie zdanie agenta z niej wyszło, i raz na
/// jakiś czas wychodził pusty. Wyścigu, którego wynik zależy od tego, kto akurat był szybszy,
/// nie da się ani przetestować, ani powtórzyć.
enum Turned {
    /// Tura skończyła się i jej wynik jest znany.
    Settled(StepReport),
    /// Tura wróciła błędem. W środku zdanie, które ma zobaczyć człowiek.
    Broke(String),
}

/// Czym skończyło się domknięcie sesji kroku — powstaje w [`Live::close_and_prove`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClosedHow {
    /// Agent wyszedł sam po zamknięciu wejścia.
    OnItsOwn,
    /// Agent wymagał eskalacji supervisora po upływie sufitu.
    WouldNotLetGo,
    /// Samo zamknięcie transportu padło.
    Broke,
    /// Stop człowieka wygrał z trwającym zamknięciem.
    StoppedByAPerson,
}

/// Trzy pola, bo trzy fakty są prawdziwie różne i każdy z osobna potrafi odebrać krokowi prawo
/// do słowa „done": sposób zamknięcia, kod lidera albo grupa bez dowodu śmierci
/// (niezmiennik 6).
#[derive(Debug)]
struct Closed {
    /// Czy proces wyszedł sam, wymagał eskalacji, zepsuł transport albo dostał Stop.
    how: ClosedHow,
    /// Kod wyjścia lidera. `None`, kiedy proces zginął od sygnału i kodu po prostu nie ma —
    /// a `None` nigdy nie jest przejściem, bo `None` to nie zero.
    code: Option<i32>,
    /// Dowód, który rozstrzygnął koniec tego kroku.
    ///
    /// Wartość, nie `bool` (2026-08-28): `Alive` niesie adres grupy i jest jedynym kluczem do
    /// [`processes::Unproven::released_by`], czyli do jedynej drogi, którą wolno zwolnić uchwyt
    /// sesji i miejsce z puli. `bool` zgubiłby i adres, i tę drogę.
    proof: GroupProof,
}

/// Czym naprawdę skończył się proces kroku, który wrócił z tury — po odróżnieniu **czyj** był
/// sygnał.
///
/// # Sześć dróg zejścia, i gdzie każda z nich mówi swoje (2026-09, Z-39)
///
/// Ta lista rozstrzyga cztery z nich, bo tylko tyle da się rozróżnić TU, przy [`Closed`].
/// Pozostałe dwie schodzą innymi drzwiami i mają własne zdania — wymienione tutaj, bo droga bez
/// nazwy jest drogą, o której nikt nie pamięta, że istnieje:
///
/// | droga | gdzie | co mówi człowiekowi |
/// |---|---|---|
/// | kod 0 | tutaj, [`HowItWentDown::ExitedCleanly`] | nic; krok przechodzi |
/// | kod ≠ 0 bez sygnału | tutaj, [`HowItWentDown::CodeWithoutASignal`] | powód od sterownika |
/// | sygnał NASZ | tutaj, [`HowItWentDown::LoadoutStoppedIt`] | `cancelled`, albo zdanie o sufcie |
/// | sygnał OBCY | tutaj, [`HowItWentDown::StoppedFromOutside`] | [`stopped_from_outside_sentence`] |
/// | limit czasu | [`Live::stop_overdue_agent`] | „ran longer than its N minute limit" |
/// | brak aplikacji agenta | [`public_start_refusal`] | „could not find … on this Mac" |
///
/// Tamte dwie nie przechodzą tędy z powodu strukturalnego, nie z przeoczenia: limit czasu kończy
/// turę jako [`Ended::Overdue`], czyli zanim powstanie [`Closed`], a vendora, którego nie ma,
/// odkrywa `start` — krok nie ma wtedy procesu, o którego zejście dałoby się zapytać.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HowItWentDown {
    /// Proces wyszedł sam i wyszedł zerem.
    ExitedCleanly,
    /// Proces wyszedł sam, ale nie zerem, i żaden sygnał w tym nie uczestniczył.
    CodeWithoutASignal(i32),
    /// Zejście jest NASZE. Stop człowieka, sufit zamknięcia i awaria transportu kończą się
    /// każde `Supervised::stop`, więc sygnał widoczny w statusie wysłał Loadout — a zdanie
    /// o obcej ręce byłoby wtedy zdaniem nieprawdziwym w najgorszym możliwym miejscu.
    LoadoutStoppedIt,
    /// Sygnał, którego Loadout nie wysłał: bez Stopu, bez limitu czasu i bez własnej eskalacji.
    StoppedFromOutside(i32),
}

impl HowItWentDown {
    /// Zdanie dla człowieka o TEJ drodze — albo `None`, kiedy droga nie ma nic do dodania ponad
    /// to, co i tak mówi stan kroku i powód od sterownika.
    ///
    /// Trzy pierwsze drogi milczą z rozmysłu: kod zero jest przejściem, kod niezerowy niesie
    /// powód, który agent podał sam, a nasze własne zatrzymanie ma już swoje zdanie u siebie.
    /// Zdanie dopisane do którejkolwiek z nich przykryłoby powód prawdziwszy od siebie.
    fn said(self) -> Option<String> {
        match self {
            Self::ExitedCleanly | Self::CodeWithoutASignal(_) | Self::LoadoutStoppedIt => None,
            Self::StoppedFromOutside(signal) => Some(stopped_from_outside_sentence(signal)),
        }
    }
}

/// Rozstrzyga, czyj był sygnał, którym zszedł ten krok.
///
/// # Dwa nośniki jednego faktu, bo powłoka-opakowanie gubi pierwszy
///
/// Status niesie numer sygnału wtedy, gdy zginął od niego proces, który sami zebraliśmy. Kiedy
/// krok jest opakowany powłoką (`sh -c`, `npm`, każdy wrapper), sygnał łapie ona: sprząta
/// i wychodzi `128 + numer`, czyli `exit 143` — dokładnie tak, jak widać to w eksporcie
/// diagnostyki biegu `20260901-150035`. Pytamy więc obu (2026-09, Z-39).
fn how_it_went_down(how: ClosedHow, code: Option<i32>, proof: &GroupProof) -> HowItWentDown {
    if !matches!(how, ClosedHow::OnItsOwn) {
        /* NASZE ZEJŚCIE ROZSTRZYGA SIĘ PIERWSZE i nie pyta o status. `StoppedByAPerson`,
         * `WouldNotLetGo` i `Broke` prowadzą każde przez naszą własną eskalację, więc sygnał,
         * który po nich zostaje w statusie, jest NASZ — a przeczytany jako obcy zamieniłby
         * każdy Stop człowieka w oskarżenie kogoś, kogo tam nie było. */
        return HowItWentDown::LoadoutStoppedIt;
    }
    let from_the_status = match proof {
        GroupProof::Dead {
            status: Some(status),
        } => supervisor::signal_that_ended(*status),
        GroupProof::Dead { status: None } | GroupProof::Alive { .. } => None,
    };
    match (from_the_status, code) {
        (Some(signal), _) => HowItWentDown::StoppedFromOutside(signal),
        (None, None) => HowItWentDown::ExitedCleanly,
        (None, Some(code)) => match supervisor::signal_behind_exit_code(code) {
            Some(signal) => HowItWentDown::StoppedFromOutside(signal),
            None if code == 0 => HowItWentDown::ExitedCleanly,
            None => HowItWentDown::CodeWithoutASignal(code),
        },
    }
}

/// Jednorazowe wejście tury; nadajnik pozostaje własnością startu również przy odmowie.
struct AgentTurn {
    spec: RunSpec,
    target: EvidenceTarget,
    events: mpsc::Sender<DecodedEvent>,
}

/// Wszystko, czego krok agenta potrzebuje, żeby ruszyć — policzone przed startem biegu.
struct AgentJob {
    /// Sterownik vendora, wzięty z fabryki raz, przy planowaniu.
    driver: Arc<dyn AgentDriver>,
    /// 2026-09 (Z-45) — to samo pole pliku workflow, przełożone raz na język wspólnego limitera.
    weight: limits::Weight,
    /// Nazwa agenta z BIBLIOTEKI — ta, którą człowiek widzi i pisze (`Backend Dev`).
    ///
    /// 2026-08-23 (T-92): notatka o zakresie „ten agent" musi umieć powiedzieć, **którego**
    /// agenta dotyczy, bo bez tego trzeci zakres nie wchodzi do żadnego promptu (T-80).
    /// Nazwa efektywna, czyli po nadpisaniu kroku, i nazwa z biblioteki, nie nazwa kafelka:
    /// `Planned::name` jest etykietą tego jednego pola na płótnie, a plik notatki pisze
    /// człowiek i zna z niego agenta, nie kafelek.
    agent_name: String,
    /// Identyfikator sesji przydzielony **z góry**, przed startem procesu [T7 §6.2]. Dzięki
    /// temu wiadomo, pod jakim numerem zapisać krok, zanim vendor cokolwiek powie.
    session: Uuid,
    /// Katalog roboczy kroku.
    cwd: PathBuf,
    /// Czy ten katalog jest nasz, czyli czy mamy go utworzyć.
    ours: bool,
    /// Gdzie ten krok ma odłożyć swoją odpowiedź — **względem [`AgentJob::cwd`]**. `None` znaczy
    /// „człowiek o żaden plik nie prosił" i wtedy nie powstaje nic.
    ///
    /// 2026-08-23 (T-90) — DO TEGO DNIA `writeResultsTo` NIE MIAŁO W CAŁYM DRZEWIE ANI JEDNEGO
    /// CZYTELNIKA. Wiersz w panelu kroku przyjmował ścieżkę, plik ją zapisywał, i nie powstawało
    /// nic — a pusty katalog wygląda dokładnie tak samo jak katalog, do którego agent nie miał
    /// nic do napisania (niezmiennik 16).
    ///
    /// Względem folderu KROKU, nie projektu, i te dwa są tym samym wyłącznie dla kroków
    /// `project`. Krok z własną kopią plików ma odłożyć wynik w NIEJ: pisanie z powrotem do
    /// folderu człowieka jest dokładnie tym, czemu ta własna kopia zapobiega.
    ///
    /// Ścieżka wyprowadzająca poza ten folder jest odmową przy planowaniu ([`where_results_go`]),
    /// więc do tego pola dochodzi wyłącznie ścieżka względna bez `..`. To, czego z napisu nie
    /// widać — dowiązanie położone tam przez kogoś wcześniej — jest odmową o chwilę później,
    /// ale wciąż przed startem kroku ([`Live::the_answer_has_somewhere_to_go`]).
    write_results_to: Option<PathBuf>,
    /// 2026-09-08 — uprawnienie Plan jest zamrożone z fizycznym krokiem; brak pola to Off.
    work_plan: crate::work_plan::Configuration,
    /// 2026-09-08 — roboczy plik jest pod katalogiem kroku; agent dostaje, lecz nie wybiera adresu.
    plan_candidate: PathBuf,
    /// Pola, które ten krok ma oddać obok swojej odpowiedzi. Pusty wektor dla kroku bez
    /// formularza — i to jest odpowiedź, nie brak: krok, o którego pola nikt nie prosił, nie ma
    /// czego oddawać i nie wolno go o nic sądzić.
    ///
    /// 2026-08-23 (T-90) — DWA CZYTELNIKI, DWIE POŁOWY JEDNEJ UMOWY: blok „jak odpowiadać"
    /// mówi agentowi, czego się od niego chce ([`Live::prompt_for`]), a wynik tury sprawdza, czy
    /// to dostał ([`Live::missing_a_required_field`]). Sama prośba jest poleceniem bez skutku
    /// (niezmiennik 16) i uczy model, że tych wierszy można nie pisać; samo wymaganie jest karą
    /// za nieodgadnięcie.
    ///
    /// Zamrożone przy planowaniu, jak wszystko inne w tej strukturze: plik poprawiony w trakcie
    /// biegu nie ma prawa zmienić zasad biegu, który już ruszył.
    handover: Vec<HandoverField>,
    /// V-02: zatwierdzone wymagania, o które ten krok ma odpowiedzieć. Pusta lista zachowuje
    /// dotychczasowy kontrakt sędziego pętli.
    criteria: Vec<crate::workflow::criteria::Criterion>,
    /// Planowana część promptu: notatki, zadanie biegu i instrukcja kafelka, już złożone.
    ///
    /// 2026-08-23 — KOMENTARZ MÓWIŁ „instrukcje kroku, dosłownie z pliku workflow" i przestał
    /// być prawdą, odkąd `plan_step` składa tu blok „co wiadomo" i nagłówek zadania biegu.
    /// Kosztowało to tytuły WSZYSTKICH przekazań: `title_of` czytało to pole, więc od chwili,
    /// w której bieg zaczął nosić zadanie, każdy tytuł zaczynał się tym samym nagłówkiem.
    /// Zmierzone na biegu `20260823-011240`: 19 przekazań, 19 identycznych tytułów, lista
    /// „co kroki sobie przekazały" nie do przejrzenia. Surowa instrukcja stoi teraz obok,
    /// w [`AgentJob::asked`].
    ///
    /// To jeszcze **nie** jest cały prompt: indeks przekazań poprzedników dokłada
    /// [`Live::prompt_for`] w chwili startu kroku. Przy planowaniu nie zszedł jeszcze
    /// nikt, więc indeksu nie ma tu z czego zbudować. Jedno i drugie jedzie do sterownika jako
    /// **dane** i wychodzi stdinem (niezmiennik 9).
    prompt: String,
    /// Osobny blok danych referencyjnych. Nie trafia do instrukcji systemowych agenta.
    reference_materials: String,
    /// O co poproszono TEN kafelek — dosłownie z pliku workflow, bez ani jednego naszego bajtu.
    ///
    /// Jedyne zdanie o tym kroku, które napisał człowiek, więc jedyne, które nadaje się na tytuł
    /// przekazania. Osobne pole, a nie ponowne składanie z `prompt`: rozbieranie własnego
    /// wyniku, żeby wyjąć z niego to, co się przed chwilą włożyło, rozjeżdża się przy pierwszym
    /// nowym bloku dokładanym do promptu — i tak właśnie powstał defekt, który to naprawia.
    asked: String,
    /// Dokładne źródła planowanej części promptu, bez treści. Przekazania dopisuje
    /// [`Live::prompt_for`] dopiero wtedy, gdy naprawdę istnieją.
    context: Vec<ContextSource>,
    /// Dla każdej pasującej aktywnej notatki: weszła do planowanej części promptu albo została
    /// odłożona przez limit. UUID fizycznego kroku dochodzi dopiero po udanym starcie procesu.
    memory: Vec<MemoryDisposition>,
    /// Model z konfiguracji efektywnej.
    model: Option<String>,
    /// Prompt systemowy agenta. To jest konfiguracja agenta, nie treść zadania.
    system_append: Option<String>,
    /// Co agentowi wolno zrobić z plikami — po ludzku, w trzech wariantach.
    /// Czy ten krok sięga do internetu — wybór agenta, policzony raz przy planowaniu.
    reaches_the_web: bool,
    policy: Policy,
    /// Które narzędzia ten krok ma pod ręką — albo `None`, czyli „tyle, ile daje polityka".
    ///
    /// Lista z definicji agenta, już przepuszczona przez sufit jego dialu
    /// (`what_this_step_may_use`). Policzona **przy planowaniu**, a nie w chwili startu kroku,
    /// z tego samego powodu, z którego stoi tu polityka: krok, który miałby to policzyć sam,
    /// mógłby odmówić w połowie biegu, a niezmiennik 12 mówi „najpóźniej przy Starcie".
    tools: Option<Vec<String>>,
    /// Który szczebel „ile myśleć" ma ten krok — z definicji efektywnej, czyli po nadpisaniu.
    ///
    /// Szczebel, nie gotowa flaga: nazwę flagi zna wyłącznie adapter
    /// (`AgentDriver::effort_argv`), a ta warstwa nie skleja komendy i nie zna ani jednej flagi
    /// vendora (niezmiennik 9). Policzone przy planowaniu, tak samo jak polityka i narzędzia.
    thinking: Thinking,
    /// Zatwierdzone Connections rozwiązane podczas planowania, zanim ruszy pierwszy proces.
    connections: Vec<crate::connections::Connection>,
    /// WF-28: sufit zamrożony z efektywnego agenta; sesja nie może go poszerzyć.
    service_access: Vec<crate::library::agents::ServiceGrant>,
    agent_messages: bool,
    /// Przelotka `vendorOptions` tego agenta, **już w kształcie argv jego aplikacji**.
    ///
    /// 2026-08-23 (T-90) — pole doszło, bo do tego dnia przelotka nie docierała do procesu ani
    /// jedną flagą. Policzona **przy planowaniu**, dokładnie jak polityka i lista narzędzi obok:
    /// wpis, którego krok nie ma prawa podać, jest odmową, a niezmiennik 12 mówi „najpóźniej
    /// przy Starcie". Krok bez przelotki ma tu pustą listę i jego argv nie zmienia się o bajt.
    passthrough: Vec<String>,
    /// Umiejętności, które ten krok naprawdę dostanie — policzone z efektywnego agenta.
    ///
    /// Policzone **przy planowaniu**, z tego samego powodu, z którego stoją tu narzędzia: nazwa,
    /// której krok nie może dostać, jest odmową, a niezmiennik 12 mówi „najpóźniej przy Starcie,
    /// nigdy w trakcie biegu". Krok, który liczyłby to sam, odmawiałby po tym, jak pierwszy agent
    /// został już opłacony.
    skills: StepSkills,
    /// `["--plugin-dir", <katalog>]` dla umiejętności TEGO kroku — albo nic.
    ///
    /// Puste przy planowaniu i wypełniane dopiero przez [`hand_the_skills_to_the_steps`]: ścieżka
    /// wskazuje katalog pod katalogiem biegu, a ten w chwili planowania jeszcze nie istnieje
    /// (plan jest czystym rachunkiem — planowanie, które zapisuje, nie da się powtórzyć przy
    /// wznowieniu).
    plugin_flags: Vec<String>,
    /// Nazwy, które ten kafelek pożyczył z repozytorium gospodarza — dosłownie z pliku workflow.
    ///
    /// Sprawdzone przy PLANOWANIU ([`borrowing_is_possible`]), przepisane dopiero po powstaniu
    /// katalogu biegu ([`bring_in_what_each_step_borrowed`]). Dwa pola, a nie jedno, bo to są
    /// dwa różne fakty: o co poproszono i co z tego naprawdę przyszło — a przy jednym polu
    /// „nie znaleziono" byłoby nie do odróżnienia od „nie proszono".
    borrows: Chosen,
    /// …i to, co naprawdę przyszło: fragment argv plus tekst do promptu.
    ///
    /// Puste przy planowaniu i wypełniane dopiero przez [`bring_in_what_each_step_borrowed`],
    /// z tego samego powodu, co [`AgentJob::plugin_flags`]: plan jest czystym rachunkiem, a to
    /// pole stoi na katalogu, którego w chwili planowania jeszcze nie ma.
    borrowed: Inherited,
    /// Po ilu minutach bez końca tury odbieramy krokowi robotę. `Duration::MAX` znaczy „nigdy".
    ///
    /// 2026-08-17 (T-35) — do tego dnia `give_up_after_minutes` z definicji agenta NIE MIAŁO
    /// ANI JEDNEGO CZYTELNIKA: zaklinowany agent wisiał do ręcznego Stopu. Według taksonomii
    /// tego repo to błąd **finansowy**, nie higieniczny — proces pali limit u dostawcy tak
    /// długo, jak długo nikt nie patrzy. `ARCHITECTURE.md` §11 zapowiada właśnie tę ochronę
    /// zamiast `--max-turns`.
    give_up_after: Duration,
    /// Ten sam limit, **liczbą minut i nietknięty** — dokładnie tak, jak stoi w definicji
    /// efektywnej (agent plus nadpisanie kroku).
    ///
    /// 2026-08-23 (T-86) — osobne pole obok [`AgentJob::give_up_after`], a nie liczba wyjęta
    /// z tamtego `Duration`, bo tamto pole niesie już naszą decyzję o zabijaniu i przy braku
    /// limitu stoi w nim `Duration::MAX`. Zdanie zbudowane z tamtej wartości mówiłoby agentowi
    /// bez limitu o pięciuset osiemdziesięciu czterech tysiącach lat.
    ///
    /// `0` znaczy „bez limitu" (`library::agents::Agent::give_up_after_minutes`).
    minutes: u32,
    /// Migawka konfiguracji **efektywnej**, zamrożona w chwili startu [T4 §5.2 p. 3].
    effective: Value,
}

/// Wczytuje plik, sprawdza go drugi raz i rozpisuje bieg — **bez dotykania dysku**.
fn plan_run(deps: &RunDeps<'_>, request: &RunRequest) -> Result<Plan, RunError> {
    plan_run_with_identity(
        deps,
        request,
        Uuid::now_v7().to_string(),
        now_ms(),
        None,
        None,
    )
}

fn planned_routes(
    file: &WorkflowFile,
    steps: &[Planned],
    arrows: &[(StepId, StepId)],
) -> Result<Vec<PlannedRoute>, RunError> {
    let Some(value) = file.extra.get("linkConditions") else {
        return Ok(Vec::new());
    };
    let declared: Vec<ConditionalLink> = serde_json::from_value(value.clone())?;
    let mut routes = Vec::new();
    for condition in declared {
        let mut found = false;
        for &(from, to) in arrows {
            if steps[from].tile_key == condition.from && steps[to].tile_key == condition.to {
                found = true;
                routes.push(PlannedRoute {
                    from,
                    to,
                    link: condition.clone(),
                });
            }
        }
        if !found {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "An imported condition points to a connection that is not in this workflow.",
            )
            .into());
        }
    }
    Ok(routes)
}

fn plan_triggered_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    delivery: &TriggerDelivery,
) -> Result<Plan, RunError> {
    plan_run_with_identity(
        deps,
        request,
        delivery.claim.run_id.clone(),
        delivery.created_at,
        Some(TriggerOrigin {
            slug: delivery.claim.slug.clone(),
            delivery_id: delivery.claim.delivery_id.clone(),
            issue_id: delivery.issue.id.clone(),
        }),
        None,
    )
}

/// Bajty i graf, od ktorych ten bieg startuje — i dowod, ze to wciaz TEN plik.
///
/// Bajty czyta sie osobno od `load()` z jednego powodu: odcisk ma odpowiadac na pytanie
/// „czy to ten sam PLIK", a odcisk liczony z naszej serializacji milczalby o kazdej zmianie,
/// ktorej nie rozumiemy. Powtorzenie bierze bajty z nagrania, wiec plik na dysku mogl
/// w miedzyczasie zniknac — i to jest w porzadku.
fn the_graph_this_run_starts_from(
    request: &RunRequest,
    lead_start: Option<&super::lead_start::LeadStart>,
    replay: Option<&Arc<super::replay::ReplayMaterial>>,
    recorded: Option<&Arc<super::replay::ReplayMaterial>>,
) -> Result<(Vec<u8>, WorkflowFile), RunError> {
    if let Some(replay) = replay {
        replay.validate().map_err(|message| {
            RunError::Refused(Note {
                level: Level::Problem,
                step_id: None,
                fix: None,
                message,
            })
        })?;
    }
    let bytes = if let Some(replay) = recorded {
        replay.graph_bytes.clone()
    } else {
        fs::read(&request.workflow)?
    };
    let file = if let Some(start) = lead_start {
        if recorded.is_none() && crate::durable_file::revision_of(&bytes) != start.expected_revision
        {
            return Err(RunError::Refused(Note {
                level: Level::Problem, step_id: None, fix: None,
                message: "Nothing started: the workflow changed after the lead selected it. Ask to start it again.".to_owned(),
            }));
        }
        // Nie drugi load(path): rewizja i deserializowany graf muszą opisywać te same bajty.
        crate::workflow::file::load_snapshot(&request.workflow, &bytes)?
    } else {
        load(&request.workflow)?
    };
    Ok((bytes, file))
}

/// Dwie odmowy, ktore padaja ZANIM powstanie katalog biegu i ruszy pierwszy proces.
///
/// Obie sa tu z tego samego powodu (niezmiennik 12): awaria w polowie biegu zostawia
/// obciety transkrypt i `run.json`, ktory mowi „running" o czyms, co juz nie zyje.
fn nothing_stops_this_run(
    file: &WorkflowFile,
    project: &Path,
    part: Option<&Part>,
) -> Result<crate::workflow::work_plan::AuthorMap, RunError> {
    // Bieg nie ufa UI (T3 §5.2): plik mógł zostać zmergowany gitem albo poprawiony ręcznie
    // między zapisem a naciśnięciem Start. Odmawiamy zdaniem WALIDATORA, słowo w słowo —
    // własne tłumaczenie byłoby drugim miejscem, w którym mieszka ten sam komunikat.
    // `check_to_run`, nie `check`: krok bez agenta jest przy zapisie ostrzeżeniem (szkic
    // w połowie zbudowany ma się zapisać), a tutaj problemem — bo za sekundę miałby ruszyć.
    // Powód w całości stoi przy `workflow::check::check_to_run`.
    if let Some(refusal) = check_to_run(file)
        .into_iter()
        .find(|note| note.level == Level::Problem)
    {
        return Err(RunError::Refused(refusal));
    }

    // 2026-09-08 (CT-05, zawężone przez CT-06): zawężenie `/run` liczymy tu, bo brak
    // w kafelku, który tym razem nie rusza, nie może blokować poprawnej części — ale nadal
    // przed pierwszym procesem.
    //
    // 2026-09-08 (CT-06) — WOŁANIE `context_ready_to_start` ZNIKA STĄD CELOWO, a nie przez
    // przeoczenie w merge'u. Materiał referencyjny rozwiązuje, zamraża i odmawia jedno
    // miejsce: `freeze_context_inputs` (niezmiennik 13). Drugie sprawdzenie w tym bloku
    // egzekwowałoby przy tym STARE znaczenie limitu 24 KiB — CT-06 ogranicza nim dodatek do
    // promptu, a nie prywatny pakiet, bo agent ma zawsze móc doczytać pełny materiał przez
    // most. Sprawdzenie PLANU zostaje: należy do WP-02 i ma tu swojego jedynego wołającego.
    let unrolled = crate::workflow::unroll::unroll(file);
    let wanted = which_nodes(&unrolled, file, part);
    let included = unrolled
        .nodes
        .iter()
        .zip(wanted)
        .filter_map(|(node, wanted)| {
            wanted
                .then(|| file.steps.get(node.step).map(Step::id))
                .flatten()
        })
        .collect::<BTreeSet<_>>();
    let work_plan_sources =
        super::workflow_plan::plan_ready_to_start(file, &included).map_err(|refusal| {
            RunError::Refused(Note {
                level: Level::Problem,
                step_id: (!refusal.step_id.is_empty()).then_some(refusal.step_id),
                message: refusal.message,
                fix: None,
            })
        })?;

    /* PRÓG DYSKU, sprawdzany zanim ruszy pierwszy proces (T-208, 2026-08-29).
     *
     * Bieg pisze do własnych drzew roboczych i do transkryptów przez cały czas trwania.
     * Dysk, który skończy się w POŁOWIE, nie daje czystej odmowy: zostawia obcięty
     * `agent-<id>.jsonl`, drzewo w połowie wypisane i `run.json`, który mówi „running" o czymś,
     * co już nie żyje. Odmowa przed startem jest jedynym momentem, w którym ta awaria jest
     * jeszcze tania — dokładnie ta sama logika, co przy niezmienniku 12.
     *
     * Nieodczytany stan dysku NIE blokuje biegu. Odmowa dlatego, że nie umiemy o coś zapytać,
     * byłaby gorsza od ryzyka, przed którym stoi ten próg. */
    if let Ok(free) = crate::engine::supervisor::free_bytes(project)
        && let Some(refusal) = no_room_refusal(free)
    {
        return Err(RunError::Refused(refusal));
    }
    Ok(work_plan_sources)
}

/// Sklada wszystko, czego planista potrzebuje o tym biegu, w jeden zamrozony obraz.
///
/// Jeden na bieg, nie jeden na krok, i to jest tresc tej funkcji: „co wiadomo" i „co budujemy"
/// odczytane per krok mogloby sie miedzy krokami jednego biegu roznic, a wtedy zadne z tych
/// dwoch pytan nie ma juz jednej odpowiedzi.
fn the_setup_for<'a>(
    deps: &'a RunDeps<'_>,
    request: &RunRequest,
    file: &'a WorkflowFile,
    unrolled: &'a Unrolled,
    wanted: &'a [bool],
    dir: &'a Path,
    recorded: Option<&'a Arc<super::replay::ReplayMaterial>>,
) -> Result<Setup<'a>, RunError> {
    let inputs = super::run_inputs::RunInputs::from_graph(file)
        .map_err(|why| RunError::Io(io::Error::other(why)))?;
    let folders = crate::workflow::unroll::folders::working_folders_at(
        file,
        unrolled,
        &inputs,
        deps.project,
        &dir.join(WORK_DIR),
    )
    .map_err(|why| RunError::Io(io::Error::other(why)))?;
    Ok(Setup {
        replay: recorded.map(AsRef::as_ref),
        wanted,
        inputs,
        folders,
        library: deps.home.join(AGENTS_DIR),
        connections: deps.home.join("connections"),
        data: deps.home,
        knows: if let Some(replay) = recorded {
            match &replay.memory_sources {
                Some(snapshot) => frozen_known(snapshot, deps.home, deps.project, None)?,
                None => empty_known(),
            }
        } else {
            what_the_agents_know(deps.home, deps.project)
        },
        is_ask: false,
        /* Zadanie z wiersza wejścia, przycięte. Brak zadania i zadanie z samych spacji to jeden
         * fakt („nic nie kazano"), a dwa różne prompty za jeden fakt to dwie różne odpowiedzi
         * na pytanie, co ten bieg buduje. */
        task: request
            .task
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_owned(),
        project: deps.project,
        dir,
        drivers: &deps.drivers,
        file,
        /* GRAF, NIE SAM PLIK: „krok przede mną" liczy się po ROZWINIĘCIU, więc runda druga pętli
         * schodzi z sędziego rundy pierwszej, a nie z kroku, który stoi przed pętlą. */
        unrolled,
    })
}

/// Strzalki wycinka, przenumerowane na jego pozycje.
fn arrows_between(
    unrolled: &Unrolled,
    place: &[Option<StepId>],
    part: Option<&Part>,
) -> Vec<(StepId, StepId)> {
    /* STRZAŁKI ZALEŻĄ OD TEGO, O KTÓRY WYCINEK CHODZI, i to jest cała różnica między dwoma
     * rodzajami powtórzenia.
     *
     * `Just` ich NIE MA: „po czym idzie ten krok" jest pytaniem o graf, a graf przy powtórzeniu
     * jednego kafelka nie ma zastosowania — poprzednicy już przebiegli i ich wynik leży
     * w przekazaniach. Zostawienie strzałek dałoby krok czekający w nieskończoność na rodzica,
     * którego w tym biegu nie ma.
     *
     * `Onward` je ZOSTAWIA, przenumerowane na pozycje w wycinku. Tam kroki po wskazanym mają iść
     * po sobie nawzajem dokładnie tak, jak narysował je człowiek — inaczej „kontynuuj od tego
     * miejsca" wypuściłoby całą resztę grafu naraz, bez ani jednej zależności.
     *
     * `filter_map` po OBU końcach, nie po jednym: strzałka wchodząca do wycinka z zewnątrz
     * (czyli od kroku, który już przebiegł) nie ma prawa zostać — to jest ten sam rodzaj
     * czekania na nieobecnego rodzica. */
    if matches!(part, Some(Part::Just(_))) {
        Vec::new()
    } else {
        unrolled
            .arrows
            .iter()
            .filter_map(|(from, to)| {
                Some((*place.get(*from)?.as_ref()?, *place.get(*to)?.as_ref()?))
            })
            .collect()
    }
}

/// Petle tego biegu, policzone z ROZWINIECIA.
fn loops_of(unrolled: &Unrolled, file: &WorkflowFile) -> Vec<Loop> {
    /* Pętle z ROZWINIĘCIA, nie z pliku, i ta różnica jest treścią: `unroll` odrzuca powrót,
     * którego ciało przecina cudze (plik z takim powrotem odmawia `check_to_run` przed planem,
     * ale ta funkcja nie ma prawa liczyć pętli inaczej niż ten, kto je rozwija). Dzięki
     * temu numer pozycji tutaj, w `Planned::in_loop` i w `settled_at` znaczy wszędzie to samo. */
    unrolled
        .loops
        .iter()
        .filter_map(|one| {
            Some(Loop {
                judge: file.steps.get(one.judge)?.id().to_owned(),
                entry: file.steps.get(one.entry)?.id().to_owned(),
                turns: one.turns,
                // `BTreeSet` chodzi rosnąco, więc ciało wychodzi stąd w kolejności z pliku — tej
                // samej, w której `unroll` emituje węzły i w której czyta się `ls handoffs/`.
                body: one
                    .body
                    .iter()
                    .filter_map(|&at| file.steps.get(at).map(|step| step.id().to_owned()))
                    .collect(),
            })
        })
        .collect()
}

/// Wejscia biegu zwiazane z krokami sprawdzajacymi — po ktorej strzalce ktore sprawdzenie idzie.
///
/// Odmowa jest tu Refused, a nie Io: niezwiazane sprawdzenie jest wada RYSUNKU, ktora czlowiek
/// poprawia na plotnie, a nie awaria systemu plikow.
fn the_inputs_bound_to_checks(
    file: &WorkflowFile,
    steps: &[Planned],
    arrows: &[(StepId, StepId)],
) -> Result<super::run_inputs::BoundInputs, RunError> {
    let input_nodes: Vec<_> = steps
        .iter()
        .map(|step| super::run_inputs::InputNode {
            node_key: &step.node_key,
            is_check: matches!(&step.job, Job::Check(_)),
        })
        .collect();
    super::run_inputs::RunInputs::from_graph(file)
        .and_then(|inputs| inputs.bind_checks(&input_nodes, arrows))
        .map_err(|message| {
            RunError::Refused(Note {
                level: Level::Problem,
                step_id: None,
                message,
                fix: None,
            })
        })
}

fn freeze_context_inputs(
    deps: &RunDeps<'_>,
    file: &WorkflowFile,
    setup: &Setup<'_>,
    steps: &mut [Planned],
    overlay: Option<&super::lead_start::LeadContextOverlay>,
) -> Result<Option<super::context_sources::Snapshot>, RunError> {
    let recipient_values = steps
        .iter()
        .filter(|step| matches!(&step.job, Job::Agent(_)))
        .map(|step| {
            (
                step.node_key.clone(),
                step.tile_key.clone(),
                step.name.clone(),
            )
        })
        .collect::<Vec<_>>();
    let recipients = recipient_values
        .iter()
        .map(
            |(node_key, tile_key, name)| super::context_inputs::Recipient {
                node_key,
                tile_key,
                name,
            },
        )
        .collect::<Vec<_>>();
    if let Some(super::lead_start::LeadContextOverlay {
        target: super::lead_start::LeadContextTarget::Steps { step_ids },
        ..
    }) = overlay
    {
        let agents = recipient_values
            .iter()
            .map(|(_, tile_key, _)| tile_key.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let chosen = step_ids
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        if chosen.len() != step_ids.len() || chosen.is_empty() || !chosen.is_subset(&agents) {
            return Err(RunError::Refused(Note {
                level: Level::Problem,
                step_id: None,
                message:
                    "Choose at least one agent step from this preview for the conversation Context."
                        .to_owned(),
                fix: None,
            }));
        }
    }
    let prepared = match overlay {
        Some(overlay) => super::context_inputs::prepare_with_overlay(
            deps.home,
            file,
            &setup.inputs,
            &recipients,
            Some(overlay),
        ),
        None => super::context_inputs::prepare(deps.home, file, &setup.inputs, &recipients),
    }
    .map_err(|refusal| {
        RunError::Refused(Note {
            level: Level::Problem,
            step_id: (!refusal.step_id.is_empty()).then_some(refusal.step_id),
            message: refusal.message,
            fix: None,
        })
    })?;
    for step in steps {
        if let Job::Agent(job) = &mut step.job {
            prepared
                .prompt_for(&step.node_key)
                .clone_into(&mut job.reference_materials);
            job.context.extend(
                prepared
                    .evidence_for(&step.node_key)
                    .map_err(RunError::Io)?,
            );
        }
    }
    Ok(prepared.into_snapshot())
}

fn plan_run_with_identity(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    id: String,
    created_at: i64,
    trigger_origin: Option<TriggerOrigin>,
    lead_start: Option<&super::lead_start::LeadStart>,
) -> Result<Plan, RunError> {
    // Bajty czytamy osobno od `load()`, bo odcisk ma odpowiadać na pytanie „czy to ten sam
    // PLIK". Odcisk liczony z naszej serializacji odpowiadałby na pytanie „czy to ten sam plik
    // po przejściu przez nas", czyli milczałby o każdej zmianie, której nie rozumiemy.
    let replay = lead_start.and_then(|start| start.replay.as_ref());
    let recorded = replay.filter(|replay| replay.mode == super::replay::ReplayMode::Recorded);
    let (bytes, file) = the_graph_this_run_starts_from(request, lead_start, replay, recorded)?;
    let work_plan_sources = nothing_stops_this_run(&file, deps.project, request.part.as_ref())?;

    /* CENNIK CZYTANY TU, PRZED KATALOGIEM BIEGU (2026-09, Z-44). Plik, którego nie da się
     * przeczytać, jest odmową Startu, a nie cichym powrotem do tabeli wbudowanej: literówka
     * w przecinku zmieniałaby wtedy cenę biegu i nikt by się o tym nie dowiedział. Odmowa pada
     * ZANIM cokolwiek powstanie na dysku i zanim ruszy pierwszy proces (niezmiennik 12). */
    let prices = the_prices_this_run_will_use(deps.home)?;

    let dir = run_directory(deps.project, &id, created_at);

    /* ROZWINIĘCIE PĘTLI, i to jest jedyne miejsce, w którym plik przestaje odpowiadać planowi
     * jeden do jednego. `unroll` oddaje graf BEZ cykli o większej liczbie węzłów, więc wszystko
     * niżej — `Dag`, pula miejsc, dowód śmierci grupy, anulowanie — nie widzi żadnej różnicy.
     * Plik bez ani jednego powrotu wychodzi z `unroll` w kształcie 1:1 (dowodzi tego kryterium
     * `a_file_with_no_way_back_comes_out_unchanged`), więc żaden istniejący bieg się nie zmienia. */
    let unrolled = crate::workflow::unroll::unroll(&file);
    let wanted = which_nodes(&unrolled, &file, request.part.as_ref());
    let setup = the_setup_for(deps, request, &file, &unrolled, &wanted, &dir, recorded)?;
    let (mut steps, place) = plan_the_nodes(&unrolled, &file, &wanted, &setup)?;
    let arrows = arrows_between(&unrolled, &place, request.part.as_ref());
    let loops = loops_of(&unrolled, &file);
    // Klucze najpierw, dopiero potem dopisywanie: `steps[child]` i `steps[parent]` naraz to
    // dwie pożyczki jednego wektora, a nie dwie różne rzeczy.
    let keys: Vec<String> = steps.iter().map(|step| step.node_key.clone()).collect();
    for &(parent, child) in &arrows {
        steps[child].depends_on.push(keys[parent].clone());
    }
    let routes = planned_routes(&file, &steps, &arrows)?;
    let context_sources = freeze_context_inputs(
        deps,
        &file,
        &setup,
        &mut steps,
        lead_start.and_then(|start| start.context.as_ref()),
    )?;
    let memory = what_this_run_knew(&setup.knows, &steps, deps.home, deps.project);
    let memory_sources =
        freeze_memory_sources(&memory, &steps, deps.home, deps.project, recorded.is_none())?;
    /* ZAMROŻENIE JEST JEDNORAZOWE, WIĘC BRAMKA STOI PRZED PLANEM, a nie w kroku: odmowa ma paść
     * ZANIM powstanie katalog biegu i zanim ruszy pierwszy proces (niezmiennik 12). Zwykły Start
     * jej nie widzi — `handoffs_from` niesie wyłącznie powtórzenie i wznowienie (`commands::rerun`). */
    if recorded.is_none() {
        the_frozen_skills_are_still_here(request.handoffs_from.as_deref(), &steps)?;
    }
    let skills = if recorded.is_some() {
        Vec::new()
    } else {
        what_this_run_froze(&steps)?
    };

    // Związane PRZED planem: `setup` pożycza `dir`, a `dir` jedzie do planu przeniesieniem.
    let asked_for = setup.task.clone();
    let inputs = the_inputs_bound_to_checks(&file, &steps, &arrows)?;
    Ok(Plan {
        replay: replay.cloned(),
        inputs,
        work_plan_sources,
        protection: BTreeMap::new(),
        id,
        dir,
        lead_origin: lead_start.map(|start| start.origin.clone()),
        input_snapshot: None,
        workspace_inputs: BTreeMap::new(),
        project_instructions: None,
        memory_sources,
        context_sources,
        additional_inputs: file
            .additional_inputs()
            .map_err(|said| RunError::Io(io::Error::other(said)))?,
        starting_results: BTreeMap::new(),
        title: file.name.clone(),
        workflow_id: file.id.clone(),
        hash: fingerprint(&bytes),
        graph: serde_json::to_value(&file)?,
        arrows,
        routes,
        concurrency: request.how_many_at_once,
        task: asked_for,
        loops,
        seeded_from: request.handoffs_from.clone(),
        // Pusto AŻ DO CHWILI, W KTÓREJ KATALOG BIEGU ISTNIEJE: ścieżki w tym wektorze
        // prowadzą do KOPII, a kopii jeszcze nie ma — plan nie dotyka dysku ani razu.
        // Wypełnia go `the_planned_run` zaraz po `seed_the_handoffs`.
        carried: Vec::new(),
        project: deps.project.to_path_buf(),
        steps,
        memory,
        skills,
        created_at,
        trigger_origin,
        // Pytamy system RAZ, tutaj: ten bieg ma nosić jedną odpowiedź przez całe życie.
        // Odczyt przy każdym zrzucie dałby wartości, które teoretycznie mogą się różnić —
        // i strażnik porównywałby wtedy coś z czymś innym.
        boot_id: crate::engine::supervisor::machine_booted_at(),
        secrets: crate::connections::secrets::Carrier::in_library(Some(deps.home)),
        prices,
    })
}

/// Stawki z biblioteki — albo odmowa Startu jednym zdaniem nazywającym plik do poprawienia.
///
/// Osobna funkcja, bo pytanie stawiają dwie drogi planowania (bieg z pliku i `/ask`), a odmowa
/// ma brzmieć w obu tak samo (niezmiennik 13). Zdanie przychodzi gotowe z
/// [`Prices::from_library`]: to ono wie, o który plik chodziło i co się w nim nie zgadzało.
fn the_prices_this_run_will_use(library: &Path) -> Result<Prices, RunError> {
    Prices::from_library(library).map_err(|why| {
        RunError::Refused(Note {
            level: Level::Problem,
            // Kropki na kafelku nie ma: to nie jest wada żadnego kroku, tylko pliku obok biegu.
            step_id: None,
            message: why.to_string(),
            fix: None,
        })
    })
}

/// Węzły rozwinięcia → kroki planu, plus mapa „gdzie każdy węzeł wylądował w wycinku".
///
/// Osobna funkcja, a nie pętla w [`plan_run_with_identity`]: to jest jedno zamknięte pytanie
/// i jedyne miejsce, w którym numeracja wycinka spotyka się z numeracją grafu. `None` w mapie
/// znaczy „ten węzeł nie wszedł do biegu".
fn plan_the_nodes(
    unrolled: &Unrolled,
    file: &WorkflowFile,
    wanted: &[bool],
    setup: &Setup<'_>,
) -> Result<(Vec<Planned>, Vec<Option<StepId>>), RunError> {
    let mut place: Vec<Option<StepId>> = vec![None; unrolled.nodes.len()];
    let mut steps = Vec::with_capacity(unrolled.nodes.len());
    for (index, node) in unrolled.nodes.iter().enumerate() {
        let Some(step) = file.steps.get(node.step) else {
            continue;
        };
        if wanted.get(index) != Some(&true) {
            continue;
        }
        /* Numer pętli bierze się z ciał policzonych przez `unroll`, a nie z drugiego obchodu
         * grafu tutaj: jedna definicja słowa „ten krok jest w tej pętli" (niezmiennik 13). */
        let in_loop = unrolled
            .loops
            .iter()
            .position(|one| one.body.contains(&node.step));
        /* `index` zostaje NUMEREM WĘZŁA ROZWINIĘCIA, a nie pozycją w `steps`, i to jest wymóg:
         * `where_it_works` pyta nim `trees_before` o poprzedników w grafie. Pozycja w wycinku
         * wskazywałaby cudzy węzeł, czyli cudze drzewo robocze. */
        place[index] = Some(steps.len());
        steps.push(plan_step(
            step, index, node.turn, node.copy, in_loop, setup,
        )?);
    }
    Ok((steps, place))
}

fn run_directory(project: &Path, id: &str, created_at: i64) -> PathBuf {
    project
        .join(PROJECT_DIR)
        .join(RUNS_DIR)
        .join(format!("{}__{id}", stamp(created_at)))
}

/// Czym jest „krok", kiedy kroku nie ma — nazwa dla odmów [`find_agent`] w biegu z `/ask`.
///
/// Tamte dwa zdania wplatają nazwę kroku, bo w biegu z pliku człowiek szuka KAFELKA. Tutaj
/// kafelka nie ma, a nazwa agenta byłaby najgorszym z możliwych wypełnień: odmowa brzmiałaby
/// „Scout has nothing to run" o agencie, którego w bibliotece nie ma.
const THE_ASK: &str = "/ask";

/// Rozpisuje bieg jednokrokowy z definicji agenta — **bez dotykania dysku**.
///
/// Ten sam [`Plan`], co przy pliku, i to jest cała treść zdania „to jest zwykły bieg": od
/// [`the_planned_run`] w dół — graf, katalog biegu, pula miejsc, dowód śmierci grupy, odbudowa
/// indeksu — nikt nie ma jak zapytać, skąd ten plan się wziął.
///
/// # Dlaczego wychodzi stąd PLIK, którego nikt nie zapisał
///
/// [`Plan::graph`] jest migawką grafu **jak biegł** i ląduje w `run.json`, skąd czyta ją
/// odbudowa indeksu i historia. Migawka w innym kształcie niż każda inna byłaby drugim
/// kształtem tej samej odpowiedzi, więc bieg z jednym agentem opisuje się dokładnie tak, jak
/// opisałby się plik z jednym kafelkiem. Na dysk ten plik nie idzie i **nie ma nazwy**:
/// [`Plan::workflow_id`] niesie identyfikator AGENTA, bo zmyślona nazwa pliku byłaby czymś,
/// czego wznowienie szukałoby kiedyś w bibliotece workflow — a nikt jej tam nigdy nie zapisał.
///
/// # Czego tu świadomie nie ma
///
/// **Walidatora.** `check_to_run` sądzi plik, któremu nie ufamy (T3 §5.2, plik mógł zostać
/// zmergowany gitem między zapisem a Startem). Ten plan powstał przed chwilą tutaj i jedyną
/// rzeczą, którą przyniósł człowiek, jest identyfikator agenta i zdanie — pierwsze sprawdza
/// [`find_agent`], a drugie nie ma czego łamać. Sądzenie własnej konstrukcji dałoby odmowę,
/// której nie da się naprawić z drugiej strony granicy.
fn ask_workflow(saved: &Agent, ask: &AskRequest, title: &str) -> WorkflowFile {
    WorkflowFile {
        format: crate::workflow::file::CURRENT,
        id: saved.id.to_string(),
        name: title.to_owned(),
        description: None,
        steps: vec![Step::Agent(AgentStep {
            /* KLUCZEM KAFELKA JEST IDENTYFIKATOR AGENTA, i to nie jest ozdoba. Okno rozpoznaje
             * po nim swój wiersz w pasku (`Line::StepState { step_id }` → `withStepStates`),
             * a jedyną rzeczą, którą okno o tym biegu wie na pewno, jest to, o KOGO poprosiło:
             * uuid kroku powstaje tutaj i nikt go po tamtej stronie nigdy nie widział, więc
             * pasek stałby na „waiting" do końca biegu. */
            id: saved.id.to_string(),
            /* Nazwa agenta jest etykietą wiersza — i tą samą nazwą, którą trzeba WPISAĆ, żeby
             * powiedzieć mu coś w trakcie (`RunControl::step_can_hear`). */
            name: saved.name.clone(),
            agent: saved.id.to_string(),
            /* Nic do nadpisania: nadpisania są różnicą między definicją agenta a tym, czego
             * chce od niego JEDEN kafelek, a tu kafelka nie ma. Agent biegnie taki, jaki jest
             * zapisany — i dlatego wiersz wejścia nie ma czym skłamać o jego ustawieniach. */
            criteria: Vec::new(),
            overrides: Map::new(),
            vendor_options: BTreeMap::new(),
            copies: 1,
            weight: crate::workflow::Weight::Ordinary,
            /* ZDANIE CZŁOWIEKA JEST INSTRUKCJĄ TEGO KROKU, więc ląduje w migawce na dysku:
             * bieg, po którym nie da się powiedzieć, o co go poproszono, jest biegiem, którego
             * nie da się potem wyjaśnić (niezmiennik 4). */
            instructions: ask.task.clone(),
            skills: Skills::default(),
            /* NIC NIE POŻYCZAMY, i to jest ta sama odpowiedź, co przy `overrides` wyżej: wybór
             * „weź to z tego repozytorium" jest własnością kafelka, a `/ask` kafelka nie ma.
             * Domyślne, czyli puste — pełne `.claude/` w folderze, który ktoś otworzył, nie
             * jest zgodą. */
            borrow: Borrow::default(),
            /* FOLDER PRACY, nie własna kopia. `/ask` jest najczęstszą czynnością dnia, a własna
             * kopia znaczy gałąź i drzewo robocze na każde zdanie — czyli cenę, którą płaci się
             * za ochronę przed kolizją, której przy jednym kroku nie ma z czym mieć
             * (niezmiennik 12 mówi o DWÓCH krokach). */
            folder: Folder::Project,
            handover: Handover::default(),
            /* `/ask` to jeden kafelek i ani jednej strzałki: nie ma stożka, który mógłby zginąć,
             * ani następnego kroku, do którego można by cokolwiek przepuścić. */
            when_it_fails: crate::workflow::WhenItFails::Stop,
            at: Point::default(),
            extra: Map::new(),
        })],
        links: Vec::new(),
        extra: Map::new(),
    }
}

fn plan_ask(deps: &RunDeps<'_>, ask: &AskRequest) -> Result<Plan, RunError> {
    let library = deps.home.join(AGENTS_DIR);
    /* ODMOWA PRZED PIERWSZYM KATALOGIEM — kolejność z `ARCHITECTURE` §4, ta sama, co przy
     * biegu z pliku. Bieg, który najpierw zakłada `runs/<ts>__<id>/`, a odmawia potem,
     * zostawia w historii ślad biegu, którego nie było (niezmiennik 4), i robi to w chwili,
     * w której człowiek pomylił się w jednym słowie.
     *
     * TA SAMA funkcja, co przy kroku z pliku — więc i to samo zdanie o agencie, którego nie
     * ma. Druga odpowiedź na pytanie „kogo nazywa ten identyfikator" rozjechałaby się przy
     * pierwszej zmianie którejkolwiek z nich (niezmiennik 13). */
    let saved = find_agent(&library, &ask.agent, THE_ASK)?;

    let id = Uuid::now_v7().to_string();
    let created_at = now_ms();
    let dir = run_directory(deps.project, &id, created_at);
    /* TYTUŁ W HISTORII TO TO, O CO POPROSZONO, w jednym wierszu — bo tym jeden bieg `/ask`
     * różni się od drugiego. Bez zdania zostaje nazwa agenta: bieg musi dać się rozpoznać na
     * liście także wtedy, gdy nikt nie kazał nic ponad „ruszaj". */
    let title = one_line(&ask.task, TITLE_LIMIT).unwrap_or_else(|| saved.name.clone());

    let file = ask_workflow(&saved, ask, &title);

    /* TEN SAM ROZWIJACZ, CO PRZY PLIKU, choć rozwijać tu nie ma czego: bieg z `/ask` ma jeden
     * kafelek i ani jednej strzałki. Graf policzony tą samą funkcją, a nie wpisany z ręki, bo
     * druga odpowiedź na pytanie „jak wygląda graf tego biegu" rozjechałaby się przy pierwszej
     * zmianie tamtej (niezmiennik 13) — a od tego grafu zależy, gdzie kroki pracują. */
    let unrolled = crate::workflow::unroll::unroll(&file);
    let inputs = super::run_inputs::RunInputs::default();
    let folders = crate::workflow::unroll::folders::working_folders_at(
        &file,
        &unrolled,
        &inputs,
        deps.project,
        &dir.join(WORK_DIR),
    )
    .map_err(|why| RunError::Io(io::Error::other(why)))?;
    let setup = Setup {
        library,
        replay: None,
        wanted: &[],
        inputs,
        folders,
        connections: deps.home.join("connections"),
        data: deps.home,
        knows: what_the_agents_know(deps.home, deps.project),
        is_ask: true,
        /* PUSTE, bo zdanie człowieka jest już instrukcją tego kroku. Podane drugi raz jako
         * zadanie biegu dałoby prompt, w którym to samo polecenie stoi dwukrotnie — raz pod
         * nagłówkiem „o co poproszono" (`with_the_task`). */
        task: String::new(),
        project: deps.project,
        dir: &dir,
        drivers: &deps.drivers,
        file: &file,
        unrolled: &unrolled,
    };
    /* JEDNA DROGA PLANOWANIA KROKU, także za cenę drugiego przejścia po bibliotece: `plan_step`
     * woła [`find_agent`] jeszcze raz, dla identyfikatora, który właśnie się znalazł. Kilka
     * małych plików czytanych dwa razy jest tańsze niż druga kopia rozpisywania kroku — a to
     * ona trzyma politykę plików (`policy_of`), model, limit czasu i migawkę konfiguracji
     * efektywnej (niezmiennik 23). */
    let steps = file
        .steps
        .iter()
        .enumerate()
        // Runda zero i kopia zero: `/ask` jest jednym krokiem jednego agenta, więc nie ma tu ani
        // pętli, ani liczby „ile naraz" na kafelku — pyta się jednego agenta jeden raz.
        .map(|(node, step)| plan_step(step, node, 0, 0, None, &setup))
        .collect::<Result<Vec<Planned>, RunError>>()?;
    // Ten sam rachunek z pamięci, co przy biegu z pliku: bieg z `/ask` też dostaje blok „co
    // wiadomo", więc też ma po sobie zostawić ślad, co model wtedy wiedział.
    let memory = what_this_run_knew(&setup.knows, &steps, deps.home, deps.project);
    let memory_sources = freeze_memory_sources(&memory, &steps, deps.home, deps.project, true)?;
    /* Ten sam rachunek z umiejętności, co przy biegu z pliku — i tu zwykle pusty: krok `/ask`
     * powstaje z `Skills::default()`, więc nie ma po co sięgać. Bramki zamrożenia nie ma, bo bieg
     * jednokrokowy niczego nie wznawia (`seeded_from: None` niżej) i nie ma z czym porównywać. */
    let skills = what_this_run_froze(&steps)?;
    let graph = serde_json::to_value(&file)?;

    Ok(Plan {
        id,
        replay: None,
        dir,
        title,
        lead_origin: None,
        inputs: super::run_inputs::BoundInputs::default(),
        work_plan_sources: crate::workflow::work_plan::AuthorMap::default(),
        protection: BTreeMap::new(),
        input_snapshot: None,
        workspace_inputs: BTreeMap::new(),
        project_instructions: None,
        memory_sources,
        context_sources: None,
        additional_inputs: file
            .additional_inputs()
            .map_err(|said| RunError::Io(io::Error::other(said)))?,
        starting_results: BTreeMap::new(),
        workflow_id: file.id.clone(),
        /* ODCISK PLANU, nie pliku: „czy to był ten sam plan" ma dla biegu jednokrokowego jedną
         * odpowiedź — ten agent i to zdanie — i dokładnie tyle jest w tych bajtach. */
        hash: fingerprint(graph.to_string().as_bytes()),
        graph,
        /* Jeden krok nie ma po czym iść: strzałka w planie o jednym węźle byłaby krawędzią do
         * siebie, czyli tym, czego `Dag::new` odmawia. */
        arrows: Vec::new(),
        routes: Vec::new(),
        concurrency: ask.how_many_at_once,
        task: ask.task.clone(),
        loops: Vec::new(),
        seeded_from: None,
        // Bieg jednokrokowy nie wznawia niczego, więc nie ma po czym przejmować.
        carried: Vec::new(),
        project: deps.project.to_path_buf(),
        steps,
        memory,
        skills,
        created_at,
        trigger_origin: None,
        // Pytamy system RAZ, jak przy planie z pliku: ten bieg ma nosić jedną odpowiedź.
        boot_id: crate::engine::supervisor::machine_booted_at(),
        secrets: crate::connections::secrets::Carrier::in_library(Some(deps.home)),
        // Ta sama bramka, co przy planie z pliku: cennik nie do przeczytania jest odmową Startu,
        // także dla biegu z jednym kafelkiem (2026-09, Z-44).
        prices: the_prices_this_run_will_use(deps.home)?,
    })
}

/// Notatki, które człowiek dopuścił do użytku, jako blok tekstu na początek promptu.
///
/// 2026-08-18 — PO CO TO ISTNIEJE. `memory::notes::what_you_know` istniało od T-17 i miało
/// wołających **wyłącznie w trzech plikach testowych**. Prompt kroku brzmiał
/// `step.instructions.clone()` i nic poza tym, więc człowiek przestawiał notatkę na „in use",
/// a agent w kolejnym biegu nic o niej nie wiedział. Siedem zielonych kryteriów T-17 stało nad
/// martwym końcem: cała sekcja Pamięć była mechanizmem bez odbiorcy.
///
/// DWA ZAKRESY, KAŻDY ZE SWOIM BUDŻETEM. `Scope::Everywhere` idzie pierwszy, bo jest szerszym
/// tłem, a `Scope::ThisProject` po nim — bliższy kontekst czyta się na końcu, tuż przed samym
/// zadaniem. Każdy zakres ma własny sufit długości (`Scope::cap`), więc dwa wywołania nie są
/// obejściem budżetu: to jest budżet policzony tak, jak go zaprojektowano [T6 §5.3].
///
/// 2026-08-22 (T-80) — TRZECI ZAKRES WCHODZI, I WCHODZI PER KROK. Do tego dnia `Scope::ThisAgent`
/// nie docierał do nikogo: filtrowanie po agencie wymaga tożsamości agenta w chwili liczenia bloku,
/// a blok był liczony raz na bieg. Człowiek przestawiał notatkę agenta na „in use", widział ją na
/// ekranie i żaden krok nigdy się o niej nie dowiadywał — mechanizm istniał, ekran o nim mówił,
/// odbiorcy nie było (niezmiennik 29). Sufit `Scope::ThisAgent::cap()` = 800 stał w kodzie od T-17
/// i nigdy nikogo nie ograniczył.
///
/// Dlatego [`Known`] niesie też **zbiór notatek**, a nie sam gotowy tekst: trzeci blok składa
/// [`what_this_step_knows`] dla każdego kroku osobno, ale zawsze z TEGO SAMEGO, raz odczytanego
/// zbioru. Odczyt katalogu przy starcie kroku dałby dwóm krokom jednego biegu dwie różne
/// odpowiedzi na pytanie „co model o tym wiedział", gdyby ktoś w międzyczasie poprawił zdanie —
/// a różnicy nie widać nigdzie poza rachunkiem za długość.
///
/// **Odczyt, który się nie udał, nie zabiera biegu** (niezmiennik 5): katalog pamięci na świeżej
/// maszynie nie istnieje i to jest stan normalny. Wtedy agent po prostu nic nie wie.
struct Known {
    text: String,
    sources: Vec<ContextSource>,
    /// Rozstrzygnięcia wspólnych zakresów, zamrożone razem z tekstem promptu. UUID kroku
    /// dochodzi do nich dopiero po udanym `AgentDriver::start`.
    memory: Vec<MemoryDisposition>,
    /// Notatki odczytane RAZ, zanim ruszył pierwszy proces. Zamrożone: od tej chwili bieg ma
    /// jedną odpowiedź na pytanie, co wiedział.
    notes: Vec<crate::memory::notes::Note>,
    /// Dokładne bajty tych samych plików, odczytane w tej samej operacji co [`Known::notes`].
    /// Trzymane do stempla, żeby między promptem a zapisem nie było drugiego odczytu.
    snapshots: BTreeMap<PathBuf, String>,
}

fn empty_known() -> Known {
    Known {
        text: String::new(),
        sources: Vec::new(),
        memory: Vec::new(),
        notes: Vec::new(),
        snapshots: BTreeMap::new(),
    }
}

fn what_the_agents_know(home: &Path, project: &Path) -> Known {
    let library_root = super::memory::notes_root(home);
    let project_root = super::memory::project_notes_root(project);
    let mut notes = Vec::new();
    let mut snapshots = BTreeMap::new();

    // 2026-08-26 (T-128): miejsce i zakres wspólnie ograniczają zasięg. Biblioteczne legacy
    // `this-project` pozostaje widoczne w katalogu, ale nie jedzie do żadnego promptu przed
    // jawnym Move; źle położony szerszy zakres w projekcie także nigdy nie rozszerza zasięgu.
    match crate::memory::notes::scan_note_snapshots(&library_root) {
        Ok(found) => {
            for snapshot in found.into_iter().filter(|snapshot| {
                matches!(
                    snapshot.note.scope,
                    crate::memory::notes::Scope::Everywhere
                        | crate::memory::notes::Scope::ThisAgent
                )
            }) {
                snapshots.insert(snapshot.note.path.clone(), snapshot.raw);
                notes.push(snapshot.note);
            }
        }
        Err(error) => {
            tracing::debug!(root = %library_root.display(), %error, "the library notes could not be read");
        }
    }
    match crate::memory::notes::scan_note_snapshots(&project_root) {
        Ok(found) => {
            for snapshot in found
                .into_iter()
                .filter(|snapshot| snapshot.note.scope == crate::memory::notes::Scope::ThisProject)
            {
                snapshots.insert(snapshot.note.path.clone(), snapshot.raw);
                notes.push(snapshot.note);
            }
        }
        Err(error) => {
            tracing::debug!(root = %project_root.display(), %error, "the project notes could not be read");
        }
    }
    known_from_sources(notes, snapshots, home, project)
}

fn known_from_sources(
    notes: Vec<crate::memory::notes::Note>,
    snapshots: BTreeMap<PathBuf, String>,
    home: &Path,
    project: &Path,
) -> Known {
    let mut known = Known {
        notes,
        snapshots,
        ..empty_known()
    };
    for scope in [
        crate::memory::notes::Scope::Everywhere,
        crate::memory::notes::Scope::ThisProject,
    ] {
        add_block(
            &mut known.text,
            &mut known.sources,
            &mut known.memory,
            &known.notes,
            scope,
            home,
            project,
        );
    }
    known
}

fn frozen_known(
    snapshot: &super::memory_sources::Snapshot,
    home: &Path,
    project: &Path,
    node: Option<&str>,
) -> io::Result<Known> {
    let sources = snapshot.notes(home, project, node)?;
    let raw = sources
        .iter()
        .map(|(note, raw)| (note.path.clone(), raw.clone()))
        .collect();
    Ok(known_from_sources(
        sources.into_iter().map(|(note, _)| note).collect(),
        raw,
        home,
        project,
    ))
}

/// Dokleja blok jednego zakresu do tego, co już wiadomo — i dopisuje rachunek z niego.
///
/// Jedno miejsce na oba użycia (dwa zakresy wspólne dla biegu i trzeci, liczony per krok), bo
/// druga kopia tej pętli byłaby drugim miejscem, w którym mieszka odpowiedź na pytanie „jak
/// wygląda blok pamięci w promptcie" (niezmiennik 13). Budżet bierze się z zakresu, więc każdy
/// zakres liczy się przeciw WŁASNEMU sufitowi [T6 §5.3]: trzeci blok dolicza się do dwóch
/// pozostałych, a nie zamiast nich.
fn add_block(
    text: &mut String,
    sources: &mut Vec<ContextSource>,
    memory: &mut Vec<MemoryDisposition>,
    notes: &[crate::memory::notes::Note],
    scope: crate::memory::notes::Scope,
    home: &Path,
    project: &Path,
) {
    let block = crate::memory::notes::what_you_know(notes, crate::memory::notes::Budget::of(scope));
    if !block.text.is_empty() {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&block.text);
    }
    for id in &block.used {
        // `id` jest unikalne wyłącznie w jednym korzeniu. Zakres rozróżnia tu legalne bliźniaki:
        // biblioteczne `everywhere:same-address` i projektowe `this-project:same-address`
        // tworzą dwa różne źródła i tylko drugie odpowiada blokowi projektu.
        let Some(note) = notes
            .iter()
            .find(|note| note.scope == scope && &note.id == id)
        else {
            continue;
        };
        let Some(relative) = note_reference(&note.path, home, project) else {
            continue;
        };
        let Some(address) = note_address(note, home, project) else {
            continue;
        };
        sources.push(ContextSource {
            kind: ContextKind::MemoryNote,
            reference: relative,
            bytes: note.rule.len(),
        });
        memory.push(MemoryDisposition {
            address,
            delivered: true,
        });
    }
    for id in &block.dropped {
        let Some(note) = notes
            .iter()
            .find(|note| note.scope == scope && &note.id == id)
        else {
            continue;
        };
        let Some(address) = note_address(note, home, project) else {
            continue;
        };
        memory.push(MemoryDisposition {
            address,
            delivered: false,
        });
    }
}

fn note_reference(path: &Path, home: &Path, project: &Path) -> Option<String> {
    path.strip_prefix(home)
        .or_else(|_| path.strip_prefix(project))
        .ok()
        .map(|relative| relative.to_string_lossy().into_owned())
}

fn note_address(
    note: &crate::memory::notes::Note,
    home: &Path,
    project: &Path,
) -> Option<super::memory::NoteAddress> {
    let place = if note.path.strip_prefix(home).is_ok() {
        super::memory::NotePlace::Library
    } else if note.path.strip_prefix(project).is_ok() {
        super::memory::NotePlace::Project
    } else {
        return None;
    };
    Some(super::memory::NoteAddress {
        place,
        id: note.id.to_string(),
    })
}

#[derive(Debug, Clone)]
struct MemoryDisposition {
    address: super::memory::NoteAddress,
    delivered: bool,
}

/// Co wie krok TEGO agenta: dwa zakresy wspólne dla biegu plus jego własny, trzeci.
///
/// Zbiór notatek jest zamrożony w [`Known::notes`], więc ta funkcja niczego nie czyta z dysku —
/// dwa kroki jednego biegu dostają dwie różne odpowiedzi tylko wtedy, kiedy różnią się agentem.
///
/// TOŻSAMOŚĆ IDZIE PRZEZ [`crate::memory::slugify`], bo plik notatki pisze **człowiek**
/// (`agent: backend-dev`), a agent w bibliotece nazywa się `Backend Dev`. Identyfikator z
/// biblioteki w tym polu byłby wartością, której człowiek nie umie ani napisać, ani przeczytać
/// w edytorze (niezmiennik 4: plik jest prawdą). Ta sama normalizacja robi z tytułu notatki
/// nazwę jej pliku, więc nie ma tu drugiej odpowiedzi na pytanie „czy te dwie nazwy to jedna".
///
/// Blok agenta stoi NA KOŃCU, tuż nad zadaniem: od najszerszego tła do najbliższego kontekstu,
/// czyli tak, jak to czyta model.
fn what_this_step_knows(
    known: &Known,
    agent: &str,
    home: &Path,
    project: &Path,
) -> (String, Vec<ContextSource>, Vec<MemoryDisposition>) {
    let mut text = known.text.clone();
    let mut sources = known.sources.clone();
    let mut memory = known.memory.clone();

    let whose = crate::memory::slugify(agent);
    let mine: Vec<crate::memory::notes::Note> = known
        .notes
        .iter()
        .filter(|note| {
            note.agent
                .as_deref()
                .is_some_and(|owner| crate::memory::slugify(owner) == whose)
        })
        .cloned()
        .collect();
    add_block(
        &mut text,
        &mut sources,
        &mut memory,
        &mine,
        crate::memory::notes::Scope::ThisAgent,
        home,
        project,
    );

    (text, sources, memory)
}

/// Jedna notatka w zrzucie biegu: **czym była**, nie co mówiła.
///
/// 2026-08-22 (T-80). `run.json` jest prawdą o biegu (niezmiennik 4), więc notatka, która
/// pojechała w promptcie i nie zostawiła tu śladu, jest faktem o biegu, którego nikt później
/// nie odtworzy. Trzy pola to dokładnie tyle, ile trzeba, żeby odpowiedzieć na pytanie „co model
/// wtedy wiedział": sama nazwa odpowiada „jakaś notatka o tej nazwie", a ta zmieniła się od
/// tamtej pory dokładnie tak, jak zmienia się w trakcie biegu.
///
/// Kopii treści tu nie ma i nie będzie: `run.json` jest rachunkiem z pamięci, nie jej kopią.
#[derive(Debug, Clone, Serialize)]
struct MemoryRecord {
    /// Która notatka — ścieżka **względem korzenia danych**. Absolutna byłaby faktem o tym
    /// laptopie, nie o biegu.
    reference: String,
    /// Odcisk zdania, które pojechało do modelu. Liczony z `rule`, bo `rule` jest jedyną częścią
    /// notatki, która tam jedzie — odcisk całego pliku zmieniałby się od poprawki w `because`,
    /// czyli mówiłby „model wiedział co innego" o biegu, w którym model dostał to samo.
    hash: String,
    /// Ile bajtów miało to zdanie w chwili startu. Ta liczba i odcisk odpowiadają na to samo
    /// pytanie dwiema drogami, więc zrzut przepisany po biegu rozjeżdża się z sobą samym.
    bytes: usize,
    /// Pełny adres z T-131. Sam `id` nie rozróżnia legalnych bliźniaków w obu korzeniach.
    address: super::memory::NoteAddress,
    /// Pochodzenie jest typowane polem, nigdy rozpoznawane po kształcie wartości.
    project: Option<String>,
    from: Option<String>,
    /// Fizyczne UUID kroków dopisane dopiero po udanym `AgentDriver::start`.
    recipients: Vec<String>,
    #[serde(rename = "leftOutFor")]
    left_out_for: Vec<String>,
    /// Czy którakolwiek planowana ścieżka naprawdę miała tę notatkę w promptcie. Tylko żywy
    /// szczegół stempla; receipt rozróżnia to później przez listy odbiorców.
    #[serde(skip)]
    carried: bool,
    /// Fizyczny plik wyłącznie na czas żywego biegu; absolutna ścieżka nie trafia do receipt.
    #[serde(skip)]
    path: PathBuf,
    /// Surowa migawka tego pliku, również tylko do stempla po starcie.
    #[serde(skip)]
    snapshot: String,
}

/// Co ten bieg wiedział, kiedy ruszał — z rachunku KROKÓW, nie z katalogu notatek.
///
/// Liczone z tego, co naprawdę wjechało w prompty (`ContextKind::MemoryNote`), a nie z całego
/// zamrożonego zbioru: notatka, która nie zmieściła się w suficie swojego zakresu, nie dojechała
/// do modelu i nie ma prawa stać w rachunku tak, jakby dojechała. Notatka, która pojawiła się
/// w katalogu po starcie, nie jest tu w ogóle — zbiór jest zamrożony przed pierwszym procesem.
fn what_this_run_knew(
    known: &Known,
    steps: &[Planned],
    home: &Path,
    project: &Path,
) -> Vec<MemoryRecord> {
    let dispositions: Vec<&MemoryDisposition> = steps
        .iter()
        .filter_map(|step| match &step.job {
            Job::Agent(job) => Some(job),
            /* `Serve` dokłada się do tego ramienia, a nie dostaje własnego: `ServeJob` niesie
             * `command`, `cwd` i `ours` — ani promptu, ani kontekstu. Krok „uruchom i zostaw"
             * nie wwozi do modelu żadnej notatki, więc nie ma czego policzyć. Osobne ramię
             * z tym samym ciałem paliłoby `match_same_arms`. */
            Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => None,
        })
        .flat_map(|job| job.memory.iter())
        .collect();

    let relevant: BTreeSet<&super::memory::NoteAddress> =
        dispositions.iter().map(|one| &one.address).collect();
    let carried: BTreeSet<&super::memory::NoteAddress> = dispositions
        .iter()
        .filter(|one| one.delivered)
        .map(|one| &one.address)
        .collect();

    let mut records: Vec<MemoryRecord> = known
        .notes
        .iter()
        .filter_map(|note| {
            let address = note_address(note, home, project)?;
            let reference = note_reference(&note.path, home, project)?;
            let snapshot = known.snapshots.get(&note.path)?;
            relevant.contains(&address).then(|| MemoryRecord {
                hash: fingerprint(note.rule.as_bytes()),
                bytes: note.rule.len(),
                reference,
                carried: carried.contains(&address),
                address,
                project: note.project.clone(),
                from: note.from.clone(),
                recipients: Vec::new(),
                left_out_for: Vec::new(),
                path: note.path.clone(),
                snapshot: snapshot.clone(),
            })
        })
        .collect();
    records.sort_by(|left, right| left.address.cmp(&right.address));
    records
}

/// Źródło pochodzi z tego samego odczytu co prompt; mapa nie rozszerza zasięgu pomiędzy
/// kopiami. Notatki odrzucone przez budżet pozostają w receipt, ale nie w paczce wejściowej.
fn freeze_memory_sources(
    memory: &[MemoryRecord],
    steps: &[Planned],
    home: &Path,
    project: &Path,
    verify_live: bool,
) -> io::Result<super::memory_sources::Snapshot> {
    let nodes = steps
        .iter()
        .filter_map(|step| match &step.job {
            Job::Agent(job) => Some((
                step.node_key.clone(),
                job.memory
                    .iter()
                    .filter(|one| one.delivered)
                    .map(|one| one.address.clone())
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    let sources = memory
        .iter()
        .filter(|record| record.carried)
        .map(|record| super::memory_sources::Source {
            address: record.address.clone(),
            raw: record.snapshot.clone(),
        })
        .collect();
    let snapshot = super::memory_sources::Snapshot::new(sources, nodes)?;
    if verify_live {
        snapshot.verify_live_sources(home, project)?;
    }
    Ok(snapshot)
}

/// Jedna umiejętność w zrzucie biegu: **czym była**, nie co mówiła.
///
/// 2026-08-28 (T-154). Lustro [`MemoryRecord`] obok, z tego samego powodu: `run.json` jest prawdą
/// o biegu (niezmiennik 4), a umiejętność, która pojechała do agenta i nie zostawiła tu śladu,
/// jest faktem o biegu, którego nikt później nie odtworzy. Sama nazwa odpowiada „jakiś materiał
/// o tej nazwie", a materiał zmienia się między biegami tak samo, jak zmienia się notatka.
///
/// Kopii treści tu nie ma i nie będzie — to jest rachunek z biblioteki, nie jej kopia. Czytany
/// z powrotem przez [`the_frozen_skills_are_still_here`], więc i `Deserialize`: druga, wąska
/// struktura na ten sam klucz byłaby dwoma kształtami jednej odpowiedzi (niezmiennik 13).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkillRecord {
    /// Która umiejętność — nazwa z definicji agenta, czyli ta, którą człowiek widzi na ekranie.
    name: String,
    /// Odcisk CAŁEGO katalogu, którym ta umiejętność pojechała do kroku.
    hash: String,
    /// Ile bajtów miał ten katalog. Ta liczba i odcisk odpowiadają na to samo pytanie dwiema
    /// drogami, więc zrzut przepisany po biegu rozjeżdża się z sobą samym.
    ///
    /// `default`, bo czyta to także biegi zapisane przed 2026-08-28 — a brak pola ma znaczyć
    /// „nie policzono", nie „nie da się odczytać" (niezmiennik 5).
    #[serde(default)]
    bytes: u64,
    /// Klucze KAFELKÓW, które ją dostały. Kafelka, nie węzła: rundy pętli dzielą jeden zbiór
    /// umiejętności i to kafelek jest tym, co człowiek naciska, żeby powtórzyć krok.
    #[serde(default)]
    steps: Vec<String>,
}

/// Po jaki materiał ten bieg sięgnął — z rachunku KROKÓW, nie z katalogu biblioteki.
///
/// 2026-08-28 (T-154). Liczone z tego, co naprawdę dojechało do kroków ([`StepSkills`]), a nie
/// z całej biblioteki: umiejętność, po którą nie sięgnął ani jeden krok, nie ma prawa stać
/// w rachunku tak, jakby sięgnął. Ten sam wybór i to samo zdanie stoi przy [`what_this_run_knew`].
///
/// Po jednej pozycji na NAZWĘ, nie na krok: kanoniczny katalog wyznacza sama nazwa
/// (`<dane>/skills/<nazwa>/`, `StepSkills::for_the_step`), więc trzy kroki z tą samą umiejętnością
/// dostają ten sam materiał i trzy odciski byłyby trzema kopiami jednej odpowiedzi.
fn what_this_run_froze(steps: &[Planned]) -> Result<Vec<SkillRecord>, RunError> {
    let mut records: Vec<SkillRecord> = Vec::new();
    for step in steps {
        /* Tylko krok agenta ma po co sięgać: ani kafelek kontrolny, ani „sprawdź", ani „uruchom
         * i zostaw" nie wołają vendora, więc nie ma czego położyć na ich półce. */
        let Job::Agent(job) = &step.job else {
            continue;
        };
        for (name, dir) in job.skills.names.iter().zip(&job.skills.dirs) {
            if let Some(record) = records.iter_mut().find(|one| &one.name == name) {
                if !record.steps.contains(&step.tile_key) {
                    record.steps.push(step.tile_key.clone());
                }
                continue;
            }
            let material = material_of(dir)?;
            records.push(SkillRecord {
                name: name.clone(),
                hash: material.hash,
                bytes: material.bytes,
                steps: vec![step.tile_key.clone()],
            });
        }
    }
    // Po nazwie, nie w kolejności napotkania: ten sam bieg ma dawać ten sam `run.json`, a
    // kolejność kroków w wycinku zależy od tego, o który wycinek poproszono.
    records.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(records)
}

/// Czy materiał, który TAMTEN bieg zamroził, jest jeszcze tym samym — **przed pierwszym procesem**.
///
/// # 2026-08-28 (T-154) — po co to istnieje
///
/// Powtórzenie kroku jest pytaniem „czy MOJA poprawka zmieniła wynik", a odpowiedź nie znaczy nic,
/// jeżeli w międzyczasie przesunęło się też wejście. Do tego dnia obie ciche wersje tej wady były
/// możliwe i żadna nie zostawiała śladu: umiejętność zdjęta z agenta dawała powtórzenie
/// z MNIEJSZYM materiałem (`StepSkills::for_the_step` odmawia dopiero wtedy, gdy nazwa JEST,
/// a katalogu nie ma), a poprawiony `SKILL.md` dawał powtórzenie z INNYM — i jedno, i drugie
/// kończyło się `Succeeded`.
///
/// `seeded_from` jest `Some` wyłącznie dla powtórzenia i wznowienia (`commands::rerun`), więc
/// zwykły Start przechodzi tędy bez ani jednego odczytu z dysku. Bieg, który tamtego rachunku
/// nie ma — bo powstał przed tą zmianą albo jego `run.json` jest nieczytelny — nie jest odmową:
/// „nie wiem, czym to było" i „to jest co innego" to dwa różne stany, a odmowa na pierwszym
/// z nich zabierałaby powtórzenie każdemu staremu biegowi (niezmiennik 5).
///
/// SĄDZIMY WYŁĄCZNIE KAFELKI, KTÓRE W TYM BIEGU RUSZAJĄ. Powtórzenie jednego kroku to
/// `Part::Just`, czyli wycinek o jednym kafelku — odmowa za materiał kafelka, który nie ma
/// pobiec, byłaby odmową o czymś, czego ten bieg nie dotknie.
fn the_frozen_skills_are_still_here(
    seeded_from: Option<&Path>,
    steps: &[Planned],
) -> Result<(), RunError> {
    let Some(before) = seeded_from else {
        return Ok(());
    };
    let froze = what_that_run_froze(before);
    for step in steps {
        let Job::Agent(job) = &step.job else {
            continue;
        };
        for record in froze.iter().filter(|record| {
            record
                .steps
                .iter()
                .any(|tile_key| tile_key == &step.tile_key)
        }) {
            let why = match job
                .skills
                .names
                .iter()
                .zip(&job.skills.dirs)
                .find(|(name, _)| *name == &record.name)
            {
                // Nazwy nie ma już w zbiorze tego kroku: człowiek zdjął ją z agenta albo z kroku.
                // Cicha alternatywa — powtórzenie z mniejszym materiałem — jest z zewnątrz nie do
                // odróżnienia od „model nie uznał, że warto po nią sięgnąć".
                None => Moved::Gone,
                Some((_, dir)) => {
                    let now = material_of(dir)?;
                    if now.hash == record.hash {
                        continue;
                    }
                    Moved::Changed
                }
            };
            return Err(RunError::Refused(Note {
                level: Level::Problem,
                // Kropka ląduje na KAFELKU, tak jak przy każdej innej odmowie tej ścieżki
                // ([`refused_by_the_skills`]).
                step_id: Some(step.tile_key.clone()),
                /* Zdanie co do słowa to, które napisał `skills`. Przepisane tutaj byłoby drugą
                 * kopią jednego zdania, a druga kopia jest zawsze tą nieaktualną (niezmiennik 23). */
                message: NotAsItWas {
                    step: step.name.clone(),
                    skill: record.name.clone(),
                    why,
                }
                .to_string(),
                fix: None,
            }));
        }
    }
    Ok(())
}

/// Rachunek z umiejętności zapisany w `run.json` tamtego biegu — pusty, kiedy go tam nie ma.
///
/// Brak pliku, plik nieczytelny i plik bez tego klucza dają jeden wynik: pusty wektor, czyli „nie
/// ma czego pilnować". Ten sam wybór w bezpieczną stronę, co przy `skills::place::recorded`
/// i z tego samego powodu — biegów sprzed 2026-08-28 nie da się w ten sposób osądzić, a odmowa
/// wystawiona z niewiedzy zabiera człowiekowi ruch, którego nic nie zastępuje.
fn what_that_run_froze(run_dir: &Path) -> Vec<SkillRecord> {
    /// `run.json` w jednym polu, które to pytanie potrzebuje. Wąski kształt, nie pełne lustro
    /// [`RunFile`]: tamten plik rośnie z każdym zadaniem, a przewrócenie się na polu, którego
    /// nie znamy, byłoby odmową o cudzej zmianie (niezmiennik 5). Ten sam wybór, co przy
    /// `commands::rerun::Finished`.
    #[derive(Debug, Deserialize)]
    struct Frozen {
        #[serde(default)]
        skills: Vec<SkillRecord>,
    }

    fs::read(run_dir.join(RUN_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Frozen>(&bytes).ok())
        .map(|one| one.skills)
        .unwrap_or_default()
}

/// Zadanie kroku, poprzedzone tym, co wiadomo.
///
/// Pusty blok znaczy „nic nie wiadomo" i wtedy prompt jest DOKŁADNIE zadaniem kroku, bez ani
/// jednego dodatkowego bajtu: nagłówek nad pustką uczy model, że ta sekcja bywa pusta,
/// i kosztuje długość za nic (ten sam powód stoi przy `Block::text`).
fn with_what_we_know(knows: &str, task: &str) -> String {
    if knows.is_empty() {
        return task.to_owned();
    }
    format!("{knows}\n\n{task}")
}

/// Znacznik, którym plik workflow wskazuje, GDZIE w promptcie kroku ma stanąć zadanie człowieka.
///
/// Ta sama rodzina, co [`COPY_MARK`] i [`COPIES_MARK`] [T3 §4.3] — plik już umie mówić
/// o rzeczach, które powstają dopiero przy starcie.
const TASK_MARK: &str = "{{task}}";

/// Znacznik, w który wchodzi numer TEJ kopii, licząc od jedynki.
const COPY_MARK: &str = "{{copy}}";

/// I znacznik, w który wchodzi, ile ich jest razem.
const COPIES_MARK: &str = "{{copies}}";

/// Instrukcja kroku z podstawionymi numerami kopii [T3 §4.3].
///
/// 2026-08-23 (T-90) — DO TEGO DNIA NIE PODSTAWIAŁ ICH NIKT, w żadnym miejscu drzewa: człowiek
/// pisał `{{copy}}` w instrukcji i agent dostawał te dwa nawiasy dosłownie.
///
/// Licząc od jedynki, jak [`name_for`] i z tego samego powodu: to jest liczba dla agenta, a „copy
/// 0 of 3" nie znaczy dla niego nic.
///
/// Podstawiamy TAKŻE przy jednej kopii, i to jest odpowiedź, nie przeoczenie: krok biegnący raz
/// jest kopią 1 z 1, a znacznik zostawiony nietknięty byłby tekstem, który nikt nigdy nie
/// zamienił na liczbę — czyli kontrolką bez skutku schowaną w prompcie.
///
/// `{{copies}}` idzie pierwsze wyłącznie dla porządku czytania: dłuższy znacznik nie zawiera
/// krótszego (`{{copy}}` wymaga `}}` zaraz po `copy`), więc kolejność nie zmienia wyniku.
fn numbered(text: &str, copy: u8, copies: u32) -> String {
    text.replace(COPIES_MARK, &copies.to_string())
        .replace(COPY_MARK, &(u32::from(copy) + 1).to_string())
}

/// Nagłówek nad zadaniem, kiedy plik nie wskazał miejsca sam.
///
/// Zdanie, nie słowo: krok czyta to razem ze swoim promptem, więc musi wiedzieć, czyje to jest
/// polecenie i że dotyczy całego biegu, a nie tylko jego jednego.
const TASK_HEADING: &str = "What the person asked for, for this whole run:";

/// Zadanie kroku z wpisanym zadaniem CAŁEGO biegu — albo bez, kiedy nikt go nie podał.
///
/// # Dwa sposoby, jeden powód
///
/// Jeśli prompt kroku zawiera [`TASK_MARK`], zadanie ląduje **dokładnie tam** — bo plik, który
/// zadał sobie trud wskazania miejsca, wie o swoim promptcie więcej niż my. Jeśli nie zawiera,
/// zadanie idzie na GÓRĘ, pod nagłówkiem. Nie na dół: prompt kroku kończy się zwykle instrukcją
/// „co oddać", a zdanie doklejone po niej czyta się jak dopisek po podpisie.
///
/// Przy pustym zadaniu prompt NIE dostaje ani nagłówka, ani jednego dodatkowego bajtu — a sam
/// znacznik **znika**. To jedyne miejsce, w którym ta funkcja zmienia prompt bez zadania, i ma
/// nazwany powód: `{{task}}` zostawiony w tekście jest jedyną rzeczą w całym promptcie, której
/// model nie umie przeczytać inaczej niż jako literalny nawias — czyli wygląda jak zepsute
/// podstawienie, którym jest.
fn with_the_task(task: &str, instructions: &str) -> String {
    if task.is_empty() {
        return instructions.replace(TASK_MARK, "");
    }
    if instructions.contains(TASK_MARK) {
        return instructions.replace(TASK_MARK, task);
    }
    format!("{TASK_HEADING}\n{task}\n\n{instructions}")
}

/// Wobec czego planujemy krok: gdzie leży biblioteka, gdzie projekt, gdzie katalog tego biegu
/// i skąd biorą się sterowniki.
struct Setup<'a> {
    replay: Option<&'a super::replay::ReplayMaterial>,
    inputs: super::run_inputs::RunInputs,
    /// WF-17: te same aliasy katalogów dla wykonania i limitu kopii przed Startem.
    folders: crate::workflow::unroll::folders::CopyPlan,
    /// WF-16 (2026-09-06): powtórzenie wycinka musi utworzyć wspólną kopię także wtedy,
    /// gdy pierwotny właściciel nie wykonuje się ponownie. Adres kopii pozostaje ten sam.
    wanted: &'a [bool],
    /// `~/.loadout/agents` — stąd bierzemy agenta, którego nazywa krok.
    library: PathBuf,
    /// `~/.loadout/connections` — wyłącznie natywne, jawnie zatwierdzone pliki.
    connections: PathBuf,
    /// `~/.loadout` — korzeń danych aplikacji, pod którym leżą kanoniczne kopie umiejętności
    /// (`skills/<nazwa>/`). Ten sam korzeń, który przy instalacji wskazuje `skills::Roots::data`.
    ///
    /// Pytamy **biblioteki**, a nie katalogów vendorów: te bywają cudze (człowiek mógł napisać
    /// tam własną umiejętność ręcznie), a bieg ma podać agentowi wyłącznie to, co Loadout
    /// naprawdę posiada.
    data: &'a Path,
    /// Co agent WIE, zanim przeczyta swoje zadanie — notatki, które człowiek dopuścił do użytku.
    ///
    /// **Czytane** RAZ, przy planowaniu, nie przy każdym kroku: ten sam bieg ma nieść jedną
    /// odpowiedź na pytanie „co wiadomo". Odczyt per krok dałby dwóm krokom tego samego biegu
    /// dwa różne konteksty, gdyby ktoś w międzyczasie dopuścił notatkę — a różnicy nie widać
    /// nigdzie poza rachunkiem za długość.
    ///
    /// Per krok SKŁADANY jest wyłącznie trzeci blok ([`what_this_step_knows`]), i to z tego
    /// samego, zamrożonego zbioru: pamięć jednego agenta nie ma jak być własnością biegu, bo
    /// dwa kroki biegu bywają dwoma różnymi agentami (2026-08-22, T-80).
    knows: Known,
    /// `/ask` ma jedno źródło `RunTask`; zwykły workflow ma osobne zadanie biegu i instrukcję.
    is_ask: bool,
    /// Co człowiek kazał zbudować TYM biegiem — puste, kiedy nie kazał nic ponad plik.
    ///
    /// Jedna wartość na bieg, dokładnie jak [`Setup::knows`] i z tego samego powodu: zadanie
    /// odczytane per krok mogłoby się różnić między krokami jednego biegu, a wtedy „co my właściwie
    /// budujemy" przestaje mieć jedną odpowiedź.
    task: String,
    /// Katalog projektu, w którym biegnie workflow.
    project: &'a Path,
    /// Katalog tego biegu. Jeszcze nie istnieje: pod nim lądują własne kopie plików.
    dir: &'a Path,
    /// Fabryka sterowników z [`RunDeps`].
    drivers: &'a super::Drivers,
    /// Plik, z którego bierze się ten bieg. Kroki czyta się z niego po numerze węzła.
    file: &'a WorkflowFile,
    /// Graf po rozwinięciu pętli — węzły i strzałki po numerach.
    ///
    /// 2026-08-20 (T-56) — WCHODZI TU, BO JEDEN WARIANT FOLDERU JEST ZDANIEM O GRAFIE.
    /// `Folder::SameCopy` znaczy „to samo drzewo, w którym pracował krok przede mną", a „przede
    /// mną" nie jest własnością kroku: niesie ją strzałka. Graf stoi w [`Setup`], a nie leci
    /// argumentem obok, z tego samego powodu, co [`Setup::knows`] — jest jeden na bieg,
    /// a policzony drugi raz per krok mógłby się między krokami różnić.
    unrolled: &'a crate::workflow::unroll::Unrolled,
}

/// Klucz węzła: klucz katalogu roboczego, a dla dalszych rund pętli ten sam z numerem rundy.
///
/// Runda zerowa NIE dostaje sufiksu, i to jest decyzja o wsteczności: plik bez pętli daje wtedy
/// dokładnie te klucze, które dawał przedtem, więc `run.json` starych biegów i nowych da się
/// porównać, a nikt, kto o pętli nie słyszał, nie widzi zmiany.
///
/// ZBUDOWANY NA [`crate::workflow::check::work_key_for`], a nie sklejony obok niego: dwa klucze różnią się dokładnie
/// jednym sufiksem i mają się nie rozjechać w dniu, w którym ktoś poprawi jeden z nich.
///
/// Klucze MUSZĄ się różnić między kopiami i między rundami, bo indeks biegu ma na nich
/// `UNIQUE (run_id, node_key)` (`store::schema`): dwa węzły o jednym kluczu to bieg, który zapisze
/// jeden i zgubi drugi — **po** zapłaceniu za oba (niezmiennik 4).
pub(crate) fn node_key_for(tile_key: &str, turn: u8, copy: u8) -> String {
    crate::workflow::check::node_key_for(tile_key, turn, copy)
}

/// Klucz katalogu pracy z klucza węzła — bez sufiksu rundy, ale z tożsamością kopii.
///
/// 2026-08-24 (T-114) — gałąź i źródło wznowienia muszą pytać o tę samą kopię, którą nazywa
/// katalog `work/`. Obcięcie także `~N` sprowadzałoby wszystkie kopie do gałęzi pierwszej.
pub(super) fn work_key_of(node_key: &str) -> &str {
    node_key.find('#').map_or(node_key, |at| &node_key[..at])
}

/// Numer rundy z klucza węzła — druga odwrotność [`node_key_for`], obok [`tile_key_of`].
///
/// **Tutaj, a nie u wołającego**, i to jest ta sama racja, co przy [`tile_key_of`] niżej: sufiks
/// rundy (`#N`) jest kształtem wymyślonym o kilkanaście linii wyżej, więc jego rozbieranie
/// gdziekolwiek indziej byłoby drugą definicją tego samego faktu (niezmiennik 13).
///
/// 2026-08-29 — POWSTAŁO DLA ZNACZNIKA POCHODZENIA KOPII ([`fan_in::Generation`]). Rozłączne
/// katalogi mówią „to nie jest ta sama kopia" i nic ponadto; pytanie „którą rundę ta kopia
/// w sobie ma" ma odpowiedź wyłącznie w kluczu węzła, bo rundy dzielą folder ([`work_key_for`]).
///
/// Brak sufiksu to runda zerowa, a nie brak odpowiedzi: klucz bez `#` należy do węzła, który
/// biegnie raz. Sufiks, którego nie da się przeczytać jako liczby, też liczy się jako zerowa —
/// panika w silniku zabiera cały bieg (`AGENTS.md` §4), a taki klucz jest kształtem, którego
/// [`node_key_for`] nie produkuje.
fn tries_in(node_key: &str) -> u8 {
    node_key
        .split_once('#')
        .and_then(|(_, turn)| turn.parse().ok())
        .unwrap_or(0)
}

/// Klucz kafelka z klucza węzła — odwrotność [`node_key_for`].
///
/// **Tutaj, a nie u wołającego**, i to jest jedyny powód, dla którego ta funkcja istnieje:
/// sufiks rundy (`#N`) i sufiks kopii (`~N`) są kształtami wymyślonymi o kilka linii wyżej, więc
/// ich rozbieranie gdziekolwiek indziej byłoby drugą definicją tego samego faktu (niezmiennik 13).
/// Historia czyta `run.json` i musi wiedzieć, o KTÓRY kafelek chodzi, żeby dało się od niego
/// wznowić.
///
/// 2026-08-23 — POWSTAŁO Z DEFEKTU ZE ZRZUTU WŁAŚCICIELA: „Pick up here" podawał `id` kroku
/// z `run.json`, czyli UUID nadany przy planowaniu, a wznowienie szuka po kluczu Z PLIKU. Skutek
/// był zdaniem-zagadką: *„01a02b3c-… is not a step in that workflow any more"* — o kroku, który
/// stoi na płótnie i nigdzie się nie ruszył.
pub(crate) fn tile_key_of(node_key: &str) -> &str {
    node_key
        .find(['#', '~'])
        .map_or(node_key, |at| &node_key[..at])
}

/// Podpis kopii: nazwa z kafelka plus „(2 of 3)".
///
/// 2026-08-23 (T-90) — KAŻDA KOPIA MÓWI, KTÓRA JEST, i to jest warunek czytelności ekranu, nie
/// ozdoba: trzy wiersze pracy pod jedną nazwą to trzy wiersze, których człowiek nie umie
/// rozróżnić. Ten sam podpis stoi w `run.json` i w indeksie przekazań następnego kroku, więc
/// krok scalający wie, którą z trzech odpowiedzi właśnie czyta.
///
/// Licząc od jedynki, nie od zera: `copy` jest polem danych, a to jest zdanie dla człowieka
/// i dla agenta — „copy 0 of 3" nie znaczy nic ani dla jednego, ani dla drugiego.
///
/// Krok biegnący raz zostaje pod swoją nazwą, co do bajtu: „(1 of 1)" byłoby dopiskiem na
/// każdym kafelku każdego workflow na dysku.
fn name_for(name: &str, copy: u8, copies: u32) -> String {
    if copies <= 1 {
        return name.to_owned();
    }
    format!("{name} ({} of {copies})", u32::from(copy) + 1)
}

/// Jeden węzeł rozwiniętego grafu → jeden krok planu.
///
/// `node` jest numerem tego węzła w [`Setup::unrolled`], a nie pozycją kroku w pliku: rundy pętli
/// mają wspólny krok i różne węzły, a „krok przede mną" jest pytaniem o węzeł.
/// `copy` jest numerem kopii tego węzła — zero dla kroku biegnącego raz. Kopie ma wyłącznie krok
/// agenta (`unroll::copies_of`), więc pozostałe trzy ramiona dostają tu zawsze zero i ich klucze
/// nie zmieniają się o bajt.
fn plan_step(
    step: &Step,
    node: usize,
    turn: u8,
    copy: u8,
    in_loop: Option<usize>,
    setup: &Setup<'_>,
) -> Result<Planned, RunError> {
    match step {
        Step::Checkpoint(ask) => Ok(Planned {
            id: Uuid::now_v7().to_string(),
            node_key: node_key_for(&ask.id, turn, copy),
            tile_key: ask.id.clone(),
            turn,
            in_loop,
            name: ask.name.clone(),
            // Kafelek kontrolny JEST pytaniem do czlowieka; drugie pytanie po nim byloby tym
            // samym pytaniem dwa razy.
            when_it_fails: WhenItFails::Stop,
            depends_on: Vec::new(),
            vendor: String::new(),
            // Kafelek kontrolny nie dotyka plików, więc nie ma czego składać ani dokąd.
            folds_in: Vec::new(),
            job: Job::Ask {
                question: ask.question.clone(),
            },
        }),
        Step::Agent(agent) => {
            let (spot, folds_in) =
                where_it_works(&agent.folder, &agent.id, &agent.name, node, copy, setup)?;
            let job = plan_agent(agent, node, copy, spot, setup)?;
            Ok(Planned {
                id: Uuid::now_v7().to_string(),
                node_key: node_key_for(&agent.id, turn, copy),
                tile_key: agent.id.clone(),
                turn,
                in_loop,
                name: name_for(&agent.name, copy, agent.copies),
                when_it_fails: agent.when_it_fails,
                depends_on: Vec::new(),
                vendor: job.driver.id().to_owned(),
                folds_in,
                job: Job::Agent(Box::new(job)),
            })
        }
        Step::Check(check) => {
            let (spot, folds_in) =
                where_it_works(&check.folder, &check.id, &check.name, node, copy, setup)?;
            Ok(Planned {
                id: Uuid::now_v7().to_string(),
                node_key: node_key_for(&check.id, turn, copy),
                tile_key: check.id.clone(),
                turn,
                in_loop,
                name: check.name.clone(),
                when_it_fails: check.when_it_fails,
                depends_on: Vec::new(),
                /* PUSTA ETYKIETA VENDORA, i to nie jest brak wartości do wypełnienia.
                 * Ten krok nie woła żadnego vendora, więc `"local"` albo `"loadout"` byłoby
                 * wymyśleniem faktu, po którym wznowienie szukałoby kiedyś sesji, której nigdy
                 * nie było — dokładnie ten sam powód, który stoi przy kafelku kontrolnym. */
                vendor: String::new(),
                folds_in,
                job: Job::Check(Box::new(CheckJob {
                    proof_mode: check.proof_mode().map_err(|message| {
                        RunError::Refused(Note {
                            level: Level::Problem,
                            step_id: Some(check.id.clone()),
                            message,
                            fix: None,
                        })
                    })?,
                    spec: CheckSpec {
                        command: check.command.clone(),
                        proof: check.proof.clone(),
                        cwd: spot.cwd,
                        required_tests: check.required_tests.clone(),
                    },
                    ours: spot.ours,
                })),
            })
        }
        Step::Serve(serve) => {
            let (spot, folds_in) =
                where_it_works(&serve.folder, &serve.id, &serve.name, node, copy, setup)?;
            Ok(Planned {
                id: Uuid::now_v7().to_string(),
                node_key: node_key_for(&serve.id, turn, copy),
                tile_key: serve.id.clone(),
                turn,
                in_loop,
                name: serve.name.clone(),
                // Uruchom-i-zostaw nie orzeka o niczyjej robocie: odmawia przy starcie albo
                // stawia proces i schodzi z drogi.
                when_it_fails: WhenItFails::Stop,
                depends_on: Vec::new(),
                // Pusta etykieta vendora - z tego samego powodu, co przy kafelku sprawdzajacym.
                vendor: String::new(),
                folds_in,
                job: Job::Serve(Box::new(ServeJob {
                    command: serve.command.clone(),
                    /* Sama NAZWA POLA, nie cały typ: to jest jedyna wartość, którą krok czyta
                     * w chwili startu, a `ServeJob` istnieje po to, żeby nie nosić ze sobą
                     * kształtu z pliku workflow. */
                    command_from: serve.command_from.clone(),
                    lifetime: serve.lifetime,
                    start_when: serve.start_when,
                    kind: serve.target_kind,
                    test_data_env: serve.test_data_env.clone(),
                    endpoints: serve.endpoints.clone(),
                    readiness: serve.readiness.clone(),
                    cwd: spot.cwd,
                    ours: spot.ours,
                })),
            })
        }
    }
}

/// Krok agenta: konfiguracja efektywna, sterownik, katalog roboczy.
/// Które węzły rozwinięcia wchodzą do tego biegu.
///
/// Jedna funkcja na oba rodzaje wycinka, bo to jest jedno pytanie zadane dwa razy inaczej —
/// a dwa warunki rozsypane po pętli planowania byłyby dwoma miejscami, w których wolno je
/// rozstrzygnąć niezgodnie (niezmiennik 13).
///
/// # Dlaczego `Onward` liczy się na ROZWINIĘTYM grafie, a nie na pliku
///
/// Bo rundy pętli są węzłami, a nie krokami. Krok wskazany wewnątrz pętli ma iść ze swoimi
/// rundami: stożek policzony na pliku dałby jeden węzeł na krok i po cichu wykasowałby powtórki,
/// czyli zamieniłby pętlę w prostą — a bieg wyglądałby na udany, robiąc coś innego, niż narysował
/// człowiek.
pub(crate) fn which_nodes(
    unrolled: &Unrolled,
    file: &WorkflowFile,
    part: Option<&Part>,
) -> Vec<bool> {
    let Some(part) = part else {
        return vec![true; unrolled.nodes.len()];
    };
    let id_of = |node: &unroll::Node| file.steps.get(node.step).map(Step::id);
    match part {
        /* TYLKO PIERWSZA RUNDA. Powtarzanie rund pętli przy ponownym odpaleniu jednego kroku
         * byłoby powtórzeniem czegoś, o co nikt nie prosił — człowiek wskazał kafelek, nie
         * pętlę. */
        Part::Just(ids) => unrolled
            .nodes
            .iter()
            .map(|node| {
                node.turn == 0 && id_of(node).is_some_and(|id| ids.iter().any(|want| want == id))
            })
            .collect(),
        Part::Onward(from) => {
            let mut wanted = vec![false; unrolled.nodes.len()];
            for (index, node) in unrolled.nodes.iter().enumerate() {
                if id_of(node) == Some(from.as_str()) {
                    wanted[index] = true;
                }
            }
            /* Domknięcie przechodnie przez powtarzany obchód, nie przez rekurencję: graf jest
             * mały (dziesiątki węzłów), a pętla bez stosu nie ma jak przepełnić stosu na pliku
             * przysłanym z zewnątrz. Kończy się, bo każdy obchód albo dokłada węzeł, albo jest
             * ostatni, a węzłów jest skończenie wiele. */
            loop {
                let mut grew = false;
                for (from_node, to_node) in &unrolled.arrows {
                    if wanted.get(*from_node) == Some(&true) && wanted.get(*to_node) == Some(&false)
                    {
                        wanted[*to_node] = true;
                        grew = true;
                    }
                }
                if !grew {
                    return wanted;
                }
            }
        }
    }
}

/// Dopisuje do rachunku kontekstu zadanie dokładnie w takim kształcie, w jakim dostał je krok.
///
/// 2026-09 (Z-45) — `/ask` niesie jedno źródło, a zwykły workflow rozdziela zadanie biegu od
/// instrukcji kroku; osobna funkcja trzyma tę różnicę w jednym miejscu i zostawia `plan_agent`
/// jako składanie gotowych części planu.
fn add_task_context(
    context: &mut Vec<ContextSource>,
    instructions: &str,
    setup: &Setup<'_>,
    node: usize,
) {
    let scope = setup
        .unrolled
        .nodes
        .get(node)
        .and_then(|node| setup.file.steps.get(node.step))
        .and_then(|step| setup.inputs.context_for(step.id()));
    let task = scope.map_or(setup.task.as_str(), |scope| scope.task.as_str());
    if setup.is_ask {
        if !instructions.is_empty() {
            context.push(ContextSource {
                kind: ContextKind::RunTask,
                reference: "ask/task".to_owned(),
                bytes: instructions.len(),
            });
        }
    } else {
        if !task.is_empty() {
            context.push(ContextSource {
                kind: ContextKind::RunTask,
                reference: "run/task".to_owned(),
                bytes: task.len(),
            });
        }
        let instruction_bytes = instructions.replace(TASK_MARK, "").len();
        if instruction_bytes > 0 {
            context.push(ContextSource {
                kind: ContextKind::WorkflowStep,
                reference: format!("workflow/steps/{node}"),
                bytes: instruction_bytes,
            });
        }
    }
}

/// Odmawia za przelotke z DEFINICJI AGENTA i dopiero potem scala te z kafelka.
///
/// Kolejnosc jest tu trescia, nie porzadkiem: kazde z tych dwoch pytan ma wlasnego sedziego
/// i wlasne zdanie dla czlowieka, a odwrocenie ich odsylaloby go do formularza agenta po
/// wiersz, ktory stoi na kafelku.
fn refuse_or_merge_the_passthrough(
    effective: &mut Agent,
    step: &AgentStep,
    replaying: bool,
) -> Result<(), RunError> {
    /* PRZELOTKA JEST SĄDZONA, ZANIM RUSZY PIERWSZY PROCES (niezmiennik 12). Wpis podnoszący
     * dial albo kolidujący z tym, co Loadout ustawia sam, zabiera CAŁY bieg — cicha alternatywa
     * (wywalić wpis i jechać) uczy człowieka, że przelotka nie działa, więc wpisuje to samo
     * jeszcze raz innym zapisem. Pierwsze zdanie z listy, bo `RunError` niesie jedną uwagę,
     * a człowiek naprawia jeden wiersz naraz. */
    if let Some(message) = crate::library::agents::passthrough_refused(effective)
        .into_iter()
        .next()
    {
        return Err(RunError::Refused(Note {
            level: Level::Problem,
            // Kropka na kafelku TEGO kroku: to jego agent niesie ten wiersz, a odmowa bez
            // wskazania kafelka zostawia człowieka ze szukaniem, którego to dotyczy.
            step_id: Some(step.id.clone()),
            message,
            fix: None,
        }));
    }
    /* PRZELOTKA MA DWA NOŚNIKI I OBA SĄ TEGO KROKU (T-90, 2026-08-24). `Overrides` nie niesie
     * `vendorOptions` z rozmysłem — to nie jest pole, które się PODMIENIA, tylko mapa, którą się
     * scala wpis po wpisie. Do tego dnia `AgentStep::vendor_options` nie miał w ścieżce biegu
     * ani jednego czytelnika: człowiek wpisywał flagę na kafelku i proces jej nie widział.
     *
     * SCALENIE STOI ZA ODMOWĄ WYŻEJ, NIE PRZED NIĄ, i to jest cała treść tej kolejności.
     * [`passthrough_refused`] pyta o DEFINICJĘ AGENTA i tak brzmi jego zdanie („delete it from
     * this agent's … options"). Wpis z kafelka wpuszczony w tamto pytanie dostałby odmowę, która
     * odsyła człowieka do formularza agenta po wiersz stojący na kafelku — a wiersze kafelka mają
     * już swojego sędziego ze swoim zdaniem: `workflow::check::the_passthrough` biegnie w
     * `check_to_run`, czyli zanim ten plan w ogóle powstanie, i mówi „remove it from this step's
     * … options". Jeden nośnik, jedno zdanie, oba przed pierwszym procesem (niezmiennik 12). */
    if !replaying {
        effective.vendor_options = crate::library::agents::passthrough_of_the_step(
            &effective.vendor_options,
            &step.vendor_options,
        );
    }
    Ok(())
}

/// Umiejetnosci tego kroku: przy powtorzeniu odtworzone z nagrania, inaczej policzone dzis.
///
/// Nagranie oddaje SAME NAZWY, bo katalogi jeszcze nie istnieja — sciezki dopisuje
/// [`hand_the_skills_to_the_steps`], kiedy katalog biegu juz stoi.
fn what_skills_this_step_gets(
    step: &AgentStep,
    saved: &Agent,
    overrides: &Overrides,
    replay_key: Option<&str>,
    setup: &Setup<'_>,
) -> Result<StepSkills, RunError> {
    Ok(if let Some(replay) = setup.replay {
        StepSkills {
            names: replay_key
                .and_then(|key| replay.skills.get(key))
                .and_then(|one| one.agent.as_ref())
                .map(|items| items.iter().map(|one| one.name.clone()).collect())
                .unwrap_or_default(),
            dirs: Vec::new(),
        }
    } else {
        what_this_step_may_reach(setup.data, setup.project, saved, overrides, step)?
    })
}

/// Polaczenia, ktore ten krok otwiera — albo odmowa z kropka na JEGO kafelku.
///
/// Odmowa pada przy planowaniu, czyli przed pierwszym procesem (niezmiennik 12): polaczenie,
/// ktorego nie da sie zlozyc, jest wada konfiguracji, a nie wynikiem biegu.
fn the_connections_this_step_opens(
    step: &AgentStep,
    effective: &Agent,
    setup: &Setup<'_>,
) -> Result<Vec<crate::connections::Connection>, RunError> {
    crate::connections::runtime::selected(&setup.connections, &effective.connections).map_err(
        |error| {
            RunError::Refused(Note {
                level: Level::Problem,
                step_id: Some(step.id.clone()),
                message: error.to_string(),
                fix: None,
            })
        },
    )
}

/// Co ten krok wie, zanim przeczyta swoje zadanie — i z czego to sie wzielo.
struct StepKnowledge {
    /// Blok „co wiadomo", ktory poprzedza tresc zadania w prompcie.
    knows: String,
    /// Wszystkie zrodla wejscia w rzeczywistej kolejnosci skladania, bez ich tresci.
    context: Vec<ContextSource>,
    /// Ktora notatka pojechala do kroku, a ktora nie i dlaczego.
    memory: Vec<MemoryDisposition>,
}

/// Sklada trzeci blok pamieci tego kroku, jego kontekst i rozliczenie notatek.
///
/// Zamrozony wybor z nagrania musi zgadzac sie co do adresu z tym, co regula kontekstu
/// dopuszcza dzisiaj: powtorzenie, ktore po cichu podmienia notatki na aktualne, nie jest
/// powtorzeniem. Wlasny kontekst kafelka bez pamieci projektu KASUJE caly blok, a nie
/// dokleja sie do niego.
fn what_this_step_will_know(
    step: &AgentStep,
    node: usize,
    instructions: &str,
    scope: Option<&crate::workflow::execution::ContextScope>,
    replay_key: Option<&str>,
    agent: &str,
    setup: &Setup<'_>,
) -> Result<StepKnowledge, RunError> {
    // Trzeci blok pamięci powstaje TUTAJ, bo tutaj po raz pierwszy wiadomo, KTÓRY agent
    // biegnie w tym kroku (2026-08-22, T-80). Zbiór notatek jest ten sam dla całego biegu.
    let (mut knows, mut context, mut memory) = if let Some(snapshot) = setup
        .replay
        .and_then(|replay| replay.memory_sources.as_ref())
    {
        let key = replay_key
            .ok_or_else(|| io::Error::other("The saved memory has no physical step address."))?;
        let known = frozen_known(snapshot, setup.data, setup.project, Some(key))?;
        let prepared = what_this_step_knows(&known, agent, setup.data, setup.project);
        let delivered: BTreeSet<_> = prepared
            .2
            .iter()
            .filter(|one| one.delivered)
            .map(|one| &one.address)
            .collect();
        let expected: BTreeSet<_> = snapshot.selected_for(key)?.iter().collect();
        if delivered != expected {
            return Err(RunError::Io(io::Error::other(
                "The saved memory selection no longer fits this step's context rules; nothing was replaced with today's notes.",
            )));
        }
        prepared
    } else {
        what_this_step_knows(&setup.knows, agent, setup.data, setup.project)
    };
    if scope.is_some_and(|scope| !scope.project_memory) {
        knows.clear();
        context.clear();
        memory.clear();
    }
    add_task_context(&mut context, instructions, setup, node);
    if let Some(scope) = scope
        && !scope.instructions.is_empty()
    {
        if !knows.is_empty() {
            knows.push_str("\n\n");
        }
        knows.push_str(&scope.instructions);
        context.push(ContextSource {
            kind: ContextKind::WorkflowStep,
            reference: format!(
                "context/{}/instructions",
                setup.inputs.context_key(&step.id).unwrap_or_default()
            ),
            bytes: scope.instructions.len(),
        });
    }
    Ok(StepKnowledge {
        knows,
        context,
        memory,
    })
}

/// Krok agenta: konfiguracja efektywna, sterownik, katalog roboczy.
///
/// `spot` przyjeżdża gotowy z [`plan_step`], bo od 2026-08-29 pytanie „gdzie ten krok pracuje"
/// ma jedną odpowiedź dla wszystkich czterech rodzajów kafelka i jedno miejsce, w którym pada
/// ([`where_it_works`]) — a jego druga połowa, lista kopii do złożenia, jest polem [`Planned`],
/// nie [`AgentJob`].
fn plan_agent(
    step: &AgentStep,
    node: usize,
    copy: u8,
    spot: Workspace,
    setup: &Setup<'_>,
) -> Result<AgentJob, RunError> {
    let replay_key = setup
        .unrolled
        .nodes
        .get(node)
        .map(|one| node_key_for(&step.id, one.turn, copy));
    let recorded = setup.replay.and_then(|replay| {
        replay_key
            .as_ref()
            .and_then(|key| replay.effective.get(key))
    });
    let saved = if let Some(agent) = recorded {
        agent.clone()
    } else {
        find_agent(&setup.library, &step.agent, &step.name)?
    };
    // Nadpisania kroku przechodzą przez `Overrides`, więc klucz, którego krok nie ma prawa
    // ruszyć (`id`, `name`, `runsWith`), odbija się o typ, a nie o walidator do zapamiętania.
    let overrides: Overrides = serde_json::from_value(Value::Object(step.overrides.clone()))?;
    let mut effective = if recorded.is_some() {
        saved.clone()
    } else {
        resolve(&saved, &overrides)?.agent
    };

    // Polityka policzona RAZ i czytana dwa razy: raz jako dial kroku, raz jako sufit jego listy
    // narzędzi. Dwa wywołania tej samej tabeli byłyby dwoma miejscami, w których krok mógłby
    // pojechać z inną polityką, niż ta, którą przepuszczono jego narzędzia.
    let policy = policy_of(effective.file_access);
    // FABRYKA WOŁANA TUTAJ, a nie tuż przed literałem struktury: od 2026-08-24 to adapter
    // odpowiada, czy ten vendor w ogóle zawęża narzędzia, więc sterownik musi istnieć, zanim
    // padnie pytanie o sufit. Nadal **raz na krok, przy planowaniu** — powód bez zmian stoi
    // niżej, przy `borrowing_is_possible`.
    let driver = (setup.drivers)(effective.runs_with);
    let tools = what_this_step_may_use(&effective, policy, step, &driver)?;
    refuse_or_merge_the_passthrough(&mut effective, step, recorded.is_some())?;
    let skills =
        what_skills_this_step_gets(step, &saved, &overrides, replay_key.as_deref(), setup)?;
    let connections = the_connections_this_step_opens(step, &effective, setup)?;

    let write_results_to = where_results_go(&effective, step)?;
    let work_plan = crate::work_plan::Configuration::from_step(step.extra.get("plan"))
        .map_err(|error| RunError::Io(io::Error::other(error.to_string())))?;
    let session = Uuid::now_v7();
    // 2026-09-08 — nazwa pochodzi z fizycznej sesji, nie z kafelka. Ponowiona próba nie
    // odziedziczy więc starego kandydata i nie uzna go za wynik nowego procesu.
    let plan_candidate = PathBuf::from(".loadout")
        .join("plan-candidates")
        .join(format!("{session}.json"));

    /* KAŻDA KOPIA WIE, KTÓRA JEST. Trzy sesje z identycznym zdaniem robią tę samą robotę trzy
     * razy, czyli są najdroższym możliwym sposobem na jedną odpowiedź — a podstawienie bez
     * rozwinięcia (do 2026-08-23 nie było żadnego z dwojga) wpisywałoby w prompt liczbę, której
     * nic po drugiej stronie nie odpowiada. */
    let instructions = numbered(&step.instructions, copy, step.copies);
    let scope = setup.inputs.context_for(&step.id);
    let StepKnowledge {
        knows,
        context,
        memory,
    } = what_this_step_will_know(
        step,
        node,
        &instructions,
        scope,
        replay_key.as_deref(),
        &effective.name,
        setup,
    )?;
    let task = scope.map_or(setup.task.as_str(), |scope| scope.task.as_str());

    // Sterownik stoi już wyżej (przy suficie narzędzi) i jest **jeden na krok**: etykieta
    // vendora idzie do `run.json` od pierwszego zrzutu, więc historia biegu wie, do kogo
    // wracać, także wtedy, gdy krok nigdy nie ruszył. Pytanie „czy ten program w ogóle umie
    // przyjąć katalog pluginu" zadajemy tej samej instancji i ZANIM powstanie katalog biegu.
    let borrows = what_this_step_borrows(&step.borrow);
    if recorded.is_none() {
        borrowing_is_possible(setup.project, &driver, &borrows, step)?;
    }
    // Policzone, ZANIM sterownik wejdzie do struktury: `Arc` idzie tam przez przeniesienie,
    // a klon tylko po to, żeby zadać jedno pytanie, byłby drugim uchwytem do niczego.
    let frozen = what_was_frozen(&effective, &driver)?;

    Ok(AgentJob {
        driver,
        weight: match step.weight {
            crate::workflow::Weight::Ordinary => limits::Weight::Ordinary,
            crate::workflow::Weight::Heavy => limits::Weight::Heavy,
        },
        agent_name: effective.name.clone(),
        session,
        cwd: spot.cwd,
        ours: spot.ours,
        write_results_to,
        work_plan,
        plan_candidate,
        // Formularz albo nic. `Handover::Plain` znaczy „oddaj to, co masz do powiedzenia,
        // i tyle" — czyli dokładnie to, co robi każdy krok bez tego pola.
        handover: match &step.handover {
            Handover::Form { fields } => fields.clone(),
            Handover::Plain(_) => Vec::new(),
        },
        criteria: step.criteria.clone(),
        // Treść zadania, z `{{copy}}` i `{{copies}}` już podstawionymi [T3 §4.3, §4.4].
        // Zadanie kroku POPRZEDZONE tym, co człowiek dopuścił do użytku (`what_the_agents_know`).
        // Bez człowieka blok jest pusty i prompt jest dokładnie zadaniem kroku —
        // `docs/ARCHITECTURE.md` §2 pytanie 5 obiecuje właśnie to.
        // Zadanie CAŁEGO biegu wchodzi do zadania kroku (`with_the_task`), a dopiero to, co z tego
        // wyszło, dostaje blok „co wiadomo". Ta kolejność jest treścią: notatki są kontekstem
        // stojącym nad wszystkim, zadanie biegu jest polem pracy, a prompt kroku jest robotą
        // w tym polu — od najogólniejszego do najkonkretniejszego, czyli tak, jak to czyta model.
        prompt: with_what_we_know(&knows, &with_the_task(task, &instructions)),
        reference_materials: String::new(),
        asked: instructions,
        context,
        memory,
        model: some_text(&effective.model),
        // Prompt systemowy agenta, nie treść zadania: treść zadania w tym polu byłaby
        // niezmiennikiem 9 złamanym po cichu, bo stąd wchodzi do argv.
        system_append: some_text(&effective.instructions),
        policy,
        // Wybór agenta, nie kroku (D6: „wszystko, co vendor wprowadzi, konfigurujemy per agent").
        reaches_the_web: effective.reaches_the_web,
        tools,
        // Z definicji EFEKTYWNEJ, czyli po złożeniu z nadpisaniem kroku: wiersz „ile myśleć"
        // w panelu kroku ma wygrywać z tym, co stoi w bibliotece — inaczej jest kontrolką bez
        // skutku (niezmiennik 16).
        thinking: effective.thinking,
        connections,
        service_access: effective.service_access.clone(),
        agent_messages: effective.agent_messages,
        // W kształcie TEJ aplikacji, policzonym raz: `--flaga wartość` dla Claude Code,
        // `-c klucz=wartość` dla Codeksa. Klucz przelotki bierze się z vendora agenta, bo to
        // plik agenta nazywa go tym słowem — nie z etykiety sterownika, którą w teście nosi
        // dubler.
        passthrough: crate::library::agents::vendor_argv(&effective, effective.runs_with.key()),
        skills,
        // Ścieżka katalogu pluginu tego kroku dopiero powstanie: plan nie dotyka dysku, a katalog
        // biegu jeszcze nie istnieje. Wypełnia to [`hand_the_skills_to_the_steps`].
        plugin_flags: Vec::new(),
        borrows,
        // Z tego samego powodu, co `plugin_flags` wyżej: wypełnia to
        // [`bring_in_what_each_step_borrowed`], kiedy katalog biegu już stoi.
        borrowed: Inherited::default(),
        // `0` znaczy „bez limitu" (`library::agents::Agent::give_up_after_minutes`), więc jedzie
        // tu jako `Duration::MAX` — tym samym kształtem, którym `Live::one_turn` opisuje każdy
        // inny krok bez terminu (`Job::Ask | Job::Check | Job::Serve`). Do 2026-08-23 stało tu
        // `.max(1)`, czyli JEDNA minuta: krok bez limitu ginął po sześćdziesięciu sekundach,
        // a odkąd blok z T-86 mówi mu wprost „there is no time limit on this step", ta minuta
        // była już nie tylko zaskoczeniem, ale i naszym własnym kłamstwem w prompcie.
        give_up_after: match effective.give_up_after_minutes {
            0 => Duration::MAX,
            minutes => Duration::from_secs(u64::from(minutes) * 60),
        },
        // Ta sama liczba, nietknięta — to ją dostaje agent (`Live::how_long_this_step_has`).
        minutes: effective.give_up_after_minutes,
        effective: frozen,
    })
}

/// Zdanie dla człowieka, którego lista narzędzi nie miała u tego vendora czego zawęzić.
///
/// Brzmi jak zdanie z formularza agenta (`src/sections/agents/more-settings.tsx`) i to jest
/// świadome: ten sam fakt powiedziany dwoma różnymi zdaniami czyta się jak dwa różne fakty.
const NOTHING_TO_NARROW: &str = "Codex doesn't take a list of tools, so this agent's list was left out. What it may reach is \
     set by 'Can change files'.";

/// Migawka konfiguracji efektywnej — plus jedno zdanie, kiedy bieg czegoś z niej nie użył.
///
/// # Po co zdanie ląduje W MIGAWCE, a nie obok niej
///
/// `run.json` jest jedyną rzeczą, która przeżywa skasowanie indeksu (niezmiennik 4), a migawka
/// jest w nim tym miejscem, które odpowiada na pytanie „z czym ten krok naprawdę pojechał".
/// Ustawienie, które ekran przyjął, a bieg pominął, jest bez tego zdania kontrolką bez skutku
/// (niezmiennik 16) — z tą różnicą, że schowaną o warstwę głębiej niż martwy przycisk, bo
/// „agent nie użył narzędzia" jest z zewnątrz nieodróżnialne od „agent uznał, że nie warto".
///
/// Klucz dopisany **po serializacji**, a nie pole w [`Agent`]: definicja agenta na dysku opisuje
/// wybór człowieka, a to jest zdanie o jednym biegu. Pole w tamtym typie wróciłoby do biblioteki
/// przy pierwszym zapisie i zostało tam na zawsze.
fn what_was_frozen(agent: &Agent, driver: &Arc<dyn AgentDriver>) -> Result<Value, RunError> {
    let mut frozen = serde_json::to_value(agent)?;
    // Tylko wtedy, gdy człowiek naprawdę coś wpisał: „wszystkie narzędzia" u vendora bez listy
    // to nie jest pominięcie, tylko zgodność, i zdanie o niej byłoby szumem w każdym biegu.
    if !driver.narrows_its_tools()
        && matches!(agent.tools, Tools::Only(_))
        && let Some(fields) = frozen.as_object_mut()
    {
        fields.insert(
            "toolsNote".to_owned(),
            Value::String(NOTHING_TO_NARROW.to_owned()),
        );
    }
    Ok(frozen)
}

/// Gdzie ten krok ma odłożyć swoją odpowiedź — albo odmowa nazywająca pole, które ją zatrzymało.
///
/// # Odmowa pada PRZED startem, jak każda inna (niezmiennik 12)
///
/// Ścieżka bezwzględna albo wyprowadzająca poza folder kroku jest odmową **całego biegu**, tutaj,
/// zanim ruszy pierwszy proces. Druga możliwa implementacja — spróbować zapisać i po cichu się
/// poddać — jest najdroższą wersją tej wady: człowiek dostaje bieg bez wyniku i bez zdania,
/// a folder, w który celował, wygląda tak samo jak folder, do którego agent nie miał nic do
/// napisania.
///
/// # Co znaczy „wyprowadza"
///
/// Cokolwiek, co nie jest czystą ścieżką w dół: korzeń dysku, przedrostek dysku i `..`
/// gdziekolwiek w środku. Dowiązanie założone po drodze przez kogoś innego prowadzi tam samo
/// i tego z samego napisu wyliczyć się nie da — pyta o to jądro
/// [`Live::the_answer_has_somewhere_to_go`], wciąż przed startem kroku, a potem raz jeszcze
/// [`Live::file_the_answer`] tuż przed zapisem, bo między jednym a drugim stoi cała tura agenta.
///
/// Puste pole i pole z samych spacji to jeden fakt („nie proszę o żaden plik"), więc jedna
/// odpowiedź: `None`.
fn where_results_go(agent: &Agent, step: &AgentStep) -> Result<Option<PathBuf>, RunError> {
    let Some(asked) = some_text(&agent.write_results_to) else {
        return Ok(None);
    };
    let path = PathBuf::from(asked.trim());
    let leaves = path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        });
    if leaves {
        return Err(RunError::Refused(Note {
            level: Level::Problem,
            // Kropka na kafelku TEGO kroku: to jego wiersz człowiek otworzy, żeby go poprawić.
            step_id: Some(step.id.clone()),
            message: format!(
                "\"{}\" has {WRITE_RESULTS_TO} set to \"{}\", and that leads out of the folder \
                 this step works in. Loadout stopped the run instead of starting it. Give it a \
                 path inside that folder, or leave the row empty.",
                step.name,
                path.display()
            ),
            fix: None,
        }));
    }
    Ok(Some(path))
}

/// Które narzędzia ten krok dostaje pod rękę — albo odmowa, jeśli prosi o coś ponad swój dial.
///
/// # 2026-08-20 (T-63) — DO TEGO DNIA `agent.tools` NIE MIAŁO TU ANI JEDNEGO CZYTELNIKA
///
/// Pole `tools` jest w formularzu agenta od T-11: człowiek je ustawia, panel kroku pokazuje je
/// jako „Agent uses: …", plik na dysku je zapisuje — i nie docierało do biegu, bo `RunSpec` nie
/// miał na nie pola, a jedynym źródłem `--tools` był sufit polityki. Człowiek zawężający narzędzia,
/// bo nie chce, żeby agent sięgał do sieci albo odpalał komendy, dostawał ekran, który to przyjmuje
/// i potwierdza; agent i tak dostawał wszystko, co daje jego dial. Nikt się o tym nie dowiedział,
/// bo „agent nie użył narzędzia" jest nieodróżnialne od „agent uznał, że nie warto" — to jest
/// martwa kontrolka (niezmiennik 16) schowana o warstwę głębiej.
///
/// # Odmowa pada TUTAJ, przy budowie zadania
///
/// Niezmiennik 12: odmowa najpóźniej przy Starcie, nigdy w trakcie biegu. Ten kod biegnie
/// w planowaniu, czyli **zanim** ruszy pierwszy proces — a `RunError::Refused` zabiera cały bieg,
/// więc nie ma stanu, w którym część kroków ruszyła z listą, której nikt nie przepuścił.
/// Alternatywa — przycięcie listy i jazda dalej — jest najdroższą wersją tej wady: agent, któremu
/// po cichu zabrano narzędzie, wygląda dokładnie jak agent, który „nie umiał".
///
/// # 2026-08-24 (T-97) — SUFIT JEST PYTANIEM DO VENDORA, NIE STAŁĄ CLAUDE'A
///
/// Do tego dnia ta funkcja przepuszczała listę **każdego** agenta przez `claude::tool_surface`,
/// bo innego sufitu w tym drzewie nie było. Dla Claude'a to jest poprawne i tak zostaje. Dla
/// Codeksa nie: jego adapter `spec.tools` nie czyta ani razu, `CAPABILITIES` mówi o tym polu
/// `Unavailable` — a mimo to `Read, Bash` wpisane agentowi Codeksa na dialu „look only"
/// zabierało CAŁY bieg, o ustawienie, które u tego vendora nie robi nic.
///
/// Odpowiada więc adapter, o siebie ([`AgentDriver::narrows_its_tools`]), a polityka „lista
/// wybiera spośród diala, nigdy ponad" zostaje tutaj, w jednym miejscu (niezmiennik 23). Druga
/// tabela nazw narzędzi per vendor jest dokładnie tym, czego ten niezmiennik zabrania.
fn what_this_step_may_use(
    agent: &Agent,
    policy: Policy,
    step: &AgentStep,
    driver: &Arc<dyn AgentDriver>,
) -> Result<Option<Vec<String>>, RunError> {
    // PYTANIE PADA PRZED WSZYSTKIM INNYM, także przed pustą listą: „wyczyszczona lista jest
    // odmową" jest zdaniem o `--tools`, a vendor bez tej flagi nie ma czego wyczyścić.
    if !driver.narrows_its_tools() {
        return Ok(None);
    }
    let wanted = match &agent.tools {
        // „Wszystkie narzędzia" jedzie do sterownika jako `None`, czyli „nie zawężaj". To jest
        // DOKŁADNIE dzisiejsze argv — sufit polityki — i dlatego ta gałąź nie woła niczego:
        // przepuszczenie sufitu przez własny filtr dawałoby ten sam wynik dłuższą drogą, a przy
        // pierwszej zmianie filtra przestałoby go dawać.
        Tools::Everything => return Ok(None),
        Tools::Only(names) => names,
    };

    let surface = tool_surface(policy, Some(wanted));
    match surface.refused {
        None => Ok(Some(surface.available)),
        Some(refused) => Err(RunError::Refused(Note {
            level: Level::Problem,
            // Kropka ląduje na kafelku TEGO kroku: to jego lista narzędzi i jego dial, a odmowa
            // bez wskazania kafelka zostawia człowieka ze szukaniem, którego agenta dotyczy.
            step_id: Some(step.id.clone()),
            message: crate::engine::drivers::claude::no_such_tools(&agent.name, &refused),
            fix: None,
        })),
    }
}

/// Po które umiejętności ten krok może sięgnąć — albo odmowa, jeśli którejś nie może dostać.
///
/// # 2026-08-22 (T-79) — DO TEGO DNIA `agent.skills` NIE MIAŁO TU ANI JEDNEGO CZYTELNIKA
///
/// `Agent.skills` jest polem formularza agenta od T-11, `~/.loadout/skills/<nazwa>/` kanoniczną
/// kopią od T-18 — i poza modułem importu **nikt tych pól nie czytał**. Człowiek zaznaczał
/// umiejętność, ekran to przyjmował, dysk zapisywał, a proces agenta nie dostawał ani jednego
/// bajtu. Nikt się o tym nie dowiadywał, bo „agent nie zna tej umiejętności" jest z zewnątrz
/// nieodróżnialne od „model nie uznał, że warto po nią sięgnąć" — to jest ta sama martwa
/// kontrolka (niezmiennik 16) schowana o warstwę głębiej, którą niezmiennik 29 nazywa wprost.
///
/// # Dwa źródła wyboru na kroku i tylko jedno rozstrzyga
///
/// Nadpisanie (`Overrides::skills`, patch RFC 7396) wygrywa, bo to ono jest **różnicą wobec
/// agenta**: brak klucza znaczy „weź to, co ma agent", `[]` znaczy „żadnych", lista znaczy
/// podzbiór — dokładnie tak, jak czyta resztę definicji `library::agents::resolve`. Pole pliku
/// workflow (`AgentStep::skills`, `"all"` albo lista) odpowiada dopiero wtedy, gdy patcha nie ma:
/// jest starsze, ma tę samą semantykę i do tego dnia też nie miało czytelnika. Odwrotna kolejność
/// znaczyłaby, że wartość domyślna jednego pola (`"all"`) kasuje jawny wybór drugiego.
///
/// # Odmowa pada TUTAJ, przy budowie zadania
///
/// Niezmiennik 12: najpóźniej przy Starcie, nigdy w trakcie biegu. Alternatywa — przyciąć listę
/// i jechać dalej — jest najdroższą wersją tej wady: człowiek zaznacza pięć umiejętności, agent
/// dostaje trzy, nic nie pada i nikt się o tym nie dowiaduje.
fn what_this_step_may_reach(
    data: &Path,
    project: &Path,
    saved: &Agent,
    overrides: &Overrides,
    step: &AgentStep,
) -> Result<StepSkills, RunError> {
    let picked = overrides.skills.clone().or_else(|| match &step.skills {
        Skills::Every(_) => None,
        Skills::Only(names) => Some(names.clone()),
    });
    let roots = crate::skills::Roots {
        home: data.parent().unwrap_or(data).to_path_buf(),
        project: Some(project.to_path_buf()),
        data: data.to_path_buf(),
    };
    let selected = crate::skills::bundle::choices(&saved.extra).map_err(|error| {
        RunError::Refused(Note {
            level: Level::Problem,
            step_id: Some(step.id.clone()),
            message: error.to_string(),
            fix: None,
        })
    })?;
    StepSkills::from_selected_sources(
        &roots,
        &saved.skills,
        picked.as_deref(),
        &step.name,
        &selected,
    )
    .map(|found| found.skills)
    .map_err(|missing| {
        RunError::Refused(Note {
            level: Level::Problem,
            // Kropka ląduje na kafelku TEGO kroku: to jego lista umiejętności, a odmowa bez
            // wskazania kafelka zostawia człowieka ze szukaniem, którego agenta dotyczy.
            step_id: Some(step.id.clone()),
            // ZDANIE CO DO SŁOWA Z `skills::Missing`. Własne brzmienie byłoby drugą kopią jednej
            // odmowy, a druga kopia jest zawsze tą nieaktualną (niezmiennik 23) — tym bardziej
            // że to samo zdanie czyta potem ekran pracy.
            message: missing.to_string(),
            fix: None,
        })
    })
}

/// Znajduje w bibliotece agenta o tym identyfikatorze.
///
/// Szukamy po `id`, nie po nazwie pliku: krok workflow nazywa agenta identyfikatorem, bo ten
/// przeżywa zmianę nazwy (T3 §3.1). Plik, którego nie da się przeczytać, **nie zabiera biegu**,
/// który go nie używa — ale jeśli szukanego nie ma, to właśnie jego błąd jest odpowiedzią,
/// bo „nie ma takiego agenta" i „ten plik jest zepsuty" naprawia się inaczej [T4 §10].
fn find_agent(library: &Path, id: &str, step: &str) -> Result<Agent, RunError> {
    // 2026-08-18 — KATALOG, KTÓREGO NIE MA, TO ZDANIE O AGENTACH, NIE O SYSTEMIE PLIKÓW.
    // `fs::read_dir(library)?` szło tu wprost w `RunError::Io`, który jest przezroczysty, więc
    // pierwsze uruchomienie po instalacji kończyło się „No such file or directory (os error 2)".
    // Katalog `agents/` powstaje dopiero przy pierwszym zapisanym agencie, czyli na świeżej
    // maszynie NIE ISTNIEJE — to jest stan normalny, nie awaria dysku, i ma o tym mówić.
    let files = read_agent_directory(library).map_err(RunError::Agent)?;
    // Katalog, który istnieje i jest pusty, jest tym samym faktem co katalog, którego nie ma:
    // nie ma z czego wybrać. Dwa różne zdania o jednym stanie byłyby dwoma miejscami prawdy.
    if files.is_empty() {
        return Err(RunError::NoAgentsSaved {
            step: step.to_owned(),
        });
    }
    let mut broken = None;
    // Nazwy, które udało się przeczytać. Zbierane po drodze, bo drugi spacer po katalogu byłby
    // drugą odpowiedzią na pytanie „kogo mam zapisanych" (niezmiennik 13).
    let mut saved: Vec<String> = Vec::new();
    for (_path, loaded) in files {
        match loaded {
            // Rewizja pliku należy do ZAPISU, a bieg tylko czyta: krok, który dostał agenta,
            // nie ma go jak odłożyć z powrotem, więc nie ma czego pilnować.
            Ok(read) if read.agent.id.to_string() == id => return Ok(read.agent),
            Ok(read) => saved.push(read.agent.name),
            Err(error) => broken = broken.or(Some(error)),
        }
    }
    /* PLIK ZEPSUTY I AGENT, KTÓREGO NIE MA, TO DWIE RÓŻNE RZECZY DO ZROBIENIA [T4 §10]:
     * pierwszą naprawia się poprawką w tym pliku, drugą wpisaniem innej nazwy. Zdanie o
     * literówce w `scout.md` wygrywa, bo dopóki tamten plik się nie czyta, „nie ma takiego
     * agenta" może być nieprawdą. */
    if let Some(error) = broken {
        return Err(RunError::Agent(error));
    }
    /* ODMOWA WYMIENIA NAZWY — i to jest cała jej treść, ten sam powód, dla którego odmowa
     * `/run` wypisuje nazwy workflow (`run-command.ts`, `noSuchWorkflow`). „No agent with that
     * id" zostawia człowieka dokładnie tam, gdzie był, a nazw, których nie widzi, nie ma jak
     * zgadnąć: powstają z plików w bibliotece (DESIGN §8).
     *
     * 2026-08-20 (T-62) — do tego dnia szło tu `AgentError::Unreadable`, więc zdanie zaczynało
     * się absolutną ścieżką katalogu i nie mówiło ani jednej nazwy. Dla biegu z pliku było to
     * słabe, dla `/ask` byłoby bezużyteczne: tam ten napis ląduje w wierszu wejścia, pół
     * sekundy po tym, jak człowiek wpisał nazwę z palca. */
    Err(RunError::Refused(Note {
        level: Level::Problem,
        // Kropka na kafelku wymaga kroku, KTÓRY ISTNIEJE (`check::Note::step_id`), a tego kroku
        // nie ma: agent, którego nazywa, nie jest w bibliotece.
        step_id: None,
        message: no_agent_called(id, &saved),
        fix: None,
    }))
}

/// Zdanie o agencie, którego w bibliotece nie ma — z nazwami tych, którzy są.
///
/// Osobna funkcja, bo składa się z dwóch kawałków, z których drugi bywa pusty: biblioteka
/// z samymi nieczytelnymi plikami nie ma czego wymienić, a zdanie „These are the ones you
/// have: ." jest gorsze niż jego brak. Pusta lista nie zdarza się w praktyce — [`find_agent`]
/// odmawia wcześniej, kiedy w katalogu nie ma ani jednego pliku — więc ten warunek jest
/// obroną kształtu zdania, nie ścieżką, którą ktoś przejdzie.
fn no_agent_called(id: &str, saved: &[String]) -> String {
    let mut said = format!("No agent saved in Agents has the id {id}.");
    if !saved.is_empty() {
        // Nazwy, nie liczba: „you have 2 agents" mówi, że jest problem, i nie mówi, jak go
        // rozwiązać. Kolejność jest kolejnością plików, czyli alfabetyczna po nazwie pliku —
        // ta sama, którą człowiek widzi w sekcji Agenci.
        let _ = write!(said, " These are the ones you have: {}.", saved.join(", "));
    }
    said
}

/// Katalog roboczy kroku i jedyna rzecz, którą trzeba o nim wiedzieć poza ścieżką.
#[derive(Debug, Clone)]
struct Workspace {
    /// Gdzie ten krok pracuje.
    cwd: PathBuf,
    /// Czy ten bieg ma ten katalog **założyć**. Fałsz dla każdego katalogu, który jest już czyjś:
    /// folder projektu, folder wskazany ręcznie, i drzewo, w którym pracował krok przed tym.
    ours: bool,
}

/// Folder kroku i klucz, pod którym leży jego katalog roboczy.
///
/// `None` dla kafelka kontrolnego: on nie dotyka plików, tylko pyta człowieka. To rozróżnienie
/// jest treścią przy [`trees_before`] — pytanie „w którym drzewie pracował krok przede mną"
/// przechodzi przez taki kafelek dalej, zamiast rozbić się o brak odpowiedzi.
fn folder_and_key(step: &Step) -> Option<(&Folder, &str)> {
    match step {
        Step::Agent(one) => Some((&one.folder, one.id.as_str())),
        Step::Check(one) => Some((&one.folder, one.id.as_str())),
        Step::Serve(one) => Some((&one.folder, one.id.as_str())),
        Step::Checkpoint(_) => None,
    }
}

/// Kopia jednego kroku, którego praca ma wejść do kopii składanej.
///
/// Nazwa jest z KAFELKA, bo jedynym powodem, dla którego ta lista niesie coś poza ścieżką, jest
/// zdanie o niezgodzie: człowiek ma przeczytać, które dwa kafelki napisały w jednym pliku co
/// innego (niezmiennik 14).
#[derive(Debug, Clone)]
struct Folded {
    /// Nazwa z kafelka, z numerem kopii, jeśli krok biegnie w kilku ([`name_for`]).
    name: String,
    /// Katalog, w którym ten krok pracował.
    cwd: PathBuf,
}

/// Gdzie pracuje jeden krok — z odpowiedzią także dla tego, który sam jej nie zna.
///
/// [`Folder::SameCopy`] jest jedynym wariantem, którego nie da się rozstrzygnąć z samego kroku:
/// „to samo drzewo, w którym pracował krok przede mną" jest zdaniem o GRAFIE. Dlatego wejście do
/// rozwiązywania folderu korzysta z jednego `CopyPlan`, wspólnego także z rachunkiem Labu.
///
/// Odmowa zamiast domysłu, kiedy odpowiedzi nie ma wcale. Ciche zejście do folderu projektu
/// byłoby dokładnie tą implementacją, przed którą ten wariant powstał: kafelek mówi „to samo
/// drzewo", a krok pisze po prawdziwych plikach człowieka. Pada **przy planowaniu**, czyli zanim
/// ruszy pierwszy proces i zanim powstanie katalog biegu (niezmiennik 12).
/// `copy` jest numerem kopii tego węzła i wchodzi WYŁĄCZNIE do klucza katalogu roboczego:
/// kropka odmowy dalej ląduje na kafelku, bo to jego wiersz człowiek otworzy.
///
/// Druga wartość jest listą kopii, które ten krok ma ZŁOŻYĆ — pusta dla każdego kroku, który
/// niczego nie składa.
fn where_it_works(
    folder: &Folder,
    key: &str,
    name: &str,
    node: usize,
    _copy: u8,
    setup: &Setup<'_>,
) -> Result<(Workspace, Vec<Folded>), RunError> {
    let chosen = setup.folders.nodes.get(node).ok_or_else(|| {
        RunError::Io(io::Error::other("the working folder names an unknown step"))
    })?;
    let root = chosen.root.as_ref().ok_or_else(|| {
        RunError::Refused(Note {
            level: Level::Problem,
            step_id: Some(key.to_owned()),
            message: crate::workflow::check::nothing_before(name),
            fix: None,
        })
    })?;
    // Alias jest wspólną polityką z rachunkiem Labu; wybór inicjalizatora wycinka nadal
    // należy do tego konkretnego biegu. FreshCopy/rundy deduplikuje istniejący layout.
    let ours = match folder {
        Folder::FreshCopy => true,
        Folder::Project => {
            setup.inputs.project_owner(setup.file, key).is_some()
                && scoped_copy_initializer(setup, key) == Some(key)
        }
        Folder::SameCopy => chosen.establishes,
        Folder::Pick { .. } => false,
    };
    let before = if matches!(folder, Folder::SameCopy) && chosen.establishes {
        trees_before(node, setup)
    } else {
        Vec::new()
    };
    Ok((
        Workspace {
            cwd: folder_path(root, setup),
            ours,
        },
        before,
    ))
}

/// Pierwszy wybrany krok zakłada wspólną kopię; nie musi być jej historycznym właścicielem.
fn scoped_copy_initializer<'a>(setup: &'a Setup<'_>, tile: &str) -> Option<&'a str> {
    let scope = setup.inputs.context_key(tile)?;
    setup
        .file
        .steps
        .iter()
        .enumerate()
        .find(|(at, step)| {
            setup.inputs.context_key(step.id()) == Some(scope)
                && folder_and_key(step).is_some_and(|(folder, _)| matches!(folder, Folder::Project))
                && setup
                    .unrolled
                    .nodes
                    .iter()
                    .enumerate()
                    .any(|(index, node)| node.step == *at && setup.wanted.get(index) == Some(&true))
        })
        .map(|(_, step)| step.id())
}

/// Katalog pracy, wspólny dla agenta, komendy i usługi. Checkpoint nie dotyka plików.
fn where_the_job_works(job: &Job) -> Option<&Path> {
    match job {
        Job::Agent(one) => Some(one.cwd.as_path()),
        Job::Check(one) => Some(one.spec.cwd.as_path()),
        Job::Serve(one) => Some(one.cwd.as_path()),
        Job::Ask { .. } => None,
    }
}

/// Katalog własnej kopii kroku o tym kluczu pracy.
///
/// Jedno miejsce: wszystkie rozstrzygnięte aliasy z `CopyPlan` adresuje [`folder_path`].
fn own_copy_at(dir: &Path, work_key: &str) -> PathBuf {
    dir.join(WORK_DIR).join(work_key)
}

/// Jedynie adresuje rozstrzygnięty alias. Reguły dziedziczenia i składania są w `CopyPlan`.
fn folder_path(root: &crate::workflow::unroll::folders::Root, setup: &Setup<'_>) -> PathBuf {
    root.at(setup.project, &setup.dir.join(WORK_DIR))
}

/// Nazwa kafelka tego węzła — z numerem kopii, jeśli krok biegnie w kilku.
///
/// Pusty napis dla węzła spoza pliku, czyli dla kształtu, którego `unroll` nie produkuje.
fn tile_name_of(node: usize, setup: &Setup<'_>) -> String {
    let Some(one) = setup.unrolled.nodes.get(node) else {
        return String::new();
    };
    let Some(step) = setup.file.steps.get(one.step) else {
        return String::new();
    };
    match step {
        // TĄ SAMĄ FUNKCJĄ, CO PODPIS KROKU W `run.json` (`plan_step`): „Build (2 of 3)" ma
        // w zdaniu o niezgodzie brzmieć dokładnie tak, jak brzmi na ekranie.
        Step::Agent(agent) => name_for(&agent.name, one.copy, agent.copies),
        other => other.name().to_owned(),
    }
}

/// W jakich katalogach pracują kroki PRZED tym — bez powtórzeń, z nazwami ich kafelków.
///
/// Obchód idzie po strzałkach **wstecz**, ze zbiorem odwiedzonych: fan-in bywa diamentem, więc
/// bez niego ten sam krok liczyłby się dwa razy i zwykłe rozwidlenie wyglądałoby jak dwa różne
/// drzewa. Iteracyjny, nie rekurencyjny — łańcuch dwudziestu kroków nie ma prawa przepełnić stosu
/// (ta sama zasada, co przy obchodach w `workflow::check`).
///
/// Mija po drodze dwa rodzaje kroków, które drzewa nie wyznaczają: kafelek kontrolny (nie dotyka
/// plików) i krok „to samo drzewo", który sam niczego nie składa (jego odpowiedź jest tym samym
/// pytaniem, zadanym dalej). Stąd „najbliższy poprzednik, jakiegokolwiek rodzaju jest".
///
/// Zero katalogów znaczy „przed tym krokiem nie ma nikogo" i jest odmową u wołającego; więcej niż
/// jeden — „poprzednicy pracują w różnych drzewach", czyli jest co składać.
fn trees_before(node: usize, setup: &Setup<'_>) -> Vec<Folded> {
    setup
        .folders
        .nodes
        .get(node)
        .into_iter()
        .flat_map(|one| &one.parents)
        .map(|(from, root)| Folded {
            name: tile_name_of(*from, setup),
            cwd: folder_path(root, setup),
        })
        .collect()
}

/// Dial „co agent może zrobić z plikami" → polityka, którą rozumie sterownik.
///
/// # Ta nazwa zostaje pod tym adresem, a tabela stoi przy dialu [2026-08-20, T-63]
///
/// Do tego dnia `match` mieszkał tutaj, a `commands::chat` trzymał jego drugą kopię, bo moduł
/// obok nie widział prywatnego elementu sąsiada. T-63 AC-4 każe skasować kopię i **mierzy** to
/// (`one_table_for_policy.rs` liczy pliki, w których stoi to odwzorowanie, i wymaga jednego).
///
/// Drogą, którą wskazywał tamten kontrakt — „lider woła `super::run::policy_of`" — pójść nie da
/// się: `chat_never_starts_a_run.rs` (T-60) asertuje, że napisu `super::run` w kodzie
/// `commands/chat.rs` NIE MA, bo brak tej zależności jest jedynym mechanizmem, którym rozmowa nie
/// może zacząć biegu. Napisanie tej samej ścieżki inaczej (`crate::commands::run`) przeszłoby przez
/// to sprawdzenie i byłoby tą samą zależnością w przebraniu — dokładnie tym, co niezmiennik 20
/// nazywa testem na obecność napisu.
///
/// Więc wspólny fakt zszedł do modułu, od którego oba moduły komend już zależą, i stanął przy
/// [`crate::library::agents::FileAccess`], czyli przy dialu, o którym mówi. Re-eksport zostaje,
/// bo pod adresem `commands::run::policy_of` wołają go dwa kryteria (T-62 `ask_one_agent.rs`
/// i T-63 `one_table_for_policy.rs`): jedna funkcja, dwie drogi do niej, zero drugich tabel.
pub use crate::library::agents::policy_of;

/// Napis albo nic. Puste pole w definicji agenta znaczy „nie mam zdania", a nie „ustaw pustkę".
fn some_text(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.to_owned())
}

/// Zaklada katalog biegu i bierze go na wlasnosc — albo odmawia, zanim cokolwiek powstanie.
///
/// Osobno od reszty, bo to jedyne miejsce, ktore odpowiada na pytanie CZYJ jest ten katalog.
/// Zastany katalog nalezy do tej proby wylacznie wtedy, gdy niesie ten sam claim Bound; kazdy
/// inny zastany jest odmowa, a nie zaproszeniem do dopisania sie do cudzego biegu.
fn claim_the_run_directory(
    plan: &Plan,
    project: &Path,
    provisional: &mut ProvisionalRun,
) -> Result<(), RunError> {
    // Proof obejmuje kazdy bieg, takze bez `fresh-copy`. Dopiero on tworzy realny katalog biegu
    // i `logs/`; zadne `create_dir_all` nie moze po cichu przejsc przez symlink przodka.
    let run_directory_was_missing = match fs::symlink_metadata(&plan.dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        Ok(_) if provisional.can_reclaim_bound_directory(&plan.id, &plan.dir) => false,
        Ok(_) => {
            return Err(RunError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "the planned run directory already exists and does not belong to this attempt",
            )));
        }
        Err(error) => return Err(RunError::Io(error)),
    };
    let prepared = prepare_run_directory(project, &plan.dir);
    if run_directory_was_missing
        && fs::symlink_metadata(&plan.dir).is_ok_and(|metadata| metadata.file_type().is_dir())
    {
        provisional.owns_run_directory(plan.dir.clone());
    }
    prepared.map_err(|problem| RunError::Io(io::Error::other(problem.to_string())))?;
    let publication = supervisor::PublicationRoot::open(&plan.dir)?;
    for scope in plan.inputs.configuration.contexts.keys() {
        publication.ensure_directory(&PathBuf::from("context").join(scope), 0o700)?;
    }
    // 2026-08-28 (T-152 review): retry dokładnie tego samego Bound claimu ma ten sam UUID i
    // katalog. Brak durable `run.json` dowodzi, że lifecycle jeszcze nie ruszył, więc ten
    // istniejący katalog nadal należy do jednej logicznej próby. Zwykły Start nie ma tego
    // provenance i nigdy nie przejmuje zastanego katalogu.
    if !run_directory_was_missing && provisional.can_reclaim_bound_directory(&plan.id, &plan.dir) {
        provisional.owns_reclaimed_run_directory(plan.dir.clone());
    }
    Ok(())
}

/// Wspolny obraz wejscia dla krokow z wlasna kopia — albo `None`, gdy zaden go nie chce.
///
/// Pilnuje, ze bajty, od ktorych startuja wszystkie wlasne kopie tego biegu, sa JEDNE: nagrane,
/// odziedziczone po wznawianym biegu albo zdjete z projektu teraz — nigdy dwa zrodla naraz.
fn capture_the_starting_files(
    plan: &Plan,
    project: &Path,
    resumed_from: Option<&Path>,
) -> Result<Option<super::input_snapshot::InputSnapshot>, RunError> {
    /* WSPOLNY OBRAZ WEJSCIA POWSTAJE WYLACZNIE DLA KROKOW Z WLASNA KOPIA, wiec jego odmowa JEST
     * odmowa zrobienia tej kopii i ma niesc to samo zdanie, co odmowa z `make_or_recover_tree`.
     * `RunError::Io` jest przezroczysty: czlowiek czyta z niego „Permission denied (os error 13)"
     * — prawde o tym, co nie udalo sie SYSTEMOWI, bez nazwy wlasnego kafelka i bez powodu,
     * dla ktorego bieg w ogole stanal. */
    let wants_its_own_copy = plan
        .steps
        .iter()
        .find(|step| match &step.job {
            Job::Agent(job) => job.ours,
            Job::Check(job) => job.ours,
            Job::Serve(job) => job.ours,
            Job::Ask { .. } => false,
        })
        .map(|step| step.name.clone());
    Ok(if let Some(step_wanting_a_copy) = wants_its_own_copy {
        /* TU NIE MA PRZED CZYM BRONIC, a bramka sadzila kazdy zastany `work/`.
         *
         * `bound_prestart` ustawia sie dla KAZDEGO biegu triggera, wiec `run_directory_was_missing`
         * bywa falszywe rowniez wtedy, gdy zaden sterownik nigdy nie ruszyl. Zastana, polzapisana
         * kopia plikowa jest wtedy zwyklym stanem po awarii, a jej naprawe obiecuja wprost
         * `make_or_recover_file_copy` i `make_or_recover_git_tree` — odmowa odbierala im te prace
         * i konczyla bieg zdaniem, ktorego nikt w repo nie czytal.
         *
         * Glosna odmowa zostaje TAM, GDZIE BRONI PRACY: sciezka wznowienia ma wlasne, osobne
         * zdanie („The earlier run's saved input is unavailable"), ktorego to nie dotyka. */
        let captured = if let Some(replay) = plan
            .replay
            .as_ref()
            .filter(|replay| replay.mode == super::replay::ReplayMode::Recorded)
        {
            replay
                .input
                .as_ref()
                .ok_or_else(|| {
                    RunError::Io(io::Error::other(
                        "The recorded starting files are unavailable.",
                    ))
                })?
                .copy_to(&plan.dir)?
        } else if let Some(previous) = resumed_from {
            let original = super::input_snapshot::read(previous).map_err(|error| RunError::Io(
                io::Error::other(format!("The earlier run's saved input is unavailable: {error}. Start a new independent run."))
            ))?;
            let work_keys: BTreeSet<_> = plan
                .steps
                .iter()
                .filter_map(|step| match &step.job {
                    Job::Agent(job) if job.ours => Some(work_key_of(&step.node_key)),
                    Job::Check(job) if job.ours => Some(work_key_of(&step.node_key)),
                    Job::Serve(job) if job.ours => Some(work_key_of(&step.node_key)),
                    _ => None,
                })
                .collect();
            // Jedna kontynuowana kopia nie miesza wyników z nowym WIP gospodarza. Wiele
            // kopii zachowuje ostrożną odmowę, zanim zgadniemy, jak połączyć różne bazy.
            if work_keys.len() > 1 {
                let current_dir = tempfile::Builder::new()
                    .prefix(".current-input-")
                    .tempdir_in(&plan.dir)?;
                let current = super::input_snapshot::capture_selected(
                    project,
                    current_dir.path(),
                    original.additional_inputs(),
                )?;
                if current.entries() != original.entries()
                    || current.git_oid() != original.git_oid()
                {
                    return Err(RunError::Io(io::Error::other(
                        "The project changed since the earlier run. Start a new independent run; its results were not overwritten.",
                    )));
                }
            }
            original.copy_to(&plan.dir)?
        } else {
            super::input_snapshot::capture_selected(project, &plan.dir, &plan.additional_inputs)
                .map_err(|why| RunError::NoFreshCopy {
                    step: step_wanting_a_copy,
                    why: why.to_string(),
                })?
        };
        Some(captured)
    } else {
        None
    })
}

/// Katalog roboczy, ktory ten kafelek zaklada dla siebie — albo nic, gdy pracuje u gospodarza.
///
/// Krok „sprawdz" dostaje wlasne drzewo ta sama droga, co krok agenta, i to jest wymog,
/// nie symetria: `cargo test` pisze po `target/`, wiec „to tylko sprawdzenie" jest
/// nieprawda, a obietnica z ARCHITECTURE §2 p. 4 jest jedna dla wszystkich krokow.
fn the_folder_this_step_owns(job: &Job) -> Option<&PathBuf> {
    match job {
        Job::Agent(job) => job.ours.then_some(&job.cwd),
        Job::Check(job) => job.ours.then_some(&job.spec.cwd),
        // Kafelek „uruchom i zostaw" idzie tą samą drogą i z tego samego powodu. Gdyby jej
        // nie szedł, krok z własną kopią dostałby `cwd` w katalogu, którego nikt nie założył,
        // i odmówiłby na `os error 2` — czyli kontrolka „fresh copy" byłaby na tym kafelku
        // kontrolką, która psuje krok (niezmiennik 16).
        Job::Serve(job) => job.ours.then_some(&job.cwd),
        Job::Ask { .. } => None,
    }
}

/// Zaklada kazda wlasna kopie — po jednej na katalog roboczy — i mowi, co naprawde powstalo.
///
/// Odmowa jest tu GLOSNA i zatrzymuje bieg przed pierwszym procesem: kopia, ktorej nie udalo
/// sie zalozyc, znaczy dwa kroki piszace po tych samych plikach, a nie jeden krok wolniej.
fn isolate_every_folder(
    plan: &mut Plan,
    project: &Path,
    provisional: &mut ProvisionalRun,
    resumed_from: Option<&Path>,
    seeds: &super::workspace_inputs::Bound,
    input: Option<&super::input_snapshot::InputSnapshot>,
) -> Result<Vec<Isolated>, RunError> {
    let mut made: Vec<Isolated> = Vec::new();
    for (at, step) in plan.steps.iter().enumerate() {
        if let Some(cwd) = the_folder_this_step_owns(&step.job)
            && !made.iter().any(|one| one.cwd == *cwd)
        {
            // Klucz WSPÓLNEJ kopii nie jest kluczem kroku, który ją teraz zakłada.
            // Przy Just(reader) właściciel publisher nie biegnie, lecz zapisany wynik
            // nadal należy do publisher. Wybranie reader zgubiłoby historyczne bajty.
            let work_key = cwd
                .file_name()
                .and_then(|key| key.to_str())
                .ok_or_else(|| {
                    RunError::Io(io::Error::other(
                        "The managed working folder has no valid key.",
                    ))
                })?;
            let branch = isolate::branch_for(&plan.id, work_key);
            /* SKĄD ODBIJA SIĘ TO DRZEWO. Przy zwykłym biegu z `HEAD`; przy wznowieniu z gałęzi,
             * na której TEN KAFELEK skończył poprzednio. Powód stoi przy [`where_it_left_off`]
             * i jest z pomiaru, nie z symetrii. */
            let from = where_it_left_off(project, resumed_from, work_key, &plan.dir)?;
            if let Some(start) = &from {
                plan.starting_results
                    .insert(work_key.to_owned(), start.record());
            }
            let seed = plan
                .inputs
                .configuration
                .context_for(tile_key_of(&step.node_key))
                .and_then(|scope| scope.workspace_seed.as_ref())
                .and_then(|key| seeds.get(key));
            let origin = seed.map(|seed| &seed.input).or(input).ok_or_else(|| {
                RunError::Io(io::Error::other("The run's saved input is missing."))
            })?;
            if let Some(seed) = seed {
                plan.workspace_inputs
                    .insert(work_key.to_owned(), seed.clone());
            }
            let content = from.as_ref().and_then(CopyStart::files).unwrap_or(origin);
            if plan.inputs.configuration.isolate_contexts
                && from.as_ref().and_then(CopyStart::oid).is_some()
            {
                return Err(io::Error::other("This protected retry needs a saved file result. Start a new protected run from its saved input instead.").into());
            }
            // Odmowa jest GŁOŚNA i zatrzymuje bieg, zanim ruszy jakikolwiek proces. Ciche
            // zejście do wspólnego katalogu dałoby dwa kroki piszące po tych samych plikach,
            // z których każdy skończyłby się „sukcesem" (niezmiennik 12).
            let done = make_or_recover_tree(
                project,
                &plan.dir,
                cwd,
                &branch,
                from.as_ref().and_then(CopyStart::oid),
                CopyInput {
                    origin,
                    content,
                    file_only: plan.inputs.configuration.isolate_contexts,
                },
                provisional,
            )
            .map_err(|why| {
                // Zastany Bound katalog może zawierać marker lub drzewo, którego exact
                // tożsamość odmawia cleanupu. Ta odmowa musi zablokować także późniejsze
                // rekurencyjne usunięcie rodzica — inaczej bezpieczny child oracle byłby
                // obchodzony jednym `remove_dir_all` poziom wyżej.
                if !provisional.owns_work_path(cwd) {
                    provisional.block_reclaimed_parent_cleanup(&plan.dir);
                }
                RunError::NoFreshCopy {
                    step: step.name.clone(),
                    why: why.to_string(),
                }
            })?;
            if from.is_some()
                && let Some(previous) = resumed_from
            {
                carry_previous_import(previous, &plan.dir, work_key, origin)?;
            }
            made.push(Isolated {
                copy_identity: supervisor::PublicationRoot::open(cwd)?.identity(),
                step: step.name.clone(),
                at,
                cwd: cwd.clone(),
                branch: match done.how {
                    isolate::How::Tree { branch } => Some(branch),
                    isolate::How::Copy => None,
                },
                left_behind: done.left_behind,
            });
            if made.len() == 1 {
                provisional.check(PrestartFaultPoint::AfterFirstIsolation)?;
            }
            if made.len() == 2 {
                provisional.check(PrestartFaultPoint::AfterSecondIsolation)?;
            }
        }
    }
    Ok(made)
}

/// Tworzy katalog biegu i to, co do niego nalezy — **dopiero po planie**.
fn lay_out_the_run_dir(
    plan: &mut Plan,
    project: &Path,
    provisional: &mut ProvisionalRun,
) -> Result<Vec<Isolated>, RunError> {
    claim_the_run_directory(plan, project, provisional)?;
    /* `handoffs_from` NIESIE DWA FAKTY NARAZ: skąd skopiować przekazania i który bieg wznowić.
     *
     * Katalog BEZ `run.json` nie jest biegiem — nie ma tam wyniku kopii, którego moglibyśmy nie
     * znaleźć, więc nie ma też czego bronić odmową. Głośne wznowienie broni biegu, który
     * ISTNIEJE, a jego zapisanego wejścia brakuje; „nie było poprzedniego biegu" zostaje stanem
     * zwykłym, a nie awarią. */
    let resumed_from: Option<PathBuf> = plan
        .seeded_from
        .clone()
        .filter(|previous| previous.join(RUN_FILE).exists());
    let input = capture_the_starting_files(plan, project, resumed_from.as_deref())?;
    /* JEDEN KATALOG ROBOCZY POWSTAJE RAZ. Rundy petli dziela katalog -- musza, bo inaczej runda 2
     * nie widzi poprawek rundy 1 -- wiec bez tego zbioru zakladalibysmy drzewo N razy w tym samym
     * miejscu, a `git worktree add` odmawia na istniejacym katalogu. */
    let seed_source = plan
        .replay
        .as_ref()
        .filter(|replay| replay.mode == super::replay::ReplayMode::Recorded)
        .map(|replay| replay.source_dir.as_path())
        .or(resumed_from.as_deref());
    let needed_seeds = plan
        .steps
        .iter()
        .filter_map(|step| {
            plan.inputs
                .configuration
                .context_for(tile_key_of(&step.node_key))
        })
        .filter_map(|scope| scope.workspace_seed.as_deref())
        .collect();
    let seeds = super::workspace_inputs::prepare(
        project,
        &plan.dir,
        &plan.inputs.configuration,
        &needed_seeds,
        seed_source,
    )?;
    let made = isolate_every_folder(
        plan,
        project,
        provisional,
        resumed_from.as_deref(),
        &seeds,
        input.as_ref(),
    )?;
    plan.input_snapshot = input;
    Ok(made)
}

/// 2026-09-06: wznowiony wynik konsumenta zawiera jego własne poprawki. Bez poprzedniej
/// delty importer porównywał je z origin i odmawiał nawet niezmienionych wejść rodziców.
/// Przenosimy bazę porównania, NIE zgodę na przygotowanie konkretnego kroku nowego biegu.
fn carry_previous_import(
    previous: &Path,
    run: &Path,
    key: &str,
    origin: &super::input_snapshot::InputSnapshot,
) -> Result<(), RunError> {
    refuse_incomplete_input(previous, key)?;
    let old = read_isolation_marker(&previous.join(ISOLATION_MARKERS_DIR).join(key))
        .map_err(io::Error::other)?;
    let Some(mut prepared) = old.as_ref().and_then(IsolationMarker::fan_in).cloned() else {
        return Ok(());
    };
    if prepared.origin != origin.id() {
        return Err(io::Error::other("The previous combined input belongs to a different saved starting copy. Nothing was overwritten.").into());
    }
    let path = run.join(ISOLATION_MARKERS_DIR).join(key);
    let mut marker = read_isolation_marker(&path)
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::other("The new working copy's ownership is missing."))?;
    if marker.fan_in().is_some() {
        return Ok(());
    }
    prepared.consumer.clear();
    prepared.parent_steps.clear();
    prepared.parents.clear();
    prepared.plan_digest.clear();
    marker.set_fan_in(prepared);
    write_isolation_marker(&path, &marker).map_err(io::Error::other)?;
    Ok(())
}

/// Powtarza layout po awarii miedzy `bind` i pierwszym `run.json`.
///
/// W tym oknie sterownik jeszcze nie ruszyl, wiec katalog kopii nie niesie pracy agenta.
/// Worktree gita juz niesie natomiast naniesiony diff czlowieka: jego nie wolno skasowac ani
/// nakladac drugi raz, dlatego wraca tylko po dowodzie oczekiwanej galezi.
/// Gałąź, na której ta kopia kafelka skończyła w poprzednim biegu — albo `None`.
///
/// # Po co to istnieje
///
/// 2026-08-23, zmierzone na biegu właściciela na `urc-monorepo`. Wznowienie z historii niosło
/// przekazania poprzedniego biegu i **nie niosło jego pracy**: świeża kopia powstawała z `HEAD`,
/// więc krok „Front" dostał czysty checkout i zaczął od zera pisać 164 pliki, które poprzedni
/// bieg zacommitował na `loadout/01a02b3c…/s_6` jako `21ad1c94`. Sędzia obok, pracujący w tej
/// samej kopii, orzekał na pustym drzewie i napisał uczciwie: *„Brak katalogu `.claude/tmp/`
/// z artefaktami zadania — nie mam czego porównywać"*.
///
/// # Po KLUCZU PRACY, nie po nazwie gałęzi z tamtego biegu
///
/// Bo klucz pracy jest tym, co rozróżnia równoległe kopie jednego kafelka. `branch_for` składa
/// nazwę z identyfikatora biegu i tego klucza, więc pytanie „gdzie ta kopia skończyła ostatnio"
/// ma dokładnie jedną odpowiedź, a składamy ją tą samą funkcją, która tamtą nazwę nadała
/// (niezmiennik 13).
///
/// WF-01 (2026-09-05): brak wyniku nie oznacza HEAD. Pusta, poprawnie domknięta kopia
/// ma zapisany dokładny commit, mimo że jej tymczasowa gałąź została już usunięta.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StartingResult {
    Git {
        oid: String,
    },
    Folder {
        manifest: PathBuf,
        snapshot: String,
        origin: String,
    },
    Unchanged {
        origin: String,
    },
}

enum CopyStart {
    Git(String),
    Folder {
        input: super::input_snapshot::InputSnapshot,
        manifest: PathBuf,
        origin: String,
    },
    Unchanged(String),
}

impl CopyStart {
    fn oid(&self) -> Option<&str> {
        match self {
            Self::Git(oid) => Some(oid),
            _ => None,
        }
    }
    fn files(&self) -> Option<&super::input_snapshot::InputSnapshot> {
        match self {
            Self::Folder { input, .. } => Some(input),
            _ => None,
        }
    }
    fn record(&self) -> StartingResult {
        match self {
            Self::Git(oid) => StartingResult::Git { oid: oid.clone() },
            Self::Folder {
                input,
                manifest,
                origin,
            } => StartingResult::Folder {
                manifest: manifest.clone(),
                snapshot: input.id().to_owned(),
                origin: origin.clone(),
            },
            Self::Unchanged(origin) => StartingResult::Unchanged {
                origin: origin.clone(),
            },
        }
    }
}

#[derive(Clone, Copy)]
struct CopyInput<'a> {
    origin: &'a super::input_snapshot::InputSnapshot,
    content: &'a super::input_snapshot::InputSnapshot,
    file_only: bool,
}

fn where_it_left_off(
    project: &Path,
    previous: Option<&Path>,
    work_key: &str,
    run_dir: &Path,
) -> Result<Option<CopyStart>, RunError> {
    let Some(previous) = previous else {
        return Ok(None);
    };
    refuse_incomplete_input(previous, work_key)?;
    let held = supervisor::PublicationRoot::open(previous)?;
    let file = held.open_regular_file(Path::new(RUN_FILE))?;
    let mut bytes = Vec::new();
    file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(io::Error::other("The earlier run description is too large.").into());
    }
    let described: Value = serde_json::from_slice(&bytes)?;
    described
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        .ok_or_else(|| io::Error::other("The earlier run's identity is missing or invalid."))?;
    let input = super::input_snapshot::read(previous)?;
    if described
        .pointer("/input_snapshot/id")
        .and_then(Value::as_str)
        != Some(input.id())
    {
        return Err(io::Error::other(
            "The earlier run's input no longer matches its saved description.",
        )
        .into());
    }
    held.validate_path_identity(previous)?;
    let input = super::workspace_inputs::for_copy(previous, work_key)?;
    let saved = described
        .get("copy_results")
        .and_then(|results| results.get(work_key))
        .ok_or_else(|| {
            io::Error::other(format!(
                "The earlier result for copy {work_key} was not saved for reuse."
            ))
        })?;
    let result: SavedCopy = serde_json::from_value(saved.clone())?;
    if !matches!(result, SavedCopy::Git { .. }) {
        if let SavedCopy::Unchanged { origin } = &result {
            if origin != input.id() {
                return Err(io::Error::other(
                    "The unchanged result names a different saved input.",
                )
                .into());
            }
            return Ok(Some(CopyStart::Unchanged(origin.clone())));
        }
        let saved = read_saved_folder(previous, work_key, &result)?;
        // WF-01/06: wcześniejszy folder może zostać usunięty przez człowieka po Starcie.
        // Nowy bieg utrwala własne bajty KAŻDEGO wyniku przed uruchomieniem agentów;
        // jego wspólny origin pozostaje osobny i nadal jest bazą łączenia wyników.
        let relative = PathBuf::from("starting").join(work_key);
        let target = run_dir.join(&relative);
        supervisor::PublicationRoot::open(run_dir)?.ensure_directory(&relative, 0o700)?;
        let frozen = super::input_snapshot::capture_saved_result(&saved.path, &target)?;
        if frozen.entries() != &saved.entries {
            return Err(io::Error::other(
                "The earlier result changed while it was being prepared. Nothing started.",
            )
            .into());
        }
        return Ok(Some(CopyStart::Folder {
            input: frozen,
            manifest: relative.join("input/manifest.json"),
            origin: saved.origin,
        }));
    }
    let SavedCopy::Git { oid } = result else {
        return Err(RunError::Io(io::Error::other(format!(
            "The earlier result for copy {work_key} is unavailable."
        ))));
    };
    if !matches!(oid.len(), 40 | 64)
        || !oid.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !isolate::names_a_commit(project, &oid)
    {
        return Err(RunError::Io(io::Error::other(format!(
            "The saved commit for copy {work_key} is missing or invalid."
        ))));
    }
    Ok(Some(CopyStart::Git(oid)))
}

fn make_or_recover_tree(
    project: &Path,
    run_dir: &Path,
    cwd: &Path,
    branch: &str,
    from: Option<&str>,
    input: CopyInput<'_>,
    provisional: &mut ProvisionalRun,
) -> Result<isolate::Made, isolate::Trouble> {
    // Walidacja stoi przed `exists`, `make` i `remove_dir_all`: inaczej niebezpieczny klucz
    // albo symlink przodka moze wskazac ofiare poza biegiem, zanim cleanup zobaczy cel.
    let marker_path = prove_generated_work_path(project, run_dir, cwd)?;
    let marker = read_isolation_marker(&marker_path)?;
    if input.file_only || !isolate::is_a_repo(project) {
        // 2026-08-28 (T-152 review) — marker Gita dowodzi, że katalog pochodzi z wcześniejszej
        // próby. Odmowa musi więc poprzedzać przejęcie ownershipu; inaczej Drop skasowałby WIP
        // odzyskanego worktree jako rzekomą kopię plikową tej próby.
        if marker.as_ref().is_some_and(|one| {
            !matches!(one,
            IsolationMarker::FileCopy { origin_snapshot: Some(origin), fan_in: None, native_skills: None }
            if origin == input.origin.id())
        }) {
            return Err(isolate::Trouble::Git(
                "the retry found an isolation record from a different input or repository"
                    .to_owned(),
            ));
        }
        // Kopiowanie może utworzyć część `cwd`, zanim zwróci błąd. Po odrzuceniu cudzego markera
        // i dowodzie ścieżki guard musi więc przejąć cel przed pierwszym zapisem; cleanup braku
        // ścieżki jest idempotentny.
        provisional.owns_file_copy(cwd.to_path_buf());
        let made = make_or_recover_file_copy(cwd, input.content)?;
        write_isolation_marker(
            &marker_path,
            &IsolationMarker::FileCopy {
                origin_snapshot: Some(input.origin.id().to_owned()),
                fan_in: None,
                native_skills: None,
            },
        )?;
        return Ok(made);
    }
    make_or_recover_git_tree(
        project,
        cwd,
        branch,
        from,
        input.origin,
        IsolationRecord {
            path: &marker_path,
            marker: marker.as_ref(),
        },
        provisional,
    )
}

fn prove_run_candidate(project: &Path, run_dir: &Path) -> Result<(), isolate::Trouble> {
    let runs_root = project.join(PROJECT_DIR).join(RUNS_DIR);
    require_one_normal_child_for(&runs_root, run_dir, unsafe_run_path)?;
    let canonical_runs = prove_generated_runs_root(project)?;
    match fs::symlink_metadata(run_dir) {
        Ok(_) => {
            let canonical_run =
                prove_real_child_for(run_dir, &canonical_runs, false, unsafe_run_path)?;
            prove_reserved_run_files(run_dir)?;
            prove_existing_run_child(&run_dir.join(LOGS_DIR), &canonical_run)?;
            prove_run_artifact_tree(run_dir, &canonical_run)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(isolate::Trouble::Copying(error)),
    }
    Ok(())
}

/// Otwiera, czyta i fsyncuje dokladnie ten `run.json`, ktory przeszedl dowod wygenerowanej
/// sciezki, a potem fsyncuje jego realny katalog. Recovery waliduje zwrocone stad bajty, wiec
/// nie moze zaakceptowac innego odczytu niz ten, ktory stal sie trwaly.
fn read_and_sync_run_file(project: &Path, run_file: &Path) -> io::Result<Option<Vec<u8>>> {
    let run_dir = run_file
        .parent()
        .ok_or_else(|| io::Error::other("the run file has no parent directory"))?;
    if run_file != run_dir.join(RUN_FILE) {
        return Err(io::Error::other(
            "the durable run file is not the exact generated run.json",
        ));
    }
    prove_run_candidate(project, run_dir)
        .map_err(|problem| io::Error::other(problem.to_string()))?;
    let directory = match fs::File::open(run_dir) {
        Ok(directory) => directory,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let directory_path_metadata = fs::symlink_metadata(run_dir)?;
    if !directory_path_metadata.file_type().is_dir() || !directory.metadata()?.file_type().is_dir()
    {
        return Err(io::Error::other(
            "the durable run directory is not a real directory",
        ));
    }
    let mut file = match OpenOptions::new().read(true).open(run_file) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    // Otwieramy tylko do odczytu, po czym ponownie pytamy o ostatni komponent: przygotowany albo
    // pozostawiony pod nazwa link jest odmowa. Atomowe no-follow wobec aktywnego swapu wymaga
    // platformowego open w `supervisor.rs`, poza OWNS T-65; ten helper nie udaje takiej gwarancji.
    let path_metadata = fs::symlink_metadata(run_file)?;
    if !path_metadata.file_type().is_file() || !file.metadata()?.file_type().is_file() {
        return Err(io::Error::other(
            "the durable run file is not a regular file",
        ));
    }
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    file.sync_all()?;
    directory.sync_all()?;
    Ok(Some(raw))
}

fn prove_existing_run_child(path: &Path, canonical_run: &Path) -> Result<(), isolate::Trouble> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            prove_real_child_for(path, canonical_run, false, unsafe_run_path)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(isolate::Trouble::Copying(error)),
    }
    Ok(())
}

fn prepare_run_directory(project: &Path, run_dir: &Path) -> Result<(), isolate::Trouble> {
    let runs_root = project.join(PROJECT_DIR).join(RUNS_DIR);
    require_one_normal_child_for(&runs_root, run_dir, unsafe_run_path)?;
    let canonical_runs = prove_generated_runs_root(project)?;
    let canonical_run = prove_real_child_for(run_dir, &canonical_runs, true, unsafe_run_path)?;
    prove_reserved_run_files(run_dir)?;
    prove_run_artifact_tree(run_dir, &canonical_run)?;
    // `logs/` istnieje od poczatku, ale tylko jako realny potomek dowiedzionego katalogu biegu.
    prove_real_child_for(
        &run_dir.join(LOGS_DIR),
        &canonical_run,
        true,
        unsafe_run_path,
    )?;
    Ok(())
}

/// Dowodzi istniejacych artefaktow przed pierwszym zapisem/driverem bez kopiowania listy ich
/// nazw. `work/` i `.isolation/` sa wyjatkami rekurencji: prawdziwy worktree moze zawierac
/// symlinki projektu, a oba korzenie, wybrane cwd i marker maja osobny, scislejszy protokol
/// izolacji ponizej.
fn prove_run_artifact_tree(run_dir: &Path, canonical_run: &Path) -> Result<(), isolate::Trouble> {
    let work_root = run_dir.join(WORK_DIR);
    let marker_root = run_dir.join(ISOLATION_MARKERS_DIR);
    let mut directories = vec![(run_dir.to_path_buf(), canonical_run.to_path_buf())];
    while let Some((directory, canonical_directory)) = directories.pop() {
        for entry in fs::read_dir(&directory).map_err(isolate::Trouble::Copying)? {
            let entry = entry.map_err(isolate::Trouble::Copying)?;
            let path = entry.path();
            let kind = entry.file_type().map_err(isolate::Trouble::Copying)?;
            if kind.is_symlink() {
                return Err(unsafe_run_path());
            }
            let canonical = fs::canonicalize(&path).map_err(isolate::Trouble::Copying)?;
            if canonical.parent() != Some(canonical_directory.as_path()) {
                return Err(unsafe_run_path());
            }
            if kind.is_dir() {
                if path != work_root && path != marker_root {
                    directories.push((path, canonical));
                }
            } else if !kind.is_file() {
                return Err(unsafe_run_path());
            }
        }
    }
    Ok(())
}

fn prove_generated_runs_root(project: &Path) -> Result<PathBuf, isolate::Trouble> {
    let canonical_project = fs::canonicalize(project).map_err(isolate::Trouble::Copying)?;
    let loadout_root = project.join(PROJECT_DIR);
    let canonical_loadout =
        prove_real_child_for(&loadout_root, &canonical_project, true, unsafe_run_path)?;
    prove_real_child_for(
        &loadout_root.join(RUNS_DIR),
        &canonical_loadout,
        true,
        unsafe_run_path,
    )
}

fn prove_reserved_run_files(run_dir: &Path) -> Result<(), isolate::Trouble> {
    match fs::symlink_metadata(run_dir.join(RUN_FILE)) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Err(unsafe_run_path()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(isolate::Trouble::Copying(error)),
    }
    match fs::symlink_metadata(run_dir.join(RUN_FILE_WRITING)) {
        Ok(_) => {
            return Err(isolate::Trouble::Copying(io::Error::other(
                "the run file staging path is already occupied; nothing ran",
            )));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(isolate::Trouble::Copying(error)),
    }
    Ok(())
}

fn prove_generated_work_path(
    project: &Path,
    run_dir: &Path,
    cwd: &Path,
) -> Result<PathBuf, isolate::Trouble> {
    let loadout_root = project.join(PROJECT_DIR);
    let runs_root = loadout_root.join(RUNS_DIR);
    let work_root = run_dir.join(WORK_DIR);
    require_one_normal_child(&runs_root, run_dir)?;
    require_one_normal_child(&work_root, cwd)?;

    // `project` jest wyborem czlowieka i moze sam byc otwarty przez symlink. Od pierwszego
    // katalogu tworzonego przez Loadout kazdy poziom musi jednak byc realnym katalogiem,
    // a kanoniczny rodzic musi byc dokladnie poprzednim, juz dowiedzionym poziomem.
    let canonical_project = fs::canonicalize(project).map_err(isolate::Trouble::Copying)?;
    let canonical_loadout = prove_real_child(&loadout_root, &canonical_project, false)?;
    let canonical_runs = prove_real_child(&runs_root, &canonical_loadout, false)?;
    let canonical_run_dir = prove_real_child(run_dir, &canonical_runs, false)?;
    let canonical_work = prove_real_child(&work_root, &canonical_run_dir, true)?;
    match fs::symlink_metadata(cwd) {
        Ok(_) => {
            prove_real_child(cwd, &canonical_work, false)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(isolate::Trouble::Copying(error)),
    }
    let marker_root = run_dir.join(ISOLATION_MARKERS_DIR);
    prove_real_child(&marker_root, &canonical_run_dir, true)?;
    let name = cwd.file_name().ok_or_else(unsafe_work_path)?;
    let marker_path = marker_root.join(name);
    match fs::symlink_metadata(&marker_path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Err(unsafe_work_path()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(isolate::Trouble::Copying(error)),
    }
    Ok(marker_path)
}

fn require_one_normal_child(parent: &Path, child: &Path) -> Result<(), isolate::Trouble> {
    require_one_normal_child_for(parent, child, unsafe_work_path)
}

fn require_one_normal_child_for(
    parent: &Path,
    child: &Path,
    problem: fn() -> isolate::Trouble,
) -> Result<(), isolate::Trouble> {
    let relative = child.strip_prefix(parent).map_err(|_| problem())?;
    let mut components = relative.components();
    if matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
    {
        Ok(())
    } else {
        Err(problem())
    }
}

fn prove_real_child(
    path: &Path,
    expected_parent: &Path,
    create: bool,
) -> Result<PathBuf, isolate::Trouble> {
    prove_real_child_for(path, expected_parent, create, unsafe_work_path)
}

fn prove_real_child_for(
    path: &Path,
    expected_parent: &Path,
    create: bool,
    problem: fn() -> isolate::Trouble,
) -> Result<PathBuf, isolate::Trouble> {
    if create {
        match fs::symlink_metadata(path) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(path).map_err(isolate::Trouble::Copying)?;
            }
            Err(error) => return Err(isolate::Trouble::Copying(error)),
        }
    }
    let metadata = fs::symlink_metadata(path).map_err(isolate::Trouble::Copying)?;
    if !metadata.file_type().is_dir() {
        return Err(problem());
    }
    let actual = fs::canonicalize(path).map_err(isolate::Trouble::Copying)?;
    if actual.parent() != Some(expected_parent) {
        return Err(problem());
    }
    Ok(actual)
}

fn unsafe_work_path() -> isolate::Trouble {
    isolate::Trouble::Copying(io::Error::other(
        "the step's file-copy path crosses a link or leaves this run's folders",
    ))
}

fn unsafe_run_path() -> isolate::Trouble {
    isolate::Trouble::Copying(io::Error::other(
        "the run path crosses a link or leaves Loadout's run folders; nothing ran",
    ))
}

fn make_or_recover_file_copy(
    cwd: &Path,
    snapshot: &super::input_snapshot::InputSnapshot,
) -> Result<isolate::Made, isolate::Trouble> {
    if !path_entry_exists(cwd)? {
        snapshot
            .materialize(cwd)
            .map_err(isolate::Trouble::Copying)?;
        return Ok(isolate::Made {
            how: isolate::How::Copy,
            left_behind: snapshot.left_behind().to_vec(),
        });
    }
    if !cwd.is_dir() {
        return Err(isolate::Trouble::Copying(io::Error::other(
            "the retry found an unexpected path where its file copy belongs",
        )));
    }
    // 2026-08-21, T-65: brak `run.json` dowodzi, ze zaden driver nie wystartowal. Usuwamy
    // wylacznie wygenerowana, potencjalnie polowiczna kopie pod tym samym katalogiem biegu.
    fs::remove_dir_all(cwd).map_err(isolate::Trouble::Copying)?;
    snapshot
        .materialize(cwd)
        .map_err(isolate::Trouble::Copying)?;
    Ok(isolate::Made {
        how: isolate::How::Copy,
        left_behind: snapshot.left_behind().to_vec(),
    })
}

#[derive(Clone, Copy)]
struct IsolationRecord<'a> {
    path: &'a Path,
    marker: Option<&'a IsolationMarker>,
}

fn make_or_recover_git_tree(
    project: &Path,
    cwd: &Path,
    branch: &str,
    from: Option<&str>,
    snapshot: &super::input_snapshot::InputSnapshot,
    record: IsolationRecord<'_>,
    provisional: &mut ProvisionalRun,
) -> Result<isolate::Made, isolate::Trouble> {
    let IsolationRecord {
        path: marker_path,
        marker,
    } = record;
    if let Some(marked) = marker
        && marked.branch() != Some(branch)
    {
        return Err(isolate::Trouble::Git(
            "the isolation record names a different branch; nothing was removed".to_owned(),
        ));
    }

    if path_entry_exists(cwd)? {
        if !worktree_points_at(project, cwd, branch) {
            return Err(isolate::Trouble::Git(
                "the retry found a different work tree at the run's reserved path; nothing was removed"
                    .to_owned(),
            ));
        }
        let head = branch_oid(project, branch)?;
        match marker {
            Some(IsolationMarker::Complete { head: expected, .. }) if expected == &head => {
                if !matches!(marker, Some(IsolationMarker::Complete { origin_snapshot: Some(origin), .. }) if origin == snapshot.id())
                    || marker.is_some_and(IsolationMarker::has_incomplete_input)
                {
                    return Err(isolate::Trouble::Git(
                        "the copy does not have the same completed saved input; nothing was removed".to_owned(),
                    ));
                }
                if provisional.owns_directory_containing_marker(marker_path) {
                    provisional.owns_git_tree(
                        cwd.to_path_buf(),
                        branch.to_owned(),
                        head,
                        marker_path.to_path_buf(),
                    );
                }
                return Ok(isolate::Made {
                    how: isolate::How::Tree {
                        branch: branch.to_owned(),
                    },
                    // Ostrzezenia policzyl pierwszy layout. Marker dowodzi, ze `git apply` i
                    // liczenie brakow doszly do konca; ponowienie diffu podwoiloby zmiany.
                    left_behind: Vec::new(),
                });
            }
            Some(IsolationMarker::Complete { .. }) => {
                return Err(isolate::Trouble::Git(
                    "the completed work tree moved to a different commit; nothing was removed"
                        .to_owned(),
                ));
            }
            Some(IsolationMarker::Recovering { head: expected, .. }) if expected == &head => {}
            Some(IsolationMarker::Recovering { .. }) => {
                return Err(isolate::Trouble::Git(
                    "the work tree changed after recovery began; nothing was removed".to_owned(),
                ));
            }
            Some(IsolationMarker::FileCopy { .. }) => {
                return Err(isolate::Trouble::Git(
                    "the copy is not a git work tree; nothing was removed".to_owned(),
                ));
            }
            None => {
                // Ten fsync jest PRZED pierwszym skutkiem cleanup. Po awarii marker jest
                // uprawnieniem wylacznie do tej sciezki, galezi i tego niezmienionego OID.
                write_isolation_marker(
                    marker_path,
                    &IsolationMarker::Recovering {
                        branch: branch.to_owned(),
                        head,
                        origin_snapshot: Some(snapshot.id().to_owned()),
                        fan_in: None,
                        native_skills: None,
                    },
                )?;
            }
        }
        cleanup_incomplete_worktree(project, cwd, branch, marker_path)?;
    } else {
        recover_missing_worktree(project, cwd, branch, snapshot, record)?;
    }

    make_new_git_tree(
        project,
        cwd,
        branch,
        from,
        snapshot,
        marker_path,
        provisional,
    )
}

fn recover_missing_worktree(
    project: &Path,
    cwd: &Path,
    branch: &str,
    snapshot: &super::input_snapshot::InputSnapshot,
    record: IsolationRecord<'_>,
) -> Result<(), isolate::Trouble> {
    let IsolationRecord {
        path: marker_path,
        marker,
    } = record;
    match marker {
        Some(IsolationMarker::Recovering { .. }) => {
            cleanup_incomplete_worktree(project, cwd, branch, marker_path)?;
        }
        Some(IsolationMarker::Complete { .. }) => {
            return Err(isolate::Trouble::Git(
                "the completed work tree is missing; nothing was removed".to_owned(),
            ));
        }
        Some(IsolationMarker::FileCopy { .. }) => {
            return Err(isolate::Trouble::Git(
                "the copy is not a git work tree; nothing was removed".to_owned(),
            ));
        }
        None if branch_exists(project, branch)? => {
            // Naturalne okno awarii: `git worktree add` zdazyl zapisac branch i admin,
            // katalog cwd fizycznie zniknal, a marker nie powstal. Prealokowana sciezka,
            // branch i OID musza wskazac jeden prunable record; dopiero potem fsyncujemy
            // Recovering i wchodzimy do tego samego idempotentnego cleanupu.
            let head = branch_oid(project, branch)?;
            match expected_worktree_admin(project, cwd, branch, &head)? {
                ExpectedWorktreeAdmin::Present { prunable: true } => {
                    write_isolation_marker(
                        marker_path,
                        &IsolationMarker::Recovering {
                            branch: branch.to_owned(),
                            head,
                            origin_snapshot: Some(snapshot.id().to_owned()),
                            fan_in: None,
                            native_skills: None,
                        },
                    )?;
                    cleanup_incomplete_worktree(project, cwd, branch, marker_path)?;
                }
                ExpectedWorktreeAdmin::Present { prunable: false } => {
                    return Err(isolate::Trouble::Git(
                        "the missing work tree is not marked removable by git; nothing was removed"
                            .to_owned(),
                    ));
                }
                ExpectedWorktreeAdmin::Absent => {
                    return Err(isolate::Trouble::Git(
                            "the recovery branch has no matching work tree administration; nothing was removed"
                                .to_owned(),
                        ));
                }
            }
        }
        None => {}
    }
    Ok(())
}

fn make_new_git_tree(
    project: &Path,
    cwd: &Path,
    branch: &str,
    from: Option<&str>,
    snapshot: &super::input_snapshot::InputSnapshot,
    marker_path: &Path,
    provisional: &mut ProvisionalRun,
) -> Result<isolate::Made, isolate::Trouble> {
    let mut added_head = None;
    let made =
        isolate::make_from_snapshot_after_add(project, cwd, branch, from, snapshot, |head| {
            added_head = Some(head.to_owned());
            provisional.owns_git_tree(
                cwd.to_path_buf(),
                branch.to_owned(),
                head.to_owned(),
                marker_path.to_path_buf(),
            );
            provisional
                .faults
                .check(PrestartFaultPoint::AfterWorktreeAdd)
        })?;
    if matches!(&made.how, isolate::How::Tree { .. }) {
        let head = added_head.ok_or_else(|| {
            isolate::Trouble::Git(
                "git created a work tree without returning its exact commit".to_owned(),
            )
        })?;
        // Dopiero caly `isolate::make` (worktree, dirty diff i lista brakow) moze wystawic
        // marker. Fsync pliku i katalogu stoi w helperze przed zwrotem do layoutu.
        write_isolation_marker(
            marker_path,
            &IsolationMarker::Complete {
                branch: branch.to_owned(),
                head,
                origin_snapshot: Some(snapshot.id().to_owned()),
                fan_in: None,
                native_skills: None,
            },
        )?;
    }
    Ok(made)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum IsolationMarker {
    Complete {
        branch: String,
        head: String,
        #[serde(
            default,
            rename = "originSnapshot",
            skip_serializing_if = "Option::is_none"
        )]
        origin_snapshot: Option<String>,
        #[serde(default, rename = "fanIn", skip_serializing_if = "Option::is_none")]
        fan_in: Option<FanInPreparation>,
        #[serde(
            default,
            rename = "nativeSkills",
            skip_serializing_if = "Option::is_none"
        )]
        native_skills: Option<crate::skills::bundle::native::NativeDelivery>,
    },
    Recovering {
        branch: String,
        head: String,
        #[serde(
            default,
            rename = "originSnapshot",
            skip_serializing_if = "Option::is_none"
        )]
        origin_snapshot: Option<String>,
        #[serde(default, rename = "fanIn", skip_serializing_if = "Option::is_none")]
        fan_in: Option<FanInPreparation>,
        #[serde(
            default,
            rename = "nativeSkills",
            skip_serializing_if = "Option::is_none"
        )]
        native_skills: Option<crate::skills::bundle::native::NativeDelivery>,
    },
    FileCopy {
        #[serde(
            default,
            rename = "originSnapshot",
            skip_serializing_if = "Option::is_none"
        )]
        origin_snapshot: Option<String>,
        #[serde(default, rename = "fanIn", skip_serializing_if = "Option::is_none")]
        fan_in: Option<FanInPreparation>,
        #[serde(
            default,
            rename = "nativeSkills",
            skip_serializing_if = "Option::is_none"
        )]
        native_skills: Option<crate::skills::bundle::native::NativeDelivery>,
    },
}

/// WF-03: dwa fakty przygotowania istniejącej kopii, nie osobny ledger wykonania grafu.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FanInPreparation {
    work_key: String,
    origin: String,
    parents: Vec<(PathBuf, String)>,
    plan_digest: String,
    input_ready: bool,
    /// 2026-09-06: folder przeżywa rundę. Tożsamość i delta importu pozwalają odświeżyć
    /// następne wejście bez potraktowania własnej pracy konsumenta jako plików rodzica.
    #[serde(default)]
    consumer: String,
    #[serde(default)]
    parent_steps: Vec<Option<String>>,
    #[serde(default)]
    imported: Option<BTreeMap<PathBuf, Option<super::input_snapshot::Entry>>>,
}

impl IsolationMarker {
    fn branch(&self) -> Option<&str> {
        match self {
            Self::Complete { branch, .. } | Self::Recovering { branch, .. } => Some(branch),
            Self::FileCopy { .. } => None,
        }
    }

    /// Dokładny commit, z którego to drzewo powstało.
    ///
    /// 2026-09-03 (Z-7) — CZYTA GO ZAMKNIĘCIE BIEGU. „Czy krok sam zacommitował swoją pracę" to
    /// pytanie o commity ponad punktem startu, a punkt startu wznowionego kroku jest gałęzią
    /// poprzedniego biegu, nie `HEAD` projektu. Marker jest jedynym miejscem, w którym ten OID
    /// stoi zapisany — i tym samym, którego pilnuje odzyskiwanie po przerwanej próbie.
    fn head(&self) -> Option<&str> {
        match self {
            Self::Complete { head, .. } | Self::Recovering { head, .. } => Some(head),
            Self::FileCopy { .. } => None,
        }
    }

    fn fan_in(&self) -> Option<&FanInPreparation> {
        match self {
            Self::Complete { fan_in, .. }
            | Self::Recovering { fan_in, .. }
            | Self::FileCopy { fan_in, .. } => fan_in.as_ref(),
        }
    }

    fn set_fan_in(&mut self, value: FanInPreparation) {
        match self {
            Self::Complete { fan_in, .. }
            | Self::Recovering { fan_in, .. }
            | Self::FileCopy { fan_in, .. } => *fan_in = Some(value),
        }
    }

    fn has_incomplete_input(&self) -> bool {
        self.fan_in().is_some_and(|one| !one.input_ready)
            || self.native_skills().is_some_and(|one| !one.complete)
    }

    fn native_skills(&self) -> Option<&crate::skills::bundle::native::NativeDelivery> {
        match self {
            Self::Complete { native_skills, .. }
            | Self::Recovering { native_skills, .. }
            | Self::FileCopy { native_skills, .. } => native_skills.as_ref(),
        }
    }

    fn set_native_skills(&mut self, value: crate::skills::bundle::native::NativeDelivery) {
        match self {
            Self::Complete { native_skills, .. }
            | Self::Recovering { native_skills, .. }
            | Self::FileCopy { native_skills, .. } => *native_skills = Some(value),
        }
    }

    fn origin_snapshot(&self) -> Option<&str> {
        match self {
            Self::Complete {
                origin_snapshot, ..
            }
            | Self::Recovering {
                origin_snapshot, ..
            }
            | Self::FileCopy {
                origin_snapshot, ..
            } => origin_snapshot.as_deref(),
        }
    }

    fn matches(&self, branch: &str, head: &str) -> bool {
        match self {
            Self::Complete {
                branch: marked_branch,
                head: marked_head,
                ..
            }
            | Self::Recovering {
                branch: marked_branch,
                head: marked_head,
                ..
            } => marked_branch == branch && marked_head == head,
            Self::FileCopy { .. } => false,
        }
    }
}

/// Zapis native delivery korzysta z markera kopii, nie z drugiego rejestru własności.
fn native_marker_path(run_dir: &Path, cwd: &Path) -> io::Result<Option<PathBuf>> {
    let Ok(relative) = cwd.strip_prefix(run_dir.join(WORK_DIR)) else {
        return Ok(None);
    };
    if relative.components().count() != 1
        || !relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    {
        return Err(io::Error::other(
            "The native skill folder has no valid working-copy address.",
        ));
    }
    Ok(Some(run_dir.join(ISOLATION_MARKERS_DIR).join(relative)))
}

fn native_cleanup_needed(run_dir: &Path, cwd: &Path) -> io::Result<bool> {
    let Some(path) = native_marker_path(run_dir, cwd)? else {
        return Ok(false);
    };
    let marker = read_isolation_marker(&path).map_err(io::Error::other)?;
    Ok(marker
        .as_ref()
        .and_then(IsolationMarker::native_skills)
        .is_some_and(crate::skills::bundle::native::NativeDelivery::needs_cleanup))
}

/// Caller utrzymuje wyłączność kopii. Przerwany zapis/cleanup zostawia complete=false,
/// więc odtworzenie procesu nie uzna częściowo sprzątniętego folderu za wynik.
fn change_native_skills(
    run_dir: &Path,
    cwd: &Path,
    skills: Option<&crate::skills::StepSkills>,
) -> io::Result<()> {
    let Some(path) = native_marker_path(run_dir, cwd)? else {
        return if skills.is_none() {
            Ok(())
        } else {
            Err(io::Error::other(
                "Native skills cannot be installed in your project folder.",
            ))
        };
    };
    let Some(mut marker) = read_isolation_marker(&path).map_err(io::Error::other)? else {
        /* Bez zapisu o kopii nie ma zapisu o dostarczonej polce, wiec nie ma czego sprzatac.
         * Sprzatanie NIE MOZE tu odmowic: odmowa przerywa domykanie drzewa przed
         * `isolate::finish`, czyli zostawia i katalog, i galaz — a wtedy krok o nieznanym
         * punkcie startu nie dostaje ostroznego wyboru, tylko brak domkniecia.
         * Instalacja bez zapisu wlasnosci dalej jest odmowa. To doslowne lustro galezi trzy
         * linie wyzej (`native_marker_path` -> `None`) i tego, jak brak markera czyta
         * `native_cleanup_needed`. */
        return if skills.is_none() {
            Ok(())
        } else {
            Err(io::Error::other(
                "The working copy's saved ownership is missing.",
            ))
        };
    };
    let mut native = match marker.native_skills() {
        Some(native) => native.clone(),
        None if skills.is_none() => return Ok(()),
        None => crate::skills::bundle::native::NativeDelivery::new(cwd)?,
    };
    native.validate(cwd)?;
    if skills.is_none() && !native.needs_cleanup() {
        return Ok(());
    }
    native.complete = false;
    marker.set_native_skills(native.clone());
    write_isolation_marker(&path, &marker).map_err(io::Error::other)?;
    match skills {
        Some(skills) => native.deliver(cwd, skills)?,
        None => native.clean(cwd)?,
    }
    marker.set_native_skills(native);
    write_isolation_marker(&path, &marker).map_err(io::Error::other)
}

/// Jedna odmowa reuse dla Git i kopii plikowych. Brak starego pola to legacy bez fan-in;
/// jawnie rozpoczęte przygotowanie bez inputReady nigdy nie awansuje przez brak procesu.
pub(super) fn refuse_incomplete_input(previous: &Path, work_key: &str) -> Result<(), RunError> {
    let marker_path = previous.join(ISOLATION_MARKERS_DIR).join(work_key);
    let marker = read_isolation_marker(&marker_path).map_err(|error| {
        io::Error::other(format!("The saved input record cannot be read: {error}"))
    })?;
    if marker.as_ref().is_some_and(|one| {
        one.fan_in().is_some_and(|prepared| {
            prepared.work_key != work_key || one.origin_snapshot() != Some(prepared.origin.as_str())
        })
    }) {
        return Err(RunError::Io(io::Error::other(
            "The saved input belongs to a different copy or saved starting files.",
        )));
    }
    if marker.is_some_and(|one| one.has_incomplete_input()) {
        return Err(RunError::Io(io::Error::other(format!(
            "The input for {work_key} was not completely prepared. Its partial folder was kept for inspection, not saved as a result. Start a new run to combine the complete parent results."
        ))));
    }
    Ok(())
}

fn read_isolation_marker(path: &Path) -> Result<Option<IsolationMarker>, isolate::Trouble> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(isolate::Trouble::Copying(error)),
    };
    serde_json::from_slice(&bytes).map(Some).map_err(|error| {
        isolate::Trouble::Copying(io::Error::new(io::ErrorKind::InvalidData, error))
    })
}

fn write_isolation_marker(path: &Path, marker: &IsolationMarker) -> Result<(), isolate::Trouble> {
    let parent = path.parent().ok_or_else(|| {
        isolate::Trouble::Copying(io::Error::other("the isolation record has no parent"))
    })?;
    fs::create_dir_all(parent).map_err(isolate::Trouble::Copying)?;
    let bytes = serde_json::to_vec(marker).map_err(|error| {
        isolate::Trouble::Copying(io::Error::new(io::ErrorKind::InvalidData, error))
    })?;
    let temp = parent.join(format!(".{}.writing", Uuid::now_v7()));
    let result = (|| -> io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        fs::File::open(parent)?.sync_all()?;
        if let Some(run_dir) = parent.parent() {
            fs::File::open(run_dir)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(isolate::Trouble::Copying)
}

fn remove_isolation_marker(path: &Path) -> Result<(), isolate::Trouble> {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(isolate::Trouble::Copying(error)),
    }
    if let Some(parent) = path.parent() {
        fs::File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(isolate::Trouble::Copying)?;
    }
    Ok(())
}

#[derive(Default)]
struct ListedWorktree {
    path: Option<PathBuf>,
    head: Option<String>,
    branch: Option<String>,
    prunable: bool,
    locked: bool,
    unsafe_shape: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum ExpectedWorktreeAdmin {
    Absent,
    Present { prunable: bool },
}

fn listed_worktrees(project: &Path) -> Result<Vec<ListedWorktree>, isolate::Trouble> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["worktree", "list", "--porcelain", "-z"])
        .output()
        .map_err(|error| isolate::Trouble::Git(error.to_string()))?;
    if !output.status.success() {
        return Err(isolate::Trouble::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    let text = String::from_utf8(output.stdout).map_err(|_| {
        isolate::Trouble::Git(
            "git returned a work tree record that is not valid text; nothing was removed"
                .to_owned(),
        )
    })?;
    parse_worktree_records(&text)
}

fn parse_worktree_records(text: &str) -> Result<Vec<ListedWorktree>, isolate::Trouble> {
    let mut records = Vec::new();
    let mut record = ListedWorktree::default();
    for field in text.split('\0') {
        if field.is_empty() {
            if record.path.is_some() || record.head.is_some() || record.branch.is_some() {
                if record.path.is_none() || record.head.is_none() {
                    return Err(malformed_worktree_record());
                }
                records.push(std::mem::take(&mut record));
            }
        } else if let Some(path) = field.strip_prefix("worktree ") {
            if record.path.replace(PathBuf::from(path)).is_some() {
                return Err(malformed_worktree_record());
            }
        } else if let Some(head) = field.strip_prefix("HEAD ") {
            if record.head.replace(head.to_owned()).is_some() {
                return Err(malformed_worktree_record());
            }
        } else if let Some(branch) = field.strip_prefix("branch ") {
            if record.branch.replace(branch.to_owned()).is_some() {
                return Err(malformed_worktree_record());
            }
        } else if field == "prunable" || field.starts_with("prunable ") {
            record.prunable = true;
        } else if field == "locked" || field.starts_with("locked ") {
            record.locked = true;
        } else if field == "bare" || field == "detached" {
            record.unsafe_shape = true;
        } else {
            // Git moze dodac pole w przyszlosci. Nieznany rekord nie blokuje sprzatania innego
            // worktree, ale nigdy sam nie staje sie uprawnieniem do kasowania.
            record.unsafe_shape = true;
        }
    }
    if record.path.is_some() || record.head.is_some() || record.branch.is_some() {
        return Err(malformed_worktree_record());
    }
    Ok(records)
}

fn malformed_worktree_record() -> isolate::Trouble {
    isolate::Trouble::Git(
        "git returned an incomplete work tree record; nothing was removed".to_owned(),
    )
}

fn expected_worktree_admin(
    project: &Path,
    cwd: &Path,
    branch: &str,
    head: &str,
) -> Result<ExpectedWorktreeAdmin, isolate::Trouble> {
    let expected_path = anchored_child_path(cwd)?;
    let expected_branch = format!("refs/heads/{branch}");
    let mut found = None;
    for record in listed_worktrees(project)? {
        let branch_matches = record.branch.as_deref() == Some(expected_branch.as_str());
        let listed_path = anchored_child_path_if_possible(
            record
                .path
                .as_deref()
                .ok_or_else(malformed_worktree_record)?,
        );
        let path_matches = listed_path.as_ref() == Some(&expected_path);
        if !branch_matches && !path_matches {
            continue;
        }
        if found.is_some()
            || !path_matches
            || !branch_matches
            || record.head.as_deref() != Some(head)
            || record.locked
            || record.unsafe_shape
        {
            return Err(isolate::Trouble::Git(
                "the work tree administration no longer matches the recovery record; nothing was removed"
                    .to_owned(),
            ));
        }
        found = Some(ExpectedWorktreeAdmin::Present {
            prunable: record.prunable,
        });
    }
    Ok(found.unwrap_or(ExpectedWorktreeAdmin::Absent))
}

fn anchored_child_path(path: &Path) -> Result<PathBuf, isolate::Trouble> {
    anchored_child_path_if_possible(path).ok_or_else(|| {
        isolate::Trouble::Git(
            "the recovery work tree path has no stable parent; nothing was removed".to_owned(),
        )
    })
}

fn anchored_child_path_if_possible(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let name = path.file_name()?;
    let parent = path.parent()?;
    if !fs::symlink_metadata(parent).ok()?.file_type().is_dir() {
        return None;
    }
    let parent = fs::canonicalize(parent).ok()?;
    Some(parent.join(name))
}

fn cleanup_incomplete_worktree(
    project: &Path,
    cwd: &Path,
    branch: &str,
    marker_path: &Path,
) -> Result<(), isolate::Trouble> {
    let marker = read_isolation_marker(marker_path)?.ok_or_else(|| {
        isolate::Trouble::Git(
            "the incomplete work tree has no durable recovery record; nothing was removed"
                .to_owned(),
        )
    })?;
    let IsolationMarker::Recovering {
        branch: marked_branch,
        head: marked_head,
        ..
    } = marker
    else {
        return Err(isolate::Trouble::Git(
            "the work tree is marked complete; nothing was removed".to_owned(),
        ));
    };
    if marked_branch != branch {
        return Err(isolate::Trouble::Git(
            "the recovery record names a different branch; nothing was removed".to_owned(),
        ));
    }

    retire_matching_worktree(project, cwd, branch, &marked_head)?;
    remove_isolation_marker(marker_path)
}

/// Zdejmuje dokładnie jedno drzewo i jego ref, oba związane ścieżką, pełnym refem i OID.
///
/// 2026-08-28 (T-152 review): ten sam rdzeń obsługuje recovery i Drop provisional guarda.
/// Rozdzielenie tych dróg wcześniej zostawiło w guardzie ślepe `branch -D`, mimo że recovery
/// miało już sprawdzenie admina i CAS. Brak ścieżki jest bezpieczny tylko dla prunable admina;
/// podmieniona ścieżka, ref albo OID zawsze zostają nietknięte.
fn retire_matching_worktree(
    project: &Path,
    cwd: &Path,
    branch: &str,
    expected_head: &str,
) -> Result<(), isolate::Trouble> {
    let cwd_exists = path_entry_exists(cwd)?;
    let admin = expected_worktree_admin(project, cwd, branch, expected_head)?;
    if cwd_exists {
        if !worktree_points_at(project, cwd, branch)
            || branch_oid(project, branch)? != expected_head
        {
            return Err(isolate::Trouble::Git(
                "the work tree no longer matches its recorded identity; nothing was removed"
                    .to_owned(),
            ));
        }
        if admin == ExpectedWorktreeAdmin::Absent {
            return Err(isolate::Trouble::Git(
                "the work tree has no matching git administration; nothing was removed".to_owned(),
            ));
        }
    }
    if let ExpectedWorktreeAdmin::Present { prunable } = admin {
        if !cwd_exists && !prunable {
            return Err(isolate::Trouble::Git(
                "the missing work tree is not marked removable by git; nothing was removed"
                    .to_owned(),
            ));
        }
        if !branch_exists(project, branch)? || branch_oid(project, branch)? != expected_head {
            return Err(isolate::Trouble::Git(
                "the recorded branch no longer matches its commit; nothing was removed".to_owned(),
            ));
        }
        let destination = cwd.display().to_string();
        git_for_recovery(
            project,
            &["worktree", "remove", "--force", "--", &destination],
        )?;
    }
    if path_entry_exists(cwd)? {
        return Err(isolate::Trouble::Git(
            "git reported removing the work tree, but its path still exists; the branch was kept"
                .to_owned(),
        ));
    }
    if expected_worktree_admin(project, cwd, branch, expected_head)?
        != ExpectedWorktreeAdmin::Absent
    {
        return Err(isolate::Trouble::Git(
            "git kept the work tree administration; the branch was kept".to_owned(),
        ));
    }
    if branch_exists(project, branch)? {
        // `update-ref` laczy porownanie i kasowanie w jednej operacji CAS. Reczne przesuniecie
        // galezi miedzy osobnym `rev-parse` i `branch -D` nie moze wpasc w okno TOCTOU.
        let reference = format!("refs/heads/{branch}");
        git_for_recovery(project, &["update-ref", "-d", &reference, expected_head])?;
    }
    if branch_exists(project, branch)? {
        return Err(isolate::Trouble::Git(
            "git kept the recorded branch after its guarded removal".to_owned(),
        ));
    }
    Ok(())
}

fn cleanup_provisional_git_tree(
    project: &Path,
    cwd: &Path,
    branch: &str,
    head: &str,
    marker_path: &Path,
) -> Result<(), isolate::Trouble> {
    if let Some(marker) = read_isolation_marker(marker_path)?
        && !marker.matches(branch, head)
    {
        return Err(isolate::Trouble::Git(
            "the isolation record no longer matches this attempt; nothing was removed".to_owned(),
        ));
    }
    retire_matching_worktree(project, cwd, branch, head)?;
    if let Some(marker) = read_isolation_marker(marker_path)? {
        if !marker.matches(branch, head) {
            return Err(isolate::Trouble::Git(
                "the isolation record changed during cleanup; it was kept".to_owned(),
            ));
        }
        remove_isolation_marker(marker_path)?;
    }
    Ok(())
}

fn path_entry_exists(path: &Path) -> Result<bool, isolate::Trouble> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(isolate::Trouble::Copying(error)),
    }
}

fn branch_exists(project: &Path, branch: &str) -> Result<bool, isolate::Trouble> {
    let reference = format!("refs/heads/{branch}");
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["show-ref", "--verify", "--quiet", &reference])
        .output()
        .map_err(|error| isolate::Trouble::Git(error.to_string()))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(isolate::Trouble::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        )),
    }
}

fn branch_oid(project: &Path, branch: &str) -> Result<String, isolate::Trouble> {
    let reference = format!("refs/heads/{branch}");
    git_for_recovery(project, &["rev-parse", "--verify", &reference])
        .map(|oid| oid.trim().to_owned())
}

fn worktree_points_at(project: &Path, cwd: &Path, branch: &str) -> bool {
    let Ok(metadata) = fs::symlink_metadata(cwd) else {
        return false;
    };
    // `canonicalize` ponizej celowo porownuje prawdziwe sciezki repozytorium, ale nie moze
    // jednoczesnie sluzyc za dowod wlasnosci wpisu pod `run/work`. Symlink w tym miejscu
    // moglby wskazac poprawny worktree poza biegiem, a cleanup usunalby cudza sciezke.
    if !metadata.file_type().is_dir() {
        return false;
    }
    let Ok(expected_cwd) = fs::canonicalize(cwd) else {
        return false;
    };
    let Some(top) = git_for_recovery(cwd, &["rev-parse", "--show-toplevel"])
        .ok()
        .and_then(|path| fs::canonicalize(path.trim()).ok())
    else {
        return false;
    };
    if top != expected_cwd {
        return false;
    }
    let Ok(reference) = git_for_recovery(cwd, &["symbolic-ref", "--quiet", "HEAD"]) else {
        return false;
    };
    if reference.trim() != format!("refs/heads/{branch}") {
        return false;
    }
    let common = |at: &Path| {
        git_for_recovery(at, &["rev-parse", "--git-common-dir"])
            .ok()
            .and_then(|path| {
                let path = PathBuf::from(path.trim());
                fs::canonicalize(if path.is_absolute() {
                    path
                } else {
                    at.join(path)
                })
                .ok()
            })
    };
    common(project).is_some_and(|expected| Some(expected) == common(cwd))
}

fn git_for_recovery(at: &Path, args: &[&str]) -> Result<String, isolate::Trouble> {
    let output = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(args)
        .output()
        .map_err(|error| isolate::Trouble::Git(error.to_string()))?;
    if !output.status.success() {
        return Err(isolate::Trouble::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Jedno drzewo robocze kroku: gdzie stoi, na czym stoi i czego do niego nie weszło.
///
/// Powstaje przy układaniu katalogu biegu, a czyta się je DWA razy: raz zaraz potem, żeby
/// powiedzieć człowiekowi, czego agent nie zobaczy, i drugi raz po biegu, żeby pracę zamknąć
/// na gałęzi albo posprzątać po kroku, który nic nie zrobił.
#[derive(Debug, Clone)]
struct Isolated {
    /// Tożsamość zwalidowanej kopii przy layout; te same bajty na obcym inode nie są własnością.
    copy_identity: PublicationIdentity,
    /// Nazwa kroku — ta z kafelka, bo to jej szuka człowiek.
    step: String,
    /// Pozycja tego kroku w księdze biegu.
    ///
    /// Po niej, a nie po nazwie: zdanie o pracy, której nie dało się zapisać, ma trafić do
    /// wiersza TEGO kroku w `run.json`, a dwa kafelki wolno nazwać tak samo.
    at: StepId,
    /// Katalog roboczy kroku.
    cwd: PathBuf,
    /// Gałąź, jeśli to jest drzewo gita. `None` dla folderu, który repozytorium nie jest.
    branch: Option<String>,
    /// Pliki, o których git nie wie, więc drzewo ich nie niesie.
    left_behind: Vec<String>,
}

/// Faktów domknięcia nie wiążemy z `LineSink`: preview może żyć dłużej niż pompa biegu.
#[derive(Debug, Clone)]
struct ClosingRun {
    id: String,
    dir: PathBuf,
    title: String,
    input_snapshot: Option<super::input_snapshot::InputSnapshot>,
    workspace_inputs: super::workspace_inputs::Bound,
}

impl ClosingRun {
    fn from_live(live: &Live) -> Self {
        Self {
            id: live.plan.id.clone(),
            dir: live.plan.dir.clone(),
            title: live.plan.title.clone(),
            input_snapshot: live.plan.input_snapshot.clone(),
            workspace_inputs: live.plan.workspace_inputs.clone(),
        }
    }

    fn input_for(&self, key: &str) -> Option<&super::input_snapshot::InputSnapshot> {
        self.workspace_inputs
            .get(key)
            .map(|seed| &seed.input)
            .or(self.input_snapshot.as_ref())
    }
}

/// Ile nazw plików mieści się w jednym wierszu, zanim zacznie być ścianą tekstu.
///
/// Pięć, nie „wszystkie": `docs/DECISIONS-LOCKED.md` §D4 stawia sufit gęstości na strumieniu,
/// a wiersz dłuższy od ekranu kosztuje resztę strumienia, nie tylko siebie.
const NAMED_AT_MOST: usize = 5;

/// Mówi, czego agent NIE zobaczy — zanim ruszy.
///
/// Cicha strata jest tu gorsza niż brak funkcji: bieg wygląda na kompletny, a agentowi brakuje
/// pliku, który człowiek widzi u siebie na ekranie. Wiersz powstaje wyłącznie wtedy, kiedy
/// naprawdę coś zostało — zdanie „zostawiono 0 plików" uczy, że tę linię wolno pominąć.
fn say_what_was_left_behind(lines: &LineSink, made: &[Isolated]) {
    for one in made {
        if one.left_behind.is_empty() {
            continue;
        }
        // Nazwy, nie sama liczba: „3 pliki" nie mówi człowiekowi, czy brakuje `.env`, czy
        // notatki, której i tak nie czytał. Ale nazwy PRZYCIĘTE, bo liczba bywa duża: zmierzone
        // 2026-08-19 na `~/Projects/meetnotes` — 188 plików nieśledzonych, czyli wiersz na pół
        // ekranu, którego nikt nie przeczyta i po którym reszta strumienia jest nie do
        // znalezienia. Pierwsze pięć wystarczy, żeby człowiek poznał RODZAJ tego, czego brakuje.
        let count = one.left_behind.len();
        let named = if count > NAMED_AT_MOST {
            format!(
                "{}, and {} more",
                one.left_behind[..NAMED_AT_MOST].join(", "),
                count - NAMED_AT_MOST
            )
        } else {
            one.left_behind.join(", ")
        };
        // Wynik świadomie porzucony: pełna kolejka do okna jest normalnym stanem
        // (`ipc::Sent`), a bieg nie ma prawa stanąć dlatego, że okno nie nadąża.
        let _ = lines.send(Line::Problem {
            agent: one.step.clone(),
            text: format!(
                "Git does not track {count} file(s), so this step's tree does not have them: \
                 {named}"
            ),
            resets_at: None,
        });
    }
}

/// Zdanie o folderze, który leży W ŚRODKU repozytorium, ale nie jest jego korzeniem.
///
/// PUBLICZNY, bo kryterium ma ten napis CZYTAĆ, nie przepisywać: ta sama zasada, co przy
/// `sections/settings/index.tsx` (`DEFAULT_LEAD_LABEL`). Napis wpisany z palca po obu stronach
/// granicy jest zielony także wtedy, gdy zdanie na ekranie i zdanie w teście to dwie różne rzeczy.
///
/// # 2026-09 (Z-9) — CO DO ZNAKU, bez ani jednego naszego słowa przed nim
///
/// Pierwsza wersja niosła prefiks „Loadout copied the files instead of branching:" i była
/// pomyłką dwojaką. Zdanie mówiło o KOPII PLIKÓW także tam, gdzie żadnej kopii nie ma — workflow
/// pracujący w samym folderze projektu nie zakłada ani jednego katalogu roboczego — a kryterium
/// pytało o treść przez `contains`, więc nadmiar przechodził niezauważony. Wymieniona jest
/// przyczyna i skutek, i tylko one: „nie jest korzeniem" samo w sobie nie mówi człowiekowi,
/// co przez to traci.
pub const INSIDE_A_REPOSITORY_BUT_NOT_ITS_ROOT: &str = "this folder is inside a git repository but is not its root, so the work will not land on \
     a branch";

/// Mówi to jedno zdanie **raz na bieg**, kiedy folder biegu leży w środku cudzego repozytorium.
///
/// # 2026-09 (Z-9) — po co to jest
///
/// `isolate::is_a_repo` odpowiada „to nie repozytorium" także o podkatalogu cudzego repozytorium,
/// i to jest jej właściwa odpowiedź: drzewo robocze założone w podkatalogu cudzego repo leżałoby
/// w jego indeksie. Ale skutkiem jest bieg, który wygląda **dokładnie** jak bieg w korzeniu —
/// kafelki idą, kroki się kończą — a pracy nie ma na żadnej gałęzi i nie będzie. Do dziś nic tego
/// nie mówiło; człowiek, który wybrał podkatalog swojego monorepo, dowiadywał się o tym przez
/// nieobecność czegoś, czego nie umiał nazwać.
///
/// # Warunkiem jest FOLDER, i tylko folder
///
/// Pierwsza wersja pytała najpierw o to, czy jakiś krok dostał własną kopię plików — i milczała
/// dla workflow, w którym każdy kafelek pracuje w samym folderze projektu (`folder: project`).
/// A to jest ten sam bieg z tą samą stratą: praca stoi w cudzym drzewie roboczym, nie na żadnej
/// naszej gałęzi, i nikt jej tam nie szuka. Warunek jest więc jeden, ten z nazwy funkcji.
///
/// **Jedno zdanie, nie jedno na krok.** To jest fakt o FOLDERZE, a nie o kafelku: powtórzony przy
/// każdym kroku uczy człowieka przewijać obok (`NAMED_AT_MOST` obok istnieje z tego samego
/// powodu). W podpisie stoi PIERWSZY kafelek planu, bo `Line::Problem` niesie nazwę tego, o kim
/// zdanie mówi (`sections/run/feed/speakers.ts`), a ten fakt dotyczy każdego kroku po kolei.
/// Bieg bez ani jednego kroku — kształt, którego walidator nie przepuszcza — podpisuje się
/// tytułem, bo cisza w tym miejscu byłaby drugim warunkiem tej funkcji.
fn say_the_folder_is_inside_a_repo(lines: &LineSink, project: &Path, plan: &Plan) {
    if !isolate::inside_a_repo_but_not_its_root(project) {
        return;
    }
    // Wynik świadomie porzucony: pełna kolejka do okna jest normalnym stanem (`ipc::Sent`),
    // a bieg nie ma prawa stanąć dlatego, że okno nie nadąża.
    let _ = lines.send(Line::Problem {
        agent: plan
            .steps
            .first()
            .map_or_else(|| plan.title.clone(), |one| one.name.clone()),
        text: INSIDE_A_REPOSITORY_BUT_NOT_ITS_ROOT.to_owned(),
        resets_at: None,
    });
}

/// Początek zdania o pożyczonym tekście, w którym przegląd coś zauważył.
///
/// PRYWATNY, w odróżnieniu od [`INSIDE_A_REPOSITORY_BUT_NOT_ITS_ROOT`] wyżej, i to jest wybór:
/// tamten napis jest treścią kryterium co do słowa, a o tym wierszu kryterium pyta ZACHOWANIEM —
/// czy strumień nazywa pożyczony plik i wiersz w nim (niezmiennik 20). Napis wystawiony na
/// zewnątrz „na wszelki wypadek" jest szwem, którego nikt nie woła.
const BORROWED_TEXT_WAS_KEPT_WITH_A_NOTE: &str = "Loadout kept the text this step borrowed from the project, and wrote down what it noticed \
     in it:";

/// Mówi **raz na kafelek**, że pożyczony przez niego tekst przeszedł przegląd nie całkiem czysto.
///
/// # 2026-09 (Z-21) — po co to jest
///
/// Bez tej linii znalezisko istnieje wyłącznie w `run.json`, czyli w pliku, którego nikt nie
/// otwiera w trakcie biegu — a fakt o cudzym tekście, który właśnie wszedł do promptu, jest
/// faktem na teraz, nie na potem (niezmiennik 29). Ciężkie znalezisko zabiera cały bieg
/// (`inherit::Error::Blocked`) i tutaj nie dochodzi; to jest zdanie o tym, co PRZEPUSZCZONO.
///
/// **Jedna linia na kafelek, nie na znalezisko**: to jest fakt o tym, co ten krok dostał, a nie
/// o każdej linii z osobna. Powtórzony trzy razy pod jednym kafelkiem uczy przewijać obok
/// (`NAMED_AT_MOST` i `say_the_folder_is_inside_a_repo` istnieją z tego samego powodu).
///
/// **Cytatu tu nie ma i to jest wybór.** Cytowana linia jest tekstem, który ktoś napisał po to,
/// żeby model ją wykonał; jej miejsce jest w `run.json`, obok reguły, która ją złapała, a nie
/// na ekranie, gdzie stoi między zdaniami Loadouta. Wiersz i plik wystarczą, żeby ją otworzyć.
fn say_what_the_borrowed_text_carries(lines: &LineSink, plan: &Plan) {
    for step in &plan.steps {
        let Job::Agent(job) = &step.job else {
            continue;
        };
        let noticed = job.borrowed.concerns();
        if noticed.is_empty() {
            continue;
        }
        let places = noticed
            .iter()
            .map(|one| format!("{} line {}", one.reference, one.line))
            .collect::<Vec<_>>()
            .join(", ");
        // Wynik świadomie porzucony: pełna kolejka do okna jest normalnym stanem (`ipc::Sent`),
        // a bieg nie ma prawa stanąć dlatego, że okno nie nadąża.
        let _ = lines.send(Line::Note {
            agent: step.name.clone(),
            text: format!("{BORROWED_TEXT_WAS_KEPT_WITH_A_NOTE} {places}"),
            // Wiersz niesie całe zdanie, więc proza za nim byłaby tym samym zdaniem drugi raz.
            body: Vec::new(),
        });
    }
}

/// Wnosi do tego biegu przekazania biegu, który go poprzedził.
///
/// 2026-08-23 — DLA PONOWNEGO ODPALENIA KROKU. Krok powtórzony sam jeden nie ma po czym iść,
/// a jego prompt składa się z instrukcji i **indeksu przekazań poprzedników** — bez nich
/// dostałby to samo zadanie z pustym kontekstem i pracował od zera nad czymś, co reszta grafu
/// już zrobiła.
///
/// Kopiujemy tylko pliki z pierwszego poziomu: `handoffs/` jest płaskie z założenia
/// (`memory::handoff`), a wejście w głąb wciągałoby tu cokolwiek, co ktoś tam kiedyś położy.
/// Brak katalogu źródłowego nie jest awarią — to bieg, po którym nie zostało ani jedno
/// przekazanie, i taki też ma być powtórzony.
///
/// # 2026-08-23 (T-88) — ZAŁĄCZNIK JEDZIE Z PLIKIEM, KTÓRY GO WOŁA
///
/// `memory::handoff` tnie ciało na `BODY_CAP`, odkłada ORYGINAŁ do `attachments/` i wstawia
/// w ciało wiersz `Moved to attachments/<nazwa>__full.md`. Ten wiersz składa Loadout, nie agent,
/// i jest liczony **od katalogu biegu** — więc bez tej drugiej kopii wznowiony krok dostawał od
/// NAS odnośnik, którego nie da się otworzyć, czyli kontrolkę bez handlera (niezmiennik 16).
///
/// Zmierzone na biegu `20260819-223942` w wersji tego samego defektu o warstwę wyżej: krok
/// dostał trzy takie wskaźniki, nie otworzył żadnego, napisał, że pełnego tekstu „nie ma",
/// i wyliczył cały dowód drugi raz wprost z repozytorium — 9 z 10 minut swojego limitu.
///
/// `attachments/` jest SIOSTRĄ `handoffs/`, nie jego podkatalogiem, więc kopiuje się osobno —
/// i tak samo cicho, kiedy go nie ma: katalog powstaje wyłącznie po biegu, w którym coś nie
/// zmieściło się w pliku, czyli w większości biegów nie powstaje wcale.
fn seed_the_handoffs(plan: &Plan) -> io::Result<()> {
    let Some(from) = &plan.seeded_from else {
        return Ok(());
    };
    for directory in handoff::publication_directories(from)? {
        let relative = directory.strip_prefix(from).map_err(io::Error::other)?;
        let into = plan.dir.join(relative);
        copy_the_files_in(
            &directory.join(crate::store::rebuild::HANDOFFS_DIR),
            &into.join(crate::store::rebuild::HANDOFFS_DIR),
        )?;
        copy_the_files_in(
            &directory.join(handoff::ATTACHMENTS_DIR),
            &into.join(handoff::ATTACHMENTS_DIR),
        )?;
    }
    Ok(())
}

/// Pliki z pierwszego poziomu jednego katalogu do drugiego. Brak źródła nie jest awarią.
///
/// Katalog docelowy powstaje dopiero, kiedy jest co do niego włożyć: pusty `attachments/`
/// w katalogu biegu jest odpowiedzią „coś tu nie zmieściło się w pliku" postawioną nad niczym,
/// a `Live::index_of_what_came_before` czyta jego ISTNIENIE jako właśnie to pytanie.
fn copy_the_files_in(source: &Path, into: &Path) -> io::Result<()> {
    let Ok(listing) = fs::read_dir(source) else {
        return Ok(());
    };
    fs::create_dir_all(into)?;
    for entry in listing {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            fs::copy(entry.path(), into.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// `run.json` poprzedniego biegu w tylu polach, ile potrzebuje przejęcie jego przekazań.
///
/// Własny, wąski kształt zamiast pełnego lustra [`RunFile`] — ten sam powód, co przy
/// `rerun::Finished`: tamten plik rośnie z każdym zadaniem, a to pytanie dotyczy dwóch rzeczy
/// i nie ma powodu przewracać się na trzeciej, której nie zna (niezmiennik 5).
#[derive(Debug, Deserialize)]
struct RunBefore {
    /// Graf, **jak biegł**. Po nim, nie po dzisiejszym pliku: pytamy o to, co wtedy stało przed
    /// czym — a plik mógł się od tamtej pory zmienić i to jest zwykły powód, dla którego ktoś
    /// w ogóle wznawia (`rerun`, „bierzemy DZISIEJSZY plik workflow").
    workflow_snapshot: WorkflowFile,
    /// Kroki tamtego biegu w kolejności ich numerów — po niej przekazanie wskazuje swój kafelek.
    #[serde(default)]
    steps: Vec<StepBefore>,
}

/// Krok tamtego biegu w jednym polu: kluczu węzła, z którego wychodzi klucz kafelka.
#[derive(Debug, Deserialize)]
struct StepBefore {
    #[serde(default)]
    node_key: String,
}

/// Co każdy krok tego biegu przejmuje po biegu, od którego go wznowiono — po jednej liście
/// na krok.
///
/// # Dwa pytania, jedna odpowiedź na kafelek
///
/// **„Co ten kafelek wtedy dostał"** odpowiada `reads:` z jego własnego przekazania: to jest
/// zapis tego, co Loadout NAPRAWDĘ wstrzyknął w tamten prompt ([`Told::reads`]), a nie tego,
/// co dziś wynika z grafu. Powtórzony kafelek ma dostać dokładnie to samo wejście — inaczej
/// „czy moja poprawka zmieniła wynik" jest pytaniem, w którym zmieniły się dwie rzeczy naraz.
///
/// **„Co się wydarzyło przede mną"** odpowiadają strzałki migawki, kiedy tamtego zapisu nie ma:
/// krok, który padł, oddaje dalej ostatnie zdanie i **nic nie przeczytał**
/// ([`Live::hand_on_its_last_words`] podaje puste `reads`), a krok, który nigdy nie ruszył, nie
/// zostawia pliku wcale. To jest dokładnie ten krok, od którego ktoś wznawia — i ma dostać
/// wszystko, na czym miał budować, aż do korzenia grafu.
///
/// # Dlaczego chód w górę zatrzymuje się na kafelku, który biegnie
///
/// Kafelek powtarzany w tym biegu odda swoją NOWĄ pracę własną strzałką. Wpisanie obok niej
/// starej kopii byłoby tym samym krokiem mówiącym w jednym indeksie dwie różne rzeczy, a dalsza
/// wspinaczka ponad niego dokładałaby krokom za nim materiał, który i tak przyjedzie do nich
/// jego świeżym przekazaniem.
fn what_the_run_before_left(plan: &Plan) -> Vec<Vec<Carried>> {
    let mut carried: Vec<Vec<Carried>> = vec![Vec::new(); plan.steps.len()];
    let Some(before) = plan.seeded_from.as_deref() else {
        return carried;
    };
    let Some(described) = fs::read(before.join(RUN_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<RunBefore>(&bytes).ok())
    else {
        // Bieg, którego pliku nie da się przeczytać, nie ma jak powiedzieć, czyj jest który
        // plik — a zgadywanie po nazwie byłoby drugą definicją numeracji kroków (niezmiennik 13).
        return carried;
    };
    let Ok(left) = handoff::scan_run_dir(before) else {
        return carried;
    };

    // Numer kroku tamtego biegu → klucz kafelka. Numer, nie nazwa: nazwy wolno powtórzyć na
    // dwóch kafelkach, a numerem przekazanie podpisuje się samo (`MetaDraft::step`).
    let tiles: Vec<&str> = described
        .steps
        .iter()
        .map(|step| tile_key_of(&step.node_key))
        .collect();

    // Nazwa pliku → kto go napisał; i klucz kafelka → jego OSTATNI plik. Rundy jednej pętli
    // piszą po kolei, więc „ostatni" jest tym o najwyższym numerze kroku.
    let mut whose: BTreeMap<&str, (&str, &handoff::Handoff)> = BTreeMap::new();
    let mut newest: BTreeMap<&str, (u32, &str)> = BTreeMap::new();
    for one in &left {
        let Some(tile) = usize::try_from(one.meta.step)
            .ok()
            .and_then(|at| tiles.get(at).copied())
        else {
            continue;
        };
        let Some(name) = one.path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        whose.insert(name, (tile, one));
        if newest
            .get(tile)
            .is_none_or(|(had, _)| *had <= one.meta.step)
        {
            newest.insert(tile, (one.meta.step, name));
        }
    }

    let running: BTreeSet<&str> = plan
        .steps
        .iter()
        .map(|step| step.tile_key.as_str())
        .collect();

    for (at, step) in plan.steps.iter().enumerate() {
        let wanted: Vec<&str> = match newest
            .get(step.tile_key.as_str())
            .and_then(|(_, name)| whose.get(name))
        {
            Some((_, had)) if !had.meta.reads.is_empty() => had
                .meta
                .reads
                .iter()
                .filter_map(|read| Path::new(read).file_name()?.to_str())
                .collect(),
            _ => stood_before(&described.workflow_snapshot, &step.tile_key, &running)
                .into_iter()
                .filter_map(|tile| newest.get(tile).map(|(_, name)| *name))
                .collect(),
        };
        carried[at] = wanted
            .into_iter()
            .filter_map(|name| {
                let (tile, had) = whose.get(name)?;
                if running.contains(tile) {
                    return None;
                }
                let source_scope =
                    crate::workflow::execution::RunInputs::from_graph(&described.workflow_snapshot)
                        .ok()?;
                if source_scope.context_key(tile)
                    != plan.inputs.configuration.context_key(&step.tile_key)
                {
                    return None;
                }
                let path = plan.dir.join(had.path.strip_prefix(before).ok()?);
                // Wskazujemy WYŁĄCZNIE to, co naprawdę leży w tym katalogu: wiersz indeksu ze
                // ścieżką bez pliku po drugiej stronie przewraca `prompt_for` i zabiera krok.
                if !path.is_file() {
                    return None;
                }
                // Czytamy KOPIĘ z nowego biegu, nie rekord źródłowy: jego względny wskaźnik
                // rozwiązuje się wtedy do `attachments/` obok tej kopii i nie może zachować
                // adresu starego biegu (T-114, 2026-08-24).
                let attachment = handoff::read_handoff(&path)
                    .ok()
                    .and_then(|copied| copied.attachment())
                    .filter(|full| full.is_file());
                Some(Carried {
                    from: had.meta.from.clone(),
                    path,
                    attachment,
                })
            })
            .collect();
    }
    carried
}

/// Kafelki, które w TAMTYM grafie stały przed tym — i których ten bieg nie powtarza.
///
/// Chód w górę po strzałkach, przechodni: krok wznowiony po środku łańcucha ma dostać nie tylko
/// swojego bezpośredniego poprzednika, ale wszystko, na czym tamten stał. Zatrzymuje się na
/// kafelku, który biegnie w tym biegu — powód stoi przy [`what_the_run_before_left`].
///
/// Kolejność wynikowa jest **kolejnością z pliku**, tą samą, którą trzyma indeks tego biegu
/// ([`Live::handed_before`]) i prefiks nazwy pliku przekazania. Kolejność obchodu grafu zależy
/// od tego, w którą stronę wyszło się z rozgałęzienia, i przy dwóch gałęziach przestaje być
/// powtarzalna.
///
/// Strzałka powrotna pętli jest tu zwykłą strzałką: runda poprawiająca stała po sędzim i jego
/// zdanie jest częścią tego, na czym stanęła. Zbiór odwiedzonych domyka koło.
fn stood_before<'a>(graph: &'a WorkflowFile, tile: &str, running: &BTreeSet<&str>) -> Vec<&'a str> {
    let mut found: BTreeSet<usize> = BTreeSet::new();
    let mut seen: BTreeSet<&str> = BTreeSet::from([tile]);
    let mut walking: Vec<&str> = vec![tile];
    while let Some(here) = walking.pop() {
        for link in &graph.links {
            if link.to != here || !seen.insert(link.from.as_str()) {
                continue;
            }
            if running.contains(link.from.as_str()) {
                continue;
            }
            if let Some(at) = graph.steps.iter().position(|one| one.id() == link.from) {
                found.insert(at);
            }
            walking.push(link.from.as_str());
        }
    }
    found
        .into_iter()
        .filter_map(|at| graph.steps.get(at).map(Step::id))
        .collect()
}

/// Zamyka drzewa po biegu: praca ląduje na gałęzi, a katalog, w którym powstała, znika.
///
/// Po kroku, który nic nie zmienił — ani w drzewie, ani commitem na swojej gałęzi — nie zostaje
/// ani gałąź, ani katalog. Po kroku, który zmienił cokolwiek, zostaje sama gałąź: praca jest
/// z niej osiągalna w całości, a katalog dokładał do tego wyłącznie kopię repozytorium na dysku
/// (T-95).
///
/// # 2026-09 (Z-9) — KATALOG KOPII PLIKOWEJ TEŻ SCHODZI
///
/// Do tego dnia warunek na `branch` przepuszczał krok bez gałęzi bez ani jednego skutku, a doc
/// mówił wprost „katalog kopii nie jest sprzątany nigdy": projekt bez repozytorium gałęzi nie ma,
/// więc katalog **był** jedynym miejscem, w którym praca kroku istniała.
///
/// Cena za to była mierzona i płacona przez człowieka: kopia niesie cały projekt bez `.git`,
/// `node_modules` i `target` ([`isolate::NOT_COPIED`]), zostaje po KAŻDYM biegu i nic w całej
/// aplikacji nie umiało jej zdjąć — ani po biegu, ani przy otwarciu folderu, ani przyciskiem.
/// Zmierzone u właściciela 2026-09-02: 87 katalogów `work/` i 3,8 GB w jednym projekcie.
///
/// **Po tej zmianie bieg w folderze bez repozytorium nie zostawia nic**, i to jest powiedziane
/// wprost, bo jest to strata: praca takiego kroku nie ma gdzie wrócić, dopóki folder nie jest
/// repozytorium. Krok, który ma coś oddać dalej, oddaje to przekazaniem (`memory::handoff`) —
/// a te leżą w katalogu biegu i zostają.
///
/// Kiedy zapis na gałąź się nie uda, katalog zostaje — a zdanie o tym idzie do wiersza tego
/// kroku w `run.json`. Bez niego bieg wygląda na udany, a jedyna kopia czyjejś pracy leży poza
/// gitem, w katalogu, którego nikt nie szuka. Tą samą drogą jedzie zdanie o katalogu albo
/// gałęzi, których nie dało się sprzątnąć (Z-7), i zdanie o kopii, której nie dało się zdjąć.
///
/// # 2026-09 (Z-10) — DLACZEGO TO JEST `async` I ODDAJE CAŁĄ ROBOTĘ PULI BLOKUJĄCEJ
///
/// Bo domykanie jednego drzewa to `git status`, `git add`, `git commit`, `git worktree remove`
/// i `remove_dir_all` — a to wszystko stało w linii, na tym samym wątku, na którym leży Stop
/// i pompa wierszy biegu. Zmierzone w `stop_answers_while_the_trees_close.rs`, na repozytorium
/// z wolnym hakiem commita: okno nie dostawało tury przez **cały** czas domykania, więc człowiek
/// naciskał Stop i nie działo się nic.
///
/// Sprzątanie po biegu, którego nikt już nie prowadzi, robi tę samą pracę tą samą
/// `isolate::finish` (`commands::reconcile::close_what_the_runs_left`) i schodzi z wątku tą samą
/// drogą: jego jedyny żywy wołający, `AppState::project_for`, jest od 2026-09 `async` i oddaje
/// całe uzgodnienie folderu puli blokującej.
async fn close_the_trees(project: &Path, made: &[Isolated], live: &Arc<Live>) {
    // Dane na własność, bo `spawn_blocking` żąda `'static`: `Isolated` jest `Clone`, a księga
    // biegu żyje w `Arc` od chwili powstania (`prepare_planned_run`), więc oba są tu darmowe.
    let project = project.to_path_buf();
    let made = made.to_vec();
    let book = Arc::clone(live);
    if let Err(joined) =
        tokio::task::spawn_blocking(move || close_every_tree(&project, &made, &book)).await
    {
        // Zadanie blokujące nie panikuje — `panic` jest w tym drzewie `deny` — więc `Err` tutaj
        // znaczy „bardzo nie tak". Wynik biegu tego nie psuje (nie psuł go też nieudany commit),
        // ale dziennik jest jedynym miejscem, w którym ktokolwiek się o tym dowie.
        tracing::error!(%joined, "the folders this run worked in could not be closed down");
    }
}

/// Ciało [`close_the_trees`], wykonywane w całości na puli blokującej.
fn close_every_tree(project: &Path, made: &[Isolated], live: &Arc<Live>) {
    let context = ClosingRun::from_live(live);
    for one in made {
        let key = one
            .cwd
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        let same_copy = supervisor::PublicationRoot::open(&one.cwd)
            .is_ok_and(|root| root.identity() == one.copy_identity);
        if !same_copy {
            record_closed_copy(
                live,
                one,
                &key,
                SavedCopy::Unavailable,
                &["The working folder changed identity, so Loadout left it untouched.".to_owned()],
            );
            continue;
        }
        match live.processes.try_finalize_copy(&one.cwd) {
            Ok(Some(_guard)) => {
                let (saved, says) = close_finalized_copy(project, one, &context);
                record_closed_copy(live, one, &key, saved, &says);
            }
            Ok(None) => {
                // Przed rejestracją callbacku: ostatnia usługa może zejść w tej samej chwili.
                // Ten fakt blokuje też retencję między Dead a trwałym zapisem rezultatu.
                let published = {
                    let mut book = live.book();
                    book.pending_finalization.insert(key.clone());
                    live.spill(&book)
                };
                if let Err(error) = published {
                    tracing::error!(run = %context.id, %error, "the deferred folder could not be recorded");
                    continue;
                }
                let weak = Arc::downgrade(live);
                let context = context.clone();
                let one = one.clone();
                let project = project.to_path_buf();
                let cwd = one.cwd.clone();
                let callback = Box::new(move || {
                    let (saved, says) = close_finalized_copy(&project, &one, &context);
                    if let Some(live) = weak.upgrade() {
                        record_closed_copy(&live, &one, &key, saved, &says);
                    } else if let Err(error) = record_deferred_result(&context, &key, &saved, &says)
                    {
                        tracing::error!(run = %context.id, %error, "the deferred folder result could not be saved");
                    }
                });
                if let Err(error) = live.processes.defer_copy_finalization(&cwd, callback) {
                    tracing::error!(run = %live.plan.id, %error, "the working folder remains pending finalization");
                }
            }
            Err(error) => record_closed_copy(
                live,
                one,
                &key,
                SavedCopy::Unavailable,
                &[format!(
                    "Loadout could not prove that this working folder is free to close: {error}"
                )],
            ),
        }
    }
}

/// Ta sama polityka po grafie i po ostatnim procesie; callback nie utrzymuje pompy IPC.
fn close_finalized_copy(
    project: &Path,
    one: &Isolated,
    context: &ClosingRun,
) -> (SavedCopy, Vec<String>) {
    if !supervisor::PublicationRoot::open(&one.cwd)
        .is_ok_and(|root| root.identity() == one.copy_identity)
    {
        return (
            SavedCopy::Unavailable,
            vec!["The working folder changed identity, so Loadout left it untouched.".to_owned()],
        );
    }
    let key = one.cwd.file_name().and_then(|one| one.to_str());
    let incomplete = key.map_or_else(
        || {
            Err(RunError::Io(io::Error::other(
                "The copy has no valid saved-input key.",
            )))
        },
        |key| refuse_incomplete_input(&context.dir, key),
    );
    // WF-03: diagnostyczna kopia nie jest wynikiem. Nie commitujemy ani nie kasujemy jej
    // po niepełnej publikacji, także gdy wszystkie procesy już zeszły.
    let complete = incomplete
        .and_then(|()| change_native_skills(&context.dir, &one.cwd, None).map_err(RunError::Io));
    if let Err(why) = complete {
        (
            SavedCopy::Unavailable,
            vec![format!(
                "{why} The partial folder is still here: {}",
                one.cwd.display()
            )],
        )
    } else {
        match &one.branch {
            Some(branch) => close_one_tree(project, one, branch, context),
            None => close_one_copy(one, context),
        }
    }
}

fn record_closed_copy(live: &Live, one: &Isolated, key: &str, saved: SavedCopy, says: &[String]) {
    let at = one.at;
    let said_now = says.join(" ");
    live.update(move |book| {
        book.copy_results.insert(key.to_owned(), saved);
        book.pending_finalization.remove(key);
        if said_now.is_empty() {
            return;
        }
        let Some(row) = book.steps.get_mut(at) else {
            return;
        };
        // DOPISUJEMY, nie nadpisujemy. Krok mógł paść z własnego powodu i tamten powód jest
        // tym, którego człowiek szuka pierwszy; ten drugi mówi mu, gdzie w takim razie leży
        // to, co agent zdążył zrobić.
        row.error = Some(match row.error.take() {
            Some(said) => format!("{said} {said_now}"),
            None => said_now,
        });
    });
}

/// Po zniknięciu Live plik nadal jest prawdą. CAS zachowuje późną refleksję i zmiany innych
/// kopii; nie odtwarza całego run.json z nieaktualnej księgi przechowanej w callbacku.
fn record_deferred_result(
    context: &ClosingRun,
    key: &str,
    saved: &SavedCopy,
    says: &[String],
) -> io::Result<()> {
    use std::io::Read as _;
    let target = context.dir.join(RUN_FILE);
    for _ in 0..3 {
        let mut bytes = Vec::new();
        supervisor::open_regular_beneath(&context.dir, Path::new(RUN_FILE))?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(io::Error::other(
                "the saved run is too large to update safely",
            ));
        }
        let mut file: Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if file.get("id").and_then(Value::as_str) != Some(context.id.as_str()) {
            return Err(io::Error::other(
                "the saved run changed identity before its folder closed",
            ));
        }
        let root = file
            .as_object_mut()
            .ok_or_else(|| io::Error::other("the saved run is not an object"))?;
        root.entry("copy_results")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| io::Error::other("the saved results are not readable"))?
            .insert(
                key.to_owned(),
                serde_json::to_value(saved).map_err(io::Error::other)?,
            );
        if let Some(pending) = root
            .get_mut("pending_finalization")
            .and_then(Value::as_array_mut)
        {
            pending.retain(|one| one.as_str() != Some(key));
        }
        let said = says.join(" ");
        if !said.is_empty()
            && let Some(steps) = root.get_mut("steps").and_then(Value::as_array_mut)
        {
            for step in steps {
                if step
                    .get("node_key")
                    .and_then(Value::as_str)
                    .is_some_and(|node| work_key_of(node) == key)
                {
                    let previous = step
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    step["error"] = Value::String(if previous.is_empty() {
                        said.clone()
                    } else {
                        format!("{previous} {said}")
                    });
                    break;
                }
            }
        }
        let updated = serde_json::to_vec_pretty(&file).map_err(io::Error::other)?;
        match DurableFilePublisher::new(&context.dir).publish_definition(
            &target,
            &updated,
            ModePolicy::Exact(DEFINITION_FILE_MODE),
            Some(&crate::durable_file::revision_of(&bytes)),
        ) {
            Ok(()) => return Ok(()),
            Err(PublishError::Changed { .. }) => {}
            Err(error) => return Err(error.into_io()),
        }
    }
    Err(io::Error::other(
        "the saved run kept changing while its folder result was recorded",
    ))
}

/// WF-25: adres zapisany PRZED cleanupem. run.json nadal jest projekcją biegu;
/// recovery konsumuje ten niezmienny ślad, gdy późny callback nie mógł zaktualizować pliku.
#[derive(Debug, Serialize, Deserialize)]
struct CopyResultReceipt {
    schema: u8,
    run_id: String,
    work_key: String,
    copy_identity: PublicationIdentity,
    origin: Option<String>,
    saved: SavedCopy,
}

fn save_copy_result(context: &ClosingRun, one: &Isolated, saved: SavedCopy) -> io::Result<()> {
    let key = one
        .cwd
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other("the working copy has no result key"))?;
    if validated_copy_path(&context.dir, key)? != one.cwd {
        return Err(io::Error::other(
            "the working copy has a different result location",
        ));
    }
    let root = supervisor::PublicationRoot::open(&one.cwd)?;
    if root.identity() != one.copy_identity {
        return Err(io::Error::other(
            "the working copy was replaced before its result was saved",
        ));
    }
    let receipt = CopyResultReceipt {
        schema: 1,
        run_id: context.id.clone(),
        work_key: key.to_owned(),
        copy_identity: one.copy_identity,
        origin: context.input_for(key).map(|input| input.id().to_owned()),
        saved,
    };
    let bytes = serde_json::to_vec(&receipt).map_err(io::Error::other)?;
    supervisor::PublicationRoot::open(&context.dir)?
        .ensure_directory(Path::new(".results"), 0o700)?;
    let target = context.dir.join(".results").join(format!("{key}.json"));
    match DurableFilePublisher::new(&context.dir).atomic_create_if_absent(
        &target,
        &bytes,
        ModePolicy::Exact(crate::durable_file::PRIVATE_FILE_MODE),
    ) {
        Ok(()) => Ok(()),
        Err(error) => {
            // Idempotencja nie oznacza nadpisania. Odczyt musi być tych samych bajtów
            // przez bezpieczny uchwyt; inny wynik pozostawia drzewo do wyjaśnienia.
            let mut old = Vec::new();
            supervisor::open_regular_beneath(
                &context.dir,
                &Path::new(".results").join(format!("{key}.json")),
            )?
            .take(64 * 1024 + 1)
            .read_to_end(&mut old)?;
            if old == bytes {
                Ok(())
            } else {
                Err(error.into_io())
            }
        }
    }
}

/// Receipt z `.results/<klucz>.json`, przyciety do rozmiaru, ktorego nikt nie przekroczy.
///
/// Sufit jest tu po to, zeby uszkodzony albo podmieniony plik nie zjadl pamieci odzyskiwania:
/// receipt to kilka pol, a nie miejsce na cudza tresc.
fn the_receipt_in(held: &supervisor::PublicationRoot, name: &str) -> io::Result<CopyResultReceipt> {
    let mut bytes = Vec::new();
    held.open_regular_file(&Path::new(".results").join(name))?
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 {
        return Err(io::Error::other("the saved result is too large"));
    }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

/// Czy ten receipt jest z TEGO biegu, o TYM kluczu i o TYM zapisanym wejsciu.
///
/// Trzy pytania naraz, bo trzy rozne pomylki daja jeden skutek: wynik dopisany do cudzego
/// biegu. Odmowa jest glosna — cichy `continue` zostawialby plik, o ktory nikt juz nie zapyta.
fn the_receipt_belongs_here(
    receipt: &CopyResultReceipt,
    name: &str,
    id: &str,
    context: &ClosingRun,
) -> io::Result<()> {
    if receipt.schema != 1
        || receipt.run_id != id
        || name != format!("{}.json", receipt.work_key).as_str()
        || receipt.origin.as_deref()
            != context
                .input_for(&receipt.work_key)
                .map(super::input_snapshot::InputSnapshot::id)
    {
        return Err(io::Error::other(
            "the saved result belongs to a different run or input",
        ));
    }
    Ok(())
}

/// Czy ktoras zywa usluga trzyma jeszcze te kopie — a wtedy jej wyniku nie wolno domykac.
///
/// Nieodczytany klucz publikacji liczy sie JAK trzymanie: „nie wiem, czyj to katalog" nie jest
/// zgoda na ruszenie go.
fn a_running_service_keeps(
    cwd: &Path,
    services: &[super::processes::ServiceRecord],
) -> io::Result<bool> {
    let copy_key = supervisor::publication_root_key(cwd)?;
    Ok(services.iter().any(|service| {
        supervisor::publication_root_key(&service.cwd)
            .map_or(true, |owned| owned == copy_key && service.keeps_copy())
    }))
}

/// Czy katalog roboczy to wciaz DOKLADNIE ta kopia, o ktorej mowi receipt.
///
/// Skasowany katalog jest w porzadku — po to istnieje pozny receipt. Katalog, ktory stoi pod
/// tym samym adresem i ma inna tozsamosc, jest cudzy i odmawia.
fn the_copy_is_the_recorded_one(cwd: &Path, receipt: &CopyResultReceipt) -> io::Result<()> {
    match fs::symlink_metadata(cwd) {
        Ok(_) => {
            if supervisor::PublicationRoot::open(cwd)?.identity() != receipt.copy_identity {
                return Err(io::Error::other(
                    "the recorded working folder changed identity",
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(())
}

/// Czy `run.json` ma juz ten wynik i nie czeka na jego domkniecie.
///
/// Dwa warunki, nie jeden: wpis, ktory stoi w `copy_results` i JEDNOCZESNIE jest na liscie
/// czekajacych, zapisal sie w polowie — a to jest dokladnie ta praca, ktora odzyskiwanie ma
/// dokonczyc.
fn the_run_already_wrote(run: &Value, work_key: &str, saved: &Value) -> bool {
    run.get("copy_results")
        .and_then(|results| results.get(work_key))
        == Some(saved)
        && !run
            .get("pending_finalization")
            .and_then(Value::as_array)
            .is_some_and(|pending| pending.iter().any(|key| key.as_str() == Some(work_key)))
}

/// Jedyny czytelnik późnych receiptów; bez tego kroku usunięte cwd gubiłoby adres wyniku.
/// Nie podnosi Partial/Unknown ani cudzej tożsamości do poprawnego wyniku.
pub(super) fn recover_recorded_results(project: &Path, run_dir: &Path) -> io::Result<usize> {
    let held = supervisor::PublicationRoot::open(run_dir)?;
    let mut bytes = Vec::new();
    held.open_regular_file(Path::new(RUN_FILE))?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(io::Error::other("the saved run is too large"));
    }
    let run: Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if matches!(
        run.get("status").and_then(Value::as_str),
        Some("running" | "paused")
    ) {
        return Ok(0);
    }
    let pending = copies_left_pending(&run);
    let entries = match held.list_directory(Path::new(".results")) {
        Ok(entries) => entries,
        // BRAK RECEIPTÓW NIE ZNACZY „NIE MA CZEGO DOMYKAĆ" (WF-06). Kopia zwykłego folderu nie
        // pisze ich wcale, więc jej klucz stoi na liście czekających zupełnie sam — a wyjście
        // z tego stanu jest w tej funkcji jedyne.
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
    if entries.is_empty() && pending.is_empty() {
        return Ok(0);
    }
    let id = run
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        .ok_or_else(|| io::Error::other("the saved run identity is not valid"))?;
    let input = super::input_snapshot::read(run_dir)?;
    if run.pointer("/input_snapshot/id").and_then(Value::as_str) != Some(input.id()) {
        return Err(io::Error::other(
            "the saved run and its original input no longer match",
        ));
    }
    let services = super::reconcile::services_bound_to_run(project, run_dir, &run)?;
    let context = ClosingRun {
        id: id.to_owned(),
        dir: run_dir.to_path_buf(),
        title: run
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        input_snapshot: Some(input.clone()),
        workspace_inputs: super::workspace_inputs::read_bound(run_dir)?,
    };
    let mut recovered = 0;
    for entry in entries {
        // Nazwa wpisu jest `OsString`; poza tym jednym miejscem cala ta sciezka mowi `&str`.
        let Some(name) = entry.name.to_str() else {
            continue;
        };
        let receipt = the_receipt_in(&held, name)?;
        the_receipt_belongs_here(&receipt, name, id, &context)?;
        let cwd = own_copy_at(run_dir, &receipt.work_key);
        require_one_normal_child(&run_dir.join(WORK_DIR), &cwd)
            .map_err(|why| io::Error::other(why.to_string()))?;
        refuse_incomplete_input(run_dir, &receipt.work_key)
            .map_err(|why| io::Error::other(why.to_string()))?;
        if a_running_service_keeps(&cwd, &services)? {
            continue;
        }
        the_copy_is_the_recorded_one(&cwd, &receipt)?;
        let SavedCopy::Git { oid } = &receipt.saved else {
            continue;
        };
        if !matches!(oid.len(), 40 | 64)
            || !oid.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !isolate::names_a_commit(project, oid)
        {
            return Err(io::Error::other(
                "the recorded result commit is unavailable",
            ));
        }
        let value = serde_json::to_value(&receipt.saved).map_err(io::Error::other)?;
        if the_run_already_wrote(&run, &receipt.work_key, &value) {
            continue;
        }
        held.validate_path_identity(run_dir)?;
        record_deferred_result(&context, &receipt.work_key, &receipt.saved, &[])?;
        recovered += 1;
    }
    recovered += close_copies_left_pending(project, run_dir, &run, &pending, &context, &services)?;
    Ok(recovered)
}

/// Klucze kopii, których ten bieg nie zdążył domknąć — tak jak leżą w jego pliku.
fn copies_left_pending(run: &Value) -> Vec<String> {
    run.get("pending_finalization")
        .and_then(Value::as_array)
        .map(|pending| {
            pending
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Krok, który pracował w tej kopii: jego miejsce w księdze i nazwa z kafelka.
///
/// Po kluczu pracy, bo to on nazywa katalog: kopia „same-copy" należy do kroku, który ją założył,
/// a nie do tego, który wszedł do niej później. `None` znaczy, że plik biegu tego katalogu nie
/// zna — a wtedy odzyskiwanie nie ma prawa nazwać go niczyim wynikiem.
fn the_step_that_worked_in(run: &Value, work_key: &str) -> Option<(StepId, String)> {
    run.get("steps")
        .and_then(Value::as_array)?
        .iter()
        .enumerate()
        .find(|(_, step)| {
            step.get("node_key")
                .and_then(Value::as_str)
                .is_some_and(|node| work_key_of(node) == work_key)
        })
        .map(|(at, step)| {
            (
                at,
                step.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
}

/// WF-06: kopie spoza gita, po których odroczone domknięcie nie ma już komu przyjść.
///
/// # Co się działo
///
/// Kopię zwykłego folderu, którą po grafie trzyma jeszcze usługa okna, bieg wpisuje do
/// `pending_finalization`, a JEDYNYM nośnikiem jej domknięcia jest callback żyjący w pamięci
/// `Processes`. Ubicie aplikacji zabiera ten callback razem z procesem i klucz zostaje w pliku
/// biegu **na zawsze**: od tej chwili [`copy_lifetime_blocker`] odmawia obu retencjom, ręcznemu
/// „Forget this run", „Keep result" i przywróceniu wyniku — zdaniem „The result folders are still
/// being saved after their services stop.", które po restarcie jest po prostu nieprawdziwe, bo
/// żadna usługa nie żyje i nic się nie zapisuje. Człowiekowi zostawało `rm -rf` z terminala,
/// czyli dokładnie to, czego ten produkt ma nie wymagać.
///
/// Droga gitowa tego nie miała: jej wynik niesie receipt w `.results/`, a samo drzewo domyka
/// pętla po [`trees_left_in`] w `reconcile`. `IsolationMarker::FileCopy` nie oddaje ani gałęzi,
/// ani `head`, więc tamta pętla nie odwiedza kopii non-git ANI RAZU — i nikt inny też nie.
///
/// # Czego pilnuje
///
/// Wynik liczy TĄ SAMĄ [`close_finalized_copy`], co zwykłe domknięcie: odzyskiwanie nie ma prawa
/// mieć własnego zdania o tym, co jest zmianą, a co niezmienioną kopią do zdjęcia. Kopii, którą
/// wciąż trzyma żywa usługa, nie dotyka — dla niej tamto zdanie jest prawdziwe. Katalogu, którego
/// już nie ma, też nie: nie ma czego obejrzeć, a zgadnięty wynik byłby gorszy niż jego brak.
fn close_copies_left_pending(
    project: &Path,
    run_dir: &Path,
    run: &Value,
    pending: &[String],
    context: &ClosingRun,
    services: &[super::processes::ServiceRecord],
) -> io::Result<usize> {
    let mut closed = 0;
    for key in pending {
        let Ok(Some(marker)) =
            read_isolation_marker(&run_dir.join(ISOLATION_MARKERS_DIR).join(key))
        else {
            continue;
        };
        // Drzewo gita ma swoją drogę; niepełne wejście nie jest pracą kroku i nie wolno go
        // nazwać wynikiem (WF-03).
        if marker.branch().is_some() || marker.has_incomplete_input() {
            continue;
        }
        let cwd = own_copy_at(run_dir, key);
        if !fs::symlink_metadata(&cwd).is_ok_and(|one| one.file_type().is_dir()) {
            continue;
        }
        require_one_normal_child(&run_dir.join(WORK_DIR), &cwd)
            .map_err(|why| io::Error::other(why.to_string()))?;
        if a_running_service_keeps(&cwd, services)? {
            continue;
        }
        let Some((at, step)) = the_step_that_worked_in(run, key) else {
            continue;
        };
        let one = Isolated {
            copy_identity: supervisor::PublicationRoot::open(&cwd)?.identity(),
            step,
            at,
            cwd,
            // Bez gałęzi, bo tej kopii nikt nie zakładał w gicie — to jest właśnie ten wariant,
            // którego odzyskiwanie nie widziało. Pliki pominięte przez gita niesie wyłącznie
            // droga drzewa i tu nikt ich nie czyta.
            branch: None,
            left_behind: Vec::new(),
        };
        let (saved, says) = close_finalized_copy(project, &one, context);
        record_deferred_result(context, key, &saved, &says)?;
        closed += 1;
    }
    Ok(closed)
}

/// Recovery nie kopiuje formatu receiptu ani polityki ownershipu z normalnego zamknięcia.
pub(super) fn save_recovered_git_result(
    project: &Path,
    run_dir: &Path,
    tree: &LeftTree,
    oid: &str,
) -> io::Result<()> {
    let mut bytes = Vec::new();
    supervisor::open_regular_beneath(run_dir, Path::new(RUN_FILE))?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(io::Error::other("the saved run is too large"));
    }
    let run: Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    // Legacy nie dostaje wymyślonego origin. Dotychczasowy cleanup z poprawnego markera
    // pozostaje, ale nie obiecuje odtworzenia wejść, których dawny build nie zachował.
    if run.get("input_snapshot").is_none_or(Value::is_null) {
        return Ok(());
    }
    let input = super::input_snapshot::read(run_dir)?;
    if run.pointer("/input_snapshot/id").and_then(Value::as_str) != Some(input.id()) {
        return Err(io::Error::other(
            "the original input does not match this run",
        ));
    }
    let id = run
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        .ok_or_else(|| io::Error::other("the saved run has no valid identity"))?;
    if !worktree_points_at(project, &tree.cwd, &tree.branch) {
        return Err(io::Error::other(
            "this is no longer the recorded working tree",
        ));
    }
    let copy_identity = supervisor::PublicationRoot::open(&tree.cwd)?.identity();
    let copy_key = supervisor::publication_root_key(&tree.cwd)?;
    for service in super::reconcile::services_bound_to_run(project, run_dir, &run)? {
        if supervisor::publication_root_key(&service.cwd)? == copy_key
            && (service.keeps_copy() || service.copy_identity != copy_identity)
        {
            return Err(io::Error::other(
                "the service has not released this exact working copy",
            ));
        }
    }
    let context = ClosingRun {
        id: id.to_owned(),
        dir: run_dir.to_path_buf(),
        input_snapshot: Some(input),
        workspace_inputs: super::workspace_inputs::read_bound(run_dir)?,
        title: run
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    };
    let copy = Isolated {
        copy_identity,
        step: tree.key.clone(),
        at: 0,
        cwd: tree.cwd.clone(),
        branch: Some(tree.branch.clone()),
        left_behind: Vec::new(),
    };
    save_copy_result(
        &context,
        &copy,
        SavedCopy::Git {
            oid: oid.to_owned(),
        },
    )
}

/// Zamyka JEDNO drzewo gita i oddaje zdania, które człowiek ma o nim przeczytać.
fn close_one_tree(
    project: &Path,
    one: &Isolated,
    branch: &str,
    context: &ClosingRun,
) -> (SavedCopy, Vec<String>) {
    let initial = base_of_tree(project, &context.dir, &one.cwd);
    let isolate::Closed {
        kept,
        tidied,
        left_behind,
    } = isolate::finish_with_saved(
        project,
        &one.cwd,
        branch,
        &format!("{}: {}", context.title, one.step),
        initial.as_deref(),
        |oid| {
            save_copy_result(
                context,
                one,
                SavedCopy::Git {
                    oid: oid.to_owned(),
                },
            )
            .map_err(|why| format!("the result receipt could not be saved: {why}"))
        },
    );
    let saved = match &kept {
        isolate::Kept::OnABranch(branch) => branch_oid(project, branch).ok(),
        isolate::Kept::Nothing => initial,
        isolate::Kept::LeftInPlace { .. } => None,
    }
    .map_or(SavedCopy::Unavailable, |oid| SavedCopy::Git { oid });
    // Trzy zdania, jedno pole: krok umie zostawić pracę poza gitem, nie dać się sprzątnąć
    // i świadomie pominąć duże nowe pliki, a człowiek ma prawo przeczytać każde z nich.
    let mut says: Vec<String> = Vec::new();
    match kept {
        isolate::Kept::LeftInPlace { branch, why } => {
            tracing::warn!(step = %one.step, branch, "this step's work is not on its branch, so its folder stays");
            says.push(why);
        }
        kept => tracing::debug!(step = %one.step, ?kept, "the step's folder was closed"),
    }
    says.extend(tidied);
    says.extend(left_behind);
    (saved, says)
}

/// WF-06 (2026-09-05): domknięcie kopii bez Git nie może kasować jedynego wyniku pracy.
/// Dopiero porównanie ze sprawdzoną migawką wejścia pozwala usunąć niezmienioną własną kopię.
/// Zmiana albo niepewność zachowuje folder i jego opis; źródłowy projekt pozostaje nietknięty.
type InspectedCopy = (
    super::input_snapshot::InputSnapshot,
    BTreeMap<PathBuf, super::input_snapshot::Entry>,
    BTreeMap<PathBuf, CopyEntryIdentity>,
);

fn close_one_copy(one: &Isolated, context: &ClosingRun) -> (SavedCopy, Vec<String>) {
    let key = one
        .cwd
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let path = Path::new(WORK_DIR).join(key);
    let origin = context.input_for(key).map(|one| one.id().to_owned());
    let inspected = (|| -> io::Result<InspectedCopy> {
        let expected_path = validated_copy_path(&context.dir, key)?;
        if expected_path != one.cwd {
            return Err(io::Error::other("the copy has a different saved location"));
        }
        let root = supervisor::PublicationRoot::open(&one.cwd)?;
        if root.identity() != one.copy_identity {
            return Err(io::Error::other(
                "the copy directory was replaced; nothing was removed",
            ));
        }
        // Własność plików powstaje PRZED porównaniem; równe bajty na późniejszym obcym inode
        // nie uprawniają do cleanupu (WF-03/06). Nie idziemy za linkami ani po ich celach.
        let owned = copy_entry_identities(&root)?;
        let input = super::workspace_inputs::for_copy(&context.dir, key)?;
        if origin.as_deref() != Some(input.id()) {
            return Err(io::Error::other("the saved starting input changed"));
        }
        let entries = super::input_snapshot::inspect(&one.cwd)?;
        root.validate_path_identity(&one.cwd)?;
        for (path, identity) in &owned {
            if root.entry_identity(path)? != Some(*identity) {
                return Err(io::Error::other(
                    "an object in the copy was replaced while checking it",
                ));
            }
        }
        Ok((input, entries, owned))
    })();
    match inspected {
        Ok((input, entries, owned)) if entries == *input.entries() => {
            match remove_unchanged_copy(&context.dir, one, owned) {
                Ok(()) => (
                    SavedCopy::Unchanged {
                        origin: input.id().to_owned(),
                    },
                    Vec::new(),
                ),
                Err(why) => kept_uncertain_copy(path, origin, &one.cwd, &why.to_string()),
            }
        }
        Ok((input, entries, _owned)) => match sync_kept_copy(&one.cwd, one.copy_identity, &entries)
            .and_then(|()| folder_digest(&entries))
        {
            /* ZATRZYMANA KOPIA TO NORMALNY WYNIK KROKU, A NIE JEGO BŁĘDNE ZAKOŃCZENIE.
             *
             * Krok, który pracował we własnej kopii i coś w niej zmienił, ZAWSZE ją zostawia:
             * zdejmowania nikt tu nawet nie próbuje. Zdanie o tym szło dotąd do `StepRun::error`,
             * czyli do pola, którym okno maluje kafelek na czerwono i które czyta wyrocznia całego
             * przepływu — więc zielony bieg sześciu agentów kończył się skargą trzech z nich na
             * to, że wykonali swoją pracę.
             *
             * Ścieżka nie ginie i nie potrzebuje tu kopii (niezmiennik 13): niesie ją
             * `copy_results`, a pokazuje `ResultFolder` w panelu biegu minionego — razem z nazwą
             * kroku, zdaniem „Changes kept in this folder." i przyciskiem, który ten folder
             * otwiera. Do pola błędu wraca wyłącznie kopia, której zdjęcie NAPRAWDĘ odmówiło
             * (`kept_uncertain_copy` niżej). */
            Ok(digest) => (
                SavedCopy::Folder {
                    path,
                    origin: Some(input.id().to_owned()),
                    digest: Some(digest),
                },
                Vec::new(),
            ),
            Err(why) => kept_uncertain_copy(path, origin, &one.cwd, &why.to_string()),
        },
        Err(why) => kept_uncertain_copy(path, origin, &one.cwd, &why.to_string()),
    }
}

fn kept_uncertain_copy(
    path: PathBuf,
    origin: Option<String>,
    cwd: &Path,
    why: &str,
) -> (SavedCopy, Vec<String>) {
    (
        SavedCopy::Folder {
            path,
            origin,
            digest: None,
        },
        vec![format!(
            "This folder was kept because Loadout could not check its complete result ({why}). It cannot be reused automatically, but you can inspect it here: {}",
            cwd.display()
        )],
    )
}

type CopyEntryIdentity = (supervisor::PublicationEntryKind, PublicationIdentity);

fn copy_entry_identities(
    root: &supervisor::PublicationRoot,
) -> io::Result<BTreeMap<PathBuf, CopyEntryIdentity>> {
    let mut owned = BTreeMap::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(directory) = pending.pop() {
        for entry in root.list_directory(&directory)? {
            let path = directory.join(&entry.name);
            let identity = root
                .entry_identity(&path)?
                .ok_or_else(|| io::Error::other("a copy entry disappeared"))?;
            if entry.kind == supervisor::PublicationEntryKind::Directory {
                pending.push(path.clone());
            }
            owned.insert(path, identity);
        }
    }
    Ok(owned)
}

fn remove_unchanged_copy(
    run_dir: &Path,
    one: &Isolated,
    owned: BTreeMap<PathBuf, CopyEntryIdentity>,
) -> io::Result<()> {
    let root = supervisor::PublicationRoot::open(&one.cwd)?;
    if root.identity() != one.copy_identity {
        return Err(io::Error::other("the copy directory was replaced"));
    }
    let mut entries: Vec<_> = owned.into_iter().collect();
    entries.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
    for (path, identity) in entries {
        root.validate_path_identity(&one.cwd)?;
        if !root.remove_entry_if_identity(&path, identity)? {
            return Err(io::Error::other(
                "a copy entry was replaced; it was not removed",
            ));
        }
    }
    let relative = one.cwd.strip_prefix(run_dir).map_err(io::Error::other)?;
    let parent = supervisor::PublicationRoot::open(run_dir)?;
    if !parent.remove_entry_if_identity(
        relative,
        (
            supervisor::PublicationEntryKind::Directory,
            one.copy_identity,
        ),
    )? {
        return Err(io::Error::other(
            "the copy directory was replaced; it was not removed",
        ));
    }
    Ok(())
}

fn validated_copy_path(run_dir: &Path, key: &str) -> io::Result<PathBuf> {
    let mut parts = Path::new(key).components();
    if !matches!(parts.next(), Some(std::path::Component::Normal(_))) || parts.next().is_some() {
        return Err(io::Error::other(
            "the saved copy key is not one folder name",
        ));
    }
    let root = supervisor::PublicationRoot::open(run_dir)?;
    let relative = Path::new(WORK_DIR).join(key);
    if !matches!(
        root.entry_identity(&relative)?,
        Some((supervisor::PublicationEntryKind::Directory, _))
    ) {
        return Err(io::Error::other(
            "the saved result is missing or is not the original folder",
        ));
    }
    Ok(run_dir.join(relative))
}

fn folder_digest(entries: &BTreeMap<PathBuf, super::input_snapshot::Entry>) -> io::Result<String> {
    use sha2::Digest as _;
    let bytes = serde_json::to_vec(entries).map_err(io::Error::other)?;
    Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
}

fn sync_kept_copy(
    cwd: &Path,
    identity: PublicationIdentity,
    entries: &BTreeMap<PathBuf, super::input_snapshot::Entry>,
) -> io::Result<()> {
    let root = supervisor::PublicationRoot::open(cwd)?;
    if root.identity() != identity {
        return Err(io::Error::other("the result folder was replaced"));
    }
    for (path, entry) in entries {
        if matches!(entry, super::input_snapshot::Entry::File { .. }) {
            root.open_regular_file(path)?.sync_all()?;
        }
        root.target(path)?.sync_directory()?;
        if matches!(entry, super::input_snapshot::Entry::Directory) {
            // target otwiera wyłącznie katalog rodzica; ta nazwa nie tworzy żadnego artefaktu.
            root.target(&path.join(".result-sync"))?.sync_directory()?;
        }
    }
    root.target(Path::new(".result-sync"))?.sync_directory()?;
    root.validate_path_identity(cwd)
}

/// Wznowienie dostaje fakty sprawdzone z dysku, nie ścieżkę skopiowaną bez walidacji z JSON.
#[derive(Debug)]
pub(super) struct SavedFolder {
    pub(super) path: PathBuf,
    pub(super) entries: BTreeMap<PathBuf, super::input_snapshot::Entry>,
    pub(super) origin: String,
}

pub(super) fn read_saved_folder(
    previous: &Path,
    work_key: &str,
    saved: &SavedCopy,
) -> Result<SavedFolder, RunError> {
    refuse_incomplete_input(previous, work_key)?;
    let SavedCopy::Folder {
        path,
        origin: Some(origin),
        digest: Some(digest),
    } = saved
    else {
        return Err(RunError::Io(io::Error::other(
            "This folder result was kept for inspection, but cannot be verified for reuse.",
        )));
    };
    if path != &Path::new(WORK_DIR).join(work_key) {
        return Err(RunError::Io(io::Error::other(
            "The saved result points outside its own working copy.",
        )));
    }
    let path = validated_copy_path(previous, work_key)?;
    let held = supervisor::PublicationRoot::open(&path)?;
    let input = super::workspace_inputs::for_copy(previous, work_key)?;
    if input.id() != origin {
        return Err(RunError::Io(io::Error::other(
            "The result belongs to different saved starting files.",
        )));
    }
    let entries = super::input_snapshot::inspect(&path)?;
    held.validate_path_identity(&path)?;
    if folder_digest(&entries)? != *digest {
        return Err(RunError::Io(io::Error::other(
            "The saved result folder changed after the run finished. It was not reused.",
        )));
    }
    Ok(SavedFolder {
        path,
        entries,
        origin: origin.clone(),
    })
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum KeptFolderState {
    Changed,
    Uncertain,
    Incomplete,
}

#[derive(Debug)]
pub(super) struct KeptFolder {
    pub(super) work_key: String,
    pub(super) path: PathBuf,
    pub(super) state: KeptFolderState,
}

/// Historia, retencja i jawne Forget pytają ten sam rdzeń o foldery, których nie wolno zgubić.
/// Po awarii przed receipt sam folder + marker nadal są prawdą; brak indeksu niczego nie zmienia.
pub(super) fn kept_folders_in(run_dir: &Path) -> io::Result<Vec<KeptFolder>> {
    let described: Value = fs::read(run_dir.join(RUN_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null);
    let entries = match fs::read_dir(run_dir.join(WORK_DIR)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut kept = Vec::new();
    for entry in entries {
        let entry = entry?;
        let key = entry
            .file_name()
            .to_str()
            .ok_or_else(|| io::Error::other("a kept folder has an unreadable name"))?
            .to_owned();
        let kind = entry.file_type()?;
        if !kind.is_dir() {
            if kind.is_symlink() {
                return Err(io::Error::other(
                    "a kept folder was replaced by a link; nothing was removed",
                ));
            }
            continue;
        }
        let path = validated_copy_path(run_dir, &key)?;
        let marker = read_isolation_marker(&run_dir.join(ISOLATION_MARKERS_DIR).join(&key))
            .map_err(|error| io::Error::other(error.to_string()))?;
        let saved = described
            .get("copy_results")
            .and_then(|one| one.get(&key))
            .and_then(|one| serde_json::from_value::<SavedCopy>(one.clone()).ok());
        let state = if marker
            .as_ref()
            .is_some_and(IsolationMarker::has_incomplete_input)
        {
            KeptFolderState::Incomplete
        } else {
            match saved {
                Some(SavedCopy::Folder {
                    digest: Some(_), ..
                }) => KeptFolderState::Changed,
                Some(SavedCopy::Folder { .. }) => KeptFolderState::Uncertain,
                Some(SavedCopy::Git { .. }) => continue,
                _ if marker.as_ref().and_then(IsolationMarker::branch).is_some() => continue,
                // Legacy worktree nie staje się kopią non-git przez brak nowego receipt.
                _ if marker.is_none() && fs::symlink_metadata(path.join(".git")).is_ok() => {
                    continue;
                }
                _ => KeptFolderState::Uncertain,
            }
        };
        kept.push(KeptFolder {
            work_key: key,
            path,
            state,
        });
    }
    kept.sort_by(|one, other| one.work_key.cmp(&other.work_key));
    Ok(kept)
}

/// Usługi mogą zginąć przed końcem odroczonego zapisu wyniku. Sam status procesu nie
/// upoważnia wtedy retencji do usunięcia run.json spod callbacku finalizera (WF-25).
pub(super) fn pending_copy_finalization(run_dir: &Path) -> io::Result<bool> {
    let described: Value =
        serde_json::from_slice(&fs::read(run_dir.join(RUN_FILE))?).map_err(io::Error::other)?;
    match described.get("pending_finalization") {
        None => Ok(false),
        Some(Value::Array(pending)) => Ok(!pending.is_empty()),
        Some(_) => Err(io::Error::other(
            "the saved finalization record is not readable",
        )),
    }
}

pub(super) fn copy_lifetime_blocker(run_dir: &Path) -> io::Result<Option<String>> {
    let described: Value =
        serde_json::from_slice(&fs::read(run_dir.join(RUN_FILE))?).map_err(io::Error::other)?;
    if matches!(
        described.get("status").and_then(Value::as_str),
        Some("running" | "paused")
    ) {
        return Ok(Some(
            "This run is still using its working folders. Stop it before removing them.".to_owned(),
        ));
    }
    if pending_copy_finalization(run_dir)? {
        return Ok(Some(
            "The result folders are still being saved after their services stop.".to_owned(),
        ));
    }
    if super::processes::read_service_records(run_dir)?
        .iter()
        .any(super::processes::ServiceRecord::keeps_copy)
    {
        return Ok(Some("A service may still be using this run's folders. Stop it and wait for confirmation before removing them.".to_owned()));
    }
    Ok(None)
}

pub(super) fn retention_blocker(run_dir: &Path) -> io::Result<Option<String>> {
    if let Some(said) = super::result_restore::removal_blocker(run_dir)? {
        return Ok(Some(said));
    }
    if let Some(said) = copy_lifetime_blocker(run_dir)? {
        return Ok(Some(said));
    }
    let kept = kept_folders_in(run_dir)?;
    if !kept.is_empty() {
        return Ok(Some(format!(
            "This run holds result folders that were kept for you: {}. Forget them only after confirming these exact paths.",
            kept.iter()
                .map(|one| one.path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    Ok(None)
}

/// Jedno drzewo, które bieg zostawił po sobie na dysku — i wszystko, czego trzeba, żeby je zamknąć.
///
/// 2026-09 (Z-9) — POLITYKA „GDZIE STOI DRZEWO TEGO KROKU" ZOSTAJE W TYM MODULE (niezmiennik 23).
/// Uzgodnienie folderu potrzebuje tej odpowiedzi, ale nie ma prawa jej sobie SKŁADAĆ: druga kopia
/// wiedzy o `work/<klucz>` i `.isolation/<klucz>` rozjechałaby się z tą przy pierwszej poprawce
/// jednej z nich, a rozjazd w TĘ stronę kasuje cudzą pracę pod cudzą ścieżką.
pub(super) struct LeftTree {
    /// Klucz pracy: nazwa katalogu w `work/` i nazwa pliku markera. Równy `node_key` kroku.
    pub(super) key: String,
    /// Katalog, w którym ten krok pracował — istnieje w chwili, gdy ta lista powstaje.
    pub(super) cwd: PathBuf,
    /// Gałąź, na którą ta praca ma trafić. Z markera, nie ze sklejenia nazwy.
    pub(super) branch: String,
    /// Commit, z którego to drzewo powstało — punkt startu dla `isolate::finish`.
    pub(super) head: String,
}

/// Drzewa, które ten bieg zostawił na dysku, po jednym na marker izolacji.
///
/// # Dlaczego z markerów, a nie z `run.json`
///
/// Bo marker jest jedynym miejscem, w którym stoi zapisany PUNKT STARTU drzewa, a bez niego
/// `isolate::finish` nie odróżnia kroku, który nic nie zrobił, od kroku, który zacommitował całą
/// swoją pracę sam (Z-7). Nazwa gałęzi też jest w markerze: zgadywanie jej z identyfikatora biegu
/// byłoby drugą regułą na to samo pytanie, a ta droga KASUJE.
///
/// # Drzewo bez katalogu jest już zamknięte i nie wchodzi na tę listę
///
/// I to nie jest optymalizacja. Marker zostaje po zamkniętym drzewie na zawsze, więc bez tego
/// warunku każde otwarcie folderu próbowałoby zamknąć każde drzewo każdego biegu w historii —
/// a `worktree remove` na nieistniejącej ścieżce odmawia, czyli człowiek dostawałby zdanie
/// o nieudanym sprzątaniu przy każdym dotknięciu projektu, przy niczym, co by się nie udało.
///
/// `symlink_metadata`, nie `exists()`: dowiązanie pod tą ścieżką nie jest katalogiem, który
/// założył Loadout, a `git -C` poszedłby za nim do cudzego drzewa.
pub(super) fn trees_left_in(run_dir: &Path) -> Vec<LeftTree> {
    let Ok(entries) = fs::read_dir(run_dir.join(ISOLATION_MARKERS_DIR)) else {
        return Vec::new();
    };
    let mut left: Vec<LeftTree> = Vec::new();
    for entry in entries.flatten() {
        let Some(key) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(Some(marker)) = read_isolation_marker(&entry.path()) else {
            continue;
        };
        // Recovery nie może nazwać częściowej publikacji pracą kroku i commitować jej.
        if marker.has_incomplete_input() {
            continue;
        }
        let (Some(branch), Some(head)) = (marker.branch(), marker.head()) else {
            continue;
        };
        let cwd = own_copy_at(run_dir, &key);
        if !fs::symlink_metadata(&cwd).is_ok_and(|one| one.file_type().is_dir()) {
            continue;
        }
        left.push(LeftTree {
            key,
            cwd,
            branch: branch.to_owned(),
            head: head.to_owned(),
        });
    }
    // Kolejność katalogu jest dowolna, a zdania dla człowieka składają się w jedno pole: bez
    // sortowania ten sam folder czytałby się przy każdym otwarciu inaczej.
    left.sort_by(|one, other| one.key.cmp(&other.key));
    left
}

/// Commit, z którego powstało drzewo tego kroku — z markera izolacji.
///
/// 2026-09-03 (Z-7) — CZYTAMY GO PRZED ZAMKNIĘCIEM DRZEWA, bo bez niego `isolate::finish` nie
/// odróżnia kroku, który nic nie zrobił, od kroku, który zacommitował całą swoją pracę sam.
/// Ścieżkę markera składa ta sama funkcja, która ją dowodzi przy zakładaniu drzewa
/// ([`prove_generated_work_path`]), więc „gdzie stoi marker tego kroku" ma jedną odpowiedź
/// (niezmiennik 13).
///
/// `None`, kiedy markera nie ma albo nie da się go przeczytać. Znaczy to „nie wiem", a nie „zero
/// commitów" — i tak to czyta `isolate::finish`: bez punktu startu gałąź zostaje.
fn base_of_tree(project: &Path, run_dir: &Path, cwd: &Path) -> Option<String> {
    let marker_path = prove_generated_work_path(project, run_dir, cwd).ok()?;
    let marker = read_isolation_marker(&marker_path).ok()??;
    marker.head().map(str::to_owned)
}

/// Co ten kafelek pożycza z repozytorium gospodarza — przełożone z kształtu pliku na pytanie.
///
/// Jedno przełożenie, w jednym miejscu: [`Borrow`] jest kształtem pliku na dysku, który ma się
/// otwierać także za rok, a [`Chosen`] jest pytaniem zadawanym cudzemu katalogowi. Rozpisanie
/// tego drugi raz gdziekolwiek indziej dałoby dwa znaczenia jednego wyboru (niezmiennik 23).
fn what_this_step_borrows(borrow: &Borrow) -> Chosen {
    Chosen {
        skills: borrow.skills.clone(),
        learnings: borrow.learnings.clone(),
        // `agent` w pliku, `subagent` w pytaniu: klucz pliku nazywa półkę, z której pochodzi
        // (`<projekt>/.claude/agents/`), a nazwa w środku odróżnia go od agenta Loadouta.
        subagent: borrow.agent.clone(),
    }
}

/// Czy to, co kafelek pożyczył, da się w ogóle dowieźć — **zanim powstanie katalog biegu**.
///
/// # Dwa pytania, dwie odpowiedzi, obie przed pierwszym procesem (niezmiennik 12)
///
/// **Czy ta nazwa jest u gospodarza.** Cicha alternatywa jest najdroższą wersją tej wady:
/// człowiek zaznacza rolę, agent nie dostaje ani jednej jej reguły, nic nie pada — bo „agent
/// nie zna tych reguł" jest z zewnątrz nieodróżnialne od „model nie uznał, że warto po nie
/// sięgnąć". Odpowiada [`wire::nothing_is_missing`], czyli ta sama funkcja, którą u siebie
/// woła `wire::from_the_host`: jedno miejsce, jedna definicja słowa „znalezione".
///
/// **Czy ten program umie przyjąć katalog pluginu.** Umiejętność ma dokładnie jedną drogę do
/// procesu — ścieżkę katalogu w argv — więc vendor, który tej drogi nie zna, nie dostanie jej
/// wcale. Pytamy o to [`AgentDriver::inheriting`] pustym fragmentem, a nie nazwą vendora:
/// nazwa w tym miejscu byłaby drugim zestawem reguł po stronie rdzenia, czyli dokładnie tym,
/// jak w repo źródłowym po cichu umarło skanowanie sekretów (niezmiennik 23). Vendor nazywa
/// **zdanie odmowy** i tylko ono — po to, żeby człowiek wiedział, czy odznaczyć umiejętność,
/// czy przełączyć program.
///
/// Sam tekst do promptu żadnej drogi vendora nie potrzebuje: jedzie stdinem, który przyjmuje
/// każdy. Krok pożyczający wyłącznie plik roli albo opis podagenta rusza więc u każdego.
fn borrowing_is_possible(
    project: &Path,
    driver: &Arc<dyn AgentDriver>,
    borrows: &Chosen,
    step: &AgentStep,
) -> Result<(), RunError> {
    if borrows.is_nothing() {
        // Pusty wybór nie ma czego nie znaleźć — i nie zaglądamy wtedy do cudzego `.claude/`
        // ani razu. Folder bez tego katalogu jest normalnym stanem cudzego repozytorium
        // (niezmiennik 5), nie powodem do odmowy.
        return Ok(());
    }

    if !borrows.skills.is_empty()
        && driver.inheriting(&[]).is_none()
        && !driver.reads_step_skills_from_its_folder()
    {
        return Err(RunError::Refused(Note {
            level: Level::Problem,
            // Kropka ląduje na kafelku TEGO kroku: to jego wybór i jego agent.
            step_id: Some(step.id.clone()),
            message: format!(
                "This step borrows a skill from this project, and {} has no way to be handed \
                 one. Loadout stopped the run instead of starting the step without it: an agent \
                 that quietly knows less than you picked answers as though there was nothing to \
                 know. Either untick the skill on this step or give it an agent that runs on a \
                 different app.",
                crate::workflow::check::vendor_name(driver.id())
            ),
            fix: None,
        }));
    }

    wire::nothing_is_missing(project, borrows).map_err(|error| {
        RunError::Refused(Note {
            level: Level::Problem,
            step_id: Some(step.id.clone()),
            /* Zdanie co do słowa to, które napisał `inherit`: wymienia i pozycję, i folder,
             * w którym jej nie było. Przepisane tutaj byłoby drugą kopią jednego zdania,
             * a druga kopia jest zawsze tą nieaktualną (niezmiennik 23). */
            message: error.to_string(),
            fix: None,
        })
    })
}

/// WF-12: jeden pakiet instrukcji projektu, przed pierwszym procesem i przed rozwinięciem
/// promptów fizycznych kopii. Wznowienie bierze pakiet poprzedniego biegu, nie dzisiejszy host.
fn bring_recorded_sources(plan: &mut Plan) -> Result<(), RunError> {
    let Some(replay) = plan
        .replay
        .as_ref()
        .filter(|replay| replay.mode == super::replay::ReplayMode::Recorded)
    else {
        return Ok(());
    };
    if let Some(instructions) = &replay.instructions {
        instructions.save_to(&plan.dir)?;
    }
    for step in &mut plan.steps {
        let Job::Agent(job) = &mut step.job else {
            continue;
        };
        let Some(bundles) = replay.skills.get(&step.node_key) else {
            continue;
        };
        if let Some(skills) = &bundles.agent {
            let root = plan.dir.join("recorded-skills").join(&step.node_key);
            job.skills.dirs.clear();
            for skill in skills {
                let directory = root.join(&skill.name);
                skill.bundle.materialize(&directory)?;
                job.skills.dirs.push(directory);
            }
        }
        if let Some(skills) = &bundles.borrowed {
            // Ten prywatny host jest wyłącznie źródłem obecnego adaptera Borrow. Oryginalny
            // projekt i metadata source nie są czytane ani przepisywane.
            let host = plan.dir.join("recorded-borrow").join(&step.node_key);
            for skill in skills {
                skill.bundle.materialize(
                    &host
                        .join(crate::skills::SHELF_CLAUDE_READS)
                        .join(&skill.name),
                )?;
            }
            if let Some(crate::workflow::Step::Agent(configured)) = replay
                .graph
                .steps
                .iter()
                .find(|one| one.id() == step.tile_key)
            {
                borrowing_is_possible(&host, &job.driver, &job.borrows, configured)?;
            }
        }
    }
    Ok(())
}

fn bring_project_instructions(plan: &mut Plan, project: &Path) -> Result<(), RunError> {
    let graph_steps = plan.graph.get("steps").and_then(Value::as_array);
    let mut choices = Vec::new();
    for step in &plan.steps {
        if !matches!(&step.job, Job::Agent(_)) {
            continue;
        }
        let configured = graph_steps
            .and_then(|steps| {
                steps.iter().find(|source| {
                    source.get("id").and_then(Value::as_str) == Some(step.tile_key.as_str())
                })
            })
            .and_then(|source| source.get("projectInstructions"));
        let selected =
            crate::inherit::instructions::override_from(configured).map_err(|error| {
                RunError::Refused(Note {
                    level: Level::Problem,
                    step_id: Some(step.tile_key.clone()),
                    message: error.to_string(),
                    fix: None,
                })
            })?;
        let selected = plan
            .inputs
            .configuration
            .context_for(&step.tile_key)
            .map_or(selected, |scope| Some(scope.project_instructions));
        choices.push((step.tile_key.clone(), selected));
    }
    let snapshot = crate::inherit::instructions::for_run(
        project,
        &plan.dir,
        &choices,
        plan.seeded_from.as_deref(),
    )
    .map_err(|error| {
        RunError::Refused(Note {
            level: Level::Problem,
            step_id: None,
            message: error.to_string(),
            fix: None,
        })
    })?;
    let instructions = snapshot.prompt(false);
    plan.project_instructions = Some(serde_json::json!({
        "id": snapshot.id(), "digest": snapshot.package_digest()?,
    }));
    for step in &mut plan.steps {
        if !snapshot.for_step(&step.tile_key) {
            continue;
        }
        if let Job::Agent(job) = &mut step.job {
            job.prompt = format!("{instructions}\n{}", job.prompt);
            job.context.extend(snapshot.context());
        }
    }
    Ok(())
}

/// Zbiera to, co KAŻDY krok pożyczył, do katalogu tego kroku — po powstaniu katalogu biegu.
///
/// Stoi TU, a nie w `plan_agent`, i to jest ta sama granica, która trzyma
/// [`hand_the_skills_to_the_steps`] w tym samym miejscu: plan jest czystym rachunkiem, a to
/// jest zapis — i to zapis pod katalog biegu, który dopiero co powstał. Odmowa padła
/// wcześniej ([`borrowing_is_possible`]), więc tutaj zostaje wyłącznie awaria dysku i stan,
/// w którym ktoś skasował plik między planowaniem a startem.
fn bring_in_what_each_step_borrowed(plan: &mut Plan, project: &Path) -> Result<(), RunError> {
    // Kopia ścieżki, nie pożyczka: `plan.steps` bierzemy niżej mutowalnie.
    let run_dir = plan.dir.clone();
    for step in &mut plan.steps {
        // Klucz kafelka zdjęty ZANIM pożyczymy zadanie mutowalnie — na niego ląduje kropka.
        let tile_key = step.tile_key.clone();
        let node_key = step.node_key.clone();
        let Job::Agent(job) = &mut step.job else {
            continue;
        };
        if job.borrows.is_nothing() {
            // Krok bez wyboru nie dotyka cudzego katalogu i nie zakłada sobie własnego:
            // katalog, który powstał, prędzej czy później zostanie komuś podany.
            continue;
        }
        let under = run_dir.join(BORROWED_DIR).join(&node_key);
        let recorded_host = run_dir.join("recorded-borrow").join(&node_key);
        let host = if plan
            .replay
            .as_ref()
            .is_some_and(|replay| replay.mode == super::replay::ReplayMode::Recorded)
        {
            recorded_host.as_path()
        } else {
            project
        };
        job.borrowed = wire::from_the_host(host, &under, &job.borrows).map_err(|error| {
            RunError::Refused(Note {
                level: Level::Problem,
                step_id: Some(tile_key),
                message: error.to_string(),
                fix: None,
            })
        })?;
        if !job.borrows.skills.is_empty()
            && job.driver.inheriting(&[]).is_none()
            && job.driver.reads_step_skills_from_its_folder()
        {
            // WF-13: nie czytamy ponownie gospodarza. Native shelf dostaje zweryfikowaną,
            // zamrożoną paczkę tego kroku. W folderze człowieka wspólny placer odmawia.
            let plugin = under.join("plugin");
            let bundles = crate::skills::bundle::read_delivered(&plugin)?;
            let selected = crate::skills::StepSkills {
                names: bundles.iter().map(|skill| skill.name.clone()).collect(),
                dirs: bundles
                    .iter()
                    .map(|skill| plugin.join("skills").join(&skill.name))
                    .collect(),
            };
            hand_native_skills(&run_dir, &job.cwd, &selected, &step.name, &step.tile_key)?;
            job.borrowed.skills_were_delivered_in_the_folder();
        }
    }
    Ok(())
}

/// Kładzie umiejętności każdego kroku tam, gdzie jego sterownik naprawdę zagląda.
///
/// DWIE DROGI, WYBRANE Z MOŻLIWOŚCI STEROWNIKA. Claude Code przyjmuje katalog umiejętności
/// argumentem (`--plugin-dir`, [S1 §3]); pozostałych pięciu czyta `.agents/skills/` w katalogu
/// roboczym kroku [T5 §3.1]. Pytamy `inheriting`, nie nazwę vendora: adapter pozostaje jedynym
/// miejscem, które zna możliwości programu (niezmiennik 23).
///
/// PO JEDNYM KATALOGU PLUGINU NA KROK ([`STEP_SKILLS_DIR`]), bo zbiór jest własnością kroku.
///
/// ODMOWA PADA TUTAJ, PRZED PIERWSZYM PROCESEM. Krok, który potrzebuje umiejętności i pracuje
/// wprost w folderze człowieka, nie ma gdzie postawić półki — a dopisanie jej do cudzego
/// repozytorium jest zmianą, o której właściciel dowiaduje się z `git status` i która zostaje
/// tam po biegu na zawsze (`docs/ARCHITECTURE.md` §8). Odmowa zabiera cały bieg, więc nie ma
/// stanu, w którym część kroków ruszyła bez tego, co człowiek zaznaczył.
fn hand_the_skills_to_the_steps(plan: &mut Plan) -> Result<(), RunError> {
    // Kopia ścieżki, nie pożyczka: `plan.steps` bierzemy niżej mutowalnie.
    let run_dir = plan.dir.clone();
    for step in &mut plan.steps {
        // Trzy napisy zdjęte z kroku ZANIM pożyczymy jego zadanie mutowalnie: nazwa dla odmowy,
        // klucz węzła dla katalogu, klucz kafelka dla kropki na płótnie.
        let name = step.name.clone();
        let node_key = step.node_key.clone();
        let tile_key = step.tile_key.clone();
        let Job::Agent(job) = &mut step.job else {
            continue;
        };
        if job.skills.names.is_empty() {
            continue;
        }

        // NASZ, CZYLI POD KATALOGIEM BIEGU. `AgentJob::ours` odpowiada na inne pytanie — „czy ten
        // krok ma ten katalog założyć" — i dla `same-copy` daje `false` mimo że drzewo jest nasze
        // (założył je krok przed nim). Tamta odpowiedź w tym miejscu odmawiałaby krokowi, który
        // w folderze człowieka nie pracuje.
        let into = run_dir.join(STEP_SKILLS_DIR).join(&node_key);
        let carried = rewrite::plugin_dir_from_the_library(&job.skills, &into)
            .map_err(|error| RunError::Io(io::Error::other(error)))?;
        // 2026-09 (Z-8): półka jest wyłącznie zastępstwem dla brakującej flagi. Kładzenie jej
        // także sterownikowi z `--plugin-dir` wnosiło pliki Loadouta do commita kroku.
        if job.driver.inheriting(&[]).is_none() {
            let selected = saved_native_skills(&into)?;
            hand_native_skills(&run_dir, &job.cwd, &selected, &name, &tile_key)?;
        }

        job.plugin_flags = rewrite::plugin_argv(&carried);
    }
    Ok(())
}

fn saved_native_skills(plugin: &Path) -> io::Result<crate::skills::StepSkills> {
    let bundles = crate::skills::bundle::read_delivered(plugin)?;
    Ok(crate::skills::StepSkills {
        names: bundles.iter().map(|skill| skill.name.clone()).collect(),
        dirs: bundles
            .iter()
            .map(|skill| plugin.join("skills").join(&skill.name))
            .collect(),
    })
}

fn hand_native_skills(
    run_dir: &Path,
    cwd: &Path,
    skills: &crate::skills::StepSkills,
    name: &str,
    tile_key: &str,
) -> Result<(), RunError> {
    if !cwd.starts_with(run_dir.join(WORK_DIR)) {
        // Wspólny komunikat placera nazywa skill i kafelek; nie ma zapisu do gospodarza.
        skills
            .into_the_step_folder(cwd, false, name)
            .map_err(|refusal| refused_by_the_skills(&refusal, tile_key.to_owned()))?;
    }
    change_native_skills(run_dir, cwd, Some(skills))?;
    Ok(())
}

/// Odmowa rozmieszczania → odmowa biegu, ze zdaniem co do słowa tym, które napisał `skills`.
///
/// Dwa stany, dwa warianty: awaria dysku jest awarią i jedzie przezroczystym [`RunError::Io`],
/// a odmowa jest zdaniem dla człowieka i jedzie [`RunError::Refused`], czyli z kropką na kafelku
/// tego kroku.
fn refused_by_the_skills(refusal: &crate::skills::Error, tile_key: String) -> RunError {
    match refusal {
        crate::skills::Error::Refused(missing) => RunError::Refused(Note {
            level: Level::Problem,
            step_id: Some(tile_key),
            message: missing.to_string(),
            fix: None,
        }),
        other => RunError::Io(io::Error::other(other.to_string())),
    }
}

// ── ŻYWY BIEG ──────────────────────────────────────────────────────────────────────────────

/// Trwały rachunek prywatnej tury Loadouta.
///
/// `ran` znaczy, że tura zakończyła się użyteczną odpowiedzią. Odmowa któregokolwiek twardego
/// opakowania, anulowanie i porażka vendora zostawiają `ran = false` oraz powód; sam zamiar
/// startu nie może udawać wykonanego, opłaconego biegu. Powód doszedł 2026-09 (Z-18), bo pięć
/// rozłącznych stanów czytało się w historii jak jeden.
#[derive(Debug, Clone, Default, Serialize)]
struct ReflectionReceipt {
    ran: bool,
    kept: usize,
    #[serde(rename = "discardedAgain")]
    discarded_again: usize,
    dropped_without_reason: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_usd: Option<f64>,
    /// Sufit ceny, który tę turę obowiązywał.
    ///
    /// `None` znaczy, że sufit nigdy nie powstał, bo nie było o co pytać: Stop, wyłączone
    /// w ustawieniach, żaden agent nie skończył, nic nie zostało. Każda droga, na której
    /// Loadout poprosił o sterownik, niesie tu liczbę — także ta, na której go nie dostał.
    ///
    /// 2026-09 (Z-38) — TRWAŁY, BO ZDANIE O ZEJŚCIU NA SUFICIE PODAJE TĘ KWOTĘ. Sufit skaluje
    /// się z ceną kroków ([`REFLECTION_BUDGET_USD`] jest podłogą), więc okno nie ma jak go
    /// odtworzyć ze stałej, a bieg odczytany z historii miesiąc później tym bardziej.
    #[serde(skip_serializing_if = "Option::is_none")]
    budget_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    why: Option<NotAsked>,
}

impl ReflectionReceipt {
    fn not_asked(why: NotAsked) -> Self {
        Self {
            why: Some(why),
            ..Self::default()
        }
    }
}

/// Bieg w trakcie: plan (niezmienny) plus księga (zmienna), plus to, czym mówi do świata.
struct Live {
    /// Wszystko, co rozstrzygnięto przed startem.
    plan: Plan,
    /// Stan, który zmienia się w trakcie. Zamek jest `std::sync::Mutex`, a każde jego wzięcie
    /// mieści się w jednym wywołaniu bez `await` (niezmiennik 8, `clippy::await_holding_lock`
    /// = deny).
    book: Mutex<Book>,
    /// P-03a: czy ta maszyna umie w ogole obsluzyc natywne okno.
    ///
    /// Liczone RAZ na bieg i tylko wtedy, gdy ktorys krok naprawde tego wymaga: sonda odpala
    /// proces, wiec pytanie zadawane przy kazdym kroku byloby cena placona takze przez biegi,
    /// ktorych to nie dotyczy.
    native_ui: tokio::sync::OnceCell<crate::engine::native_ui::NativeUiAccess>,
    /// Linie na ekran, **po jednej**. Sklejaniem zajmuje się pompa z T-07 i tylko ona: bieg,
    /// który skleja u siebie, ustala okno, którego nikt nie zmierzył, i odbiera pompie jedyną
    /// rzecz, dla której ta pompa powstała.
    lines: LineSink,
    /// Jeden właściciel trwałych wiadomości tego biegu; nigdy współdzielony z retry/new run.
    messages: Arc<crate::bridge::messages::Mailbox>,
    /// Stop i Continue sięgają tędy do środka biegu.
    control: RunControl,
    /// Ten sam per-run obserwator co przy przygotowaniu; nigdy globalny hook.
    faults: Arc<dyn PrestartFaultInjector>,
    /// Rejestr rzeczy, które mają zostać żywe po swoim kroku (kafelek „uruchom i zostaw").
    processes: std::sync::Arc<crate::commands::processes::Processes>,
    /// Wspólna pula miejsc **całej aplikacji** i pauza dostawcy — jedne drzwi dla obu
    /// (`engine::limits`, nagłówek pliku): wysyłka pyta bieg, bieg pyta pulę. Uchwyt jest
    /// klonem cudzej puli, nigdy własną: pula zakładana per bieg jest nie do odróżnienia od
    /// tej, przez którą dwie karty dają `2 × limit` agentów naraz (niezmiennik 11).
    gate: limits::Run,
    /// Ile wolno wydać na ten bieg — albo `None`, kiedy człowiek nie postawił sufitu.
    ///
    /// Liczy się suma `cost_usd` kroków, które SIĘ SKOŃCZYŁY: tylko one mają cenę, a krok
    /// w połowie tury nie wie jeszcze, ile będzie kosztował.
    ///
    /// 2026-09 (Z-44) — AKAPIT MÓWIŁ „krok, którego vendor ceny nie podaje (Codex), liczy się
    /// jako zero" i przestał być prawdą. Vendor, który ceny nie podaje, dostaje ją z tabeli
    /// stawek ([`Plan::prices`]) — a krok, którego z niej wycenić się nie da, pod postawionym
    /// sufitem **nie rusza wcale** ([`Live::no_way_to_price_this_one`]). Zero było tą trzecią
    /// odpowiedzią, przy której sufit zostawał napisem.
    budget_usd: Option<f64>,
    /// Kroki, których nie ruszyliśmy, bo sufit był już przekroczony — dokładne zdanie na krok.
    ///
    /// Zdanie, nie bit, bo równoległy krok może dopisać koszt po pierwszej odmowie. Potomek ma
    /// dostać ten sam powód, który dostał zatrzymany rodzic, a nie późniejszy rachunek końcowy.
    /// Osobne pole, a nie odczyt z księgi, bo stany końcowe kroków wpisuje na końcu planista
    /// ([`Live::close_the_book`]) i przykryłby to, co zapisał tu bieg. `std::sync::Mutex`
    /// i nigdy trzymany przez `await` (niezmiennik 8).
    stopped_by_the_budget: Mutex<Vec<Option<String>>>,
    /// Ile z sufitu jest ODŁOŻONE dla kroku, który właśnie pracuje — po jednej pozycji na krok.
    ///
    /// 2026-09 (Z-13b) — BEZ TEGO POLA SUFIT JEST MIĘKKI. Reszta liczyła się z tur, które już
    /// wróciły, więc przy `at_once = 3` każdy z trzech startujących obok siebie kroków dostawał
    /// tę samą, pełną resztę i bieg mógł wydać około trzykrotności kwoty, którą postawił
    /// człowiek — dowiadując się o tym z rachunku. Odłożona kwota liczy się jako rozdysponowana
    /// od chwili, w której krok dostał miejsce z puli, aż do jego zejścia.
    ///
    /// `None` na pozycji znaczy „ten krok nie trzyma w tej chwili niczego": tak zaczyna każdy,
    /// tak kończy każdy ([`Live::give_back_the_share`]), i tak zostaje krok, którego bieg nie ma
    /// sufitu. Kwota MALEJE o prawdziwą cenę wróconej tury ([`Live::settle_the_share`]) i znika
    /// dopiero przy zejściu kroku, bo dopiero wtedy wiadomo, że ten krok już nic nie wyda.
    ///
    /// Osobny zamek, jak [`Live::stopped_by_the_budget`] obok: to nie jest stan, który jedzie
    /// do `run.json` — trwałym śladem wydatku jest cena tury. `std::sync::Mutex` i nigdy
    /// trzymany przez `await` (niezmiennik 8).
    share_of_the_budget: Mutex<Vec<Option<f64>>>,
    /// Chwila startu biegu. Kurator dostaje czas **argumentem**, bo kurator z własnym zegarem
    /// nie da się przetestować bez `sleep`.
    began: Instant,
    /// Gdzie leży przekazanie każdego kroku i jego pełna kopia, jeśli powstała — po jednym
    /// wpisie na krok, w kolejności z pliku workflow. `None` znaczy „ten krok jeszcze nic nie
    /// oddał": kafelek kontrolny nie oddaje nigdy, a krok anulowany albo padnięty nie ma czego
    /// przekazać.
    ///
    /// Zamek osobny od [`Live::book`] z rozmysłem: to nie jest stan, który jedzie do `run.json`.
    /// Ścieżka przekazania **jest** w plikach — nazwa pliku otwiera się numerem kroku — więc
    /// druga kopia w `run.json` byłaby drugim miejscem, w którym mieszka jeden fakt
    /// (niezmiennik 13), i tym, które kłamie po pierwszej ręcznej edycji katalogu.
    ///
    /// **Nie przechodzi przez `await`** (niezmiennik 8): oba wywołania, które go biorą
    /// ([`Live::filed`], [`Live::handed_before`]), oddają go w tym samym wyrażeniu.
    handoffs: Mutex<Vec<Option<handoff::Written>>>,
    /// Kroki, które NIE przeszły, a mimo to przepuściły robotę dalej — po jednej pozycji na krok.
    ///
    /// 2026-08-23 (T-87) — jedyny czytelnik jest jeden: etykieta wiersza w indeksie następnego
    /// kroku ([`WhatItIs::StepThatFailed`]). Bez tego pola krok stojący za `carry-on` dostaje plik
    /// nie do odróżnienia od materiału, który ktoś przyjął — a agent, który tego nie wie, buduje
    /// na odrzuconej robocie i nazywa to wynikiem.
    ///
    /// Zapisuje wyłącznie [`Live::when_this_one_fails`], czyli to samo jedno miejsce, które
    /// rozstrzyga o każdej porażce. Osobny zamek, jak [`Live::handoffs`] obok, i z tego samego
    /// powodu: to nie jest stan, który jedzie do `run.json`.
    did_not_pass: Mutex<Vec<Option<String>>>,
    /// Proza, którą krok zdążył powiedzieć w swojej turze — po jednej pozycji na krok, sklejona
    /// w kolejności, w jakiej padła. Pusty napis znaczy „ten krok nie powiedział jeszcze nic".
    ///
    /// 2026-08-23 (T-87) — TO JEST JEDYNA KOPIA TEGO TEKSTU PO STRONIE BIEGU. `AgentEvent::Said`
    /// dociera wyłącznie do kuracji ([`forward`]), czyli na ekran, a `StepRun::summary` powstaje
    /// dopiero z UDANEJ tury. Krok, którego tura wróciła błędem, nie miał więc ani jednego
    /// nośnika dla tego, co zdążył napisać — i zostawiał po sobie pusty plik, choć powiedział
    /// pół strony.
    ///
    /// Pisze [`forward`], czyta [`Live::hand_on_its_last_words`]. Osobny zamek, jak
    /// [`Live::handoffs`] obok, i z tego samego powodu: to nie jest stan, który jedzie do
    /// `run.json` — jego trwałym śladem jest plik przekazania.
    said_so_far: Mutex<Vec<String>>,
    /// Runda, w której pętla się DOMKNĘŁA — po jednej pozycji na pętlę planu, w tej samej
    /// kolejności co [`Plan::loops`]. `None` na pozycji znaczy „ta pętla jeszcze nie przeszła".
    ///
    /// 2026-08-22 — WEKTOR, NIE JEDNO POLE. Przy dwóch pętlach jedno pole znaczyłoby, że werdykt
    /// `pass` w gałęzi frontowej pomija rundy gałęzi backendowej — czyli praca, której nikt nie
    /// sprawdził, jedzie dalej jako zrobiona. Czytane przed każdym krokiem ciała pętli i przez to
    /// jedyny nośnik faktu „dalszych rund TEJ pętli już nie potrzebujemy".
    settled_at: Mutex<Vec<Option<u8>>>,
    /// Klucz węzła, który OSTATNI stanął do pracy w tym katalogu — po jednym wpisie na katalog.
    ///
    /// 2026-08-29 — ZNACZNIK POCHODZENIA KOPII, i bez niego składanie nie ma jak zauważyć, że
    /// zbiera pracę z dwóch różnych rund. Plan mówi tylko, KTO miał w tej kopii pracować
    /// ([`Planned::folds_in`] powstaje przy planowaniu), a to jest za mało: gałąź pominięta
    /// w rundzie drugiej zostaje z pracą rundy pierwszej, bo rundy dzielą folder
    /// ([`work_key_for`]) — i złożenie jej z gałęzią, która w rundzie drugiej naprawdę
    /// pracowała, jest nie do odróżnienia od poprawnego. To pole zapisuje, kto tam pracował
    /// NAPRAWDĘ.
    ///
    /// Klucz jest KATALOGIEM: to
    /// katalog jest tym, co rundy dzielą, a dwie kopie jednego kafelka mają dwa różne.
    /// `std::sync::Mutex` i nigdy trzymany przez `await` (niezmiennik 8): obaj wołający oddają
    /// go w tym samym wyrażeniu.
    lineage: Mutex<BTreeMap<PathBuf, String>>,
    /// Wynik kroku używany wyłącznie przez zapisane warunki jego strzałek.
    route_evidence: Mutex<Vec<Option<RouteEvidence>>>,
    /// Trwały dowód wyboru, kopiowany do `run.json` przy każdym zrzucie.
    route_decisions: Mutex<Vec<RouteDecision>>,
    /// 2026-09-08 — przypięty rodzic i zakres; zamek nie przechodzi przez `await`.
    plan_turns: Mutex<Vec<Option<crate::work_plan::Prepared>>>,
    /// 2026-09-08 — wersja oddana dopiero po pełnym sukcesie; nigdy sam przypięty input.
    plan_outputs: Mutex<Vec<Option<crate::work_plan::PlanVersion>>>,
}

/// Zmienna połowa biegu — dokładnie to, co zmienia się między zrzutami `run.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum SavedCopy {
    Git {
        oid: String,
    },
    Folder {
        path: PathBuf,
        origin: Option<String>,
        digest: Option<String>,
    },
    Unchanged {
        origin: String,
    },
    #[serde(other)]
    Unavailable,
}

struct Book {
    /// Usługa jeszcze korzysta z kopii albo trwa jej końcowy zapis; chroni też przed retencją.
    pending_finalization: BTreeSet<String>,
    /// Czytelnik wznowienia korzysta z dokładnego wyniku, nie z obecnej końcówki gałęzi.
    copy_results: BTreeMap<String, SavedCopy>,
    /// Stan **biegu**. Jedyne miejsce, w którym istnieje `paused`.
    status: RunState,
    /// Czy bieg stoi na pytaniu do człowieka.
    ///
    /// 2026-08-17 (T-31) — powody, dla których bieg stoi, są od teraz DWA i mijają niezależnie:
    /// to pytanie i limit dostawcy. Bez tego pola oba pisałyby `status` bezwarunkowo i kasowały
    /// się nawzajem — ten, który skończył pierwszy, ogłaszałby bieg jako idący, choć drugi wciąż
    /// go trzyma. Do `run.json` to pole nie wychodzi: `RunFile` bierze z księgi sam `status`,
    /// a dwa pola o jednym fakcie na ekranie są dokładnie tym, czego zabrania niezmiennik 13.
    asking: bool,
    /// Kiedy ruszył pierwszy krok.
    started_at: Option<i64>,
    /// Kiedy skończył się ostatni.
    ended_at: Option<i64>,
    /// Po jednym wpisie na krok, w kolejności z pliku workflow.
    steps: Vec<StepRun>,
    /// Zamrożone fakty notatek plus listy fizycznych kroków dopisywane po udanym starcie.
    memory: Vec<MemoryRecord>,
    /// Jedyny trwały fakt o prywatnej turze po biegu.
    reflection: ReflectionReceipt,
}

/// Co bieg wie o jednym kroku.
#[derive(Debug, Clone)]
struct StepRun {
    assessment: Option<crate::engine::drivers::command::assessment::Assessment>,
    end_cause: Option<super::run_inputs::EndCause>,
    result_files: Option<super::run_inputs::ResultFiles>,
    /// 2026-09-08 — wersja kroku; dokument nadal ma jedną kopię w `plans/`.
    plan_version: Option<WorkPlanReceipt>,
    /// Stan kroku. `paused` tu nie istnieje i nie ma go w [`StepState`] — to jest stan biegu.
    status: StepState,
    execution: ExecutionFacts,
    /// Dlaczego planner musiał domknąć węzeł, którego produkt nie uruchomił. Osobno od stanu:
    /// `Succeeded` nadal odblokowuje graf, a ten fakt mówi prawdę w `run.json` (2026-09, Z-33).
    not_run_because: Option<String>,
    /// Co sędzia powiedział o tej rundzie. `None` dla kroków poza sędzią i rund, które nie
    /// ruszyły; stan kroku pozostaje osobnym faktem.
    round_outcome: Option<handoff::Verdict>,
    /// Kiedy krok ruszył.
    started_at: Option<i64>,
    /// Kiedy się skończył.
    ended_at: Option<i64>,
    /// Proces potomny, jeśli sterownik go miał.
    pid: Option<i32>,
    /// Grupa procesów — to po niej sprząta odzyskiwanie po awarii (T-20).
    pgid: Option<i32>,
    /// **Każda** grupa, którą ten krok uruchomił, razem z grupą lidera.
    ///
    /// 2026-09 (Z-01d) — OBOK `pgid`, nigdy zamiast (niezmiennik 25). Tamto pole jest adresem
    /// sesji i zapisuje się je przed pierwszym zdarzeniem; to jest spisem tego, co po kroku
    /// zostało, i powstaje dopiero przy zejściu, bo dopiero wtedy wiadomo, co krok zdążył odpalić.
    /// Bez niego z kroku zostawała po awarii jedna liczba, a `claude` uruchamia każdą komendę
    /// narzędzia Bash we własnej grupie — czyli wszystko, co naprawdę pali limit, leżało poza
    /// sprzątaniem.
    pgids: Vec<i32>,
    /// Kod wyjścia.
    exit_code: Option<i32>,
    /// Czy supervisor dostał z jądra dowód, że cała grupa procesu nie żyje.
    ///
    /// 2026-08-28 — DO TEGO DNIA `false` NIE ZNACZYŁO „ŻYJE": naturalne `close()` zbierało
    /// lidera i nie produkowało [`GroupProof`] ani razu, więc każdy udany krok kończył się tu
    /// fałszem, którego nie dało się odróżnić od ocalałego. Teraz dowód pada na KAŻDEJ ścieżce
    /// terminalnej — Stop, limit czasu, udana tura i udana komenda — a `false` znaczy dokładnie
    /// to, co mówi niezmiennik 6: nikt nie usłyszał `ESRCH`, więc grupę traktujemy jak żywą.
    ///
    /// Jedyny wyjątek jest widoczny z pliku workflow, nie z tego pola: kafelek „uruchom
    /// i zostaw" ma zostawić żywy proces z rozmysłu i nie przechodzi ani przez `one_turn`, ani
    /// przez `run_check`.
    death_proof: bool,
    /// Ile kosztował.
    cost_usd: Option<f64>,
    /// `Some(true)` tylko dla kwoty policzonej z liczników Codeksa; brak oznacza pomiar albo
    /// brak ceny i dzięki temu `run.json` nie nazywa nieznanej kwoty szacunkiem.
    cost_estimate: Option<bool>,
    /// Rzeczywiste liczniki z terminalnego [`crate::engine::drivers::Outcome`]. `None` znaczy,
    /// że krok nie dostał wyniku agenta (np. Check albo odmowa przed startem), nie zero.
    /// Nazwy są wspólne dla obu vendorów od granicy sterownika (2026-09, Z-48).
    uncached_input: Option<u64>,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
    output: Option<u64>,
    vendor_turns: Option<u32>,
    /// Jedna linia dla szyny agentów.
    summary: Option<String>,
    /// Powód, jeśli coś poszło nie tak.
    error: Option<String>,
    /// Numer sygnału, którym ten krok zszedł, **choć Loadout żadnego nie wysłał**.
    ///
    /// 2026-09 (Z-39) — OSOBNO OD `error`, choć zdanie dla człowieka powstaje z tej liczby.
    /// Tamto pole jest tekstem i przez to nie odpowiada na pytanie „co się stało" inaczej niż
    /// przez porównywanie napisów; ten numer jest faktem, który przeżywa skasowanie
    /// `loadout.db` (niezmiennik 4) i po nim jednym poznaje się, że bieg meetnotes
    /// `20260901-150035` nie padł sam — dostał `kill -TERM` spoza aplikacji.
    ///
    /// `None` znaczy „nikt z zewnątrz tego kroku nie tknął", nie zero.
    stopped_from_outside: Option<i32>,
    /// Czyjego wyniku ten krok NIE MA, choć jedzie dalej — po jednym zdaniu na poprzednika.
    ///
    /// 2026-09 (Z-39) — powstaje wyłącznie przy `carry-on` (i przy `ask-me` → „jedź dalej"),
    /// czyli wtedy, gdy graf puszcza dalej krok, którego materiał nie istnieje. Pusta lista
    /// znaczy, że ten krok dostał wszystko, po co przyszedł — i to jest odpowiedź, nie brak.
    ran_without: Vec<String>,
    /// Nagłówki, których agent nie napisał, a `memory::handoff::reshape` je za niego wstawił.
    ///
    /// Pusta lista znaczy, że odpowiedź przyszła w umówionym kształcie — i to jest odpowiedź,
    /// a nie brak odpowiedzi (powód przy [`StepEntry::repaired`]).
    repaired: Vec<String>,
    /// Czy odpowiedź nie zmieściła się w `BODY_CAP` i część leży w `attachments/`.
    truncated: bool,
    /// Co aplikacja agenta wczytała z folderu sama z siebie, zanim krok powiedział słowo.
    ///
    /// `None` znaczy „ten krok o tym nic nie powiedział" i tak zostaje: kafelek „sprawdź" nie
    /// woła agenta, Codex tego nie ogłasza, a krok zdjęty przed pierwszym zdarzeniem nie zdążył.
    /// Pusty rekord wpisany na siłę mówiłby to samo jednym kluczem więcej w każdym kroku każdego
    /// biegu w historii — ta sama decyzja, co przy [`StepRun::repaired`].
    loaded_by_the_app: Option<LoadedByTheApp>,
    /// Co przegląd zauważył w tekście, który ten krok pożyczył z projektu — i przepuścił.
    ///
    /// 2026-09 (Z-21) — ZNANE PRZED PIERWSZYM ZDARZENIEM, nie po nim: pożyczki przepisuje
    /// `bring_in_what_each_step_borrowed`, zanim powstanie ta księga, więc jako jedyne pole
    /// [`StepRun`] wchodzi wypełnione (`Live::new`). Ciężkie znalezisko nie ma jak tu dojechać —
    /// zabrało cały bieg przy planowaniu.
    borrowed_concerns: Vec<BorrowedConcern>,
}

/// 2026-09-08 — trwały adres wersji w wyniku kroku, bez drugiej kopii całego dokumentu.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkPlanReceipt {
    document_id: String,
    version: u64,
    version_id: String,
}

impl From<&crate::work_plan::PlanVersion> for WorkPlanReceipt {
    fn from(version: &crate::work_plan::PlanVersion) -> Self {
        Self {
            document_id: version.document_id.clone(),
            version: version.version,
            version_id: version.version_id.clone(),
        }
    }
}

/// Co aplikacja agenta dobrała sobie z folderu kroku — zapisywane do `run.json`.
///
/// # Dlaczego to jest własny typ, a nie [`crate::evidence::ContextSource`]
///
/// Tamten opisuje JEDNO źródło trzema polami (`kind`, `reference`, `bytes`) i tak ma zostać: on
/// jedzie do prywatnego manifestu wejścia, gdzie liczy się rozmiar materiału, który wszedł do
/// promptu. Tutaj rozmiaru nie znamy — CLI podaje nazwy, nie bajty — a pytanie jest inne: nie
/// „ile tego było", tylko „co to było". Rodzaj bierzemy z tamtej zamkniętej listy
/// ([`ContextKind::LoadedByTheApp`]), bo to jest jedno miejsce z odpowiedzią na pytanie „skąd
/// wziął się kontekst tego kroku" (niezmiennik 13).
///
/// **Nazwy pól są `snake_case` i to jest kontrakt z `store::rebuild`**, dokładnie jak reszta
/// [`StepEntry`]; `kind` serializuje się `camelCase`, bo to jest kształt [`ContextKind`], nie
/// nasz schemat pliku.
#[derive(Debug, Clone, Serialize)]
struct LoadedByTheApp {
    kind: ContextKind,
    /// Sama nazwa katalogu, nigdy pełna ścieżka: bezwzględna ścieżka z katalogu domowego
    /// człowieka w pliku, który zostaje po biegu, jest tym, czego `evidence::validate_manifest`
    /// odmawia manifestowi wejścia (2026-09, Z-16).
    folder: String,
    plugins: Vec<String>,
    slash_commands: Vec<String>,
    skills: Vec<String>,
    mcp_servers: Vec<String>,
    memory_paths: Vec<String>,
    agents: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct ExecutionFacts {
    /// Ciało logicznej próby minęło wszystkie skróty „nie ma pracy” i naprawdę się zaczęło.
    executed: bool,
    /// Produkcyjny start oddał ownership procesu; nie wynika z PID-u ani statusu kroku.
    process_started: bool,
}

/// Zużycie WSZYSTKICH tur, które ten krok naprawdę obsłużył.
///
/// L-01 (2026-09-06) — do dnia, w którym krok mógł mieć więcej niż jedną turę, zapis zużycia
/// był podstawieniem: `step.cost_usd = turn.cost_usd`. Druga tura przepisywała rachunek
/// pierwszej, więc człowiek widział cenę ostatniego zdania zamiast ceny kroku. Sumowanie musi
/// mieszkać w jednym miejscu, bo to jest polityka, nie arytmetyka rozsypana po ścieżkach
/// zejścia (niezmiennik 23).
#[derive(Debug, Default, Clone)]
struct TurnTotals {
    cost_usd: Option<f64>,
    tokens: crate::engine::drivers::Tokens,
    /// Wewnętrzne tury zgłoszone przez vendora, zsumowane.
    vendor_turns: u32,
    /// Ile tur obsłużył Loadout. Zero znaczy „krok miał dokładnie jedną turę".
    handled: u32,
}

impl TurnTotals {
    fn add(&mut self, one: &DriverOutcome) {
        if let Some(spent) = one.cost_usd {
            self.cost_usd = Some(self.cost_usd.unwrap_or(0.0) + spent);
        }
        self.tokens.uncached_input += one.tokens.uncached_input;
        self.tokens.cache_read += one.tokens.cache_read;
        self.tokens.cache_write += one.tokens.cache_write;
        self.tokens.output += one.tokens.output;
        self.vendor_turns += one.turns;
        self.handled += 1;
    }

    /// Te totals powiększone o jeszcze jedną turę, bez zmiany oryginału.
    fn with(&self, one: &DriverOutcome) -> Self {
        let mut both = self.clone();
        both.add(one);
        both
    }

    fn record(&self, step: &mut StepRun, cost_is_estimate: bool) {
        step.cost_usd = self.cost_usd;
        step.cost_estimate = (self.cost_usd.is_some() && cost_is_estimate).then_some(true);
        step.uncached_input = Some(self.tokens.uncached_input);
        step.cache_read = Some(self.tokens.cache_read);
        step.cache_write = Some(self.tokens.cache_write);
        step.output = Some(self.tokens.output);
        // 2026-09 (Z-48) — adapter bez licznika tur zapisuje zgodnościowe zero, a zero na
        // trwałej granicy jest brakiem, nie liczbą.
        step.vendor_turns = (self.vendor_turns > 0).then_some(self.vendor_turns);
    }
}

/// Stan **biegu**: pięć wartości z `CHECK` przy tabeli `runs` w `store::schema`.
///
/// Szóstej — `interrupted` — stąd nie da się napisać i tak ma być: wpisuje ją odzyskiwanie po
/// awarii aplikacji (T-20), przy starcie, biegom, które nie miały jak dokończyć. Bieg, który
/// sam siebie melduje jako przerwany, to bieg, który jeszcze żyje.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RunState {
    /// Bieg idzie.
    Running,
    /// Bieg stoi na punkcie kontrolnym i czeka na człowieka.
    Paused,
    /// Koniec, wszystko się udało.
    Succeeded,
    /// Koniec, coś padło.
    Failed,
    /// Koniec, bo zatrzymał go człowiek.
    Cancelled,
}

/// Jedno wejście indeksu: przekazanie jednego poprzednika, gotowe do wpisania w prompt.
#[derive(Debug)]
struct Handed {
    /// Nazwa kafelka, który to oddał. Ta sama, która stoi na ekranie jako etykieta wiersza —
    /// prompt nazywa więc krok tym samym słowem, co UI (niezmiennik 13). Identyfikator kroku
    /// ani uuid agenta nie mają tu czego szukać (niezmiennik 14).
    from: String,
    /// Gdzie ten plik leży, **bezwzględnie**: katalogiem roboczym kroku `fresh-copy` jest
    /// `work/<krok>`, więc ścieżka względna katalogu biegu nie rozwiązałaby się z miejsca,
    /// w którym agent naprawdę stoi.
    path: PathBuf,
    /// Pełna kopia tej samej odpowiedzi, kiedy ciało przekazania zostało ucięte.
    ///
    /// Nie trafia do trwałego pliku: prompt składa bezwzględny adres z kopii bieżącego biegu,
    /// więc przeniesienie lub wznowienie nie zostawia w nim starego katalogu.
    attachment: Option<PathBuf>,
    /// 2026-09 (Z-41) — długość pełnej kopii zmierzona przez TEN bieg przy publikacji. Przejęty
    /// plik ma `None`, bo bieżący bieg nie widział jego zapisu i nie może zgadywać kotwicy.
    attachment_bytes: Option<usize>,
    /// Udany poprzednik zapisał poprawne przekazanie, ale żadna jego sekcja nie niesie treści.
    left_nothing: bool,
    /// Czym ten plik jest dla kroku, który go czyta. Powód całego pola stoi przy
    /// [`IS_WHAT_THE_STEP_BEFORE_LEFT`].
    what: WhatItIs,
}

/// Konkretna odmowa budowy indeksu, którą wolno pokazać człowiekowi słowo w słowo.
#[derive(Debug, thiserror::Error)]
#[error("Handoff {name} was changed after {from} published it.")]
struct HandoffChangedAfterPublication {
    name: String,
    from: String,
}

impl HandoffChangedAfterPublication {
    fn for_handed(hand: &Handed) -> Self {
        Self {
            name: hand.path.file_name().map_or_else(
                || hand.path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            from: hand.from.clone(),
        }
    }
}

/// Konkretna odmowa przygotowania planu pracy, którą wolno pokazać człowiekowi słowo w słowo.
///
/// 2026-09-08 — bez osobnego typu wspólna granica błędów kontekstu zastępowała nazwany brak
/// planu ogólnym komunikatem, chociaż kolejny krok został poprawnie zatrzymany przed procesem.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct WorkPlanRefused(String);

/// Czym jest plik wymieniony w indeksie — z punktu widzenia kroku, który ten indeks czyta.
///
/// Zamknięta lista, bo etykieta jest po to, żeby ROZRÓŻNIAĆ: nazwa kafelka i ścieżka mówią, skąd
/// plik pochodzi, a to jest za mało, kiedy dwa wiersze jednego indeksu przychodzą od tego samego
/// kroku z dwóch różnych rund.
///
/// Numer próby liczy się od jedynki, a nie od zera: `turn` jest polem danych, a to jest zdanie
/// dla człowieka i dla agenta — „try 0 of 3" nie znaczy nic ani dla jednego, ani dla drugiego.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WhatItIs {
    /// Krok, po którym ten krok idzie po strzałce.
    StepBefore,
    /// To samo, tylko tamten krok nie przeszedł i przepuścił robotę dalej.
    StepThatFailed,
    /// Wejście pętli: to, co dostała jej pierwsza runda.
    WhatYouStartedWith,
    /// Wcześniejsza runda TEGO kroku.
    YourOwnTry { which: u8, of: u8 },
    /// Wcześniejsza próba pracy, którą ocenia sędzia tej pętli.
    EarlierTryOfTheWork { which: u8, of: u8 },
    /// Wcześniejsza runda sędziego tej pętli.
    WhatTheTesterSaid { which: u8, of: u8 },
    /// Plik przejęty po biegu, od którego ten bieg wznowiono.
    FromAnEarlierRun,
}

impl WhatItIs {
    /// Zdanie, które staje w wierszu indeksu.
    fn said(self) -> String {
        match self {
            Self::StepBefore => IS_WHAT_THE_STEP_BEFORE_LEFT.to_owned(),
            Self::StepThatFailed => IS_WHAT_A_STEP_THAT_FAILED_LEFT.to_owned(),
            Self::WhatYouStartedWith => IS_WHAT_YOU_STARTED_WITH.to_owned(),
            Self::YourOwnTry { which, of } => {
                format!("{IS_YOUR_OWN_EARLIER_ANSWER}, try {which} of {of}")
            }
            Self::EarlierTryOfTheWork { which, of } => {
                format!("{IS_AN_EARLIER_TRY_OF_THE_WORK}, try {which} of {of}")
            }
            Self::WhatTheTesterSaid { which, of } => {
                format!("{IS_WHAT_THE_TESTER_SAID}, try {which} of {of}")
            }
            Self::FromAnEarlierRun => IS_WHAT_AN_EARLIER_RUN_LEFT.to_owned(),
        }
    }
}

/// Jeden plik, który zostawił POPRZEDNI bieg — już skopiowany do katalogu tego biegu.
///
/// Osobny typ od [`Handed`], bo odpowiada na inne pytanie: `Handed` powstaje z numeru kroku
/// TEGO biegu ([`Live::filed`]), a tutaj żaden krok tego biegu nie ma numeru, pod którym można
/// by ten plik znaleźć. Sklejenie obu w jeden wektor po `StepId` znaczyłoby, że przejęty plik
/// musi udawać krok, którego w tym biegu nie ma.
#[derive(Debug, Clone)]
struct Carried {
    /// Nazwa kroku, który go napisał — dosłownie ta z jego front-mattera, czyli ta, którą tamten
    /// bieg pokazywał na ekranie.
    from: String,
    /// Kopia w katalogu **tego** biegu. Skończony bieg jest historią i nie ma prawa się zmienić
    /// dlatego, że ktoś go wznowił (niezmiennik 4), więc prompt nigdy nie wskazuje w jego katalog.
    path: PathBuf,
    /// Pełna kopia pod katalogiem bieżącego biegu, nigdy adresem biegu źródłowego.
    attachment: Option<PathBuf>,
}

/// Co krok dostaje na wejściu: prompt, ślad po tym, co do niego wstrzyknięto, i katalogi,
/// które musi móc otworzyć.
#[derive(Debug)]
struct Told {
    /// Instrukcja kroku plus indeks przekazań poprzedników. Jedzie stdinem (niezmiennik 9).
    prompt: String,
    /// Dokładnie to, co Loadout wstrzyknął — nie to, co agent twierdzi, że przeczytał.
    /// Pochodzenie, o którym nie da się skłamać [T6 §10.2]; wchodzi jako `reads` do przekazania
    /// **tego** kroku.
    ///
    /// Ścieżki względem katalogu biegu, a nie bezwzględne jak w prompcie: przekazanie jest
    /// plikiem, który przeżywa `cp -r` katalogu biegu (niezmiennik 4), a ścieżka z `/var/folders`
    /// w środku przestaje po takiej kopii cokolwiek znaczyć.
    reads: Vec<String>,
    /// Wszystkie źródła wejścia w rzeczywistej kolejności kompozycji, bez ich treści.
    context: Vec<ContextSource>,
    /// Katalog przekazań, kiedy krok ma co czytać. Pusty, kiedy nie ma: `--add-dir` na katalog,
    /// w którym nic dla tego kroku nie leży, poszerza mu dostęp bez powodu.
    extra_dirs: Vec<PathBuf>,
}

/// 2026-09-08 — wspólna kotwica integralności kandydata i opublikowanego przekazania.
fn regular_file_length(path: &Path) -> io::Result<usize> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::other("the path is not one regular file"));
    }
    usize::try_from(metadata.len())
        .map_err(|_| io::Error::other("the file length does not fit on this machine"))
}

/// 2026-09-08 — oba odczyty używają tej samej kontroli typu pliku, limitu i długości.
fn plan_candidate_bytes(
    cwd: &Path,
    prepared: &crate::work_plan::Prepared,
) -> Result<Vec<u8>, crate::work_plan::Error> {
    let path = Live::room_for_the_answer(cwd, prepared.candidate())
        .map_err(|error| crate::work_plan::Error::NotDelivered(error.to_string()))?;
    let before = regular_file_length(&path).map_err(|error| {
        let reason = if error.kind() == io::ErrorKind::NotFound {
            "the candidate file is missing".to_owned()
        } else {
            format!("the candidate file could not be read safely: {error}")
        };
        crate::work_plan::Error::NotDelivered(reason)
    })?;
    if before > crate::work_plan::MAX_CANDIDATE_BYTES {
        return Err(crate::work_plan::Error::NotDelivered(format!(
            "the candidate is larger than {} bytes",
            crate::work_plan::MAX_CANDIDATE_BYTES
        )));
    }
    let bytes = fs::read(&path).map_err(|error| {
        crate::work_plan::Error::NotDelivered(format!(
            "the candidate file could not be read: {error}"
        ))
    })?;
    let after = regular_file_length(&path).map_err(|error| {
        crate::work_plan::Error::NotDelivered(format!(
            "the candidate file changed while Loadout read it: {error}"
        ))
    })?;
    if before != after || bytes.len() != before {
        return Err(crate::work_plan::Error::NotDelivered(
            "the candidate file changed while Loadout read it".to_owned(),
        ));
    }
    Ok(bytes)
}

impl Live {
    /// Świeży bieg: wszystkie kroki czekają, nic jeszcze nie ruszyło.
    fn new(
        mut plan: Plan,
        lines: LineSink,
        control: RunControl,
        slots: Limiter,
        processes: std::sync::Arc<crate::commands::processes::Processes>,
        budget_usd: Option<f64>,
        faults: Arc<dyn PrestartFaultInjector>,
    ) -> Self {
        // Kopia stanów kroków, którą dostaje limit dostawcy, jest **martwa z rozmysłem**:
        // `engine::limits::Run` ma pełny dostęp do statusów i podejść dokładnie po to, żeby
        // T-21 mogło dowieść, że pauza ich nie rusza (`[T7 §7.2]`: „a pause, not a failure").
        // Księga tego biegu żyje niżej, w [`Live::book`], i to ona jedzie do `run.json`.
        let gate = limits::Run::new(slots, &vec![StepState::Pending; plan.steps.len()]);
        let steps = plan
            .steps
            .iter()
            .map(|planned| StepRun {
                status: StepState::Pending,
                execution: ExecutionFacts::default(),
                end_cause: None,
                assessment: None,
                result_files: None,
                plan_version: None,
                not_run_because: None,
                round_outcome: None,
                started_at: None,
                ended_at: None,
                pid: None,
                pgid: None,
                pgids: Vec::new(),
                exit_code: None,
                death_proof: false,
                cost_usd: None,
                cost_estimate: None,
                uncached_input: None,
                cache_read: None,
                cache_write: None,
                output: None,
                vendor_turns: None,
                summary: None,
                error: None,
                stopped_from_outside: None,
                ran_without: Vec::new(),
                repaired: Vec::new(),
                truncated: false,
                loaded_by_the_app: None,
                // Fakt o pożyczonym tekście jest znany PRZED biegiem, a nie w jego trakcie:
                // przepisała go `bring_in_what_each_step_borrowed`, więc księga zaczyna z nim
                // w ręku. Krok, który nie pożyczał niczego, ma tu pustą listę i żadnego klucza
                // w `run.json`.
                borrowed_concerns: match &planned.job {
                    Job::Agent(job) => job.borrowed.concerns().to_vec(),
                    Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => Vec::new(),
                },
            })
            .collect();
        let stopped_by_the_budget = Mutex::new(vec![None; plan.steps.len()]);
        let share_of_the_budget = Mutex::new(vec![None; plan.steps.len()]);
        let handoffs = Mutex::new(vec![None; plan.steps.len()]);
        let did_not_pass = Mutex::new(vec![None; plan.steps.len()]);
        let said_so_far = Mutex::new(vec![String::new(); plan.steps.len()]);
        let settled_at = Mutex::new(vec![None; plan.loops.len()]);
        let route_evidence = Mutex::new(vec![None; plan.steps.len()]);
        let plan_turns = Mutex::new(vec![None; plan.steps.len()]);
        let plan_outputs = Mutex::new(vec![None; plan.steps.len()]);
        // Od tej chwili receipt jest częścią zmiennej księgi. Plan nie trzyma drugiej kopii,
        // która mogłaby rozjechać się z listami odbiorców podczas równoległych startów.
        let memory = std::mem::take(&mut plan.memory);
        let messages = Arc::new(crate::bridge::messages::Mailbox::new(
            super::lead_start::RunRef {
                workspace: plan.project.clone(),
                run_id: plan.id.clone(),
            },
            plan.dir.clone(),
            lines.clone(),
        ));
        Self {
            plan,
            native_ui: tokio::sync::OnceCell::new(),
            book: Mutex::new(Book {
                pending_finalization: BTreeSet::new(),
                copy_results: BTreeMap::new(),
                status: RunState::Running,
                asking: false,
                started_at: None,
                ended_at: None,
                steps,
                memory,
                reflection: ReflectionReceipt::default(),
            }),
            lines,
            messages,
            control,
            faults,
            gate,
            budget_usd,
            stopped_by_the_budget,
            share_of_the_budget,
            began: Instant::now(),
            handoffs,
            did_not_pass,
            said_so_far,
            settled_at,
            lineage: Mutex::new(BTreeMap::new()),
            route_evidence,
            route_decisions: Mutex::new(Vec::new()),
            plan_turns,
            plan_outputs,
            processes,
        }
    }

    /// Zamek na księdze. Zatruty odplatamy zamiast panikować: `panic!` w silniku zabiera cały
    /// bieg (AGENTS.md §4), a księga po panice jednego kroku jest dalej poprawna.
    fn book(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Pierwszy zrzut `run.json`. Jego błąd zatrzymuje bieg, bo bieg bez pliku nie ma historii.
    fn open_the_book(&self) -> Result<(Vec<u8>, PublicationIdentity), RunError> {
        let book = self.book();
        let bytes = self.run_file_bytes(&book)?;
        let (run_dir, run_file) = self.durable_run_location()?;
        let identity = DurableFilePublisher::new(&run_dir)
            .with_publication(|batch| {
                batch.atomic_create_if_absent_with_identity(
                    &run_file,
                    &bytes,
                    ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
                )
            })
            .map_err(PublishError::into_io)?;
        Ok((bytes, identity))
    }

    /// Zmienia księgę i **od razu** zrzuca ją na dysk — wszystko pod jednym zamkiem.
    ///
    /// Zapis siedzi pod zamkiem nie z ostrożności, tylko dlatego, że inaczej plik potrafi cofnąć
    /// się w czasie: dwa kroki kończące się w tej samej chwili budują JSON z dwóch różnych
    /// stanów, a wygrywa ten, który zdąży później do `rename`. Ogłoszona pauza nadpisana zrzutem
    /// sprzed pauzy jest awarią, której nikt nie zobaczy — bieg po prostu stoi, a plik mówi, że
    /// idzie.
    ///
    /// Błąd zrzutu w locie **loguje się i nie zatrzymuje biegu**: cztery żywe agenty to zły
    /// moment na przewracanie wszystkiego z powodu jednego nieudanego zapisu. Pierwszy zrzut
    /// jest inny i idzie przez [`Live::open_the_book`].
    /// Stan kroku **do okna**, jednym wierszem `stepState`.
    ///
    /// 2026-08-18 — PO CO TO ISTNIEJE. Stan kroku żył wyłącznie w księdze i w `run.json`, więc
    /// okno nie dostawało go nigdy: `RunState.steps` przychodziło z pliku workflow w chwili
    /// kliknięcia Start, z każdym krokiem na `pending`, i **zostawało tak do końca biegu**.
    /// Skutkiem nie była nieaktualna liczba: pasek loadoutu stał na samych obrysach, a kafelek
    /// agenta, który właśnie edytował pliki, pokazywał „waiting". Sześć z siedmiu stanów
    /// z `docs/ARCHITECTURE.md` §5 było po stronie okna NIEOSIĄGALNYCH.
    ///
    /// `node_key`, nie `id`: okno rozpoznaje swój kafelek po identyfikatorze **z pliku
    /// workflow**, bo z tego pliku powstał plan paska, zanim Rust powiedział pierwsze słowo
    /// (`src/state/run.ts`, `withStepStates` porównuje `step.id === line.stepId`). Świeży uuid
    /// biegu byłby tu kluczem, którego okno nigdy nie widziało.
    ///
    /// `name`, nie identyfikator, w polu `agent`: to ten sam podpis, którym ten krok mówi
    /// w każdym innym wierszu (`forward(…, plan.steps[id].name)`), więc szyna agentów nie
    /// dostaje dwóch nazw na jeden kafelek (niezmiennik 13).
    ///
    /// Wiersz idzie **poza kuratorem** i to jest wymóg, nie skrót: kuracja rozstrzyga, co
    /// człowiek czyta o CZYNNOŚCIACH agenta (niezmiennik 15), a to jest fakt o biegu. Puszczony
    /// przez sklejanie zniknąłby w grupie `read` albo poczekał na jej domknięcie — czyli pasek
    /// przestawiałby się z opóźnieniem względem tego, co widać w strumieniu.
    /// Dlaczego ten węzeł jest rundą pętli, która już się domknęła.
    ///
    /// Porównanie jest na NUMERZE RUNDY, nie na „czy pętla przeszła": rundy do tej, w której padł
    /// werdykt `pass`, naprawdę się wykonały i ich stan jest prawdziwy. Pomijamy wyłącznie to,
    /// co jest PO niej. Numer próby jedzie w odpowiedzi, bo dokładnie on zostaje później zapisany.
    fn not_run_because(&self, id: StepId) -> Option<String> {
        let step = &self.plan.steps[id];
        /* Tylko ciało pętli, i to TEJ pętli, do której ten węzeł należy. Krok spoza wszystkich
         * pętli ma rundę zero i nigdy nie zostałby pominięty, ale warunek stoi tu wprost, żeby
         * ten kod nie zależał od tego, jak `unroll` numeruje. */
        let which = step.in_loop?;
        if step.turn == 0 {
            return None;
        }
        let settled = self
            .settled_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        /* 2026-08-22 — POZYCJA TEJ PĘTLI, nie jedno wspólne pole. Do tego dnia wystarczyło
         * „runda > 0", bo pętla była jedna; przy dwóch to samo porównanie kasowałoby rundy
         * gałęzi, która niczego jeszcze nie przeszła. */
        settled
            .get(which)
            .copied()
            .flatten()
            .filter(|&turn| step.turn > turn)
            // `turn` jest zerowy wewnątrz planu, a trwałe zdanie czyta człowiek (2026-09,
            // Z-33). „try 0" byłoby technicznie spójne i produktowo fałszywe.
            .map(|turn| format!("loop settled at try {}", turn + 1))
    }

    fn has_routes(&self, id: StepId) -> bool {
        self.plan.routes.iter().any(|route| route.from == id)
    }

    fn remember_evidence(&self, id: StepId, evidence: RouteEvidence) {
        let mut all = self
            .route_evidence
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        all[id] = Some(evidence);
    }

    fn remember_handoff_evidence(&self, id: StepId, text: &str) {
        if !self.has_routes(id) {
            return;
        }
        self.remember_evidence(id, RouteEvidence::Handoff(fields_said_in(text)));
    }

    /// Umówione pole, którego w odpowiedzi zabrakło — gotowe zdanie, albo `None`.
    ///
    /// # Prośba bez skutku jest poleceniem bez handlera (niezmiennik 16)
    ///
    /// Blok „jak odpowiadać" wymienia pola i mówi, w jakim kształcie mają wrócić
    /// ([`FIELDS_ASKED_FOR`]). Bez tej drugiej połowy odpowiedź bez umówionego wiersza wygląda
    /// dokładnie jak odpowiedź, w której akurat nie było co w niego wpisać — i model uczy się,
    /// że tych wierszy można nie pisać wszędzie.
    ///
    /// PIERWSZE BRAKUJĄCE, nie wszystkie: zdanie nazywa jedno pole, bo człowiek czyta je na
    /// karcie kroku i naprawia jedną rzecz naraz. Pola bez `required` wolno pominąć i to jest
    /// cała różnica między formularzem a listą życzeń.
    ///
    /// Krok bez formularza nie ma czego oddawać, więc nie ma go za co sądzić: implementacja
    /// wymagająca wiersza `klucz: wartość` od KAŻDEGO kroku zaczerwieniłaby połowę biegów,
    /// o które nikt nie prosił.
    fn missing_a_required_field(&self, id: StepId, said: &str) -> Option<String> {
        let Job::Agent(job) = &self.plan.steps[id].job else {
            return None;
        };
        let given = fields_said_in(said);
        let missing = job
            .handover
            .iter()
            .filter(|field| field.required == Some(true))
            .find(|field| !given.contains_key(field.name.trim()))?;
        Some(format!(
            "This step was asked to hand back \"{}\" on a line of its own, and its answer has no \
             such line.",
            missing.name.trim()
        ))
    }

    /// Liczy drogę po kroku bez zapisu do księgi ani listy decyzji.
    ///
    /// 2026-08-25 (T-101) — TEN RACHUNEK MUSI DAĆ SIĘ ZROBIĆ PRZED ZIELONĄ LINIĄ KROKU.
    /// Gdy odmowa powstawała dopiero w callbacku planisty, krok zdążył już ogłosić sukces,
    /// a planista po cichu zapisywał porażkę i omijał `whenItFails`. Czysty rachunek pozwala
    /// [`Live::finish_this_step`] najpierw rozstrzygnąć odmowę wspólną polityką, a prawidłową
    /// drogę planista zapisuje później dokładnie raz.
    fn chosen_route_after(&self, id: StepId) -> Result<ChosenRoute, String> {
        let relevant: Vec<&PlannedRoute> = self
            .plan
            .routes
            .iter()
            .filter(|route| route.from == id)
            .collect();
        if relevant.is_empty() {
            return Ok(ChosenRoute {
                route: scheduler::Route::All,
                decision: None,
            });
        }
        let evidence = self
            .route_evidence
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
            .flatten();
        let links: Vec<ConditionalLink> = relevant.iter().map(|route| route.link.clone()).collect();
        let selected = match crate::workflow::select_branch(
            &links,
            &self.plan.steps[id].tile_key,
            evidence.as_ref(),
        ) {
            Ok(Some(selected)) => selected,
            Ok(None) => {
                return Ok(ChosenRoute {
                    route: scheduler::Route::All,
                    decision: None,
                });
            }
            Err(error) => return Err(error.to_string()),
        };
        let Some(route) = relevant
            .into_iter()
            .find(|route| route.link.to == selected.to)
        else {
            return Err("The selected next step is not in this run.".to_owned());
        };
        let Some(evidence) = evidence else {
            return Err(
                "This step did not produce the value needed to choose what runs next.".to_owned(),
            );
        };
        Ok(ChosenRoute {
            route: scheduler::Route::Only(vec![route.to]),
            decision: Some(RouteDecision {
                step_id: self.plan.steps[id].tile_key.clone(),
                to: self.plan.steps[route.to].tile_key.clone(),
                evidence,
            }),
        })
    }

    /// Oddaje planiście wyłącznie prawidłową drogę i zapisuje jej trwały paragon.
    fn route_after(&self, id: StepId) -> scheduler::Route {
        let chosen = match self.chosen_route_after(id) {
            Ok(chosen) => chosen,
            // Obrona przed zmianą dowodu między końcem kroku a callbackiem planisty. W zwykłym
            // biegu odmowę przejął wcześniej `finish_this_step`, więc ta gałąź nie jest polityką.
            Err(message) => return self.refuse_route(id, &message),
        };
        if let Some(decision) = chosen.decision {
            self.route_decisions
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(decision);
        }
        chosen.route
    }

    fn refuse_route(&self, id: StepId, message: &str) -> scheduler::Route {
        self.update(|book| book.steps[id].error = Some(message.to_owned()));
        scheduler::Route::Blocked
    }

    /// Zapisuje werdykt sędziego pętli i mówi, czy to była ostatnia szansa.
    ///
    /// Oddaje `true`, kiedy sędzia OSTATNIEJ rundy nie przepuścił roboty — wtedy krok ma wrócić
    /// `Failed`, żeby stożek za pętlą został `Skipped` i praca nie pojechała dalej na czymś, co
    /// nie przeszło. To jest cała treść limitu tur: bez tego wyczerpanie prób wyglądałoby jak
    /// sukces.
    fn verdict_after(
        &self,
        id: StepId,
        said: &str,
        native: Option<&crate::engine::native_ui::NativeUiAccess>,
    ) -> Option<&'static str> {
        let step = &self.plan.steps[id];
        let (which, the_loop) = self.judging(step)?;
        /* V-02 (incydent I-05): ZATWIERDZONA LISTA BIJE OSTATNI WIERSZ. Krok QA opisał braki
         * obowiązkowych zachowań i w tej samej odpowiedzi napisał `outcome: pass` — zgodnie
         * z instrukcją, którą dostał. Kiedy człowiek zatwierdził wymagania, wynik powstaje
         * z ich kompletności, a nie ze słowa, które model wybrał na końcu. */
        let approved = match &step.job {
            Job::Agent(job) => job.criteria.clone(),
            Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => Vec::new(),
        };
        let judged = (!approved.is_empty())
            .then(|| {
                let mut judged = crate::workflow::criteria::judge(&approved, said);
                /* P-03a: POTWIERDZENIE, KTÓREGO TA MASZYNA NIE MOGŁA WYKONAĆ, NIE JEST
                 * POTWIERDZENIEM. Zgoda na sterowanie cudzym oknem jest uprawnieniem systemu,
                 * przyznawanym przez człowieka; bez niej scenariusz natywny nie miał czym się
                 * odbyć, cokolwiek weryfikator o nim napisał. To nie jest też wada produktu —
                 * nikt niczego nie zmierzył — więc wynik idzie do „nie zmierzono", czyli
                 * do człowieka. */
                if native.is_some_and(|access: &crate::engine::native_ui::NativeUiAccess| {
                    !access.can_judge_the_product()
                }) {
                    crate::workflow::criteria::without_a_native_route(&approved, &mut judged);
                }
                judged
            })
            .filter(|one| one.outcome != crate::workflow::criteria::Outcome::Passed);
        let verdict = match &judged {
            Some(_) => crate::memory::handoff::Verdict::Fail,
            None => crate::memory::handoff::verdict_in(said),
        };
        // 2026-08-25 (T-100) — zapisujemy to samo rozstrzygnięcie, którym sterujemy pętlą,
        // zanim którakolwiek gałąź wróci. Osobne parsowanie dla `run.json` mogłoby pokazać
        // odmowę i jednocześnie domknąć rundę albo odwrotnie (niezmiennik 13).
        self.update(|book| book.steps[id].round_outcome = Some(verdict));
        if verdict == crate::memory::handoff::Verdict::Pass {
            self.settle(which, step.turn);
            return None;
        }
        if step.turn + 1 < the_loop.turns {
            return None;
        }
        /* 2026-08-23 — I POWÓD, BO BEZ NIEGO TEN KROK BYŁ CZERWONY BEZ ANI JEDNEGO ZDANIA.
         *
         * Do dziś `error` zostawało `null`, a jedynym śladem było `summary` ucięte do 240
         * bajtów — które przy sędzim piszącym prozą zaczynało się słowem „PASS". Człowiek
         * dostawał więc czerwony krok, którego podsumowanie mówi, że przeszedł.
         *
         * DWA ZDANIA, NIE JEDNO. Dla biegu „nie przepuścił" i „nic nie powiedział" są tym samym
         * — i tak zostaje. Dla człowieka to robota do poprawki kontra zepsuty kontrakt, czyli
         * dwie różne czynności. Jedno zdanie na oba stany kazałoby mu zgadywać, którą wykonać.
         */
        /* ZDANIE O WYMAGANIACH WYGRYWA Z OGÓLNYM „nie przepuścił", bo mówi CO ZROBIĆ:
         * wymaganie bez odpowiedzi jest brakiem raportu, wymaganie bez pomiaru jest brakiem
         * środowiska, a wymaganie niespełnione jest robotą do poprawki. Jedno zdanie na
         * wszystkie trzy kazałoby człowiekowi zgadywać, którą z nich wykonać. */
        if let Some(judged) = &judged {
            let said = judged.said();
            self.update(|book| book.steps[id].error = Some(said.clone()));
            return Some(WORK_LEFT_A_REQUIREMENT_OPEN);
        }
        let why = if crate::memory::handoff::said_an_outcome(said) {
            "The tester did not pass this work, and there were no tries left."
        } else {
            "The tester never said whether this work passed, so it counts as not passed. Its \
             answer has to end with a line saying `outcome: pass` or `outcome: fail`."
        };
        Some(why)
    }

    /// Co dzieje sie z biegiem, kiedy TEN krok nie przeszedl — jedno miejsce dla kazdej porazki.
    ///
    /// 2026-08-23 — ZAMOWIENIE WLASCICIELA: „workflows zawsze ma miec opcje kontynuacji a nie
    /// slepe punkty". Do dzis kazda porazka konczyla sie tak samo — `StepReport::Failed`, po
    /// ktorym planista malowal caly stozek potomkow na `skipped`, bez zdania i bez wyboru.
    ///
    /// JEDNA FUNKCJA NA WSZYSTKIE DROGI PORAZKI, i to jest cala jej tresc. Sedzia po wyczerpaniu
    /// prob, agent ktory sie przewrocil i komenda ktora nie przeszla roznily sie tylko tym,
    /// KTORE zdanie zapisza — a co do skutku byly jednym slepym punktem. Druga kopia tej decyzji
    /// przy ktorejkolwiek z nich rozjechalaby sie z pierwsza (niezmiennik 13).
    ///
    /// POWOD ZAPISUJEMY ZAWSZE, takze przy `Stop`: krok czerwony bez zdania jest tym, na co
    /// wlasciciel patrzyl przez cale wczoraj. `get_or_insert` nie nadpisuje powodu, ktory ktos
    /// zapisal wczesniej i wie wiecej — na przyklad o niekompletnym dowodzie.
    async fn when_this_one_fails(&self, id: StepId, why: &str) -> StepReport {
        let chosen = self.plan.steps[id].when_it_fails;
        let said = match chosen {
            WhenItFails::Stop => why.to_owned(),
            WhenItFails::CarryOn => {
                format!("{why} The steps after it were set to carry on anyway.")
            }
            WhenItFails::AskMe => format!("{why} You were asked what to do next."),
        };
        self.update(|book| {
            let _ = book.steps[id].error.get_or_insert(said);
        });

        match chosen {
            WhenItFails::Stop => StepReport::Failed,
            WhenItFails::CarryOn => self.and_still_hands_something_on(id),
            /* PYTAMY TA SAMA DROGA, CO KAFELEK KONTROLNY. `wait_for_a_person` bierze `StepId`,
             * a nie rodzaj kroku, wiec parkowania biegu nie trzeba pisac drugi raz — a odpowiedz
             * czlowieka staje sie przekazaniem tego kroku, czyli dociera do nastepnego.
             *
             * Odpowiedz znaczy „jedz dalej", a NIE „to sie udalo": krok zostaje czerwony, bo
             * naprawde nie przeszedl. Stop w tym miejscu zostaje anulowaniem — to jest ta sama
             * odpowiedz, ktora Stop znaczy wszedzie indziej w tej aplikacji. */
            WhenItFails::AskMe => {
                match self
                    .wait_for_a_person(id, Some(&self.what_now(id, why)))
                    .await
                {
                    StepReport::Succeeded => self.and_still_hands_something_on(id),
                    other => other,
                }
            }
        }
    }

    /// Krok nie przeszedl, a robota jedzie dalej — wiec zostawia po sobie plik i jest w indeksie
    /// nastepnego kroku oznaczony jako to, czym jest.
    ///
    /// 2026-08-23 (T-87) — CICHA LUKA W INDEKSIE JEST GORSZA NIZ ZLA WIADOMOSC. Nastepny krok
    /// czyta przekazania swoich rodzicow i nic wiecej, wiec krok, ktory padl i kazal jechac dalej,
    /// znikal z jego indeksu bez sladu — a brak wiersza wyglada dokladnie tak samo jak galaz,
    /// ktorej nigdy nie bylo. Agent budowal na materiale, ktorego nikt nie przyjal, i nie mial
    /// jak sie o tym dowiedziec.
    ///
    /// `Stop` tedy NIE chodzi, i to jest jedyny wyjatek: za nim nie biegnie nic, wiec nie ma komu
    /// tego pliku przeczytac (niezmiennik 21).
    fn and_still_hands_something_on(&self, id: StepId) -> StepReport {
        let failed_because = self
            .book()
            .steps
            .get(id)
            .and_then(|step| step.error.clone())
            .unwrap_or_else(|| "This step did not pass.".to_owned());
        if let Some(slot) = self
            .did_not_pass
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(id)
        {
            *slot = Some(failed_because);
        }
        self.tell_the_next_ones_what_they_are_missing(id);
        self.hand_on_its_last_words(id);
        StepReport::FailedAndCarriedOn
    }

    /// Krokom po strzałce mówi, czyjego wyniku nie dostaną — na karcie i w strumieniu.
    ///
    /// # 2026-09 (Z-39) — po co, skoro po kroku zostaje plik
    ///
    /// Bo po TYM kroku plik jest pusty i nikt tego nie widzi. `hand_on_its_last_words` niżej
    /// oddaje to, co krok zdążył powiedzieć — a krok ubity z zewnątrz nie zdążył powiedzieć nic.
    /// Następny dostaje więc wiersz w indeksie i zero treści pod nim, czyli kształt nieodróżnialny
    /// od poprzednika, który był po prostu małomówny. Bieg meetnotes `20260901-150035` przejechał
    /// tak Final Plan (17 minut) i Combine (26 minut).
    ///
    /// # WYŁĄCZNIE PO OBCYM ZEJŚCIU, i to jest zawężenie z powodem
    ///
    /// Zwykła porażka `carry-on` zostawia po sobie prozę, którą agent zdążył napisać, oraz powód
    /// stojący na jego własnej karcie — następny krok ma wtedy i materiał, i wyjaśnienie. Zdanie
    /// dopisywane przy KAŻDYM `carry-on` byłoby wierszem przy każdym kroku każdego biegu, w którym
    /// cokolwiek nie przeszło, czyli szumem (niezmiennik 16 w duchu).
    ///
    /// Jedno zdanie idzie w dwa miejsca i to jest ta sama odpowiedź (niezmiennik 13):
    /// `Line::Problem` mówi je człowiekowi, który patrzy, a `ran_without` w `run.json` — temu,
    /// który wróci jutro.
    fn tell_the_next_ones_what_they_are_missing(&self, id: StepId) {
        let stopped_from_outside = self
            .book()
            .steps
            .get(id)
            .is_some_and(|step| step.stopped_from_outside.is_some());
        if !stopped_from_outside {
            return;
        }
        let said = ran_without_sentence(&self.plan.steps[id].name);
        let node_key = self.plan.steps[id].node_key.as_str();
        let after: Vec<StepId> = self
            .plan
            .steps
            .iter()
            .enumerate()
            .filter(|(_, next)| next.depends_on.iter().any(|key| key == node_key))
            .map(|(next, _)| next)
            .collect();
        if after.is_empty() {
            return;
        }
        // JEDEN ZAPIS NA WSZYSTKIE DZIECI: `update` zrzuca `run.json` przy każdym wywołaniu, więc
        // pętla wokół niego byłaby tyloma zapisami pliku, ile krok ma potomków.
        self.update(|book| {
            for next in &after {
                book.steps[*next].ran_without.push(said.clone());
            }
        });
        for next in after {
            // Wynik świadomie porzucony: pełna kolejka do okna jest normalnym stanem
            // (`ipc::Sent`), a bieg nie ma prawa stanąć dlatego, że okno nie nadąża.
            let _ = self.lines.send(Line::Problem {
                agent: self.plan.steps[next].name.clone(),
                text: said.clone(),
                resets_at: None,
            });
        }
    }

    /// Przekazanie z tym, co ten krok zdazyl powiedziec — moze byc puste.
    ///
    /// NIC NIE NADPISUJEMY. Krok, ktory zdazyl oddac wynik i dopiero potem nie przeszedl — sedzia
    /// po wyczerpaniu prob, komenda, ktora wystartowala i padla, czlowiek, ktory odpowiedzial na
    /// pytanie — ma juz plik z prawdziwa trescia. Drugi zapis zamienilby ja na pustke.
    ///
    /// Puste ciało jest tu ODPOWIEDZIA, nie brakiem: agent, ktoremu tura przewrocila sie
    /// w polowie, nie powiedzial nic i to wlasnie ma stanac w indeksie nastepnego kroku.
    fn hand_on_its_last_words(&self, id: StepId) {
        let already = self
            .handoffs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .is_some_and(Option::is_some);
        if already {
            return;
        }
        /* PROZA AGENTA WYGRYWA Z PODSUMOWANIEM, i to jest cala roznica miedzy plikiem, ktory
         * ratuje ture, a plikiem, ktory ja tylko odnotowuje. `StepRun::summary` powstaje
         * z UDANEGO wyniku (`one_turn`), wiec tura, ktora wrocila bledem, nie ma go wcale —
         * a agent zdazyl w niej powiedziec, co zrobil i na czym stanal. Ten tekst zbiera
         * [`Live::said_so_far`], po zdarzeniu na blok.
         *
         * Podsumowanie zostaje dla krokow, ktore nie mowia proza: komenda, ktora nie wystartowala,
         * nie powiedziala ani slowa, a jej jedno zdanie jest wszystkim, co po niej zostaje. */
        let last_words = self
            .what_it_managed_to_say(id)
            .unwrap_or_else(|| self.book().steps[id].summary.clone().unwrap_or_default());
        self.hand_over(id, &last_words, &[]);
    }

    /// To, co ten krok zdazyl powiedziec proza. `None`, kiedy nie powiedzial nic.
    ///
    /// Zamek powstaje i ginie w jednym wyrazeniu, bez `await` w srodku (niezmiennik 8).
    fn what_it_managed_to_say(&self, id: StepId) -> Option<String> {
        self.said_so_far
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .filter(|said| !said.trim().is_empty())
            .cloned()
    }

    /// Dopisuje blok prozy do tego, co ten krok zdazyl powiedziec.
    fn also_said(&self, id: StepId, text: &str) {
        if let Some(said) = self
            .said_so_far
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(id)
        {
            if !said.is_empty() {
                said.push('\n');
            }
            said.push_str(text);
        }
    }

    /// Zapisuje, co aplikacja agenta wczytała temu krokowi z folderu (2026-09, Z-16).
    ///
    /// # Dlaczego to idzie do KSIĘGI, a nie obok niej
    ///
    /// Bo pytanie brzmi „co ten krok dostał", a odpowiedź ma przeżyć skasowanie `loadout.db`
    /// (niezmiennik 4) — czyli musi być w `run.json`. Wpis powstaje w chwili, w której CLI się
    /// przedstawia, i od tej chwili leży w księdze: koniec kroku tylko dopisuje do niej stan
    /// końcowy, więc krok, który nie przeszedł, został zatrzymany przez człowieka albo trafił
    /// w sufit czasu, ma ten sam rekord co krok udany. Zapis na końcu kroku miałby go
    /// dokładnie tam, gdzie nikt go nie potrzebuje.
    ///
    /// **Pierwszy wpis wygrywa.** Vendor przysyła `init` raz na sesję, a nie raz na turę — ale
    /// gdyby przysłał drugi, ten sam krok miałby dwa różne zdania o tym samym folderze, a widać
    /// byłoby drugie.
    ///
    /// # WYŁĄCZNIE TO, CO CLI OGŁOSIŁO — nigdy to, co znaleźliśmy na dysku
    ///
    /// Listy niżej przepisujemy z `system/init` bez ani jednego wniosku: to CLI mówi, co wzięło.
    /// `CLAUDE.md` w tym rekordzie **nie ma czego szukać** i to jest rozstrzygnięcie, nie
    /// przeoczenie (2026-09-04, Z-16).
    ///
    /// Do tego dnia stało tu jedno `is_file()` na katalogu, który podało CLI, a jego wynik jechał
    /// na ekran jako „this step also reads CLAUDE.md…". To zrównywało DWIE RÓŻNE RZECZY: że plik
    /// tam leży, i że agent go przeczytał. Na 2.1.251 jedno wynikało z drugiego — i na tym stał
    /// incydent, w którym sześć kroków biegu `20260823-145648` zapisało pliki wyników wbrew temu,
    /// co kazał im Loadout. Sonda z 2026-09-04 na 2.1.260 (trzy przebiegi z kontrolą negatywną,
    /// tabela przy `engine::drivers::claude::LEAN_CONTEXT`) pokazała, że przy dzisiejszym argv
    /// Loadouta plik do kroku **nie dociera** — więc to zdanie wysyłało człowieka szukającego
    /// przyczyny pod zły plik. Dokładnie ta klasa wady, dla której ten rekord powstał, o warstwę
    /// wyżej.
    ///
    /// `system/init` nie ma pola, które mówiłoby o pliku instrukcji, więc wiarygodnego sygnału
    /// nie ma wcale — a rekord bez zgadywania jest krótszy i prawdziwy. Kiedy taki sygnał się
    /// pojawi, to jest miejsce, w którym ma go czytać.
    fn also_loaded(&self, id: StepId, loaded: &LoadedFromTheFolder) {
        let mut book = self.book();
        let Some(step) = book.steps.get_mut(id) else {
            return;
        };
        if step.loaded_by_the_app.is_some() {
            return;
        }
        step.loaded_by_the_app = Some(LoadedByTheApp {
            kind: ContextKind::LoadedByTheApp,
            folder: loaded
                .folder
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            plugins: loaded.plugins.clone(),
            slash_commands: loaded.slash_commands.clone(),
            skills: loaded.skills.clone(),
            mcp_servers: loaded.mcp_servers.clone(),
            memory_paths: loaded.memory_paths.clone(),
            agents: loaded.agents.clone(),
        });
    }

    /// Pytanie, ktore staje na ekranie, kiedy krok nie przeszedl, a czlowiek chcial byc pytany.
    ///
    /// Niesie CZTERY rzeczy, bo bez ktorejkolwiek nie da sie odpowiedziec: ktory krok, co sie
    /// z nim stalo, co to znaczy dla reszty biegu i obie drogi wyjscia nazwane wprost. Zdanie
    /// „a step failed, continue?" jest pytaniem, na ktore mozna odpowiedziec tylko zgadujac.
    fn what_now(&self, id: StepId, why: &str) -> String {
        format!(
            "\"{}\" did not pass. {why}\n\nAnswer here and the steps after it will run anyway — \
             whatever you write goes to them as this step's notes. To stop instead, press Stop.",
            self.plan.steps[id].name,
        )
    }

    /// Pętla, której sędzią jest ten krok — razem z jej numerem pozycji.
    ///
    /// Sędzią jest krok, z którego WYCHODZI powrót. Krok stojący w pętli, ale nie na jej powrocie,
    /// jest zwykłym krokiem. Numer pozycji jedzie razem z pętlą, bo to on wskazuje wiersz
    /// w [`Live::settled_at`] — szukanie go drugi raz po kluczu kafelka dałoby dwie odpowiedzi
    /// na jedno pytanie.
    fn judging(&self, step: &Planned) -> Option<(usize, &Loop)> {
        self.plan
            .loops
            .iter()
            .enumerate()
            .find(|(_, one)| step.tile_key == one.judge)
    }

    /// Węzły TEJ rundy pętli, bez samego sędziego — czyli praca, o którą pyta „czy było co robić".
    ///
    /// Całe ciało, nie kafelek wejściowy: wejście jest tym, którego DRZEWO sędzia ocenia, a to
    /// jest inne pytanie. Runda, w której wejście odmówiło startu (sufit wydatku, brak wyceny,
    /// odmowa fan-in — wszystkie wracają przed `execution.executed`), a późniejszy krok ciała
    /// pobiegł i zostawił pracę, ma sędziego DOSTAĆ.
    ///
    /// Sędzia wypada z listy, żeby „ciało jest puste" znaczyło to samo także w pętli
    /// dwukafelkowej: jego własne wykonanie jest tym, co ta odpowiedź dopiero rozstrzyga.
    fn round_body_without_the_judge(&self, the_loop: &Loop, turn: u8) -> Vec<StepId> {
        let mut body: Vec<StepId> = Vec::new();
        for tile in &the_loop.body {
            if *tile != the_loop.judge {
                body.extend(self.nodes_of(tile, turn));
            }
        }
        body
    }

    /// P-03a: czy da się tu wykonać scenariusz na pełnej aplikacji — pytane raz na bieg
    /// i **wyłącznie** dla kroku, który tego naprawdę wymaga.
    ///
    /// Zdanie o braku możliwości pokazujemy człowiekowi od razu: bez niego jedynym śladem
    /// byłoby „nie zmierzono" przy kryterium, a to nie mówi, czego brakuje ani kto to nada.
    async fn native_route_for(
        &self,
        id: StepId,
    ) -> Option<crate::engine::native_ui::NativeUiAccess> {
        let needed = match &self.plan.steps[id].job {
            Job::Agent(job) => job.criteria.iter().any(|one| {
                one.required && one.method == crate::workflow::criteria::Method::FullRuntime
            }),
            Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => false,
        };
        if !needed {
            return None;
        }
        let access = self
            .native_ui
            .get_or_init(|| async { crate::engine::native_ui::can_drive_native_ui().await })
            .await
            .clone();
        if !access.can_judge_the_product() {
            let _ = self.control.show_in_the_run(Line::Problem {
                agent: self.plan.steps[id].name.clone(),
                text: access.said().to_owned(),
                resets_at: None,
            });
        }
        Some(access)
    }

    /// Zapala „ta pętla się domknęła w tej rundzie".
    ///
    /// PIERWSZY `pass` WYGRYWA: druga runda nie ma jak przepisać rundy pierwszej na późniejszą.
    /// Jedno miejsce dla obu sędziów — agenta i kroku „sprawdź" — bo dwa `get_or_insert` na tym
    /// samym wektorze rozjechałyby się przy pierwszej poprawce.
    fn settle(&self, which: usize, turn: u8) {
        let mut settled = self
            .settled_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = settled.get_mut(which) {
            slot.get_or_insert(turn);
        }
    }

    /// To samo, ale werdykt przychodzi z **wyjścia komendy**, nie z ust agenta.
    ///
    /// Dwa wejścia do jednej pętli, i drugie nie jest duplikatem pierwszego: sędzia-agent zostaje
    /// jedyną drogą dla repo, które sprawdzeń nie ma (D7, „Co musi przetrwać nawet przy zerowej
    /// ceremonii"), a krok „sprawdź" jest drogą dla repo, które je ma. Skasowanie [`Live::verdict_after`]
    /// nie byłoby uproszczeniem, tylko usunięciem ścieżki awaryjnej.
    ///
    /// Oddaje `true`, kiedy ten krok ma wrócić [`StepReport::Failed`] — a to znaczy trzy różne
    /// rzeczy zależnie od tego, gdzie ten krok stoi, i wszystkie trzy są tu wypisane:
    ///
    /// * **krok „sprawdź" spoza pętli** — werdykt jest wprost stanem kroku. Komenda nie przeszła,
    ///   więc krok padł i stożek za nim zostaje `Skipped`: praca nie ma prawa pojechać dalej na
    ///   czymś, co nie przeszło.
    /// * **sędzia pętli, który nie przepuścił, i ma jeszcze próbę** — krok wraca `Succeeded`,
    ///   mimo że komenda padła, i to nie jest kłamstwo: planista zmniejsza stopień wejściowy
    ///   dzieci WYŁĄCZNIE po tym stanie (`engine::scheduler`), a dzieckiem sędziego jest powrót
    ///   do roboty. `Failed` w tym miejscu zatrzymałby pętlę na pierwszej czerwonej rundzie,
    ///   czyli skasowałby całą jej treść. Że runda padła, widać po `exit_code` i po przekazaniu
    ///   z wyjściem komendy, nie po słowie `succeeded`. Ta sama droga, którą chodzi
    ///   [`Live::verdict_after`] dla sędziego-agenta.
    /// * **sędzia pętli w OSTATNIEJ rundzie, który nie przepuścił** — `Failed`, bo prób już nie
    ///   ma. Bez tej gałęzi wyczerpanie limitu tur wyglądałoby jak sukces, czyli limit byłby
    ///   ozdobą.
    ///
    /// Werdykt `pass` zapala `settled_at` i wtedy rundy PO tej zostają pominięte
    /// ([`Live::not_run_because`]) — nie przepalone. To jest jedyne miejsce, w którym wyjście
    /// komendy rozstrzyga o kształcie biegu, i jedyna różnica między „domknęło się na tym, co się
    /// stało" a „domknęło się na tym, co ktoś powiedział".
    fn verdict_of_a_check(&self, id: StepId, passed: bool) -> bool {
        let step = &self.plan.steps[id];
        // Krok „sprawdź" stojący w pętli, ale nie na jej powrocie, jest zwykłym krokiem
        // i jego werdykt jest wprost jego stanem.
        let Some((which, the_loop)) = self.judging(step) else {
            return !passed;
        };

        // 2026-09 (Z-33) — zapisujemy ten sam werdykt, którym sterujemy pętlą. `Succeeded`
        // nieostatniej czerwonej rundy jest wyłącznie mechaniką planisty; historia czyta ten
        // osobny fakt, żeby nie pokazać człowiekowi, że sprawdzenie przeszło (niezmiennik 29).
        let verdict = if passed {
            handoff::Verdict::Pass
        } else {
            handoff::Verdict::Fail
        };
        self.update(|book| book.steps[id].round_outcome = Some(verdict));

        if passed {
            self.settle(which, step.turn);
            return false;
        }
        step.turn + 1 >= the_loop.turns
    }

    /// Opis uruchomienia zapisany na samym kafelku — droga dla kroku, ktory nie bierze
    /// komendy od poprzednika.
    fn the_description_on_the_tile(job: &ServeJob) -> crate::workflow::LaunchDescription {
        crate::workflow::LaunchDescription {
            command: job.command.clone(),
            kind: crate::workflow::TargetKind::default(),
            test_data_env: None,
            subdirectory: String::new(),
            environment: BTreeMap::new(),
            required_env: Vec::new(),
            endpoints: job.endpoints.clone(),
            readiness: job.readiness.clone(),
        }
    }

    /// Zapisuje opis aplikacji i NIE odpala jej: start nalezy wtedy do agenta, nie do biegu.
    ///
    /// Krok konczy sie sukcesem dopiero wtedy, gdy opis DA SIE przekazac dalej. Sam zapis,
    /// ktorego nastepny krok nie zobaczy, jest kafelkiem bez skutku (niezmiennik 16).
    fn configure_without_starting(
        &self,
        id: StepId,
        description: &crate::workflow::LaunchDescription,
        owner: super::processes::ServiceOwner,
    ) -> StepReport {
        match self
            .processes
            .configure_description(description, self.tag_for(id), owner)
        {
            Ok(reference) => {
                let result = serde_json::json!({"service":reference,"status":"Configured, not started","endpoints":[]});
                self.hand_over(id, &format!("## Answer\n{result}\n\n## Evidence\nLoadout saved the app description. No process has started.\n\n## Open questions\nAn allowed agent may start this app.\n"), &[]);
                if self
                    .handoffs
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(id)
                    .is_none_or(Option::is_none)
                {
                    return self.refuse_step(id, "The app was configured, but its description could not be handed to the next step.");
                }
                self.update(|book| {
                    book.steps[id].summary = Some(
                        "The app is configured, not started. An allowed agent may start it."
                            .to_owned(),
                    );
                });
                StepReport::Succeeded
            }
            Err(why) => {
                self.refuse_step(id, &format!("Loadout could not configure this app: {why}"))
            }
        }
    }

    /// Czeka, az uruchomiona aplikacja odpowie pod swoim adresem — i oddaje ten adres dalej.
    ///
    /// „Proces wstal" i „aplikacja odpowiada" to dwa rozne fakty, a nastepny krok pyta o ten
    /// drugi. Bez opisu gotowosci krok konczy sie na pierwszym z nich i to jest cala WF-26.
    async fn wait_until_the_app_answers(
        &self,
        id: StepId,
        description: &crate::workflow::LaunchDescription,
        started: &super::processes::StartedProcess,
        cancel: &CancellationToken,
    ) -> StepReport {
        let Some(ready) = &description.readiness else {
            return StepReport::Succeeded;
        };
        let Some(reference) = &started.service else {
            return self.refuse_step(id, "The started app has no saved owner.");
        };
        match self
            .processes
            .wait_until_ready(reference, ready, cancel)
            .await
        {
            super::processes::ReadinessEnd::Ready(current) => {
                let result = serde_json::json!({ "service": current.service, "endpoints": current.endpoints, "readiness": current.readiness });
                self.hand_over(id, &format!("## Answer\n{result}\n\n## Evidence\nLoadout checked the responding process and its address.\n\n## Open questions\nNone.\n"), &[]);
                if self
                    .handoffs
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(id)
                    .is_none_or(Option::is_none)
                {
                    return self.refuse_step(
                        id,
                        "The app was ready, but its address could not be handed to the next step.",
                    );
                }
                self.update(|book| {
                    book.steps[id].summary = Some("The app is ready to use.".to_owned());
                });
                StepReport::Succeeded
            }
            super::processes::ReadinessEnd::Failed { message, proof } => {
                self.update(|book| {
                    book.steps[id].death_proof = matches!(proof, Some(GroupProof::Dead { .. }));
                });
                self.refuse_step(id, &message)
            }
            super::processes::ReadinessEnd::Cancelled { proof } => {
                self.update(|book| {
                    book.steps[id].death_proof = matches!(proof, Some(GroupProof::Dead { .. }));
                });
                StepReport::Cancelled
            }
        }
    }

    /// Podnosi proces kafelka „uruchom i zostaw" i **oddaje go rejestrowi**.
    ///
    /// Wraca, gdy proces WSTAŁ — nie gdy zejdzie. Czekanie na koniec zatrzymałoby graf na
    /// zawsze: serwer dev nie kończy się nigdy i właśnie o to w nim chodzi. Powód całego kroku
    /// stoi przy [`crate::workflow::ServeStep`] i jest zmierzony na biegu właściciela.
    ///
    /// WF-26: bez konfiguracji oddaje Started, z nią czeka tylko do gotowości.
    async fn start_and_leave(
        &self,
        id: StepId,
        job: &ServeJob,
        cancel: &CancellationToken,
    ) -> StepReport {
        // Włączone commandFrom jest jawnym wyborem źródła, także gdy ręczna
        // komenda została w pliku do późniejszego ponownego użycia.
        let description = match self.command_a_step_handed_over(id, job) {
            Ok(Some(said)) => said,
            Ok(None) => Self::the_description_on_the_tile(job),
            Err(why) => return self.refuse_step(id, &why),
        };
        let line = description.command.trim();
        if line.is_empty() {
            // Odmowa, nie ciche przejście: krok bez komendy jest kafelkiem bez skutku, a bieg,
            // który go „wykona", uczy człowieka, że ten kafelek działa.
            return self.refuse_step(
                id,
                "This step has no command, so there is nothing to start.",
            );
        }
        let owner = super::processes::ServiceOwner {
            reference: super::processes::ServiceRef {
                workspace: self.plan.project.clone(),
                run_id: self.plan.id.clone(),
                node_key: self.plan.steps[id].node_key.clone(),
                service_id: Uuid::now_v7().to_string(),
                generation: 1,
            },
            run_dir: self.plan.dir.clone(),
            cwd: job.cwd.clone(),
            lifetime: job.lifetime,
        };
        if job.start_when == crate::workflow::ServiceStartWhen::Asked {
            return self.configure_without_starting(id, &description, owner);
        }
        if job.start_when == crate::workflow::ServiceStartWhen::Unknown {
            return self.refuse_step(
                id,
                "This app uses a start setting this version does not support.",
            );
        }
        match self.processes.start_owned_description(
            &description,
            /* ZE ZNACZNIKIEM, i to jest ta droga, którą poprzednie podejście go zgubiło
             * (2026-09, Z-01d). Kafelek „uruchom i zostaw" idzie tędy — `Processes::start` →
             * `start_to_stay` — czyli obok tej jednej funkcji, w której znacznik wtedy stał.
             * Jego proces ma z rozmysłu przeżyć krok, więc po awarii aplikacji jest dokładnie
             * tym, czego odzyskiwanie szuka. */
            self.tag_for(id),
            owner,
        ) {
            Ok(started) => {
                self.update(|book| {
                    let step = &mut book.steps[id];
                    step.execution.process_started = true;
                    step.summary = Some(format!("Started and left running: {line}"));
                    step.pgid = Some(started.pgid);
                });
                self.wait_until_the_app_answers(id, &description, &started, cancel)
                    .await
            }
            // Zdanie mówi, CO nie wstało: `os error 2` samo nie mówi nic (DESIGN §8).
            Err(error) => {
                self.refuse_step(id, &format!("Loadout could not start \"{line}\": {error}"))
            }
        }
    }

    /// Co powiedzial krok, ktory zostal wskazany po nazwie — dopisane do znalezionych wierszy.
    ///
    /// Wskazany krok musi byc DOKLADNIE tym przodkiem: kopia i proba tez sie licza. Krok spoza
    /// stozka przodkow albo ten sam krok to odmowa, a nie pusty wynik — inaczej „wybierz krok"
    /// bylaby kontrolka, ktora cicho nie robi nic (niezmiennik 16).
    fn what_the_named_producer_said(
        &self,
        id: StepId,
        producer: &str,
        field: &str,
        found: &mut Vec<String>,
    ) -> Result<(), String> {
        let ancestors = super::run_inputs::ancestors_of(id, &self.plan.arrows);
        let Some(at) = self
            .plan
            .steps
            .iter()
            .position(|step| step.node_key == producer)
            .filter(|at| *at != id && ancestors.contains(at))
        else {
            return Err("The selected app description must come from an exact earlier step, copy and attempt in this workflow.".to_owned());
        };
        let written = self
            .handoffs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(at)
            .and_then(Option::as_ref)
            .cloned();
        if let Some(written) = written {
            let body = super::run_inputs::read_output(&self.plan.dir, &self.plan.id, at, &written)
                .map_err(|_| {
                    "The selected app description could not be read safely from its saved result."
                        .to_owned()
                })?;
            if let Some(said) = fields_said_in(&body).get(field) {
                found.push(said.clone());
            }
        }
        Ok(())
    }

    /// Czyta zapisane przekazanie jako opis aplikacji — ograniczony rozmiarem i bez podazania
    /// za dowiazaniami.
    ///
    /// Bajty, ktore urosly w trakcie czytania, i tresc niezgodna z opublikowana sa tu odmowa:
    /// z tego pliku za chwile powstanie wiersz powloki, wiec „prawie te same bajty" nie wystarcza.
    fn read_a_saved_description(
        path: &Path,
        parent: &Path,
        name: &std::ffi::OsStr,
    ) -> anyhow::Result<handoff::Handoff> {
        let root = crate::engine::supervisor::PublicationRoot::open(parent)?;
        let mut file = root.open_regular_file(Path::new(name))?;
        if file.metadata()?.len() > 65536 {
            anyhow::bail!("saved app description is too large");
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(&mut file, 65537), &mut bytes)?;
        if bytes.len() > 65536 {
            anyhow::bail!("saved app description grew while being read");
        }
        root.validate_path_identity(parent)?;
        let read = handoff::parse_handoff(path, &bytes)?;
        if read.bytes_mismatch() {
            anyhow::bail!("saved app description changed after publication");
        }
        Ok(read)
    }

    /// Zamienia to, co oddal poprzednik, w opis uruchomienia — w formacie, ktory wybral kafelek.
    ///
    /// Format nierozpoznany jest odmowa, a nie cichym powrotem do „to zwykla komenda": nowszy
    /// plik uruchomilby wtedy cudzy napis jako wiersz powloki.
    fn the_launch_description_from(
        said: String,
        format: crate::workflow::CommandFormat,
        job: &ServeJob,
    ) -> Result<crate::workflow::LaunchDescription, String> {
        Ok(match format {
            crate::workflow::CommandFormat::LaunchDescription => {
                super::processes::launch::parse(&said)?
            }
            crate::workflow::CommandFormat::Command => crate::workflow::LaunchDescription {
                command: said,
                // Kafelek mówi, czym jest to, co uruchamia; opis od agenta mówi to samo polem
                // o tej samej nazwie (niezmiennik 13 — jedno pytanie, jedna odpowiedź).
                kind: job.kind,
                test_data_env: job.test_data_env.clone(),
                subdirectory: String::new(),
                environment: BTreeMap::new(),
                required_env: Vec::new(),
                endpoints: job.endpoints.clone(),
                readiness: job.readiness.clone(),
            },
            crate::workflow::CommandFormat::Unknown => {
                return Err(
                    "Choose a command or an app description as this step's source.".to_owned(),
                );
            }
        })
    }

    /// Wiersz powłoki, który oddał krok przed tym — albo `None`, gdy ten kafelek go nie oczekuje.
    ///
    /// # Po co to istnieje
    ///
    /// Zamówienie właściciela 2026-08-30: „agent sam ma rozkminić jakie komendy użyć do
    /// odpalenia, my nie ingerujemy bo nie chcę w każdym projekcie osobno wpisywać na front
    /// i backend command". Komenda uruchamiająca aplikację jest inna w każdym repo, a wpisana
    /// ręcznie w workflow zamienia jeden wielokrotnego użytku plik w plik na jeden projekt.
    ///
    /// # Nowego mechanizmu tu nie ma
    ///
    /// Prośba o nazwane pole jedzie do agenta od dawna ([`Handover::Form`]), parser
    /// `nazwa: wartość` stoi w [`fields_said_in`], a brak pola wymaganego jest już odmową.
    /// Ta funkcja mówi wyłącznie, KTÓRE z tych pól jest komendą.
    ///
    /// # SKAN SEKRETÓW, którego ta droga inaczej by ominęła
    ///
    /// `workflow::check::a_command_carrying_a_secret` sądzi komendę **przy zapisie**, nad tekstem
    /// z PLIKU — a komenda wyprodukowana przez agenta nie przechodzi tamtędy ani razu i leci
    /// prosto do `/bin/sh -c`. Uzasadnienie przy `const SHELL` opiera się wprost na tym, że
    /// komendę napisał człowiek i że przeszła skan. Przepuszczamy ją więc przez TEN SAM
    /// `secret_shaped` i odmawiamy startu, zamiast odpalać.
    fn command_a_step_handed_over(
        &self,
        id: StepId,
        job: &ServeJob,
    ) -> Result<Option<crate::workflow::LaunchDescription>, String> {
        let Some(source) = &job.command_from else {
            return Ok(None);
        };
        let field = &source.field;
        let mut found = Vec::new();
        let paths = if let Some(producer) = &source.producer {
            self.what_the_named_producer_said(id, producer, field, &mut found)?;
            Vec::new()
        } else {
            self.handed_before(id)
                .into_iter()
                .map(|hand| hand.path)
                .collect()
        };
        // WF-27: zgodność starego pliku oznacza jednego producenta, nie last-wins.
        // Odczyt jest ograniczony i no-follow, zanim modelowe dane staną się komendą.
        for path in paths {
            let parent = path
                .parent()
                .ok_or("The app description has no saved folder.")?;
            let name = path
                .file_name()
                .ok_or("The app description has no saved file.")?;
            let read = Self::read_a_saved_description(&path, parent, name).map_err(|_| {
                "The selected app description could not be read safely from its saved result."
                    .to_owned()
            })?;
            if let Some(said) = fields_said_in(&read.body).get(field) {
                found.push(said.clone());
            }
        }
        if found.len() > 1 {
            return Err(format!(
                "More than one earlier result contains \"{field}\". Choose the exact step, copy and attempt to start the app from."
            ));
        }
        let Some(said) = found.pop() else {
            /* ODMOWA NAZYWA POLE I KAFELEK. „Nothing to start" zostawia człowieka przed grafem,
             * w którym wszystko wygląda poprawnie — a brakuje jednego wiersza w odpowiedzi
             * poprzednika (niezmiennik 29: zdanie ląduje w `book.steps[id].error`, czyli tam,
             * gdzie człowiek je czyta). */
            return Err(format!(
                "This step takes its command from a field called \"{field}\", and the step \
                 before it did not hand one over."
            ));
        };

        let description = Self::the_launch_description_from(said, source.format, job)?;
        crate::workflow::check::launch_description(&description)?;
        Ok(Some(description))
    }

    /// Krok, który nie ruszył, z powodem zapisanym tam, gdzie człowiek go szuka.
    fn refuse_step(&self, id: StepId, said: &str) -> StepReport {
        self.update(|book| {
            book.steps[id].error = Some(said.to_owned());
            book.steps[id].end_cause = Some(super::run_inputs::EndCause::Refused);
        });
        StepReport::Failed
    }

    /// P-01: nazywa sąsiednie katalogi, których nie będzie w kopiach roboczych.
    ///
    /// Skan jest jeden na bieg, nie jeden na krok: manifesty projektu są te same dla wszystkich,
    /// a chodzenie po drzewie raz na kafelek byłoby tą samą odpowiedzią policzoną n razy.
    fn say_which_neighbours_will_be_missing(&self) {
        /* „Własna kopia" poznaje się po katalogu roboczym, nie po polu `folder`: to samo
         * `same-copy` raz jest folderem projektu, a raz kopią założoną przez krok przed nim. */
        let works_in_its_own_copy: Vec<&str> = self
            .plan
            .steps
            .iter()
            .filter(|step| match &step.job {
                Job::Agent(job) => job.cwd != self.plan.project,
                Job::Check(check) => check.spec.cwd != self.plan.project,
                Job::Ask { .. } | Job::Serve(_) => false,
            })
            .map(|step| step.name.as_str())
            .collect();
        if works_in_its_own_copy.is_empty() {
            return;
        }
        let neighbours = neighbours::outside_the_project(&self.plan.project);
        if neighbours.is_empty() {
            return;
        }
        for step in works_in_its_own_copy {
            let _ = self.control.show_in_the_run(Line::Problem {
                agent: step.to_owned(),
                text: neighbours::said(step, &neighbours),
                resets_at: None,
            });
        }
    }

    fn announce(&self, id: StepId, state: StepState) {
        let _ = self.lines.send(Line::StepState {
            agent: self.plan.steps[id].name.clone(),
            /* `tile_key`, NIE `node_key`: rundy petli maja unikalny `node_key` (wymog
             * `UNIQUE (run_id, node_key)` w bazie), a okno rozpoznaje kafelek po kluczu Z PLIKU.
             * Wyslanie tu klucza z sufiksem znaczy, ze okno nie zna zadnego z nadeslanych krokow
             * i po cichu porzuca kazda linie stanu -- pasek stoi pusty przez caly bieg. Ten sam
             * klucz zlewa trzy rundy w jedna karte agenta, czyli spelnia warunek "nie ma byc
             * widac, ze spawnujemy nowych agentow". */
            step_id: self.plan.steps[id].tile_key.clone(),
            state: state.name().to_owned(),
        });
    }

    fn update(&self, edit: impl FnOnce(&mut Book)) {
        let mut book = self.book();
        edit(&mut book);
        if let Err(error) = self.spill(&book) {
            tracing::error!(run = %self.plan.id, %error, "the run file could not be written");
        }
    }

    /// Znacznik, który każdy proces tego kroku poniesie w środowisku.
    ///
    /// # Jedno źródło obu wartości, i to jest cała treść tej funkcji (2026-09, Z-01d)
    ///
    /// `self.plan.id` i `self.plan.steps[id].id` to **dokładnie** te dwa napisy, które lądują
    /// w `run.json` jako `id` biegu i `id` kroku — a odzyskiwanie porównuje pierwszy z nich co do
    /// bajta. Poprzednie podejście brało tu `RunSpec::run_id`, czyli `job.session`, czyli
    /// identyfikator SESJI vendora: ten sam napis, który stoi w `agent_session_id`, i nigdy równy
    /// identyfikatorowi biegu. Skutek był cichy i dokładnie odwrotny do zamierzonego — własna
    /// żywa grupa wychodziła obca i nie dostawała sygnału w ogóle.
    fn tag_for(&self, id: StepId) -> StepTag {
        StepTag::new(&self.plan.id, &self.plan.steps[id].id)
    }

    /// Dopisuje do księgi grupy, które ten krok po sobie zostawił.
    ///
    /// ADDYTYWNIE (niezmiennik 25): `pgids` rośnie i nigdy nie kasuje tego, co już w nim stoi,
    /// a `pgid` zostaje tam, gdzie był. Krok bywa zatrzymywany kilka razy pod rząd — pierwsza
    /// próba widzi drzewo, którego druga już nie zobaczy, bo lider zdążył zejść — więc zapis,
    /// który podmienia listę, gubiłby dokładnie tę grupę, dla której to pole powstało.
    ///
    /// Pusta lista nie robi nic i nie pisze pliku: krok bez procesu (kafelek kontrolny, dubel bez
    /// grupy) nie ma o czym meldować, a zapis „nic się nie stało" przy każdym kroku każdego biegu
    /// to plik przepisywany bez powodu.
    fn note_pgids(&self, id: StepId, groups: &[i32]) {
        if groups.is_empty() {
            return;
        }
        self.update(|book| {
            let step = &mut book.steps[id];
            for &pgid in groups {
                if !step.pgids.contains(&pgid) {
                    step.pgids.push(pgid);
                }
            }
        });
    }

    /// Księga → `run.json` przez ten sam trwały publisher, którego używa recovery z T-202.
    fn spill(&self, book: &Book) -> Result<(), RunError> {
        let bytes = self.run_file_bytes(book)?;
        let (run_dir, run_file) = self.durable_run_location()?;
        DurableFilePublisher::new(&run_dir)
            .atomic_replace(
                &run_file,
                &bytes,
                ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
            )
            .map_err(PublishError::into_io)?;
        Ok(())
    }

    fn durable_run_location(&self) -> Result<(PathBuf, PathBuf), RunError> {
        let run_dir = fs::canonicalize(&self.plan.dir)?;
        let run_file = run_dir.join(RUN_FILE);
        Ok((run_dir, run_file))
    }

    fn run_file_bytes(&self, book: &Book) -> Result<Vec<u8>, RunError> {
        Ok(serde_json::to_vec_pretty(&self.run_file(book)?)?)
    }

    /// Widok `run.json` na tę chwilę.
    fn run_file<'a>(&'a self, book: &'a Book) -> Result<RunFile<'a>, RunError> {
        let steps = self
            .plan
            .steps
            .iter()
            .zip(&book.steps)
            .map(|(planned, run)| step_entry(planned, run))
            .collect();

        Ok(RunFile {
            id: &self.plan.id,
            lead_origin: self.plan.lead_origin.as_ref(),
            project_instructions: self.plan.project_instructions.as_ref(),
            memory_sources: self.plan.memory_sources.binding()?,
            context_sources: self
                .plan
                .context_sources
                .as_ref()
                .map(super::context_sources::Snapshot::binding)
                .transpose()?,
            input_snapshot: self.plan.input_snapshot.as_ref().map(|snapshot| {
                serde_json::json!({
                    "id": snapshot.id(), "manifest": "input/manifest.json"
                })
            }),
            workspace_inputs: super::workspace_inputs::wire(&self.plan.workspace_inputs),
            starting_results: &self.plan.starting_results,
            copy_results: &book.copy_results,
            pending_finalization: &book.pending_finalization,
            workflow_id: &self.plan.workflow_id,
            workflow_hash: &self.plan.hash,
            workflow_snapshot: &self.plan.graph,
            title: &self.plan.title,
            task: &self.plan.task,
            status: book.status,
            concurrency: self.plan.concurrency,
            created_at: self.plan.created_at,
            trigger_origin: self.plan.trigger_origin.as_ref(),
            // Kiedy wstała maszyna, na której ten bieg ruszył. STRAŻNIK odzyskiwania po awarii:
            // bez niego `recovery::decide` odmawia sprzątania (`NO_BOOT_TIME`), bo po restarcie
            // zapisany `pgid` z dużym prawdopodobieństwem należy do niewinnego procesu
            // (`kern.maxproc` = 16 000, więc PID-y przewijają się w godzinach).
            boot_id: self.plan.boot_id.as_deref(),
            started_at: book.started_at,
            ended_at: book.ended_at,
            error: None,
            route_decisions: self
                .route_decisions
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
            // Fakty notatki są zamrożone przed pierwszym procesem; tylko listy fizycznych UUID
            // rosną w księdze, dokładnie na granicy udanego startu sterownika.
            memory: &book.memory,
            /* Z PLANU, nie z księgi: materiał jest zamrożony przed pierwszym procesem i nic
             * w trakcie biegu nie ma prawa go ruszyć — inaczej `run.json` opisywałby bibliotekę
             * taką, jaka jest PO biegu. */
            skills: &self.plan.skills,
            /* SUFIT MILCZY, KIEDY GO NIE MA. Bieg, którego nikt nie ograniczył, nie ma o sufcie
             * nic do powiedzenia, a klucz mówiący „bez sufitu" przy każdym biegu w historii jest
             * długością zapłaconą za milczenie — ta sama decyzja, co przy `death_proof`
             * i `repaired` obok. */
            budget_usd: self.budget_usd,
            /* WYDATEK JUŻ NIE IDZIE Z NIM W PARZE (2026-09, Z-38). Do tego dnia było tu
             * `self.budget_usd.map(...)`, czyli: bieg bez sufitu nie zapisywał swojej ceny
             * wcale. Cena biegu jest faktem o biegu, a nie o sufcie — i to jedyne miejsce, do
             * którego wchodzi cena prywatnej tury ([`final_spend_in`]), więc przy tamtym
             * warunku tura, która wydała 58 centów na darmowym biegu, nie zostawiała po sobie
             * ani jednej liczby. Pliki są prawdą (niezmiennik 4).
             *
             * Warunkiem jest dziś POMIAR, nie sufit: `None` zostaje dla biegu, w którym nikt
             * ceny nie podał, bo „nie wiadomo" i „nic nie kosztowało" to dwa różne zdania
             * (niezmiennik 17), a `history::summary` czyta je osobno po stronie kroków. */
            spent_usd: anything_was_priced(book).then(|| final_spend_in(book)),
            reflection: &book.reflection,
            steps,
        })
    }

    /// Jeden krok, od pierwszego wpisu w księdze po ostatni.
    ///
    /// `self: Arc<Self>`, bo pętla czytająca zdarzenia sterownika jest **osobnym zadaniem**
    /// ([`forward`]) i musi umieć powiedzieć o limicie dostawcy temu samemu biegowi.
    async fn step(
        self: Arc<Self>,
        id: StepId,
        cancel: CancellationToken,
        started: scheduler::Started,
    ) -> StepReport {
        // Miejsce ZANIM cokolwiek wpiszemy do księgi: `running` z chwilą startu wpisaną przed
        // wzięciem miejsca to ten sam fałsz, przed którym stoi niezmiennik 11 — krok stojący
        // w kolejce czytałby się jak krok, który działa.
        //
        // Trzyma się do końca kroku, bo `Slot` oddaje miejsce w `Drop`: wychodzi więc także
        // przez panikę i przez anulowanie. Miejsce zwracane wyłącznie na szczęśliwej ścieżce
        // daje pulę, która kurczy się przez cały bieg, aż nic już nie startuje.
        /* RUNDA, KTÓREJ NIE POTRZEBUJEMY, KOŃCZY SIĘ TU — przed wzięciem miejsca z puli i bez
         * dotknięcia sterownika.
         *
         * `Succeeded`, i to nie jest wybór: planista zmniejsza stopień wejściowy dzieci WYŁĄCZNIE
         * po tym stanie (`engine::scheduler`). Krok za pętlą wisi na sędzim rundy OSTATNIEJ, więc
         * gdyby pominięta runda wróciła czymkolwiek innym — a `Failed` i `Cancelled` oznaczają
         * cały stożek jako `Skipped` — praca za pętlą nie ruszyłaby nigdy. Ósmy stan maszyny nie
         * wchodzi w grę: `steps.status` ma w bazie `CHECK` na siedmiu nazwach, a niezmiennik 25
         * zabrania przepisywania tabel, więc każda ISTNIEJĄCA baza odmówiłaby wiersza już PO
         * zapłaceniu za bieg.
         *
         * Że runda nie biegła, zapisuje `not_run_because`; koszt, log i przekazanie pozostają
         * puste. To rozdziela stan planisty od prawdy widocznej w `run.json` i historii
         * (2026-09, Z-33), bez ósmego stanu w bazie.
         *
         * PRZED miejscem z puli, nie po: runda, której nikt nie potrzebuje, nie ma prawa stać
         * w kolejce po zasób wart ~583 MB i blokować kroku, który naprawdę ma coś do zrobienia. */
        // 2026-09-05 (WF-04): brak diffu Git nie jest werdyktem. Wynik może być tekstem,
        // pracą późniejszego kroku albo innej kopii; sędzia z grafu musi go ocenić.
        // Pomijamy wyłącznie przyszłe próby po rzeczywistym `pass`, tą samą drogą dla
        // sędziego-agenta i Check, bez zmiany polityki zachowania plików.
        if let Some(why) = self.not_run_because(id) {
            self.update(|book| {
                let step = &mut book.steps[id];
                step.not_run_because = Some(why);
                step.end_cause = Some(super::run_inputs::EndCause::LoopSettled);
                step.summary = Some(NOT_NEEDED.to_owned());
            });
            return self.finish_this_step(id, StepReport::Succeeded).await;
        }

        /* RUNDA, KTÓREJ CIAŁO NIE RUSZYŁO ANI RAZU, NIE MA CZEGO SĄDZIĆ.
         *
         * WF-04 słusznie zdjęło pytanie o diff Gita: wynikiem pętli bywa sam tekst, więc krok,
         * który odpowiedział i nie tknął plików, dalej idzie pod sędziego. Ale krok, który
         * W TEJ RUNDZIE NIE WYKONAŁ SIĘ WCALE — bo zatrzymał go sufit wydatku albo cudza
         * porażka — nie zostawił ani tekstu, ani plików. Sędzia postawiony nad taką rundą pyta
         * o pustkę, odpowiada `fail` i otwiera rundę następną; a pod przekroczonym sufitem
         * kolejne rundy jadą stożkiem `carry-on` i płacą JUŻ PO tym, jak skończyły się
         * pieniądze. Zmierzone na wyroczni T-149: 46,25 USD przy sufcie 8,00 USD.
         *
         * PYTAMY O FAKT WYKONANIA, NIE O PLIKI, więc warunek jest ŚCIŚLE WĘŻSZY niż dawne
         * `nothing_to_judge`: żaden krok, który naprawdę ruszył, nie traci przez to sędziego.
         *
         * PYTAMY O CAŁE CIAŁO PĘTLI, NIE O JEJ KAFELEK WEJŚCIOWY (2026-09-06). Wejście jest tym
         * kafelkiem, którego DRZEWO ocenia sędzia — i to jest jedyna rzecz, do której ono się tu
         * nadaje. Na pytanie „czy ta runda coś zrobiła" odpowiada całe ciało: sufit wydatku,
         * brak wyceny i odmowa fan-in wracają PRZED `execution.executed = true`, a stożek za
         * budżetową odmową ma bramkę sufitu jawnie wyłączoną (T-101, `carries_on_past_the_budget`)
         * — więc późniejszy krok ciała ma ZAPROJEKTOWANĄ zgodę na bieg i naprawdę zostawia pracę.
         * Pytanie o samo wejście pomijało wtedy sędziego, domykało pętlę na `settle()` i puszczało
         * cały stożek za nią na pracy, której nikt nie ocenił, ze zdaniem „the step before this one
         * never ran." na karcie sędziego stojącego na strzałce za krokiem, który właśnie pobiegł.
         *
         * Sędzia jest wyłączony z listy, żeby `!body.is_empty()` znaczyło „to ciało poza sędzią"
         * także w pętli dwukafelkowej — czytamy to jako jedno zdanie, nie jako sumę dwóch ról.
         *
         * STOI POD `not_run_because`, NIE NAD NIM, i to jest cała różnica: rundy pętli, która
         * domknęła się po prawdziwym `pass`, mają zachować dzisiejsze „loop settled at try N",
         * a nie to zdanie. Nad blokiem ta gałąź przewracała pięć zielonych kryteriów
         * (`runcmd_loop`, `loop_judges_results_without_file_changes`). */
        let empty_round = self
            .judging(&self.plan.steps[id])
            .and_then(|(which, the_loop)| {
                let turn = self.plan.steps[id].turn;
                let body = self.round_body_without_the_judge(the_loop, turn);
                // Zamek księgi powstaje i ginie tutaj, bez ani jednego `await` (niezmiennik 8).
                let book = self.book();
                let nothing_ran =
                    !body.is_empty() && body.iter().all(|&at| !book.steps[at].execution.executed);
                nothing_ran.then_some((which, turn))
            });
        if let Some((which, turn)) = empty_round {
            // Pętla domyka się na TEJ rundzie: dalsze próby pytałyby o tę samą pustkę.
            self.settle(which, turn);
            self.update(|book| {
                let step = &mut book.steps[id];
                step.end_cause = Some(super::run_inputs::EndCause::LoopSettled);
                step.summary = Some(NOTHING_RAN.to_owned());
            });
            return self.finish_this_step(id, StepReport::Succeeded).await;
        }

        /* PRACA RODZICÓW WCHODZI DO JEDNEJ KOPII TUTAJ — przed miejscem z puli i przed `match`
         * po rodzaju kroku (2026-08-29).
         *
         * PRZED MIEJSCEM Z PULI, bo krok, który nie ruszy, nie ma prawa stać w kolejce po zasób
         * wart ~583 MB — ten sam powód, dla którego stoi tu pominięta runda pętli.
         *
         * PRZED `match`, bo sterownik nie ma prawa zobaczyć niezgody ANI RAZU. Gdyby składanie
         * siedziało w [`Live::run_agent`], krok z dwoma rodzicami piszącymi w jednym pliku
         * różnie startowałby proces, płacił za turę i dopiero potem odmawiał — a agent zdążyłby
         * przeczytać kopię, w której jedna ze stron po cichu wygrała.
         *
         * Zdanie idzie na ekran tą samą drogą, co `say_what_was_left_behind`: `Line::Problem`
         * z nazwą kafelka, bo to jego wiersz człowiek otworzy. */
        let prepared = self.fold_what_came_before(id);
        if matches!(prepared, Ok(fan_in::ApplyOutcome::Cancelled)) {
            return StepReport::Cancelled;
        }
        if let Err(why) = prepared {
            // Wynik świadomie porzucony: pełna kolejka do okna jest normalnym stanem
            // (`ipc::Sent`), a bieg nie ma prawa stanąć dlatego, że okno nie nadąża.
            let _ = self.lines.send(Line::Problem {
                agent: self.plan.steps[id].name.clone(),
                text: why.clone(),
                resets_at: None,
            });
            let report = self.when_this_one_fails(id, &why).await;
            return self.finish_this_step(id, report).await;
        }
        self.stamp_this_copy(id);

        /* `mut`, bo krok agenta MOŻE oddać to miejsce dalej (2026-08-28): grupa, której nie dało
         * się dowieść jako martwej, zabiera permit ze sobą do rejestru aplikacji — inaczej pula
         * zwolniłaby miejsce po czymś, co dalej biegnie i dalej płaci (niezmiennik 11). Na każdej
         * innej drodze ta wartość ginie dokładnie tam, gdzie ginęła: na końcu tej funkcji, czyli
         * już PO `finish_this_step`. */
        let mut slot = match &self.plan.steps[id].job {
            // Krok „sprawdź" i agent oznaczony jako ciężki biorą miejsce z puli **i** jedno
            // miejsce ciężkie ([`weight_of`]), więc dwa takie kroki nigdy nie idą obok siebie,
            // choćby pula miała ich osiem.
            // Pytanie do człowieka nie waży nic i miejsca nie bierze wcale — to jest cała
            // różnica między tymi dwoma ramionami.
            /* KAFELEK „URUCHOM I ZOSTAW" NIE BIERZE MIEJSCA, i to jest decyzja, nie pominięcie.
             * Pula odpowiada na pytanie „ilu agentów naraz" (niezmiennik 11), a ten krok żadnego
             * nie woła — trwa tyle, co `spawn`. Miejsce trzymane przez serwer, który żyje cały
             * bieg, wyjęłoby z puli jedno na stałe i zagłodziło kroki, które naprawdę pracują. */
            Job::Agent(_) | Job::Check(_) => {
                /* TURA, KTÓREJ NIKT NIE UMIE WYCENIĆ, NIE RUSZA POD SUFITEM (2026-09, Z-44).
                 *
                 * PRZED pytaniem o resztę sufitu, bo to jest inne pytanie: tam chodzi o to, czy
                 * zostało dość pieniędzy, a tu o to, czy w ogóle da się je policzyć. Krok, którego
                 * cena nie wchodzi do sumy (Z-13b), przechodziłby każde pytanie o resztę — także
                 * po przekroczeniu sufitu, bo sam do niego nie dokłada ani centa.
                 *
                 * PRZED miejscem z puli i przed sterownikiem, jak każda inna odmowa startu:
                 * krok, który nie ruszy, nie ma prawa stać po zasób wart ~583 MB, a proces,
                 * który już ruszył, jest opłacony niezależnie od tego, co zrobimy potem. */
                if let Job::Agent(job) = &self.plan.steps[id].job
                    && let Some(said) = self.no_way_to_price_this_one(job)
                {
                    return self.the_budget_stops_this_one(id, said).await;
                }
                /* SUFIT WYDATKU PYTANY DWA RAZY, i oba pytania są konieczne.
                 *
                 * Przed kolejką, żeby krok, dla którego pieniędzy już nie ma, nie stał po zasób
                 * wart ~583 MB i nie blokował nikogo. Po kolejce, bo pieniądze wydaje się
                 * WŁAŚNIE wtedy, kiedy ten krok czeka: kroki, które trzymały miejsca, kończą
                 * tury i dopisują swoje ceny do księgi. Samo pierwsze pytanie przepuszczałoby
                 * każdy krok, który stanął w kolejce, zanim ktokolwiek zdążył zapłacić. */
                if let Some(said) = self.the_budget_is_spent(id) {
                    return self.the_budget_stops_this_one(id, said).await;
                }
                let Some(slot) = self
                    .a_slot_for_this_step(&cancel, weight_of(&self.plan.steps[id].job))
                    .await
                else {
                    // Stop, zanim ten krok w ogóle ruszył: `cancelled`, nie `skipped` — nikt
                    // wyżej nie padł, człowiek zatrzymał bieg [T7 §9.3]. Księga zostaje bez
                    // chwili startu, bo startu nie było, a stan końcowy dopisze planista.
                    return StepReport::Cancelled;
                };
                /* UDZIAŁ PRZYZNAJEMY I ODKŁADAMY TUTAJ, w tej samej chwili, w której ten krok
                 * dostał miejsce z puli (2026-09, Z-13b). To jest jedyne miejsce, w którym
                 * wiadomo, że krok NAPRAWDĘ rusza — a dwa kroki, które dostają miejsce w tej
                 * samej chwili, przyznałyby sobie tę samą resztę, gdyby kwota liczyła się
                 * gdziekolwiek indziej niż pod jednym zamkiem. */
                if let Some(said) = self
                    .a_share_for(id)
                    .and_then(|share| self.not_a_cent_to_run_on(share))
                {
                    // Miejsce oddajemy OD RAZU, nie na końcu kroku: krok, który nie ruszy,
                    // nie ma prawa trzymać miejsca potrzebnego komuś, kto jeszcze może biec.
                    drop(slot);
                    return self.the_budget_stops_this_one(id, said).await;
                }
                Some(slot)
            }
            // Kafelek kontrolny miejsca NIE bierze i to jest wybór, nie przeoczenie: pula liczy
            // agentów po ~583 MB (`[T7 §7.1, V]`), a pytanie do człowieka nie waży nic i potrafi
            // czekać godzinami. Pytanie trzymające miejsce ze wspólnej puli zagłodziłoby przy
            // limicie 1 wszystkie pozostałe karty, i to na tak długo, jak długo nikt nie patrzy
            // na ekran.
            //
            // 2026-08-23 — kafelek „uruchom i zostaw" dołącza do tego ramienia z bliźniaczego
            // powodu: trwa tyle, co `spawn`, i nie woła żadnego agenta. Miejsce trzymane przez
            // serwer, który żyje cały bieg, wyjęłoby z puli jedno na stałe.
            Job::Ask { .. } | Job::Serve(_) => None,
        };

        // 2026-09 (Z-30) — START MELDUJEMY PLANIŚCIE DOKŁADNIE TUTAJ, w tej samej chwili, w której
        // księga dostaje `running`. Wyżej krok czekał na miejsce w puli albo na sufit wydatku,
        // a kafelek „zapytaj" i „uruchom i zostaw" nie czekają na nic — dla nich to jest po
        // prostu chwila, w której naprawdę się zaczynają. Krok, który skończył się przed tą
        // linią, porzuca potwierdzenie i po panice zadania czyta się jako pominięty, nie jako
        // taki, który pracował i nie przeszedł (niezmiennik 11).
        started.now();
        let at = now_ms();
        self.update(|book| {
            book.started_at.get_or_insert(at);
            let step = &mut book.steps[id];
            step.execution.executed = true;
            step.status = StepState::Running;
            step.started_at = Some(at);
        });
        // Po zapisie do księgi, nie przed: ekran, który wie o kroku wcześniej niż `run.json`,
        // pokazywałby po awarii aplikacji stan, którego odzyskiwanie nie potrafi odtworzyć.
        self.announce(id, StepState::Running);

        let report = match &self.plan.steps[id].job {
            Job::Agent(job) => self.run_agent(id, job, &cancel, &mut slot).await,
            Job::Ask { question } => self.wait_for_a_person(id, question.as_deref()).await,
            Job::Check(job) => self.run_check(id, job, &cancel, &mut slot).await,
            Job::Serve(job) => self.start_and_leave(id, job, &cancel).await,
        };

        self.finish_this_step(id, report).await
    }

    /// Odmawia za niekompletne wejscie odziedziczonego folderu i sprząta cudzą dostawę
    /// umiejętności, zanim ten krok cokolwiek do niego zniesie.
    ///
    /// Folder przekazany dalej bywa cudzy i bywa zastany po błędzie, więc „nie ma czego składać"
    /// nie jest tu dowodem, że jest w nim komplet.
    fn the_folder_is_fit_to_work_in(&self, step: &Planned) -> Result<(), String> {
        // WF-03: CarryOn może przekazać TEN SAM folder krokowi, który sam już niczego nie
        // składa. Puste folds_in nie dowodzi kompletności folderu odziedziczonego po błędzie.
        if let Some(cwd) = where_the_job_works(&step.job)
            && let Ok(relative) = cwd.strip_prefix(self.plan.dir.join(WORK_DIR))
            && relative.components().count() == 1
            && let Some(key) = relative.to_str()
        {
            refuse_incomplete_input(&self.plan.dir, key).map_err(|error| error.to_string())?;
        }
        if let Some(cwd) = where_the_job_works(&step.job) {
            self.clean_native_result(cwd)?;
        }
        Ok(())
    }

    /// Czy znacznik zastanej kopii opisuje TO wejście i czy jego przygotowanie się dokończyło.
    ///
    /// Obie odmowy są głośne: kopia z cudzym wejściem albo z wejściem złożonym w połowie daje
    /// krok, który „przeszedł" nad plikami, których nikt nie złożył do końca.
    fn the_marker_still_fits(marker: Option<&IsolationMarker>, origin: &str) -> Result<(), String> {
        if marker.is_some_and(|one| one.origin_snapshot() != Some(origin)) {
            return Err(
                "This copy belongs to a different saved input. Nothing was overwritten.".to_owned(),
            );
        }
        if marker.is_some_and(IsolationMarker::has_incomplete_input) {
            return Err("This copy's input was not completely prepared. Start a new run; this partial folder was kept for inspection.".to_owned());
        }
        Ok(())
    }

    /// Odmawia, gdy rodzice tego kroku pochodzą z RÓŻNYCH prób tej samej rundy.
    ///
    /// Dwie jednakowo stare kopie z różnych prób nie są wejściem bieżącej rundy: złożone razem
    /// dałyby krok pracujący na pliku, którego w żadnej próbie nie było.
    fn refuse_mixed_tries(
        &self,
        step: &Planned,
        parents: &[fan_in::Parent<'_>],
    ) -> Result<(), String> {
        // Także dwóch jednakowo starych rodziców nie jest wejściem bieżącej rundy.
        if let Some(current) = self.generation_of(&step.node_key) {
            for parent in parents {
                if let Some(previous) = parent.born
                    && previous.loop_at == current.loop_at
                    && previous.which != current.which
                {
                    return Err(fan_in::Trouble::MixedTries {
                        one: parent.name.to_owned(),
                        one_try: previous.which,
                        other: step.name.clone(),
                        other_try: current.which,
                        of: current.of,
                    }
                    .to_string());
                }
            }
        }
        Ok(())
    }

    /// Zatrzymuje procesy każdej składanej kopii i bierze prawo do jej domknięcia.
    ///
    /// Gwarancje żyją tak długo, jak wynik tej funkcji: składanie plików pod żywym procesem
    /// to dwaj pisarze w jednym drzewie, a nie wolniejsze składanie.
    fn stop_the_copies_before_folding(
        &self,
        parents: &[fan_in::Parent<'_>],
        into: &Path,
    ) -> Result<Vec<super::processes::CopyFinalizationGuard>, String> {
        let mut parent_guards = Vec::new();
        let mut guarded = BTreeSet::new();
        for cwd in parents
            .iter()
            .map(|parent| parent.cwd)
            .chain(std::iter::once(into))
        {
            if !guarded.insert(cwd) {
                continue;
            }
            self.copy_processes_stopped(cwd)?;
            parent_guards.push(self.processes.try_finalize_copy(cwd).map_err(|error| error.to_string())?
                .ok_or_else(|| "A background process is still using one of these working folders. Stop it before combining its files.".to_owned())?);
            change_native_skills(&self.plan.dir, cwd, None).map_err(|error| error.to_string())?;
        }
        Ok(parent_guards)
    }

    /// Zapis o tym, CO i SKĄD ten krok właśnie do siebie zniósł.
    ///
    /// Delta importu jedzie tu razem z odciskami rodziców, bo następna runda odświeża wejście
    /// trójstronnie: bez niej własna praca konsumenta wygląda jak plik rodzica.
    fn the_fan_in_preparation(
        into: &Path,
        origin: &str,
        incoming: &fan_in::MergePlan,
        consumer: &str,
        parent_steps: Vec<Option<String>>,
    ) -> Result<FanInPreparation, String> {
        let imported = incoming
            .changes
            .iter()
            .map(|one| (one.path.clone(), one.after.clone()))
            .collect();
        Ok(FanInPreparation {
            work_key: into
                .file_name()
                .and_then(|one| one.to_str())
                .ok_or_else(|| "The copy has no valid input key.".to_owned())?
                .to_owned(),
            origin: origin.to_owned(),
            parents: incoming.parents().map_err(|error| error.to_string())?,
            plan_digest: incoming.digest().map_err(|error| error.to_string())?,
            input_ready: false,
            consumer: consumer.to_owned(),
            parent_steps,
            imported: Some(imported),
        })
    }

    /// Wystawia złożone pliki do kopii i dopiero po ich zapisaniu znaczy wejście jako gotowe.
    ///
    /// Kolejność jest tu treścią: znacznik z `input_ready` postawiony PRZED zapisem obiecywałby
    /// po awarii komplet, którego w folderze nie ma.
    fn stage_and_apply(
        &self,
        into: &Path,
        merge: &fan_in::MergePlan,
        marker_path: &Path,
        marker: &mut IsolationMarker,
        mut preparation: FanInPreparation,
    ) -> Result<fan_in::ApplyOutcome, String> {
        marker.set_fan_in(preparation.clone());
        write_isolation_marker(marker_path, marker).map_err(|error| error.to_string())?;
        let cancel = self.control.cancel_token();
        if cancel.is_cancelled() {
            return Ok(fan_in::ApplyOutcome::Cancelled);
        }
        let staged = fan_in::stage_plan(into, &self.plan.dir, merge)
            .map_err(|trouble| trouble.to_string())?;
        let applied = staged
            .apply(
                |into, applied| self.faults.after_fan_in_operation(into, applied),
                || cancel.is_cancelled(),
            )
            .map_err(|trouble| trouble.to_string())?;
        if applied == fan_in::ApplyOutcome::Cancelled || cancel.is_cancelled() {
            return Ok(fan_in::ApplyOutcome::Cancelled);
        }
        preparation.input_ready = true;
        marker.set_fan_in(preparation);
        write_isolation_marker(marker_path, marker).map_err(|error| error.to_string())?;
        Ok(fan_in::ApplyOutcome::Ready)
    }

    /// Znosi pracę rodziców tego kroku do JEDNEJ kopii — tej, w której zaraz stanie sterownik.
    ///
    /// `Ready` dla kroku, który niczego nie składa, i dla przygotowanego bieżącego wejścia;
    /// `Err` niesie gotowe zdanie dla człowieka.
    ///
    /// WF-03/20 (2026-09-06): trwałe inputReady dotyczy konkretnej rundy i jej rodziców.
    /// Następna runda odświeża import trójstronnie, zachowując własną pracę konsumenta.
    ///
    /// Zamek zamyka się i otwiera w jednym wyrażeniu, bez `await` w środku (niezmiennik 8).
    fn fold_what_came_before(&self, id: StepId) -> Result<fan_in::ApplyOutcome, String> {
        let step = &self.plan.steps[id];
        self.the_folder_is_fit_to_work_in(step)?;
        if step.folds_in.is_empty() {
            return Ok(fan_in::ApplyOutcome::Ready);
        }
        // Kafelek kontrolny nie pracuje w żadnym katalogu, więc nie ma dokąd składać. Do tego
        // ramienia nikt dziś nie dochodzi — kafelek bez folderu nie dostaje listy rodziców —
        // ale odpowiedź „nie ma dokąd" jest tu jedyną, która nie kłamie.
        let Some(into) = where_the_job_works(&step.job) else {
            return Ok(fan_in::ApplyOutcome::Ready);
        };
        let marker_path = prove_generated_work_path(&self.plan.project, &self.plan.dir, into)
            .map_err(|error| error.to_string())?;
        let snapshot = into
            .file_name()
            .and_then(|key| key.to_str())
            .and_then(|key| self.plan.workspace_inputs.get(key))
            .map(|seed| &seed.input)
            .or(self.plan.input_snapshot.as_ref())
            .ok_or_else(|| {
                "The saved input for combining these copies is missing. This step was not started."
                    .to_owned()
            })?;
        let mut marker = read_isolation_marker(&marker_path).map_err(|error| error.to_string())?;
        Self::the_marker_still_fits(marker.as_ref(), snapshot.id())?;
        // Migawka pod jednym zamkiem, oddanym w tym samym wyrażeniu (niezmiennik 8): rodzice
        // tego kroku już zeszli, więc nikt tych wpisów w trakcie nie przestawi, a zamek trzymany
        // przez całe składanie stałby otworem przez cały obchód dwóch drzew projektu.
        let parent_steps: Vec<Option<String>> = {
            let lineage = self.lineage.lock().unwrap_or_else(PoisonError::into_inner);
            step.folds_in
                .iter()
                .map(|one| lineage.get(&one.cwd).cloned())
                .collect()
        };
        let born: Vec<_> = parent_steps
            .iter()
            .map(|key| key.as_deref().and_then(|key| self.generation_of(key)))
            .collect();
        let parents: Vec<fan_in::Parent<'_>> = step
            .folds_in
            .iter()
            .zip(&born)
            .map(|(one, born)| fan_in::Parent {
                name: one.name.as_str(),
                cwd: one.cwd.as_path(),
                born: *born,
            })
            .collect();
        self.refuse_mixed_tries(step, &parents)?;
        let _parent_guards = self.stop_the_copies_before_folding(&parents, into)?;
        let incoming =
            fan_in::plan_frozen(&parents, snapshot).map_err(|trouble| trouble.to_string())?;
        let preparation = Self::the_fan_in_preparation(
            into,
            snapshot.id(),
            &incoming,
            &step.node_key,
            parent_steps,
        )?;
        let old = marker.as_ref().and_then(IsolationMarker::fan_in);
        if let Some(old) = old
            && old.input_ready
            && old.consumer == preparation.consumer
            && old.parent_steps == preparation.parent_steps
            && old.parents == preparation.parents
            && old.plan_digest == preparation.plan_digest
            && old.imported == preparation.imported
        {
            return Ok(fan_in::ApplyOutcome::Ready);
        }
        let empty = BTreeMap::new();
        let previous = match old {
            Some(old) => old.imported.as_ref().ok_or_else(||
                "This older copy does not record its previous combined input. Start a new independent run; its files were not overwritten.".to_owned())?,
            None => &empty,
        };
        let merge = incoming
            .refresh(snapshot, previous, into, &step.name)
            .map_err(|trouble| trouble.to_string())?;
        let marker = marker.get_or_insert_with(|| IsolationMarker::FileCopy {
            origin_snapshot: Some(snapshot.id().to_owned()),
            fan_in: None,
            native_skills: None,
        });
        self.stage_and_apply(into, &merge, &marker_path, marker, preparation)
    }

    /// Sam exit bieżącego lidera nie zwalnia kopii współdzielonej z innym procesem.
    fn copy_processes_stopped(&self, cwd: &Path) -> Result<(), String> {
        let book = self.book();
        if self
            .plan
            .steps
            .iter()
            .zip(&book.steps)
            .any(|(planned, state)| {
                where_the_job_works(&planned.job) == Some(cwd)
                    && state.execution.process_started
                    && !state.death_proof
            })
        {
            return Err("A process using this working folder has not been confirmed stopped. Its files were kept, not accepted as a result.".to_owned());
        }
        Ok(())
    }

    fn clean_native_result(&self, cwd: &Path) -> Result<(), String> {
        if !native_cleanup_needed(&self.plan.dir, cwd).map_err(|error| error.to_string())? {
            return Ok(());
        }
        self.copy_processes_stopped(cwd)?;
        let _guard = self.processes.try_finalize_copy(cwd).map_err(|error| error.to_string())?
            .ok_or_else(|| "A background process is still using this working folder. Its skill files were left untouched.".to_owned())?;
        change_native_skills(&self.plan.dir, cwd, None).map_err(|error| error.to_string())
    }

    /// Zapisuje, że to ten węzeł pracował w swoim katalogu — znacznik pochodzenia kopii.
    ///
    /// 2026-08-29 — PO SKŁADANIU I PRZED STEROWNIKIEM, i oba końce tej granicy są treścią.
    /// Po składaniu, bo krok, który się o nie rozbił, w swojej kopii nie pracował i nie ma czego
    /// stemplować. Przed sterownikiem, bo krok poniżej ma stanąć na tym samym fakcie, na którym
    /// stoi każdy inny: „ten węzeł tu wszedł", a nie „ten węzeł się udał" — kopia po nieudanej
    /// turze też trzyma jego pracę i też jest tej rundy.
    ///
    /// Kafelek kontrolny nie ma katalogu ([`where_the_job_works`]), więc nie zostawia stempla.
    fn stamp_this_copy(&self, id: StepId) {
        let step = &self.plan.steps[id];
        let Some(mine) = where_the_job_works(&step.job) else {
            return;
        };
        self.lineage
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(mine.to_path_buf(), step.node_key.clone());
    }

    /// Z której próby której pętli jest praca, którą zostawił po sobie ten węzeł.
    ///
    /// `None` dla węzła spoza wszystkich pętli, i to jest odpowiedź, a nie brak odpowiedzi: krok,
    /// który biegnie raz, nie należy do żadnej próby, więc nie ma z czym się nie zgadzać. Bez tego
    /// ramienia zwykłe „zaplanuj raz, pętla obok, potem ktoś to zbiera" byłoby odmową.
    ///
    /// Numer próby liczy się od jedynki, tą samą decyzją, co [`WhatItIs`]: `turn` jest polem
    /// danych, a to jest zdanie dla człowieka — „try 0 of 2" nie znaczy nic.
    fn generation_of(&self, node_key: &str) -> Option<fan_in::Generation> {
        let tile = tile_key_of(node_key);
        let (loop_at, the_loop) = self
            .plan
            .loops
            .iter()
            .enumerate()
            .find(|(_, one)| one.body.iter().any(|step| step == tile))?;
        Some(fan_in::Generation {
            loop_at,
            which: tries_in(node_key).saturating_add(1),
            of: the_loop.turns,
        })
    }

    /// Domyka krok dopiero po rozstrzygnięciu, czy jego udany wynik ma jedną prawidłową drogę.
    ///
    /// 2026-08-25 (T-101) — `Route::Blocked` rozstrzygane w schedulerze było za późno: krok
    /// zdążył zapisać i wysłać `succeeded`, a dopiero potem księga kończyła z `failed`. Tutaj
    /// odmowa przechodzi przez [`Live::when_this_one_fails`] PRZED jednym zapisem i jedną linią
    /// stanu, więc polityka kroku oraz to, co widzi człowiek, mają ten sam wynik.
    async fn finish_this_step(&self, id: StepId, report: StepReport) -> StepReport {
        let report = if report != StepReport::Cancelled && self.plan.inputs.is_producer(id) {
            match self.freeze_result_files(id) {
                Ok(files) => {
                    self.update(|book| book.steps[id].result_files = files);
                    report
                }
                Err(error) => {
                    self.update(|book| {
                        book.steps[id].error = Some(format!(
                            "The result files could not be saved for the next check: {error}"
                        ));
                        book.steps[id].end_cause =
                            Some(super::run_inputs::EndCause::InfrastructureFailed);
                    });
                    StepReport::Failed
                }
            }
        } else {
            report
        };
        let report = if report == StepReport::Succeeded {
            match self.chosen_route_after(id) {
                Ok(_) => report,
                Err(why) => {
                    // Dokładny istniejący powód wygrywa z dopiskiem polityki, tak samo jak przy
                    // innych odmowach startu. `when_this_one_fails` wybiera skutek, nie treść.
                    self.update(|book| book.steps[id].error = Some(why.clone()));
                    self.when_this_one_fails(id, &why).await
                }
            }
        } else {
            report
        };

        let ended = match report {
            StepReport::Succeeded => StepState::Succeeded,
            // Oba warianty porażki dają ten sam STAN. Różnią się wyłącznie tym, co planista
            // robi z potomkami — a stan mówi o tym kroku, nie o jego stożku.
            StepReport::Failed | StepReport::FailedAndCarriedOn => StepState::Failed,
            StepReport::Cancelled => StepState::Cancelled,
        };
        self.update(|book| {
            let step = &mut book.steps[id];
            step.status = ended;
            if ended == StepState::Cancelled {
                step.end_cause = Some(super::run_inputs::EndCause::Cancelled);
            } else if step.end_cause.is_none() {
                step.end_cause = Some(if ended == StepState::Succeeded {
                    super::run_inputs::EndCause::Completed
                } else {
                    super::run_inputs::EndCause::InfrastructureFailed
                });
            }
            if step.execution.executed {
                step.ended_at = Some(now_ms());
            }
        });
        /* NIEWYKORZYSTANA RESZTA UDZIAŁU WRACA DO SUFITU (2026-09, Z-13b). Tędy schodzi każdy
         * krok, więc to jest jedyne miejsce, w którym wystarczy to napisać raz — a bez tego
         * pierwszy krok, który wydał mniej, niż mu przyznano, zamraża różnicę do końca biegu
         * i kolejne kroki dzielą sufit, którego część nie należy już do nikogo.
         *
         * POZA `update`, nie w środku: udział przyznaje się pod zamkiem rezerwacji, który bierze
         * potem zamek księgi, i ta kolejność ma w tym pliku pozostać jedyna (niezmiennik 8). */
        self.give_back_the_share(id);
        if report == StepReport::FailedAndCarriedOn {
            // JEDNA LINIA NIESIE CAŁE ROZSTRZYGNIĘCIE: krok padł ORAZ polityka puściła bieg dalej.
            // `LineSink` jest celowo stratny, więc para `StepState::Failed` + `StepCarriedOn`
            // mogła rozdzielić się na granicy kolejki i zostawić człowiekowi dokładnie połowę
            // prawdy. Nieznany addytywny wariant starsze okno porzuci jako jeden fakt; dzisiejsze
            // przy jego odbiorze ustawia oba pola atomowo.
            let _ = self.lines.send(Line::StepCarriedOn {
                agent: self.plan.steps[id].name.clone(),
                // Klucz Z PLIKU, tak samo jak w `announce`: fizyczne rundy pętli i kopie mają
                // w księdze osobne `node_key`, ale okno zna wyłącznie klucz kafelka.
                step_id: self.plan.steps[id].tile_key.clone(),
            });
        } else {
            self.announce(id, ended);
        }
        report
    }

    /// Dostawca kazał czekać: bieg przestaje wysyłać i **mówi o tym dyskowi**.
    ///
    /// Adapter na pięć linii, polityka w jednym rdzeniu (niezmiennik 23): co znaczy „odmowa",
    /// rozstrzyga `engine::limits::read_gate` i wyłącznie on — prawdziwa linia z **udanego**
    /// biegu niesie `"status":"allowed"` obok dwóch pól ze słowem „rejected", a te zdarzenia to
    /// 1,3% normalnego strumienia `[T7 §4.3, V]`. Wersja czytająca tutaj `pause_run` byłaby
    /// drugą kopią tej reguły, a przy dwóch kopiach zawsze czyta się tę, która akurat kłamie.
    /// Wracają więc na drut dokładnie te dwa pola, które tamten czyta, pod nazwami z drutu.
    ///
    /// **Chwili powrotu nie zapisujemy do `run.json`** i to jest wybór (niezmiennik 13): niesie
    /// ją już wiersz kuratora (`Line::Problem::resets_at`), a godzinę lokalną rysuje z niej front
    /// [T7 §7.2]. Druga kopia byłaby drugą rzeczą do utrzymania w zgodzie, a jedna z dwóch jest
    /// zawsze tą nieaktualną. Na dysku zostaje sam fakt „bieg stoi" — bo stan, który nie dociera
    /// na dysk, nie przeżywa awarii aplikacji (niezmiennik 4).
    fn the_provider_said_wait(&self, status: &str, resets_at: i64) {
        let told = serde_json::json!({ "status": status, "resetsAt": resets_at });
        if let limits::Gate::PausedUntil(_) = self.gate.pause_handle().saw(&told, now_unix()) {
            self.update(|book| run_stands_or_moves(book, true));
        }
    }

    /// Miejsce ze **wspólnej puli aplikacji**, i limit dostawcy w tej samej pętli.
    ///
    /// `None` znaczy „nie ruszaj tego kroku": bieg zatrzymał człowiek, zanim to miejsce się
    /// zwolniło. Nie jest to błąd i nie jest to `Err` (niezmiennik 7) — Stop, który mimo
    /// wszystko wpuszcza agenta po to, żeby go zaraz zabić, płaci dostawcy za turę, której
    /// nikt nie zobaczy.
    ///
    /// **Pętla, nie jedno pytanie**, bo odmowy są dwie różne i tylko jedna z nich mija sama:
    /// [`limits::Refusal::Paused`] wraca natychmiast i mówi „nie teraz", a czekanie na wolne
    /// miejsce siedzi już w środku [`limits::Run::dispatch`]. Bieg, który po odmowie zaczeka na
    /// miejsce w puli, trzymałby zasób potrzebny komuś, kto może biec, i zajmował go przez całe
    /// pięciogodzinne okno limitu.
    /// Ile ten bieg zdążył wydać: suma cen tur, które SIĘ SKOŃCZYŁY.
    ///
    /// Krok w połowie tury nie wie jeszcze, ile będzie kosztował, a krok, którego vendor ceny
    /// nie podaje, liczy się jako zero — obie te rzeczy mówi zdanie pomocy przy kontrolce sufitu,
    /// bo obie są widoczne dla człowieka jako różnica między rachunkiem a tą liczbą.
    fn spent_so_far(&self) -> f64 {
        step_spend_in(&self.book.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Co każdy krok powiedział na koniec — po jednej linii, w kolejności z grafu.
    ///
    /// 2026-09 (Z-38) — TA SAMA LINIA, KTÓRĄ CZŁOWIEK CZYTA PRZY KAFELKU, nie druga jej wersja:
    /// `summary` powstaje raz, z ostatniej wypowiedzi agenta ([`summary_of`]), i stąd jedzie do
    /// `run.json`, na szynę agentów i — od tego zadania — do promptu refleksji. Refleksja bez
    /// tego zna wyłącznie ZLECENIA (tytuł przekazania jest instrukcją z pliku workflow) i musi
    /// otworzyć każdy plik, żeby dowiedzieć się, co z nich wyszło.
    ///
    /// Krok, który nic nie powiedział — kafelek „sprawdź", tura zdjęta przed pierwszym słowem —
    /// wypada z listy. Wiersz z pustym zdaniem jest tokenami wydanymi na myślnik.
    ///
    /// Zamek na księdze powstaje i ginie w tym jednym wywołaniu, bez `await` (niezmiennik 8).
    fn what_the_steps_said(&self) -> Vec<SaidByAStep> {
        let book = self.book();
        self.plan
            .steps
            .iter()
            .zip(&book.steps)
            .filter_map(|(step, run)| {
                let said = run.summary.as_deref().map(str::trim).unwrap_or_default();
                (!said.is_empty()).then(|| SaidByAStep {
                    name: step.name.clone(),
                    said: said.to_owned(),
                })
            })
            .collect()
    }

    /// Ile z sufitu jest już ROZDYSPONOWANE: każda zaksięgowana cena plus udziały odłożone dla
    /// kroków, które pracują w tej chwili.
    ///
    /// 2026-09 (Z-13b) — INNE PYTANIE NIŻ [`Live::spent_so_far`] i dlatego inna funkcja. Tamto
    /// odpowiada człowiekowi „ile ten bieg wydał" i stoi w `run.json.spent_usd` oraz w zdaniu
    /// o pominiętym kroku, więc liczy wyłącznie tury, które SIĘ SKOŃCZYŁY. To odpowiada bramce
    /// „ile z sufitu jest już zajęte", a zajęte jest jedno i drugie: tura opłacona przed zejściem
    /// kroku i udział odłożony dla tury, która właśnie trwa.
    ///
    /// Wektor rezerwacji przychodzi ARGUMENTEM, bo jedyne miejsce, które potrzebuje tej sumy pod
    /// własnym zamkiem, to przyznawanie udziału — a `std::sync::Mutex` nie jest wejściowalny
    /// ponownie. Kolejność zamków jest tam ustalona i jest tylko jedna: rezerwacje, potem księga.
    fn what_this_run_has_committed(&self, shares: &[Option<f64>]) -> f64 {
        let priced: f64 = self
            .book()
            .steps
            .iter()
            .filter_map(|step| step.cost_usd)
            .sum();
        priced + shares.iter().flatten().sum::<f64>()
    }

    /// Przez ile ten sufit dzieli się w tej chwili — **szerokość równoległości**, nigdy mniej
    /// niż jeden.
    ///
    /// 2026-09 (Z-13b) — DLACZEGO NIE „ILE BIEGNIE TERAZ + 1". Bo przy PIERWSZYM starcie ta
    /// liczba wynosi jeden: pierwszy krok rezerwuje wtedy całą resztę, a kroki obok niego kończą
    /// jako pominięte. Sufit robi się twardy kosztem równoległości, czyli kosztem jedynej rzeczy,
    /// dla której ten produkt istnieje (niezmiennik 11). Dzielnikiem jest więc liczba, którą
    /// człowiek ustawił suwakiem, przycięta do tego, ile kroków w ogóle ma teraz prawo ruszyć —
    /// inaczej bieg liniowy przy suwaku na ośmiu marnowałby siedem ósmych sufitu.
    ///
    /// LICZONE Z KSIĘGI I Z GRAFU, nie z licznika wejść do tury: licznik jest wyścigiem, a wyścig
    /// wygrany przez pierwszy krok to dokładnie ta wada, którą to zamyka. „Ma teraz prawo ruszyć"
    /// znaczy: sam jeszcze nie osiadł, a każdy jego rodzic już osiadł.
    ///
    /// PRZESZACOWANIE JEST BEZPIECZNE, NIEDOSZACOWANIE NIE. Dzielnik policzony za szeroko —
    /// bo w rachunku stoi krok, którego stożek i tak zaraz padnie — daje udziały mniejsze od
    /// potrzebnych i najwyżej zostawia niewydane pieniądze. Za wąski pozwala wydać ponad sufit.
    fn how_many_share_the_rest(&self) -> usize {
        let settled: Vec<bool> = self
            .book()
            .steps
            .iter()
            .map(|step| has_settled(step.status))
            .collect();
        let ready = settled
            .iter()
            .enumerate()
            .filter(|&(child, &is_over)| {
                !is_over
                    && self
                        .plan
                        .arrows
                        .iter()
                        .all(|&(parent, to)| to != child || settled[parent])
            })
            .count();
        self.gate.at_once().min(ready).max(1)
    }

    /// Udział tego kroku w tym, co z sufitu zostało — `None`, kiedy nikt sufitu nie postawił
    /// albo kiedy ten krok jedzie mimo niego.
    ///
    /// `keep_it` znaczy „odłóż tę kwotę do zejścia kroku i policz ją innym jako zajętą". Pytanie
    /// sprzed kolejki odkłada NIC, i to jest treść: krok, który dopiero stanie po miejsce z puli,
    /// trzymałby pieniądze potrzebne komuś, kto właśnie pracuje, a do swojej tury wszedłby potem
    /// z kwotą policzoną przed czekaniem.
    ///
    /// Nigdy poniżej zera: kwota ujemna oddana vendorowi jest albo błędem składni przy starcie,
    /// albo — gorzej — argumentem, który znaczy wtedy co innego.
    fn a_share_of_what_is_left(&self, id: StepId, keep_it: bool) -> Option<f64> {
        if self.carries_on_past_the_budget(id) {
            return None;
        }
        let budget = self.budget_usd?;
        // Obie te odpowiedzi biorą zamek księgi, więc padają PRZED zamkiem rezerwacji: kolejność
        // rezerwacje → księga jest w tym pliku jedyna i ma taka zostać (niezmiennik 8).
        //
        // `u32`, nie `usize`, bo `f64::from(u32)` jest bezstratne i dzielenie niżej nie potrzebuje
        // wtedy ani jednej linii wyciszającej lint. Szerokość mieści się w `1..=8`
        // (`limits::clamp_at_once`), więc wartość zapasowa nie pada nigdy — a gdyby padła, myli
        // się w stronę bezpieczną: im większy dzielnik, tym mniejszy udział.
        let width = u32::try_from(self.how_many_share_the_rest()).unwrap_or(u32::MAX);
        let can_spend = matches!(self.plan.steps[id].job, Job::Agent(_));
        let mut shares = self
            .share_of_the_budget
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let left = (budget - self.what_this_run_has_committed(&shares)).max(0.0);
        /* DZIELNIKIEM JEST SZEROKOŚĆ RÓWNOLEGŁOŚCI, nie liczba kroków biegnących w tej chwili
         * (2026-09, Z-13b). Powód w całości stoi przy [`Live::how_many_share_the_rest`] i jest
         * jednym zdaniem: tamta liczba przy pierwszym starcie wynosi jeden. */
        let share = left / f64::from(width);
        /* ODKŁADAMY WYŁĄCZNIE DLA KROKU, KTÓRY MA CZYM WYDAĆ. Kafelek „sprawdź" bierze miejsce
         * z puli i pyta o sufit tą samą drogą, ale nie woła żadnego agenta i nie ma ceny —
         * pieniądze odłożone dla niego zabrałyby udział krokom, które naprawdę płacą, i wróciłyby
         * dopiero po jego komendzie. */
        if keep_it
            && can_spend
            && let Some(row) = shares.get_mut(id)
        {
            *row = Some(share);
        }
        Some(share)
    }

    /// Ile ten krok dostałby, gdyby ruszył teraz — bez odkładania czegokolwiek.
    fn what_a_share_would_be(&self, id: StepId) -> Option<f64> {
        self.a_share_of_what_is_left(id, false)
    }

    /// Udział tego kroku, policzony i **odłożony w jednym wyrażeniu**.
    ///
    /// Jedno wyrażenie, bo dwa kroki, które dostają miejsce z puli w tej samej chwili, inaczej
    /// przyznałyby sobie tę samą resztę — czyli dokładnie tę wadę, którą Z-13b zamyka.
    fn a_share_for(&self, id: StepId) -> Option<f64> {
        self.a_share_of_what_is_left(id, true)
    }

    /// Ile ten krok trzyma w tej chwili — tyle, ile mu wolno wydać do końca jego tury.
    fn the_share_held_by(&self, id: StepId) -> Option<f64> {
        self.share_of_the_budget
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .copied()
            .flatten()
    }

    /// Tura wróciła z prawdziwą ceną: udział maleje o to, ile naprawdę kosztowała.
    ///
    /// MALEJE, A NIE ZNIKA (2026-09, Z-13b), i to jest różnica między rozliczeniem a odjęciem
    /// drugi raz. Po tej linii suma „zaksięgowana cena + reszta udziału" jest dalej równa temu,
    /// co temu krokowi przyznano, więc nie ma ani jednej chwili, w której zapłacone pieniądze
    /// są dla kroków obok niewidzialne. Reszta zostaje odłożona do zejścia kroku, bo do tej
    /// chwili krok trzyma jeszcze miejsce z puli i formalnie może wydać dalej — zwolnić ją
    /// wcześniej znaczyłoby obiecać komuś obok pieniądze, które wciąż mają właściciela.
    fn settle_the_share(&self, id: StepId, spent: f64) {
        let mut shares = self
            .share_of_the_budget
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(Some(share)) = shares.get_mut(id) {
            *share = (*share - spent).max(0.0);
        }
    }

    /// Oddaje resztę udziału, której ten krok już nie wyda.
    ///
    /// Woła to każde zejście kroku ([`Live::finish_this_step`]) oraz odmowa startu z powodu
    /// sufitu — bez tego pierwsza porażka zamraża pieniądze do końca biegu i kolejne kroki
    /// dostają udziały z sufitu, którego część nie należy już do nikogo.
    fn give_back_the_share(&self, id: StepId) {
        if let Some(row) = self
            .share_of_the_budget
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(id)
        {
            *row = None;
        }
    }

    /// Zdanie o sufcie, albo `None`, kiedy z tego udziału da się jeszcze zapłacić za turę.
    ///
    /// PYTA O UDZIAŁ, NIE O CAŁĄ RESZTĘ (2026-09, Z-13b), i to jest jedna droga odmowy dla obu
    /// powodów: sufit wyczerpany daje udział zerowy, a sufit tak niski, że nie starcza na kroki
    /// stojące obok siebie, daje udział poniżej centa. Oba są tym samym „nie ma za co ruszyć"
    /// i człowiek ma o obu przeczytać to samo.
    ///
    /// Zdanie powstaje TUTAJ, razem z decyzją, i niesie obie liczby: bez nich pominięty krok jest
    /// nie do odróżnienia od kroku pominiętego przez cudzą porażkę, a bieg kończący się rzędem
    /// pustych wierszy jest tym ślepym punktem, dla którego to repo powstało. Liczone z kwoty
    /// ZAKSIĘGOWANEJ, nie z rozdysponowanej: zdanie o pieniądzach, których nikt jeszcze nie
    /// wydał, mówiłoby człowiekowi coś, czego nie ma na rachunku.
    fn not_a_cent_to_run_on(&self, share: f64) -> Option<String> {
        let budget = self.budget_usd?;
        let spent = self.spent_so_far();
        // 2026-09 (Z-12): księga, zdanie i flaga vendora rozliczają centy. Udział mniejszy niż
        // cent nie daje dodatniej kwoty, którą wolno przekazać CLI, więc jest już wydany.
        (share < 0.01).then(|| {
            format!(
                "Skipped: this run had spent ${spent:.2} of the ${budget:.2} it was allowed, so \
                 nothing new was started. Steps already working were left to finish."
            )
        })
    }

    /// To samo pytanie sprzed kolejki: czy jest po co stawać po miejsce z puli.
    fn the_budget_is_spent(&self, id: StepId) -> Option<String> {
        self.not_a_cent_to_run_on(self.what_a_share_would_be(id)?)
    }

    /// Zdanie o kroku, którego tury nie da się wycenić — albo `None`, kiedy da się albo kiedy
    /// nikt sufitu nie postawił.
    ///
    /// # Dlaczego sufit rozstrzyga (2026-09, Z-44)
    ///
    /// Bo bez niego nie ma czego dotrzymać. Bieg bez sufitu jedzie z nieznanym modelem dokładnie
    /// tak, jak jechał: liczniki wracają, a wiersz końcowy mówi człowiekowi, że ceny nie zna
    /// (Z-12). Dopiero kwota postawiona w Settings jest obietnicą — a obietnica, której produkt
    /// nie umie dotrzymać, jest gorsza od jej braku.
    ///
    /// PYTA STEROWNIK, NIE NAZWĘ VENDORA. Rdzeń wie, że sufit trzeba dotrzymać; czy dla tego
    /// modelu da się to policzyć, wie wyłącznie adapter (niezmiennik 23). Warunek
    /// `if driver.id() == "codex"` byłby tą samą wadą, którą T-92 opisało przy `with_settings`:
    /// każdy dubel podający się za tego vendora dostawałby odmowę o szew, o którym nic nie wie.
    ///
    /// # Stożka `carry-on` ta bramka NIE pomija, choć sąsiednia pomija
    ///
    /// [`Live::a_share_of_what_is_left`] przepuszcza stożek, któremu człowiek jawnie kazał jechać
    /// mimo sufitu (T-101), i to jest tam poprawne: tamto pytanie brzmi „czy zostało dość
    /// pieniędzy", a odpowiedź człowieka brzmiała „wydaj więcej". Tu pytanie jest inne — „czy da
    /// się je w ogóle policzyć" — i na nie `carry-on` nigdy nie odpowiedział. Jechać mimo KWOTY
    /// to nie to samo, co przestać ją MIERZYĆ: po turze bez ceny sufit nie jest przekroczony,
    /// tylko nieznany, i to do końca biegu.
    ///
    /// 2026-09 (Z-44, druga runda) — TA GAŁĄŹ TU BYŁA I BYŁA DZIURĄ, bo `carry-on` jest wartością
    /// DOMYŚLNĄ (`workflow::WhenItFails`, decyzja właściciela 2026-08-23). Pierwsza odmowa robiła
    /// więc ze swojego następnika stożek, a stożek omijał tę bramkę — czyli w każdym zwykłym
    /// pliku o dwóch krokach na strzałce drugi krok uruchamiał CLI z modelem bez ceny, pod
    /// postawionym sufitem. Zmierzone na `a_step_after_a_refused_one_is_refused_too…`:
    /// `[Skipped, Succeeded]` zamiast `[Skipped, Skipped]`. Bramka nieznanej ceny pyta więc
    /// wyłącznie o sufit i o sterownik, i nie ma ani jednego wyjątku.
    fn no_way_to_price_this_one(&self, job: &AgentJob) -> Option<String> {
        let budget = self.budget_usd?;
        if job
            .driver
            .can_price_a_turn(job.model.as_deref(), &self.plan.prices)
        {
            return None;
        }
        // Ten sam zwrot, którym o modelu bez nazwy mówi uwaga po turze (`unknown_price_notice`):
        // dwa zdania o tym samym braku, napisane osobno, rozjeżdżają się przy pierwszej zmianie.
        let model = job.model.as_deref().unwrap_or(THE_MODEL_WITH_NO_NAME);
        Some(format!(
            "Loadout doesn't know what {model} costs, so it can't keep this run under \
             ${budget:.2}. Set the price in {WHERE_PRICES_LIVE} or run without a ceiling."
        ))
    }

    /// Czy ten krok stoi w stożku, któremu człowiek jawnie kazał jechać mimo sufitu.
    ///
    /// 2026-08-25 (T-101) — WYJĄTEK JEST STOŻKIEM, NIE WYŁĄCZNIKIEM CAŁEGO BIEGU. Sam fakt
    /// przekroczenia dalej zatrzymuje nowe kroki (T-94). Dopiero budżetowa odmowa, która przeszła
    /// przez `when_this_one_fails` i zapaliła `did_not_pass`, oznacza jawne `carry-on` albo
    /// odpowiedź człowieka na `ask-me`; wtedy kroki po NIEJ mają naprawdę wystartować. Gałąź
    /// równoległa, która nie leży pod tą decyzją, nadal pyta o sufit jak wcześniej.
    ///
    /// Liczone z dwóch istniejących faktów, bez trzeciej flagi, która mogłaby się z nimi rozjechać:
    /// dokładne zdanie w `stopped_by_the_budget` mówi, że korzeń zatrzymał sufit, a `did_not_pass`
    /// mówi, że wspólna polityka puściła jego pracę dalej.
    fn carries_on_past_the_budget(&self, id: StepId) -> bool {
        let stopped: Vec<bool> = self
            .stopped_by_the_budget
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(Option::is_some)
            .collect();
        let carried: Vec<bool> = self
            .did_not_pass
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(Option::is_some)
            .collect();
        let mut seen = vec![false; self.plan.steps.len()];
        let mut stack: Vec<StepId> = stopped
            .iter()
            .zip(carried.iter())
            .enumerate()
            .filter_map(|(root, (&by_budget, &carried_on))| {
                (by_budget && carried_on).then_some(root)
            })
            .collect();
        while let Some(from) = stack.pop() {
            for &(parent, child) in &self.plan.arrows {
                if parent != from || std::mem::replace(&mut seen[child], true) {
                    continue;
                }
                if child == id {
                    return true;
                }
                stack.push(child);
            }
        }
        false
    }

    /// Krok, którego nie ruszamy, bo pieniądze się skończyły.
    ///
    /// To nie jest Stop człowieka: skutek wybiera `whenItFails`, a stan końcowy tłumaczy na
    /// `skipped` [`Live::name_what_the_budget_stopped`]. Dzięki temu `carry-on` naprawdę puszcza
    /// pracę dalej, `ask-me` naprawdę pyta, a raport nie nazywa sufitu anulowaniem.
    async fn the_budget_stops_this_one(&self, id: StepId, said: String) -> StepReport {
        // Odłożony udział wraca do sufitu TUTAJ, bo ta droga nie przechodzi przez
        // [`Live::finish_this_step`]: krok odmówiony po przyznaniu miejsca zdążył swój udział
        // odłożyć i bez tej linii trzymałby te pieniądze do końca biegu (2026-09, Z-13b).
        self.give_back_the_share(id);
        if let Some(row) = self
            .stopped_by_the_budget
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(id)
        {
            *row = Some(said.clone());
        }
        self.update(|book| {
            let step = &mut book.steps[id];
            step.status = StepState::Skipped;
            let _ = step.error.get_or_insert(said.clone());
        });
        self.announce(id, StepState::Skipped);
        self.when_this_one_fails(id, &said).await
    }

    async fn a_slot_for_this_step(
        &self,
        cancel: &CancellationToken,
        weight: limits::Weight,
    ) -> Option<limits::Slot> {
        // Czy ten krok kiedykolwiek odbił się od limitu. Tylko taki krok ma prawo ogłosić, że
        // bieg rusza dalej: inaczej każdy zwykły krok pisałby `running` po biegu, który stoi
        // z zupełnie innego powodu.
        let mut waited = false;
        loop {
            let asked = tokio::select! {
                // `biased`, żeby Stop wygrywał z miejscem zwalnianym w tej samej chwili: krok,
                // który po Stopie dostaje permit, startuje agenta i zabija go w następnej
                // linijce.
                biased;
                () = cancel.cancelled() => return None,
                asked = self.gate.dispatch_as(weight) => asked,
            };
            match asked {
                limits::Dispatch::Granted(slot) => {
                    if waited {
                        // Bieg ruszył dalej SAM, o `resetsAt`, i nikt nie musiał nic nacisnąć.
                        // Pytamy bramę jeszcze raz zamiast wpisać `false`: druga linia limitu
                        // mogła wejść, kiedy spaliśmy, i wtedy bieg dalej stoi.
                        let still = !self.gate.still_paused_for().is_zero();
                        self.update(|book| run_stands_or_moves(book, still));
                    }
                    return Some(slot);
                }
                limits::Dispatch::Refused(limits::Refusal::Paused) => {
                    waited = true;
                    // Dokładnie do końca pauzy i ani razu wcześniej. Wersja pytająca co sto
                    // milisekund budzi bieg trzy tysiące razy w pięciogodzinnym oknie, żeby
                    // 2999 razy usłyszeć to samo — a `resetsAt` jest znane od pierwszego
                    // zdarzenia i nikt go po drodze nie skraca (`engine::limits`).
                    let left = self.gate.still_paused_for();
                    tokio::select! {
                        biased;
                        () = cancel.cancelled() => return None,
                        () = tokio::time::sleep(left) => {}
                    }
                }
            }
        }
    }

    /// Vendorowy fragment argv tego kroku: zatwierdzone Connections plus szczebel „ile myśleć".
    ///
    /// # Dlaczego szczebel jedzie TĘDY, a nie polem [`RunSpec`] (2026-08-23, T-91)
    ///
    /// `RunSpec` nie ma `Default` i konstruuje go w tym drzewie ponad trzydzieści miejsc, więc
    /// nowe pole w literale byłoby trzydziestoma plikami zmienionymi po to, żeby dowieźć jedną
    /// flagę. [`DriverConfiguration::arguments`] jest już kanałem na „gotowy fragment argv tego
    /// jednego kroku" — tędy jadą zatwierdzone Connections i tędy jedzie to.
    ///
    /// PO połączeniach, nie przed: kolejność opcji globalnych nie zmienia u żadnego z vendorów
    /// znaczenia, ale kolejność STAŁA znaczy, że dwa identyczne biegi dają ten sam napis w `ps` —
    /// a to jest jedyny sposób, żeby dało się je porównać.
    ///
    /// Ta warstwa nie zna ani jednej flagi vendora (niezmiennik 9): pyta adapter
    /// ([`AgentDriver::effort_argv`]) o parę gotową do wklejenia, a o sam poziom — jedyną tabelę
    /// przy szczeblu. Pusto znaczy „ten vendor nie ma czym tego przyjąć" (atrapy silnika,
    /// `absent`) i wtedy do argv nie idzie ani jeden bajt więcej.
    fn vendor_arguments_for(
        &self,
        id: StepId,
        job: &AgentJob,
        service_connection: Option<&crate::connections::Connection>,
    ) -> anyhow::Result<DriverConfiguration> {
        let mut connections = job.connections.clone();
        if let Some(connection) = service_connection {
            if connections.iter().any(|one| one.name == connection.name) {
                return Err(anyhow::anyhow!(
                    "an approved Connection uses the name reserved for Loadout's app tools; rename that Connection before starting this step"
                ));
            }
            connections.push(connection.clone());
        }
        let mut configuration = if connections.is_empty() {
            DriverConfiguration::default()
        } else {
            let directory = self
                .plan
                .dir
                .join("connections")
                .join(&self.plan.steps[id].node_key);
            /* WARTOŚCI BIERZE NOŚNIK LOADOUTA, nie samo środowisko tego okna (2026-09, Z-23):
             * bieg zaczęty z aplikacji uruchomionej z Docka nie widział ani jednej zmiennej
             * wyeksportowanej w powłoce człowieka, więc krok z zatwierdzonym Połączeniem
             * odmawiał tam, gdzie ten sam krok z terminala ruszał. Nośnik przyjechał planem —
             * powód przy [`Plan::secrets`]. */
            crate::connections::runtime::for_driver_with_secrets(
                &directory,
                job.driver.id(),
                &connections,
                &self.plan.secrets,
            )?
        };
        configuration
            .arguments
            .extend(job.driver.effort_argv(effort_level(job.thinking)));
        /* SUFIT WYDATKU JEDZIE TĄ SAMĄ RURĄ, i po tym samym co wyżej: to jest „gotowy fragment
         * argv tego jednego kroku".
         *
         * PO CO, SKORO SUFIT PILNUJE JUŻ BIEG. Bo bieg liczy tury SKOŃCZONE: sumę zna po fakcie,
         * a tura, która sama jedna przebije resztę, jest już wtedy opłacona. Vendor, któremu
         * powiemy, ile mu wolno, zatrzyma ją od środka.
         *
         * PYTAMY KROK, CZYM JEST — dokładnie tak samo, jak przy zatwierdzonych Connections dwie
         * linie wyżej (`connections::runtime::for_driver` bierze `driver.id()`). Nazwa flagi
         * mieszka w adapterze tego jednego vendora (niezmiennik 23), a decyzja „ile jeszcze
         * wolno" tutaj, w rdzeniu.
         *
         * 2026-08-24 — CZYSTSZY SZEW WYMAGAŁBY METODY W `engine/drivers/mod.rs` (obok
         * `effort_argv`), a tamten plik nie należy do T-94 (`AGENTS.md` §7). Zgłoszone,
         * nie rozstrzygnięte tutaj.
         *
         * 2026-09 (Z-13b) — JEDZIE TU UDZIAŁ TEGO KROKU, NIE CAŁA RESZTA SUFITU. Reszta oddana
         * w całości każdemu startującemu krokowi znaczy, że bieg z suwakiem na trzech może wydać
         * trzy sufity — i że drugi oraz trzeci krok dostają liczbę, która była prawdziwa tylko
         * dla pierwszego. Kwota jest już odłożona ([`Live::a_share_for`] przy miejscu z puli),
         * więc tutaj się ją wyłącznie czyta. */
        if let Some(share) = self.the_share_held_by(id)
            && job.driver.id() == crate::engine::drivers::claude::VENDOR
        {
            configuration
                .arguments
                .extend(crate::engine::drivers::claude::budget_argv(share));
        }

        /* PRZELOTKA NA KOŃCU, i to jest wybór o wsteczności: krok, którego agent nie prosi
         * o nic ponad, dostaje argv **co do bajtu** takie jak przedtem, bo ta lista jest wtedy
         * pusta. Fragment jest już w kształcie tej aplikacji (`library::agents::vendor_argv`) —
         * ta warstwa nie skleja komendy i nie zna ani jednej flagi vendora (niezmiennik 9). */
        configuration
            .arguments
            .extend(job.passthrough.iter().cloned());
        Ok(configuration)
    }

    /// Sterownik tego kroku, niosący fragment argv, który ten bieg odziedziczył — albo odmowa.
    ///
    /// Fragment niesie nazwę flagi **jednego** vendora (`--plugin-dir`), więc sterownik, który
    /// jej nie zna, nie może jej dostać i nie ma jak jej udawać. Pytamy o to
    /// [`AgentDriver::inheriting`], bo fabryka wydaje sterownik jako `Arc<dyn AgentDriver>`
    /// i konkretny typ jest tu już zgubiony.
    ///
    /// ODMOWA, NIE CICHE POMINIĘCIE, i to jest jedyny powód, dla którego ta funkcja zwraca
    /// `Result`. Krok, który po cichu nie dostał wybranych umiejętności, kończy się „sukcesem"
    /// i odpowiedzią bez nich: „agent nie zna umiejętności" jest z zewnątrz nieodróżnialne od
    /// „model nie uznał, że warto jej użyć". Zdanie odmowy jedzie tą samą drogą, co nieudany
    /// start procesu — wierszem na ekranie kroku i polem `error` w `run.json`.
    ///
    /// Skojarzona, nie metoda na `&self`, i to jest zdanie o tym zadaniu: do 2026-08-23 czytała
    /// stąd `Live::inherited`, czyli JEDEN wybór na cały bieg. Odkąd wybór jest własnością
    /// kafelka, odpowiedź zależy wyłącznie od argumentów — a `self` w podpisie sugerowałby, że
    /// gdzieś w biegu stoi drugie źródło tego, co ten krok pożyczył.
    fn carrying_what_we_inherited(
        driver: &Arc<dyn AgentDriver>,
        from_the_host: &[String],
        of_the_step: &[String],
    ) -> anyhow::Result<Arc<dyn AgentDriver>> {
        if from_the_host.is_empty() && of_the_step.is_empty() {
            // Nic do niesienia i nie ma o co pytać: ten sam sterownik, bez klonowania czegokolwiek
            // poza licznikiem.
            return Ok(Arc::clone(driver));
        }
        // JEDEN FRAGMENT, DWA ŹRÓDŁA. Adapter dostaje gotową listę i dalej nie wie, czym jest
        // umiejętność ani skąd przyszła (niezmiennik 23) — a katalog gospodarza i katalog kroku
        // to dwa różne katalogi, więc jadą jako dwie pary `--plugin-dir <ścieżka>`.
        let mut flags = from_the_host.to_vec();
        flags.extend_from_slice(of_the_step);

        match driver.inheriting(&flags) {
            Some(carrying) => Ok(carrying),
            // MATERIAŁ GOSPODARZA MA TYLKO TĘ JEDNĄ DROGĘ, więc vendor, który jej nie zna, nie
            // dostanie go w ogóle — i wtedy krok nie rusza. Cicha alternatywa daje bieg, w którym
            // człowiek zaznaczył umiejętności, agent nie dostał żadnej i nic tego nie mówi.
            None if !from_the_host.is_empty() => Err(anyhow::anyhow!(
                "this agent app cannot be handed the skills you brought in from this project. \
                 Loadout stopped the step instead of starting it without them: an agent that \
                 quietly knows less than you picked answers as though there was nothing to know."
            )),
            // UMIEJĘTNOŚCI TEGO KROKU MAJĄ DRUGĄ DROGĘ: sterownik bez flagi dostał je w katalogu
            // roboczym pod `.agents/skills/` ([`hand_the_skills_to_the_steps`]). Sterownik z flagą
            // wszedłby w `Some` wyżej, więc ten wariant znaczy dokładnie „półka już dojechała".
            None => Ok(Arc::clone(driver)),
        }
    }

    /// Sterownik tego kroku z **jego własnym plikiem ustawień** — albo odmowa.
    ///
    /// # Co ten plik naprawdę robi (zmierzone 2026-08-23, T-92)
    ///
    /// Niesie dwie rzeczy naraz, bo `--settings` wskazuje jeden dokument i drugiego nośnika
    /// po prostu nie ma:
    ///
    /// 1. **Auto-pamięć tego kroku wraca do biegu.** W `system/init` każdego kroku Claude'a
    ///    `memory_paths.auto` wskazywał `~/.claude/projects/<projekt>/memory/`, czyli katalog,
    ///    który człowiek dzieli ze swoimi sesjami interaktywnymi. Krok Loadouta pisał tam bez
    ///    pytania i bez śladu w biegu: nikt tego nie widział, nikt nie kurował, a zdanie napisane
    ///    przez agenta w cudzym biegu wracało potem do promptu człowieka jako jego własna
    ///    notatka. [T6 §10.4] nazywa przekierowanie tego katalogu per bieg „najlepszym leverem
    ///    znalezionym w researchu".
    /// 2. **Odmowy gospodarza zaczynają obowiązywać.** `--setting-sources ""` odcina
    ///    `.claude/settings.json` projektu w całości — razem z tym, co gospodarz naprawdę chciał
    ///    ZABRONIĆ. Wraca to wyłącznie jako tekst, przepisany przez [`super::super::engine::drivers::host::deny_rules`],
    ///    i wchodzi do tego samego pliku.
    ///
    /// **Per KROK, nie per bieg**, i klucz jest ten sam, którym bieg nazywa `work/<krok>`: dwa
    /// kroki jednego biegu bywają dwoma różnymi agentami, a jeden katalog pamięci na obu daje
    /// notatkę, o której nie wiadomo, czyja jest.
    ///
    /// # Odmowa czyta się z TYPU, nie z nazwy vendora (poprawione 2026-08-23, druga runda T-92)
    ///
    /// `Some(Err(…))` znaczy „ten sterownik plik bierze i nie udało się go napisać" — krok wtedy
    /// **nie rusza**, bo bez tego pliku pisze pamięć do katalogu człowieka i nie egzekwuje ani
    /// jednej odmowy gospodarza. `None` znaczy „ten vendor nie ma gdzie tego przyjąć" i krok
    /// rusza normalnie: tak odpowiada Codex i tak odpowiada każdy dubel silnika, który o tym
    /// szwie nic nie wie.
    ///
    /// Pierwsza runda miała tu `None if driver.id() == "claude"`, czyli to samo rozróżnienie
    /// zgadywane po etykiecie vendora. Zmierzone: odmawiało startu każdemu dublerowi podającemu
    /// się za `"claude"`, który tej metody nie implementuje — a `product_path_end_to_end`,
    /// wyrocznia całej drogi produktu, jest właśnie takim dublerem i poszła na czerwono przy
    /// sześciu zielonych kryteriach. Wyrocznia stoi poza blokiem `OWNS` tego zadania i nie ona
    /// była tu wadą.
    fn with_its_own_settings(
        &self,
        id: StepId,
        job: &AgentJob,
        driver: &Arc<dyn AgentDriver>,
    ) -> anyhow::Result<Arc<dyn AgentDriver>> {
        let wanted = crate::engine::drivers::StepSettings {
            dir: self.plan.dir.clone(),
            // Fizyczna praca, nie logiczny kafelek: kopie `s~2` mają wspólną pamięć kafelka,
            // ale są równoległymi procesami i nie mogą współdzielić stanu Claude'a [T-127].
            work_key: work_key_of(&self.plan.steps[id].node_key).to_owned(),
            memory: self.step_memory_dir(id),
            // Z KATALOGU ROBOCZEGO KROKU, nie z katalogu projektu biegu: krok pracujący we
            // własnej kopii plików ma tam swoją kopię `.claude/`, a odmowy czyta się z tego
            // repo, w którym agent naprawdę stoi.
            deny: crate::engine::drivers::host::deny_rules(&job.cwd),
        };
        match driver.with_settings(&wanted) {
            Some(Ok(carrying)) => {
                // Katalog zakłada TA warstwa, nie sterownik: układ katalogów biegu jest jej
                // wiedzą (`docs/ARCHITECTURE.md` §8), a sterownik, który wybiera sobie miejsce,
                // wybiera `$TMPDIR` — czyli artefakt biegu poza biegiem. I dopiero TERAZ, kiedy
                // wiadomo, że ktoś tam napisze: pusty `mem/<krok>` w biegu vendora, który tego
                // pliku nie umie wczytać, jest katalogiem bez ani jednego czytelnika
                // (niezmiennik 21).
                fs::create_dir_all(&wanted.memory)?;
                Ok(carrying)
            }
            // Powód od sterownika zostaje W ŁAŃCUCHU, pod zdaniem dla człowieka: „nie udało się
            // napisać pliku" bez ścieżki i bez błędu systemu jest odmową, której nikt nie naprawi.
            Some(Err(error)) => Err(error.context(
                "this agent app could not be given its own settings file, so Loadout did not \
                 start the step: without it the step writes what it learns into the folder you \
                 share with your own sessions, and forbids less than this project asked for.",
            )),
            None => Ok(Arc::clone(driver)),
        }
    }

    /// `<katalog biegu>/mem/<krok>` — dokąd ten krok pisze swoją auto-pamięć.
    ///
    /// Klucz KAFELKA, ten sam, którym nazywa się `work/<krok>`: rundy jednej pętli dzielą kafelek,
    /// więc dzielą też pamięć — i to jest właściwa odpowiedź, bo to dalej ten sam agent robiący
    /// tę samą robotę drugi raz.
    fn step_memory_dir(&self, id: StepId) -> PathBuf {
        self.plan
            .dir
            .join(STEP_MEMORY_DIR)
            .join(&self.plan.steps[id].tile_key)
    }

    /// Składa bezpieczny manifest dokładnie w kolejności, w której powstał finalny prompt.
    fn evidence_for_agent(
        &self,
        id: StepId,
        prompt_bytes: usize,
        context: Vec<ContextSource>,
        borrowed: &Inherited,
    ) -> EvidenceTarget {
        let mut inherited_context = borrowed
            .sources()
            .iter()
            .map(|source| ContextSource {
                kind: match source.kind {
                    InheritedSourceKind::Skill => ContextKind::InheritedSkill,
                    InheritedSourceKind::Learning => ContextKind::InheritedLearning,
                },
                reference: source.reference.clone(),
                bytes: source.bytes,
            })
            .collect::<Vec<_>>();
        inherited_context.extend(context);
        EvidenceTarget::workflow_step(
            self.plan.dir.clone(),
            self.plan.steps[id].id.clone(),
            SafeInputManifest {
                prompt_bytes,
                context: inherited_context,
                images: Vec::new(),
            },
        )
    }

    /// Krok, który nie ruszył: zdanie na ekran, kurator domknięty, zwykła droga porażki.
    ///
    /// Zdanie jedzie tą samą kolejką, którą mówi agent — bo dla człowieka patrzącego na bieg to
    /// jest to samo miejsce, w którym pojawiłaby się jego pierwsza linia.
    ///
    /// **Nadajnik ginie na obu końcach, i to jest warunek poprawności, nie higiena.** Na ścieżce
    /// startu zabiera `events` sam `start`; tutaj nie zabiera go nikt, a `pump.await` kończy się
    /// dopiero na zamkniętej kolejce. Nadawca, który przeżył krok, trzyma kurator otwarty — czyli
    /// odmowa wyglądałaby jak agent zawieszony na zawsze.
    ///
    /// Wynik rozstrzyga [`Live::when_this_one_fails`], czyli to samo miejsce, co przy każdej innej
    /// porażce: krok, który nie ruszył, jest krokiem, który nie przeszedł, a ustawienie „co, kiedy
    /// ten nie przejdzie" nie ma powodu znaczyć tu czegoś innego (niezmiennik 21).
    async fn never_started(
        self: &Arc<Self>,
        id: StepId,
        why: String,
        events: mpsc::Sender<DecodedEvent>,
        ours: mpsc::Sender<DecodedEvent>,
        pump: tokio::task::JoinHandle<()>,
    ) -> StepReport {
        let _ = ours
            .send(AgentEvent::Notice { text: why.clone() }.into())
            .await;
        drop(events);
        drop(ours);
        let _ = pump.await;
        self.when_this_one_fails(id, &why).await
    }

    /// Składa wszystkie nakładki sterownika w jedynej bezpiecznej kolejności przed startem.
    fn configured_driver_for_agent(
        &self,
        id: StepId,
        job: &AgentJob,
        target: EvidenceTarget,
        leftovers: &Arc<StepLeftovers>,
        service_connection: Option<&crate::connections::Connection>,
    ) -> anyhow::Result<Arc<dyn AgentDriver>> {
        let configuration = self.vendor_arguments_for(id, job, service_connection)?;
        /* STAWKI IDĄ PIERWSZE, przed każdą inną nakładką (2026-09, Z-44), i to jest ta sama
         * wymuszona kolejność, co niżej: każde z tych opakowań oddaje KLON sterownika, więc
         * tabela założona później zginęłaby przy pierwszym opakowaniu klonującym sterownik
         * sprzed niej. `None` nie odmawia startu — vendor, który cen nie liczy, nie ma czego
         * przyjąć, a atrapy nie mają tego szwu wcale. */
        let priced = job
            .driver
            .priced_from(&self.plan.prices)
            .unwrap_or_else(|| Arc::clone(&job.driver));
        let driver = if configuration.arguments.is_empty() {
            Arc::clone(&priced)
        } else {
            match priced.configured(&configuration) {
                Some(driver) => driver,
                /* Zatwierdzone polaczenie jest zgoda czlowieka wyrazona w imporcie, wiec
                 * krok, ktory ich nie dostanie, NIE RUSZA. Sam szczebel tej wagi nie ma:
                 * jest ustawieniem, nie zgoda, a stary dubel silnika bez tego szwu ma dalej
                 * dac sie uzyc do testowania planisty. */
                None if !job.connections.is_empty() || service_connection.is_some() => {
                    return Err(anyhow::anyhow!(
                        "this agent app cannot use the approved Connections. Loadout stopped the step instead of starting it without them."
                    ));
                }
                None => Arc::clone(&priced),
            }
        };
        let driver =
            Self::carrying_what_we_inherited(&driver, job.borrowed.flags(), &job.plugin_flags)?;
        /* 2026-08-22, przy scalaniu T-34 z T-75: KOLEJNOSC TYCH OPAKOWAN JEST
         * WYMUSZONA, nie dowolna. Kazde z nich oddaje KLON sterownika, wiec opakowanie
         * zalozone wczesniej ginie, jesli pozniejsze klonuje sterownik sprzed niego.
         * Connections ida pierwsze, bo `configured` startuje od `job.driver`; dziedziczenie
         * drugie; dowody ostatnie, bo tylko wtedy nadajnik dowodow siedzi na sterowniku,
         * ktory naprawde pojdzie do `start`. Odwrocenie tej kolejnosci jest niewidoczne:
         * wszystko sie kompiluje, bieg rusza, a znika albo `--mcp-config`, albo plik dowodu. */
        let driver = if self.plan.protection.contains_key(&id) {
            driver
        } else {
            self.with_its_own_settings(id, job, &driver)?
        };
        let driver = match driver.with_evidence(target) {
            Some(driver) => driver,
            /* Stare duble silnika nie znaja surowego drutu i pozostaja uzyteczne do
             * testowania planisty. Produkcyjna fabryka ma tylko te dwa identyfikatory;
             * dla nich brak szwu jest odmowa, nigdy cichym biegiem bez dowodu. */
            None if matches!(driver.id(), "claude" | "codex") => {
                return Err(anyhow::anyhow!(
                    "this agent app cannot preserve its private run evidence"
                ));
            }
            None => driver,
        };
        /* REJESTR OCALAŁYCH IDZIE OSTATNI, i to jest ta sama wymuszona kolejność, co wyżej: każde
         * z tych opakowań oddaje KLON sterownika, więc założone wcześniej ginie, gdy późniejsze
         * klonuje sterownik sprzed niego. Ten szew nie odmawia startu przy `None` — vendor bez
         * własnego procesu startowego nie ma czego zostawiać, a atrapy nie mają go wcale
         * (2026-09, Z-4). */
        let driver = driver
            .leaving_leftovers_with(Arc::clone(leftovers) as Arc<dyn KeepsLeftovers>)
            .unwrap_or(driver);
        /* ZNACZNIK IDZIE JAKO OSTATNI, i to jest ta sama wymuszona kolejność, co wyżej: każde
         * z tych opakowań oddaje KLON sterownika, więc znacznik założony wcześniej zginąłby przy
         * pierwszym następnym opakowaniu — cicho, bo wszystko dalej się kompiluje i bieg dalej
         * rusza (2026-09, Z-01d). `None` nie odmawia startu: vendor bez własnego procesu nie ma
         * czego znaczyć, a atrapy nie mają tego szwu wcale. */
        let tag = self.tag_for(id);
        let driver = driver.for_step(&tag).unwrap_or(driver);
        match self.plan.protection.get(&id) {
            Some(boundary) => boundary.agent(driver.as_ref()),
            None => Ok(driver),
        }
    }

    /// Konfiguruje sterownik i oddaje mu nadajnik dokładnie raz.
    ///
    /// Gdy konfiguracja odmawia, `events` spada razem z tą ramką przed powrotem błędu. Dzięki
    /// temu kurator nie zostaje otwarty na ścieżce, na której żaden sterownik nie przejął kanału.
    async fn start_agent_turn(
        &self,
        id: StepId,
        job: &AgentJob,
        turn: AgentTurn,
        leftovers: &Arc<StepLeftovers>,
        service_connection: Option<&crate::connections::Connection>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let driver =
            self.configured_driver_for_agent(id, job, turn.target, leftovers, service_connection)?;
        driver.start(turn.spec, turn.events).await
    }

    fn prepared_work_plan(
        &self,
        id: StepId,
        job: &AgentJob,
    ) -> Result<Option<crate::work_plan::Prepared>, String> {
        if job.work_plan.mode() == crate::work_plan::Mode::Off {
            return Ok(None);
        }
        if let Some(prepared) = self
            .plan_turns
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .and_then(Clone::clone)
        {
            return Ok(Some(prepared));
        }
        let human_requirements = job
            .criteria
            .iter()
            .map(|criterion| crate::work_plan::HumanRequirement {
                id: criterion.id.clone(),
                text: criterion.behaviour.clone(),
                acceptance: vec![crate::work_plan::AcceptanceCriterion {
                    text: criterion.behaviour.clone(),
                    verification: criterion.method.said().to_owned(),
                }],
            })
            .collect();
        let current = match job.work_plan.mode() {
            crate::work_plan::Mode::Update | crate::work_plan::Mode::Use => {
                Some(self.inherited_work_plan(id)?)
            }
            crate::work_plan::Mode::Create
            | crate::work_plan::Mode::Off
            | crate::work_plan::Mode::Unknown => None,
        };
        let prepared = job
            .work_plan
            .prepare_pinned(
                self.plan
                    .dir
                    .join("plans")
                    .join(crate::work_plan::DOCUMENT_ID),
                job.plan_candidate.clone(),
                human_requirements,
                current,
            )
            .map_err(|error| error.to_string())?;
        let mut turns = self
            .plan_turns
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(slot) = turns.get_mut(id) else {
            return Err("Loadout could not pin this step's plan version.".to_owned());
        };
        *slot = Some(prepared.clone());
        Ok(Some(prepared))
    }

    fn inherited_work_plan(&self, id: StepId) -> Result<crate::work_plan::PlanVersion, String> {
        let step = self
            .plan
            .steps
            .get(id)
            .ok_or_else(|| "Loadout could not follow this step's plan source.".to_owned())?;
        let sources = self
            .plan
            .work_plan_sources
            .sources
            .get(&step.node_key)
            .ok_or_else(|| {
                format!(
                    "Loadout did not start this step because {} has no resolved plan source.",
                    step.name
                )
            })?;
        let outputs = self
            .plan_outputs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let mut found = BTreeMap::new();
        for source in sources {
            let Some(at) = self
                .plan
                .steps
                .iter()
                .position(|candidate| candidate.node_key == *source)
            else {
                return Err("Loadout could not follow this step's plan source.".to_owned());
            };
            // 2026-09-08 (WP-02): `Same plan as` może wskazać Use w pętli. Późniejsze próby
            // bywają prawidłowo pominięte, więc bierzemy wyłącznie wynik próby, która ruszyła.
            if let Some(version) = outputs.get(at).and_then(Clone::clone) {
                found.insert(version.version_id.clone(), version);
            }
        }
        match found.len() {
            1 => found
                .into_values()
                .next()
                .ok_or_else(|| "Loadout could not pin this step's plan version.".to_owned()),
            0 => Err(format!(
                "Loadout did not start this step because the earlier work chosen for {} did not provide a plan.",
                step.name
            )),
            _ => Err(format!(
                "Loadout did not start this step because the earlier work chosen for {} used different plan versions.",
                step.name
            )),
        }
    }

    fn record_plan_output(
        &self,
        id: StepId,
        version: &crate::work_plan::PlanVersion,
    ) -> Result<(), String> {
        let mut outputs = self
            .plan_outputs
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(slot) = outputs.get_mut(id) else {
            return Err("Loadout could not remember this step's plan version.".to_owned());
        };
        *slot = Some(version.clone());
        drop(outputs);
        // 2026-09-08 — dokument zostaje wyłącznie w `plans/`; wynik kroku zapisuje jego adres,
        // żeby po restarcie było wiadomo, którą wersję ten konkretny Use naprawdę dostał.
        self.update(|book| {
            book.steps[id].plan_version = Some(WorkPlanReceipt::from(version));
        });
        Ok(())
    }

    /// Zwykły Step dostaje wyłącznie swój host usług. Nie jest Desk-em Leada.
    async fn service_bridge_for(
        &self,
        id: StepId,
        job: &AgentJob,
        expires: CancellationToken,
    ) -> anyhow::Result<Option<crate::bridge::host::Bridge>> {
        let prepared = self
            .plan_turns
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .and_then(Clone::clone);
        let has_plan = prepared
            .as_ref()
            .and_then(crate::work_plan::Prepared::current)
            .is_some();
        let step = &self.plan.steps[id];
        let context = match &self.plan.context_sources {
            Some(snapshot) => match (
                snapshot.access_for(&self.plan.dir, &step.node_key, expires.clone())?,
                snapshot.recorder_for(&self.plan.dir, &step.node_key)?,
            ) {
                (Some(access), Some(recorder)) => Some(
                    crate::bridge::context::ContextDesk::recording(access, recorder),
                ),
                (None, None) => None,
                _ => {
                    return Err(anyhow::anyhow!(
                        "The saved reference-material permissions do not match this step."
                    ));
                }
            },
            None => None,
        };
        if job.service_access.is_empty() && !job.agent_messages && !has_plan && context.is_none() {
            return Ok(None);
        }
        let services = (!job.service_access.is_empty()).then(|| {
            Arc::new(super::processes::services::ServiceAccess::for_step(
                Arc::clone(&self.processes),
                self.plan.project.clone(),
                self.plan.id.clone(),
                job.service_access.clone(),
                expires.clone(),
            ))
        });
        let messages = if job.agent_messages {
            Some(
                self.messages
                    .join(
                        step.node_key.clone(),
                        &step.name,
                        self.plan
                            .inputs
                            .configuration
                            .context_key(&step.tile_key)
                            .map(str::to_owned),
                        expires.clone(),
                    )
                    .map_err(anyhow::Error::msg)?,
            )
        } else {
            None
        };
        let plan = prepared.as_ref().and_then(|prepared| {
            prepared.current().map(|version| {
                crate::bridge::work_plan::PlanDesk::new(
                    format!("{}:{}", self.plan.id, step.node_key),
                    version,
                    expires.clone(),
                )
            })
        });
        let access = Arc::new(crate::bridge::messages::StepDesk {
            services,
            messages,
            context,
            plan,
        });
        Ok(Some(
            crate::bridge::host::Bridge::open_with_tools(
                &std::env::temp_dir(),
                access.tools(),
                access,
            )
            .await?,
        ))
    }

    /// Prompt TEJ tury i specyfikacja, ktora pojedzie do sterownika.
    ///
    /// Skladane teraz, a nie przy planowaniu, i to z dwoch powodow naraz: indeks przekazan ma
    /// co wymienic dopiero wtedy, gdy poprzednicy zeszli, a odziedziczony tekst dopisany raz
    /// w `AgentJob::prompt` roslby o kopie na kazda runde petli.
    ///
    /// `Err` niesie gotowe zdanie dla czlowieka; ksiega dostaje je zanim ta funkcja wroci.
    fn the_spec_this_turn_sends(
        &self,
        id: StepId,
        job: &AgentJob,
    ) -> Result<(RunSpec, Vec<String>, Vec<ContextSource>), String> {
        // Prompt składamy TERAZ, a nie przy planowaniu: indeks przekazań ma co wymienić dopiero
        // wtedy, gdy poprzednicy zeszli, a przy planowaniu nie ruszył jeszcze nikt.
        let Told {
            prompt,
            reads,
            context,
            extra_dirs,
        } = match self.prompt_for(id, &job.prompt, &job.context, job.minutes) {
            Ok(told) => told,
            Err(error) => {
                let text = if let Some(refusal) = error.downcast_ref::<WorkPlanRefused>() {
                    refusal.to_string()
                } else {
                    error
                        .downcast_ref::<HandoffChangedAfterPublication>()
                        .map_or_else(|| CONTEXT_NOT_PROVEN.to_owned(), ToString::to_string)
                };
                // 2026-08-25 (T-101) — TEN SAM POWÓD, ALE JEDNE DRZWI PORAŻKI. Zapis przed
                // `never_started` zachowuje dokładny tekst odmowy; wspólne domknięcie dopiero
                // potem pyta `whenItFails`, więc `carry-on` i `ask-me` nie są tu martwe.
                self.update(|book| book.steps[id].error = Some(text.clone()));
                return Err(text);
            }
        };
        // ODZIEDZICZONY TEKST DOPISUJE SIĘ TUTAJ, czyli tam, gdzie prompt tury naprawdę powstaje.
        // Doklejenie w `plan_agent` weszłoby do `AgentJob::prompt`, a ten idzie do `run.json`
        // i do każdej następnej rundy pętli — więc cudze reguły rosłyby o kopię na rundę.
        // `applied_to` rozstrzyga też, że `system_append` wraca nietknięty: treść w tym polu
        // staje się `--append-system-prompt`, czyli argumentem widocznym w `ps` (niezmiennik 9).
        let spec = job.borrowed.applied_to(RunSpec {
            run_id: job.session,
            cwd: job.cwd.clone(),
            // Instrukcja i indeks jadą jako DANE. Ta warstwa nie skleja komendy i nie zna ani
            // jednej flagi vendora (niezmiennik 9).
            prompt,
            model: job.model.clone(),
            system_append: job.system_append.clone(),
            policy: job.policy,
            /* Wybór AGENTA, przeniesiony bez interpretacji — tak samo jak `tools` niżej i z tego
             * samego powodu: krok, który liczyłby to sam, mógłby odpowiedzieć inaczej niż to,
             * co człowiek widzi w formularzu (niezmiennik 13). */
            reaches_the_web: job.reaches_the_web,
            // Lista z definicji agenta, przepuszczona przez sufit jego dialu **przy planowaniu**
            // (`what_this_step_may_use`). Tu jest już tylko przeniesieniem: krok, który liczyłby to
            // sam, mógłby odmówić w połowie biegu (niezmiennik 12).
            tools: job.tools.clone(),
            // Katalog przekazań, kiedy krok ma co czytać. Odnośnik do pliku, którego agentowi nie
            // wolno otworzyć, jest odnośnikiem bez handlera (niezmiennik 16).
            extra_dirs,
            resume: None,
        });
        Ok((spec, reads, context))
    }

    /// Most uslug tej tury i polaczenie, ktorym sterownik go dosiegnie.
    ///
    /// Oba powstaja PRZED startem sterownika: adres mostu podany po starcie opisywalby uslugi,
    /// ktorych w chwili pytania jeszcze nie ma.
    async fn the_service_bridge(
        &self,
        id: StepId,
        job: &AgentJob,
        expires: CancellationToken,
    ) -> Result<
        (
            Option<crate::bridge::host::Bridge>,
            Option<crate::connections::Connection>,
        ),
        String,
    > {
        let bridge = self
            .service_bridge_for(id, job, expires)
            .await
            .map_err(|why| why.to_string())?;
        let connection = bridge
            .as_ref()
            .map(super::super::bridge::host::Bridge::as_connection)
            .transpose()
            .map_err(|why| why.to_string())?;
        Ok((bridge, connection))
    }

    /// Krok, ktorego sterownik nie wystartowal: odmowa jedzie kolejka kuratora, a odbiornik gasnie.
    ///
    /// Domkniecie odbiornika jest tu na KAZDYM nieudanym starcie, nie tylko przy ocalalym: krok
    /// bez uchwytu nie ma juz czego powiedziec, a zatrzymany nadajnik wiesza `pump.await`.
    async fn the_start_that_broke(
        &self,
        id: StepId,
        job: &AgentJob,
        error: &anyhow::Error,
        ours: mpsc::Sender<DecodedEvent>,
        finish_forward: &CancellationToken,
        leftovers: &Arc<StepLeftovers>,
    ) -> Turned {
        let text = public_start_refusal(job.driver.id(), error);
        // `.into()` — `DecodedEvent::from(AgentEvent)` podstawia `tool: None`. Nieudany
        // start nie jest czynnością narzędzia, więc brak faktu jest tu prawdą, nie luką.
        let _ = ours
            .send(AgentEvent::Notice { text: text.clone() }.into())
            .await;
        drop(ours);
        /* ODBIORNIK KURATORA DOMYKAMY TU, NA KAŻDYM NIEUDANYM STARCIE (2026-09, Z-4).
         *
         * `drop(ours)` zdejmuje NASZ klon nadajnika, ale nie ten, który pojechał do
         * sterownika: `codex::start_app_conversation` wystawia czytnika (`app_server_actor`)
         * PRZED uzgodnieniem, więc uzgodnienie, które padło, zostawia zadanie trzymające
         * `events`. Porzucenie `JoinHandle` go nie przerywa, a przy `Alive` czytniki
         * zostają przy uchwycie z rozmysłu (`cleanup_after_proof` dołącza je wyłącznie po
         * `Dead`) — kolejka kuratora nie zamyka się więc nigdy i `pump.await` niżej nie
         * wraca. Bieg nie dochodzi do `settle()`, a folder odmawia każdego następnego
         * Startu aż do restartu Loadouta, czyli dokładnie ta wada, którą Z-4 zamyka.
         *
         * NA KAŻDYM `Err`, nie tylko przy ocalałym: krok bez uchwytu nie ma już czego
         * powiedzieć, a zatrzymany nadajnik jest wyciekiem niezależnie od tego, czy grupę
         * dało się dowieść. Zakolejkowana odmowa wyżej NIE ginie — `forward` zamyka
         * przyjmowanie i dopiero potem opróżnia to, co przyjęte. */
        finish_forward.cancel();
        self.the_start_that_never_happened(id, &text, leftovers.left_behind());
        Turned::Broke(text)
    }

    /// Krok agenta: sterownik, zdarzenia, linie, koniec albo anulowanie.
    async fn run_agent(
        self: &Arc<Self>,
        id: StepId,
        job: &AgentJob,
        cancel: &CancellationToken,
        slot: &mut Option<limits::Slot>,
    ) -> StepReport {
        let (events, inbox) = mpsc::channel::<DecodedEvent>(EVENT_QUEUE);
        // Osobny, jednokrotny kanał decyzji z pompy do właściciela `AgentHandle`. Pompa widzi
        // typowany `ToolEnd`, ale tylko `one_turn` ma prawo anulować i dowieść zejścia procesu.
        let (runtime_faults, mut runtime_fault) = mpsc::channel(1);
        // Sterownik gwarantuje dokładnie jedno `Finished`. Osobny sygnał po jego przetworzeniu
        // jest barierą FIFO: wynik procesu nie może wyprzedzić wcześniejszego `ToolEnd`, choć
        // kanał wyniku i kanał zdarzeń są od siebie niezależne.
        let (finished_events, mut finished_event) = mpsc::channel(1);
        // Osobny sygnał domknięcia odbiornika na wypadek, gdy żywa grupa przejdzie do rejestru
        // razem z uchwytem, który nadal trzyma klon nadajnika. Zamknięcie odbiornika zachowuje
        // już zakolejkowane zdarzenia, a nie czeka na upuszczenie zachowanego uchwytu.
        let finish_forward = CancellationToken::new();
        // Odbiór staje PRZED startem sterownika: vendor ma prawo powiedzieć pierwsze zdarzenia
        // jeszcze w `start`, a kanał bez odbiorcy zatrzymałby go na pierwszym pełnym buforze.
        // Limit dostawcy przychodzi właśnie tędy, więc pętla dostaje CAŁY bieg, nie same linie.
        let pump = tokio::spawn(forward(
            Arc::clone(self),
            inbox,
            self.plan.steps[id].name.clone(),
            id,
            runtime_faults,
            finished_events,
            finish_forward.clone(),
        ));
        // Własny klon nadawcy zostaje po to, żeby o nieudanym starcie dało się powiedzieć tą samą
        // drogą, którą mówi agent. Musi zginąć na OBU gałęziach — nadawca, który przeżył krok,
        // trzyma kurator otwarty i `pump.await` niżej nie wróciłby nigdy.
        let ours = events.clone();

        /* ZANIM RUSZY STEROWNIK: czy odpowiedź tego kroku ma dokąd pójść. Ścieżka wyprowadzająca
         * poza folder kroku jest odmową PRZED startem (niezmiennik 12), a dowiązania nie widać
         * z samego napisu, więc planista nie ma czego odrzucić — pytanie stoi więc tutaj, w
         * ostatniej chwili, w której nie ruszył jeszcze ani jeden proces. Krok wraca zwykłą
         * drogą porażki, bo to jest zwykła porażka: `run.json` dostaje zdanie nazywające pole,
         * a ustawienie „co, kiedy ten nie przejdzie" działa tu tak samo, jak wszędzie indziej. */
        if let Err(why) = Self::the_answer_has_somewhere_to_go(job) {
            return self.never_started(id, why, events, ours, pump).await;
        }

        // WF-13: fan-in/Check/ostatnia kopia usuwają wyłącznie własne, niezmienione pliki
        // dostawy. SameCopy i kolejne rundy odtwarzają wybrany pakiet przed procesem,
        // ze źródła zamrożonego przy Start, nigdy z dzisiejszej biblioteki/gospodarza.
        if let Err(why) = self.republish_native_skills(id, job) {
            return self.never_started(id, why, events, ours, pump).await;
        }

        let (spec, reads, context) = match self.the_spec_this_turn_sends(id, job) {
            Ok(told) => told,
            Err(text) => return self.never_started(id, text, events, ours, pump).await,
        };

        // Start **nie** ściga się z anulowaniem i to jest wybór, nie przeoczenie: żeby zejść po
        // grupie procesów, trzeba mieć uchwyt, a uchwyt wydaje dopiero `start`. Zdjęcie tego
        // `await` w połowie zostawiłoby proces, który właśnie wstał, bez nikogo, kto by o nim
        // wiedział — czyli dokładnie ten osierocony `claude` palący limit w tle, przed którym
        // stoją niezmienniki 6 i 10. Token widzi więc dopiero tura, i widzi go od środka.
        // Fragment argv od gospodarza dojeżdża do TEGO vendora albo krok nie rusza — trzeciej
        // możliwości nie ma i to jest cała treść tych czterech linii. Sterownik, który po cichu
        // zignorowałby przyniesioną ścieżkę katalogu pluginu, dałby bieg, w którym człowiek
        // zaznaczył umiejętności, agent nie dostał żadnej i nic tego nie mówi.
        let target = self.evidence_for_agent(id, spec.prompt.len(), context, &job.borrowed);
        let evidence = target.clone();
        // Upuszczenie samego nasłuchu nie zamyka już przyjętych połączeń. Prawa gasną
        // z tą turą także dla trzymanego starego gniazda, przed każdą drogą powrotu.
        let services_expire = cancel.child_token();
        let _services_expire = services_expire.clone().drop_guard();
        let (_service_bridge, service_connection) =
            match self.the_service_bridge(id, job, services_expire).await {
                Ok(both) => both,
                Err(why) => return self.never_started(id, why, events, ours, pump).await,
            };
        /* MIEJSCE KROKU JEDZIE DO STEROWNIKA NA CZAS STARTU (2026-09, Z-4). Nieudany start vendora
         * potrafi zostawić żywą grupę, a permit należy do kroku, nie do sterownika — więc gdyby
         * ocalały wjechał do rejestru bez niego, pula zwolniłaby miejsce po czymś, co dalej
         * biegnie. Kiedy sterownik niczego nie zostawił, miejsce wraca linijkę niżej i `one_turn`
         * dostaje je nietknięte. */
        let leftovers = Arc::new(StepLeftovers::holding(
            Arc::clone(&self.processes),
            slot.take(),
        ));
        let started = self
            .start_agent_turn(
                id,
                job,
                AgentTurn {
                    spec,
                    target,
                    events,
                },
                &leftovers,
                service_connection.as_ref(),
            )
            .await;
        *slot = leftovers.take_back();

        let turned = match started {
            Ok(handle) => {
                // Uchwyt znaczy, że proces wstał i prompt dojechał przez stdin. Zapisujemy
                // odbiorcę przed `wait`: późniejsza porażka tury nie cofa prawdziwej dostawy.
                self.update(|book| book.steps[id].execution.process_started = true);
                self.record_memory_for_started_step(id, &job.memory);
                drop(ours);
                self.one_turn(
                    handle,
                    LiveAgentTurn {
                        id,
                        cancel,
                        runtime_fault: &mut runtime_fault,
                        finished_event: &mut finished_event,
                        finish_forward: &finish_forward,
                        reads: &reads,
                        evidence: &evidence,
                        slot,
                    },
                )
                .await
            }
            Err(error) => {
                self.the_start_that_broke(id, job, &error, ours, &finish_forward, &leftovers)
                    .await
            }
        };

        // Czekamy na kurator, zanim krok wróci: linie kroku muszą wyjść, ZANIM planista wypuści
        // następny. Bez tego strzałka „po" przestaje znaczyć „po" na ekranie, choć w silniku
        // dalej znaczy.
        let _ = pump.await;
        match turned {
            Turned::Settled(report) => report,
            /* DOPIERO TERAZ, bo dopiero teraz wiadomo, co ten krok zdążył powiedzieć: proza
             * agenta jedzie tą samą kolejką, którą właśnie domknęliśmy, a to ona jest ciałem
             * przekazania, jakie zostawia po sobie krok jadący dalej mimo porażki (T-87 AC-5). */
            Turned::Broke(why) => self.when_this_one_fails(id, &why).await,
        }
    }

    /// Zapisuje krok, którego sterownik nie wystartował — razem z tym, co ten start zostawił.
    ///
    /// 2026-08-28 (T-152): bez uchwytu nie ma prozy agenta, więc publiczna odmowa jest jedyną
    /// treścią, którą `carry-on` może uczciwie przekazać potomkowi.
    ///
    /// 2026-09 (Z-4) — NIEUDANY START POTRAFI ZOSTAWIĆ ŻYWĄ GRUPĘ. Sterownik oddał ją do rejestru
    /// razem z miejscem tego kroku ([`StepLeftovers`]), ale człowiek dowiaduje się o niej wyłącznie
    /// z księgi — a do tego dnia ta droga nie zapisywała ani adresu, ani zdania, więc ocalały po
    /// nieudanym starcie był niewidzialny (niezmiennik 29).
    ///
    /// Osobna metoda, a nie ramię `match`, bo `run_agent` stoi pod sufitem stu linii.
    fn the_start_that_never_happened(&self, id: StepId, text: &str, survivor: Option<GroupId>) {
        self.update(|book| {
            let step = &mut book.steps[id];
            step.summary = Some(text.to_owned());
            /* Zachowujemy dokładną publiczną przyczynę przed wspólną polityką porażki.
             * `when_this_one_fails` używa `get_or_insert`, więc dopisek o `carry-on` albo pytaniu
             * nie może zastąpić faktu, dlaczego proces nie wystartował. */
            step.error = Some(text.to_owned());
            let Some(group) = survivor else {
                return;
            };
            // Adres jest jedyną rzeczą, po której człowiek znajdzie tę grupę w `ps`, a odzyskiwanie
            // przy następnym starcie po niej sprząta [T7 §6.2].
            step.pid = Some(group.pid);
            step.pgid = Some(group.pgid);
            step.death_proof = false;
            /* NASZE ZDANIE WYGRYWA Z POWODEM ODMOWY, tym samym idiomem, którym
             * `Ended::RepeatedToolFailure` przykrywa powód agenta: „nie wystartował" zostaje
             * w `summary`, a `error` mówi o tym, co MOŻE JESZCZE BIEC i palić limit u dostawcy.
             * Z dwóch zdań tylko to drugie każe komuś sprawdzić maszynę. To samo zdanie, co na
             * każdej innej drodze z `death_proof: false` poza żywym Stopem — powód przy
             * `Ended::Turn(Err)`. */
            step.error = Some(STEP_SURVIVOR_ERROR.to_owned());
        });
    }

    fn record_memory_for_started_step(&self, id: StepId, memory: &[MemoryDisposition]) {
        let step_id = self.plan.steps[id].id.clone();
        self.update(|book| {
            for disposition in memory {
                let Some(record) = book
                    .memory
                    .iter_mut()
                    .find(|record| record.address == disposition.address)
                else {
                    continue;
                };
                // Jedna notatka i jeden fizyczny krok należą dokładnie do jednej listy.
                record.recipients.retain(|recipient| recipient != &step_id);
                record
                    .left_out_for
                    .retain(|recipient| recipient != &step_id);
                let recipients = if disposition.delivered {
                    &mut record.recipients
                } else {
                    &mut record.left_out_for
                };
                recipients.push(step_id.clone());
                recipients.sort();
                recipients.dedup();
            }
        });
    }

    /// Krok „sprawdź": nasza komenda, nasz werdykt, zero sesji agenta.
    ///
    /// Ta funkcja nie tworzy `RunSpec`, nie pyta fabryki [`super::Drivers`] o sterownik i nie ma
    /// jak zapłacić za turę u vendora — i to jest jej treść, nie pominięcie (AC-4). Implementacja
    /// routująca ten krok przez [`plan_agent`] przewróciłaby się na `RunError::NoAgentsSaved`
    /// w repo, w którym nikt nie zapisał ani jednego agenta, a nie ma powodu, żeby taki krok
    /// jakiegokolwiek agenta potrzebował.
    /// Fakty producentów są danymi. Ani ich output, ani identyfikator nie zmienia komendy.
    fn republish_native_skills(&self, id: StepId, job: &AgentJob) -> Result<(), String> {
        if job.driver.inheriting(&[]).is_some()
            || (job.skills.names.is_empty() && job.borrows.skills.is_empty())
        {
            return Ok(());
        }
        self.copy_processes_stopped(&job.cwd)?;
        let _guard = self.processes.try_finalize_copy(&job.cwd).map_err(|error| error.to_string())?
            .ok_or_else(|| "A background process is still using this working folder. Its skill files were left untouched.".to_owned())?;
        let step = &self.plan.steps[id];
        let mut plugins = Vec::new();
        if !job.skills.names.is_empty() {
            plugins.push(self.plan.dir.join(STEP_SKILLS_DIR).join(&step.node_key));
        }
        if !job.borrows.skills.is_empty() {
            plugins.push(
                self.plan
                    .dir
                    .join(BORROWED_DIR)
                    .join(&step.node_key)
                    .join("plugin"),
            );
        }
        for plugin in plugins {
            let skills = saved_native_skills(&plugin).map_err(|error| error.to_string())?;
            hand_native_skills(
                &self.plan.dir,
                &job.cwd,
                &skills,
                &step.name,
                &step.tile_key,
            )
            .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn finalizable_working_folder(
        &self,
        id: StepId,
    ) -> io::Result<Option<(PathBuf, crate::commands::processes::CopyFinalizationGuard)>> {
        let proved = {
            let book = self.book();
            !book.steps[id].execution.process_started || book.steps[id].death_proof
        };
        if !proved {
            return Err(io::Error::other(
                "The producing agent is not confirmed stopped.",
            ));
        }
        let Some(cwd) = where_the_job_works(&self.plan.steps[id].job) else {
            return Ok(None);
        };
        let cwd = cwd.to_path_buf();
        let guard = self
            .processes
            .try_finalize_copy(&cwd)?
            .ok_or_else(|| io::Error::other("A background app is still using these files."))?;
        self.copy_processes_stopped(&cwd)
            .map_err(io::Error::other)?;
        Ok(Some((cwd, guard)))
    }

    fn finish_work_plan(&self, id: StepId) -> Result<(), String> {
        let Job::Agent(job) = &self.plan.steps[id].job else {
            return Ok(());
        };
        if job.work_plan.mode() == crate::work_plan::Mode::Off {
            return Ok(());
        }
        let prepared = self
            .plan_turns
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .and_then(Clone::clone)
            .ok_or_else(|| {
                crate::work_plan::Error::NotDelivered(
                    "Loadout did not pin the plan parent for this try".to_owned(),
                )
                .to_string()
            })?;
        if job.work_plan.mode() == crate::work_plan::Mode::Use {
            let current = prepared.current().cloned().ok_or_else(|| {
                crate::work_plan::Error::NotDelivered(
                    "Loadout did not keep the plan version pinned to this step".to_owned(),
                )
                .to_string()
            })?;
            return self.record_plan_output(id, &current);
        }
        let Some((cwd, _guard)) = self.finalizable_working_folder(id).map_err(|error| {
            crate::work_plan::Error::NotDelivered(error.to_string()).to_string()
        })?
        else {
            return Err(crate::work_plan::Error::NotDelivered(
                "this step has no working folder".to_owned(),
            )
            .to_string());
        };
        let bytes = plan_candidate_bytes(&cwd, &prepared).map_err(|error| error.to_string())?;
        let step = &self.plan.steps[id];
        let stamp = crate::work_plan::Stamp {
            run_id: self.plan.id.clone(),
            document_id: crate::work_plan::DOCUMENT_ID.to_owned(),
            step_id: step.id.clone(),
            attempt: u32::from(step.turn).saturating_add(1),
            operation: format!("{}:{}:{}", self.plan.id, step.id, step.turn),
            parent: prepared.current().map(|version| version.version_id.clone()),
            at: crate::commands::now_utc(),
        };
        let publication = prepared
            .publish(&stamp, &bytes)
            .map_err(|error| error.to_string())?;
        let version = match publication {
            crate::work_plan::Publication::Published(version)
            | crate::work_plan::Publication::Unchanged(version)
            | crate::work_plan::Publication::AlreadyPublished(version) => version,
        };
        self.record_plan_output(id, &version)?;
        let path = cwd.join(prepared.candidate());
        // 2026-09-08 — kandydat nie jest historią planu. Po publikacji czyta go już tylko
        // sprzątanie; błąd kasowania nie może cofnąć atomowo opublikowanej wersji.
        if let Err(error) = fs::remove_file(&path) {
            tracing::debug!(%error, "the published plan candidate was left in the working folder");
        }
        Ok(())
    }

    /// 2026-09-08 — tylko podgląd do korekty; publikacja nadal czeka na dowód śmierci procesu.
    fn plan_candidate_problem_during_session(&self, id: StepId, job: &AgentJob) -> Option<String> {
        if !job.work_plan.writes_candidate() {
            return None;
        }
        let prepared = self
            .plan_turns
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .and_then(Clone::clone)?;
        let path = job.cwd.join(prepared.candidate());
        if matches!(
            fs::symlink_metadata(&path),
            Err(ref error) if error.kind() == io::ErrorKind::NotFound
        ) {
            return None;
        }
        plan_candidate_bytes(&job.cwd, &prepared)
            .and_then(|bytes| prepared.validate(&bytes))
            .err()
            .map(|error| error.to_string())
    }

    fn freeze_result_files(
        &self,
        id: StepId,
    ) -> io::Result<Option<super::run_inputs::ResultFiles>> {
        let Some((cwd, _guard)) = self.finalizable_working_folder(id)? else {
            return Ok(None);
        };
        change_native_skills(&self.plan.dir, &cwd, None)?;
        let relative = PathBuf::from("check-inputs").join(&self.plan.steps[id].node_key);
        supervisor::PublicationRoot::open(&self.plan.dir)?.ensure_directory(&relative, 0o700)?;
        // WF-16 (2026-09-06): Project/Pick nie są własnością biegu. Zamrożenie wyniku
        // nie daje nowej zgody na import prywatnego .env gospodarza. Własna kopia ma już
        // rozstrzygnięte wejścia WF-14 i zachowuje także pliki utworzone później przez agenta.
        let snapshot = if cwd.starts_with(self.plan.dir.join(WORK_DIR)) {
            super::input_snapshot::capture_saved_result(&cwd, &self.plan.dir.join(&relative))?
        } else {
            super::input_snapshot::capture_selected(
                &cwd,
                &self.plan.dir.join(&relative),
                &self.plan.additional_inputs,
            )?
        };
        Ok(Some(super::run_inputs::ResultFiles {
            snapshot: snapshot.id().to_owned(),
            directory: relative.to_string_lossy().into_owned(),
        }))
    }

    fn input_for_check(&self, id: StepId) -> io::Result<Option<String>> {
        let Some(selected) = self.plan.inputs.for_check(id) else {
            return Ok(None);
        };
        let mut results = Vec::with_capacity(selected.len());
        let mut bytes = 0_usize;
        for &producer in selected {
            let (status, error, cause, files) = {
                // Mutex nigdy nie przechodzi przez odczyt pliku ani await (niezmiennik 8).
                let book = self.book.lock().unwrap_or_else(PoisonError::into_inner);
                let step = &book.steps[producer];
                if step
                    .error
                    .as_ref()
                    .is_some_and(|error| error.len() > super::run_inputs::MAX_CHECK_INPUT_BYTES)
                {
                    return Err(io::Error::other(
                        "The selected check input exceeds 256 KiB.",
                    ));
                }
                (
                    step.status.name().to_owned(),
                    step.error.clone(),
                    step.end_cause
                        .unwrap_or(super::run_inputs::EndCause::Unknown),
                    step.result_files.clone(),
                )
            };
            let written = self
                .handoffs
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(producer)
                .cloned()
                .flatten();
            let output = written
                .as_ref()
                .map(|written| {
                    super::run_inputs::read_output(&self.plan.dir, &self.plan.id, producer, written)
                })
                .transpose()?;
            bytes = bytes
                .saturating_add(output.as_ref().map_or(0, String::len))
                .saturating_add(error.as_ref().map_or(0, String::len));
            if bytes > super::run_inputs::MAX_CHECK_INPUT_BYTES {
                return Err(io::Error::other(
                    "The selected check input exceeds 256 KiB.",
                ));
            }
            let files = files
                .map(|record| -> io::Result<Value> {
                    let snapshot =
                        super::input_snapshot::read(&self.plan.dir.join(&record.directory))?;
                    if snapshot.id() != record.snapshot {
                        return Err(io::Error::other(
                            "The selected result files belong to another publication.",
                        ));
                    }
                    Ok(serde_json::json!({"root":snapshot.files(),"snapshotId":snapshot.id()}))
                })
                .transpose()?;
            results.push(super::run_inputs::InputResult {
                node_key: self.plan.steps[producer].node_key.clone(),
                status,
                output,
                error,
                cause,
                files,
            });
        }
        super::run_inputs::encode(&results).map(Some)
    }

    /// Uniewaznia wynik sprawdzenia, ktorego dane egzaminacyjne zmienily sie w trakcie.
    ///
    /// Nie „nie przeszlo", tylko „nie zmierzono": werdykt nad danymi, ktorych nikt nie widzial
    /// w komplecie, wysylalby czlowieka naprawiac produkt zamiast fikstury.
    fn refuse_if_the_data_moved(
        &self,
        id: StepId,
        boundary: Option<&protection::Boundary>,
        saved_input: Option<&str>,
        report: &mut crate::engine::drivers::command::CheckReport,
    ) {
        if let Some(boundary) = boundary {
            let unchanged = boundary
                .validate()
                .and_then(|()| self.input_for_check(id))
                .is_ok_and(|now| now.as_deref() == saved_input);
            if !unchanged {
                report.passed = false;
                report.assessment = Some(crate::engine::drivers::command::assessment::Assessment {
                    outcome: crate::engine::drivers::command::assessment::Outcome::NotJudged,
                    receipt: None,
                    reason: "The evaluation data changed. This result was not measured.".to_owned(),
                });
            }
        }
    }

    /// Zapisuje wynik sprawdzenia do ksiegi — z powodem, ktory czlowiek naprawde przeczyta.
    ///
    /// Zdanie o wymaganych testach wygrywa z powodem egzaminatora, bo odpowiada na wczesniejsze
    /// pytanie: wada bywa w komendzie albo w katalogu, z ktorego ja uruchomiono, a nie w produkcie.
    fn record_the_check(
        &self,
        id: StepId,
        report: &crate::engine::drivers::command::CheckReport,
        proven_dead: bool,
    ) {
        self.update(|book| {
            let step = &mut book.steps[id];
            step.exit_code = report.exit_code;
            step.summary = summary_of(&report.output);
            step.assessment.clone_from(&report.assessment);
            step.death_proof = proven_dead;
            if !proven_dead {
                step.error = Some(CHECK_SURVIVOR_ERROR.to_owned());
            }
            step.end_cause = Some(if !proven_dead {
                super::run_inputs::EndCause::UnprovenStop
            } else if report.assessment.as_ref().is_some_and(|one| {
                one.outcome == crate::engine::drivers::command::assessment::Outcome::NotJudged
            }) {
                super::run_inputs::EndCause::InfrastructureFailed
            } else if report.passed {
                super::run_inputs::EndCause::Completed
            } else {
                super::run_inputs::EndCause::TaskFailed
            });
            if proven_dead
                && let Some(assessment) = &report.assessment
                && !report.passed
            {
                step.error = Some(assessment.reason.clone());
            }
            /* V-01: ZDANIE O WYMAGANYCH TESTACH WYGRYWA Z POWODEM EGZAMINATORA, bo
             * odpowiada na wcześniejsze pytanie. „Sprawdzenie nie przeszło" nad suitą,
             * która wykonała dwa niezwiązane testy zamiast trzynastu wymaganych, wysyła
             * człowieka szukać wady w produkcie — a wada jest w komendzie albo
             * w katalogu, z którego ją uruchomiono (incydenty I-03/I-04). */
            if proven_dead
                && let Some(missing) = report.required.as_ref().filter(|one| {
                    !crate::engine::drivers::command::RequiredTests::all_confirmed(one)
                })
            {
                step.error = Some(missing.said());
            }
        });
    }

    /// Trzy wyniki, nie dwa — bo sprawdzenie, ktore nie mialo czego zmierzyc, jest osobnym stanem.
    ///
    /// Zewnetrzny egzaminator, ktory nic nie orzekl, i wymagane testy, ktore sie nie wykonaly,
    /// to nie jest ani zaliczenie, ani wada produktu. Nazwane `Failed` wysylalyby czlowieka
    /// naprawiac cos, czego nikt nie zmierzyl.
    fn the_check_outcome(report: &crate::engine::drivers::command::CheckReport) -> CheckOutcome {
        let not_measured = report.assessment.as_ref().is_some_and(|one| {
            one.outcome == crate::engine::drivers::command::assessment::Outcome::NotJudged
        }) || report
            .required
            .as_ref()
            .is_some_and(|one| !one.did_not_run.is_empty() && one.did_not_pass.is_empty());
        /* V-02: TRZY WYNIKI, NIE DWA. Sprawdzenie, które nie miało czego
         * zmierzyć — bo zewnętrzny egzaminator nic nie orzekł albo bo wymagane
         * testy w ogóle się nie wykonały — nie jest ani zaliczeniem, ani wadą
         * produktu. Nazwane `Failed` wysyłałoby człowieka naprawiać coś,
         * czego nikt nie zmierzył. */
        if not_measured {
            CheckOutcome::NotJudged
        } else if report.passed {
            CheckOutcome::Passed
        } else {
            CheckOutcome::Failed
        }
    }

    async fn run_check(
        &self,
        id: StepId,
        job: &CheckJob,
        cancel: &CancellationToken,
        slot: &mut Option<limits::Slot>,
    ) -> StepReport {
        // ZE ZNACZNIKIEM (2026-09, Z-01d): komenda sprawdzająca to najczęściej `npm test` albo
        // `cargo test`, czyli dokładnie ta rzecz, która potrafi zostawić po sobie serwer w tle.
        let driver = CommandDriver::new()
            .for_step(self.tag_for(id))
            .with_proof_mode(job.proof_mode);
        // START I CZEKANIE OSOBNO, a nie jednym `CommandDriver::run`, i to jest cała różnica
        // między księgą, która pomaga po awarii, a księgą, która opisuje przeszłość: `run` to
        // `start` plus `settle().await`, więc wraca dopiero PO całym sprawdzeniu — a `pid`
        // i `pgid` zapisane wtedy są nieobecne przez cały czas, w którym komenda naprawdę biegła.
        let input = match self.input_for_check(id) {
            Ok(input) => input,
            Err(error) => return self.refuse_step(id, &error.to_string()),
        };
        let boundary = self.plan.protection.get(&id);
        let saved_input = boundary.and(input.clone());
        let mut spec = job.spec.clone();
        let driver = if let Some(boundary) = boundary {
            spec.cwd = boundary.check_cwd(&spec.cwd).to_path_buf();
            match boundary.check(driver, input.as_deref()) {
                Ok(driver) => driver,
                Err(error) => return self.refuse_step(id, &error.to_string()),
            }
        } else {
            driver
        };
        let mut live = match driver.start_with_input(&spec, input) {
            Ok(live) => live,
            Err(error) => {
                // Zdanie nazywa KOMENDĘ, bo to ona się nie uruchomiła. „Nie udało się" bez
                // podmiotu wysyła człowieka szukać wady w agencie, którego tu nie ma.
                let text = format!("Loadout could not start this check: {error}");
                self.update(|book| {
                    book.steps[id].error = Some(text);
                    book.steps[id].end_cause =
                        Some(super::run_inputs::EndCause::InfrastructureFailed);
                });
                /* 2026-08-23 (T-87) — I TA DROGA TEZ IDZIE PRZEZ JEDNO MIEJSCE. Do tego dnia
                 * wracalo stad gole `StepReport::Failed`, wiec ustawienie czlowieka „jedz dalej
                 * mimo wszystko" bylo na tej sciezce martwe: literowka w nazwie katalogu zabierala
                 * ze soba caly stozek za krokiem, cicho i bez wyboru. Powod jest juz w ksiedze
                 * i mowi wiecej, wiec `get_or_insert` tam go nie tknie. */
                return self
                    .when_this_one_fails(id, "This check could not be started.")
                    .await;
            }
        };

        self.update(|book| book.steps[id].execution.process_started = true);

        // `pid` i `pgid` do księgi, ZANIM cokolwiek popłynie z wyjścia — dokładnie jak przy
        // agencie (`one_turn`): po awarii aplikacji nie ma już kogo o nie zapytać, a to po nich
        // sprząta odzyskiwanie [T7 §6.2]. `Checking::group` jest zwykłą wartością, dostępną
        // synchronicznie zaraz po starcie, więc ten zapis nie czeka na nic.
        let group = live.group();
        self.update(|book| {
            let step = &mut book.steps[id];
            step.pid = Some(group.pid);
            step.pgid = Some(group.pgid);
        });

        let end = live.settle(cancel).await;

        match end.how {
            CheckHow::Ran(mut report) => {
                /* 2026-08-28 — WERDYKT DOPIERO PO DOWODZIE ZEJŚCIA GRUPY.
                 *
                 * `Checking::settle` na ścieżce `Settled::Exited` nie wołało `stop()` ani razu:
                 * kod wyjścia komendy mówi o LIDERZE, a zapłacone są wnuki [T7 §3.1]. Zielone
                 * „passed" nad grupą, która dalej odpowiada na sygnał zerowy, jest tym samym
                 * `Ok(())`, przed którym stoi `GroupProof` (niezmiennik 6) — tylko o warstwę
                 * obok kroku agenta. */
                let proof = self.prove_check_settled(id, &mut live).await;
                let proven_dead = matches!(&proof, GroupProof::Dead { .. });
                self.refuse_if_the_data_moved(id, boundary, saved_input.as_deref(), &mut report);
                self.record_the_check(id, &report, proven_dead);
                // Ocalały jedzie do rejestru PRZED przekazaniem i przed werdyktem: za nimi stoi
                // `when_this_one_fails`, które przy ustawieniu „zapytaj mnie" czeka na człowieka —
                // a przez cały ten czas pula kłamałaby o wolnym miejscu (2026-09, Z-4).
                self.check_released_by(live, slot, &proof);
                /* WYJŚCIE KOMENDY MA DWÓCH CZYTELNIKÓW (niezmiennik 21): werdykt wyżej
                 * i przekazanie do następnego kroku tutaj. 2026-08-31 — przekazywany tekst jest
                 * ogonem do 64 KiB i zaczyna się zdaniem o pominięciu, gdy pełny strumień był
                 * dłuższy; werdykt już zobaczył cały strumień. Bez drugiego czytelnika runda 1
                 * pętli nie wie, co padło w rundzie 0, i pętla nie ma po co istnieć. `reads` jest
                 * puste, bo do komendy nie wstrzykujemy niczyjego przekazania — komenda nie czyta
                 * promptu. */
                self.hand_over(id, &report.output, &[]);
                /* PRZED WERDYKTEM I PRZED DROGAMI WARUNKOWYMI, ale ZA zapisem przekazania: plik
                 * z wyjściem komendy ma istnieć niezależnie od tego, co postanowimy z biegiem
                 * (ta sama kolejność, co niżej). Ocalały nie ma prawa wyjść tędy `Succeeded`,
                 * bo tylko po tym stanie planista wypuszcza potomków. */
                if !proven_dead {
                    return self
                        .when_this_one_fails(id, "Something this check started is still running.")
                        .await;
                }
                if self.has_routes(id) {
                    self.remember_evidence(
                        id,
                        RouteEvidence::Check(Self::the_check_outcome(&report)),
                    );
                    return StepReport::Succeeded;
                }
                /* WERDYKT PO ZAPISIE PRZEKAZANIA, nie przed: plik z wyjściem komendy ma istnieć
                 * niezależnie od tego, co postanowimy z biegiem — ta sama kolejność, którą trzyma
                 * krok agenta.
                 *
                 * Jeden warunek, nie dwa. `verdict_of_a_check(…) || !report.passed` czytało się
                 * niewinnie i kasowało pętlę: sędzia, który nie przepuścił, ale ma jeszcze próbę,
                 * musi wrócić `Succeeded`, bo tylko po tym stanie planista wypuszcza jego dzieci —
                 * a jego dzieckiem jest powrót do roboty. Cała różnica między „sprawdzenie padło"
                 * i „runda padła, próbujemy dalej" mieszka więc w tamtej funkcji i nigdzie
                 * indziej. */
                if self.verdict_of_a_check(id, report.passed) {
                    self.when_this_one_fails(id, "The checks it runs did not pass.")
                        .await
                } else {
                    StepReport::Succeeded
                }
            }
            // Anulowanie jest WARTOŚCIĄ, nie błędem (niezmiennik 7), a dowód zejścia grupy
            // przyszedł już w `how` — to sterownik go zdobył, nie my.
            CheckHow::Stopped(first_proof) => {
                self.stop_cancelled_check(id, live, slot, first_proof).await
            }
            CheckHow::Overdue(first_proof) => {
                self.stop_overdue_check(id, live, slot, first_proof).await
            }
        }
    }

    /// Kończy komendę, która przekroczyła własny limit czasu, i utrwala jej rzeczywisty dowód.
    ///
    /// Osobna metoda, a nie ramię `match`, z tego samego powodu co bliźniak niżej: `run_check`
    /// stoi pod sufitem stu linii, a oddanie ocalałego dołożyło każdemu ramieniu po parę.
    async fn stop_overdue_check(
        &self,
        id: StepId,
        mut live: Checking,
        slot: &mut Option<limits::Slot>,
        first_proof: GroupProof,
    ) -> StepReport {
        let proof = self.prove_check_dead(&mut live, first_proof).await;
        // PRZED ZAPISEM DOWODU (2026-09, Z-01d) — powód przy `Live::note_pgids`.
        self.note_pgids(id, &live.descendant_groups());
        let unproven = matches!(&proof, GroupProof::Alive { .. });
        let proven_dead = matches!(&proof, GroupProof::Dead { .. });
        self.update(|book| {
            // Powód nazywa LIMIT CZASU i mówi, co zrobić. Liczba minut przychodzi ZE STAŁEJ,
            // a nie z tego zdania: dwa miejsca, w których mieszka jedna liczba, rozjeżdżają się
            // przy pierwszej zmianie i to zdanie zostaje tym nieaktualnym.
            let minutes = GIVE_UP_AFTER.as_secs() / 60;
            let step = &mut book.steps[id];
            step.death_proof = proven_dead;
            step.error = Some(if unproven {
                format!(
                    "This check ran longer than {minutes} minutes, and Loadout could not make \
                     sure it stopped, so it may still be running."
                )
            } else {
                format!(
                    "This check ran longer than {minutes} minutes, so Loadout stopped it. Split \
                     the work, or run fewer things in one step."
                )
            });
        });
        self.check_released_by(live, slot, &proof);
        // Ta sama droga, co kazda inna porazka (T-87 AC-5): krok, ktory nie zdazyl, tez byl
        // slepym punktem — jedna wolna komenda konczyla caly bieg.
        self.when_this_one_fails(id, "This check ran out of time.")
            .await
    }

    /// Kończy komendę zatrzymaną przez człowieka i utrwala jej rzeczywisty dowód.
    ///
    /// Bliźniak [`Live::stop_cancelled_agent`] po stronie kroku „sprawdź" i z tym samym
    /// rozstrzygnięciem: ocalały NIE jest anulowaniem. Osobna metoda, a nie ramię `match`, bo
    /// `run_check` stoi pod sufitem stu linii, a to jest ten sam podział, który po stronie agenta
    /// istnieje od dawna.
    async fn stop_cancelled_check(
        &self,
        id: StepId,
        mut live: Checking,
        slot: &mut Option<limits::Slot>,
        first_proof: GroupProof,
    ) -> StepReport {
        let proof = self.prove_check_dead(&mut live, first_proof).await;
        // PRZED ZAPISEM DOWODU (2026-09, Z-01d) — powód przy `Live::note_pgids`.
        self.note_pgids(id, &live.descendant_groups());
        let unproven = matches!(&proof, GroupProof::Alive { .. });
        let proven_dead = matches!(&proof, GroupProof::Dead { .. });
        self.update(|book| {
            let step = &mut book.steps[id];
            step.death_proof = proven_dead;
            if unproven {
                step.error = Some(
                    "Loadout could not make sure this check stopped, so it may still be running."
                        .to_owned(),
                );
            }
        });
        self.check_released_by(live, slot, &proof);
        if unproven {
            // 2026-09 (Z-4), to samo zdanie, co przy agencie po żywym Stopie: Stop całego biegu
            // zostaje anulowaniem, ale TEN krok nie może nazywać się anulowanym — jego grupa
            // przeżyła pełną eskalację, a potomków wypuszcza wyłącznie stan udany.
            StepReport::Failed
        } else {
            StepReport::Cancelled
        }
    }

    /// Zachowuje uchwyt komendy sprawdzającej po pierwszym niepełnym dowodzie Stopu —
    /// **przez skończoną liczbę pełnych eskalacji**, nie w nieskończoność.
    ///
    /// 2026-09 (Z-4) — do tego dnia ta pętla kręciła się aż do `ESRCH` i była drugą drogą, na
    /// której bieg nie schodził NIGDY: `settle()` nie zapadało, Stop nie wracał, a zapadka folderu
    /// odmawiała każdego następnego Startu aż do restartu Loadouta. Sufit jest ten sam, co przy
    /// każdej innej eskalacji tego pliku, bo polityka trzech prób jest jedna (niezmiennik 23).
    ///
    /// **Ocalałego nie ma tu komu oddać**, i to jest różnica wobec kroku agenta: uchwyt komendy
    /// nie jest `Box<dyn AgentHandle>` i nie wchodzi do [`processes::Unproven`]. Po suficie
    /// zostaje więc ostatnia linia obrony, którą [`Checking`] ma zawsze — gwardia `Drop` na
    /// [`crate::engine::supervisor::Supervised`], czyli twardy `killpg` plus zebranie lidera.
    /// `Alive` wraca stąd jako brak dowodu i to on pisze zdanie dla człowieka. To jest ŚWIADOMIE
    /// SŁABSZE niż po stronie agenta — `Drop` zabija, ale nie dowodzi `ESRCH`, a miejsce z puli
    /// wraca do niej razem z ramką kroku. Pełne domknięcie wymaga drugiego rodzaju właściciela
    /// w rejestrze ocalałych i jest zgłoszone jako rozszerzenie planu (`AGENTS.md` §7).
    async fn prove_check_dead(&self, live: &mut Checking, first: GroupProof) -> GroupProof {
        /* PIERWSZY DOWÓD LICZY SIĘ JAKO PRÓBA NR 1 (2026-09, Z-4). `Checking::give_up` zdobywa go
         * przez `Supervised::stop`, czyli przez dokładnie tę samą pełną eskalację TERM → łaska →
         * KILL → dowód, którą robi `Checking::cancel` niżej. Potraktowany jako „zerowy" dawał
         * CZTERY eskalacje tam, gdzie polityka produktu mówi trzy — a wtedy ta jedna droga liczy
         * inaczej niż pozostałe trzy i sufit przestaje być jedną liczbą (niezmiennik 23). */
        let mut proof = first;
        for attempt in 1..=LIVE_STOP_ATTEMPTS {
            if matches!(proof, GroupProof::Dead { .. }) {
                return proof;
            }
            tracing::error!(
                attempt,
                attempts = LIVE_STOP_ATTEMPTS,
                "a check group is still alive after a full escalation"
            );
            // Odstęp i kolejna eskalacja WYŁĄCZNIE między próbami: po ostatniej nie ma na co
            // czekać, a dodatkowy `cancel()` byłby czwartą próbą pod trzyliterową stałą.
            if attempt < LIVE_STOP_ATTEMPTS {
                tokio::time::sleep(LIVE_STOP_RETRY_PAUSE).await;
                proof = live.cancel().await;
            }
        }
        proof
    }

    /// Kończy krok po limicie wyłącznie przez supervisor i utrwala jego rzeczywisty dowód.
    ///
    /// Oddaje dowód razem z werdyktem — dokładnie jak [`Live::stop_cancelled_agent`] i z tego
    /// samego powodu (2026-09, Z-4): bez `GroupProof` w ręku wołający nie ma jak przekazać uchwytu
    /// ani miejsca z puli rejestrowi aplikacji ([`processes::Unproven`]), a `Alive` znaczy dokładnie
    /// tyle, że oba te zasoby muszą tam pojechać. Do tego dnia ta droga nie wracała w ogóle:
    /// `prove_agent_dead` kręciło się bez sufitu, więc krok po limicie czasu nad grupą, której nie
    /// da się dowieść, zabierał ze sobą cały bieg i folder aż do restartu Loadouta.
    async fn stop_overdue_agent(
        &self,
        id: StepId,
        handle: &mut dyn AgentHandle,
        limit: Duration,
    ) -> (StepReport, GroupProof) {
        let proof = self.prove_agent_dead(handle).await;
        // PRZED ZAPISEM DOWODU, na tej drodze jak na każdej innej (2026-09, Z-01d): uchwyt jeszcze
        // żyje, więc jeszcze jest kogo zapytać, co ten krok po sobie zostawił.
        self.note_pgids(id, &handle.descendant_groups());
        let proven_dead = matches!(&proof, GroupProof::Dead { .. });
        self.update(|book| {
            let step = &mut book.steps[id];
            step.death_proof = proven_dead;
            step.error = Some(if proven_dead {
                format!(
                    "This step ran longer than its {} minute limit, so Loadout stopped it. Give \
                     it more minutes in the agent, or split the work.",
                    limit.as_secs() / 60
                )
            } else {
                /* ZDANIE O OCALAŁYM WYGRYWA ZE ZDANIEM O LIMICIE (2026-09, Z-4). Oba są prawdą,
                 * ale mówią człowiekowi zrobić dwie różne rzeczy: tamto każe przestawić minuty,
                 * a to mówi, że coś MOŻE JESZCZE BIEC i palić limit u dostawcy. Zdanie o minutach
                 * nad żywą grupą jest tym samym „stopped it", przed którym stoi niezmiennik 6. */
                STEP_SURVIVOR_ERROR.to_owned()
            });
        });
        // Powod jest juz zapisany wyzej i mowi wiecej niz zdanie ponizej, wiec `get_or_insert`
        // go nie tknie. Ustawienie czlowieka rozstrzyga jednak tak samo, jak przy kazdej innej
        // porazce: krok, ktory nie zdazyl, tez byl slepym punktem.
        let report = self
            .when_this_one_fails(id, "This step ran out of time.")
            .await;
        (report, proof)
    }

    /// Schodzi po turze, która przebiła udział kroku w sufcie — **przez sterownik**, jak każde
    /// inne zejście (niezmienniki 6 i 10).
    ///
    /// 2026-09 (Z-13b) — TA DROGA ISTNIEJE DLA VENDORA BEZ WŁASNEJ FLAGI SUFITU. Claude dostaje
    /// `--max-budget-usd` i zatrzyma turę sam, od środka; Codex takiej flagi nie ma i jego turę
    /// musi przerwać Loadout, na własnej estymacie z tabeli cen. Anulowanie samego zadania Rusta
    /// zostawiłoby proces systemowy żywy, więc idzie to tą samą eskalacją, co limit czasu.
    ///
    /// Zdanie wychodzi na ekran PRZED zabijaniem: eskalacja trwa sekundy, a człowiek ma się
    /// dowiedzieć, dlaczego jego krok właśnie znika, w chwili, w której to się dzieje.
    /// Osobna metoda, a nie ramię `match`, bo `finish_agent_turn` stoi pod sufitem stu linii.
    async fn stop_agent_over_its_share(
        &self,
        id: StepId,
        handle: &mut dyn AgentHandle,
        said: &str,
    ) -> GroupProof {
        // Wynik świadomie porzucony: pełna kolejka do okna jest normalnym stanem (`ipc::Sent`),
        // a bieg nie ma prawa stanąć dlatego, że okno nie nadąża.
        let _ = self.lines.send(Line::Problem {
            agent: self.plan.steps[id].name.clone(),
            text: said.to_owned(),
            resets_at: None,
        });
        let proof = self.prove_agent_dead(handle).await;
        // PRZED ZAPISEM DOWODU, tak samo jak na każdej innej drodze zejścia agenta (2026-09,
        // Z-01d): uchwyt jeszcze żyje, więc jeszcze jest kogo zapytać, co ten krok zostawił.
        self.note_pgids(id, &handle.descendant_groups());
        let proven_dead = matches!(proof, GroupProof::Dead { .. });
        self.update(|book| {
            let step = &mut book.steps[id];
            step.death_proof = proven_dead;
            if !proven_dead {
                /* ZDANIE O OCALAŁYM WYGRYWA ZE ZDANIEM O SUFICIE, tym samym idiomem, co po
                 * limicie czasu: oba są prawdą, ale tylko to drugie każe komuś sprawdzić
                 * maszynę, bo agent, którego nie dało się dowieść jako martwego, dalej pali
                 * limit u dostawcy — i dalej wydaje. */
                step.error = Some(LIVE_STOP_SURVIVOR_ERROR.to_owned());
            }
        });
        proof
    }

    /// Kończy krok po Stopie i oddaje dowód właścicielowi uchwytu oraz ciężkiego slotu.
    async fn stop_cancelled_agent(
        &self,
        id: StepId,
        handle: &mut dyn AgentHandle,
    ) -> (StepReport, GroupProof) {
        let proof = self.prove_agent_dead(handle).await;
        // PRZED ZAPISEM DOWODU (2026-09, Z-01d). To jest ta droga, dla której cała ta praca
        // powstała: po nieudanym Stopie ocalały wnuk nie trafiał do pliku, więc odzyskiwanie przy
        // następnym otwarciu folderu nie miało czego szukać.
        self.note_pgids(id, &handle.descendant_groups());
        let proven_dead = matches!(&proof, GroupProof::Dead { .. });
        self.update(|book| {
            let step = &mut book.steps[id];
            step.death_proof = proven_dead;
            if !proven_dead {
                step.error = Some(LIVE_STOP_SURVIVOR_ERROR.to_owned());
            }
        });
        let report = if proven_dead {
            StepReport::Cancelled
        } else {
            // 2026-08-27 — Stop całego biegu pozostaje anulowaniem, ale ten krok nie może być
            // nazwany anulowanym: należąca do niego grupa nadal mogła żyć po pełnej eskalacji.
            StepReport::Failed
        };
        (report, proof)
    }

    /// Zamyka sesję kroku i **dowodzi**, że po jej grupie nie zostało nic.
    ///
    /// Dwa czasowniki w jednym, bo są jedną rzeczą — końcem kroku — i bo tylko tutaj oba fakty
    /// są jeszcze razem: `close()` zamyka wejście i zbiera LIDERA, oddając jego kod wyjścia,
    /// a wnuk nie jest naszym dzieckiem i nie zobaczy go żaden nasz `wait()`
    /// [T7 §3.1: `total=2 orphaned=2` przy statusie dziecka mówiącym „zabity"].
    ///
    /// 2026-08-28 — DO TEGO DNIA DRUGIEJ POŁOWY NIE BYŁO. `GroupProof` powstawał wyłącznie po
    /// Stopie, po limicie czasu i po `close()`, które PADŁO; tura, która skończyła się dobrze,
    /// nie pytała jądra o nic. Krok zapalał się więc człowiekowi na „done" nad grupą, która dalej
    /// mieli i dalej płaci — a niezmiennik 6 nie zna stanu „chyba nie żyje".
    ///
    /// **Wołane przed `hand_over` i przed [`Live::finish_this_step`]**, bo to tamte wypuszczają
    /// potomków i zapalają stan na ekranie: dowód wzięty za nimi byłby dowodem, którego człowiek
    /// nie zdążył zobaczyć, a punkt kontrolny za krokiem zdążyłby już zapytać.
    async fn close_and_prove(
        &self,
        id: StepId,
        handle: &mut dyn AgentHandle,
        evidence: &EvidenceTarget,
        cancel: &CancellationToken,
    ) -> Closed {
        // `claude` z otwartym stdinem czeka w nieskończoność, więc krok bez tego zostawia żywy
        // proces [T1 §2, §4.6].
        let (how, code) = {
            let closing = handle.close();
            tokio::pin!(closing);
            tokio::select! {
                // 2026-09 — wynik zamknięcia ma pierwszeństwo, jeśli przyszedł w tej samej
                // chwili co Stop; to zachowuje kolejność końca żywej tury powyżej.
                biased;
                closed = &mut closing => match closed {
                    Ok(code) => (ClosedHow::OnItsOwn, code),
                    Err(error) if error.downcast_ref::<DidNotLetGo>().is_some() => {
                        (ClosedHow::WouldNotLetGo, None)
                    }
                    Err(_) => (ClosedHow::Broke, None),
                },
                () = cancel.cancelled() => (ClosedHow::StoppedByAPerson, None),
            }
        };
        if matches!(how, ClosedHow::Broke) {
            evidence.mark_incomplete();
        }
        let proof = match how {
            ClosedHow::OnItsOwn | ClosedHow::WouldNotLetGo | ClosedHow::StoppedByAPerson => {
                self.prove_step_dead(handle).await
            }
            ClosedHow::Broke => {
                /* `close()` PADŁO, więc sesji nie da się już domknąć w paśmie: dowód bierzemy
                 * pełną eskalacją przerwaniem, nie `proof_of_death()`.
                 *
                 * 2026-09 (Z-4): ta gałąź ma teraz TEN SAM SUFIT co reszta. Do tego dnia kręciła
                 * się bez końca, bo powrót na `Alive` zrzuciłby `Box<dyn AgentHandle>` i osierocił
                 * grupę. Dziś wołający oddaje uchwyt i miejsce z puli rejestrowi niedowiedzionych
                 * (`released_by`), więc `Alive` niczego już nie osieroca — i bieg schodzi zamiast
                 * wisieć, a zapadka folderu nie blokuje następnego Startu do restartu. */
                self.prove_agent_dead(handle).await
            }
        };
        // PRZED ZAPISEM DOWODU (2026-09, Z-01d). Także tutaj, choć to jest droga UDANA: `close()`
        // zbiera lidera, a wnuk nie jest naszym dzieckiem i nie zobaczy go żaden nasz `wait()`
        // [T7 §3.1] — więc krok, który zszedł sam, potrafi zostawić dokładnie taką samą grupę,
        // jak krok zatrzymany siłą.
        self.note_pgids(id, &handle.descendant_groups());
        let proven_dead = matches!(proof, GroupProof::Dead { .. });
        self.update(|book| book.steps[id].death_proof = proven_dead);
        Closed { how, code, proof }
    }

    /// Zwalnia zasoby kroku — albo oddaje je rejestrowi aplikacji, kiedy dowodu nie było.
    ///
    /// 2026-08-28 — ZWOLNIENIE PROWADZI PRZEZ DOWÓD I TYLKO PRZEZ NIEGO. Uchwyt sesji i miejsce
    /// z puli wchodzą tu w JEDNĄ wartość ([`processes::Unproven`]), z której wyjście prowadzi
    /// wyłącznie przez [`processes::Unproven::released_by`] — a ta żąda `GroupProof` i przepuszcza
    /// tylko `Dead`, i tylko raz, bo bierze `self` przez wartość.
    ///
    /// Do tego dnia obie te rzeczy ginęły razem z ramką [`Live::one_turn`] także wtedy, gdy
    /// dowodu nie było, i każda z osobna jest tą samą wadą:
    ///
    /// * porzucony uchwyt był **jedyny**, więc nikt już nie mógł ponowić eskalacji na tej grupie
    ///   — z `run.json` zostawał `pgid`, czyli adres, a nie właściciel;
    /// * oddany permit to miejsce, które natychmiast zajmuje następny agent po ~583 MB, obok
    ///   grupy, która dalej pali limit u dostawcy (niezmiennik 11).
    ///
    /// Krok i tak kończy się `failed` ze zdaniem dla człowieka — ale to jest zdanie o tym, czego
    /// nie wiemy, a nie zgoda na sprzątnięcie.
    fn released_by(
        &self,
        handle: Box<dyn AgentHandle>,
        slot: &mut Option<limits::Slot>,
        proof: GroupProof,
    ) {
        let address = handle.group();
        let (_proof, kept) =
            processes::Unproven::new(handle, slot.take(), address).released_by(proof);
        match kept {
            // Miejsce wraca do kroku i ginie dopiero za `announce`: permit ma przeżyć grupę,
            // nie odwrotnie.
            processes::Kept::Released(returned) => *slot = returned,
            processes::Kept::Retained(still) => self.processes.keep_unproven(still),
        }
    }

    /// Dowód zejścia grupy po turze, która **skończyła się sama** — w tej samej ograniczonej
    /// polityce, co żywy Stop.
    ///
    /// 2026-08-28 — osobny czasownik uchwytu ([`AgentHandle::proof_of_death`]), a nie `cancel()`:
    /// tamta prowadzi przerwaniem w paśmie i czeka na odpowiedź, której proces po `close()` już
    /// nie wyśle, więc każdy udany krok płaciłby całym oknem przerwania.
    ///
    /// **Ograniczona, nie wieczna**, tym samym sufitem, co [`Live::prove_agent_dead`]: bieg
    /// zamrożony na zawsze przy grupie, której nie da się dowieść, jest gorszy niż uczciwe
    /// „nie wiem" na kafelku. Do 2026-09 tamta pętla sufitu nie miała i to była cała różnica
    /// między tymi dwoma czasownikami; dziś różnicą jest już tylko sam czasownik.
    ///
    /// **Sufit nie jest pozwoleniem na porzucenie.** Ostatni `Alive` wraca stąd w całości, razem
    /// z adresem grupy, i to on decyduje, że uchwyt oraz miejsce z puli jadą do rejestru
    /// aplikacji zamiast zginąć z ramką tej funkcji ([`processes::Unproven`]).
    async fn prove_step_dead(&self, handle: &mut dyn AgentHandle) -> GroupProof {
        let mut last = GroupProof::Alive {
            group: handle.group(),
        };
        for attempt in 1..=LIVE_STOP_ATTEMPTS {
            let proof = handle.proof_of_death().await;
            if matches!(proof, GroupProof::Dead { .. }) {
                return proof;
            }
            last = proof;
            tracing::error!(
                attempt,
                attempts = LIVE_STOP_ATTEMPTS,
                "the process group of a finished step is still alive after a full escalation"
            );
            if attempt < LIVE_STOP_ATTEMPTS {
                tokio::time::sleep(LIVE_STOP_RETRY_PAUSE).await;
            }
        }
        last
    }

    /// To samo dla komendy kroku „sprawdź", która doszła do końca sama.
    ///
    /// [`Checking::cancel`] jest już czystym [`crate::engine::supervisor::Supervised::stop`] —
    /// nie ma tu przerwania w paśmie do pominięcia, więc uchwyt komendy nie potrzebuje drugiego
    /// czasownika.
    async fn prove_check_settled(&self, id: StepId, live: &mut Checking) -> GroupProof {
        // 2026-09 (Z-4) — `GroupProof`, nie `bool`. Ostatni `Alive` decyduje, że uchwyt komendy
        // i jej miejsce z puli jadą do rejestru ocalałych zamiast zginąć z ramką kroku, a `bool`
        // nie niesie ani adresu grupy, ani niczego, na czym dałoby się tę decyzję oprzeć.
        let mut last = GroupProof::Alive {
            group: Some(live.group()),
        };
        for attempt in 1..=LIVE_STOP_ATTEMPTS {
            let proof = live.cancel().await;
            // PRZED ODDANIEM DOWODU WOŁAJĄCEMU (2026-09, Z-01d), także na drodze udanej: wiersz
            // powłoki bywa całym `npm test` z serwerem w tle, a zdanie o grupie lidera nie mówi
            // o nim nic. `id` jest tu argumentem właśnie po to — bez niego ten zapis musiałby
            // stać u wołającego, czyli w drugim miejscu na każdą z trzech dróg zejścia komendy.
            self.note_pgids(id, &live.descendant_groups());
            if matches!(proof, GroupProof::Dead { .. }) {
                return proof;
            }
            last = proof;
            tracing::error!(
                attempt,
                attempts = LIVE_STOP_ATTEMPTS,
                "the process group of a finished check is still alive after a full escalation"
            );
            if attempt < LIVE_STOP_ATTEMPTS {
                tokio::time::sleep(LIVE_STOP_RETRY_PAUSE).await;
            }
        }
        last
    }

    /// Zwalnia zasoby komendy — albo oddaje je rejestrowi aplikacji, kiedy dowodu nie było.
    ///
    /// 2026-09 (Z-4) — bliźniak [`Live::released_by`] po stronie kroku „sprawdź". Do tego dnia ta
    /// droga nie istniała: uchwyt komendy spadał z ramki `run_check`, a jego miejsce z puli wracało
    /// do niej razem z krokiem. `Drop` na [`crate::engine::supervisor::Supervised`] posyłał grupie
    /// dziewiątkę, ale **nie dowodził `ESRCH`** — więc zostawała grupa, o którą nikt nie mógł już
    /// zapytać, i pula, która o niej nie wie (niezmienniki 6 i 11). Ponawianie należy od tej pory
    /// do `Processes::prove_the_unproven`, dokładnie jak przy sesji agenta.
    fn check_released_by(
        &self,
        live: Checking,
        slot: &mut Option<limits::Slot>,
        proof: &GroupProof,
    ) {
        if matches!(proof, GroupProof::Dead { .. }) {
            // Dowód był: uchwyt ginie tutaj i jego gwardia `Drop` nie ma już czego zabijać
            // (`Supervised::proved_dead`), a miejsce zostaje przy kroku i schodzi z jego ramką.
            return;
        }
        self.processes.keep_leftover(Box::new(live), slot.take());
    }

    /// Ponawia pełną eskalację przerwaniem, ale nie zamraża aplikacji na zawsze.
    ///
    /// 2026-08-27 — trzy próby są polityką produktu, nie parametrem wołającego. Każde
    /// `cancel()` przechodzi przez pełne TERM → łaska → KILL → dowód supervisora; odstęp jest
    /// tylko między próbami. Po ostatnim `Alive` posiadany uchwyt wraz z ciężkim slotem przechodzi
    /// do rejestru żywych procesów: adres nie wystarcza, bo tylko właściciel może ponowić dowód.
    /// `Alive` wraca jako brak dowodu, nigdy jako `Dead`.
    ///
    /// 2026-09 (Z-4) — JEDNO CIAŁO NA WSZYSTKICH PIĘCIU DROGACH, i to nie jest porządkowanie.
    /// Do tego dnia stały tu dwie funkcje o tym samym ciele i różnym suficie: żywy Stop i seria
    /// porażek narzędzia miały te trzy próby, a limit czasu kroku, tura, która wróciła błędem,
    /// i nieudane `close()` szły pętlą bez końca. Ta sama grupa, której nie da się dowieść,
    /// kończyła więc bieg w dwóch przypadkach i wieszała go w trzech pozostałych — a wieszała
    /// razem z folderem, bo `settle()` nie zapadało (niezmiennik 23).
    async fn prove_agent_dead(&self, handle: &mut dyn AgentHandle) -> GroupProof {
        // Adres z uchwytu jest tym, co wiemy PRZED pierwszą próbą; każdy kolejny `Alive` niesie
        // własny i nadpisuje ten wstępny. Zwrócenie świeżo zmyślonego `Alive` gubiłoby jedno
        // i drugie, a wołający dostawałby odmowę bez adresu (2026-08-28).
        let mut last = GroupProof::Alive {
            group: handle.group(),
        };
        for attempt in 1..=LIVE_STOP_ATTEMPTS {
            let proof = handle.cancel().await;
            if matches!(proof, GroupProof::Dead { .. }) {
                return proof;
            }
            last = proof;
            tracing::error!(
                attempt,
                attempts = LIVE_STOP_ATTEMPTS,
                "an agent group is still alive after a full escalation"
            );
            if attempt < LIVE_STOP_ATTEMPTS {
                tokio::time::sleep(LIVE_STOP_RETRY_PAUSE).await;
            }
        }
        last
    }

    /// Czeka na dokładnie jeden z czterech końców tury, zachowując pierwszeństwo istniejącej
    /// polityki oraz barierę FIFO między wynikiem procesu i zdarzeniem `Finished`.
    async fn wait_for_agent_turn(
        handle: &mut dyn AgentHandle,
        turn: &mut LiveAgentTurn<'_>,
        limit: Duration,
    ) -> Ended {
        let cancel = turn.cancel;
        let runtime_fault = &mut *turn.runtime_fault;
        let finished_event = &mut *turn.finished_event;
        let waiting = handle.wait();
        tokio::pin!(waiting);
        let overdue = tokio::time::sleep(limit);
        tokio::pin!(overdue);
        tokio::select! {
            // `biased`, bo tura, która właśnie się skończyła, ma pierwszeństwo przed Stopem
            // wpadającym w tej samej chwili. Limit czasu stoi po Stopie z tego samego powodu.
            biased;
            done = &mut waiting => match done {
                Err(error) => Ended::Turn(Err(error)),
                Ok(outcome) => {
                    // App Server oddaje wynik przed zdarzeniem. Bariera dowodzi, że pompa policzyła
                    // każdy wcześniejszy `ToolEnd`, zanim wynik zostanie przyjęty.
                    let _ = finished_event.recv().await;
                    match runtime_fault.try_recv() {
                        Ok(why) => why.into(),
                        Err(_) => Ended::Turn(Ok(outcome)),
                    }
                }
            },
            () = cancel.cancelled() => Ended::Stopped,
            Some(why) = runtime_fault.recv() => why.into(),
            () = &mut overdue => Ended::Overdue,
        }
    }

    /// Rozlicza normalny wynik tury dopiero po zamknięciu sesji i dowodzie zejścia grupy.
    async fn finish_completed_agent_turn(
        &self,
        mut handle: Box<dyn AgentHandle>,
        outcome: DriverOutcome,
        cost_is_estimate: bool,
        turn: &mut LiveAgentTurn<'_>,
        carried: &TurnTotals,
    ) -> StepReport {
        let id = turn.id;
        // L-01: rachunek kroku to suma tur, które naprawdę się odbyły. Wynik i powód
        // pochodzą wyłącznie z tej ostatniej.
        let totals = carried.with(&outcome);
        let Closed { how, code, proof } = self
            .close_and_prove(id, handle.as_mut(), turn.evidence, turn.cancel)
            .await;
        let proven_dead = matches!(proof, GroupProof::Dead { .. });
        // PRZED `released_by`, bo tamto konsumuje dowód — a to jest jedyne miejsce, w którym
        // status zebranego lidera jeszcze istnieje i da się z niego przeczytać numer sygnału
        // (2026-09, Z-39).
        let went_down = how_it_went_down(how, code, &proof);
        self.released_by(handle, &mut *turn.slot, proof);
        if !proven_dead {
            turn.finish_forward.cancel();
        }
        // Sukces to zero **i** `is_error == false` (niezmiennik 19, ARCHITECTURE §5).
        let evidence_complete = turn.evidence.is_healthy();
        let ok = outcome.ok
            && matches!(how, ClosedHow::OnItsOwn)
            && evidence_complete
            && proven_dead
            && matches!(code, None | Some(0));
        self.update(|book| {
            let step = &mut book.steps[id];
            step.exit_code = code;
            totals.record(step, cost_is_estimate);
            step.end_cause = Some(if !proven_dead {
                super::run_inputs::EndCause::UnprovenStop
            } else if !matches!(how, ClosedHow::OnItsOwn) || !evidence_complete {
                super::run_inputs::EndCause::InfrastructureFailed
            } else if matches!(outcome.reason, FinishReason::Cancelled) {
                super::run_inputs::EndCause::Cancelled
            } else if matches!(outcome.reason, FinishReason::LimitReached) {
                super::run_inputs::EndCause::LimitReached
            } else if ok {
                super::run_inputs::EndCause::Completed
            }
            // Ogólne Failed(String) vendora nie odróżnia awarii logowania od błędu pracy.
            // Nie zgadujemy na podstawie prozy; brak dowodu zostaje jawnie nieznany.
            else {
                super::run_inputs::EndCause::Unknown
            });
            step.summary = summary_of(&outcome.text);
            /* NUMER SYGNAŁU WCHODZI DO KSIĘGI ZAWSZE, także wtedy, gdy zdanie o nim przegra
             * z powodem wyżej (2026-09, Z-39). To jest fakt, nie tekst: przeżywa skasowanie
             * `loadout.db` (niezmiennik 4) i po nim jednym poznaje się, że ten krok nie padł. */
            if let HowItWentDown::StoppedFromOutside(signal) = went_down {
                step.stopped_from_outside = Some(signal);
            }
            if matches!(how, ClosedHow::WouldNotLetGo) && proven_dead {
                step.error = Some(STEP_WOULD_NOT_LET_GO_ERROR.to_owned());
            } else if matches!(how, ClosedHow::Broke) || !evidence_complete {
                /* Surowy blad zapisu moze zawierac prywatna sciezke albo tekst vendora.
                 * Ksiege i ekran dostaja staly rodzaj; szczegol zostaje lokalnie przy
                 * prywatnym artefakcie, ktory nadal ma stan niekompletny. */
                step.error = Some(
                    "Loadout could not preserve this agent's private run evidence. The \
                     step was not accepted as complete."
                        .to_owned(),
                );
            } else if !proven_dead {
                /* Agent zrobił swoje, ale Loadout nie umie powiedzieć, czy coś po nim zostało.
                 * Ten nasz powód wygrywa z powodem agenta. */
                step.error = Some(STEP_SURVIVOR_ERROR.to_owned());
            } else if let Some(said) = went_down.said() {
                /* NASZE ZDANIE WYGRYWA Z POWODEM STEROWNIKA, tym samym idiomem, którym wygrywa
                 * z nim `STEP_SURVIVOR_ERROR` wyżej (2026-09, Z-39). Vendor mówi wtedy „The agent
                 * stopped without ever sending its result" — prawdę o tym, co widział, i zdanie
                 * o agencie, który nie zrobił nic złego: został ubity z zewnątrz. Człowiek szuka
                 * po nim wady u agenta i nie znajduje jej, bo jej tam nie ma. */
                step.error = Some(said);
            } else if !ok
                && let FinishReason::Failed(said) = &outcome.reason
                && let Some(short) = one_line(said, SUMMARY_LIMIT)
            {
                step.error = Some(short);
            }
        });
        /* ODŁOŻONY UDZIAŁ MALEJE O TO, ILE TA TURA NAPRAWDĘ KOSZTOWAŁA (2026-09, Z-13b) — nie
         * znika, bo krok trzyma jeszcze miejsce z puli i schodzi dopiero kilka kroków niżej.
         * Powód w całości stoi przy [`Live::settle_the_share`].
         *
         * Za zamkiem księgi, nie w środku: kolejność zamków rezerwacje → księga jest w tym pliku
         * jedyna (niezmiennik 8). */
        if let Some(spent) = outcome.cost_usd {
            self.settle_the_share(id, spent);
        }
        if matches!(how, ClosedHow::StoppedByAPerson) {
            // 2026-09 — Stop całego biegu pozostaje anulowaniem, lecz krok z grupą nadal żywą
            // nie może dostać tej nazwy. To ta sama granica, co w `stop_cancelled_agent`.
            if proven_dead {
                StepReport::Cancelled
            } else {
                StepReport::Failed
            }
        } else if ok {
            // Przekazanie schodzi na dysk przed zwolnieniem potomków przez scheduler.
            self.hand_over(id, &outcome.text, turn.reads);
            self.remember_handoff_evidence(id, &outcome.text);
            let native = self.native_route_for(id).await;
            let why = self
                .file_the_answer(id, &outcome.text)
                .err()
                .or_else(|| self.missing_a_required_field(id, &outcome.text))
                .or_else(|| {
                    self.verdict_after(id, &outcome.text, native.as_ref())
                        .map(str::to_owned)
                })
                // 2026-09-08 — publikacja stoi za KAŻDYM kontraktem wyniku. Zielony proces
                // bez wymaganego pola nie może zostawić planu, którego krok nie dostarczył.
                .or_else(|| self.finish_work_plan(id).err());
            match why {
                Some(why) => self.when_this_one_fails(id, &why).await,
                None => StepReport::Succeeded,
            }
        } else {
            self.when_this_one_fails(id, "This step did not finish what it was given.")
                .await
        }
    }

    /// Rozdziela rozstrzygnięcie tury bez zmiany właściciela uchwytu ani ciężkiego slotu.
    async fn finish_agent_turn(
        &self,
        mut handle: Box<dyn AgentHandle>,
        finished: Ended,
        cost_is_estimate: bool,
        limit: Duration,
        turn: &mut LiveAgentTurn<'_>,
        carried: &TurnTotals,
    ) -> Turned {
        let id = turn.id;
        match finished {
            // Timeout przechodzi przez sterownik; anulowanie samego zadania Rusta zostawiłoby
            // proces systemowy żywy (niezmienniki 6 i 10).
            Ended::Overdue => {
                let (report, proof) = self.stop_overdue_agent(id, handle.as_mut(), limit).await;
                let proven_dead = matches!(&proof, GroupProof::Dead { .. });
                // Ta sama trójka, co przy Stopie i z tego samego powodu (2026-09, Z-4): `Alive`
                // zabiera uchwyt i miejsce do rejestru, a odbiornik kuratora trzeba wtedy domknąć
                // osobno — zachowany uchwyt trzyma klon nadajnika zdarzeń, więc `pump.await`
                // w `run_agent` nie wróciłby nigdy i krok wisiałby mimo sufitu.
                self.released_by(handle, &mut *turn.slot, proof);
                if !proven_dead {
                    turn.finish_forward.cancel();
                }
                Turned::Settled(report)
            }
            Ended::Stopped => {
                let (report, proof) = self.stop_cancelled_agent(id, handle.as_mut()).await;
                let proven_dead = matches!(&proof, GroupProof::Dead { .. });
                self.released_by(handle, &mut *turn.slot, proof);
                if !proven_dead {
                    turn.finish_forward.cancel();
                }
                Turned::Settled(report)
            }
            Ended::RepeatedToolFailure => {
                let _ = self.lines.send(Line::Problem {
                    agent: self.plan.steps[id].name.clone(),
                    text: REPEATED_TOOL_FAILURE_SENTENCE.to_owned(),
                    resets_at: None,
                });
                let proof = self.prove_agent_dead(handle.as_mut()).await;
                // PRZED ZAPISEM DOWODU, tak samo jak na każdej innej drodze zejścia agenta
                // (2026-09, Z-01d). Powtórzona awaria narzędzia kończy krok równie twardo jak
                // Stop i równie łatwo zostawia wnuka we własnej grupie — a bez tej linii jego
                // numer nie trafiał do `run.json` i odzyskiwanie nie miało czego szukać.
                self.note_pgids(id, &handle.descendant_groups());
                let proven_dead = matches!(proof, GroupProof::Dead { .. });
                self.update(|book| {
                    let step = &mut book.steps[id];
                    step.death_proof = proven_dead;
                    if !proven_dead {
                        step.error = Some(LIVE_STOP_SURVIVOR_ERROR.to_owned());
                    }
                });
                // `Alive` zabiera uchwyt i slot do rejestru; `Dead` zwalnia oba zwykłą drogą.
                self.released_by(handle, &mut *turn.slot, proof);
                if !proven_dead {
                    turn.finish_forward.cancel();
                }
                Turned::Broke(REPEATED_TOOL_FAILURE_SENTENCE.to_owned())
            }
            Ended::OverItsShare { allowed } => {
                let said = over_its_share_sentence(allowed);
                let proof = self
                    .stop_agent_over_its_share(id, handle.as_mut(), &said)
                    .await;
                let proven_dead = matches!(proof, GroupProof::Dead { .. });
                self.released_by(handle, &mut *turn.slot, proof);
                if !proven_dead {
                    turn.finish_forward.cancel();
                }
                Turned::Broke(said)
            }
            Ended::Turn(Err(error)) => {
                let proof = self.prove_agent_dead(handle.as_mut()).await;
                // PRZED ZAPISEM DOWODU, tak samo jak na każdej innej drodze zejścia agenta
                // (2026-09, Z-01d). Tura, która się przewróciła, zostawia wnuka równie chętnie
                // jak tura zatrzymana — a to jest droga, którą krok schodzi po awarii sterownika,
                // czyli dokładnie wtedy, gdy Loadout najmniej wie o tym, co jeszcze biegnie.
                self.note_pgids(id, &handle.descendant_groups());
                let proven_dead = matches!(&proof, GroupProof::Dead { .. });
                self.update(|book| {
                    let step = &mut book.steps[id];
                    step.death_proof = proven_dead;
                    /* NASZE ZDANIE WYGRYWA Z POWODEM STEROWNIKA (2026-09, Z-4), tym samym idiomem,
                     * którym `Ended::RepeatedToolFailure` przykrywa powód agenta. Do tego dnia
                     * z tej drogi zostawał wyłącznie błąd tury — prawdziwy, ale mówiący o czymś
                     * innym: człowiek czytał, dlaczego tura padła, i nie dowiadywał się, że po
                     * tym kroku MOŻE COŚ JESZCZE BIEC i palić limit u dostawcy.
                     *
                     * JEDNO ZDANIE NA WSZYSTKIE DROGI OCALAŁEGO POZA ŻYWYM STOPEM: `death_proof:
                     * false` znaczy tu dokładnie to samo, co po limicie czasu kroku, więc krok
                     * mówi to samo, co tamten. Rozróżnienie zostaje wyłącznie tam, gdzie zdanie
                     * odpowiada na inne pytanie człowieka — po Stopie, który ktoś nacisnął
                     * (`LIVE_STOP_SURVIVOR_ERROR`, powód przy tamtej stałej). */
                    step.error = Some(if proven_dead {
                        error.to_string()
                    } else {
                        STEP_SURVIVOR_ERROR.to_owned()
                    });
                });
                // Tura, która się przewróciła, nie zwalnia z dowodu (2026-09, Z-4): bez tej pary
                // linii ocalała grupa ginęła razem z ramką, czyli dokładnie w chwili, w której
                // Loadout właśnie przyznał, że nie wie, czy coś jeszcze biegnie.
                self.released_by(handle, &mut *turn.slot, proof);
                if !proven_dead {
                    turn.finish_forward.cancel();
                }
                Turned::Broke("This step's agent stopped in the middle of its turn.".to_owned())
            }
            Ended::Turn(Ok(outcome)) => Turned::Settled(
                self.finish_completed_agent_turn(handle, outcome, cost_is_estimate, turn, carried)
                    .await,
            ),
        }
    }

    /// Jedna tura agenta: czekaj na koniec albo na Stop, a udany wynik oddaj następnym.
    ///
    /// Tura, która wróciła błędem, zostaje [`Turned::Broke`] i rozstrzyga się w `run_agent`
    /// dopiero po opróżnieniu kuratora.
    async fn one_turn(
        &self,
        mut handle: Box<dyn AgentHandle>,
        mut turn: LiveAgentTurn<'_>,
    ) -> Turned {
        let id = turn.id;
        let cost_is_estimate = handle.session().vendor == "codex";
        // `pid` i `pgid` zapisujemy przed pierwszym zdarzeniem; po awarii to adres odzyskiwania.
        if let Some(group) = handle.group() {
            self.update(|book| {
                let step = &mut book.steps[id];
                step.pid = Some(group.pid);
                step.pgid = Some(group.pgid);
            });
        }

        /* Głos jest dostępny przez całą turę i zdejmowany na wspólnej drodze po każdym wyniku. */
        let message_generation = self.control.step_session_started(
            &self.plan.steps[id].node_key,
            &self.plan.steps[id].name,
            handle.voice(),
        );

        // Zegar rusza przy czekaniu, nie przy planowaniu ani oczekiwaniu na wolny slot.
        let limit = match &self.plan.steps[id].job {
            Job::Agent(job) => job.give_up_after,
            Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => Duration::MAX,
        };
        /* L-01: SUFIT CZASU DOTYCZY CAŁEGO KROKU, NIE KAŻDEJ TURY Z OSOBNA (2026-09-06).
         * Zegar zerowany przy każdej wiadomości oddaje krokowi nieskończony czas w zamian
         * za jedno zdanie co kilka minut — a limit kroku jest tym, co gasi bieg, który
         * przestał posuwać się do przodu. */
        let started = tokio::time::Instant::now();
        let node_key = self.plan.steps[id].node_key.clone();
        let mut carried = TurnTotals::default();
        let mut finished = Self::wait_for_agent_turn(handle.as_mut(), &mut turn, limit).await;
        /* L-01 (incydent I-01): tura, która wróciła wynikiem, NIE JEST końcem sesji, dopóki
         * kolejka tego kroku ma czym ją przedłużyć. Do 2026-09-06 pierwszy `result` szedł
         * prosto w zamknięcie wejścia i zabicie grupy — razem z turą, którą vendor już zaczął
         * dla przyjętej wiadomości człowieka. */
        loop {
            let Ended::Turn(Ok(done)) = finished else {
                break;
            };
            let Some(voice) = handle.voice() else {
                // Sesja bez głosu nie ma jak dostać kolejnej tury; nic też nie mogło do niej
                // wejść (`accept_into_the_queue` odmawia bez głosu).
                self.control.stop_accepting(&node_key, message_generation);
                finished = Ended::Turn(Ok(done));
                break;
            };
            let correction = match &self.plan.steps[id].job {
                Job::Agent(job) => self.plan_candidate_problem_during_session(id, job),
                Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => None,
            };
            let next = if let Some(why) = correction {
                // 2026-09-08 — poprawka jedzie do TEJ SAMEJ sesji i pod pozostałym czasem
                // kroku. Nowy agent zgubiłby przypięte wejścia i podwoił koszt formatu.
                format!(
                    "Loadout checked the plan candidate and could not accept it: {why} Correct the candidate at the same path, then finish this step again."
                )
            } else {
                let Some(next) = self
                    .control
                    .next_turn_or_stop_accepting(&node_key, message_generation)
                else {
                    finished = Ended::Turn(Ok(done));
                    break;
                };
                next
            };
            if voice.send(ToAgent::Turn(next)).await.is_err() {
                // Transport padł między przyjęciem a podaniem. Krok kończy się na tym, co ma;
                // nieoddane zdania rozliczy `step_session_finished`.
                self.control.stop_accepting(&node_key, message_generation);
                finished = Ended::Turn(Ok(done));
                break;
            }
            // Rozliczenie udziału idzie PER TURA, nie na końcu: sufit wydatku sprawdzany
            // w środku następnej tury musi widzieć, ile ten krok już wydał.
            if let Some(spent) = done.cost_usd {
                self.settle_the_share(id, spent);
            }
            carried.add(&done);
            self.update(|book| carried.record(&mut book.steps[id], cost_is_estimate));
            finished = Self::wait_for_agent_turn(
                handle.as_mut(),
                &mut turn,
                limit.saturating_sub(started.elapsed()),
            )
            .await;
        }
        let report = self
            .finish_agent_turn(
                handle,
                finished,
                cost_is_estimate,
                limit,
                &mut turn,
                &carried,
            )
            .await;
        self.control
            .step_session_finished(&self.plan.steps[id].node_key, message_generation);
        report
    }

    /// Prompt kroku: jego **własna instrukcja**, indeks przekazań poprzedników i umowa o tym,
    /// jak odpowiedzieć.
    ///
    /// Instrukcja stoi pierwsza i jest w prompcie zawsze. Prompt złożony z samych cudzych wyników
    /// oddaje agentowi pracę wszystkich pozostałych i ani jednego zdania o tym, co ma z nią
    /// zrobić.
    ///
    /// Indeks jest **listą ścieżek**, nigdy treścią (D6 punkt 5, nagłówek modułu). Krok bez
    /// poprzedników dostaje swoją instrukcję i nic więcej: pusty nagłówek „steps before this one"
    /// nad zerem wpisów jest zdaniem o niczym, a agent przeczyta go jako zgubione wejście.
    ///
    /// Umowa ([`HOW_TO_ANSWER`]) stoi **na końcu i za indeksem**, i to jest treść, nie kosmetyka:
    /// indeks jest listą materiałów, a umowa mówi, co oddać. Umowa przeczytana przed listą czyta
    /// się jak podpis pod pierwszą jej pozycją.
    ///
    /// # Dlaczego indeks jest w `if`, a nie w gałęzi z własnym `return`
    ///
    /// 2026-08-23 (T-86) — do tego dnia krok bez poprzedników wychodził stąd `return`em zaraz za
    /// `handed.is_empty()`. Każde zdanie doklejane do promptu trzeba więc było dopisać w DWÓCH
    /// miejscach, a implementacja, która dopisała je w jednym, zostawiała połowę biegu bez ani
    /// jednego słowa — i nikt by tego nie zobaczył, bo prompt kroku nie trafia na żaden ekran.
    /// Jedna droga wyjścia jest tu jedyną strukturą, w której „każdy krok dostaje to samo" jest
    /// prawdą z budowy, a nie z uwagi piszącego.
    fn prompt_for(
        &self,
        id: StepId,
        instructions: &str,
        planned_context: &[ContextSource],
        minutes: u32,
    ) -> anyhow::Result<Told> {
        let handed = self.handed_before(id);
        let mut told = Told {
            prompt: instructions.to_owned(),
            reads: Vec::with_capacity(handed.len()),
            context: planned_context.to_vec(),
            extra_dirs: Vec::new(),
        };
        if let Job::Agent(job) = &self.plan.steps[id].job
            && !job.reference_materials.is_empty()
        {
            // 2026-09-08 (CT-06) — materiały są osobnym blokiem danych przed przekazaniami;
            // kontrakt odpowiedzi zostaje na końcu i `system_append` pozostaje nietknięty.
            told.prompt.push_str("\n\n");
            told.prompt.push_str(&job.reference_materials);
        }
        if !handed.is_empty() {
            self.index_of_what_came_before(&handed, &mut told)?;
            if let Job::Agent(job) = &self.plan.steps[id].job
                && !job.driver.carries_extra_dirs()
            {
                told.prompt.push_str("\n\n");
                told.prompt.push_str(HANDOFF_PATHS_ARE_OUTSIDE);
            }
        }
        told.prompt.push_str("\n\n");
        told.prompt.push_str(HOW_TO_ANSWER);
        if let Job::Agent(job) = &self.plan.steps[id].job
            && let Some(instruction) = self
                .prepared_work_plan(id, job)
                .map_err(|why| anyhow::Error::new(WorkPlanRefused(why)))?
                .and_then(|prepared| prepared.instruction())
        {
            // 2026-09-08 — stoi po zakazie zapisu zwykłego wyniku, aby kandydat był jawnie
            // nazwanym wyjątkiem i nie zastąpił przekazania, które Loadout nadal zapisuje sam.
            told.prompt.push_str(&instruction);
        }
        told.prompt.push_str("\n\n");
        told.prompt.push_str(&Self::how_long_this_step_has(minutes));
        // Pola specyficzne dla kafelka stoją za wspólnym blokiem odpowiedzi i czasu. Dzięki
        // temu każdy agent dostaje ten sam kontrakt bajt w bajt, a sędzia dopiero po nim swój
        // wymagany nośnik wyniku (T-86 AC-1, T-100 AC-1).
        self.ask_for_the_agreed_fields(id, &mut told);
        self.ask_for_an_outcome(id, &mut told);
        Ok(told)
    }

    /// Dokłada listę umówionych pól oraz wymagany wynik sędziego pętli.
    ///
    /// Wewnątrz bloku „jak odpowiadać" i zaraz za nim, bo to jest ta sama rzecz: co ten krok ma
    /// oddać. Nagłówek nad zerem pól byłby zdaniem o niczym, tak samo jak pusty indeks przekazań
    /// (powód przy [`Live::prompt_for`]).
    ///
    /// OPIS Z PLIKU JEDZIE NIETKNIĘTY. Człowiek napisał go po to, żeby agent wypełnił pole
    /// właściwą rzeczą; sama nazwa jest pytaniem, którego agent musi się domyślić.
    ///
    /// Warunek pola `outcome` jest ten sam, którego używa [`Live::verdict_after`] do sądzenia
    /// odpowiedzi. Zwykły krok go nie dostaje, a sędzia dostaje je także bez formularza — jedna
    /// lista, dwie połowy jednej umowy (niezmiennik 13).
    fn ask_for_the_agreed_fields(&self, id: StepId, told: &mut Told) {
        let Job::Agent(job) = &self.plan.steps[id].job else {
            return;
        };
        let asks_for_outcome = self.judging(&self.plan.steps[id]).is_some();
        if job.handover.is_empty() && !asks_for_outcome {
            return;
        }
        told.prompt.push_str("\n\n");
        told.prompt.push_str(FIELDS_ASKED_FOR);
        told.prompt.push('\n');
        for field in job.handover.iter().filter(|field| {
            // 2026-08-25 (T-100) — wynik pętli jest polem Loadout, nie drugim formularzem
            // człowieka. Jedna automatyczna linia zapobiega dwóm sprzecznym umowom o tym samym
            // kluczu, kiedy starszy workflow miał już własne pole nazwane `outcome`.
            !asks_for_outcome || !field.name.trim().eq_ignore_ascii_case("outcome")
        }) {
            // `write!` do `String`, nie `push_str(&format!(…))` — ten sam powód, co przy indeksie
            // przekazań (clippy `format_push_string`), i ten sam `let _`: zapis do `String` nie
            // ma jak zawieść.
            let _ = write!(
                told.prompt,
                "\n{}: {}{}",
                field.name.trim(),
                field.describe.trim(),
                if field.required == Some(true) {
                    FIELD_IS_NEEDED
                } else {
                    ""
                }
            );
        }
        if asks_for_outcome {
            // `pass` i `fail` stoją w pokazanym kształcie odpowiedzi: model nie musi zgadywać
            // ani dozwolonych wartości, ani tego, że to pole jest wymagane w każdej rundzie.
            let _ = write!(told.prompt, "\noutcome: pass or fail{FIELD_IS_NEEDED}");
        }
        told.prompt.push_str("\n\n");
        told.prompt.push_str(FIELDS_ARE_REQUIRED);
    }

    /// Zdanie, którym blok nazywa limit czasu **tego** kroku.
    ///
    /// # Limit, o którym wie wyłącznie ten, kto zabija, jest karą, a nie ograniczeniem
    ///
    /// `give_up_after` odbiera krokowi robotę po czasie (`Live::one_turn` → [`Ended::Overdue`])
    /// i do 2026-08-23 nie wchodził do promptu ani jedną literą. Agent planował
    /// sześćdziesięciominutową robotę w kroku, który ma dziesięć minut, i ginął w połowie bez
    /// jednego zdania w tym, co przekazuje dalej — czyli bieg płacił za całą turę i nie dostawał
    /// z niej nic.
    ///
    /// # Liczba jest z definicji EFEKTYWNEJ, czyli po nadpisaniu na kroku
    ///
    /// Nie z samej definicji agenta: dla kroku, który niczego nie zawęża, obie odpowiadają tak
    /// samo, więc rozjazd nie ma jak się pokazać — a człowiek, który zawęził czas na panelu
    /// kroku, dostawałby agenta planującego pracę na trzy razy dłużej, niż mu wolno.
    ///
    /// Skojarzona, nie metoda na `&self`: odpowiedź zależy wyłącznie od argumentu, a `self`
    /// w podpisie sugerowałby, że gdzieś w biegu stoi drugie źródło tej liczby.
    fn how_long_this_step_has(minutes: u32) -> String {
        if minutes == 0 {
            return NO_TIME_LIMIT.to_owned();
        }
        format!(
            "You have {minutes} minutes for this step. When the time is up the step is stopped \
             where it stands and nothing of it reaches what comes next, so plan the work to fit \
             and answer while you still can."
        )
    }

    /// Lista ścieżek do tego, co zostawili poprzednicy tego kroku — plus prawo ich otwarcia.
    ///
    /// Wołana wyłącznie wtedy, gdy jest co wymienić: nagłówek nad zerem wpisów jest zdaniem
    /// o niczym (powód przy [`Live::prompt_for`]).
    fn index_of_what_came_before(&self, handed: &[Handed], told: &mut Told) -> anyhow::Result<()> {
        told.prompt.push_str("\n\n");
        told.prompt.push_str(HANDOFF_INDEX_OPENS);
        for hand in handed {
            let handoff_bytes = Self::published_handoff_bytes(hand)?;
            // `write!` do `String`, nie `push_str(&format!(…))`: ten drugi alokuje bufor
            // pośredni tylko po to, żeby go zaraz skopiować i wyrzucić (clippy
            // `format_push_string`). Zapis do `String` jest nieomylny — `fmt::Error` może
            // zwrócić tylko sam formatter — więc wynik idzie do `let _`, a nie do `expect()`,
            // który w tym drzewie jest `warn`, czyli pod `-D warnings` też fatalny.
            // ETYKIETA STOI W TYM SAMYM WIERSZU, CO ŚCIEŻKA, i to jest wymóg, nie układ: odnośnik
            // i to, czym on jest, czytane z dwóch osobnych list są dwiema listami do zestawienia
            // w głowie — a agent, który tego nie zrobi, otwiera wszystkie pliki po kolei.
            // 2026-08-28 (T-152 repair) — POWÓD PORAŻKI ZOSTAJE W TREŚCI PRZEKAZANIA. Powtórzony
            // tutaj rozpychał 50-znakową etykietę relacji do 163 znaków i łamał sufit T-87.
            let mut label = hand.what.said();
            if hand.left_nothing {
                label.push_str("; left nothing");
            }
            if let Some(full) = &hand.attachment {
                // Dopisek jest chwilową pomocą promptu, nie częścią przenośnego przekazania.
                // Stoi wewnątrz tej samej etykiety relacji, żeby zwykły następnik i wznowienie
                // zachowały swoje prawdziwe, różne pochodzenie (T-114, 2026-08-24).
                let _ = write!(label, "; full text: {}", full.display());
            }
            let _ = write!(
                told.prompt,
                "\n- {}: {} ({label})",
                hand.from,
                hand.path.display()
            );
            told.reads.push(self.filed_as(&hand.path));
            told.context.push(ContextSource {
                kind: ContextKind::Handoff,
                reference: told
                    .reads
                    .last()
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("a handoff lost its safe reference"))?,
                bytes: handoff_bytes,
            });
            // Jeden katalog na cały bieg, więc pętla dopisuje go raz — ale bierze go ze ścieżki,
            // a nie ze stałej: druga kopia nazwy `handoffs` byłaby drugim miejscem do poprawienia
            // w dniu, w którym `memory::handoff` zmieni nazwę katalogu, i tym niepoprawionym.
            if let Some(dir) = hand.path.parent()
                && !told.extra_dirs.iter().any(|had| had == dir)
            {
                told.extra_dirs.push(dir.to_owned());
            }
        }
        // 2026-08-20 — CIĘCIE PRZEKAZANIA ROBI DRUGI KATALOG, A PRAWA DOSTAWAŁ TYLKO PIERWSZY.
        // `memory::handoff` ucina ciało na `BODY_CAP`, pisze ORYGINAŁ do `attachments/` i wstawia
        // w ciało wiersz `Moved to attachments/<nazwa>__full.md`. Ten wiersz składa Loadout, nie
        // agent, więc krok dostawał od NAS odnośnik, którego nie wolno mu było otworzyć — czyli
        // dokładnie kontrolkę bez handlera z niezmiennika 16, przed którą ostrzega nagłówek tego
        // modułu („skoro ścieżka jest jedyną drogą do treści, to musi działać").
        //
        // Zmierzone na biegu `20260819-223942`: krok Analysis dostał trzy takie wskaźniki, nie
        // otworzył żadnego, napisał, że pełnego załącznika „nie ma", i wyliczył cały dowód po raz
        // drugi wprost z repo — 9 z 10 minut swojego limitu na pracę, która leżała gotowa obok.
        //
        // Warunek to ISTNIENIE KATALOGU, nie nazwa pliku składana tu po raz drugi: katalog
        // powstaje wyłącznie wtedy, gdy jakieś przekazanie tego biegu zostało ucięte, więc jego
        // obecność JEST tym pytaniem. Wersja z ponownym składaniem `<nazwa>__full.md` rozjechałaby
        // się po cichu z `handoff::write_inner` (ten sam powód stoi nad `Transcript`), a wersja
        // bezwarunkowa dawałaby `--add-dir` na ścieżkę, której nie ma — czyli zamieniałaby
        // nieczytelny załącznik w nieuruchomiony krok.
        for hand in handed {
            if let Some(attachments) = hand.attachment.as_ref().and_then(|path| path.parent())
                && !told.extra_dirs.iter().any(|had| had == attachments)
            {
                told.extra_dirs.push(attachments.to_path_buf());
            }
        }
        told.prompt.push_str("\n\n");
        told.prompt.push_str(HANDOFF_INDEX_CLOSES);
        Ok(())
    }

    /// Sprawdza pliki, które Loadout sam wystawił jako wejście kroku, zanim powstanie ich wiersz.
    ///
    /// 2026-09 (Z-41) — front-matter kotwiczy długość ciała przekazania, a `Written` długość
    /// pełnej kopii. Awaria odczytu znaczy dla kroku to samo: nie dostaje już bajtów, które
    /// poprzednik opublikował, więc nie wolno uruchomić go nad innym kontekstem.
    fn published_handoff_bytes(hand: &Handed) -> anyhow::Result<usize> {
        // ZNIKNIETY PLIK TO NIE ZMIENIONY PLIK (2026-09-05). Pierwsza wersja tej funkcji mapowala
        // KAZDA awarie na zdanie „Handoff … was changed after … published it", wiec krok, ktoremu
        // poprzednik USUNAL wynik, czytal na karcie nieprawde o tym, co sie stalo — a lawka
        // `context_failures_take_the_chosen_path` robi dokladnie to (sabotazysta „remove Source's
        // result") i przypina ogolne zdanie z T-101. Konkretne zdanie nalezy sie wylacznie
        // przypadkowi, w ktorym plik JEST, a jego tresc nie zgadza sie z tym, co opublikowano.
        let changed = || HandoffChangedAfterPublication::for_handed(hand);
        // 2026-09-08 — TA LINIA PRZYWRACALA WADE, KTORA KOMENTARZ WYZEJ OPISUJE JAKO NAPRAWIONA.
        // `map_err(|_| changed())` mapowal takze BRAK pliku, wiec krok, ktoremu poprzednik
        // usunal wynik, czytal na karcie „was changed after … published it" — nieprawde o tym,
        // co sie stalo. Zniknieciu nalezy sie zdanie ogolne (`CONTEXT_NOT_PROVEN`), bo tamten
        // przypadek nie wie, ktory plik i dlaczego; „zmieniony" nalezy sie wylacznie plikowi,
        // ktory JEST. Zlapane przy landowaniu WP-03 przez `a_missing_handoff_stops_the_step`.
        let bytes = regular_file_length(&hand.path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                anyhow::Error::new(error)
            } else {
                changed().into()
            }
        })?;
        let published = handoff::read_handoff(&hand.path).map_err(|_| changed())?;
        if published.bytes_mismatch() {
            return Err(changed().into());
        }

        if let Some(full) = &hand.attachment {
            let full_bytes = regular_file_length(full).map_err(|_| changed())?;
            if let Some(expected) = hand.attachment_bytes
                && full_bytes != expected
            {
                return Err(changed().into());
            }
        }
        Ok(bytes)
    }

    /// Dokłada zdanie o wyniku — **tylko sędziemu pętli**.
    ///
    /// Warunek jest ten sam, którego używa [`Live::verdict_after`] do czytania wyniku
    /// (`judging`), i to jest cała poprawność tego szwu: gdyby pytał inaczej, istniałby krok
    /// proszony o wiersz, którego nikt nie czyta, albo — gorzej — krok czytany bez pytania.
    /// Jedno pytanie, jedna odpowiedź, jeden warunek (niezmiennik 13).
    ///
    /// Zwykły krok nie dostaje ani bajtu więcej: prośba o wynik skierowana do kogoś, kto nie
    /// jest sędzią, jest poleceniem bez skutku, czyli tym samym, co kontrolka bez handlera.
    fn ask_for_an_outcome(&self, id: StepId, told: &mut Told) {
        /* V-02: ZATWIERDZONA LISTA WZMACNIA INSTRUKCJĘ TEGO KROKU, nie wszystkich użyć
         * uniwersalnego bloku o wyniku. Powstaje wyłącznie wtedy, kiedy człowiek naprawdę
         * zatwierdził wymagania — krok bez listy pyta dokładnie tym samym zdaniem, co dotąd.
         *
         * Blok wchodzi także do kroku, którego nikt nie sądzi pętlą: wymagania są umową
         * o tym, co ma być potwierdzone, a nie mechanizmem domykania rundy. */
        let approved = match &self.plan.steps[id].job {
            Job::Agent(job) => job.criteria.clone(),
            Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => Vec::new(),
        };
        if !approved.is_empty() {
            told.prompt.push_str("\n\n");
            told.prompt
                .push_str(&crate::workflow::criteria::asked_for(&approved));
            told.prompt.push('\n');
        }
        if self.judging(&self.plan.steps[id]).is_none() {
            return;
        }
        told.prompt.push_str("\n\n");
        told.prompt.push_str(OUTCOME_ASKED_FOR);
        told.prompt.push('\n');
    }

    /// Przekazania kroków, po których idzie ten krok — **w kolejności z grafu**.
    ///
    /// Kolejnością jest pozycja kroku w pliku workflow, i to nie jest wybór kosmetyczny: druga
    /// możliwa kolejność — chwila zakończenia — zmienia się z biegu na bieg, bo zależy od tego,
    /// który agent akurat odpowiedział szybciej. Prompt, który dwa razy z rzędu wygląda inaczej,
    /// jest promptem, którego nie da się z niczym porównać, a przy trzech poprzednikach to
    /// przestaje być teorią. Pozycja w pliku jest przy tym tą samą liczbą, którą niesie prefiks
    /// nazwy pliku przekazania i wiersz w `run.json`, więc indeks w prompcie i `ls handoffs/`
    /// czyta się w jednym porządku.
    ///
    /// Poprzednik, który nic nie oddał, **wypada z listy**: kafelek kontrolny nie oddaje nigdy,
    /// a wpis bez pliku byłby ścieżką, której agent nie ma jak otworzyć.
    ///
    /// # 2026-08-23 (T-87): runda pętli nie jest krokiem, który zaczyna od zera
    ///
    /// Do tego dnia ta funkcja brała WYŁĄCZNIE bezpośrednich poprzedników po strzałce, a jedynym
    /// poprzednikiem rundy k+1 kroku roboczego jest powrót od sędziego. Agent poprawiający dostawał
    /// więc jedno zdanie krytyki i **nic więcej**: ani planu, od którego zaczął, ani własnej
    /// poprzedniej odpowiedzi, którą miał poprawić. Zmierzone w biegu `20260823-145648`: `s_2#1`
    /// dostał tylko `12__verification-1`, a `s_2#2` tylko `13__verification-1` — w czterech biegach
    /// dwie z trzech pętli nie zbiegły się ani razu, dziewięć rund i zero przejść. Trudno się
    /// dziwić: każda runda zaczynała od pustej kartki.
    ///
    /// Krok spoza pętli i runda ZEROWA dostają dokładnie to, co dostawały: pierwsza runda nie ma
    /// czego pamiętać, a dokładanie jej odnośnika do pliku, którego jeszcze nie ma, byłoby
    /// ścieżką bez pliku po drugiej stronie.
    ///
    /// SORTOWANIE PO NUMERZE KROKU JEST SORTOWANIEM PO (POZYCJA W PLIKU, RUNDA, KOPIA) — `unroll`
    /// emituje węzły w kolejności z pliku, a kopie każdej rundy jedna za drugą. Dzięki temu kolejność
    /// indeksu nie zależy od tego, kto skończył pierwszy, i czyta się tak samo jak `ls handoffs/`.
    fn handed_before(&self, id: StepId) -> Vec<Handed> {
        // Migawka pod jednym zamkiem, bez ani jednego `await` w środku (niezmiennik 8). Kopia
        // całego wektora, a nie zamek trzymany przez resztę funkcji: `what_that_loop_produced`
        // pyta o te same przekazania, a `std::sync::Mutex` nie jest wznawialny.
        let filed: Vec<Option<handoff::Written>> = self
            .handoffs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let unpassed: Vec<Option<String>> = self
            .did_not_pass
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();

        let mut wanted: Vec<StepId> = Vec::new();
        for parent in ends(&self.plan.arrows, |&(parent, child)| {
            (child == id).then_some(parent)
        }) {
            match self.leaving_a_loop(parent, id) {
                Some(which) => wanted.extend(self.what_that_loop_produced(which, &filed)),
                None => wanted.push(parent),
            }
        }
        wanted.extend(self.what_this_try_already_knows(id, &filed));
        wanted.sort_unstable();
        wanted.dedup();

        /* PRZEJĘTE PO POPRZEDNIM BIEGU STOI PIERWSZE, i to jest kolejność w czasie, nie gust:
         * tamten bieg wydarzył się wcześniej w całości. Wiersz „what an earlier run left here"
         * wmieszany między świeże czytałby się jak jeszcze jedna gałąź tego biegu. */
        let mut index: Vec<Handed> = self
            .plan
            .carried
            .get(id)
            .into_iter()
            .flatten()
            .map(|one| Handed {
                from: one.from.clone(),
                path: one.path.clone(),
                attachment: one.attachment.clone(),
                attachment_bytes: None,
                // `run.json` starszego biegu nie jest tu wczytany ze stanem każdego kroku.
                // Bez dowodu sukcesu nie nazywamy przejętej pustki udanym wynikiem.
                left_nothing: false,
                what: WhatItIs::FromAnEarlierRun,
            })
            .collect();
        index.extend(wanted.into_iter().filter_map(|step| {
            if self
                .plan
                .inputs
                .configuration
                .context_key(&self.plan.steps[id].tile_key)
                != self
                    .plan
                    .inputs
                    .configuration
                    .context_key(&self.plan.steps.get(step)?.tile_key)
            {
                return None;
            }
            let written = filed.get(step)?.as_ref()?;
            let succeeded = unpassed.get(step).is_none_or(Option::is_none);
            Some(Handed {
                from: self.plan.steps.get(step)?.name.clone(),
                path: written.path.clone(),
                attachment: written.attachment.clone(),
                attachment_bytes: written.attachment_bytes,
                left_nothing: succeeded && written.left_nothing,
                what: self.what_it_is(id, step, &unpassed),
            })
        }));
        index
    }

    /// Numer pętli, z której WYCHODZI ta strzałka. `None`, kiedy nie wychodzi z żadnej.
    ///
    /// Strzałka wewnątrz jednej pętli — z rundy k do rundy k tego samego ciała — nie wychodzi
    /// nigdzie, więc krok, który ją czyta, dostaje zwykłego poprzednika.
    fn leaving_a_loop(&self, parent: StepId, child: StepId) -> Option<usize> {
        let which = self.plan.steps.get(parent)?.in_loop?;
        (self.plan.steps.get(child)?.in_loop != Some(which)).then_some(which)
    }

    /// Ostatnie przekazanie, jakie NAPRAWDĘ wyprodukowała każda kopia kroku tej pętli.
    ///
    /// 2026-08-23 (T-87) — TO JEST NAPRAWA FAN-INU, ZMIERZONA NA BIEGU WŁAŚCICIELA. Strzałka
    /// z pętli na zewnątrz wychodzi z rundy OSTATNIEJ (`workflow::unroll`), a rundy po tej,
    /// w której padł werdykt `pass`, są pomijane bez sterownika ([`Live::not_run_because`])
    /// i nie oddają nic. Krok za pętlą wisiał więc na węźle, który z definicji nie napisał ani
    /// słowa: w biegu `20260823-145648` synteza z TRZEMA strzałkami wchodzącymi dostała dwa
    /// pliki, obie krytyki negatywne, i **zero** z gałęzi, które przeszły. Design
    /// i Implementation tego biegu powstały na syntezie, która widziała same odmowy.
    ///
    /// „Ostatnie wyprodukowane", nie „ostatnia runda": to jest cała różnica między gałęzią, która
    /// przeszła w rundzie pierwszej, a tą, która przepaliła wszystkie trzy.
    ///
    /// CAŁE CIAŁO, nie sam sędzia: pętla oddaje dalej robotę **i** to, co o niej orzeczono.
    /// Sam werdykt bez pracy jest recenzją bez recenzowanego, a sama praca bez werdyktu nie mówi,
    /// czy ktokolwiek ją przyjął.
    fn what_that_loop_produced(
        &self,
        which: usize,
        filed: &[Option<handoff::Written>],
    ) -> Vec<StepId> {
        let Some(the_loop) = self.plan.loops.get(which) else {
            return Vec::new();
        };
        // 2026-09-05 (WF-05): ostatni węzeł kafelka był ostatnią KOPIĄ, nie wynikiem
        // wszystkich kopii. Work key zachowuje kopię, usuwając wyłącznie numer rundy.
        let mut latest: BTreeMap<&str, (u8, StepId)> = BTreeMap::new();
        for (at, step) in self.plan.steps.iter().enumerate() {
            if step.in_loop != Some(which)
                || !the_loop.body.contains(&step.tile_key)
                || !filed.get(at).is_some_and(Option::is_some)
            {
                continue;
            }
            let last = latest
                .entry(work_key_of(&step.node_key))
                .or_insert((step.turn, at));
            if step.turn > last.0 {
                *last = (step.turn, at);
            }
        }
        latest.into_values().map(|(_, at)| at).collect()
    }

    /// Co ta runda już wie — a czego dziś nie widziała: wejście pętli, wcześniejsze próby pracy,
    /// własne wcześniejsze odpowiedzi i wcześniejsze werdykty sędziego.
    ///
    /// Pusta lista dla kroku spoza pętli i dla rundy zerowej. Numery kroków, nie ścieżki: filtr
    /// „a czy ten krok cokolwiek oddał" stoi jeden, w [`Live::handed_before`].
    ///
    /// WEJŚCIE PĘTLI ROZWIĄZUJEMY TĄ SAMĄ DROGĄ, CO [`Live::handed_before`] — czyli przez
    /// [`Live::leaving_a_loop`], nie po literalnym rodzicu z grafu. Pętla poprzedzająca oddaje
    /// dalej rundą OSTATNIĄ (`workflow::unroll`), a ta po werdykcie `pass` nie biegnie wcale, więc
    /// runda pierwsza kolejnej pętli dostawała wejście przez [`Live::what_that_loop_produced`],
    /// a jej runda druga — literalny węzeł bez pliku, czyli **nic**. Ten sam fakt liczony dwoma
    /// drogami odpowiadałby dwiema różnymi listami (niezmiennik 13), a różnicę widać dopiero
    /// w drugiej rundzie drugiej pętli — czyli tam, gdzie nikt nie patrzy.
    ///
    /// Dlatego ta funkcja bierze `filed`: „ostatnie WYPRODUKOWANE przekazanie" jest pytaniem
    /// o pliki, a migawkę robi wywołujący, pod jednym zamkiem i bez `await` (niezmiennik 8).
    fn what_this_try_already_knows(
        &self,
        id: StepId,
        filed: &[Option<handoff::Written>],
    ) -> Vec<StepId> {
        let Some(step) = self.plan.steps.get(id) else {
            return Vec::new();
        };
        let Some(which) = step.in_loop else {
            return Vec::new();
        };
        let Some(the_loop) = self.plan.loops.get(which) else {
            return Vec::new();
        };
        if step.turn == 0 {
            return Vec::new();
        }

        // Wejście pętli: to, co dostała jej PIERWSZA runda. Liczone z grafu, a nie zapamiętane
        // przy tamtym kroku, bo pętla zaczyna się raz i jej wejście się nie zmienia.
        let mut knows: Vec<StepId> = Vec::new();
        for entry in self.nodes_of(&the_loop.entry, 0) {
            for parent in ends(&self.plan.arrows, |&(parent, child)| {
                (child == entry).then_some(parent)
            }) {
                // Powrót od sędziego TEJ pętli nie jest jej wejściem — własne rundy dokłada
                // pętla niżej, i to one niosą numer próby.
                let from_this_loop = self
                    .plan
                    .steps
                    .get(parent)
                    .is_some_and(|before| before.in_loop == Some(which));
                if from_this_loop {
                    continue;
                }
                match self.leaving_a_loop(parent, entry) {
                    Some(other) => knows.extend(self.what_that_loop_produced(other, filed)),
                    None => knows.push(parent),
                }
            }
        }

        // Sędzia dostaje KAŻDĄ wcześniejszą próbę kroku, do którego wraca pętla, a pozostali
        // dostają własne wcześniejsze odpowiedzi. Obie strony dostają wszystkie wcześniejsze
        // werdykty sędziego. Implementacja niosąca tylko rundę tuż przed tą gubi pierwszą próbę
        // w całości, więc nie da się odróżnić poprawki od tego samego błędu opisanego inaczej.
        for turn in 0..step.turn {
            if step.tile_key == the_loop.judge {
                knows.extend(self.nodes_of(&the_loop.entry, turn));
            } else {
                // Historia własna nie jest historią pierwszej znalezionej kopii kafelka.
                knows.extend(self.nodes_of(&step.tile_key, turn).filter(|&at| {
                    work_key_of(&self.plan.steps[at].node_key) == work_key_of(&step.node_key)
                }));
            }
            knows.extend(self.nodes_of(&the_loop.judge, turn));
        }
        knows
    }

    /// Wszystkie kopie kafelka w tej rundzie. Wybór jednej wymaga dodatkowo work key.
    fn nodes_of<'a>(&'a self, tile: &'a str, turn: u8) -> impl Iterator<Item = StepId> + 'a {
        self.plan
            .steps
            .iter()
            .enumerate()
            .filter_map(move |(at, step)| {
                (step.tile_key == tile && step.turn == turn).then_some(at)
            })
    }

    /// Czym jest plik `from` dla kroku `id` — jedno miejsce z odpowiedzią na to pytanie.
    ///
    /// Kolejność warunków jest treścią. „Nie przeszedł" wygrywa ze wszystkim: materiał, którego
    /// nikt nie przyjął, ma być rozpoznawalny niezależnie od tego, skąd przyszedł. Potem pytamy
    /// o pętlę, i tylko dla rund POZA pierwszą — runda zerowa czyta swoich poprzedników dokładnie
    /// tak, jak każdy krok spoza pętli.
    fn what_it_is(&self, id: StepId, from: StepId, unpassed: &[Option<String>]) -> WhatItIs {
        if unpassed.get(from).is_some_and(Option::is_some) {
            return WhatItIs::StepThatFailed;
        }
        let (Some(step), Some(before)) = (self.plan.steps.get(id), self.plan.steps.get(from))
        else {
            return WhatItIs::StepBefore;
        };
        let Some(the_loop) = step.in_loop.and_then(|which| self.plan.loops.get(which)) else {
            return WhatItIs::StepBefore;
        };
        if step.turn == 0 {
            return WhatItIs::StepBefore;
        }
        // Numer próby od jedynki: `turn` jest polem danych, a to jest zdanie dla czytającego.
        let which = before.turn.saturating_add(1);
        let of = the_loop.turns;
        if work_key_of(&before.node_key) == work_key_of(&step.node_key) {
            return WhatItIs::YourOwnTry { which, of };
        }
        if step.tile_key == the_loop.judge
            && before.tile_key == the_loop.entry
            && before.turn < step.turn
        {
            return WhatItIs::EarlierTryOfTheWork { which, of };
        }
        if before.tile_key == the_loop.judge {
            return WhatItIs::WhatTheTesterSaid { which, of };
        }
        if before.in_loop != step.in_loop {
            return WhatItIs::WhatYouStartedWith;
        }
        WhatItIs::StepBefore
    }

    /// Ścieżka przekazania widziana **z katalogu biegu**, czyli tak, jak zapisuje ją plik.
    ///
    /// Bezwzględna ścieżka spoza tego katalogu byłaby w `reads` zapisem prawdziwym dokładnie do
    /// pierwszego przeniesienia katalogu biegu; nazwa pliku sama w sobie zostaje, kiedy `dir`
    /// nie jest przedrostkiem — a to znaczy, że przekazanie leży gdzieś, gdzie tego biegu nie ma.
    fn filed_as(&self, path: &Path) -> String {
        path.strip_prefix(&self.plan.dir)
            .unwrap_or(path)
            .display()
            .to_string()
    }

    /// Odnotowuje, gdzie leży przekazanie tego kroku i jego pełna kopia, jeśli powstała.
    ///
    /// Zamek powstaje i ginie w jednym wyrażeniu, bez `await` w środku (niezmiennik 8).
    fn filed(&self, id: StepId, written: handoff::Written) {
        if let Some(slot) = self
            .handoffs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(id)
        {
            *slot = Some(written);
        }
    }

    /// Wynik kroku → plik przekazania w `handoffs/`.
    ///
    /// **Front-matter składa Loadout, ciałem jest to, co oddał agent** (`ARCHITECTURE` §8,
    /// [T6 §10.2]). Ciała nie parsujemy i nie czyścimy: blok `---`, który model wkleił do swojej
    /// odpowiedzi, ma zostać tam, gdzie go postawił, bo jest jedynym śladem próby, na którą
    /// człowiek może zareagować. Wszystkie siedem pól niżej pochodzi z pliku workflow albo z tego
    /// biegu — ani jedno z tekstu, który przyszedł od modelu.
    ///
    /// Nieudany zapis **loguje się i nie przewraca kroku**, tą samą decyzją, co zrzut `run.json`
    /// w locie ([`Live::update`]): tura jest już zapłacona, a jej wynik jest dalej prawdziwy.
    /// Cena tej decyzji stoi w dzienniku wprost — następny krok dostaje wtedy prompt bez tego
    /// odnośnika.
    fn hand_over(&self, id: StepId, said: &str, reads: &[String]) {
        let step = &self.plan.steps[id];
        let draft = MetaDraft {
            run: self.plan.id.clone(),
            // Numer Loadouta, ten sam, którym ten krok nazywa się w `run.json` i w
            // `RunReport::steps`. Liczenie od jedynki byłoby drugą numeracją kroków, żyjącą
            // wyłącznie w nazwach plików — a wtedy `handoffs/03__…` i czwarty wiersz na ekranie
            // są tym samym krokiem tylko dla kogoś, kto zna przesunięcie.
            step: u32::try_from(id).unwrap_or(u32::MAX),
            from: step.name.clone(),
            to: ends(&self.plan.arrows, |&(parent, child)| {
                (parent == id).then_some(child)
            })
            .into_iter()
            .filter_map(|child| self.plan.steps.get(child).map(|step| step.name.clone()))
            .collect(),
            // Jeden rodzaj dla każdego kroku, i to jest niezmiennik 27 zapisany w danych: silnik
            // nie zna pojęcia „recenzja", więc nie ma jak nazwać wyniku kroku inaczej dlatego,
            // że ten krok recenzował. `findings` jest tym, czym `docs/ARCHITECTURE.md` §8 nazywa
            // wynik kroku w swoim własnym przykładzie (`02__research-auth__findings.md`).
            kind: Kind::Findings,
            title: title_of(step),
            reads: reads.to_vec(),
        };

        let directory = self
            .plan
            .inputs
            .configuration
            .context_key(&step.tile_key)
            .map_or_else(
                || self.plan.dir.clone(),
                |scope| self.plan.dir.join("context").join(scope),
            );
        match handoff::write_handoff(&directory, draft, said) {
            Ok(written) => {
                // 2026-08-23 (T-86) — DO TEGO DNIA STAŁO TU `tracing::debug!` I TYLE.
                //
                // Licznik, który warto oglądać [T6 §11.1], szedł na poziom, którego aplikacja
                // nie ma włączonego — czyli nie widział go nikt (niezmiennik 21). Teraz jedzie
                // do `run.json`, bo to jedyny zapis biegu, który przeżywa skasowanie
                // `loadout.db` (niezmiennik 4).
                //
                // Zapisujemy BEZWARUNKOWO, także kształt umówiony: `update` jest jedyną drogą
                // do księgi, a warunek postawiony tutaj zostawiałby w niej wartość z poprzedniej
                // rundy pętli. Klucze znikają dopiero przy serializacji ([`StepEntry::repaired`]),
                // czyli w miejscu, które o długość pliku naprawdę pyta.
                //
                // I dla KAŻDEGO kroku, który cokolwiek oddaje — także dla wyjścia komendy i dla
                // zdania człowieka z kafelka kontrolnego. Zawężenie do kroków agenta byłoby
                // warunkiem, o który nie prosi żadne kryterium, i chowałoby prawdziwy fakt:
                // następny krok dostaje wskaźnik na plik, który `reshape()` przepisał, niezależnie
                // od tego, kto ten tekst napisał.
                self.update(|book| {
                    let step = &mut book.steps[id];
                    step.repaired = written
                        .repaired
                        .iter()
                        .map(|section| section.name().to_owned())
                        .collect();
                    step.truncated = written.truncated;
                });
                self.filed(id, written);
            }
            Err(error) => tracing::error!(
                run = %self.plan.id,
                step = id,
                %error,
                "this step's result could not be handed over, so the next step is not told about it"
            ),
        }
    }

    /// Odpowiedź kroku → plik pod ścieżką, którą wskazał człowiek. `Ok(())`, kiedy nie wskazał.
    ///
    /// # Zapisuje LOADOUT, nie agent, i to jest cała treść tego szwu
    ///
    /// Blok „jak odpowiadać" mówi każdemu krokowi wprost: *„Do not write your results to a
    /// file"* ([`HOW_TO_ANSWER`]). Gdyby tę ścieżkę miał obsłużyć agent, produkt kazałby mu
    /// robić dokładnie to, czego przed chwilą zabronił — a krok z dialem „look only" nie umiałby
    /// tego wykonać i spaliłby turę na próbie. Zmierzone na biegu `20260823-145648`: sześć kroków
    /// Claude'a zaczyna podsumowanie od zdania o tym, że nie mogą utworzyć pliku.
    ///
    /// **Cała odpowiedź, co do bajtu**, nie podsumowanie: plik z jedną linią wygląda jak wynik
    /// i nim nie jest, a nikt nie porównuje go z transkryptem, którego nikt nie trzyma.
    ///
    /// **To jest KOPIA, nie zamiana.** Przekazanie w `handoffs/` powstaje jak dotąd
    /// ([`Live::hand_over`]) i to na nim stoi indeks następnego kroku — zapis, który by je
    /// zastąpił, uciszyłby cały ruch między krokami, a bieg dalej wyglądałby na udany.
    ///
    /// # Nieudany zapis czyni krok NIEPRZESZŁYM — inaczej niż nieudane przekazanie
    ///
    /// Tamto ma drugą drogę do człowieka (wiersz na ekranie, podsumowanie kroku); tutaj plik
    /// JEST tym, o co człowiek poprosił, i jego brak jest nieodróżnialny od agenta, który nie
    /// miał nic do powiedzenia. Wraca więc zdaniem, a rozstrzyga je jedno miejsce, przez które
    /// przechodzi każda porażka ([`Live::when_this_one_fails`]).
    ///
    /// # I PYTA O MIEJSCE DRUGI RAZ, choć zapytał już przed startem
    ///
    /// [`Live::the_answer_has_somewhere_to_go`] stawia to samo pytanie, zanim ruszy pierwszy
    /// proces — i to tamto jest odmową, o którą prosi kryterium. To tutaj zostaje mimo niego,
    /// bo między jednym a drugim stoi CAŁA tura agenta, który miał prawo pisać po tym samym
    /// folderze. Sprawdzenie zdjęte stąd zamieniłoby odmowę w wyścig: dowiązanie podłożone
    /// w tym oknie przepuściłoby zapis po drugiej stronie, a bieg wyglądałby na udany.
    fn file_the_answer(&self, id: StepId, said: &str) -> Result<(), String> {
        let Job::Agent(job) = &self.plan.steps[id].job else {
            return Ok(());
        };
        let Some(under) = job.write_results_to.as_ref() else {
            return Ok(());
        };
        Self::room_for_the_answer(&job.cwd, under)
            .and_then(|path| fs::write(&path, said))
            .map_err(|error| {
                format!(
                    "Loadout could not put this step's answer where {WRITE_RESULTS_TO} points \
                     (\"{}\"): {error}",
                    under.display()
                )
            })
    }

    /// Miejsce na odpowiedź: katalogi po drodze założone, ścieżka sprawdzona, plik jeszcze nie
    /// zapisany.
    ///
    /// Oddaje ścieżkę, pod którą wolno pisać, albo błąd nazywający powód. Wołają to dwa miejsca
    /// — [`Live::the_answer_has_somewhere_to_go`] przed startem kroku i [`Live::file_the_answer`]
    /// tuż przed zapisem — i to jest jedna funkcja z rozmysłem: druga kopia tej decyzji
    /// rozjechałaby się z pierwszą (niezmiennik 13), a rozjazd znaczyłby tu odmowę przed startem
    /// dla jednych ścieżek i cichy zapis poza folderem dla innych.
    fn room_for_the_answer(cwd: &Path, under: &Path) -> io::Result<PathBuf> {
        let path = cwd.join(under);
        let root = cwd.canonicalize()?;
        if let Some(parent) = path.parent() {
            // Katalogi po drodze zakłada Loadout: implementacja, która tego nie robi,
            // wygląda jak zapis, który się nie udał i nic o tym nie powiedział.
            fs::create_dir_all(parent)?;
            /* DOWIĄZANIE PYTAMY JĄDRO, nie napis. `..` odmawia planista, ale katalog
             * `results` mógł już wcześniej być dowiązaniem gdzie indziej — i wtedy ścieżka
             * czysta co do znaków wyprowadza z folderu kroku tak samo skutecznie. */
            if !parent.canonicalize()?.starts_with(&root) {
                return Err(io::Error::other(LEADS_OUT_OF_THE_FOLDER));
            }
        }
        /* I TO SAMO PYTANIE O OSTATNI CZŁON, bo katalog nad nim bywa czysty. `results/`
         * leży dokładnie tam, gdzie ma leżeć, a `results/report.md` jest dowiązaniem do
         * pliku człowieka poza folderem kroku — `fs::write` idzie po dowiązaniu i zapisuje
         * TAM. Sprawdzenie samego katalogu przepuszcza więc dokładnie tę ucieczkę, przed
         * którą stoi, i to bez ani jednego znaku `..` w ścieżce.
         *
         * `symlink_metadata`, nie `exists`: pytanie brzmi „czy coś tu już leży", a nie „czy
         * po dowiązaniu coś jest" — i tylko ono odróżnia wolne miejsce od dowiązania
         * wiszącego, po którym `fs::write` też pisze.
         *
         * DOWIĄZANIA NIE ZDEJMUJEMY. Skasowanie go i zapis w jego miejsce chroni cudzy plik
         * i niszczy po cichu to, co człowiek sam sobie w tym folderze ustawił. */
        match path.symlink_metadata() {
            // Wolne miejsce: plik powstanie w katalogu, o który już zapytaliśmy wyżej.
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
            Ok(here) => {
                let real = path.canonicalize().map_err(|error| {
                    if error.kind() == io::ErrorKind::NotFound && here.is_symlink() {
                        io::Error::other(
                            "that path is a link to something that is not there, so there is \
                             no telling where the answer would land",
                        )
                    } else {
                        error
                    }
                })?;
                if !real.starts_with(&root) {
                    return Err(io::Error::other(LEADS_OUT_OF_THE_FOLDER));
                }
            }
        }
        Ok(path)
    }

    /// Czy odpowiedź tego kroku ma dokąd pójść — pytanie postawione PRZED pierwszym procesem.
    ///
    /// # Odmowa jest przed startem, i to jest treść, nie kolejność
    ///
    /// Napis z `..` i ścieżkę bezwzględną odrzuca planista ([`where_results_go`]), patrząc na
    /// same znaki. Dowiązania z napisu nie widać: `results/report.md` jest ścieżką czystą co do
    /// znaków i wyprowadza z folderu kroku dokładnie tak samo skutecznie, kiedy ktoś położył tam
    /// wcześniej link do cudzego pliku. To pytanie umie postawić tylko jądro i tylko na dysku —
    /// więc stoi tutaj, w ostatniej chwili, w której nic jeszcze nie ruszyło.
    ///
    /// Zadane dopiero przy zapisie byłoby odmową PO turze, za którą człowiek zapłacił, i po
    /// krokach, które zdążyły dotknąć jego plików — czyli dokładnie tym, czego zakazuje
    /// niezmiennik 12.
    ///
    /// Krok bez wskazanej ścieżki nie prosi o żaden plik, więc nie ma tu czego sprawdzać.
    fn the_answer_has_somewhere_to_go(job: &AgentJob) -> Result<(), String> {
        if let Some(under) = job.write_results_to.as_ref() {
            Self::room_for_the_answer(&job.cwd, under).map_err(|error| {
                format!(
                    "Loadout did not start this step: {WRITE_RESULTS_TO} points at \"{}\", and \
                     {error}.",
                    under.display()
                )
            })?;
        }
        if job.work_plan.writes_candidate() {
            Self::room_for_the_answer(&job.cwd, &job.plan_candidate).map_err(|error| {
                format!(
                    "Loadout did not start this step because its plan candidate path could not \
                     be kept inside the working folder: {error}."
                )
            })?;
        }
        Ok(())
    }

    /// Kafelek kontrolny: bieg staje i pyta człowieka (T3 §6.1 reguła 5).
    ///
    /// Stoi **bieg**, nie krok: `paused` jest stanem biegu i nie ma go w maszynie stanów kroku
    /// (`docs/ARCHITECTURE.md` §5). Nic za pytaniem nie startuje, bo dopóki ten krok nie wróci
    /// z `Succeeded`, planista nie zdejmuje stopnia wejściowego jego potomkom — a pytanie, które
    /// pojawia się po tym, jak agent już zrobił swoje, nie jest pytaniem.
    async fn wait_for_a_person(&self, id: StepId, question: Option<&str>) -> StepReport {
        // Nasłuch PRZED ogłoszeniem pauzy. Powód stoi przy `RunControl::listen_for_go_on`:
        // odpowiedź przychodzi w reakcji na to, co widać na dysku, więc kolejność odwrotna ma
        // okno, w którym Continue trafia do nikogo i bieg stoi już na zawsze.
        let step = &self.plan.steps[id];
        let listening = super::checkpoint::park(
            &self.control,
            step.node_key.clone(),
            question.unwrap_or(&step.name).to_owned(),
        );
        let info = listening.info.clone();
        // Fakt „bieg stoi" ma jednego właściciela — [`RunControl`] — a wpis w `run.json` jest
        // jego trwałym lustrem: stan, który nie dociera na dysk, nie przeżywa awarii aplikacji
        // (niezmiennik 4), a stan, który istnieje wyłącznie na dysku, nie da się o nic zapytać
        // z drugiej strony okna.
        self.control.pause();
        self.update(|book| {
            book.asking = true;
            // `true`, bo pytanie stoi tu i teraz; drugi powód czyta [`run_stands_or_moves`].
            run_stands_or_moves(book, book.asking);
        });
        self.ask(id, &info);

        if let Some(said) = listening.wait().await {
            if let Some(run) = self.control.run_address() {
                send_batch(
                    &self.lines,
                    vec![Line::QuestionAnswered {
                        agent: step.name.clone(),
                        run_id: run.id,
                        checkpoint_id: info.checkpoint_id,
                        answer: said.clone(),
                    }],
                );
            }
            let other_questions = !super::checkpoint::list(&self.control).is_empty();
            if !other_questions {
                self.control.resume();
            }
            /* ODPOWIEDŹ CZŁOWIEKA STAJE SIĘ PRZEKAZANIEM TEGO KROKU.
             *
             * To jest jedyne uczciwe miejsce, w które może pójść: kafelek kontrolny nie woła
             * żadnego agenta, więc nie ma komu jej „wysłać" — a krok, który idzie PO nim, i tak
             * czyta przekazania swoich rodziców (`hand_over`, indeks przekazań w prompcie).
             * Zdanie człowieka wchodzi więc do pracy tą samą drogą, którą wchodzi wynik agenta,
             * i widać je w `handoffs/` razem z resztą biegu (niezmiennik 4: pliki są prawdą).
             *
             * Nic nie piszemy, kiedy człowiek nie napisał nic: puste przekazanie dołożyłoby
             * do promptu następnego kroku nagłówek nad pustką, czyli kosztowałoby długość
             * za informację, której nie ma. */
            if !said.trim().is_empty() {
                if self.has_routes(id) {
                    self.remember_evidence(id, RouteEvidence::Checkpoint(said.clone()));
                }
                self.hand_over(id, &said, &[]);
            }
            // Limit dostawcy mógł wejść W TRAKCIE pytania i wtedy odpowiedź człowieka nie
            // wznawia niczego: bieg dalej stoi, tylko już z innego powodu.
            let still = !self.gate.still_paused_for().is_zero();
            self.update(|book| {
                book.asking = other_questions;
                run_stands_or_moves(book, still || other_questions);
            });
            StepReport::Succeeded
        } else {
            self.control.resume();
            // Stop przy pytaniu. Krok jest `cancelled`, a jego potomkowie też — nie `skipped`,
            // bo nikt nie padł: człowiek powiedział stop (ARCHITECTURE §5). Statusu biegu nie
            // ruszamy, bo bieg nie rusza dalej — zamyka go `close_the_book`. Gaśnie samo
            // `asking`, żeby krok kończący się obok nie ogłosił pauzy, której już nie ma.
            self.update(|book| book.asking = false);
            StepReport::Cancelled
        }
    }

    /// Pytanie na ekran.
    ///
    /// Wiersz powstaje tutaj, a nie w kuratorze, bo punkt kontrolny nie jest zdarzeniem agenta —
    /// jest kafelkiem w pliku workflow. To ta sama droga, którą `Line::Run` i `Line::Step`
    /// dokłada planista (`engine::line`, nagłówek [`Line`]). Bez tego wiersza pole `question`
    /// nie miałoby ani jednego czytelnika, a pytanie, którego nie widać, zatrzymuje bieg bez
    /// powodu widocznego dla człowieka.
    ///
    /// 2026-08-17 — synchroniczna, odkąd wiersz jedzie do pompy przez `try_send`. `async fn`
    /// bez ani jednego `await` w środku jest czerwony u `clippy::unused_async`, a udawane
    /// czekanie przed pytaniem byłoby jedynym miejscem w tym pliku, w którym punkt kontrolny
    /// zależy od tego, czy okno nadąża.
    fn ask(&self, id: StepId, question: &super::checkpoint::CheckpointInfo) {
        let step = &self.plan.steps[id];
        let line = Line::Asked {
            agent: step.name.clone(),
            question: Some(crate::engine::line::QuestionAddress {
                question_id: question.checkpoint_id.clone(),
                checkpoint_id: Some(question.checkpoint_id.clone()),
                run_id: self.control.run_address().map(|run| run.id),
                operation: "continue_run".to_owned(),
            }),
            // Kafelek bez wpisanego pytania mówi swoją nazwą — ona też jest zdaniem, które
            // napisał człowiek.
            text: question.question.clone(),
            // Warianty odpowiedzi są polem kroku dopiero w T3 §7.1; pusta lista znaczy
            // „odpowiedz własnymi słowami", nie „pytanie bez treści".
            options: question.options.clone(),
        };
        send_batch(&self.lines, vec![line]);
    }

    /// Nazywa kroki zatrzymane sufitem i ich pominięty stożek stanem `skipped`.
    ///
    /// Planista widzi porażkę, bo to ona pozwala `whenItFails` rozstrzygnąć, czy stożek ma stanąć
    /// czy jechać dalej. Dla człowieka ta porażka nie jest jednak awarią agenta ani Stopem — jest
    /// postawioną wcześniej granicą. Tłumaczenie stoi tutaj, po planiście i przed jednym zapisem
    /// księgi, a dokładne zdanie rodzica idzie wyłącznie przez jego faktycznie pominięte dzieci.
    fn name_what_the_budget_stopped(&self, states: &mut [StepState]) {
        let mut stopped = self
            .stopped_by_the_budget
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let roots: Vec<(StepId, String)> = stopped
            .iter()
            .enumerate()
            .filter_map(|(id, why)| why.clone().map(|why| (id, why)))
            .collect();
        for (root, why) in roots {
            let mut stack = vec![root];
            while let Some(from) = stack.pop() {
                for &(parent, child) in &self.plan.arrows {
                    if parent != from || states.get(child) != Some(&StepState::Skipped) {
                        continue;
                    }
                    let Some(slot) = stopped.get_mut(child) else {
                        continue;
                    };
                    if slot.is_some() {
                        continue;
                    }
                    *slot = Some(why.clone());
                    stack.push(child);
                }
            }
        }
        for (state, why) in states.iter_mut().zip(stopped.iter()) {
            if why.is_some() {
                *state = StepState::Skipped;
            }
        }
    }

    /// Zamyka księgę stanami **od planisty**.
    ///
    /// Stany bierzemy stamtąd, a nie z tego, co zapisały same kroki, bo tylko planista wie
    /// o stożku: krok, który nigdy nie ruszył, bo ktoś wyżej padł albo bo bieg zatrzymano, ma
    /// tu swój powód (`skipped` kontra `cancelled`) i to jest różnica, o którą UI pyta pierwsze.
    fn close_the_book(&self, states: &[StepState], cancelled: bool) {
        let at = now_ms();
        /* 2026-08-23 — POMINIETY KROK MOWI, PRZEZ KOGO. Zamowienie wlasciciela brzmialo
         * „zadnych slepych punktow", a najciemniejszym z nich byl krok `skipped` z `error: null`:
         * jego bieg konczyl sie trzema pustymi wierszami i ani jednym zdaniem o tym, co je
         * skasowalo. W biegu `20260823-011240` bylo tak z `Synteza`, `Design` i `Implementation`.
         *
         * Liczone TUTAJ, po planiscie, a nie w `mark_cone`: tamta funkcja jest czysta i nie zna
         * ksiegi, a przepchniecie do niej ksiegi zamienialoby planiste w cos, co pisze po dysku.
         * Tu mamy komplet stanow koncowych i graf, wiec przodek liczy sie raz i na pewno. */
        let blamed = self.who_stopped_them(states);
        /* SUFIT WYDATKU PISZE PRZED OGÓLNYM OBWINIANIEM. Krok zatrzymany wprost ma już zdanie
         * z chwili decyzji, a `name_what_the_budget_stopped` skopiowało je tylko do jego
         * pominiętego stożka. Odwrotna kolejność wstawiałaby tam ogólne „poprzednik nie
         * przeszedł", po czym `get_or_insert` nie pozwoliłby już powiedzieć prawdy o budżecie. */
        let stopped_by_the_budget = self
            .stopped_by_the_budget
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        self.update(|book| {
            for (index, (row, &state)) in book.steps.iter_mut().zip(states).enumerate() {
                row.status = state;
                if row.end_cause.is_none() {
                    row.end_cause = match state {
                        StepState::Cancelled => Some(super::run_inputs::EndCause::Cancelled),
                        StepState::Skipped if cancelled => {
                            Some(super::run_inputs::EndCause::Cancelled)
                        }
                        StepState::Skipped
                            if stopped_by_the_budget
                                .get(index)
                                .is_some_and(Option::is_some) =>
                        {
                            Some(super::run_inputs::EndCause::InfrastructureFailed)
                        }
                        StepState::Skipped if blamed.iter().any(|(at, _)| *at == index) => {
                            Some(super::run_inputs::EndCause::DependencySkipped)
                        }
                        StepState::Skipped => Some(super::run_inputs::EndCause::BranchNotSelected),
                        _ => None,
                    };
                }
            }
            for (row, why) in book.steps.iter_mut().zip(stopped_by_the_budget.iter()) {
                if row.status == StepState::Skipped
                    && let Some(why) = why
                {
                    let _ = row.error.get_or_insert(why.clone());
                }
            }
            for (at_step, why) in &blamed {
                if let Some(row) = book.steps.get_mut(*at_step) {
                    let _ = row.error.get_or_insert(why.clone());
                }
            }
            book.status = if cancelled {
                RunState::Cancelled
            } else if states.contains(&StepState::Failed) {
                RunState::Failed
            } else {
                RunState::Succeeded
            };
            book.ended_at = Some(at);
        });
    }

    /// Dla kazdego pominietego kroku: zdanie o tym, KTORY krok go skasowal.
    ///
    /// Idzie w gore po strzalkach do NAJBLIZSZEGO przodka, ktory nie przeszedl. Najblizszego,
    /// bo to on jest ta rzecza, ktora czlowiek moze poprawic — wskazanie korzenia lancucha
    /// kazaloby mu samemu odtwarzac droge przez graf.
    ///
    /// Anulowanie ma WLASNE zdanie i to nie jest kosmetyka (niezmiennik 7): krok pominiety, bo
    /// ktos nacisnal Stop, nie ma prawa czytac sie jak krok pominiety przez cudza porazke.
    fn who_stopped_them(&self, states: &[StepState]) -> Vec<(StepId, String)> {
        let mut out = Vec::new();
        for (id, &state) in states.iter().enumerate() {
            if state != StepState::Skipped {
                continue;
            }
            let mut seen = vec![false; states.len()];
            let mut stack = vec![id];
            while let Some(here) = stack.pop() {
                if std::mem::replace(&mut seen[here], true) {
                    continue;
                }
                for &(from, to) in &self.plan.arrows {
                    if to != here {
                        continue;
                    }
                    match states.get(from) {
                        Some(StepState::Failed) => {
                            out.push((
                                id,
                                format!(
                                    "Skipped: \"{}\" did not pass, and nothing after it was set \
                                     to carry on.",
                                    self.plan.steps[from].name,
                                ),
                            ));
                            stack.clear();
                            break;
                        }
                        Some(StepState::Cancelled) => {
                            out.push((
                                id,
                                format!("Skipped: \"{}\" was stopped.", self.plan.steps[from].name),
                            ));
                            stack.clear();
                            break;
                        }
                        Some(_) | None => stack.push(from),
                    }
                }
            }
        }
        out
    }
}

/// Bieg stoi albo idzie — **wyliczone z obu powodów naraz**, nigdy wpisane z jednego.
///
/// Powody są dwa i mijają niezależnie: pytanie do człowieka ([`Live::wait_for_a_person`]) i limit
/// dostawcy ([`Live::the_provider_said_wait`]). Dwa bezwarunkowe przypisania do `status` kasują
/// się nawzajem — ten powód, który skończył pierwszy, ogłasza bieg jako idący, choć drugi wciąż
/// go trzyma, i na ekranie wygląda to jak bieg, który wysyła do zamkniętego okna.
///
/// Wolno to wołać **wyłącznie w trakcie biegu**: stany końcowe wpisuje [`Live::close_the_book`]
/// i nic po nim nie pyta już o to, czy bieg idzie.
fn run_stands_or_moves(book: &mut Book, paused_by_the_provider: bool) {
    book.status = if book.asking || paused_by_the_provider {
        RunState::Paused
    } else {
        RunState::Running
    };
}

/// Zdarzenia jednego kroku → wiersze na ekran.
///
/// Kuracja mieszka w [`Curator`] i **tylko** tam (niezmiennik 15): ta pętla nie decyduje, który
/// wiersz istnieje ani co mówi, tylko podaje zdarzenia po kolei i wypuszcza to, co się domknęło.
///
/// 2026-08-16 — `tool: None` jest tu granicą, nie niedopatrzeniem. Fakty o narzędziu
/// (`engine::line::Tool`) wyjmuje z linii drutu `stream::decode`, a `AgentDriver` oddaje już
/// samo zdarzenie neutralne, więc na tej drodze wiersze `read`/`edit`/`ran` nie mają z czego
/// powstać. Szew, w którym te dwie drogi mają się spotkać, należy do T-07 (`ARCHITECTURE` §4:
/// `stream.rs` stoi między nadzorem a kuratorem); dopisanie tu drugiej klasyfikacji byłoby
/// drugą implementacją kuracji, czyli tą, o której nikt by nie pamiętał.
/// 2026-08-17 (T-31) — pętla dostaje CAŁY bieg, a nie same linie, i to jest cała różnica między
/// „widać banner" a „bieg umie się wznowić o właściwej godzinie". `AgentEvent::RateLimit`
/// docierał tu i zostawał wierszem na ekranie, a wysyłka szła dalej, jakby nic nie zaszło —
/// czyli następny agent dostawał odmowę, a okno limitu paliło się na odmowach.
async fn forward(
    live: Arc<Live>,
    mut inbox: mpsc::Receiver<DecodedEvent>,
    agent: String,
    id: StepId,
    runtime_fault: mpsc::Sender<WhyTheTurnMustEnd>,
    finished_event: mpsc::Sender<()>,
    finish: CancellationToken,
) {
    let mut curator = Curator::new();
    let mut repeated_failures = RepeatedToolFailures::default();
    let mut tripped = false;
    // 2026-08-25 (T-115) — NAZWA KROKU NIE JEST KLUCZEM. Dwa wezly moga miec ten sam tekst
    // na ekranie; indeks planu pozostaje rozny i nie pozwala, by uwaga o nieznanej cenie
    // jednego wezla zostala dopisana do koncowego wiersza drugiego.
    let step_key = id.to_string();
    /* Czy okno już wie, że ten agent myśli. Powód, dla którego to jest tu, a nie w kuratorze,
     * stoi przy wysyłce niżej. */
    let mut told_it_thinks = false;
    // 2026-08-18 — PACZKA NIESIE TERAZ FAKT O NARZEDZIU, nie samo zdarzenie. Do tego dnia
    // kanal sterownika mial typ `Sender<AgentEvent>`, wiec `Tool` — rodzina czynnosci, pelna
    // sciezka, pelne wyjscie — ginal na granicy sterownika, a ta petla musiala podac
    // `tool: None`. Skutkiem nie byla gorsza jakosc wiersza: `Curator` bez `seen.tool` zwraca
    // `Vec::new()`, wiec wiersze `read`, `search`, `edit` i `ran` NIE POWSTAWALY NIGDY
    // i strumien pokazywal wylacznie proze agenta. Powod, dla ktorego naprawa nalezy tutaj,
    // a nie do drugiej tabeli nazw narzedzi w tym pliku, stoi przy `engine::drivers::DecodedEvent`
    // (niezmienniki 15 i 23).
    let mut closing = false;
    loop {
        let decoded = if closing {
            inbox.recv().await
        } else {
            tokio::select! {
                // Najpierw zamknij przyjmowanie nowych zdarzeń, ale nie zgub tych, które już
                // są w ograniczonej kolejce. `Receiver::close` właśnie po to rozdziela
                // „nie przyjmuj więcej” od „opróżnij to, co przyjęte”.
                biased;
                () = finish.cancelled() => {
                    inbox.close();
                    closing = true;
                    continue;
                }
                decoded = inbox.recv() => decoded,
            }
        };
        let Some(DecodedEvent { event, tool }) = decoded else {
            break;
        };
        let finishes_turn = matches!(&event, AgentEvent::Finished(_));
        // Decyzja biegu czyta fakty PRZED kuracją. Kurator przycina wynik dla ekranu, a dokładne
        // pełne wyjście jest częścią podpisu porażki. Jeden sygnał wystarcza; właściciel uchwytu
        // anuluje proces i zamyka ten kanał po dowodzie zejścia.
        if !tripped && repeated_failures.observe(&event, tool.as_ref()) {
            tripped = true;
            let _ = runtime_fault.try_send(WhyTheTurnMustEnd::RepeatedToolFailure);
        }
        /* SUFIT WYDATKU DZIAŁA W ŚRODKU TURY, i to jest jedyna droga dla vendora, który flagi
         * sufitu nie ma (2026-09, Z-13b).
         *
         * Claude dostaje `--max-budget-usd` i zatrzyma turę sam; Codex takiej flagi nie ma,
         * a jego tura jest wyceniana dopiero po zakończeniu — czyli wtedy, kiedy jest już
         * opłacona. `AgentEvent::Spending` niesie estymatę z tabeli cen w trakcie, więc dopiero
         * tutaj da się powiedzieć „dość".
         *
         * POLITYKA JEST TU, W RDZENIU, a nie w adapterze (niezmiennik 23): ta linia nie zna ani
         * jednego vendora — pyta o udział tego kroku i o to, co krok o sobie donosi. Adapter
         * odpowiada wyłącznie za to, czy w ogóle umie donieść.
         *
         * JEDEN SYGNAŁ NA TURĘ, wspólny z bezpiecznikiem narzędzi: kanał ma pojemność jeden,
         * a właściciel uchwytu i tak schodzi po pierwszym powodzie. */
        if !tripped
            && let AgentEvent::Spending { estimate_usd } = &event
            && let Some(allowed) = live.the_share_held_by(id)
            && *estimate_usd >= allowed
        {
            tripped = true;
            let _ = runtime_fault.try_send(WhyTheTurnMustEnd::OverItsShare { allowed });
        }
        // PRZED kuracją, nie po niej: wiersz jest zdaniem dla człowieka, a to niżej jest
        // decyzją dla biegu. Kolejność odwrotna dokłada okno, w którym ekran już wie, a bieg
        // jeszcze wysyła.
        if let AgentEvent::RateLimit {
            status, resets_at, ..
        } = &event
        {
            live.the_provider_said_wait(status, *resets_at);
        }
        /* 2026-08-23 (T-87) — PROZA KROKU ZOSTAJE TAKŻE POZA EKRANEM. Do tego dnia `Said` szedł
         * wyłącznie do kuracji, a jedynym trwałym śladem tury było `StepRun::summary` pisane
         * z UDANEGO wyniku. Krok, którego tura wróciła błędem, oddawał więc następnemu krokowi
         * plik pusty — mimo że agent zdążył napisać, co zrobił i na czym stanął. Tutaj, bo tędy
         * przechodzi KAŻDE zdarzenie kroku, i przed kuracją, bo kuracja skleja wiersze i nie
         * jest zobowiązana zachować całego tekstu. */
        if let AgentEvent::Said { text } = &event {
            live.also_said(id, text);
        }
        /* 2026-09 (Z-16) — CO FOLDER DAŁ TEMU KROKOWI, ZANIM POWIEDZIAŁ SŁOWO. Tutaj, bo tędy
         * przechodzi KAŻDE zdarzenie kroku, i w tym samym miejscu co proza wyżej, bo obie te
         * rzeczy są zapisem trwałym, a nie wierszem na ekran (kurator oddaje z tego zdarzenia
         * `Vec::new()` z rozmysłu — powód stoi w `engine::line`). */
        if let AgentEvent::LoadedFromTheFolder(loaded) = &event {
            live.also_loaded(id, loaded);
        }
        let at_ms = u64::try_from(live.began.elapsed().as_millis()).unwrap_or(u64::MAX);
        let seen = Seen {
            agent: &agent,
            at_ms,
            event: &event,
            tool: tool.as_ref(),
        };
        let batch = curator.observe_with_step_key(seen, Some(&step_key));

        /* SLOT „Thinking…" DOSTAJE SWÓJ NOŚNIK — i to jest jedyne miejsce, w którym wolno mu
         * go dostać.
         *
         * 2026-08-18. `docs/ARCHITECTURE.md` linia 178 daje dla `thinking` i `thinking_tokens`
         * wprost: „*nic w strumieniu* — stały slot na dole, nadpisywany". Kolumna tej tabeli
         * nazywa się „Co widać", więc zdanie mówi dwie rzeczy naraz: żadnego wiersza HISTORII,
         * ale slot ma pokazywać. Do dziś pokazywał nic: jedynym śladem myślenia był
         * `Curator::status`, którego w produkcji **nikt nie czytał**, więc dolna strefa ekranu
         * była martwa także wtedy, gdy agent myślał minutami.
         *
         * DLACZEGO TU, A NIE W KURATORZE. Próba odwrotna — `Curator::observe` oddające
         * `vec![Line::Thinking]` — przewróciła CZTERY kryteria w dwóch plikach, z których jedno
         * przepuszcza prawdziwą pompę przez złotą fiksturę szesnastu zdarzeń i żąda dokładnie
         * trzech wierszy. I miały rację: wektor kuratora JEST strumieniem historii, więc wiersz
         * w nim jest wierszem w historii. Tutaj nie jest: to jest osobna wysyłka obok kuracji,
         * a rejestr po stronie okna kieruje ten rodzaj na trasę `now`, gdzie widok go
         * NADPISUJE, nigdy nie dokłada (`src/sections/run/feed/model.ts`, gałąź `route === 'now'`
         * robi `continue`).
         *
         * TYLKO NA ZMIANĘ STANU i tylko wtedy, gdy kuracja nie oddała ani jednego wiersza.
         * Wiersz na każde zdarzenie myślenia to cztery wiadomości na turę przez pompę za jeden
         * fakt; wiersz wysłany RAZEM z prawdziwym wierszem zapalałby slot w tej samej paczce,
         * w której widok go gasi (prawdziwa linia gasi slot — [T2 §7.2 wiersz 4]). Gaśnięcia
         * nie wysyłamy wcale: robi je okno, na pierwszej prawdziwej linii, i to jest jego jedna
         * odpowiedź na to pytanie (niezmiennik 13). */
        if batch.is_empty() {
            if !told_it_thinks && curator.status() == Some(Status::Thinking) {
                let _ = live.lines.send(Line::Thinking {
                    agent: agent.clone(),
                });
                told_it_thinks = true;
            }
        } else {
            told_it_thinks = false;
        }

        send_batch(&live.lines, batch);
        if finishes_turn {
            // Potwierdzenie idzie dopiero po decyzjach biegu i kuracji tego zdarzenia. Ponieważ
            // kanał wejściowy jest FIFO, odbiorca wie wtedy, że każdy wcześniejszy `ToolEnd`
            // został już policzony, zanim przyjmie niezależny wynik procesu.
            let _ = finished_event.try_send(());
        }
    }
    // Ostatnia grupa sklejania wyszłaby inaczej nigdy, a użytkownik zobaczyłby o wiersz mniej,
    // niż się wydarzyło — najgorszy rodzaj zgubienia, bo cichy.
    send_batch(&live.lines, curator.flush());
}

/// Wiersze kuratora oddane pompie, **po jednym**.
///
/// 2026-08-17 — funkcja jest synchroniczna i to jest cała treść tego szwu. `LineSink::send`
/// robi `try_send`: albo ma miejsce, albo nie ma, i nigdy nie każe czekać. Wersja z `await`
/// zatrzymywałaby na pełnej kolejce pętlę czytającą stdout agenta — czyli spowalniała agenta
/// dlatego, że okno nie nadąża, co jest dokładnie tą własnością, którą pompa miała skasować
/// (`ipc::LineSink`, [T7 §4.1]).
///
/// Odpowiedzi `Sent` nie liczymy tutaj i to też jest wybór: bilans przyjętych i porzuconych
/// wraca JEDNĄ drogą, z `PumpStats` po drugiej stronie [`crate::ipc::spawn_pump`]
/// (niezmiennik 13). Drugi licznik w biegu byłby drugą liczbą o tym samym zdarzeniu — a przy
/// dwóch zawsze czyta się tę, która akurat kłamie.
fn send_batch(lines: &LineSink, batch: Vec<Line>) {
    for line in batch {
        let _ = lines.send(line);
    }
}

/// Jedna linia podsumowania kroku dla szyny agentów. `None`, kiedy agent nic nie powiedział.
fn summary_of(text: &str) -> Option<String> {
    one_line(text, SUMMARY_LIMIT)
}

/// Tytuł przekazania: **to, o co poproszono ten krok**, w jednym wierszu.
///
/// Z pliku workflow, nigdy z odpowiedzi modelu. `title` jest polem front-mattera, a te pisze
/// Loadout (`ARCHITECTURE` §8): tytuł wzięty z ciała oddawałby zdanie „co to za przekazanie" temu,
/// kto ma najwięcej do zyskania na tym, żeby ono dobrze brzmiało. Kafelek bez własnego zdania mówi
/// swoją nazwą — ona też jest zdaniem, które napisał człowiek.
fn title_of(step: &Planned) -> String {
    match &step.job {
        Job::Agent(job) => one_line(&job.asked, TITLE_LIMIT),
        Job::Ask { question } => question
            .as_deref()
            .and_then(|question| one_line(question, TITLE_LIMIT)),
        // Tytułem przekazania kroku „sprawdź" jest to, co ten krok URUCHOMIŁ — jedyne zdanie
        // o nim, które napisał człowiek, i to samo, które człowiek widzi w panelu kafelka.
        // Ten sam powód dla kafelka „uruchom i zostaw": tym, co po nim zostaje, jest URUCHOMIONE
        // polecenie, i to jest jedyne zdanie o nim, które napisał człowiek.
        Job::Check(job) => one_line(&job.spec.command, TITLE_LIMIT),
        Job::Serve(job) => one_line(&job.command, TITLE_LIMIT),
    }
    .unwrap_or_else(|| step.name.clone())
}

/// Tekst zwinięty do jednej linii i przycięty do `limit` bajtów, po granicy znaku. `None`, kiedy
/// nie zostało ani jedno słowo.
///
/// Jedna pętla na oba wywołania: podsumowanie kroku i tytuł przekazania różnią się wyłącznie
/// limitem, a druga kopia byłaby tą, która kiedyś zacznie ciąć w środku znaku — i przewróci bieg
/// na pierwszym emoji w odpowiedzi agenta.
fn one_line(text: &str, limit: usize) -> Option<String> {
    let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return None;
    }
    let mut end = line.len().min(limit);
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    Some(line[..end].to_owned())
}

/// Numery kroków po drugiej stronie strzałek — **rosnąco i każdy raz**.
///
/// Rosnąco, czyli w kolejności z pliku workflow: to jedyny porządek, o którym nie decyduje
/// przypadek (patrz [`Live::handed_before`]). Bez powtórzeń, bo dwie strzałki między tą samą parą
/// kroków są w pliku legalne, a wpis wymieniony dwa razy każe krokowi zapłacić tokenami za tę samą
/// pracę dwa razy — i podwaja jedną z dwóch stron, którą krok syntezujący ma zważyć.
fn ends(
    arrows: &[(StepId, StepId)],
    pick: impl Fn(&(StepId, StepId)) -> Option<StepId>,
) -> Vec<StepId> {
    let mut out: Vec<StepId> = arrows.iter().filter_map(pick).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Ile ta księga wydała: suma cen tur, które SIĘ SKOŃCZYŁY.
///
/// Wolna funkcja, nie metoda, bo pyta o to dwóch: [`Live::spent_so_far`] spod własnego zamka
/// i [`Live::run_file`], któremu księgę podano już otwartą. Metoda biorąca zamek drugi raz
/// zakleszczyłaby zrzut na dysk — a `std::sync::Mutex` nie jest wejściowalny ponownie.
fn step_spend_in(book: &Book) -> f64 {
    book.steps
        .iter()
        .filter(|step| step.ended_at.is_some())
        .filter_map(|step| step.cost_usd)
        .sum()
}

/// Czy ten krok już OSIADŁ: stan końcowy, z którego nic nowego nie wyjdzie.
///
/// 2026-09 (Z-13b) — wolna funkcja przy [`step_spend_in`], a nie metoda na `StepState`:
/// `engine::step` nie należy do tego zadania (`AGENTS.md` §7), a jedynym pytającym jest podział
/// sufitu ([`Live::how_many_share_the_rest`]). Pełny `match` zamiast `matches!` z rozmysłem:
/// ósmy stan maszyny przewróci wtedy kompilację, zamiast po cichu policzyć się jako „jeszcze
/// może ruszyć" i zawęzić dzielnik.
fn has_settled(state: StepState) -> bool {
    match state {
        StepState::Succeeded | StepState::Failed | StepState::Cancelled | StepState::Skipped => {
            true
        }
        StepState::Pending | StepState::Ready | StepState::Running => false,
    }
}

/// Końcowy rachunek obejmuje również prywatną refleksję — także tę, która nie odpowiedziała.
///
/// Scheduler celowo nie używa tej funkcji: refleksja zaczyna się dopiero po zakończeniu grafu,
/// więc nie może wpływać na decyzję, czy wolno rozpocząć następny krok.
fn final_spend_in(book: &Book) -> f64 {
    step_spend_in(book) + book.reflection.cost_usd.unwrap_or(0.0)
}

/// Czy ktokolwiek w tym biegu podał cenę: którykolwiek krok albo prywatna tura.
///
/// 2026-09 (Z-38) — TO JEST WARUNEK ZAPISU `spent_usd`, i pyta o POMIAR, nie o sufit. Bieg,
/// w którym żaden vendor ceny nie podał, nie ma prawa zapisać zera: „nikt nie zmierzył" i „nie
/// kosztowało nic" to dwa różne zdania (niezmiennik 17), a zero widziane w historii jest tym
/// drugim.
fn anything_was_priced(book: &Book) -> bool {
    book.steps.iter().any(|step| step.cost_usd.is_some()) || book.reflection.cost_usd.is_some()
}

/// Ile miejsca bierze krok tego rodzaju.
///
/// # Dlaczego „sprawdź" jest ciężkie, a agent wybiera wagę (niezmiennik 26)
///
/// Bo `./verify.sh full` odpala `cargo`, `cargo` odpala `rustc`, a dwa równoległe linki
/// przypinają kompresor pamięci macOS i zamrażają maszynę przy zerowym swapie. Zwykła tura
/// agenta jest rozmową, ale krok, któremu człowiek zleca build, pełną suitę albo przeglądarkę,
/// niesie tę różnicę w pliku workflow.
///
/// **Pole kroku, nie jego nazwa ani rola** (niezmiennik 27). `if step.name == "check"` byłoby
/// etapem zaszytym w silniku; tu odpowiedź niesie jawny fakt z grafu.
fn weight_of(job: &Job) -> limits::Weight {
    match job {
        Job::Agent(job) => job.weight,
        Job::Check(_) => limits::Weight::Heavy,
        // Kafelek kontrolny i „uruchom i zostaw" nie proszą tędy o miejsce w ogóle
        // ([`Live::step`]), więc odpowiedź dla nich nie ma czytelnika i jest tą samą, co dla
        // rozmowy.
        Job::Ask { .. } | Job::Serve(_) => limits::Weight::Ordinary,
    }
}

// ── KSZTAŁT `run.json` ─────────────────────────────────────────────────────────────────────

/// `run.json`, tak jak ląduje na dysku.
///
/// Nazwy pól są dokładnie tymi, które czyta `store::rebuild` — rozjazd znaczy, że po skasowaniu
/// bazy dostaje się co innego, niż się miało (niezmiennik 4). Dlatego są w `snake_case`,
/// a `effective` w środku kroku zostaje w `camelCase`: to jest migawka cudzego kształtu
/// (`library::agents::Agent`), nie nasz schemat.
#[derive(Debug, Serialize)]
struct RunFile<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    lead_origin: Option<&'a super::lead_start::LeadStartOrigin>,
    id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_snapshot: Option<Value>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    workspace_inputs: BTreeMap<String, super::workspace_inputs::SeedRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_instructions: Option<&'a Value>,
    memory_sources: super::memory_sources::Binding,
    #[serde(skip_serializing_if = "Option::is_none")]
    context_sources: Option<super::context_sources::Binding>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    starting_results: &'a BTreeMap<String, StartingResult>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    copy_results: &'a BTreeMap<String, SavedCopy>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pending_finalization: &'a BTreeSet<String>,
    workflow_id: &'a str,
    /// Odcisk pliku workflow — „czy to był ten sam plan?".
    workflow_hash: &'a str,
    /// Graf **jak biegł**. Bez niego poprawiony workflow po cichu zmienia opowieść starych
    /// biegów stojących w historii [T7 §5.4].
    workflow_snapshot: &'a Value,
    title: &'a str,
    /// O co poproszono ten bieg — dosłownie to, co człowiek wpisał.
    ///
    /// PUSTY NAPIS, NIE BRAK POLA. „Nic nie kazano" jest odpowiedzią, a nie brakiem odpowiedzi:
    /// bieg puszczony bez zadania i bieg z pliku sprzed tej zmiany wyglądałyby wtedy identycznie,
    /// a to są dwie różne historie. Czytelnicy starych plików biorą `#[serde(default)]`.
    task: &'a str,
    status: RunState,
    concurrency: usize,
    created_at: i64,
    /// Brak dla recznego Startu; trigger zapisuje tylko zredagowane identyfikatory receipt.
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_origin: Option<&'a TriggerOrigin>,
    /// Kiedy wstała maszyna, na której ten bieg ruszył. Czyta to `store::rebuild` i po nim
    /// odzyskiwanie po awarii decyduje, czy wolno sprzątnąć zapisaną grupę procesów.
    #[serde(skip_serializing_if = "Option::is_none")]
    boot_id: Option<&'a str>,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    error: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    route_decisions: Vec<RouteDecision>,
    /// Co ten bieg wiedział przy starcie — odwołanie, odcisk i liczba bajtów na notatkę.
    ///
    /// Brak pola znaczy „ten bieg nie wiedział nic", i to jest prawda o biegu ruszonym na
    /// maszynie bez ani jednej notatki w użyciu. Pusta lista wpisana na siłę mówiłaby to samo
    /// jednym kluczem więcej w każdym `run.json` w historii.
    #[serde(skip_serializing_if = "<[MemoryRecord]>::is_empty")]
    memory: &'a [MemoryRecord],
    /// Po jaki materiał ten bieg sięgnął przy starcie — nazwa, odcisk i liczba bajtów na
    /// umiejętność.
    ///
    /// 2026-08-28 (T-154). Brak pola znaczy „ten bieg nie sięgnął po żadną", i to jest prawda
    /// o biegu, którego kroki nie mają ani jednej umiejętności — czyli o większości. Pusta lista
    /// wpisana na siłę mówiłaby to samo jednym kluczem więcej w każdym `run.json` w historii; ta
    /// sama decyzja, co przy `memory` obok.
    ///
    /// CZYTANE, NIE TYLKO PISANE (niezmiennik 21): `the_frozen_skills_are_still_here` bierze
    /// stąd odciski, kiedy człowiek każe powtórzyć krok tego biegu.
    #[serde(skip_serializing_if = "<[SkillRecord]>::is_empty")]
    skills: &'a [SkillRecord],
    /// Ile wolno było wydać na ten bieg. Brak klucza znaczy „nikt nie postawił sufitu".
    ///
    /// Na dysku, a nie tylko w pamięci, bo pliki są prawdą (niezmiennik 4): sufit, który znika
    /// razem z oknem, nie umie wyjaśnić po fakcie, dlaczego trzy kroki zostały pominięte.
    #[serde(skip_serializing_if = "Option::is_none")]
    budget_usd: Option<f64>,
    /// Ile ten bieg naprawdę wydał — suma cen tur, które się skończyły, **plus prywatna tura**.
    ///
    /// 2026-09 (Z-38) — NIE IDZIE JUŻ W PARZE Z SUFITEM. Do tego dnia zapisywał się wyłącznie
    /// biegowi z sufitem, więc jedyne miejsce, w którym w ogóle istnieje cena refleksji,
    /// znikało dla każdego biegu, którego nikt nie ograniczył. Brak klucza znaczy dziś dokładnie
    /// jedno: nikt w tym biegu nie podał ceny ([`anything_was_priced`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    spent_usd: Option<f64>,
    reflection: &'a ReflectionReceipt,
    steps: Vec<StepEntry<'a>>,
}

fn step_entry<'a>(planned: &'a Planned, run: &'a StepRun) -> StepEntry<'a> {
    StepEntry {
        id: &planned.id,
        node_key: &planned.node_key,
        name: &planned.name,
        agent: &planned.vendor,
        kind: match &planned.job {
            Job::Agent(_) => "agent",
            Job::Check(_) => "check",
            Job::Ask { .. } => "checkpoint",
            Job::Serve(_) => "serve",
        },
        depends_on: &planned.depends_on,
        status: run.status,
        end_cause: run.end_cause,
        assessment: run.assessment.as_ref(),
        result_files: run.result_files.as_ref(),
        plan_version: run.plan_version.as_ref(),
        execution: ExecutionEntry {
            executed: run.execution.executed,
            process_started: run.execution.process_started,
        },
        not_run_because: run.not_run_because.as_deref(),
        round_outcome: run.round_outcome,
        // Ponowienie kroku („uruchom jeszcze raz od tego miejsca") jest w v1.1
        // [PLAN §7], więc każdy krok ma tu dziś dokładnie jedno podejście.
        attempt: 0,
        agent_session_id: match &planned.job {
            Job::Agent(job) if run.execution.process_started => Some(job.session.to_string()),
            Job::Agent(_) | Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => None,
            // Kafelek kontrolny i krok „sprawdź" nie mają sesji, bo nie mają vendora.
            // Wpisany identyfikator byłby numerem, pod którym wznowienie szukałoby
            // kiedyś rozmowy, której nigdy nie było.
        },
        pid: run.pid,
        pgid: run.pgid,
        pgids: &run.pgids,
        exit_code: run.exit_code,
        death_proof: run.death_proof,
        started_at: run.started_at,
        ended_at: run.ended_at,
        cost_usd: run.cost_usd,
        cost_estimate: run.cost_estimate,
        uncached_input: run.uncached_input,
        cache_read: run.cache_read,
        cache_write: run.cache_write,
        output: run.output,
        vendor_turns: run.vendor_turns,
        summary: run.summary.as_deref(),
        error: run.error.as_deref(),
        stopped_from_outside: run.stopped_from_outside,
        ran_without: &run.ran_without,
        effective: match &planned.job {
            Job::Agent(job) => Some(&job.effective),
            // Nie ma czego zamrażać: ani kafelek kontrolny, ani krok „sprawdź" nie mają
            // konfiguracji agenta, bo żadnego agenta nie wołają.
            // 2026-08-23 — kafelek „uruchom i zostaw" też nie ma czego zamrażać: nie woła
            // agenta, tylko odpala polecenie i idzie dalej.
            Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => None,
        },
        repaired: &run.repaired,
        truncated: run.truncated,
        loaded_by_the_app: run.loaded_by_the_app.as_ref(),
        borrowed_concerns: &run.borrowed_concerns,
    }
}

/// Krok w `run.json`.
#[derive(Debug, Serialize)]
struct StepEntry<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    assessment: Option<&'a crate::engine::drivers::command::assessment::Assessment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_cause: Option<super::run_inputs::EndCause>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result_files: Option<&'a super::run_inputs::ResultFiles>,
    /// 2026-09-08 — adres kompletnego dokumentu w `plans/`, a nie jego druga kopia.
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_version: Option<&'a WorkPlanReceipt>,
    id: &'a str,
    node_key: &'a str,
    name: &'a str,
    agent: &'a str,
    /// Zamknięty rodzaj kroku; diagnostyka nie zgaduje po obecności artefaktów agenta.
    kind: &'static str,
    depends_on: &'a [String],
    status: StepState,
    #[serde(flatten)]
    execution: ExecutionEntry,
    /// Addytywny fakt dla rund domkniętych wyłącznie na potrzeby planisty. Brak zachowuje
    /// dotychczasowy kształt każdego kroku, który naprawdę biegł albo został pominięty inaczej.
    #[serde(skip_serializing_if = "Option::is_none")]
    not_run_because: Option<&'a str>,
    /// Rozstrzygnięcie sędziego tej rundy. Brak klucza znaczy, że ten węzeł nie był sędzią albo
    /// nie ruszył; stare pliki bez pola zachowują właśnie tę wartość domyślną.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    round_outcome: Option<handoff::Verdict>,
    attempt: u32,
    agent_session_id: Option<String>,
    pid: Option<i32>,
    pgid: Option<i32>,
    /// Każda grupa procesów, którą ten krok uruchomił — powód w całości przy [`StepRun::pgids`].
    ///
    /// BRAK KLUCZA, KIEDY KROK NIE ZOSTAWIŁ ŻADNEJ, i to jest ta sama decyzja, co przy
    /// `death_proof` i `repaired`: pusta lista przy każdym kafelku kontrolnym każdego biegu
    /// w historii jest długością zapłaconą za milczenie. Odzyskiwanie czyta brak klucza jak pustą
    /// listę (`commands::reconcile::numbers`), więc starsze pliki biegów znaczą dokładnie to samo.
    #[serde(skip_serializing_if = "<[i32]>::is_empty")]
    pgids: &'a [i32],
    exit_code: Option<i32>,
    /// Tylko rzeczywisty dowód supervisora. Brak pola oznacza „nie dowiedziono”, nigdy
    /// „dowiedziono, bo krok wygląda na zakończony”.
    death_proof: bool,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_estimate: Option<bool>,
    uncached_input: Option<u64>,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
    output: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vendor_turns: Option<u32>,
    summary: Option<&'a str>,
    error: Option<&'a str>,
    /// Numer sygnału, którym ktoś SPOZA Loadouta zatrzymał ten krok.
    ///
    /// # 2026-09 (Z-39) — po co ta liczba jest w pliku, skoro zdanie stoi w `error`
    ///
    /// Bo zdanie jest tekstem i tydzień później nikt nie odróżni go od zdania sąsiedniego bez
    /// czytania go w całości; ten klucz odpowiada na to samo pytanie jedną wartością i przeżywa
    /// skasowanie `loadout.db` (niezmiennik 4). Na biegu meetnotes `20260901-150035` to jest
    /// jedyny fakt, po którym poznaje się, że cztery kroki nie padły — dostały `kill -TERM`
    /// spoza aplikacji, w parach po 300 ms.
    ///
    /// BRAK KLUCZA, KIEDY NIKT Z ZEWNĄTRZ NICZEGO NIE TKNĄŁ — ta sama decyzja, co przy
    /// `repaired` i `pgids` obok: klucz mówiący „nic się nie stało" przy każdym kroku każdego
    /// biegu w historii jest długością zapłaconą za milczenie.
    #[serde(skip_serializing_if = "Option::is_none")]
    stopped_from_outside: Option<i32>,
    /// Czyjego wyniku ten krok nie ma, choć pojechał dalej — po jednym zdaniu na poprzednika.
    ///
    /// 2026-09 (Z-39) — do tego dnia `carry-on` nie zostawiał po sobie ani jednego trwałego
    /// śladu w kroku, który z niego skorzystał: powód stał przy kroku, który PADŁ, a ten za nim
    /// wyglądał na zwykły krok, który po prostu tak odpowiedział. Bieg `20260901-150035` poszedł
    /// tak przez Final Plan (17 minut) i Combine (26 minut) nad ubitym researchem.
    ///
    /// BRAK KLUCZA DLA KROKU, KTÓRY DOSTAŁ WSZYSTKO — powód jak wyżej.
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    ran_without: &'a [String],
    /// Konfiguracja **efektywna**, zamrożona w chwili startu [T4 §5.2 p. 3]. `None` dla kafelka
    /// kontrolnego: on nie woła agenta, więc nie ma czego zamrażać.
    effective: Option<&'a Value>,
    /// Nagłówki, które Loadout dopisał do odpowiedzi tego kroku, **po nazwie i w kolejności
    /// dopisywania**.
    ///
    /// # 2026-08-23 (T-86) — do tego dnia ta liczba szła wyłącznie do `tracing::debug!`
    ///
    /// `memory::handoff::write_handoff` oddaje ją od początku i od początku jest prawdziwa,
    /// tylko nie widział jej NIKT: aplikacja nie ma włączonego poziomu debug, a `run.json` jest
    /// jedynym miejscem, które przeżywa skasowanie `loadout.db` (niezmiennik 4). Artefakt
    /// liczony i nieczytany jest dokładnie tym, czego zabrania niezmiennik 21.
    ///
    /// Co to zmienia dla człowieka: „agent nie oddał umówionego kształtu" jest z zewnątrz
    /// nieodróżnialne od „agent oddał kształt, a Loadout go zgubił", bo przekazanie na dysku ma
    /// trzy nagłówki w OBU przypadkach — `reshape()` je dopisuje. Pierwsze naprawia się jednym
    /// zdaniem w prompcie kroku, drugie jest wadą produktu.
    ///
    /// PO NAZWIE, NIE LICZBĄ: sama liczba odsyła człowieka do otwarcia pliku i porównania go
    /// okiem z tym, co pamięta z odpowiedzi.
    ///
    /// BRAK KLUCZA, KIEDY NIE BYŁO CZEGO DOPISAĆ. Klucz mówiący „nic się nie stało" przy każdym
    /// kroku każdego biegu jest długością zapłaconą za milczenie — i tą samą decyzją, którą
    /// obok podjęto dla `death_proof`.
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    repaired: &'a [String],
    /// Czy odpowiedź tego kroku nie zmieściła się w limicie i część leży w `attachments/`.
    ///
    /// Niezależna od `repaired` i to nie jest szczegół: kształt bywa umówiony, a treść i tak
    /// ucięta — następny krok nie zobaczy wtedy w pliku, na który go wskazano, całej odpowiedzi.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    truncated: bool,
    /// Co aplikacja agenta wczytała z folderu tego kroku **sama z siebie** — nie Loadout.
    ///
    /// # 2026-09 (Z-16) — do tego dnia ten fakt nie istniał nigdzie
    ///
    /// Linia `system/init` wymienia z nazwy całą powierzchnię folderu, którą CLI wzięło —
    /// pluginy, polecenia z ukośnikiem, umiejętności, serwery narzędzi, katalog pamięci,
    /// podagentów — a sterownik czytał z niej cztery pola i sześć porzucał. Nie było więc gdzie
    /// zobaczyć, co ten krok naprawdę dostał: ani w oknie, ani w pliku, który zostaje po biegu.
    ///
    /// **Dlaczego to jest ważniejsze, niż wyglądało.** Na 2.1.251 `CLAUDE.md` gospodarza docierał
    /// do kroku mimo `--setting-sources ""` i sześć kroków biegu `20260823-145648` zapisało przez
    /// to pliki wyników wbrew temu, co kazał im Loadout; na 2.1.260 (zmierzone 2026-09-04) już
    /// nie dociera. Vendor odwrócił to bez słowa w changelogu, więc wniosek „izolacja działa" ma
    /// termin ważności, a ten klucz go nie ma: niesie CYTAT z `system/init`, nie wniosek, i po
    /// nim jednym da się rozpoznać następną taką zmianę po fakcie.
    ///
    /// BRAK KLUCZA, KIEDY KROK NIC O TYM NIE POWIEDZIAŁ. Kafelek kontrolny, krok „sprawdź",
    /// Codex i każdy bieg zapisany przed tą zmianą są dokładnie w tej sytuacji — a klucz mówiący
    /// „nic nie wczytano" przy każdym z nich jest długością zapłaconą za milczenie, i to gorszą
    /// niż zwykle, bo czyta się jak odpowiedź. Ta sama decyzja, co przy `repaired` obok.
    #[serde(skip_serializing_if = "Option::is_none")]
    loaded_by_the_app: Option<&'a LoadedByTheApp>,
    /// Co przegląd zauważył w tekście, który ten krok pożyczył z projektu — reguła, plik,
    /// wiersz i cytat, po jednym wpisie na linię.
    ///
    /// # 2026-09 (Z-21) — po co to jest w pliku, skoro bieg poszedł dalej
    ///
    /// Bo to jest jedyne miejsce, w którym ten fakt przeżywa bieg. Linia w strumieniu mówi
    /// o tym człowiekowi, kiedy patrzy; `run.json` odpowiada na to samo pytanie tydzień
    /// później, kiedy odpowiedź kroku okazała się dziwna i trzeba wiedzieć, co dokładnie
    /// dostał w prompcie z cudzego repozytorium. Ten plik przeżywa skasowanie `loadout.db`
    /// (niezmiennik 4).
    ///
    /// CIĘŻKIE ZNALEZISKO NIE MA JAK TU TRAFIĆ: zabrało cały bieg przed katalogiem biegu, więc
    /// ta lista mówi wyłącznie o tym, co przepuszczono.
    ///
    /// BRAK KLUCZA, KIEDY NIE BYŁO NICZEGO — ta sama decyzja, co przy `repaired` i `pgids`
    /// obok: klucz mówiący „nic nie zauważono" przy każdym kroku każdego biegu w historii jest
    /// długością zapłaconą za milczenie, a większość kroków nie pożycza niczego.
    #[serde(skip_serializing_if = "<[BorrowedConcern]>::is_empty")]
    borrowed_concerns: &'a [BorrowedConcern],
}

#[derive(Debug, Serialize)]
struct ExecutionEntry {
    /// Próba minęła wszystkie skróty i weszła w ciało kroku.
    executed: bool,
    /// Start oddał ownership procesu; osobny fakt od wykonania checkpointu albo odmowy startu.
    process_started: bool,
}

// ── DROBIAZGI ──────────────────────────────────────────────────────────────────────────────

/// Milisekundy epoki. Zegar przestawiony wstecz daje zero zamiast liczby ujemnej: kolumna
/// `created_at` sortuje historię i data sprzed epoki wywróciłaby tę kolejność.
pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// Sekundy epoki — **ta sama jednostka, w której jedzie `resetsAt`** `[T7 §7.2, V]`.
///
/// Liczona z [`now_ms`], żeby bieg miał jeden zegar: druga droga do `SystemTime` znaczyłaby dwa
/// miejsca do poprawienia, kiedy zegar wymaga poprawki, i jedno z nich zostałoby stare. Ta sama
/// liczba w milisekundach mówi `duration_until_reset`, że limit wraca za 300 000 sekund — a to
/// wygląda na usterkę zegara, nie na pomyloną jednostkę, więc szuka się tego godzinami.
fn now_unix() -> i64 {
    now_ms() / 1_000
}

/// `<ts>` z nazwy katalogu biegu: `20260816-194804`, czas UTC.
///
/// Bez dwukropków i bez podkreśleń — nazwę katalogu rozcina się na pierwszym `__`, a dwukropek
/// nie jest znakiem, który przeżyje port na Windows. Sortuje się leksykograficznie, więc
/// `ls` w katalogu biegów daje historię w kolejności.
///
/// Algorytm dni→data jest standardowy (proleptyczny kalendarz gregoriański, era 400-letnia)
/// i stoi tu drugi raz w tym drzewie, obok `memory::handoff`. To nie jest przeoczenie: tamta
/// funkcja jest prywatna, a `src-tauri/src/memory/handoff.rs` nie należy do tego zadania, więc
/// jej udostępnienie jest pytaniem do człowieka (AGENTS.md §7), nie cichym dopiskiem w cudzym
/// pliku. `chrono` odpada z tego samego powodu — `Cargo.toml` też nie jest nasz.
fn stamp(at_ms: i64) -> String {
    let secs = u64::try_from(at_ms.max(0) / 1_000).unwrap_or(0);
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

    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Odcisk pliku workflow: odpowiada na pytanie **„czy to był ten sam plan"** i na żadne inne.
///
/// FNV-1a po bajtach z dysku, szesnaście znaków szesnastkowo. Nie jest to funkcja
/// kryptograficzna i nie ma nią być: pytanie brzmi „czy plik jest ten sam", a nie „czy ktoś go
/// podrobił". `sha2` nie jest zależnością tego drzewa, a `Cargo.toml` nie należy do tego
/// zadania (AGENTS.md §7) — więc wybór jest między tymi ośmioma wierszami a odciskiem, którego
/// nie ma wcale.
fn fingerprint(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    //! Zadanie z wiersza wejścia wchodzi w prompt kroku — trzy przypadki, jeden na każdą drogę.
    //!
    //! # Dlaczego testy jednostkowe W TYM PLIKU, a nie w `tests/it/`
    //!
    //! [`with_the_task`] jest prywatna, a droga do niej z zewnątrz prowadzi przez `plan_run`, czyli
    //! przez katalog biblioteki, plik workflow, definicję agenta i fabrykę sterowników. Test tej
    //! jednej funkcji przez tamto stanowisko kosztowałby sto linii fikstury, żeby sprawdzić
    //! sklejanie dwóch napisów — a zapora, której koszt sprawdzenia jest wyższy niż koszt
    //! napisania, jest zaporą niesprawdzoną. Ten sam precedens i to samo uzasadnienie stoi przy
    //! `run_request` w `src-tauri/src/ipc.rs`.
    //!
    //! # Słaba wersja tych kryteriów
    //!
    //! `assert!(out.contains(TASK))`. Przechodzi dla implementacji, która **zawsze** dokleja
    //! nagłówek, także przy pustym zadaniu — czyli dla tej, która każdemu biegowi bez zadania
    //! dopisuje do promptu nagłówek nad pustką i każe za niego płacić długością. Rozstrzyga
    //! porównanie CO DO BAJTU w przypadku pustym.

    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    use super::{TASK_MARK, node_key_for, tile_key_of, wait_for_reflection, with_the_task};
    use crate::commands::RunError;
    use crate::engine::drivers::{AgentHandle, Outcome as DriverOutcome, SessionRef};
    use crate::engine::supervisor::{GroupId, GroupProof};

    /// Prompt kroku, taki jak w pliku workflow.
    const STEP: &str = "Write the tests first, then the code.";

    /// Zadanie z wiersza wejścia.
    const TASK: &str = "build a pretty todo list";

    struct ReflectionWithoutDeathProof {
        close_called: Arc<AtomicBool>,
    }

    #[async_trait]
    impl AgentHandle for ReflectionWithoutDeathProof {
        fn session(&self) -> SessionRef {
            SessionRef {
                vendor: "test",
                id: "reflection-without-death-proof".to_owned(),
            }
        }

        fn group(&self) -> Option<GroupId> {
            None
        }

        async fn send(&mut self, _text: String) -> anyhow::Result<()> {
            Ok(())
        }

        async fn wait(&mut self) -> anyhow::Result<DriverOutcome> {
            std::future::pending().await
        }

        async fn cancel(&mut self) -> GroupProof {
            GroupProof::Alive { group: None }
        }

        async fn close(&mut self) -> anyhow::Result<Option<i32>> {
            self.close_called.store(true, Ordering::Release);
            Ok(None)
        }
    }

    #[tokio::test]
    async fn reflection_stop_refuses_without_death_proof() {
        let close_called = Arc::new(AtomicBool::new(false));
        let mut handle = ReflectionWithoutDeathProof {
            close_called: Arc::clone(&close_called),
        };
        let cancel = CancellationToken::new();
        cancel.cancel();

        let result = wait_for_reflection(&mut handle, cancel).await;

        assert!(matches!(
            &result,
            Err(RunError::Io(error))
                if error.to_string()
                    == "Loadout could not make sure the agent stopped after learning from this \
                        run, so it may still be running."
        ));
        assert!(
            !close_called.load(Ordering::Acquire),
            "close waits for a process to exit by itself, so calling it after an unproven Stop \
             would hide the refusal behind an unbounded wait"
        );
    }

    /* ── Klucz węzła a klucz kafelka ─────────────────────────────────────────────────────────
     *
     * Rundy pętli MUSZĄ mieć różne klucze węzła: `steps` w bazie ma `UNIQUE (run_id, node_key)`,
     * więc trzy rundy o jednym kluczu wywróciłyby odbudowę indeksu — po zapłaceniu za cały bieg.
     * Ograniczenia nie da się zmigrować (niezmiennik 25 zabrania przepisywania tabel).
     *
     * Runda ZEROWA nie dostaje sufiksu, i to jest decyzja o wsteczności: plik bez pętli daje
     * wtedy dokładnie te klucze, które dawał przedtem. Bez tego każdy istniejący bieg zapisałby
     * się z innymi kluczami niż jego poprzednicy i żadne dwa `run.json` nie dałyby się porównać.
     *
     * Słabą wersją tego kryterium jest sprawdzenie samej UNIKALNOŚCI dwóch kluczy. Przechodzi ją
     * implementacja doklejająca `#0` do rundy zerowej — czyli ta, która łamie wsteczność, i to
     * po cichu, bo unikalność ma nietkniętą. */

    #[test]
    fn the_first_turn_keeps_the_key_the_file_gave_it() {
        assert_eq!(
            node_key_for("s_test", 0, 0),
            "s_test",
            "a file with no loop has to plan exactly the keys it planned before, or no two run \
             records in this project can be compared with each other again"
        );
    }

    #[test]
    fn later_turns_get_keys_of_their_own() {
        assert_eq!(node_key_for("s_test", 1, 0), "s_test#1");
        assert_eq!(node_key_for("s_test", 2, 0), "s_test#2");
        assert_ne!(
            node_key_for("s_test", 1, 0),
            node_key_for("s_test", 2, 0),
            "the run index keys steps by this string and refuses a repeat, so two turns sharing \
             one key would fail the rebuild AFTER every agent has already been paid for"
        );
    }

    /* ── Kopia a runda ───────────────────────────────────────────────────────────────────────
     *
     * DODANE, nie w miejsce czegokolwiek (2026-08-23, T-90). Kopie i rundy są dwoma wymiarami
     * jednego kafelka i mnożą się przez siebie, więc klucz musi rozróżniać obie osie naraz:
     * kopia 2 rundy 1 i kopia 1 rundy 2 są dwoma różnymi węzłami, a jeden klucz dla obu to bieg,
     * który zapisze jeden i zgubi drugi.
     *
     * Kopia ZEROWA nie dostaje sufiksu, dokładnie jak runda zerowa i z tego samego powodu:
     * workflow, w którym nikt nie prosił o kopie, planuje klucze co do bajtu takie jak przedtem
     * — łącznie z nazwą katalogu `work/<klucz>`, którą wznowienie potrafi odzyskać. */

    #[test]
    fn later_copies_get_keys_of_their_own() {
        assert_eq!(
            node_key_for("s_build", 0, 0),
            "s_build",
            "a step that runs once has to plan the key it planned before"
        );
        assert_eq!(node_key_for("s_build", 0, 1), "s_build~2");
        assert_eq!(node_key_for("s_build", 0, 2), "s_build~3");
    }

    #[test]
    fn a_copy_inside_a_loop_is_told_apart_on_both_axes() {
        let keys = [
            node_key_for("s_try", 0, 0),
            node_key_for("s_try", 0, 1),
            node_key_for("s_try", 1, 0),
            node_key_for("s_try", 1, 1),
        ];
        let mut unique = keys.clone().to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            keys.len(),
            "two copies over two turns are four nodes of one tile, and the run index keeps one \
             row per key: {keys:?}"
        );
        for key in &keys {
            assert_eq!(
                tile_key_of(key),
                "s_try",
                "and every one of them still says which tile it belongs to — that is how the \
                 window draws ONE card for them and how Pick up here finds the step again"
            );
        }
    }

    #[test]
    fn no_task_leaves_the_step_prompt_byte_for_byte() {
        assert_eq!(
            with_the_task("", STEP),
            STEP,
            "a run started without a task has to send the step's prompt exactly as the file has \
             it. An empty heading above nothing teaches the model that the section is sometimes \
             empty, and costs length for nothing"
        );
    }

    #[test]
    fn a_task_goes_where_the_file_pointed() {
        let pointed = format!("Context: {TASK_MARK}\n\nWrite the tests first.");
        assert_eq!(
            with_the_task(TASK, &pointed),
            format!("Context: {TASK}\n\nWrite the tests first."),
            "a file that took the trouble to mark the spot knows more about its own prompt than \
             we do, so the task belongs exactly there and nowhere else"
        );
    }

    #[test]
    fn without_a_mark_the_task_goes_on_top_under_a_heading() {
        let out = with_the_task(TASK, STEP);
        assert!(
            out.ends_with(STEP),
            "the step's own prompt has to stay whole and stay last: it usually ends with what to \
             hand back, and a sentence pasted after that reads like a note after the signature. \
             What came out was:\n{out}"
        );
        assert!(
            out.starts_with("What the person asked for"),
            "without a mark the task goes on top, named — an unlabelled sentence glued to a \
             prompt is indistinguishable from a typo in the file. What came out was:\n{out}"
        );
        assert!(
            out.contains(TASK),
            "and it has to actually carry the task. What came out was:\n{out}"
        );
    }

    #[test]
    fn a_mark_left_over_without_a_task_disappears() {
        let pointed = format!("Context: {TASK_MARK}\n\nGo.");
        assert_eq!(
            with_the_task("", &pointed),
            "Context: \n\nGo.",
            "`{{task}}` left in the text is the one thing in the whole prompt the model cannot \
             read as anything but a literal brace — it looks like the broken substitution it is"
        );
    }
}
