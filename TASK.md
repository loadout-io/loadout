# T-131 — Bieżący katalog zachowuje prawdziwe pochodzenie bez heurystyki UUID

T-129 jest **ZAMKNIĘTE bez lądowania**. Jego trzy kryteria były zielone, lecz zdanie
„pełny UUID nie może wystąpić po etykiecie projektu” było błędne: produkcyjny importer bierze
nazwę projektu z surowego basename katalogu, więc projekt legalnie może nazywać się dokładnie
jak UUID biegu. Runda naprawcza zaproponowała rozpoznawanie projektu po kształcie stringa i
maskowanie go jako `another project`; orchestrator słusznie przerwał bieg. Commit `6b8ad1d`
jest dowodem tej drogi na skróty i nie może wejść do produktu.

To zadanie jest świeżym następcą całego T-129. Zachowuje jego prawdziwy zakres: bieżące
`Block::dropped`, zasięg notatki, rozdzielone pochodzenie projektu i biegu oraz uczciwy ekran
bieżącego agenta. Koryguje wyłącznie fałszywy zakaz UUID: o znaczeniu decyduje typowane pole,
nigdy regex ani kształt wartości. T-130 rusza dopiero po zielonym wylądowaniu T-131.

**Read first:** `tasks/T-129.md` jako opis zamkniętego incydentu, nie źródło testów ·
`tasks/T-139.md` · `AGENTS.md` §2a i niezmienniki 4, 5, 13, 14, 16, 19, 20, 23, 25 i 29 ·
`docs/DECISIONS-LOCKED.md` D2 i D5 · `src-tauri/src/import/mod.rs` (`project_name`, tylko do
odczytu) · `src-tauri/src/memory/notes.rs` (`Note`, `Origin`, `Block`, `what_you_know`) ·
`src-tauri/src/commands/memory.rs` (`NoteWire`, `list_note_catalog_inner`) ·
`src/state/memory.ts` · `src/sections/memory/{index.tsx,note-row.tsx}` ·
`src/sections/run/session/mount.tsx` · historyczne wyrocznie wymienione w `OWNS`.
Nie czytaj `docs/research/`.

## Kto to robi

- **Agent:** Codex przez jeden pełny bieg Harnessu.
- **Druga opinia:** osobny Codex na innym modelu, tylko do odczytu; jawny właścicielski
  wyjątek operacyjny od D3 z powodu kończącego się budżetu Claude'a.

## Granice implementacji

`NoteWire` dostaje addytywny fakt `leftOut`: `true` wyłącznie dla notatki `in-use`, którą ten
sam `what_you_know` odłożył poza **bieżący** limit jej zakresu. Długa notatka może wypaść, a
krótsza za nią nadal wejść. Biblioteka liczy legalne `everywhere` oraz `this-agent`, projekt
legalne `this-project`; `this-agent` liczy się osobno dla znormalizowanego właściciela. Wynik
przypina się do pełnego `(place, id)`, więc bliźniaczy `id` w obu korzeniach nie dzieli stanu.
`leftOut` nie jest „last run”, nie zapisuje się do notatki i nie jest wyprowadzane ze statusu,
sumy ani pozycji.

Kanoniczny import zapisuje nazwę projektu w `project:`. `from:` jest wyłącznie
identyfikatorem biegu, który zaproponował regułę. Model, `NoteWire` i TypeScript niosą oba
fakty oddzielnie jako `project` i `from`. Legacy import z `from:` oraz kompletem `source`,
`source_hash`, `app` czyta się jako projekt; gołe legacy `from:` czyta się jako bieg. Odczyt
nie przepisuje pliku, a `NoteAddress { place, id }` pozostaje jedynym adresem mutacji.

Wartości `project` i `from` są opaque: po normalnym trimie front matteru zachowują treść co do
bajta. Zabronione są regex UUID, parsowanie identyfikatora, maskowanie, fallback `another
project` i każda inna próba zgadywania po wartości. Użyj tej samej literalnej wartości
`019b0131-aaaa-7bbb-8ccc-0123456789ab` po obu stronach granicy:

- `project = <ta wartość>, from = null` pokazuje dokładnie
  `Imported from 019b0131-aaaa-7bbb-8ccc-0123456789ab`;
- `project = null, from = <ta wartość>` pokazuje dokładnie
  `Suggested after run 019b0131-aaaa-7bbb-8ccc-0123456789ab`.

Każdy bieżący wiersz mówi prawdziwy zasięg: `Every project`, `This project` albo
`Only <agent>`. Biblioteczne legacy w `Earlier project notes` nie mówi `This project` przed
Move. Lead `In use` jest neutralny wobec mieszaniny zakresów i limitu. `leftOut` pozostaje w
katalogu z jawnym zdaniem, że obecnie nie trafia do promptów z powodu limitu długości.

