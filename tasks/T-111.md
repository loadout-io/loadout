# T-111 — Lead Codeksa startuje bez prywatnych MCP i zachowuje Connections

To jest drugie, ostatnie zastępstwo celu zamkniętego najpierw jako T-105, a potem T-110.
T-105 wymagało nieobsługiwanej przez App Server flagi `--ignore-user-config`. T-110 wybrało
właściwy mechanizm konfiguracji per wątek, lecz jego pełna bramka po naprawie zawisła na
`lead_evidence_is_durable.rs`: stara atrapa App Servera nie odpowiadała na nowe `config/read`,
a plik leżał poza OWNS. Kontraktu nie rozszerzamy po biegu i gałęzi T-110 nie lądujemy.

Mechanizm T-110 pozostaje właściwy, ale wyrocznia nie opiera się już wyłącznie na umowie własnej
atrapy. Oficjalny App Server dokumentuje `config/read` z parametrem `includeLayers: false` oraz
wynik pod `result.config`; wygenerowany `ThreadStartParams` ma mapę `config`. Kod źródłowy
OpenAI `ConfigManager::load_with_cli_overrides` zamienia każdą parę tej mapy z JSON do TOML
i dopina ją **po** nakładkach CLI, a referencja konfiguracji definiuje
`mcp_servers.<id>.enabled=false` jako wyłączenie serwera bez usuwania jego konfiguracji.
To rozstrzyga dwie uwagi recenzenta T-110 o kształcie i pierwszeństwie nakładki.

Po `initialize`, ale przed `thread/start`, sterownik czyta efektywną konfigurację. Każdy serwer,
którego nie ma w `DriverConfiguration.servers`, dostaje bezpiecznie zakodowane
`mcp_servers.<id>.enabled=false`; każde Connection jawnie zatwierdzone w Loadoucie dostaje
`enabled=true`, więc prywatny wpis o tej samej nazwie nie może go wyłączyć. Nazwy i konfiguracja
prywatnych serwerów lecą wyłącznie przez JSON-RPC stdin — nie przez argv, plik tymczasowy,
dowody ani log. Błąd `config/read` albo zły kształt odmawia przed `thread/start`.

Uwaga recenzenta T-110 o pierwszej wiadomości z obrazem jest rozstrzygnięta przez wcześniejszy
kontrakt prywatności T-34, a nie przez pokazanie arbitralnego payloadu vendora. Dla wiadomości
tekstowej kod i treść odmowy App Servera docierają do okna. Dla szkicu z obrazem zostaje
bezpieczne `Loadout could not send that message.`; istniejące E2E T-34 wymaga, żeby sentinel
vendora **nie** pojawił się wtedy na ekranie. Nie zmieniaj `src/sections/run/index.tsx` ani tej
wyroczni.

**Read first:** `src-tauri/src/engine/drivers/codex.rs` (`app_server_sandbox`,
`app_server_actor`, handshake i `app_server_argv`) · `src-tauri/src/engine/drivers/mod.rs`
(`DriverConfiguration.servers`) · `src-tauri/src/connections/runtime.rs` (`toml_key`,
`codex_overrides`) · `src-tauri/tests/lead_evidence_is_durable.rs` i
`src-tauri/tests/lead_image_reaches_both_vendors.rs` (pełne fikstury protokołu) ·
`src-tauri/tests/it/the_lead_reaches_the_connections.rs` · `tasks/T-34.md` AC-6 · oficjalne
źródła: `https://learn.chatgpt.com/docs/app-server` (`config/read`),
`https://learn.chatgpt.com/docs/config-file/config-reference`
(`mcp_servers.<id>.enabled`),
`https://github.com/openai/codex/blob/main/codex-rs/app-server-protocol/schema/typescript/v2/ThreadStartParams.ts`
oraz
`https://github.com/openai/codex/blob/main/codex-rs/app-server/src/config_manager.rs`
(`load_with_cli_overrides`).

## Kto to robi

- **Agent:** `rust-core`. Bez `run.rs`; musi wylądować przed T-102, bo oba dotykają
  `codex.rs`.
- **Druga opinia:** inny vendor niż pisarz (D3).

## AC-1 Obie drogi Codeksa używają słów protokołu
check: cargo test --test it codex_lead_uses_protocol_sandbox::
expect: (\d+) passed

`app_server_sandbox` oddaje `read-only` / `workspace-write` / `danger-full-access` — te same
trzy wartości, które droga `exec` podaje przez `-s`. Fikstura prawdziwej drogi leada odpowiada
na pełny handshake, przechwytuje `thread/start` i wiąże oba miejsca jedną tabelą. Każda stara
fikstura App Servera używana przez pełną suitę odpowiada na `config/read`, więc nowa runda
protokołu nie może zawiesić testu spoza kryterium.

## AC-2 Tekstowa odmowa App Servera mówi człowiekowi dlaczego
check: cargo test --test it codex_lead_shows_app_server_refusal::
expect: (\d+) passed

Odpowiedź JSON-RPC z `error` daje zdanie z pełnym dynamicznym `error.code` i `error.message`
vendora. Kryterium idzie produkcyjną drogą sterownika do `say_to_orchestrator_from_window`,
czyli dokładnej wartości odrzucenia komendy Tauri, którą istniejący `why()` pokazuje przy
wiadomości tekstowej. Kontrola bez `error` otwiera rozmowę. Nie zmieniaj prywatności obrazu:
pełna bramka zachowuje E2E T-34, które dla załącznika pokazuje bezpieczne stałe zdanie i nie
renderuje arbitralnego błędu vendora.

## AC-3 Prywatne MCP są wyłączone, a zatwierdzone Connections działają
check: cargo test --test it codex_lead_curates_mcp_servers::
expect: (\d+) passed

Fikstura przechodzi przez produkcyjny `CodexDriver` i pełny handshake. `config/read` zwraca
zwykły prywatny serwer, identyfikator z kropką i cudzysłowem oraz nazwę pokrywającą się z
zatwierdzonym Connection. W `thread/start.config` dwa prywatne wpisy mają dokładnie
`enabled=false`, a Connection dokładnie `enabled=true` i nigdy `false`. Segmenty klucza TOML
powstają w jednej współdzielonej funkcji używanej także przez `connections::runtime`; test
nie akceptuje dwóch alternatywnych kodowań.

Ta sama próba daje Connection nie-sekretną zmienną środowiska i dowodzi z procesu-atrapy tylko
jej **obecności**, nigdy wartości: App Server musi startować przez supervisor z tym samym
jawnym środowiskiem co `exec`. Prywatne identyfikatory nie występują w argv, dowodzie ani
stderr. Błąd JSON-RPC, brak `result.config` i `mcp_servers` o złym typie odmawiają przed
`thread/start` oraz zostawiają dowód śmierci procesu. Kontrola bez prywatnych
serwerów zachowuje dzisiejsze argv co do bajta. Żaden test nie zapisuje ani nie modyfikuje
konfiguracji użytkownika.

<!-- OWNS
tasks/T-111.md
src-tauri/src/engine/drivers/codex.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/src/connections/runtime.rs
src-tauri/tests/lead_evidence_is_durable.rs
src-tauri/tests/lead_image_reaches_both_vendors.rs
src-tauri/tests/it/the_lead_reaches_the_connections.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/codex_lead_uses_protocol_sandbox.rs
src-tauri/tests/it/codex_lead_shows_app_server_refusal.rs
src-tauri/tests/it/codex_lead_curates_mcp_servers.rs
-->
