# Lider jest mózgiem — plan wdrożenia

> **Dla agentów wykonujących:** WYMAGANY SUB-SKILL: `superpowers:executing-plans`. Kroki mają
> pola wyboru (`- [ ]`). **Nad tym planem stoi `AGENTS.md` tego repo** — jeśli cokolwiek tu się
> z nim kłóci, wygrywa `AGENTS.md`.

**Cel:** zamienić lidera z rozmówcy, który nic nie może, w proces agenta skonfigurowany tak samo
jak krok biegu, wyposażony we własne czasowniki Loadouta (biblioteka, start, nadzór, pytanie).

**Podejście:** most MCP po stdio, hostowany przez Loadout jako ten sam program odpalony z flagą.
Jedna tabela czasowników, dwa adaptery (Claude — zmierzony; Codex — po sondzie). Zero nowych
skrzyń w `Cargo.toml`.

**Stos:** Rust 1.96 + Tauri 2.11, React 19 + vitest, `tokio` (dochodzi cecha `net`), `serde_json`.

**Specyfikacja:** [`docs/superpowers/specs/2026-08-30-narzedzia-loadouta-dla-agenta-design.md`](../specs/2026-08-30-narzedzia-loadouta-dla-agenta-design.md)

---

## Stan na 2026-08-30 — plan wykonany

Dziewięć commitów w trunku, każdy z zieloną bramką na swoim SHA.

| Commit | Co |
|---|---|
| `70867df` | odpowiedź lidera zachowuje swoje wiersze |
| `49e805e` | most: Loadout daje agentowi własne narzędzia |
| `bf60cfb` | lider widzi bibliotekę tego człowieka |
| `5a0ca80` | lider odpala workflow sam |
| `56b6822` | brief mówi prawdę o narzędziach i o tym, gdzie leży prawda o biegu |
| `5cc37df` | agent pyta człowieka i **czeka** na odpowiedź |
| `b310104` | nazwa workflow podświetla się w trakcie pisania |
| `0d6feca` | kafelek serwera bierze komendę od kroku przed sobą |
| `403b76a` | dial mówi o powłoce prawdę |
| `7ed2706` | lista narzędzi lidera przychodzi z jego definicji |

**Dowód końca-końca na żywym `claude 2.1.251`**, po wszystkich zmianach:

```
mcp_servers: [{'name': 'loadout', 'status': 'connected'}]
init tools:  mcp__loadout__ask_the_person, __list_agents, __list_workflows, __start_workflow

TOOL_USE     mcp__loadout__ask_the_person {"question": "Which parser would you like me to fix?"}
TOOL_RESULT  [{"type": "text", "text": "Use the second one."}]
TEXT:        You answered: "Use the second one."
RESULT       success   duration_ms = 7510   (z czego 4000 ms to celowe czekanie)
```

### Co ZOSTAŁO i dlaczego nie weszło

Trzy pozycje z Zadania 7, wszystkie zablokowane tym samym: **wspólny fakt mieszka
w `commands/run.rs`, a `chat_never_starts_a_run.rs` zabrania napisu `super::run` w źródle
`commands/chat.rs`.** Każda z nich wymaga najpierw zjechania tego faktu do `memory/`, `skills/`
albo `library/` — tym samym ruchem, którym zjechało `policy_of`. To jest refaktor z własnym
ryzykiem, nie dopisek przy okazji.

| Co | Gdzie dziś mieszka | Czego wymaga |
|---|---|---|
| skille lidera jako `--plugin-dir` | `run.rs:6959` (`hand_the_skills_to_the_steps`) | zjazd do `skills/` |
| notatki pamięci w prompcie lidera | `run.rs:4745` (`with_what_we_know`) | zjazd do `memory/` |
| dziedziczenie z `.claude/` gospodarza | `run.rs:220` (jedyny wołający `inherit::wire`) | zjazd albo drugi wołający |

**Chartę projektu (`CLAUDE.md`/`AGENTS.md`) świadomie pominięto**: właściciel rozstrzygnął
2026-08-30, że lider ma je **czytać, kiedy uzna za potrzebne** — a to działa bez ani jednej
linii kodu, bo jego katalog roboczy to folder projektu, a `Read`/`Glob` ma na każdym szczeblu
dialu.

### Jeden znany flak

