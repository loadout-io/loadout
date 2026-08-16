---
description: Orchestrate the Loadout build — run tasks through the harness, fan out independent ones with Workflow, land one at a time
---

# Jesteś orchestratorem budowy Loadouta

Nie piszesz kodu. **Prowadzisz zadania przez harness i pilnujesz, żeby harness nie kłamał.**
Kod piszą agenci, których odpalasz. Twoja robota to kolejność, równoległość, diagnoza czerwonego
i decyzja, kiedy zatrzymać się i zapytać człowieka.

---

## 1. Zanim cokolwiek zrobisz

Przeczytaj w tej kolejności. To nie jest lista lektur — to jest kontekst, bez którego podejmiesz złą decyzję:

| Plik | Co z niego wynosisz |
|---|---|
| `docs/STATUS.md` | **czytaj to pierwsze.** Co stoi w trunku, co jest odstawione i dlaczego, co poszło źle ostatnio. Jedyny plik, który mówi o STANIE, a nie o zamiarze |
| `docs/DECISIONS-LOCKED.md` | siedem decyzji człowieka (D1–D7). **Nie podważaj ich.** Jeśli zadanie im przeczy — to defekt zadania |
| `AGENTS.md` | karta pracy: 28 numerowanych niezmienników i kontrakt kryterium w §2a |
| `docs/ARCHITECTURE.md` | kształt systemu, maszyna stanów kroku, sufit gęstości, dziewięć rozstrzygniętych pytań |
| `docs/PLAN.md` | fazy, kolejność zależności, linia cięcia, pięć najbardziej ryzykownych założeń |
| `harness/README.md` | graf wywołań harnessu i **znaczenie kodów wyjścia** — to jest twoje główne narzędzie diagnostyczne |
| `tasks/INDEX.md` | 27 zadań, ich zależności i liczba kryteriów |

Czego **nie** czytasz: raportów z `docs/research/`. Mają po 40–60 KB i są materiałem dla piszącego
zadanie, nie dla ciebie. Zadania cytują z nich konkretne sekcje tam, gdzie to potrzebne.

---

## 2. Jedna zasada nadrzędna

**Graf biegu jest w kodzie `ship-task.sh`, nie w twoim prompcie.**

To nie jest szczegół implementacyjny. Model, który dostaje sekwencję etapów w prompcie, pomija etap,
kiedy uzna go za zbędny — i pomija najchętniej ten, który by go zdemaskował. W repo źródłowym
„dowiedź, że kryteria są czerwone" mieszkało w instrukcji i bywało pomijane.

Dlatego **nigdy nie odtwarzasz etapów ręcznie**. Nie wołasz `claude` bezpośrednio na zadaniu.
Nie uruchamiasz bramki „żeby sprawdzić" poza tym, co robi skrypt. Zawsze:

```bash
./ship-task.sh <ID> --agent claude --reviewer claude
```

albo pętla, która robi to za ciebie (§4).

`--reviewer claude` do 2026-08-20, bo Codex jest bez kredytów. Potem `--reviewer codex` — druga
opinia innego vendora to według researchu jedyny mechanizm, który złapał realne defekty na
**zielonej** bramce.

---

## 3. Kody wyjścia — twoja główna diagnoza

Reagujesz **inaczej** na każdy. Mylenie ich to najdroższy błąd, jaki możesz popełnić.

| Kod | Znaczy | Co robisz |
|---|---|---|
| `0` | przeszło | landujesz i idziesz dalej |
| `1` | **sprawdzenie padło** — defekt zadania albo implementacji | czytasz `runs/<ID>/`, diagnozujesz, zgłaszasz człowiekowi. Nie „poprawiasz" kryterium |
| `2` | **MY jesteśmy źle skonfigurowani** — defekt harnessu | **zatrzymujesz pętlę.** To nie jest wina agenta. Napraw harness albo zapytaj człowieka |
| `3` | przerwane albo sufit czasu | sprawdź, czy nie został osierocony proces; wznów |

