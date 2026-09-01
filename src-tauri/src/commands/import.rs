//! Granica IPC importu. Czysty rdzeń mieszka w `crate::import`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::commands::Drivers;
use crate::commands::skills::{
    Ended, MAY_STILL_BE_RUNNING, give_up_after, off_the_wire, one_turn, some_text,
    the_agent_saved_as,
};
use crate::engine::drivers::{AgentHandle, DecodedEvent, FinishReason, Policy, RunSpec};
use crate::engine::supervisor::GroupProof;
use crate::import::apply::ImportReceipt;
use crate::import::compare::{self, Comparison};
use crate::import::{Compatibility, ImportError, ImportPreview, Result};
use crate::library::agents::{Overrides, resolve};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySetup {
    pub workspace: PathBuf,
    pub expected_source_hashes: BTreeMap<PathBuf, String>,
    pub enable_connections: Vec<String>,
    /// Elementy `needs_choice`, które człowiek jawnie postanowił zostawić poza migracją.
    #[serde(default)]
    pub leave_out: Vec<String>,
    /// Pozycje, których człowiek nie chce zapisać. Osobne od usunięcia jednego zachowania.
    #[serde(default)]
    pub excluded_items: Vec<String>,
    /// Pozycje zachowane w planie, ale bez zachowania wymagającego rozstrzygnięcia.
    #[serde(default)]
    pub without_behavior: Vec<String>,
}

pub fn scan_setup_inner(home: &Path, workspace: &Path) -> Result<ImportPreview> {
    // Katalog domowy CZŁOWIEKA, nie biblioteka Loadouta: stąd czytamy `~/.claude.json`, żeby
    // serwery zapisane `claude mcp add --scope local|user` też trafiły na listę.
    crate::import::translate::preview_with_personal(workspace, home)
}

/// Jeszcze raz czyta repo i akceptuje z webviewa wyłącznie wybór włączenia znanych połączeń.
pub fn apply_setup_inner(
    home: &Path,
    personal: &Path,
    request: &ApplySetup,
) -> Result<ImportReceipt> {
    /* TEN SAM WIDOK, CO PRZY SCANIE. Gdyby tu stała `preview()` bez twoich zakresów, włączenie
     * `linear-server` wracałoby jako „The import requested a connection that was not in the
     * latest Scan." — czyli odmowa dla pozycji, którą ekran właśnie pokazał. */
    let mut preview =
        crate::import::translate::preview_with_personal(&request.workspace, personal)?;
    if preview.draft.source_hashes != request.expected_source_hashes {
        return Err(ImportError::Changed);
    }
    let requested: BTreeSet<&str> = request
        .enable_connections
        .iter()
        .map(String::as_str)
        .collect();
    let known: BTreeSet<&str> = preview
        .draft
        .connections
        .iter()
        .map(|connection| connection.id.as_str())
        .collect();
    if !requested.is_subset(&known) {
        return Err(ImportError::Save(
            "The import requested a connection that was not in the latest Scan.".to_owned(),
        ));
    }
    let leave_out: BTreeSet<&str> = request.leave_out.iter().map(String::as_str).collect();
    let resolvable: BTreeSet<&str> = preview
        .draft
        .report
        .mappings
        .iter()
        .filter(|mapping| mapping.compatibility.blocks())
        .map(|mapping| mapping.item_id.as_str())
        .collect();
    if !leave_out.is_subset(&resolvable) {
        return Err(ImportError::Save(
            "The import tried to leave out an item that was not unresolved in the latest Scan."
                .to_owned(),
        ));
    }
    let excluded_items: BTreeSet<&str> =
        request.excluded_items.iter().map(String::as_str).collect();
    let known_items: BTreeSet<&str> = preview
        .draft
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    if !excluded_items.is_subset(&known_items) {
        return Err(ImportError::Save(
            "The import tried to exclude an item that was not in the latest Scan.".to_owned(),
        ));
    }
    let without_behavior: BTreeSet<&str> = request
        .without_behavior
        .iter()
        .map(String::as_str)
        .collect();
    let behavior_choices: BTreeSet<&str> = preview
        .draft
        .report
        .mappings
        .iter()
        .filter(|mapping| mapping.compatibility == Compatibility::NeedsChoice)
        .map(|mapping| mapping.item_id.as_str())
        .collect();
    if !without_behavior.is_subset(&behavior_choices) {
        return Err(ImportError::Save(
            "The import tried to remove behavior from an item that did not offer that choice in the latest Scan."
                .to_owned(),
        ));
    }
    let excluded: BTreeSet<&str> = leave_out.union(&excluded_items).copied().collect();
    if !excluded.is_disjoint(&without_behavior) {
        return Err(ImportError::Save(
            "An imported item cannot be excluded and kept without behavior at the same time."
                .to_owned(),
        ));
    }
    for mapping in &mut preview.draft.report.mappings {
        if excluded.contains(mapping.item_id.as_str()) {
            mapping.compatibility = Compatibility::Adjusted;
            "You chose not to import this item.".clone_into(&mut mapping.message);
        } else if without_behavior.contains(mapping.item_id.as_str()) {
            mapping.compatibility = Compatibility::Adjusted;
            "This item will be imported without that project behavior."
                .clone_into(&mut mapping.message);
        }
    }
    preview
        .draft
        .items
        .retain(|item| !excluded.contains(item.id.as_str()));
    crate::import::translate::keep_selected_outputs(&mut preview.draft);
    for connection in &mut preview.draft.connections {
        connection.enabled = requested.contains(connection.id.as_str());
    }
    crate::import::translate::refresh_statuses(&mut preview.draft);
    crate::import::apply::apply(home, &preview.draft)
}

