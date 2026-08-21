# T-65 — Triggers: widoczny zegar i trwałe przyjęcie sprawy do biegu

Zgłoszenie właściciela 2026-08-20: kiedy Linear przypisze sprawę do właściciela, otwarta
aplikacja ma uruchomić wskazany workflow i przekazać sprawę jako zadanie. T-64 dowiozło format
pliku, bezpieczne zapytanie przez `curl` i pierwszy kursor. Nie dowiozło ekranu, zegara ani
granicy między „Linear zwrócił sprawę" a „bieg został trwale przyjęty".

## Rozstrzygnięcie właściciela z 2026-08-21

Właściciel polecił wprost zaplanować i wykonać T-65 oraz pozostawił wybór rozwiązania agentowi.
To cofa poprzednią blokadę wyłącznie w zakresie szwu potrzebnego temu zadaniu:

- trwałego oczekującego trafienia w pliku pod `~/.loadout/triggers/`;
- z góry przydzielonego UUID v7 przyszłego biegu;
- atomowego momentu akceptacji: pierwszego `run.json`, zapisanego przed pierwszym procesem;
- zredagowanej biblioteki triggerów i trwałego przełącznika `enabled`.

Jeden żywy bieg na aplikację, `stop_run` bez identyfikatora i działanie wyłącznie przy otwartej
aplikacji zostają bez zmian. Nie powstaje Etap B z T-71, rejestr wielu biegów ani daemon.

## Dlaczego stary kontrakt był niewykonalny

`RunState.workflow` jest tylko optymistycznym lustrem jednego okna. Odmowa drugiego Startu
wchodzi w `finally` i może wyzerować je, gdy pierwszy bieg nadal żyje. Nie wolno na nim oprzeć
decyzji o pobraniu sprawy. Autorytetem pozostaje `AppState.live` pod rustowym zamkiem (T-69).

Samo przesunięcie kursora przed albo po `launchRun` też nie wystarcza:

- przed startem: wyścig z ręcznym Startem zjada sprawę po `ALREADY_GOING`;
- po starcie: awaria po pierwszym `run.json`, a przed kursorem, uruchamia ją ponownie.

Do tego okno nie miało jak odkryć plików triggerów ani zapisać `enabled`: T-64 wystawiło tylko
`check_trigger(slug)`. Zustand bez `list_triggers` i `set_trigger_enabled` byłby atrapą, a klucz
Lineara nie może przekroczyć granicy IPC.

## Wybrany protokół

1. `check_trigger` najpierw pyta rustowy uchwyt, czy bieg jest zajęty. `busy` wraca przed
   `curl`, kursorem i jakimkolwiek nowym trafieniem. Zamek nie przechodzi przez `await`.
2. Dla każdego sluga istnieje ukryty, atomowo zapisywany ledger. Pierwsze odpytanie zapisuje
   zastany backlog jako widziany i tylko uzbraja trigger. Następne odpytanie zapisuje **każdy**
   nowy `Issue.id`, nie tylko najnowszy, w deterministycznej kolejności.
3. Ten sam `Issue.id` nie staje się nowym trafieniem po zmianie `updatedAt`. Kursor przyspiesza
   odczyt, ale ledger identyfikatorów rozstrzyga „czy ta sprawa już była".
4. Pending powstaje przed kursorem i niesie pełną sprawę, workflow z konfiguracji, UUID v7
   przyszłego biegu oraz czas utworzenia. Restart oddaje ten sam pending i ten sam UUID.
5. Okno przekazuje claim do istniejącego `launchRun`; ta funkcja dalej rozstrzyga plik,
   aktywny workspace, limit oraz zdania odmowy. `run/io.start` wysyła claim opcjonalnym polem
   istniejącej komendy `run_workflow`; ręczny Start wysyła `null`.
6. `run_workflow` pod rustowym zamkiem sprawdza, czy claim nadal istnieje i pasuje do sluga,
   workflow i UUID, wiąże go raz z wybranym projektem, a następnie bierze zwykłą drogę
   `run_workflow_inner`. Wyścig po odpytaniu kończy się `ALREADY_GOING` i zostawia pending.
7. Plan biegu używa UUID i czasu z pending. Pierwszy atomowy `run.json` zawiera zredagowane
   pochodzenie (`slug`, `delivery_id`, `issue_id`) i powstaje przed pierwszym procesem.
   Dopiero po nim pending przechodzi w `accepted`.
8. Po awarii ledger godzi się bez SQLite: `bound` bez tego `run.json` ponawia ten sam UUID;
   `bound` z tym `run.json` staje się `accepted` i nigdy nie uruchamia drugiego biegu.

Gwarancja brzmi uczciwie: jedna sprawa dostaje najwyżej jedną trwałą akceptację i jeden UUID
biegu. Twardy crash po uruchomieniu zewnętrznego agenta nie daje „exactly once" jego efektów bez
resumowalnego protokołu vendora; istniejący `run.json` oznacza bieg przyjęty i nie jest
automatycznie odpalany ponownie. Stop człowieka także nie cofa sprawy do pending.

