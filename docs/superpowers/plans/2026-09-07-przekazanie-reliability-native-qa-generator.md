# Przekazanie: niezawodność workflow, QA na pełnej aplikacji i generator agentów

Data: 2026-09-07. Odpowiada na §12 planu
[`2026-09-06-reliability-native-qa-generator.md`](2026-09-06-reliability-native-qa-generator.md).

To jest **raport wykonania**, nie plan. Wszystko poniżej da się sprawdzić poleceniem, które stoi
obok liczby.

## 1. Gdzie to leży

| | |
|---|---|
| Repo | `/Users/jakubgawronski/Projects/loadout-reliability-native-qa-generator` (worktree Loadouta) |
| Gałąź | `feat/reliability-native-qa-generator` |
| Baza | `f81d4b11` — snapshot WIP z `workflow-reliability-build`, wierny co do pliku |
| HEAD | `89aa8490`, czternaście commitów nad bazą |
| Niezacommitowane | **brak** (`git status` pusty) |
| Nie zmergowane | nic z tego nie jest na `main`; niczego nie opublikowano, niczego nie wypchnięto |

Źródłowy worktree `/Users/jakubgawronski/Projects/loadout-workflow-reliability-build` **nie został
tknięty**: snapshot powstał z `git diff HEAD --binary` plus kopii plików nieśledzonych, a `diff -r`
między drzewami różni się wyłącznie o `src-tauri/gen` (artefakt buildu).

### Baza NIE jest zielona i nigdy nie była

Odziedziczony WIP ma **35 czerwonych testów** i **81 ostrzeżeń clippy**. Zmierzone na osobnym
worktree stojącym na samym `f81d4b11` (`/Users/jakubgawronski/Projects/loadout-baseline-check`),
żeby nie było wątpliwości, czyje to jest. Czerwień dotyczy sprzątania kopii roboczych, transakcji
prestartu i izolacji — obszarów, których to zadanie nie tyka.

Po całej pracy: **1639 passed, 35 failed** i **81 ostrzeżeń**. Lista porażek jest identyczna co do
nazwy (`diff` pusty), a ostrzeżeń jest dokładnie tyle samo. Zero regresji, zero dołożonych
ostrzeżeń.

## 2. Co zrobione, a co nie

| ID | Stan | Czym to sprawdzić |
|---|---|---|
| L-00 | zrobione | snapshot `f81d4b11`, worktree, `diff -r` |
| L-01 | zrobione | `cargo test --test it step_turn_queue_is_atomic::` → 5/5 |
| L-02 | zrobione | `cargo test --test it a_saved_end_cause_beats_a_guess::` → 5/5 |
| V-01 | zrobione | `cargo test --test it required_checks_run_the_requested_tests::` → 11/11 |
| V-02 | zrobione | `cargo test --test it mandatory_criteria_survive_the_summary::` → 11/11 |
| V-03 | zrobione | `cargo test --test it the_shipped_workflow_checks_before_it_believes::` → 5/5 |
| P-01 | zrobione w części I-08 | `cargo test --test it a_neighbour_repository_is_named_before_the_run::` → 4/4 |
| P-02 | zrobione w części izolacji danych | `cargo test --test it a_test_instance_keeps_its_data_apart::` → 4/4 |
| P-03a | **rozpoznane i zamknięte werdyktem** | `cargo test --test it native_scenarios_need_a_real_route_to_the_window::` → 6/6 + 1 żywa `--ignored` |
| P-03b | **NIE zrobione** | powód w §5 |
| G-01, G-02 | zrobione | `cargo test --test it an_agent_is_written_by_the_vendor_that_was_asked::` → 11/11 |
| G-03 | zrobione | `npx vitest run e2e/tests/two-buttons-ask-two-different-vendors.spec.ts` → 2/2 w chromium |
| G-04 | zrobione w części rozróżnienia | `npx vitest run src/sections/agents/an-agent-can-be-written-from-a-description.test.tsx` → 4/4 |
| M-01…M-04 | **NIE zrobione** | powód w §5 |
| E-01 | macierz zrobiona, pełny odbiór **nie** | §4 |

