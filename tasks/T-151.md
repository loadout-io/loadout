# T-151 — Widoczna rewizja nie jest cofana przed Run

Dzisiejszy stempel pamięci zapisuje z powrotem zamrożony snapshot notatki, więc potrafi skasować
edycję wykonaną już po ułożeniu promptu. Osobno edytor workflowu ma 400 ms debounce i nie łączy
kliknięcia Run z zakończeniem zapisu: bieg może dostać poprzednią rewizję, a starszy zapis
in-flight może później wygrać z nowszym. Trzeci brak jest czysto prezentacyjny — model danych zna
`project | fresh-copy | same-copy`, lecz panel pokazuje tylko binarny wybór folder/kopia.

Zadanie domyka te trzy małe granice świeżości i widocznej konfiguracji. Nie publikuje innych
plików atomowo, nie zmienia listowania bibliotek i nie implementuje wielorodzicowego fan-in;
te odpowiedzialności należą odpowiednio do T-202 i T-153.

**Zależy od:** T-150.

**Read first:** `AGENTS.md` §2a oraz niezmienniki 4, 12, 13, 16, 19, 24 i 29 ·
`tasks/T-139.md` · `src-tauri/src/memory/notes.rs` (`mark_used_from_snapshot`, `persist`) ·
`src-tauri/tests/t139_two_roots_snapshot_and_tombstones.rs` · `src/state/workflows.ts` ·
`src/sections/workflows/{index,editor}.tsx` ·
`src/sections/workflows/step-panel/panel.tsx`.

## Tryb wykonania — DIRECT / medium

1. Codex tworzy osobny worktree przez `./worktree.sh T-151`; nie używa `ship-task.sh`,
   `review.sh` ani `repair.sh`.
2. Przed implementacją dodaje wszystkie trzy kompletne targety i najwęższe kompilowalne
   szkielety. Każdy target uruchamia osobno, a `before` zapisuje czerwień na asercji starego
   zachowania.
3. Po implementacji uruchamia trzy wskazane targety i `./verify.sh quick`; osobny Codex robi
   lekki read-only review diffu bez `review.sh`. Jedyne lądowanie to pojedyncze
   `./integrate.sh <gałąź>`, które wykonuje pełną bramkę.
4. Nie zmienia `harness/`, `checks/`, `verify.sh`, lokalnych workflowów ani receiptów operatora.
   Ciężkie Cargo nie biegnie równolegle z innym zadaniem.

## Uczciwe `before`

Wszystkie trzy dokładne pliki testów istnieją przed `./verify.sh before`. Rustowy target kompiluje
się i przechodzi przez produkcyjny snapshot oraz stempel. Oba browserowe targety korzystają z
istniejącego `e2e/harness.ts`, prawdziwego edytora i kliknięć; nie wywołują store, `saveNow` ani
handlera Run bezpośrednio. Padają odpowiednio na cofniętych bajtach, złej kolejności taśmy i braku
trzeciej kontrolki. Brak modułu, collection error, compile error, zero testów, timeout i sztuczny
sentinel nie są czerwienią.

## AC-1 Stempel zmienia wyłącznie aktualny plik
check: cargo test --test t151_note_stamp_uses_current_file
expect: (\d+) passed

Target zamraża prawdziwy prompt z notatką, po czym przed stemplem zmienia jej body, `modified`
i nieznany klucz front matter. Produkcyjny stempel ponownie czyta plik spod aktualnej ścieżki
i aktualizuje w tych **bieżących** bajtach wyłącznie `last_used_at`; nowsze body, metadane i
nieznany klucz przeżywają. Zamrożony `RunSpec.prompt` oraz receipt nadal opisują starszy snapshot,
więc kontekst biegu nie zmienia się w połowie.

Osobne sceny wykonują Move i Discard po zamrożeniu. Brak starej ścieżki jest stanem
autorytatywnym: stempel nie odtwarza snapshotu, nie nadpisuje celu Move i nie wskrzesza odrzuconej
notatki. T-139 zostaje zmieniony tylko w asercjach, które dziś wymagają utraty
`T139-EDITED-AFTER-PROMPT`; wszystkie pozostałe fakty jego scenariusza zostają.