## Kształt ekranu i zegara

- Szósta sekcja nazywa się `Triggers`; to lista wierszy, nie płótno.
- Pusty stan pochodzi z `sectionEntry('triggers').empty`, ma najwyżej 12 słów i jedną kropkę.
- Poprawny wiersz mówi źródło, warunek, prawdziwą nazwę workflow i jeden stan. Ma najwyżej
  cztery elementy z tekstem oraz jeden przełącznik. Niepoprawny plik pokazuje wyłącznie nazwany
  problem i nie ma przełącznika, bo jego nieznanych pól nie da się bezpiecznie zachować.
- Lista z Rusta nigdy nie serializuje `api_key`. Uszkodzony plik jest nazwanym problemem,
  nie znikającym wpisem.
- Przełącznik zmienia widok dopiero po atomowym zapisie pliku; zapis zachowuje sekret,
  pozostałe pola i prawa pliku.
- Watcher montuje się przy korzeniu aplikacji, nie w ekranie. Przełączenie sekcji go nie zdejmuje;
  zamknięcie korzenia zatrzymuje zegar. Nie nakłada dwóch pytań o ten sam trigger.
- Stan widoczny rozróżnia: jeszcze niesprawdzony, uzbrojony, czekający na żywy bieg, odmówiony
  oraz przyjęty. Czas pochodzi z trwałego receipt, nie z `Date.now()` po długim biegu.

Interfejs użytkownika jest po angielsku (D5). Nie używa żargonu z listy vocabulary.

## Red-before-green

Przed `./verify.sh before` wszystkie moduły muszą istnieć jako szkielety. TypeScript ma się
zebrać i paść na asercji zachowania; Rust ma mieć kompletne sygnatury z przejściowym `todo!()`
i paść w czasie wykonania. Brak modułu, brak komendy, błąd kolekcji albo nieznany filtr nie jest
czerwienią (§2a AGENTS.md).

## Kryteria akceptacji

## AC-1 Sekcja ma szóste miejsce i prawdziwy ekran
check: npx --no-install vitest run src/sections/triggers/mounted.test.tsx
expect: (\d+) passed

Test czyta prawdziwy rejestr i odkrywanie ekranów. Dowodzi wpisu `triggers`, angielskiej etykiety,
limitu pustego zdania, renderu prawdziwego `TriggersScreen` przez `App`, glifu bez `<circle>` i
`<line>` oraz podniesienia pięciu istniejących luster liczby sekcji wyłącznie o szósty wpis.

## AC-2 Prawdziwy ekran pokazuje bibliotekę i trwałą kontrolkę
check: npx --no-install vitest run src/sections/triggers/row-says-what-happens.test.tsx
expect: (\d+) passed

Podmiotem jest `TriggersScreen`, nie samotny helper (niezmiennik 29). Test renderuje dwa różne
triggery, brakujący workflow i uszkodzony plik. Woła ten sam handler co przełącznik, a potem
pyta magazyn: stan zmienia się dopiero po potwierdzeniu IO, odmowa zostawia poprzednią wartość
i staje na ekranie. Każdy poprawny wiersz mieści się w suficie gęstości i ma dokładnie jeden
żywy przełącznik; uszkodzony wpis ma nazwany problem i zero kontrolek.

## AC-3 Zegar żyje z aplikacją, tyka ponownie i daje się zatrzymać
check: npx --no-install vitest run src/state/triggers-watch.test.ts
expect: (\d+) passed

Ze sztucznym zegarem i wstrzykniętym IO test dowodzi pierwszego i drugiego tiku w nazwanym
odstępie, pomijania wyłączonych, braku nakładania zapytań jednego sluga, dalszej pracy po jednej
odmowie oraz pełnego `stopWatching`. Osobna asercja czyta produkcyjny montaż w korzeniu: watcher
startuje także wtedy, gdy człowiek nigdy nie otworzył sekcji Triggers.

## AC-4 O zajętości decyduje Rust, nie stan okna
check: cargo test --test it trigger_busy_does_not_poll::
expect: (\d+) passed

Żywy Start i żywy `/ask` osobno dają `busy` **przed** wywołaniem fetchera, zapisem kursora i
utworzeniem delivery. Po `settle()` następna próba działa. Sygnatura i wynik tej decyzji nie
biorą żadnego pola z webviewa opisującego, czy coś według okna biegnie.

## AC-5 Trafienie idzie istniejącą drogą Startu z kanonicznym zadaniem
check: npx --no-install vitest run src/state/triggers-take-the-start-path.test.ts
expect: (\d+) passed