Front: **2043 passed, 5 failed** — te same pięć, które są czerwone na bazie.
Checki repo: `boundary`, `vocabulary`, `tests-listed`, `invoke-args`, `wired`, `suppressions`,
`tokens` — 7/7. `density` jest czerwona **także na bazie**: mówi „could not measure", bo kolektor
nie jest wpięty w ten check.

## 3. Co się naprawdę zmieniło, incydent po incydencie

**I-01 — wiadomość ginęła razem z turą.** Kolejka tur należy dziś do Loadouta
(`SessionChannel::waiting`), a nie do vendora. Sprawdzenie pustej kolejki i zamknięcie
przyjmowania to **jedno wzięcie zamka** (`RunControl::next_turn_or_stop_accepting`) — dwie
operacje zostawiały okno, w którym wiadomość jest przyjęta i nigdy nie podana. Sufit czasu
należy do kroku, nie do tury: mutacja `limit` zamiast `limit - elapsed` czerwieni kryterium
natychmiast. Zużycie sumuje się przez wszystkie obsłużone tury, dokładnie raz.

**I-02 — `infrastructure-failed` czytane jako `unknown`.** Zapisany `end_cause` wygrywa z całą
heurystyką opartą o kod wyjścia i obecność artefaktów. Stary raport bez tego pola zachowuje swoją
dotychczasową odpowiedź. Lead widzi ten sam fakt w `read_run_summary`.

**I-03/I-04 — dwa niezwiązane testy zamiast trzynastu.** Check potwierdza testy **po nazwie**
(`required::Scan`, adaptery libtest i TAP), a nie po liczniku. Sądzone STRUMIENIOWO i wyłącznie po
`stdout`: zachowywany ogon ma 64 KiB, więc sądzenie po nim kłamałoby o każdej prawdziwej suicie,
a `stderr` mógłby podszyć się pod wynik. Katalog roboczy sprawdzany PRZED spawnem i nazwany
w zdaniu.

**I-05 — proza opisuje braki, ostatni wiersz mówi `pass`.** Kiedy człowiek zatwierdził listę
wymagań, wynik powstaje z jej kompletności, a nie ze słowa na końcu. Trzy wyniki zamiast dwóch:
niezmierzone kryterium nie jest ani zaliczeniem, ani dowodem wady.

**I-06/I-07 — QA bez narzędzia do okna i tak wydało werdykt.** Kryterium wymagające pełnej
aplikacji, zameldowane jako zdane na maszynie bez zgody na sterowanie oknem, przechodzi do
„nie zmierzono" — **kodem**, nie prośbą w prompcie.

**I-08 — zależność od sąsiedniego repo.** Krok pracujący we własnej kopii słyszy, zanim ruszy
pierwszy proces, że `src-tauri/Cargo.toml` potrzebuje `../murmur-server`, którego w kopii nie
będzie — razem z obiema drogami wyjścia.

## 4. Macierz regresji z §11

| Próba | Gdzie jest sądzona |
|---|---|
| Wiadomość dokładnie przy pierwszym result | `step_turn_queue_is_atomic::queued_messages_get_their_own_turns_before_the_session_closes` |
| Wiadomość do wcześniejszej próby/node w innym runie | `step_turn_queue_is_atomic::a_misaddressed_message_never_reaches_a_live_session` |
| 0 testów / niewłaściwe testy / stary wynik | `required_checks_run_the_requested_tests::` (11 przypadków) |
| QA pisze pass przy brakującym obowiązkowym scenariuszu | `mandatory_criteria_survive_the_summary::a_pass_line_cannot_outvote_a_requirement_it_never_answered` |
| UI działa na mocku, kryterium wymaga backendu | `mandatory_criteria_...::a_mocked_confirmation_does_not_satisfy_a_runtime_requirement` |
| Obca aplikacja zajmuje port | odziedziczone: `probe_owned_endpoint` + `supervisor::listener_owner` |
| Poprawka zmienia źródła po QA | odziedziczone: `boundary.validate()` + `input_for_check` w `commands::run` |
| Generowanie dla dwóch vendorów | `an_agent_is_written_...::the_chosen_app_runs_it_with_nothing_to_read_and_nothing_to_write` + e2e |
| Model żąda zapisu configu, sekretów lub większych praw | `an_agent_is_written_...::a_generated_agent_cannot_widen_itself_through_raw_settings` |
| Cancel generation równolegle do workflow | `an_agent_is_written_...::cancelling_one_generation_ends_only_that_one` |
| Brak CLI, loginu, uprawnienia | `an_agent_is_written_...::a_missing_app_says_so`, `native_scenarios_...::` |
| Próba zmiany egzaminatora przez mierzony bieg | odziedziczone: `processes_cannot_rewrite_their_examiner` |

