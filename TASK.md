# T-03 — Silnik: nadzór procesów i dowód, że grupa nie żyje

`claude` na tej maszynie nie jest programem, tylko **skryptem powłoki**, który odpala Node v24.16.0
— więc `Command::new("claude")` daje ci powłokę, a model biegnie we wnuku. `tokio::process::Child::kill()`
sygnalizuje wyłącznie bezpośrednie dziecko: zmierzone `A after kill: total=2 orphaned=2` — dwoje
wnucząt przeniesionych pod PID 1, dalej mielą i dalej palą limit [T7 §3.1]. **To jest błąd
finansowy, nie higieniczny, i jest całkowicie niewidoczny**: twój `wait()` wrócił, status jest
„zabity", test jest zielony, a rachunek rośnie w tle. Efekt drugiego rzędu jest jeszcze gorszy —
sieroty dziedziczą stdout, więc potok **nigdy nie dochodzi do EOF**, potwierdzone przez `lsof`
[T7 §3.1]: „czytaj do EOF" przeciwko wyciekłej grupie to nie wyciek, to wieczne zawieszenie.
Trzecia pułapka jest jednolinijkowa: `tokio::time::timeout` wokół kroku **anuluje zadanie Rusta, nie
proces systemowy** [T7 §10.8, niezmiennik 10]. Kod zwraca `Timeout`, testy przechodzą, agent żyje.

**Read first:**
`docs/research/topics/T7-orchestration-engine.md` §3.1 (zmierzony wyciek i zajęty potok — liczby,
nie anegdota), §3.2 (`process-wrap` 9.1.0, nie `command-group`; sygnatury odzyskane z kompilatora:
`start_kill` jest **synchroniczne**, `wait` zwraca **boxed** future), §3.3 (eskalacja TERM→łaska→KILL,
zombie, gdzie dokładnie kończy się `tokio::time::timeout`), §8.2 (dwie pułapki macOS: `sh -c`
exec-optymalizuje pojedynczą komendę i twój znacznik znika z `argv`; skopiowany binarny plik
systemowy dostaje SIGKILL od podpisu kodu — używaj małych plików `#!/bin/sh`).
`docs/research/topics/T1-agent-drivers.md` §4.6 (SIGTERM przerywa turę, zabija drzewo Basha,
odpala hooki `SessionEnd`, wychodzi **143** — dlatego nigdy nie prowadzimy KILL-em) oraz sekcja
„Worth adding": `claude -p` **czeka** na subagentów w tle, domyślnie do **10 minut** — limit czasu
Loadouta krótszy od tego sufitu jest jedyną rzeczą, która nie pozwala zaklinowanemu subagentowi
trzymać procesu sterownika.
`docs/ARCHITECTURE.md` §5 (każda ścieżka limitu czasu przechodzi przez eskalację) i §3 (kod
platformowy **tylko tutaj**).
`AGENTS.md` §3 — niezmienniki 3, 6, 9, 10, 21, 24.

## Kto to robi

- **Agent:** `rust-core` — pisze `codex`
- **Druga opinia:** `claude` (nigdy ten sam vendor; D3)
- **Artefakty biegu:** `runs/T-03/` — transkrypt, plik wyników, plan. Nigdy `$TMPDIR`.

## Co to zadanie posiada

- `src-tauri/src/engine/supervisor.rs` — spawn w grupie procesów, eskalacja SIGTERM→SIGKILL,
  **dowód śmierci grupy**, limit czasu, gwardia `Drop`, utwardzone środowisko.
  **Jedyny plik w repo z `#[cfg(unix)]` / `#[cfg(windows)]`** (niezmiennik 3).
- Sześć plików testowych wymienionych w `check:` (blok OWNS na końcu).

**Czego NIE posiadasz.** `engine/mod.rs` należy do T-02 i zawiera już `pub mod supervisor;`
— sprawdź to jako pierwszą rzecz. Jeśli tej linii nie ma, to jeden wiersz poza twoim blokiem OWNS:
AGENTS.md §7, zapytaj człowieka, nie dopisuj.

**Wszystko, czego dotyka test integracyjny, musi być `pub`.** Pliki w `src-tauri/tests/` to osobne
skrzynie i `pub(crate)` jest z nich niewidoczny: `spawn`, `stop`, `run_with_deadline`, `GroupProof`,
`StdinPlan` i zwracana wartość z `pid`/`pgid`. `libc` jest już zależnością paczki
(`[target.'cfg(unix)'.dependencies]` w `src-tauri/Cargo.toml`), więc cele testowe go widzą —
**nie dopisuj nic do `Cargo.toml`**, jest poza blokiem OWNS i poza listą dozwolonych ścieżek.
Jeśli czegoś naprawdę brakuje, to jest pytanie do człowieka (AGENTS.md §7), a nie powód, żeby
wołać `kill` przez `std::process::Command`.

