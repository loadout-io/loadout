# T-56 — Łańcuch kroków pracuje w JEDNYM drzewie, a krok ciężki bierze w puli własne miejsce

Ta fala buduje harness Loadouta z Loadouta. Zasada, która ją trzyma, jest jedna i to zadanie
jest jej trzecim wcieleniem: **harness jest NASZ. Z repo gospodarza bierzemy TEKST — zadania,
instrukcje, reguły — a nigdy MASZYNERIĘ, i bierzemy go przez PRZEPISANIE do siebie, nie przez
wskazanie palcem na jego ustawienia.** Wczytany plik gospodarza jest cudzą maszyną udającą naszą
konfigurację; `cargo test` policzony poza naszą pulą jest cudzą pamięcią wydaną z naszego konta.
Dwa kryteria niżej to dwie postacie tej samej pomyłki.

Dwie ciche porażki, którym to zadanie zapobiega.

**Pierwsza — łańcuch, który udaje, że pracuje razem.** Harness ma JEDNO drzewo robocze repo:
pisze w nim implementacja, potem sprawdzenie, potem druga opinia, potem poprawka. `Folder` nie
umie tego powiedzieć, więc dziś wybiera się między dwoma kłamstwami: `project` (poprawka pisze po
plikach człowieka) albo `fresh-copy` (każdy krok dostaje WŁASNE drzewo, więc poprawka nie widzi
kodu, który ma poprawić). Oba warianty **kończą się sukcesem**: agent dostał folder, coś w nim
napisał, krok jest zielony. Nikt nie zgłosi biegu, w którym recenzent czytał nie ten kod.

**Druga — pula, która nie wie, co przepuszcza.** Pula miejsc jest jedna na aplikację, ma zakres
`1..=8` (domyślnie 3), i **nic nie odróżnia kroku ciężkiego od zwykłego**: krok, który odpala
`cargo test`, kosztuje w niej dokładnie tyle, co rozmowa. Niezmiennik 26 mówi „nie uruchamiaj
dwóch ciężkich `cargo` naraz na tym Macu", a przy suwaku na 3 harness-jako-workflow uruchamia
trzy. Maszyna zamarza przy zerowym swapie, a bieg wygląda na wolny, nie na zepsuty.

**Read first:**
`tasks/T-52.md` **w całości** — po nim „własna kopia" znaczy **własne drzewo robocze**
(`git worktree` na własnej gałęzi dla repozytorium, klon systemowy dla folderu, który repem nie
jest), a materializacja izolacji przenosi się z `copy_project_into` do `commands/isolate.rs`;
to jest model, na którym stoi AC-1,
`src-tauri/src/workflow/mod.rs` (enum `Folder` ok. linii 160 — trzy warianty i `is_own_copy`;
`Link` ok. 247 — `max_turns` odróżnia strzałkę powrotu od zwykłego „po"; `AgentStep.extra`
ok. 131 — dlaczego starszy build nie kasuje pola nowszego),
`src-tauri/src/commands/run.rs` (`workspace` ok. 1035 — **jedyne** miejsce rozwiązywania folderu,
wołane z `plan_agent` z `&step.id`; `lay_out_the_run_dir` ok. 1104 — zbiór `copied` dedupikuje
po `cwd`, więc drzewo powstaje raz na katalog roboczy, a rundy pętli dzielą ten katalog),
`src-tauri/src/workflow/check.rs` (`Facts`, `one_folder_two_steps`, `the_same_files`, `islands` —
i `When::Saving` kontra `When::Running`),
`src-tauri/src/engine/limits.rs` (nagłówek pliku o zasięgu puli; `Pool`, `Slot`, `Limiter`,
`Run::dispatch` — jedyne wejście do puli),
`src-tauri/src/engine/drivers/fake.rs` (`Recorder` — **stempluje `std::time::Instant`**, patrz
pułapka w AC-2),
`src-tauri/tests/it/fresh_copy_isolates_steps.rs` (wzorzec dublera sterownika, który CZYTA i PISZE
w `spec.cwd` — AC-1 stoi na tym samym),
`src-tauri/tests/it/engine_overlap.rs` (jak się dowodzi nakładania okien i dlaczego tamten plik
świadomie NIE używa czasu wirtualnego),
`docs/harness-as-workflow.md` ustalenie **U-2** — to jest całe uzasadnienie AC-1,
`worktree.sh` (sekcje „node_modules" i „target/" — jak TO repo podstawia zależności: klon
copy-on-write `cp -Rc` na APFS, i dlaczego świadomie NIE dzieli `target/`),
`AGENTS.md` §3 niezmienniki **11, 12, 25, 26, 27** oraz §2a.