// ── PORÓWNANIE KOPII ───────────────────────────────────────────────────────────────────────
//
// 2026-08-29 (T-76). Skan zostawia pozycje, o których adapter sam mówi „Let an agent compare
// them before import." — i do tego dnia nikt tego agenta nie wołał. Ta droga go woła: jedno
// pytanie, jedna tura, poza grafem, dokładnie tym samym kształtem, którym pyta draft
// umiejętności (`commands::skills::draft_skill_inner`). Maszyneria tury jest tamta,
// nieprzepisana (niezmiennik 23); tutaj jest wyłącznie to, czym te dwa pytania się różnią:
// inna treść, inny slot i inne miejsce, w którym ląduje odpowiedź.
//
// TRZY GRANICE, KTÓRYCH TA DROGA NIE PRZEKRACZA, i każda jest tu widoczna z kodu:
//
//   * **agent doradza, decyduje człowiek.** Zwracamy [`Comparison`] i ani jednego zapisu.
//     Kopia „wybrana" przez agenta nie jedzie na dysk żadną drogą — to samo rozgraniczenie,
//     co przy weryfikatorze (AGENTS.md §2).
//   * **nic się nie uruchamia.** `Policy::ReadOnly`, `reaches_the_web: false`, `extra_dirs`
//     puste, `cwd` w pustym katalogu roboczym POZA projektem. Treść kopii jedzie w pytaniu,
//     więc obietnica ekranu („Scan reads setup files only. It does not run hooks, skills,
//     agents, or connections.") zostaje prawdą także tutaj.
//   * **pozycja nie traci pochodzenia.** [`Comparison::compared`] niesie te same ścieżki,
//     które wiersz nosił przed kliknięciem, i nie zdejmuje z niego ani jednej.

/// Odmowa dla drugiego pytania zadanego, kiedy pierwsze jeszcze trwa.
const ALREADY_COMPARING: &str =
    "An agent is already comparing copies. Wait for that one to finish, or stop it first.";

/// Odmowa dla pozycji, której w świeżym planie nie ma.
///
/// Ten sam kształt zdania, co przy [`apply_setup_inner`] („…that was not in the latest Scan"):
/// obie drogi czytają projekt jeszcze raz i obie mogą zastać go zmienionym, więc człowiek ma
/// przeczytać jedno wyjaśnienie, nie dwa (niezmiennik 13).
const NOT_IN_THE_SCAN: &str =
    "This tried to compare an item that was not in the latest Scan. Scan the project again.";

/// Odmowa dla pozycji, z której nie da się przeczytać ani jednego pliku.
const NOTHING_TO_READ: &str =
    "Loadout could not read any of the files this item came from, so there is nothing to compare.";

/// Odmowa dla tury, która skończyła się czysto i bez ani jednego zdania.
const SAID_NOTHING: &str =
    "The agent finished without saying anything about these copies. Ask again.";