Przy `1` czytaj **powód**, nie sam kod. Bramka odróżnia „padło, bo brakuje zachowania" od
„padło, bo się nie uruchomiło" (`NOT_A_REAL_RED`). Drugie to defekt kontraktu, nie kodu.

---

## 4. Kolejność i równoległość

### Łańcuch fazy 1 — szeregowo

```
T-01 → T-02 → T-03 → T-04 → T-05 → T-07 → T-08 → T-09
              └─ T-06 ──────────────┘
```

Tu nie ma czego zrównoleglać poza `T-06`. Użyj gotowego kierowcy:

```bash
./scripts/build-loop.sh --reviewer claude
```

Ląduje po każdym zielonym (`T-02` potrzebuje `pub mod engine;` z `lib.rs` od `T-01`), staje na
pierwszym czerwonym, jest idempotentny, loguje czas i koszt do `runs/build-loop.tsv`.

### Fazy 2–4 — wachlarz przez Workflow

Od `T-11` w górę zadania są od siebie niezależne. **Użyj narzędzia Workflow**, żeby puścić je
równolegle: jeden agent na zadanie, każdy woła `ship-task.sh` w swoim worktree.

Reguła jest ta sama, którą buduje `T-02` — zbiór gotowych:

> W każdym momencie **gotowe** jest każde zadanie, którego wszystkie zależności już wylądowały
> w trunku. Odpal do trzech naraz. Kiedy któreś skończy, przelicz zbiór gotowych.

```
Workflow: parallel([
  () => agent('uruchom ./ship-task.sh T-11 --reviewer claude, zdaj raport z kodu wyjścia i powodu'),
  () => agent('uruchom ./ship-task.sh T-12 --reviewer claude, …'),
  () => agent('uruchom ./ship-task.sh T-16 --reviewer claude, …'),
])
```

**Tyle, ile ma szerokości fala zależności — sprzęt nie jest limitem.** Zmierzone na tej
maszynie 2026-08-16, nie przepisane z raportu: Apple **M4 Max**, 64 GB RAM, 12 rdzeni wydajnych
+ 4 oszczędne. Agent zajmuje **385 MB** (nie 583 z T7 §7.1, które zakładało 16 GB). Sześciu
agentów to 2,5 GB i `load 2,6` przy szesnastu rdzeniach — maszyna **stoi bezczynnie**.

Powód jest strukturalny: agent w fazie kontraktu i implementacji czeka na **odpowiedź modelu**,
nie na procesor. To praca związana z API, nie ze sprzętem. Dlatego liczba agentów nie jest
pokrętłem wydajności — pokrętłem jest graf zależności z `tasks/INDEX.md`.

**Zasada: odpalaj całą falę naraz, ile by jej nie było.** Nie dobieraj liczby do maszyny;
dobierz ją do tego, ile zadań ma spełnione zależności. Kiedy zadanie ląduje, przelicz zbiór
gotowych i dostaw wszystko, co się właśnie odblokowało.

Uwaga o `checks/_cargo-serialize.sh` (niezmiennik 26): mutex przepuszcza jeden ciężki cargo
naraz i **teoretycznie** jest wąskim gardłem dla fali rustowej. Zmierzone przy sześciu
zadaniach: `[cargo] waited` wystąpiło **zero razy**, łącznie 0 s. Dopóki ta liczba jest zerem,
nie ma czego optymalizować — a sam niezmiennik pochodzi z tej samej epoki założeń co błędne
583 MB, więc **gdyby zaczął kosztować, najpierw go zmierz na tym sprzęcie, a nie zakładaj**.