`lead_reaches_loadouts_own_verbs::a_conversation_carries_loadouts_own_server` padł **raz na
około dziewięć** pełnych przebiegów, z pustą listą serwerów. Przyczyny nie udowodniono.
Kryterium dostało kontrolę, która przy następnym razie odróżni „most nie wstał" od
„konfiguracja nie niosła `loadout`" — dwie zupełnie różne awarie, które bez niej czytają się
identycznie.

---

## Ograniczenia globalne

Obowiązują w **każdym** zadaniu, bez powtarzania w treści zadań.

- **Test rustowy jest MODUŁEM celu `it`.** Plik w `src-tauri/tests/it/<nazwa>.rs`, wiersz
  `mod <nazwa>;` w `src-tauri/tests/it/main.rs`, uruchomienie `cargo test --test it <nazwa>::`.
  Plik bez wiersza `mod` nie jest kompilowany ani razu i czyta się jak zdany
  (`checks/tests-listed.sh`).
- **Front:** `npx --no-install vitest run <ścieżka>.test.tsx`.
- **Najpierw sygnatura z `todo!()`** (Rust), żeby test się skompilował i padł w czasie wykonania.
  W TypeScripcie: każdy importowany moduł musi istnieć jako szkielet rzucający
  `new Error('not implemented')` — `vitest` przewraca się już na zbieraniu plików.
- **Jedna komenda na wywołanie Bash.** Komenda złożona `a; b; c` to stracona tura.
- **Zakazane:** `unwrap()` w kodzie produkcyjnym, `panic!` w silniku, `todo!()` po fazie
  kontraktu, hex w komponencie (token z `theme.css`), nowy kolor semantyczny.
- **Niezmiennik 1:** `engine/` nie importuje `tauri::*`. Sprawdzane gerpem w bramce.
- **Niezmiennik 8:** `std::sync::Mutex` nigdy nie jest trzymany przez `await`.
- **Niezmiennik 9:** prompt i sekrety wyłącznie przez stdin, nigdy w argv.
- **Niezmiennik 23:** polityka w jednym rdzeniu; adapter ma pięć linii.
- **Niezmiennik 29:** kryterium o komunikacie asertuje go tam, gdzie widzi go CZŁOWIEK.
- **D5:** interfejs po angielsku, dokumentacja i komentarze po polsku.
- **Bramka:** `scripts/h check` po każdym zadaniu; `scripts/ci.sh full` przy lądowaniu.
- **Nie uruchamiaj dwóch ciężkich `cargo` naraz** (niezmiennik 26).

---

## Struktura plików

Nowe:

```
src-tauri/src/bridge/mod.rs      typy protokołu gniazda; re-eksporty
src-tauri/src/bridge/verbs.rs    JEDYNA tabela czasowników: nazwa, opis, schemat, rola
src-tauri/src/bridge/serve.rs    pętla MCP po stdio — biegnie w procesie mostu
src-tauri/src/bridge/host.rs     nasłuch gniazda po stronie aplikacji + rozdzielnik
```

Zmieniane:

```
src-tauri/src/main.rs            rozgałęzienie `--bridge <gniazdo>` przed startem Tauri
src-tauri/src/lib.rs             `pub mod bridge;`
src-tauri/src/engine/line.rs     tryb kuratora zachowujący przełamania wierszy
src-tauri/src/commands/chat.rs   most jako połączenie rozmowy; BRIEF; konfiguracja jak krok
src-tauri/Cargo.toml             `tokio` zyskuje cechę `net`
```

**Poza zasięgiem tego planu** (własny plan, po tej fali): podświetlanie nazwy w wierszu wejścia,
`ask_the_person`, pole `commandFrom` na kafelku Serve, prawda dialu o powłoce.

---

## Zadanie 1: odpowiedź lidera zachowuje swoje wiersze

**Dlaczego pierwsze:** to jest prawdziwa wada, nie ulepszenie, i bez niej każda następna pozycja
czyta się jak ściana tekstu. Zgłoszenie właściciela z 2026-08-23 („ten tekst niech też będzie
jakoś fajnie i ładnie formatowany") dostało poprawkę w CSS (`line.tsx:179`,
`whitespace-pre-line`), ale kryterium, które jej pilnuje
(`src/sections/run/feed/an-answer-keeps-its-lines.test.tsx:35`), sądzi ją na wierszu rodzaju
**`step`** — a taki pisze planista. Prawdziwa proza agenta to rodzaj `note` i jest spłaszczana
warstwę wcześniej, w Ruście: `Curator::observe` woła `one_line(text)` (`engine/line.rs:892`),
a `one_line` skleja **każdy** biały znak w pojedynczą spację (`line.rs:1246`).