/// Ile zdarzeń mieści się w kanale porównania, zanim sterownik na nim stanie.
///
/// Ta sama liczba, co w `commands::skills`, i tak samo niepożyczona: pojemność bufora jednego
/// pytania nie jest tym samym faktem, co pojemność bufora drugiego. Tutaj nikt tych linii nie
/// czyta, więc liczba decyduje wyłącznie o tym, jak często drenaż budzi się przy zalewie.
const EVENT_QUEUE: usize = 256;

/// Czym skończyło się jedno porównanie kopii.
///
/// **Anulowanie jest wariantem wartości, nigdy błędem** (niezmiennik 7): `Err(Cancelled)`
/// zmusza każdego wołającego do rozróżniania „to się nie udało" od „to zatrzymał człowiek",
/// a rozróżnienie zgubione raz jest zgubione wszędzie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareOutcome {
    /// Agent powiedział, czym te kopie się różnią.
    Compared(Comparison),
    /// Człowiek zatrzymał porównanie.
    Cancelled,
}

/// Miejsce na JEDNO porównanie naraz i uchwyt do tego, które trwa teraz.
///
/// **Osobne od [`crate::commands::skills::Drafting`], mimo identycznego kształtu**, i to nie
/// jest przeoczenie: draft umiejętności i porównanie kopii to dwa różne pytania, zadawane
/// z dwóch różnych ekranów. Jedno miejsce na oba znaczyłoby, że rozpoczęte porównanie odmawia
/// napisania umiejętności w sąsiedniej karcie — a Stop w jednej sekcji ubija robotę w drugiej.
///
/// Powód, dla którego miejsce jest JEDNO, a nie „ile naraz z suwaka", stoi w całości przy
/// [`crate::commands::skills::Drafting`]: tej liczby nie ma dziś z czego wziąć, a granica
/// udająca wspólną pulę byłaby czwartym miejscem, w którym nie znaczy tego, co mówi.
#[derive(Debug, Default)]
pub struct Comparing {
    /// `Some` znaczy „ktoś właśnie porównuje" i niesie token **tego** porównania.
    ///
    /// `std::sync::Mutex` i **nigdy trzymany przez `await`** (niezmiennik 8): każde wzięcie
    /// tego zamka mieści się w jednym wyrażeniu, które kopiuje token albo go odkłada i oddaje
    /// zamek. Zamek trzymany przez turę zawiesiłby Stop na czas czytania przez model — czyli
    /// dokładnie wtedy, kiedy Stop jest do czegokolwiek potrzebny.
    ///
    /// Token jest **własny**, a nie wzięty z uchwytu biegu: `AppState.live` jest PODMIENIANY
    /// przy każdym Starcie, więc porównanie trzymające się tamtego traci swój token w chwili,
    /// w której człowiek uruchomi bieg w innej karcie.
    working: Mutex<Option<CancellationToken>>,
}

impl Comparing {
    /// Miejsce, na którym nikt nie porównuje.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// „Stop" z okna: zatrzymuje porównanie, które trwa teraz. Bez porównania nie robi nic.
    ///
    /// Dowód zejścia grupy **nie wraca tędy** i to jest wybór: [`GroupProof`] czyta tura
    /// (`handle.cancel().await`), a niesie go odpowiedź [`compare_copies_inner`] — czyli to
    /// samo wywołanie, na które okno już czeka (niezmiennik 13).
    pub fn stop(&self) {
        // Zamek wzięty i oddany w JEDNYM wyrażeniu, przed czymkolwiek, co czeka (niezmiennik 8).
        // Zatruty zamek odplatamy zamiast panikować: `panic!` w agentowym runtime zabiera cały
        // bieg (AGENTS.md §4), a uchwyt po panice jednej tury jest dalej poprawnym uchwytem.
        let token = self
            .working
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(token) = token {
            token.cancel();
        }
    }