Bieżący ekran agenta pozostaje przybliżeniem do czasu T-130, ale `notesFor` wyklucza
`suggested`, cudzy `this-agent` i `leftOut`. Nie nazywa tego zamrożonym rachunkiem ani
nie używa słów „last run”.

## Bezpieczne ponowne użycie pracy T-129

Dopiero **po** certyfikowanym, czerwonym `before` wolno zastosować, w tej kolejności, wyłącznie
trzy produkcyjne commity: `dca0c89`, `7939635`, `d38cbde`. Nie przenoś całej gałęzi, TASK.md,
commitów kontraktu/testów T-129 ani `6b8ad1d`. Nowe specy są prawdą T-131; stare targety T-129
nie mogą wrócić. Jeżeli któryś z trzech dobrych commitów konfliktuje, przenieś jego zachowanie
w granicach `OWNS`, nie jego historię ani stare asercje.

## Uczciwy `before`

Przed `./verify.sh before` istnieją wszystkie trzy nowe targety i nie importują nowych symboli.
Rust korzysta z istniejących wejść importu, refleksji i katalogu, a wynik czyta przez istniejące
typy lub `serde_json::Value`. TypeScript buduje obecny `Note`, a przyszłe pola dopina przez
strukturalny spread. Każdy target kompiluje się, uruchamia i pada na asercji brakującego
zachowania albo markupu. Brak modułu, błąd kolekcji, błąd typów, `0 passed`, `#[ignore]` i
niedopasowany test nie są czerwienią.

## AC-1 Katalog wylicza pominięcia i rozróżnia pochodzenie wyłącznie po polu

check: cargo test --test t131_current_memory_catalog_truth
expect: (\d+) passed

Standalone target dowodzi literalnych `leftOut` zgodnych z `Block::{used,dropped}` dla obu
korzeni, bliźniaczego `id`, dwóch właścicieli `this-agent` oraz układu, w którym długa reguła
wypada, a późniejsza krótka wchodzi. Misplaced, legacy i `suggested` nie dostają fikcyjnego
pominięcia, a odczyt nie zapisuje `leftOut` do plików.

Ten sam target tworzy import przez produkcyjne preview/apply z katalogu, którego basename to
`019b0131-aaaa-7bbb-8ccc-0123456789ab`. Plik i drut mają `project` równe tej pełnej nazwie oraz
brak `from`. Kandydatka zapisana przez produkcyjną ścieżkę biegu z tą samą wartością ma
`from` równe wartości i brak `project`. Legacy `from + source + source_hash + app` czyta się
jako projekt, gołe `from` jako bieg; oba pliki pozostają bajt w bajt takie same po odczycie.
Pełny adres nadal składa się wyłącznie z `place + id`.

## AC-2 Ekran pokazuje prawdziwy zasięg i nie zgaduje po kształcie UUID

check: npx --no-install vitest run src/sections/memory/t131-current-memory-truth.test.tsx
expect: (\d+) passed

Test montuje prawdziwy `MemoryScreen` i produkcyjny `NoteRow`. Widoczny markup obejmuje wszystkie
zakresy, legacy, neutralny lead i `in-use + leftOut`. Dwa wiersze dostają dokładnie tę samą
wartość UUID, raz w `project`, raz w `from`, i muszą pokazać dwa dokładne zdania z kontraktu.
`Imported from another project`, zamiana projektu na zdanie biegu albo biegu na zdanie importu
są porażką. Test nie poprzestaje na helperze ani testowym propsie.

Historyczna wyrocznia `imported-notes-say-where-they-came-from.test.tsx` używa typowanego
`project`, nie przeciążonego `from`, i wymaga zdania importu na prawdziwym ekranie.

## AC-3 Bieżący ekran agenta nie pokazuje niczego, czego obecny prompt nie dostał

check: npx --no-install vitest run src/sections/run/session/t131-left-out-is-not-given.test.tsx
expect: (\d+) passed

Test mockuje wyłącznie transport `list_notes`, woła produkcyjne `readWhatWasGiven` i montuje
prawdziwy `AgentScreen`. Agent widzi wpuszczoną notatkę globalną i własną, ale nie widzi
kandydatki, notatki innego agenta ani `leftOut`. Kontrola montuje `MemoryScreen` i nadal widzi
pominięty wiersz: wykluczenie z `Given` nie może polegać na skasowaniu go z katalogu.

<!-- OWNS
tasks/T-131.md
src-tauri/src/memory/notes.rs
src-tauri/src/commands/memory.rs
src-tauri/tests/t131_current_memory_catalog_truth.rs
src-tauri/tests/it/memory_notes_injection.rs
src/state/memory.ts
src/sections/memory/index.tsx
src/sections/memory/note-row.tsx
src/sections/memory/t131-current-memory-truth.test.tsx
src/sections/memory/imported-notes-say-where-they-came-from.test.tsx
src/sections/run/session/mount.tsx
src/sections/run/session/t131-left-out-is-not-given.test.tsx
-->
