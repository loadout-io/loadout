# T-79 — Wybrane skille docierają do uruchomionego modelu

Skille mają dziś pełną drogę na dysk i zero drogi do biegu. `Agent.skills` jest polem formularza
agenta, `AgentStep.skills` jest polem pliku workflow o gotowej semantyce (`"all"` albo lista),
`~/.loadout/skills/<nazwa>/` jest kanoniczną kopią — a poza modułem importu **nikt tych pól nie
czyta**. Jedyny wołający dziedziczenia stoi w `commands/run.rs:2890` i podaje `Chosen::default()`,
czyli pusty wybór. To jest ta sama klasa, którą niezmiennik 29 nazywa wprost: kryterium zielone,
funkcja martwa — a z zewnątrz „agent nie zna tej umiejętności" jest nieodróżnialne od „model nie
uznał, że warto po nią sięgnąć".

**Semantyka, bez nowego typu.** Zbiór efektywny liczy się tak, jak liczy się cała reszta definicji
agenta: `library::agents::resolve()` scala patchem RFC 7396, więc brak klucza na kroku znaczy
„weź to, co ma agent", `[]` znaczy „żadnych", a lista znaczy podzbiór **skilli tego agenta**.
Nazwa spoza zbioru agenta jest odmową, nie cichym dołożeniem.

**Czego NIE ruszamy i dlaczego.** `Chosen` (`inherit/wire.rs`) opisuje wybór spośród skilli
**repozytorium gospodarza** i jest walidowany wobec skanu `.claude/skills/` tamtego repo —
nazwa spoza skanu daje `Error::NotInTheHost`. Wepchnięcie tam nazw z naszej biblioteki byłoby
odmową dla każdej z nich. Skille natywne dostają własną drogę: ta sama funkcja przepisująca
(`inherit::rewrite::plugin_dir`), inny korzeń źródłowy — biblioteka zamiast cudzego repo.

**`RunSpec` zostaje nietknięty.** Ten typ nie ma `Default` i konstruuje go 31 plików (6 w `src/`,
25 w `tests/`), więc dopisanie pola wywraca dwadzieścia pięć plików testowych spoza każdego
sensownego zakresu. Wybrane nazwy i hashe ich kanonicznych plików jadą tą samą drogą, którą jedzie
dziedziczenie — jedną wartością na bieg — i lądują w zrzucie biegu. Sterowniki mają już dwa
gotowe, dyn-safe szwy z domyślnym `None`: `with_inherited` i `configured`.

**Read first:** `src-tauri/src/inherit/wire.rs` (nagłówek — dlaczego ścieżka wolno do argv, a treść
nie) · `src-tauri/src/inherit/rewrite.rs` (`plugin_dir`, `plugin_argv`) · `src-tauri/src/skills/mod.rs`
(`DESTINATION_DIRS` — dwa katalogi, sześciu vendorów; kompilatora nie ma i nie będzie) ·
`src-tauri/src/skills/place.rs` (`discovery_from_init`) · `AGENTS.md` niezmienniki 9, 16, 29.

## Kto to robi

- **Agent:** `rust-core`
- **Druga opinia:** inny vendor niż pisarz (D3).

## Zanim napiszesz pierwszą specyfikację — dwie rzeczy o celu `it`

1. **Adresowanie.** Kryterium woła `cargo test --test it <modul>::`, nigdy
   `cargo test --test <modul>`. Cel `it` jest jeden i zbiorczy.
2. **Wpis `mod`.** Nowy plik wymaga linii `mod <nazwa>;` w `src-tauri/tests/it/main.rs`. Bez niej
   plik kompiluje się do niczego i nie uruchamia ani jednego testu — czyli **wygląda jak zestaw,
   który przeszedł**, a bramka melduje „exit 0 but no evidence of execution" i czyta to jako defekt
   KONTRAKTU, nie implementacji. Dlatego `main.rs` jest w OWNS tego zadania.

   Zmierzone 2026-08-22 na pierwszym biegu T-79: faza kontraktu napisała dwie specyfikacje po
   kilkanaście kilobajtów i nie dopisała ani jednego `mod`. Runda naprawcza powtórzyła ten sam
   błąd, bo widziała ten sam komunikat i nie wiedziała, co on znaczy. Bieg skończył się niczym.

