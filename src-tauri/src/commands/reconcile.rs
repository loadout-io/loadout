//! Uzgodnienie biegów z **plikami**, w chwili otwarcia folderu.
//!
//! # Po co to istnieje
//!
//! Bieg, który zginął razem z aplikacją, zostawał w swoim `run.json` na zawsze jako `running`.
//! Zmierzone u właściciela 2026-08-23: trzy takie biegi naraz, siedem grup procesów dawno
//! martwych, a historia pokazywała je jako pracę w toku.
//!
//! Odzyskiwanie po awarii ISTNIAŁO i nie miało jak ich zobaczyć, z dwóch niezależnych powodów:
//!
//! 1. **Patrzyło nie tam.** `lib::recover_from_last_time` czyta wiersze z bazy otwartej przy
//!    starcie okna, czyli z `~/.loadout/loadout.db`. Biegi folderu mają WŁASNY indeks
//!    (`<folder>/.loadout/loadout.db`), więc w tamtej bazie ich nie ma. Zmierzone: biblioteka
//!    miała 19 biegów i ani jednego `running`, a obu zombie właściciela nie było w niej wcale.
//! 2. **Naprawiało nie to.** Wynik szedł wyłącznie do bazy (`store.writer().recovered`), a
//!    historia i diagnostyka czytają `run.json`. Nawet gdyby zobaczyło, plik dalej by kłamał.
//!
//! Ten moduł domyka oba: czyta stan z PLIKÓW (niezmiennik 4) i do plików go zapisuje.
//!
//! # Czego tu nie ma
//!
//! **Ani jednej reguły.** Wszystkie — strażnik czasu startu maszyny, użyteczność zapisanego
//! `pgid`, kolejność „przeczytaj, rozstrzygnij, dopiero działaj" — mieszkają w [`crate::recovery`]
//! i zostają tam. Ten plik dostarcza im wierszy z innego źródła i zapisuje ich wynik w innym
//! miejscu; druga kopia decyzji o tym, kiedy wolno strzelić do grupy procesów, byłaby tą, która
//! kiedyś strzeli po restarcie maszyny w niewinny proces.
//!
//! Nie ma tu też **automatycznego wznowienia** — recovery wyłącznie sprząta osierocone grupy
//! i oznacza przerwane biegi oraz kroki. Jawne wznowienie istniejącej sesji należy do adaptera.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::isolate;
use super::processes::{ServiceRecord, ServiceState, read_service_records, save_service_record};
use crate::durable_file::{DEFINITION_FILE_MODE, DurableFilePublisher, ModePolicy};
use crate::engine::supervisor;
use crate::recovery::{self, Machine, RecoveryRow};

/// Nazwa pliku biegu. Ta sama, którą pisze `commands::run` i czyta `commands::history`.
const RUN_FILE: &str = "run.json";

/// Katalog biegów wewnątrz folderu człowieka.
const RUNS_DIR: &str = ".loadout/runs";

/// Zdanie wpisywane krokowi, który nie przeżył zamknięcia aplikacji.
///
/// Po ludzku i bez naszych słów z drutu (niezmiennik 14): `reason` z [`crate::recovery`] jest
/// słowem dla bazy, a to jest zdanie dla człowieka, który patrzy na wiersz w historii.
const STEP_CUT_OFF: &str =
    "Loadout closed while this step was still running, so the step was cut off with it.";

/// Zdanie dla biegu, który stał na pytaniu, kiedy okno zniknęło.
///
/// WŁASNE, a nie to samo, co dla kroku uciętego w pracy, i różnica jest dla człowieka całą
/// treścią: tam agent pracował i został przerwany, tu **nic nie pracowało** — bieg czekał na
/// odpowiedź, której nie było już komu podać.
const RUN_LEFT_ON_A_QUESTION: &str = "Loadout closed while this run was waiting for your answer, so there was nobody left to \
     carry it on. Start it again to pick the work up.";

/// Co uzgodnienie zastało i co z tym zrobiło — do dziennika, nie na ekran.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reconciled {
    /// Ile biegów przepisano z `running` na przerwane.
    pub runs: usize,
    /// Ile kroków przepisano.
    pub steps: usize,
    /// Ile grup procesów udowodniono jako martwe.
    pub reaped: usize,
    /// Ile grup **wciąż żyje** mimo zamknięcia aplikacji. Nie zero znaczy sierotę palącą limit.
    pub still_alive: usize,
    /// Ile drzew roboczych domknięto po biegach, których nikt już nie prowadzi (2026-09, Z-9).
    pub closed: usize,
    /// Ile biegów zeszło z dysku, bo człowiek prosił o krótszą historię (2026-09, Z-9).
    pub forgotten: usize,
    /// Ile katalogów roboczych **nie** dało się domknąć, bo są sprzed markera (2026-09, Z-46).
    ///
    /// Nie zero znaczy: stoją w projekcie, nikt ich nie zamknie sam z siebie, a Loadout właśnie
    /// wpisał zdanie o każdym z nich do opisu jego biegu. Zdejmuje je dopiero człowiek, z historii.
    pub left_over: usize,
}

/// Ile biegów zostaje w folderze projektu po sprzątaniu.
///
/// Typ, a nie goła liczba, i to jest wybór na jedno konkretne ryzyko: `reconcile_runs_keeping`
/// KASUJE katalogi, więc argument `0` czytany jako „nie trzymaj nic" byłby jednym znakiem między
/// sprzątaniem i skasowaniem całej historii projektu. Konstruktor [`Keep::everything`] nazywa
/// bezczynność po imieniu i jest tym, co dostaje każdy wołający, który o retencję nie prosił.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keep {
    /// Ile najświeższych biegów zostaje. `None` znaczy „wszystkie" i jest domyślną odpowiedzią.
    last_runs: Option<usize>,
}

impl Keep {
    /// Nic nie schodzi z dysku. Odpowiedź dla folderu, o którego historię nikt nie prosił.
    #[must_use]
    pub const fn everything() -> Self {
        Self { last_runs: None }
    }

    /// Zostaje `how_many` najświeższych biegów. `0` znaczy „wszystkie", nie „żaden".
    ///
    /// Zero jedzie tu z `SettingsWire::keep_last_runs`, gdzie jest wartością, którą `serde`
    /// wstawia KAŻDEMU dzisiejszemu plikowi — powód w całości stoi przy tamtym polu. Przełożenie
    /// „zero to wszystkie" mieszka w jednym miejscu i to jest to miejsce.
    #[must_use]
    pub const fn last_runs(how_many: u32) -> Self {
        Self {
            last_runs: if how_many == 0 {
                None
            } else {
                Some(how_many as usize)
            },
        }
    }
}

/// Uzgadnia biegi tego folderu z tym, co naprawdę żyje na maszynie, i **nic nie kasuje**.
///
/// Wołane z [`crate::workspace`] w chwili otwarcia folderu — raz na folder, spod zamka na liście
/// kart, czyli w jedynej chwili, w której nikt inny tych plików nie trzyma.
///
/// Nie oddaje odmowy. Folder bez katalogu biegów, bieg z nieczytelnym `run.json`, plik bez prawa
/// zapisu — każde z nich jest jednym biegiem mniej w tym uzgodnieniu, a nie folderem, którego nie
/// da się otworzyć (niezmiennik 5). Człowiek, któremu nie otwiera się projekt, bo jeden stary
/// plik biegu jest uszkodzony, traci znacznie więcej niż jeden wiersz historii.
#[must_use]
pub fn reconcile_runs(project: &Path) -> Reconciled {
    reconcile_runs_keeping(project, &Keep::everything())
}

