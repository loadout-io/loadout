//! Jeden odczyt, dwa ujścia: surowe bajty na dysk, zdarzenia do kuratora [T7 §4.2].
//!
//! Kolejność w [`pump`] jest częścią kontraktu, nie stylem: **najpierw tee, potem parsowanie**
//! (`docs/ARCHITECTURE.md` §4). Plik `logs/agent-<id>.jsonl` jest źródłem prawdy — to on
//! pozwala skasować `loadout.db` bez straty (`ARCHITECTURE.md` §2 pyt. 2) i to jego użytkownik
//! wysyła jako dowód. W chwili, w której przestaje być bajtowo tym, co wypluło dziecko,
//! kasowanie indeksu przestaje być bezpieczne.
//!
//! Stąd dwie decyzje, które wyglądają na drobiazgi i nie są:
//!
//! - **`read_until(b'\n')`, nigdy `BufReader::lines()`.** `lines()` zjada `\r` i gubi to, czy
//!   linia w ogóle miała znak końca; po takim przejściu bajtowa identyczność jest nie do
//!   spełnienia, a diff, w którym ktoś „posprząta" pętlę do `lines()`, wygląda niewinnie.
//! - **Bufor leci na dysk bez tknięcia `serde_json`.** Runda przez `serde_json` w obie strony
//!   zamienia `0.14836290000000002` na `0.148362`, rozwija escape `<` i zmienia kolejność
//!   kluczy — a każda z tych trzech zmian jest niewidoczna w porównaniu stringów po `trim()`.
//!
//! Linia niesparsowalna jest w tee tak samo jak każda inna, bo tee dzieje się przed
//! dekodowaniem. Sama pętla nigdy nie kończy biegu na nieznanym zdarzeniu (niezmiennik 5):
//! ścieżka dysku nie gubi nigdy, ścieżka widoku wolno gubić [T7 §4.1].
//!
//! # Szew wobec vendora
//!
//! [`decode`] jest jedynym miejscem, które zna nazwy z drutu Claude'a, i robi dokładnie dwie
//! rzeczy: pyta [`ClaudeDecoder`] (T-04) o zdarzenia neutralne wobec vendora i **z tej samej
//! linii** dokłada [`Tool`] — fakty, które [`AgentEvent`] świadomie gubi, a kuracja ich
//! potrzebuje (rodzina czynności, pełna ścieżka, pełne wyjście) [T1 §8.2].
//!
//! Polityka „co znaczy które zdarzenie" zostaje w sterowniku, a nie jest tu przepisana
//! (niezmiennik 23): dwa mapowania Claude'a w dwóch plikach rozjechałyby się przy pierwszej
//! zmianie u vendora, a rozjechałyby się po cichu. T-10 dokłada `decode_codex` o tym samym
//! zwrocie ([`Decoded`]) i nie tyka ani kuratora, ani tej pętli — to jest test, czy ten szew
//! jest abstrakcją, czy fikcją.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use super::drivers::AgentEvent;
use super::drivers::claude::ClaudeDecoder;
use super::line::{Action, Curator, Line, Seen, Tool};

/// Ile strumień miał linii i ile z nich nic nam nie powiedziało.
///
/// Licznik istnieje, bo ma czytelnika (niezmiennik 21): zero przy niepustym biegu znaczy
/// „dekoder połknął śmieci jako nic", a to jest dokładnie ta awaria, której nie widać po
/// niczym innym — bieg kończy się `Ok`, historia jest krótsza, nikt nie pyta dlaczego.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// Ile linii przeszło przez pętlę.
    pub lines: usize,
    /// Ile z nich nie dało się przeczytać jako znane zdarzenie: nie-JSON, nieznany `type`,
    /// znany `type` bez wymaganej treści.
    pub unrecognised: usize,
}

/// Jedno zdarzenie razem z faktami, których ono samo nie niesie.
#[derive(Debug)]
pub struct DecodedEvent {
    /// Zdarzenie neutralne wobec vendora.
    pub event: AgentEvent,
    /// To, czego kuracja potrzebuje ponad zdarzenie. `None` dla zdarzeń bez narzędzia.
    pub tool: Option<Tool>,
}

