# Loadout — zadania domykające niezawodny workflow wieloagentowy

> Aktualizacja 2026-09-05: przed wykonaniem którejkolwiek karty przeczytaj
> [reweryfikację main i rozszerzenie WF-21–WF-28](REVALIDATION-2026-09-05.md).
> Ma ona pierwszeństwo w zakresie statusów, zależności i poprawek kontraktów.
> Baza odczytana do reweryfikacji to `715567effb059ec3e684de2ba17d231b854d0b04`.
> Worktree tego dokumentu nadal stoi na starszym SHA — nie wykonano merge ani rebase.

Status: **projekt wykonania, nie zaimplementowane poprawki**. Data: 2026-09-04.
Punkt odniesienia: `1c6e579c94507cfaf701b16a32bfe35d99b78d05`.

## 1. Cel i granice

Po wykonaniu tych zadań Loadout ma uruchamiać grafy z równoległymi agentami i skończonymi
pętlami, zachowywać komplet ich rezultatów, pozwalać rozmawiać z Leadem podczas biegu,
przekazywać jawnie wybrane instrukcje i umiejętności repo oraz porównywać całe workflow w Labie.
Nie przepisujemy silnika. Zachowujemy istniejący scheduler, sterowniki, supervisor,
globalną pulę miejsc, kontrolę wydatku i plikową historię.

„Dowolne repo” oznacza brak zależności od struktury Loadouta: Git i zwykły folder, dowolny
stos z przygotowaniem podanym przez człowieka. Nie oznacza automatycznej znajomości każdego
package managera, wykonywania zastanych hooków ani prawa do dowolnego pliku gospodarza.
Git bez pierwszego commita może dostać jawną odmowę; nie tworzymy commita bez pytania.

„Komunikacja” obejmuje trzy różne rzeczy: przekazania po zależnościach, dopowiedzenie
do obsługiwanej sesji oraz opcjonalną skrzynkę wiadomości między krokami. Przyjęcie tekstu
przez kanał nie dowodzi przeczytania ani wykonania polecenia przez model.

„Testowanie harnessu” oznacza tutaj ocenę **workflow zbudowanego w aplikacji**, łącznie
z jego planowaniem, sprawdzeniami i rundami naprawy. Nie zmieniamy harnessu `scripts/h`,
którym rozwijany jest Loadout. Ocenie podlega rezultat, nie deklaracja autora workflow,
że jego własne sprawdzenia były zielone.

### Rozstrzygnięcia wspólne

1. Graf pozostaje zamrożony podczas biegu. Lead może odczytać stan, odpowiedzieć w imieniu
   człowieka na wskazane pytanie, zatrzymać wskazany bieg lub rozpocząć nową próbę.
   Nie dodaje i nie przestawia kroków żywego grafu.
2. Nie dodajemy rodzaju kafelka, ogólnego dziennika transakcji, daemona ani drugiego runnera.
   Przygotowanie projektu i zewnętrzne sprawdzenie w Labie to istniejące kroki `Check`.
3. Trzy istniejące tożsamości nie mogą się zlewać: kafelek grafu, jego kopia i próba pętli.
   `work_key_of(node_key)` zachowuje kopię i usuwa próbę. Nie wymyślamy zastępczej numeracji.
4. `RunRef` identyfikuje workspace i UUID biegu z `run.json`. Operacje na biegu nigdy nie
   oznaczają „znajdź ostatni”. Pole widoczne jako nazwa jest etykietą, nie identyfikatorem.
5. Pliki pozostają prawdą. Dodatkowe manifesty mają nazwanych czytelników: odtwarzanie,
   składanie plików, historia lub Lab. SQLite nie dostaje niezastępowalnego stanu.
6. Zamrażamy wejścia i materiały źródłowe, nie składamy archiwum pełnych promptów ani sekretów.
   Złożony prompt jedzie stdin. Istniejące ograniczenia eksportu raportów nadal obowiązują.
7. Snapshot agenta/skilla w pomiarze jest zapisem tego, co zmierzono. Nie zmienia tożsamości
   zatwierdzonego workflow i nie unieważnia zatwierdzenia po edycji biblioteki; obowiązuje
   [dziedziczenie zamiast kopiowania](../patterns/05-inherit-dont-copy.md).
8. `0444` i hash nie są sandboxem wobec procesu tego samego użytkownika. Nie obiecujemy
   izolacji bez egzekwowanej polityki procesu i testu rzeczywistej odmowy zapisu.

## 2. Podstawa diagnozy

To lokalizacja zachowań w bazowym SHA, nie lista rzeczy już poprawionych:

| Problem | Produkcyjna ścieżka do odczytania przed zadaniem |
|---|---|
| Różne wejście świeżych kopii | `commands/isolate.rs`, `make_from_after_add`: HEAD/WIP pobierane przy tworzeniu kolejnych drzew |
| Utrata usunięcia/rename/mode/symlink | `commands/fan_in.rs`, `Change`, `changes_in`, `fold` |
| Niepełne składanie uznane za zrobione | `commands/run.rs`, `fold_what_came_before`: wpis do `folded` przed zakończeniem składania |
| Pętla pomija ocenę wyniku tekstowego | `commands/run.rs`, `nothing_to_judge` i wywołanie przed rozróżnieniem pracy sędziego |
| Pętla gubi inne kopie | `commands/run.rs`, `what_that_loop_produced`, `node_of` |
| Utrata pracy bez Git | `commands/run.rs`, `close_one_copy`, `close_every_tree` |
| Start Leada zależy od aktywnej karty | `bridge/library.rs::start` → `run/io.ts` → `run-command.ts` → `launch.ts` |
| Obietnica wiadomości bez kanału | `run/index.tsx`, lista odbiorców; `commands/run.rs`, rejestr `voice`; sterownik `codex exec` |
| Lead nie ma całego sterowania | `bridge/verbs.rs`, `bridge/library.rs`; obecne operacje w `ipc.rs` i `commands/rerun.rs` |
| Niejawne/różne dziedziczenie | `import/mod.rs`, `inherit/scan.rs`, `inherit/rewrite.rs`, `skills/place.rs`, `commands/chat.rs` |
| Lab mierzy pojedynczy agent/skill | `lab/mod.rs::Subject`, `lab/plan.rs::compose` |
| Stara historia dostaje nowe kryteria | `commands/lab.rs::read_board_inner`, `score_one(&open.set, past)` |

Ścieżki Rust w tabeli są względne do `src-tauri/src/`, frontend do `src/sections/`.
Usunięcie w fan-in odtworzono także małym testem produkcyjnego modułu podczas audytu.
Pozostałe punkty wynikają z prześledzenia kodu; każde zadanie musi dopiero dostarczyć swój
produkcyjny test RED. Zielone testy istniejącej funkcji nie obalają brakującego scenariusza.

## 3. Jak wykonywać zadania

Każda karta niżej wraz z §1, §3, tabelą zależności i wskazanymi poprzednikami jest kompletnym
kontraktem do przekazania kolejnemu modelowi. Symbole proponowanych typów są kontraktem
semantycznym: dostosuj nazwę do istniejącego odpowiednika, nie twórz drugiego tylko dla nazwy.

1. Przeczytaj aktualne `AGENTS.md`, `docs/DECISIONS-LOCKED.md` oraz wskazane funkcje.
   Sprawdź SHA, zmiany, worktree i aktywne biegi. Jeżeli problem już naprawiono, dowiedź
   tego kryterium zamiast pisać równoległą implementację.
2. Jeden piszący na zakres. Harness: `scripts/h run <id> --prompt "..."`; do promptu
   dołącz treść kontraktu, nie samą ścieżkę do pliku z innego worktree. Nie przywracaj
   starego obowiązkowego formatu `tasks/*.md`, `OWNS`, `verify.sh` ani `runs/last.json`.
3. Najpierw wykonujący się RED. Nowe API dostaje kompilowalny szkielet. Brak importu,
   błąd kompilacji lub zero testów nie jest dowodem reprodukcji.
4. Rust: nowy moduł `src-tauri/tests/it/<nazwa>.rs` i deklaracja w `it/main.rs`.
   Uruchomienie z `src-tauri`: `cargo test --test it <nazwa>::`.
   Frontend: `npx --no-install vitest run <konkretny-plik>.test.tsx` lub `.test.ts`.
   W tabeli podano nazwy głównych nowych testów, nie wymaganie tworzenia nowego targetu.
5. Testy runtime korzystają z produkcyjnego `run_workflow_inner`/mostu/IPC oraz dublerów
   sterowników. Badają cwd, rzeczywiste pliki przekazań, skutki i odczyt historii.
   Dubler nie implementuje schedulerowi oczekiwanego algorytmu. Nakładanie pracy dowodzi
   bariera startu dwóch wykonawców, nie delikatny próg milisekund.
6. Zdanie lub odmowa jest asercją na produkcyjnej drodze do markup/kliknięcia, nie tylko
   na zwróconym enumie. Treść UI po angielsku, bez surowych identyfikatorów z drutu.
7. Rejestrację nowego IPC aktualizuj w istniejących lustrach
   `src-tauri/commands.golden.txt` i `src/sections/commands-wired.test.ts`, jeśli wymagane.
   To nie daje prawa do edycji wyroczni w `checks/` ani `harness/`.
8. Nie rozszerzaj zakresu o refaktor całego `run.rs`. Jeśli konieczny nowy plik nie mieści
   się w karcie albo trzeba dotknąć chronionych skryptów — zatrzymaj się i podaj konkretną
   potrzebę właścicielowi. Nie osłabiaj istniejącego sprawdzenia, żeby dostać zieleń.
9. Ciężkie Cargo i integracje są sekwencyjne. Pełna suita przy `scripts/h land <id>`;
   nie uruchamiaj jej w każdej rundzie zadania. Żywe płatne smoke wymagają uzgodnionego
   projektu testowego i limitu; sam ten dokument ich nie uruchamia.
10. Raport końcowy zadania: zmienione zachowanie, SHA, komenda i wynik RED/GREEN,
    droga obserwacji w UI, ograniczenia. „Zaimplementowane”, „sprawdzone na tym SHA”
    i „zintegrowane” to osobne stwierdzenia.

### Istniejąca kolejka — nie uruchamiać duplikatów

Na bazowym SHA Z-35 jest w kodzie. W kolejce Z-36–Z-50 pozostają pozycje TODO, a część
ma aktywne worktree. To fotografia, nie upoważnienie do przejęcia cudzych zmian.
Ten dokument nie zmienia statusów tamtej kolejki.