    /// Zajmuje jedyne miejsce na porównanie. `None` znaczy „ktoś już porównuje".
    ///
    /// Odmowa mieszka TUTAJ, a nie w widoku: przycisk wygaszony na ekranie jest sugestią, bo
    /// zostaje klawiatura i wywołanie komendy wprost. Warunkiem jest samo WYWOŁANIE.
    fn claim(&self) -> Option<Claim<'_>> {
        // Sprawdzenie i zajęcie w JEDNYM wzięciu zamka. Dwa osobne zostawiają okno, w którym
        // dwa pytania zadane w tej samej chwili widzą oba wolne miejsce — a wtedy „jeden naraz"
        // jest zdaniem, nie własnością. Zamek ginie razem z tym wyrażeniem, przed pierwszym
        // `await` (niezmiennik 8).
        let mut working = self.working.lock().unwrap_or_else(PoisonError::into_inner);
        if working.is_some() {
            return None;
        }
        let stop = CancellationToken::new();
        *working = Some(stop.clone());
        Some(Claim {
            comparing: self,
            stop,
        })
    }

    /// Oddaje miejsce. Wołane wyłącznie przez [`Claim`], czyli na każdej drodze wyjścia z tury.
    fn release(&self) {
        *self.working.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

/// Zajęte miejsce na jedno porównanie — oddawane samo, na KAŻDEJ drodze wyjścia.
///
/// Struktura z [`Drop`], a nie para wywołań „zajmij" / „oddaj": dróg wyjścia z jednej tury jest
/// osiem (odmowa slotu, pozycja spoza planu, nieczytelne kopie, odmowa biblioteki, nieudany
/// start, Stop, limit czasu i sukces), a miejsce oddane w siedmiu z ośmiu jest miejscem,
/// którego już nikt nigdy nie dostanie.
struct Claim<'a> {
    comparing: &'a Comparing,
    /// Token TEGO porównania — ten sam, który cofa [`Comparing::stop`].
    stop: CancellationToken,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.comparing.release();
    }
}

/// Pusty katalog roboczy jednej tury — POZA projektem i sprzątany na każdej drodze wyjścia.
///
/// # Dlaczego pusty i dlaczego nie w projekcie
///
/// Bo `cwd` jest jedyną rzeczą, którą agent na `Policy::ReadOnly` widzi bez pytania. Katalog
/// projektu w tym polu oddawałby mu do przeczytania CAŁE cudze repozytorium — czyli robiłby
/// to, czego ekran importu obiecuje nie robić, i to przy pytaniu o dwa pliki. Kopie jadą więc
/// w pytaniu (`import::compare::question`), a tura pracuje w katalogu, w którym nic nie leży.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(run: Uuid) -> std::io::Result<Self> {
        let root = std::env::temp_dir().join(format!("loadout-compare-{run}"));
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Bez `?` i bez `expect`: to jest ostatnia droga wyjścia z tury, a katalog, którego nie
        // udało się usunąć z katalogu tymczasowego, nie jest niczym, o czym warto przewrócić
        // odpowiedź dla człowieka.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Jedna pozycja planu → zdania agenta o jej kopiach, jedną turą poza grafem.
