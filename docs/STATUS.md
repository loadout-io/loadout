# Stan budowy — 2026-08-17, 18:10

Ten plik jest **żywy**. Aktualizuje go orchestrator po każdym lądowaniu. Prawdą o zadaniu jest
`tasks/<ID>.md`; tutaj jest wyłącznie to, czego z plików zadań nie widać: co już stoi w trunku,
co stanęło i dlaczego.

## Liczby

| | |
|---|---|
| commitów lądowania | **32** (`git log --oneline main \| grep -c 'land task-'`) |
| trunk | **CZERWONY** — `full-test` i `full-clippy`, paragon `runs/last.json` 2026-08-17T17:16 |
| żywe gałęzie | **cztery**: T-29 (ahead 12) · T-32 (5) · T-28 (4) · T-37 (1, sam `TASK.md`) |
| nie zaczęte | **S-3, T-10** — kredyty Codeksa wracają 2026-08-20; `drivers/absent.rs` uczciwie odmawia dla `Vendor::Codex` zamiast kłamać ClaudeDriverem |
| napisane, nieuruchomione | **T-38** (szew front↔Rust + strażnik kluczy argumentów) |

## Trzy rzeczy, które trzeba przeczytać przed jakąkolwiek pracą

### 1. Pętla pracy przestała być używana — i to jest przyczyna, nie objaw

**Od `14f5a31` (land task-T-31) main dostał 12 commitów i ani jednego `chore(main): land`.**
Praca idzie wprost na trunk, bez gałęzi, bez dowodu czerwieni, bez bramki per zadanie:

| commit | co wniósł | czyje zadanie |
|---|---|---|
| `62eef89` | `commands/run.rs`, `commands/mod.rs`, `lib.rs`, dwa testy `fresh_copy_*` | T-33, komplet |
| `d9d31b7` | `commands/run.rs` + `step_deadline_stops_the_agent.rs` | **T-35 AC-1 z trzech** |
| `a7a2d87` | oba pliki testów szkieletowych | T-28 |
| `6bb2566`, `2fdc866` | powłoka jako makieta, boczne menu 196 px | T-37 |

Konsekwencja jest strukturalna, nie estetyczna: **zadanie, które nie przechodzi przez gałąź,
nie przechodzi przez `verify.sh full` ze swoim kontraktem.** Jedyną bramką, która mogłaby to
złapać, jest bramka trunka — a ta od wczoraj umiera na budżecie (punkt 2). Dlatego dzisiejsze
awarie były niewidzialne: nie było komu ich zobaczyć.

**T-35 jest w połowie na trunku.** AC-1 wylądowało wprost, a plików dla AC-2 i AC-3 nie ma
w żadnym refie repozytorium. Zadanie jest częściowo spełnione i nikt tego nie odnotował.

**T-37 ma kontrakt na gałęzi, a pracę na main.** Gałąź `task-T-37` niesie jeden commit i jest
nim sam `TASK.md`. Implementacja stoi w `2fdc866` i `6bb2566`, poza gałęzią.

### 2. Mina, która wybuchnie przy najbliższym lądowaniu T-28

`a7a2d87` dodał oba pliki testów szkieletowych **wprost na main**, a gałąź `task-T-28`
(ahead 4) niesie **własną, rozjechaną kopię tych samych dwóch plików**:
`git merge-base --is-ancestor a7a2d87 task-T-28` mówi **nie**. Różnica to 13 linii w każdym
pliku, i tą różnicą jest dokładnie `#[ignore]` z `6e55daf`.

**Więc lądowanie T-28 po cichu cofnie ogrodzenie płatnych testów** i wróci do hurtowego
uruchamiania dwóch prawdziwych sesji `claude` w każdej pełnej bramce. To ten sam kształt, który
ten plik opisuje niżej przy `refresh_harness_from_trunk`: cofnięte kryterium nie psuje testów,
tylko je **osłabia**, więc bramka po takim lądowaniu jest **zielona**.

Przed lądowaniem T-28: pogodzić te dwie wersje ręcznie i sprawdzić, że `#[ignore]` przeżyło.

### 3. `full-test` nie wisi na kodzie — wisi na skanowaniu świeżych binarek

