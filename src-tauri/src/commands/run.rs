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
//!    znaczy „ktoś wyżej padł" i UI kłamałoby o powodzie (`docs/ARCHITECTURE.md` §5).
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
//! # Czego ta warstwa świadomie NIE robi
//!
//! - **Sama nie ogląda surowego strumienia.** `AgentDriver` oddaje już zdarzenie neutralne, więc
//!   surowych bajtów ta warstwa nie widzi ani jednego. NIE ZNACZY TO, ŻE NIKT ICH NIE ZAPISUJE:
//!   od T-34 `logs/agent-<id>.jsonl` powstaje w każdym biegu i pisze go [`crate::evidence`],
//!   któremu ta warstwa daje wyłącznie katalog biegu i identyfikator kroku
//!   ([`Live::evidence_for_agent`]). Do 2026-08-23 stało tu zdanie odwrotne — „katalog `logs/`
//!   powstaje, ale nikt tam nie pisze" — i było nieprawdą w każdym biegu właściciela, czyli
//!   uczyło następnego pisarza szukać szwu, który już istnieje.
//! - **Nie rozwija `copies`** [T3 §4.4]. Krok z `copies: 3` biegnie tu jako jedna sesja:
//!   rozwinięcie zmienia liczbę węzłów grafu, a `RunReport::steps` jest kontraktem „jeden wpis
//!   na krok pliku". To jest zadanie dla tego, kto zrobi też własne kopie plików.
//! - Kopiuje pliki projektu przy `fresh-copy` (T-33) — patrz [`copy_project_into`].

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

use super::isolate;
use super::triggers::{self, DeliveryState, TriggerClaim, TriggerDelivery, TriggerOrigin};
use super::{Outcome, Part, RunControl, RunDeps, RunError, RunReport, RunRequest};
use crate::engine::StepId;
use crate::engine::dag::Dag;
use crate::engine::drivers::claude::tool_surface;
use crate::engine::drivers::command::{
    CheckHow, CheckSpec, Checking, CommandDriver, GIVE_UP_AFTER,
};
use crate::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Policy, RunSpec,
};
use crate::engine::limits::{self, Limiter};
use crate::engine::line::{Curator, Line, Seen, Status};
use crate::engine::scheduler;
use crate::engine::step::{StepReport, StepState};
use crate::engine::supervisor::GroupProof;
use crate::evidence::{ContextKind, ContextSource, EvidenceTarget, SafeInputManifest};
use crate::inherit::rewrite;
use crate::inherit::wire::{self, Chosen, Inherited, InheritedSourceKind};
use crate::ipc::LineSink;
use crate::library::agents::{Agent, Overrides, Tools, read_agent_file, resolve};
use crate::memory::handoff::{self, Kind, MetaDraft};
use crate::skills::StepSkills;
use crate::workflow::check::{Level, Note, check_to_run};
use crate::workflow::file::load;
use crate::workflow::unroll::{self, Unrolled};
use crate::workflow::{
    AgentStep, CheckOutcome, ConditionalLink, Folder, Handover, Point, RouteEvidence, Skills, Step,
    WhenItFails, WorkflowFile,
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

/// Opis biegu: bieg, jego kroki i migawki. To jest **prawda** (niezmiennik 4).
const RUN_FILE: &str = "run.json";

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

/// Podsumowanie sędziego, dla którego nie było czego sprawdzać.
///
/// **Nie „przeszło".** Krok, który nic nie sprawdził, nie ma prawa czytać się jak krok, który
/// sprawdził i przepuścił — brak ceremonii znaczy „nikt tego nie sprawdził", nigdy „sprawdzone
/// i dobrze" (D7). Zdanie mówi więc, co się naprawdę stało, i mówi to w `run.json`, na karcie
/// kroku i w podsumowaniu biegu.
const NOTHING_CHANGED: &str = "Nothing to check: the step before this one changed no files.";

/// Katalog, pod którym powstają własne kopie plików dla kroków `fresh-copy`.
const WORK_DIR: &str = "work";

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

/// Domyślna etykieta wiersza indeksu: plik zostawił krok, po którym ten krok idzie po strzałce.
///
/// 2026-08-23 (T-87) — ZAMKNIĘTA LISTA ETYKIET ZACZYNA SIĘ TUTAJ i ma dokładnie pięć pozycji.
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

/// Początek etykiety wcześniejszej rundy sędziego. „Tester", bo tak nazywa go człowiek — nasze
/// słowo („judge") nie znaczy nic po drugiej stronie promptu.
const IS_WHAT_THE_TESTER_SAID: &str = "what the tester said last time";

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
/// PO ANGIELSKU I BEZ NASZYCH SŁÓW Z DRUTU, tak jak [`HANDOFF_INDEX_OPENS`] i
/// [`OUTCOME_ASKED_FOR`] obok (decyzja D5, niezmiennik 14): agent czyta „what this step passes
/// on", nigdy „handoff".
const HOW_TO_ANSWER: &str = "\
Your last message is what this step passes on. The step after yours reads it and nothing else, \
so leave nothing worth keeping outside it.

Write it under these three headings, each one alone on its line and in this order:

## Answer
what the step after yours needs to know.

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
    run_workflow_with_slots(deps, request, lines, Limiter::new(request.how_many_at_once)).await
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
    deps.control.begin();
    deps.control.lines_go_to(lines.clone());
    let report = the_whole_triggered_run(
        deps,
        request,
        claim,
        lines,
        Limiter::new(request.how_many_at_once),
    )
    .await;
    deps.control.lines_go_quiet();
    deps.control.settle();
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
    /* „Ruszyliśmy" zapala się PRZED pierwszym `?`, a nie po walidacji, i to jest celowe: bieg
     * odrzucony przez walidator też przechodzi tę funkcję, więc zapali za chwilę `settle()` —
     * a `is_working()` czyta oba znaczniki i odpowie wtedy „nie ma czego zatrzymywać". Zapalenie
     * dopiero po walidacji dałoby okno czasu, w którym bieg już czyta dysk, a zamknięcie okna
     * uznałoby, że nie ma nic do roboty. */
    deps.control.begin();
    /* Strumień oddajemy biegowi DO UCHWYTU, bo tura człowieka przychodzi spoza pętli kroku:
     * komendą z okna, w chwili, w której krok czeka na agenta. Klon, nie przekazanie: pompa ma
     * jednego właściciela, a `LineSink` jest klonowalny właśnie dlatego, że sypie do niej kilku
     * producentów naraz. */
    deps.control.lines_go_to(lines.clone());
    let report = the_whole_run(deps, request, lines, slots).await;
    /* PORZUCAMY NADAJNIK, ZANIM OGŁOSIMY ZEJŚCIE, i ta kolejność ma zmierzony powód. Pompa kończy
     * się na zamkniętej kolejce, czyli dopiero wtedy, gdy zniknie każdy `LineSink` — a nasz klon
     * siedzi w uchwycie. Bez tej linii wisiało piętnaście testów biegu i wisiałby każdy prawdziwy
     * bieg (powód w całości przy `RunControl::lines_go_quiet`). */
    deps.control.lines_go_quiet();
    deps.control.settle();
    report
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
    run_agent_with_slots(deps, ask, lines, Limiter::new(ask.how_many_at_once)).await
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
    // Kolejność i powód każdej z tych czterech linii stoją przy `run_workflow_with_slots`.
    // Ta sama czwórka, nie jej wariant: uchwyt biegu odpowiada na pytanie „czy jest co
    // zatrzymywać" tak samo dla obu rodzajów biegu, bo Stop nie wie, którym z nich jest ten,
    // który idzie.
    deps.control.begin();
    deps.control.lines_go_to(lines.clone());
    let report = the_whole_ask(deps, ask, lines, slots).await;
    deps.control.lines_go_quiet();
    deps.control.settle();
    report
}

/// Bieg od wczytania pliku do zamknięcia księgi. Wydzielony z [`run_workflow_inner`], żeby
/// dowód z `settle()` schodził dokładnie raz, niezależnie od tego, którym `?` się stąd wyszło.
async fn the_whole_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    lines: LineSink,
    slots: Limiter,
) -> Result<RunReport, RunError> {
    the_planned_run(deps, plan_run(deps, request)?, lines, slots, None).await
}

/// Claim triggera przechodzi przez ten sam plan i wykonanie, a ledger oplata tylko dwie
/// atomowe granice: zwiazanie przed katalogiem i akceptacje po pierwszym `run.json`.
async fn the_whole_triggered_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    claim: &TriggerClaim,
    lines: LineSink,
    slots: Limiter,
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
    if let DeliveryState::Accepted { run_file, .. } =
        triggers::reconcile_delivery(deps.home, claim, |bound_file| {
            read_and_sync_run_file(deps.project, bound_file)
        })?
    {
        return Ok(TriggerRunReport::AlreadyAccepted {
            id: claim.run_id.clone(),
            run_file,
        });
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
    };
    match the_planned_run(deps, plan, lines, slots, Some(acceptance)).await {
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
) -> Result<RunReport, RunError> {
    the_planned_run(deps, plan_ask(deps, ask)?, lines, slots, None).await
}

struct TriggerAcceptance {
    home: PathBuf,
    claim: TriggerClaim,
}

/// Rozpisany plan → katalog, kroki, indeks. **Jedna droga wykonania na oba rodzaje biegu.**
///
/// Wydzielone 2026-08-20 (T-62) z [`the_whole_run`], bez zmiany ani jednej linii w środku:
/// od tego miejsca w dół nie ma jak zapytać, czy plan przyszedł z pliku, czy z jednego zdania
/// w wierszu wejścia — i to jest jedyny sposób, żeby „`/ask` to zwykły bieg" było własnością
/// kodu, a nie zdaniem w komentarzu.
async fn the_planned_run(
    deps: &RunDeps<'_>,
    mut plan: Plan,
    lines: LineSink,
    slots: Limiter,
    acceptance: Option<TriggerAcceptance>,
) -> Result<RunReport, RunError> {
    // Graf budujemy po walidatorze, ale przed katalogiem: `Dag::new` odmawia cyklu przy
    // konstrukcji i jest ostatnią linią obrony, nie pierwszą (`engine::dag`).
    let dag = Dag::new(plan.steps.len(), &plan.arrows)?;

    let isolated = lay_out_the_run_dir(&plan, deps.project)?;
    // PRZEKAZANIA POPRZEDNIKÓW, kiedy to jest powtórzenie jednego kroku. Kopia, nie dowiązanie
    // i nie odczyt w miejscu: skończony bieg jest historią i nie ma prawa się zmienić dlatego,
    // że ktoś powtórzył kafelek (niezmiennik 4). Przed `Live::new`, bo prompt kroku czyta
    // indeks przekazań w chwili startu.
    seed_the_handoffs(&plan)?;
    // PRZED `Live::new`, bo ten pochłania `lines`: człowiek ma usłyszeć o brakach
    // zanim ruszy pierwszy agent, a nie po tym, jak zapłacił za jego turę.
    say_what_was_left_behind(&lines, &isolated);
    // Dziedziczenie stoi TU, a nie w `plan_run`: tamta funkcja nie dotyka dysku, a katalog
    // pluginu jest zapisem — i musi lądować pod katalogiem biegu, który dopiero co powstał.
    let inherited = what_the_host_lends(deps.project, &plan.dir)?;
    // Umiejętności kroków lądują TU, obok dziedziczenia i z tego samego powodu: obie drogi piszą,
    // a piszą pod katalog biegu, który dopiero co powstał. Przed `Live::new`, bo odmowa („ten krok
    // pracuje w twoim folderze") ma paść, zanim ruszy pierwszy proces (niezmiennik 12).
    hand_the_skills_to_the_steps(&mut plan)?;
    let live = Arc::new(Live::new(
        plan,
        inherited,
        lines,
        deps.control.clone(),
        slots,
        std::sync::Arc::clone(&deps.processes),
    ));
    // Pierwszy zrzut idzie z `?`: bieg, którego nie da się zapisać na dysk, nie ma prawa ruszyć,
    // bo plikami stoi cała jego historia. Zrzuty w locie są już tylko logowane — patrz
    // [`Live::update`].
    live.open_the_book()?;
    // Ten rename jest granica dokladnie-jeden: ledger domykamy po atomowym pliku, lecz przed
    // pierwszym sterownikiem. Po awarii oba pliki daja sie pogodzic bez SQLite (niezmiennik 4).
    if let Some(acceptance) = acceptance {
        let run_file = live.plan.dir.join(RUN_FILE);
        // Atomowy rename chroni czytelnika przed polowa JSON-u, ale dopiero fsync pliku i
        // katalogu czyni ten rename dowodem, ktory recovery moze przyjac po restarcie procesu.
        if read_and_sync_run_file(deps.project, &run_file)?.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "the run file disappeared before it could be saved safely",
            )
            .into());
        }
        triggers::accept_delivery(&acceptance.home, &acceptance.claim, &run_file, now_ms())?;
    }

    let run_step = {
        let live = Arc::clone(&live);
        move |id: StepId, cancel: CancellationToken| {
            let live = Arc::clone(&live);
            async move { live.step(id, cancel).await }
        }
    };
    // 2026-08-17 (T-31) — semafor planisty ma tu NIC nie ograniczać, i to jest cała treść tego
    // podpięcia. „Ile naraz" jest liczbą CAŁEJ APLIKACJI, więc miejsce bierze każdy krok
    // osobno, ze wspólnej puli ([`Live::a_slot_for_this_step`]). Semafor zakładany per bieg
    // odpowiadał poprawnie na pytanie o jeden bieg i nie odpowiadał w ogóle na to, które zadaje
    // niezmiennik 11: dwie karty po dwa agenty to cztery agenty po ~583 MB, czyli zamrożony
    // laptop, a nie szybsza praca (`docs/ARCHITECTURE.md` §6a).
    //
    // Tyle permitów, ile kroków — czyli tyle, ile trzeba, żeby ten semafor nie odmówił nigdy.
    // Nie zmienia to niczego, przed czym broni `engine::scheduler`: permit wspólnej puli bierze
    // dalej ZADANIE, nie pętla wysyłki, więc różnica między „w kolejce" a „działa" zostaje tam,
    // gdzie była, a szerokość wysyłki dalej nie udaje równoległości.
    let route_after = {
        let live = Arc::clone(&live);
        move |id: StepId, _report: StepReport| live.route_after(id)
    };
    let outcome = scheduler::execute_routed(
        &dag,
        dag.len(),
        deps.control.cancel_token(),
        run_step,
        route_after,
    )
    .await;

    live.close_the_book(&outcome.states, outcome.cancelled);
    // Drzewa domykamy PO księdze, a przed odbudową indeksu: sprzątanie pustego drzewa
    // kasuje katalog `work/<krok>`, więc odbudowa ma czytać stan już posprzątany.
    close_the_trees(deps.project, &isolated, &live.plan.title);
    // Indeks powstaje Z KATALOGU BIEGU, nigdy obok niego (niezmiennik 4): baza nie ma jak
    // powiedzieć niczego, czego nie ma w plikach, bo czyta dokładnie te pliki.
    deps.store.rebuild_from(&live.plan.dir).await?;

    Ok(RunReport {
        id: live.plan.id.clone(),
        dir: live.plan.dir.clone(),
        outcome: if outcome.cancelled {
            Outcome::Cancelled
        } else {
            Outcome::Done
        },
        steps: outcome.states,
    })
}

