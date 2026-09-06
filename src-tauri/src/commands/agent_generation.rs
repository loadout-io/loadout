//! G-02: wygenerowanie agenta przez WYBRANEGO vendora, w izolacji, poza rozmową Leada.
//!
//! # Co tu jest cienkie, a co grube
//!
//! Cienka jest komenda. Lifecycle należy do backendu, frontend prosi, pokazuje i anuluje.
//! Grube jest ograniczenie: proces generatora nie dostaje praw do repozytorium, do biblioteki
//! ani do usług tylko dlatego, że **tworzony** agent będzie ich potrzebował.
//!
//! # Czym naprawdę jest tu „bez narzędzi" (zmierzone 2026-09-06)
//!
//! Sprawdzone w API sterowników, zgodnie z poleceniem planu, i odpowiedź nie jest jednym
//! słowem:
//!
//! * Claude ma listę narzędzi, ale **pusta lista jest w tym drzewie ODMOWĄ**, nie wartością —
//!   `claude::tool_surface` zwraca `ToolsRefused::NothingChosen`, bo `--tools ""` u vendora
//!   znaczy „żadnych narzędzi" i z zewnątrz wygląda dokładnie jak zawieszony agent.
//! * Codex nie ma listy narzędzi w ogóle (`AgentDriver::narrows_its_tools` → `false`);
//!   to, po co sięga, wynika wyłącznie z trybu piaskownicy.
//!
//! Ograniczenie, które **działa u obu i jest egzekwowane**, a nie zadeklarowane w prompcie
//! systemowym, składa się więc z trzech rzeczy naraz: polityka tylko do odczytu, wyłączona
//! sieć i **pusty katalog roboczy**. Generator nie ma czego przeczytać, bo nie stoi w żadnym
//! repozytorium; nie ma czego zapisać, bo polityka na to nie pozwala; i nie ma dokąd wyjść.
//! To jest granica procesu, nie miękka prośba „nie edytuj".
//!
//! Czego to NIE jest: sesji bez ani jednego czasownika. Gdyby ktoś tego potrzebował, brakuje
//! do tego wartości w `RunSpec::tools`, która nie jest odmową — i to jest ta jedna rzecz,
//! której obecne API nie umie wyrazić.

use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::engine::drivers::{AgentEvent, DecodedEvent, FinishReason, Policy, RunSpec};
use crate::library::agent_generation::{Draft, Wanted, read_draft};
use crate::library::agents::Vendor;

/// Czemu nie powstał szkic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerationFailed {
    /// Vendor nie jest tu dostępny — brak CLI, brak logowania.
    NoVendor { said: String },
    /// Człowiek anulował tę jedną operację.
    Cancelled,
    /// Sufit czasu całej operacji, razem z korektą.
    OutOfTime { said: String },
    /// Odpowiedź przyszła i nie jest kontraktem — także po jedynej korekcie.
    NotADraft { said: String },
    /// Transport padł.
    Broke { said: String },
}

impl GenerationFailed {
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Self::NoVendor { said } | Self::OutOfTime { said } => said.clone(),
            Self::NotADraft { said } | Self::Broke { said } => said.clone(),
            Self::Cancelled => "You stopped this before it finished. Nothing was saved.".to_owned(),
        }
    }
}

/// Ile razy wolno poprosić o poprawienie FORMATU. Jeden raz, i tylko o format.
///
/// Braki semantyczne — nieistniejąca umiejętność, model spoza katalogu — pokazujemy człowiekowi
/// przy szkicu. Pętla samonaprawy na nich kręciłaby się w nieskończoność, bo model nie ma jak
/// dowiedzieć się rzeczy, których w jego kontekście nie ma.
const CORRECTIONS: usize = 1;

