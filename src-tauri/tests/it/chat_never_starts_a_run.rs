//! Rozmowa z agentem wiodącym: jedna sesja na wiele tur, i **żadnej drogi do uruchomienia biegu**.
//!
//! # Po co to istnieje
//!
//! Rozstrzygnięcie właściciela 2026-08-19, dwa zdania i oba są tu sądzone. Najpierw „ten czat
//! nadrzędny powinien być jak z orchiestratorem, czyli sobie piszemy/zmieniamy coś itp, a sztywne
//! flow dopiero po komendzie" — czyli rozmowa ma istnieć. Potem, na pytanie wprost, czy rozmowa
//! może sama odpalać workflow: **„nie, tylko komendy determinują akcje workflow"**.
//!
//! Poprzedzała to wersja, którą właściciel odrzucił po zobaczeniu skutku: proza bez ukośnika
//! po cichu startowała wybrany workflow („jak piszę bez komendy… to się na nowo całe workflow
//! odpala"). Dlatego „nie uruchamia biegu" nie jest tu prośbą w promptcie systemowym — jest
//! własnością struktury i ostatni test w tym pliku pyta o nią wprost.
//!
//! # Słaba wersja tych kryteriów
//!
//! `assert!(chat.say(...).await.is_ok())`. Przechodzi dla implementacji, która na każde zdanie
//! odpala NOWY proces (czyli płaci zimny start i gubi całą rozmowę), i przechodzi dla takiej,
//! która przy okazji uruchamia bieg. Rozstrzygają: licznik `start` u dublera, wiersz `Told`
//! w strumieniu i skan zależności modułu.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `ipc_read_paths` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::chat::{BRIEF, Chat, LEAD};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, ToAgent, Tokens, Voice,
};
use loadout_lib::engine::line::LineKind;
use loadout_lib::ipc::{LineSource, line_channel};
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Pojemność strumienia linii. Z zapasem — mierzymy obecność wierszy, nie przepustowość.
const LINES: usize = 32;

/// Co dubler zapamiętał: prompty, z którymi go uruchomiono, i tury, które dostał głosem.
#[derive(Debug, Default)]
struct Watch {
    /// Po jednym wpisie na KAŻDE uruchomienie procesu. Długość tej listy jest treścią
    /// pierwszego kryterium: rozmowa ma być jedną sesją, nie procesem na zdanie.
    starts: Mutex<Vec<String>>,
    /// Tury, które dojechały do sesji GŁOSEM — czyli wszystko po pierwszym zdaniu.
    ///
    /// Bez tego kryterium mówiłoby tylko „nie było drugiego startu", a to przechodzi także dla
    /// implementacji, która drugie zdanie po cichu WYRZUCA: jeden proces, zero tur, zero śladu.
    turns: Mutex<Vec<String>>,
}

