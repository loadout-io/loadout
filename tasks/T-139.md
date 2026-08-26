# T-139 — Lądowalny oracle dwóch korzeni pamięci

T-138 pozostaje paragonem działającej implementacji H16/H18, której nie wolno wylądować.
Enforced `before` było uczciwe, pełna suita oraz wszystkie trzy AC przeszły, lecz obie pełne
bramki miały 18/19. Jedyna naprawa usunęła pierwszy lint testu Move, po czym Clippy znalazł
131-wierszową funkcję snapshot/reflection. Recenzent wykazał też, że exact tombstone był
mierzony tylko w jednym korzeniu, więc oracle nie dowodziło, że tombstone biblioteki blokuje
automatyczny zapis do projektu.

To zadanie jest świeżym, kompletnym następcą T-138 i startuje wyłącznie z bieżącego `main`.
Ma trzy nowe, globalnie unikalne targety. Nie przenoś `TASK.md`, commita kontraktowego
`f782ef9`, commita speców `330a49d`, pustego `9e8ec91`, naprawy testu `b997767`, żadnego
targetu T-138 ani całej gałęzi `task-T-138`. Możesz czytać zachowane targety T-138 wyłącznie
jako materiał diagnostyczny, lecz nowe pliki mają własne markery T139, poprawioną strukturę i
nie mogą być ich niezmienioną kopią.

Dopiero po uczciwie certyfikowanym `before` wolno zastosować, w tej kolejności, siedem
commitów produkcyjnych T-138: `705f433`, `7d9bbc9`, `124cc46`, `6642567`, `5ceea68`,
`d439a25` i `3dba18d`. Wszystkie dotykane przez nie pliki są jawnie w `OWNS`. Nie przenoś
bezpośrednio commitów T-137/T-136/T-128: ich właściwa produkcja jest już skonsolidowana w
siedmiu wskazanych commitach.

**Read first:** `tasks/T-138.md` · `docs/STATUS.md` wpis zamknięcia T-138 ·
`/Users/jakubgawronski/Projects/Loadout/runs/T-138/{review.txt,repair.txt,gate-final.json}` ·
read-only `src-tauri/tests/t138_two_roots_snapshot_and_tombstones.rs`,
`src-tauri/tests/t138_move_durability_protocol.rs` i
`e2e/tests/t138-memory-addresses-real-actions.spec.ts` w zachowanym worktree T-138 ·
`AGENTS.md` §2a i niezmienniki 4, 5, 12, 13, 16, 19, 20, 21, 23, 25, 29 ·
`docs/DECISIONS-LOCKED.md` D2 · produkcja i historyczne wyrocznie wymienione w `OWNS`.
Nie czytaj `docs/research/`.

## Kto to robi

- **Agent:** Codex przez jeden pełny bieg Harnessu.
- **Druga opinia:** osobny Codex na innym modelu, tylko do odczytu; jawnie zatwierdzony
  wariant operacyjny właściciela przy kończącym się budżecie Claude'a.

## Granice implementacji

Semantyka produktu jest pełnym kontraktem T-138: `NoteAddress { place, id }` rozróżnia
bibliotekę i projekt; ten sam `id` może istnieć w obu miejscach. Biblioteka daje promptowi
tylko `everywhere` i `this-agent`, projekt tylko własne `this-project`, a biblioteczne legacy
czeka w `Earlier project notes` na Move. Refleksja zapisuje `this-project` pod właściwym
projektem i przechodzi pełny łańcuch `Reflecting → Settings → Evidence → Budget → Start`.

Stempel zapisuje dokładne bajty snapshotu zamrożonego dla promptu. Jednorazowy callback
zmienia plik po zamrożeniu planu, lecz przed produkcyjnym stemplem; późniejszy prawdziwy
`RunSpec.prompt` oraz wynik po normalizacji wyłącznie linii `last_used_at` nadal odpowiadają
ORIGINAL, nigdy EDITED. Fixture ma poprawny, niekanoniczny front matter, nieznany klucz i body.

