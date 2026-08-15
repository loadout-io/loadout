# T-02 — Silnik: graf i planista na FakeDriverze

poprzedni prototyp miał `max_parallel`, miał zielone testy i **nigdy nie uruchomił dwóch agentów naraz**:
`max_parallel` było tylko szerokością wysyłki — jeden worker, `run_ready(1)`, cztery „równoległe"
pasy w rozłącznych oknach po ~0,5 s (`daemon/src/lib.rs:154,:175`, `docs/handoff.md:144-165`)
[raport 01 §7.3]. Żaden test tego nie złapał, bo każdy pytał „czy oba się skończyły", a oba się
skończyły. **To jest cicha porażka tego zadania**: planista, który przechodzi wszystko, o co go
pytasz, i nie robi nic naraz — a równoległość jest całą przesłanką produktu. Druga cicha porażka
jest jeszcze tańsza do popełnienia: prototyp z T7 §2.4 oznaczał wszystko poniżej **anulowanego**
kroku jako `Skipped`, więc po świadomym naciśnięciu Stop UI tłumaczyłoby ośmiu krokom, że „ktoś
wyżej padł". Trzecia: `Dag`, który przyjmuje cykl, bo `is_acyclic()` istnieje, ale nikt go nie woła
przy konstrukcji — wtedy pętla planisty kończy się natychmiast (zero korzeni, `inflight == 0`),
melduje sukces i zostawia bieg, w którym nic się nie wydarzyło.