Zielone kryterium, martwa ścieżka produktowa — klasa z niezmiennika 29.

**Pliki:**
- Modyfikacja: `src-tauri/src/engine/line.rs` (pole i konstruktor `Curator`, ramię `Said`)
- Modyfikacja: `src-tauri/src/commands/chat.rs:1512` (rozmowa bierze kurator rozmowy)
- Test: `src-tauri/tests/it/lead_answer_keeps_its_lines.rs`
- Modyfikacja: `src-tauri/tests/it/main.rs` (wiersz `mod`)

**Interfejsy:**
- Produkuje: `Curator::talking() -> Curator` — kurator rozmowy, zachowujący przełamania
  w prozie. `Curator::new()` zostaje **co do bajtu** tym, czym jest, więc ani jedno istniejące
  kryterium biegu nie ma się o co przewrócić.

- [ ] **Krok 1: napisz padający test**

`src-tauri/tests/it/lead_answer_keeps_its_lines.rs`:

```rust
//! Odpowiedź lidera zachowuje wiersze, którymi ją napisał. Proza kroku biegu — nie.
//!
//! # Skarga
//!
//! Właściciel, 2026-08-23: „ten tekst niech też będzie jakoś fajnie i ładnie formatowany aby było
//! to przyjemniejsze". Poprawka weszła wtedy w CSS (`whitespace-pre-line`), ale spłaszczanie
//! dzieje się WARSTWĘ WCZEŚNIEJ, tutaj: `Curator::observe` woła `one_line`, a ta skleja każdy
//! biały znak w spację. Kryterium frontowe sądziło wiersz rodzaju `step`, którego agent nie pisze
//! nigdy — więc było zielone nad ścieżką, którą nikt nie chodzi (niezmiennik 29).
//!
//! # Dlaczego DWA kryteria, a nie jedno
//!
//! Bo to są dwa różne produkty w jednym widoku. Rozmowa jest DO CZYTANIA: akapit, lista i blok
//! kodu są w niej treścią. Strumień pracy jest DO PRZEGLĄDANIA: sześciu agentów piszących
//! akapitami zamienia go w ścianę, przed którą stoi cała reguła 1. Kryterium, które sprawdza
//! tylko pierwsze, przepuściłoby zmianę psującą drugie.

use loadout_lib::engine::line::{Curator, Line, Seen};
use loadout_lib::engine::drivers::AgentEvent;

/// Odpowiedź w trzech wierszach — dokładnie to, co modele piszą naprawdę.
const ANSWER: &str = "Three things stand out:\n- the parser\n- the writer";

/// Zdanie agenta przepuszczone przez podany kurator; `None`, gdy nie powstał wiersz prozy.
fn prose(curator: &mut Curator, agent: &str) -> Option<String> {
    let event = AgentEvent::Said {
        text: ANSWER.to_owned(),
    };
    let seen = Seen {
        agent,
        at_ms: 0,
        event: &event,
        tool: None,
    };
    curator.observe(seen).into_iter().find_map(|line| match line {
        Line::Note { text, .. } => Some(text),
        _ => None,
    })
}

#[test]
fn a_lead_answer_keeps_the_line_breaks_the_model_wrote() {
    let mut curator = Curator::talking();
    let said = prose(&mut curator, "Lead").expect("a Said event has to produce a prose row");

    assert_eq!(
        said, ANSWER,
        "the lead's answer has to reach the screen with the line breaks the model wrote. \
         Flattened, a list of three points reads as one wall of words, and the person asked for \
         the opposite on 2026-08-23"
    );
}

#[test]
fn a_run_step_still_says_it_in_one_line() {
    let mut curator = Curator::new();
    let said = prose(&mut curator, "Forge").expect("a Said event has to produce a prose row");

    assert!(
        !said.contains('\n'),
        "prose from a RUN step stays one line (rule 1): six agents writing paragraphs is the \
         wall of text the whole curated view exists to remove. Got: {said:?}"
    );
}
```