## Niezmienniki

- **3 — kod zależny od platformy istnieje tylko tutaj.** `checks/quick-boundary.sh` przewraca się
  na `#[cfg(windows|unix|target_os|target_family)]` w każdym innym pliku `src-tauri/src/**`.
  Cicha wersja złamania: `libc::SIGTERM` zaimportowany w pliku wywołującym „na chwilę", żeby coś
  szybko sprawdzić. Wystaw z tego pliku funkcje neutralne (`stop`, `reap_group`), nie stałe sygnałów.
- **6 — zabijamy grupę i dowodzimy, że nie żyje.** `kill(-pgid, 0)` musi dać `ESRCH`, a dopóki nie
  dał, traktujemy grupę jako **żywą**. Cicha wersja złamania: funkcja `stop()` zwracająca
  `io::Result<()>` — `Ok(())` znaczy wtedy „wysłałem sygnał", a wołający czyta „nie żyje".
  Dlatego zwracasz **wartość dowodu**, nie jednostkę.
- **9 — prompt i sekrety wyłącznie przez stdin.** `env_clear()` plus jawna lista przepuszczanych
  nazw, w **jednej** stałej w tym pliku (niezmiennik 23: polityka w jednym rdzeniu, adaptery po
  pięć linii). Cicha wersja: sterownik dokłada sobie zmienną inline „bo tak szybciej", i tak umarło
  skanowanie sekretów w repo źródłowym [raport 05 §4].
- **10 — `tokio::time::timeout` anuluje zadanie Rusta, nie proces.** Każda ścieżka limitu czasu
  przechodzi przez `stop()`. Cicha wersja: `match tokio::time::timeout(d, child.wait()).await
  { Err(_) => return Outcome::Timeout, .. }` — kompiluje się, czyta się dobrze, zostawia żywego agenta.
- **21 — nie pisz artefaktu, którego żaden skrypt nie czyta.** Zwracany `pid`/`pgid` istnieje po to,
  żeby T-06 go zapisał, a T-20 po nim posprzątał. Jeśli w tym zadaniu wyprodukujesz cokolwiek innego
  „na przyszłość" — usuń.
- **24 — komentuj DLACZEGO, zwłaszcza incydent.** Przy `ProcessGroup::leader()`, przy oknie łaski
  i przy pętli dowodowej ma stać datowany powód z liczbą z T7 §3.1.

## Kryteria akceptacji

Wszystkie sześć testów odpala **prawdziwe procesy**, więc wszystkie są `#[ignore]` i nie biegną
w pętli wewnętrznej: `./verify.sh quick` ich nie dotyka, a `checks/full-test.sh` woła wyłącznie
`cargo test --lib`, które celów integracyjnych nie widzi. **CI uruchamia je przez bramkę**:
`scripts/ci.sh` → `./verify.sh full` → `harness/gate.py` odpala każdą linię `check:` poniżej,
a każda niesie `-- --include-ignored`. To jedyna droga, którą te testy w ogóle biegną — bez
`--include-ignored` cargo zamelduje `0 passed`, a bramka odrzuci to jako brak dowodu (niezmiennik 19).

Procesy testowe to **pliki `#!/bin/sh` zapisane do `tempfile::tempdir()`**, nigdy `sh -c` z jedną
komendą (powłoka exec-optymalizuje ją i znacznik znika z `argv`) i nigdy kopia `/bin/sleep`
(podpis kodu macOS zabija ją natychmiast: `Killed: 9`) [T7 §8.2]. Każde oczekiwanie w teście owiń
własnym `tokio::time::timeout` i asertuj na `Ok(_)` — inaczej regresja objawi się jako zawieszenie,
bramka zwróci rc 124, a to jest fałszywa czerwień, nie dowód.

Zanim odpalisz `./verify.sh before`: wpuść kompilujący się szkielet (sygnatury + jawnie złe
wartości zwrotne) i raz `cargo test --no-run --tests`. Tier `before` daje sprawdzeniu 20 s.

## AC-1 Po `stop()` nie żyje **cała grupa**, a dowodem jest `ESRCH`, nie zwrócony status dziecka
check: cargo test --test supervisor_group_death -- --include-ignored

Skrypt-rodzic odpala dwa skrypty-wnuki w tle, każdy z unikalnym znacznikiem w `argv`, i czeka.
Po `spawn()` test potwierdza, że wnuki żyją (skan `ps -eo pid,ppid,pgid,args` po znaczniku).
Po `stop(grace)` asercje: zwrócona wartość to `GroupProof::Dead`; `libc::kill(-pgid, 0)` daje
`Err` z `ESRCH`; skan `ps` nie znajduje **ani jednego** procesu ze znacznikiem; żaden proces ze
znacznikiem nie ma `ppid == 1`. Dodatkowo: `stop()` wołane drugi raz na tej samej grupie zwraca
`GroupProof::Dead` i nie zwraca błędu.