Trzy pełne bramki z rzędu (T-29 o 11:22, T-32 o 16:05, trunk o 17:16) padły **wyłącznie** na
`full-test`, każda w **3600 s = 2 × budżet 1800 s**, z komunikatem „it is waiting for something
that is not going to arrive". Komunikat był dosłownie prawdziwy — czekała na `syspolicyd`.

Zmierzone 2026-08-17 na dwóch celach, po dwa razy każdy:

| binarka | 1. uruchomienie | 2. uruchomienie | sam test w środku |
|---|---|---|---|
| `store_strict_schema` | **36 s** | **0 s** | 0,01 s |
| `workflow_check_ids` | **59 s** | **0 s** | — |

To skanowanie kodu macOS przy **pierwszym** uruchomieniu niepodpisanej binarki debug, z wynikiem
zapamiętanym per plik. W top CPU stoi `syspolicyd` i `XprotectService`. `cargo test --tests`
buduje **125** binarek, a każda relinkowana zmiana każe przeskanować je od nowa: 125 × ~35 s
to ponad godzina, więc 1800 s nie ma szans. Dlatego `full-clippy` schodzi w 144 s na tej samej
skrzyni — clippy **nie uruchamia** binarek.

**Wniosek dla harnessu, nieprzyjemny.** Budżet podniesiono 600 → 1800 s „z pomiaru"
(`b768fbf`, przyczyna do kolejki jako Q-6). Pomiar zawierał ten narzut, czyli **limit podniesiono
pod problem środowiskowy zamiast go usunąć** — i to tylko odsunęło ścianę o jedną fazę projektu.
Q-6 jest tym samym rozstrzygnięte: przyczyną nie jest wolny kod.

Zanim zaczniesz szukać winnego testu: **uruchom tę samą binarkę dwa razy.** Drugi przebieg
bliski zeru znaczy, że to skanowanie. Naprawa trwała jest po stronie maszyny (Developer Tools
dla narzędzia odpalającego cargo); obejście bez uprawnień to rozgrzanie binarek raz, poza bramką.

### Pierwsza uczciwa czerwień, którą ten budżet maskował

`src-tauri/tests/stream_raw_tee_live.rs` — `every_raw_line_is_in_the_file_including_the_one_nobody_understands`
pada **deterministycznie** w 20,01 s, czyli równo we własnym `LIMIT`, na `Error: Elapsed(())`.
Trzy przebiegi, wszystko rozgrzane, atrapa to skrypt `/bin/sh` (więc **nie** narzut skanowania).

Ten sam test przeszedł w **0,6 s** przy lądowaniu T-34 (`runs/T-34/gate-final.json`,
2026-08-16T23:47, `failed []`). Więc albo regresja wniesiona od wczoraj wieczorem, albo
zależność od środowiska — przy lądowaniu biegł w worktree T-34, pomiar robiony na main.
**Drugiej możliwości nie wykluczono.** Bisekcja odłożona: każdy jej krok to relink 125 binarek
plus ich ponowne skanowanie.

## Gdzie co jest

Cztery żywe gałęzie mają **parami rozłączne** zbiory OWNS, a jedyny plik zmieniany przez
wszystkie cztery to `TASK.md`, którego na main nie ma. Przecięcie OWNS `T-38` z T-28/T-29/T-32/T-37
jest **puste**, więc T-38 może biec obok nich — czeka wyłącznie na zielony trunk.

Około **osiemnastu worktree po zamkniętych zadaniach jest wciąż zamontowanych** (T-06…T-31,
wszystkie `ahead=0`). Każde niesie klon `node_modules` i symlink do wspólnego `target/`.
Nie są potrzebne i mieszają obraz przy diagnozie.

## Siedem szwów — stan 2026-08-17