## Kolejność względem T-55

**T-56 i T-55 NIE mogą biec równolegle.** Dzielą trzy pliki: `src-tauri/src/workflow/mod.rs`,
`src-tauri/src/workflow/check.rs` i `src-tauri/src/commands/run.rs`. Równoległe zadania są w tym
repo bezpieczne wyłącznie dlatego, że ścieżki testów są globalnie unikalne (§2a p. 2) — to nie
chroni przed dwoma gałęziami przepisującymi ten sam enum.

Kolejność: **T-55 ląduje pierwsze, T-56 odbijasz od zaktualizowanego trunku.** Praktycznie:
`FROM=main ./worktree.sh T-56` dopiero po tym, jak `./integrate.sh T-55` przeszedł pełną bramkę.
Jeśli zaczynasz, a T-55 jeszcze nie wylądowało — zatrzymaj się i zapytaj człowieka (AGENTS.md §7),
zamiast stackować gałąź na cudzej pracy w toku.

## Zależność od T-52

**T-52 musi wylądować PRZED tym zadaniem.** Ono definiuje, czym jest własne drzewo kroku:
rozstrzygnięcie właściciela z 2026-08-19 mówi, że „własna kopia" przestaje znaczyć kopię bajtów
i zaczyna znaczyć **własne drzewo robocze** — repozytorium dostaje `git worktree` na własnej
gałęzi, folder niebędący repem dostaje klon systemowy. Powód jest zmierzony i zapisany w T-52:
ręczny walker przegrywa z systemem plików po kawałku, a praca z kopii nie ma drogi powrotnej.

