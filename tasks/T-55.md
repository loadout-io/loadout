# T-55 — krok „sprawdź": komendę odpala silnik, werdykt wystawia Loadout, nigdy agent

Zamknięty zbiór rodzajów kroku ma dziś dwa elementy: `agent` i `checkpoint`. Pierwszy odpala
vendora i wraca z tym, co agent **powiedział**. Drugi zatrzymuje bieg i pyta człowieka. Żaden
nie umie tego, co robi `verify.sh`: uruchomić komendy należącej do Loadouta, przeczytać jej
wyniku i **samemu** orzec, czy przeszła. Pomiar tego braku jest już zapisany i nie jest niczyją
opinią — `docs/harness-as-workflow.md`, ustalenie U-1: dwa z sześciu etapów naszego własnego
harnessu (`gate`, `land`) nie wyrażają się w edytorze i **oba przewracają się o tę samą
brakującą rzecz**. Stoją dziś na kafelku kontrolnym, czyli na pytaniu do człowieka, bo to jest
jedyna uczciwa odpowiedź, jaką schemat umie dać.

**Cicha porażka numer jeden — ta, przed którą U-1 broni wprost.** Zrobić z etapu sprawdzenia
krok agenta o instrukcji „uruchom `./verify.sh full` i powiedz, czy przeszło". Plik się waliduje,
bieg startuje, transkrypt mówi `checks passed`, kafelek jest zielony — i sprzedaliśmy jedyne
rozróżnienie, dla którego ten produkt powstał: **co agent powiedział** kontra **co się stało**
(`docs/research/projects/00-SYNTHESIS.md` §2.1). Nikt tego nie zgłosi, bo wszystko wygląda na
skończone.

**Cicha porażka numer dwa — ta, która jest w produkcie DZISIAJ.** Pętla z limitem tur weszła
2026-08-19 (`Link::max_turns`, `workflow::unroll`, `Live::verdict_after`) i domyka się na wierszu
`outcome: pass` napisanym przez agenta-sędziego. Zmierzone na czystym drzewie: `outcome:` jako
znacznik werdyktu nie występuje w `src-tauri/src/` ani w `src/` **nigdzie poza funkcją, która go
czyta** (`memory::handoff::verdict_in`). Protokół werdyktu ma więc wyłącznie połowę czytającą —
nic w produkcie nie mówi sędziemu, żeby ten wiersz napisał. Sędzia, który go nie napisze, dostaje
`Verdict::Fail` z domyślnej wartości i pętla kręci się do wyczerpania limitu; sędzia, który
napisze go z uprzejmości nad czerwonymi testami, zamyka pętlę na obietnicy. Oba przypadki
kosztują prawdziwe tury i oba wyglądają jak działający produkt.

**Cicha porażka numer trzy — finansowa, więc niewidoczna aż do rachunku.** Komenda sprawdzająca
rozwidla dzieci: `verify.sh` woła `cargo`, `cargo` woła `rustc`, `npm test` woła `vitest`.
Krok, który zabija bezpośrednie dziecko zamiast grupy, zostawia wnuki pod PID 1 — zmierzone
w tym repo jako `A after kill: total=2 orphaned=2` [T7 §3.1], a w tej fali drugi raz na hakach
repo gospodarza: jeden bieg zostawił **14 sierot**, eksperymenty łącznie 30
[zmierzone 2026-08-19]. Sierota trzyma też stdout, więc potok nigdy nie dochodzi do EOF i krok
wygląda na wiecznie biegnący.

**Zasada nadrzędna tej fali, powiedziana dla tego zadania.** Harness jest **nasz**. Z cudzego
tekstu — z tego, co powiedział agent, z tego, co stoi w cudzych ustawieniach — dziedziczymy
**treść**, nigdy **maszynerię**, i dziedziczymy ją przez **przepisanie do siebie**, nie przez
wczytanie cudzego mechanizmu. Krok „sprawdź" jest tą zasadą o jedno piętro niżej: Loadout
uruchamia sprawdzenie **sam** i orzeka **sam**. Zdanie agenta może być treścią przekazania;
werdyktem nie będzie nigdy.

