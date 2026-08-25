# T-115 — Wydatki obu vendorów: pełne zastępstwo T-102 z wyrocznią, która odróżnia kolumny i sumę

T-102 pozostaje dowodem, nie źródłem kodu. Formalnie przeszło 20/20, lecz recenzent znalazł
dwie implementacje błędne względem zadania, które jego specy nadal zazieleniały:

1. Terra i Luna dostawały po milionie tokenów wejścia, cache i wyjścia. Zamiana dowolnych
   dwóch stawek zachowuje sumę i przechodzi test.
2. Prawdziwy ekran dostawał jeden płatny wiersz. Kod pokazujący tylko pierwszy albo tylko
   ostatni koszt przechodzi test „sumy biegu".

Jedyna runda naprawy T-102 poprawiła lint i odmówiła zmiany zamrożonych wyroczni. Ta gałąź
startuje z aktualnego `main`. **Nie przenosi commitów, testów ani implementacji z
`task-T-102`.** Zachowuje cel produktu, lecz cztery nowe ścieżki testów muszą od początku
dać uczciwe runtime-red `before`.

Decyzja D-5 pozostaje bez zmiany: właściciel używa subskrypcji obu vendorów, więc dolary są
analityką. Twardy stop istnieje tylko wtedy, gdy człowiek jawnie ustawił kwotę przy Starcie.
Nie zmieniaj wzoru miękkiego budżetu ×N ani zachowania T-94.

**Read first:** `tasks/T-102.md` wyłącznie jako opis celu i zamkniętej pułapki ·
`src-tauri/src/engine/drivers/codex.rs` (`app_usage`, obecne `cost_usd: None`) ·
`src-tauri/src/engine/drivers/mod.rs` (`Outcome`, domyślne zdolności traitu) ·
`src-tauri/src/commands/run.rs` (`one_turn`, `spent_in`, `prompt_for`,
`HANDOFF_INDEX_CLOSES`) · `src-tauri/src/engine/line.rs` (`Line::Done`) ·
`src/sections/run/strip/model.ts` (`stripFor`, `spendFor`) ·
`src/sections/run/index.tsx` (produkcyjne wywołanie `stripFor`) · `AGENTS.md`
niezmiennik 29 (zdanie i suma muszą zostać sprawdzone tam, gdzie widzi je człowiek).

## Kto to robi

- **Agent:** jeden bieg Harnessu, Codex jako pisarz; Rust i frontend w tym samym worktree.
- **Druga opinia:** osobne wywołanie Codex na innym modelu, zgodnie z jawną decyzją właściciela.

## Mandat na literały

Jeżeli implementacja dokłada pole do `Line::Done`, wolno zaktualizować wyłącznie niezbędne
literały w istniejących testach drutu i fiksturach wymienionych w OWNS. Mandat nie obejmuje
osłabiania ich asercji. Najpierw rozważ nośnik bez nowego pola, np. oznaczenie szacunku tylko
w `run.json`, jeżeli cały kontrakt pozostaje wtedy prawdziwy.

## AC-1 Każdy znany model ma trzy rozróżnialne stawki, a bieg sumuje szacunek
check: cargo test --test it t115_codex_prices_keep_token_columns_distinct::
expect: (\d+) passed

Prawdziwy adapter `exec --json` emituje dla **każdego** znanego prefiksu modelu nierówne
liczniki: 10 000 wejścia, 5 000 cache i 20 000 wyjścia. Oczekiwane koszty to dokładnie
Sol `$0.442`, Terra `$0.261`, Luna `$0.0261`; wersjonowany sufiks co najmniej jednego modelu
także trafia po prefiksie. Test ma być tak zbudowany, żeby zamiana dowolnych dwóch kolumn
cennika dla Terra albo Luna była czerwona — sama suma stawek dla miliona każdego typu jest
zakazana jako wyrocznia.

