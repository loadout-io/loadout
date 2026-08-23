# T-97 — Codex na równi z Claude'em

D3 mówi, że oba vendory są zakresem v1 i że cross-vendorowa para jest domyślna. `CodexDriver`
działa (T-10), ale w pięciu miejscach Codex jest vendorem drugiej kategorii — i właściciel
używa go w co drugim kroku (Planner, Research 2, Verification 1/3, Implementation):

1. **Kuracja.** `engine/stream.rs` ma jedno `decode(claude: &mut ClaudeDecoder, …)`; obiecane
   `decode_codex` nie istnieje, a `CodexDriver` wypełnia `DecodedEvent::tool` jako `None`.
   Krok Codeksa pokazuje w strumieniu **wyłącznie prozę** — zero `Read 6 files`, `Changed
   src/x.rs`, `Ran tests`. Codex ma własną taksonomię (`command_execution`, `file_change`,
   `reasoning`, `agent_message`, `web_search`, `mcp_tool_call`; ARCHITECTURE §6), złoty plik
   `docs/research/fixtures/codex-stream.jsonl` leży w repo od S-3.
2. **Sieć.** `reachesTheWeb` daje Claude'owi `WebFetch`/`WebSearch` na **każdym** dialu;
   Codex dostaje `network_access=true` tylko przy `workspace-write`. Agent Codex `look-only`
   z włączoną siecią nie ma sieci i nikt mu o tym nie mówi — komentarz w `codex.rs` nazywa
   asymetrię i zostawia ją.
3. **Narzędzia.** `what_this_step_may_use` (`run.rs`) sądzi `Tools::Only([...])` agenta Codex
   przeciw sufitowi **Claude'a** i potrafi odmówić całego biegu o listę, której `CodexDriver`
   nigdy nie czyta (`CAPABILITIES` mówi: `tools` u Codeksa `Unavailable`).
4. **Koszt.** `cost_usd: None` w każdym kroku Codeksa (zmierzone w `run.json` właściciela);
   `exec --json` raportuje zużycie tokenów (`app_usage` czyta je dla app-server), ale dla `exec`
   nic nie trafia do `Line::Done` ani do `run.json`. Budżet z T-94 liczy takie kroki jako zero.
5. **Lead.** `commands/chat.rs` woła na sterowniku wyłącznie `with_evidence` — nigdy
   `configured()` — więc rozmowa z Leadem nie ma połączeń MCP u **żadnego** vendora; dla Codeksa
   dodatkowo ścieżka `app-server` nie dokleja `configuration.arguments` wcale.

**Read first:** `src-tauri/src/engine/stream.rs` (`decode`, `Decoded`, `DecodedEvent`) ·
`src-tauri/src/engine/drivers/codex.rs` (`emit`, `emit_app`, `app_usage`, `build_exec_argv`,
`start_conversation`) · `src-tauri/src/engine/line.rs` (`Line::Done`, jak `cost_usd`/tokeny
wchodzą na drut) · `src-tauri/src/commands/run.rs` (`what_this_step_may_use`, `one_turn` —
gdzie księga dostaje `input_tokens`/`output_tokens`) · `src-tauri/src/commands/chat.rs`
(`spec_for`, `begin` — kolejność opakowań jak w `Live::run_agent`) ·
`src-tauri/src/library/agents.rs` (`CAPABILITIES`) · `src/sections/agents/capabilities.ts`
(lustro ręczne), `more-settings.tsx` · `docs/ARCHITECTURE.md` §6 (tabela zdarzeń) ·
`tests/it/driver_codex_stream.rs` (istniejący parser — rozszerz, nie dubluj) ·
`AGENTS.md` niezmienniki 5, 14, 23.

## Kto to robi

- **Agent:** `rust-core` na AC-1, AC-3, AC-4, AC-5, potem `frontend` na AC-2 — jeden worktree,
  jedna bramka.
- **Druga opinia:** inny vendor niż pisarz (D3).

## Poszerzenie zakresu — 2026-08-23, policzone przed biegiem