**Read first:**
`docs/research/topics/T7-orchestration-engine.md` §2.3 (dokładny kształt pętli: zbiór gotowych rządzi
zależnościami, semafor rządzi zasobami — i **permit bierzemy WEWNĄTRZ zadania**, bo inaczej `ready`
vs `running` jest kosmetyką), §2.4 (defekt cancel-vs-skip, znaleziony przez test, z wypisanym
wektorem stanów), §8.1–8.2 (fake z zachowaniami, `start_paused` wymaga `test-util` i **implikuje
runtime jednowątkowy**, więc test na nakładanie musi być wielowątkowy z realnymi krótkimi snami),
§9.3 (tabela przejść kroku).
`docs/ARCHITECTURE.md` §5 (ta sama tabela w wersji wiążącej; `paused` jest stanem **biegu**, nigdy
kroku) i §3 (granica: `engine/` nie zna Tauri).
`AGENTS.md` §3 — niezmienniki 1, 7, 8, 11, 19, 24, **27**.
`docs/DECISIONS-LOCKED.md` §D7 (dlaczego planista nie ma prawa znać pojęcia „recenzja").
`AGENTS.md` §2a — kontrakt kryterium w pięciu punktach. Dlatego siedem kryteriów to siedem plików w `src-tauri/tests/`.

## Kto to robi

- **Agent:** `rust-core` — pisze `claude`
- **Druga opinia:** `codex` (nigdy ten sam vendor; D3)
- **Artefakty biegu:** `runs/T-02/` — transkrypt, plik wyników, plan. Nigdy `$TMPDIR`:
  na maszynie źródłowej przetrwały wyłącznie paragony w repo [raport 06 §7].

## Co to zadanie posiada

- `src-tauri/src/engine/mod.rs` — `Engine`, wspólne typy biegu, deklaracje modułów.
  Tu mieszka też jedyna deklaracja `fake.rs` (patrz niżej).
- `src-tauri/src/engine/dag.rs` — `Dag`, `children()`, stopnie wejściowe, odmowa cyklu **przy
  konstrukcji**. Listy sąsiedztwa; `petgraph` świadomie nieobecny [T7 §9.1].
- `src-tauri/src/engine/scheduler.rs` — `execute()`: zbiór gotowych + `JoinSet` + `Semaphore`
  + `CancellationToken`.
- `src-tauri/src/engine/step.rs` — `StepState`, `StepEvent`, funkcja przejścia.
- `src-tauri/src/engine/drivers/fake.rs` — deterministyczny dubler kroku dla testów planisty.
- Siedem plików testowych wymienionych w `check:` (blok OWNS na końcu).

**Czego NIE posiadasz, a czego potrzebujesz.** Moduł `engine` musi być zadeklarowany w
`src-tauri/src/lib.rs` (`pub mod engine;`) — a `lib.rs` należy do T-01. Jeśli tej linii nie ma,
to **jeden wiersz poza twoim blokiem OWNS**: AGENTS.md §7, zatrzymaj się i poproś człowieka,
zanim cokolwiek napiszesz. Nie dopisuj sobie ścieżki do `TASK.md` — `checks/quick-scope.sh`
traktuje edycję kontraktu jako naruszenie i ma na to osobny komunikat.

**Dlaczego `fake.rs` jest podpięty po ścieżce.** `src-tauri/src/engine/drivers/mod.rs` należy do
T-04. Do jego wylądowania jedynym mieszkańcem katalogu jest twój `fake.rs`, więc podepnij go
z `engine/mod.rs`:

```rust
// T-04 tworzy drivers/mod.rs (trait AgentDriver + ClaudeDriver). Do tego czasu katalog nie ma
// własnego modułu, a drivers/mod.rs nie należy do T-02 (mapa własności, AGENTS.md §7).
#[path = "drivers/fake.rs"]
pub mod fake;
```

Zostaw w `engine/mod.rs` datowany komentarz z **dokładną listą wierszy**, których dołożą kolejne
zadania (`pub mod supervisor;` — T-03, `pub mod drivers;` — T-04, `pub mod stream;` i
`pub mod line;` — T-05). Każdy z nich jest jednym wierszem poza blokiem OWNS tamtego zadania,
więc każdy jest osobnym pytaniem do człowieka; lista sprawia, że pytanie da się zadać w dziesięć
sekund zamiast czytać cały plan.

**Wszystko, czego dotyka test integracyjny, musi być `pub`.** Pliki w `src-tauri/tests/` to osobne
skrzynie i `pub(crate)` jest z nich niewidoczny; dotyczy to `Dag`, `execute`, `Outcome`, `StepState`,
`StepEvent`, funkcji przejścia i `FakeDriver`. „Naprawa" przez przeniesienie testu do `#[cfg(test)]`
wewnątrz modułu złamałaby regułę „jedno kryterium, jedna ścieżka pliku" z `AGENTS.md` §2a.

## Niezmienniki

- **1 — `engine/` nie importuje `tauri::*`.** Tu łamie się cicho przez **string**, nie przez
  `use`: `checks/quick-boundary.sh` grepuje `-i tauri` po niekomentowanych liniach każdego pliku
  w `src-tauri/src/engine/`, więc literał ze ścieżką `src-tauri/...` w kodzie przewraca granicę.
  Ścieżki do plików niech przychodzą argumentem, nie stałą.
- **7 — anulowanie jest wartością, nie błędem.** `execute()` zwraca `Outcome`, nigdy
  `Err(Cancelled)`. I nigdy globalny `AtomicBool`: bool przecieka między biegami, więc drugi bieg
  po anulowanym startuje jako już anulowany i **kończy się w milisekundach z samymi `Cancelled`** —
  wygląda to jak szybki bieg, nie jak awaria. Stąd kryterium AC-6 uruchamia ten sam graf drugi raz
  na świeżym `CancellationToken`.
- **27 — żaden etap biegu nie jest zaszyty w Ruście.** `execute()` dostaje graf i go wykonuje;
  nie zna pojęć „recenzja", „bramka" ani „poprawka". Kusi tu skrót o jednej linii —
  `if node.kind == Review` albo `if cfg.run_review` — który wygląda niewinnie i **na zawsze**
  przypina ceremonię do kodu zamiast do konfiguracji workflow (decyzja D7). Skutek jest taki, że
  użytkownik chcący jednego agenta bez niczego i tak dostanie recenzję, bo nie ma jej jak wyłączyć.
  Węzeł z recenzentem jest dla planisty zwykłym krokiem i niczym więcej.
- **8 — `std::sync::Mutex` nigdy nie jest trzymany przez `await`.** Rejestrator w `fake.rs` kusi
  dokładnie do tego: `log.lock().push(mark); sleep(d).await;` w jednym wyrażeniu. To zakleszcza
  bieg przy `limit > 1` i wygląda jak zawieszenie agenta, nie jak błąd blokady. Zdejmij guard
  przed `await`, udokumentuj to **na polu**, a `clippy::await_holding_lock` (deny w
  `Cargo.toml`) pilnuje reszty.
- **11 — „ile naraz" musi znaczyć naraz.** Semafor bierzemy **wewnątrz** zadania z `JoinSet`,
  nie w pętli wysyłki [T7 §2.3]. Wersja z permitem w pętli przechodzi każdy test na górne
  ograniczenie (`peak <= limit`) i po cichu kasuje rozróżnienie `ready` / `running` oraz
  możliwość pokazania kroku jako zakolejkowanego.
- **19 — kod wyjścia to nie dowód.** `cargo test --test X` na pliku, w którym wszystkie testy są
  `#[ignore]`, kończy się zerem i melduje `0 passed`. Bramka to wyłapie, ale ty masz to wiedzieć
  wcześniej: jeśli oznaczasz test `#[ignore]`, komenda w `check:` musi nieść `-- --include-ignored`.
  W T-02 nie ma testów `#[ignore]` — wszystkie siedem biegnie.
- **24 — komentuj DLACZEGO, zwłaszcza incydent.** Przy `biased;` w `select!` i przy pobraniu
  permitu wewnątrz zadania ma stać datowany powód z numerem sekcji T7. Bez tego pierwszy
  „porządkujący" refaktor zdejmie oba i żaden test jednostkowy tego nie powie.

## Kryteria akceptacji

Zanim odpalisz `./verify.sh before`: wpuść **kompilujący się szkielet** (typy, sygnatury, ciała
zwracające jawnie złą wartość — pusty `Vec`, `None`, `StepState::Pending`). Test, który się nie
kompiluje, niczego nie uruchomił; `harness/gate.py` odrzuca `error[E0432]` / `unresolved import`
jako fałszywą czerwień. Odpal też raz `cargo test --no-run --tests` **przed** bramką: w tierze
`before` sprawdzenie ma 20 s, a zimna kompilacja zależności skończy się rc 124, którego bramka też
nie uznaje za czerwień.

## AC-1 Dwa niezależne kroki zajmują **nachodzące na siebie** okna czasu, a przy limicie 1 nie zachodzą wcale
check: cargo test --test engine_overlap

Graf: dwa węzły bez krawędzi. `Behaviour::Busy(300 ms)` w obu. `FakeDriver` zapisuje do wspólnego
rejestratora `Instant` wejścia (po pobraniu permitu, wewnątrz zadania) i wyjścia.
Bieg A, `limit = 2`: `min(end_a, end_b) - max(start_a, start_b) >= 150 ms`.
Bieg B, ten sam graf, `limit = 1`: `max(start_a, start_b) >= min(end_a, end_b)` — przecięcie puste.
Test musi być `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`. **Nie**
`start_paused`: czas wirtualny implikuje runtime jednowątkowy i przeskakuje do przodu, kiedy
runtime staje bezczynny, więc „nakładanie" przestaje cokolwiek znaczyć [T7 §8.1].

*Słaba asercja:* `assert!(states.iter().all(|s| *s == StepState::Succeeded))` plus
`assert!(elapsed < 2 * STEP)`. Przechodzi dokładnie ta implementacja, którą to kryterium istnieje,
żeby odrzucić — jeden worker w rozłącznych oknach, bo oba kroki naprawdę się skończyły, a suma
dwóch snów po 300 ms bywa poniżej progu na obciążonej maszynie. Rozróżnia: **zapisane przedziały**
i **bieg kontrolny z `limit = 1` w tym samym pliku** — jedna stała nie zaspokoi obu naraz.

## AC-2 Szczyt równoczesności nigdy nie przekracza limitu, a przy nadmiarze gotowych **dochodzi do limitu**
check: cargo test --test engine_concurrency_limit

Część property (`proptest`, 300 przypadków): losowe DAG-i, krawędzie tylko z niższego indeksu do
wyższego (acykliczne z konstrukcji [T7 §8.2]), `n ∈ 1..=10`, `limit ∈ 1..=4`, kroki
`Behaviour::Succeed`. Licznik `AtomicUsize` inkrementowany **wewnątrz ciała kroku**, maksimum
zapamiętane w drugim atomiku. Asercja: `peak <= limit`. Runtime budowany w ciele przypadku musi być
wielowątkowy (`Builder::new_multi_thread`) — na jednowątkowym `peak` bywa 1 zawsze i property
przechodzi, nic nie mierząc.
Część deterministyczna w tym samym pliku: 8 węzłów bez krawędzi, `limit = 3`,
`Behaviour::Busy(120 ms)`, asercja `peak == 3` — dokładnie, nie „co najmniej 1".

*Słaba asercja:* samo `peak <= limit`. Przechodzi je implementacja z jednym workerem
(`peak == 1` przy `limit == 4`) — czyli defekt poprzedniego prototypu, którego to zadanie ma nie powtórzyć.
Rozróżnia: `peak == 3` przy ośmiu gotowych i limicie 3.

## AC-3 Każdy węzeł biegnie dokładnie raz i żaden nie startuje przed końcem wszystkich rodziców
check: cargo test --test engine_order

`proptest`, 300 przypadków, ten sam generator DAG-ów co w AC-2, kroki `Behaviour::Succeed`.
`FakeDriver` zapisuje monotoniczny numer sekwencji przy wejściu i przy wyjściu.
Trzy asercje: `run_count[i] == 1` dla każdego `i`; dla każdej krawędzi `(p, c)` zachodzi
`finish_seq[p] < start_seq[c]`; wszystkie stany końcowe to `Succeeded`.
Model referencyjny (zbiór krawędzi) test wylicza **sam, w pliku testu** — nie wolno mu wołać
`dag.children()` ani `dag.in_degree()`, bo wtedy błąd w odwracaniu krawędzi jest niewidoczny
dla obu stron [T7 §8.2].

*Słaba asercja:* tylko „wszystkie stany to `Succeeded`". Przechodzi implementacja, która nie
woła `run_step` ani razu i od razu maluje wektor stanów, oraz taka, która uruchamia dzieci przed
rodzicami. Rozróżnia: licznik uruchomień równy 1 i porównanie numerów sekwencji na krawędziach.

## AC-4 `Dag::new` odmawia cyklu i krawędzi do nieistniejącego węzła — przy konstrukcji, nie przy biegu
check: cargo test --test engine_dag_construction

`Dag::new(n, edges) -> Result<Dag, DagError>`. Przypadki:
`[(0,1),(1,2),(2,0)]` → `Err`, a treść błędu nazywa co najmniej jeden węzeł z cyklu;
`[(0,1),(1,2),(2,1)]` → `Err` — **cykl przy istniejącym korzeniu** (węzeł 0 ma stopień 0);
`[(1,1)]` → `Err` (pętla własna);
`n = 3, [(0,9)]` → `Err` innego wariantu niż cykl;
`[(0,1),(0,2),(1,3),(2,3)]` → `Ok`, a `in_degree() == [0,1,1,2]`.

*Słaba asercja:* osobne `is_acyclic()`, które test woła wprost, podczas gdy `Dag::new` przyjmuje
wszystko. Wtedy planista dostaje graf bez korzeni, pętla kończy się przy `inflight == 0` w
pierwszym obrocie i **melduje bieg, w którym nic nie biegło**. Rozróżnia: asercja na typie zwrotnym
samego `Dag::new` **plus** przypadek `[(0,1),(1,2),(2,1)]` — sprawdzenie „czy istnieje węzeł o
stopniu 0" przechodzi go i przewraca się dopiero na liczeniu przetworzonych węzłów (Kahn).

## AC-5 Poniżej `failed` jest `skipped`, poniżej `cancelled` jest `cancelled`, a status terminalny nie jest nadpisywany
check: cargo test --test engine_cone_reason

Scenariusz A (`limit = 2`), graf `0→1→2`, `0→3`, `4→5`, krok 1 = `Behaviour::Fail`:
stany `[Succeeded, Failed, Skipped, Succeeded, Succeeded, Succeeded]`.
Scenariusz B (`limit = 2`), graf `0→1→2`, `0→3`, krok 1 = `Behaviour::Hang`, anulowanie po 150 ms:
stany `[Succeeded, Cancelled, Cancelled, Succeeded]` — węzeł 2 to **`Cancelled`, nie `Skipped`**.
Scenariusz C (remis), graf `0→2`, `1→2`, krok 0 = `Fail` (natychmiast), krok 1 = `Hang`,
anulowanie po 150 ms: węzeł 2 to `Skipped`. Reguła: **wygrywa powód, który wystąpił pierwszy;
status terminalny nigdy nie jest przepisywany.** Zapisz ją komentarzem przy przejściu po stożku.

*Słaba asercja:* `assert!(matches!(states[2], Skipped | Cancelled))` albo
`assert_ne!(states[2], Succeeded)`. Przechodzi ją dokładnie defekt z T7 §2.4, gdzie wszystko
poniżej anulowania melduje `Skipped` i UI kłamie o powodzie dla ośmiu kroków. Rozróżnia:
`assert_eq!` na konkretnym wariancie w A **i** w B, w jednym pliku — jedna stała nie zaspokoi obu.

## AC-6 Anulowanie jest wartością, dociera do środka kroku, zostawia każdy węzeł w stanie końcowym i nie wycieka do następnego biegu
check: cargo test --test engine_cancel_outcome

Graf `0→1→2`, `limit = 1`, krok 0 = `Behaviour::Hang` (30 s). Anulowanie po 100 ms. Asercje:
`execute()` wraca w mniej niż 1 s; zwraca wartość `Outcome` z `cancelled == true` (typ zwrotny nie
jest `Result<_, Cancelled>` — niezmiennik 7); w wektorze stanów **nie ma** `Pending`, `Ready` ani
`Running`; rejestrator `FakeDriver` ma wpis `CancelSeen` dla kroku 0. Na koniec ten sam graf
biegnie **drugi raz** na świeżym `CancellationToken` i wszystkie stany to `Succeeded`.

*Słaba asercja:* samo `assert!(elapsed < Duration::from_secs(1))`. Przechodzi `js.abort_all()`,
czyli anulowanie zadań Rusta bez powiadomienia kroku — w T-03 ten sam kształt zostawia żywy proces
systemowy palący limit. Rozróżnia: wpis `CancelSeen` (token wszedł do środka kroku) oraz drugi
bieg na nowym tokenie, który wywraca każdą wersję z globalnym `AtomicBool`.

## AC-7 Tabela przejść kroku odrzuca przejścia nielegalne, a `paused` nie jest stanem kroku
check: cargo test --test engine_step_states

`next(state, ev) -> Option<StepState>`, tabela z `docs/ARCHITECTURE.md` §5. Legalne:
`(Pending, InDegreeZero) → Ready`, `(Pending, UpstreamFailed) → Skipped`,
`(Pending, UpstreamCancelled) → Cancelled`, `(Ready, PermitAcquired) → Running`,
`(Running, ExitOk) → Succeeded`, `(Running, ExitError) → Failed`, `(Running, Timeout) → Failed`,
`(Running, UserCancelled) → Cancelled`, `(Failed, Retry) → Pending`, `(Cancelled, Retry) → Pending`,
`(Skipped, Retry) → Pending`. Nielegalne, wszystkie `None`: `(Succeeded, UserCancelled)`,
`(Succeeded, Retry)`, `(Cancelled, InDegreeZero)`, `(Skipped, PermitAcquired)`.
Serializacja: siedem stringów `"pending" "ready" "running" "succeeded" "failed" "cancelled"
"skipped"` deserializuje się do swoich wariantów (te same wartości niesie `CHECK` w schemacie
SQLite, T7 §5.4), a `"paused"` **jest odrzucane** — pauza jest stanem biegu [T7 §9.3].

*Słaba asercja:* implementacja `fn next(_from, ev) -> Option<StepState> { Some(target_of(ev)) }`,
która ignoruje stan wejściowy. Przechodzi każdą asercję na przejściach legalnych, a w biegu pozwala
anulować krok, który już się udał, i policzyć jego dzieci drugi raz. Rozróżnia: cztery przypadki
zwracające `None` i odrzucenie `"paused"`.

## Świadomie poza zakresem

- **Prawdziwe procesy, grupy procesów, SIGTERM→SIGKILL, limit czasu kroku** — T-03. Twój planista
  woła domknięcie `run_step` i nie wie, że po drugiej stronie kiedyś będzie `claude`.
- **`trait AgentDriver`, `AgentEvent`, `ClaudeDriver`, `drivers/mod.rs`** — T-04. Trait z jedną
  implementacją to trait wymyślony; dostaje dwie dopiero w T-04 i T-10. Twój `FakeDriver` **nie
  implementuje żadnego traitu** i nie musi go implementować później: dublerem na poziomie sterownika
  są w T-04 skrypty na dysku, bo one przechodzą prawdziwą ścieżkę uruchomienia procesu.
- **Czytanie NDJSON, tee surowego logu, kuracja zdarzenie→linia** — T-05.
- **Zapis `pid`, `pgid`, `status` do SQLite** — T-06. Planista nie dotyka bazy; niezmiennik 2.
- **Suwak „How many agents at once?", `add_permits` w locie, pauza przy limicie dostawcy** — T-21.
- **Walidacja workflow przy zapisie (cykle, nakładające się ścieżki, osierocone kroki)** — T-12.
  Odmowa cyklu w `Dag::new` to ostatnia linia obrony, nie pierwsza; nie buduj tu komunikatów dla UI.
- **Odzyskiwanie po awarii, `interrupted`, sprzątanie po `pgid`** — T-20.
- **Ponowienie kroku z UI** — T-15. `next(Failed, Retry)` istnieje, ale w tej fazie nikt jej nie woła.
- **`petgraph` / `daggy`** — nie dodajemy. Listy sąsiedztwa wystarczą, cykl jako *ścieżka* będzie
  potrzebny dopiero, kiedy edytor będzie musiał go narysować [T7 §9.4].

<!-- OWNS
src-tauri/src/lib.rs
src-tauri/src/engine/mod.rs
src-tauri/src/engine/dag.rs
src-tauri/src/engine/scheduler.rs
src-tauri/src/engine/step.rs
src-tauri/src/engine/drivers/fake.rs
src-tauri/tests/engine_overlap.rs
src-tauri/tests/engine_concurrency_limit.rs
src-tauri/tests/engine_order.rs
src-tauri/tests/engine_dag_construction.rs
src-tauri/tests/engine_cone_reason.rs
src-tauri/tests/engine_cancel_outcome.rs
src-tauri/tests/engine_step_states.rs
-->
