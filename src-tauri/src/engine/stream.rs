//! Jeden odczyt, dwa ujścia: surowe bajty na dysk, zdarzenia do kuratora [T7 §4.2].
//!
//! Kolejność w [`Recorder`] jest częścią kontraktu, nie stylem: **najpierw tee, potem parsowanie**
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
//! # Dwie pętle, jeden transkrypt (T-34, 2026-08-16)
//!
//! [`pump`] nie jest jedynym czytelnikiem strumienia i nie mógł nim zostać: sterownik żywej
//! sesji czyta stdout procesu, który sam wystartował, i po drodze wypuszcza jeszcze
//! [`AgentEvent`] do uchwytu — bo to na nim stoi `wait()`, eskalacja anulowania i feature-
//! detekcja przerwania. Dlatego trzy zdania wyżej mieszkają w [`Recorder`], a nie w ciele tej
//! pętli, i obie drogi wołają **ten sam** typ. Wersja z drugim `File::create` w sterowniku
//! wygląda niewinnie i jest dokładnie tym rozjazdem, o którym nikt nie pamięta: bajtowa
//! identyczność łamie się wtedy w jednej z dwóch pętli, a plik czyta się tak samo.
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
//! zmianie u vendora, a rozjechałyby się po cichu.
//!
//! # `decode_codex` — bliźniak, i dlaczego czekał do 2026-08-24 (T-97)
//!
//! Do tego dnia stało tu zdanie „T-10 dokłada `decode_codex` o tym samym zwrocie". T-10 go nie
//! dołożyło i nie mogło: `CodexDriver` wypuszczał [`DecodedEvent`] z `tool: None`, więc kurator
//! nie miał z czego wybrać wariantu wiersza i krok Codeksa pokazywał **wyłącznie prozę** — ani
//! jednego `Read`, `Edit` czy `Ran`, choć jego strumień je niesie.
//!
//! Brakowało jednej rzeczy i nie był nią kod: **przechwytu**. `codex-stream.jsonl` ze spike'u S-3
//! jest kopertą awaryjną (`thread.started`, `turn.started`, `error`, `turn.failed`) — tamten bieg
//! wpadł w limit konta, zanim agent cokolwiek zrobił, więc nie ma w nim ANI JEDNEGO `item.*`.
//! Dekoder napisany pod same kształty z dokumentacji byłby przetestowany wobec naszych przekonań,
//! a nie wobec tego, co Codex naprawdę wypisuje — dokładnie ta cicha porażka, przed którą
//! ostrzega S-3. Plik został więc **nietknięty**: kryterium tamtego spike'u sprawdza właśnie
//! wariant „zablokowany" i asertuje, że nie ma w nim `item.completed`.
//!
//! Żywy strumień wszedł **obok**, jako `docs/research/fixtures/codex-stream-live.jsonl` — 11 linii
//! z prawdziwego `codex exec --json`, i to on jest wejściem tego dekodera.
//!
//! Czego w nim nie ma i nie będzie: `reasoning`. Zmierzone 2026-08-24 na `codex-cli 0.148.0`
//! trzema drogami — sześć prawdziwych biegów, sonda z siecią i sonda z
//! `model_reasoning_effort=high` plus `model_reasoning_summary=detailed` — **ten typ nie pada
//! w trybie `exec` ani razu**; tabela w `ARCHITECTURE.md` §6 wymienia go za raportem T2 i ta
//! pozycja się zestarzała. Odwzorowanie istnieje mimo to i sądzi je linia podana dekoderowi
//! wprost (`codex_steps_show_their_actions`), żeby zadziałało w dniu, w którym vendor je włączy;
//! dopisanie go do złotego pliku byłoby wymyśleniem biegu, który się nie zdarzył.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use super::drivers::AgentEvent;
use super::drivers::claude::ClaudeDecoder;
use super::drivers::codex::CodexDecoder;
use super::line::{Action, Curator, Line, Seen, Tool};

/// Jedno zdarzenie razem z faktami, których ono samo nie niesie.
///
/// 2026-08-18 — definicja przeprowadziła się do `engine::drivers`, bo od tego dnia jest to
/// **ładunek kanału sterownika**, nie prywatny wynik tej pętli (powód stoi przy tamtym typie:
/// `Tool` ginął na granicy i wiersze `read`/`edit`/`ran` nie powstawały nigdy). Re-eksport, a nie
/// druga definicja: jedna nazwa ma mieć jedną ścieżkę, a wszystko, co dziś pisze
/// `stream::DecodedEvent`, mówi o tym samym typie.
pub use super::drivers::DecodedEvent;

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