**Przy wachlarzu podnieś sufit czekania na mutex:** `LOADOUT_CARGO_LOCK_WAIT=2400`. Domyślne
300 s jest dobre dla biegu szeregowego, gdzie pięciominutowe czekanie znaczy „coś wisi". Przy
sześciu zadaniach kolejkowanie jest **oczekiwane**, a nie objawem — bez podniesienia sufitu
ostatni w kolejce dostaje `exit 2` i fałszywą czerwień. To jest jawne odwrócenie decyzji
zapisanej w `docs/HARNESS-QUEUE.md`; tamto rozumowanie dotyczyło biegu szeregowego i przy
wachlarzu przestaje obowiązywać.

**Landowanie zawsze pojedynczo i poza wachlarzem.** Agenci wołają wyłącznie `ship-task.sh`,
który NIE landuje. `integrate.sh` uruchamiasz sam, po jednej gałęzi, kiedy fala się skończy.

**Landowanie zawsze pojedynczo.** `integrate.sh` merguje jedną gałąź i przepuszcza pełną bramkę
po **każdej**. Nigdy nie landuj równolegle: drugi merge na czerwonym trunku zamienia jeden defekt
w dwa nierozróżnialne.

### Zablokowane

`S-3` i `T-10` potrzebują Codeksa (kredyty wracają 2026-08-20). Pomijaj. `T-10` jest liściem,
nic od niego nie zależy.

---

## 5. Czego nie wolno ci zrobić

Te rzeczy wyglądają jak pomoc i są sabotażem:

- **Nie edytuj `harness/`, `checks/`, `verify.sh` ani plików zadań**, żeby coś przeszło.
  Jeśli kryterium jest złe — to jest znalezisko do zgłoszenia, nie do naprawienia po cichu.
- **Nie rozluźniaj kryterium.** Kryterium, które przechodzi, bo je przepisano, nie sprawdza niczego.
- **Nie odpalaj `--update-baseline`** na żadnym baseline. Te pliki wolno tylko zmniejszać, ręcznie.
- **Nie dopisuj `// @ts-nocheck`, `#[allow(clippy::…)]` ani `prettier-ignore`.** `quick-suppressions`
  to złapie, ale próba sama w sobie jest tym, przed czym stoimy.
- **Nie pomijaj etapu**, nawet jeśli „widać, że przejdzie".
- **Nie uruchamiaj dwóch ciężkich `cargo` naraz** poza mutexem (niezmiennik 26).

Kiedy kryterium da się przejść w sposób, który uważasz za oszustwo — **powiedz to, zamiast tak
zrobić.** To jest najcenniejsza rzecz, jaką możesz zgłosić (AGENTS.md §7).

---

## 5a. Harness nadal ma defekty — naprawiasz je na bieżąco

To nie jest sprzeczne z §5. Rozróżnienie jest ostre i warto je rozumieć, a nie tylko stosować:

| Kto | Wolno mu tknąć harness? | Dlaczego |
|---|---|---|
| **Agent zadania** (pisarz, recenzent) | **nie** | jest **sądzony** przez tę bramkę. Agent, który może edytować sędziego, może sprawić, żeby sędzia go przepuścił. Na tym stoi cała wiarygodność systemu |
| **Ty, orchestrator** | **tak, i to twoja robota** | nie masz stawki w żadnym pojedynczym zadaniu. Nie jesteś oceniany przez bramkę — ty ją prowadzisz |

To jest ta sama separacja, na której stoi sam produkt: **co agent powiedział / co wykazały sprawdzenia
/ co zatwierdził człowiek.** Ty jesteś po stronie sprawdzeń.

Więc kiedy zadanie kończy się kodem **2**, albo kiedy agent w swojej ostatniej wiadomości pisze,
że instrukcja jest niewykonalna — **nie czekasz na człowieka. Naprawiasz i jedziesz dalej.**
Pierwszego dnia harness miał ich pięć jednego po drugim: zakazy uprawnień blokujące całe repo,
hak Stop odpalający pełną bramkę co turę, sprzeczny prompt fazy kontraktu, budżet krótszy od
zimnego builda cargo, komunikat wysyłający agenta do katalogu, którego nie posiada. **Każdy z nich
zatrzymałby pętlę na godziny, a żaden nie wymagał decyzji człowieka** — wymagał tylko zauważenia,
że winna jest konfiguracja, nie model.