## 5. Czego NIE zrobiłem i dlaczego — bez chowania pod „non-blocking"

**P-03b — QA nie obsługuje jeszcze okna w biegu.** Droga jest rozpoznana i zmierzona (§6), ale
scenariusze natywne nie są jeszcze wykonywane przez krok QA. Brakuje trzech rzeczy, każda
nazwana: przekazania kroku QA tożsamości uruchomionej instancji (PID), potwierdzenia okna po
stronie readiness i zapisu dowodów pomocniczych. Do tego czasu **status native QA to
„nie zmierzono"** — i to jest egzekwowane kodem, a nie deklarowane.

**Potwierdzenie okna dla celu natywnego (P-02 punkt 6).** `wait_until_ready` nie dostaje dziś
opisu uruchomienia, więc `TargetKind` tam nie dociera. Sonda okna czekałaby bez wołającego,
a `checks/wired.sh` słusznie by to złapał. Nazwane zamiast wpół wpięte.

**Kafelek „uruchom i zostaw" nie umie powiedzieć, że startuje aplikację natywną.**
`kind`/`testDataEnv` stoją na `LaunchDescription` (opis od agenta), a `ServeStep` ich nie ma.
Dopisanie ich bez kontrolki w panelu byłoby polem, którego nikt nie ustawi (niezmiennik 16).

**„Sesja bez ani jednego narzędzia" jest dziś niewyrażalna.** Sprawdzone w API sterowników,
zgodnie z poleceniem planu: u Claude'a pusta lista narzędzi jest w tym drzewie ODMOWĄ
(`ToolsRefused::NothingChosen`), a Codex nie ma listy narzędzi w ogóle. Ograniczenie, które
działa u OBU i jest egzekwowane, składa się z trzech rzeczy naraz: polityka tylko-do-odczytu,
wyłączona sieć i **pusty katalog roboczy**. Żadnego fallbacku do `Everything`.

**Zweryfikowany katalog modeli nie istnieje.** `Available::models` jedzie dziś pusta, co znaczy
„nie sprawdzamy" — i model wskazany przez generatora przechodzi, a widzi go człowiek. Stara
statyczna lista z formularza nie jest dowodem bieżącej dostępności, więc jej nie użyłem.

**M-01…M-04 (Murmur) — nie zaczęte.** To osobne repozytorium z własnymi regułami, a w pliku,
który M-01 ma zmieniać (`src-tauri/src/storage/processing_queue_store.rs`), leży czyjaś
niezacommitowana zmiana. Ta sama dyscyplina, którą plan nakłada w L-00, mówi tu: uzgodnij bazę
z właścicielem, zanim cokolwiek ruszysz.

**Pełny odbiór produktu z §11 — nie wykonany.** Wymaga płatnych biegów i zgody, a plan wprost
mówi, że sam ich nie zleca.

## 6. Czego potrzebuję od człowieka