/// To samo, plus retencja: `keep` mówi, ile biegów zostaje w folderze.
///
/// # 2026-09 (Z-9) — dlaczego sprzątanie jest TUTAJ, a nie w [`with_reaper`]
///
/// Bo `with_reaper` odpowiada na inne pytanie i ma jednego wołającego poza produkcją: kryterium,
/// które podstawia własny domykacz grup procesów, żeby sprawdzić, że NIC nie zostało zabite.
/// Sprzątanie wciągnięte tam zmieniłoby znaczenie tamtego szwu — a jego podpis jest przypięty
/// dwoma kryteriami.
///
/// **Cztery kroki, wszystkie PO odzyskiwaniu**, i ta kolejność jest treścią: bieg, który zginął
/// razem z aplikacją, dopiero co dostał tu status `interrupted`, więc dopiero teraz jest
/// widoczny jako bieg, którego nikt już nie prowadzi. Domknięcie w odwrotnej kolejności
/// pominęłoby dokładnie ten jeden przypadek, dla którego to sprzątanie istnieje.
#[must_use]
pub fn reconcile_runs_keeping(project: &Path, keep: &Keep) -> Reconciled {
    let mut done = with_reaper(project, reap_if_it_is_ours);
    done.closed = close_what_the_runs_left(project);
    // PO domknięciu, nie przed: `worktree remove` zdejmuje swój wpis sam, a `prune` sprząta po
    // katalogach, które zniknęły cudzą ręką — po przerwanym biegu, po `rm -rf` człowieka, po
    // projekcie przeniesionym na inny dysk. Wpis bez katalogu odmawia potem założenia drzewa
    // pod tą samą ścieżką, czyli następnego biegu tego kroku.
    if let Err(said) = isolate::prune_trees(project) {
        tracing::debug!(
            project = %project.display(),
            said,
            "git would not tidy up its own list of places to work"
        );
    }
    // Cicho, bo to jest uprzejmość wobec cudzego repozytorium, a nie warunek pracy: folder, do
    // którego nie wolno nam dopisać jednej linii wyciszenia, dalej jest folderem, w którym
    // Loadout biega.
    if let Err(said) = isolate::exclude_our_folder(project) {
        tracing::debug!(
            project = %project.display(),
            said,
            "our own folder could not be added to the list only this clone of the project reads"
        );
    }
    // PO `prune_trees`, bo pytanie brzmi „co git wciąż zna": wpis po katalogu, którego już nie ma,
    // czytałby się jako leżak, o którym trzeba powiedzieć — a to jest śmieć, który właśnie zszedł
    // sam (2026-09, Z-46).
    done.left_over = say_what_the_runs_did_not_close(project);
    done.forgotten = forget_all_but_the_last(project, keep);
    done
}

/// Wpisuje krokom zdanie o katalogach roboczych, których to sprzątanie **nie** domknęło.
///
/// # 2026-09 (Z-46) — CISZA MIAŁA JEDNĄ PRZYCZYNĘ
///
/// [`close_what_the_runs_left`] chodzi po markerach izolacji (`<bieg>/.isolation/<klucz>`), czyli
/// po notatce, którą bieg pisze o każdym katalogu, jaki sobie otworzył. Bieg sprzed tej notatki
/// nie ma jej wcale, więc jego katalog jest dla tamtej pętli niewidzialny — nie zamyka go i nie
/// mówi o nim ani słowa. Zmierzone u właściciela 2026-09-03 na `urc-monorepo`: dziennik zameldował
/// „75 folder(s) closed", a `git worktree list` dalej wymieniał dwanaście katalogów po 264 MB.
///
/// # NIC NIE KASUJE, i to jest cała różnica wobec pętli wyżej
///
/// Tamta ma marker, czyli wie, na której gałęzi ta praca ma wylądować i od którego commita drzewo
/// powstało. Tutaj nie wiadomo nic poza tym, że katalog stoi — a `worktree remove` bez tej wiedzy
/// zdejmuje jedyną kopię niezapisanej pracy. Zdanie idzie więc tam, gdzie człowiek o biegu czyta
/// (niezmiennik 29), a zdejmuje to dopiero kontrolka w historii ([`super::sweep`]).
fn say_what_the_runs_did_not_close(project: &Path) -> usize {
    let mut said: BTreeMap<PathBuf, BTreeMap<String, String>> = BTreeMap::new();
    for (dir, work) in super::sweep::folders_the_runs_did_not_close(project) {
        let Some(run) = read_run(&dir) else { continue };
        let key = work
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(step) = the_step_called(&run, &key) else {
            // Opis biegu nie zna już tego kroku, więc nie ma przy czym postawić zdania. Katalog
            // dalej stoi i dalej liczy się do tego, co proponuje zdjąć historia.
            continue;
        };
        /* KATALOG ZE ZNACZNIKIEM MA JUŻ SWOJE ZDANIE i jest ono konkretniejsze: pętla wyżej
         * PRÓBOWAŁA go domknąć i niesie powód od gita (`isolate::could_not_tidy`). To zdanie
         * mówi co innego — „ten bieg jest starszy niż znacznik" — więc postawione nad drzewem,
         * które znacznik MA, byłoby po prostu nieprawdą (2026-09, Z-46). */
        if crate::commands::run::trees_left_in(&dir)
            .iter()
            .any(|one| one.key == key)
        {
            continue;
        }
        said.entry(dir)
            .or_default()
            .insert(step.id, super::sweep::a_folder_we_did_not_close(&work));
    }
    let mut named = 0;
    for (dir, steps) in &said {
        let rows: BTreeMap<&str, &str> = steps
            .iter()
            .map(|(id, one)| (id.as_str(), one.as_str()))
            .collect();
        named += note_on_steps(dir, &rows);
    }
    named
}

/// Czy tego biegu nikt już nie prowadzi — po katalogu, w którym leży jego opis.
///
/// 2026-09 (Z-46) — WIDOCZNE DLA MODUŁU OBOK, bo zamiatacz starych biegów ([`super::sweep`])
/// zadaje dokładnie to samo pytanie o dokładnie te same katalogi. Druga kopia tego warunku byłaby
/// tą, która pewnego dnia uzna bieg pracujący w tej chwili za skończony i zaproponuje człowiekowi
/// skasowanie katalogu, w którym agent właśnie pisze (niezmiennik 23).
pub(super) fn run_is_over(dir: &Path) -> bool {
    read_run(dir).is_some_and(|run| is_over(&run))
}

/// Identyfikator biegu z jego opisu, albo `None` — z niego składa się przedrostek jego gałęzi.
///
/// 2026-09 (Z-46) — widoczne dla [`super::sweep`] z tego samego powodu, co [`run_is_over`]: to
/// jest jedna odpowiedź na pytanie „czyj jest ten katalog", czytana z jednego pliku.
pub(super) fn run_named_by(dir: &Path) -> Option<String> {
    let run = read_run(dir)?;
    let id = text(&run, "id");
    (!id.trim().is_empty()).then_some(id)
}

