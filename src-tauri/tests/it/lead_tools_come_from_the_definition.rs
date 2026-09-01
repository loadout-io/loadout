//! Lista narzędzi lidera przychodzi z jego definicji — jak u kroku biegu.
//!
//! # Po co to istnieje
//!
//! Rozstrzygnięcie właściciela 2026-08-30: „lidera traktujemy jak proces claude/codex, i de facto
//! orchiestrator, to ma być taki nasz główny brain tego wszystkiego, chcę mieć elastyczność".
//!
//! Do tego dnia `Agent.tools` było dla lidera **martwą kontrolką**: człowiek zawężał listę
//! w formularzu, a rozmowa dostawała cały sufit swojej polityki. Krok biegu brał ją z definicji
//! od T-63; lider nie brał jej wcale — i był przez to jedynym agentem w aplikacji, który nie
//! dostawał tego, co sam sobie ustawiłeś.
//!
//! # Decyzja, którą ten plik utrwala
//!
//! Kod nazywał tę lukę i zostawiał ją człowiekowi wprost: „czy lider z listą ponad swoim dialem
//! ma ODMÓWIĆ ROZMOWY". Odpowiedź brzmi tak, i jest to wybór spójności z biegiem — ale ma cenę
//! i cena jest tu nazwana: źle skonfigurowany lider przestaje rozmawiać.
//!
//! Cichy zestaw byłby gorszy, bo to jest ta sama wada, którą bieg nazwał już raz: agent, któremu
//! po cichu zabrano narzędzie, wygląda z zewnątrz dokładnie jak agent, który „nie umiał".
//! Dlatego trzecie kryterium pyta o TREŚĆ odmowy, nie o jej istnienie.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom, co w pozostałych
// plikach tego celu.
#![allow(clippy::expect_used)]

use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;

use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::chat::{Lead, Terminal, Threads};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens, Voice,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{LineSource, line_channel};
use loadout_lib::library::agents::{Agent, FileAccess, Tools, Vendor};

const LINES: usize = 64;

/// Sterownik-dubler, który zapamiętuje listę narzędzi, jaką dostał w `RunSpec`.
#[derive(Debug, Clone)]
struct Watching {
    seen: Arc<Mutex<Vec<Option<Vec<String>>>>>,
}

#[async_trait]
impl AgentDriver for Watching {
    fn id(&self) -> &'static str {
        "claude"
    }

    /// `true`, bo to jest fakt o vendorze, który rozstrzyga, czy lista w ogóle jedzie.
    fn narrows_its_tools(&self) -> bool {
        true
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("claude".to_owned()),
        })
    }

    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec.tools.clone());
        let (voice, _heard) = mpsc::channel(4);
        Ok(Box::new(Quiet {
            events,
            session: SessionRef {
                vendor: "claude",
                id: spec.run_id.to_string(),
            },
            voice,
        }))
    }
}

#[derive(Debug)]
struct Quiet {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    voice: Voice,
}

#[async_trait]
impl AgentHandle for Quiet {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn voice(&self) -> Option<Voice> {
        Some(self.voice.clone())
    }

    fn group(&self) -> Option<GroupId> {
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
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

/// Rozmowa z liderem o tej definicji; oddaje to, co sterownik dostał w `RunSpec::tools`.
async fn what_the_lead_was_given(
    agent: Agent,
) -> Result<(Option<Option<Vec<String>>>, Option<String>, Option<String>), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    save_agent_inner(home.path(), &agent, None).expect("the library accepts this lead");

    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver: Arc<dyn AgentDriver> = Arc::new(Watching {
        seen: Arc::clone(&seen),
    });
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));

    let lead = Lead::pointed_at(home.path(), Some(&agent.id.to_string()))
        .expect("the lead this test just saved is in the library");
    let terminal = Terminal {
        id: "terminal-1".to_owned(),
        folder: project.path().to_path_buf(),
    };

    let threads = Threads::new();
    threads.library_is(home.path().to_path_buf());
    let (sink, mut source): (_, LineSource) = line_channel(LINES);
    threads.terminal_lines_go_to(&terminal, sink);

    let refusal = threads
        .say_in(&drivers, &lead, &terminal, "what can you do?")
        .await
        .err()
        .map(|error| error.to_string());

    let given = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .first()
        .cloned();

    /* ZDANIE Z EKRANU, nie z wartości zwróconej: kryterium o komunikacie asertuje go tam, gdzie
     * widzi go CZŁOWIEK (niezmiennik 29). Szukamy wśród wierszy, bo strumień niesie też turę. */
    let mut said_on_screen = None;
    while let Some(line) = source.try_next() {
        if let loadout_lib::engine::line::Line::Problem { text, .. } = line {
            said_on_screen = Some(text);
        }
    }
    Ok((given, refusal, said_on_screen))
}