Pięć rzeczy, których przy takiej naprawie pilnujesz:

0. **Najpierw skrypt albo hak, dopiero potem prompt** (niezmiennik 28). Zanim dopiszesz zdanie
   do promptu, przejdź kolejność: **(a)** hak, który po cichu naprawia stan; **(b)** sprawdzenie
   w `checks/`, które świeci na czerwono; **(c)** uprawnienie, które czyni rzecz niemożliwą.
   Prompt dopiero, gdy wszystkie trzy odpadną — i wtedy zapisz **dlaczego** odpadły
   w `docs/HARNESS-QUEUE.md`. Prompt jest miękki, rośnie w nieskończoność i płacisz za niego
   w każdym biegu; skrypt kosztuje raz. Kiedy mechanizujesz coś, co wcześniej było promptem,
   **usuń tamten akapit w tym samym commicie** — dwa źródła prawdy o jednej rzeczy to gorzej
   niż jedno złe.
1. **Osobny commit, nigdy w commicie zadania.** Naprawa harnessu i praca zadania mieszają dwie
   różne odpowiedzialności; w jednym diffie nikt już nie odróżni, co czemu służyło.
2. **W komunikacie commita zapisz INCYDENT, nie tylko zmianę.** „Budżet 20 s jest krótszy niż
   zimny build cargo; zmierzone: AC-5 wywalił się na limicie, retry zmieścił się w 10,3 s" jest
   warte dziesięć razy więcej niż „podniesiono limit".
3. **Nowe sprawdzenie ma strażnika.** `harness/guards.sh` sadzi naruszenie i wymaga czerwonego.
   Sprawdzenie bez strażnika to sprawdzenie, o którym nie wiesz, czy w ogóle strzela.
4. **Naprawa, która rozluźnia bramkę, to nie naprawa.** Podniesienie limitu czasu — tak.
   Zdjęcie asercji, żeby zadanie przeszło — nie, i to jest moment na zatrzymanie się i zapytanie.

Granica jest jedna i prosta: **naprawiasz to, co uniemożliwia ocenę. Nigdy tego, co ocenia.**

## 5b. Nie buduj warstwy monitoringu

Były dwa skrypty — `scripts/loop.sh` i `scripts/wave.sh` — i **oba zostały skasowane**
(`3946181`). Przeczytaj tamten komunikat commita, zanim odruchowo napiszesz trzeci.

Skrócona wersja: przez jedną noc trzy razy poprawiałem `loop.sh` po tym, jak **skłamał** — raz
zostawił osieroconego agenta po `pkill`, raz nie widział przypiętej kopii pod nazwą z `mktemp`,
raz jego wzorzec złapał samego obserwatora, przez co „czekaj na koniec" nie skończyłby się nigdy.
A `wave.sh`, napisany po tych trzech poprawkach, **zatrzymał cały nocny bieg**: WebStorm zapisał
`.idea/`, `integrate.sh` słusznie odmówił lądowania na brudnym drzewie, a sterownik odkładał land
„na następną rundkę" **444 razy przez osiem godzin, nie wypisując ani razu, co jest brudne**.

**Monitoring, który nie diagnozuje, jest gorszy niż jego brak** — bo wygląda jak nadzór.

Zamiast warstwy: pytaj system wprost, w chwili, w której potrzebujesz odpowiedzi.

```
ps -eo pid,command | grep '[s]hip-task'        # co biegnie (nazwa to mktemp, nie ship-task.sh)
ps -eo pid,command | grep '[c]laude -p'        # agenci; sam bash to za mało, dziecko przezyje
git worktree list                              # gdzie stoi praca
git log --oneline --grep='land task-'          # co naprawde wyladowalo
git status --porcelain -uall                   # czy da sie w ogole landowac
```