AC-1 opisuje **wskazanie** drzewa („to samo, w którym pracował krok przede mną"), a nie jego
zakładanie. To rozróżnienie jest cała przyczyna kolejności: dopóki T-52 nie wylądowało, nie ma
czego wskazywać, bo `fresh-copy` znaczy jeszcze co innego, a `commands/run.rs` i
`src-tauri/tests/it/main.rs` są w OWNS obu zadań.

Praktycznie: **T-56 odbijasz od zaktualizowanego trunku**, czyli `FROM=main ./worktree.sh T-56`
dopiero po tym, jak `./integrate.sh T-52` (i `./integrate.sh T-55`) przeszły pełną bramkę. Jeśli
zaczynasz, a któregoś z nich nie ma na trunku — zatrzymaj się i zapytaj człowieka (AGENTS.md §7).

## Czego to zadanie NIE zmienia

- **Trzy dotychczasowe warianty `Folder` zostają** z tymi samymi nazwami i tym samym zachowaniem.
  Dokładamy czwarty; nie przenazywamy żadnego (niezmiennik 25).
- **Czym jest własne drzewo, rozstrzyga T-52** i to zadanie tego nie rusza. `same-copy` tylko
  WSKAZUJE katalog roboczy poprzednika; jak ten katalog powstał — drzewem gita na własnej gałęzi
  czy klonem systemowym — nie jest tu ani pytaniem, ani asercją.
- **Niezmiennik 12 zostaje.** `workflow::check` dalej odmawia dwóch równoległych kroków celujących
  w ten sam folder, a odmowa dalej pada najpóźniej przy Starcie (`check_to_run`), nigdy w trakcie.
- **Pula zostaje jedna na aplikację** i dalej ma zakres `1..=8`. Limit ciężkich jest **wewnątrz**
  niej, nie obok: krok ciężki bierze miejsce z puli **i** miejsce z węższego limitu.
- **Żaden etap biegu nie wchodzi do Rusta** (niezmiennik 27). „Ciężki" jest **własnością kroku
  z grafu**, nigdy wnioskiem z jego nazwy ani roli. `if step.name == "check"` w silniku wywraca
  bramkę tak samo jak `if review_enabled`.

## Kryteria akceptacji

## AC-1 Łańcuch trzech kroków pracuje w JEDNYM drzewie, a krok bez poprzednika jest odmową
check: cargo test --test it folder_same_copy_as_before::
expect: (\d+) passed

Nowy wariant `Folder`, na drucie `{"use": "same-copy"}` — „to samo drzewo robocze, w którym
pracował krok przede mną". Rozwiązuje się na katalog roboczy **najbliższego poprzednika po
strzałkach**, jakiegokolwiek rodzaju ten poprzednik jest; drzewo powstaje przy tym dokładnie raz
(bieg zakłada je raz na katalog roboczy — dziś dedupikuje to zbiór `copied` w
`lay_out_the_run_dir`, po `cwd`).

Fikstura: trzy kroki agenta w łańcuchu `s_one → s_two → s_three`. `s_one` ma `fresh-copy`,
`s_two` i `s_three` mają `same-copy`. Dubler sterownika czyta i pisze w `spec.cwd` i melduje tę
ścieżkę — dubler oddający same zdarzenia przeszedłby ten test na implementacji, która nie zakłada
żadnego drzewa (`fresh_copy_isolates_steps.rs` ma gotowy kształt). Katalog projektu w fiksturze
może być repozytorium albo nie — AC-1 nie sądzi tego, JAK drzewo powstało (to robią kryteria
T-52), tylko ILU ich jest i kto w którym pracuje.

Asercje: (a) trzy zameldowane `cwd` są **równe** — jedno drzewo robocze, nie trzy; (b) ten
katalog **nie jest** katalogiem projektu i leży pod `work/` katalogu biegu, czyli jest tym samym
drzewem, które bieg założył dla `s_one`; (c) plik, który `s_one` utworzył, `s_three` odczytuje
z tą samą treścią, a plik, który `s_one` zmienił, ma w `s_three` treść po zmianie; (d) katalog
projektu tego nie widzi — po biegu utworzonego pliku w nim nie ma; (e) krok z `same-copy` i **bez
wchodzącej strzałki** daje z `check()` dokładnie jedną uwagę `Level::Problem`, jej `step_id` jest
identyfikatorem tego kroku, a `message` jest gotowym angielskim zdaniem nazywającym krok **po
nazwie z kafelka** (niezmiennik 14); ta sama fikstura po dociągnięciu strzałki nie ma ani jednego
problemu; (f) migracja jest addytywna (niezmiennik 25): plik sprzed tej zmiany — z `project`,
z `fresh-copy`, z `pick` i z krokiem **bez klucza `folder`** — dalej się wczytuje,
`Folder::default()` to nadal `Project`, a `same-copy` serializuje się z powrotem na
`{"use":"same-copy"}`; (g) kontrola przeciw pustemu czytaniu: mniej niż trzy zameldowane `cwd`
to błąd testu, nie zieleń.

Fikstura do (e) ma być **łańcuchem, nie wyspą**: `s_head` (`same-copy`, bez wejścia) → `s_tail`.
Krok odłączony od reszty grafu dostaje z `islands()` własne ostrzeżenie i wtedy nie wiadomo, która
reguła zaświeciła.

*Słaba asercja:* samo `assert_eq!(cwd_one, cwd_three)`. Przechodzi dla **szkieletu, który sprowadza
`same-copy` do `project`** — trzy kroki w folderze projektu też są „jednym katalogiem", a plik
napisany przez pierwszy jest widoczny dla trzeciego, tylko z całkowicie złego powodu. To jest
dokładnie ta implementacja, którą kryterium ma odrzucić: kafelek mówi „to samo drzewo", a krok
pisze po prawdziwych plikach człowieka. Rozróżniają to (b) i (d): wspólny katalog musi leżeć pod
`work/`, a projekt nie ma prawa zobaczyć utworzonego pliku. Druga słaba wersja, tym razem w (e):
`assert!(!notes.is_empty())` — przechodzi na **cudzej** uwadze, bo krok bez wejść bywa wyspą i
`islands()` mówi o nim swoje. Rozróżnia `Level::Problem` **plus** `step_id` równy identyfikatorowi
tego kroku.

## AC-2 Krok ciężki bierze miejsce z węższego limitu, więc trzy ciężkie nie nakładają się
check: cargo test --test it heavy_step_takes_its_own_slot::
expect: (\d+) passed

Pula 3, limit ciężkich 1. Dwa biegi tego samego kształtu w jednym pliku: trzy prośby **ciężkie**
i trzy prośby **zwykłe**. Obie idą tymi samymi drzwiami — `Run::dispatch`, z wagą jako
**argumentem prośby**. Druga pula z własnym wejściem łamie zasadę z nagłówka `limits.rs`
(„wysyłka pyta bieg, bieg pyta pulę"): krok ciężki wziąłby wtedy miejsce bokiem, z pominięciem
pauzy limitu dostawcy. Miejsce ciężkie oddaje **ten sam `Drop`**, co zwykłe — osobne
`release_heavy()` przecieka przy panice i przy anulowaniu, a pula kurczy się przez cały bieg,
aż nic już nie startuje.

Asercje: (a) trzy okna ciężkie są **parami rozłączne** — przecięcie każdej pary jest zerowe;
(b) wszystkie trzy ciężkie naprawdę weszły i wyszły: trzy wejścia, trzy wyjścia; (c) każde okno
ciężkie trwa dokładnie tyle, co sen zadania, na zegarze wirtualnym; (d) trzy okna zwykłe mają
**wspólną chwilę** — zmierzone `running_now() == 3`, kiedy wszystkie trzy są w środku;
(e) zagnieżdżenie: krok ciężki zajmuje też jedno ze zwykłych miejsc, więc przy jednym ciężkim
w środku mieszczą się jeszcze najwyżej dwa zwykłe, a `running_now()` nigdy nie przekracza trzech
— bez tego osiem ciężkich kroków biegłoby obok trzech zwykłych i sufit pamięci z niezmiennika 26
przestaje cokolwiek znaczyć; (f) limit ciężkich jest przycinany w tym samym jednym miejscu, co
`at_once`: 0 i 99 wychodzą z konstruktora wewnątrz `1..=at_once`, bo limit zero to pula, w której
żaden krok ciężki nigdy nie ruszy, a to nie jest „ostrożniej", tylko zakleszczenie.

**Zegar wirtualny, i dlaczego akurat tu.** `#[tokio::test(flavor = "current_thread",
start_paused = true)]`, a „pracą" prośby jest wyłącznie `tokio::time::sleep`. Pięć testów w tym
repo mówi wprost coś odwrotnego (`engine_overlap`, `limits_are_global_across_runs`,
`limits_dial_raises`, `runcmd_parallel`, `workspace_global_slots`: „nigdy `start_paused`") i mają
rację **u siebie** — ich praca jest prawdziwa (dubler sterownika, prawdziwy proces, prawdziwy sen),
więc czas wirtualny przeskoczyłby dokładnie to, co mierzą. Tutaj praca to jeden `sleep` i nic
więcej, więc zegar wirtualny mierzy to, co pula zrobiła z prośbą, i **nic** o tym, jak obciążona
jest maszyna. Cena prawdziwego zegara jest zmierzona: cztery testy biegu mierzą czas ściennie
i na zajętej maszynie dają fałszywą czerwień, a limit 20 s siedzi w teście, nie w produkcie.

Dwie pułapki mechaniczne, które zamieniają ten test w kłamstwo, jeśli je przeoczysz:
1. **`Recorder` z `engine/drivers/fake.rs` stempluje `std::time::Instant`**, który za zegarem
   wirtualnym NIE idzie. Pod `start_paused` każde jego okno ma kilka mikrosekund i wszystkie się
   nakładają. Ten test stawia własne znaczniki na `tokio::time::Instant`.
2. `start_paused` implikuje runtime jednowątkowy, który przesuwa zegar, kiedy staje bezczynny.
   Jeden `std::thread::sleep`, jeden blokujący zamek albo jeden prawdziwy proces w środku prośby
   zamraża cały runtime i pomiar przestaje być pomiarem. Nic w tym teście nie ma prawa blokować
   wątku.

*Słaba asercja:* `assert_eq!(limiter.heavy_at_once(), 1)` — albo jakakolwiek asercja o **obecności
pola**. Przechodzi dla implementacji, która zapisuje liczbę i nigdy o nic jej nie pyta, czyli dla
defektu z niezmiennika 11: poprzedni prototyp miał `max_parallel`, miał zielone testy i nigdy nie uruchomił
dwóch agentów naraz. Druga słaba wersja: sama rozłączność trzech okien ciężkich. Przechodzi przy
limicie ciężkich równym zero (nic nie biegnie, pusty zbiór okien jest rozłączny) i dla
implementacji, w której każda prośba wraca natychmiast (okna zerowej długości nigdy się nie
przecinają). Rozróżniają to trzy asercje, których **jedna stała nie zaspokoi naraz**: trzy wejścia
i trzy wyjścia, okno długie jak sen, oraz bieg kontrolny ze zwykłymi krokami **w tym samym pliku**,
który musi się NAKŁADAĆ — implementacja szeregująca wszystko przechodzi połowę ciężką i pada na
zwykłej.

## Zanim odpalisz `./verify.sh before`

- **Wariant enuma najpierw.** Bez `Folder::SameCopy` test z AC-1 albo się nie kompiluje (jeśli
  dotyka wariantu z Rusta), albo pada na wczytaniu pliku — a „nie da się wczytać" jest na liście
  `NOT_A_REAL_RED`. Dołóż wariant i pozwól mu rozwiązywać się na cokolwiek; asercje (b) i (d)
  z AC-1 istnieją właśnie po to, żeby uczciwy szkielet padł, a nie przeszedł bokiem.
- **`todo!()` jest zakazane** — `clippy::todo` stoi na `deny` w `[workspace.lints]`. Szkielet
  zwraca wartość, która się kompiluje i jest zła, nie panikuje.
- **Dwie linie `mod` w `src-tauri/tests/it/main.rs`**, alfabetycznie, jak reszta. Plik bez wpisu
  kompiluje się do niczego i wygląda dokładnie jak zestaw, który przeszedł; pilnuje tego
  `checks/quick-tests-listed.sh`.
- **Pętla pracy na `./verify.sh before --only AC-n`**, nie na `full`: gołe `vitest run` łapie
  `e2e/**/*.spec.ts`, a `clippy --all-targets` kasuje odciski `cargo test`. Ostatnią komendą tury
  ma być `./verify.sh quick`.

## Świadomie poza zakresem

- **Rozwidlenie na wspólnym drzewie.** Dwa kroki `same-copy` wychodzące z JEDNEGO poprzednika
  rozwiązują się na ten sam katalog i mogą biec równocześnie — to jest kolizja z niezmiennika 12
  i `the_same_files` musi ją zobaczyć. Ale `(SameCopy, SameCopy)` nie da się rozstrzygnąć bez
  grafu: dwa kroki schodzące z **różnych** drzew to nie jest kolizja, a dziś ta funkcja dostaje
  wyłącznie parę folderów. **Żadne AC tego nie sądzi** i mówimy to wprost, zamiast przemilczeć:
  to jest luka w wyroczni, nie defekt kodu. Zrób najmniejszą wersję, która nie kłamie, i **zgłoś
  w uwagach**, że kryterium na to nie ma.
- **Wiele wchodzących strzałek.** Krok `same-copy` z fan-inem, którego poprzednicy pracują
  w różnych katalogach, nie ma odpowiedzi na pytanie „które drzewo". Odmowa z nazwaniem kroku jest
  właściwym zachowaniem; AC-1 sprawdza wyłącznie przypadek zerowego poprzednika.
- **Skąd bierze się „ciężki" w schemacie.** Pole na kroku jest addytywne (`#[serde(default)]`,
  niezmiennik 25) i to jest cała zmiana; AC-2 sądzi **pulę**, nie pole, i tak ma zostać.
  Napis na ekranie należy do płótna (T-13) i tam ma być zdaniem po angielsku bez żargonu
  (niezmiennik 14) — `heavy` na drucie jest kluczem pliku, nie etykietą kontrolki.
- **Suwak „ile ciężkich naraz" w interfejsie.** Kontrolka bez handlera nie wchodzi do repo
  (niezmiennik 16), a handler bez ekranu to martwy kod. Limit ciężkich ma na razie jedną, stałą
  wartość w Ruście; suwak wchodzi razem z ekranem, który go pokazuje.
- **Dzielenie `target/` między drzewami kroków.** `worktree.sh` odwrócił tę decyzję 2026-08-17
  i powód jest błędem POPRAWNOŚCI, nie wydajności: jeden `CARGO_TARGET_DIR` dla dwóch checkoutów
  o tej samej nazwie pakietu daje jeden odcisk i cargo melduje `Fresh` przeciwko CUDZYM
  artefaktom. Krok ciężki płaci zimnym cache i to jest cena przyjęta świadomie.
- **Migracja plików workflow na dysku.** Nic nie przepisujemy. Stare pliki wczytują się bez zmiany
  (AC-1 f), a nowy wariant pojawia się dopiero tam, gdzie człowiek go wybierze.

<!-- OWNS
src-tauri/src/workflow/mod.rs
src-tauri/src/workflow/check.rs
src-tauri/src/commands/run.rs
src-tauri/src/engine/limits.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/folder_same_copy_as_before.rs
src-tauri/tests/it/heavy_step_takes_its_own_slot.rs
-->
