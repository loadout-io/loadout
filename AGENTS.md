# AGENTS.md — karta pracy w repo Loadout

Czytasz to jako pierwszy plik. Jeśli coś w twoim zadaniu kłóci się z tym dokumentem — wygrywa ten dokument.
Jeśli kłóci się z `docs/DECISIONS-LOCKED.md` — wygrywa tamten.

Dokumentacja po polsku. **Interfejs użytkownika po angielsku.** (decyzja D5)

---

## 1. Co budujemy

Aplikacja desktopowa (Rust + Tauri + React, macOS), w której układasz graf agentów kodujących
i go uruchamiasz. Zastępuje Superset, Warpa i ręcznie klejone harnessy.

Poprzednie podejście (`~/Projects/poprzedni prototyp`, 75 tys. linii Rusta w dwa dni) umarło na złożoność:
osiem rodzajów „autorytetu", trzy maszyny stanów, cztery migracje schematu w dwudniowym repo,
i **nigdy nie uruchomiło agentów naprawdę równolegle**. Cała ta historia jest opisana
w `docs/research/projects/` i jest wiążąca jako lista rzeczy, których nie powtarzamy.

---

## 2. Pętla pracy

### Wyjątek właściciela — authoring zadań

Na jawne polecenie właściciela „utwórz taski” agent może utworzyć nowe `tasks/<ID>.md`
oraz zaktualizować `docs/PLAN.md` bez wcześniejszego taska. Ten wyjątek obejmuje wyłącznie
kontrakty zadań i dokumentację; kod produkcyjny nadal wymaga osobnego worktree, czerwonego
`before` i `OWNS`. Dodane 2026-08-21 po wyraźnym „masz approve na wszystko, god mode”, żeby
zgoda właściciela nie była interpretowana jako zgoda na pominięcie bramki.

```
tasks/<ID>.md                       ← zadanie już istnieje; nie wymyślasz go sam
   │
   ▼  ./worktree.sh <ID>            ← własna kopia repo, własna gałąź
   │
   ▼  ./verify.sh before            ← DOWIEDŹ, że kryteria są czerwone, ZANIM cokolwiek napiszesz
   │                                  zielone „before" = kryterium nic nie sprawdza
   ▼  implementacja
   │
   ▼  ./verify.sh quick             ← w pętli, ~20 s
   │
   ▼  ./verify.sh full              ← przed oddaniem
   │
   ▼  ./review.sh <vendor>          ← druga opinia, innego vendora, tylko do odczytu
   │
   ▼  ./repair.sh                   ← DOKŁADNIE jedna runda poprawek, jeśli są uwagi
   │
   ▼  ./integrate.sh <gałąź>        ← jedna gałąź naraz, pełna bramka po KAŻDEJ
```

Trzy rzeczy, które w tej pętli są nienegocjowalne:

- **`before` musi być czerwone z właściwego powodu.** Bramka zna ~24 sposoby, na jakie sprawdzenie
  wykłada się, nie uruchamiając niczego (brak modułu, brak komendy, `Tests N skipped (N)`, rc 124/127).
  To nie liczy się jako czerwone.
- **Recenzent nie może zatwierdzić ani zablokować.** Jego schemat odpowiedzi ma `verdict ∈ {concern, none}`.
  Strukturalnie nie ma czego zatwierdzić. Niedostępny recenzent to `exit 0` z notatką, nie awaria.
- **Jedna runda poprawek.** Recenzent planuje (tylko do odczytu), pisarz wykonuje plan, którego nie napisał.
  Potem bramka. Jeśli dalej czerwono — woła człowieka.

---

## 2a. Kontrakt kryterium — wszystko, co musisz o nim wiedzieć

Nie czytaj `harness/gate.py`, żeby się tego dowiedzieć. Całość jest tutaj.

1. **Jedno kryterium, jedna linia `check:`, dokładnie jeden plik testu wskazany ścieżką.**
   Nigdy filtr po nazwie (`-t`, `--grep`, `--test-name`): filtr, który nic nie dopasował,
   raportuje sukces i tak vitest dwa razy pokonał warstwę `before` w repo źródłowym.
2. **Ścieżka testu jest globalnie unikalna** we wszystkich plikach zadań. To jest jedyny powód,
   dla którego równoległe zadania są bezpieczne.