/// Transkrypt jednego kroku: dysk i ekran, w tej kolejności.
///
/// Istnieje jako typ, a nie jako trzy linijki w [`pump`], odkąd pętli czytających jest dwie:
/// [`pump`] czyta gotowy strumień, a `ClaudeDriver::start` (T-34) czyta stdout żywego procesu
/// i po drodze wypuszcza jeszcze zdarzenia do uchwytu sesji. Reguła „najpierw tee, potem
/// parsowanie" przepisana w dwóch pętlach rozjeżdża się po cichu, a rozjazd widać dopiero po
/// pierwszym skasowaniu `loadout.db` — czyli wtedy, kiedy tych linii nie ma już nigdzie.
///
/// Kolejność wywołań jest kontraktem tego typu: [`Recorder::raw`] na **każdą** przeczytaną
/// linię, zanim ktokolwiek spróbuje ją zrozumieć, i dopiero potem [`Recorder::curate`] na to,
/// co z niej wyszło.
#[derive(Debug)]
pub struct Recorder {
    /// `logs/agent-<krok>.jsonl` — ten plik, który użytkownik wysyła jako dowód i który
    /// pozwala skasować indeks.
    ///
    /// Bez `BufWriter`, świadomie: ścieżka dysku nie gubi nigdy [T7 §4.1], a bufor
    /// w przestrzeni użytkownika gubi dokładnie ogon — czyli te linie, dla których człowiek
    /// otwiera ten plik.
    file: tokio::fs::File,
    /// Maszyna pięciu reguł zwijania. Kuracja mieszka wyłącznie w niej (niezmiennik 15);
    /// ten typ podaje jej zdarzenia i przekazuje dalej to, co się domknęło.
    curator: Curator,
    /// Czyj to strumień. Wchodzi w każdy wiersz i w klucz grupy sklejania, więc dwa agenty
    /// czytające pliki w tej samej sekundzie to dwa wiersze, nie jeden.
    agent: String,
    /// Wiersze na ekran. Zamknięty odbiornik nie ma prawa zatrzymać zapisu na dysk.
    lines: mpsc::Sender<Line>,
    /// Chwila, od której liczy się `at_ms` każdego wiersza.
    ///
    /// Zegar czytamy w [`Recorder::curate`] i **tylko** tam. Kurator dostaje czas argumentem,
    /// bo kurator z własnym zegarem nie da się przetestować bez `sleep`, a test ze `sleep`
    /// mierzy planistę systemu operacyjnego, nie okno sklejania.
    started: Instant,
}

impl Recorder {
    /// Otwiera plik transkryptu i bierze kanał, którym pójdą wiersze.
    ///
    /// Katalogu **nie zakłada**: układ `<repo>/.loadout/runs/<ts>__<id>/{run.json,logs/}`
    /// należy do warstwy, która zakłada bieg (`docs/ARCHITECTURE.md` §8), a sterownik ma tam
    /// dopisać plik, a nie wymyślać sobie własne miejsce. Błąd wraca do wołającego, bo krok
    /// poproszony o transkrypt nie ma prawa udawać, że go zrobił.
    pub async fn create(
        path: &Path,
        agent: String,
        lines: mpsc::Sender<Line>,
    ) -> std::io::Result<Self> {
        Ok(Self {
            file: tokio::fs::File::create(path).await?,
            curator: Curator::new(),
            agent,
            lines,
            started: Instant::now(),
        })
    }

    /// Kładzie na dysk **bajty**, dokładnie te i w tej kolejności, w jakiej wyszły z procesu.
    ///
    /// Wołane przed jakąkolwiek próbą zrozumienia linii — także dla tej, której nikt nie
    /// zrozumie, bo to właśnie ona jest potrzebna w zgłoszeniu błędu (niezmiennik 5).
    /// Bufor jedzie tu bez tknięcia `serde_json`: runda w obie strony zamienia
    /// `0.14836290000000002` na `0.148362`, rozwija escape `<` i zmienia kolejność kluczy,
    /// a każda z tych trzech zmian jest niewidoczna w porównaniu napisów po `trim()`.
    pub async fn raw(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.file.write_all(bytes).await
    }