*Słaba asercja:* `assert!(!status.success())` na statusie bezpośredniego dziecka. To jest **dokładnie
ten pomiar, który w T7 §3.1 zwrócił `total=2 orphaned=2`** — rodzic zginął, wnuki żyją i palą limit.
Rozróżnia: `ESRCH` na `-pgid` plus skan `ps` po znaczniku, czyli pomiar spoza drzewa naszego procesu.

## AC-2 Prowadzimy SIGTERM-em, a SIGKILL przychodzi dopiero po oknie łaski
check: cargo test --test supervisor_term_then_kill -- --include-ignored

Przypadek grzeczny: skrypt z `trap 'echo bye > "$MARKER"; exit 0' TERM`, w pętli. `stop(grace = 2 s)`.
Asercje: plik znacznika istnieje i ma treść (SIGTERM naprawdę dotarł i został obsłużony); status
wyjścia to czyste wyjście, nie sygnał 9; czas trwania `stop()` jest wyraźnie krótszy od `grace`
(< 1 s), czyli nie czekaliśmy całego okna po nic.
Przypadek uparty: skrypt z `trap '' TERM` w pętli. Asercje: `GroupProof::Dead`; czas trwania
`stop()` jest **>= grace** (czekaliśmy, zanim eskalowaliśmy); status niesie sygnał 9.
Okno łaski to 5–10 s w produkcji, jedno ukryte ustawienie, nie kontrolka w UI [T7 §3.3]; w teście
podaj je argumentem.

*Słaba asercja:* „proces zniknął". Spełnia ją prowadzenie SIGKILL-em, które jest o tyle gorsze,
że `claude` na SIGTERM dosypuje transkrypt i zwalnia zamek sesji, a na SIGKILL nie robi nic
[T1 §4.6]. Efekt jest niewidoczny do pierwszej sesji, której nie da się wznowić. Rozróżnia:
istnienie pliku znacznika w przypadku grzecznym **i** `elapsed >= grace` w przypadku upartym.

## AC-3 Limit czasu przechodzi przez ścieżkę zabijania, a nie przez porzucenie future
check: cargo test --test supervisor_timeout_kills -- --include-ignored

Skrypt śpi 30 s i odpala wnuka ze znacznikiem, który też śpi 30 s.
`run_with_deadline(cmd, Duration::from_millis(300))`. Asercje: wynik to wariant limitu czasu;
`libc::kill(-pgid, 0)` **po powrocie funkcji** daje `ESRCH`; skan `ps` po znaczniku nie znajduje
wnuka; całość mieści się poniżej 5 s.

*Słaba asercja:* `assert!(tokio::time::timeout(d, fut).await.is_err())`, czyli sprawdzenie, że
limit został **zgłoszony**. Przechodzi je `let _ = tokio::time::timeout(d, child.wait()).await;` —
zadanie Rusta znika, proces zostaje, i to jest jedyny opisany w T7 defekt z adnotacją „łatwo
zregresować, pokryj testem" (§10.8). Rozróżnia: `ESRCH` na `-pgid` **po** powrocie i brak wnuka
w `ps` — jedno i drugie mierzy system operacyjny, nie nasz kod.

## AC-4 Po zabiciu grupy strumień wyjścia dochodzi do EOF
check: cargo test --test supervisor_pipe_eof -- --include-ignored

Skrypt-rodzic odpala wnuka, który dziedziczy stdout, pisze jedną linię i śpi 30 s; rodzic wychodzi
natychmiast. Test czyta stdout do EOF, ale opakowuje odczyt w `tokio::time::timeout(2 s, ..)`.
Asercje: przed `stop()` odczyt do EOF **nie** kończy się w 2 s (potok trzyma wnuk — to jest ta
zmierzona sytuacja z `lsof`); po `stop()` odczyt do EOF kończy się i zwraca `Ok(_)` poniżej 2 s;
linia napisana przez wnuka nie zginęła.

*Słaba asercja:* asercja na statusie wyjścia rodzica. Nic nie mówi o potoku, a to potok wiesza
silnik: „czytaj do EOF" przeciwko wyciekłej grupie nie jest wyciekiem, tylko nieskończonym
oczekiwaniem [T7 §3.1]. Rozróżnia: **oba** ograniczone czasowo odczyty w jednym teście — ten przed
`stop()`, który musi się nie udać, i ten po, który musi się udać.

## AC-5 Dziecko dostaje wyczyszczone środowisko z jawnej listy i puste stdin, a nie to, co odziedziczyło
check: cargo test --test supervisor_env_hygiene -- --include-ignored