3. **Numeracja `## AC-n` biegnie 1..n bez luk.**
4. **Zielone wymaga licznika przejść w wyjściu.** Exit 0 bez „N passed" / „Ran N tests" jest
   czerwone — kod testowany biegnie w tym samym procesie, którego kod wyjścia czytasz.
   `expect: none` to jedyna, recenzowalna furtka.
5. **W warstwie `before` kryterium musi paść dlatego, że BRAKUJE ZACHOWANIA** — nie dlatego, że
   sprawdzenie nie ruszyło. „Brak modułu", „nie znaleziono plików testowych", „nie da się wczytać
   manifestu" i ~30 innych podpisów nie liczą się jako czerwone.
   **W Ruście oznacza to: najpierw sygnatura z `todo!()`**, żeby test się skompilował i padł
   w czasie wykonania. Test, który się nie kompiluje, niczego nie uruchomił.

   **W TypeScripcie to samo, tylko pułapka jest inna.** `vitest` przewraca się już na
   **zbieraniu** plików: brakujący `src/state/skills.ts` daje „Failed to load … Cannot find
   module", a to jest podpis z `NOT_A_REAL_RED`, nie czerwień. Więc zanim odpalisz
   `./verify.sh before`, każdy moduł i komponent, który test importuje, **musi istnieć jako
   pusty szkielet**: funkcja rzucająca `throw new Error("not implemented")`, hook zwracający
   akcje, które jeszcze nic nie robią, komponent renderujący pusty fragment. Import ma się
   rozwiązać, a test paść **na asercji**. To jest dokładny odpowiednik `todo!()` i obowiązuje
   tak samo — kryterium frontendowe bez szkieletu spala rundę identycznie jak rustowe.

## 3. Reguły wiążące