| Obecna praca | Zasada integracji z tym planem |
|---|---|
| Z-36 / Z-40 | Zachowują kolejkę tur, postęp narzędzi i interrupt Leada. WF-08–10 konsumują ich wynik, nie implementują ponownie. |
| Z-37 / Z-38 | Zachować historię zakończonych kafelków i budżet refleksji; serializacja zmian `run.rs`/widoku biegu. |
| Z-39 | WF-10 rozszerza tę samą implementację `stop_run` o dokładny adres i weryfikowane potwierdzenie. Jeśli jeszcze nie powstała: jeden właściciel obu zakresów. Rozpoznawanie obcego sygnału pozostaje w Z-39. |
| Z-41 | Współdzielić ochronę publikowanych przekazań; nie uznawać `0444` za wystarczający sandbox wyroczni Labu. |
| Z-42 | WF-01/12/14 używają sondy dostępu przed kopiowaniem i uruchomieniem. |
| Z-43 / Z-44 / Z-45 / Z-48 | Nie omijać projektowego claimu triggera, znanych ograniczeń kosztu, wagi kroku i rachunku tokenów. |
| Z-46 | WF-06 definiuje zachowany wynik, który retencja ma omijać. Jeden kontrakt, nie druga retencja. |
| Z-47 | WF-12/13 dostarczają wspólny spis oczekiwanego kontekstu. „Przekazano” i „vendor zgłosił odczyt” pozostają różnymi faktami. |
| Z-49 | WF-09 używa istniejącego ograniczonego odczytu przekazań, nie pobiera całych transkryptów. |
| Z-50 | WF-08 rozszerza jedno źródło capabilities i jego UI. Jeśli Z-50 nie wystartowało, dołączyć jego zakres do tego samego wykonawcy; nie pisać dwóch opisów możliwości. |

### Kolejność i zależności

| ID | Rezultat | Poprzedniki funkcjonalne |
|---|---|---|
| WF-01 | Jeden obraz wejścia dla izolowanych kopii | — |
| WF-02 | Pełne operacje plikowe w fan-in | WF-01 |
| WF-03 | Niekompletne składanie nigdy nie uruchamia konsumenta | WF-02 |
| WF-04 | Pętla ocenia też wynik tekstowy | — |
| WF-05 | Wyniki wszystkich kopii i prób | WF-04 |
| WF-06 | Zachowanie rezultatów bez Git | WF-01, WF-03 |
| WF-07 | Start Leada w prawidłowym projekcie z potwierdzeniem | — |
| WF-08 | Prawdziwe możliwości wysyłania wiadomości | WF-07 |
| WF-09 | Odczyt stanu dokładnego biegu przez Leada | WF-07 |
| WF-10 | Stop, odpowiedź, ponowienie i wiadomość przez Leada | WF-08, WF-09 |
| WF-11 | Opcjonalna skrzynka agent–agent | WF-05, WF-08, WF-16 |
| WF-12 | Wspólne instrukcje repo dla sesji | WF-01 |
| WF-13 | Kompletny bundle skilla i zgodność sesji | WF-12 |
| WF-14 | Jawne dodatkowe wejścia i przygotowanie kopii | WF-01, WF-12 |
| WF-15 | Niezmienna definicja historycznego pomiaru | WF-01, WF-12, WF-13 |
| WF-16 | Rozdzielone wejścia i kontekst fragmentów grafu | WF-03, WF-05, WF-14, WF-15 |
| WF-17 | Cały workflow jako wariant w Labie | WF-16 |
| WF-18 | Niezależna, odporna na fałszywą zieleń ocena | WF-17 |
| WF-19 | Obsługa porównań workflow w UI Labu | WF-18 |
| WF-20 | Przekrojowy odbiór produktu | WF-06, WF-10, WF-11, WF-13, WF-19 |

Zalecana ścieżka ograniczania ryzyka: WF-04 → WF-05 → WF-01 → WF-02 → WF-03 → WF-06,
następnie WF-07–10, WF-12–19, WF-11 i WF-20. Brak zależności w tabeli nie oznacza
bezpiecznej równoległej edycji: większość kart dotyka `run.rs`, `chat.rs` albo `it/main.rs`.
Równolegle można przygotowywać niezależne testy/odczyty i front po zamrożeniu API.

## WF-01 — Jedna baza plików dla wszystkich izolowanych kopii

**Cel.** Usunąć założenie, że osobne odczyty HEAD i WIP dają tę samą bazę.

**Kontrakt implementacji.**

- W fazie przed pierwszym procesem utworzyć `InputSnapshot`: identyfikator, Git OID jeśli
  istnieje, manifest ścieżek oraz odnośniki do utrwalonych bajtów. Manifest uwzględnia typ,
  executable bit i cel symlinka. Nie trzymać całego repo jako `Vec<Vec<u8>>` w RAM.
- Przechwycić raz obecnie dozwolony zakres tracked + WIP; dla non-git obecną dozwoloną
  zawartość folderu. Współdzielić istniejącą listę wykluczeń. Politykę dodatkowych plików
  rozszerzy dopiero WF-14.
- Do wszystkich `make_from` przekazać odnośnik do tego obrazu. Kolejna kopia nie czyta
  bieżącego HEAD/diff projektu. Rejestracja Git worktree nadal idzie istniejącą drogą.
- Równość dotyczy wspólnego obrazu wejściowego **nowej pracy**, nie wyzerowania wznowień.
  Obecne wcześniejsze wyniki per work key pozostają osobnymi, zamrożonymi nakładkami:
  zapisać origin snapshot i odnośnik do wyniku startowego każdej kopii. Dwie wznowione
  kopie mogą mieć różne pliki; fan-in zna ich wspólne pochodzenie i liczy całość zmian
  względem origin, a nie względem dwóch różnych lokalnych baseline'ów.
- To nie jest atomowy filesystem snapshot całego dysku: wykryta zmiana pliku podczas
  capture daje ograniczone ponowienie, najwyżej dwa, a następnie odmowę przed procesami.
  Po capture kopie muszą być identyczne, nawet gdy człowiek dalej edytuje projekt.
- Manifest i obraz w katalogu biegu publikuje obecny właściciel plików; początkowy `run.json`
  odsyła do nich przed przyjęciem biegu. Czytelnicy: izolacja, fan-in, recovery i Lab.
- Ponowienie z wcześniejszej bazy kopiuje potrzebne dane do nowego biegu. Brak/niezgodność
  starego obrazu oznacza odmowę ponownego użycia, nigdy domyślne porównanie do obecnego HEAD.
  Dotychczasowy odczyt historii starych biegów pozostaje możliwy.
- Jeśli wznowienie wymaga łączenia nowego WIP gospodarza z wynikami o innym origin,
  pierwsza wersja odmawia tego automatycznego fan-in i proponuje nowy niezależny bieg.
  Nie gubi starej pracy ani nie ignoruje po cichu nowych zmian. Same-copy jednego
  wskazanego, dostępnego wyniku nie potrzebuje zgadywania wspólnej bazy wielu rodziców.

**Pliki.** `src-tauri/src/commands/{isolate,run,rerun}.rs`, mały
`src-tauri/src/commands/input_snapshot.rs`, deklaracja modułu w `commands/mod.rs`;
platformowe operacje tylko `engine/supervisor.rs`.

**Test / RED.** `isolated_copies_share_one_input`: bariera po utworzeniu pierwszej kopii,
zmiana tracked pliku w projekcie, utworzenie drugiej przez produkcyjne przygotowanie.
Obie muszą czytać te same zamrożone bajty. Baseline czyta zmienny stan.

**Akceptacja.** Git WIP i non-git; zmiana w trakcie capture; błąd odczytu; uszkodzony
manifest; odtworzenie bez SQLite; brak nowych sekretów/argv. Błąd przed startem nie zostawia
żywego procesu ani claimu. Wznowienie dwóch kopii zachowuje ich odmienne wcześniejsze
zmiany; brak wspólnego origin odmawia fan-in. Manifest ma rzeczywistych czytelników w testach.

**Poza zakresem.** Kopiowanie caches, automatyczne zależności, globalny content-addressed store.

## WF-02 — Fan-in zachowuje usunięcia, rename, symlinki i tryb pliku

**Cel.** Konsument ma dostać pełny wynik pracy rodziców, nie tylko ich istniejące pliki.

**Kontrakt implementacji.**

- Zastąpić bajtowe `Change` stanem `before/after: Option<Entry>`, gdzie `Entry` to katalog,
  plik z referencją do zawartości i executable bit albo symlink z celem. `None` znaczy brak.
- Zmiany liczyć po sumie ścieżek bazy WF-01 i rodzica. Rename jest delete + add;
  nie implementować heurystycznego rozpoznawania przeniesień.
- Rodzic równy bazie nie głosuje przeciw zmienionemu. Jedna zmieniona wersja przechodzi;
  kilka identycznych przechodzi; różne zmiany to konflikt przed jakimkolwiek zapisem.
- Delete/modify, różne cele linka, plik/katalog oraz usunięcie/zastąpienie przodka plikiem
  lub linkiem przeciw operacji potomka to konflikty. Zwykły katalog i jego dzieci nie są
  konfliktem: dwaj rodzice mogą dodać różne pliki w tym samym nowym katalogu.
  Nie dereferencjonować symlinków przy odczycie/składaniu. Pliki binarne są bajtami.
- Składanie dotyczy wyłącznie prywatnego celu. Rodzice muszą być zamknięci lub mieć istniejący
  dowód zamrożenia wyniku. Żywy Serve piszący po drzewie nie staje się zwykłym stabilnym rodzicem.
- Odmowa wskazuje ścieżkę i nazwy konfliktujących kroków w historii i strumieniu.

**Pliki.** `commands/{fan_in,isolate,run}.rs`, neutralne helpery w
`engine/supervisor.rs`; prefiks Rust jak w WF-01.

**Test / RED.** `fan_in_keeps_file_operations`: prawdziwy temp Git, dwa kroki,
jeden usuwa `obsolete.txt`, drugi modyfikuje inny plik; agent downstream `same-copy`
sprawdza nieobecność pierwszego w rzeczywistym cwd. Baseline zostawia plik.

**Akceptacja.** Add/modify/delete/rename/binary/symlink/executable bit, zgodne zmiany,
delete/modify i konflikt strukturalny; odwrócona kolejność kończenia rodziców daje ten sam
wynik. Nowy katalog z plikami i różne dzieci dodane przez dwóch rodziców przechodzą.
Nie zmienia się projekt ani rodzice. Konflikt jest widoczny przez `read_run_inner`
i produkcyjny komponent historii; konsument nie startuje.

**Granica gotowości.** Normalna ścieżka jest kompletna tutaj. Odporność na błąd w połowie
publikacji kończy WF-03; nie ogłaszać wcześniej fan-in odpornym na awarie.

## WF-03 — Częściowy fan-in nie jest gotowym wejściem

**Cel.** Awaria składania nie może uruchomić agenta na częściowym wyniku ani utrwalić go
jako udanej pracy do następnego wznowienia.

**Kontrakt implementacji.**

- Najpierw pełny, sprawdzony `MergePlan`; potem staging bajtów/linków należący do biegu.
  Dopiero po wykryciu wszystkich konfliktów wolno zmieniać docelowe drzewo.