    /// Podaje kuratorowi jedno zdarzenie razem z faktami, których ono samo nie niesie,
    /// i wypuszcza wiersze, które przez nie się domknęły.
    ///
    /// `tool` jest tu całą różnicą między „agent coś zrobił" a „agent przeczytał `src/csv.rs`":
    /// bez niego kurator nie ma z czego wybrać wariantu wiersza i żadna czynność nie zostawia
    /// śladu (`docs/ARCHITECTURE.md` §6).
    pub async fn curate(&mut self, event: &AgentEvent, tool: Option<&Tool>) {
        let at_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let seen = Seen {
            agent: &self.agent,
            at_ms,
            event,
            tool,
        };
        for line in self.curator.observe(seen) {
            send(&self.lines, line).await;
        }
    }

    /// Domyka transkrypt: ostatnia grupa sklejania na ekran, reszta bufora na dysk.
    ///
    /// Bez tego ostatnia grupa biegu nie wyszłaby nigdy, a użytkownik zobaczyłby o wiersz
    /// mniej, niż się wydarzyło — najgorszy rodzaj zgubienia, bo cichy.
    pub async fn close(mut self) -> std::io::Result<()> {
        for line in self.curator.flush() {
            send(&self.lines, line).await;
        }
        self.file.flush().await
    }
}

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
    let mut recorder = Recorder::create(tee, agent.to_owned(), lines).await?;
    let mut claude = ClaudeDecoder::new();
    let mut stats = Stats::default();
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
        recorder.raw(&buffer).await?;
        stats.lines += 1;

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
                    recorder.curate(&decoded.event, decoded.tool.as_ref()).await;
                }
            }
        }
    }

    recorder.close().await?;
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

/// Linia drutu Codeksa → zdarzenia gotowe dla kuratora.
///
/// Bliźniak [`decode`] i **drugi dekoder przed tym samym kuratorem**, nigdy drugi kurator:
/// reguły zwijania, wybór wariantu wiersza i licznik zostają jedną maszyną w [`Curator`]
/// (niezmiennik 15). Tutaj dochodzi wyłącznie to, czego [`AgentEvent`] świadomie nie niesie —
/// rodzina czynności, **pełna** ścieżka i **pełne** wyjście [T1 §8.2].
///
/// # Dlaczego to stoi tu, a nie w `drivers/codex.rs`
///
/// Bo tam byłaby druga implementacja kuracji (niezmienniki 15 i 23), a sterownik ma zostać
/// tabelą „co znaczy które zdarzenie" i niczym więcej. Nagłówek `codex.rs` zgłaszał tę dziurę
/// od T-10 wprost: „transkrypt kroku Codeksa pokaże prozę agenta, ale nie wiersze `read`, `edit`
/// ani `ran`". To jest jej domknięcie.
///
/// # Parowanie idzie po `id` czynności, z TEJ SAMEJ linii
///
/// Codex opisuje jedną czynność dwiema liniami (`item.started`, potem `item.completed`) i obie
/// niosą jej `id`. Fakt zbudowany z innej linii niż zdarzenie rozjechałby się na pierwszej turze,
/// w której dwie czynności są w locie naraz — a przy Codeksie są, bo komenda i szukanie potrafią
/// iść równolegle.
pub fn decode_codex(codex: &mut CodexDecoder, line: &str) -> Decoded {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Decoded::Unrecognised;
    };

    let facts = codex_facts(&value);
    let events = codex
        .push(line)
        .into_iter()
        .map(|event| {
            let tool = match &event {
                AgentEvent::ToolStart { id, .. } | AgentEvent::ToolEnd { id, .. } => {
                    facts.get(id).cloned()
                }
                // ZMIANA PLIKU NIE MA U CODEKSA `id` CZYNNOŚCI, bo nie jest u niego czynnością:
                // `item.completed` typu `file_change` daje po jednym `FileEdit` na plik i ani
                // jednego `ToolStart`. Kluczem jest więc ścieżka — jedyna wartość, którą niosą
                // i zdarzenie, i ta sama linia drutu.
                AgentEvent::FileEdit { path } => facts.get(&path.display().to_string()).cloned(),
                _ => None,
            };
            DecodedEvent { event, tool }
        })
        .collect();

    Decoded::Events(events)
}