/// Domyka drzewa robocze biegów, których nikt już nie prowadzi. Oddaje liczbę domkniętych.
///
/// # Po co to istnieje
///
/// Sprzątanie po biegu żyło do 2026-09 wyłącznie w `commands::run::close_the_trees`, czyli
/// w kodzie biegu, który właśnie się kończy. Bieg ubity razem z aplikacją nie kończy się nigdy:
/// zostawiał katalog z pełnym checkoutem repozytorium, wpis w rejestrze gita i pracę agenta,
/// która nigdy nie doszła na gałąź. Zmierzone u właściciela 2026-09-02 na `urc-monorepo`:
/// 87 katalogów `work/`, 89 wpisów, 99 gałęzi, 3,8 GB.
///
/// # TĄ SAMĄ `isolate::finish`, i to jest cały sens (niezmiennik 23)
///
/// Nie „skasuj katalog", tylko dokładnie ta funkcja, którą woła koniec biegu: praca ląduje na
/// gałęzi, gałąź po kroku, który nic nie zrobił, schodzi, a katalog zdejmuje git razem ze swoim
/// wpisem. Druga polityka sprzątania obok byłaby tą, która kiedyś skasuje czyjąś pracę —
/// `remove_dir_all` w tym miejscu zdejmuje jedyną kopię tego, co agent zdążył napisać.
///
/// # Bieg w stanie NIETERMINALNYM zostaje nietknięty
///
/// `running` i `paused` znaczą „ktoś to prowadzi". Uzgodnienie wyżej właśnie przepisało na
/// `interrupted` te, których nikt nie prowadzi, więc wszystko, co dalej stoi w `running`, należy
/// do biegu żywego w TEJ sesji — a drzewo zamknięte pod pracującym agentem to jego katalog
/// roboczy skasowany w połowie zdania.
fn close_what_the_runs_left(project: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(project.join(RUNS_DIR)) else {
        return 0;
    };
    let mut closed = 0;
    for entry in entries.flatten() {
        let dir = entry.path();
        let Some(run) = read_run(&dir) else { continue };
        if !is_over(&run) {
            continue;
        }
        let mut said = service_blocker_notes(project, &dir, &run);
        if let Err(why) = super::run::recover_recorded_results(project, &dir) {
            tracing::warn!(run = %dir.display(), %why, "saved results could not be recovered; working folders were kept");
            publish_step_notes(&dir, &said);
            continue;
        }
        let title = run
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        for tree in crate::commands::run::trees_left_in(&dir) {
            // WF-25: koniec kafelka Serve nie kończy jego usługi. Także niezrozumiały
            // rekord jest brakiem dowodu, nigdy pustą listą właścicieli katalogu.
            if services_protect_copy(project, &dir, &run, &tree.cwd) {
                continue;
            }
            let step = the_step_called(&run, &tree.key);
            let isolate::Closed { kept, tidied, .. } = isolate::finish_with_saved(
                project,
                &tree.cwd,
                &tree.branch,
                // Ten sam temat commita, co po zwykłym biegu (`commands::run::close_one_tree`):
                // człowiek czytający `git log` dzień później ma poznać bieg i krok.
                &format!(
                    "{title}: {}",
                    step.as_ref().map_or(&tree.key, |one| &one.name)
                ),
                Some(tree.head.as_str()),
                |oid| {
                    super::run::save_recovered_git_result(project, &dir, &tree, oid)
                        .map_err(|why| format!("the recovered result could not be saved: {why}"))
                },
            );
            closed += 1;
            let mut says: Vec<String> = Vec::new();
            if let isolate::Kept::LeftInPlace { why, .. } = kept {
                says.push(why);
            }
            says.extend(tidied);
            /* ZDANIE MUSI DOJŚĆ TAM, GDZIE CZŁOWIEK JE CZYTA (niezmiennik 29). Wartość zwrócona
             * przez `isolate::finish` dowodzi, że mechanizm istnieje; opis biegu otwarty w oknie
             * (`history::read_run_inner` → `PastStepWire.error`) jest jedynym miejscem, w którym
             * ktokolwiek dowie się, że w projekcie stoi katalog, którego nie dało się zdjąć. */
            if let (Some(step), false) = (step, says.is_empty()) {
                said.insert(step.id, says.join(" "));
            }
        }
        if let Err(why) = super::run::recover_recorded_results(project, &dir) {
            tracing::warn!(run = %dir.display(), %why, "the newly saved folder result could not be recorded in the run");
        }
        publish_step_notes(&dir, &said);
    }
    closed
}

fn publish_step_notes(dir: &Path, said: &BTreeMap<String, String>) {
    if said.is_empty() {
        return;
    }
    let rows = said
        .iter()
        .map(|(id, one)| (id.as_str(), one.as_str()))
        .collect();
    note_on_steps(dir, &rows);
}