- W istniejących metadanych kopii biegu zapisać rozpoczęcie przygotowania: work key, baza,
  rodzice, digest planu. Po pełnym zastosowaniu i utrwaleniu zapisać `inputReady`.
  To dwa fakty przygotowania kopii, nie nowy ogólny ledger czy maszyna stanów schedulera.
- Wpis do `folded` dopiero po sukcesie. Błąd, Stop lub przerwanie przed `inputReady`
  blokuje każdego konsumenta. Brak znacznika sukcesu nigdy nie oznacza sukcesu domyślnego.
- Nie commitować częściowego celu jako skończonej pracy; `where_it_left_off` i recovery
  nie mogą go wybrać. Rodzice zostają zachowani. Kolejna próba odtwarza nowy cel z pełnej
  bazy i rodziców; gdy ich brakuje, jawnie odmawia.
- Nie podmieniać atomowo całego zarejestrowanego Git worktree. Gwarancją jest kompletne
  wejście albo brak startu konsumenta, nie atomowy rename całego repo.
- Operacje zakotwiczyć w otwartym katalogu, nie podążać za podmienionym symlinkiem przodka.
  Destrukcyjny cleanup wymaga tożsamości posiadanego obiektu; te same bajty na cudzym inode
  nie uprawniają do usunięcia. Kod zależny od platformy tylko w supervisorze.

**Pliki.** `commands/{fan_in,run,reconcile}.rs`, `engine/supervisor.rs`, istniejący model
odczytu historii w `commands/history.rs` jeżeli potrzebny do komunikatu.

**Test / RED.** `interrupted_fan_in_never_starts_a_step`: wąski fault injector publikacji
po pierwszej zmianie; produkcyjny bieg, ponowny odczyt i recovery. Konsument nie może
wystartować ani po błędzie, ani przez cache `folded` przy kolejnej próbie.

**Akceptacja.** Błąd przed zapisem / po pierwszym zapisie / przed oznaczeniem gotowości,
przerwanie i restart, podmiana symlinka oraz cudzy inode. Kompletni rodzice pozostają;
historia wyjaśnia odmowę. Udane składanie wykonuje się raz dla danego wejścia, późniejsza
próba pętli nie nadpisuje wykonanej pracy przypadkowym resetem do bazy.

## WF-04 — Każda pętla ocenia rzeczywisty rezultat

**Cel.** Brak zmian Git nie pomija sędziego, bo rezultatem może być analiza lub przekazanie.

**Kontrakt implementacji.** Usunąć `nothing_to_judge` z decyzji o uruchomieniu sędziego.
Nie zastępować go sprawdzaniem nazwy etapu ani innej heurystyki „nie było kodowania”.
Pętlę kończy istniejący `outcome: pass` lub wynik Check; `fail` uruchamia następną próbę
do limitu. Zachować pomijanie dalszych prób po `pass`, polityki Stop/CarryOn/AskMe i uczciwą
historię pominięcia. Nie zmieniać `isolate::touched`, używanego także do ochrony wyników.

**Pliki.** `src-tauri/src/commands/run.rs`.

**Test / RED.** `loop_judges_results_without_file_changes`: temp Git, agenci bez skilli
zmieniających drzewo; research oddaje wyłącznie tekst, judge odpowiada fail, potem pass.
Dubler syntezy odczytuje rzeczywiste przekazania z promptu. Baseline pomija sędziego.

**Akceptacja.** Judge-agent oraz judge-Check, zmiana wyłącznie późniejszego kroku ciała
lub drugiej kopii, wczesny pass i wyczerpanie limitu dla wszystkich polityk błędu.
W historii pominięta przyszła próba nie jest wykonanym sprawdzeniem.

## WF-05 — Kontekst pętli nie gubi kopii

**Cel.** Synteza dostaje ostatni wynik każdej kopii, a kopia historię swoich prób.

**Kontrakt implementacji.**

- `what_that_loop_produced`: ostatnie faktycznie opublikowane przekazanie per `work_key`,
  nie jedno `.next_back()` per kafelek. Wynik sędziego również zachować.
- Własna historia: poprzednie próby tej samej kopii. Historia wejścia dla sędziego:
  wcześniejsze próby wszystkich kopii wejściowego kafelka, nie pierwszej znalezionej.
- Zastąpić niejednoznaczne `node_of(tile, turn)` wyszukaniem konkretnego work key lub
  wszystkich kopii kafelka w próbie. Kolejność: kafelek w grafie → próba → numer kopii.
- Deduplikować po rzeczywistym węźle. Brak/odrzucenie wyniku musi zachować istniejącą
  etykietę, nie podszywać się pod poprawny wynik innej kopii.

**Pliki.** `commands/run.rs`; `workflow/unroll.rs` tylko gdy RED wykaże brak tożsamości,
której obecny format nie dostarcza — nie zmieniać formatu identyfikatorów profilaktycznie.

**Test / RED.** `loop_copies_keep_each_result`: trzy kopie oddają różne markery, judge pass
w pierwszej z trzech dopuszczonych prób; synteza czyta pliki wskazane przez prawdziwy RunSpec.
Musi otrzymać trzy wyniki. Baseline wybiera jeden.

**Akceptacja.** Dwie próby i własna historia każdej kopii; wcześniejsze wyniki wszystkich
kopii u judge; odwrotna kolejność zakończenia; dwie rozłączne pętle; jedna kopia z błędem
i CarryOn; brak regresji zwykłego wielorodzicowego handoffu bez pętli.

## WF-06 — Wynik non-git przeżywa koniec i awarię biegu

**Cel.** Nie usuwać jedynej kopii zmienionych plików człowieka.

**Kontrakt implementacji.**

- Porównać końcowe drzewo non-git z manifestem WF-01. Zmiana bajtów, typu, linka lub trybu
  oznacza wynik do zachowania; błąd porównania lub brak manifestu oznacza niepewność
  i także zachowanie. Sam mtime nie wystarcza.
- Sukces, błąd, Stop i recovery zachowują zmienioną/niepewną kopię. Nie kopiować jej
  automatycznie do projektu. Nieukończone wejście WF-03 zachować jako diagnostyczne,
  ale nigdy opisywać jako ukończony wynik.
- Stan i ścieżka są odtwarzalne z plików biegu. Historia ma działające `Open result folder`.
  Niezmienioną własną kopię można usunąć po dowodzie śmierci procesów.
- Z-46 omija jedyny zachowany wynik niezależnie od wieku. Jego usunięcie jest osobną
  świadomą operacją z dokładną listą ścieżek, nie skutkiem zwykłej retencji.

**Pliki.** `commands/{isolate,run,reconcile,history}.rs`, istniejące typy i komponent
rezultatu w `src/sections/run/`, istniejący handler otwierania folderu; adapter retencji
Z-46 wyłącznie po uzgodnieniu z jego właścicielem, bez nowej polityki retencji.

**Test / RED.** `non_git_results_survive_run_completion`: agent dodaje, modyfikuje i usuwa
plik w kopii zwykłego folderu; po zakończeniu wynik jest odczytywalny. Baseline kasuje kopię.
Frontend: `src/sections/run/non-git-result-can-be-opened.test.tsx`.

**Akceptacja.** Wszystkie cztery rodzaje końca, brak/uszkodzony manifest, niezmieniona kopia,
retencja i jawne usunięcie, produkcyjny odczyt historii i kliknięcie otwierające dokładnie
zachowaną ścieżkę. Oryginalny folder pozostaje nietknięty.

## WF-07 — Start Leada ma właściwy adres i prawdziwe potwierdzenie

**Cel.** Rozmowa z projektu A nie uruchamia B po zmianie karty. Narzędzie potwierdza
przyjęty bieg, nie sam zamiar emisji `/run`.

**Kontrakt implementacji.**

- Most tworzy konkretny `StartRequest`: request ID, tożsamość rozmowy, kanoniczny workspace,
  wybrany plik workflow, rewizja i zadanie. Rozwiązuje nazwę raz w projekcie rozmowy.
- Zachować obecny transport przez okno, ale wysyłać ustrukturyzowane żądanie. Front zakłada
  kanał i przekazuje request ID do backendu. Nie rozwiązuje ponownie nazwy, `/run`, folderu
  ani `activeWorkspace`. Zwykła sugestia w prozie pozostaje sugestią bez autostartu.
- Backend konsumuje oczekujący request najwyżej raz i używa istniejącej drogi
  `begin_run` → `run_workflow_in_project`. Pending request nie jest drugim schedulerem.
  Duplikat zwraca ten sam wynik/stan żądania, nigdy drugi bieg.
- Request ID zawiera nonce instancji aplikacji. Gwarancja powtórzeń dotyczy tej instancji;
  po restarcie stare ID jest odmawiane, nie odtwarzane jako nowe żądanie. Korelację requestu
  i rozmowy zapisać przy przyjęciu w run.json, żeby historia pozwalała znaleźć bieg po
  utraconej odpowiedzi. Recovery odtwarza istniejący bieg, nie ponawia MCP startu.
- Dodać wąskie potwierdzenie pomiędzy `prepare_planned_run` i `run_planned_graph`, po
  zajęciu claimu i trwałym początkowym `run.json`: `Started { request_id, run: RunRef }`.
  To „przyjęto do wykonania”, nie „ukończono” ani „wszystkie procesy już ruszyły”.
- `start_workflow` czeka na to potwierdzenie lub konkretną odmowę. Nie czeka na koniec grafu.
  Utrata okna/kanału przed przekazaniem requestu daje odmowę, nie `asked: true`.
- Transport przed odebraniem przez backend ma limit 30 s. Po rozpoczęciu prestartu
  obowiązuje istniejący lifecycle przygotowania; utrata odpowiedzi nie anuluje ukradkiem
  przyjętego biegu. Jego uchwyt pozostaje własnością AppState i jest dostępny przez status.
  Claim oraz wygaśnięcie są atomowym wyborem Pending→Claimed albo Pending→Expired.
  Późny frontend po zgłoszonym timeout dostaje Expired i nie może uruchomić procesu.
- Rewizja, walidacja, budżet, rollback, trigger claim i Stop przechodzą wspólną drogą.
  Nie uruchamiać nieśledzonego `tokio::spawn` bez właściciela uchwytu/zakończenia.

**Pliki.** `bridge/library.rs`, `commands/{chat,mod,run}.rs`, `ipc.rs`, `engine/line.rs`,
`src/ipc/types.ts`, `src/state/run.ts`, `src/sections/run/{io,auto-start,run-command,launch}.ts`,
`src/sections/workflows/io.ts`; dopuszczalny mały moduł requestów w `commands/`.

**Test / RED.** `lead_start_is_bound_to_its_workspace` przez rzeczywisty most i
`src/sections/run/lead-start-keeps-its-workspace.test.ts`: rozmowa A, identyczne nazwy
workflow w A/B, aktywna karta B; zmiana karty również podczas await. Uruchomić tylko A.