1. **Zgód macOS dla aplikacji Loadout**: Automation (sterowanie „System Events") i Accessibility.
   Zmierzone 2026-09-06: `osascript` + `System Events` na tej maszynie **działa** — odczyt
   `count windows` procesu wskazanego przez `unix id` odpowiada poprawnie, a żywa sonda z drzewa
   testowego mówi `Ready`. Ale zgody macOS są przypisane do BINARKI: „Ready" zmierzone z procesu
   testowego nie jest zgodą dla aplikacji Loadout. Ta poprosi o nią własnym oknem systemowym przy
   pierwszym użyciu i tylko człowiek może ją dać.
2. **Decyzji o torze M** — czy ruszać meetnotes, mając w docelowym pliku czyjąś niezacommitowaną
   zmianę.
3. **Zgody na płatne biegi**, jeżeli §11 ma zostać wykonane naprawdę.

Czego **nie** potrzebuję i czego nie ruszałem: aktywnej biblioteki użytkownika (`~/.loadout`),
restartu aplikacji, instalacji połączeń, mikrofonu, publikacji.

## 7. Jedna granica, o którą się otarłem i wycofałem

Próbowałem zrobić tak, żeby nierozstrzygalna trasa nie puszczała OBU gałęzi naraz przy
ustawieniu `carry-on`. Zapaliły się trzy kryteria T-101
(`a_blocked_way_out_takes_the_chosen_path`, `every_failure_shares_one_door`), które mówią
wprost: `Route::Blocked` jest porażką kroku przechodzącą tymi samymi drzwiami, co każda inna —
routing dostarcza dokładny powód, a `when_it_fails` wybiera skutek. To jest **rozstrzygnięte
kryterium, nie luka**, więc poprawkę wycofałem, a szablon z V-03 ustawia na tym kafelku `stop`.
Nowe jest wyłącznie to, że wynik BEZ POMIARU (`CheckOutcome::NotJudged`) w ogóle dociera do
wyboru drogi jako własna wartość, zamiast udawać zaliczenie albo wadę.

## 8. Trzy wady, które znalazłem we własnej pracy, zanim znalazł je ktoś inny

1. **Sądzenie wymaganych testów po zebranym tekście.** Krok „sprawdź" zachowuje ostatnie 64 KiB,
   więc każdy test prawdziwej suity sprzed ogona wyglądałby na niewykonany. Poprawione na skan
   strumieniowy; kryterium dowodzi własnej przesłanki — asertuje, że wymaganej linii NIE MA
   w zachowanym wyjściu, i mimo to jest potwierdzona.
2. **Pompa zdarzeń generatora czekała na zamknięcie kanału, którego nadajnik trzymał żywy
   uchwyt.** Cały cel testowy wisiał kilkanaście minut. Uchwyt schodzi przed czekaniem.
3. **`setState` rodzica wewnątrz funkcji aktualizującej stan.** React wykonuje ją w trakcie
   renderu; prawdziwa przeglądarka zgłaszała to jako błąd, a w StrictMode szkic wchodziłby dwa
   razy. Aktualność operacji rozstrzyga dziś `useRef`, poza renderem.

## 9. Jak to obejrzeć

```bash
cd /Users/jakubgawronski/Projects/loadout-reliability-native-qa-generator

# każde kryterium tego zadania, jedno po drugim
cargo test --manifest-path src-tauri/Cargo.toml --test it step_turn_queue_is_atomic::
cargo test --manifest-path src-tauri/Cargo.toml --test it required_checks_run_the_requested_tests::
cargo test --manifest-path src-tauri/Cargo.toml --test it mandatory_criteria_survive_the_summary::

# co ta maszyna naprawdę potrafi z natywnym oknem
cargo test --manifest-path src-tauri/Cargo.toml --test it native_scenarios -- --ignored --nocapture

# oba przyciski, prawdziwe kliknięcie, prawdziwa przeglądarka
npx --no-install vitest run e2e/tests/two-buttons-ask-two-different-vendors.spec.ts
```

Szablon i rola do obejrzenia przed importem:
`.loadout/workflows/verified-change.json`, `.loadout/agents/verifies-the-running-app.md`.
Kroki `s_plan`, `s_backend`, `s_frontend`, `s_combine` mają w polu `agent` napis
`REPLACE_WITH_…`: to jest miejsce na twoje role, a nie identyfikator.