`Line::Done` nie ma dziś pól tokenowych, a AC-4 ich wymaga. Nowe pole w wariancie enuma
przewraca **każdy** jego literał: jest ich pięć i wszystkie leżą w plikach, które są
kryteriami wylądowanych zadań (`stream_*` → T-05, `ipc_line_wire_golden` → T-07,
`driver_codex_finish` → T-10). Dochodzą więc do OWNS z mandatem **na literał, nigdy na
asercję**: dopisujesz brakujące pola w konstrukcji i **nic poza tym**.

Jeden z nich wymaga drugiej linijki i to jest świadome: `ipc_line_wire_golden` porównuje
kształt drutu z `src/ipc/line-wire.golden.json` (masz go w OWNS). Kształt drutu **zmienia
się celowo**, więc fikstura idzie za nim — na tym polega golden. Czego NIE WOLNO: osłabić
porównania, dopisać do fikstury pola, którego drut nie niesie, ani odwrotnie.

`StepEntry` w `run.json` ma pola `input_tokens`, `output_tokens` i `cached_tokens` **od T-06**
— Codex ich po prostu nigdy nie wypełniał. Ta połowa AC-4 nie wymaga ani jednego nowego pola.

**Dopisane 2026-08-24, po zgłoszeniu pisarza:** `src/sections/run/feed/fixtures/lines.ts` też
dochodzi do OWNS, na trzy mechaniczne linie. Łańcuch jest wymuszony, nie wybrany: pola tokenowe
na `Line::Done` → trzy klucze w `src/ipc/line-wire.golden.json` → `src/ipc/types.ts` musi je
odbić, bo `types.test.ts` porównuje ZBIORY kluczy z goldenem → TypeScript wymaga tych pól
w literale `done` w tamtej fiksturze. Poszerzenie wyżej wyliczyło tylko literały rustowe.

## AC-1 Krok Codeksa pokazuje, co zrobił, nie tylko co powiedział
check: cargo test --test it codex_steps_show_their_actions::
expect: (\d+) passed

Złoty plik **`docs/research/fixtures/codex-stream-live.jsonl`** przepuszczony przez kurację daje
te same rodzaje linii, co strumień Claude'a dla tych samych czynności: `command_execution` → `ran`
(z `exit_code`), `file_change` → `edit` (ścieżka, rodzaj), `web_search` → `search`,
`agent_message` → proza, nieznany typ → porzucony bez przewracania biegu (niezmiennik 5).
Reguły zwijania (okno 2 s, licznik) są te same — kryterium nie dopisuje drugiego kuratora, tylko
drugi dekoder przed tym samym.

**Który to plik i dlaczego nie ten stary** (rozstrzygnięte 2026-08-24 po pierwszym biegu tego
zadania). `codex-stream.jsonl` z S-3 ma **cztery linie i ani jednego z tych rodzajów**: to jest
koperta biegu, który padł na wyczerpanych kredytach (`thread.started`, `turn.started`, `error`,
`turn.failed`). Nie wolno go podmienić — kryterium S-3 sprawdza właśnie ten wariant „zablokowany"
i asertuje, że nie ma w nim ani jednego `item.completed`. Żywy strumień wchodzi więc **obok**,
jako drugi plik, nagrany prawdziwym `codex exec --json` (11 linii).

**`reasoning` dowodzisz osobną asercją, nie fiksturą, i to jest zmierzony fakt o vendorze.**
Sprawdzone trzema drogami 2026-08-24 na `codex-cli 0.148.0`: sześć prawdziwych biegów Codeksa
u właściciela, sonda z siecią i sonda z `model_reasoning_effort=high` **plus**
`model_reasoning_summary=detailed` — **`reasoning` nie pada w trybie `exec` ani razu**.
Tabela w `ARCHITECTURE.md` §6 wymienia go za raportem T2 i ta pozycja się zestarzała.
Odwzorowanie ma więc istnieć i być sprawdzone linią podaną dekoderowi wprost (zwykły test
jednostkowy), żeby zadziałało, gdyby vendor kiedyś zaczął je wysyłać — ale fikstury pod nie
**nie wolno dopisywać**.

## AC-2 Agent Codex „look-only" z siecią słyszy, że sieci nie dostanie
check: npx --no-install vitest run src/sections/agents/codex-web-needs-write-access.test.tsx
expect: (\d+) passed