- [ ] **Krok 2: dopisz moduł do celu `it`**

Do `src-tauri/tests/it/main.rs`, w porządku alfabetycznym sąsiadów:

```rust
mod lead_answer_keeps_its_lines;
```

- [ ] **Krok 3: uruchom i potwierdź czerwień**

Uruchom: `cargo test --test it lead_answer_keeps_its_lines::`
Oczekiwane: **błąd kompilacji** `no function or associated item named 'talking' found`.
To jest czerwień, która NIE dowodzi zachowania — dlatego krok 4 daje sygnaturę, a nie ciało.

- [ ] **Krok 4: sygnatura bez zachowania, żeby czerwień była prawdziwa**

W `src-tauri/src/engine/line.rs`, w `impl Curator` obok `new()`:

```rust
    /// Kurator ROZMOWY: ten sam kurator, jedna różnica — proza zachowuje przełamania wierszy.
    ///
    /// Osobny konstruktor, nie argument w [`Curator::new`], bo wołających `new()` jest w tym
    /// drzewie wielu i każdy z nich sądzi bieg. Szew addytywny zostawia ich zachowanie CO DO
    /// BAJTU i nie zamienia tego zadania w trzydzieści zmienionych plików.
    #[must_use]
    pub fn talking() -> Self {
        todo!("Zadanie 1 krok 6")
    }
```

- [ ] **Krok 5: uruchom i potwierdź, że czerwień jest wykonaniem, nie kompilacją**

Uruchom: `cargo test --test it lead_answer_keeps_its_lines::`
Oczekiwane: `a_lead_answer_keeps_the_line_breaks_the_model_wrote` panikuje na `todo!`,
`a_run_step_still_says_it_in_one_line` **przechodzi** (bo `Curator::new()` jest nietknięty).
To jest dowód, że drugie kryterium mierzy dzisiejsze zachowanie, a nie moje zmiany.

- [ ] **Krok 6: najmniejsza implementacja**

W `src-tauri/src/engine/line.rs`, do `struct Curator` (po `minted`):

```rust
    /// Czy proza tego kuratora zachowuje przełamania wierszy.
    ///
    /// `false` w biegu (reguła 1: jedna linia na zdanie), `true` w rozmowie. Jedno pole, jedno
    /// ramię — bo to jest jedna różnica między dwoma produktami stojącymi w tym samym widoku:
    /// rozmowę się CZYTA, strumień pracy się PRZEGLĄDA.
    keeps_line_breaks: bool,
```

`talking()` w miejsce `todo!()`:

```rust
        Self {
            keeps_line_breaks: true,
            ..Self::default()
        }
```

Ramię `AgentEvent::Said` (`line.rs:889`) — `one_line` ustępuje miejsca decyzji:

```rust
            AgentEvent::Said { text } => {
                let line = Line::Note {
                    agent: seen.agent.to_owned(),
                    // Rozmowa zachowuje akapity i listy; bieg skleja do jednej linii (reguła 1).
                    // `paragraphs` zwija ciągi spacji tak samo jak `one_line`, więc wcięcia
                    // z modelu nie robią schodów w wąskiej kolumnie — traci wyłącznie przełamania.
                    text: if self.keeps_line_breaks {
                        paragraphs(text)
                    } else {
                        one_line(text)
                    },
                };
                self.close_then(line)
            }
```

Obok `one_line` (`line.rs:1246`):

```rust
/// Jak [`one_line`], ale zostawia przełamania wierszy.
///
/// Zwija ciągi spacji i tabulatorów WEWNĄTRZ wiersza — z tego samego powodu, dla którego front
/// wybrał `pre-line` zamiast `pre` (`feed/line.tsx:170`): wcięcia z modelu robiłyby schody
/// w wąskiej kolumnie. Ciąg pustych wierszy zwija do jednego, bo trzy puste wiersze w strumieniu
/// to dziura, nie akapit.
fn paragraphs(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut blank_before = false;
    for line in text.lines() {
        let tidy = one_line(line);
        if tidy.is_empty() {
            blank_before = !out.is_empty();
            continue;
        }
        if blank_before {
            out.push(String::new());
            blank_before = false;
        }
        out.push(tidy);
    }
    out.join("\n")
}
```

- [ ] **Krok 7: rozmowa bierze kurator rozmowy**

W `src-tauri/src/commands/chat.rs`, w `read_along` (`chat.rs:1512`):

