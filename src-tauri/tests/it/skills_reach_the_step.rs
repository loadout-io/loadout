//! AC-1 dla T-79: krok dostaje DOKŁADNIE te umiejętności, które wynikają z jego agenta
//! i z nadpisania na kroku — ani jednej mniej, ani jednej więcej.
//!
//! `Agent.skills` jest polem formularza agenta od T-11, `AgentStep.overrides.skills` polem pliku
//! workflow o gotowej semantyce, a `~/.loadout/skills/<nazwa>/` kanoniczną kopią — i poza modułem
//! importu **nikt tych pól nie czyta**. To jest ta sama klasa, którą niezmiennik 29 nazywa wprost:
//! kryterium zielone, funkcja martwa. Z zewnątrz „agent nie zna tej umiejętności" jest
//! nieodróżnialne od „model nie uznał, że warto po nią sięgnąć", więc jedynym uczciwym pomiarem
//! jest to, co widzi **proces**, a nie to, co zwraca funkcja.
//!
//! **Słabą wersją tego kryterium jest `assert!(!seen.is_empty())` na kroku bez nadpisania.**
//! Przechodzi dla implementacji, która wpycha każdemu krokowi całą bibliotekę — czyli dla tej,
//! która łamie obie reguły naraz: `[]` na kroku przestaje cokolwiek znaczyć, a umiejętność,
//! której nikt nie wybrał, dojeżdża do agenta razem z resztą. Rozróżnia to sześć kroków jednego
//! biegu, sądzonych **równością zbiorów**, plus trzecia umiejętność zasiana w bibliotece i nie
//! przypisana nikomu.
//!
//! **DRUGĄ SŁABĄ WERSJĄ JEST SĄDZENIE SAMYCH NADPISAŃ.** Wybór na kroku ma dwa źródła —
//! `Overrides::skills` (patch panelu kroku) i `AgentStep::skills` (pole pliku workflow, `"all"`
//! albo lista, T3 §3.1) — i implementacja czytająca tylko pierwsze przechodzi każdy przypadek
//! z nadpisaniem. Człowiek, który zawęził umiejętności w pliku workflow, dostaje wtedy agenta
//! znającego wszystko, co ma agent, i nic tego nie mówi. Dlatego trzy z sześciu kroków niżej
//! **nie mają nadpisania w ogóle**.
//!
//! DLACZEGO ZBIÓR CZYTAMY Z DYSKU, A NIE Z POLA `StepSkills`. Bo pole odpowiada na pytanie „co
//! policzyliśmy", a to kryterium pyta „co dostał proces". Dubler patrzy więc w obie półki, do
//! których vendor naprawdę zagląda: katalog pluginu wskazany w argv (`--plugin-dir`, tak czyta
//! umiejętności Claude Code) i `.agents/skills/` w katalogu roboczym kroku (tak czyta je
//! pozostałych pięciu, T5 §3.1). Kryterium nie rozstrzyga, którą z tych dwóch dróg wybierze
//! implementacja — rozstrzyga, że zbiór na końcu drogi jest ten i tylko ten. Osobno sądzą je
//! `skills_reach_claude` i `skills_reach_codex`.
//!
//! KAŻDY KROK PRACUJE NA WŁASNEJ KOPII, i to nie jest ozdoba fikstury: krok pracujący wprost
//! w folderze człowieka jest osobnym rozstrzygnięciem (AC-4, `Why::WouldWriteIntoYourFolder`),
//! a fikstura, która by je wywoływała, mierzyłaby odmowę zamiast zbioru.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` z tego samego powodu i **wyłącznie dodane**, nie w miejsce niczego: siedem
// punktów tego kryterium (a–g) mierzy JEDEN bieg i musi stać w jednym `#[test]`. Nie jest to
// wybór stylu — sześć kroków jednej ławki dzieli jeden magazyn, jeden dubler i jedną migawkę
// tego, co dubler zobaczył, więc cięcie po granicy funkcji znaczyłoby sześć osobnych biegów
// albo stan dzielony między testami, które cargo uruchamia równolegle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte dwie mają w biegu własne wymagania
/// co do dowodów, a to kryterium sądzi zbiór umiejętności, nie sterownik.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(20);

/// Umiejętność przypisana agentowi i **wybrana** na kroku zawężającym.
const ALPHA: &str = "alpha";
/// Umiejętność przypisana agentowi i **odznaczona** na kroku zawężającym.
const BETA: &str = "beta";
/// Umiejętność, która leży w bibliotece i **nie jest przypisana nikomu**.
///
/// To jest cała druga połowa tego kryterium: implementacja, która podaje agentowi zawartość
/// katalogu `~/.loadout/skills/`, przechodzi każdą asercję o obecności `alpha` i `beta`.
const GAMMA: &str = "gamma";