/// Co model dostaje razem z opisem roli. Krótkie i dosłowne: to jest kontrakt formatu,
/// a nie druga instrukcja o tym, jak być agentem.
fn asked_for(wanted: &Wanted) -> String {
    let runs_with = match wanted.target {
        Vendor::ClaudeCode => "claude-code",
        Vendor::Codex => "codex",
    };
    let skills = list_or_none(&wanted.available.skills);
    let connections = list_or_none(&wanted.available.connections);
    let services = list_or_none(&wanted.available.services);
    format!(
        "Write the settings for ONE agent that will run on {runs_with}. Answer with a single \
         JSON object and nothing else: no prose before it, no code fence around it.\n\n\
         Keys: name, summary, instructions, color, model, thinking, fileAccess, \
         giveUpAfterMinutes, tools, reachesTheWeb, skills, connections, serviceAccess, \
         agentMessages, vendorOptions, assumptions, because.\n\n\
         `instructions` is the agent's own standing brief, written for it, not about it. \
         `assumptions` lists what you had to guess from the description. `because` gives one \
         short line for each setting a person would question — what it is for, not how you \
         thought about it.\n\n\
         Only these skills exist here: {skills}. Only these connections: {connections}. Only \
         these services: {services}. Naming anything else does not make it available, and it \
         will be shown to the person as missing rather than saved.\n\n\
         You are writing settings. Do not do the work the described agent would do, and do not \
         change any file.\n\n\
         The person described the agent like this:\n\n{}",
        wanted.described
    )
}

fn list_or_none(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(", ")
    }
}

/// Odpala wybranego vendora i oddaje szkic.
///
/// `cancel` należy do TEJ operacji. Anulowanie generowania nie ma prawa zatrzymać workflow,
/// a Stop workflow nie ma prawa zabić generowania — dlatego to jest własny token, a nie
/// globalny stan (niezmiennik 7).
pub async fn generate(
    driver: &dyn crate::engine::drivers::AgentDriver,
    wanted: &Wanted,
    cancel: &CancellationToken,
) -> Result<Draft, GenerationFailed> {
    match driver.probe().await {
        Ok(probe) if probe.found => {}
        Ok(_) | Err(_) => {
            return Err(GenerationFailed::NoVendor {
                said: "That app is not available on this computer, so nothing could write the \
                       agent. Install or sign in to it, or use the other one."
                    .to_owned(),
            });
        }
    }
    let empty = tempfile::Builder::new()
        .prefix("loadout-generation")
        .tempdir()
        .map_err(|_| GenerationFailed::Broke {
            said: "Loadout could not make an empty folder for this, so nothing was written."
                .to_owned(),
        })?;
    let started = tokio::time::Instant::now();
    let mut asked = asked_for(wanted);
    let mut last: Option<String> = None;
    for attempt in 0..=CORRECTIONS {
        let left = wanted.deadline.saturating_sub(started.elapsed());
        if left.is_zero() {
            return Err(out_of_time(wanted));
        }
        let said = one_turn(driver, wanted, &asked, empty.path(), left, cancel).await?;
        match read_draft(wanted, said.as_bytes()) {
            Ok(draft) => return Ok(draft),
            Err(why) => {
                let said_why = why.said();
                if attempt == CORRECTIONS {
                    return Err(GenerationFailed::NotADraft { said: said_why });
                }
                /* JEDNA KOREKTA I TYLKO O FORMAT. Prośba niesie zdanie WALIDATORA, nie własne
                 * tłumaczenie: dwa miejsca z tym samym komunikatem rozjeżdżają się przy
                 * pierwszej poprawce (niezmiennik 13). */
                asked = format!(
                    "{said_why}\n\nAnswer again with the single JSON object described earlier, \
                     and nothing else.",
                );
                last = Some(said);
            }
        }
    }
    Err(GenerationFailed::NotADraft {
        said: last.map_or_else(
            || "Nothing usable came back.".to_owned(),
            |_| "The answer was still not the settings this asks for. Nothing was saved.".to_owned(),
        ),
    })
}

fn out_of_time(wanted: &Wanted) -> GenerationFailed {
    GenerationFailed::OutOfTime {
        said: format!(
            "Writing the agent took longer than {} seconds, so it was stopped. Your description \
             is still here — try again.",
            wanted.deadline.as_secs()
        ),
    }
}