/// Lider z tą listą i tym dialem.
fn lead_with(tools: Tools, access: FileAccess) -> Agent {
    let mut agent = Agent::example();
    "Scout".clone_into(&mut agent.name);
    agent.runs_with = Vendor::ClaudeCode;
    agent.file_access = access;
    agent.tools = tools;
    agent
}

#[tokio::test]
async fn a_narrowed_list_reaches_the_conversation() -> Result<(), Box<dyn Error>> {
    let (given, refusal, _) = what_the_lead_was_given(lead_with(
        Tools::Only(vec!["Read".to_owned(), "Grep".to_owned()]),
        FileAccess::LookOnly,
    ))
    .await?;

    assert_eq!(
        refusal, None,
        "this list fits under Look only, so nothing refuses"
    );
    assert_eq!(
        given,
        Some(Some(vec!["Read".to_owned(), "Grep".to_owned()])),
        "the list the person set in the form has to reach the session. Until 2026-08-30 it did \
         not, and the control in Agents was dead for every lead — the one agent in this app that \
         did not get what you configured"
    );
    Ok(())
}

#[tokio::test]
async fn everything_still_means_the_ceiling_of_the_dial() -> Result<(), Box<dyn Error>> {
    let (given, refusal, _) =
        what_the_lead_was_given(lead_with(Tools::Everything, FileAccess::LookOnly)).await?;

    assert_eq!(refusal, None);
    assert_eq!(
        given,
        Some(None),
        "\"everything\" means \"do not narrow\", which is byte for byte the argv this lead got \
         before this change. Passing the ceiling through our own filter would give the same \
         answer by a longer road, and stop giving it the first time the filter changes"
    );
    Ok(())
}

/// 2026-08-30 — TEN PRZYPADEK ZMIENIŁ SIĘ PO POMIARZE NA PRAWDZIWEJ BIBLIOTECE.
///
/// Pierwsza wersja wymagała, żeby lista ponad dialem ODMÓWIŁA ROZMOWY — dla spójności z krokiem
/// biegu. Sprawdzone na bibliotece właściciela: **18 z 29 agentów** ma taką listę (sami
/// `claude-code`; Codex nie zawęża w ogóle). Każdy z nich przestałby po tamtej wersji rozmawiać.
///
/// Spójność była pozorna: krok biegu ma furtkę, której rozmowa nie ma — `AgentStep::overrides`
/// podnosi dial na kafelku, więc ten sam agent biega poprawnie. Lider jest samą definicją.
///
/// Asymetria ma powód, nie tylko pomiar: bieg startuje sześciu agentów bez nadzoru i płaci od
/// pierwszej sekundy, a rozmowa to jedna tura z człowiekiem przy ekranie. Odebranie mu rozmowy
/// zabiera zarazem jedyne miejsce, w którym mógłby zapytać dlaczego.
///
/// Zostaje więc to, czego ten przypadek naprawdę broni: **nie po cichu**. Zdanie ma dojść na
/// ekran i nazwać oba pola do poprawienia.
#[tokio::test]
async fn a_list_above_the_dial_says_so_without_taking_the_conversation_away()
-> Result<(), Box<dyn Error>> {
    let (given, refusal, said_on_screen) = what_the_lead_was_given(lead_with(
        Tools::Only(vec!["Read".to_owned(), "Write".to_owned()]),
        FileAccess::LookOnly,
    ))
    .await?;

    assert_eq!(
        refusal, None,
        "the conversation still happens. Taking it away would take away the one place this \
         person could ask why — and it would take it away from 18 of the 29 agents in a real \
         library, measured 2026-08-30"
    );
    assert_eq!(
        given,
        Some(None),
        "and the lead works with the ceiling of its dial — byte for byte what it had before this \
         change. Quietly handing it the narrowed list instead would be the worst of both: an \
         agent stripped of a tool looks exactly like an agent that could not do the job"
    );

    let said = said_on_screen.expect("a list above the dial has to say so on screen");
    assert!(
        said.contains("Write"),
        "the refusal names the TOOL, because that is one of the two fields the person edits: \
         it said {said:?}"
    );
    /* SŁOWEM PRODUKTU, nie moim: dial nazywa się na ekranie „look only" i tak samo brzmi
     * w odmowie (`claude::on_screen`). Kryterium wpisujące tu własną wielką literę sądziłoby
     * zdanie, którego aplikacja nie produkuje. */
    assert!(
        said.contains("look only"),
        "and it names the DIAL, because that is the other one. Either half alone leaves half the \
         people at an instruction that cannot work: {said:?}"
    );
    assert!(
        said.contains("Scout"),
        "and it names the AGENT, because a person with six leads has to know which one to open: \
         {said:?}"
    );
    Ok(())
}