**Read first:**
`docs/harness-as-workflow.md` (CAŁY; ustalenie **U-1** jest uzasadnieniem tego zadania i wylicza
trzy rzeczy, które ten rodzaj kroku musi umieć; blok JSON „Blok, który czyta test" jest wyrocznią
`src-tauri/tests/it/harness_workflow_findings_match_doc.rs` — patrz „Świadomie poza zakresem"),
`docs/DECISIONS-LOCKED.md` (**D7**, tabela „Ceremonia jest elementem grafu" — wiersz
`bramka (verify.sh) → krok typu „sprawdź" — uruchamia twoje checki`; oraz **D6**, którego regułą
jest „wszystko, co **vendor** wprowadzi, konfigurujemy per agent"),
`src-tauri/src/workflow/mod.rs` (enum [`Step`], nagłówek modułu o `deny_unknown_fields`
i `#[serde(flatten)] extra`, [`Link::max_turns`] i akapit z 2026-08-19 o tym, dlaczego powrót
jest polem na strzałce, a nie nowym kafelkiem),
`src-tauri/src/workflow/check.rs` (`notes()`, `Facts`, `facts()`, `a_step_without_an_agent`,
`a_step_without_a_task`, `one_folder_two_steps` — zwłaszcza wiersz, w którym `folder: None`
**wyłącza** krok z reguły kolizji),
`src-tauri/src/workflow/file.rs` (`CURRENT`, `MIGRATIONS`, `load`, `save` i kolejność
„najpierw sprawdź, potem dotknij dysku"),
`src-tauri/src/engine/drivers/mod.rs` (trait `AgentDriver` — po to, żeby wiedzieć, czego krok
„sprawdź" **nie** implementuje; oraz `Policy`, `Outcome`, `FinishReason`),
`src-tauri/src/engine/supervisor.rs` (`spawn`, `StdinPlan`, `Supervised::stop`, `GroupProof`,
`GroupId`, `DEFAULT_GRACE`, `PASSTHROUGH`, oraz `run_with_deadline` — i powód, dla którego
**nie wolno go tu użyć**, opisany przy AC-3),
`src-tauri/src/memory/handoff.rs` (`verdict_in`, `Verdict`, `VERDICT_MARK` i trzy akapity
o tym, dlaczego decyduje ostatni znacznik i dlaczego musi być całym wierszem),
`src-tauri/src/commands/run.rs` (`plan_step`, `plan_agent`, `Job`, `Planned`, `Live::step`,
`Live::hand_over`, `Live::already_settled`, `Live::verdict_after`),
`src-tauri/tests/it/runcmd_loop.rs` (żywa pętla na dublerze sterownika — wzór fikstury dla AC-5),
`src-tauri/tests/it/supervisor_group_death.rs` (wzór dowodu śmierci: skrypt-rodzic, dwoje
wnucząt ze znacznikiem w `argv`, `kill(-pgid, 0)` i skan `ps`),
`AGENTS.md` (§2a — kontrakt kryterium; §3 niezmienniki **1, 3, 6, 9, 10, 12, 19, 21, 24, 25, 27**).

**Bramka wejściowa.** To zadanie stoi na pętli, która weszła 2026-08-19 (commit `41e76b1`).
Jeśli w drzewie nie ma `src-tauri/tests/it/runcmd_loop.rs` albo `Link::max_turns` nie istnieje —
zatrzymaj się i zapytaj człowieka (AGENTS.md §7), bo AC-5 nie ma wtedy czego domykać.

## Rozstrzygnięcie architektoniczne: to NIE jest trzeci rodzaj kafelka

Zapisz to w kodzie, bo bez tego pierwszy recenzent słusznie zaświeci: `workflow/mod.rs` nosi dziś
zdanie *„Dwa rodzaje kafelka. To jest cała lista i ma taka zostać (D6, ARCHITECTURE §6b)"*, a to
zadanie dokłada wariant do tego samego enuma.

Sprzeczności nie ma i rozstrzyga ją plik stojący nad `AGENTS.md`. Reguła D6 brzmi w całości:
*„Wszystko, co **vendor** wprowadzi, konfigurujemy per agent — nigdy jako nowy typ węzła"*,
a jej konsekwencja z `ARCHITECTURE.md` §6b ma dopisany zakres: *„Liczba rodzajów kafelka zostaje
dwa **niezależnie od tego, ile funkcji dowiozą vendorzy**"*. D6 broni płótna przed powtarzaniem
funkcji Claude'a i Codeksa. Krok „sprawdź" nie jest funkcją żadnego vendora — jest mechanizmem
Loadouta, wymienionym **z nazwy** w tabeli D7 (`bramka (verify.sh) → krok typu „sprawdź" —
uruchamia twoje checki`). Obie decyzje zapisano tego samego dnia i obie są zamknięte.

Dlatego zdanie w `workflow/mod.rs` **poprawiasz razem z powodem i datą** (niezmiennik 24), a nie
kasujesz: „dwa rodzaje kroku agenta / kafelka wobec vendorów" zostaje prawdą, dochodzi trzeci
rodzaj, który vendora nie zna.

Drugie rozstrzygnięcie, ważniejsze dla silnika: **krok „sprawdź" nie nazywa ETAPU, tylko RODZAJ
STEROWNIKA** — dokładnie tak, jak `claude` stoi obok `codex`. Niezmiennik 27 zakazuje `if
review_enabled` i każdego innego warunku nazywającego etap; nie zakazuje ramienia `match`
mówiącego, **czym** jest kafelek (`Step::Agent` i `Step::Checkpoint` stoją tam od pierwszego dnia).
Test rozróżniający jest jednozdaniowy: *czy da się zapisać graf, w którym ten krok stoi w innym
miejscu albo nie stoi wcale?* Dla kroku „sprawdź" — tak, trywialnie, bo kolejność mieszka
wyłącznie w grafie. Dla etapu zaszytego w Ruście — nie, i dlatego tamten jest zakazany.

Konsekwencja praktyczna: implementacja kroku „sprawdź" mieszka w `engine/drivers/command.rs`,
obok `claude.rs` i `absent.rs`, a nie w planiście. Planista dostaje z niej wynik i nie wie,
że ten krok „jest bramką".

## Kto to robi

- **Agent:** `rust-core`
- **Druga opinia:** `codex` (pisze `claude` — decyzja D3)
- **Artefakty biegu:** `runs/T-55/` (transkrypt, plik wyników, plan) — nigdy `$TMPDIR`

## Co to zadanie posiada

- `src-tauri/src/engine/drivers/command.rs` — **cała** implementacja rodzaju sterownika:
  `CheckSpec` (co uruchomić), `CheckReport` (co z tego wyszło), czysta funkcja werdyktu,
  dopasowanie wzorca dowodu i `CommandDriver::run`, który startuje komendę **przez
  `engine::supervisor::spawn`**. Jedno ograniczenie na cały plik, dokładnie jak w `codex.rs`
  z T-10: **zero `#[cfg(unix)]`, zero `libc`, zero numerów sygnałów** — zabijanie i eskalacja
  należą do `supervisor.rs` (niezmiennik 3, egzekwuje `checks/quick-boundary.sh`).
- `src-tauri/src/engine/drivers/mod.rs` — **wyłącznie** wiersz `pub mod command;` obok
  `pub mod absent;` i `pub mod claude;`. Żadnej innej zmiany: trait `AgentDriver` i jego typy
  są własnością T-04 i krok „sprawdź" ich nie dotyka — to jest treść AC-4, nie szczegół.
- `src-tauri/src/workflow/mod.rs` — wariant `Step::Check(CheckStep)` i struktura `CheckStep`,
  plus poprawka zdania o dwóch rodzajach kafelka (sekcja wyżej).
- `src-tauri/src/workflow/check.rs` — `facts()` dla nowego wariantu i **jedna nowa reguła**
  (krok „sprawdź" bez wzorca dowodu i bez komendy).
- `src-tauri/src/commands/run.rs` — `Job::Check`, ramię w `plan_step`, wykonanie kroku
  w `Live::step` i podpięcie werdyktu pod `Live::verdict_after`.
- `src-tauri/tests/it/main.rs` — pięć wierszy `mod`. Plik testowy bez wpisu kompiluje się do
  niczego i **wygląda dokładnie jak zestaw, który przeszedł** (`checks/quick-tests-listed.sh`).
- Pięć plików w `src-tauri/tests/it/` — po jednym na kryterium, bo `check:` wskazuje **jeden**
  plik po ścieżce.

### Kształt, na którym stoją kryteria

Nazwy są propozycją, kryteria są kontraktem — ale nie wymyślaj trzeciego pola treści, bo
schemat jest tą jedną rzeczą, którą użytkownik może **stracić**.

```rust
// workflow/mod.rs — wariant enuma Step, na drucie "kind": "check"
pub struct CheckStep {
    pub id: String,
    pub name: String,               // „Run the checks" — to widzi człowiek
    #[serde(default)] pub command: String,   // wiersz powłoki, dosłownie jak wpisał go człowiek
    #[serde(default)] pub proof: String,     // wzorzec dowodu
    #[serde(default)] pub folder: Folder,
    #[serde(default)] pub at: Point,
    #[serde(flatten)] pub extra: Map<String, Value>,
}
```

`#[serde(default)]` na `command` i `proof` jest rozstrzygnięciem, nie niedbałością: plik
poprawiony ręcznie ma się **wczytać** i dostać zdanie z `check()` przy kafelku, a nie odbić się
o błąd serde, którego użytkownik nie umie umiejscowić (T3 §8.4, ten sam powód co przy `at`).
Odmowa pada przy **zapisie**, a nie przy odczycie.

**Wzorzec dowodu** to zwykły tekst z **jednym** metaznakiem: sekwencja `(\d+)` znaczy „co
najmniej jedna cyfra", wszystko poza nią jest literałem, a dopasowanie jest szukaniem podciągu
w połączonym stdout i stderr. To celowo **ta sama notacja**, którą człowiek pisze w linii
`expect:` naszego własnego harnessu (AGENTS.md §2a punkt 4) — jedna notacja, jedno znaczenie,
w bramce i w aplikacji. Skrzyni `regex` w tym drzewie nie ma; dopisanie zależności to zmiana
`Cargo.toml`, czyli ścieżki spoza OWNS, czyli zatrzymanie się i pytanie do człowieka
(AGENTS.md §7). **Nie dopisuj jej po cichu** — jeśli uważasz, że dwadzieścia wierszy własnego
dopasowania to zła odpowiedź, powiedz to w uwagach zamiast to zrobić.

```rust
// engine/drivers/command.rs
pub struct CheckSpec  { pub command: String, pub proof: String, pub cwd: PathBuf }
pub struct CheckReport { pub passed: bool, pub exit_code: Option<i32>, pub matched: bool,
                         pub output: String, pub took: Duration }
pub enum CheckHow      { Ran(CheckReport), Stopped(GroupProof), Overdue(GroupProof) }
pub struct CheckEnd    { pub group: GroupId, pub how: CheckHow }

#[must_use] pub fn proof_matches(proof: &str, output: &str) -> bool;
#[must_use] pub fn passed(exit_code: Option<i32>, output: &str, proof: &str) -> bool;
```

Trzy rzeczy, których ten plik **nie** ma robić i które ratują rundę:

1. **Nie `Command::spawn` z ręki.** `process_group(0)`, `env_clear()` plus [`PASSTHROUGH`]
   i potoki mieszkają w `supervisor::spawn` — polityka jest jedna i w rdzeniu (niezmiennik 23).
2. **Nie `run_with_deadline`.** Wygląda idealnie (robi całą eskalację), ale podaje
   `StdinPlan::Null` i **nigdy nie opróżnia potoków**. `cargo test` wypisujący więcej niż
   ~64 KB staje na `write`, krok wisi na 100% „running", a wyjścia — czyli jedynej rzeczy,
   z której powstaje werdykt — i tak nie ma. Czytaj stdout i stderr do EOF sam.
3. **Nie „sprawdzenie", które sprawdza samo siebie** (AGENTS.md §4). Krok bez komendy nie jest
   krokiem, który przeszedł.

Limit czasu jednego kroku „sprawdź" jest **stałą w `command.rs`** równą 30 minutom, z powodem
dopisanym przy niej: tyle wynosi budżet naszej własnej pełnej bramki (1800 s). Pola na kafelku
nie ma — patrz „Świadomie poza zakresem".

Komenda idzie do `/bin/sh -c <command>`, bo człowiek napisze `./verify.sh full && npm test`,
a nie listę argumentów. Dwie rzeczy do dopisania komentarzem obok (niezmiennik 24): (a) literał
`/bin/sh` jest długiem — w dniu, w którym pojawi się Windows, wybór powłoki przenosi się do
`supervisor.rs`, do tej samej gałęzi `cfg`, w której stoi `ProcessGroup::leader()`; (b)
niezmiennik 9 **nie jest tu złamany**: zakazuje promptów i sekretów w argv, a komenda
sprawdzająca jest ani jednym, ani drugim i ma być widoczna w `ps`, żeby człowiek poznał
swój własny bieg.

## Niezmienniki, których to zadanie dotyka

- **19 — kod wyjścia to nie dowód.** To jest cała treść AC-2 i powód, dla którego pole `proof`
  jest obowiązkowe. Cicho łamie się tak: werdykt liczony z samego `rc == 0`. Suita, która nie
  uruchomiła ani jednego testu, wychodzi zerem; `os._exit(0)` na poziomie modułu zazielenia
  wszystko.
- **6 — zabijamy grupę i dowodzimy, że nie żyje.** Cicho łamie się tak: `child.kill()` zamiast
  `Supervised::stop`, albo `GroupProof` przeczytany jako `Ok` i porzucony. Wnuk pod PID 1 pali
  limit i trzyma stdout.
- **10 — `tokio::time::timeout` anuluje zadanie Rusta, nie proces.** Cicho łamie się tak:
  `timeout(limit, wait()).await` i `return Overdue` bez ani jednego sygnału.
- **12 — dwa kroki nie mogą pisać po tych samych ścieżkach.** Cicho łamie się tak:
  `facts()` oddaje dla kroku „sprawdź" `folder: None`, bo „to tylko sprawdzenie" — a wtedy
  `one_folder_two_steps` **pomija go całkowicie** (`let (Some(mine), Some(theirs)) = … else
  continue`) i dwa równoległe kroki budujące w jednym katalogu zapisują się bez słowa.
  `cargo test` pisze po `target/`; to nie jest krok tylko do odczytu.
- **25 — migracje są addytywne i idempotentne.** `CURRENT` zostaje `1`, `MIGRATIONS` zostaje
  puste. Cicho łamie się tak: podniesienie `format` na `2` „bo doszedł rodzaj kroku" — dodanie
  wariantu do enuma tagowanego wewnętrznie **jest** addytywne, a podniesienie wersji czyni
  każdy istniejący plik nieczytelnym dla starszego builda i wymaga migracji, której nie ma.
- **21 — nie pisz artefaktu, którego nikt nie czyta.** Wyjście komendy ma dwóch czytelników:
  werdykt i przekazanie do następnego kroku. Bez tego drugiego pętla z AC-5 jest bezużyteczna,
  bo runda 1 nie wie, co padło w rundzie 0.
- **27 — żaden etap nie jest zaszyty w Ruście.** Patrz rozstrzygnięcie wyżej. Cicho łamie się
  tak: `if step.is_gate() { … }` w planiście albo domyślne wstawienie kroku „sprawdź" do nowego
  workflow.
- **1 i 3 — silnik nie zna okna, kod platformowy tylko w `supervisor.rs`.** Cicho łamią się
  tak: `use crate::ipc::Line;` w `command.rs`, żeby „od razu pokazać wynik", i `libc::kill`
  w anulowaniu, bo to trzy linijki.

## Kryteria akceptacji

**Jak zaczerwienić to poprawnie.** `clippy::todo` jest `deny` w `[workspace.lints]`, więc
sygnatury zwracają świadomie złą wartość, nigdy `todo!()`: `passed` oddaje `false`,
`proof_matches` oddaje `false`, `CommandDriver::run` oddaje `CheckHow::Ran` z pustym wyjściem
i `exit_code: None`, nowa reguła w `check.rs` nie dopisuje ani jednej uwagi. Wariant
`Step::Check` i wiersz `pub mod command;` **muszą istnieć przed** `./verify.sh before` — test,
który się nie kompiluje, niczego nie uruchomił, a `unresolved import` jest na liście
`NOT_A_REAL_RED` (AGENTS.md §2a punkt 5).

Uwaga na kryterium bilansowe: **AC-4 przechodzi na szkielecie, który nie robi nic** — zero
wywołań sterownika agenta jest przecież prawdą także wtedy, gdy nie dzieje się nic. Dlatego
AC-4 ma obok licznika asercję pozytywną (krok naprawdę się wykonał i ma werdykt), i to ona
jest tym, co w warstwie `before` pada.

Atrapa `AgentDriver` mieszka **w pliku testu**; wzór gotowy w
`src-tauri/tests/it/runcmd_loop.rs`. Nie używaj `engine::drivers::fake` — to dubler planisty
i nie implementuje tego traitu. Każdy plik testu zaczyna się od
`#![allow(clippy::unwrap_used, clippy::expect_used)]` z powodem: `checks/full-clippy.sh` biegnie
`--all-targets -- -D warnings`. **Żadnego `#[ignore]`** — ani jedna linia `check:` w tym pliku
nie niesie `--include-ignored`, więc test oznaczony `#[ignore]` zamelduje `0 passed`, a to nie
jest dowód (niezmiennik 19). Wolno tak, bo procesy, które te testy odpalają, to `/bin/sh`
i skrypty z `tempfile::tempdir()` — milisekundy i zero pieniędzy, w przeciwieństwie do testów
z prawdziwym `claude`.

## AC-1 Schemat przyjmuje krok „sprawdź", odmawia zapisu bez dowodu i nie łamie starych plików
check: cargo test --test it check_step_schema::
expect: (\d+) passed

Cztery grupy asercji na `workflow::{file, check}`:

(a) **Obieg tam i z powrotem.** `WorkflowFile` z jednym `Step::Check { command: "./verify.sh
full", proof: "(\\d+) passed", .. }` zapisuje się i wczytuje jako **równy** oryginałowi
(`PartialEq` na całej strukturze, nie porównanie pojedynczych pól). W tekście pliku klucz
rodzaju brzmi dokładnie `"kind": "check"` — porównaj na sparsowanym `serde_json::Value`, nie
gerpem po napisie. Nieznany klucz dopisany ręcznie do kroku (`"note": "z nowszego builda"`)
przeżywa obieg w `extra` (T3 §3.2).

(b) **Brak dowodu to odmowa, nie ostrzeżenie.** Dla kroku z `proof: ""` `check()` oddaje uwagę
o `Level::Problem` (nie `Warning`) wskazującą `step_id` tego kroku, a `file::save` zwraca
`SaveError::Refused` **i nie dotyka dysku**: bajty poprzedniej wersji pliku są po odmowie
identyczne. To samo dla `command: ""`. Zdanie uwagi jest po angielsku, mówi, co zrobić, i nie
niesie słowa „regex", „exit code" ani „pattern" w znaczeniu żargonowym (niezmiennik 14,
`checks/quick-vocabulary.sh`). Powód, dla którego to jest problem, a nie ostrzeżenie — inaczej
niż przy kroku agenta bez agenta: kafelek bez agenta czeka na wybór z listy, którą człowiek
zaraz zobaczy, a krok sprawdzający bez dowodu **jest gotowy i kłamie** — uruchomi się i orzeknie
na samym kodzie wyjścia (niezmiennik 19).

(c) **Stary plik dalej się wczytuje.** Plik z `"format": 1` i wyłącznie krokami `agent`
i `checkpoint` — dosłowny tekst w teście, nie zbudowany z naszych typów — wczytuje się bez
błędu, a `file::CURRENT == 1` i `file::MIGRATIONS.is_empty()`. Ta asercja pilnuje migracji
addytywnej (niezmiennik 25) i jest jedyną rzeczą, która wyłapie podniesienie wersji formatu.

(d) **Reguła kolizji folderów widzi ten krok.** Dwa kroki „sprawdź" bez żadnej strzałki między
nimi, oba z `Folder::Project`, dają uwagę z `one_folder_two_steps`. Bez tej asercji `facts()`
z `folder: None` przechodzi i niezmiennik 12 przestaje po cichu obowiązywać dla całej klasy
kroków.

*Słaba asercja:* `assert!(!check(&workflow).is_empty())` po skasowaniu wzorca dowodu. Przechodzi
dla implementacji, która oddaje `Level::Warning` — a ostrzeżenie **nie blokuje `save()`**
(`file::save` odrzuca wyłącznie na pierwszym `Level::Problem`), więc plik, który miał być
odrzucony, ląduje na dysku i biegnie. Rozróżniają to dwie asercje: porównanie `note.level`
z `Level::Problem` **oraz** odczyt bajtów pliku po odmowie. Druga słaba wersja to sam obieg
tam i z powrotem: przechodzi dla builda, który podniósł `format` na `2` — czyli dla takiego,
w którym każdy workflow zapisany wczoraj przestaje się otwierać. Rozróżnia to punkt (c).

## AC-2 Werdykt powstaje z dwóch rzeczy naraz: kodu wyjścia ORAZ dopasowania wzorca
check: cargo test --test it check_step_verdict::
expect: (\d+) passed

Tabela czterech przebiegów na czystej funkcji `command::passed`, wszystkie w jednym teście,
wzorzec we wszystkich ten sam: `(\d+) passed`.

| | kod wyjścia | wyjście komendy | werdykt |
|---|---|---|---|
| (a) | `Some(0)` | `test result: ok. 12 passed; 0 failed` | **przeszło** |
| (b) | `Some(0)` | `error: no test target matched` | **nie przeszło** |
| (c) | `Some(1)` | `test result: FAILED. 11 passed; 1 failed` | **nie przeszło** |
| (d) | `Some(101)` | `thread 'main' panicked` | **nie przeszło** |

Przypadek (b) jest sednem niezmiennika 19 i jedynym powodem, dla którego pole `proof` w ogóle
istnieje: suita, która nie uruchomiła **ani jednego** testu, wychodzi zerem. Przypadek (c) jest
jego lustrem: licznik przejść jest w wyjściu, a mimo to komenda padła.

Do tego trzy asercje na samym dopasowaniu (`command::proof_matches`), bo bez nich metaznak jest
ozdobą: `(\d+) passed` **nie** dopasowuje się do ` passed` (zero cyfr to za mało), dopasowuje się
do `1 passed` i do `1234 passed`, a wzorzec bez metaznaku (`0 failed`) jest zwykłym podciągiem.
Wyjście do dopasowania jest **złączeniem stdout i stderr** — `cargo test` pisze podsumowanie na
stdout, a `npm` swoje na stderr, i wzorzec ma trafić w oba. Piąta asercja: `exit_code: None`
(proces zginął od sygnału, więc kodu po prostu nie ma) **nigdy** nie jest przejściem — `None`
to nie zero.

*Słaba asercja:* przetestowanie wyłącznie przekątnej, czyli (a) i (d). Przechodzi dla trzech
różnych implementacji naraz: tej, która czyta sam kod wyjścia i wzorzec ignoruje; tej, która
czyta samo dopasowanie i kod wyjścia ignoruje; i tej poprawnej. Rozróżniają je **wyłącznie**
przypadki spoza przekątnej: (b) zabija pierwszą, (c) zabija drugą. Kryterium bez obu z nich
jest kryterium, które nie potrafi zaświecić.

## AC-3 Komenda biegnie pod tym samym nadzorem co agent i zostawia po sobie dowód śmierci
check: cargo test --test it check_step_process_group::
expect: (\d+) passed

Fikstura jak w `supervisor_group_death.rs`: skrypt `#!/bin/sh` w `tempfile::tempdir()`, który
odpala **dwoje dzieci w tle**, każde z unikalnym znacznikiem w `argv` (pętla krótkich snów, nie
pojedyncze `sleep` — powłoka exec-optymalizuje ostatnią komendę i znacznik znika z `argv`
[T7 §8.2]), a sam kręci się dalej. To jest kształt każdej prawdziwej komendy sprawdzającej:
`verify.sh` woła `cargo`, `cargo` woła `rustc`.

Asercje: (a) `CheckEnd::group` jest znane **zanim** cokolwiek zostanie przeczytane z wyjścia —
`pgid` jest zwykłą wartością dostępną od razu po starcie, nie czymś wyłuskanym z pierwszej linii
[T7 §6.2]; (b) po anulowaniu w trakcie `how` jest `CheckHow::Stopped(GroupProof::Dead { .. })` —
**wartością, nie błędem** (niezmiennik 7); (c) test pyta **jądro, nie nas**: `kill(-pgid, 0)`
zwraca `ESRCH`; (d) skan `ps -eo pid,ppid,pgid,args` po unikalnym znaczniku nie znajduje ani
jednego procesu, **w tym żadnego z `ppid == 1`** — to jest ta asercja, która widzi wnuki,
których nasz `wait()` nie zobaczy nigdy; (e) `cancel` zawołane drugi raz jest bezbłędne i nie
produkuje drugiego wyniku; (f) `command.rs` przechodzi granicę z niezmiennika 3 — to sprawdza
`checks/quick-boundary.sh`, nie ten test, i jest tu wymienione, żebyś nie „poprawił" testu
przez `libc::kill` w sterowniku.

Sygnał zerowy w pliku testu jest w porządku: `checks/quick-boundary.sh` wyłącza ścieżki
`*/tests/*` ze wszystkich trzech granic, **po ścieżce, nigdy po treści**, bo test nie jest
częścią wysyłanego artefaktu — a ten konkretny test z definicji pyta system operacyjny zamiast
naszego kodu (niezmiennik 20).

*Słaba asercja:* `assert!(matches!(how, CheckHow::Stopped(_)))`. Przechodzi dla implementacji,
która owija `wait()` w `tokio::time::timeout` i wraca, nie wysławszy ani jednego sygnału —
czyli dla tej, która zostawia żywą grupę mielącą w tle (niezmiennik 10; osierocony proces to
błąd finansowy, nie higieniczny). Przechodzi też dla `child.kill()`, bo bezpośrednie dziecko
naprawdę ginie, a `wait()` naprawdę wraca ze statusem „zabity" — dokładnie ten pomiar zwrócił
`A after kill: total=2 orphaned=2`. Rozróżniają to wyłącznie asercje (c) i (d), bo obie mierzą
system operacyjny, a nie naszą wartość zwrotną. W tej fali ten sam kształt zmierzono drugi raz:
hak `PreToolUse` repo gospodarza startuje proces we własnej grupie, jego dziecko dostaje
`ppid=1` i **przeżywa wyjście `claude`** — jeden bieg zostawił 14 sierot [zmierzone 2026-08-19].

## AC-4 Krok „sprawdź" nie tworzy sesji agenta — ani modelu, ani promptu, ani wywołania
check: cargo test --test it check_step_has_no_agent::
expect: (\d+) passed

Żywy bieg (`run_workflow_inner`) na workflow, który ma **dokładnie jeden** krok — sprawdzający —
i na bibliotece agentów, która jest **pusta**. Atrapa `AgentDriver` liczy wywołania `start`,
`probe` i `id`; fabryka `Drivers` jest opakowana w domknięcie liczące, ile razy ktoś w ogóle
poprosił o sterownik agenta.

Asercje: (a) licznik `start` na atrapie wynosi **zero**; (b) licznik wywołań fabryki `Drivers`
wynosi **zero** — to jest asercja mocniejsza niż (a), bo łapie także implementację, która
przechodzi przez `plan_agent`, dostaje sterownik i dopiero potem się rozmyśla; (c) bieg
**kończy się powodzeniem**, mimo że katalog biblioteki agentów nie istnieje — implementacja
routująca ten krok przez `plan_agent` przewróciłaby się tu na `RunError::NoAgentsSaved`, więc
ta asercja jest jednocześnie kontrolą negatywną; (d) w `run.json` krok stoi w stanie końcowym,
jego pole vendora jest **puste** (żadnej wymyślonej etykiety `"local"` ani `"loadout"`), a jego
wynik pochodzi z wyjścia komendy — asercja porównuje zapisany tekst z tym, co skrypt wypisał;
(e) żaden `RunSpec` nie powstał — atrapa nie ma czego zapamiętać, a asercja stoi na jej
wewnętrznym wektorze, nie na liczniku.

*Słaba asercja:* `assert!(spec.model.is_none())`. Przechodzi dla implementacji, która **odpala
agenta bez modelu** — czyli płaci za turę u vendora, żeby ten opowiedział o wyniku komendy.
To jest dokładnie ta cicha porażka, przed którą broni U-1. Druga słaba wersja jest gorsza,
bo wygląda na mocną: sam licznik równy zeru przechodzi dla **szkieletu, który nie robi nic**,
i zazieleniłby to kryterium w warstwie `before`. Rozróżniają to asercje (c) i (d): krok musiał
naprawdę wystartować, naprawdę skończyć i naprawdę zapisać wynik wzięty z wyjścia komendy.

## AC-5 Pętla domyka się na werdykcie kroku „sprawdź", a nie na słowie agenta
check: cargo test --test it check_step_closes_the_loop::
expect: (\d+) passed

Graf: `s_write` (agent, atrapa sterownika) → `s_check` (krok sprawdzający), plus powrót
`s_check → s_write` z `max_turns: 3`. Komenda `s_check` to skrypt dopisujący wiersz do pliku
licznika w `tempfile::tempdir()` (ścieżka bezwzględna wpisana wprost w komendę — środowisko
jest czyszczone przez `PASSTHROUGH`, więc przez zmienną nie przejdzie): przy **pierwszym**
uruchomieniu wypisuje `test result: FAILED. 0 passed; 3 failed` i kończy się jedynką, przy
**drugim** wypisuje `test result: ok. 3 passed; 0 failed` i zerem. Wzorzec dowodu: `(\d+) passed`
— dopasuje się w obu rundach, więc o werdykcie rozstrzyga wyłącznie kod wyjścia i to jest
celowe: gdyby o wszystkim decydował wzorzec, kryterium nie odróżniłoby tej implementacji od
takiej, która czyta samo dopasowanie.

Atrapa agenta oddaje **zawsze** ten sam tekst — zdanie bez ani jednego znacznika `outcome:`.

Asercje: (a) bieg kończy się powodzeniem; (b) plik licznika ma **dokładnie dwa** wiersze —
komenda biegła dwa razy, więc runda trzecia została pominięta, a nie przepalona; (c) atrapa
sterownika zobaczyła prompt `s_write` **dokładnie dwa razy**; (d) strażnik postawiony wprost
w teście: `assert_eq!(memory::handoff::verdict_in(SAID), memory::handoff::Verdict::Fail)` —
tekst, którym mówi atrapa, jest według starego protokołu **odmową**, więc implementacja dalej
domykająca pętlę na słowie agenta przepaliłaby wszystkie trzy rundy i skończyła porażką;
(e) `s_check` zostawił przekazanie w katalogu biegu, a jego ciało niesie fragment wyjścia
komendy — bez tego runda 1 nie wie, co padło w rundzie 0, i pętla nie ma po co istnieć
(niezmiennik 21); (f) wariant wyczerpania: ten sam graf z `max_turns: 2` i skryptem, który
**nigdy** nie wychodzi zerem, kończy bieg porażką, a licznik ma dokładnie dwa wiersze.

**`verdict_in` zostaje i nie ruszasz jej.** Sędzia-agent jest jedyną drogą dla repo, które
sprawdzeń nie ma — a przy zerowej ceremonii UI ma mówić „no checks configured" uczciwie,
zamiast pokazywać zieleń (D7, „Co musi przetrwać nawet przy zerowej ceremonii").
`src-tauri/tests/it/runcmd_loop.rs` ma zostać zielone **bez jednej zmiany w swoim pliku**;
jeśli twoja zmiana w `Live::verdict_after` je przewraca, to nie jest kolizja kryteriów, tylko
znak, że skasowałeś ścieżkę awaryjną zamiast dołożyć drugą.

*Słaba asercja:* „bieg skończył się po dwóch rundach". Przechodzi dla implementacji, która dalej
czyta `outcome:` z ust agenta i tylko przypadkiem zatrzymała się w tym samym miejscu — a także
dla takiej, która zawsze robi dwie rundy. Rozróżniają to trzy rzeczy naraz: strażnik (d), który
zamienia tekst atrapy w **odmowę** w starym protokole; licznik uruchomień komendy (b), bo
tylko on odróżnia „rundy nie było" od „runda przeszła"; oraz sufit `max_turns: 3`, przy którym
zatrzymanie się na dwóch jest decyzją, a nie zbiegiem okoliczności.

## Świadomie poza zakresem

- **Cały frontend.** To zadanie nie dotyka `src/`. Kafelek kroku „sprawdź" na płótnie, jego
  modal z polami „Command to run" i „Proof that it ran", ikona i kolor — osobna praca, po tej.
  Do tego czasu krok istnieje w schemacie i w silniku, a człowiek dodaje go, poprawiając plik
  ręcznie. **Kontrolka bez handlera nie wchodzi do repo** (niezmiennik 16), więc nie dokładaj
  „na zapas" ani jednego pola w UI.
- **`docs/harness-as-workflow.md` i `.loadout/workflows/ship-task.json`.** Po tym zadaniu
  dokument dalej mówi `"missing_kind": "check"` o etapach `gate` i `land` — i **tak ma zostać**,
  dopóki ktoś nie przepisze tych dwóch kafelków. Wyrocznia
  `src-tauri/tests/it/harness_workflow_findings_match_doc.rs` sprawdza to wprost: uwaga o braku
  jest czerwona w chwili, gdy plik workflow **ma** krok tego rodzaju. Zmiana `s_gate` na
  `"kind": "check"` bez jednoczesnej poprawki dokumentu przewraca cudze kryterium w pliku,
  którego to zadanie nie posiada. Zgłoś w uwagach, że dokument czeka na aktualizację; nie rób
  jej tutaj.
- **Pole limitu czasu na kroku.** Stała 30 minut w `command.rs` wystarcza, dopóki nie ma
  kontrolki, która by ją ustawiała. Pole w schemacie bez pola w UI jest kontrolką bez handlera.
- **Ponawianie i „jeśli próba się nie uda".** D7 wymienia rundę naprawczą jako osobny mechanizm,
  a ten mechanizm już jest — to `Link::max_turns`. Krok „sprawdź" nie dostaje własnego licznika
  prób; dwa liczniki rund w jednym grafie to dwa miejsca prawdy o tym, ile razy coś biegło.
- **Zmiana traitu `AgentDriver`.** Krok „sprawdź" go **nie implementuje** i to jest treść AC-4,
  a nie pominięcie. Jeśli w trakcie wyjdzie, że planista nie umie wykonać kroku bez
  `AgentHandle` — to jest wynik badania i zapisujesz go w uwagach (która metoda, jaki kształt,
  jaka byłaby najmniejsza zmiana), a nie obchodzisz atrapą sterownika udającą agenta.
- **Skrzynia `regex`.** Patrz „Kształt, na którym stoją kryteria": `Cargo.toml` jest poza OWNS.
- **Uruchamianie kroku „sprawdź" w kilku kopiach.** `copies` należy do kroku agenta.
  Sprawdzenie uruchomione trzy razy naraz w jednym katalogu to niezmiennik 12 złamany
  z definicji.
- **`ship-task.sh` sterowany plikiem workflow.** Dopóki nie ma kafelka i modalu, harness dalej
  biegnie skryptem. To zadanie zdejmuje **jedyną** przeszkodę z U-1, nie wszystkie.

<!-- OWNS
src-tauri/src/engine/drivers/command.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/src/workflow/mod.rs
src-tauri/src/workflow/check.rs
src-tauri/src/commands/run.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/check_step_schema.rs
src-tauri/tests/it/check_step_verdict.rs
src-tauri/tests/it/check_step_process_group.rs
src-tauri/tests/it/check_step_has_no_agent.rs
src-tauri/tests/it/check_step_closes_the_loop.rs
-->