Numerowane, bo pliki zadań je cytują („niezmiennik 6").

### Architektura

1. **`engine/` nie importuje `tauri::*`.** Sprawdzane gerpem w bramce. Bez tego silnik nie da się
   przetestować bez okna, a osobny daemon nigdy nie powstanie.
2. **Do SQLite pisze wyłącznie `store::writer`.** Drugie połączenie zapisujące to zakleszczenie,
   nie „czasem wolniej".
3. **Kod zależny od platformy istnieje tylko w `engine/supervisor.rs`.** `#[cfg(windows)]` gdziekolwiek
   indziej przewraca bramkę. To jest jedyny powód, dla którego port na Windows będzie gałęzią `cfg`,
   a nie przepisaniem.
4. **Pliki są prawdą. SQLite jest indeksem.** `loadout.db` musi dać się skasować bez utraty czegokolwiek.
   Jeśli piszesz pole, którego nie da się odtworzyć z plików — łamiesz ten niezmiennik.

### Procesy i współbieżność

5. **Nigdy nie wywalaj biegu na nieznanym zdarzeniu.** `#[serde(other)]` na każdym enumie z drutu
   i `Option<T>` na każdym polu, które nie jest niezbędne. Vendorzy dokładają typy zdarzeń co tydzień,
   po cichu. Nieznaną linię logujemy do pliku debug i porzucamy.
6. **Zabijamy grupę procesów i dowodzimy, że nie żyje.** `process_group(0)` przy starcie,
   SIGTERM → łaska → SIGKILL, potem `kill(-pgid, 0)` musi dać `ESRCH`. Dopóki nie ma dowodu —
   traktujemy jako żywe. Osierocony `claude` pali limit w tle; to błąd finansowy, nie higieniczny.
7. **Anulowanie jest wartością, nie błędem.** `enum Outcome { Done, Cancelled }`, nigdy `Err(Cancelled)`.
   I nigdy globalny `AtomicBool` — bool przecieka między operacjami. Monotoniczna generacja `AtomicU64`.
8. **`std::sync::Mutex` nigdy nie jest trzymany przez `await`.** Udokumentuj to na samym polu.
9. **Prompt i sekrety wyłącznie przez stdin.** Nigdy w argv, nigdy w pliku tymczasowym, nigdy w logu.
   `env_clear()` plus jawna lista przepuszczanych zmiennych.
10. **`tokio::time::timeout` wokół kroku anuluje zadanie Rusta, nie proces systemowy.**
    Każda ścieżka limitu czasu przechodzi przez eskalację zabijania w supervisorze.
11. **„Ile naraz" musi znaczyć naraz.** poprzedni prototyp miał `max_parallel`, które było tylko szerokością
    wysyłki: jeden worker, `run_ready(1)`, cztery „równoległe" pasy w rozłącznych oknach po ~0,5 s.
    Równoległość to cała przesłanka tego produktu. Test musi dowodzić nakładania się w czasie.
12. **Dwa kroki nie mogą pisać po tych samych ścieżkach.** Odmowa najpóźniej przy Starcie, nigdy
    w trakcie biegu. Kolizja widoczna z samego pliku jest przy zapisie ostrzeżeniem, a przed biegiem
    problemem: szkic, w którym kafelki leżą luzem, zanim człowiek pociągnie strzałki, ma się
    ZAPISAĆ. Odmowa i tak pada, zanim ruszy pierwszy proces (`check_to_run` w `commands::run`).

### Interfejs

13. **Jeden fakt, jedno miejsce.** Limit żywych regionów na fakt wynosi 1. poprzedni prototyp pokazywał stan
    połączenia w sześciu miejscach.
14. **Zero żargonu w tekście widocznym dla użytkownika.** Wiążąca jest tabela
    `docs/research/projects/00-SYNTHESIS.md` §2.2. Egzekwuje `checks/quick-vocabulary.sh`.
    Enum z drutu (`gate.decision_recorded`) nigdy nie trafia na ekran.
15. **Kuracja dzieje się w Ruście, w mapowaniu zdarzenie→linia, nie w CSS.** Jeśli „czysty widok"
    da się zepsuć zmianą arkusza stylów, to nie jest czysty widok.
16. **Kontrolka bez handlera nie wchodzi do repo.** poprzedni prototyp ma trzy martwe przyciski.
17. **UI nie rysuje relacji, których nie ma w danych.** Żadnych ozdobnych krzywych między
    zakodowanymi na sztywno współrzędnymi.
18. **Sufit gęstości z `docs/ARCHITECTURE.md` §7 jest mierzony, nie oceniany okiem.** Baseline może tylko maleć.

### Uczciwość harnessu

19. **Kod wyjścia to nie dowód.** Zielone bez licznika przejść jest czerwone. Kod testowany biegnie
    w tym samym procesie, którego kod wyjścia czytasz — `os._exit(0)` na poziomie modułu zazielenia
    całą suitę.
20. **Test sprawdza zachowanie, nie obecność stringa.** Selftest w spreadsheet asertował
    `"--sandbox workspace-write" in ship-task.sh`, przechodził **na komentarzu**, a żywa flaga
    brzmiała `danger-full-access`. Zasadź prawdziwe naruszenie, wymagaj czerwonego, przywróć.
21. **Nie pisz artefaktu, którego żaden skrypt nie czyta.**
22. **Ewaluacja nie mieszka wewnątrz systemu, który mierzy.** W meetnotes refaktor skasował 1499 linii
    ewaluacji i nikt nie zauważył przez trzy dni.
23. **Polityka mieszka w jednym rdzeniu, adaptery mają po pięć linii.** Przepisanie polityki w adapterze
    per vendor to sposób, w jaki skanowanie sekretów po cichu umarło.

### Higiena

24. **Komentuj DLACZEGO, zwłaszcza incydent.** Datowany powód przy każdej nieoczywistej linii.
    To najtańsza konwencja na tej liście i powód, dla którego 53-tysięczne drzewo meetnotes
    da się nawigować.
25. **Migracje są addytywne i idempotentne.** `CREATE TABLE IF NOT EXISTS` + `add_column_if_missing`.
    `DROP`, `ALTER … DROP COLUMN` i przepisywanie wierszy są zakazane.
26. **Nie uruchamiaj dwóch ciężkich `cargo`/`rustc` naraz na tym Macu.** Kilka równoległych linków
    przypina kompresor pamięci macOS i zamraża maszynę przy zerowym swapie.

### Silnik

*Numeracja jest dopisywalna. Nigdy nie przenumerowuj — zadania cytują reguły po numerze,
a przesunięcie o jeden zamienia wszystkie cytowania w ciche kłamstwo.*

27. **Żaden etap biegu nie jest zaszyty w Ruście.** W `scheduler.rs` nie ma prawa istnieć
    `if review_enabled` ani żaden inny warunek nazywający etap. Kolejność mieszka **wyłącznie
    w grafie**; silnik wykonuje graf i nie zna pojęcia „recenzja" — krok z agentem-recenzentem
    jest dla niego zwykłym krokiem. To jedyny sposób, żeby decyzja D7 była prawdziwa, a nie
    deklarowana: etap zaszyty w kodzie **jest** domyślny i nie da się go wyłączyć konfiguracją.
    Sprawdzane gerpem w bramce razem z niezmiennikiem 1.

28. **Najpierw skrypt albo hak, dopiero potem prompt.** Kiedy jakieś zachowanie ma się
    powtarzać albo przestać się powtarzać, kolejność prób jest ustalona i nie wolno jej
    odwracać: **(1)** czy da się to wymusić hakiem, który po cichu naprawia stan
    (`PostToolUse` formatujący zapisany plik); **(2)** czy da się to wykryć sprawdzeniem
    w `checks/`, które świeci na czerwono; **(3)** czy da się to uczynić niemożliwym przez
    uprawnienia w `.claude/settings.json`. Dopiero kiedy wszystkie trzy odpadną — prompt.

    Powód jest mierzalny, nie estetyczny. Prompt jest **miękki**: bieg może go zignorować
    i nikt się o tym nie dowie, bo nie ma kto sprawdzić. Rośnie monotonicznie, bo każdy
    incydent dokłada akapit, a nikt nigdy żadnego nie usuwa. I kosztuje tokeny w **każdym**
    biegu, na zawsze. Skrypt jest twardy, deterministyczny, kosztuje raz i sam siebie testuje.
    Zmierzone 2026-08-15: „pamiętaj uruchomić formatter" w promcie kontraktu bieg wykonywał
    niekonsekwentnie i kosztowało to całą rundę naprawczą za przecinek; hak `PostToolUse`
    skasował tę klasę czerwieni w całości i zwolnił cztery wiersze promptu. Ta sama historia
    z backtickami w heredocach: ostrzeżenie w promcie wracało, `prompt_backticks`
    w `scripts/ci.sh` nie wróciło ani razu.

    Kiedy prompt **jest** właściwym narzędziem: gdy chodzi o zachowanie, a nie o stan, który
    da się wykryć i naprawić. „Jedna komenda na wywołanie Bash" zostaje promptem, bo hak
    odmawiający kosztuje dokładnie tę samą turę, którą kosztuje odmowa uprawnień — zysku
    zero, a dochodzi ryzyko fałszywej odmowy. Wybór promptu ma być **udokumentowany**
    (`docs/HARNESS-QUEUE.md`, sekcja „czego świadomie nie mechanizujemy”), nie domyślny.

29. **Kryterium o komunikacie albo odmowie asertuje je tam, gdzie CZŁOWIEK je widzi** — nigdy
    tylko w funkcji, która je produkuje. Zwrócona wartość dowodzi, że mechanizm istnieje;
    zdanie na ekranie dowodzi, że produkt działa. Między jednym a drugim mieszka klasa wady,
    dla której to repo powstało: **kryterium zielone, funkcja martwa.**

    Zmierzone 2026-08-20, w jednej fali siedmiu zadań. Recenzent — ten sam vendor, inny model —
    złapał tę klasę **cztery razy na ZIELONEJ bramce**, a żadne z siedemnastu kryteriów tej
    fali jej nie widziało:

    - przycisk propozycji renderował się wyłącznie z propsem `command`, którego `HistoryRow`
      nie miał, a żaden produkcyjny wołający nie podawał. Jedyną ścieżką zapalającą asercję
      był test, który podawał go wprost;
    - odmowa użycia narzędzia była dowodzona na wartości `ToolsRefused`, a nie na zdaniu, które
      z niej powstaje — regresja gubiąca `step_id` przeszłaby, zostawiając człowieka z odmową,
      która nie mówi, który kafelek odmówił;
    - zdanie odmowy `/run` miało wracać **i być pokazane**, a testowana była wyłącznie połowa
      „wracać";
    - wiersz z odpowiedzią wiersza wejścia istniał w modelu i nie miał drogi na ekran.

    **Jak to spełnić, kiedy nie ma jsdom.** Czysty moduł dowodzi TREŚCI zdania; `renderToStatic
    Markup` dowodzi, że zdanie jest w markupie i wisi na prawdziwej ścieżce; `e2e/harness.ts`
    dowodzi, że dochodzi tam po prawdziwym kliknięciu. Kryterium wolno wybrać jedno z trzech,
    ale **nie wolno poprzestać na czwartym** — na wartości zwróconej przez funkcję, której nikt
    nie woła.

    To jest ta sama różnica, na której stoi cały produkt: **co agent powiedział** kontra
    **co się stało** (`docs/research/projects/00-SYNTHESIS.md` §2.1). Kryterium, które pyta
    wyłącznie funkcję, pyta o pierwsze.

---

## 4. Zakazane → zamiast tego

| Zakazane | Zamiast tego |
|---|---|
| `unwrap()` w kodzie produkcyjnym | `?` z wariantem błędu, albo `expect()` z powodem i uzasadnieniem |
| `panic!` w silniku | zwróć `Err`; panika w agentowym runtime zabiera cały bieg |
| Globalny `AtomicBool` na anulowanie | monotoniczna generacja `AtomicU64` (niezmiennik 7) |
| `Err(Cancelled)` | wariant wartości `Outcome::Cancelled` |
| `cargo clippy --all-targets` w pętli | `cargo clippy --lib`; pełna forma tylko w bramce |
| Nowy kolor semantyczny | jeden z czterech istniejących, albo brak koloru |
| Hex w kodzie komponentu | token z `src/styles/theme.css` |
| Trait z jedną implementacją | konkretny typ, dopóki nie ma drugiej |
| Migracja schematu „na przyszłość" | jedna wersja, aż zajdzie potrzeba drugiej |
| Pełny SHA-256 w głównym widoku | 8 znaków, pełny za kliknięciem |
| Wiersze transkryptu rozwinięte domyślnie | zwinięte; wyjątki to proza, pytania, błędy, struktura |
| Enum z drutu jako tekst w UI | zdanie po angielsku z tabeli tłumaczeń |
| „Sprawdzenie", które sprawdza samo siebie | uczciwy stan „no checks configured" |
| Recenzent, który blokuje | uwaga bez mocy sprawczej; decyduje bramka |
| Komenda złożona: `a; b; c` w jednym Bashu | **jedna komenda na wywołanie.** Claude Code rozbija złożone i pyta o zgodę na każdy człon; w biegu bez człowieka nie ma kto jej dać, więc to jest stracona tura. Zmierzone: 13 odmów w jednej fazie |
| Czytanie `harness/gate.py`, żeby poznać kontrakt kryterium | kontrakt jest w §2a tego pliku, w pięciu zdaniach. 34 KB Pythona to nie jest specyfikacja |

---

## 5. Gdzie co leży

```
docs/DECISIONS-LOCKED.md      decyzje człowieka — nie podważaj ich
docs/ARCHITECTURE.md          kształt systemu, maszyna stanów, sufit gęstości
docs/PLAN.md                  fazy, kolejność, linia cięcia MVP
docs/design/DESIGN.md         tokeny i komponenty; theme.css jest jego lustrem
docs/research/projects/       rekonesans trzech repo źródłowych + synteza
docs/research/topics/         osiem raportów tematycznych + ADR-y
docs/patterns/<nn>-<nazwa>.md wzorce, które zadania cytują po nazwie pliku
tasks/<ID>.md                 zadania; bramka parsuje z nich WYŁĄCZNIE `## AC-n` i `check:`
harness/                      bramka, schemat recenzji, snapshoty
checks/                       pojedyncze sprawdzenia; nazwa pliku steruje odkrywaniem
runs/last.json                paragon ostatniego uruchomienia bramki
```

---

## 6. Komendy, które działają

```bash
npm install && cargo fetch     # pierwsze uruchomienie

npm run app                    # aplikacja (Tauri dev)
npm run dev                    # sam frontend w przeglądarce, bez Rusta

./verify.sh before             # dowiedź, że kryteria są czerwone
./verify.sh quick              # ~20 s: fmt, clippy --lib, tsc, zakres plików
./verify.sh full               # wszystko + testy
./verify.sh full --only AC-3   # jedno kryterium

./worktree.sh <ID>             # wypisuje ścieżkę do nowej kopii repo — to cały interfejs
./review.sh codex              # druga opinia
./integrate.sh <gałąź>         # jedna gałąź, pełna bramka po każdej
```

`scripts/ci.sh` jest jedynym źródłem prawdy o tym, co znaczy „zielone".
Workflow GitHuba tylko go opakowuje.

---

## 7. Kiedy przestać i zapytać człowieka

- Bramka jest czerwona po jednej rundzie poprawek.
- Zadanie wymaga dotknięcia ścieżki, której nie ma w jego bloku `<!-- OWNS -->`.
- Trzeba zmienić coś w `harness/`, `checks/`, `verify.sh` albo w `docs/DECISIONS-LOCKED.md`.
- Kryterium akceptacji da się przejść w sposób, który jego zdaniem jest oszustwem.
  **Powiedz to zamiast tak zrobić.** To najcenniejsza rzecz, jaką możesz zgłosić.
