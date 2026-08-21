# T-74 — Linear: trigger da się utworzyć, sprawdzić i zmienić bez pisania JSON-a

Zgłoszenie właściciela 2026-08-21, po otwarciu wylądowanego T-65: *„ja tak nie chcę, że muszę
sam coś wchodzić i definiować; powinien być cały UI, aby wybrać connector, podać klucz, ustawić
schedule i wybrać workflow — najpierw Linear"*. To jest jawne rozszerzenie zakresu przez
właściciela. T-65 dowiozło trwałą dostawę i bibliotekę, ale wprost zostawiło formularz poza
zakresem. Ten kontrakt domyka produktową drogę Lineara; człowiek nie tworzy ani nie edytuje
`~/.loadout/triggers/*.json` ręcznie.

## Rozstrzygnięcia właściciela przełożone na produkt

1. **Najpierw jeden prawdziwy connector: Linear.** Nowy szkic zaczyna się od wyboru Lineara.
   Nie pokazujemy martwych kontrolek Jira, ClickUp ani Slack. Kształt danych jest dopisywalny,
   ale interfejs oferuje wyłącznie zachowanie, które istnieje (niezmiennik 16).
2. **„Schedule" oznacza częstotliwość sprawdzania, nie cron.** Wybory to 1, 5, 15 i 60 minut,
   a plik bez nowego pola zachowuje dotychczasową minutę. Zegar działa tylko przy otwartej
   aplikacji, tak jak T-65. Trigger czasowy i działanie po zamknięciu są osobnymi produktami.
3. **Warunek jest prawdą o zapytaniu.** Dziś jedyne zapytanie Lineara brzmi „issue assigned to
   me", więc formularz pokazuje `When — An issue is assigned to you` jako fakt, nie jako
   jednoelementową ani wolnotekstową kontrolkę. Pole `condition` zapisuje kanoniczne
   `assigned-to-me`; dowolny tekst byłby obietnicą, której zapytanie nie wykonuje.
4. **Klucz wpisuje się w oknie, ale okno nigdy go nie odczytuje.** Nowy trigger wymaga pola
   `type=password`. Edycja pokazuje wyłącznie „A Linear key is saved" i puste pole do
   zastąpienia; pusty zapis zachowuje bieżący sekret. Klucz przechodzi webview → Rust tylko po
   jawnym Test albo Save, nie wraca w typie, zdaniu, `Debug` ani logu. T-64 pozostawiło
   Keychain poza zakresem: w tym zadaniu sekret nadal jest w prywatnym pliku konfiguracji,
   tworzonym z prawami 0600 przed pierwszym bajtem. Nie nazywamy tego szyfrowaniem at rest.
5. **Test połączenia nie jest odpytywaniem triggera.** Osobne zapytanie `viewer` sprawdza klucz
   i odpowiedź Lineara, ale nie przyjmuje issue, nie uzbraja kursora, nie dotyka ledgeru i nie
   uruchamia workflow. W bramce sieć zastępuje wstrzyknięta odpowiedź; żywy klucz nie wchodzi
   do testów.
6. **Nazwa pliku nie jest polem formularza.** Rust wybija niezmienny slug `linear-<uuid-v7>`.
   Człowiek wybiera connector, workflow i częstotliwość, nie ścieżkę na dysku.
7. **Dysk pierwszy, ekran drugi.** Create/Edit/Delete zmienia listę dopiero po potwierdzeniu
   Rusta. Edycja niesie zredagowaną migawkę wszystkich niesekretnych pól; ich ręczna zmiana
   między odczytem i zapisem daje nazwaną odmowę, nie ciche nadpisanie. Migawka nie niesie
   hasha ani innej pochodnej klucza: jeżeli ręcznie zmienił się wyłącznie sekret, puste pole
   edycji zachowuje jego świeżą wartość, a wpisany nowy klucz jawnie ją zastępuje.
8. **Delete jest świadomie destrukcyjne i dwustopniowe.** Pytanie mówi, że oczekująca sprawa,
   która jeszcze nie zaczęła biegu, zostanie odrzucona. Rust pod wspólnym zamkiem kończy
   Pending jako anulowane, utrwala ledger, a dopiero potem atomowo chowa konfigurację. Bound
   znaczy, że Start już wiąże bieg; Delete odmawia wtedy przed jakąkolwiek mutacją i prosi
   poczekać. Anulowanie rozpoczętego Startu należy do Stop, nie do usuwania konfiguracji. Crash
   przed ukryciem zostawia trigger widoczny i bez pracy udającej Pending; crash po ukryciu
   zostawia go usuniętego. Ukryty plik kończący usunięcie jest sprzątany przy następnym odczycie,
   więc nie powstaje artefakt bez czytelnika (niezmiennik 21).

## Kształt ekranu