fn skill_file(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Reads one file and says in a line what it is for.\n---\n\n\
         Answer with a single sentence.\n"
    )
}

/// Agent z DWIEMA umiejętnościami. `gamma` nie jest tu wymieniona i nie ma prawa nigdzie dojechać.
const SCRIBE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d1
name: Scribe
summary: Writes things down
color: slate
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: [alpha, beta]
connections: []
---
Do the work.
";

/// Sześć kroków jednego agenta, sześć odpowiedzi na pytanie „co ten krok umie".
///
/// Brak klucza `skills` w `overrides` to co innego niż `[]`, i to jest cała semantyka RFC 7396,
/// którą `library::agents::resolve` już ma: brak klucza znaczy „weź to, co ma agent", a pusta
/// lista znaczy „żadnych". Plik, w którym oba wyglądałyby tak samo, nie umiałby o tym nic
/// powiedzieć.
///
/// TRZY OSTATNIE KROKI NIE MAJĄ NADPISANIA W OGÓLE i to jest druga połowa tej semantyki.
/// `AgentStep::skills` jest polem PLIKU workflow (`"all"` albo lista, T3 §3.1) — starszym od
/// `Overrides::skills`, o tym samym znaczeniu i do 2026-08-22 też bez czytelnika. Kryterium
/// sądzące wyłącznie nadpisania nie umie odróżnić implementacji, która to pole czyta, od tej,
/// która je ignoruje: obie dają dla `overrides` ten sam wynik, a dla człowieka, który zapisał
/// wybór w pliku workflow, jedna z nich milczy. Krok szósty pyta o KOLEJNOŚĆ tych dwóch źródeł:
/// nadpisanie jest różnicą wobec agenta, więc wygrywa — odwrotna kolejność znaczyłaby, że
/// wartość domyślna jednego pola kasuje jawny wybór drugiego.
///
/// Każdy krok na WŁASNEJ KOPII — powód w nagłówku pliku.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_step_skills",
  "name": "Three steps, three answers",
  "steps": [
    {
      "kind": "agent",
      "id": "s_inherits",
      "name": "Inherits",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "inherits everything the agent has",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_none",
      "name": "None",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": { "skills": [] },
      "instructions": "none of them, on purpose",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_subset",
      "name": "Subset",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": { "skills": ["alpha"] },
      "instructions": "subset of what the agent has",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 480, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_file_all",
      "name": "File all",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "skills": "all",
      "instructions": "file-all is what this workflow file says",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 720, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_file_list",
      "name": "File list",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "skills": ["alpha"],
      "instructions": "file-list is what this workflow file says",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 960, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_override_wins",
      "name": "Override wins",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": { "skills": ["beta"] },
      "skills": ["alpha"],
      "instructions": "override-wins over what the file says",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 1200, "y": 0 }
    }
  ],
  "links": []
}
"#;