## AC-1 Krok dostaje dokładnie te skille, które wynikają z agenta i nadpisania
check: cargo test --test it skills_reach_the_step::
expect: (\d+) passed

Atrapa sterownika widzi zbiór policzony z efektywnego agenta: brak klucza na kroku daje wszystkie
skille agenta, `[]` daje zero, lista daje podzbiór. Skill przypisany agentowi i wyłączony na kroku
**nie jest widoczny**. Żaden skill spoza biblioteki nie dołącza się po cichu — kryterium sadzi
w katalogu domowym skill, którego nikt nie wybrał, i wymaga, żeby go nie było.

## AC-2 Nieznany skill zatrzymuje bieg przed pierwszym procesem
check: cargo test --test it skills_missing_stops_the_run::
expect: (\d+) passed

Nazwa, której nie ma w bibliotece, oraz nazwa spoza zbioru agenta dają odmowę w czasie planowania,
z nazwą brakującego skilla w zdaniu. Kryterium dowodzi, że **żaden proces nie wystartował** —
licznikiem uruchomień na atrapie, nie brakiem wyjścia. `SKILL.md`, którego nie da się przeczytać
albo który nie przechodzi walidatora, jest tą samą odmową.

## AC-3 Claude widzi wybrane skille w katalogu tego biegu
check: cargo test --test it skills_reach_claude::
expect: (\d+) passed

W katalogu biegu powstaje katalog pluginu z dokładnie wybranymi skillami, `--plugin-dir` z jego
ścieżką stoi w argv (nigdy sama flaga bez wartości), a zdarzenie inicjujące sesji potwierdza
rejestrację tych nazw. Pusty wybór nie tworzy katalogu i nie dokłada flagi.

## AC-4 Codex dostaje równoważny zestaw i nie zostawia śladu w repo człowieka
check: cargo test --test it skills_reach_codex::
expect: (\d+) passed

Skille są osiągalne z katalogu roboczego kroku przez `.agents/skills/`, a po biegu **w projekcie
człowieka nie ma ani jednego pliku, którego Loadout nie zapisał do własnego katalogu biegu**.
Dwie drogi są zielone: krok bierze własną kopię folderu, albo odmawia zdaniem nazywającym skill
i folder. Ciche dopisanie katalogu do cudzego repozytorium jest czerwone.

## AC-5 Człowiek widzi, czego zabrakło
check: npx --no-install vitest run src/sections/run/skills-refusal-is-visible.test.tsx
expect: (\d+) passed

Zdanie odmowy z AC-2, z nazwą brakującego skilla i nazwą kroku, stoi w prawdziwym markupie
strumienia biegu — nie tylko w wartości zwróconej przez funkcję (niezmiennik 29).

<!-- OWNS
tasks/T-79.md
src-tauri/src/commands/run.rs
src-tauri/src/inherit/mod.rs
src-tauri/src/inherit/wire.rs
src-tauri/src/inherit/rewrite.rs
src-tauri/src/skills/mod.rs
src-tauri/src/skills/place.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/src/engine/drivers/claude.rs
src-tauri/src/engine/drivers/codex.rs
src-tauri/src/engine/drivers/fake.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/skills_reach_the_step.rs
src-tauri/tests/it/skills_missing_stops_the_run.rs
src-tauri/tests/it/skills_reach_claude.rs
src-tauri/tests/it/skills_reach_codex.rs
src/sections/run/index.tsx
src/sections/run/skills-refusal-is-visible.test.tsx
-->