**Akceptacja.** Duplikat ID; odmowa rewizji/claimu/budżetu; brak okna; opóźniony agent
(ack wraca przed końcem); brak ack przed trwałym prestartem; wynik odmowy dochodzi do
Leada i markup historii. Timeout→późny transport nie startuje biegu; restart→replay starego
ID nie startuje drugiego. Odmowa nie zostawia procesu ani claimu.

## WF-08 — UI i most mówią prawdę o wiadomościach

**Cel.** Nie pokazywać możliwości dopowiedzenia do sesji, która nie ma kanału odbiorczego.

**Kontrakt implementacji.**

- Zdolność per `RunRef` i rzeczywisty `node_key` wynika z utworzonego uchwytu/rejestracji
  `voice`, nie ze stanu `running` ani samej nazwy vendora. Próba pętli jest częścią adresu.
- Jeden wynik wysyłki: `AcceptedBySession`, `UnsupportedDuringRun`, `RecipientFinished`,
  `NoSuchStep`, `StaleRun`, `Disconnected`. Pierwszy znaczy przyjęcie do kanału sesji.
- Lista adresatów, ręczne wskazanie kroku i narzędzie mostu używają tej samej kontroli.
  Nie przekierowywać nieobsługiwanego `@step` do Leada. Pokaż konkretną odmowę.
- Opis możliwości Leada z Z-50 wynika z efektywnej konfiguracji tools/policy. Ten sam
  wynik trafia do `Lead::brief` i UI. Deklarowany vendor nie jest dowodem dostępu do Bash/Edit.
- Zachować Codex Lead przez App Server. Nie dopisywać pozornej obsługi stdin do `codex exec`
  i nie wymieniać w tym zadaniu jego protokołu; brak direct message ma być uczciwie widoczny.

**Pliki.** `engine/drivers/{mod,claude,codex}.rs` w granicach istniejących możliwości,
`commands/{chat,mod,run}.rs`, `ipc.rs`, `src/state/run.ts`,
`src/sections/run/{index.tsx,addressee.ts,entry/}` i istniejące typy IPC.

**Test / RED.** `step_message_capability_matches_the_session` oraz
`src/sections/run/message-capability-is-honest.test.tsx`: działający uchwyt bez `voice`
nie może udawać zakończonego ani przyjąć tekstu. Uchwyt z `voice` dostaje dokładny tekst.

**Akceptacja.** Opóźniona wiadomość nie trafia do następnej próby/nowego biegu; capability
znika po zakończeniu; read-only/editable Lead ma zgodny opis; odmowa pojawia się na
rzeczywistej ścieżce Entry. Z-36/Z-40 nie zostają zastąpione drugą kolejką tur.

## WF-09 — Lead odczytuje konkretny bieg

**Cel.** Pytanie „co się dzieje?” ma odpowiedź z runtime, nie z domysłu modelu.

**Kontrakt implementacji.** Dodać `get_run_status({ run_id? })`. Workspace wiąże rozmowa.
Brak ID oznacza aktywny bieg tego workspace; gdy go nie ma, `NoActiveRun`, nie najnowszy
historyczny. Jawne ID pozwala czytać historię, ale nie deklaruje prawa do sterowania nią.
Odpowiedź: RunRef, chwila odczytu, workflow/rewizja, status, ograniczona lista kroków z ID,
nazwami i stanem, oczekiwane pytanie, zwięzły błąd, odnośniki do przekazań. Używać RunControl
i obecnego czytnika historii, stronicowania Z-49; bez nowej bazy i pełnych surowych logów.
Kolejna tura Leada otrzymuje krótki adres biegu i informację o narzędziu statusu, nie stałą
kopię całego `run.json`. Wyświetlany skrót i wynik narzędzia pochodzą z tego samego odczytu.

**Pliki.** `bridge/{verbs,library}.rs`, `commands/{history,chat,mod}.rs`, `ipc.rs`;
opcjonalny mały `commands/run_status.rs`, istniejące mapowanie linii dla UI.

**Test / RED.** `lead_reads_the_addressed_run`: rzeczywisty host mostu, dwa workspace'y
i dwa kolejne biegi A1/A2. Wyszukanie A1 po starcie A2 nadal zwraca A1.
Frontend: `src/sections/run/lead-status-reaches-the-stream.test.tsx`.

**Akceptacja.** Brak aktywnego biegu, krok pracujący, błąd, checkpoint, ukończenie,
nieczytelny artefakt i odtworzenie historii po usunięciu testowego indeksu SQLite.
Próba wskazania obcego workspace jest odmową. Status nie rozszerza uprawnień.

## WF-10 — Lead steruje biegiem bez pomyłki adresata

**Cel.** Rozmowa podczas pracy daje istniejące operacje bez edycji żywego grafu.

**Kontrakt implementacji.**

- Każda mutacja wymaga dokładnego `run_id`; host wybiera właściwy uchwyt w workspace
  rozmowy i trzyma ten klon do końca. Po await nie wyszukuje ponownie „aktualnego”.
- `stop_run`: zwykła ścieżka Stop, wynik `Stopped` dopiero po dowodzie śmierci grup.
  `StillAlive` jest problemem, nie sukcesem. Wykorzystać Z-39, nie drugie zabijanie procesu.
- Samo `confirmed:true` napisane przez model nie jest zgodą człowieka. Wiązać zgodę
  z prawdziwą odpowiedzią przez istniejące `ask_person`, konkretnym RunRef i operacją;
  host zużywa ją najwyżej raz. Bez tej odpowiedzi most prosi człowieka, nie wykonuje Stop.
  Rozszerzyć `Waiting::park/answer` i `Threads::answer_in` o backendowe `question_id`
  oraz powiązanie operacji. Treść potwierdzenia i przyciski generuje Rust, nie model.
  Jednorazowy token zgody wiąże rozmowę, RunRef, operację i pytanie; wygasa z rozmową,
  zakończeniem/zastąpieniem biegu lub odrzuceniem. Nie wymaga przeżycia restartu.
- `continue_run({run_id, checkpoint_id, answer})`: ID obejmuje generację konkretnego
  zaparkowanego pytania. Odpowiedź musi pochodzić od człowieka i być dopuszczona przez
  istniejącą semantykę pytania. Lista opcji jest podpowiedzią, nie zamkniętym enumem:
  zachować tekst własnymi słowami oraz pytania bez opcji. Spóźniona/powtórzona odpowiedź
  nie zwalnia następnego checkpointu. Model nie klasyfikuje zgody człowieka na nowo.
  Host bierze treść z zapisanej odpowiedzi człowieka, nie ufa kopii `answer` przepisanej
  przez model. IPC i most zużywają tę samą zgodę; jedno kliknięcie nie wykonuje dwóch akcji.
- `rerun_step({source_run_id, step_id})`: dodać `again_from_run` obok obecnego `rerun::again`
  wybierającego najnowszy bieg. Współdzielić implementację; wejścia biorą się z podanego
  źródła, nowy RunRef wraca przez WF-07, źródłowy `run.json` pozostaje niezmienny.
  `step_id` oznacza cały kafelek, zgodnie z `Part::Just`: uruchamia wszystkie jego
  skonfigurowane kopie, nie jedną wskazaną próbę. Każda kopia ma własne źródłowe wejście
  per work key. UI i wynik narzędzia podają liczbę kopii; pojedynczy node key jest odmową
  z wyjaśnieniem, nie cichym wyborem całego kafelka. Rerun jednej kopii to osobny przyszły zakres.
  Zachować obecną semantykę aktualnego workflow, lecz zmianę względem źródła pokazać
  przed startem i wymagać świadomej akceptacji; nie udawać odtworzenia starej konfiguracji.
- `send_to_step({run_id, node_key, text})` konsumuje dokładnie API/wynik WF-08.
  Nie tworzy nowej sesji ani nie kieruje wiadomości do innego odbiorcy po odmowie.
- Jedno mapowanie odmów zasila MCP i strumień: nieznany/stary bieg, stare pytanie,
  brak obsługi wiadomości, brak źródłowego wyniku, zmieniony workflow, nieudowodniony Stop.

**Pliki.** `bridge/{verbs,library}.rs`, `commands/{chat,run,rerun,mod}.rs`, `ipc.rs`,
obecny rejestr pytań i kanały UI w `src/sections/run/`.

**Test / RED.** `lead_controls_only_the_addressed_run`,
`src/sections/run/lead-control-results-are-visible.test.tsx`.
Wywołać rzeczywiste narzędzia mostu, nie tylko sprawdzać ich nazwy w `tools/list`.

**Akceptacja.** Opóźniony Stop/Continue/Send A1 nie dotyka A2; Stop A nie przerywa B;
sfałszowane `confirmed:true` bez odpowiedzi nie wystarcza; odpowiedź checkpointu 1
nie puszcza 2; retry starszego biegu czyta starsze wejścia mimo istnienia nowszego;
retry dwóch kopii zachowuje ich odmienne wejścia; pytanie bez opcji przyjmuje oryginalny
tekst człowieka; oryginał nie zmienia bajtów; rezultaty operacji w historii i na ekranie.

## WF-11 — Opcjonalne wiadomości pomiędzy agentami

**Cel.** Równoległe kroki różnych vendorów mogą wymieniać informacje, bez udawania
natychmiastowego wstrzyknięcia tekstu do każdej sesji.

**Kontrakt implementacji.**

- Dodać wyłączoną domyślnie konfigurację komunikacji do definicji agenta i nadpisania
  w kroku, zgodnie z obecnym mechanizmem dziedziczenia. Brak pola zachowuje dotychczasowe
  `Role::Step` bez tych narzędzi. Nie jest to nowy kafelek ani własny system subagentów.
- Istniejący host MCP udostępnia `list_peers`, `send_message` i `read_messages`.
  Host wiąże nadawcę z RunRef i konkretnym node key; model nie podaje `from` ani obcego run ID.
  `list_peers` zwraca adresy konkretnych kopii/prób, nie samą nazwę kafelka.
- Adresaci: uprawnione kroki tego samego zamrożonego grafu i zakresu kontekstu, które
  mogą jeszcze odczytać wiadomość. Zakończony/pominięty odbiorca daje odmowę. Nie wysyłać
  automatycznie do „następnej kopii” lub przyszłej próby po zakończeniu wskazanej.
- Wpis: ID klienta do deduplikacji, monotoniczny sequence nadany przez host, nadawca,
  adresat, tekst i czas. Jeden właściciel publikacji per run, istniejący bezpieczny writer
  plików; nie drugie połączenie zapisujące SQLite. Plik jest czytany przez most i historię.
  Klucz deduplikacji to `(RunRef, sender_node_key, client_id)`; porównanie obejmuje adresata
  i treść. Dwóch nadawców może użyć tego samego client_id bez kolizji.