/// Fakty o czynnościach z jednej linii Codeksa, po identyfikatorze czynności (albo po ścieżce).
///
/// To jest cała różnica między [`AgentEvent`] a tym, czego potrzebuje kuracja — ta sama, którą
/// po stronie Claude'a wypełnia [`tool_facts`]: zdarzenie niesie etykietę po ludzku i `id`,
/// a wiersz potrzebuje **rodziny** czynności (`Ran` to nie `Edit`), **pełnej ścieżki** i **pełnego
/// wyjścia** (bez niego reguła 3 nie ma z czego wziąć dwudziestu linii).
///
/// Pusta mapa jest normalną odpowiedzią: `thread.started`, `turn.started` i `turn.completed`
/// nie opisują żadnej czynności.
fn codex_facts(value: &Value) -> HashMap<String, Tool> {
    let mut facts = HashMap::new();
    let Some(item) = value.get("item") else {
        return facts;
    };
    // Zapowiedź czy wynik — rozstrzyga typ LINII, nie pole `status` w środku: `status` bywa
    // nieobecny, a wtedy fakt „skończone" nie do odróżnienia od „ruszyło".
    let began = value.get("type").and_then(Value::as_str) == Some("item.started");

    match item.get("type").and_then(Value::as_str) {
        Some("command_execution") => {
            let Some(id) = text_at(item, "id") else {
                return facts;
            };
            if began {
                if let Some(command) = text_at(item, "command") {
                    facts.insert(
                        id,
                        Tool::Started {
                            action: Action::Ran,
                            target: command,
                        },
                    );
                }
            } else {
                // PEŁNE wyjście, nieprzycięte: przycinanie jest kuracją i dzieje się w
                // [`Curator`], a reguła 3 potrzebuje OSTATNICH dwudziestu linii — czyli tych,
                // które każde skrócenie po drodze zabiera jako pierwsze.
                facts.insert(
                    id,
                    Tool::Ended {
                        output: text_at(item, "aggregated_output").unwrap_or_default(),
                    },
                );
            }
        }
        Some("web_search") => {
            let Some(id) = text_at(item, "id") else {
                return facts;
            };
            /* ZAPYTANIE PRZYCHODZI O LINIĘ ZA PÓŹNO, i to jest zmierzony fakt o tym vendorze
             * (`codex-stream-live.jsonl`, 2026-08-24): `item.started` typu `web_search` niesie
             * `query: ""`, a prawdziwe zapytanie stoi dopiero w `item.completed`.
             *
             * Zapowiedź zakłada więc fakt **z pustym tematem** i to jest celowe: wiersz otwiera
             * wyłącznie `Curator::tool_start`, więc zapowiedź bez faktu znaczyłaby brak wiersza
             * w ogóle — szukanie zniknęłoby z transkryptu, choć się odbyło. Temat dokłada
             * `Curator::tool_end` z linii, która go zna. */
            let query = text_at(item, "query");
            if began {
                facts.insert(
                    id,
                    Tool::Started {
                        action: Action::Search,
                        target: query.unwrap_or_default(),
                    },
                );
            } else {
                facts.insert(
                    id,
                    Tool::Ended {
                        output: query.unwrap_or_default(),
                    },
                );
            }
        }
        Some("mcp_tool_call") => {
            let Some(id) = text_at(item, "id") else {
                return facts;
            };
            // Serwer narzędzi jedzie jako `Ran`, tak samo jak u Claude'a `mcp__<serwer>__<nazwa>`
            // (patrz [`action_of`]): to jest czynność w cudzej aplikacji, nie czytanie pliku.
            let label = [text_at(item, "server"), text_at(item, "tool")]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            if began {
                if !label.is_empty() {
                    facts.insert(
                        id,
                        Tool::Started {
                            action: Action::Ran,
                            target: label,
                        },
                    );
                }
            } else {
                facts.insert(id, Tool::Ended { output: label });
            }
        }
        Some("file_change") if !began => {
            // Po jednym fakcie na PLIK, nie jednym na czynność: `changes[]` bywa listą, a jeden
            // wiersz na całą listę powiedziałby człowiekowi, że zmienił się jeden plik, podczas
            // gdy zmieniły się trzy. Klucz jest ścieżką, bo tym samym kluczem szuka wyżej
            // `FileEdit` — u tego vendora zmiana pliku nie ma `id` czynności.
            for change in item
                .get("changes")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                if let Some(path) = text_at(change, "path") {
                    facts.insert(
                        path.clone(),
                        Tool::Started {
                            action: Action::Edit,
                            target: path,
                        },
                    );
                }
            }
        }
        _ => {}
    }

    facts
}

/// Niepusty napis spod klucza — albo nic.
///
/// Pusty napis jest tu tym samym, co brak pola: `query: ""` w zapowiedzi szukania niesie tyle
/// samo treści, co jego nieobecność, a wiersz zbudowany na nim byłby wierszem bez tematu.
fn text_at(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
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
