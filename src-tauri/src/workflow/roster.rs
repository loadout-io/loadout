//! Czy ten workflow **da się uruchomić z tą biblioteką** — sądzone przy budowaniu, nie przy Starcie.
//!
//! # Po co to istnieje
//!
//! 2026-08-22 — TRZY ODMOWY POD RZĄD NA JEDNYM BIEGU, wszystkie zgłoszone przez właściciela
//! i wszystkie możliwe do policzenia, zanim ruszył pierwszy proces:
//!
//! * `figma-extractor is set to look only and asks for mcp__figma__…`
//! * `Connection figma does not exist in the Loadout library.`
//! * `design-qa is set to look only and asks for mcp__playwright, Bash, Write, Edit.`
//!
//! Każda z nich padała **po** naciśnięciu Start, na kroku, którego człowiek w tej chwili nie
//! oglądał, i po jednej naraz: naprawiasz pierwszą, klikasz Start, dostajesz drugą. `check.rs`
//! sądzi w tym czasie sam plik — kształt grafu, strzałki, koła — i o bibliotece nie wie nic,
//! więc płótno malowało „Ready to run" nad workflow, który nie miał prawa ruszyć.
//!
//! Ten moduł zamyka tę lukę: bierze plik I bibliotekę, i mówi WSZYSTKO naraz, na kafelkach,
//! w chwili budowania.
//!
//! # Dlaczego osobny moduł, a nie kolejna reguła w `check`
//!
//! `check::notes` jest czystą funkcją nad plikiem i taką ma zostać — woła ją zapis, a zapis nie
//! ma prawa zależeć od tego, co akurat leży w `~/.loadout/agents`. Plik, który zapisuje się dziś,
//! ma się zapisać także jutro, kiedy ktoś skasuje agenta. Dlatego to jest **druga** lista uwag,
//! sklejana z tamtą dopiero w komendzie okna (`commands::workflows::check_workflow_inner`).
//!
//! # Reguły są tu POŻYCZONE, nigdy przepisane
//!
//! Sufit narzędzi liczy `engine::drivers::claude::beyond`, a zdanie odmowy składa
//! `claude::no_such_tools` — te same dwie funkcje, których używa Start. Druga kopia którejkolwiek
//! z nich dałaby ekran mówiący „gotowe" nad workflow, który Start odrzuca, i to przy pierwszej
//! zmianie sufitu. To jest dokładnie ta klasa wady, dla której ten moduł powstał.

use serde::Serialize;

use crate::connections::Connection;
use crate::engine::drivers::claude::{ToolsRefused, beyond, no_such_tools};
use crate::library::agents::{Agent, FileAccess, Tools, policy_of, resolve};

use super::WorkflowFile;
use super::check::{Level, Note};

/// Naprawa, którą Loadout umie wykonać sam.
///
/// **Wariant istnieje wyłącznie wtedy, gdy naprawa jest jednoznaczna.** Nie ma tu wariantu
/// „coś z tym zrób": auto-fix, który zgaduje, jest gorszy od zdania, bo zmienia konfigurację
/// w sposób, którego człowiek nie wybrał i nie zobaczy. Tam, gdzie naprawa jest decyzją
/// (wyłączone połączenie, brakująca umiejętność), uwaga zostaje samym zdaniem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Fix {
    /// Podnieś dial TEGO KROKU do najniższego, który pokrywa wybrane narzędzia.
    ///
    /// Kroku, nie agenta: dial na kroku jest nadpisaniem, więc naprawa dotyczy jednego kafelka
    /// i nie rusza tej samej roli w pięciu innych workflow. Najniższy, bo naprawa ma zdjąć
    /// odmowę, a nie przy okazji dać agentowi wszystko.
    WidenFileAccess {
        step: String,
        to: FileAccess,
        /// Dial, na którym stoi teraz — zdanie na przycisku mówi „z czego na co".
        from: FileAccess,
    },
    /// Zdejmij te narzędzia z listy AGENTA.
    ///
    /// Agenta, nie kroku: `mcp__…` nie jest czasownikiem plikowym i żaden dial go nie obejmuje,
    /// więc podniesienie dialu tego nie naprawia. Narzędzia serwera dojeżdżają do kroku przez
    /// Connections i lista `--tools` nie ma na nie wpływu.
    DropTools {
        agent: String,
        agent_name: String,
        tools: Vec<String>,
    },
}

/// Uwagi, których nie da się policzyć z samego pliku.
///
/// `agents` to biblioteka, `connections` to zatwierdzone połączenia, `skills` to nazwy katalogów
/// umiejętności. Wszystko trzy przychodzi argumentem, bo ta funkcja ma dać się osądzić bez
/// dysku — tak samo jak reszta walidatora.
#[must_use]
pub fn check_the_roster(
    file: &WorkflowFile,
    agents: &[Agent],
    connections: &[Connection],
    skills: &[String],
) -> Vec<Note> {
    let mut notes = Vec::new();

    /* PUSTA BIBLIOTEKA NIE JEST LISTĄ ZARZUTÓW. Bez ani jednego zapisanego agenta zdanie „tej
     * nazwy nie ma w bibliotece" jest prawdziwe o KAŻDYM kroku i nie mówi człowiekowi nic, czego
     * nie powie mu jedno zdanie: nie masz jeszcze agentów. Ten stan ma już swoją odmowę przy
     * Starcie (`RunError::NoAgentsSaved`) i swój pusty ekran w sekcji Agenci. */
    if agents.is_empty() {
        return notes;
    }

    for step in &file.steps {
        let super::Step::Agent(one) = step else {
            continue;
        };
        // Krok bez wybranego agenta ma już swoją uwagę w `check` i drugie zdanie o tym samym
        // stanie byłoby dwoma zgłoszeniami o jednej przyczynie.
        if one.agent.trim().is_empty() {
            continue;
        }
        let Some(saved) = agents
            .iter()
            .find(|agent| agent.id.to_string() == one.agent)
        else {
            notes.push(note(
                &one.id,
                format!(
                    "\"{}\" names an agent that is no longer in your library. Pick one for it in \
                     the step panel, or save that agent again in Agents.",
                    one.name
                ),
                None,
            ));
            continue;
        };

        /* Nadpisania kroku są w pliku SUROWĄ MAPĄ (`AgentStep::overrides`), a `resolve` przyjmuje
         * typ — więc przechodzą tędy przez serde, dokładnie tą samą drogą, którą idą przy Starcie
         * (`commands::run`). Patch, którego nie da się złożyć, jest stanem PLIKU i mówi o nim
         * `workflow::file`; tutaj milczymy, zamiast zgadywać czyjś błąd. */
        let Ok(overrides) =
            serde_json::from_value(serde_json::Value::Object(one.overrides.clone()))
        else {
            continue;
        };
        let Ok(effective) = resolve(saved, &overrides) else {
            continue;
        };
        let effective = effective.agent;

        tools_fit_the_dial(&one.id, saved, &effective, &mut notes);
        named_things_exist(&one.id, &effective, connections, skills, &mut notes);
    }

    notes
}

