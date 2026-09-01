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

Zadania nie ma. Jest **prompt**, który właściciel podaje harnessowi:

```bash
scripts/h run <id> --prompt "co ma powstać"
```

```
worktree → plan → implementacja → checki + weryfikacja
                       ↑                    │
                       └──── max 2 poprawki ┘
```

Weryfikator (inny vendor niż piszący, decyzja D3) odpowiada na JEDNO pytanie: czy zadanie
zostało zrobione i czy ta funkcjonalność działa. Trzy wyjścia: `DZIALA`, `NIE_DZIALA`
+ co konkretnie, `NIE_WIEM`. Nie recenzuje kodu, nie żąda dowodów, nie proponuje ulepszeń.
Przy `NIE_DZIALA` poprawia **ten sam** agent, przez `claude --continue`. Po dwóch nieudanych
poprawkach STOP i pytanie do człowieka.

Pełne okablowanie: [`.loadout/h/README.md`](.loadout/h/README.md). Reguły niżej są wiążące
niezależnie od tego, kto je wykonuje.

Cztery rzeczy w tej pętli są nienegocjowalne:

- **Test z sekcji „Test" planu musi paść na starym kodzie, ZANIM powstanie poprawka.**
  Test, który przechodził od początku, niczego nie dowodzi. To jedyna rzecz, której harness
  nie egzekwuje mechanicznie — sprawdza ją weryfikator, patrząc na diff.
- **Zielone wymaga licznika przejść.** `exit 0` bez ani jednego zameldowanego przejścia jest
  czerwone (niezmiennik 19). Kod testowany biegnie w tym samym procesie, którego kod wyjścia
  czytasz. Pilnuje tego `PASS_COUNT` w `.loadout/h/h.py` i to jedyna rzecz, która została
  z całej dawnej maszynerii dowodowej.
- **Kryterium dotyczy zdania, które widzi CZŁOWIEK**, nie wartości zwróconej przez funkcję
  (niezmiennik 29). Zielone kryterium nad martwą funkcją jest wadą, dla której to repo powstało.
- **Wyrocznia jest dla biegu niezapisywalna.** `.loadout/h/`, `checks/`, `scripts/`,
  `AGENTS.md` i `docs/DECISIONS-LOCKED.md` są w `deny` — bieg nie ma jak osłabić tego, co go
  sądzi. Jeśli check jest zły, mówi to (§7), a nie zmienia.

Suita całego repo nie należy do pętli zadania. Biegnie raz, przy lądowaniu:
`scripts/h land <id>` robi merge i odpala `scripts/ci.sh full`.

## 2a. Kontrakt kryterium — wszystko, co musisz o nim wiedzieć