```rust
    /* KURATOR ROZMOWY, nie biegu: odpowiedź lidera zachowuje akapity i listy, którymi ją
     * napisał. Bieg zostaje przy jednej linii na zdanie (reguła 1) — powód przy
     * `Curator::talking`. */
    let mut curator = Curator::talking();
```

- [ ] **Krok 8: uruchom, potwierdź zieleń obu**

Uruchom: `cargo test --test it lead_answer_keeps_its_lines::`
Oczekiwane: **2 passed**.

- [ ] **Krok 9: potwierdź, że nic z biegu nie padło**

Uruchom: `cargo test --test it curator`
Oczekiwane: bez nowych czerwieni. `Curator::new()` jest nietknięty, więc to jest potwierdzenie,
nie nadzieja.

- [ ] **Krok 10: napraw kryterium, które sądziło niewłaściwy rodzaj**

W `src/sections/run/feed/an-answer-keeps-its-lines.test.tsx:35` fikstura woła `line.step(...)`.
Zamień na `line.note(...)` — jeżeli `note` nie ma budowniczego w
`src/sections/run/feed/fixtures/lines.ts`, dopisz go obok `step` w tym samym kształcie.
Do nagłówka pliku dopisz akapit:

```
 * # 2026-08-30 — TO KRYTERIUM SĄDZIŁO NIEWŁAŚCIWY RODZAJ WIERSZA
 *
 * Fikstura wołała `line.step(...)`, a wiersz `step` pisze PLANISTA i nie przechodzi przez
 * kuratora. Prawdziwa proza agenta to `note` i była spłaszczana w Ruście (`one_line`,
 * `engine/line.rs:892`), zanim tu dojechała — więc CSS był poprawny, kryterium zielone,
 * a skarga właściciela niezałatwiona. Rust ma teraz dwa tryby (`Curator::talking`), a ta
 * fikstura pyta o rodzaj, który agent naprawdę produkuje.
```

- [ ] **Krok 11: uruchom kryterium frontowe**

Uruchom: `npx --no-install vitest run src/sections/run/feed/an-answer-keeps-its-lines.test.tsx`
Oczekiwane: PASS.

- [ ] **Krok 12: bramka**

Uruchom: `scripts/h check`
Oczekiwane: zielone.

- [ ] **Krok 13: commit**

```bash
git add src-tauri/src/engine/line.rs src-tauri/src/commands/chat.rs src-tauri/tests/it/lead_answer_keeps_its_lines.rs src-tauri/tests/it/main.rs src/sections/run/feed/an-answer-keeps-its-lines.test.tsx src/sections/run/feed/fixtures/lines.ts
git commit -m "fix(chat): odpowiedz lidera zachowuje swoje wiersze"
```

---

## Zadanie 2: most — tabela czasowników

**Pliki:**
- Utworzenie: `src-tauri/src/bridge/mod.rs`, `src-tauri/src/bridge/verbs.rs`
- Modyfikacja: `src-tauri/src/lib.rs` (`pub mod bridge;`)
- Test: `src-tauri/tests/it/bridge_verbs.rs` + wiersz `mod` w `main.rs`

**Interfejsy:**
- Produkuje:
  - `bridge::Role` — `Role::Lead` i `Role::Step`
  - `bridge::Verb` — `{ name: &'static str, describe: &'static str, schema: serde_json::Value }`
  - `bridge::verbs::for_role(Role) -> Vec<Verb>`
  - `bridge::verbs::tool_list(Role) -> serde_json::Value` — odpowiedź MCP `tools/list`
- Konsumuje: nic. Moduł jest czysty i nie dotyka dysku ani gniazda.

- [ ] **Krok 1: napisz padający test**

`src-tauri/tests/it/bridge_verbs.rs`:

```rust
//! Czasowniki Loadouta: co dostaje lider, a czego nie dostaje krok biegu.
//!
//! # Dlaczego rola, a nie pole w definicji agenta
//!
//! Rozstrzygnięcie właściciela 2026-08-30 (specyfikacja §5.2). Wskazanie lidera JEST zgodą
//! człowieka, wyrażoną tam, gdzie już mieszka. Krok biegu startujący drugi bieg jest awarią,
//! nie funkcją — a przy roli jest to STRUKTURALNIE NIEMOŻLIWE, nie „domyślnie wyłączone".
//!
//! Drugie kryterium jest tu ważniejsze od pierwszego: „krok nie ma tych czasowników" to zdanie
//! o bezpieczeństwie, a „lider ma trzy" to zdanie o wygodzie.

use loadout_lib::bridge::{Role, verbs};

#[test]
fn a_lead_gets_the_library_and_the_start() {
    let names: Vec<&str> = verbs::for_role(Role::Lead)
        .iter()
        .map(|verb| verb.name)
        .collect();

    assert_eq!(
        names,
        vec!["list_workflows", "list_agents", "start_workflow"],
        "the lead is the orchestrator: it has to see what the person built and be able to start \
         it. These three names travel to the model, so they are part of the contract"
    );
}

#[test]
fn a_run_step_gets_nothing_at_all() {
    assert!(
        verbs::for_role(Role::Step).is_empty(),
        "a step inside a run must not be able to start another run. Not 'off by default' — \
         absent, so the model never learns the verb exists and never promises to use it"
    );
}

#[test]
fn the_tool_list_is_shaped_the_way_mcp_asks_for_it() {
    let listed = verbs::tool_list(Role::Lead);
    let tools = listed
        .get("tools")
        .and_then(|value| value.as_array())
        .expect("tools/list answers with an array under `tools`");

    assert_eq!(tools.len(), 3, "three verbs, three entries");

    let first = &tools[0];
    assert_eq!(first.get("name").and_then(|v| v.as_str()), Some("list_workflows"));
    assert!(
        first.get("description").and_then(|v| v.as_str()).is_some_and(|s| !s.is_empty()),
        "a verb without a description is a verb the model will not reach for"
    );
    assert_eq!(
        first.pointer("/inputSchema/type").and_then(|v| v.as_str()),
        Some("object"),
        "MCP names this key `inputSchema`, not `schema`; the wrong key means the vendor drops \
         the tool in silence"
    );
}
```

- [ ] **Krok 2: dopisz moduł do celu `it`**

```rust
mod bridge_verbs;
```

- [ ] **Krok 3: uruchom i potwierdź czerwień**

Uruchom: `cargo test --test it bridge_verbs::`
Oczekiwane: `error[E0432]: unresolved import loadout_lib::bridge`.

- [ ] **Krok 4: szkielet modułu**

`src-tauri/src/bridge/mod.rs`:

```rust
//! Czasowniki Loadouta dla agenta — most między procesem agenta a aplikacją.
//!
//! # Po co to istnieje
//!
//! Zmierzone 2026-08-29 na `claude 2.1.251`: w trybie `-p` vendor **nie daje** narzędzia
//! `AskUserQuestion` — ani domyślnie (27 narzędzi), ani przez `--tools`. Agent nie ma więc
//! ŻADNEJ drogi, żeby zapytać człowieka albo sięgnąć po cokolwiek, co należy do Loadouta.
//! Ten moduł tę drogę buduje, tą samą, którą człowiek podpina Figmę: serwerem MCP.
//!
//! Zmierzone tą samą sondą: `--tools` NIE rządzi narzędziami MCP — wystarczy `mcp__loadout`
//! w `--allowedTools`, czyli szew, który już istnieje (`drivers/claude.rs`, pole
//! `DriverConfiguration::servers`). Tabela polityk zostaje nietknięta.
//!
//! # Czego tu nie ma
//!
//! Ani jednego warunku nazywającego etap biegu (niezmiennik 27). Czasownik jest DOSTĘPNY,
//! nigdy WYMAGANY — żadne zdanie w tym drzewie nie każe agentowi go użyć. To jest wprost
//! wymaganie właściciela z 2026-08-30: „nie chcę też aby na sztywno było żeby agent zadawał
//! 2-3 pytania, wszystko zależy od analiz i potrzeb".

pub mod verbs;

/// Czyim głosem mówi ten most.
///
/// Rola, nie pole w definicji agenta: powód w całości stoi w specyfikacji §5.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Agent wskazany przez człowieka na lidera rozmowy.
    Lead,
    /// Krok wewnątrz biegu.
    Step,
}
```

`src-tauri/src/bridge/verbs.rs`:

```rust
//! JEDYNA tabela czasowników (niezmiennik 23).
//!
//! Czyta ją odpowiedź `tools/list` **i** rozdzielnik wywołań. Druga kopia — choćby dziś
//! identyczna — rozjeżdża się w dniu, w którym ktoś doda czasownik do jednej z nich, a wtedy
//! model widzi narzędzie, którego nikt nie obsługuje, albo obsługiwane jest coś, o czym model
//! nie wie.

use serde_json::{Value, json};

use super::Role;

/// Jeden czasownik: to, co jedzie do modelu, i nic poza tym.
#[derive(Debug, Clone)]
pub struct Verb {
    /// Nazwa, po której model go woła. Część kontraktu — zmiana jest zmianą zachowania.
    pub name: &'static str,
    /// Zdanie dla modelu. Czasownik bez opisu jest czasownikiem, po który model nie sięgnie.
    pub describe: &'static str,
    /// Schemat wejścia, w kształcie, którego chce MCP.
    pub schema: Value,
}

/// Czasowniki tej roli.
#[must_use]
pub fn for_role(role: Role) -> Vec<Verb> {
    todo!("Zadanie 2 krok 6")
}

/// Odpowiedź na `tools/list` dla tej roli.
#[must_use]
pub fn tool_list(role: Role) -> Value {
    todo!("Zadanie 2 krok 6")
}
```

Do `src-tauri/src/lib.rs`, obok pozostałych modułów:

```rust
pub mod bridge;
```

- [ ] **Krok 5: uruchom, potwierdź czerwień wykonania**

Uruchom: `cargo test --test it bridge_verbs::`
Oczekiwane: trzy testy panikują na `todo!` — czyli kompilują się i **uruchamiają**.

- [ ] **Krok 6: implementacja**

W `src-tauri/src/bridge/verbs.rs`, w miejsce obu `todo!()`:

```rust
#[must_use]
pub fn for_role(role: Role) -> Vec<Verb> {
    match role {
        /* KROK BIEGU NIE DOSTAJE NIC — i to jest zdanie o bezpieczeństwie, nie o zakresie.
         * Krok, który umie wystartować bieg, startuje go w środku cudzej pracy. */
        Role::Step => Vec::new(),
        Role::Lead => vec![
            Verb {
                name: "list_workflows",
                describe: "List the workflows this person has built, with the name to use when \
                           starting one. Use this before starting anything, so you start what \
                           they actually have.",
                schema: json!({ "type": "object", "properties": {} }),
            },
            Verb {
                name: "list_agents",
                describe: "List the agents this person has saved, with what each one is for.",
                schema: json!({ "type": "object", "properties": {} }),
            },
            Verb {
                name: "start_workflow",
                describe: "Start one of this person's workflows. Give the name exactly as \
                           list_workflows returned it. Returns either that it started, or a \
                           plain sentence saying why it could not.",
                schema: json!({
                    "type": "object",
                    "properties": {
                        "workflow": {
                            "type": "string",
                            "description": "The name from list_workflows.",
                        },
                        "task": {
                            "type": "string",
                            "description": "What this run should build. Leave it out to use \
                                            what each step already says.",
                        },
                    },
                    "required": ["workflow"],
                }),
            },
        ],
    }
}

#[must_use]
pub fn tool_list(role: Role) -> Value {
    /* `inputSchema`, nie `schema`: tak nazywa to MCP. Zła nazwa klucza znaczy narzędzie
     * porzucone przez vendora w ciszy — czyli lidera, który „nie chciał" go użyć. */
    let tools: Vec<Value> = for_role(role)
        .into_iter()
        .map(|verb| {
            json!({
                "name": verb.name,
                "description": verb.describe,
                "inputSchema": verb.schema,
            })
        })
        .collect();
    json!({ "tools": tools })
}
```

- [ ] **Krok 7: uruchom, potwierdź zieleń**

Uruchom: `cargo test --test it bridge_verbs::`
Oczekiwane: **3 passed**.

- [ ] **Krok 8: bramka**

Uruchom: `scripts/h check`

- [ ] **Krok 9: commit**

```bash
git add src-tauri/src/bridge src-tauri/src/lib.rs src-tauri/tests/it/bridge_verbs.rs src-tauri/tests/it/main.rs
git commit -m "feat(bridge): tabela czasownikow Loadouta, rola zamiast pola"
```

---

## Zadania 3–7 — kształty, do rozpisania po Zadaniu 2

