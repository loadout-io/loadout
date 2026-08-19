# Stan budowy — 2026-08-18, 00:40

Ten plik jest **żywy**. Aktualizuje go orchestrator po każdym lądowaniu. Prawdą o zadaniu jest
`tasks/<ID>.md`; tutaj jest wyłącznie to, czego z plików zadań nie widać: co już stoi w trunku,
co stanęło i dlaczego.

## 2026-08-19, 22:20 — harness jest NASZ: dziedziczymy tekst, nigdy maszynerie

**Wyladowane: T-53 (4 kryteria) i T-10 (6).** Pelna bramka po kazdym: 15 sprawdzen, 0 czerwonych.
Do tego zamkniety spike **S-3** i **trzy naprawy harnessu**, kazda z kontrola w obie strony.

Pytanie wlasciciela brzmialo: co sie stanie, gdy Loadout odpali agentow w repo, ktore ma juz
WLASNY harness (mierzone na `../meetnotes`, ale to tylko przyklad). Odpowiedz jest zmierzona,
nie zalozona, i odwrotna do pierwszej hipotezy.

### Kierunek „wczytaj ustawienia gospodarza, odejmij haki" NIE ISTNIEJE

Zmierzone na 11 biegach `claude -p`: kazdy z `--setting-sources project` odpalil hak gospodarza
(7/7); `--settings <plik>` SUMUJE sie z projektowym i nie gasi hakow nawet podana pusta lista
`PreToolUse`; `--bare` gasi je kosztem OAuth (`Not logged in`), wiec na subskrypcji jest
bezuzyteczny. Zostaje kierunek odwrotny: **odetnij wszystko, potem odbuduj wiedze po swojemu.**

Cena wczytania jest twarda, nie estetyczna: **hak PreToolUse gospodarza startuje proces we
WLASNEJ grupie procesow, a jego dziecko dostaje ppid=1 i przezywa wyjscie `claude`.** Zmierzone:
jeden bieg zostawil 14 sierot, eksperymenty lacznie 30 zywych procesow ubitych recznie. Przy
zaladowanych ustawieniach gospodarza **niezmiennik 6 jest nie do utrzymania** — zabicie naszej
grupy nie dotyka ani jednej z tamtych.

Zmierzone ryzyko, ktore ta fala zamyka: nasz agent wywolal projektowego podagenta gospodarza
(`release-engineer`), ktory wystartowal jako osobny proces i spalil **38-41 tys. tokenow
calkowicie poza widokiem i rozliczeniem Loadouta**.

### Dwie rzeczy, w ktorych mylil sie research po drodze

1. **`--allowedTools` to lista AUTO-ZATWIERDZANIA, nie filtr dostepnosci.** `Task`/`Agent`
   i `Skill` sa dostepne w KAZDEJ z trzech polityk. Filtrem jest `--tools` — twarda biala lista, i to
   ona wchodzi do sterownika (T-53 AC-1). Czarna lista nie wystarcza: domyslna powierzchnia ma
   osiem sciezek startu procesu (Task, Workflow, SendMessage, CronCreate, RemoteTrigger,
   ScheduleWakeup, EnterWorktree, Monitor) i cicho urosnie przy nastepnym wydaniu CLI.
2. **`init.tools` nie jest powierzchnia uprawnien.** Lista pod `ReadOnly` zawiera `Bash`.
   Porownywanie polityk przez dlugosc tej listy to blad kategorii — 27 pozycji to BAZA CLI,
   a wymienienie `Glob` albo `Grep` w `--allowedTools` odslania oba, dajac 29.

### Ustawienia gospodarza moga nas ROZSZERZYC, nie tylko zawezic

`sandbox.autoAllowBashIfSandboxed: true` przepuszcza dowolna komende mimo naszego
`--allowedTools`. Blok `env` gospodarza nadpisuje srodowisko podane przez Loadouta (jego haki
czytaja wlasne zmienne, wiec haki i `env` to jedna calosc). Dlatego przepisujemy **wylacznie
`permissions.deny`** — `src-tauri/src/engine/drivers/host.rs`, T-53 AC-4.

### Trzy naprawy harnessu, kazda po prawdziwym incydencie