- Pusty ekran ma jedną żywą akcję `Create trigger`; znika instrukcja dodawania pliku ręką.
- Formularz ma: Connector, Linear API key, When, Check every, Workflow oraz akcje
  `Test connection`, `Save` i `Cancel`. Brak workflow lub klucza ma widoczny powód przy Save.
- Test odpowiada na prawdziwym ekranie jednym zdaniem sukcesu albo odmowy. Sukces Testu nie
  zamyka formularza i nie zapisuje triggera.
- Zdrowy wiersz otwiera edycję, zachowuje istniejący przełącznik i nadal mieści się w suficie
  gęstości. Uszkodzony plik pozostaje problemem bez kontrolek i bez wymyślonych pól.
- Edycja pozwala zmienić workflow, cadence oraz opcjonalnie zastąpić klucz. Delete ma własne
  widoczne potwierdzenie; `window.confirm` nie wchodzi do webviewa.
- Interfejs jest po angielsku (D5) i korzysta wyłącznie z istniejących tokenów.

## Red-before-green

Przed `./verify.sh before` wszystkie importowane moduły istnieją. `form.tsx` renderuje pusty
szkielet, nowe metody magazynu i IO odmawiają `not implemented`, a rustowe typy i sygnatury
kończą się przejściowym `todo!()`. Testy muszą się zebrać i paść na asercjach zachowania;
brak komendy, modułu albo błąd typów nie jest czerwienią (§2a AGENTS.md).

## Kryteria akceptacji

## AC-1 Prawdziwy ekran prowadzi od pustej biblioteki do kompletnego formularza
check: npx --no-install vitest run src/sections/triggers/setup-is-visible.test.tsx
expect: (\d+) passed

Test renderuje prawdziwy `TriggersScreen` przez pustą i niepustą ścieżkę. Dowodzi: dokładnie
jednej akcji Create; braku instrukcji o ręcznym pliku; panelu osadzonego w ekranie; jedynego
prawdziwego wyboru connectora Linear; pola klucza `type=password`, którego wartość nigdy nie
jest w markupie edycji; uczciwego zdania o warunku; czterech cadence; prawdziwych nazw i ścieżek
workflow; Test/Save/Cancel; widocznego powodu blokującego Save; oraz wiersza otwierającego
edycję bez zabrania przełącznika. Uszkodzony wpis nadal ma zero kontrolek.

## AC-2 Create, Edit, Test i Delete mają prawdziwe handlery i widoczne skutki
check: npx --no-install vitest run src/sections/triggers/setup-actions-are-real.test.tsx
expect: (\d+) passed

Test używa prawdziwego `TriggersScreen`, formularza i wstrzykniętego magazynu. Każda kontrolka
woła jedną właściwą akcję: Test nie zapisuje; Create wysyła Linear, kanoniczny warunek, ścieżkę
workflow, cadence i wpisany klucz; Edit wysyła `null` dla pustego pola klucza; lista zmienia się
dopiero po potwierdzeniu IO; odmowa zostawia panel i wpisane niesekretne pola oraz stoi na
ekranie (niezmiennik 29). Delete najpierw pokazuje zdanie o odrzuceniu oczekującej sprawy,
Cancel nie dotyka IO, a potwierdzenie usuwa wiersz dopiero po sukcesie dysku. Odmowa dla biegu,
który już startuje, zostawia panel i wiersz oraz stoi na prawdziwym ekranie (niezmiennik 29).

## AC-3 Rust tworzy i edytuje pełny plik bez ujawnienia albo nadpisania sekretu
check: cargo test --test it trigger_editor_writes_safe_file::
expect: (\d+) passed

Na prawdziwym katalogu test dowodzi: świeżego sluga z UUID v7; kompletnego JSON-u; `0600`
ustawionego na pustym pliku przed pierwszym bajtem; no-clobber przy Create; domyślnego enabled;
backcompat pliku bez cadence = 1; roundtrip 1/5/15/60; odmowy nieznanego source, złego warunku,
cadence, klucza i brakującego workflow **przed** plikiem. `None` przy Edit zachowuje dokładny
klucz, `Some` zastępuje go, a tryb pliku zostaje. Ręczna zmiana pola niesekretnego daje odmowę
i pozostawia identyczne bajty; osobny przypadek zmienia ręką tylko klucz i dowodzi, że `None`
zachowuje jego świeżą wartość. Żaden zwrot, `Display`, `Debug` ani serializacja nie niesie obu
secret-shaped wartości. Symlink i ścieżka niebędąca zwykłym plikiem są odmawiane.

## AC-4 Test połączenia z Linearem nie zmienia ani jednego pliku
check: cargo test --test it trigger_connection_test_has_no_effect::
expect: (\d+) passed

Wstrzyknięty fetcher dostaje osobne zapytanie `viewer` przez ten sam builder `curl` stdin,
`env_clear`, HTTPS i timeout co watcher. Test obejmuje nowy klucz, zastępczy klucz i zapisany
klucz z edycji. Migawka całego `home` przed i po jest byte-for-byte identyczna: zero config,
kursora, ledgeru, delivery i runu. Zły klucz, HTML, odpowiedź API i błąd procesu dają odrębne,
naprawialne zdania bez sekretu; fetcher biegnie dokładnie raz. Rdzeń probe nie przyjmuje `home`,
więc nie ma ścieżki, do której mógłby pisać.