/// Zatrzymuje bieg i **wraca dopiero z dowodem**, że nic po nim nie żyje.
///
/// Zwraca [`Outcome::Cancelled`] jako wartość, nigdy `Err` (niezmiennik 7). `Ok(())` zaraz po
/// wysłaniu sygnału byłoby tym samym błędem, przed którym broni `GroupProof`: wołający
/// przeczytałby „nie żyje" tam, gdzie napisano „wysłałem SIGTERM" (niezmiennik 6).
///
/// **Warunek dla wołającego (T-07):** ten `RunControl` ma należeć do biegu, który ruszył albo
/// już zszedł. Dowód zapala [`run_workflow_inner`] na każdej swojej drodze wyjścia, więc bieg
/// zakończony i bieg odrzucony wracają stąd natychmiast — ale uchwyt biegu, którego nikt nigdy
/// nie uruchomił, nie ma czego dowieść i czekanie na niego nie ma końca.
pub async fn stop_run_inner(deps: &RunDeps<'_>) -> Result<Outcome, RunError> {
    deps.control.stop();
    // Czekamy na bieg, a nie na siebie. Kroki schodzą po swoich grupach procesów same — tylko
    // one wiedzą, co mają po sobie posprzątać — a `settle()` zapala się dopiero, kiedy
    // `run_workflow_inner` naprawdę wróciło.
    deps.control.wait_until_settled().await;
    // Bieg, którego token jest anulowany, melduje `cancelled` także wtedy, gdy ostatni krok
    // zdążył się udać (`scheduler::execute` czyta token na końcu). Dwa różne zdania o jednym
    // biegu byłyby dwoma miejscami, w których mieszka jedna odpowiedź.
    Ok(Outcome::Cancelled)
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
    /// uuid v7 biegu — sortuje się po czasie.
    id: String,
    /// `<projekt>/.loadout/runs/<ts>__<id>/`. Policzony tutaj, tworzony dopiero po planie.
    dir: PathBuf,
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
    /// Milisekundy epoki: kiedy ten bieg powstał.
    created_at: i64,
    /// Zrodlo triggera; brak pola w JSON zachowuje doslownie ksztalt recznego biegu.
    trigger_origin: Option<TriggerOrigin>,
    /// Kiedy wstała maszyna. Czytane RAZ, przy planowaniu: ten sam bieg ma nosić jedną
    /// odpowiedź, a nie tyle, ile razy ktoś zapyta system.
    boot_id: Option<String>,
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
    /// Katalog, w ktorym to wstaje. Dla serwera dev jest trescia, nie szczegolem: podaje kod
    /// z TEGO drzewa, wiec weryfikacja w kopii kroku oglada dokladnie te prace.
    cwd: PathBuf,
    /// Czy katalog jest nasz - jak [`AgentJob::ours`].
    ours: bool,
}

/// Wszystko, czego krok „sprawdź" potrzebuje, żeby ruszyć — policzone przed startem biegu.
struct CheckJob {
    /// Co uruchomić, po czym poznać i gdzie. Prosto z pliku workflow, bez ani jednego naszego
    /// słowa: komenda jest tym, co człowiek wpisał.
    spec: CheckSpec,
    /// Czy katalog roboczy jest nasz, czyli czy mamy go utworzyć — jak [`AgentJob::ours`].
    ours: bool,
}

/// Czym skończyło się czekanie na turę. Trzy stany, bo `Option` umiał powiedzieć dwa, a od
/// T-35 „skończył się czas" jest czymś innym niż „człowiek nacisnął Stop": pierwsze jest
/// porażką kroku z nazwanym powodem, drugie jest anulowaniem i nie jest niczyją winą.
enum Ended {
    /// Tura wróciła sama — z wynikiem albo z błędem sterownika.
    Turn(anyhow::Result<crate::engine::drivers::Outcome>),
    /// Człowiek nacisnął Stop.
    Stopped,
    /// Krok przekroczył swój limit czasu.
    Overdue,
}

/// Wszystko, czego krok agenta potrzebuje, żeby ruszyć — policzone przed startem biegu.
struct AgentJob {
    /// Sterownik vendora, wzięty z fabryki raz, przy planowaniu.
    driver: Arc<dyn AgentDriver>,
    /// Identyfikator sesji przydzielony **z góry**, przed startem procesu [T7 §6.2]. Dzięki
    /// temu wiadomo, pod jakim numerem zapisać krok, zanim vendor cokolwiek powie.
    session: Uuid,
    /// Katalog roboczy kroku.
    cwd: PathBuf,
    /// Czy ten katalog jest nasz, czyli czy mamy go utworzyć.
    ours: bool,
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
    /// Zatwierdzone Connections rozwiązane podczas planowania, zanim ruszy pierwszy proces.
    connections: Vec<crate::connections::Connection>,
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
    plan_run_with_identity(deps, request, Uuid::now_v7().to_string(), now_ms(), None)
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
    )
}

