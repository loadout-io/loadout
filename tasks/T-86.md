# T-86 — Każdy krok agenta wie, jak oddać wynik i ile ma czasu

Loadout ma wobec agenta bardzo konkretne oczekiwania i **nie mówi mu żadnego z nich**.
Przekazanie powstaje z **ostatniej wypowiedzi** agenta (`Live::one_turn` → `hand_over(id,
&turn.text, reads)` w `commands/run.rs`), jest cięte na 8 KB (`memory/handoff.rs`, `BODY_CAP`)
z przeniesieniem reszty do `attachments/`, a `reshape()` dopisuje brakujące nagłówki
`## Answer / ## Evidence / ## Open`. Jedyny agent, który dostaje zdanie o tym, jak ma odpowiedzieć,
to sędzia pętli (`OUTCOME_ASKED_FOR`, dopisane 2026-08-23) — i to zdanie powstało dopiero po tym,
jak osiem biegów właściciela przepaliło komplet rund, bo nikt nie poprosił o wiersz `outcome:`.

Koszt drugiej połowy tej luki jest zmierzony w transkryptach biegu `20260823-145648`
(`~/Projects/urc-monorepo/.loadout/runs/`): **sześć** kroków Claude'a zaczyna podsumowanie od
„*Write access is disabled in this session, so I can't create the handoff file — the findings are
below*". Agent spalił tury na próbę zapisania pliku wyników, bo jego instrukcje (importowane
z `ship-task`) każą pisać do `.claude/tmp/`, a dial `look-only` to blokuje. Gdyby wiedział, że
jego odpowiedź **jest** przekazaniem, nie próbowałby wcale.

Trzecia rzecz, której agent nie wie: ile ma czasu. `give_up_after` zabija krok po limicie
(`stop_overdue_agent`), ale do promptu nie wchodzi — agent planuje 60-minutową robotę w kroku,
który ma 10 minut, i ginie w połowie bez jednego zdania w przekazaniu.

**To zadanie dodaje do promptu każdego kroku agenta jeden stały blok po angielsku** — ten sam
dla obu vendorów, obok istniejącego indeksu przekazań — i zapisuje do `run.json`, czy agent się
go trzymał. Nie zmienia ani formatu przekazania, ani `reshape()`, ani sędziego.

**Read first:** `src-tauri/src/commands/run.rs` (`prompt_for`, `ask_for_an_outcome`,
`OUTCOME_ASKED_FOR`, `HANDOFF_INDEX_OPENS` — tam mieszkają stałe promptu i tam wchodzi nowa;
`AgentJob::give_up_after`; `run_file` / `StepEntry`) · `src-tauri/src/memory/handoff.rs`
(`Written { repaired, truncated }`, `reshape`, `cap`) · `AGENTS.md` niezmienniki 14, 21, 28 ·
`docs/research/projects/00-SYNTHESIS.md` §2.2 (słowa, których nie wolno użyć w zdaniu do agenta:
„handoff", „verdict" — piszemy „what you pass on", „the outcome line").

## Kto to robi

- **Agent:** `rust-core`
- **Druga opinia:** inny vendor niż pisarz (D3).

## Zanim napiszesz pierwszą specyfikację

Kryterium woła `cargo test --test it <modul>::`; nowy plik wymaga linii `mod <nazwa>;`
w `src-tauri/tests/it/main.rs` (jest w OWNS). Prompt bierzesz tak, jak robią to
`inherit_reaches_the_prompt.rs` i `skills_reach_the_step.rs`: przez `FakeDriver`, który
zatrzymuje `RunSpec`. Nie przez odczyt transkryptu.

## Poszerzenie zakresu — decyzja właściciela, 2026-08-23, po pierwszym biegu

Pierwszy bieg skończył się tak: **wszystkie trzy kryteria zielone**, `full-test` czerwony na
jednej asercji w pliku, którego to zadanie nie posiadało, i jedna uwaga recenzenta Codeksa na
zielonych kryteriach. Właściciel rozstrzygnął obie sprawy. To jest poszerzenie **uprawnień**;
kryteria są bajt w bajt te same, co w kontrakcie certyfikowanym na gałęzi (porównane
mechanicznie: linie `## AC-`, `check:`, `expect:`).

### 1. `product_path_end_to_end.rs` wchodzi do OWNS z jednym, wąskim mandatem

Wolno ci w tym pliku zmienić **wyłącznie formę** asercji promptu w teście
`a_saved_agent_a_saved_workflow_and_a_run_that_actually_ran` (ok. linii 164). Dziś żąda ona
równości całego promptu z `WHAT_TO_DO`; ma żądać trzech rzeczy naraz: **jeden** prompt, prompt
**zaczyna się** od `WHAT_TO_DO`, i zawiera go **dokładnie raz**.

**Zdanie tej asercji zostaje słowo w słowo.** Ono jest po twojej zmianie nadal prawdziwe:
instrukcja człowieka dociera do sterownika dosłownie i jeden raz — stoi na początku promptu,
przed twoim blokiem. Nieprawdziwa robi się tylko forma równości.