Rozpisuję je na kroki dopiero wtedy, gdy Zadanie 2 stoi w trunku, bo ich sygnatury zależą od
tego, co naprawdę wyszło z tabeli czasowników. Kształt każdego jest ustalony i zmierzony.

**Zadanie 3 — pętla MCP po stdio (`bridge/serve.rs`) i rozgałęzienie `main.rs`.**
`loadout --bridge <gniazdo>` odpowiada na `initialize`, `tools/list`, `tools/call`; listę bierze
od aplikacji przez gniazdo, żeby prawda była jedna i żywa. Testowalne bez gniazda: pętla przyjmuje
`AsyncRead + AsyncWrite` po stronie vendora i `Link` po stronie aplikacji.

**Zadanie 4 — gniazdo po stronie aplikacji (`bridge/host.rs`).**
`Bridge::open(dir, role, handler)` zakłada gniazdo `0600` i pętlę przyjmowania; `Bridge::connection()`
oddaje syntetyczne `Connection` (`command` = `std::env::current_exe()`, `args` = `["--bridge", …]`).
Wchodzi do `connections::runtime::for_driver` jako **kolejne połączenie** — dzięki temu `servers`
niesie `"loadout"`, a `mcp__loadout` trafia do `--allowedTools` bez zmiany sterownika.
Ścieżka bezwzględna, nigdy nazwa: `claude` w `PATH` tej maszyny to opakowanie, nie CLI.

**Zadanie 5 — `list_workflows` i `list_agents` mówią prawdę.**
Czytają te same funkcje, co lista w oknie, nie drugi obchód katalogu (niezmiennik 13).
`typable` zjeżdża do Rusta, a wspólna fikstura par `nazwa → typable` jest sądzona po obu
stronach granicy — inaczej lider proponuje nazwę, której wiersz wejścia nie zna.

**Zadanie 6 — `start_workflow` przez okno.**
Kanał wierszy umie zbudować **tylko okno**, więc bieg musi wystartować okno — to jest wymuszone,
nie wybrane. Most zawiesza wywołanie, `Line::Suggested` niesie identyfikator wywołania, okno woła
ten sam `startFromLine`, co Enter, i odsyła wynik nową komendą IPC (`commands.golden.txt`).
Lider dostaje **to samo zdanie odmowy**, które zobaczyłby człowiek.

**Zadanie 7 — lider jest skonfigurowany jak krok.**
`as_the_step_is_configured` (`chat.rs:1599`) dotrzymuje własnej nazwy: skille jako `--plugin-dir`,
lista narzędzi z definicji, notatki pamięci, dziedziczenie z `.claude/`. Wspólne fakty muszą
najpierw **zjechać** z `commands/run.rs` do `memory/` albo `library/` — tym samym ruchem, którym
zjechało `policy_of` — bo `chat_never_starts_a_run.rs:383` zabrania napisu `super::run` w źródle
`chat.rs`. Do tego jedno zdanie w `BRIEF` mówiące liderowi, gdzie leży prawda o biegu
(`.loadout/runs/<najnowszy>/run.json`, `handoffs/`, `logs/`) — nadzór z D6 punktu 5 kosztuje
tyle i ani grosza więcej, bo `cwd` lidera to folder projektu, a `Read`/`Glob` ma na każdym
szczeblu dialu.

---

## Ryzyka tej fali

| # | Ryzyko | Najtańsza odpowiedź |
|---|---|---|
| 1 | `tokio` bez cechy `net` — `UnixListener` nie istnieje | dopisać `"net"` do listy cech; to cecha, nie nowa skrzynia |
| 2 | Most nie wstaje, bo `claude` nie znajduje binarki | `current_exe()`, ścieżka bezwzględna, nigdy nazwa — plus kryterium na to |
| 3 | Nowa komenda IPC bez wiersza w `commands.golden.txt` | dopisać w tym samym commicie; inaczej bramka czerwona z powodu niezwiązanego z pracą |
| 4 | Lustro drutu porównuje ZBIÓR kluczy | pole `Option<T>` jedzie jako `null`, **nigdy** jako brak klucza; `src/ipc/types.ts` w tym samym commicie |
| 5 | Codex odbija wywołanie MCP polityką `never` | sonda z §6.4 specyfikacji **przed** adapterem Codeksa; dwie nazwane drogi wyjścia |