## AC-5 Każdy trigger naprawdę przestrzega własnej częstotliwości
check: npx --no-install vitest run src/state/triggers-cadence.test.ts
expect: (\d+) passed

Sztuczny zegar dowodzi osobno triggerów 1, 5, 15 i 60 minut: żaden wolniejszy nie pyta za
wcześnie, każdy pyta na swoim terminie, a minutowy pyta ponownie. Wyłączony jest pomijany,
wywołania jednego sluga się nie nakładają, zmiana cadence przelicza następny termin, a Stop
kasuje heartbeat i spóźniony wynik. Montaż pozostaje przy korzeniu; przejście do innej sekcji
nie zatrzymuje zegara.

## AC-6 Nowe akcje przechodzą jedną nazwaną krawędzią IPC
check: npx --no-install vitest run src/sections/triggers/editor-asks-rust.test.ts
expect: (\d+) passed

Test wykonuje `createTrigger`, `updateTrigger`, `deleteTrigger` i `testLinearConnection` na
atrapie `invoke`, czyta prawdziwe sygnatury `ipc.rs`, złotą listę i tabelę `EDGES`. Każdy eksport
woła dokładnie jedną literalną komendę z kompletem argumentów pod nazwami, których oczekuje
Tauri; klucz występuje wyłącznie w żądaniu Test/Save, nigdy w odpowiedzi ani liście. Kontrola
odmawia przy pustej złotej liście i przy eksporcie bez wykonanego przypadku.

## AC-7 Delete kończy oczekującą dostawę i odmawia biegu, który już startuje
check: cargo test --test it trigger_editor_deletes_safely::
expect: (\d+) passed

Test zasadza Pending, po czym potwierdzone Delete zapisuje je jako anulowane przed ukryciem
pliku; żaden późniejszy poll ani claim nie może go uruchomić. Osobny Bound daje nazwaną,
naprawialną odmowę **przed** zmianą któregokolwiek bajtu: Delete nie ściga się z biegiem, który
już startuje. Zmieniona ręką migawka, brak pliku, symlink i uszkodzony ledger także odmawiają
bez zmiany drzewa. Macierz awarii przed i po atomowym ukryciu dowodzi dwóch uczciwych stanów:
widoczny trigger bez fałszywego Pending albo usunięty trigger z odzyskiwalnym cleanupem — nigdy
aktywny config nad skasowanym ledgerem. Sukces usuwa wpis z prawdziwego `list`, sprząta nazwany
tombstone przy następnym odczycie i nie wypisuje klucza.

## Świadomie poza zakresem

- Jira, ClickUp, Slack i ich kontrolki;
- OAuth, Keychain i twierdzenie o szyfrowaniu sekretu na dysku;
- dowolne filtry Lineara inne niż „assigned to me";
- cron, webhook, daemon i działanie po zamknięciu aplikacji;
- wiele równoległych biegów albo zmiany w silniku;
- żywe wywołanie Lineara w bramce;
- edycja uszkodzonego pliku, który nie ma bezpiecznej migawki.

<!-- OWNS
tasks/T-74.md
docs/ARCHITECTURE.md
docs/STATUS.md
docs/mockup/index.html
src/sections/triggers/index.tsx
src/sections/triggers/row.tsx
src/sections/triggers/form.tsx
src/sections/triggers/io.ts
src/sections/triggers/asks-rust-once.test.ts
src/sections/triggers/row-says-what-happens.test.tsx
src/sections/triggers/last-fire-is-honest.test.tsx
src/sections/triggers/setup-is-visible.test.tsx
src/sections/triggers/setup-actions-are-real.test.tsx
src/sections/triggers/editor-asks-rust.test.ts
src/state/triggers.ts
src/state/triggers-watch.test.ts
src/state/triggers-take-the-start-path.test.ts
src/state/triggers-cadence.test.ts
src/sections/commands-wired.test.ts
src-tauri/Cargo.toml
src-tauri/src/commands/triggers.rs
src-tauri/src/ipc.rs
src-tauri/commands.golden.txt
src-tauri/tests/it/main.rs
src-tauri/tests/it/trigger_file_format.rs
src-tauri/tests/it/trigger_key_never_in_argv.rs
src-tauri/tests/it/trigger_library_is_safe_to_edit.rs
src-tauri/tests/it/trigger_busy_does_not_poll.rs
src-tauri/tests/it/trigger_run_is_accepted_once.rs
src-tauri/tests/it/trigger_editor_writes_safe_file.rs
src-tauri/tests/it/trigger_connection_test_has_no_effect.rs
src-tauri/tests/it/trigger_editor_deletes_safely.rs
-->