/// Jedna tura generatora, w pustym katalogu i bez prawa zapisu.
async fn one_turn(
    driver: &dyn crate::engine::drivers::AgentDriver,
    wanted: &Wanted,
    asked: &str,
    empty: &std::path::Path,
    left: Duration,
    cancel: &CancellationToken,
) -> Result<String, GenerationFailed> {
    let (events, mut inbox) = mpsc::channel::<DecodedEvent>(64);
    let spec = RunSpec {
        run_id: wanted.operation,
        // PUSTY KATALOG. Generator nie stoi w żadnym repozytorium, więc nie ma czego przeczytać
        // ani czego popsuć — i to jest granica procesu, nie prośba w prompcie.
        cwd: PathBuf::from(empty),
        prompt: asked.to_owned(),
        model: None,
        system_append: None,
        // Tylko do odczytu. Zapis biblioteki jest czynnością CZŁOWIEKA, po obejrzeniu szkicu.
        policy: Policy::ReadOnly,
        // Bez sieci: pisanie konfiguracji nie wymaga świata, a wyjście na zewnątrz jest
        // poszerzeniem uprawnień, o które nikt nie prosił.
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    };
    /* ANULOWANIE SPRAWDZANE PRZED STARTEM, nie tylko w `select!`. Dwie gotowe gałęzie
     * rozstrzyga tam losowanie, więc „nacisnąłem Stop i mimo to poszło" byłoby zachowaniem
     * prawdziwym raz na kilka razy — czyli najgorszym rodzajem. */
    if cancel.is_cancelled() {
        return Err(GenerationFailed::Cancelled);
    }
    let mut handle = driver
        .start(spec, events)
        .await
        .map_err(|_| GenerationFailed::Broke {
            said: "That app did not start, so nothing wrote the agent.".to_owned(),
        })?;
    // Pompa zdarzeń musi być opróżniana, inaczej adapter zatrzyma się na pełnym kanale.
    let drain = tokio::spawn(async move {
        let mut last = String::new();
        while let Some(DecodedEvent { event, .. }) = inbox.recv().await {
            if let AgentEvent::Finished(outcome) = event {
                last = outcome.text;
            }
        }
        last
    });
    let waited = tokio::select! {
        // Stop człowieka wygrywa z wynikiem, który przyszedł w tej samej chwili: i tak
        // niczego nie zapisujemy, a odpowiedź „przecież zdążyło" nie jest tą, o którą prosił.
        biased;
        () = cancel.cancelled() => {
            let _ = handle.cancel().await;
            drain.abort();
            return Err(GenerationFailed::Cancelled);
        }
        () = tokio::time::sleep(left) => {
            let _ = handle.cancel().await;
            drain.abort();
            return Err(out_of_time(wanted));
        }
        done = handle.wait() => done,
    };
    let _ = handle.close().await;
    /* UCHWYT SCHODZI PRZED CZEKANIEM NA POMPĘ, i to nie jest porządkowanie. Nadajnik zdarzeń
     * należy do uchwytu, więc dopóki uchwyt żyje, `inbox.recv()` nie ma jak wrócić `None`
     * i czekanie na pompę nie kończy się nigdy. Zmierzone tu 2026-09-06: cały cel testowy
     * wisiał kilkanaście minut, zanim to zeszło. */
    drop(handle);
    let _ = drain.await;
    match waited {
        Ok(outcome) if matches!(outcome.reason, FinishReason::Completed) || outcome.ok => {
            Ok(outcome.text)
        }
        Ok(outcome) => Err(GenerationFailed::NotADraft {
            said: format!(
                "That app stopped before writing the agent{}.",
                match &outcome.reason {
                    FinishReason::Failed(_) => " and did not say what it wrote",
                    _ => "",
                }
            ),
        }),
        Err(_) => Err(GenerationFailed::Broke {
            said: "The connection to that app broke while it was writing the agent.".to_owned(),
        }),
    }
}