///
/// `library` to `~/.loadout`, a `personal` to katalog domowy CZŁOWIEKA — oba przychodzą
/// **argumentem**, nigdy czytane ze środka: katalog odczytany tutaj znaczyłby, że każdy test
/// pyta prawdziwą bibliotekę i prawdziwe `~/.claude.json`.
///
/// `agent` jest **identyfikatorem** zapisanego agenta: model, prompt systemowy i limit czasu
/// biorą się z jego definicji przez `library::agents::resolve`, a nie z niczego wpisanego tutaj.
/// Dial bezpieczeństwa jest jedynym polem, którego z definicji NIE bierzemy — wolno go tylko
/// obniżyć (D6), a tu jest obniżony do końca.
///
/// Odmowa jest **zdaniem dla człowieka**, a nie wariantem [`ImportError`]: ten enum opisuje
/// skanowanie i zapis planu, a żadne z jego czterech zdań nie mówi prawdy o turze agenta.
/// Nowy wariant znaczyłby, że `import/mod.rs` rośnie o pojęcie, którego czysty rdzeń importu
/// nie ma i mieć nie powinien.
pub async fn compare_copies_inner(
    library: &Path,
    personal: &Path,
    drivers: &Drivers,
    comparing: &Comparing,
    workspace: &Path,
    item_id: &str,
    agent: &str,
) -> std::result::Result<CompareOutcome, String> {
    // JEDEN NARAZ, i odmowa PRZED czymkolwiek, co dotyka dysku albo sterownika: drugie pytanie
    // ma zostawić pierwsze nietknięte, a jedynym sposobem, żeby to była prawda, jest nie zaczynać.
    let Some(claim) = comparing.claim() else {
        return Err(ALREADY_COMPARING.to_owned());
    };

    /* TEN SAM WIDOK, CO PRZY SCANIE, i tą samą funkcją, którą czyta [`apply_setup_inner`].
     * Plan przysłany z okna byłby planem, który okno mogło poprawić — a stąd jedzie treść
     * cudzych plików do modelu, więc ścieżki muszą pochodzić z tego, co Loadout właśnie
     * przeczytał z dysku, nie z tego, co dostał przez granicę. */
    let preview = crate::import::translate::preview_with_personal(workspace, personal)
        .map_err(|error| error.to_string())?;
    let Some(item) = preview.draft.items.iter().find(|item| item.id == item_id) else {
        return Err(NOT_IN_THE_SCAN.to_owned());
    };

    // Kopie czytamy TUTAJ, po tej stronie granicy, i jadą dalej wyłącznie jako tekst pytania.
    let copies = compare::copies_of(&preview.snapshot.root, item);
    if copies.is_empty() {
        return Err(NOTHING_TO_READ.to_owned());
    }
    let shown: Vec<PathBuf> = copies.iter().map(|copy| copy.path.clone()).collect();

    // Kto ma to przeczytać. Model, prompt systemowy i limit czasu biorą się z JEGO zapisanej
    // definicji, złożonej tym samym `resolve`, którym składa je krok biegu.
    let saved = the_agent_saved_as(library, agent).map_err(|error| error.to_string())?;
    let effective = resolve(&saved, &Overrides::default())
        .map_err(|error| error.to_string())?
        .agent;

    // Ten sam identyfikator nosi tura i jej katalog roboczy: gdyby katalog kiedyś przeżył
    // awarię aplikacji, widać z jego nazwy, czyj był.
    let run = Uuid::now_v7();
    let scratch = Scratch::new(run).map_err(|error| error.to_string())?;
    let spec = RunSpec {
        run_id: run,
        // Pusty katalog POZA projektem — powód w całości stoi przy [`Scratch`].
        cwd: scratch.root.clone(),
        // Pytanie razem z treścią obu kopii jedzie jako DANE, wyłącznie stdinem
        // (niezmiennik 9): ta warstwa nie skleja komendy i nie zna ani jednej flagi vendora.
        prompt: compare::question(item, &copies),
        model: some_text(&effective.model),
        // Prompt systemowy agenta, nie pytanie. Pytanie w tym polu byłoby niezmiennikiem 9
        // złamanym po cichu, bo stąd wchodzi do argv — a argv widzi `ps` każdego użytkownika
        // tej maszyny, razem z cudzymi plikami, które w tym pytaniu jadą.
        system_append: some_text(&effective.instructions),
        // DIAL WOLNO TYLKO OBNIŻYĆ (D6). Odpowiedź wraca strumieniem, a obie kopie już tu są,
        // więc do pisania po dysku nie ma powodu — a dial skopiowany z definicji wygląda
        // poprawnie do chwili, w której ktoś każe porównać kopie najmocniejszemu agentowi.
        policy: Policy::ReadOnly,
        /* Sieci NIE MA: to pytanie dotyczy dwóch plików, które przyjechały razem z nim. */
        reaches_the_web: false,
        tools: None,
        /* Ani jednego katalogu, i to jest połowa obietnicy „Scan reads setup files only".
         * Korzeń projektu tutaj oddawałby agentowi całe cudze repozytorium do przeczytania —
         * przy pytaniu o dwa pliki, które i tak dostał w treści. */
        extra_dirs: Vec::new(),
        resume: None,
    };

    // Odbiór staje PRZED startem sterownika: vendor ma prawo powiedzieć pierwsze zdarzenia
    // jeszcze w `start`, a kanał bez odbiorcy zatrzymałby go na pierwszym pełnym buforze.
    let (events, inbox) = mpsc::channel::<DecodedEvent>(EVENT_QUEUE);
    let drain = tokio::spawn(off_the_wire(inbox));

    let compared = match (drivers)(effective.runs_with).start(spec, events).await {
        // Vendor bez adaptera odmawia dokładnie tutaj i jego zdanie jest CAŁĄ odpowiedzią:
        // panika zabrałaby okno, a cisza zostawiłaby człowieka przy kontrolce, która nic nie robi.
        Err(error) => Err(error.to_string()),
        Ok(mut handle) => {
            let limit = give_up_after(effective.give_up_after_minutes);
            let ended = one_turn(&mut *handle, &claim.stop, limit).await;
            what_came_of_it(&mut *handle, ended, limit, item.id.clone(), &shown).await
        }
    };

    // Uchwyt zszedł razem z gałęzią wyżej, więc kanał jest zamknięty i drenaż kończy się sam.
    // Czekamy na niego, zamiast go porzucić: zadanie przeżywające to wywołanie trzymałoby
    // odbiornik otwarty i przy następnym pytaniu nie dałoby się powiedzieć, czyje linie są czyje.
    let _ = drain.await;
    compared
}