impl Watch {
    fn starts(&self) -> Vec<String> {
        self.starts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn turns(&self) -> Vec<String> {
        self.turns
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[derive(Debug)]
struct Fake {
    watch: Arc<Watch>,
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

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.watch
            .starts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec.prompt.clone());

        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        /* Dubler ODPOWIADA prozą, bo bez tego nie da się sprawdzić, czy odpowiedź modelu wraca
         * do strumienia — a to jest połowa tego, czym rozmowa jest dla człowieka. */
        let _ = events
            .send(
                (AgentEvent::Said {
                    text: "I can prepare a draft; you start it with /run.".to_owned(),
                })
                .into(),
            )
            .await;
        /* ODBIORNIK ŻYJE TAK DŁUGO, JAK SESJA — i to jest sedno tego dublera. Porzucony razem
         * ze `start` zamykałby kanał, więc druga tura odbijałaby się o „stopped listening"
         * i mierzylibyśmy własne sprzątanie zamiast rozmowy. Zadanie przepisuje tury do `Watch`,
         * żeby dało się sprawdzić, że drugie zdanie NAPRAWDĘ doszło. */
        let (voice, mut heard) = mpsc::channel(4);
        let watch = Arc::clone(&self.watch);
        tokio::spawn(async move {
            while let Some(said) = heard.recv().await {
                if let ToAgent::Turn(text) = said {
                    watch
                        .turns
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(text);
                }
            }
        });
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
    /// Nadajnik, który uchwyt wydaje jako głos. Odbiornik czyta osobne zadanie założone w `start`.
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
            took: Duration::from_millis(1),
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

/// Wszystkie wiersze, które czekają w strumieniu — rodzaj i tekst.
fn drained(source: &mut LineSource) -> Vec<(LineKind, String)> {
    let mut out = Vec::new();
    while let Some(line) = source.try_next() {
        out.push((line.kind(), line.text().to_owned()));
    }
    out
}

#[tokio::test]
async fn two_sentences_are_one_session_and_both_show_up() -> Result<(), Box<dyn Error>> {
    let watch = Arc::new(Watch::default());
    let driver = Fake {
        watch: Arc::clone(&watch),
    };
    let (sink, mut source) = line_channel(LINES);
    let mut chat = Chat::new(sink);
    let here = std::env::temp_dir();

    assert!(
        !chat.is_live(),
        "a fresh conversation must not have a process yet: a session started when the screen \
         mounts pays a provider for a turn nobody asked for"
    );

    chat.say(&driver, here.clone(), "what should the checker look at?")
        .await
        .expect("the first sentence opens the conversation");
    assert!(
        chat.is_live(),
        "after the first sentence the session stands"
    );

    chat.say(&driver, here, "and add a second reviewer")
        .await
        .expect("the second sentence is another turn, not another process");

    // ── (a) JEDNA SESJA, NIE PROCES NA ZDANIE ───────────────────────────────────────────────
    //
    // Implementacja startująca proces na każde zdanie płaci zimny start i odbudowę kontekstu za
    // każdym razem, a przede wszystkim GUBI ROZMOWĘ: drugie zdanie trafia do modelu, który nie
    // słyszał pierwszego. Wygląda to jak agent, który nie pamięta, o czym mówiliście.
    /* ODDAJEMY STEROWANIE, bo tury czyta osobne zadanie, a `tokio::test` jest jednowątkowy —
     * asercja postawiona wprost mierzyłaby chwilę PRZED odczytem. Pętla z sufitem, nie `sleep`:
     * czekamy na zdarzenie, a nie na zegar, więc test nie mierzy planisty systemu. */
    for _ in 0..64u8 {
        if !watch.turns().is_empty() {
            break;
        }
        tokio::task::yield_now().await;
    }

    let starts = watch.starts();
    assert_eq!(
        watch.turns(),
        vec!["and add a second reviewer".to_owned()],
        "the second sentence has to reach the SESSION, not just avoid starting a new process. \
         An implementation that quietly drops it looks identical on the process count and loses \
         half the conversation."
    );
    assert_eq!(
        starts.len(),
        1,
        "two sentences have to be two turns of ONE session. {} process start(s) means the second \
         sentence went to a model that never heard the first: {starts:?}",
        starts.len()
    );

    // ── (b) PIERWSZE ZDANIE JEST PIERWSZĄ TURĄ ──────────────────────────────────────────────
    assert_eq!(
        starts.first().map(String::as_str),
        Some("what should the checker look at?"),
        "the sentence a person typed IS the first turn — a filler prompt sent before it would be \
         a turn somebody pays for and nobody asked for"
    );

    // ── (c) OBA ZDANIA CZŁOWIEKA WIDAĆ W STRUMIENIU ─────────────────────────────────────────
    let lines = drained(&mut source);
    let mine: Vec<&str> = lines
        .iter()
        .filter(|(kind, _)| *kind == LineKind::Told)
        .map(|(_, text)| text.as_str())
        .collect();
    assert_eq!(
        mine,
        vec![
            "what should the checker look at?",
            "and add a second reviewer"
        ],
        "both of the person's sentences have to be in the stream, in order. This is the defect the \
         owner reported three times about the run: \"na pewno nie widac moich wiadomosci\"."
    );

    // ── (d) ODPOWIEDŹ WRACA ─────────────────────────────────────────────────────────────────
    assert!(
        lines
            .iter()
            .any(|(kind, text)| *kind == LineKind::Note && text.contains("/run")),
        "what the lead agent said has to reach the stream too — a conversation showing only one \
         side is not a conversation. The lines were: {lines:?}"
    );
    Ok(())
}

#[tokio::test]
async fn reopening_the_screen_keeps_the_conversation() -> Result<(), Box<dyn Error>> {
    /* ZMIERZONE 2026-08-19 W DZIENNIKU APLIKACJI, nie wymyślone: po pierwszym uruchomieniu stało
     * tam „the pump for this run closed its books delivered=0" trzy razy pod rząd. Powód: `open_chat`
     * woła KAŻDY montaż ekranu pracy i każde przeładowanie okna, a pierwsza wersja zamykała wtedy
     * całą rozmowę. Skutek dla człowieka: wyjście na Agentów i powrót gubiło wątek — czyli dokładnie
     * to, po co ta rozmowa istnieje. */
    let watch = Arc::new(Watch::default());
    let driver = Fake {
        watch: Arc::clone(&watch),
    };
    let (first_sink, mut first_source) = line_channel(LINES);
    let mut chat = Chat::new(first_sink);
    let here = std::env::temp_dir();

    chat.say(&driver, here.clone(), "hello").await?;
    assert!(chat.is_live(), "the first sentence opens the session");

    // Ekran zamontował się na nowo: nowy kanał, ta sama rozmowa.
    let (second_sink, mut second_source) = line_channel(LINES);
    chat.lines_go_to(second_sink);

    chat.say(&driver, here, "still here?").await?;

    // ── (a) SESJA ŻYJE DALEJ ────────────────────────────────────────────────────────────────
    assert_eq!(
        watch.starts().len(),
        1,
        "reopening the screen must not start a second session: the person would lose everything \
         said so far, which is the whole point of having a conversation"
    );

    // ── (b) WIERSZE IDĄ DO NOWEGO KANAŁU ────────────────────────────────────────────────────
    //
    // Bez tego punktu (a) przechodziłby dla implementacji, która trzyma sesję i pisze dalej
    // w kanał, którego nikt już nie słucha — czyli dla rozmowy, która żyje i jest niewidoczna.
    let fresh = drained(&mut second_source);
    assert!(
        fresh
            .iter()
            .any(|(kind, text)| *kind == LineKind::Told && text == "still here?"),
        "after the screen reopened, lines have to reach the NEW stream. They went: {fresh:?}"
    );

    let stale = drained(&mut first_source);
    assert!(
        !stale.iter().any(|(_, text)| text == "still here?"),
        "and they must not keep going to the stream nobody watches any more: {stale:?}"
    );
    Ok(())
}

/// 2026-08-30 — PRZESŁANKA TEGO PRZYPADKU ZOSTAŁA COFNIĘTA PRZEZ CZŁOWIEKA, WIĘC ZMIENIŁ SIĘ
/// JEGO PODMIOT.
///
/// Do tego dnia sądził zdanie „you cannot start a workflow run" i miał rację: rozmowa nie miała
/// żadnej drogi do biegu. Rozstrzygnięcie właściciela z 2026-08-30 („rusza samo") tę drogę
/// otwiera przez czasownik `start_workflow`, więc tamto zdanie stało się nieprawdą — a prompt,
/// który kłamie o własnych narzędziach, jest gorszy niż jego brak.
///
/// **Rzecz, której ten przypadek naprawdę bronił, zostaje i jest sądzona niżej:** lider nie ma
/// prawa powiedzieć, że coś zaczął, dopóki nie dowiedział się tego od narzędzia. To jest ta sama
/// ochrona — człowiek nie zostaje z obietnicą, za którą nic nie stoi — tylko wypowiedziana
/// o świecie, w którym start jest możliwy.
///
/// Kryterium strukturalne niżej (`the_conversation_has_no_way_to_reach_a_run`) zostaje **bez
/// zmian i dalej przechodzi**: nowa władza mieszka w `crate::bridge`, a nie w tym module.
#[tokio::test]
async fn the_brief_never_lets_the_model_claim_a_start_it_did_not_get() {
    let brief = BRIEF.to_lowercase();
    assert!(
        brief.contains("never say you have started"),
        "the brief has to forbid claiming a start outright. A model that says 'running it now' \
         and did not leaves the person watching a stream where nothing will ever appear — and \
         that is true whether or not it CAN start things"
    );
    assert!(
        brief.contains("unless a tool told you"),
        "and it has to name what the permission depends on. 'Be careful' is advice; 'only when a \
         tool told you it went' is a rule the model can actually follow, because the tool answers \
         and prose does not"
    );
    assert!(
        brief.contains("start_workflow"),
        "and it has to name the verb, or the model has a tool it was never told about — which is \
         the same as not having it"
    );
    assert_eq!(
        LEAD, "Lead",
        "the speaker label is what a person reads in the stream, so it comes from the word DESIGN \
         §8 sanctions (`orchestrator` is on the jargon table; `lead agent` is its replacement)"
    );
}

#[tokio::test]
async fn the_conversation_has_no_way_to_reach_a_run() {
    /* KRYTERIUM STRUKTURALNE, i to ono jest odpowiedzią na „tylko komendy determinują akcje
     * workflow". Zdanie w promptcie systemowym jest prośbą, którą model może zignorować; brak
     * zależności jest faktem o programie.
     *
     * Czytamy ŹRÓDŁO, bo pytanie dotyczy tego, co ten moduł może w ogóle zawołać. Test sprawdzający
     * „czy bieg nie wystartował" przechodziłby dla implementacji, która startuje go w gałęzi,
     * o której nikt nie pomyślał — a ta asercja nie ma takiej luki. */
    let source = include_str!("../../src/commands/chat.rs");
    /* Bez linii komentarza: nagłówek tego modułu MÓWI o biegu (wyjaśnia, czego nie ma), więc
     * skan po całym pliku łapałby własną dokumentację. */
    let code: String = source
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//") && !trimmed.starts_with("/*") && !trimmed.starts_with('*')
        })
        .collect::<Vec<_>>()
        .join("\n");

    for forbidden in [
        "run_workflow",
        "RunDeps",
        "RunControl",
        "RunRequest",
        "super::run",
        "Store",
    ] {
        assert!(
            !code.contains(forbidden),
            "the conversation reaches `{forbidden}`, so it has a path to starting or recording a \
             run. The owner decided that only commands do that, and a promise in the system prompt \
             is not a mechanism — the absence of this dependency is."
        );
    }
}