- `send_message` zwraca `Stored` dopiero po utrwaleniu. Powtórzenie ID i tych samych
  bajtów zwraca ten sam sequence; inne bajty z tym ID odmawiają. Nie deklarować exactly-once
  wykonania instrukcji przez model. `read_messages(after_sequence)` nie usuwa wpisów.
  Zwraca wyłącznie inbox tożsamości wywołującego związanej przez host; model nie podaje
  dowolnego adresata do odczytu. Wspólny scope nie daje prawa do czytania cudzej skrzynki.
- Limity początkowe: 16 KiB UTF-8 na wiadomość, 1000 wpisów / 8 MiB treści na bieg,
  do 100 wpisów na odczyt, wyraźna informacja o dalszej stronie. Przekroczenie jest odmową,
  nie cichym obcięciem. Limity są stałymi w jednym module, nie trzema kopiami polityki.
- Odczyt jest natychmiastowy, bez nieskończonego `wait` zajmującego slot. Agent odpytuje
  podczas pracy; komunikacja nie zwalnia zależności i nie oznacza zakończenia kroku.
- Retry/new run nie dziedziczy skrzynki. Historyczny odczyt pozostaje możliwy z plików.
  UI pokazuje fakt i autorów, treść po rozwinięciu. Nie dodawać tekstu wiadomości do argv,
  debug logów i publicznego eksportu poza obecną polityką prywatności.

**Pliki.** `bridge/{verbs,library}.rs`, mały `bridge/messages.rs`,
`commands/{run,history}.rs`, `workflow/mod.rs`, typ/resolve agenta w `library/agents.rs`,
istniejące formularze agenta/kroku oraz mapa linii i historia w `src/sections/run/`.

**Test / RED.** `agents_exchange_scoped_messages`: dwa dublerowane vendory przez
rzeczywisty host mostu i produkcyjne sesje, równoczesna wysyłka, odczyt przy drugim kroku.
Frontend: `src/sections/run/stored-message-is-not-read-message.test.tsx`.

**Akceptacja.** Brak zgubionych wpisów, deduplikacja, limity/strony, restart odczytu,
odmowa innego biegu/kopii/zakresu i zakończonego odbiorcy. Wiadomość nie uruchamia
zablokowanego kroku. `Stored` nie zamienia się w „Read” bez dowodu. Workflow bez włączenia
komunikacji ma dotychczasowe narzędzia i zachowanie. Dwaj nadawcy z identycznym client_id
nie kolidują; próba odczytu cudzej skrzynki niczego nie ujawnia.

## WF-12 — Jeden kontrakt instrukcji repo dla Leada i kroków

**Cel.** To samo repo ma jawne, odtwarzalne reguły niezależnie od miejsca użycia i vendora.

**Kontrakt implementacji.**

- Wspólny resolver `InstructionSnapshot` używany przez Lead i workflow. Źródło zawiera
  ścieżkę względną, rodzaj, zakres i digest; treść źródłowa jest prywatnym materiałem
  wejściowym, nie debug logiem. Historia pokazuje źródła/rozmiary/stan dostarczenia.
- Projekt ma `Use project instructions`, krok i Lead dziedziczą ustawienie z możliwością
  jawnego nadpisania. Stare pliki bez pola zachowują dotychczasowe wyłączenie. Nowy projekt
  pokazuje wykryte źródła i pozwala włączyć je świadomie; otwarcie folderu nic nie wykonuje.
- Wykrywać `AGENTS.md`, `CLAUDE.md` i `.claude/rules/**/*.md` w wybranym repo. Osobno wybierany
  `CLAUDE.local.md`, bo może nieść prywatne ustalenia. Bez automatycznego dołączania globalnych
  instrukcji z katalogu domowego.
- Zachować zakres katalogu/poddrzewa oraz `paths` reguły. Głębszy zakres uszczegóławia
  rodzica; przy tym samym zakresie AGENTS ma pierwszeństwo przed dodatkowymi instrukcjami
  Claude. Reguły w tym samym zakresie mają stabilną kolejność ścieżek. Nie udawać
  mechanicznego rozwiązania sprzeczności dwóch zdań naturalnego języka.
- Przekazać treść wraz z oznaczeniami zakresów, nie skleić `web/**` i `api/**` jako dwóch
  globalnych nakazów. Nie twierdzić „dostarczono”, jeśli model dostał tylko nazwę pliku.
- Jawne includy obsłużyć tylko w wybranych źródłach i wewnątrz projektu: wykrywać cykle,
  normalizować ścieżki, odmówić wyjścia przez symlink/`..`. Nie podążać za dowolnymi linkami
  Markdown lub URL. Wspierany syntax include opisać i pokryć fixture; inny pozostaje tekstem.
- Początkowe limity: 256 plików, 64 KiB na plik, 512 KiB całego pakietu. Przekroczenie
  lub nieczytelny wybrany plik daje odmowę z nazwą źródła, nie ciche obcięcie kontekstu.
- Bieg używa jednej migawki we wszystkich próbach i kopiach; Lead odświeża pomiędzy turami.
  Odpowiadając o trwającym biegu, rozróżnia bieżące repo od migawki używanej przez ten bieg.
- Instrukcje nie zmieniają permissions/env/hooków/MCP/sandboxu/vendorOptions. Ich tekst
  trafia stdin. Nie uruchamiać zastanych hooków jako skutku importu instrukcji.
- Telemetria Z-47 odróżnia „Loadout supplied” od „vendor reported loading”. Natywnego
  autoładowania przez CLI nie ukrywać i nie przypisywać mu niepotwierdzonej kontroli.

**Pliki.** `inherit/{mod,scan,wire}.rs`, nowy `inherit/instructions.rs`,
`workflow/mod.rs`, `commands/{run,chat}.rs`, `evidence.rs`, wąskie szwy driverów;
ustawienia projektu/Leada/kroku i istniejący panel kontekstu w `src/sections/`.

**Test / RED.** `repository_instructions_reach_lead_and_steps`: root AGENTS, dodatkowy
CLAUDE, odmienne zakresy web/api; asercja na rzeczywistym RunSpec/stdin obu driverów,
nie tylko na wyniku skanera. `src/sections/run/project-instructions-are-explicit.test.tsx`.

**Akceptacja.** Cztery powierzchnie Claude/Codex × Lead/krok, zachowane zakresy, niezmienne
bajty w biegu mimo edycji repo; kolejna tura widzi zmianę. Fixture hooka/permissions nie
wykonuje ani nie podnosi uprawnień. Include/cykl/rozmiar/odczyt kończą się widoczną odmową;
stare pliki pozostają kompatybilne. Nie obiecywać, że model zawsze zastosuje miękką instrukcję.

## WF-13 — Skill jest kompletnym bundle, nie samym SKILL.md

**Cel.** Wybrany skill ma działające zasoby względne i jednakowe pochodzenie w każdej sesji.

**Kontrakt implementacji.**

- Jeden resolver `ResolvedSkill { name, source, bundle }` dla Borrow, biblioteki, importu,
  Leada i kroków. Domyślna kolejność projekt → globalne półki użytkownika → biblioteka;
  przy tej samej nazwie i różnych bajtach UI wymaga jawnego wyboru źródła zamiast cichej
  zmiany. Wybór wiąże źródło, nie tylko nazwę.
- Bundle obejmuje cały katalog skilla: SKILL.md, scripts, references, assets i inne
  zasoby. Nie próbować parsować prozy, aby zgadywać, które pliki „są potrzebne”.
  Zachować strukturę względną i prawa helperów. Brak zadeklarowanego zasobu/nieczytelność
  lub link wychodzący poza bundle oznacza niekompletne dostarczenie i odmowę.
- Zamrozić bundle dla biegu/tury. Materializacja niczego nie uruchamia, nie nadpisuje
  `.agents/skills` ani `.claude/skills` w projekcie człowieka. Wykorzystać własne katalogi
  sesji i istniejący publisher z walidacją pochodzenia przy cleanup.
- Granica drivera przyjmuje ten sam rozwiązany bundle. Adapter wykorzystuje rzeczywisty
  natywny mechanizm danego vendora; nie budować własnego interpretera skilli.
- Pierwszy krok implementacji adapterów: lokalny, ograniczony odczyt `--help`/schematu
  App Servera i istniejącego kodu dla wersji na maszynie. Spisać potwierdzony sposób
  przekazania ścieżki/bundle dla czterech sesji. Nie zgadywać flag i nie wnioskować,
  że brak historycznego `--plugin-dir` wyklucza każdą drogę Codexa.
- Rozdzielić `Delivered` (kompletne osiągalne zasoby) od `Unavailable(reason)` i od
  „model faktycznie użył”. Niewspierana sesja daje informację przed startem; rozmowa może
  jawnie działać bez skilla, ale nie liczy się to jako dowieziona zgodność.
- Pełne ukończenie tej karty wymaga działających czterech sesji. Jeśli dostępny natywny
  protokół którejś nie potrafi obsłużyć, raportuje się blokadę i konkretny brak API;
  samo ostrzeżenie nie zamyka zadania. Nie rozszerzać samodzielnie D6 o własny skill runner.

**Pliki.** `skills/{mod,place,ingest}.rs`, `inherit/{scan,rewrite,wire}.rs`,
`import/{adapters,apply}.rs` wyłącznie współdzielenie bundle,
`commands/{skills,chat,run}.rs`, drivery, listy/wybór skilli i panel kontekstu.

**Test / RED.** `complete_skill_bundle_reaches_every_supported_session`: SKILL.md
odsyła do losowego markera w references i helpera scripts; Borrow na baseline gubi zasoby.
Frontend: `src/sections/skills/skill-source-and-delivery-are-visible.test.tsx`.

**Akceptacja.** Cztery sesje, identyczny wybór źródła, osiągalność zasobów, brak modyfikacji
repo gospodarza, zmiana źródła po starcie, kolizja/link/brak zasobu. Aktualizować stare testy
przypinające kopiowanie tylko SKILL.md do nowego kontraktu, nie osłabiać wyroczni harnessu.
Po integracji mały żywy smoke odczytu markera przez oba vendory, z SHA i wersjami CLI;
helper wykonuje się dopiero na właściwe polecenie i w granicach polityki.

## WF-14 — Jawne wejścia i przygotowanie świeżej kopii

**Cel.** Repo ma sposób zadeklarowania potrzebnych plików i zależności, bez ukrytej instalacji.

**Kontrakt implementacji.**

- Do konfiguracji wejścia workflow dodać listę dodatkowych ścieżek względnych/globów
  używaną przy WF-01. Jeden obraz źródła dla wszystkich kopii; nie odmienne filtry
  per gałąź, które unieważniają wspólną bazę fan-in.
- Domyślna lista pusta. Dodatkowe wybrane untracked pliki trafiają do snapshotu razem
  z tracked WIP. Podgląd pokazuje konkretne ścieżki/rozmiar i zaznacza ignored pliki.
  `.env`/ignored nie dołącza się automatycznie; jawny wybór wymaga ostrzeżenia o prywatnych
  danych. Nie umieszczać zawartości w logu ani eksporcie raportu.