Formularz agenta z `runsWith: codex`, `fileAccess: look-only` i włączonym „Can it reach the
web" pokazuje pod przełącznikiem zdanie, że Codex sięga do sieci tylko wtedy, gdy może zmieniać
pliki, i że ten agent sieci nie dostanie; przy `ask-first`/`work-freely` zdania nie ma; u Claude'a
nigdy. `capabilities.ts` dostaje ten fakt jako dane, nie jako `if` w komponencie.

## AC-3 Lista narzędzi agenta Codex nie odmawia biegu
check: cargo test --test it codex_tools_never_refuse_the_run::
expect: (\d+) passed

Krok agenta Codex z `tools: {only: ["Read", "Bash"]}` startuje (lista jest ignorowana tak,
jak ignoruje ją sterownik) i dostaje w `run.json` jedno zdanie w `effective`/uwadze, że Codex
nie zawęża narzędzi; ten sam krok u Claude'a jest sądzony jak dziś. Sufit narzędzi jest
pytaniem do vendora (`AgentDriver`), nie stałą Claude'a w `run.rs` — to jest jedyna zmiana
w `what_this_step_may_use`.

## AC-4 Krok Codeksa raportuje tokeny, a koszt — kiedy go zna
check: cargo test --test it codex_steps_report_their_tokens::
expect: (\d+) passed

Zdarzenie końcowe `exec --json` z użyciem tokenów daje `input_tokens`/`output_tokens`
(i `cached_tokens`, jeśli są) w `Line::Done` i w `run.json` kroku. `cost_usd` zostaje `None`,
dopóki vendor nie poda kwoty — kryterium **nie** wymaga cennika i nie pozwala go wpisać na
sztywno; za to `Line::Done` bez kosztu, a z tokenami, pokazuje w pasku `12k tokens` zamiast
pustki. Bieg bez żadnego kroku z kosztem nie pokazuje `$0.00`.

## AC-5 Lead dostaje połączenia u obu vendorów
check: cargo test --test it the_lead_reaches_the_connections::
expect: (\d+) passed

Rozmowa z Leadem, którego agent ma `connections: ["x"]`, dostaje u Claude'a `--mcp-config`
z plikiem połączeń, a u Codeksa opcje `-c mcp_servers.x.…` **także na ścieżce `app-server`**
(przed podkomendą). Kolejność opakowań w `chat.rs` jest ta sama, co w `Live::run_agent`
(Connections → dziedziczenie → dowody) i stoi w komentarzu. Agent bez połączeń ma argv co do
bajtu jak dziś.

## Sprzątanie po drodze

Komentarz `decode_codex` w nagłówku `stream.rs` (ok. linii 41) przestaje być obietnicą.
`drivers/mod.rs` ok. linii 34 („do czasu T-10") — T-10 wylądowało; popraw.

<!-- OWNS
tasks/T-97.md
src-tauri/src/engine/stream.rs
src-tauri/src/engine/line.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/src/engine/drivers/codex.rs
src-tauri/src/engine/drivers/claude.rs
src-tauri/src/commands/run.rs
src-tauri/src/commands/chat.rs
src-tauri/src/library/agents.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/driver_codex_finish.rs
src-tauri/tests/it/ipc_line_wire_golden.rs
src-tauri/tests/it/stream_closing_lines.rs
src-tauri/tests/it/stream_collapse_defaults.rs
src-tauri/tests/it/stream_curation_fixture.rs
docs/research/fixtures/codex-stream-live.jsonl
src/sections/run/feed/fixtures/lines.ts
src-tauri/tests/it/codex_steps_show_their_actions.rs
src-tauri/tests/it/codex_tools_never_refuse_the_run.rs
src-tauri/tests/it/codex_steps_report_their_tokens.rs
src-tauri/tests/it/the_lead_reaches_the_connections.rs
src/sections/agents/capabilities.ts
src/sections/agents/more-settings.tsx
src/sections/agents/agent-form.tsx
src/sections/agents/codex-web-needs-write-access.test.tsx
src/sections/run/strip/model.ts
src/ipc/types.ts
src/ipc/line-wire.golden.json
-->
