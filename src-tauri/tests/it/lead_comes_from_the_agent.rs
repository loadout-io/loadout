//! Lider bierze vendora, model, politykę i instrukcje z **zapisanej definicji agenta**.
//!
//! # Po co to istnieje
//!
//! Do dziś lider był zaszyty: `ipc::AppState::chat_driver` oddawał `Vendor::ClaudeCode` na sztywno,
//! a `RunSpec.model` przy starcie rozmowy był `None`. Własna dokumentacja tej funkcji zapowiadała
//! ten dzień wprost — „w dniu, w którym orchestrator stanie się konfigurowalny, ta funkcja zniknie
//! na rzecz jego zapisanej definicji".
//!
//! Cicha porażka, przed którą stoi ten plik: lider, który odpowiada innym modelem niż wybrany,
//! wygląda **dokładnie** jak lider, który się myli. Nie ma żadnego sygnału, po którym człowiek
//! mógłby to odróżnić, a jedyną rzeczą, która się zmieniła, był jego własny klik.
//!
//! # Słaba wersja tych kryteriów
//!
//! `assert_eq!(spec.model, Some("gpt-5-codex"))` i nic więcej. Przechodzi dla implementacji, która
//! czyta definicję, bierze z niej `model` i **dalej startuje Claude'em dla każdego vendora** —
//! czyli dla wyboru, który wygląda na działający i nie działa. Rozstrzyga to fabryka oddająca INNY
//! dubler na vendora: sesja albo stanęła u Codeksa, albo u Claude'a, i widać to co do sztuki.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `chat_never_starts_a_run` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::chat::{ChatError, Lead, Threads};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Policy, Probe, RunSpec, SessionRef, Tokens, Voice,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::{Agent, FileAccess, Vendor};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Ile miejsca w strumieniu linii. Z zapasem — mierzymy specyfikację sesji, nie przepustowość.
const LINES: usize = 32;

/// Model wpisany w definicję. Rozpoznawalny i **nie** domyślny u żadnego vendora: wartość, którą
/// vendor ma sam z siebie, przeszłaby także dla implementacji, która pola `model` nie czyta.
const MODEL: &str = "gpt-5-codex";

/// Instrukcje z definicji. Zdanie, którego nie ma w `BRIEF`, bo inaczej „instrukcje dojechały"
/// byłoby nieodróżnialne od „dojechał sam brief".
const INSTRUCTIONS: &str = "You look after the repository in this folder and you are blunt.";

/// Co dubler jednego vendora zapamiętał.
#[derive(Debug, Default)]
struct Watch {
    /// Specyfikacja KAŻDEGO uruchomienia. Sama długość tej listy odpowiada na pytanie, którego
    /// asercja o `model` nie zadaje: czy sesja stanęła u TEGO vendora.
    started: Mutex<Vec<RunSpec>>,
}