| | szew | stan |
|---|---|---|
| `T-27` | komendy Tauri + adaptery `io.ts` | **wylądowane** |
| `T-34` | tee surowego strumienia → `logs/` | **wylądowane, ale jego test live jest dziś czerwony** — patrz wyżej |
| `T-36` | filtr eskalacji na przelotce (D6) | **wylądowane** (`b70cc82`) |
| `T-30` | most `Vec<Line>` → pompa, komendy biegu, Start | **wylądowane** (`03c0e52`) |
| `T-31` | limit globalny na żywej ścieżce | **wylądowane** (`14f5a31`) |
| `T-32` | przekazanie → prompt następnego kroku | gałąź żywa, 5 commitów; bramka RED tylko na `full-test` |
| `T-33` | fresh-copy to prawdziwa kopia | na main wprost (`62eef89`), bez lądowania |
| `T-35` | limit czasu kroku + recovery + boot time | **AC-1 z trzech** na main wprost (`d9d31b7`) |
| `T-28` | szkielet chodzący | testy na main (`a7a2d87`), gałąź rozjechana — **patrz mina wyżej** |
| `T-29` | klikanie po prawdziwej aplikacji | gałąź żywa, 12 commitów; **3/3 kryteria zielone**, bramka RED tylko na `full-test` |
| `T-38` | szew front↔Rust + strażnik kluczy `invoke` | kontrakt napisany i zwalidowany, nieuruchomiony |

**T-29 i T-32 nie są zepsute.** Oba mają wszystkie własne kryteria zielone i 15 z 16 sprawdzeń;
oba przewróciło jedno sprawdzenie projektowe, i to na powodzie środowiskowym. `integrate.sh`
odmówił lądowania słusznie i powiedział dlaczego: *„the gate is red on main BEFORE anything was
merged — nothing was landed. fix trunk first, otherwise the first branch gets blamed for it."*

## Wzorzec, którego bramka strukturalnie nie widzi — cztery przypadki jednego dnia

To jest najcenniejsza rzecz, jaka wyszła 2026-08-16, i jedyna, której **nie da się zmechanizować
istniejącymi środkami**. Za każdym razem kryterium sprawdza coś **węższego** niż niezmiennik,
którego broni — i przechodzi na implementacji, która niezmiennika nie spełnia.

| gdzie | kryterium sprawdza | niezmiennik mówi |
|---|---|---|
| T-14 | że przycisk `＋ Create` **istnieje** i ma `data-create` | kontrolka ma **działać** — każdy test używa `renderToStaticMarkup`, który nigdy nie odpala `onClick` |
| T-18 | ścieżki rozmieszczania osobno | uzasadnienie AC-6 opisuje scenariusz, którego **żaden test nie przechodzi** |
| T-06 AC-3 | że pragmy **mają wartość** 5000 / 1 | że to **nasz kod** je ustawia — a stos ustawia obie sam, więc asercja przechodziła na helperze, który nie robi nic |
| T-06 AC-5 | że połączenie z `Store::reader()` **nie umie pisać** | niezmiennik 2: **żadnego** drugiego pisarza — a `Store::open(path)` wołane dwa razy na tej samej ścieżce tworzy drugie zapisujące połączenie i drugie zadanie pisarza, i nic tego nie zabrania |

Dwa pierwsze przeszły bramkę i wylądowały zgodnie z kontraktem. Trzeci **złapaliśmy przypadkiem** —
kontrola negatywna okazała się fałszywa w sposób wykrywalny; gdyby rusqlite dawał domyślnie 5001
zamiast 5000, siedziałoby to w repo jako zielone. Czwarty znalazł recenzent na **zielonej bramce**
i on jest najgroźniejszy, bo dotyczy niezmiennika architektonicznego, a scenariusz jest realny:
`T-24` otwiera kilka workspace'ów naraz, więc dwie karty na tym samym folderze to dwa `Store::open`
na tym samym pliku.

**Do zrobienia z tym trzy rzeczy, w tej kolejności:**