/// Co wyszło z jednej linii drutu.
///
/// Rozróżnienie „zero zdarzeń" od „nie umiem tego przeczytać" jest **całym** licznikiem
/// [`Stats::unrecognised`]: `system/init` i haki sesji dają zero zdarzeń i są w porządku,
/// a linia, której nikt nie zrozumiał, ma zostać policzona, zanim zniknie.
#[derive(Debug)]
pub enum Decoded {
    /// Linia przeczytana. Pusty wektor jest normalną odpowiedzią.
    Events(Vec<DecodedEvent>),
    /// Nie-JSON, nieznany `type`, albo znany `type` bez wymaganej treści.
    Unrecognised,
}

/// Typy linii, dla których mamy jakąkolwiek regułę. Wszystko inne jest nierozpoznane —
/// i ma zostać **policzone**, a nie po cichu połknięte.
const KNOWN_TYPES: [&str; 5] = ["system", "assistant", "user", "rate_limit_event", "result"];

/// Czyta NDJSON linia po linii, kopiuje **bajty** do `tee` przed parsowaniem i wysyła gotowe
/// wiersze na `lines`.
///
/// Kończy się dopiero na końcu wejścia i **nigdy nie kończy biegu na jednej linii**: linia,
/// której nie da się przeczytać, podnosi [`Stats::unrecognised`] i pętla idzie dalej. Cicha
/// wersja złamania to `?` w środku pętli — pierwsze zdarzenie, które vendor doda w przyszłym
/// tygodniu, urywa bieg w połowie i wygląda to jak awaria agenta, nie jak nasz parser
/// (niezmiennik 5).
pub async fn pump<R>(
    mut reader: R,
    tee: &Path,
    agent: &str,
    lines: mpsc::Sender<Line>,
) -> anyhow::Result<Stats>
where
    R: AsyncBufRead + Unpin + Send,
{
    // Bez `BufWriter`, świadomie: ścieżka dysku nie gubi nigdy [T7 §4.1], a bufor w przestrzeni
    // użytkownika gubi dokładnie ogon — czyli te linie, dla których człowiek otwiera ten plik.
    let mut tee_file = tokio::fs::File::create(tee).await?;

    let mut claude = ClaudeDecoder::new();
    let mut curator = Curator::new();
    let mut stats = Stats::default();

    // Zegar czytamy TUTAJ i tylko tutaj. Kurator dostaje czas argumentem, bo kurator z własnym
    // zegarem nie da się przetestować bez `sleep`, a test ze `sleep` mierzy planistę systemu
    // operacyjnego, nie okno sklejania.
    let started = Instant::now();
    let mut buffer: Vec<u8> = Vec::with_capacity(8 * 1024);

    loop {
        buffer.clear();
        // `read_until`, nie `lines()`: `lines()` zjada `\r` i po takim przejściu bajtowa
        // identyczność tee jest nie do spełnienia. Ta jedna linia jest całym AC-5.
        if reader.read_until(b'\n', &mut buffer).await? == 0 {
            break;
        }
        // TEE PRZED PARSOWANIEM. Linia, której nikt nie zrozumie, jest w pliku tak samo jak
        // każda inna — a to właśnie ona jest potrzebna w zgłoszeniu błędu.
        tee_file.write_all(&buffer).await?;
        stats.lines += 1;

        let at_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let Ok(text) = std::str::from_utf8(&buffer) else {
            // Bajty nie-UTF-8 są w tee i tam zostają czytelne dla człowieka; dla dekodera to
            // linia nie do przeczytania, więc ma być policzona.
            stats.unrecognised += 1;
            continue;
        };

        let text = text.trim();
        if text.is_empty() {
            // Pusta linia nie jest uszkodzeniem: NDJSON kończy się nią przy każdym normalnym
            // wyjściu, a licznik śmieci ma zostać liczbą, którą warto czytać.
            continue;
        }

        match decode(&mut claude, text) {
            Decoded::Unrecognised => stats.unrecognised += 1,
            Decoded::Events(events) => {
                for decoded in events {
                    let seen = Seen {
                        agent,
                        at_ms,
                        event: &decoded.event,
                        tool: decoded.tool.as_ref(),
                    };
                    for line in curator.observe(seen) {
                        send(&lines, line).await;
                    }
                }
            }
        }
    }

    // Ostatnia grupa biegu wyszłaby inaczej nigdy, a użytkownik zobaczyłby o wiersz mniej,
    // niż się wydarzyło.
    for line in curator.flush() {
        send(&lines, line).await;
    }
    tee_file.flush().await?;

    Ok(stats)
}