Move używa jednego rdzenia i portu `MoveIo`: temp w katalogu celu, zapis, sync pliku, publish
no-clobber, sync katalogu celu, unlink źródła, sync katalogu źródła. `RecordingMoveIo`
deleguje do `RealMoveIo` i zapisuje wyłącznie operacje zakończone `Ok`; próby i odmowy mają
osobne liczniki. TOCTOU przy publish nie nadpisuje konkurencyjnego celu i zachowuje źródło.

Automatyczny zapis tłumi dokładny prefiks `<id>__` we wszystkich fizycznych korzeniach.
Kandydat `similar-slug` przechodzi obok `similar-slug-extra__…`; exact tombstone projektu
tłumi zapis do projektu. Osobna obowiązkowa scena sadzi exact tombstone wyłącznie pod
`library_root/discarded`, woła
`record_project_candidate_from_run(library_root, project_root, draft, run)` i wymaga błędu
`PreviouslyDiscarded(expected_id)`, braku adresu w `scan_notes(project_root)` oraz
byte-identycznego drzewa projektu względem snapshotu sprzed wywołania. Wołanie
`record_candidate` na samej bibliotece nie spełnia tej sceny. Kontrola negatywna zapisuje
projekt dokładnie raz i pozostawia bibliotekę byte-identyczną. Żywa notatka nadal wygrywa,
a import ręczny przechodzi.

UI opóźnia katalog A, przełącza na B i ignoruje późną odpowiedź A. Legacy jest lokalizowane
wyłącznie wewnątrz `[data-zone="earlier-project"]`, nie istnieje w `suggested`, a po Move
przechodzi do projektu. Prawdziwe kliknięcia Move, Use, Stop i Discard zawsze niosą zamrożone
`{ catalogFolder, place, id }`; projektowy duplikat nie usuwa bibliotecznego.

## Uczciwy `before`

Przed enforced `before` istnieją trzy nowe targety i kompilowalne szkielety. Każdy rustowy
test oraz helper ma najwyżej 90 linii od pierwszego commita kontraktowego; wspólne setupy i
asercje są nazwanymi funkcjami opisującymi zachowanie. Targety muszą być czyste dla
`cargo clippy --all-targets` bez żadnego `#[allow(clippy::…)]`. To ograniczenie obowiązuje
przed i po przejęciu produkcji — nie czekaj z podziałem długiej funkcji na rundę naprawczą.

Rustowe targety kompilują się, uruchamiają testy i padają na asercjach brakującego zachowania.
Browserowy target montuje prawdziwą aplikację i pada na DOM lub taśmie po kliknięciu, nie na
imporcie, kolekcji ani starcie serwera. Brak symbolu, targetu, błąd TypeScript, `0 passed`,
`#[ignore]` lub niedopasowany test nie są czerwienią. Dopiero po certyfikacji wolno zastosować
siedem wskazanych commitów produkcyjnych.

## AC-1 Dwa korzenie, realna refleksja, snapshot i tombstone obu korzeni

check: cargo test --test t139_two_roots_snapshot_and_tombstones
expect: (\d+) passed

Standalone target zachowuje pełne sceny AC-1 T-138 z markerami T139: literalny multizbiór
`(place, id)`, izolację A/B, pełny trace i fizyczne artefakty refleksji, receipt `ran = true`,
`kept = 1`, byte-identyczny niekanoniczny snapshot po interleavingu, adresowane mutacje,
prawdziwy filesystemowy Move, exact shared-prefix tombstone, żywą notatkę oraz import.

Nowa scena wielokorzeniowa jest niezależna od sceny jednego korzenia. Tombstone o dokładnym
`<id>__` istnieje wyłącznie w bibliotece, katalog projektu jest pusty, a produkcyjne
`record_project_candidate_from_run` próbuje zapisać `this-project` do projektu. Test wymaga
typed refusal oraz braku pliku w obu `notes/`; kontrola negatywna z
`<id>-extra__<czas>.md` przepuszcza dokładnie jeden plik projektowy i nie zmienia biblioteki.
Odmowa dodatkowo pozostawia byte-identyczne drzewo projektu i pusty wynik odpowiedniego
adresu w `scan_notes(project_root)`. Żadna asercja nie grepuje źródła produkcyjnego ani nie
używa produkcyjnego helpera jako własnej wyroczni.

