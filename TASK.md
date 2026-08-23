# T-92 — Learningi mają producenta

Podsystem pamięci jest zbudowany od strony **czytnika** i nigdy nie został użyty od strony
**pisarza**. Stan zmierzony 2026-08-23 po 23 biegach właściciela:

- `memory::notes` ma `record_candidate`, `record_candidate_for`, `promote`, budżety, wymuszony
  wybór, blok `What you know` wpięty w prompt (`what_the_agents_know`, `what_this_step_knows`).
- `~/.loadout/memory/` **nie istnieje**. Tabela `memory` w SQLite ma 0 wierszy.
- `record_candidate*` ma wołających wyłącznie w testach; jedyny produkcyjny pisarz to importer
  (`import/apply.rs`). Nie ma komendy IPC, która tworzy albo odrzuca notatkę; mockup Pamięci ma
  przycisk „Discard", a `MemoryState` ma tylko `use`/`stopUsing`/`cancel`.
- `last_used_at` jest pisane raz jako `null` i nigdy aktualizowane — `notes.rs` mówi, że robi to
  „składanie promptu (T-15)", a nie robi; wymuszony wybór „najdawniej użyte" sortuje więc po id.
- W `system/init` każdego kroku Claude'a widać `memory_paths.auto:
  ~/.claude/projects/<projekt>/memory/` — auto-pamięć Claude Code jest **włączona** w krokach
  Loadouta i pisze do katalogu, który człowiek dzieli ze swoimi sesjami interaktywnymi. T6 §10.4
  nazwał przekierowanie jej per bieg (`--settings '{"autoMemoryDirectory": …}'`) „najlepszym
  leverem znalezionym w researchu"; `ClaudeDriver::with_settings` i `RunSettings::write` są
  gotowe i **nie mają wołającego** (`claude.rs`, ok. linii 841–865 opisuje dokładnie, jak je wpiąć
  i na czym stanęło).

Właściciel obszedł ten brak poza produktem: krok „Learings" na końcu workflow i agent
`learnings-extractor`, piszące do `.claude/learnings/` w repo gospodarza. Loadout tego nie czyta
(T-93). To zadanie daje pamięci pierwszego pisarza, który jest **w produkcie**, z dyscypliną
z T6 §5.3: jedna tania refleksja po biegu, najwyżej trzy kandydatki, każda z `because`, nigdy
`in-use`.

**Read first:** `src-tauri/src/memory/notes.rs` (`record_candidate_for`, `NoteDraft`, `because`,
`last_used_at`, `what_you_know`, `Status`) · `src-tauri/src/commands/memory.rs`
(`stop_using_note_inner` — tam jest wzór „dług: przenieść do notes.rs"; zrób `discard`
w `notes.rs` od razu) · `src-tauri/src/commands/run.rs` (`close_the_book`, `what_the_agents_know`
— tu stempluje się `last_used_at`; `Live::run_agent` — kolejność opakowań sterownika, tu wchodzi
`with_settings`) · `src-tauri/src/engine/drivers/claude.rs` (`with_settings`, `RunSettings`,
komentarz o fabryce `Arc<dyn AgentDriver>`) · `src-tauri/src/engine/drivers/host.rs`
(`deny_rules` — wpięcie `--settings` egzekwuje przy okazji `permissions.deny` gospodarza) ·
`src-tauri/src/engine/drivers/mod.rs` (`AgentDriver::inheriting`, `with_evidence` — wzór szwu
z domyślnym `None`) · `src-tauri/src/ipc.rs` (`put_note_to_use` — wzór komendy) ·
`src/sections/commands-wired.test.ts` (lustro komend — nowa komenda = nowy wiersz) ·
`src/state/memory.ts`, `src/sections/memory/index.tsx`, `note-row.tsx` ·
`docs/research/topics/T6-memory.md` §5.3, §10.4 · `AGENTS.md` niezmienniki 4, 9, 21.

## Kto to robi

- **Agent:** `rust-core` na AC-1…AC-5, potem `frontend` na AC-6 — jeden worktree, jedna bramka.
- **Druga opinia:** inny vendor niż pisarz (D3).

## AC-1 Po biegu zostają najwyżej trzy kandydatki, każda z powodem
check: cargo test --test it a_run_leaves_suggestions::
expect: (\d+) passed

Po `close_the_book` biegu, który ma co najmniej jedno przekazanie, Loadout uruchamia **jedną**
krótką turę refleksji (przez fabrykę sterowników, polityka tylko-do-odczytu, `cwd` = katalog
biegu, model z jednej stałej, limit czasu z jednej stałej) z prośbą o najwyżej trzy rzeczy
warte zapamiętania, każda jako wiersz `rule:` i wiersz `because:`. Każda poprawna para staje
się notatką `suggested` o zakresie `this-project`, z `from` = id biegu. Notatka powstaje przez
`record_candidate_for`, nigdy z `status: in-use`. Bieg bez przekazań nie woła refleksji wcale.
Na `FakeDriver`: trzy pary → trzy pliki; cztery pary → trzy pliki; zero → zero.

## AC-2 Kandydatka bez powodu nie powstaje, a powtórzenie nie tworzy drugiej
check: cargo test --test it a_suggestion_needs_a_because::
expect: (\d+) passed

Para bez `because` (albo z pustym) jest pominięta i policzona w dzienniku, nie zapisana.
Ta sama reguła z dwóch biegów daje jedną notatkę z `occurrences: 2`, bez zmiany `status`
(auto-promocja jest unieważniona przez ARCHITECTURE §2 pyt. 5 — to zostaje).

## AC-3 Notatkę sugerowaną da się odrzucić, a używaną — nie tą drogą
check: cargo test --test it a_suggestion_can_be_discarded::
expect: (\d+) passed

`memory::notes::discard(root, id, by)` usuwa plik notatki `suggested` (przenosi do
`<root>/discarded/` z datą, nie kasuje — nic nie jest twardo usuwane, T6 §5.3) wyłącznie dla
`Actor::You`; notatka `in-use` jest odmową ze zdaniem („Stop using it first"). Komenda IPC
`discard_note` jest zarejestrowana i widoczna w lustrze komend. Przy okazji `stop_using_note_inner`
przenosi się do `notes.rs` obok `promote` — dług z nagłówka `commands/memory.rs` spłacony.

## AC-4 Notatka pamięta, kiedy ostatnio weszła do promptu
check: cargo test --test it a_note_remembers_when_it_was_used::
expect: (\d+) passed

Kiedy blok `What you know` zawiera regułę notatki, Loadout stempluje `last_used_at` w jej pliku
(jedna linia, jak `promote` zmienia `status`). Notatka `in-use` poza budżetem (`Block::dropped`)
nie dostaje stempla. Wymuszony wybór przy pełnej pamięci sortuje wtedy naprawdę po użyciu:
notatka użyta wczoraj stoi za notatką użytą nigdy.

## AC-5 Auto-pamięć Claude'a pisze do katalogu biegu i staje się kandydatkami
check: cargo test --test it claude_memory_stays_in_the_run::
expect: (\d+) passed

Krok Claude'a dostaje `--settings <plik>` z `autoMemoryDirectory = <katalog biegu>/mem/<krok>`
i `autoMemoryEnabled = true` (plus reguły `permissions.deny` gospodarza z `host::deny_rules`,
bo ten sam plik), przez szew `AgentDriver::with_settings` z domyślnym `None` — Codex zwraca
`None` i nie dostaje nic. Po turze pliki tematyczne z `mem/<krok>/` (bez `MEMORY.md`) stają się
notatkami `suggested` o zakresie `this-agent` z `agent` = nazwa agenta i `because` = zdanie
o pochodzeniu (bieg, krok). Katalog użytkownika `~/.claude/projects/…/memory/` nie jest
dotykany. Jeśli szew na traicie nie wystarczy, bo `--settings` wymaga typu konkretnego —
**stój i zgłoś**, nie ruszaj fabryki w `lib.rs`.

## AC-6 Ekran Pamięć ma „Discard" przy notatce sugerowanej
check: npx --no-install vitest run src/sections/memory/suggested-can-be-discarded.test.tsx
expect: (\d+) passed

Wiersz notatki `suggested` ma obok „Use this" przycisk „Discard", który woła `discard_note`
i po odpowiedzi (bez optymistycznej aktualizacji, jak reszta ekranu) usuwa wiersz; wiersz
`in-use` tego przycisku **nie ma**. Markup przez `renderToStaticMarkup`, zachowanie przez akcję
store'a z `vi.mock` na IPC.

## Sprzątanie po drodze

`notes.rs` ok. linii 230 („stempluje T-15") — popraw na to, co robi to zadanie. Komentarz
w `claude.rs` ok. linii 841–865 o braku wołającego `with_settings` przestaje być prawdą.

<!-- OWNS
tasks/T-92.md
src-tauri/src/memory/mod.rs
src-tauri/src/memory/notes.rs
src-tauri/src/commands/memory.rs
src-tauri/src/commands/run.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/src/engine/drivers/claude.rs
src-tauri/src/engine/drivers/codex.rs
src-tauri/src/ipc.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/a_run_leaves_suggestions.rs
src-tauri/tests/it/a_suggestion_needs_a_because.rs
src-tauri/tests/it/a_suggestion_can_be_discarded.rs
src-tauri/tests/it/a_note_remembers_when_it_was_used.rs
src-tauri/tests/it/claude_memory_stays_in_the_run.rs
src/sections/commands-wired.test.ts
src/ipc/run.ts
src/state/memory.ts
src/sections/memory/index.tsx
src/sections/memory/io.ts
src/sections/memory/note-row.tsx
src/sections/memory/suggested-can-be-discarded.test.tsx
-->