/// Linia drutu Claude'a → zdarzenia gotowe dla kuratora.
///
/// Klasyfikacja stoi **przed** sterownikiem, bo sterownik ma inny kontrakt: dla niego linia
/// z nieznanym `type` jest rozpoznana i pusta, i to jest dla niego słuszne. Tutaj pytanie
/// brzmi „ile strumienia przepadło", a na nie odpowiada tylko licznik, który widzi różnicę
/// między `system/init` (zero zdarzeń, wszystko w porządku) a `{"type":"quantum_flux"}`
/// (zero zdarzeń, bo nie wiemy, co to było).
pub fn decode(claude: &mut ClaudeDecoder, line: &str) -> Decoded {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Decoded::Unrecognised;
    };
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return Decoded::Unrecognised;
    };
    if !KNOWN_TYPES.contains(&kind) {
        return Decoded::Unrecognised;
    }
    // Znany `type` bez wymaganej treści to nie jest „nic się nie stało": to jest tura, której
    // treść zginęła po drodze, i policzenie jej jest jedynym sposobem, żeby ktoś się o tym
    // kiedykolwiek dowiedział.
    let complete = match kind {
        "assistant" | "user" => content_of(&value).is_some(),
        "rate_limit_event" => value.get("rate_limit_info").is_some(),
        _ => true,
    };
    if !complete {
        return Decoded::Unrecognised;
    }

    let facts = tool_facts(&value);
    let events = claude
        .push(line)
        .into_iter()
        .map(|event| {
            // Sparowanie po `id`, nie po pozycji: sterownik wypuszcza z jednego bloku raz
            // jedno zdarzenie, a raz dwa (`ToolEnd` plus `FileEdit`), więc zip po indeksie
            // rozjechałby się na pierwszej udanej zmianie pliku.
            let tool = match &event {
                AgentEvent::ToolStart { id, .. } | AgentEvent::ToolEnd { id, .. } => {
                    facts.get(id).cloned()
                }
                _ => None,
            };
            DecodedEvent { event, tool }
        })
        .collect();

    Decoded::Events(events)
}

/// Bloki treści wiadomości, jeśli linia w ogóle je niesie.
fn content_of(value: &Value) -> Option<&Value> {
    value.get("message")?.get("content")
}

/// Fakty o narzędziach z jednej linii, po identyfikatorze wywołania.
///
/// To jest cała różnica między [`AgentEvent`] a tym, czego potrzebuje kuracja: zdarzenie
/// niesie etykietę po ludzku i `id`, a wiersz potrzebuje **rodziny** czynności (`Read` to nie
/// `Edit`), **pełnej ścieżki** (rozwinięcie pokazuje pliki) i **pełnego wyjścia** (bez niego
/// reguła 3 nie ma z czego wziąć dwudziestu linii).
fn tool_facts(value: &Value) -> HashMap<String, Tool> {
    let mut facts = HashMap::new();
    let Some(blocks) = content_of(value).and_then(Value::as_array) else {
        return facts;
    };

    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("tool_use") => {
                let Some(id) = block.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if let Some((action, target)) = action_of(name, block.get("input")) {
                    facts.insert(id.to_owned(), Tool::Started { action, target });
                }
            }
            Some("tool_result") => {
                let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
                    continue;
                };
                facts.insert(
                    id.to_owned(),
                    Tool::Ended {
                        output: output_of(block.get("content")),
                    },
                );
            }
            _ => {}
        }
    }

    facts
}

