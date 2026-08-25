# T-126 — Refleksja ma fizyczny dowód, późny Stop i trzy stabilne drogi z ekranu

T-125 pozostaje dowodem, nie źródłem kodu. Zamknięto je bez lądowania po jedynej rundzie
naprawy. Enforced `before` uczciwie certyfikowało cztery targety, a pierwsza bramka wykazała
realny brak receipt i wadliwy późny Stop. Recenzent znalazł cztery luki wyroczni: exact-once
wracało przed opóźnionym duplikatem, Stop fabrykował `Dead` bez supervisora, budżet dało się
zgadnąć z promptu, a stary `run.json` był sprawdzany tylko na brak błędu.

Naprawa domknęła prawdziwy proces grupowy, neutralne prompty i stare pola historii, ale
autorytatywna bramka pozostała 15/20. AC-1 skanowało `<home>/memory/notes`, choć `scan_notes`
samo dopisuje `notes/`, więc czytało `notes/notes`. AC-2 czekało stałe 6 sekund w każdym teście
przy domyślnym limicie Vitest 5 sekund, więc wszystkie sześć scen kończyło się timeoutem.
Pełny clippy odsłonił ścisłe porównanie `f64`, a formatter — kolejność importów.

To zadanie jest świeżym następcą H14. Startuje z trunka po T-121 i T-124.
**Nie przenosi commitów, implementacji, speców ani testów z `task-T-125`.** Ma cztery nowe,
globalnie unikalne targety. Refleksja pozostaje jedną prywatną turą Loadouta po skończonym
biegu, nie krokiem grafu.

**Read first:** nagłówek `docs/STATUS.md` i opis zamknięcia T-125 ·
`src-tauri/tests/run_evidence_reaches_the_product.rs` jako zamrożony kontrakt fizycznego UUID ·
`src-tauri/src/commands/run.rs` (`Live::run_agent`, `what_this_run_taught_us`,
`a_short_turn_about`, `REFLECTION_*`) · `src-tauri/src/memory/handoff.rs`
(`Section::name`, `Handoff::left_nothing`) · `src-tauri/src/memory/notes.rs`
(`scan_notes` przyjmuje korzeń pamięci i samo dopisuje `notes/`) · wylądowane T-121 w
`store/` i T-124 w `memory::notes` · `src-tauri/src/evidence.rs` ·
`src-tauri/src/engine/drivers/mod.rs`, `claude.rs` i `supervisor.rs` ·
`src-tauri/src/ipc.rs` · `src/sections/run/index.tsx`, `io.ts`, `start.tsx`, `launch.ts`,
`run-command.ts`, `requested-launch.ts` i `requested.ts` · prawdziwa droga
`src/sections/workflows/index.tsx` → `editor.tsx` → `canvas/problems.tsx` ·
`e2e/harness.ts` i wzorzec sceny w
`e2e/tests/skill-refusal-survives-a-real-click.spec.ts` · istniejące duble w
`a_run_leaves_suggestions.rs` i `a_suggestion_needs_a_because.rs` · `AGENTS.md`
niezmienniki 4, 6, 7, 10, 16, 19, 26 i 29.

## Kto to robi

- **Agent:** Codex przez jeden pełny bieg Harnessu.
- **Druga opinia:** osobny Codex na innym modelu, tylko do odczytu; jawny właścicielski
  wyjątek operacyjny od D3 ze względu na kończący się budżet Claude'a.

## Mandaty brzegowe — obowiązkowa część implementacji

W `a_run_leaves_suggestions.rs` i `a_suggestion_needs_a_because.rs` wolno zmienić wyłącznie
implementacje `AgentDriver`. Każdy dubel, który jawnie zwraca `Some` z `reflecting()`, musi
w klonie przenieść `with_settings`, `with_evidence` i dodatni budżet. Scenariusze, liczby tur
i asercje pozostają bez zmian. Brak któregokolwiek twardego opakowania w produkcji odmawia
przed spawnem; nie wolno rozluźnić wymagań, żeby stare duble przeszły.