/// WF-25: bezpiecznie pozostawiony folder musi mieć wyjaśnienie w prawdziwej historii.
/// Zbieramy je także przed recovery receiptów: uszkodzony record usługi potrafi odmówić
/// tamtego odczytu, zanim pętla cleanupu w ogóle dojdzie do swojej blokady.
fn service_blocker_notes(project: &Path, dir: &Path, run: &Value) -> BTreeMap<String, String> {
    let mut notes = BTreeMap::new();
    for tree in super::run::trees_left_in(dir) {
        if !services_protect_copy(project, dir, run, &tree.cwd) {
            continue;
        }
        let Some(step) = the_step_called(run, &tree.key) else {
            continue;
        };
        let sentence = format!(
            "Loadout could not prove that all services using this working folder have stopped, so the folder was kept: {}",
            tree.cwd.display()
        );
        let previous = run
            .get("steps")
            .and_then(Value::as_array)
            .and_then(|steps| steps.iter().find(|one| text(one, "id") == step.id))
            .and_then(|one| one.get("error"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let said = if previous.is_empty() {
            sentence
        } else if previous.contains(&sentence) {
            previous.to_owned()
        } else {
            format!("{previous} {sentence}")
        };
        notes.insert(step.id, said);
    }
    notes
}

/// Czy tego biegu nikt już nie prowadzi.
///
/// Lista jest ZAMKNIĘTA po drugiej stronie: `running` i `paused` znaczą „trwa", a każdy inny
/// status — także taki, którego ta wersja nie zna — znaczy „skończony". Odwrotny warunek
/// („wymień statusy końcowe") przy pierwszym nowym statusie zostawiałby drzewa w ciszy.
fn is_over(run: &Value) -> bool {
    !matches!(
        run.get("status").and_then(Value::as_str),
        Some("running" | "paused")
    )
}

/// Krok tego biegu o tym kluczu pracy — jego identyfikator i nazwa dla człowieka.
///
/// `node_key`, nie `id`: klucz pracy jest nazwą katalogu w `work/` i nazwą pliku markera, a to
/// tym samym słowem nazywa kafelek plik workflow. `None` znaczy „opis biegu tego kroku już nie
/// zna" — drzewo domykamy tak czy owak, bo stoi na dysku niezależnie od tego, co o nim wiemy.
fn the_step_called(run: &Value, work_key: &str) -> Option<NamedStep> {
    run.get("steps")
        .and_then(Value::as_array)?
        .iter()
        .find(|one| {
            one.get("node_key")
                .and_then(Value::as_str)
                .is_some_and(|node| super::run::work_key_of(node) == work_key)
        })
        .map(|one| NamedStep {
            id: text(one, "id"),
            name: text(one, "name"),
        })
}

/// Krok w dwóch słowach: czym się adresuje w pliku i jak się nazywa na ekranie.
struct NamedStep {
    id: String,
    name: String,
}

/// Zdejmuje z dysku biegi starsze niż `keep` ostatnich. Oddaje liczbę zapomnianych.
///
/// # TĄ SAMĄ DROGĄ, CO PRZYCISK (niezmiennik 23)
///
/// `history::forget_run_inner`, nie własne `remove_dir_all`: retencja i „forget this run" to
/// jedna czynność zamówiona na dwa sposoby, a kopia tej polityki tutaj byłaby tą, która pominie
/// ostrożność. Ostrożność jest konkretna: bieg, którego gałąź jest W TEJ CHWILI wyjęta do pracy
/// w innym drzewie, nie schodzi wcale — ani gałąź, ani katalog.
///
/// Odmowa jednego biegu **nie zatrzymuje pozostałych**: to jest sprzątanie w tle przy otwarciu
/// folderu, a nie czynność, którą ktoś właśnie zamówił i której odpowiedzi czeka. Zdanie idzie
/// do dziennika.
fn forget_all_but_the_last(project: &Path, keep: &Keep) -> usize {
    let Some(last_runs) = keep.last_runs else {
        return 0;
    };
    // `run_dirs` oddaje biegi od NAJŚWIEŻSZEGO (nazwa katalogu otwiera się znacznikiem czasu
    // UTC), więc „zostaw N ostatnich" jest tu pominięciem pierwszych N pozycji.
    let mut forgotten = 0;
    for dir in super::handoffs::run_dirs(project)
        .into_iter()
        .skip(last_runs)
    {
        let Some(name) = dir.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        match super::history::forget_run_inner(project, name) {
            Ok(_) => forgotten += 1,
            Err(error) => tracing::debug!(
                run = name,
                said = %error,
                "this run was not forgotten, so the folder keeps one more than asked"
            ),
        }
    }
    forgotten
}

/// Jedyna polityka strzału do zastanej grupy — i **jedyne** miejsce, w którym mieszka.
///
/// # Trzy odpowiedzi w tej kolejności, i kolejność jest treścią (2026-09, Z-01d)
///
/// 1. **Grupa pusta** → dowód śmierci, bez ani jednego sygnału. To jest najczęstszy przypadek po
///    awarii: numer stoi w pliku, a proces zszedł dawno temu. Zapytanie idzie pierwsze, bo bez
///    niego pusta grupa przechodziłaby przez pełną eskalację i wyglądała jak sprzątanie, którego
///    nie było.
/// 2. **Znacznik nazywa INNY bieg** → grupa jest obca i nie dostaje **niczego**. Numery procesów
///    przewijają się na macOS w godzinach, więc `pgid` zapisany przez nas potrafi dziś należeć do
///    czyjejś pracy — a jeśli ta praca sama mówi, czyja jest, to jest to najtwardsza odpowiedź,
///    jaką da się dostać. Poprzednie podejście uznawało każde `Some(...)` za zgodę na zabicie
///    i zostało za to odrzucone: znacznik JAKIEGOKOLWIEK biegu nie jest znacznikiem TEGO biegu.
/// 3. **Brak znacznika albo znacznik tego biegu** → normalna eskalacja przez `reap_group`.
///    Brak znaczy „nie da się przeczytać", nigdy „cudza": powód w całości przy
///    [`supervisor::run_behind_group`]. Grupa pod `/bin/sh` nie pokazuje ani jednej zmiennej,
///    a to jest dokładnie ta sierota, dla której całe to sprzątanie istnieje.
fn reap_if_it_is_ours(target: &recovery::ReapTarget) -> recovery::ReapOutcome {
    if supervisor::group_is_empty(target.pgid) {
        return recovery::ReapOutcome::ProvenDead;
    }
    if let Some(behind) = supervisor::run_behind_group(target.pgid)
        && behind != target.run_id
    {
        return recovery::ReapOutcome::Foreign;
    }
    match supervisor::reap_group(target.pgid) {
        supervisor::GroupProof::Dead { .. } => recovery::ReapOutcome::ProvenDead,
        supervisor::GroupProof::Alive { .. } => recovery::ReapOutcome::StillAlive,
    }
}

/// Ta sama polityka, wystawiona odzyskiwaniu z biblioteki (`lib::recover_from_last_time`) —
/// tylko że grupy czekają na swoje okna łaski **obok siebie**.
///
/// 2026-09 (Z-01d) — istnieje, bo do tego dnia tamta droga miała **własną kopię** decyzji o tym,
/// kiedy wolno strzelić do grupy: dwa ramiona `match` nad `reap_group`, bez pytania o znacznik.
/// Polityka mieszka w jednym rdzeniu, a adaptery mają po pięć linii (niezmiennik 23) — dwie
/// kopie znaczyłyby, że sprzątanie po awarii zabija cudze grupy dokładnie wtedy, gdy biegi
/// mieszkają w bibliotece, a nie w folderze.
///
/// 2026-09 (Z-30) — DROGA STARTOWA BYŁA SYNCHRONICZNA I SZŁA PO KOLEI. Jedna sierota ignorująca
/// SIGTERM kosztuje pełne okno łaski plus dowód po dziewiątce (`DEFAULT_GRACE` +
/// `PROOF_AFTER_KILL`), a pięć takich grup kosztowało pięć takich okien jedno po drugim — i to
/// w środku startu aplikacji, w wątku, który miał zaraz pokazać okno. Sekwencyjny bliźniak tej
/// funkcji zszedł razem z tą poprawką: po przepięciu obu dróg nie miał już ani jednego wołającego,
/// a martwa funkcja publiczna gnije na zielono (powód w całości w `checks/quick-wired.sh`).
///
/// Adapter, nie druga polityka (niezmiennik 23): pięć wątków siedzi w [`reap_each_group_apart`],
/// a jedyne, co dokłada ta funkcja, to **jedno** `spawn_blocking` na całość — żeby wątek
/// wykonawczy startu nie czekał na żaden z nich (niezmiennik 8 mówi to samo o zamkach).
#[must_use]
pub async fn reap_what_is_ours_concurrently(
    plan: &recovery::RecoveryPlan,
) -> recovery::RecoveryReport {
    let planned: Vec<i32> = plan.reap.iter().map(|target| target.pgid).collect();
    let plan = plan.clone();
    tokio::task::spawn_blocking(move || reap_each_group_apart(&plan, reap_if_it_is_ours))
        .await
        .unwrap_or_else(|_| recovery::RecoveryReport {
            // Sprzątanie, które nie doszło do końca, nie jest dowodem śmierci: bez `ESRCH` grupa
            // jest ŻYWA (niezmiennik 6). Pusty raport czytałby się jak „nie było czego sprzątać".
            unproven: planned,
            ..recovery::RecoveryReport::default()
        })
}

/// Sprząta KAŻDĄ grupę z planu obok pozostałych i składa z odpowiedzi jeden raport.
///
/// 2026-09 (Z-30) — RDZEŃ, NIE ADAPTER: tędy chodzą OBIE produkcyjne drogi odzyskiwania —
/// biblioteka (`lib::recover_from_last_time` przez [`reap_what_is_ours_concurrently`]) i pliki
/// folderu (`reconcile_runs_keeping` przez [`with_reaper`]). Pierwsza wersja tej poprawki
/// przyspieszyła wyłącznie bibliotekę, a folder — czyli droga, którą sprząta się po zamkniętym
/// oknie — dalej czekał na pięć okien łaski jedno po drugim.
///
/// Wątki z zakresu (`std::thread::scope`), nie zadania tokio, i to jest wybór na jedno konkretne
/// ograniczenie: pętla dowodowa `supervisor::reap_group` czeka na jądro **synchronicznie**
/// (`std::thread::sleep`), a droga folderu jest w całości synchroniczna — biegnie już wewnątrz
/// `spawn_blocking` w `ipc::AppState::settle_what_the_last_window_left`. Zakres pozwala przy tym
/// domykaczowi pożyczyć stan wołającego, więc kryterium nie musi kupować `Arc` na barierę.
///
/// Polityka się nie zmienia ANI O SŁOWO (niezmiennik 23): kto jest nasz, rozstrzyga ten sam
/// [`reap_if_it_is_ours`], a trzy odpowiedzi na trzy listy rozkłada ten sam [`recovery::apply`].
/// Równoległe jest wyłącznie CZEKANIE.
#[must_use]
pub fn reap_each_group_apart<F>(plan: &recovery::RecoveryPlan, reap: F) -> recovery::RecoveryReport
where
    F: Fn(&recovery::ReapTarget) -> recovery::ReapOutcome + Clone + Send,
{
    let answers: Vec<recovery::ReapOutcome> = std::thread::scope(|apart| {
        let waiting: Vec<_> = plan
            .reap
            .iter()
            .map(|target| {
                let reap = reap.clone();
                apart.spawn(move || reap(target))
            })
            .collect();
        waiting
            .into_iter()
            // Domykacz, który padł, nie jest dowodem śmierci — zameldowanie posprzątanej grupy,
            // której nikt nie sprzątnął, jest tu jedynym naprawdę drogim błędem.
            .map(|one| one.join().unwrap_or(recovery::ReapOutcome::StillAlive))
            .collect()
    });

    // Trzy odpowiedzi na trzy listy rozkłada TEN SAM rdzeń, co przy sprzątaniu po kolei
    // (niezmiennik 23): `apply` chodzi po `plan.reap` w kolejności, a my odpowiadamy w tej samej.
    let mut answers = answers.into_iter();
    recovery::apply(plan, &mut |_| {
        answers.next().unwrap_or(recovery::ReapOutcome::StillAlive)
    })
}

/// To samo, z **wstrzykniętym** domykaczem grup procesów.
///
/// Istnieje z dokładnie tego samego powodu, co domknięcie w [`recovery::apply`], i powód ten jest
/// tam zapisany: kryterium akceptacji ma móc podstawić własny i sprawdzić, że NIC nie zostało
/// zabite, bez zabijania czegokolwiek na prawdziwej maszynie. Test wołający wersję z prawdziwym
/// `killpg` strzelałby do grupy o numerze wpisanym w fikstrze — a numery procesów przewijają się
/// w godzinach.
///
/// 2026-09 (Z-30) — domykacz wjeżdża tu **wartością**, a nie przez `&mut dyn FnMut`, i to jest
/// cena za jedno konkretne zachowanie: grupy tej drogi czekają obok siebie ([`reap_each_group_apart`]),
/// a domknięcia pożyczonego na wyłączność nie da się dać pięciu wątkom naraz. Kryterium, które
/// chce policzyć pytania, trzyma swój licznik za zamkiem — tak samo, jak trzyma go produkcja.
#[must_use]
pub fn with_reaper<F>(project: &Path, reap: F) -> Reconciled
where
    F: Fn(&recovery::ReapTarget) -> recovery::ReapOutcome + Clone + Send,
{
    let (rows, where_they_live, services) = rows_from_files(project);
    /* PUSTA LISTA NIE KOŃCZY TEGO PRZEBIEGU, i kryterium złapało tu prawdziwy błąd. „Nie ma
     * czego dobijać" nie znaczy „nie ma czego sprzątać": folder, w którym stoi wyłącznie bieg
     * zaparkowany na pytaniu, ma zero kroków w `running` — czyli dokładnie ten przypadek, dla
     * którego `settle_the_parked` powstało, i dokładnie ten, który wychodził stąd nietknięty.
     *
     * Wyjście wcześniej byłoby też DRUGIM miejscem wołania tego samego sprzątania, a drugie
     * miejsce jest tym, którego kryterium nie sądzi (mutacja jednego z nich nie zapala niczego).
     * Jeden przebieg do końca, jedno wołanie. */
    let parked = settle_the_parked(project);
    if rows.is_empty() {
        return Reconciled {
            runs: parked,
            ..Reconciled::default()
        };
    }

    let machine = Machine {
        boot_id: supervisor::machine_booted_at().unwrap_or_default(),
        own_pgid: supervisor::own_process_group(),
    };
    let plan = recovery::decide(&rows, &machine);
    // TEN SAM rdzeń, co przy odzyskiwaniu biblioteki (2026-09, Z-30): pięć sierot ignorujących
    // SIGTERM czeka tu obok siebie, a nie pięć okien łaski jedno po drugim — cała ta droga biegnie
    // w `spawn_blocking`, na wątku, na którym okno czeka na odpowiedź.
    let report = reap_each_group_apart(&plan, reap);
    record_service_outcomes(&services, &plan, &report);
    // 2026-08-27: sam licznik `unproven` ukrywał finansowo istotną sierotę przed człowiekiem.
    // Łączymy wynik domykacza z oryginalnym wierszem, bo tylko plik niesie oba identyfikatory,
    // które pozwalają rozpoznać ocalały proces bez zgadywania po samym PGID.
    let survivor_warnings = what_the_person_has_to_read(&rows, &report);

    let mut done = Reconciled {
        reaped: report.reaped.len(),
        still_alive: report.unproven.len(),
        ..Reconciled::default()
    };
    let mut rewritten: Vec<&String> = Vec::new();
    for change in &plan.run_status {
        let Some(dir) = where_they_live.get(&change.run_id) else {
            continue;
        };
        let steps: Vec<(&str, &str)> = plan
            .step_status
            .iter()
            .map(|one| (one.step_id.as_str(), one.status.as_str()))
            .collect();
        if write_back(dir, &change.status, &steps, &survivor_warnings) {
            rewritten.push(&change.run_id);
            done.runs += 1;
            done.steps += steps.len();
        }
    }
    /* ZDANIE MUSI DOJŚĆ TAKŻE DO BIEGU, KTÓREGO NIKT NIE PRZEPISUJE (2026-09, Z-01d).
     *
     * Pętla wyżej chodzi po `plan.run_status`, czyli po biegach, którym odzyskiwanie zmienia
     * status. Bieg zatrzymany przez człowieka schodzi jednak jako `cancelled` i to jest o nim
     * PRAWDA — nikt tego nie przepisuje, więc ani jedno zdanie o jego ocalałym nie miało jak
     * trafić do pliku. Z okna wyglądało to na bieg zamknięty czysto, przy grupie, która dalej
     * biegła i dalej paliła limit (niezmiennik 29). */
    done.steps += write_back_warnings(&where_they_live, &rewritten, &rows, &survivor_warnings);
    done.runs += parked;
    done
}

/// Zdanie dla człowieka przy każdym kroku, którego grupa **nie** dostała dowodu śmierci.
///
/// Mapa po `step_id`, bo tak adresuje ją zapis do pliku, i po każdej grupie tego kroku, nie tylko
/// po `pgid` lidera (2026-09, Z-01d): grupa obca i grupa, która przeżyła eskalację, to dwie różne
/// wiadomości, a obie muszą dojechać do tego samego pola błędu.
fn what_the_person_has_to_read(
    rows: &[RecoveryRow],
    report: &recovery::RecoveryReport,
) -> BTreeMap<String, String> {
    let mut warnings = BTreeMap::new();
    for row in rows {
        for pgid in row.pgid.into_iter().chain(row.pgids.iter().copied()) {
            /* GRUPA OBCA IDZIE PIERWSZA, bo mówi coś, czego drugie zdanie nie mówi wcale: nie
             * „nie udało się zatrzymać", tylko „to nie jest nasze i nie tknęliśmy tego". */
            if report.foreign.contains(&pgid) {
                warnings.insert(row.step_id.clone(), foreign_warning(pgid));
                break;
            }
            if report.unproven.contains(&pgid) {
                warnings.insert(row.step_id.clone(), survivor_warning(row.pid, pgid));
                break;
            }
        }
    }
    warnings
}

/// Dopisuje samo zdanie o grupie do biegów, których status **jest prawdziwy** i zostaje.
///
/// Zwraca liczbę przepisanych kroków. Statusu nie tyka ani razu i to jest cała różnica wobec
/// [`write_back`]: bieg zamknięty jako `cancelled` ma takim zostać, a wiedza o jego ocalałym
/// jest dopiskiem do kroku, nie powodem, żeby przepisać cokolwiek innego.
fn write_back_warnings(
    where_they_live: &BTreeMap<String, PathBuf>,
    already_rewritten: &[&String],
    rows: &[RecoveryRow],
    warnings: &BTreeMap<String, String>,
) -> usize {
    let mut written = 0;
    for (run_id, dir) in where_they_live {
        if already_rewritten.contains(&run_id) {
            continue;
        }
        let mine: BTreeMap<&str, &str> = rows
            .iter()
            .filter(|row| &row.run_id == run_id)
            .filter_map(|row| {
                warnings
                    .get(&row.step_id)
                    .map(|said| (row.step_id.as_str(), said.as_str()))
            })
            .collect();
        if mine.is_empty() {
            continue;
        }
        written += note_on_steps(dir, &mine);
    }
    written
}

/// Wpisuje zdanie przy wymienionych krokach `run.json` — **w miejsce**, bez gubienia pól.
///
/// Ten sam idiom, co [`write_back`], i z tego samego powodu: plik biegu niesie migawkę grafu
/// i klucze, których ta wersja może nie znać, więc czytamy i piszemy `Value`.
fn note_on_steps(dir: &Path, said: &BTreeMap<&str, &str>) -> usize {
    let Some(mut run) = read_run(dir) else {
        return 0;
    };
    let mut written = 0;
    {
        let Some(rows) = run
            .as_object_mut()
            .and_then(|map| map.get_mut("steps"))
            .and_then(Value::as_array_mut)
        else {
            return 0;
        };
        for row in rows.iter_mut() {
            let Some(step) = row.as_object_mut() else {
                continue;
            };
            let id = step
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let Some(warning) = said.get(id.as_str()) else {
                continue;
            };
            // Wcześniejszy błąd kroku nie może ukryć faktu o jego grupie: historia renderuje
            // tylko to jedno pole (`PastStepWire.error`).
            step.insert("error".to_owned(), Value::String((*warning).to_owned()));
            written += 1;
        }
    }
    if written > 0 && publish_run(dir, &run) {
        written
    } else {
        0
    }
}

/// Biegi, które stały na PYTANIU, kiedy okno zniknęło.
///
/// # Po co to jest osobno
///
/// Bo przebieg wyżej ich nie widzi i nie ma jak: pyta o kroki stojące w `running`, żeby mieć co
/// dobić, a bieg zaparkowany na punkcie kontrolnym **nie ma ani jednego takiego kroku**. Nic nie
/// pracuje, nic nie pali pieniędzy — i właśnie dlatego stoi tak w nieskończoność. Zmierzone
/// u właściciela 2026-08-23: bieg `20260819-160548` czekał na odpowiedź **czwarty dzień**, przez
/// kilkanaście restartów aplikacji, i żadne sprzątanie go nie dotykało.
///
/// # Dlaczego to jest porzucenie, a nie cierpliwość
///
/// Pytanie punktu kontrolnego żyje WYŁĄCZNIE w żywym strumieniu okna (`feed/model.ts`: `waiting`
/// bierze się z linii, która przyjechała na drucie). Okno, które zniknęło, zabrało je ze sobą —
/// a `continue_run` nie bierze identyfikatora biegu, więc nie ma czym w ten bieg wycelować.
/// Zostaje więc bieg, na który nie da się odpowiedzieć ŻADNĄ drogą. Nazwanie tego „pauzą" jest
/// obietnicą, której nie ma jak dotrzymać.
///
/// # Dlaczego wolno to zrobić bez pytania o rozruch maszyny
///
/// Bo tu nie ma do kogo strzelać. Strażnik `boot_id` broni niewinnych procesów przed sygnałem
/// (`recovery::decide`), a ten przebieg nie wysyła ani jednego sygnału — przepisuje jedno słowo
/// w pliku. Warunkiem jest za to CHWILA: sprzątanie biegnie, zanim to okno cokolwiek uruchomi
/// (`ipc::AppState::settle_everything_left_behind`), więc każda pauza zastana w tym momencie
/// należy do kogoś, kogo już nie ma.
///
/// Kroków nie ruszamy. Żaden z nich nie pracował, a `pending` mówi o nich prawdę: nie zaczęły się
/// i już się nie zaczną. Zdanie o tym niesie bieg, bo to jego dotyczy.
fn settle_the_parked(project: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(project.join(RUNS_DIR)) else {
        return 0;
    };
    let mut settled = 0;
    for entry in entries.flatten() {
        let dir = entry.path();
        let Some(run) = read_run(&dir) else { continue };
        if run.get("status").and_then(Value::as_str) != Some("paused") {
            continue;
        }
        /* Bieg z krokiem w `running` należy do przebiegu wyżej — ten ma co dobić, więc musi
         * przejść przez strażnika rozruchu maszyny. Tutaj wchodzą tylko te, przy których nie ma
         * ani jednego żywego kroku. */
        let anything_working = run
            .get("steps")
            .and_then(Value::as_array)
            .is_some_and(|steps| {
                steps
                    .iter()
                    .any(|one| one.get("status").and_then(Value::as_str) == Some("running"))
            });
        if anything_working {
            continue;
        }
        if write_back_with_reason(&dir, recovery::RUN_INTERRUPTED, RUN_LEFT_ON_A_QUESTION) {
            settled += 1;
        }
    }
    settled
}

/// Jak [`write_back`], ale zapisuje też zdanie na samym biegu i nie tyka kroków.
fn write_back_with_reason(dir: &Path, run_status: &str, why: &str) -> bool {
    let Some(mut run) = read_run(dir) else {
        return false;
    };
    let at = crate::commands::run::now_ms();
    let Some(map) = run.as_object_mut() else {
        return false;
    };
    map.insert("status".to_owned(), Value::String(run_status.to_owned()));
    if map.get("ended_at").is_none_or(Value::is_null) {
        map.insert("ended_at".to_owned(), Value::from(at));
    }
    if map.get("error").is_none_or(Value::is_null) {
        map.insert("error".to_owned(), Value::String(why.to_owned()));
    }
    publish_run(dir, &run)
}

/// Wiersze do rozstrzygnięcia, przeczytane z plików biegów tego folderu.
///
/// LUSTRO ZAPYTANIA Z `recovery::rows_to_judge`, co do warunku: bierzemy każdy krok biegu, który
/// stoi w `running` albo `paused`, **albo** żywy krok (`ready`/`running`) z biegu o innym statusie.
/// Rozjazd tych dwóch warunków znaczyłby, że po skasowaniu bazy odzyskiwanie sądzi inny zbiór niż
/// przed nim.
fn rows_from_files(
    project: &Path,
) -> (
    Vec<RecoveryRow>,
    BTreeMap<String, PathBuf>,
    Vec<SavedService>,
) {
    let mut rows = Vec::new();
    let mut where_they_live = BTreeMap::new();
    let mut services = Vec::new();
    let Ok(entries) = std::fs::read_dir(project.join(RUNS_DIR)) else {
        return (rows, where_they_live, services);
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        let Some(run) = read_run(&dir) else { continue };
        let run_status = run
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let run_id = run
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let boot = run
            .get("boot_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let Some(steps) = run.get("steps").and_then(Value::as_array) else {
            continue;
        };
        if add_service_rows(project, &dir, &run, &mut rows, &mut services) {
            where_they_live.insert(run_id.clone(), dir.clone());
        }
        let has_cut_off_work = matches!(run_status.as_str(), "running" | "paused")
            || steps.iter().any(|one| {
                matches!(
                    one.get("status").and_then(Value::as_str),
                    Some("ready" | "running")
                )
            })
            /* TRZECI WARUNEK: KROK ZOSTAWIŁ GRUPY I NIE MA DOWODU (2026-09, Z-01d).
             *
             * Bez niego bieg zatrzymany przez człowieka jest tu NIEWIDZIALNY — schodzi jako
             * `cancelled`, jego kroki jako `cancelled`, a wnuk, którego eskalacja nie dosięgła,
             * biegnie dalej i dalej pali limit u dostawcy. To jest dokładnie ten przypadek, dla
             * którego zapis `pgids` w ogóle powstał, i do tego dnia nikt go nawet nie czytał. */
            || steps.iter().any(|one| {
                !numbers(one, "pgids").is_empty()
                    && one.get("death_proof").and_then(Value::as_bool) != Some(true)
            });
        if !has_cut_off_work {
            continue;
        }
        where_they_live.insert(run_id.clone(), dir);
        for step in steps {
            rows.push(RecoveryRow {
                step_id: text(step, "id"),
                run_id: run_id.clone(),
                run_status: run_status.clone(),
                step_status: text(step, "status"),
                run_boot_id: boot.clone(),
                pid: number(step, "pid"),
                pgid: number(step, "pgid"),
                pgids: numbers(step, "pgids"),
                // Brak klucza czyta się jak „nie dowiedziono", nigdy jak „dowiedziono": starsze
                // pliki biegów nie mają go wcale, a `false` jest jedyną bezpieczną odpowiedzią.
                death_proof: step.get("death_proof").and_then(Value::as_bool) == Some(true),
            });
        }
    }
    (rows, where_they_live, services)
}

/// Tylko migawka jednego przebiegu recovery, nie drugi rejestr żywych usług.
struct SavedService {
    dir: PathBuf,
    record: ServiceRecord,
}

/// Osobne źródło tych samych `RecoveryRow`, nie druga polityka spawnu ani kill.
/// Zakończony Serve nie ma pgids w starym kroku; trwałe usługi mają własny zapis.
fn add_service_rows(
    project: &Path,
    dir: &Path,
    run: &Value,
    rows: &mut Vec<RecoveryRow>,
    services: &mut Vec<SavedService>,
) -> bool {
    let before = rows.len();
    match services_bound_to_run(project, dir, run) {
        Ok(records) => {
            for record in records {
                if record.state == ServiceState::Dead
                    || record.state == ServiceState::Unknown
                    || record.lifetime == crate::workflow::ServiceLifetime::Unknown
                {
                    continue;
                }
                rows.push(RecoveryRow {
                    step_id: record.step_id.clone(),
                    run_id: text(run, "id"),
                    run_status: text(run, "status"),
                    // Ten wiersz dokłada WYŁĄCZNIE grupę pozostawioną po spawnie. Stan samego
                    // kafelka sądzi jego zwykły wiersz; Running tutaj liczyłoby go dwa razy.
                    step_status: "succeeded".to_owned(),
                    run_boot_id: run
                        .get("boot_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    pid: None,
                    pgid: record.pgid,
                    pgids: record.pgid.into_iter().collect(),
                    death_proof: false,
                });
                services.push(SavedService {
                    dir: dir.to_path_buf(),
                    record,
                });
            }
        }
        Err(error) => tracing::warn!(run = %text(run, "id"), %error,
            "the saved services could not be identified; their folders remain protected"),
    }
    rows.len() != before
}

/// Spójność plikowych adresów musi być sprawdzona PRZED oddaniem jakiegokolwiek PGID
/// do polityki recovery. Cudzy record nie otrzymuje tożsamości z aktualnego workspace.
pub(super) fn services_bound_to_run(
    project: &Path,
    dir: &Path,
    run: &Value,
) -> std::io::Result<Vec<ServiceRecord>> {
    let records = read_service_records(dir)?;
    let workspace = supervisor::publication_root_key(project)?;
    let id = text(run, "id");
    let steps = run.get("steps").and_then(Value::as_array);
    for record in &records {
        // Historyczne rekordy mogą nazywać systemowy alias /var. Klucz służy
        // porównaniu, nigdy jako dowód własności podmienionego katalogu.
        let record_workspace = supervisor::publication_root_key(&record.reference.workspace)?;
        let record_cwd = supervisor::publication_root_key(&record.cwd)?;
        supervisor::PublicationRoot::open(&record.reference.workspace)?;
        match supervisor::PublicationRoot::open(&record.cwd) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let step_matches = steps.is_some_and(|steps| {
            steps.iter().any(|step| {
                text(step, "id") == record.step_id
                    && text(step, "node_key") == record.reference.node_key
            })
        });
        if record_workspace != workspace
            || record.reference.run_id != id
            || record.reference.generation == 0
            || !step_matches
            || !record.cwd.is_absolute()
            || !record_cwd.starts_with(&workspace)
            || record
                .cwd
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(std::io::Error::other(
                "the saved service does not belong to this run and folder",
            ));
        }
    }
    Ok(records)
}

fn record_service_outcomes(
    services: &[SavedService],
    plan: &recovery::RecoveryPlan,
    report: &recovery::RecoveryReport,
) {
    for saved in services {
        let Some(pgid) = saved.record.pgid else {
            continue;
        };
        if !plan
            .reap
            .iter()
            .any(|target| target.pgid == pgid && target.run_id == saved.record.reference.run_id)
        {
            continue;
        }
        let state = if report.reaped.contains(&pgid) {
            ServiceState::Dead
        } else if report.unproven.contains(&pgid) || report.foreign.contains(&pgid) {
            ServiceState::Unproven
        } else {
            // Brak boot, inny boot albo nieużywalny numer nie są ESRCH dla tej grupy.
            // Bez dowodu nie zwalniamy kopii i nie wymyślamy nowego adresu procesu.
            continue;
        };
        if saved.record.state == state {
            continue;
        }
        let mut record = saved.record.clone();
        record.state = state;
        if let Err(error) = save_service_record(&saved.dir, &record) {
            // Stary plik nadal blokuje cleanup: samo wysłanie sygnału nie publikuje Dead.
            tracing::error!(service = %record.reference.service_id, %error,
                "the recovered service proof could not be saved");
        }
    }
}

fn services_protect_copy(project: &Path, dir: &Path, run: &Value, cwd: &Path) -> bool {
    let Ok(records) = services_bound_to_run(project, dir, run) else {
        return true;
    };
    records.iter().any(|record| {
        let same_path = record.cwd == cwd
            || supervisor::publication_root_key(&record.cwd)
                .ok()
                .zip(supervisor::publication_root_key(cwd).ok())
                .is_some_and(|(owned, actual)| owned == actual);
        same_path
            && (record.keeps_copy()
                || !supervisor::PublicationRoot::open(cwd)
                    .is_ok_and(|root| root.identity() == record.copy_identity))
    })
}

/// Wpisuje rozstrzygnięcie z powrotem do `run.json` — **w miejsce**, bez gubienia pól.
///
/// Czytamy i piszemy `Value`, a nie typowaną strukturę, i to jest rozstrzygnięcie: plik biegu
/// niesie migawkę grafu, przelotki vendorów i klucze, których ta wersja może nie znać. Przepisanie
/// go przez typ tej wersji skasowałoby wszystko, czego typ nie ma — czyli dokładnie tę wadę,
/// przed którą `AgentStep::extra` broni pliki workflow.
fn write_back(
    dir: &Path,
    run_status: &str,
    steps: &[(&str, &str)],
    survivor_warnings: &BTreeMap<String, String>,
) -> bool {
    let Some(mut run) = read_run(dir) else {
        return false;
    };
    let at = crate::commands::run::now_ms();
    let Some(map) = run.as_object_mut() else {
        return false;
    };
    map.insert("status".to_owned(), Value::String(run_status.to_owned()));
    if map.get("ended_at").is_none_or(Value::is_null) {
        map.insert("ended_at".to_owned(), Value::from(at));
    }
    if let Some(rows) = map.get_mut("steps").and_then(Value::as_array_mut) {
        for row in rows.iter_mut() {
            let Some(step) = row.as_object_mut() else {
                continue;
            };
            let id = step
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let Some((_, status)) = steps.iter().find(|(want, _)| *want == id.as_str()) else {
                continue;
            };
            step.insert("status".to_owned(), Value::String((*status).to_owned()));
            if step.get("ended_at").is_none_or(Value::is_null) {
                step.insert("ended_at".to_owned(), Value::from(at));
            }
            if let Some(warning) = survivor_warnings.get(&id) {
                // 2026-08-27: wcześniejszy błąd kroku nie może ukryć faktu, że jego proces
                // przeżył sprzątanie; historia renderuje tylko to jedno pole błędu.
                step.insert("error".to_owned(), Value::String(warning.clone()));
            } else if step.get("error").is_none_or(Value::is_null) {
                step.insert("error".to_owned(), Value::String(STEP_CUT_OFF.to_owned()));
            }
        }
    }
    publish_run(dir, &run)
}

/// Publikuje pojedynczy zaktualizowany receipt wspólnym durable replace z T-202.
///
/// 2026-08-28 (T-152): recovery zachowuje nieznane pola przez `Value`, ale pełne bajty muszą
/// wejść przez ten sam fsync/rename/no-follow rdzeń co pozostałe pliki będące prawdą. Reconcile
/// nigdy nie tworzy `run.json`; polityka trybu jest wyłącznie bezpiecznym defaultem, gdyby cel
/// zniknął pomiędzy odczytem a publikacją.
fn publish_run(dir: &Path, run: &Value) -> bool {
    let Ok(mut bytes) = serde_json::to_vec_pretty(run) else {
        return false;
    };
    bytes.push(b'\n');
    DurableFilePublisher::new(dir)
        .atomic_replace(
            &dir.join(RUN_FILE),
            &bytes,
            ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
        )
        .is_ok()
}

/// Zdanie trafiające do `PastStepWire.error`, czyli jedynego błędu pokazywanego w historii.
fn survivor_warning(leader_pid: Option<i32>, process_group_id: i32) -> String {
    match leader_pid {
        Some(leader_pid) => format!(
            "This process survived Loadout's attempt to stop it. Inspect it manually: PID \
             {leader_pid}; PGID {process_group_id}."
        ),
        // Starszy plik może nie mieć PID-u. Nie wymyślamy liczby, ale nadal pokazujemy PGID,
        // który domykacz naprawdę sprawdził i po którym człowiek może rozpoznać grupę.
        None => format!(
            "This process group survived Loadout's attempt to stop it. Inspect it manually: \
             PGID {process_group_id}."
        ),
    }
}

/// Zdanie o grupie, która **należy do innego biegu** — czyli do której Loadout nie strzelił.
///
/// 2026-09 (Z-01d) — po angielsku (D5) i bez naszych słów z drutu (niezmiennik 14). Osobne od
/// [`survivor_warning`], bo mówi o czymś innym: tamto znaczy „próbowaliśmy i nie wyszło", a to
/// znaczy „nie próbowaliśmy, bo to nie nasze". Człowiek robi po nich dwie różne rzeczy — po
/// tamtym idzie ubić proces, po tym nie ma czego ubijać, a numer w pliku jest po prostu stary.
fn foreign_warning(process_group_id: i32) -> String {
    format!(
        "The process group written down for this step now belongs to a different run, so Loadout \
         left it alone. Nothing was stopped: PGID {process_group_id}."
    )
}

/// `run.json` tego katalogu, albo `None`. Nieczytelny plik jest jednym biegiem mniej.
fn read_run(dir: &Path) -> Option<Value> {
    let bytes = std::fs::read(dir.join(RUN_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn text(step: &Value, key: &str) -> String {
    step.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn number(step: &Value, key: &str) -> Option<i32> {
    step.get(key)
        .and_then(Value::as_i64)
        .and_then(|one| i32::try_from(one).ok())
}

/// Lista liczb spod `key`. Brak klucza, zła forma i pozycja, która liczbą nie jest, dają pustą
/// listę albo o jedną pozycję mniej — nigdy odmowy otwarcia folderu (niezmiennik 5).
fn numbers(step: &Value, key: &str) -> Vec<i32> {
    step.get(key)
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_i64)
                .filter_map(|one| i32::try_from(one).ok())
                .collect()
        })
        .unwrap_or_default()
}