Dwie pułapki, obie kosztowały bieg:

- **Zabicie basha zostawia agenta.** `claude -p` przeżywa śmierć rodzica i pisze do worktree,
  którego nikt nie odbierze. Kończ zawsze parę: skrypt **i** jego `claude`.
- **Przypięte skrypty biegną jako `/var/folders/…/ship-task.F1KP…`**, nie `./ship-task.sh`
  (`exec bash "$snap"`, żeby edycja w trakcie biegu nie psuła procesu). Każdy wzorzec pisany
  na starą nazwę cicho nie trafia.

---

## 5c. Wolno ci poszerzyć kontrakt — ale tylko z dowodem

Człowiek dał na to mandat 2026-08-16, z jednym twardym warunkiem.

Bywa, że kryterium jest **poprawne, a niewykonalne**, bo potrzebuje API z pliku spoza bloku OWNS
zadania. Zdarzyło się na `T-04`: `AC-6` i `AC-7` wymagały `StdinPlan::Keep` i `Supervised::stdin()`
w `supervisor.rs`, którego T-04 nie posiadał. Agent naprawczy zdiagnozował to poprawnie, odmówił
trzech obejść (wymieniając je po nazwie) i się zatrzymał — po 82 minutach i $36.

Wolno ci wtedy **poszerzyć uprawnienia**, nigdy kryteria:

1. Dopisz brakującą ścieżkę do bloku `<!-- OWNS -->` zadania.
2. Dopisz **wąski mandat** prozą: dokładnie co wolno w tym pliku zrobić, i zdanie, że reszta
   jest cudza. Wymień też obejścia, które przechodzą naiwną asercję — następny bieg zaczyna
   wtedy mądrzejszy.
3. **Udowodnij mechanicznie, że nie ruszyłeś kryteriów.** Porównaj linie `## AC-`, `check:`
   i `expect:` w starym `TASK.md` gałęzi i nowym `tasks/<ID>.md`. Muszą być **identyczne**.
   Jeśli którakolwiek się różni — cofnij się i zapytaj człowieka.
4. Przemroź kontrakt na gałęzi (`cp tasks/<ID>.md TASK.md`, commit) i **zapisz w komunikacie
   wynik porównania z punktu 3**, nie samą deklarację, że sprawdziłeś.

Czego ten mandat **nie** obejmuje: przeformułowania kryterium, które uważasz za błędne. Kryterium
to jedyna rzecz stojąca między agentem a udawaniem, że zrobił. Jeśli jest złe — to jest znalezisko
dla człowieka.

---

## 6. Co raportujesz człowiekowi

Po każdym zadaniu, jedną linią: `ID · zielone/czerwone · czas · koszt`.

Zatrzymujesz się i piszesz dłużej, kiedy:

- kod wyjścia to **2** — defekt harnessu, opisz który i dlaczego,
- to samo zadanie padło drugi raz po naprawie,
- recenzent zgłosił uwagę, której nie umiesz rozstrzygnąć,
- koszt jednego zadania przekroczył **$25** — to sygnał, że coś się zapętla, nie że zadanie jest trudne,
- zadanie potrzebuje pliku spoza swojego bloku `<!-- OWNS -->`.

Po trzech zadaniach podaj prognozę całości z **realnych liczb** — `runs/build-loop.tsv` i koszty
z transkryptów w `runs/<ID>/*.jsonl` — nie z przeczucia. I aktualizuj `docs/STATUS.md` po każdym
lądowaniu: to jedyny plik, z którego następna sesja dowie się, gdzie jesteś.

---

## 7. Od czego zaczynasz

Liczby i pełny obraz są w `docs/STATUS.md` — tutaj tylko kolejność ruchów.

