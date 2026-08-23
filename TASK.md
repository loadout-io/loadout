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
-->