Notatka nie ma trwałego ID ani fingerprintu niezależnego od ścieżki. Ten task nie udaje więc,
że rozpozna arbitralną podmianę pliku pod tą samą nazwą: w takim przypadku aktualny plik jest
prawdą i wolno zmienić wyłącznie jego `last_used_at`. Wykrywanie innej tożsamości wymagałoby
najpierw osobnej decyzji o trwałym ID.

## AC-2 Run nie może zobaczyć rewizji starszej niż widoczna
check: npx --no-install vitest run e2e/tests/t151-run-uses-visible-workflow-revision.spec.ts
expect: (\d+) passed

Spec zmienia widoczne pole workflowu i natychmiast klika Run, przed upływem 400 ms. Produkcyjna
taśma dowodzi, że zapis rewizji widocznej w chwili kliknięcia zakończył się przed
`run_workflow`, a ten dostaje nazwę pliku wskazującą na co najmniej tę rewizję. Backend może
zobaczyć tę samą albo jeszcze nowszą zapisaną wersję, nigdy starszą.

Ten task nie dodaje snapshotu workflowu do żądania Run. Dzisiejszy protokół przekazuje tylko
nazwę pliku, a ekran Run montuje się dopiero po opuszczeniu edytora. Obietnica dokładnie tych
samych bajtów mimo późniejszej edycji wymagałaby osobnego, wersjonowanego kontraktu
frontend → IPC → planner → receipt; nie wolno jej sugerować samym debounce'em.

Store ma jedną monotoniczną, serializowaną kolejkę zapisu per otwarty workflow. Nowsza rewizja
jest koaleskowana, ale nigdy nie wyprzedza wcześniejszego zapisu tak, aby starsze zakończenie
mogło nadpisać ją na dysku. Test zatrzymuje starszy autosave in-flight, wykonuje nowszą edycję
i klika Run. Nowszy zapis rusza dopiero po rozliczeniu starszego, a `run_workflow` dopiero po
rozliczeniu nowszego; po obu loader widzi najnowsze bajty. Run czeka na potwierdzenie co najmniej
swojej captured revision, nie na dowolny pending zapis.

Odmowa zapisu zostawia człowieka w edytorze, pokazuje po angielsku, że zmian nie zapisano,
i nie wywołuje `run_workflow`. Kolejne poprawne zapisanie tej samej lub nowszej rewizji odblokowuje
Run bez restartu. Test nie omija produkcyjnego debounce, IO ani prawdziwego kliknięcia.

## AC-3 Agent ma trzy jednoznaczne wybory pracy w plikach
check: npx --no-install vitest run e2e/tests/t151-agent-folder-choice-round-trips.spec.ts
expect: (\d+) passed

Prawdziwy panel agenta pokazuje trzy wzajemnie wykluczające się kontrolki odpowiadające dokładnie
`project`, `fresh-copy` i `same-copy`. Angielskie etykiety mówią odpowiednio o pracy w folderze
projektu, rozpoczęciu w nowej kopii oraz kontynuowaniu wcześniejszej pracy w plikach; nie używają
żargonu worktree/branch i nie obiecują automatycznego lądowania zmian do projektu.

Spec wybiera po kolei każdą wartość, zapisuje workflow, zamyka go, ładuje ponownie produkcyjnym IO
i widzi ten sam pojedynczy wybór. Zmiana jednego wariantu wyłącza pozostałe, a istniejące pliki
z każdym z trzech wariantów otwierają się bez migracji. Wielorodzicowy `same-copy` nadal może
zostać odmówiony przez dzisiejszy preflight do czasu T-153; ta kontrolka zapisuje intencję,
nie implementuje Git fan-in.

<!-- OWNS
tasks/T-151.md
src-tauri/src/memory/notes.rs
src-tauri/tests/t139_two_roots_snapshot_and_tombstones.rs
src-tauri/tests/t151_note_stamp_uses_current_file.rs
src/state/workflows.ts
src/sections/workflows/index.tsx
src/sections/workflows/editor.tsx
src/sections/workflows/step-panel/panel.tsx
src/sections/workflows/step-panel/fresh-copy-row.test.tsx
e2e/tests/t151-run-uses-visible-workflow-revision.spec.ts
e2e/tests/t151-agent-folder-choice-round-trips.spec.ts
-->