/// Nazwa narzędzia → rodzina czynności i to, czego dotyczy.
///
/// `None` znaczy „nie wiemy, co to za czynność" i kończy się brakiem wiersza — świadomie:
/// wiersz zgadnięty z nieznanej nazwy narzędzia byłby wierszem zmyślonym, a lista narzędzi
/// rośnie u vendora co tydzień (niezmiennik 5).
fn action_of(name: &str, input: Option<&Value>) -> Option<(Action, String)> {
    let (action, target) = match name {
        "Read" | "NotebookRead" | "Glob" => (
            Action::Read,
            first_string(input, &["file_path", "notebook_path", "pattern"]),
        ),
        "Grep" | "WebSearch" | "WebFetch" => (
            Action::Search,
            first_string(input, &["pattern", "query", "url"]),
        ),
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => (
            Action::Edit,
            first_string(input, &["file_path", "notebook_path"]),
        ),
        "Bash" | "BashOutput" | "KillShell" => (
            Action::Ran,
            first_string(input, &["command", "description"]),
        ),
        "AskUserQuestion" => (
            Action::Asked,
            first_string(input, &["question", "description"]),
        ),
        "Task" | "Agent" => (
            Action::Agent,
            first_string(input, &["description", "subagent_type"]),
        ),
        // Serwery narzędzi jadą pod `mcp__<serwer>__<narzędzie>` i nie mają wspólnego kształtu
        // wejścia; nazwa serwera jest jedyną wartością, której nie trzeba zgadywać.
        mcp if mcp.starts_with("mcp__") => (Action::Ran, Some(mcp_label(mcp))),
        _ => return None,
    };

    Some((action, target?))
}

/// Pierwsza z `keys`, która w `input` jest napisem.
///
/// Kolejność kluczy jest kolejnością preferencji, bo vendor nazywa to samo pole różnie
/// w różnych narzędziach. Brak wszystkich znaczy „nie wiemy, czego ta czynność dotyczyła"
/// i kończy się brakiem wiersza, nie wierszem z pustym miejscem.
fn first_string(input: Option<&Value>, keys: &[&str]) -> Option<String> {
    let input = input?;
    keys.iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
        .map(str::to_owned)
}

/// `mcp__notion__search` → `Notion search`. Podkreślenia z drutu nigdy nie trafiają na ekran
/// (niezmiennik 14).
fn mcp_label(name: &str) -> String {
    let mut parts = name.trim_start_matches("mcp__").split("__");
    let server = parts.next().unwrap_or_default().replace('_', " ");
    match parts.next() {
        Some(tool) => format!("{server} {}", tool.replace('_', " ")),
        None => server,
    }
}

/// Treść wyniku narzędzia jako tekst — **pełny**, nieprzycięty.
///
/// Przycinanie jest kuracją i dzieje się w [`Curator`], nie po drodze: reguła 3 potrzebuje
/// ostatnich dwudziestu linii, a te są na końcu, więc wszystko, co utnie tę wartość tutaj,
/// utnie dokładnie przyczynę błędu.
fn output_of(content: Option<&Value>) -> String {
    match content {
        None => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
    }
}

/// Wysyła wiersz do widoku i **nie przerywa biegu**, kiedy nie ma go komu odebrać.
///
/// Ścieżka dysku nie gubi nigdy, ścieżka widoku wolno gubić [T7 §4.1]. Zamknięty odbiornik
/// znaczy „okno się zamknęło", a to nie jest powód, żeby przestać zapisywać strumień, który
/// wciąż leci z procesu dziecka.
async fn send(lines: &mpsc::Sender<Line>, line: Line) {
    if lines.send(line).await.is_err() {
        tracing::debug!("nobody is listening to the feed any more; the tee keeps going");
    }
}