fn plan_run_with_identity(
    deps: &RunDeps<'_>,
    request: &RunRequest,
    id: String,
    created_at: i64,
    trigger_origin: Option<TriggerOrigin>,
) -> Result<Plan, RunError> {
    // Bajty czytamy osobno od `load()`, bo odcisk ma odpowiadać na pytanie „czy to ten sam
    // PLIK". Odcisk liczony z naszej serializacji odpowiadałby na pytanie „czy to ten sam plik
    // po przejściu przez nas", czyli milczałby o każdej zmianie, której nie rozumiemy.
    let bytes = fs::read(&request.workflow)?;
    let file = load(&request.workflow)?;

    // Bieg nie ufa UI (T3 §5.2): plik mógł zostać zmergowany gitem albo poprawiony ręcznie
    // między zapisem a naciśnięciem Start. Odmawiamy zdaniem WALIDATORA, słowo w słowo —
    // własne tłumaczenie byłoby drugim miejscem, w którym mieszka ten sam komunikat.
    // `check_to_run`, nie `check`: krok bez agenta jest przy zapisie ostrzeżeniem (szkic
    // w połowie zbudowany ma się zapisać), a tutaj problemem — bo za sekundę miałby ruszyć.
    // Powód w całości stoi przy `workflow::check::check_to_run`.
    if let Some(refusal) = check_to_run(&file)
        .into_iter()
        .find(|note| note.level == Level::Problem)
    {
        return Err(RunError::Refused(refusal));
    }

    let dir = run_directory(deps.project, &id, created_at);

    /* ROZWINIĘCIE PĘTLI, i to jest jedyne miejsce, w którym plik przestaje odpowiadać planowi
     * jeden do jednego. `unroll` oddaje graf BEZ cykli o większej liczbie węzłów, więc wszystko
     * niżej — `Dag`, pula miejsc, dowód śmierci grupy, anulowanie — nie widzi żadnej różnicy.
     * Plik bez ani jednego powrotu wychodzi z `unroll` w kształcie 1:1 (dowodzi tego kryterium
     * `a_file_with_no_way_back_comes_out_unchanged`), więc żaden istniejący bieg się nie zmienia. */
    let unrolled = crate::workflow::unroll::unroll(&file);
    let setup = Setup {
        library: deps.home.join(AGENTS_DIR),
        connections: deps.home.join("connections"),
        data: deps.home,
        knows: what_the_agents_know(deps.home),
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
        dir: &dir,
        drivers: &deps.drivers,
        file: &file,
        /* GRAF, NIE SAM PLIK: „krok przede mną" liczy się po ROZWINIĘCIU, więc runda druga pętli
         * schodzi z sędziego rundy pierwszej, a nie z kroku, który stoi przed pętlą. */
        unrolled: &unrolled,
    };
    let wanted = which_nodes(&unrolled, &file, request.part.as_ref());
    /* Gdzie każdy węzeł rozwinięcia wylądował w wycinku. `None` znaczy „nie wszedł" i to jest
     * jedyne miejsce, w którym numeracja wycinka spotyka się z numeracją grafu. */
    let mut place: Vec<Option<StepId>> = vec![None; unrolled.nodes.len()];
    let mut steps = Vec::with_capacity(unrolled.nodes.len());
    for (index, node) in unrolled.nodes.iter().enumerate() {
        let Some(step) = file.steps.get(node.step) else {
            continue;
        };
        if !wanted[index] {
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
        steps.push(plan_step(step, index, node.turn, in_loop, &setup)?);
    }
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
    let arrows: Vec<(StepId, StepId)> = if matches!(request.part, Some(Part::Just(_))) {
        Vec::new()
    } else {
        unrolled
            .arrows
            .iter()
            .filter_map(|(from, to)| {
                Some((*place.get(*from)?.as_ref()?, *place.get(*to)?.as_ref()?))
            })
            .collect()
    };
    /* Pętle z ROZWINIĘCIA, nie z pliku, i ta różnica jest treścią: `unroll` odrzuca powrót,
     * którego ciało przecina cudze (plik z takim powrotem odmawia `check_to_run` kilka linii
     * wyżej, ale ta funkcja nie ma prawa liczyć pętli inaczej niż ten, kto je rozwija). Dzięki
     * temu numer pozycji tutaj, w `Planned::in_loop` i w `settled_at` znaczy wszędzie to samo. */
    let loops: Vec<Loop> = unrolled
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
        .collect();
    // Klucze najpierw, dopiero potem dopisywanie: `steps[child]` i `steps[parent]` naraz to
    // dwie pożyczki jednego wektora, a nie dwie różne rzeczy.
    let keys: Vec<String> = steps.iter().map(|step| step.node_key.clone()).collect();
    for &(parent, child) in &arrows {
        steps[child].depends_on.push(keys[parent].clone());
    }
    let routes = planned_routes(&file, &steps, &arrows)?;
    let memory = what_this_run_knew(&setup.knows, &steps, deps.home);

    // Związane PRZED planem: `setup` pożycza `dir`, a `dir` jedzie do planu przeniesieniem.
    let asked_for = setup.task.clone();
    Ok(Plan {
        id,
        dir,
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
        project: deps.project.to_path_buf(),
        steps,
        memory,
        created_at,
        trigger_origin,
        // Pytamy system RAZ, tutaj: ten bieg ma nosić jedną odpowiedź przez całe życie.
        // Odczyt przy każdym zrzucie dałby wartości, które teoretycznie mogą się różnić —
        // i strażnik porównywałby wtedy coś z czymś innym.
        boot_id: crate::engine::supervisor::machine_booted_at(),
    })
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

    let file = WorkflowFile {
        format: crate::workflow::file::CURRENT,
        id: saved.id.to_string(),
        name: title.clone(),
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
            overrides: Map::new(),
            vendor_options: BTreeMap::new(),
            copies: 1,
            /* ZDANIE CZŁOWIEKA JEST INSTRUKCJĄ TEGO KROKU, więc ląduje w migawce na dysku:
             * bieg, po którym nie da się powiedzieć, o co go poproszono, jest biegiem, którego
             * nie da się potem wyjaśnić (niezmiennik 4). */
            instructions: ask.task.clone(),
            skills: Skills::default(),
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
    };

    /* TEN SAM ROZWIJACZ, CO PRZY PLIKU, choć rozwijać tu nie ma czego: bieg z `/ask` ma jeden
     * kafelek i ani jednej strzałki. Graf policzony tą samą funkcją, a nie wpisany z ręki, bo
     * druga odpowiedź na pytanie „jak wygląda graf tego biegu" rozjechałaby się przy pierwszej
     * zmianie tamtej (niezmiennik 13) — a od tego grafu zależy, gdzie kroki pracują. */
    let unrolled = crate::workflow::unroll::unroll(&file);
    let setup = Setup {
        library,
        connections: deps.home.join("connections"),
        data: deps.home,
        knows: what_the_agents_know(deps.home),
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
        .map(|(node, step)| plan_step(step, node, 0, None, &setup))
        .collect::<Result<Vec<Planned>, RunError>>()?;
    // Ten sam rachunek z pamięci, co przy biegu z pliku: bieg z `/ask` też dostaje blok „co
    // wiadomo", więc też ma po sobie zostawić ślad, co model wtedy wiedział.
    let memory = what_this_run_knew(&setup.knows, &steps, deps.home);
    let graph = serde_json::to_value(&file)?;

    Ok(Plan {
        id,
        dir,
        title,
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
        project: deps.project.to_path_buf(),
        steps,
        memory,
        created_at,
        trigger_origin: None,
        // Pytamy system RAZ, jak przy planie z pliku: ten bieg ma nosić jedną odpowiedź.
        boot_id: crate::engine::supervisor::machine_booted_at(),
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
    /// Notatki odczytane RAZ, zanim ruszył pierwszy proces. Zamrożone: od tej chwili bieg ma
    /// jedną odpowiedź na pytanie, co wiedział.
    notes: Vec<crate::memory::notes::Note>,
}

fn what_the_agents_know(home: &Path) -> Known {
    let root = super::memory::notes_root(home);
    let Ok(notes) = crate::memory::notes::scan_notes(&root) else {
        tracing::debug!(root = %root.display(), "the notes could not be read; no step will carry them");
        return Known {
            text: String::new(),
            sources: Vec::new(),
            notes: Vec::new(),
        };
    };
    let mut text = String::new();
    let mut sources = Vec::new();
    for scope in [
        crate::memory::notes::Scope::Everywhere,
        crate::memory::notes::Scope::ThisProject,
    ] {
        add_block(&mut text, &mut sources, &notes, scope, home);
    }
    Known {
        text,
        sources,
        notes,
    }
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
    notes: &[crate::memory::notes::Note],
    scope: crate::memory::notes::Scope,
    home: &Path,
) {
    let block = crate::memory::notes::what_you_know(notes, crate::memory::notes::Budget::of(scope));
    if block.text.is_empty() {
        return;
    }
    if !text.is_empty() {
        text.push_str("\n\n");
    }
    text.push_str(&block.text);
    for id in &block.used {
        let Some(note) = notes.iter().find(|note| &note.id == id) else {
            continue;
        };
        let Ok(relative) = note.path.strip_prefix(home) else {
            continue;
        };
        sources.push(ContextSource {
            kind: ContextKind::MemoryNote,
            reference: relative.to_string_lossy().into_owned(),
            bytes: note.rule.len(),
        });
    }
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
fn what_this_step_knows(known: &Known, agent: &str, home: &Path) -> (String, Vec<ContextSource>) {
    let mut text = known.text.clone();
    let mut sources = known.sources.clone();

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
        &mine,
        crate::memory::notes::Scope::ThisAgent,
        home,
    );

    (text, sources)
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
}

/// Co ten bieg wiedział, kiedy ruszał — z rachunku KROKÓW, nie z katalogu notatek.
///
/// Liczone z tego, co naprawdę wjechało w prompty (`ContextKind::MemoryNote`), a nie z całego
/// zamrożonego zbioru: notatka, która nie zmieściła się w suficie swojego zakresu, nie dojechała
/// do modelu i nie ma prawa stać w rachunku tak, jakby dojechała. Notatka, która pojawiła się
/// w katalogu po starcie, nie jest tu w ogóle — zbiór jest zamrożony przed pierwszym procesem.
fn what_this_run_knew(known: &Known, steps: &[Planned], home: &Path) -> Vec<MemoryRecord> {
    let carried: BTreeSet<&str> = steps
        .iter()
        .filter_map(|step| match &step.job {
            Job::Agent(job) => Some(job),
            /* `Serve` dokłada się do tego ramienia, a nie dostaje własnego: `ServeJob` niesie
             * `command`, `cwd` i `ours` — ani promptu, ani kontekstu. Krok „uruchom i zostaw"
             * nie wwozi do modelu żadnej notatki, więc nie ma czego policzyć. Osobne ramię
             * z tym samym ciałem paliłoby `match_same_arms`. */
            Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => None,
        })
        .flat_map(|job| job.context.iter())
        .filter(|source| source.kind == ContextKind::MemoryNote)
        .map(|source| source.reference.as_str())
        .collect();

    known
        .notes
        .iter()
        .filter_map(|note| {
            let reference = note.path.strip_prefix(home).ok()?.to_string_lossy();
            carried.contains(reference.as_ref()).then(|| MemoryRecord {
                hash: fingerprint(note.rule.as_bytes()),
                bytes: note.rule.len(),
                reference: reference.into_owned(),
            })
        })
        .collect()
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
/// Ta sama rodzina, co `{{copy}}` i `{{copies}}` [T3 §4.3] — plik już umie mówić o rzeczach,
/// które powstają dopiero przy starcie.
const TASK_MARK: &str = "{{task}}";

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

/// Klucz węzła: `id` kroku z pliku, a dla dalszych rund pętli ten sam klucz z numerem rundy.
///
/// Runda zerowa NIE dostaje sufiksu, i to jest decyzja o wsteczności: plik bez pętli daje wtedy
/// dokładnie te klucze, które dawał przedtem, więc `run.json` starych biegów i nowych da się
/// porównać, a nikt, kto o pętli nie słyszał, nie widzi zmiany.
fn node_key_for(tile_key: &str, turn: u8) -> String {
    if turn == 0 {
        return tile_key.to_owned();
    }
    format!("{tile_key}#{turn}")
}

/// Klucz kafelka z klucza węzła — odwrotność [`node_key_for`].
///
/// **Tutaj, a nie u wołającego**, i to jest jedyny powód, dla którego ta funkcja istnieje:
/// sufit rundy (`#N`) jest kształtem wymyślonym o dwie linie wyżej, więc jego rozbieranie
/// gdziekolwiek indziej byłoby drugą definicją tego samego faktu (niezmiennik 13). Historia
/// czyta `run.json` i musi wiedzieć, o KTÓRY kafelek chodzi, żeby dało się od niego wznowić.
///
/// 2026-08-23 — POWSTAŁO Z DEFEKTU ZE ZRZUTU WŁAŚCICIELA: „Pick up here" podawał `id` kroku
/// z `run.json`, czyli UUID nadany przy planowaniu, a wznowienie szuka po kluczu Z PLIKU. Skutek
/// był zdaniem-zagadką: *„01a02b3c-… is not a step in that workflow any more"* — o kroku, który
/// stoi na płótnie i nigdzie się nie ruszył.
pub(crate) fn tile_key_of(node_key: &str) -> &str {
    node_key.split_once('#').map_or(node_key, |(tile, _)| tile)
}

/// Jeden węzeł rozwiniętego grafu → jeden krok planu.
///
/// `node` jest numerem tego węzła w [`Setup::unrolled`], a nie pozycją kroku w pliku: rundy pętli
/// mają wspólny krok i różne węzły, a „krok przede mną" jest pytaniem o węzeł.
fn plan_step(
    step: &Step,
    node: usize,
    turn: u8,
    in_loop: Option<usize>,
    setup: &Setup<'_>,
) -> Result<Planned, RunError> {
    match step {
        Step::Checkpoint(ask) => Ok(Planned {
            id: Uuid::now_v7().to_string(),
            node_key: node_key_for(&ask.id, turn),
            tile_key: ask.id.clone(),
            turn,
            in_loop,
            name: ask.name.clone(),
            // Kafelek kontrolny JEST pytaniem do czlowieka; drugie pytanie po nim byloby tym
            // samym pytaniem dwa razy.
            when_it_fails: WhenItFails::Stop,
            depends_on: Vec::new(),
            vendor: String::new(),
            job: Job::Ask {
                question: ask.question.clone(),
            },
        }),
        Step::Agent(agent) => {
            let job = plan_agent(agent, node, setup)?;
            Ok(Planned {
                id: Uuid::now_v7().to_string(),
                node_key: node_key_for(&agent.id, turn),
                tile_key: agent.id.clone(),
                turn,
                in_loop,
                name: agent.name.clone(),
                when_it_fails: agent.when_it_fails,
                depends_on: Vec::new(),
                vendor: job.driver.id().to_owned(),
                job: Job::Agent(Box::new(job)),
            })
        }
        Step::Check(check) => {
            let spot = where_it_works(&check.folder, &check.id, &check.name, node, setup)?;
            Ok(Planned {
                id: Uuid::now_v7().to_string(),
                node_key: node_key_for(&check.id, turn),
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
                job: Job::Check(Box::new(CheckJob {
                    spec: CheckSpec {
                        command: check.command.clone(),
                        proof: check.proof.clone(),
                        cwd: spot.cwd,
                    },
                    ours: spot.ours,
                })),
            })
        }
        Step::Serve(serve) => {
            let spot = where_it_works(&serve.folder, &serve.id, &serve.name, node, setup)?;
            Ok(Planned {
                id: Uuid::now_v7().to_string(),
                node_key: node_key_for(&serve.id, turn),
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
                job: Job::Serve(Box::new(ServeJob {
                    command: serve.command.clone(),
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
fn which_nodes(unrolled: &Unrolled, file: &WorkflowFile, part: Option<&Part>) -> Vec<bool> {
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

fn plan_agent(step: &AgentStep, node: usize, setup: &Setup<'_>) -> Result<AgentJob, RunError> {
    let saved = find_agent(&setup.library, &step.agent, &step.name)?;
    // Nadpisania kroku przechodzą przez `Overrides`, więc klucz, którego krok nie ma prawa
    // ruszyć (`id`, `name`, `runsWith`), odbija się o typ, a nie o walidator do zapamiętania.
    let overrides: Overrides = serde_json::from_value(Value::Object(step.overrides.clone()))?;
    let effective = resolve(&saved, &overrides)?.agent;

    // Polityka policzona RAZ i czytana dwa razy: raz jako dial kroku, raz jako sufit jego listy
    // narzędzi. Dwa wywołania tej samej tabeli byłyby dwoma miejscami, w których krok mógłby
    // pojechać z inną polityką, niż ta, którą przepuszczono jego narzędzia.
    let policy = policy_of(effective.file_access);
    let tools = what_this_step_may_use(&effective, policy, step)?;
    let skills = what_this_step_may_reach(setup.data, &saved, &overrides, step)?;
    let connections =
        crate::connections::runtime::selected(&setup.connections, &effective.connections).map_err(
            |error| {
                RunError::Refused(Note {
                    level: Level::Problem,
                    step_id: Some(step.id.clone()),
                    message: error.to_string(),
                    fix: None,
                })
            },
        )?;

    let spot = where_it_works(&step.folder, &step.id, &step.name, node, setup)?;
    // Trzeci blok pamięci powstaje TUTAJ, bo tutaj po raz pierwszy wiadomo, KTÓRY agent
    // biegnie w tym kroku (2026-08-22, T-80). Zbiór notatek jest ten sam dla całego biegu.
    let (knows, mut context) = what_this_step_knows(&setup.knows, &effective.name, setup.data);
    if setup.is_ask {
        if !step.instructions.is_empty() {
            context.push(ContextSource {
                kind: ContextKind::RunTask,
                reference: "ask/task".to_owned(),
                bytes: step.instructions.len(),
            });
        }
    } else {
        if !setup.task.is_empty() {
            context.push(ContextSource {
                kind: ContextKind::RunTask,
                reference: "run/task".to_owned(),
                bytes: setup.task.len(),
            });
        }
        let instruction_bytes = step.instructions.replace(TASK_MARK, "").len();
        if instruction_bytes > 0 {
            context.push(ContextSource {
                kind: ContextKind::WorkflowStep,
                reference: format!("workflow/steps/{node}"),
                bytes: instruction_bytes,
            });
        }
    }

    Ok(AgentJob {
        // Fabryka wołana **raz, przy planowaniu**, a nie w kroku: etykieta vendora stoi
        // w `run.json` od pierwszego zrzutu, więc historia biegu wie, do kogo wracać, także
        // wtedy, gdy krok nigdy nie ruszył.
        driver: (setup.drivers)(effective.runs_with),
        session: Uuid::now_v7(),
        cwd: spot.cwd,
        ours: spot.ours,
        // Treść zadania. `{{copy}}` i `{{copies}}` podstawia dopiero rozwinięcie kroku na kopie
        // [T3 §4.3, §4.4] — tego rozwinięcia w tym zadaniu nie ma i `copies > 1` biegnie tu
        // jako jedna sesja. Podstawienie bez rozwinięcia wpisywałoby w prompt liczbę, której
        // nic po drugiej stronie nie odpowiada.
        // Zadanie kroku POPRZEDZONE tym, co człowiek dopuścił do użytku (`what_the_agents_know`).
        // Bez człowieka blok jest pusty i prompt jest dokładnie zadaniem kroku —
        // `docs/ARCHITECTURE.md` §2 pytanie 5 obiecuje właśnie to.
        // Zadanie CAŁEGO biegu wchodzi do zadania kroku (`with_the_task`), a dopiero to, co z tego
        // wyszło, dostaje blok „co wiadomo". Ta kolejność jest treścią: notatki są kontekstem
        // stojącym nad wszystkim, zadanie biegu jest polem pracy, a prompt kroku jest robotą
        // w tym polu — od najogólniejszego do najkonkretniejszego, czyli tak, jak to czyta model.
        prompt: with_what_we_know(&knows, &with_the_task(&setup.task, &step.instructions)),
        asked: step.instructions.clone(),
        context,
        model: some_text(&effective.model),
        // Prompt systemowy agenta, nie treść zadania: treść zadania w tym polu byłaby
        // niezmiennikiem 9 złamanym po cichu, bo stąd wchodzi do argv.
        system_append: some_text(&effective.instructions),
        policy,
        // Wybór agenta, nie kroku (D6: „wszystko, co vendor wprowadzi, konfigurujemy per agent").
        reaches_the_web: effective.reaches_the_web,
        tools,
        connections,
        skills,
        // Ścieżka katalogu pluginu tego kroku dopiero powstanie: plan nie dotyka dysku, a katalog
        // biegu jeszcze nie istnieje. Wypełnia to [`hand_the_skills_to_the_steps`].
        plugin_flags: Vec::new(),
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
        effective: serde_json::to_value(&effective)?,
    })
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
fn what_this_step_may_use(
    agent: &Agent,
    policy: Policy,
    step: &AgentStep,
) -> Result<Option<Vec<String>>, RunError> {
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
    saved: &Agent,
    overrides: &Overrides,
    step: &AgentStep,
) -> Result<StepSkills, RunError> {
    let picked = overrides.skills.clone().or_else(|| match &step.skills {
        Skills::Every(_) => None,
        Skills::Only(names) => Some(names.clone()),
    });
    StepSkills::for_the_step(data, &saved.skills, picked.as_deref(), &step.name).map_err(
        |missing| {
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
        },
    )
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
    let listing = match fs::read_dir(library) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(RunError::NoAgentsSaved {
                step: step.to_owned(),
            });
        }
        Err(error) => return Err(RunError::Io(error)),
    };
    let mut files: Vec<PathBuf> = listing
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    // Katalog, który istnieje i jest pusty, jest tym samym faktem co katalog, którego nie ma:
    // nie ma z czego wybrać. Dwa różne zdania o jednym stanie byłyby dwoma miejscami prawdy.
    if files.is_empty() {
        return Err(RunError::NoAgentsSaved {
            step: step.to_owned(),
        });
    }
    // `read_dir` nie obiecuje żadnej kolejności, a odpowiedź „którego agenta wzięliśmy" nie ma
    // prawa zależeć od systemu plików.
    files.sort();

    let mut broken = None;
    // Nazwy, które udało się przeczytać. Zbierane po drodze, bo drugi spacer po katalogu byłby
    // drugą odpowiedzią na pytanie „kogo mam zapisanych" (niezmiennik 13).
    let mut saved: Vec<String> = Vec::new();
    for path in files {
        match read_agent_file(&path) {
            Ok(agent) if agent.id.to_string() == id => return Ok(agent),
            Ok(agent) => saved.push(agent.name),
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

/// Gdzie pracuje jeden krok — z odpowiedzią także dla tego, który sam jej nie zna.
///
/// [`Folder::SameCopy`] jest jedynym wariantem, którego nie da się rozstrzygnąć z samego kroku:
/// „to samo drzewo, w którym pracował krok przede mną" jest zdaniem o GRAFIE. Dlatego wejście do
/// rozwiązywania folderu jest tutaj, a nie w [`workspace`] — i jest dalej jedno, bo obie drogi
/// schodzą się w tej funkcji.
///
/// Odmowa zamiast domysłu, w obu brakujących odpowiedziach. Ciche zejście do folderu projektu
/// byłoby dokładnie tą implementacją, przed którą ten wariant powstał: kafelek mówi „to samo
/// drzewo", a krok pisze po prawdziwych plikach człowieka. Pada **przy planowaniu**, czyli zanim
/// ruszy pierwszy proces i zanim powstanie katalog biegu (niezmiennik 12).
fn where_it_works(
    folder: &Folder,
    key: &str,
    name: &str,
    node: usize,
    setup: &Setup<'_>,
) -> Result<Workspace, RunError> {
    if let Some(spot) = workspace(folder, setup.project, setup.dir, key) {
        return Ok(spot);
    }

    let mut before = trees_before(node, setup);
    let refuse = |message: String| {
        Err(RunError::Refused(Note {
            level: Level::Problem,
            // Kropka ląduje na kafelku TEGO kroku: to on nie ma odpowiedzi na pytanie „które
            // drzewo", więc to jego człowiek otworzy.
            step_id: Some(key.to_owned()),
            message,
            fix: None,
        }))
    };
    match before.len() {
        // TO SAMO ZDANIE, CO W WALIDATORZE, i dlatego przychodzi z `workflow::check`. Tą drogą
        // człowiek nie idzie: `check_to_run` mówi to samo kilkadziesiąt linii wcześniej i bieg
        // odmawia tam. Ale ta funkcja musi zwrócić WARTOŚĆ, a jedyną wartością, która tu nie
        // kłamie, jest odmowa — folder projektu wpisany w to miejsce byłby cichym powrotem
        // do wady, którą `same-copy` usuwa.
        0 => refuse(crate::workflow::check::nothing_before(name)),
        1 => Ok(Workspace {
            // Gość w cudzym drzewie: zakłada je krok, który je NAZWAŁ (`fresh-copy`), a bieg
            // robi to raz na katalog roboczy (`lay_out_the_run_dir` dedupikuje po `cwd`).
            // `ours: true` tutaj znaczyłoby dwa kroki, z których każdy chce założyć to samo
            // drzewo, a wtedy o wyniku decyduje kolejność w pliku.
            cwd: before.remove(0),
            ours: false,
        }),
        // FAN-IN Z RÓŻNYCH DRZEW. Krok, przed którym stoją dwa kroki pracujące gdzie indziej,
        // nie ma odpowiedzi na pytanie „które drzewo" — a wybranie pierwszego z brzegu znaczyłoby
        // bieg, w którym poprawka czyta nie ten kod. Żadne kryterium tego nie sądzi (TASK.md,
        // „Świadomie poza zakresem”); odmowa nazywa krok, bo to jedyna rzecz, którą da się
        // powiedzieć uczciwie.
        //
        // Liczba mówi o KATALOGACH, nie o krokach: trzy kroki pracujące w dwóch drzewach są
        // dwiema odpowiedziami, nie trzema, i człowiek ma szukać dwóch miejsc.
        trees => refuse(format!(
            "\"{name}\" is set to work in the same folder as the step before it, and the steps \
             before it work in {trees} different folders. Leave one arrow into it, or give it a \
             fresh copy."
        )),
    }
}

/// W jakich katalogach pracują kroki PRZED tym — bez powtórzeń.
///
/// Obchód idzie po strzałkach **wstecz**, ze zbiorem odwiedzonych: fan-in bywa diamentem, więc
/// bez niego ten sam krok liczyłby się dwa razy i zwykłe rozwidlenie wyglądałoby jak dwa różne
/// drzewa. Iteracyjny, nie rekurencyjny — łańcuch dwudziestu kroków nie ma prawa przepełnić stosu
/// (ta sama zasada, co przy obchodach w `workflow::check`).
///
/// Mija po drodze dwa rodzaje kroków, które drzewa nie wyznaczają: kafelek kontrolny (nie dotyka
/// plików) i kolejny krok „to samo drzewo" (jego odpowiedź jest tym samym pytaniem, zadanym dalej).
/// Stąd „najbliższy poprzednik, jakiegokolwiek rodzaju jest".
///
/// Zero katalogów znaczy „przed tym krokiem nie ma nikogo", więcej niż jeden — „poprzednicy
/// pracują w różnych drzewach". Obie odpowiedzi są odmowami u wołającego.
fn trees_before(node: usize, setup: &Setup<'_>) -> Vec<PathBuf> {
    let mut seen = vec![false; setup.unrolled.nodes.len()];
    // Ten krok od razu jako odwiedzony: strzałka do siebie samego jest kształtem, którego
    // `Dag::new` odmawia, ale obchód nie ma prawa się o nią zapętlić, gdyby jednak tu doszła.
    if let Some(mine) = seen.get_mut(node) {
        *mine = true;
    }
    let mut stack = vec![node];
    let mut found: Vec<PathBuf> = Vec::new();
    while let Some(at) = stack.pop() {
        for &(from, to) in &setup.unrolled.arrows {
            if to != at {
                continue;
            }
            // Numer spoza listy węzłów jest kształtem niemożliwym (`unroll` numeruje węzły
            // i strzałki razem), więc pomijamy go zamiast indeksować: panika w silniku zabiera
            // cały bieg (`AGENTS.md` §4).
            let Some(first_time) = seen.get_mut(from).filter(|been| !**been) else {
                continue;
            };
            *first_time = true;
            let step = setup
                .unrolled
                .nodes
                .get(from)
                .and_then(|one| setup.file.steps.get(one.step));
            // Krok, którego nie ma w pliku, nie wyznacza drzewa i nie ma poprzedników do
            // odpytania — `unroll` numeruje węzły z tego samego pliku, więc to jest kształt
            // niemożliwy, a nie ścieżka, którą ktoś przejdzie.
            let Some((folder, key)) = step.and_then(folder_and_key) else {
                stack.push(from);
                continue;
            };
            match workspace(folder, setup.project, setup.dir, key) {
                Some(spot) if !found.contains(&spot.cwd) => found.push(spot.cwd),
                Some(_) => {}
                // `same-copy`: to samo pytanie, tylko o krok dalej wstecz.
                None => stack.push(from),
            }
        }
    }
    found
}

/// Gdzie krok pracuje i czy ten katalog jest nasz — **jeśli mówi to sam krok**.
///
/// 2026-08-17 (T-33) — `fresh-copy` dostaje **własny katalog z kopią plików projektu**.
///
/// Do tego dnia dostawał katalog PUSTY, a `ARCHITECTURE.md` §2 p. 4 obiecuje „każdy krok dostaje
/// własną kopię twoich plików". To nie była brakująca wygoda: `workflow::check` odmawia zapisu
/// workflow, w którym dwa kroki piszą po tych samych ścieżkach (T-12), i ta walidacja ZAKŁADA,
/// że fresh-copy chroni. Nie chroniła — więc krok „na własnej kopii" pracował na pustce zamiast
/// na projekcie, co jest gorsze od kolizji: agent nie widzi plików, które ma zmienić.
///
/// Ta funkcja dalej tylko WSKAZUJE katalog. Kopiowanie robi [`lay_out_the_run_dir`], bo dotyka
/// dysku, a plan ma być czystym rachunkiem — planowanie, które zapisuje, nie da się powtórzyć
/// przy wznowieniu.
///
/// 2026-08-20 (T-56) — `None` DLA [`Folder::SameCopy`], i to jest cała treść tego wariantu.
/// „To samo drzewo, w którym pracował krok przede mną" jest zdaniem o GRAFIE, a tutaj wchodzi
/// jeden folder i klucz jednego węzła — nie ma z czego wyliczyć odpowiedzi. Odpowiada
/// [`where_it_works`], które widzi strzałki; brak wartości mówi to wprost, zamiast schodzić po
/// cichu do folderu projektu.
fn workspace(folder: &Folder, project: &Path, dir: &Path, node_key: &str) -> Option<Workspace> {
    let (cwd, ours) = match folder {
        Folder::Project => (project.to_path_buf(), false),
        // Katalog wskazany ręcznie jest cudzy: nie tworzymy go, bo „nie ma takiego folderu" jest
        // odpowiedzią, a utworzenie go po cichu zamienia literówkę w pusty bieg.
        Folder::Pick { path } => (PathBuf::from(path), false),
        Folder::FreshCopy => (dir.join(WORK_DIR).join(node_key), true),
        Folder::SameCopy => return None,
    };
    Some(Workspace { cwd, ours })
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

/// Tworzy katalog biegu i to, co do niego należy — **dopiero po planie**.
fn lay_out_the_run_dir(plan: &Plan, project: &Path) -> Result<Vec<Isolated>, RunError> {
    // Proof obejmuje kazdy bieg, takze bez `fresh-copy`. Dopiero on tworzy realny katalog biegu
    // i `logs/`; zadne `create_dir_all` nie moze po cichu przejsc przez symlink przodka.
    prepare_run_directory(project, &plan.dir)
        .map_err(|problem| RunError::Io(io::Error::other(problem.to_string())))?;
    /* JEDEN KATALOG ROBOCZY POWSTAJE RAZ. Rundy petli dziela katalog -- musza, bo inaczej runda 2
     * nie widzi poprawek rundy 1 -- wiec bez tego zbioru zakladalibysmy drzewo N razy w tym samym
     * miejscu, a `git worktree add` odmawia na istniejacym katalogu. */
    let mut made: Vec<Isolated> = Vec::new();
    for step in &plan.steps {
        // Krok „sprawdź" dostaje własne drzewo tą samą drogą, co krok agenta, i to jest wymóg,
        // nie symetria: `cargo test` pisze po `target/`, więc „to tylko sprawdzenie" jest
        // nieprawdą, a obietnica z ARCHITECTURE §2 p. 4 jest jedna dla wszystkich kroków.
        let fresh = match &step.job {
            Job::Agent(job) => job.ours.then_some(&job.cwd),
            Job::Check(job) => job.ours.then_some(&job.spec.cwd),
            // Kafelek „uruchom i zostaw" idzie tą samą drogą i z tego samego powodu. Gdyby jej
            // nie szedł, krok z własną kopią dostałby `cwd` w katalogu, którego nikt nie założył,
            // i odmówiłby na `os error 2` — czyli kontrolka „fresh copy" byłaby na tym kafelku
            // kontrolką, która psuje krok (niezmiennik 16).
            Job::Serve(job) => job.ours.then_some(&job.cwd),
            Job::Ask { .. } => None,
        };
        if let Some(cwd) = fresh
            && !made.iter().any(|one| one.cwd == *cwd)
        {
            let branch = isolate::branch_for(&plan.id, &step.tile_key);
            /* SKĄD ODBIJA SIĘ TO DRZEWO. Przy zwykłym biegu z `HEAD`; przy wznowieniu z gałęzi,
             * na której TEN KAFELEK skończył poprzednio. Powód stoi przy [`where_it_left_off`]
             * i jest z pomiaru, nie z symetrii. */
            let from = where_it_left_off(project, plan.seeded_from.as_deref(), &step.tile_key);
            // Odmowa jest GŁOŚNA i zatrzymuje bieg, zanim ruszy jakikolwiek proces. Ciche
            // zejście do wspólnego katalogu dałoby dwa kroki piszące po tych samych plikach,
            // z których każdy skończyłby się „sukcesem" (niezmiennik 12).
            let done = make_or_recover_tree(
                project,
                &plan.dir,
                cwd,
                &branch,
                from.as_deref().unwrap_or("HEAD"),
            )
            .map_err(|why| RunError::NoFreshCopy {
                step: step.name.clone(),
                why: why.to_string(),
            })?;
            made.push(Isolated {
                step: step.name.clone(),
                cwd: cwd.clone(),
                branch: match done.how {
                    isolate::How::Tree { branch } => Some(branch),
                    isolate::How::Copy => None,
                },
                left_behind: done.left_behind,
            });
        }
    }
    Ok(made)
}

/// Powtarza layout po awarii miedzy `bind` i pierwszym `run.json`.
///
/// W tym oknie sterownik jeszcze nie ruszyl, wiec katalog kopii nie niesie pracy agenta.
/// Worktree gita juz niesie natomiast naniesiony diff czlowieka: jego nie wolno skasowac ani
/// nakladac drugi raz, dlatego wraca tylko po dowodzie oczekiwanej galezi.
/// Gałąź, na której ten kafelek skończył w poprzednim biegu — albo `None`.
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
/// # Po KAFELKU, nie po nazwie gałęzi z tamtego biegu
///
/// Bo kafelek jest tym, co przeżywa bieg. `branch_for` składa nazwę z identyfikatora biegu
/// i klucza kafelka, więc pytanie „gdzie ten kafelek skończył ostatnio" ma dokładnie jedną
/// odpowiedź, a składamy ją tą samą funkcją, która tamtą nazwę nadała (niezmiennik 13).
///
/// `None`, kiedy czegokolwiek brakuje — nie ma poprzedniego biegu, nie da się przeczytać jego
/// `run.json`, albo gałąź została skasowana. Wtedy drzewo odbija się od `HEAD`, czyli robi to,
/// co robiło zawsze. Cichy powrót jest tu poprawny: „nie było czego przenieść" i „przeniesiono"
/// dają to samo drzewo, kiedy poprzedni bieg tego kafelka nie tknął.
fn where_it_left_off(project: &Path, previous: Option<&Path>, tile: &str) -> Option<String> {
    let bytes = fs::read(previous?.join(RUN_FILE)).ok()?;
    let described: Value = serde_json::from_slice(&bytes).ok()?;
    let branch = isolate::branch_for(described.get("id")?.as_str()?, tile);
    // Sprawdzamy, ŻE ISTNIEJE, zanim ją podamy: `git worktree add` z nieistniejącym punktem
    // startu odmawia całemu biegowi, a brak gałęzi po skasowanym biegu jest zwykłym stanem.
    isolate::names_a_commit(project, &branch).then_some(branch)
}

fn make_or_recover_tree(
    project: &Path,
    run_dir: &Path,
    cwd: &Path,
    branch: &str,
    from: &str,
) -> Result<isolate::Made, isolate::Trouble> {
    // Walidacja stoi przed `exists`, `make` i `remove_dir_all`: inaczej niebezpieczny klucz
    // albo symlink przodka moze wskazac ofiare poza biegiem, zanim cleanup zobaczy cel.
    let marker_path = prove_generated_work_path(project, run_dir, cwd)?;
    let marker = read_isolation_marker(&marker_path)?;
    if !isolate::is_a_repo(project) {
        /* Kopia plikowa punktu startu nie zna i znać nie może: bez gita nie ma gałęzi, na której
         * poprzedni bieg mógłby cokolwiek zostawić. Wznowienie w projekcie bez repozytorium
         * dostaje więc to, co dostawało zawsze — kopię tego, co leży w projekcie. */
        return make_or_recover_file_copy(project, cwd, branch, marker.is_some());
    }
    make_or_recover_git_tree(project, cwd, branch, from, &marker_path, marker.as_ref())
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
    project: &Path,
    cwd: &Path,
    branch: &str,
    has_git_marker: bool,
) -> Result<isolate::Made, isolate::Trouble> {
    if has_git_marker {
        return Err(isolate::Trouble::Git(
            "the retry found a git isolation record, but this project is no longer the same git repository"
                .to_owned(),
        ));
    }
    if !path_entry_exists(cwd)? {
        return isolate::make(project, cwd, branch);
    }
    if !cwd.is_dir() {
        return Err(isolate::Trouble::Copying(io::Error::other(
            "the retry found an unexpected path where its file copy belongs",
        )));
    }
    // 2026-08-21, T-65: brak `run.json` dowodzi, ze zaden driver nie wystartowal. Usuwamy
    // wylacznie wygenerowana, potencjalnie polowiczna kopie pod tym samym katalogiem biegu.
    fs::remove_dir_all(cwd).map_err(isolate::Trouble::Copying)?;
    isolate::make(project, cwd, branch)
}

fn make_or_recover_git_tree(
    project: &Path,
    cwd: &Path,
    branch: &str,
    from: &str,
    marker_path: &Path,
    marker: Option<&IsolationMarker>,
) -> Result<isolate::Made, isolate::Trouble> {
    if let Some(marked) = marker
        && marked.branch() != branch
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
            None => {
                // Ten fsync jest PRZED pierwszym skutkiem cleanup. Po awarii marker jest
                // uprawnieniem wylacznie do tej sciezki, galezi i tego niezmienionego OID.
                write_isolation_marker(
                    marker_path,
                    &IsolationMarker::Recovering {
                        branch: branch.to_owned(),
                        head,
                    },
                )?;
            }
        }
        cleanup_incomplete_worktree(project, cwd, branch, marker_path)?;
    } else {
        match marker {
            Some(IsolationMarker::Recovering { .. }) => {
                cleanup_incomplete_worktree(project, cwd, branch, marker_path)?;
            }
            Some(IsolationMarker::Complete { .. }) => {
                return Err(isolate::Trouble::Git(
                    "the completed work tree is missing; nothing was removed".to_owned(),
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
    }

    let made = isolate::make_from(project, cwd, branch, from)?;
    if matches!(&made.how, isolate::How::Tree { .. }) {
        let head = branch_oid(project, branch)?;
        // Dopiero caly `isolate::make` (worktree, dirty diff i lista brakow) moze wystawic
        // marker. Fsync pliku i katalogu stoi w helperze przed zwrotem do layoutu.
        write_isolation_marker(
            marker_path,
            &IsolationMarker::Complete {
                branch: branch.to_owned(),
                head,
            },
        )?;
    }
    Ok(made)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum IsolationMarker {
    Complete { branch: String, head: String },
    Recovering { branch: String, head: String },
}

impl IsolationMarker {
    fn branch(&self) -> &str {
        match self {
            Self::Complete { branch, .. } | Self::Recovering { branch, .. } => branch,
        }
    }
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

    let cwd_exists = path_entry_exists(cwd)?;
    let admin = expected_worktree_admin(project, cwd, branch, &marked_head)?;
    if cwd_exists {
        if !worktree_points_at(project, cwd, branch) || branch_oid(project, branch)? != marked_head
        {
            return Err(isolate::Trouble::Git(
                "the work tree no longer matches its recovery record; nothing was removed"
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
        if !branch_exists(project, branch)? || branch_oid(project, branch)? != marked_head {
            return Err(isolate::Trouble::Git(
                "the recovery branch no longer matches its recorded commit; nothing was removed"
                    .to_owned(),
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
            "git reported removing the incomplete work tree, but its path still exists; the branch was kept"
                .to_owned(),
        ));
    }
    if expected_worktree_admin(project, cwd, branch, &marked_head)? != ExpectedWorktreeAdmin::Absent
    {
        return Err(isolate::Trouble::Git(
            "git kept the incomplete work tree administration; the branch was kept".to_owned(),
        ));
    }
    if branch_exists(project, branch)? {
        // `update-ref` laczy porownanie i kasowanie w jednej operacji CAS. Reczne przesuniecie
        // galezi miedzy osobnym `rev-parse` i `branch -D` nie moze wpasc w okno TOCTOU.
        let reference = format!("refs/heads/{branch}");
        git_for_recovery(project, &["update-ref", "-d", &reference, &marked_head])?;
    }
    if branch_exists(project, branch)? {
        return Err(isolate::Trouble::Git(
            "git kept the recovery branch after its guarded removal".to_owned(),
        ));
    }
    remove_isolation_marker(marker_path)
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
    /// Nazwa kroku — ta z kafelka, bo to jej szuka człowiek.
    step: String,
    /// Katalog roboczy kroku.
    cwd: PathBuf,
    /// Gałąź, jeśli to jest drzewo gita. `None` dla folderu, który repozytorium nie jest.
    branch: Option<String>,
    /// Pliki, o których git nie wie, więc drzewo ich nie niesie.
    left_behind: Vec<String>,
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
fn seed_the_handoffs(plan: &Plan) -> io::Result<()> {
    let Some(from) = &plan.seeded_from else {
        return Ok(());
    };
    let source = from.join(crate::store::rebuild::HANDOFFS_DIR);
    let Ok(listing) = fs::read_dir(&source) else {
        return Ok(());
    };
    let into = plan.dir.join(crate::store::rebuild::HANDOFFS_DIR);
    fs::create_dir_all(&into)?;
    for entry in listing {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            fs::copy(entry.path(), into.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Zamyka drzewa po biegu: praca ląduje na gałęzi, a po kroku, który nic nie zmienił, nie
/// zostaje ani gałąź, ani wpis w `git worktree list`.
fn close_the_trees(project: &Path, made: &[Isolated], title: &str) {
    for one in made {
        let Some(branch) = &one.branch else { continue };
        let kept = isolate::finish(
            project,
            &one.cwd,
            branch,
            &format!("{}: {}", title, one.step),
        );
        tracing::debug!(step = %one.step, ?kept, "the step's tree was closed");
    }
}

/// Co ten bieg bierze z repozytorium, w którym pracuje: katalog pluginu z wybranymi
/// umiejętnościami (jedzie argv) i tekst z learnings oraz podagenta (jedzie promptem).
///
/// # Dlaczego dziś oddaje zawsze pusto — i czyja to decyzja
///
/// `Chosen` jest tu **stałą pustą**, bo wybór człowieka nie ma dziś nośnika: nazwy zaznaczonych
/// pozycji musiałyby przyjść razem z naciśnięciem Start, czyli polem w [`RunRequest`] — a ten
/// typ mieszka w `commands/mod.rs`, który **nie leży w bloku OWNS** T-57 (`AGENTS.md` §7).
/// Zbudowanie sobie w zamian własnego źródła prawdy (plik pod `~/.loadout/`, którego nikt nie
/// zapisuje) byłoby czytaniem artefaktu, którego nie pisze żaden skrypt — czyli niezmiennikiem
/// 21 złamanym od drugiej strony.
///
/// **To nie jest wołający na pokaz.** Pusty wybór jest stanem domyślnym z AC-4 i biegnie tu tę
/// samą drogą, którą pojedzie wybór niepusty: katalog biegu, prompt każdego kroku i argv
/// sterownika są już spięte i sądzone czterema kryteriami. Brakuje **jednej** rzeczy — pola,
/// którym ekran powie, co człowiek zaznaczył. Do tego czasu bieg zachowuje się dokładnie tak,
/// jak obiecuje AC-4: pełne `.claude/` w cudzym repozytorium nie jest zgodą.
///
/// Liczone RAZ na bieg, nie per krok, dokładnie jak [`Setup::knows`] i z tego samego powodu:
/// dwa kroki jednego biegu mają czytać ten sam kontekst, a różnicy nie widać nigdzie poza
/// rachunkiem za długość.
fn what_the_host_lends(project: &Path, run_dir: &Path) -> Result<Inherited, RunError> {
    wire::from_the_host(project, run_dir, &Chosen::default()).map_err(|error| {
        /* `RunError` nie ma wariantu na dziedziczenie, a `commands/mod.rs` nie leży w bloku OWNS
         * tego zadania. `Io` jest wariantem PRZEZROCZYSTYM, więc niesie zdanie odmowy co do
         * słowa — a to zdanie jest tu całą treścią: „Loadout was told to bring in the skill
         * \"x\" …" wymienia pozycję, której zabrakło, i po to zostało napisane. */
        RunError::Io(io::Error::other(error))
    })
}

/// Kładzie umiejętności każdego kroku tam, gdzie vendorzy naprawdę zaglądają — **obiema drogami**.
///
/// DWIE PÓŁKI, BO VENDORZY MAJĄ DWIE. Claude Code przyjmuje katalog umiejętności wyłącznie
/// argumentem (`--plugin-dir`, [S1 §3]); pozostałych pięciu nie umie go przyjąć w ogóle i czyta
/// `.agents/skills/` w katalogu roboczym kroku [T5 §3.1]. Kładziemy więc obie i nie pytamy, który
/// vendor to jest: warunek nazywający vendora w tym miejscu jest dokładnie tym drugim zestawem
/// reguł, przez który w repo źródłowym po cichu umarło skanowanie sekretów (niezmiennik 23).
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
        let ours = job.cwd.starts_with(&run_dir);
        job.skills
            .into_the_step_folder(&job.cwd, ours, &name)
            .map_err(|refusal| refused_by_the_skills(&refusal, tile_key))?;

        let into = run_dir.join(STEP_SKILLS_DIR).join(&node_key);
        let carried = rewrite::plugin_dir_from_the_library(&job.skills, &into)
            .map_err(|error| RunError::Io(io::Error::other(error)))?;
        job.plugin_flags = rewrite::plugin_argv(&carried);
    }
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

/// Bieg w trakcie: plan (niezmienny) plus księga (zmienna), plus to, czym mówi do świata.
struct Live {
    /// Wszystko, co rozstrzygnięto przed startem.
    plan: Plan,
    /// Co ten bieg wziął z repozytorium gospodarza: fragment argv i tekst do promptu.
    ///
    /// Jedna wartość na bieg, policzona raz ([`what_the_host_lends`]), bo katalog pluginu jest
    /// jeden i jego ścieżka nie ma prawa różnić się między krokami. Trzyma to `Live`, a nie
    /// [`Plan`], dokładnie z tego powodu, dla którego stoi obok: `Plan` powstaje **przed**
    /// katalogiem biegu, a dziedziczenie do tego katalogu pisze.
    inherited: Inherited,
    /// Stan, który zmienia się w trakcie. Zamek jest `std::sync::Mutex`, a każde jego wzięcie
    /// mieści się w jednym wywołaniu bez `await` (niezmiennik 8, `clippy::await_holding_lock`
    /// = deny).
    book: Mutex<Book>,
    /// Linie na ekran, **po jednej**. Sklejaniem zajmuje się pompa z T-07 i tylko ona: bieg,
    /// który skleja u siebie, ustala okno, którego nikt nie zmierzył, i odbiera pompie jedyną
    /// rzecz, dla której ta pompa powstała.
    lines: LineSink,
    /// Stop i Continue sięgają tędy do środka biegu.
    control: RunControl,
    /// Rejestr rzeczy, które mają zostać żywe po swoim kroku (kafelek „uruchom i zostaw").
    processes: std::sync::Arc<crate::commands::processes::Processes>,
    /// Wspólna pula miejsc **całej aplikacji** i pauza dostawcy — jedne drzwi dla obu
    /// (`engine::limits`, nagłówek pliku): wysyłka pyta bieg, bieg pyta pulę. Uchwyt jest
    /// klonem cudzej puli, nigdy własną: pula zakładana per bieg jest nie do odróżnienia od
    /// tej, przez którą dwie karty dają `2 × limit` agentów naraz (niezmiennik 11).
    gate: limits::Run,
    /// Chwila startu biegu. Kurator dostaje czas **argumentem**, bo kurator z własnym zegarem
    /// nie da się przetestować bez `sleep`.
    began: Instant,
    /// Gdzie leży przekazanie każdego kroku — po jednym wpisie na krok, w kolejności z pliku
    /// workflow. `None` znaczy „ten krok jeszcze nic nie oddał": kafelek kontrolny nie oddaje
    /// nigdy, a krok anulowany albo padnięty nie ma czego przekazać.
    ///
    /// Zamek osobny od [`Live::book`] z rozmysłem: to nie jest stan, który jedzie do `run.json`.
    /// Ścieżka przekazania **jest** w plikach — nazwa pliku otwiera się numerem kroku — więc
    /// druga kopia w `run.json` byłaby drugim miejscem, w którym mieszka jeden fakt
    /// (niezmiennik 13), i tym, które kłamie po pierwszej ręcznej edycji katalogu.
    ///
    /// **Nie przechodzi przez `await`** (niezmiennik 8): oba wywołania, które go biorą
    /// ([`Live::filed`], [`Live::handed_before`]), oddają go w tym samym wyrażeniu.
    handoffs: Mutex<Vec<Option<PathBuf>>>,
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
    did_not_pass: Mutex<Vec<bool>>,
    /// Runda, w której pętla się DOMKNĘŁA — po jednej pozycji na pętlę planu, w tej samej
    /// kolejności co [`Plan::loops`]. `None` na pozycji znaczy „ta pętla jeszcze nie przeszła".
    ///
    /// 2026-08-22 — WEKTOR, NIE JEDNO POLE. Przy dwóch pętlach jedno pole znaczyłoby, że werdykt
    /// `pass` w gałęzi frontowej pomija rundy gałęzi backendowej — czyli praca, której nikt nie
    /// sprawdził, jedzie dalej jako zrobiona. Czytane przed każdym krokiem ciała pętli i przez to
    /// jedyny nośnik faktu „dalszych rund TEJ pętli już nie potrzebujemy".
    settled_at: Mutex<Vec<Option<u8>>>,
    /// Wynik kroku używany wyłącznie przez zapisane warunki jego strzałek.
    route_evidence: Mutex<Vec<Option<RouteEvidence>>>,
    /// Trwały dowód wyboru, kopiowany do `run.json` przy każdym zrzucie.
    route_decisions: Mutex<Vec<RouteDecision>>,
}

/// Zmienna połowa biegu — dokładnie to, co zmienia się między zrzutami `run.json`.
struct Book {
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
}

/// Co bieg wie o jednym kroku.
#[derive(Debug, Clone)]
struct StepRun {
    /// Stan kroku. `paused` tu nie istnieje i nie ma go w [`StepState`] — to jest stan biegu.
    status: StepState,
    /// Kiedy krok ruszył.
    started_at: Option<i64>,
    /// Kiedy się skończył.
    ended_at: Option<i64>,
    /// Proces potomny, jeśli sterownik go miał.
    pid: Option<i32>,
    /// Grupa procesów — to po niej sprząta odzyskiwanie po awarii (T-20).
    pgid: Option<i32>,
    /// Kod wyjścia.
    exit_code: Option<i32>,
    /// Czy supervisor dostał z jądra dowód, że cała grupa procesu nie żyje.
    ///
    /// `false` nie znaczy „żyje”: naturalne `close()` zbiera lidera, ale nie produkuje
    /// [`GroupProof`]. `true` zapisujemy wyłącznie na ścieżce Stop/limitu, która naprawdę
    /// dostała [`GroupProof::Dead`] (niezmiennik 6).
    death_proof: bool,
    /// Ile kosztował.
    cost_usd: Option<f64>,
    /// Rzeczywiste liczniki z terminalnego [`crate::engine::drivers::Outcome`]. `None` znaczy,
    /// że krok nie dostał wyniku agenta (np. Check albo odmowa przed startem), nie zero.
    turns: Option<u32>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_tokens: Option<u64>,
    /// Jedna linia dla szyny agentów.
    summary: Option<String>,
    /// Powód, jeśli coś poszło nie tak.
    error: Option<String>,
    /// Nagłówki, których agent nie napisał, a `memory::handoff::reshape` je za niego wstawił.
    ///
    /// Pusta lista znaczy, że odpowiedź przyszła w umówionym kształcie — i to jest odpowiedź,
    /// a nie brak odpowiedzi (powód przy [`StepEntry::repaired`]).
    repaired: Vec<String>,
    /// Czy odpowiedź nie zmieściła się w `BODY_CAP` i część leży w `attachments/`.
    truncated: bool,
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
    /// Czym ten plik jest dla kroku, który go czyta. Powód całego pola stoi przy
    /// [`IS_WHAT_THE_STEP_BEFORE_LEFT`].
    what: WhatItIs,
}

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
    /// Wcześniejsza runda sędziego tej pętli.
    WhatTheTesterSaid { which: u8, of: u8 },
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
            Self::WhatTheTesterSaid { which, of } => {
                format!("{IS_WHAT_THE_TESTER_SAID}, try {which} of {of}")
            }
        }
    }
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

impl Live {
    /// Świeży bieg: wszystkie kroki czekają, nic jeszcze nie ruszyło.
    fn new(
        plan: Plan,
        inherited: Inherited,
        lines: LineSink,
        control: RunControl,
        slots: Limiter,
        processes: std::sync::Arc<crate::commands::processes::Processes>,
    ) -> Self {
        // Kopia stanów kroków, którą dostaje limit dostawcy, jest **martwa z rozmysłem**:
        // `engine::limits::Run` ma pełny dostęp do statusów i podejść dokładnie po to, żeby
        // T-21 mogło dowieść, że pauza ich nie rusza (`[T7 §7.2]`: „a pause, not a failure").
        // Księga tego biegu żyje niżej, w [`Live::book`], i to ona jedzie do `run.json`.
        let gate = limits::Run::new(slots, &vec![StepState::Pending; plan.steps.len()]);
        let steps = plan
            .steps
            .iter()
            .map(|_| StepRun {
                status: StepState::Pending,
                started_at: None,
                ended_at: None,
                pid: None,
                pgid: None,
                exit_code: None,
                death_proof: false,
                cost_usd: None,
                turns: None,
                input_tokens: None,
                output_tokens: None,
                cached_tokens: None,
                summary: None,
                error: None,
                repaired: Vec::new(),
                truncated: false,
            })
            .collect();
        let handoffs = Mutex::new(vec![None; plan.steps.len()]);
        let did_not_pass = Mutex::new(vec![false; plan.steps.len()]);
        let settled_at = Mutex::new(vec![None; plan.loops.len()]);
        let route_evidence = Mutex::new(vec![None; plan.steps.len()]);
        Self {
            plan,
            inherited,
            book: Mutex::new(Book {
                status: RunState::Running,
                asking: false,
                started_at: None,
                ended_at: None,
                steps,
            }),
            lines,
            control,
            gate,
            began: Instant::now(),
            handoffs,
            did_not_pass,
            settled_at,
            route_evidence,
            route_decisions: Mutex::new(Vec::new()),
            processes,
        }
    }

    /// Zamek na księdze. Zatruty odplatamy zamiast panikować: `panic!` w silniku zabiera cały
    /// bieg (AGENTS.md §4), a księga po panice jednego kroku jest dalej poprawna.
    fn book(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Pierwszy zrzut `run.json`. Jego błąd zatrzymuje bieg, bo bieg bez pliku nie ma historii.
    fn open_the_book(&self) -> Result<(), RunError> {
        let book = self.book();
        self.spill(&book)
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
    /// Czy ten węzeł jest rundą pętli, która już się domknęła.
    ///
    /// Porównanie jest na NUMERZE RUNDY, nie na „czy pętla przeszła": rundy do tej, w której padł
    /// werdykt `pass`, naprawdę się wykonały i ich stan jest prawdziwy. Pomijamy wyłącznie to,
    /// co jest PO niej.
    fn already_settled(&self, id: StepId) -> bool {
        let step = &self.plan.steps[id];
        /* Tylko ciało pętli, i to TEJ pętli, do której ten węzeł należy. Krok spoza wszystkich
         * pętli ma rundę zero i nigdy nie zostałby pominięty, ale warunek stoi tu wprost, żeby
         * ten kod nie zależał od tego, jak `unroll` numeruje. */
        let Some(which) = step.in_loop else {
            return false;
        };
        if step.turn == 0 {
            return false;
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
            .is_some_and(|turn| step.turn > turn)
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
        let fields: BTreeMap<String, String> = text
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
            .filter(|(name, value)| !name.is_empty() && !value.is_empty())
            .collect();
        self.remember_evidence(id, RouteEvidence::Handoff(fields));
    }

    fn route_after(&self, id: StepId) -> scheduler::Route {
        let relevant: Vec<&PlannedRoute> = self
            .plan
            .routes
            .iter()
            .filter(|route| route.from == id)
            .collect();
        if relevant.is_empty() {
            return scheduler::Route::All;
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
            Ok(None) => return scheduler::Route::All,
            Err(error) => return self.refuse_route(id, &error.to_string()),
        };
        let Some(route) = relevant
            .into_iter()
            .find(|route| route.link.to == selected.to)
        else {
            return self.refuse_route(id, "The selected next step is not in this run.");
        };
        let Some(evidence) = evidence else {
            return self.refuse_route(
                id,
                "This step did not produce the value needed to choose what runs next.",
            );
        };
        self.route_decisions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(RouteDecision {
                step_id: self.plan.steps[id].tile_key.clone(),
                to: self.plan.steps[route.to].tile_key.clone(),
                evidence,
            });
        scheduler::Route::Only(vec![route.to])
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
    fn verdict_after(&self, id: StepId, said: &str) -> Option<&'static str> {
        let step = &self.plan.steps[id];
        let (which, the_loop) = self.judging(step)?;
        if crate::memory::handoff::verdict_in(said) == crate::memory::handoff::Verdict::Pass {
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
            WhenItFails::CarryOn => StepReport::FailedAndCarriedOn,
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
                    StepReport::Succeeded => StepReport::FailedAndCarriedOn,
                    other => other,
                }
            }
        }
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

    /// Czy ciało tej pętli zostawiło cokolwiek do sprawdzenia.
    ///
    /// Pyta o drzewo kroku, DO którego wraca powrót — czyli implementera. Sędzia z własną świeżą
    /// kopią ma drzewo puste zawsze, więc pytanie postawione u niego pomijałoby każdą weryfikację.
    ///
    /// Krok pracujący wprost w folderze człowieka (`folder: project`) nie jest tu rozstrzygalny
    /// — jego „drzewo" to całe repo z cudzą pracą w środku — i wtedy odpowiadamy `false`, czyli
    /// „jest co sprawdzać". Milczenie w stronę weryfikacji, nigdy w stronę jej pominięcia.
    fn nothing_to_judge(&self, which: usize) -> bool {
        let Some(the_loop) = self.plan.loops.get(which) else {
            return false;
        };
        let Some(entry) = self
            .plan
            .steps
            .iter()
            .find(|step| step.tile_key == the_loop.entry)
        else {
            return false;
        };
        let Job::Agent(job) = &entry.job else {
            return false;
        };
        // Folder projektu znaczy „nie wiem": zmiany w nim mogą być czyjekolwiek.
        if !job.ours {
            return false;
        }
        !crate::commands::isolate::touched(&self.plan.project, &job.cwd)
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
    /// ([`Live::already_settled`]) — nie przepalone. To jest jedyne miejsce, w którym wyjście
    /// komendy rozstrzyga o kształcie biegu, i jedyna różnica między „domknęło się na tym, co się
    /// stało" a „domknęło się na tym, co ktoś powiedział".
    fn verdict_of_a_check(&self, id: StepId, passed: bool) -> bool {
        let step = &self.plan.steps[id];
        // Krok „sprawdź" stojący w pętli, ale nie na jej powrocie, jest zwykłym krokiem
        // i jego werdykt jest wprost jego stanem.
        let Some((which, the_loop)) = self.judging(step) else {
            return !passed;
        };

        if passed {
            self.settle(which, step.turn);
            return false;
        }
        step.turn + 1 >= the_loop.turns
    }

    /// Podnosi proces kafelka „uruchom i zostaw" i **oddaje go rejestrowi**.
    ///
    /// Wraca, gdy proces WSTAŁ — nie gdy zejdzie. Czekanie na koniec zatrzymałoby graf na
    /// zawsze: serwer dev nie kończy się nigdy i właśnie o to w nim chodzi. Powód całego kroku
    /// stoi przy [`crate::workflow::ServeStep`] i jest zmierzony na biegu właściciela.
    ///
    /// Nie `async`: `Processes::start` wraca natychmiast, z `pgid` już w ręku.
    fn start_and_leave(&self, id: StepId, job: &ServeJob) -> StepReport {
        let line = job.command.trim();
        if line.is_empty() {
            // Odmowa, nie ciche przejście: krok bez komendy jest kafelkiem bez skutku, a bieg,
            // który go „wykona", uczy człowieka, że ten kafelek działa.
            return self.refuse_step(
                id,
                "This step has no command, so there is nothing to start.",
            );
        }
        match self
            .processes
            .start(&crate::engine::drivers::command::StartSpec {
                command: line.to_owned(),
                cwd: job.cwd.clone(),
            }) {
            Ok(started) => {
                self.update(|book| {
                    book.steps[id].summary = Some(format!("Started and left running: {line}"));
                    book.steps[id].pgid = Some(started.pgid);
                });
                StepReport::Succeeded
            }
            // Zdanie mówi, CO nie wstało: `os error 2` samo nie mówi nic (DESIGN §8).
            Err(error) => {
                self.refuse_step(id, &format!("Loadout could not start \"{line}\": {error}"))
            }
        }
    }

    /// Krok, który nie ruszył, z powodem zapisanym tam, gdzie człowiek go szuka.
    fn refuse_step(&self, id: StepId, said: &str) -> StepReport {
        self.update(|book| book.steps[id].error = Some(said.to_owned()));
        StepReport::Failed
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

    /// Księga → `run.json`, przez plik tymczasowy i `rename`.
    fn spill(&self, book: &Book) -> Result<(), RunError> {
        let text = serde_json::to_string_pretty(&self.run_file(book))?;
        let writing = self
            .plan
            .dir
            .join(format!(".run.json.{}.writing", Uuid::now_v7()));
        let result = (|| -> io::Result<()> {
            // `create_new` jest atomowym no-follow dla ostatniego komponentu. Losowa nazwa
            // usuwa wspolny cel miedzy zrzutami, wiec przygotowany symlink nie moze zostac
            // otwarty przez `truncate`, jak dawny staly `run.json.writing`.
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&writing)?;
            file.write_all(text.as_bytes())?;
            drop(file);
            fs::rename(&writing, self.plan.dir.join(RUN_FILE))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&writing);
        }
        result?;
        // `rename` w obrębie jednego katalogu jest atomowe: czytelnik widzi albo poprzedni plik
        // w całości, albo nowy w całości, i nigdy zera bajtów w środku.
        Ok(())
    }

    /// Widok `run.json` na tę chwilę.
    fn run_file<'a>(&'a self, book: &'a Book) -> RunFile<'a> {
        let steps = self
            .plan
            .steps
            .iter()
            .zip(&book.steps)
            .map(|(planned, run)| StepEntry {
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
                // Ponowienie kroku („uruchom jeszcze raz od tego miejsca") jest w v1.1
                // [PLAN §7], więc każdy krok ma tu dziś dokładnie jedno podejście.
                attempt: 0,
                agent_session_id: match &planned.job {
                    Job::Agent(job) => Some(job.session.to_string()),
                    // Kafelek kontrolny i krok „sprawdź" nie mają sesji, bo nie mają vendora.
                    // Wpisany identyfikator byłby numerem, pod którym wznowienie szukałoby
                    // kiedyś rozmowy, której nigdy nie było.
                    Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => None,
                },
                pid: run.pid,
                pgid: run.pgid,
                exit_code: run.exit_code,
                death_proof: run.death_proof,
                started_at: run.started_at,
                ended_at: run.ended_at,
                cost_usd: run.cost_usd,
                turns: run.turns,
                input_tokens: run.input_tokens,
                output_tokens: run.output_tokens,
                cached_tokens: run.cached_tokens,
                summary: run.summary.as_deref(),
                error: run.error.as_deref(),
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
            })
            .collect();

        RunFile {
            id: &self.plan.id,
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
            // Prosto z planu, przy KAŻDYM zrzucie ta sama wartość: policzona przed pierwszym
            // procesem i od tej chwili nietknięta.
            memory: &self.plan.memory,
            steps,
        }
    }

    /// Jeden krok, od pierwszego wpisu w księdze po ostatni.
    ///
    /// `self: Arc<Self>`, bo pętla czytająca zdarzenia sterownika jest **osobnym zadaniem**
    /// ([`forward`]) i musi umieć powiedzieć o limicie dostawcy temu samemu biegowi.
    async fn step(self: Arc<Self>, id: StepId, cancel: CancellationToken) -> StepReport {
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
         * Że runda nie biegła, widać po czymś innym niż słowo `succeeded`: zerowym koszcie, braku
         * logu, braku przekazania i zdaniu w podsumowaniu kroku. Droga jest ta sama, którą kończy
         * się kafelek kontrolny — krok, który nigdy nie woła vendora, nie jest tu nowością.
         *
         * PRZED miejscem z puli, nie po: runda, której nikt nie potrzebuje, nie ma prawa stać
         * w kolejce po zasób wart ~583 MB i blokować kroku, który naprawdę ma coś do zrobienia. */
        /* SĘDZIA, KTÓRY NIE MA CZEGO SĄDZIĆ, NIE BIEGNIE — i pętla się na tym domyka.
         *
         * 2026-08-22, prośba właściciela: „jak backend nie ma czego implementować, to żeby bez
         * sensu się nie odbijać". Zmierzone na jego biegu: `Backend check` przeszedł trzy pełne
         * rundy nad pracą, której nie było, napisał w każdej to samo — „no backend code or schema
         * changes to verify" — i skończył jako `failed`, bo jedynym wyjściem z pętli był werdykt
         * `pass`. Kara za uczciwość, płacona prawdziwymi procesami i tokenami.
         *
         * PYTAMY GITA, NIE AGENTA (`isolate::touched`), i pytamy o drzewo IMPLEMENTERA. To jest
         * fakt, nie deklaracja: model nie ma jak go ograć, a wątpliwość liczy się jako „coś się
         * wydarzyło", bo pominięta weryfikacja jest droższa od jednej zbędnej rundy. */
        if let Some(which) = self.judging(&self.plan.steps[id]).map(|(which, _)| which)
            && self.nothing_to_judge(which)
        {
            let at = now_ms();
            // Pętla domyka się na TEJ rundzie: dalszych już nie potrzebujemy, a bez tego kolejne
            // startowałyby po kolei i każda pytała o to samo puste drzewo.
            self.settle(which, self.plan.steps[id].turn);
            self.update(|book| {
                let step = &mut book.steps[id];
                step.status = StepState::Succeeded;
                step.started_at = Some(at);
                step.ended_at = Some(at);
                step.summary = Some(NOTHING_CHANGED.to_owned());
            });
            self.announce(id, StepState::Succeeded);
            return StepReport::Succeeded;
        }

        if self.already_settled(id) {
            let at = now_ms();
            self.update(|book| {
                let step = &mut book.steps[id];
                step.status = StepState::Succeeded;
                step.started_at = Some(at);
                step.ended_at = Some(at);
                step.summary = Some(NOT_NEEDED.to_owned());
            });
            self.announce(id, StepState::Succeeded);
            return StepReport::Succeeded;
        }

        let _slot = match &self.plan.steps[id].job {
            // Krok „sprawdź" bierze miejsce razem z krokami agenta, i to jest wybór z powodem:
            // `./verify.sh full` odpala `cargo`, `cargo` odpala `rustc`, a to jest ta sama waga
            // na maszynie, przed którą stoi niezmiennik 11. Pytanie do człowieka nie waży nic
            // i miejsca nie bierze — to jest cała różnica między tymi dwoma ramionami.
            /* KAFELEK „URUCHOM I ZOSTAW" NIE BIERZE MIEJSCA, i to jest decyzja, nie pominięcie.
             * Pula odpowiada na pytanie „ilu agentów naraz" (niezmiennik 11), a ten krok żadnego
             * nie woła — trwa tyle, co `spawn`. Miejsce trzymane przez serwer, który żyje cały
             * bieg, wyjęłoby z puli jedno na stałe i zagłodziło kroki, które naprawdę pracują. */
            Job::Agent(_) | Job::Check(_) => {
                let Some(slot) = self.a_slot_for_this_step(&cancel).await else {
                    // Stop, zanim ten krok w ogóle ruszył: `cancelled`, nie `skipped` — nikt
                    // wyżej nie padł, człowiek zatrzymał bieg [T7 §9.3]. Księga zostaje bez
                    // chwili startu, bo startu nie było, a stan końcowy dopisze planista.
                    return StepReport::Cancelled;
                };
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

        let at = now_ms();
        self.update(|book| {
            book.started_at.get_or_insert(at);
            let step = &mut book.steps[id];
            step.status = StepState::Running;
            step.started_at = Some(at);
        });
        // Po zapisie do księgi, nie przed: ekran, który wie o kroku wcześniej niż `run.json`,
        // pokazywałby po awarii aplikacji stan, którego odzyskiwanie nie potrafi odtworzyć.
        self.announce(id, StepState::Running);

        let report = match &self.plan.steps[id].job {
            Job::Agent(job) => self.run_agent(id, job, &cancel).await,
            Job::Ask { question } => self.wait_for_a_person(id, question.as_deref()).await,
            Job::Check(job) => self.run_check(id, job, &cancel).await,
            Job::Serve(job) => self.start_and_leave(id, job),
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
            step.ended_at = Some(now_ms());
        });
        self.announce(id, ended);
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
    async fn a_slot_for_this_step(&self, cancel: &CancellationToken) -> Option<limits::Slot> {
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
                asked = self.gate.dispatch() => asked,
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
    fn carrying_what_we_inherited(
        &self,
        driver: &Arc<dyn AgentDriver>,
        of_the_step: &[String],
    ) -> anyhow::Result<Arc<dyn AgentDriver>> {
        let from_the_host = self.inherited.flags();
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
            // UMIEJĘTNOŚCI TEGO KROKU MAJĄ DRUGĄ DROGĘ i już nią dojechały: leżą w katalogu
            // roboczym pod `.agents/skills/` ([`hand_the_skills_to_the_steps`]), a odmowa
            // rozmieszczenia zabrała cały bieg kilkadziesiąt linii wcześniej. Flaga jest tu
            // dodatkiem dla jedynego vendora, który ją czyta — nie jedynym kanałem — więc jej
            // brak nie zabiera krokowi niczego i nie ma o czym milczeć.
            None => Ok(Arc::clone(driver)),
        }
    }

    /// Składa bezpieczny manifest dokładnie w kolejności, w której powstał finalny prompt.
    fn evidence_for_agent(
        &self,
        id: StepId,
        prompt_bytes: usize,
        context: Vec<ContextSource>,
    ) -> EvidenceTarget {
        let mut inherited_context = self
            .inherited
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

    /// Krok agenta: sterownik, zdarzenia, linie, koniec albo anulowanie.
    async fn run_agent(
        self: &Arc<Self>,
        id: StepId,
        job: &AgentJob,
        cancel: &CancellationToken,
    ) -> StepReport {
        let (events, inbox) = mpsc::channel::<DecodedEvent>(EVENT_QUEUE);
        // Odbiór staje PRZED startem sterownika: vendor ma prawo powiedzieć pierwsze zdarzenia
        // jeszcze w `start`, a kanał bez odbiorcy zatrzymałby go na pierwszym pełnym buforze.
        // Limit dostawcy przychodzi właśnie tędy, więc pętla dostaje CAŁY bieg, nie same linie.
        let pump = tokio::spawn(forward(
            Arc::clone(self),
            inbox,
            self.plan.steps[id].name.clone(),
        ));
        // Własny klon nadawcy zostaje po to, żeby o nieudanym starcie dało się powiedzieć tą samą
        // drogą, którą mówi agent. Musi zginąć na OBU gałęziach — nadawca, który przeżył krok,
        // trzyma kurator otwarty i `pump.await` niżej nie wróciłby nigdy.
        let ours = events.clone();

        // Prompt składamy TERAZ, a nie przy planowaniu: indeks przekazań ma co wymienić dopiero
        // wtedy, gdy poprzednicy zeszli, a przy planowaniu nie ruszył jeszcze nikt.
        let Told {
            prompt,
            reads,
            context,
            extra_dirs,
        } = match self.prompt_for(id, &job.prompt, &job.context, job.minutes) {
            Ok(told) => told,
            Err(_error) => {
                let text = "Loadout could not prove the context files for this agent, so it did \
                            not start the step."
                    .to_owned();
                let _ = ours
                    .send(AgentEvent::Notice { text: text.clone() }.into())
                    .await;
                drop(events);
                drop(ours);
                self.update(|book| book.steps[id].error = Some(text));
                let _ = pump.await;
                return StepReport::Failed;
            }
        };

        // ODZIEDZICZONY TEKST DOPISUJE SIĘ TUTAJ, czyli tam, gdzie prompt tury naprawdę powstaje.
        // Doklejenie w `plan_agent` weszłoby do `AgentJob::prompt`, a ten idzie do `run.json`
        // i do każdej następnej rundy pętli — więc cudze reguły rosłyby o kopię na rundę.
        // `applied_to` rozstrzyga też, że `system_append` wraca nietknięty: treść w tym polu
        // staje się `--append-system-prompt`, czyli argumentem widocznym w `ps` (niezmiennik 9).
        let spec = self.inherited.applied_to(RunSpec {
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

        // Start **nie** ściga się z anulowaniem i to jest wybór, nie przeoczenie: żeby zejść po
        // grupie procesów, trzeba mieć uchwyt, a uchwyt wydaje dopiero `start`. Zdjęcie tego
        // `await` w połowie zostawiłoby proces, który właśnie wstał, bez nikogo, kto by o nim
        // wiedział — czyli dokładnie ten osierocony `claude` palący limit w tle, przed którym
        // stoją niezmienniki 6 i 10. Token widzi więc dopiero tura, i widzi go od środka.
        // Fragment argv od gospodarza dojeżdża do TEGO vendora albo krok nie rusza — trzeciej
        // możliwości nie ma i to jest cała treść tych czterech linii. Sterownik, który po cichu
        // zignorowałby przyniesioną ścieżkę katalogu pluginu, dałby bieg, w którym człowiek
        // zaznaczył umiejętności, agent nie dostał żadnej i nic tego nie mówi.
        let target = self.evidence_for_agent(id, spec.prompt.len(), context);
        let evidence = target.clone();
        let configured = (|| -> anyhow::Result<Arc<dyn AgentDriver>> {
            let driver = if job.connections.is_empty() {
                Arc::clone(&job.driver)
            } else {
                let directory = self
                    .plan
                    .dir
                    .join("connections")
                    .join(&self.plan.steps[id].node_key);
                let configuration = crate::connections::runtime::for_driver(
                    &directory,
                    job.driver.id(),
                    &job.connections,
                    |name| std::env::var_os(name),
                )?;
                job.driver.configured(&configuration).ok_or_else(|| {
                    anyhow::anyhow!(
                        "this agent app cannot use the approved Connections. Loadout stopped the step instead of starting it without them."
                    )
                })?
            };
            let driver = self.carrying_what_we_inherited(&driver, &job.plugin_flags)?;
            /* 2026-08-22, przy scalaniu T-34 z T-75: KOLEJNOSC TYCH TRZECH OPAKOWAN JEST
             * WYMUSZONA, nie dowolna. Kazde z nich oddaje KLON sterownika, wiec opakowanie
             * zalozone wczesniej ginie, jesli pozniejsze klonuje sterownik sprzed niego.
             * Connections ida pierwsze, bo `configured` startuje od `job.driver`; dziedziczenie
             * drugie; dowody ostatnie, bo tylko wtedy nadajnik dowodow siedzi na sterowniku,
             * ktory naprawde pojdzie do `start`. Odwrocenie tej kolejnosci jest niewidoczne:
             * wszystko sie kompiluje, bieg rusza, a znika albo `--mcp-config`, albo plik dowodu. */
            match driver.with_evidence(target) {
                Some(driver) => Ok(driver),
                /* Stare duble silnika nie znaja surowego drutu i pozostaja uzyteczne do
                 * testowania planisty. Produkcyjna fabryka ma tylko te dwa identyfikatory;
                 * dla nich brak szwu jest odmowa, nigdy cichym biegiem bez dowodu. */
                None if matches!(driver.id(), "claude" | "codex") => Err(anyhow::anyhow!(
                    "this agent app cannot preserve its private run evidence"
                )),
                None => Ok(driver),
            }
        })();
        let started = match configured {
            Ok(driver) => driver.start(spec, events).await,
            Err(refusal) => {
                // NADAJNIK GINIE TAKŻE NA TEJ GAŁĘZI, i to nie jest higiena. Na ścieżce startu
                // zabiera go `start`; tutaj nie zabiera go nikt, a `pump.await` niżej kończy się
                // dopiero na zamkniętej kolejce. Nadawca, który przeżył krok, trzyma kurator
                // otwarty — czyli odmowa wyglądałaby jak agent zawieszony na zawsze (ten sam
                // powód stoi przy `ours` piętnaście linii wyżej).
                drop(events);
                Err(refusal)
            }
        };

        let report = match started {
            Ok(handle) => {
                drop(ours);
                self.one_turn(id, handle, cancel, &reads, &evidence).await
            }
            Err(error) => {
                let text = format!("Loadout could not start this agent: {error}");
                // `.into()` — `DecodedEvent::from(AgentEvent)` podstawia `tool: None`. Nieudany
                // start nie jest czynnością narzędzia, więc brak faktu jest tu prawdą, nie luką.
                let _ = ours
                    .send(AgentEvent::Notice { text: text.clone() }.into())
                    .await;
                drop(ours);
                self.update(|book| book.steps[id].error = Some(text));
                StepReport::Failed
            }
        };

        // Czekamy na kurator, zanim krok wróci: linie kroku muszą wyjść, ZANIM planista wypuści
        // następny. Bez tego strzałka „po" przestaje znaczyć „po" na ekranie, choć w silniku
        // dalej znaczy.
        let _ = pump.await;
        report
    }

    /// Krok „sprawdź": nasza komenda, nasz werdykt, zero sesji agenta.
    ///
    /// Ta funkcja nie tworzy `RunSpec`, nie pyta fabryki [`super::Drivers`] o sterownik i nie ma
    /// jak zapłacić za turę u vendora — i to jest jej treść, nie pominięcie (AC-4). Implementacja
    /// routująca ten krok przez [`plan_agent`] przewróciłaby się na `RunError::NoAgentsSaved`
    /// w repo, w którym nikt nie zapisał ani jednego agenta, a nie ma powodu, żeby taki krok
    /// jakiegokolwiek agenta potrzebował.
    async fn run_check(
        &self,
        id: StepId,
        job: &CheckJob,
        cancel: &CancellationToken,
    ) -> StepReport {
        let driver = CommandDriver::new();
        // START I CZEKANIE OSOBNO, a nie jednym `CommandDriver::run`, i to jest cała różnica
        // między księgą, która pomaga po awarii, a księgą, która opisuje przeszłość: `run` to
        // `start` plus `settle().await`, więc wraca dopiero PO całym sprawdzeniu — a `pid`
        // i `pgid` zapisane wtedy są nieobecne przez cały czas, w którym komenda naprawdę biegła.
        let mut live = match driver.start(&job.spec) {
            Ok(live) => live,
            Err(error) => {
                // Zdanie nazywa KOMENDĘ, bo to ona się nie uruchomiła. „Nie udało się" bez
                // podmiotu wysyła człowieka szukać wady w agencie, którego tu nie ma.
                let text = format!("Loadout could not start this check: {error}");
                self.update(|book| book.steps[id].error = Some(text));
                return StepReport::Failed;
            }
        };

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
            CheckHow::Ran(report) => {
                self.update(|book| {
                    let step = &mut book.steps[id];
                    step.exit_code = report.exit_code;
                    step.summary = summary_of(&report.output);
                });
                /* WYJŚCIE KOMENDY MA DWÓCH CZYTELNIKÓW (niezmiennik 21): werdykt wyżej
                 * i przekazanie do następnego kroku tutaj. Bez tego drugiego runda 1 pętli nie
                 * wie, co padło w rundzie 0, i pętla nie ma po co istnieć. `reads` jest puste,
                 * bo do komendy nie wstrzykujemy niczyjego przekazania — komenda nie czyta
                 * promptu. */
                self.hand_over(id, &report.output, &[]);
                if self.has_routes(id) {
                    self.remember_evidence(
                        id,
                        RouteEvidence::Check(if report.passed {
                            CheckOutcome::Passed
                        } else {
                            CheckOutcome::Failed
                        }),
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
                let proof = self.prove_check_dead(&mut live, first_proof).await;
                let unproven = matches!(&proof, GroupProof::Alive);
                let proven_dead = matches!(&proof, GroupProof::Dead { .. });
                self.update(|book| {
                    let step = &mut book.steps[id];
                    step.death_proof = proven_dead;
                    if unproven {
                        step.error = Some(
                            "Loadout could not make sure this check stopped, so it may still be \
                             running."
                                .to_owned(),
                        );
                    }
                });
                StepReport::Cancelled
            }
            CheckHow::Overdue(first_proof) => {
                let proof = self.prove_check_dead(&mut live, first_proof).await;
                let unproven = matches!(&proof, GroupProof::Alive);
                let proven_dead = matches!(&proof, GroupProof::Dead { .. });
                self.update(|book| {
                    // Powód nazywa LIMIT CZASU i mówi, co zrobić. Liczba minut przychodzi ZE
                    // STAŁEJ, a nie z tego zdania: dwa miejsca, w których mieszka jedna liczba,
                    // rozjeżdżają się przy pierwszej zmianie i to zdanie zostaje tym nieaktualnym.
                    let minutes = GIVE_UP_AFTER.as_secs() / 60;
                    let step = &mut book.steps[id];
                    step.death_proof = proven_dead;
                    step.error = Some(if unproven {
                        format!(
                            "This check ran longer than {minutes} minutes, and Loadout could not \
                             make sure it stopped, so it may still be running."
                        )
                    } else {
                        format!(
                            "This check ran longer than {minutes} minutes, so Loadout stopped it. \
                             Split the work, or run fewer things in one step."
                        )
                    });
                });
                StepReport::Failed
            }
        }
    }

    /// Zachowuje uchwyt komendy sprawdzającej po pierwszym niepełnym dowodzie Stopu.
    async fn prove_check_dead(&self, live: &mut Checking, mut proof: GroupProof) -> GroupProof {
        while matches!(proof, GroupProof::Alive) {
            tracing::error!(
                "a check group is still alive after escalation; Loadout retains its handle and \
                 will retry"
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
            proof = live.cancel().await;
        }
        proof
    }

    /// Kończy krok po limicie wyłącznie przez supervisor i utrwala jego rzeczywisty dowód.
    async fn stop_overdue_agent(
        &self,
        id: StepId,
        handle: &mut dyn AgentHandle,
        limit: Duration,
    ) -> StepReport {
        let proof = self.prove_agent_dead(handle).await;
        let proven_dead = matches!(&proof, GroupProof::Dead { .. });
        self.update(|book| {
            let step = &mut book.steps[id];
            step.death_proof = proven_dead;
            step.error = Some(format!(
                "This step ran longer than its {} minute limit, so Loadout stopped it. Give it \
                 more minutes in the agent, or split the work.",
                limit.as_secs() / 60
            ));
        });
        // Powod jest juz zapisany wyzej i mowi wiecej niz zdanie ponizej, wiec `get_or_insert`
        // go nie tknie. Ustawienie czlowieka rozstrzyga jednak tak samo, jak przy kazdej innej
        // porazce: krok, ktory nie zdazyl, tez byl slepym punktem.
        self.when_this_one_fails(id, "This step ran out of time.")
            .await
    }

    /// Kończy krok po Stopie i nie myli wysłanego sygnału z dowodem martwej grupy.
    async fn stop_cancelled_agent(&self, id: StepId, handle: &mut dyn AgentHandle) -> StepReport {
        let proof = self.prove_agent_dead(handle).await;
        let proven_dead = matches!(&proof, GroupProof::Dead { .. });
        self.update(|book| {
            let step = &mut book.steps[id];
            step.death_proof = proven_dead;
        });
        StepReport::Cancelled
    }

    /// Zachowuje jedynego właściciela uchwytu aż supervisor dowiedzie `Dead`.
    ///
    /// `Alive` nie jest wynikiem końcowym. Powrót z tej funkcji na takim dowodzie zrzuciłby
    /// `Box<dyn AgentHandle>` i osierocił proces, więc pełna eskalacja jest ponawiana w tym samym
    /// stosie tak długo, jak długo istnieje coś, czego nie umiemy uznać za martwe.
    async fn prove_agent_dead(&self, handle: &mut dyn AgentHandle) -> GroupProof {
        loop {
            let proof = handle.cancel().await;
            if matches!(proof, GroupProof::Dead { .. }) {
                return proof;
            }
            tracing::error!(
                "an agent group is still alive after escalation; Loadout retains its handle and \
                 will retry"
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Jedna tura agenta: czekaj na koniec albo na Stop, a udany wynik oddaj następnym.
    ///
    /// `reads` jest listą tego, co Loadout wstrzyknął w prompt tej tury ([`Live::prompt_for`]),
    /// i jedzie prosto do front-mattera przekazania, które z niej powstanie.
    async fn one_turn(
        &self,
        id: StepId,
        mut handle: Box<dyn AgentHandle>,
        cancel: &CancellationToken,
        reads: &[String],
        evidence: &EvidenceTarget,
    ) -> StepReport {
        // `pid` i `pgid` zapisujemy, ZANIM cokolwiek popłynie ze stdout [T7 §6.2]: po awarii
        // aplikacji nie ma już kogo o nie zapytać, a to po nich sprząta odzyskiwanie (T-20).
        if let Some(group) = handle.group() {
            self.update(|book| {
                let step = &mut book.steps[id];
                step.pid = Some(group.pid);
                step.pgid = Some(group.pgid);
            });
        }

        /* GŁOS KROKU JEST DOSTĘPNY PRZEZ CAŁĄ TURĘ — i to jest cała naprawa „nie da się napisać
         * do agenta". Rejestrujemy pod NAZWĄ kroku, bo to ją człowiek widzi w strumieniu i na
         * kafelku szyny; identyfikator wewnętrzny byłby kluczem, którego okno nigdy nie dostało.
         *
         * Zdejmujemy w `finally`-podobnym miejscu na końcu tej funkcji, nie w gałęzi sukcesu:
         * krok anulowany i krok po limicie czasu też przestają słuchać, a głos zostawiony po nich
         * proponowałby rozmowę z sesją, której nie ma. */
        if let Some(voice) = handle.voice() {
            self.control.step_can_hear(&self.plan.steps[id].name, voice);
        }

        // Limit czasu kroku. Zegar rusza TUTAJ, przy czekaniu na turę, a nie przy planowaniu:
        // krok czekający w kolejce na wolne miejsce nie zużywa niczyich pieniędzy, więc liczenie
        // mu tego czasu ubijałoby kroki tym częściej, im dłuższa kolejka.
        let limit = match &self.plan.steps[id].job {
            Job::Agent(job) => job.give_up_after,
            // Kafelek kontrolny czeka na CZŁOWIEKA i nie pali niczyich pieniędzy, więc limit
            // agenta go nie dotyczy. Ubijanie pytania po dwudziestu minutach byłoby karą za to,
            // że ktoś poszedł na obiad.
            //
            // Krok „sprawdź" nie przechodzi TĄ funkcją — nie ma tury agenta, na którą można by
            // czekać — a swój limit nosi jako stałą w `engine::drivers::command`. Ramię stoi tu
            // wprost, żeby czwarty rodzaj kroku nie skompilował się bez decyzji, ile mu wolno.
            // Kafelek „uruchom i zostaw" też nie: jego tura kończy się w chwili, w której
            // proces wstał, a to, co wstało, ma żyć dalej z rozmysłu.
            Job::Ask { .. } | Job::Check(_) | Job::Serve(_) => Duration::MAX,
        };

        let finished = {
            let waiting = handle.wait();
            tokio::pin!(waiting);
            let overdue = tokio::time::sleep(limit);
            tokio::pin!(overdue);
            tokio::select! {
                // `biased`, bo tura, która właśnie się skończyła, ma pierwszeństwo przed Stopem
                // wpadającym w tej samej chwili: zabijanie czegoś, co już zeszło, zamieniłoby
                // udany krok w anulowany zależnie od tego, który poll wypadł pierwszy. Z tego
                // samego powodu limit czasu stoi PO Stopie: człowiek, który nacisnął Stop
                // w ostatniej sekundzie, ma dostać „anulowane", a nie „przekroczony limit".
                biased;
                done = &mut waiting => Ended::Turn(done),
                () = cancel.cancelled() => Ended::Stopped,
                () = &mut overdue => Ended::Overdue,
            }
            // Pożyczka `handle` kończy się razem z tym blokiem — dopiero po nim wolno zawołać
            // `cancel()` albo `close()` na tym samym uchwycie.
        };

        /* Głos zdejmujemy PO tym `match`, nie w gałęziach: krok anulowany i krok po limicie czasu
         * też przestają słuchać, a `report` jest jedynym miejscem, przez które przechodzą
         * wszystkie trzy drogi wyjścia. */
        let report = match finished {
            // PRZEKROCZONY LIMIT IDZIE TĄ SAMĄ DROGĄ, CO STOP: przez sterownik.
            //
            // `tokio::time::timeout` owinięty wokół `handle.wait()` wygląda identycznie i jest
            // o trzy linijki tańszy — i jest błędem, przed którym stoi niezmiennik 10: anuluje
            // ZADANIE RUSTA, a proces systemowy zostaje żywy i pali limit u dostawcy do końca
            // świata. Dlatego tutaj wołamy `cancel()` i pytamy o DOWÓD zejścia grupy.
            Ended::Overdue => self.stop_overdue_agent(id, handle.as_mut(), limit).await,
            // ANULOWANIE IDZIE PRZEZ STEROWNIK, nie przez zdjęcie zadania Rusta. `tokio::time::
            // timeout` wokół kroku wygląda tak samo i jest o linijkę tańszy — i zostawia żywą
            // grupę procesów palącą limit u dostawcy (niezmienniki 6 i 10).
            Ended::Stopped => self.stop_cancelled_agent(id, handle.as_mut()).await,
            Ended::Turn(Err(error)) => {
                let proof = self.prove_agent_dead(handle.as_mut()).await;
                self.update(|book| {
                    let step = &mut book.steps[id];
                    step.death_proof = matches!(proof, GroupProof::Dead { .. });
                    step.error = Some(error.to_string());
                });
                StepReport::Failed
            }
            Ended::Turn(Ok(turn)) => {
                // Normalne zakończenie idzie przez `close`: `claude` z otwartym stdinem czeka
                // w nieskończoność, więc krok bez tego zostawia żywy proces [T1 §2, §4.6].
                let closed = handle.close().await;
                if closed.is_err() {
                    evidence.mark_incomplete();
                    let proof = self.prove_agent_dead(handle.as_mut()).await;
                    self.update(|book| {
                        book.steps[id].death_proof = matches!(proof, GroupProof::Dead { .. });
                    });
                }
                let close_succeeded = closed.is_ok();
                let code = closed.ok().flatten();
                // Sukces to zero **i** `is_error == false` (niezmiennik 19, ARCHITECTURE §5).
                // Samo zero z drivera nie kończy kroku sukcesem — agent, który wypisał „nie dam
                // rady" i wyszedł czysto, nie zrobił tego, o co go proszono.
                let evidence_complete = evidence.is_healthy();
                let ok = turn.ok
                    && close_succeeded
                    && evidence_complete
                    && matches!(code, None | Some(0));
                self.update(|book| {
                    let step = &mut book.steps[id];
                    step.exit_code = code;
                    step.cost_usd = turn.cost_usd;
                    step.turns = Some(turn.turns);
                    step.input_tokens = Some(turn.tokens.input);
                    step.output_tokens = Some(turn.tokens.output);
                    step.cached_tokens = Some(turn.tokens.cached);
                    step.summary = summary_of(&turn.text);
                    if !close_succeeded || !evidence_complete {
                        /* Surowy blad zapisu moze zawierac prywatna sciezke albo tekst vendora.
                         * Ksiege i ekran dostaja staly rodzaj; szczegol zostaje lokalnie przy
                         * prywatnym artefakcie, ktory nadal ma stan niekompletny. */
                        step.error = Some(
                            "Loadout could not preserve this agent's private run evidence. The \
                             step was not accepted as complete."
                                .to_owned(),
                        );
                    } else if !ok
                        && let FinishReason::Failed(said) = &turn.reason
                        && let Some(short) = one_line(said, SUMMARY_LIMIT)
                    {
                        /* 2026-08-23 — POWÓD PORAŻKI DOJEŻDŻA WRESZCIE DO PLIKU.
                         *
                         * `FinishReason::Failed(why)` powstawał w sterowniku Claude'a i nie
                         * czytał go NIKT: `engine::line::done_line` sięgał do `reason` tylko po
                         * `Cancelled`, a tutaj `error` ustawiało się wyłącznie przy kłopocie
                         * z zapisem dowodów. `run.json` miał więc `"error": null` przy każdym
                         * kroku, który padł — także w biegach, za które właściciel zapłacił.
                         *
                         * Kolejność warunków jest treścią: kłopot z NASZYM zapisem wygrywa
                         * z powodem agenta, bo mówi o rzeczy, której człowiek nie naprawi
                         * poprawką promptu. */
                        step.error = Some(short);
                    }
                });
                if ok {
                    // Przekazanie schodzi na dysk PRZED powrotem z tury, i to jest cały warunek
                    // poprawności tego szwu: stopień wejściowy potomkom zdejmuje planista dopiero
                    // po tym powrocie (`engine::scheduler`). Zapis dopisany za `run_agent`
                    // otwierałby okno, w którym następny krok już wystartował, a jego prompt nie
                    // ma jeszcze czego wymienić — i wyglądałoby to na przekazanie gubione raz na
                    // sto biegów, czyli na wyścig, którego nikt nie umie powtórzyć.
                    //
                    // Tylko po **udanym** kroku: agent, który wyszedł błędem, nie oddał wyniku,
                    // a plik z jego ostatnim zdaniem czytałby się jak wynik. Powód porażki jedzie
                    // do księgi i na ekran, tamtą drogą.
                    self.hand_over(id, &turn.text, reads);
                    self.remember_handoff_evidence(id, &turn.text);
                    /* WERDYKT SEDZIEGO PETLI, czytany z tego samego tekstu, ktory wlasnie stal sie
                     * przekazaniem. Po `pass` pętla sie domyka i dalsze rundy zostana pominiete
                     * (`already_settled`); po `fail` w OSTATNIEJ rundzie krok wraca `Failed`,
                     * zeby stozek za petla zostal `Skipped` i praca nie pojechala dalej na czyms,
                     * co nie przeszlo. Bez tej drugiej polowy wyczerpanie prob wygladaloby jak
                     * sukces -- czyli limit tur bylby ozdoba.
                     *
                     * Werdykt czytamy PO zapisie przekazania, nie przed: plik z raportem testera
                     * ma istniec niezaleznie od tego, co postanowimy z biegiem. */
                    match self.verdict_after(id, &turn.text) {
                        Some(why) => self.when_this_one_fails(id, why).await,
                        None => StepReport::Succeeded,
                    }
                } else {
                    self.when_this_one_fails(id, "This step did not finish what it was given.")
                        .await
                }
            }
        };
        self.control.step_went_quiet(&self.plan.steps[id].name);
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
        if !handed.is_empty() {
            self.index_of_what_came_before(&handed, &mut told)?;
        }
        told.prompt.push_str("\n\n");
        told.prompt.push_str(HOW_TO_ANSWER);
        told.prompt.push_str("\n\n");
        told.prompt.push_str(&Self::how_long_this_step_has(minutes));
        self.ask_for_an_outcome(id, &mut told);
        Ok(told)
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
             where it stands and nothing of it reaches the step after yours, so plan the work to \
             fit and answer while you still can."
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
            // `write!` do `String`, nie `push_str(&format!(…))`: ten drugi alokuje bufor
            // pośredni tylko po to, żeby go zaraz skopiować i wyrzucić (clippy
            // `format_push_string`). Zapis do `String` jest nieomylny — `fmt::Error` może
            // zwrócić tylko sam formatter — więc wynik idzie do `let _`, a nie do `expect()`,
            // który w tym drzewie jest `warn`, czyli pod `-D warnings` też fatalny.
            // ETYKIETA STOI W TYM SAMYM WIERSZU, CO ŚCIEŻKA, i to jest wymóg, nie układ: odnośnik
            // i to, czym on jest, czytane z dwóch osobnych list są dwiema listami do zestawienia
            // w głowie — a agent, który tego nie zrobi, otwiera wszystkie pliki po kolei.
            let _ = write!(
                told.prompt,
                "\n- {}: {} ({})",
                hand.from,
                hand.path.display(),
                hand.what.said()
            );
            told.reads.push(self.filed_as(&hand.path));
            let metadata = fs::symlink_metadata(&hand.path)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                anyhow::bail!("a handoff context source is not a real regular file");
            }
            told.context.push(ContextSource {
                kind: ContextKind::Handoff,
                reference: told
                    .reads
                    .last()
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("a handoff lost its safe reference"))?,
                bytes: usize::try_from(metadata.len())?,
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
        let attachments = self.plan.dir.join(handoff::ATTACHMENTS_DIR);
        if attachments.is_dir() && !told.extra_dirs.iter().any(|had| had == &attachments) {
            told.extra_dirs.push(attachments);
        }
        told.prompt.push_str("\n\n");
        told.prompt.push_str(HANDOFF_INDEX_CLOSES);
        Ok(())
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
    /// SORTOWANIE PO NUMERZE KROKU JEST SORTOWANIEM PO (POZYCJA W PLIKU, RUNDA) — `unroll` emituje
    /// węzły w kolejności z pliku, a rundy jednego kroku jedna za drugą. Dzięki temu kolejność
    /// indeksu nie zależy od tego, kto skończył pierwszy, i czyta się tak samo jak `ls handoffs/`.
    fn handed_before(&self, id: StepId) -> Vec<Handed> {
        // Migawka pod jednym zamkiem, bez ani jednego `await` w środku (niezmiennik 8). Kopia
        // całego wektora, a nie zamek trzymany przez resztę funkcji: `what_that_loop_produced`
        // pyta o te same przekazania, a `std::sync::Mutex` nie jest wznawialny.
        let filed: Vec<Option<PathBuf>> = self
            .handoffs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let unpassed: Vec<bool> = self
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
        wanted.extend(self.what_this_try_already_knows(id));
        wanted.sort_unstable();
        wanted.dedup();

        wanted
            .into_iter()
            .filter_map(|step| {
                Some(Handed {
                    from: self.plan.steps.get(step)?.name.clone(),
                    path: filed.get(step).cloned().flatten()?,
                    what: self.what_it_is(id, step, &unpassed),
                })
            })
            .collect()
    }

    /// Numer pętli, z której WYCHODZI ta strzałka. `None`, kiedy nie wychodzi z żadnej.
    ///
    /// Strzałka wewnątrz jednej pętli — z rundy k do rundy k tego samego ciała — nie wychodzi
    /// nigdzie, więc krok, który ją czyta, dostaje zwykłego poprzednika.
    fn leaving_a_loop(&self, parent: StepId, child: StepId) -> Option<usize> {
        let which = self.plan.steps.get(parent)?.in_loop?;
        (self.plan.steps.get(child)?.in_loop != Some(which)).then_some(which)
    }

    /// Ostatnie przekazanie, jakie NAPRAWDĘ wyprodukował każdy krok tej pętli.
    ///
    /// 2026-08-23 (T-87) — TO JEST NAPRAWA FAN-INU, ZMIERZONA NA BIEGU WŁAŚCICIELA. Strzałka
    /// z pętli na zewnątrz wychodzi z rundy OSTATNIEJ (`workflow::unroll`), a rundy po tej,
    /// w której padł werdykt `pass`, są pomijane bez sterownika ([`Live::already_settled`])
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
    fn what_that_loop_produced(&self, which: usize, filed: &[Option<PathBuf>]) -> Vec<StepId> {
        let Some(the_loop) = self.plan.loops.get(which) else {
            return Vec::new();
        };
        the_loop
            .body
            .iter()
            .filter_map(|tile| {
                self.plan
                    .steps
                    .iter()
                    .enumerate()
                    .filter(|(at, step)| {
                        &step.tile_key == tile && filed.get(*at).is_some_and(Option::is_some)
                    })
                    .map(|(at, _)| at)
                    .next_back()
            })
            .collect()
    }

    /// Co ta runda już wie — a czego dziś nie widziała: wejście pętli, własne wcześniejsze
    /// odpowiedzi i wcześniejsze werdykty sędziego.
    ///
    /// Pusta lista dla kroku spoza pętli i dla rundy zerowej. Numery kroków, nie ścieżki: filtr
    /// „a czy ten krok cokolwiek oddał" stoi jeden, w [`Live::handed_before`].
    fn what_this_try_already_knows(&self, id: StepId) -> Vec<StepId> {
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
        let mut knows: Vec<StepId> = self
            .node_of(&the_loop.entry, 0)
            .map(|entry| {
                ends(&self.plan.arrows, |&(parent, child)| {
                    (child == entry).then_some(parent)
                })
            })
            .unwrap_or_default()
            .into_iter()
            .filter(|&parent| {
                self.plan
                    .steps
                    .get(parent)
                    .is_some_and(|before| before.in_loop != Some(which))
            })
            .collect();

        // Własne poprzednie odpowiedzi i poprzednie werdykty sędziego — WSZYSTKIE, nie sama
        // ostatnia. Implementacja niosąca tylko rundę tuż przed tą gubi pierwszą próbę w całości,
        // więc agent powtarza błąd, który sędzia raz już odrzucił.
        for turn in 0..step.turn {
            knows.extend(self.node_of(&step.tile_key, turn));
            knows.extend(self.node_of(&the_loop.judge, turn));
        }
        knows
    }

    /// Numer węzła po kluczu kafelka i rundzie. `None`, kiedy tej rundy nie ma w tym wycinku.
    fn node_of(&self, tile: &str, turn: u8) -> Option<StepId> {
        self.plan
            .steps
            .iter()
            .position(|step| step.tile_key == tile && step.turn == turn)
    }

    /// Czym jest plik `from` dla kroku `id` — jedno miejsce z odpowiedzią na to pytanie.
    ///
    /// Kolejność warunków jest treścią. „Nie przeszedł" wygrywa ze wszystkim: materiał, którego
    /// nikt nie przyjął, ma być rozpoznawalny niezależnie od tego, skąd przyszedł. Potem pytamy
    /// o pętlę, i tylko dla rund POZA pierwszą — runda zerowa czyta swoich poprzedników dokładnie
    /// tak, jak każdy krok spoza pętli.
    fn what_it_is(&self, id: StepId, from: StepId, unpassed: &[bool]) -> WhatItIs {
        if unpassed.get(from).copied().unwrap_or(false) {
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
        if before.tile_key == step.tile_key {
            return WhatItIs::YourOwnTry { which, of };
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

    /// Odnotowuje, gdzie leży przekazanie tego kroku.
    ///
    /// Zamek powstaje i ginie w jednym wyrażeniu, bez `await` w środku (niezmiennik 8).
    fn filed(&self, id: StepId, path: PathBuf) {
        if let Some(slot) = self
            .handoffs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(id)
        {
            *slot = Some(path);
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

        match handoff::write_handoff(&self.plan.dir, draft, said) {
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
                self.filed(id, written.path);
            }
            Err(error) => tracing::error!(
                run = %self.plan.id,
                step = id,
                %error,
                "this step's result could not be handed over, so the next step is not told about it"
            ),
        }
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
        let listening = self.control.listen_for_go_on();
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
        self.ask(id, question);

        if listening.wait().await {
            self.control.resume();
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
            if let Some(said) = self.control.take_answer() {
                if self.has_routes(id) {
                    self.remember_evidence(id, RouteEvidence::Checkpoint(said.clone()));
                }
                self.hand_over(id, &said, &[]);
            }
            // Limit dostawcy mógł wejść W TRAKCIE pytania i wtedy odpowiedź człowieka nie
            // wznawia niczego: bieg dalej stoi, tylko już z innego powodu.
            let still = !self.gate.still_paused_for().is_zero();
            self.update(|book| {
                book.asking = false;
                run_stands_or_moves(book, still);
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
    fn ask(&self, id: StepId, question: Option<&str>) {
        let step = &self.plan.steps[id];
        let line = Line::Asked {
            agent: step.name.clone(),
            // Kafelek bez wpisanego pytania mówi swoją nazwą — ona też jest zdaniem, które
            // napisał człowiek.
            text: question.unwrap_or(&step.name).to_owned(),
            // Warianty odpowiedzi są polem kroku dopiero w T3 §7.1; pusta lista znaczy
            // „odpowiedz własnymi słowami", nie „pytanie bez treści".
            options: Vec::new(),
        };
        send_batch(&self.lines, vec![line]);
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
        self.update(|book| {
            for (row, &state) in book.steps.iter_mut().zip(states) {
                row.status = state;
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
async fn forward(live: Arc<Live>, mut inbox: mpsc::Receiver<DecodedEvent>, agent: String) {
    let mut curator = Curator::new();
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
    while let Some(DecodedEvent { event, tool }) = inbox.recv().await {
        // PRZED kuracją, nie po niej: wiersz jest zdaniem dla człowieka, a to niżej jest
        // decyzją dla biegu. Kolejność odwrotna dokłada okno, w którym ekran już wie, a bieg
        // jeszcze wysyła.
        if let AgentEvent::RateLimit {
            status, resets_at, ..
        } = &event
        {
            live.the_provider_said_wait(status, *resets_at);
        }
        let at_ms = u64::try_from(live.began.elapsed().as_millis()).unwrap_or(u64::MAX);
        let seen = Seen {
            agent: &agent,
            at_ms,
            event: &event,
            tool: tool.as_ref(),
        };
        let batch = curator.observe(seen);

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

// ── KSZTAŁT `run.json` ─────────────────────────────────────────────────────────────────────

/// `run.json`, tak jak ląduje na dysku.
///
/// Nazwy pól są dokładnie tymi, które czyta `store::rebuild` — rozjazd znaczy, że po skasowaniu
/// bazy dostaje się co innego, niż się miało (niezmiennik 4). Dlatego są w `snake_case`,
/// a `effective` w środku kroku zostaje w `camelCase`: to jest migawka cudzego kształtu
/// (`library::agents::Agent`), nie nasz schemat.
#[derive(Debug, Serialize)]
struct RunFile<'a> {
    id: &'a str,
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
    steps: Vec<StepEntry<'a>>,
}

/// Krok w `run.json`.
#[derive(Debug, Serialize)]
struct StepEntry<'a> {
    id: &'a str,
    node_key: &'a str,
    name: &'a str,
    agent: &'a str,
    /// Zamknięty rodzaj kroku; diagnostyka nie zgaduje po obecności artefaktów agenta.
    kind: &'static str,
    depends_on: &'a [String],
    status: StepState,
    attempt: u32,
    agent_session_id: Option<String>,
    pid: Option<i32>,
    pgid: Option<i32>,
    exit_code: Option<i32>,
    /// Tylko rzeczywisty dowód supervisora. Brak pola oznacza „nie dowiedziono”, nigdy
    /// „dowiedziono, bo krok wygląda na zakończony”.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    death_proof: bool,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    cost_usd: Option<f64>,
    turns: Option<u32>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_tokens: Option<u64>,
    summary: Option<&'a str>,
    error: Option<&'a str>,
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

    use super::{TASK_MARK, node_key_for, with_the_task};

    /// Prompt kroku, taki jak w pliku workflow.
    const STEP: &str = "Write the tests first, then the code.";

    /// Zadanie z wiersza wejścia.
    const TASK: &str = "build a pretty todo list";

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
            node_key_for("s_test", 0),
            "s_test",
            "a file with no loop has to plan exactly the keys it planned before, or no two run \
             records in this project can be compared with each other again"
        );
    }

    #[test]
    fn later_turns_get_keys_of_their_own() {
        assert_eq!(node_key_for("s_test", 1), "s_test#1");
        assert_eq!(node_key_for("s_test", 2), "s_test#2");
        assert_ne!(
            node_key_for("s_test", 1),
            node_key_for("s_test", 2),
            "the run index keys steps by this string and refuses a repeat, so two turns sharing \
             one key would fail the rebuild AFTER every agent has already been paid for"
        );
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