- Wzorzec bez dopasowań, brak/nieczytelność wskazanej ścieżki i wyjście poza projekt
  blokują Start przed pierwszym procesem. Limity rozmiaru i liczby odziedziczyć z istniejącej
  polityki kopiowania, podać widoczną przyczynę zamiast po cichu pomijać.
- Zależności przygotowuje jawny istniejący Check w fresh-copy, a konsumenci używają
  same-copy. UI może utworzyć taki fragment z komendy człowieka oraz rzeczywistego
  sprawdzenia środowiska z licznikiem przejść. Nie generować `echo "1 passed"`.
- Nie dodawać `prepare_enabled` do schedulera. Usunięcie kroku z grafu usuwa przygotowanie.
  Dwa osobne świeże drzewa wymagają własnego przygotowania; wykluczone `node_modules`
  i `target` nie są obiecane jako wynik fan-in. Po fan-in zależności sprawdza/przygotowuje
  kolejny jawny Check, jeśli są potrzebne.
- Git bez pierwszego commita: konkretna odmowa, bez automatycznego init/commit.

**Pliki.** `workflow/{mod,check}.rs`, `commands/{run,isolate,input_snapshot}.rs`,
formularz workflow, `src/sections/workflows/step-panel/{where-it-works,check-panel}.tsx`,
typy i serializacja workflow.

**Test / RED.** `fresh_copy_carries_selected_inputs`: tracked WIP + wybrany untracked
plik + niewybrany `.env`; agent czyta dwa pierwsze, nie trzeci. Dodatkowo
`preparation_precedes_consumers_in_the_same_copy`: komenda tworzy środowisko, prawdziwy
test sprawdza jego marker, dopiero potem rusza konsument. Front:
`src/sections/workflows/step-panel/fresh-copy-inputs-round-trip.test.tsx`.

**Akceptacja.** Zapis/odczyt ustawień, podgląd, brak dopasowań, link zewnętrzny, brak dostępu,
porażka/Stop/timeout przygotowania, usunięcie Check z grafu, czyste repo i monorepo.
Nie deklarować automatycznego wsparcia managera, którego w tym zadaniu nie wykrywano.

## WF-15 — Historyczny pomiar nie zmienia definicji po edycji zestawu

**Cel.** Wynik z wczoraj jest oceną wczorajszego wejścia i kryteriów, nie dzisiejszego formularza.

**Kontrakt implementacji.**

- Przy planowaniu pomiaru utrwalić `MeasurementDefinition`: wersja, set ID i rewizja,
  aktywne przypadki z kryteriami, warianty po rozwiązaniu konfiguracji, zadeklarowane
  powtórzenia, identyfikatory wejść, źródeł instrukcji i bundle skilli.
- Zapisać definicję w istniejącym wygenerowanym planie i zachować ją w `workflow_snapshot`
  biegu; większe materiały źródłowe są odnośnikami do prywatnych snapshotów w jego katalogu.
  Czytelnik `score_one` korzysta wyłącznie z definicji historycznej i faktów tego biegu.
  Nie tworzyć drugiej mutowalnej tabeli wyników.
- Rejestrować faktyczne rozstrzygnięcia agenta/modelu/effortu/opcji oraz dostępne wersje CLI
  i SHA Loadouta. Nie zapisywać wartości sekretów, złożonego promptu ani środowiska procesu.
  Sekret opisuje się jako wymaganą zależność, nie materiał do reprodukcji.
- Materiały skopiować do własności pomiaru; usunięcie/retencja źródłowego biegu nie ma
  kasować wejścia nowego. Błąd integralności oznacza pomiar nieporównywalny.
- Stary bieg bez definicji: `Criteria snapshot unavailable` i brak przeliczania jego
  wyniku nowymi kryteriami. Czytelna stara historia zostaje; żadnego dopisywania zgadywanej
  definicji do starych run.json.
- Porównywalność wyznacza fingerprint wejścia + aktywnych kryteriów i warunków pomiaru.
  Konfiguracja wariantu jest zmienną badaną, więc jej różnica sama nie unieważnia porównania.
  Zmiana przypadku/kryterium tworzy nową serię; nie przelicza poprzedniej.

**Pliki.** `lab/{mod,plan,results}.rs`, `commands/lab.rs`, punkt zapisu snapshotu
w `commands/run.rs`, istniejący odczyt historii i model w `src/sections/lab/`.

**Test / RED.** `lab_history_uses_its_original_definition`: zmierzyć fixture, zmienić
expect/command/proof i agenta w aktualnym zestawie, odczytać historię przez `read_board_inner`.
Pierwotny wynik i kryteria pozostają. Baseline sądzi przez `open.set`.
Front: `src/sections/lab/old-results-keep-their-criteria.test.tsx`.

**Akceptacja.** Edycja/usunięcie obecnego zestawu, zmiana biblioteki, retencja źródła,
legacy bez snapshotu, nieczytelny snapshot, odbudowa indeksu. Złożony prompt ani sekret
nie trafia do nowego artefaktu. Model/effort/opcje są odczytem faktu, nie aktualnej biblioteki.

## WF-16 — Ogólne wejścia i zakresy kontekstu wewnątrz jednego biegu

**Cel.** Umożliwić wiele niezależnych prób w jednym grafie bez przecieku ich kontekstu,
drugiego schedulera i specjalnego `if lab` w silniku.

**Dlaczego osobna karta.** Obecny `index_of_what_came_before` udostępnia całe wspólne
`handoffs/` i `attachments/`; samo prefixowanie nazw nie daje izolacji. `Check` nie dostaje
wejścia poprzednika, bo CommandDriver używa `StdinPlan::Null`.

**Kontrakt implementacji.**

- Do przygotowanego wykonania, nie algorytmu schedulera, dodać ogólne `RunInputs`:
  mapę workspace seedów, zakresów kontekstu, typowanych wejść Check i politykę publikowania
  skutków. Zwykły bieg bez nadpisań dostaje dotychczasowe domyślne zachowanie.
- `WorkspaceSeed` wskazuje zweryfikowany snapshot WF-01 i zarządzaną własność, nie dowolną
  ścieżkę Pick. Kilka kroków Project może wskazywać jeden zarządzany workspace; walidator
  widzi ich alias i odmawia kolidującym równoległym zapisom przed startem.
- `ContextScope` wskazuje źródło instrukcji/task/memory i własne katalogi przekazań
  oraz załączników. Publikacja, indeks w promptcie, extra_dirs, recovery
  i handoffs_from respektują ten sam zakres. Nie udostępniać katalogu nadrzędnego wszystkich
  zakresów tylko dla wygody drivera. Brak scope w dawnym biegu znaczy jeden domyślny scope.
  Późniejszy WF-11 konsumuje gotowy scope; ta karta nie implementuje skrzynki wiadomości.
- Kluczem kanału żywego kroku jest rzeczywisty node key + RunRef, nie nazwa „Implement”.
  Nazwy mogą być takie same w różnych zakresach; UI pokazuje etykietę komórki osobno.
- `CheckInput` jest opcjonalnym typowanym bindingiem: lista wskazanych rezultatów,
  ich status/przyczyna zakończenia i odnośnik do zamrożonych plików. Host rozwiązuje dane
  po zależnościach i podaje ograniczony JSON stdin. Bez interpolacji tekstu agenta do shell.
  Brak bindingu zachowuje EOF i obecne działanie Check; wejścia przekraczające limit odmawiają.
- Ustrukturyzować potrzebne przyczyny końca: wynik pracy/checka, odmowa prestartu/awaria
  infrastruktury, anulowanie, pominięcie przez zależność lub wybór gałęzi. Rozszerzyć obecne
  metadane, nie dublować StepState. Nie klasyfikować przez parsowanie angielskiego error.
- `RunEffects` pozwala wyłączyć publikację refleksji/pamięci do realnego projektu przy
  pozostawieniu normalnych rezultatów i historii. Izolowana próba nie zmienia warunków
  kolejnej przez globalną pamięć. Sterowniki i supervisor nadal wspólne.
- Ta sama ogólna konfiguracja wykonania może zabronić zewnętrznych dopowiedzeń do kroków.
  Odmowa jest sprawdzana we wspólnej drodze przyjęcia wiadomości, także przez MCP Leada,
  nie tylko przez schowanie pola w UI. Nie zmienia to wewnętrznych przekazań grafu.
- To jest izolacja routingu i deklarowanych katalogów. Jeżeli efektywna polityka procesu
  pozwala czytać cały dysk, nie nazywać jej ochroną przed celowym odczytem obcego scope.
  Wymagania ochrony w pomiarze egzekwuje WF-18, zamiast udawać, że sam inny cwd wystarcza.

**Pliki.** `commands/{mod,run,isolate,reconcile}.rs`, mały `commands/run_inputs.rs`,
`engine/drivers/{mod,command}.rs` i `engine/supervisor.rs` tylko transport stdin/polityki,
`workflow/check.rs` dla rzeczywistych aliasów, odczyt historii.

**Test / RED.** `run_context_scopes_do_not_share_inputs`: dwa zakresy z tymi samymi
nazwami kroków, odmiennymi task/AGENTS/markerami, dwie gałęzie w każdym; produkcyjne
prompty, allowed dirs i publikacja nie mieszają plików. Dodatkowo
`check_reads_only_its_bound_input`: Check czyta JSON stdin i weryfikuje wskazany marker.

**Akceptacja.** Zwykły bieg bez bindings bez zmian; wspólne sloty i Stop; brak aliasowej
kolizji Project; scope zachowany po recovery/ponowieniu; brak publikacji pamięci do hosta;
tekst `$(...)`/cudzysłowy w wyniku pozostają danymi stdin, nie komendą. Test nie udaje
systemowej odmowy dostępu, jeśli weryfikuje tylko RunSpec.

## WF-17 — Lab kompiluje cały workflow, nie zastępczego pojedynczego agenta

**Cel.** Porównać dwa harnessy różniące się grafem, pętlą, modelem lub instrukcjami.

**Kontrakt danych.**

- Dodać `Subject::Workflow { id }`. Wariant ma jawne źródło workflow i opcjonalne
  nadpisania konkretnych kroków. Nie używać pola `agent` do przechowywania ścieżki grafu.
- Tu rzeczywiście zmienia się schemat zestawu: czytać format 1 do modelu w pamięci,
  format 2 pisać przy świadomym zapisie; nie przepisywać wszystkich plików w tle.
  Zachować nieznane pola, odmawiać nowszego nieobsługiwanego formatu.
- Przypadek ma zadanie, wejście z WF-14, powtórzenia (domyślnie 1, zakres 1–20) i niezależne
  kryteria. Wariant workflow wskazuje `outputStep`: jeden Agent albo Check, copies=1,
  poza pętlą, bez wewnętrznych następników; każdy inny krok musi mieć drogę do niego.
  Wiele wyników wymaga jawnej syntezy/joinu autora workflow, nie „ostatnio zakończonego”.