Scena snapshot/reflection jest od początku podzielona na setup, uruchomienie projektu A,
uruchomienie projektu B i niezależne asercje. Żadna funkcja w targetcie nie przekracza 90
linii, a nazwy helperów zachowują przyczynowość testu zamiast ukrywać całe AC w jednym helperze.

## AC-2 Move raportuje wyłącznie udane operacje i zachowuje pełną kopię

check: cargo test --test t139_move_durability_protocol
expect: (\d+) passed

Target zachowuje pełne AC-2 T-138. Sukces ma dokładny ślad StageIn, SyncFile,
PersistNoClobber, SyncDir(target), RemoveFile, SyncDir(source). Każda odmowa ma osobny licznik
dojścia i exact successful prefix kończący się przed nieudaną operacją. Publish race kończy
się po SyncFile, zachowuje źródło i konkurencyjny cel. Sync target, unlink i sync source
zachowują właściwe granice dwóch/jednej pełnej kopii. Identyczne wyniki odmów są grupowane w
jednym ramieniu `match`; nie ma suppression ani funkcji przekraczającej 90 linii.

## AC-3 Legacy stoi we właściwej strefie, a cztery kliknięcia zachowują adres

check: npx --no-install vitest run e2e/tests/t139-memory-addresses-real-actions.spec.ts
expect: (\d+) passed

Spec zachowuje pełne AC-3 T-138 z markerami T139 i używa wyłącznie `e2e/harness.ts`.
Początkowy legacy locator jest dzieckiem `earlier-project`, a ten sam adres jest nieobecny w
`suggested`. Po Move biblioteczny legacy znika z wcześniejszej strefy i projektowy pojawia się
w `suggested`. Następnie prawdziwe kliknięcia Use, Stop i Discard zachowują folder B oraz
pełny adres. Po każdej pełnej odpowiedzi dokładny multizbiór DOM i marker bibliotecznego
duplikatu są poprawne; późna odpowiedź A niczego nie cofa. Końcowa taśma zawiera dokładnie
dwa listowania i cztery mutacje, bez setterów Zustand, bez globalnego fallback locatora i bez
`waitForTimeout` jako jedynego dowodu.

<!-- OWNS
tasks/T-139.md
src-tauri/src/memory/notes.rs
src-tauri/src/commands/memory.rs
src-tauri/src/commands/run.rs
src-tauri/src/ipc.rs
src-tauri/commands.golden.txt
src-tauri/tests/t139_two_roots_snapshot_and_tombstones.rs
src-tauri/tests/t139_move_durability_protocol.rs
src-tauri/tests/it/a_note_remembers_when_it_was_used.rs
src-tauri/tests/it/a_run_leaves_suggestions.rs
src-tauri/tests/it/a_suggestion_can_be_discarded.rs
src-tauri/tests/it/a_suggestion_needs_a_because.rs
src-tauri/tests/it/memory_reaches_only_its_agent.rs
src-tauri/tests/run_evidence_reaches_the_product.rs
src-tauri/tests/t126_late_stop_and_empty_handoff.rs
src-tauri/tests/t126_private_reflection_receipt_and_evidence.rs
src/state/memory.ts
src/state/memory.test.ts
src/sections/memory/io.ts
src/sections/memory/index.tsx
src/sections/memory/note-row.tsx
src/sections/memory/forced-choice.tsx
src/sections/memory/mounted.test.tsx
src/sections/memory/note-row.test.tsx
src/sections/memory/suggested-can-be-discarded.test.tsx
src/sections/memory/imported-notes-say-where-they-came-from.test.tsx
src/sections/commands-wired.test.ts
src/sections/read-paths-populate.test.ts
src/sections/run/session/mount.tsx
src/sections/run/session/given-is-real.test.tsx
src/sections/state-chip-is-a-pill-with-its-wash.test.tsx
e2e/tests/t139-memory-addresses-real-actions.spec.ts
-->