- **`ac30479` — cztery konsumenty OWNS czytaly ten blok na trzy rozne sposoby.** 42 z 60 plikow
  zadan konczy blok bajtami `...cancel.rs-->`, bez nowej linii. `quick-scope.sh` kasowal `sed '$d'`
  CALA ostatnia linie (ginela ostatnia sciezka), a `before-spec-owns.sh` z regexem `\n-->`
  **nie dopasowywal wcale** i wychodzil zerem z napisem „nothing to judge" — czyli NIE SADZIL
  NICZEGO na 42 zadaniach. To niezmiennik 19 zlamany po cichu wewnatrz bramki. T-10 wpadl przez
  to w pelne zakleszczenie: napiszesz plik -> `quick-scope` czerwony, nie napiszesz -> AC-6
  czerwone, TASK.md zablokowany.
- **`04a346e` — kanarek `tasks/T-01.md` pilnowal polityki, ktora wlasciciel cofnal** commitem
  `533eab8`. T-53 skonczylo 4/4 zielone i utknelo na czerwieni, ktorej zadna dozwolona sciezka
  nie gasi. Zdjecie jest bezpieczne: `Edit/Write(TASK.md)` zostaja w `deny`, wiec pisarz dalej
  nie tknie wlasnego kontraktu.
- **`699ef25` — kod 2 znaczy „nie twoje" na calej dlugosci.** `quick-permissions` oddawal 1 przy
  sprzecznosci konfiguracji, choc CALY jego material (`.claude/settings.json`, blok OWNS, on sam)
  lezy poza zasiegiem pisarza. Teraz oddaje 2. Razem z tym **zawezona karta w `integrate.sh`**:
  stara wersja wybaczala KAZDY kod 2 na trunku, wiec sama pierwsza naprawa otworzylaby dziure.
  Wybacza teraz wylacznie przy SWIEZYM paragonie z pusta lista `misconfigured` (nowe pole w
  `runs/last.json`); brak paragonu i paragon o innym commicie znacza odmowe.

**Zasada dla nastepnych sprawdzen:** sprawdzenie, ktorego caly material lezy poza zasiegiem
pisarza, oddaje 2, nie 1.

### S-3 zamkniety, T-10 odblokowane — ale pokrycie parsera jest zdegradowane

`docs/research/fixtures/codex-stream.jsonl` pochodzi z PRAWDZIWEGO biegu `codex exec --json`
(commit `7a24fd4`), ale zawiera wylacznie **koperte awaryjna**: cztery zdarzenia
(`thread.started`, `turn.started`, `error`, `turn.failed`), bo konto Codeksa jest bez kredytow
**do 2026-08-20 05:30**. Ani jednego `item.*`. T-10 AC-2 przewidzialo ten przypadek i wymaga
oznaczenia mapowan `item.*` komentarzem `[3p]`. **Po 5:30 S-3 leci ponownie i ten plik ma sie
POWIEKSZYC** — to jest zaplanowane, nie regresja.

Dwa pomiary przy okazji: stdout Codeksa jest czystym JSONL, a stderr niesie `Reading additional
input from stdin...` (potwierdza T2 §9.3: nigdy `2>&1`). `--ignore-user-config` USUWA ladowanie
cudzych serwerow MCP — bieg bez tej flagi probowal odswiezyc OAuth dla figma, notion i linear,
zanim ruszyla tura. Codex nie ma `--strict-mcp-config`, wiec to jedyny znany srodek.

### Codex jest slabszym adapterem i to trzeba zapisac, a nie zalozyc symetrie