/// Co z tury wynikło dla człowieka: zdania o kopiach, anulowanie jako wartość, albo odmowa.
async fn what_came_of_it(
    handle: &mut dyn AgentHandle,
    ended: Ended,
    limit: Duration,
    item_id: String,
    shown: &[PathBuf],
) -> std::result::Result<CompareOutcome, String> {
    match ended {
        // PRZEKROCZONY LIMIT IDZIE TĄ SAMĄ DROGĄ, CO STOP: przez sterownik, po dowód. Powód
        // nazywa LIMIT CZASU i liczbę, którą trzeba zmienić — inaczej człowiek szuka wady
        // w agencie, którego nikt nie zepsuł.
        Ended::Overdue => {
            let minutes = limit.as_secs() / 60;
            Err(match handle.cancel().await {
                GroupProof::Alive { .. } => format!(
                    "This comparison ran longer than its {minutes} minute limit, and Loadout \
                     could not make sure the agent stopped, so it may still be running."
                ),
                GroupProof::Dead { .. } => format!(
                    "This comparison ran longer than its {minutes} minute limit, so Loadout \
                     stopped it. Give that agent more minutes."
                ),
            })
        }
        // ANULOWANIE IDZIE PRZEZ STEROWNIK, nie przez zdjęcie zadania Rusta (niezmienniki 6 i 10).
        Ended::Stopped => match handle.cancel().await {
            // Dowód zejścia grupy jest, więc nie ma o czym mówić: anulowanie jest WARTOŚCIĄ.
            GroupProof::Dead { .. } => Ok(CompareOutcome::Cancelled),
            // Dopóki dowodu nie ma, traktujemy grupę jak żywą (niezmiennik 6). Osierocony agent
            // czyta dalej, a płaci za to człowiek.
            GroupProof::Alive { .. } => Err(MAY_STILL_BE_RUNNING.to_owned()),
        },
        Ended::Turn(Err(error)) => Err(error.to_string()),
        Ended::Turn(Ok(turn)) => {
            // Normalne zakończenie idzie przez `close`: `claude` z otwartym stdinem czeka
            // w nieskończoność, więc tura bez tego zostawia żywy proces [T1 §2, §4.6].
            let code = handle.close().await.ok().flatten();
            // Sukces to zero **i** `ok` (niezmiennik 19). Agent, który wypisał „nie dam rady"
            // i wyszedł czysto, nie porównał niczego.
            if !turn.ok || !matches!(code, None | Some(0)) {
                return Err(nothing_came_back(&turn.reason));
            }
            let said = turn.text.trim();
            if said.is_empty() {
                return Err(SAID_NOTHING.to_owned());
            }
            Ok(CompareOutcome::Compared(Comparison {
                item_id,
                // TE SAME ŚCIEŻKI, KTÓRE POJECHAŁY W PYTANIU, i ani jednej innej: zdanie „an
                // agent read A and B" ma mówić o plikach, które agent naprawdę dostał.
                compared: shown.to_vec(),
                keep: compare::what_it_suggests(said, shown),
                said: said.to_owned(),
            }))
        }
    }
}

/// Zdanie o turze, która skończyła się bez porównania.
fn nothing_came_back(reason: &FinishReason) -> String {
    match reason {
        FinishReason::Failed(said) => format!("The agent could not compare these copies: {said}"),
        FinishReason::LimitReached => {
            "The agent stopped before it finished, because it ran into a limit of its own. \
             Try again."
                .to_owned()
        }
        FinishReason::Cancelled | FinishReason::Completed => {
            "The agent stopped before it said anything. Ask again.".to_owned()
        }
    }
}