Test ustawia w swoim procesie `LOADOUT_SECRET_MARKER` na losową wartość, po czym uruchamia przez
supervisora skrypt `printenv > "$1"`. Asercje na treści pliku: **nie ma** nazwy
`LOADOUT_SECRET_MARKER` ani jej wartości; jest `PATH`; zbiór nazw zawiera się w
`PASSTHROUGH ∪ {PWD, SHLVL, _}` (trzy ostatnie dokłada sama powłoka).
Druga część, stdin: skrypt `cat > "$1"` uruchomiony z planem `StdinPlan::Null` kończy się poniżej
1 s i produkuje pusty plik — dziecko dostało EOF natychmiast. Bez tego `claude` czeka ~3 s i
wypisuje `Warning: no stdin data received in 3s…` [T1 §4.6]; przy czterech agentach to dwanaście
sekund niczego.

*Słaba asercja:* `assert!(!format!("{cmd:?}").contains(secret))` albo przegląd `cmd.get_envs()`.
Obie certyfikują zero: `Command::get_envs()` zwraca **wyłącznie zmienne ustawione jawnie** i nie
mówi ani słowa o tym, czy `env_clear()` w ogóle padło, a środowisko odziedziczone nie pojawia się
w `Debug`. To jest niezmiennik 20 w czystej postaci — test czyta reprezentację, nie zachowanie.
Rozróżnia: odczyt **środowiska widzianego przez dziecko**, wypisanego przez samo dziecko.

## AC-6 Uchwyt porzucony na ścieżce błędu i tak zabija grupę, a po biegu nie zostaje zombie
check: cargo test --test supervisor_drop_guard -- --include-ignored

Część pierwsza: uchwyt supervisora powstaje w wewnętrznym bloku, blok kończy się **bez** wołania
`stop()` (symulacja wczesnego `?`). Asercje: w ciągu 1 s `libc::kill(-pgid, 0)` daje `ESRCH`,
a skan `ps` nie znajduje znacznika.
Część druga: pięć krótkich procesów uruchomionych i zatrzymanych po kolei. Asercja: dla żadnego
zapamiętanego `pid` `ps -o stat=` nie zwraca stanu `Z`, a lista zapamiętanych pid-ów jest niepusta
(inaczej asercja przechodzi na pustym zbiorze).

*Słaba asercja:* `assert!(child.id().is_none())` po `wait()`. Mówi tylko, że **my** przestaliśmy
trzymać uchwyt; nie mówi nic o grupie i w ogóle nie dotyka ścieżki, na której wołający wraca
wcześniej przez `?` — a to jest ta ścieżka, którą naprawdę wychodzi się z funkcji spawnującej.
Rozróżnia: `ESRCH` po samym porzuceniu uchwytu, bez jednego wywołania `stop()`.

## Świadomie poza zakresem

- **Zabezpieczenie czasem startu przed ponownym użyciem PID (`sysctl kern.boottime`)** — T-20.
  Tutaj wystawiasz `reap_group(pgid) -> GroupProof`; decyzję *czy wolno* podejmuje odzyskiwanie.
- **Zapis `pid` i `pgid` do SQLite** — T-06. Ty je **zwracasz** z `spawn()` w jednej wartości,
  synchronicznie, zanim cokolwiek zostanie przeczytane ze stdout: kolejność „wygeneruj, zapisz,
  dopiero spawn" jest tym, co czyni odzyskiwanie w ogóle możliwym [T7 §6.2].
- **Czytanie NDJSON, tee na dysk, sklejanie** — T-05. Ty dajesz `ChildStdout` i gwarancję EOF.
- **Argumenty `claude` i `codex`, polityki, interrupt `control_request`** — T-04 i T-10.
  Supervisor nie zna ani jednej nazwy vendora.
- **Windows `JobObject`.** Miejsce wywołania jest to samo co `ProcessGroup::leader()` [T7 §9.2],
  więc zostaw gałąź `#[cfg(windows)]` z `unimplemented` powodem opisanym słowami — ale nie próbuj
  jej weryfikować, nie ma tu hosta Windows [T7 §11.3].
- **Cooldown po awarii spawnu (ochrona przed burzą restartów)** — nie w v1; jednym zdaniem
  w komentarzu, gdzie by wszedł.
- **Prawdziwe PTY** — D4, v1.1.

<!-- OWNS
src-tauri/src/lib.rs
src-tauri/src/engine/supervisor.rs
src-tauri/tests/supervisor_group_death.rs
src-tauri/tests/supervisor_term_then_kill.rs
src-tauri/tests/supervisor_timeout_kills.rs
src-tauri/tests/supervisor_pipe_eof.rs
src-tauri/tests/supervisor_env_hygiene.rs
src-tauri/tests/supervisor_drop_guard.rs
-->