Prawdziwy `run.json` zapisuje tokeny, kwotę oraz oznaczenie „estimate" dla kroku Codeksa;
`spent_in` uwzględnia tę kwotę. Kontrola w tym samym biegu: zmierzona kwota kroku Claude'a
nie dostaje oznaczenia szacunku. Tabela cen istnieje w jednym miejscu produkcji.

## AC-2 Prawdziwy ekran pokazuje sumę co najmniej dwóch płatnych kroków
check: npx --no-install vitest run src/sections/run/strip/t115-spend-sums-both-vendors-on-screen.test.tsx
expect: (\d+) passed

Test montuje produkcyjną sekcję Run i zasila ją co najmniej dwoma zakończonymi liniami z
różnymi, niezerowymi kosztami — jedną odpowiadającą Codeksowi i jedną Claude'owi. Oczekiwany
tekst paska jest ich sumą i **nie może być równy ani pierwszej, ani ostatniej kwocie**.
Bez ustawionego limitu ekran pokazuje samą sumę, bez `of`; z limitem pokazuje `X of Y`.
Kontrola: same nieznane koszty nie pokazują `$0.00`.

Bezpośrednie wywołanie `spendFor()` albo `stripFor()` nie spełnia AC: asercja ma przejść
przez produkcyjne `index.tsx`, bo zadanie naprawia właśnie brakujące podłączenie do ekranu.

## AC-3 Nieznany model zachowuje tokeny i mówi człowiekowi, że ceny nie zna
check: cargo test --test it t115_unknown_codex_price_stays_unknown::
expect: (\d+) passed

Model spoza jedynej tabeli zachowuje wszystkie trzy liczniki w `run.json`, nie dostaje kwoty
ani fałszywego oznaczenia pomiaru/szacunku. Produkcyjny końcowy wiersz kroku niesie jedno
angielskie zdanie, że cena tego modelu nie jest znana. Test sprawdza zserializowany wiersz,
który trafia do UI, nie wyłącznie prywatny wynik funkcji. Nigdzie nie pojawia się `$0.00`.

## AC-4 Krok Codeksa dostaje otwieralne adresy plików, a prompt Claude'a się nie zmienia
check: cargo test --test it t115_codex_handoff_paths_are_actionable::
expect: (\d+) passed

Nowa zdolność sterownika ma domyślną wartość „dodatkowe katalogi są przenoszone"; Codex
jawnie mówi „nie". Prawdziwie zmontowany prompt kroku Codeksa dodaje jedno angielskie zdanie,
że wymienione pliki leżą poza katalogiem pracy i trzeba czytać je po podanych pełnych
ścieżkach. Test otwiera co najmniej jedną z tych ścieżek z katalogu pracy kroku. Kontrola:
prompt tego samego kroku na sterowniku Claude'a pozostaje bajt w bajt taki jak przed zadaniem.

<!-- OWNS
tasks/T-115.md
src-tauri/src/commands/run.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/src/engine/drivers/codex.rs
src-tauri/src/engine/line.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/t115_codex_prices_keep_token_columns_distinct.rs
src-tauri/tests/it/t115_unknown_codex_price_stays_unknown.rs
src-tauri/tests/it/t115_codex_handoff_paths_are_actionable.rs
src-tauri/tests/it/codex_steps_report_their_tokens.rs
src-tauri/tests/it/stream_closing_lines.rs
src-tauri/tests/it/stream_collapse_defaults.rs
src-tauri/tests/it/stream_curation_fixture.rs
src-tauri/tests/it/ipc_line_wire_golden.rs
src-tauri/tests/it/driver_codex_finish.rs
src/ipc/line-wire.golden.json
src/ipc/types.ts
src/sections/run/feed/fixtures/lines.ts
src/sections/run/strip/model.ts
src/sections/run/strip/t115-spend-sums-both-vendors-on-screen.test.tsx
src/sections/run/strip/strip.test.ts
src/sections/run/index.tsx
-->