Browserowy test AC-2 nie importuje ani nie woła `requestRun`, `launchRequested`, `launchRun`
lub handlerów komponentów. Montuje prawdziwą aplikację przez istniejący `e2e/harness.ts`,
klika widoczne kontrolki w Chromium i czyta wyłącznie DOM oraz taśmę
`window.__TAURI_INTERNALS__`. `e2e/harness.ts` jest przyrządem tylko do odczytu i nie należy
do `OWNS`. Pierwsza porażka `before` następuje po udanym zamontowaniu aplikacji, na braku
zachowania lub złym argumencie, nie na rozruchu Vite/Chromium, braku modułu, EPERM portu albo
przekroczonym czasie.

Każda funkcja nowego testu Rust ma najwyżej 90 wierszy, a każda dotknięta funkcja produkcyjna
najwyżej 100. Anulowanie prywatnej tury wydziel do małych helperów. Fallible helpery zwracają
`Result`, infallible konkretny typ. Porównania `f64` używają skończonej tolerancji, nigdy
`assert_eq!`. Przed oddaniem cały zakres jest zgodny z `cargo fmt`; bez `panic!`, `unwrap`,
`expect`, `#[allow(clippy::…)]`, `@ts-nocheck` i `prettier-ignore`.

## AC-1 Prywatny tryb ma dokładne evidence, receipt i zachowuje stary oraz zwykły bieg
check: cargo test --test t126_private_reflection_receipt_and_evidence
expect: (\d+) passed

Atrapa ma jawny stan klona: bazowy driver jest zwykłym krokiem, a tylko wynik `reflecting()`
jest refleksją. Nie wolno rozpoznawać trybu po promptcie, modelu, kolejności ani nazwie pliku.
Refleksja wymaga prywatnej pamięci `<bieg>/mem/_reflection/`,
`EvidenceTarget::reflection` i dodatniego sufitu ceny. Osobna awaria każdego opakowania daje
zero startów oraz prawdziwe `run.json reflection.ran == false`; nie ma fallbacku do gołego
drivera.

Zwykły krok ma logiczny klucz `build`, lecz test czyta jego **fizyczny UUID** z prawdziwego
`run.json` i po nim wymaga `logs/agent-<uuid>.jsonl`, `.stderr.log` i `.input.json`.
Zamrożony `run_evidence_reaches_the_product.rs` pozostaje zielony. Refleksja zapisuje
dokładnie `reflection.jsonl`, `reflection.stderr.log` i `reflection.input.json`; pełny listing
`logs/` odmawia dodatkowego `reflection*` i chroni sentinel poza biegiem.

Jedna uziemiona reguła i jedna bez powodu dają dokładnie `ran:true`, `kept:1`,
`dropped_without_reason:1` oraz rzeczywisty `cost_usd`. Test skanuje kandydatki przez
`scan_notes(<home>/memory)`, nigdy `<home>/memory/notes`; produkcji nie wolno pisać
zduplikowanego `notes/notes`, żeby zazielenić błędną ścieżkę. Po usunięciu pola `reflection`
z realnego `run.json` publiczny czytnik historii zachowuje co najmniej id, status i komplet
kroków — sam brak błędu lub pusty syntetyczny wiersz nie wystarcza.

## AC-2 Widoczny checkbox steruje trzema drogami dokładnie raz, także po cichym oknie
check: npx --no-install vitest run e2e/tests/t126-reflection-choice-real-routes.spec.ts
expect: (\d+) passed

Test uruchamia prawdziwy frontend w Chromium. Najpierw czeka na zamontowany ekran Run,
znajduje widoczne `Learn from this run`, wymaga domyślnego `checked === true`, klika do
`false` i sprawdza DOM po rerenderze; potem klika do `true` i sprawdza ponownie. Stan należy
do produkcyjnego `Run`, nie do modułowej zmiennej obok drzewa.

Na świeżych kartach, jawnie dla `false` i `true`, test przechodzi trzema drogami człowieka:

1. prawdziwy `button[data-workflow-run="manual"]`;
2. wpisanie `/run <workflow>` i Enter w prawdziwym wierszu;
3. Workflows → prawdziwy kafelek → widoczny `Run` w edytorze → ekran Run → zamontowany efekt.

