# T-91 — Poziom myślenia dociera do obu vendorów

`Agent.thinking` ma cztery wartości (`quick`, `balanced`, `deep`, `deepest`), pole w formularzu,
wiersz nadpisania w panelu kroku, a doc w `library/agents.rs` mówi, że jest „tłumaczone niżej
na `--effort` i `model_reasoning_effort`". Gerp po całym drzewie: te dwa napisy występują
wyłącznie w `import/adapters.rs` — przy **czytaniu** konfiguracji Codeksa. Żaden sterownik,
żadne pole `RunSpec`, żaden budowniczy argv nie konsumuje `thinking`. Planner właściciela ma
`thinking: deepest` i biegnie na domyślnym wysiłku od pierwszego dnia.

Obie flagi **istnieją**, zmierzone 2026-08-23 na tej maszynie:

- `claude --help` (2.1.241): `--effort <level>` z wartościami `low, medium, high, xhigh, max`.
- `codex`: `-c model_reasoning_effort=<minimal|low|medium|high|xhigh>` (klucz konfiguracji,
  ten sam, który czyta importer).

Odwzorowanie, jedno, w jednym miejscu: `quick → low`, `balanced → medium`, `deep → high`,
`deepest → xhigh`. `max` Claude'a zostaje poza tabelą do decyzji człowieka — cztery wartości
u nas, pięć u vendora, a przelotka z T-90 pozwala wpisać `--effort max` ręcznie.

**Szew, nie nowe pole literału.** `RunSpec` nie ma `Default` i ma 31 miejsc konstrukcji; nowe
pole w literale to 31 plików spoza OWNS. Poziom jedzie tym samym szwem, co połączenia i przelotka:
`DriverConfiguration.arguments` przez `AgentDriver::configured` — albo, jeśli po T-90 przelotka
ma własny budowniczy argumentów per vendor, jako jeden wpis w nim. Wybierz to, co po T-90 jest
jednym miejscem.

**Read first:** `src-tauri/src/library/agents.rs` (`Thinking`, doc przy polu — popraw go) ·
`src-tauri/src/engine/drivers/claude.rs` (`command`, kolejność argv; `--effort` wchodzi obok
`--model`) · `src-tauri/src/engine/drivers/codex.rs` (`build_exec_argv`, `first_turn_argv` —
`-c` musi stać przed `exec`, jak opcje globalne z połączeń; `app-server` dla Leada dostaje to
w `thread/start`, jeśli protokół ma pole, inaczej — zgłoś) · `src-tauri/src/commands/run.rs`
(`plan_agent`, gdzie `effective.thinking` jest dostępne) · `src-tauri/src/commands/chat.rs`
(`spec_for` — Lead też ma `thinking`) · `tasks/T-90.md` AC-2.

## Kto to robi

- **Agent:** `rust-core`
- **Druga opinia:** inny vendor niż pisarz (D3).

## AC-1 Codex dostaje poziom wysiłku w argv pierwszej tury
check: cargo test --test it thinking_reaches_codex::
expect: (\d+) passed

Krok agenta Codex z `thinking: deep` ma w argv `-c model_reasoning_effort=high` **przed** `exec`;
cztery poziomy dają cztery wartości z tabeli; nadpisanie kroku wygrywa z definicją agenta.
Tura wznowienia (`exec resume`) nie powtarza flagi, bo Codex trzyma ją w wątku — kryterium
sprawdza obie tury.

## AC-2 Claude dostaje poziom wysiłku w argv
check: cargo test --test it thinking_reaches_claude::
expect: (\d+) passed

Krok agenta Claude z `thinking: deepest` ma w argv `--effort xhigh`; cztery poziomy, cztery
wartości; nadpisanie kroku wygrywa. Przelotka z T-90, która podaje `--effort`, jest kolizją
z flagą ustawianą przez Loadout — czyli odmową z nazwą flagi, tak jak reszta listy
zarezerwowanych. Lead (`chat.rs`) dostaje to samo co krok.

## AC-3 Tabela odwzorowania jest jedna
check: cargo test --test it one_table_for_thinking::
expect: (\d+) passed

Tak jak `one_table_for_policy.rs`: odwzorowanie `Thinking → poziom vendora` występuje w drzewie
`src-tauri/src/` **dokładnie raz**, poza `import/` (importer tłumaczy w drugą stronę i ma prawo
do własnej tabeli). Druga kopia jest czerwona.

## Sprzątanie po drodze

Doc przy `Agent::thinking` w `library/agents.rs` ma mówić, gdzie naprawdę leży tłumaczenie.
`docs/ARCHITECTURE.md` §11 (S-2) aktualizuje orchestrator po fazie — nie ty.

<!-- OWNS
tasks/T-91.md
src-tauri/src/library/agents.rs
src-tauri/src/engine/drivers/claude.rs
src-tauri/src/engine/drivers/codex.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/src/commands/chat.rs
src-tauri/src/commands/run.rs
src-tauri/src/connections/runtime.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/thinking_reaches_codex.rs
src-tauri/tests/it/thinking_reaches_claude.rs
src-tauri/tests/it/one_table_for_thinking.rs
-->