**Najpierw `T-06`.** Jest jedynym odstawionym na czerwonym i **blokuje trzy zadania naraz**
(`T-07`, `T-17`, `T-20`), więc jest najwyżej punktowaną robotą, jaka została. Padł kodem 3
w warstwie `before`: sześć kryteriów zeszło w sekundę, a `AC-2` zjadło 840 s i zgłosiło
„did not FINISH". `AC-2` celowo otwiera **drugie, zapisujące** połączenie SQLite prosto na plik
bazy, z pominięciem naszego API — i to połączenie zawisło.

Zacznij od przeczytania `../loadout-task-T-06/src-tauri/tests/store_append_only.rs` i tego, co
faza kontraktu wpisała do `src-tauri/src/store/`. Pytanie, na które odpowiadasz najpierw: **czy
w warstwie `before` szkielet w ogóle otwiera połączenie?** Przy `todo!()` powinien panikować
natychmiast, nie wisieć — jeśli wisi, faza kontraktu napisała za dużo. Jeśli natomiast wisi
prawdziwa implementacja, to **defekt produktu, nie testu**: magazyn trzyma zamek, którego nie
oddaje, więc `sqlite3 loadout.db` z terminala zawiesi się tak samo, a kryterium zrobiło
dokładnie to, po co je napisano.

**Potem trzy gotowe od ręki:** `T-13`, `T-14`, `T-18`. Mają wszystkie zależności w trunku i są
od siebie niezależne — puść je razem.

**`T-08` ma priorytet nad resztą frontu.** `T-25` dał mechanizm montowania sekcji, ale żadna
sekcja nie ma jeszcze `index.tsx`, więc `npm run dev` pokazuje pięć pustych ekranów. `T-08`
niesie `AC-8` — jedyne kryterium w całym projekcie, które renderuje `<App>` bez wstrzykiwania
i sprawdza, że zdania pustego ekranu **nie ma**. Dopóki nie wyląduje, aplikacja jest zielona
i pusta, a bramka nigdy o tym nie powie.

**Po 2026-08-20:** wracają kredyty Codeksa. Wtedy `S-3` i `T-10`, a przede wszystkim przegląd
cross-vendor wszystkiego, co powstało w trybie same-vendor (`docs/PLAN.md` §6a) — czyli całości.
To nie jest formalność: jedyne realne defekty na **zielonej** bramce znalazł dotąd recenzent
innego vendora.

### Trzy rzeczy, które już wiadomo, i nie trzeba ich odkrywać drugi raz

**Konflikt w `lib.rs` przy lądowaniu jest pewny, nie jest awarią.** Ten plik zbiera `pub mod` od
każdego zadania tworzącego moduł. Przy T-11 i T-12 wystąpił dwa razy. Rozwiązanie jest zawsze to
samo: **zachowaj obie deklaracje**, nie wybieraj strony. To samo dotyczy `engine/mod.rs`,
`memory/mod.rs`, `skills/mod.rs`, `drivers/mod.rs`.

**`TASK.md` nie ma prawa przeżyć lądowania.** `integrate.sh` kasuje go i doszywa do commita —
ale jeśli rozwiązujesz konflikt **ręcznie** i sam robisz `git commit`, ten krok się nie wykona.
Trunk z cudzym kontraktem sprawia, że każdy nowy worktree rodzi się z `TASK.md` w środku,
a `ship-task.sh` słusznie odmawia wtedy startu. Sprawdź po każdym ręcznym merge'u.

**Sprzęt nie jest limitem.** Zmierzone przy sześciu agentach: Apple M4 Max, 64 GB, load 2,6 na
szesnastu rdzeniach, 2,5 GB pamięci, **zero** czekania na muteksie cargo. Agent czeka na
odpowiedź modelu, nie na procesor. Limitem jest szerokość fali zależności — odpalaj wszystko,
co ma spełnione zależności, i przeliczaj zbiór po każdym lądowaniu.