/// Instrukcja kroku → nazwa kroku. Krok rozpoznajemy po treści zadania, bo `RunSpec` nie niesie
/// nazwy kroku, a instrukcja jest tym, co ten krok naprawdę dostał (niezmiennik 9 — jedzie tam
/// jako dane). Tablica, a nie łańcuch `if`-ów: przy sześciu krokach gałąź `else` cichcem
/// przypisywałaby cudzy zbiór krokowi, którego nikt nie rozpoznał.
const STEPS: [(&str, &str); 6] = [
    ("inherits", "Inherits"),
    ("none", "None"),
    ("subset", "Subset"),
    ("file-all", "File all"),
    ("file-list", "File list"),
    ("override-wins", "Override wins"),
];

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn each_step_gets_the_skills_its_agent_and_its_overrides_add_up_to()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("scribe", SCRIBE)?;
    for name in [ALPHA, BETA, GAMMA] {
        bench.skill(name)?;
    }
    let workflow = bench.workflow("step-skills", WORKFLOW)?;
    let store = Store::open(&bench.db())?;
    let seen = Arc::new(Seen::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&seen)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 3,
        task: None,
        only: None,
        handoffs_from: None,
    };

    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; STEPS.len()],
        "all {} steps have to finish, or the assertions below are true of steps that never \
         ran. They ended as {:?}",
        STEPS.len(),
        report.steps
    );

    let looked = seen.snapshot();
    let mut named = STEPS
        .iter()
        .map(|(_, name)| (*name).to_owned())
        .collect::<Vec<_>>();
    named.sort();
    assert_eq!(
        looked.keys().cloned().collect::<Vec<_>>(),
        named,
        "every step has to reach the driver under its own name, or this test is measuring one \
         step several times. It saw: {:?}",
        looked.keys().collect::<Vec<_>>()
    );

    // (a) BRAK KLUCZA NA KROKU = WSZYSTKO, CO MA AGENT. Ten przypadek stoi pierwszy, bo
    //     implementacja oddająca każdemu krokowi pusty zbiór przechodzi punkty (b) i (c)
    //     i zostawiłaby obraz „w połowie zielony".
    assert_eq!(
        looked.get("Inherits").cloned().unwrap_or_default(),
        set(&[ALPHA, BETA]),
        "the step left the skills key off, so it takes what its agent has - both of them. \
         RFC 7396 is how the rest of this definition already merges (library::agents::resolve), \
         and a missing key there has always meant \"follow the agent\". It reached the driver \
         with {:?}",
        looked.get("Inherits")
    );

    // (b) `[]` NA KROKU = ŻADNYCH. Inna wartość niż brak klucza, i musi być inna: człowiek,
    //     który wyczyścił listę na kroku, powiedział „żadnych", a nie „nie mam zdania".
    assert_eq!(
        looked.get("None").cloned().unwrap_or_default(),
        BTreeSet::new(),
        "this step cleared the list, so it gets nothing. An empty list read as \"no opinion\" \
         hands the agent everything the human just took away, and nothing on any screen says so. \
         It reached the driver with {:?}",
        looked.get("None")
    );

    // (c) LISTA NA KROKU = PODZBIÓR. `beta` jest na agencie i wyłączona tutaj — czyli dokładnie
    //     ta umiejętność, którą cicha implementacja dokłada „bo agent ją ma".
    assert_eq!(
        looked.get("Subset").cloned().unwrap_or_default(),
        set(&[ALPHA]),
        "this step narrowed its agent down to one skill. {BETA} is on the agent and switched off \
         here, so it must not be within reach: an agent that quietly knows more than the human \
         picked is an agent whose permissions no screen describes. It reached the driver with {:?}",
        looked.get("Subset")
    );

    // (d) `"skills": "all"` W PLIKU WORKFLOW, BEZ NADPISANIA = WSZYSTKO, CO MA AGENT. To pole
    //     jest starsze od nadpisań i ma tę samą semantykę; przeczytane jako pusta lista
    //     zabierałoby umiejętności każdemu krokowi, który nigdy niczego nie zawężał — czyli
    //     KAŻDEMU krokowi zapisanemu przed tym zadaniem, bo `"all"` jest tam wartością domyślną.
    assert_eq!(
        looked.get("File all").cloned().unwrap_or_default(),
        set(&[ALPHA, BETA]),
        "this step says \"all\" in the workflow file and overrides nothing, so it takes what its \
         agent has - both of them. It reached the driver with {:?}",
        looked.get("File all")
    );

    // (e) LISTA W PLIKU WORKFLOW, BEZ NADPISANIA = PODZBIÓR. TA asercja jest jedyną, która widzi
    //     implementację czytającą wyłącznie nadpisania: dla kroku bez nadpisania taka
    //     implementacja oddaje wszystko, co ma agent, i milczy o wyborze, który człowiek zapisał
    //     w pliku workflow. Z zewnątrz wygląda to jak agent, który po prostu nie sięgnął po
    //     {BETA}.
    assert_eq!(
        looked.get("File list").cloned().unwrap_or_default(),
        set(&[ALPHA]),
        "this step narrows its agent down to one skill in the workflow file itself, without any \
         override. {BETA} is on the agent and is not on that list, so it must not be within \
         reach: a step that quietly knows more than the file says is a step whose permissions no \
         screen describes. It reached the driver with {:?}",
        looked.get("File list")
    );

    // (f) DWA ŹRÓDŁA, JEDNO ROZSTRZYGA. Nadpisanie jest RÓŻNICĄ wobec agenta, więc wygrywa
    //     z polem pliku; odwrotna kolejność znaczyłaby, że starsze pole kasuje wybór zrobiony
    //     na panelu kroku, a człowiek widzi wtedy na ekranie jedno, a agent dostaje drugie.
    assert_eq!(
        looked.get("Override wins").cloned().unwrap_or_default(),
        set(&[BETA]),
        "this step has {ALPHA} in the workflow file and {BETA} in its override. The override is \
         the difference against the agent, so it decides; the other way round the panel would \
         show one choice and the process would get the other. It reached the driver with {:?}",
        looked.get("Override wins")
    );

    // (g) NIC SPOZA WYBORU. Zasiane w bibliotece, przypisane nikomu — i to jest ta asercja,
    //     której nie przechodzi implementacja podająca agentowi zawartość `~/.loadout/skills/`.
    for (step, reachable) in &looked {
        assert!(
            !reachable.contains(GAMMA),
            "{GAMMA} sits in the library and no agent and no step ever asked for it, and step \
             \"{step}\" could reach it anyway. Handing an agent the whole shelf makes every \
             narrowing above meaningless: the human unticks a skill and the agent keeps it."
        );
    }

    Ok(())
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Zbiór umiejętności w zasięgu każdego kroku, po jego nazwie.
#[derive(Debug, Default)]
struct Seen(Mutex<BTreeMap<String, BTreeSet<String>>>);