**Kontrakt wykonania.**

- `lab::plan` rozwija case × variant × repeat do jednego zwykłego grafu. Zachowuje
  krawędzie, warunki, limity pętli, copies, failure policies i rzeczywiste typy kroków.
  Nie zastępuje grafu jednym agentem odczytującym jego opis.
- Prefixować tożsamości i wszystkie typowane referencje, także warunki przechowywane
  obecnie w `WorkflowFile.extra.linkConditions`. Nie podmieniać stringów globalnym replace.
  Nie modyfikować dowolnych vendorOptions ani prozy. Utrwalić jawny `CellBinding`:
  case/variant/repeat, mapę oryginalnych node IDs, output, grader, seed i context scope.
- Wszystkie komórki danego przypadku mają ten sam seed i materiał kontekstowy, niezależne
  katalogi i skrzynki. Project wskazuje zarządzaną kopię komórki, FreshCopy własne drzewo
  z jej seed, SameCopy normalnie dziedziczy/składa. Nie zamieniać Project na cudze Pick.
- `{{task}}`, pamięć i instrukcje pochodzą z case scope, nie jednego globalnego deps.project.
  Konfigurację agentów/skilli zamrozić z WF-15, nie odczytywać ponownie w połowie pomiaru.
- Po output dołączyć zewnętrzny Check oceny, z odrębnym scope i CheckInput WF-16.
  Graf subjectu nie może adresować jego wyników. Nie zmieniać failure policy subjectu
  wyłącznie po to, żeby Check oceny został uruchomiony.
- Pomiar porównawczy ustawia zakaz zewnętrznych dopowiedzeń z WF-16. Status i Stop
  pozostają dostępne, lecz człowiek ani Lead nie mogą w połowie podpowiedzieć rozwiązania
  jednemu wariantowi. Skrzynka pomiędzy krokami subjectu jest częścią jego zamrożonej
  konfiguracji i pozostaje dozwolona. Próba zewnętrznej wysyłki daje jawną odmowę;
  nie zmienia fingerprintu po fakcie ani nie udaje niezmienionych warunków pomiaru.
- Pierwszy zakres wspiera automatyczne workflow Agent/Check z pętlami i warunkami.
  Checkpoint wymagający człowieka, Serve, zewnętrzny Pick i mutujące zewnętrzne integracje
  dają konkretną odmowę przed Startem pomiaru. Nie autozatwierdzać pytań, nie wisieć
  bez wyjaśnienia w macierzy i nie ryzykować kolizji portów. Normalny Run nadal je wspiera.
- Przed alokacją i rozwijaniem obliczyć rozmiar przez checked arithmetic. Początkowe
  limity pomiaru: 100 komórek, 1000 rozwiniętych węzłów, 5000 krawędzi i 100 zarządzanych
  drzew. Wliczać copies, próby pętli, grader oraz cele fan-in. Przekroczenie/overflow
  daje konkretną odmowę, nie częściowe przygotowanie. Obecne ograniczenia copies/turns
  nie zastępują limitu łącznego. UI pokaże liczby z tego samego kalkulatora.
  Zwykła ścieżka `run_eval_set`/`run_workflow_in_project`, jedna pula miejsc i wspólny budżet.

**Pliki.** `lab/{mod,file,plan}.rs`, `commands/lab.rs`, `commands/run_inputs.rs`,
punkt przekazania planu w `ipc.rs`, model/typy IPC Labu. Bez zmian schedulera o nazwie Lab.

**Test / RED.** `lab_runs_the_whole_workflow`: dwa warianty, każdy dwie równoległe gałęzie,
jedna pętla i synteza. Bariera dowodzi nakładania, znaczniki wyników dowodzą rzeczywistego
wykonania całego grafu, a nie tylko serializacji. Każda komórka widzi własny task i wejście.

**Akceptacja.** Agent/Skill zestawy nadal działają; format 1→odczyt→świadomy zapis 2;
zmiana grafu wariantu zmienia rzeczywiście wykonane kroki; remap warunków; zgodne aliasy
folderów; refusal niewspieranego kształtu widoczny w UI; rezultat wiązany przez CellBinding,
nie split nazwy i nie kolejność kończenia. Zmiana live biblioteki nie zmienia biegu.

## WF-18 — Niezależne kryteria i uczciwa klasyfikacja wyniku

**Cel.** Harness nie ocenia sam siebie. Lab odróżnia porażkę zadania od braku pomiaru.

**Kontrakt wyroczni.**

- Kryteria i kod zewnętrznego Check pochodzą z zaakceptowanego przypadku WF-15, nie
  z agentowego outputu ani pliku zmienionego w badanym repo. Własne checki subjectu są
  częścią badanego harnessu, nie niezależnym kryterium jego skuteczności.
- Materiały wyroczni trzymać poza zapisywalnym drzewem subjectu. Grader dostaje zamrożony
  wynik po zakończeniu jego producentów, read-only oraz JSON z CheckInput. Nie ma
  domyślnego dostępu do wyników innych komórek. Checker i subject nie dzielą katalogu
  importów: narzędzie uruchamiać ze wskazanej zaufanej lokalizacji, nie przez plik podmienialny
  w repo lub PATH/cwd badanego rozwiązania.
- Check orzeka z własnego procesu: exit + rzeczywisty dodatni licznik. `exit 0`,
  zero tests, tekst „passed” z handoffu ani zielone wewnętrzne checki nie wystarczają.
  Jeżeli przypadek bada wyłącznie pola odpowiedzi, nazwać to oceną odpowiedzi, nie testem
  działania aplikacji; dla workflow rozwiązującego kod wymagać zewnętrznego command/proof.
- Dla gwarantowanej oceny samo regex `command/proof` nie wystarcza: badany moduł może
  wypisać `100 passed` i wykonać `os._exit(0)` podczas importu przez test. Niezmienione
  pliki egzaminatora nie chronią jego licznika przed kodem uruchomionym w tym samym procesie.
  Wprowadzić opcjonalny ogólny tryb dowodu Check `ExternalAssessmentV1`; normalny Check
  zachowuje dotychczasowy OutputPattern. To protokół wyniku sterownika, nie nowy etap grafu.
- W `ExternalAssessmentV1` zaufany egzaminator jest osobnym procesem od uruchamianego
  kodu subjectu. Nie importuje ani nie wykonuje tego kodu w swoim interpreterze. Steruje
  ograniczonym procesem badanego programu i sam asertuje jego wynik/protokół/pliki.
  Stdout/stderr badanego programu to dane przechwycone przez egzaminator, nigdy strumień
  dowodu przekazany do parsera Check. Potomkowie nadal podlegają supervisorowi i polityce.
- Egzaminator zwraca dokładnie jeden ograniczony do 64 KiB JSON na własnym stdout:
  `{format:1, status:"completed"|"subject-cannot-load"|"infrastructure-failed",
  passed:<u32>, failed:<u32>, reason:<string>}`. Diagnostyka idzie stderr. Zaufany kod
  egzaminatora zwiększa liczniki po własnych asercjach; nie przepisuje liczb subjectu.
  Sukces wymaga exit 0, completed, passed > 0 i failed = 0. Completed z failed > 0
  oznacza DidNotPass. Subject-cannot-load jest DidNotPass z przyczyną; infrastructure-failed,
  brak/nieprawidłowa odpowiedź lub crash egzaminatora oznacza NotJudged. Żaden z nich
  nie może dać zieleni. To egzaminator rozróżnia np. SyntaxError rozwiązania od braku
  własnej biblioteki testowej; host nie zgaduje tego z angielskiego stderr.
- Arbitralne istniejące `command/proof`, w tym unit test importujący subject do swojego
  procesu, mogą zostać jako pomiar diagnostyczny, ale nie dostają oznaczenia odporności
  na fałszowanie. Pierwsza gwarantowana ścieżka to zaufany test zewnętrznego zachowania
  przez powyższy protokół. UI pokazuje ten zakres przed uruchomieniem.
- Przed i po pomiarze weryfikować tożsamość oraz integralność definicji i wejść wyroczni.
  Zmiana/utrata materiału daje `NotJudged: evaluation data changed`, nigdy wynik agenta.

**Granica ochrony — obowiązkowo jawna.**

- Nie wystarczy usunąć ścieżki z promptu ani ustawić `0444`. Tryb nazywany chronionym
  wymaga sprawdzonej odmowy zapisu do wyroczni i odczytu obcego scope przez każdy proces
  subjectu: agenta, jego shell, Check oraz dopuszczone narzędzia potomne.
- Użyć jednej polityki filesystem procesu w `engine/supervisor.rs`, propagowanej przez
  drivery; żaden vendor adapter nie buduje własnego wrappera/polityki. Dopuszczone zapisy
  to własne drzewa komórki i jawne prywatne katalogi runtime, nie nadrzędny katalog pomiaru.
  Uprawnienia skryptu Check również podlegają tej granicy, nie tylko permission dial agenta.
- Przed dodaniem obietnicy ochrony wykonać lokalny test faktycznego zapisu, podmiany przez
  rename, chmod i procesu potomnego. Brak zdolności platformy/drivera oznacza odmowę
  chronionego pomiaru **przed pierwszym płatnym procesem**. Nie raportować tej karty jako
  ukończonej, jeśli jedyną implementacją jest refusal każdej konfiguracji.
- Minimalny odbiór: obaj vendorzy z Agent + zwykły Check mają działającą egzekucję granicy
  na macOS. Jeżeli wymaga to nowej systemowej usługi, uprawnień aplikacji albo odejścia od
  supervisor policy, zatrzymać kartę z konkretnym blokiem i decyzją właściciela; nie wdrażać
  po cichu nowego kontenera/daemona. Niefenced pomiar może pozostać jawnie diagnostyczny,
  ale nie może udawać certyfikowanego, niezapisywalnego egzaminatora.

**Kontrakt klasyfikacji.**

| Fakty pomiaru | Wynik |
|---|---|
| Prawidłowy output i wszystkie niezależne kryteria przeszły | Passed |
| Wyrocznia wykonała się poprawnie i wykazała niespełnienie zadania | DidNotPass |
| Subject rzeczywiście wykonał pracę, zakończył się błędem zadania i nie dostarczył wymaganego outputu | DidNotPass z przyczyną; nie trzeba uruchamiać grader na nieistniejącym wyniku |
| Stop, zamknięcie aplikacji, niedostępny vendor, błąd przygotowania lub nieudowodnione zatrzymanie | NotJudged, właściwa przyczyna |
| Uszkodzona/zmieniona wyrocznia, brak historycznej definicji, błąd infrastruktury grader | NotJudged, nieważny pomiar |
| Nieaktywna gałąź lub nieudana wcześniejsza próba później naprawionej pętli | Same w sobie nie rozstrzygają wyniku |