Nie ma odpowiednika `--tools`, `--disallowedTools` ani `--setting-sources`. `--ignore-user-config`
tyka WYLACZNIE `$CODEX_HOME/config.toml`, a `--ignore-rules` tylko pliki `.rules` — **zadna flaga
nie wylacza projektowego `.codex/hooks.json` gospodarza** (meetnotes ma tam te same trzy straze
co po stronie Claude'a). Jedyna obrona to zaufanie hakow po haszu tresci, czyli obrona MASZYNY,
nie Loadouta: hak raz zatwierdzony wystartuje. Dla adaptera: piaskownica (`-s read-only` /
`workspace-write`) jest glowna dzwignia, `--ephemeral` bez zapisu sesji, i **nigdy**
`--dangerously-bypass-hook-trust`.

### Co czeka

| co | stan |
|---|---|
| **T-54** — dziedziczenie wiedzy (5 kryteriow) | **w biegu**, faza kontraktu |
| **T-55** — krok „sprawdz" (5 kryteriow) | napisane, czeka na wolna maszyne |
| **T-56** — jedna kopia dla lancucha + krok ciezki (2) | napisane, **czeka na T-52** |
| **T-52** — izolacja jako drzewo gita | napisane przez wlasciciela, galaz `T-52`, niezlandowane |
| S-3 ponownie + przeglad cross-vendor | po 2026-08-20 05:30 |

**Wada, ktorej ta fala NIE zamyka:** bramka dalej nie odroznia „czerwien z mojego zakresu" od
„czerwien odziedziczona z trunku w trakcie biegu". `refresh_harness_from_trunk` jest projektowane
i moze wniesc czerwien, ktorej zadanie nie spowodowalo — T-53 musialo zglosic defekt konfiguracji
(semantyka kodu 2) pod kodem 1, bo nie ma czym powiedziec tego inaczej. `699ef25` zamyka tylko te
klase, w ktorej sprawdzenie SAMO wie, ze sadzi nasza konfiguracje.

## 2026-08-19 — sekcja Skills umie przyjac tresc, nie tylko adres

**Wyladowane: T-42 (4 kryteria) i T-43 (3).** Pelna bramka po kazdym: 15 sprawdzen, 0 czerwonych.
Zamowienie czlowieka brzmialo „chce napisac jakiego chce skilla, a program buduje z niego skilla
kompatybilnego z claude/codex", z wyborem „opis -> agent pisze". Rozbite na trzy kontrakty, bo to
sa trzy rozne dowody: **T-42** droga wejscia dla TRESCI (trzy pytania -> `place::emit` -> zapis ->
`ingest::from_folder`, ten sam skan co przy linku), **T-43** jedna tura agenta POZA grafem
(`AgentDriver::start` -> `Outcome.text` -> trzy pola formularza), **T-44** wybor „ten projekt /
wszedzie" (w toku).

### Co z tego wynika dla produktu

Zlota lista komend: 24 -> 29 (`author_skill`, `draft_skill`, `stop_draft` z tej fali, `open_chat`
i `say_to_orchestrator` z pracy wlasciciela). Karta przegladu przestala twierdzic, ze wie, skad
przyszla umiejetnosc: plakietka „From the internet" byla wpisana NA SZTYWNO i ignorowala
`item.fromTheInternet` -- prawdziwa przez konstrukcje, dopoki jedyna droga byl link. Pochodzenie
lezy teraz w plikach (`~/.loadout/skills/origins.json`), a nie w domysle z istnienia kopii
kanonicznej, i ma ostrozny domyslny: kopia bez zapisu pochodzenia jest „z internetu", bo do tej
fali tylko taka droga tworzyla kopie.

### Trzy znaleziska, ktorych ta fala NIE zamiata (AGENTS.md §7)

1. **Utrata danych osiagalna z okna, naprawiona po drodze w T-42 AC-1(c).** `review_skill_inner`
   liczyl sciezke kopii kanonicznej z pola `name` front-mattera i robil na niej `remove_dir_all`
   (`commands/skills.rs:350-351`); `from_folder` nie waliduje nazwy, a `Skill::default()` daje
   `name: ""`. Sprawdzone `rustc`: `PathBuf::from("/a/b").join("")` to `"/a/b/"`. Link do dowolnego
   `SKILL.md` BEZ pola `name:` kasowal `~/.loadout/skills/` razem z `installed.json`.
2. **Globalny limit „ile naraz" nie jest podpiety w produkcji.** `run_workflow_with_slots(…, slots)`
   nie ma wolajacego poza testami, a `run_workflow_inner:237` zaklada wlasny `Limiter` na kazdy
   bieg. Kryterium T-31 dowodzi globalnosci, bo podaje pule argumentem. Trzy karty po trzech
   agentach to dziewieciu agentow przy suwaku na 3 (niezmiennik 11). Dlatego T-43 nie udaje, ze
   bierze slot -- ma jawna granice „jeden draft naraz".
3. **Lista pol zdjetych przez `emit` nie ma konsumenta** (`let (doc, _) = emit(skill)`,
   `place.rs:545`). `hooks:` znika z pliku bez ani jednego zdania na ekranie. Do tego
   `allowed-tools` jest w `SPEC_FIELDS`, wiec JEDZIE do obu katalogow vendorow z samym `Warn` --
   umiejetnosc moze przydzielic sobie narzedzia, a przy tekscie pisanym przez model przestaje to
   byc rzadkie.

### Dwa defekty harnessu, naprawione osobnymi commitami

Odslonil je stos galezi (T-43 odbity od niewyladowanego `task-T-42`, bo trunk byl brudny). Oba
mialy ten sam ksztalt: pytanie o stan dysku rozstrzygane po BRZMIENIU komunikatu.

- `0140979` -- `exit 0 but no evidence` bylo liczone jako „kryterium przechodzi", wiec kazdy
  wznowiony bieg z kryterium rustowym konczyl sie kodem 2 przy uczciwie czerwonych kryteriach.
- `c696fc0` -- „czy sa specyfikacje" rozstrzygane po napisie `did not RUN`; kryterium rustowe bez
  modulu udawalo istniejacy plik, wiec bieg szedl NAPRAWIAC pliki, ktorych nie ma. Teraz pyta
  dysku przez `gate.spec_tokens` -- ten sam parser, ktory sadzi kontrakt.

Oba z kontrola w obie strony na prawdziwych bajtach funkcji; grozny przypadek („PASSES before
implementation") dalej odmawia.

### Cena infrastruktury, zmierzona

T-42 kosztowalo **~$36,50**, z czego **$12,15 to strata na infrastrukturze**: limit sesji (429 po
811 ms, faza pisarza nie ruszyla) i ubicie biegu na granicy tury (7 minut pisania, `result:
error_during_execution`, $8,44 za prace, ktorej nikt nie odebral). Zamkniete przez
`scratchpad/detach.py` (podwojny fork + `setsid`, kod wyjscia do `runs/<ID>/wave.rc`) -- ten sam
bieg odczepiony przezyl cztery granice tury. Do czekania na wynik uzywaj `Monitor` z
`persistent: true`, nie `run_in_background`: czekacz ginie na kazdej granicy tury, praca nie.

**Falszywa czerwien, ktorej nie warto szukac drugi raz:** `product_path_end_to_end`,
`run_reaches_the_pump`, `runcmd_snapshot` i `runcmd_parallel` wieszaja sie na ZAJETEJ maszynie --
mierza nakladanie sie na prawdziwym zegarze i maja limit 20 s w sobie, wiec
`CHECK_TIMEOUT_OVERRIDE` ich nie podniesie. Przy siedmiu agentach w tle: cztery czerwone.
Na bezczynnej maszynie ta sama migawka: 15 sprawdzen, 0 czerwonych, 16 s.

## 2026-08-18, 05:30 — pietnascie kryteriow jednego dnia i aplikacja, ktora naprawde chodzi

**Suita jednostkowa: 88 plikow / 440 testow zielonych. E2E w prawdziwym chromium: 13/13.**
Dowiezione tego dnia: T-37 (3 kryteria), T-38 (8), T-39 (7). Kontroli negatywnych: **101 w piecu
rownoleglych pasach plus 3 moje**, wszystkie czerwone, wszystkie przywrocone po md5.

### Aplikacja dziala — zmierzone, nie zadeklarowane

Zrzut zywego okna 05:24 pokazuje menu 196 px ze znakiem i `LOADOUT`, piec sekcji, stopke
`Claude · Codex ready`, pasek kart z `＋`, wybor workflow, **wlaczony** `Start`, suwak „ile
naraz", pusty stan z zaproszeniem, **szyne agentow** po prawej i **wiersz wejscia** na dole.

**Dowod, ze to nie atrapa:** w wyborze stoi `New workflow 2`, a na dysku lezy
`~/.loadout/workflows/new-workflow-2.json` z polem `"name": "New workflow 2"`. Lancuch
plik → `list_workflows` → `invoke` → okno jest prawdziwy w obie strony — te pliki powstaly
wczesniej przyciskiem `Create`.

### Biale okno przy starcie — przyczyna zamknieta, NIE jest defektem produktu

Dwie przyczyny, obie srodowiskowe. (1) `tauri dev` obserwuje `src-tauri/` i **restartuje
aplikacje po kazdym zapisie** — przy pieciu agentach piszacych rownolegle okno ginelo co
kilkadziesiat sekund, a czlowiek widzial „szary ekran na chwile". (2) vite pre-bunduje
zaleznosci na zadanie i pierwsze wejscie po zmianie ich zestawu blokuje `/src/main.tsx`
na **32 s**; webview trzyma wtedy polaczenia i pokazuje pusta strone.
**Rozpoznanie jest jednolinijkowe:** `curl -o /dev/null -w '%{time_total}' /src/main.tsx`
mierzy ten czas wprost. Okno maluje sie natychmiast po tym, jak serwer zaczyna oddawac modul.

### Trzy rzeczy, ktore znalazl dopiero sprawdzajacy

Pieciu niezaleznych sprawdzajacych z poleceniem „domyslaj sie na niekorzysc pasa". Kazdy
odtworzyl po jednej kontroli negatywnej SAM i przeszukal pliki pod katem zaslepek.
**Atrap nie znalezli zadnych.** Znalezli trzy rzeczy, ktorych nie widzialo zadne kryterium:

1. **Zamkniecie karty z zywym biegiem nie anulowalo biegu.** `WorkspaceTab.agents` bylo pisane
   tylko przy zakladaniu karty i zawsze zerem, wiec `requestClose` zawsze wchodzil w galaz
   „nic tu nie chodzi": karta znikala bez pytania i bez `cancel()`. Osierocony agent dalej palil
   limit (niezmiennik 6 — blad finansowy). `CloseConfirm` byl przez to kodem NIEOSIAGALNYM.
   Naprawione, kryterium T-39 AC-7 z trzema sondami.
2. **`useMemory.load` i `useSkills.load` nie mialy wolajacego** — sciezka odczytu byla zbudowana
   i martwa, wiec obie sekcje dalej nie czytaly dysku. Naprawione.
3. **`commands-wired.test.ts` byl czerwony**: doszly dwie krawedzie bez wiersza w tabeli strazy.
   Dopisane, 16 → 18.

### Co zostalo do prod-ready

- **T-41 (napisane)** — odpowiedz czlowieka NIE dochodzi do agenta. `answer()` jest czysto
  lokalne: pytanie znika z ekranu, agent dalej czeka. To jedyna znana martwa kontrolka i jedyna,
  ktora **klamie**. Nie jest to podpiecie kabla — `RunControl` nie ma uchwytu do zywej sesji,
  wiec trzeba przeciagnac kanal przez granice. `AgentDriver::send` juz istnieje.
- **T-40 (napisane)** — wyrocznia „kazda kontrolka cos robi" poza pieciu ekranami: stany
  zagniezdzone, pola i selecty, oraz dowod, ze skutek jest TYM skutkiem.
- **`quick-types` nie umie byc czerwony na kodzie zadania** — prawdziwy blad typow melduje jako
  „our TypeScript configuration is broken — this is not your code", kodem 2, o ktorym bramka
  sama pisze „never a red". Trafilo mnie dwa razy jednego dnia.
- **`tests/it/main.rs` to nowy kregoslup bez `merge=union` i bez wlasciciela** — dwa zadania
  dodajace test naraz dadza pewny konflikt.

## Liczby

| | |
|---|---|
| commitów lądowania | **34** |
| trunk | **ZIELONY** — 14 sprawdzeń, 0 porażek, 390 s (`runs/last.json`, 2026-08-18 00:31) |
| zadań w `tasks/` | 42 |
| żywe gałęzie | **cztery**: T-38 · T-29 · T-28 · T-37 (T-32 wylądowało, worktree do sprzątnięcia) |
| zablokowane kalendarzem | **S-3, T-10** — kredyty Codeksa wracają 2026-08-20 |

## Co się dziś zmieniło w tempie pracy — i dlaczego to jest najważniejszy wpis

| | było | jest |
|---|---|---|
| `checks/full-test.sh` | do 3600 s, czyli **timeout** | **224 s** |
| `cargo clippy --all-targets` | 455–1200 s | **6 s** na ciepłym drzewie |
| `./verify.sh quick` | ~300 s | **37 s** |
| `./verify.sh full` na trunku | nie kończyło się | **390 s** |
| lądowanie gałęzi | ~2 h | **9–11 min** |

**Przyczyna była jedna: `src-tauri/tests/` miało 122 pliki, a Rust robi z każdego pliku osobne
binarium** linkujące całą bibliotekę z 527 skrzyniami. Same testy wykonują się w **6,0 s**;
reszta była składaniem i pierwszym uruchamianiem 122 programów. Dwie niezależne miary tego samego:

- linkowanie — kontrolowany pomiar jednego celu po dotknięciu `commands/run.rs`: **60 s i 62 s**;
- **pierwsze** uruchomienie świeżej, niepodpisanej binarki debug — `store_strict_schema` **36 s**,
  `workflow_check_ids` **59 s**, przy **0 s** za drugim razem i teście trwającym 0,01 s. To jest
  skanowanie macOS (`syspolicyd`, `XprotectService`), zapamiętywane per plik.

Obie miary mnożyły się przez 122. Pliki są teraz **modułami jednego celu** (`tests/it/main.rs`),
czyli jeden link i jedno skanowanie. Tak samo robią ripgrep (`autotests = false` + jeden
`[[test]]`) i cargo (`tests/testsuite/main.rs`, ~150 linii `mod`). `src-tauri/Cargo.toml` sam to
deklarował od pierwszego dnia — „`cargo test --lib` jest CAŁĄ powierzchnią testową" — a kod łamał
tę deklarację 122 razy.

Dla skali, zmierzone na tej maszynie: `../meetnotes` ma **950** skrzyń (prawie dwa razy więcej
niż my) i **jedno** binarium testowe — 19 835 plików w `target/debug/deps` wobec naszych 886 645.

### Trzy rzeczy, które z tego wynikają dla piszącego

1. **Kryterium woła `cargo test --test it <moduł>::`**, nie `cargo test --test <moduł>`. Filtr
   z dwukropkami, nie sam podciąg: `--test it store` łapie także `store_pragmas` i `storage_x`.
2. **Nowy plik w `tests/it/` wymaga linii `mod` w `main.rs`.** Bez niej nie kompiluje się, nie
   uruchamia ani jednego testu i **wygląda jak zestaw, który przeszedł**. Pilnuje tego
   `checks/quick-tests-listed.sh` — mechaniczny, bez kompilacji, więc działa też wtedy, gdy
   drzewo się nie buduje.
3. **Test mierzący albo zmieniający stan CAŁEGO PROCESU zostaje osobnym celem w `tests/`.**
   Dziś dwa: `shell_logging` (liczy deskryptory przez `/dev/fd`, instaluje globalny hak paniki)
   i `supervisor_env_hygiene` (`env::set_var`). W scalonym binarium mierzyłyby 285 cudzych
   testów — `shell_logging` dostał 96 zamiast swojej liczby przy pierwszym lądowaniu po scaleniu.

## Praca, która weszła z pominięciem pętli — i co to kosztuje

Cztery zadania weszły **wprost na trunk**, bez gałęzi i bez tieru `before`: **T-28, T-33, T-35,
T-37**. Powód był policzalny — fala kosztowała ~2 h przy około 40% skuteczności za pierwszym
razem — ale konsekwencja jest realna i zostaje zapisana:

**Te cztery nie mają dowodu, że ich kryteria były najpierw CZERWONE**, ani drugiej opinii.
Kryterium za wąskie od urodzenia jest w nich niewykryte.

**Powód tego skrótu zniknął.** Pełna bramka idzie 9 minut. Cztery przebiegi to niecała godzina
i to jest najtańszy sposób odzyskania tego dowodu.

Najlepszy argument za tym stoi w `f35466f`: pełna bramka na trunku złapała **prawdziwe
naruszenie projektu** we wczorajszej pracy — `libc::getpgrp()` w `lib.rs` zamiast w
`supervisor.rs` (niezmiennik 3, dwa sprawdzenia naraz). Bez niej nikt by tego nie zobaczył.

## Co stoi w trunku, a czego nie widać z plików zadań

- **Aplikacja się uruchamia i zapisuje.** Cztery wady widoczne dopiero z prawdziwego okna:
  białe okno od IPv6 (`host: false` wiązało serwer na `::1`, WKWebView pyta o IPv4 i **nie
  zgłasza żadnego błędu**), brak `.manage()` (trzy komendy biegu padały „state not managed"),
  katalog projektu wskazujący na `src-tauri/`, oraz `Store::open` poza runtime'em tokio.
- **Sekcje są podpięte do prawdziwych adapterów.** Do 2026-08-17 wszystkie pięć `io.ts` istniało
  i **żadnego nie wołał kod produkcyjny** — jedynym importerem był test. Ekrany były trwale puste,
  a Create odmawiał pod palcem.
- **Edytor workflow jest osiągalny.** Płótno i panel kroku miały testy i **ani jednego miejsca
  montowania**. Siedem takich komponentów znalazł jeden pomiar; `checks/quick-wired.sh` pilnuje
  teraz strony Rusta, strona TS została jako dług.
- **„Własna kopia twoich plików" znaczy kopię** (T-33). Wcześniej `fresh-copy` dawał pusty
  katalog, więc krok pracował na pustce — gorzej niż kolizja, bo agent nie widzi plików.
- **Krok ma limit czasu** (T-35 AC-1), egzekwowany **przez sterownik**, nie przez
  `tokio::time::timeout` — tamto anuluje zadanie Rusta i zostawia żywy proces (niezmiennik 10).
- **Odzyskiwanie po awarii biegnie przy starcie okna** (T-35 AC-2/AC-3). Wymagało zbudowania
  **sześciu** brakujących ogniw, z których **pięć było w komentarzach opisanych jako istniejące**:
  odczyt `kern.boottime`, kolumna `boot_id`, `add_column_if_missing`, `reap_group`
  (`unimplemented!()`), odczyt wierszy i zapis znacznika przy starcie biegu.

## Wada, która wraca — nazwana, bo trafiona ponad dziesięć razy

**Kryterium sprawdza coś węższego niż niezmiennik, którego pilnuje.** Wzorcowy przykład: asercja
`TITLEBAR_HEIGHT <= 96` była **zielona przy 138 px** realnego chrome, bo mierzyła jeden pasek
z trzech. Drugi: „strzałka znaczy po" porównywała chwile odbioru paczek, więc padała losowo,
gdy dwa kroki trafiły w to samo 16-milisekundowe okno pompy.

Trzy rzeczy, które to rozróżniają, i wszystkie trzy są w tym repo sprawdzone:

1. **Wartość oczekiwaną czytaj z pliku, nie przepisuj.** Test wpisujący `196` z palca przechodzi
   też wtedy, gdy makieta mówi 220.
2. **Kontrola negatywna do każdego kryterium.** Dwie moje dzisiejsze sondy przechodziły **także
   przed poprawką** — dowiedziałem się tego wyłącznie dlatego, że je zasadziłem.
3. **Napisz w nagłówku, jaka byłaby SŁABA wersja tej asercji i co ją odróżnia.**

## Co dalej, po właścicielu

| co | kto | stan |
|---|---|---|
| T-38 — szew front↔Rust, klucze argumentów | agent redesignu | 8 kryteriów, gałąź `T-38` |
| T-37 — trzy testy kryteriów układu | agent redesignu | **kod w trunku, testów nie ma** |
| T-29 — e2e w przeglądarce | agent redesignu | odłożone świadomie do po redesignie |
| S-3, T-10 — drugi vendor | czeka | kredyty Codeksa 2026-08-20; `drivers/absent.rs` odmawia głośno zamiast udawać Claude'a |
| przepuścić T-28/T-33/T-35/T-37 przez pętlę | orchestrator | ~1 h, odzyskuje dowód czerwieni |
| Q-6 — zegar ścienny nie odróżnia „wolne" od „wisi" | kolejka | `docs/HARNESS-QUEUE.md` |
| Q-7 — liczba celów testowych | **zamknięte** | 122 → 1, opisane wyżej |

## Mina, o której trzeba pamiętać przed lądowaniem T-28

`a7a2d87` dodał oba pliki testów szkieletowych **wprost na main**, a gałąź `task-T-28` niesie
**własną, rozjechaną kopię tych samych plików**. Różnicą jest dokładnie `#[ignore]` z `6e55daf`,
czyli ogrodzenie płatnych testów uruchamiających prawdziwe procesy `claude`.

**Lądowanie T-28 bez uzgodnienia po cichu cofnie to ogrodzenie** — a bramka po takim lądowaniu
będzie **zielona**, bo cofnięte kryterium nie psuje testów, tylko je osłabia.