impl Seen {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, step: &str, reachable: BTreeSet<String>) {
        self.lock().insert(step.to_owned(), reachable);
    }

    fn snapshot(&self) -> BTreeMap<String, BTreeSet<String>> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, BTreeSet<String>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Nazwy katalogów umiejętności leżących pod `<dir>` — pusto, kiedy tej półki nie ma.
///
/// Katalog bez `SKILL.md` **nie liczy się**: to jest ta sama reguła, po której poznaje się
/// umiejętność w cudzym repozytorium (`inherit::scan::skills`), i bez niej pusty katalog
/// o właściwej nazwie udawałby dojechaną umiejętność.
fn skills_under(dir: &Path) -> BTreeSet<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return BTreeSet::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

/// Wszystko, po co ten krok może naprawdę sięgnąć — obie półki, których szukają vendorzy.
///
/// Katalog pluginu przychodzi ŚCIEŻKĄ Z ARGV, nie z naszej wiedzy o tym, gdzie go położyliśmy:
/// flaga wskazująca katalog o poziom obok tego, który powstał, jest dokładnie tą cichą porażką,
/// przed którą stoi całe to zadanie.
fn within_reach(cwd: &Path, flags: &[String]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for (index, flag) in flags.iter().enumerate() {
        if flag == "--plugin-dir"
            && let Some(dir) = flags.get(index + 1)
        {
            found.extend(skills_under(&PathBuf::from(dir).join("skills")));
        }
    }
    found.extend(skills_under(&cwd.join(".agents").join("skills")));
    found.extend(skills_under(&cwd.join(".claude").join("skills")));
    found
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        seen,
        flags: Vec::new(),
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, który zagląda tam, gdzie zagląda vendor.
#[derive(Debug)]
struct Fake {
    seen: Arc<Seen>,
    /// Fragment argv przyniesiony przez warstwę wyżej — dokładnie tyle, ile wie o nim adapter
    /// (niezmiennik 23). Pusty znaczy „nie było czego nieść" i rozstrzygnął to ktoś inny.
    flags: Vec<String>,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        VENDOR
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(VENDOR.to_owned()),
        })
    }

    /// Szew, którym gotowy fragment argv dojeżdża do sterownika. `Some`, bo ten dubler UMIE go
    /// przyjąć — sterownik oddający tu `None` przy niepustym fragmencie zatrzymuje krok, i tak
    /// ma być (`Live::carrying_what_we_inherited`).
    fn inheriting(&self, flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            seen: Arc::clone(&self.seen),
            flags: flags.to_vec(),
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // Krok rozpoznajemy po treści zadania (powód przy [`STEPS`]). Zadanie, którego nie ma
        // w tablicy, ląduje pod SWOJĄ TREŚCIĄ, nie pod cudzą nazwą: asercja o nazwach kroków ma
        // wtedy paść i pokazać, czego test nie rozpoznał.
        let step = STEPS
            .iter()
            .find(|(instruction, _)| spec.prompt.starts_with(instruction))
            .map_or_else(|| spec.prompt.clone(), |(_, name)| (*name).to_owned());
        self.seen
            .record(&step, within_reach(&spec.cwd, &self.flags));

        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                (AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.clone().unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                })
                .into(),
            )
            .await;

        Ok(Box::new(Turn { events, session }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<loadout_lib::engine::supervisor::GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> loadout_lib::engine::supervisor::GroupProof {
        loadout_lib::engine::supervisor::GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(home.path().join("skills"))?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        // Żeby „własna kopia twoich plików" miała co kopiować.
        fs::write(project.path().join("notes.txt"), "written by the human")?;
        Ok(Self { home, project })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(
            self.home.path().join("agents").join(format!("{slug}.md")),
            text,
        )?;
        Ok(())
    }

    /// Kanoniczna kopia jednej umiejętności: `<dane>/skills/<nazwa>/SKILL.md`.
    fn skill(&self, name: &str) -> Result<(), Box<dyn Error>> {
        let dir = self.home.path().join("skills").join(name);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), skill_file(name))?;
        Ok(())
    }

    fn workflow(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{slug}.json"));
        fs::write(&path, text)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}
