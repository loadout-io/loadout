# Stan budowy — 2026-08-18, 00:40

Ten plik jest **żywy**. Aktualizuje go orchestrator po każdym lądowaniu. Prawdą o zadaniu jest
`tasks/<ID>.md`; tutaj jest wyłącznie to, czego z plików zadań nie widać: co już stoi w trunku,
co stanęło i dlaczego.

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