Klasyfikować po strukturze przyczyn WF-16, nie samym `failed/skipped` lub tekście error.
Wynik pominiętego grader nie jest automatycznie zerem za zadanie. Pokazywać coverage:
ile przypadków/powtórzeń naprawdę oceniono; `NotJudged` nie zwiększa mianownika skuteczności.
Czas i koszt sumować po rzeczywistych wykonaniach komórki; brak kosztu to unknown/partial,
nie zero. Pojedynczy przebieg nie uzasadnia twierdzenia o statystycznej przewadze.

**Pliki.** `lab/{results,plan}.rs`, `commands/{lab,run_inputs,run}.rs`,
`engine/drivers/{command,mod,claude,codex}.rs` w granicach transportu policy,
`engine/supervisor.rs`, `workflow/mod.rs` i typy Check dla opcjonalnego protokołu,
istniejący odczyt i kuracja wyniku. Polityka w supervisorze, parser dowodu w CommandDriver;
adaptery nie zawierają własnych kopii tych reguł.

**Test / RED.** `lab_judges_results_outside_the_subject`: subject fałszuje własny test
i pisze „100 passed”, ale zostawia wadliwy rezultat; zewnętrzny Check ma dać DidNotPass.
`lab_separates_task_failure_from_missing_measurement`: tabela powyżej przez produkcyjny
bieg/read_board. `lab_oracle_cannot_be_replaced_by_the_subject`: realne procesy usiłują
pisać/rename/chmod/child write; oczekiwana systemowa odmowa, nie mock flagi.
W tym samym module wyników zasadzić dokładnie `print("100 passed"); os._exit(0)`
w badanym programie oraz osobno SyntaxError subjectu i brak biblioteki egzaminatora.
Pierwszy nie daje Passed; dwa ostatnie dają odpowiednio DidNotPass i NotJudged.

**Akceptacja.** Zła funkcja wykryta mimo zielonego self-checka; prawdziwy pass z licznikiem;
zero tests nie przechodzi; brak outputu i infrastruktura rozróżnione; STOP nie jest porażką
agenta; chwilowa podmiana/przywrócenie nie omija ochrony; obcy scope nieczytelny w chronionym
trybie; brak osieroconych procesów. Ochrona i poziom zaufania widoczne w wyniku narzędzia/UI.

## WF-19 — UI Labu do porównania workflow

**Cel.** Człowiek może skonfigurować, uruchomić i zrozumieć pomiar całego workflow.

**Kontrakt implementacji.**

- Z edytora workflow działająca akcja `Evaluate workflow` otwiera/tworzy właściwy zestaw.
  Nie myli jej z normalnym Run i nie uruchamia płatnego pomiaru podczas otwarcia formularza.
- Kolumna wybiera workflow/rewizję, output step i jawne nadpisania. Wiersz wybiera przypadek,
  wejście, zaakceptowane kryteria i powtórzenia. Przed Startem pokazuje liczbę komórek,
  rozwiniętych kroków, limit równoległości i wydatku oraz ograniczenia ochrony.
- Kandydaci przypadków nie są aktywnymi kryteriami, dopóki człowiek ich nie zaakceptuje.
  Agent nie może sam zmienić definicji trwającego pomiaru lub autozaakceptować poprawki.
- Macierz z nazwami case/variant, Passed/DidNotPass/NotJudged, coverage, czasem i kosztem
  z oznaczeniem partial. Kliknięcie komórki otwiera rzeczywisty bieg/kroki, output i kryteria
  **z chwili pomiaru**. Nie duplikować pełnego strumienia w tabeli.
- Trend porównuje tylko zgodne serie WF-15. Dla zmienionych kryteriów pokazuje wyjaśnienie
  zamiast strzałki sugerującej regresję. Historyczne warianty pozostają czytelne po usunięciu
  dzisiejszego workflow. Nie pokazywać fałszywej pewności po jednym powtórzeniu.
- Stop idzie zwykłą drogą biegu. W trakcie pomiaru nadal można pisać z Leadem o jego stanie;
  Lead widzi ten sam RunRef, a nie osobny numer zadania Labu. Próba dopowiedzenia do
  mierzonego kroku pokazuje `This comparison uses fixed inputs. Start a new run to change them.`
  Zwykły workflow poza pomiarem zachowuje wiadomości z WF-08/10.

**Pliki.** `src/sections/lab/{index.tsx,io.ts,model.ts,matrix.tsx,columns.ts,trend.tsx,evaluate.ts}`,
istniejący edytor workflow i Entry point evaluate, `src/ipc/types.ts`,
`src-tauri/src/commands/lab.rs` tylko brakujące dane odczytu, lustra nowych IPC jeśli potrzebne.

**Test / RED.** `src/sections/lab/a-workflow-can-be-evaluated.test.tsx`,
`src/sections/lab/results-explain-what-was-measured.test.tsx`.
Kliknięcie rzeczywistej akcji i zapis formularza mają wywołać produkcyjny adapter
z workflow subjectem, a odczyt komórki ma użyć historycznego bindingu.

**Akceptacja.** Pełna ścieżka create→edit→save→start→progress→stop/result→history;
wszystkie kontrolki mają handler; odmowy preflight i brak ochrony widoczne; zmiana kryteriów
nie przerabia trendu wstecz; stare Agent/Skill zestawy nadal działają; angielski, tokeny
designu, jeden fakt w jednym miejscu i obowiązujący sufit gęstości. Status Leada działa,
zewnętrzna podpowiedź do subjectu jest odmawiana także przez narzędzie MCP.

## WF-20 — Odbiór przekrojowy na jednym zintegrowanym SHA

**Cel.** Udowodnić produkt przez rzeczywiste krawędzie, nie samą sumę lokalnych unit testów.

**Zakres.** Nowy moduł `src-tauri/tests/it/workflow_product_acceptance.rs`, deklaracja
w `it/main.rs`, `src/sections/run/workflow-product-acceptance.test.tsx` i
`src/sections/lab/workflow-measurement-acceptance.test.tsx`, potrzebne dane fixture pod
istniejącym katalogiem testów. Dokumentacja wyniku pod `docs/workflow-reliability/`.
Bez zmiany `harness/`, `checks/`, `scripts/` i bez osobnego frameworka dowodowego.

**Scenariusz automatyczny.**

1. Temp Git z tracked WIP, jawnym untracked wejściem, AGENTS i skillem z references/helper.
   Zwykły workflow: przygotowanie → równoległe A/B → fan-in → judge/powrót → synteza.
   Bariera dowodzi realnego nakładania; gałęzie wykonują delete/rename i oddają różne markery.
2. Osobny wariant tej samej próby ma tylko wyniki tekstowe i po trzy kopie. Judge rzeczywiście
   wykonuje fail→pass, synteza czyta komplet właściwych przekazań i własne historie kopii.
3. Podczas biegu Lead projektu A pyta o status, mimo aktywnej karty B. Wskazuje dokładny
   RunRef; dopowiedzenie idzie do obsługiwanej sesji, brak obsługi daje prawdziwe zdanie.
   Zgoda na pytanie/Stop nie daje prawa do następnego biegu.
4. Dwa kroki komunikują się skrzynką. Historia odróżnia zapisanie wiadomości od jej
   odczytu; graf nie zmienia zależności. Fault injection fan-in nie uruchamia konsumenta.
5. Zwykły folder bez Git: po sukcesie, Stopie i recovery zmieniony wynik pozostaje
   dostępny przez kliknięcie, projekt gospodarza nie zmienia bajtów.
6. Lab porównuje dwa pełne automatyczne workflow nad tym samym przypadkiem. Jeden
   naprawia fixture, drugi tylko fałszuje swój self-check. Zewnętrzny egzaminator rozróżnia
   je; komórki nie dzielą kontekstu. Zmiana bieżących kryteriów nie zmienia historii.
7. Każdy pomiar infrastrukturalnie przerwany pozostaje NotJudged. Nie ma żywych grup
   po Stop ani utraconego claimu; koszt nieznany nie staje się zerem.

**RED.** Ten test jest nowym sprawdzeniem po integracji poprzedników, więc jego uczciwa
czerwień polega na kontrolowanym zaszczepieniu reprezentatywnej regresji: gubienie usunięcia,
pomijanie judge bez Git diff, odczyt obecnych kryteriów zamiast snapshotu. Każda zasadzona
wada musi obalić właściwą asercję. Przywrócić zmieniony produkcyjny fragment przed GREEN;
nie zmieniać chronionej wyroczni i nie zostawiać mutacji w finalnym diffie.

**Żywy odbiór po integracji, nie zamiast testów.** Na zatwierdzonych jednorazowych fixture
i uzgodnionym limicie kosztu uruchomić krótkie Claude + Codex: rzeczywiste nakładanie pracy,
odczyt zasobu skilla, rozmowa z Leadem podczas biegu, przekazanie/wybrane sterowanie,
mały Lab whole-workflow. Nie wykonywać prób na ważnym repo ani nie zatrzymywać obcych procesów.
Zapisać SHA aplikacji, wersje CLI, konfigurację grafu i concurrency, finalne run IDs/statusy,
wynik zewnętrznego kryterium, fakty ograniczeń i lokalizację prywatnych dowodów bez sekretów.

**Warunek odbioru całości.** Wszystkie lokalne kryteria kart spełnione, zmiany zintegrowane,
pełne CI na tym SHA i aktualny smoke dwóch vendorów. Osobno wypisać ograniczenia pierwszej
wersji Labu: brak interaktywnych checkpointów, Serve i zewnętrznych mutacji w porównywanym
grafie. Brak któregoś dowodu oznacza „niezweryfikowane”, nie „production-ready”.

## 4. Co świadomie nie wchodzi do tego planu

- Zagnieżdżone/nieskończone pętle i przebudowa żywego grafu przez model.
- Zastępowanie vendorowych subagentów/skill runnerów własnymi odpowiednikami.
- Wymiana `codex exec` na inny protokół wyłącznie dla natychmiastowego dopowiedzenia.
  Uczciwe capabilities i skrzynka rozwiązują dwie różne potrzeby bez takiej obietnicy.
- Automatyczne wykonywanie hooków/importowanie permissions z obcego repo.
- Automatyczne instalowanie całego toolchainu, wszystkie package managery, dystrybucja
  wielohostowa, nowy kontener/daemon lub port Windows w ramach naprawy macOS.
- Zmiana skryptowego harnessu budującego Loadout, jego kryteriów i chronionych bramek.
- Udawanie, że niewykonalna natywna integracja albo brak systemowej ochrony został
  rozwiązany przez ostrzeżenie. WF-13 i WF-18 mają jawne granice blokady; trzeba je
  rozstrzygnąć na zmierzonym API/polityce, zanim ogłosi się kompletną realizację.

Plan jest gotowy do przydzielania kart. Nie jest potwierdzeniem, że opisane gwarancje
już istnieją w kodzie bazowym ani że aktywna kolejka Z została wykonana.