Reszta tego pliku jest cudza (należy do T-34): nie dopisuj asercji, nie zmieniaj fikstury,
nie tykaj drugiego testu w tym pliku.

**Obejście, które przechodzi naiwną wersję tej zmiany i którego nie wolno ci zrobić:** samo
`assert!(prompts[0].contains(WHAT_TO_DO))`, bez `starts_with` i bez liczby wystąpień.
Przepuszcza prompt, w którym zdanie człowieka jest doklejone na końcu albo dwa razy — czyli
dokładnie ten defekt, po którym tamtą asercję napisano: pusty `instructions` daje bieg, który
kończy się `Ok` i wygląda identycznie jak bieg udany.

### 2. `giveUpAfterMinutes: 0` ma znaczyć brak limitu w SILNIKU, nie tylko w prompcie

Znalazł to recenzent Codeksa na zielonych kryteriach: `plan_agent` liczy dziś
`give_up_after_minutes.max(1) * 60`, więc zero to w rzeczywistości **jedna minuta**, a krok
ginie przez `Ended::Overdue`. AC-2 każe przy zerze powiedzieć agentowi, że limitu nie ma —
prompt obiecywałby więc coś, czego produkt nie dotrzymuje, a to jest gorsze niż brak zdania.

Zero ma dawać `Duration::MAX`, tak jak dostają je dziś punkt kontrolny i krok „sprawdź".
`run.rs` masz w OWNS od początku, więc to mieści się w zakresie. **AC-2 nie zmienia się ani
o słowo** — zmienia się to, czy jego zdanie jest prawdziwe.

## AC-1 Każdy krok agenta dostaje zdanie o tym, że jego odpowiedź jest tym, co przekazuje dalej
check: cargo test --test it every_step_is_told_how_to_answer::
expect: (\d+) passed

Prompt **każdego** kroku agenta — nie tylko sędziego — kończy się stałym blokiem, który mówi
co najmniej trzy rzeczy: że ostatnia wiadomość agenta jest tym, co następny krok przeczyta;
że ma użyć nagłówków `## Answer`, `## Evidence`, `## Open` w tej kolejności; że **nie** ma
zapisywać wyników do plików, bo Loadout zrobi to sam. Blok jest jedną stałą obok
`OUTCOME_ASKED_FOR`, stoi **po** indeksie przekazań i **przed** zdaniem sędziego, a krok
sędziego dostaje oba. Krok bez poprzedników dostaje blok tak samo jak krok z trzema.

Kontrola przeciw słabej asercji: kryterium porównuje **zmontowany tekst**, nie stałą — bieg,
w którym stała istnieje, a `prompt_for` jej nie dokleja, ma być czerwony.

## AC-2 Agent wie, ile ma minut, i wie, kiedy nie ma limitu
check: cargo test --test it the_step_knows_its_deadline::
expect: (\d+) passed

Ten sam blok nazywa limit czasu kroku liczbą minut z efektywnej definicji (agent + nadpisanie
kroku), a przy `giveUpAfterMinutes: 0` mówi wprost, że limitu nie ma — nie pisze „0 minutes".
Dwa kroki z różnymi limitami w jednym biegu dostają dwie różne liczby.

## AC-3 Bieg zapisuje, czy agent trzymał się umowy
check: cargo test --test it run_json_records_handoff_repairs::
expect: (\d+) passed

`run.json` dostaje na kroku agenta pole mówiące, co Loadout musiał zrobić z odpowiedzią:
które sekcje dopisał (`repaired`) i czy ciął (`truncated`). Agent, który oddał trzy sekcje
w dobrej kolejności i zmieścił się w limicie, ma tam pusto i `false`; agent, który oddał gołą
prozę, ma wymienione dopisane nagłówki. Pole jest addytywne (`#[serde(default)]`,
`skip_serializing_if` przy pustym), więc stare pliki `run.json` czytają się bez zmian,
a `store::rebuild` ich nie traci. Do dziś te dwie liczby szły wyłącznie do `tracing::debug!`,
czyli nikt ich nie widział (niezmiennik 21).

## Sprzątanie po drodze

Nagłówek modułu `commands/run.rs` (ok. linii 119) twierdzi, że bieg „nie tee'uje surowego
strumienia do `logs/agent-<id>.jsonl`". Od T-34 robi to `evidence.rs` i pliki leżą na dysku
w każdym biegu właściciela. Popraw to zdanie w tym zadaniu — plik jest w OWNS, a nieaktualny
nagłówek uczy następnego pisarza nieprawdy.

<!-- OWNS
tasks/T-86.md
src-tauri/src/commands/run.rs
src-tauri/src/memory/handoff.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/every_step_is_told_how_to_answer.rs
src-tauri/tests/it/the_step_knows_its_deadline.rs
src-tauri/tests/it/run_json_records_handoff_repairs.rs
src-tauri/tests/it/product_path_end_to_end.rs
-->