1. ~~`T-06 AC-5` / niezmiennik 2~~ — **rozstrzygnięte 2026-08-16.** Decyzja człowieka: `Store::open`
   **nie** dostaje własnej obrony; gwarancja mieszka w rejestrze workspace'ów, bo `ARCHITECTURE.md`
   §6a reguła 1 już to rozstrzygnęła („otwarcie folderu, który ma kartę, przełącza na nią").
   Konsekwencja: skoro gwarancja jest tam, to tam musi być udowodniona — `T-24 AC-3` dostał drugą
   połowę. Sprawdzał, że rejestr ma jedną pozycję i że `WorkspaceId` się zgadza; **nie** sprawdzał,
   że nie powstał drugi magazyn. A `WorkspaceId` jest wyliczany ze ścieżki, więc zgadza się zawsze —
   także w implementacji z dwoma zapisującymi połączeniami. Piąty przypadek tego samego wzorca,
   znaleziony przez zastosowanie go do samego siebie.
2. Przegląd cross-vendor po 2026-08-20 (`docs/PLAN.md` §6a) — **z tym wzorcem jako pytaniem
   przewodnim**, a nie ogólnym „przejrzyjmy wszystko". Pytanie brzmi: *czy to kryterium sprawdza
   niezmiennik, czy tylko jego najłatwiejszy objaw?*
3. Rozważyć, czy da się z tego zrobić sprawdzenie. Na dziś **nie umiem** — „kryterium jest węższe
   niż jego uzasadnienie" to sąd o sensie, nie o stanie, więc niezmiennik 28 nie ma tu czego chwycić.
   Zapisane w `docs/HARNESS-QUEUE.md` jako świadomie niezmechanizowane.

**Zablokowane brakiem kredytów Codeksa (2):** `S-3 T-10`. Kredyty wracają **2026-08-20**.
Wtedy też `docs/PLAN.md` §6a każe zrobić przegląd cross-vendor wszystkiego, co powstało
w trybie same-vendor — a to jest całość.

## T-06 — zamknięte 2026-08-16, po trzech fałszywych startach

Wylądowało. Warto pamiętać dwie rzeczy, bo obie były zaskoczeniem:

**Zwis AC-2 nie miał nic wspólnego z SQLite.** Poprzednia hipoteza („magazyn trzyma zamek zapisu")
była błędna. To było zakleszczenie kanału tokio: test trzymał żywy klon `Writer`, a `Store::close()`
czekał na zadanie pisarza, które kończy się dopiero po upadku **ostatniego** nadawcy. Naprawa:
`Job::Close` idący **kanałem** — FIFO domyka wcześniejsze wsady, więc „zapisane" dalej znaczy
zapisane, a zamknięcie nie zależy od tego, kto jeszcze trzyma uchwyt.

**AC-3 miało dwa fałszywe założenia o świecie**, oba tej samej klasy: `busy_timeout` = 0 i
`foreign_keys` = 0 na gołym połączeniu. Nieprawda — rusqlite ustawia timeout sam, a bundlowany
SQLite ma klucze obce włączone. Obie domyślne równe wymaganym, więc obie asercje były puste.
Kryterium poprawione decyzją człowieka: kontrola nie porównuje się już z domyślnymi wartościami
stosu, tylko wymusza wartość **nieakceptowalną** i dopiero potem woła `apply_pragmas`.


## T-26 — dopisane 2026-08-16 po URUCHOMIENIU aplikacji

`npm run dev` pokazywał pięć zakładek i pięć pustych ekranów, mimo że komponenty czterech sekcji
były wylądowane i zielone. Nikt nie napisał `src/sections/<id>/index.tsx`, bo **żadne kryterium
o to nie prosiło** — a `src/sections/workflows/` nie miał nawet właściciela w żadnym bloku OWNS,
więc napisanie tam pliku byłoby zapisem poza zakresem.

Zapis, który to spowodował, stoi w `tasks/T-08.md` przy AC-8: „pozostałe sekcje dostają to za
darmo". Nie dostały. Szósty przypadek wzorca z sekcji wyżej — tym razem znaleziony **wyłącznie**
przez uruchomienie produktu, bo żaden automat nie ogląda okna.

Pisząc kontrakt złapałem w nim własny defekt, zanim wydałem na niego bieg: kryteria wymagały
„dwóch agentów w magazynie" dowodzonych przez prawdziwe odkrywanie, a w całym drzewie Rusta jest
**zero** `#[tauri::command]` i nie ma `src/ipc.ts` — dane nie mają dziś skąd przyjść. Każde
kryterium ma teraz dwie połowy: **(a) montaż** bez wstrzykiwania czegokolwiek i **(b) treść**
na ekranie renderowanym wprost, z magazynem zasianym atrapą `Io`.

## Dwie kolejne naprawy harnessu, 2026-08-16 wieczorem

**Zamrożenie kontraktu cofało CUDZE pliki zadań.** `refresh_harness_from_trunk` zamrażał całe
`tasks/`, więc podciągnięcie trunka rewertowało na gałęzi pliki zmienione w międzyczasie. Objaw:
fałszywa czerwień `quick-scope` na T-20 (poprzedni bieg zapisał ją nawet jako „commit człowieka").
**Groźniejsze:** lądowanie wniosłoby ten revert na trunk i po cichu skasowało cudzą pracę —
a bramka po takim lądowaniu jest **zielona**, bo cofnięte kryterium nie psuje testów, tylko je
osłabia. Zamrożenie dotyczy teraz wyłącznie `tasks/$ID.md`; strażnik
`contract_freeze_touches_only_its_own_task` sprawdza wszystkie trzy strony naraz.

**Biegi są odpalane odczepione.** Dwa razy tego dnia menedżer zadań ubił WSZYSTKIE biegi naraz
na granicy tury (człowiek potwierdził, że to nie on). `scratchpad/detach.py`: podwójny fork +
`os.setsid`, kod wyjścia do `runs/<ID>/wave.rc`. Czekacz na ten plik może ginąć dowolnie —
praca o nim nie wie. macOS nie ma `setsid(1)`, stąd Python.

## Co naprawiono w harnessie 2026-08-16 (każdy ze strażnikiem albo z kontrolą negatywną)

Wszystkie wyszły z jednego incydentu — T-06 — i każdy z nich zatrzymałby pętlę znowu.

**`063c7e0` — sufit poziomu liczy retry, który bramka sama przyznaje.** `run_one` po timeoucie
pyta drugi raz (zamierzone), więc oracle autoryzuje do `2*b`. Sufit liczył `b`, poziom miał
460 s przy zjedzonych 840 s i przewracał się na „GATE TOO SLOW" — **za czas, który sam wydał**,
przykrywając jedyną actionable wiadomość. Kod 3 znaczy „przerwane albo maszyna" i wysyła
orchestratora po osierocone procesy; prawdą był defekt kontraktu. Liczenie wyjęte do
`ceiling_for()`, strażnik `hung_check_reads_as_red_not_as_a_slow_gate` w `scripts/ci.sh`.
Sufit **nadal** łapie bramkę wolną bez powodu — sprawdzone kontrolą negatywną.

**`a70af28` — faza kontraktu dostaje JEDNĄ rundę naprawczą.** Strona implementacyjna miała ją
od początku, kontraktowa nie miała żadnej — więc jeden błąd w szkielecie kasował cały bieg
i wymagał ręcznego skasowania worktree. Runda dostaje powody **z paragonu**: bramka rozróżnia
trzy kształty fałszywej czerwieni („did not FINISH", „PASSES before implementation",
„did not RUN") i każdy ma inną, nazwaną naprawę. Przy okazji routing wznowienia przestał
zgadywać z licznika commitów (na T-06 `commit_leftovers` dorobił drugi commit i skrypt kazał
wyrzucić pracę bez ani jednej linii implementacji) — pyta paragon, niezmiennik 20.

**`870b53f` — wznowienie nie płaci trzy razy za ten sam przebieg `before`.** Bez tego T-06
zapłaciłby 3 × 840 s plus zbędne przepisanie kontraktu od zera.

**`efda313` + `60c5f5e` — odcisk asercji, i strażnik dla niego.** Nowa runda otwiera jedną drogę
na skróty: „spraw, żeby padało INACZEJ" da się przeczytać jako „asertuj mniej". Reguła istniała
w promptcie i była miękka (niezmiennik 28). Teraz `assertion_fingerprint` liczy linie niosące
asercje per plik specyfikacji, a **każda** faza po certyfikacji kontraktu — implementacja,
naprawa, naprawa kontraktu — porównuje się z tą bazą. Spadek zatrzymuje bieg i nazywa plik.
Skasowany plik liczy się jako strata wszystkich asercji, bo inaczej najprostsze obejście było
niewidoczne.

**Odświeżenie oracle'a, i dlaczego trzeba było go naprawiać dwa razy.** Poprawka sufitu była dla
T-06 **nieosiągalna**: `worktree.sh` wycina cały katalog roboczy, więc gałąź niesie **własną**
kopię `harness/`, i ta stara kopia oddała 3 tam, gdzie nowa oddaje 1. `ship-task.sh` znał tę
klasę — podciągał trunk, ale dopiero **przed rundą naprawczą**, czyli po dwóch osądach.
Podniesione na start biegu (`refresh_harness_from_trunk`, strażnik pyta i o działanie,
i o kolejność). Druga próba padła znowu, bo ten merge **konfliktuje na `lib.rs`** — czyli
istniejące odświeżenie przed rundą naprawczą też cicho nie działało. Naprawione regułą
`merge=union` w `.gitattributes`: to zapis tego, co i tak robi się ręcznie za każdym razem,
i przy okazji zdejmuje ten konflikt z **każdego drugiego lądowania** w `integrate.sh`.

Trzy fałszywe starty T-06 nauczyły jednej rzeczy o samym harnessie: **naprawa bramki nie działa
wstecz na gałęzie już wycięte**, dopóki nie ma czym jej tam wpuścić.

## Co poszło nie tak przez noc — żeby nie powtórzyć

Sterownik falowy wylądował T-25 i T-16 o 03:03, a potem **stał osiem godzin**. WebStorm zapisał
pliki `.idea/`, `integrate.sh` słusznie odmówił lądowania na brudnym drzewie, a sterownik
odkładał land „na następną rundkę" **444 razy, nie wypisując ani razu, co jest brudne**.

Przyczyną nie był WebStorm, tylko **dwie definicje czystego drzewa**: `checks/quick-scope.sh`
ma `\.idea` na liście `GENERATED` od początku, `git status --porcelain` nie miał o tym pojęcia.
`.gitignore` dostał `.idea/` i `.vscode/` w `d08b6f0`.

Sterownik i `scripts/loop.sh` zostały **usunięte** (`3946181`). Nie odtwarzaj ich bez przeczytania
tamtego komunikatu commita: przez jedną noc trzy razy poprawiałem monitoring po tym, jak skłamał,
i i tak nie złapał jedynej awarii, która się wydarzyła. **Monitoring, który nie diagnozuje, jest
gorszy niż jego brak**, bo wygląda jak nadzór.

## Kolizje kręgosłupa — już rozwiązane regułą, nie ręką

`src-tauri/src/lib.rs` zbiera `pub mod` od **każdego** zadania tworzącego moduł, więc przy
dwóch takich zadaniach konflikt jest **pewny**. Zdarzył się przy T-11 i T-12, a potem zablokował
odświeżanie harnessu na T-06.

**Od 2026-08-16 nie musisz go rozwiązywać ręcznie.** `.gitattributes` niesie
`src-tauri/src/lib.rs merge=union`, czyli zapisaną wprost regułę „zachowaj obie deklaracje,
nigdy nie wybieraj strony". Obowiązuje tak samo przy `integrate.sh` i przy odświeżaniu harnessu
na gałęzi. Zweryfikowane na T-06: po merge'u stoi pięć deklaracji — `engine`, `store` z gałęzi,
`library`, `memory`, `workflow` z trunka. Nic nie zginęło, nic się nie zdublowało.

Jeden haczyk, który kosztował fałszywy start: **reguła musi być w gałęzi**, żeby zadziałała
przy merge'u. Gałąź wycięta przed jej wprowadzeniem potrzebuje jednorazowego wpuszczenia pliku
(`cp .gitattributes` + commit). Wszystkie gałęzie wycinane od teraz dostają go z trunka.

`engine/mod.rs`, `memory/mod.rs`, `skills/mod.rs` i `drivers/mod.rs` **celowo** zostały poza
regułą: mimo nazwy niosą prawdziwy kod, więc union mógłby skleić tam dwie wersje funkcji
zamiast dwóch deklaracji. Uzasadnienie w `docs/HARNESS-QUEUE.md`.
`harness/task-spine.py` dalej pilnuje, żeby każde zadanie miało gdzie dopisać swój wiersz.

## Rzecz, która nie zatrzyma bramki i dlatego jest groźna

`T-25` dał mechanizm montowania sekcji (`src/ui/screens.ts`, powłoka szuka
`src/sections/<id>/index.tsx`), ale **żadna sekcja nie ma jeszcze swojego `index.tsx`**.
`npm run dev` na `localhost:5273` pokazuje pięć pustych ekranów i **będzie tak pokazywał**,
dopóki nie wyląduje `T-08` — to on ma `AC-8`, jedyne kryterium, które renderuje `<App>` bez
wstrzykiwania i sprawdza, że zdania pustego ekranu tam **nie ma**.

Kryteria pozostałych sekcji to testy komponentowe wołane wprost na plikach. Przechodzą bez
montażu. Bramka nigdy o tym nie powie.