Watcher woła `launchRun` z prawdziwym wyborem workflow, identyfikatorem claimu i zadaniem
złożonym z identyfikatora, tytułu i treści sprawy. `launchRun` przekazuje claim do `start`, a
ręczny Start przekazuje `null`. Odmowy `NO_FOLDER`, `GONE_FROM_DISK`, `NOTHING_TO_RUN` i napis
Rusta są zachowane. Moduł triggerów nie importuje `run/io.ts` ani `invoke`.

## AC-6 Ekran rozróżnia czuwanie, czekanie, odmowę i przyjęcie
check: npx --no-install vitest run src/sections/triggers/last-fire-is-honest.test.tsx
expect: (\d+) passed

Każdy stan jest zasiany i sądzony na markupie prawdziwego `TriggersScreen` (niezmiennik 29).
Jeszcze niesprawdzony mówi to wprost; uzbrojony mówi, że czuwa i jeszcze nic nie uruchomił; busy
mówi, że zachował sprawę; odmowa jest słowo w słowo; accepted podaje workflow i czas receipt.
Pięć zdań jest parami różnych.

## AC-7 Biblioteka jest zredagowana i przełącznik zapisuje prawdziwy plik
check: cargo test --test it trigger_library_is_safe_to_edit::
expect: (\d+) passed

Listowanie prawdziwych plików zwraca dla zdrowego wpisu slug, source, condition, workflow i
enabled, a dla nieparsowalnego wyłącznie slug i nazwany problem — żadnych zmyślonych wartości.
Klucz nie występuje w serializacji ani `Debug`. Ukryte pliki stanu nie są konfiguracjami.
Przełączenie zapisuje atomowo, zachowuje klucz, pozostałe pola i prawa pliku; ponowne `load`
widzi nową wartość.

## AC-8 Jedna sprawa ma jedną trwałą akceptację biegu
check: cargo test --test it trigger_run_is_accepted_once::
expect: (\d+) passed

Macierz sądzi: dwa nowe issue między tikami bez straty; ten sam `Issue.id` z nowszym czasem bez
duplikatu; pending przed kursorem; restart odzyskujący ten sam delivery i UUID; odmowę workflow,
oraz `ALREADY_GOING` pozostawiające pending; plan używający prealokowanego UUID;
zredagowane origin w początkowym `run.json`; przyjęcie po tym pliku i przed pierwszym wywołaniem
FakeDrivera; crash między `run.json` a domknięciem ledgeru rozpoznany bez drugiego katalogu i
drugiego startu; odrzucenie podrobionego claimu i autentycznego claimu z innym workflow;
odmowę `run.json`, którego ID albo origin nie pasuje; oraz zwolnienie żywej zapadki po odmowie
i po `AlreadyAccepted`. Kontrola dowodzi, że ręczny Start nadal sam wybija UUID.

## Świadomie poza zakresem

- działanie po zamknięciu aplikacji, LaunchAgent i daemon;
- wiele jednoczesnych biegów oraz `stop_run(id)` z Etapu B T-71;
- formularz tworzenia triggera, Jira, ClickUp i Slack;
- gwarancja dokładnie jednokrotnych zewnętrznych efektów agenta po twardym crashu;
- naprawa globalnego `RunState.workflow`; T-65 przestaje mu ufać, ale nie zmienia paska Run.

<!-- OWNS
tasks/T-65.md
docs/ARCHITECTURE.md
docs/STATUS.md
docs/mockup/index.html
src/main.tsx
src/App.tsx
src/sections/triggers/index.tsx
src/sections/triggers/row.tsx
src/sections/triggers/io.ts
src/sections/triggers/asks-rust-once.test.ts
src/sections/triggers/mounted.test.tsx
src/sections/triggers/row-says-what-happens.test.tsx
src/sections/triggers/last-fire-is-honest.test.tsx
src/state/triggers.ts
src/state/triggers-watch.test.ts
src/state/triggers-wait-for-the-run.test.ts
src/state/triggers-take-the-start-path.test.ts
src/sections/run/launch.ts
src/sections/run/io.ts
src/sections/run/start-args-complete.test.tsx
src/sections/commands-wired.test.ts
src/ui/sections.tsx
src/ui/shell/nav-icons.tsx
src/ui/shell/sections.test.tsx
src/ui/shell/controls.test.tsx
src/ui/shell/screen-mount.test.tsx
src/ui/shell/screen-fallback.test.tsx
src/sections/empty-screen-invites.test.tsx
src/sections/radii-band-reaches-the-sections.test.tsx
src-tauri/src/commands/triggers.rs
src-tauri/src/commands/mod.rs
src-tauri/src/commands/run.rs
src-tauri/src/ipc.rs
src-tauri/commands.golden.txt
src-tauri/tests/it/main.rs
src-tauri/tests/it/trigger_busy_does_not_poll.rs
src-tauri/tests/it/trigger_library_is_safe_to_edit.rs
src-tauri/tests/it/trigger_run_is_accepted_once.rs
-->