/// Uwaga wagi problemu — te blokują Run, a każda z tych przyczyn naprawdę zatrzymuje Start.
fn note(step_id: &str, message: String, fix: Option<Fix>) -> Note {
    Note {
        level: Level::Problem,
        step_id: Some(step_id.to_owned()),
        message,
        fix: fix.map(Box::new),
    }
}

/// Lista narzędzi kontra dial — dokładnie to pytanie, które zadaje sterownik przy składaniu argv.
fn tools_fit_the_dial(step_id: &str, saved: &Agent, effective: &Agent, notes: &mut Vec<Note>) {
    let Tools::Only(wanted) = &effective.tools else {
        // „Wszystko" znaczy „weź sufit swojego dialu" i nie da się tego przekroczyć.
        return;
    };
    if wanted.is_empty() {
        notes.push(note(
            step_id,
            no_such_tools(&effective.name, &ToolsRefused::NothingChosen),
            None,
        ));
        return;
    }

    let policy = policy_of(effective.file_access);
    let above = beyond(policy, wanted);
    if above.is_empty() {
        return;
    }

    // ZDANIE JEST TO SAMO, KTÓRE PADNIE PRZY STARCIE. Człowiek, który zobaczy je najpierw na
    // kafelku, a potem w odmowie biegu, ma widzieć jedną usterkę, nie dwie.
    let message = no_such_tools(
        &effective.name,
        &ToolsRefused::AbovePolicy {
            policy,
            tools: above.clone(),
        },
    );

    // Narzędzie serwera nie jest czasownikiem plikowym: żaden dial go nie obejmuje, więc jedyną
    // naprawą jest zdjęcie go z listy. Rozdział idzie po nazwie, bo `mcp__<serwer>__<narzędzie>`
    // jest jedynym kształtem, jaki vendorzy tu nadają [`engine::stream`].
    let from_a_server: Vec<String> = above
        .iter()
        .filter(|name| name.starts_with("mcp__"))
        .cloned()
        .collect();
    if !from_a_server.is_empty() {
        notes.push(note(
            step_id,
            format!(
                "{message} Tools from a connection are not picked here — the connection brings \
                 them.",
            ),
            Some(Fix::DropTools {
                agent: saved.id.to_string(),
                agent_name: saved.name.clone(),
                tools: from_a_server,
            }),
        ));
        return;
    }

    notes.push(note(
        step_id,
        message,
        lowest_dial_that_covers(wanted).map(|to| Fix::WidenFileAccess {
            step: step_id.to_owned(),
            to,
            from: effective.file_access,
        }),
    ));
}

/// Najniższy dial, przy którym cała ta lista mieści się w suficie.
///
/// Kolejność jest kolejnością dialu na ekranie, więc naprawa nigdy nie przeskakuje pozycji,
/// której człowiekowi by wystarczyło.
fn lowest_dial_that_covers(wanted: &[String]) -> Option<FileAccess> {
    [FileAccess::AskFirst, FileAccess::WorkFreely]
        .into_iter()
        .find(|dial| beyond(policy_of(*dial), wanted).is_empty())
}

/// Czy wszystko, co ten krok nazywa, naprawdę leży w bibliotece i jest włączone.
fn named_things_exist(
    step_id: &str,
    effective: &Agent,
    connections: &[Connection],
    skills: &[String],
    notes: &mut Vec<Note>,
) {
    for wanted in &effective.connections {
        match connections
            .iter()
            .find(|one| one.id == *wanted || one.name == *wanted)
        {
            None => notes.push(note(
                step_id,
                format!(
                    "\"{}\" uses the connection \"{wanted}\", and your library has nothing saved \
                     under that name. Import it from the project, or take it off the agent.",
                    effective.name
                ),
                None,
            )),
            Some(found) if !found.enabled => notes.push(note(
                step_id,
                format!(
                    "\"{}\" uses the connection \"{}\", and it is turned off. Turn it on when you \
                     import the project setup, or take it off the agent.",
                    effective.name, found.name
                ),
                None,
            )),
            Some(_) => {}
        }
    }

    for wanted in &effective.skills {
        if !skills.iter().any(|have| have == wanted) {
            notes.push(note(
                step_id,
                format!(
                    "\"{}\" uses the skill \"{wanted}\", and your library has nothing saved under \
                     that name. Save it in Skills, or take it off the agent.",
                    effective.name
                ),
                None,
            ));
        }
    }
}
