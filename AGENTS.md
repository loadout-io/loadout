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
12. **Dwa kroki nie mogą pisać po tych samych ścieżkach.** Odmowa przy zapisie workflow, nie w trakcie biegu.

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