impl Watch {
    fn started(&self) -> Vec<RunSpec> {
        self.started
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Specyfikacja pierwszego (i w tych kryteriach jedynego) uruchomienia.
    fn first(&self) -> Option<RunSpec> {
        self.started().into_iter().next()
    }
}

#[derive(Debug)]
struct Fake {
    /// Etykieta vendora — ta sama, która ląduje w [`SessionRef::vendor`].
    id: &'static str,
    watch: Arc<Watch>,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        self.id
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(self.id.to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let session = SessionRef {
            vendor: self.id,
            id: spec.run_id.to_string(),
        };
        self.watch
            .started
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec);
        /* Odbiornik głosu żyje tak długo, jak sesja: porzucony razem ze `start` zamykałby kanał,
         * a wtedy każda następna tura odbijałaby się o „stopped listening" i mierzylibyśmy własne
         * sprzątanie. Tury same tutaj nikogo nie interesują — o nie pyta AC-2. */
        let (voice, mut heard) = mpsc::channel(4);
        tokio::spawn(async move { while heard.recv().await.is_some() {} });
        Ok(Box::new(Turn {
            events,
            session,
            voice,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    voice: Voice,
}

#[async_trait]
impl AgentHandle for Turn {
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

/// Fabryka sterowników, która oddaje **inny** dubler na vendora.
///
/// To jest cała siła punktu (a). Jeden dubler dla obu vendorów odpowiadałby wyłącznie na pytanie
/// „czy sesja stanęła", a nie na „czy stanęła u tego, kogo wskazuje definicja" — a między tymi
/// dwoma zdaniami leży dokładnie ten defekt, który to zadanie zamyka.
fn two_vendors() -> (Drivers, Arc<Watch>, Arc<Watch>) {
    let claude = Arc::new(Watch::default());
    let codex = Arc::new(Watch::default());
    let claude_driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        id: "claude",
        watch: Arc::clone(&claude),
    });
    let codex_driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        id: "codex",
        watch: Arc::clone(&codex),
    });
    let drivers: Drivers = Arc::new(move |vendor| match vendor {
        Vendor::ClaudeCode => Arc::clone(&claude_driver),
        Vendor::Codex => Arc::clone(&codex_driver),
    });
    (drivers, claude, codex)
}

/// Definicja agenta, jaką człowiek zapisał w bibliotece.
///
/// `Agent::example()` jako baza, bo „jak wygląda zapisany agent" ma w tym repo jedną odpowiedź
/// (`library::agents`), a ręcznie wypisane piętnaście pól byłoby drugą — i tą, która przestanie
/// się deserializować przy pierwszym nowym kluczu.
fn definition(id: u128, name: &str, vendor: Vendor, access: FileAccess) -> Agent {
    Agent {
        id: Uuid::from_u128(id),
        name: name.to_owned(),
        runs_with: vendor,
        model: MODEL.to_owned(),
        file_access: access,
        instructions: INSTRUCTIONS.to_owned(),
        ..Agent::example()
    }
}

/// Zapisuje definicję w bibliotece i oddaje jej identyfikator — dokładnie ten napis, którym okno
/// wskazuje lidera.
///
/// Przez `save_agent_inner`, nie przez własny zapis pliku: agent zapisany inną drogą niż
/// produkcyjna sprawdzałby czytnik na bajtach, których produkcja nigdy nie wyprodukuje.
fn saved(library: &Path, agent: &Agent) -> String {
    save_agent_inner(library, agent).expect("the library has to take a saved agent");
    agent.id.to_string()
}

/// Jedno zdanie powiedziane liderowi w tym zakresie.
///
/// Strumień zakładamy tak, jak zakłada go okno (`open_chat` → `lines_go_to`), bo wątek bez kanału
/// jest wątkiem, którego wierszy nikt nie odbiera — a to jest inny stan niż ten, o który pytamy.
async fn one_sentence(drivers: &Drivers, lead: &Lead, cwd: PathBuf) -> Threads {
    let (sink, _source) = line_channel(LINES);
    let mut threads = Threads::new();
    threads.lines_go_to(cwd.clone(), sink);
    threads
        .say(drivers, lead, cwd, "what should the checker look at?")
        .await
        .expect("the first sentence to a pointed-at lead has to open a thread");
    threads
}

#[tokio::test]
async fn the_vendor_the_model_and_the_brief_come_from_the_definition() -> Result<(), Box<dyn Error>>
{
    let library = tempfile::tempdir()?;
    let agent = definition(1, "Codex Lead", Vendor::Codex, FileAccess::LookOnly);
    let who = saved(library.path(), &agent);

    let (drivers, claude, codex) = two_vendors();
    let lead = Lead::pointed_at(library.path(), Some(&who))
        .map_err(|refusal| refusal.to_string())
        .expect("the agent was just saved, so the pointed-at lead has to resolve");

    let _threads = one_sentence(&drivers, &lead, std::env::temp_dir()).await;

    // ── (a) VENDOR Z DEFINICJI, NIE ZASZYTY ─────────────────────────────────────────────────
    //
    // Ta para asercji jest jedyną, której nie przechodzi implementacja czytająca definicję
    // i startująca Claude'em „na wszelki wypadek". Zaszyty vendor nie znika, kiedy pojawia się
    // odczyt definicji — zostaje jako gałąź domyślna, a gałąź domyślna jest tym, czego
    // konfiguracją nie da się wyłączyć.
    assert_eq!(
        codex.started().len(),
        1,
        "the lead points at an agent whose definition says `codex`, so the session has to stand \
         at the Codex driver. It started {} Codex session(s) and {} Claude one(s).",
        codex.started().len(),
        claude.started().len()
    );
    assert!(
        claude.started().is_empty(),
        "a Claude session stood for a lead defined as `codex`. That is the hard-wired vendor \
         surviving as a default branch: it looks like a working choice and it is not, and the \
         person has no signal to tell it apart from a lead that is simply wrong."
    );

    let spec = codex
        .first()
        .expect("the Codex driver recorded the session it started");

    // ── (b) MODEL Z DEFINICJI (DZIŚ ZAWSZE `None`) ──────────────────────────────────────────
    assert_eq!(
        spec.model.as_deref(),
        Some(MODEL),
        "the model from the saved definition has to reach RunSpec.model. `None` means \"whatever \
         the vendor has by default\", so a lead configured for one model answers with another and \
         nothing on screen says so."
    );

    // ── (d) INSTRUKCJE RAZEM Z BRIEFEM, NIE ZAMIAST NIEGO ───────────────────────────────────
    let brief = spec
        .system_append
        .as_deref()
        .expect("the lead's session has to carry a system prompt");
    assert!(
        brief.contains(INSTRUCTIONS),
        "the agent's own instructions never reached the system prompt, so the lead is an agent \
         nobody configured. It said: {brief}"
    );
    assert!(
        brief.contains("/run"),
        "the instructions replaced the brief instead of joining it. A lead without the sentence \
         naming what DOES start work promises \"already starting it\" and leaves the person \
         waiting for something that never comes. It said: {brief}"
    );
    Ok(())
}

#[tokio::test]
async fn the_dial_comes_from_the_table_the_run_itself_uses() -> Result<(), Box<dyn Error>> {
    // ── (c) JEDNA TABELA `FileAccess` -> `Policy`, TA Z BIEGU ───────────────────────────────
    //
    // Dwa punkty, nie jeden, i to jest treść tego przypadku: implementacja z własną, drugą kopią
    // tabeli najczęściej myli się dokładnie na jednym końcu dialu — a jeden zgodny punkt
    // przechodzi też dla stałej wpisanej na sztywno.
    let library = tempfile::tempdir()?;
    let (drivers, _claude, codex) = two_vendors();

    for (access, expected) in [
        (FileAccess::LookOnly, Policy::ReadOnly),
        (FileAccess::WorkFreely, Policy::Unrestricted),
    ] {
        let library = library.path().join(format!("{access:?}"));
        let agent = definition(2, "Dial", Vendor::Codex, access);
        let who = saved(&library, &agent);
        let lead = Lead::pointed_at(&library, Some(&who))
            .map_err(|refusal| refusal.to_string())
            .expect("the agent was just saved, so the pointed-at lead has to resolve");

        assert_eq!(
            lead.policy(),
            expected,
            "an agent saved as {access:?} has to start the conversation with {expected:?}. That \
             answer lives in ONE table — the one the run reads (`commands::run::policy_of`) — and \
             a second copy of it is how secret scanning quietly died in the source repo \
             (invariant 23)."
        );

        let _threads = one_sentence(&drivers, &lead, std::env::temp_dir()).await;
        let spec = codex
            .started()
            .pop()
            .expect("the Codex driver recorded the session it started");
        assert_eq!(
            spec.policy, expected,
            "the dial read out of the definition never reached the session: RunSpec.policy was \
             {:?} for an agent saved as {access:?}. A lead told `look only` that can write is a \
             lead the person did not agree to.",
            spec.policy
        );
    }
    Ok(())
}

#[tokio::test]
async fn nobody_pointed_at_is_a_refusal_that_names_the_next_move() -> Result<(), Box<dyn Error>> {
    // ── (e) KONTROLA: BRAK LIDERA TO ODMOWA, NIE CICHY POWRÓT DO ZASZYTEGO CLAUDE'A ─────────
    //
    // Bez tego punktu wszystko wyżej przechodzi także dla implementacji, która przy braku wyboru
    // wraca do dzisiejszego zachowania. Taki powrót jest gorszy niż odmowa: rozmowa idzie, płaci
    // i odpowiada — tylko nie ten agent, którego człowiek wybrał.
    let library = tempfile::tempdir()?;
    let agent = definition(3, "Somebody", Vendor::ClaudeCode, FileAccess::AskFirst);
    let who = saved(library.path(), &agent);

    let refusal = Lead::pointed_at(library.path(), None)
        .err()
        .map(|refusal| refusal.to_string())
        .expect(
            "with nobody pointed at, resolving the lead has to REFUSE. Falling back to a \
             hard-wired vendor is the defect this criterion exists to end.",
        );
    let said = refusal.to_lowercase();
    assert!(
        said.contains("lead"),
        "the refusal has to say WHAT is missing, in the word the person reads on screen. It said: \
         {refusal}"
    );
    assert!(
        ["pick", "choose", "save"]
            .iter()
            .any(|verb| said.contains(verb)),
        "a refusal that does not name the next move leaves the person exactly where they were \
         (DESIGN §8). It said: {refusal}"
    );

    // Kontrola dodatnia do powyższego: gdyby ta zapora odmawiała WSZYSTKIEMU, oba zdania wyżej
    // byłyby zielone dla lidera, którego nigdy nie da się wskazać — czyli dla rozmowy, która
    // odmawia zawsze. Wskazany i zapisany agent musi przechodzić.
    assert!(
        Lead::pointed_at(library.path(), Some(&who)).is_ok(),
        "an agent that IS saved and IS pointed at has to resolve. Without this line the two \
         assertions above pass for a lead nobody can ever choose."
    );

    // I odwrotnie: wskazanie na kogoś, kogo w bibliotece nie ma, to inna czynność naprawcza niż
    // brak wskazania — pierwszą naprawia wybranie kogoś innego, drugą wybranie kogokolwiek.
    assert!(
        Lead::pointed_at(library.path(), Some("00000000-0000-0000-0000-0000000000ff")).is_err(),
        "an id that no saved agent carries has to be refused too, not resolved to whoever is \
         first in the folder."
    );
    Ok(())
}

/// Nieczytelny plik w bibliotece: odmowa, która nazywa TEN plik.
///
/// Żadne kryterium tego nie wymaga i to jest cały powód, dla którego ten przypadek istnieje.
/// [`ChatError::CouldNotReadTheLibrary`] jest osiągalne prawdziwą drogą — `list_agents_inner`
/// przewraca całą listę na pierwszym pliku, którego czytnik nie rozumie (T-11), a nie pomija go
/// po cichu — więc wariant bez ani jednego wołającego w teście jest wariantem, którego
/// przecelowanie zauważy dopiero człowiek z rozmową, która nie startuje.
///
/// Plik piszemy BAJTAMI, nie przez `save_agent_inner`: produkcyjny zapis nigdy nie wyprodukuje
/// pliku, którego produkcyjny czytnik nie umie przeczytać, a mierzymy dokładnie taki — ten, który
/// powstaje z ręcznej edycji (pliki są prawdą, niezmiennik 4, więc człowiek je otwiera).
#[tokio::test]
async fn a_file_the_reader_cannot_parse_is_named_not_swallowed() -> Result<(), Box<dyn Error>> {
    let library = tempfile::tempdir()?;
    let agent = definition(4, "Readable", Vendor::ClaudeCode, FileAccess::AskFirst);
    let landed = save_agent_inner(library.path(), &agent)?;
    let who = agent.id.to_string();

    // Kontrola dodatnia PRZED zepsuciem czegokolwiek: dopóki w katalogu leży sam zapisany agent,
    // wskazanie na niego przechodzi. Bez tej linii asercja niżej byłaby zielona także dla
    // fikstury, której nie da się przeczytać nigdy — czyli mierzyłaby siebie, nie ten plik.
    assert!(
        Lead::pointed_at(library.path(), Some(&who)).is_ok(),
        "the agent was just saved, so pointing at it has to resolve while the library is still \
         readable."
    );

    /* Obok niego, w TYM SAMYM katalogu — ścieżka z `save_agent_inner`, nie zgadywana z reguły
     * nazwy pliku (ta reguła mieszka w `write_agent_file` i nie ma prawa zostać przepisana tu).
     * Treść: pierwszy wiersz myślników skasowany, czyli najczęstsza ręczna pomyłka. */
    let broken = landed
        .parent()
        .expect("a saved agent file lies inside the library folder")
        .join("hand-edited.md");
    std::fs::write(
        &broken,
        "runs_with: codex\nno dashes up top, so this is not an agent definition\n",
    )?;

    let refusal = Lead::pointed_at(library.path(), Some(&who)).expect_err(
        "a library holding a file the reader cannot parse has to REFUSE. Resolving the lead \
             anyway would answer out of a library that is half read, and the half that was \
             dropped is the half the person just edited.",
    );

    assert!(
        matches!(&refusal, ChatError::CouldNotReadTheLibrary(_)),
        "an unparseable file has to come back as the library-read refusal, never as `no such \
         lead`: the repairs differ — fix THAT file, versus pick somebody else — and one sentence \
         for two states leaves half the people following an instruction that cannot work. It \
         said: {refusal}"
    );
    assert!(
        refusal.to_string().contains("hand-edited.md"),
        "the refusal has to name the file, because \"fix that file\" is only doable when the \
         person can see which one (T4 §10). It said: {refusal}"
    );
    Ok(())
}