Helper najpierw **polluje tylko do pierwszego** `run_workflow` z limitem najwyżej 4 sekundy,
potem obserwuje co najmniej 300 ms ciszy i dopiero wtedy czyta pełną taśmę. Każdy test ma
jawny limit co najmniej 15 sekund; nie wolno czekać stałe 6 sekund przy domyślnym 5-sekundowym
limicie Vitest. Każda droga produkuje dokładnie jedno IPC z jawnym `reflectionEnabled`
równym widocznemu checkboxowi. Powrót/remount Run także nie dodaje wywołania.

Scena odpowiada na realne `list_workspaces`, `list_workflows`, `load_workflow`,
`check_workflow`, `list_agents`, `list_skills` i `run_workflow`; nie zasiewa prywatnego stanu
modułów. Nie spełnia AC: `renderToStaticMarkup`, osobny builder, wywołanie settera, import
`requested.ts`, bezpośrednie helpery startu, ręczne wykonanie propsa, grep albo asercja przed
cichym oknem.

## AC-3 Późny Stop dowodzi śmierci prawdziwej grupy; puste przekazanie nie płaci
check: cargo test --test t126_late_stop_and_empty_handoff
expect: (\d+) passed

Wyłączony checkbox daje zero tur. `true` z użytecznym handoffem daje dokładnie jedną. Udany
krok z pustym ciałem pozostaje `succeeded`, zapisuje prawdziwy handoff
`Handoff::left_nothing() == true`, a następny czytelnik widzi trzy sekcje nazwane wyłącznie
przez `Section::name()`. Refleksja rozpoznaje semantykę, nie rozmiar, daje zero tur i
`reflection.ran == false`.

Osobny scenariusz pozwala schedulerowi skończyć zwykły krok, czeka na dowód, że prywatny
proces refleksji już wystartował i nadal żyje, dopiero wtedy wywołuje Stop. Uchwyt ma prawdziwy
PGID i deleguje anulowanie do supervisora; test odmawia dla `group() == None`, fabrykowanego
`GroupProof::Dead` albo samego licznika wywołań. Stop czeka na `ESRCH`, proces po nim nie
istnieje, kandydatka i koszt nie powstają, a `run.json reflection.ran == false`.

Fixture zachowuje oba `TempDir` do końca asercji. Zakazane: kasowanie pustego pliku, zmiana
formatu/T-114, krok failed/cancelled/skipped, zgadywanie po bajtach, `tokio::timeout` bez
`AgentHandle::cancel` oraz drop zadania Rusta uznany za śmierć procesu.

## AC-4 Budżet należy do stanu klona, nie modelu ani promptu
check: cargo test --test t126_budget_is_clone_state
expect: (\d+) passed

Prawdziwy spawn refleksji emituje dokładnie jedno `--max-budget-usd <stała>`, zachowuje
`REFLECTION_MODEL` i politykę tylko-do-odczytu. Zwykły krok z tym samym modelem i bez budżetu
nie dostaje limitu; drugi zwykły krok z własnym budżetem zachowuje kwotę.

Wszystkie zwykłe i refleksyjne spawny dostają **identyczny neutralny prompt**. Dwa klony
refleksji w różnych cwd dostają ten sam dodatni limit. Porównania kwot używają tolerancji
dla `f64`. Jedynym rozróżnieniem jest stan klona z AC-1; model-based detection, prompt
sniffing, cwd sniffing i globalna flaga pozostawiają test czerwony.

<!-- OWNS
tasks/T-126.md
src-tauri/src/commands/run.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/src/engine/drivers/claude.rs
src-tauri/src/evidence.rs
src-tauri/src/ipc.rs
src-tauri/tests/it/a_run_leaves_suggestions.rs
src-tauri/tests/it/a_suggestion_needs_a_because.rs
src-tauri/tests/t126_private_reflection_receipt_and_evidence.rs
src-tauri/tests/t126_late_stop_and_empty_handoff.rs
src-tauri/tests/t126_budget_is_clone_state.rs
src/sections/run/index.tsx
src/sections/run/io.ts
src/sections/run/start.tsx
src/sections/run/launch.ts
src/sections/run/run-command.ts
src/sections/run/requested-launch.ts
src/sections/run/requested.ts
src/sections/run/reflection/toggle.tsx
e2e/tests/t126-reflection-choice-real-routes.spec.ts
-->