1. **Test rustowy jest MODUŁEM jedynego celu integracyjnego, nigdy nowym plikiem wprost
   w `src-tauri/tests/`.** Plik w `src-tauri/tests/it/<nazwa>.rs`, deklaracja `mod <nazwa>;`
   w `src-tauri/tests/it/main.rs`, wywołanie `cargo test --test it <nazwa>::`.

   To nie jest kwestia gustu, tylko pomiar: Rust robi z każdego pliku w `tests/` osobne
   binarium, które statycznie linkuje całą bibliotekę razem z 527 skrzyniami Tauri — ~60 s za
   sztukę, przy 6,0 s wykonania wszystkich testów razem. Stara reguła („globalnie unikalna
   ścieżka pliku na kryterium") zamówiła 462 takie binaria i tak `full-test` dorósł do budżetu
   9000 s. Moduł w celu `it` daje tę samą liczbę testów, te same asercje i **jeden** link.
   Pilnuje tego `checks/tests-listed.sh`: plik bez wiersza `mod` nie jest kompilowany ani razu,
   a test nieobecny czyta się dokładnie jak zdany.
2. **Front wskazuje plik po ścieżce:** `npx --no-install vitest run <ścieżka>.test.tsx`.
3. **Zawężenie nie może zazielenić checka.** Filtr, który nic nie dopasował, da `0 passed`
   i polegnie na liczniku przejść. Dlatego zawężanie jest bezpieczne.
4. **W Ruście: najpierw sygnatura z `todo!()`**, żeby test się skompilował i padł w czasie
   wykonania. Test, który się nie kompiluje, niczego nie uruchomił. W TypeScripcie ta sama
   zasada, inna pułapka: `vitest` przewraca się już na **zbieraniu** plików, więc każdy moduł,
   który test importuje, musi istnieć jako pusty szkielet — funkcja rzucająca
   `throw new Error("not implemented")`, komponent renderujący pusty fragment. Import ma się
   rozwiązać, a test paść **na asercji**.

## 3. Reguły wiążące

Numerowane, bo kontrakty biegów i prompty cytują je po numerze („niezmiennik 6").

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
| `cargo test --tests` albo cały katalog vitesta w pętli | zawężone polecenie: `cargo test --test it <moduł>::`, pojedynczy plik spec |
| Nowy kolor semantyczny | jeden z czterech istniejących, albo brak koloru |
| Hex w kodzie komponentu | token z `src/styles/theme.css` |
| Trait z jedną implementacją | konkretny typ, dopóki nie ma drugiej |
| Migracja schematu „na przyszłość" | jedna wersja, aż zajdzie potrzeba drugiej |
| Pełny SHA-256 w głównym widoku | 8 znaków, pełny za kliknięciem |
| Wiersze transkryptu rozwinięte domyślnie | zwinięte; wyjątki to proza, pytania, błędy, struktura |
| Enum z drutu jako tekst w UI | zdanie po angielsku z tabeli tłumaczeń |
| „Sprawdzenie", które sprawdza samo siebie | uczciwy stan „no checks configured" |
| Recenzent, który blokuje albo recenzuje kod poza zadaniem | jedno pytanie: DZIAŁA / NIE_DZIAŁA + konkret |
| Komenda złożona: `a; b; c` w jednym Bashu | **jedna komenda na wywołanie.** Claude Code rozbija złożone i pyta o zgodę na każdy człon; w biegu bez człowieka nie ma kto jej dać, więc to jest stracona tura. Zmierzone: 13 odmów w jednej fazie |
| Dopisanie czegokolwiek do `.loadout/h/` bez sprawdzenia w `runs/`, czy to kiedykolwiek złapało realny błąd | poprzedni harness miał 9323 linie i to jest powód, dla którego go nie ma |

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
.loadout/h/                   CAŁY harness: h.py, checks.json, trzy prompty, guards.sh
.loadout/h/checks.json        zmienione ścieżki -> checki. Jedyne miejsce, gdzie się je dodaje
checks/*.sh                   sprawdzenia WŁASNE (niezmienniki, D1, D5); jednolinijkowce
                              stoją wprost w checks.json
runs/<id>/                     transkrypty biegu; `.git/h/<id>.json` trzyma jego stan
```

---

## 6. Komendy, które działają

```bash
npm install && cargo fetch     # pierwsze uruchomienie

npm run app                    # aplikacja (Tauri dev)
npm run dev                    # sam frontend w przeglądarce, bez Rusta

scripts/h run <id> --prompt "co ma powstać"     # cały bieg
scripts/h check                # checki dla zmienionych ścieżek (to woła hak Stop)
scripts/h check density        # jeden check po nazwie, także manualny
scripts/h land <id>            # merge gałęzi + PEŁNE CI na trunku
scripts/h list | status <id> | clean <id>

./worktree.sh <nazwa>          # wypisuje ścieżkę do nowej kopii repo — to cały interfejs
```

`scripts/ci.sh` jest jedynym źródłem prawdy o tym, co znaczy „zielone".
Workflow GitHuba tylko go opakowuje.

---

## 7. Kiedy przestać i zapytać człowieka

- Bramka jest czerwona po jednej rundzie poprawek.
- Zadanie wymaga dotknięcia pliku, którego plan nie przewidywał, a nie da się go pominąć.
- Trzeba zmienić coś w `.loadout/h/`, `checks/`, `scripts/` albo w `docs/DECISIONS-LOCKED.md`.
- Kryterium akceptacji da się przejść w sposób, który jego zdaniem jest oszustwem.
  **Powiedz to zamiast tak zrobić.** To najcenniejsza rzecz, jaką możesz zgłosić.
