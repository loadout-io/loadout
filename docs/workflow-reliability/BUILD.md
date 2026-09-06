# Budowa niezawodnych workflow — dziennik wykonania

Baza: `715567effb059ec3e684de2ba17d231b854d0b04` (`main`, 2026-09-05).
Gałąź: `workflow-reliability-build`. Praca trwa w osobnym worktree; poniższe wyniki
dotyczą zmienianego drzewa, nie wydanego ani scalonego SHA.

Kontrakt: [TASKS.md](TASKS.md), z pierwszeństwem aktualizacji
[REVALIDATION-2026-09-05.md](REVALIDATION-2026-09-05.md).
Polecenie właściciela obejmuje wykonanie całego planu. Nie daje zgody na osłabienie
wyroczni ani naruszanie innych kopii repozytorium. Cargo i integracja są szeregowe.

## Potwierdzone przejścia RED → GREEN

Każdy moduł niżej biegnie przez `cargo test --manifest-path src-tauri/Cargo.toml
--test it <moduł>::`. Liczby oznaczają wykonane testy, nie sam kod wyjścia.

| Zakres | Moduł | RED | Najnowsze sprawdzenie |
|---|---|---|---|
| WF-01 wspólne wejście | isolated_copies_share_one_input | 0/2 przed zmianą; później 0/1 dla pustego wznowienia i 0/2 dla plain resume | 8/8, w tym dwa różne wyniki Git i bez Gita |
| WF-02 operacje plikowe | fan_in_keeps_file_operations | 0/5 | 5/5 przed rozszerzeniem WF-03 |
| WF-03 przerwane składanie | interrupted_fan_in_never_starts_a_step | 0/3; rozszerzenie 7/8 ujawniło obejście przez CarryOn | 8/8 |
| WF-04 rzeczywista ocena | loop_judges_results_without_file_changes | 0/8 | 8/8 |
| WF-05 historia kopii | loop_copies_keep_each_result | 1/7; poprawiony fake CarryOn osobno 0/1 | 7/7 |
| WF-07 start z właściwej rozmowy | lead-start-keeps-its-workspace.test.tsx; lead_start_is_bound_to_its_workspace | 1/3 frontend | 4/4 frontend; 6/6 Rust |
| WF-09 adresowany status | lead_reads_the_addressed_run | 1/3 | 3/3 |
| WF-21 historia projektu | lead_finds_and_reads_project_history | 0/3; rozszerzenie 5/6 wykryło brak odnośnika listy | 6/6; frontend 2/2 |
| WF-22 nowe otwarcie lidera | a_new_lead_knows_saved_project_facts | 0/1 | 1/1; zapisane fakty bez udawania kontynuacji transkryptu |
| WF-25 żywe usługi | a_live_preview_keeps_its_working_copy | 0/6 przed implementacją | 10/10; zapis przed cleanupem 3/3; recovery 7/7 |
| WF-06 non-git wyniki | non_git_results_survive_run_completion | 0/3 | 5/5; browser 2/2 po adresowanym Open |
| WF-08 prawdziwa możliwość wiadomości | step_message_capability_matches_the_session | 0/2; dodatkowe 0/2 ujawniło podwójne echo odmowy | 3/3 wraz z FixedInputs; browser 2/2 |
| WF-10 potwierdzone sterowanie | lead_controls_only_the_addressed_run; checkpoint_answers_keep_their_generation | 0/2; checkpoint 0/1, potem 1/2 dla Lead continue | 3/3 sterowanie; checkpoint + continue 2/2; frontend 4/4 |
| WF-12 instrukcje projektu | repository_instructions_reach_lead_and_steps | 1/5, osobno 0/1 na podwójnym echo | pierwsze 5/5, rozszerzone 10/11: fixture replay trafiła we wcześniejszy guard WF-01, poprawione wejście czeka |
| WF-14 wybrane pliki | fresh_copy_carries_selected_inputs | 0/3 | 3/3, czwarty test przygotowania czeka; browser 1/1 po RED 0/1 |
| WF-15 kryteria historyczne | lab_history_uses_its_original_definition | 0/3, potem 5/8 i 8/10 na integralności | 10/10; frontend historii 2/2 i wcześniejsze testy Labu 18/18 |
| WF-13 pełne bundle | complete_skill_bundle_reaches_every_supported_session | 0/3, potem 3/4 dla odczytu historii | 4/4; natywny Lead, import i osobne rozmowy czekają na runtime RED |
| WF-16 jawny Check stdin | check_reads_only_its_bound_input | 0/1; rozszerzenie 0/1 dla frozen files i przyczyny | 1/1 i 1 ignorowany program-fixture uruchamiany przez realny Check; test prywatnych wejść hosta czeka |
| WF-16 zakresy kontekstu | run_context_scopes_do_not_share_inputs | 0/1 dla aliasów, potem 0/1 dla history reader | 1/1; rozszerzenie partial retry RED 0/1, poprawka czeka |
| WF-26 gotowość usług | a_preview_is_ready_before_its_consumer_starts | 0/6 | 6/6 i 1 ignorowany program-fixture; dodatkowy test wyścigu listenera czeka |
| WF-27 opis uruchomienia | agents_discover_how_different_repositories_start | 1/6; osobno exact grandparent RED | 8/8, rzeczywiste procesy w dwóch repo; frontend 6/6 |
| WF-28 uprawnienia usług | agents_manage_only_their_project_services | host ukrytego narzędzia 0/1; configured-not-started 1/2 | host dispatch GREEN; tryb startWhen asked w budowie |

Rozszerzenia WF-01 dotyczą także ograniczonego ponawiania capture, odczytu bez SQLite,
uszkodzonego manifestu oraz podmiany bajtów tej samej długości. Wyniki wcześniejszych
kopii są adresowane dokładnym OID, nie aktualną końcówką gałęzi ani fallbackiem do HEAD.

## Aktualizacja po wspólnych batchach (2026-09-06)

Ta sekcja zastępuje starsze wartości „czeka” w tabeli powyżej. To nadal wyniki WIP,
nie ukończone karty ani weryfikacja jednego finalnego SHA.

- WF-12: Rust 11/11, UI 3/3; WF-14: Rust 4/4, UI 1/1. Błąd czwartego fixture
  Node wynikał z domyślnego reportera zamiast TAP — nie był RED poprawki produktu.
- WF-13: pełny moduł 12/12. Obejmuje natywne wejście Lead Codex, osobne pakiety rozmów,
  import zamrożonych zasobów, odmowę escaping symlink, Claude plugin i Codex fresh-copy
  także z Borrow. Dwa stare testy Lead są czerwone przez intencjonalną zmianę kontraktu:
  wymagany wybór różniących się źródeł i katalog per rozmowa zamiast per agent.
  Natywne ścieżki korzystają także z dubli CLI; to nie jest płatny odbiór semantyki vendorów.
  Pozostaje provenance-safe wyłączenie materiału zainstalowanego przez gospodarza z wyników.
- WF-16: Check stdin i prywatność hosta 2/2 (+ ignorowany rzeczywisty program potomny);
  zakresy, częściowe retry i zarządzane workspace seeds 2/2. Seed jest czytany z zapisu
  adresowanego biegu, zamrażany prywatnie i wiązany z rzeczywistym work key oraz copy result.
  Regresje: wspólne wejście 8/8, fan-in 5/5, non-git 5/5, recovery usług 7/7.
- WF-23: właściwy backend źródło → podgląd → zgoda → nowy bieg starego grafu/modelu/plików
  miał 1/1 GREEN; UI historii ma 3/3. Kolejne dwa właściwe RED wykazały ukrycie różnicy
  grafu w trybie Current oraz sufit uprawnień po zmianie agenta przypisanego do kafelka.
  Poprawka czeka na ponowny batch. Źródłowa pamięć notatek do pełnego replay pozostaje otwarta.
- WF-24: właściwy runtime RED 0/1 (`prepare_result_restore` nieznane), UI RED 0/1.
  Eksport dokładnego wyniku do nowego folderu bez agenta i hooków Git jest w implementacji.
- WF-26: 7/7 (+ ignorowany program potomny), WF-27: 8/8.
- WF-28: host status/start/logs/restart/stop i granice uprawnień 9/9. Osobny test realnej
  konfiguracji zwykłych sesji Step wykazał 0/2 RED: obu vendorom brakowało mostu aplikacji.
  Transport jest w implementacji. Nie mylić hostowego GREEN z kompletną drogą sesji.
- WF-17: pierwszy proper RED odmówił formatu 2. Kompilator pełnych grafów jest w budowie;
  kolejny test zatrzymał się na błędnym kluczu fixture `maxTurns` zamiast `max_turns`.
  To błąd fixture, nie nowy RED produktu. Po korekcie ponownie sprawdzamy wykonanie.
- WF-18: lokalny pomiar wykonalności systemowej granicy dał 7 PASS (realne odmowy zapisu,
  rename/chmod i potomków). Transport produkcyjny, protokół zewnętrznego egzaminatora,
  klasyfikacja i odbiór obu vendorów pozostają do wykonania. Sama feasibility nie zamyka karty.

## Praca w toku i zakres pozostały

- WF-01–07: domknięcie przypadków granicznych, regresji i non-git retencji.
- WF-06: wyniki i kontrolowane usunięcie mają zawężone przejścia; odbiór przekrojowy pozostaje.
- WF-09/21/22: rozszerzenia granic odczytu, aktualności i zdania na ekranie oraz regresje.
- WF-25: panel 11/11, browser 3/3 (w tym wykrycie usługi po początkowym pustym odczycie),
  strict TypeScript przechodzi. Przyczyną recovery była chwilowa odmowa probe EPERM, nie brak zabicia.
  Bounded probe może teraz zobaczyć późniejsze ESRCH, ale po jakiejkolwiek odmowie nie odzyskuje
  prawa do SIGKILL. Nowe dwa testy RED, cały moduł startup reaper GREEN 4/4.
- WF-10: one-use zgody, dokładne ID checkpointu, Lead continue i UI odpowiedzi mają przejścia;
  regresja Z-39 wykazała brak nazw w potwierdzeniu Stop, poprawka czeka na ponowne 5 testów.
- WF-12/14: działają opt-in i selekcja; rozszerzone testy i natywne bundle WF-13 w toku.
- WF-26: realne HTTP/Stop/obcy listener mają przejścia; frontend 9/9 i editor 4/4.
- WF-16: JSON stdin, zamrożone pliki producenta, oddzielne task/instructions/handoffs/attachments,
  history reader i wyłączenie skutków pamięci mają przejścia. Partial retry jest w naprawie;
  odrębne workspace seeds nadal do wykonania. To nie jest systemowa ochrona filesystem.
- WF-12 frontend opt-in i history sources: 3/3, w tym osobny opis materiału dostarczonego
  przez Loadout i ładowania natywnego.
- WF-11/13, pozostałe WF-16–19, WF-23/24 i WF-28: nadal do wykonania.
- WF-20: odbiór przekrojowy nadal nieodbyty. Pełne CI, niezależna weryfikacja
  innym vendorem, uruchomienie desktopu i integracja na main nadal przed nami.

Zielony wąski test nie oznacza zamknięcia całej karty ani gotowości produktu.
Nie uruchamiano jeszcze płatnych sesji modelowych na potrzeby odbioru tej implementacji.

## Najnowszy checkpoint roboczy (2026-09-06, kolejne wspólne batche)

Poniższe wyniki zastępują wcześniejsze statusy tych samych wycinków. Nadal brak finalnego
SHA, pełnego CI, odbioru desktopu i integracji. Worktree: `workflow-reliability-build`.

- WF-13: pełne zasoby, natywna dostawa i bezpieczne usuwanie wyłącznie plików gospodarza
  **18/18 GREEN**. Realne zmiany użytkownika w skillach pozostają wynikiem; podmiana inode
  lub treści zatrzymuje cleanup zamiast kasować cudzy obiekt. Stare testy Lead: 4/4.
- WF-23: stary graf/model/pliki i ograniczenie uprawnień **2/2 GREEN**, także realne
  związanie zatwierdzonego limitu $4 mimo argumentu Start $8. Źródłowe notatki są odtwarzane
  po zmianie/usunięciu żywych plików: **2/2 GREEN**, w tym limit 512 KiB przed procesami.
  UI zapisanych ustawień i limitu: 3/3. Debug Start nie zapisuje odwracalnej treści rewizji:
  cały moduł adresowania Start 7/7.
- WF-24: eksport dokładnego obiektu Git bez agenta/hooków, blokada Forget przez aktywny
  podgląd oraz Keep/unpin **3/3 GREEN**. Non-git eksport binarnych plików/linków/trybu oraz
  odmowa zajętego celu i podmienionego katalogu z przekopiowanym receipt: **1/1 GREEN**.
  UI restore/keep: 2/2. Nie oznacza to jeszcze przekrojowego odbioru retencji.
- WF-28: realny host usług, Lead z dokładną zgodą Start/Restart/Stop oraz granice adresów
  **11/11 GREEN**. Step native configuration/Bridge: 2/2, log końcowego błędu procesu: 1/1.
- WF-17: rzeczywiste dwa grafy z równoległością, pętlą i syntezą × dwa powtórzenia, wynik
  4/4 komórek. Dodatkowo zachowane typowane CheckInput/context/seed i jawna półka/plik/rewizja
  workflow: cały moduł **3/3 GREEN + 1 ignorowany rzeczywisty program potomny**.
  Nie zakończono jeszcze kalkulatora rozmiaru rozwiniętego grafu ani wszystkich wejść case.
- WF-18: parser zewnętrznego egzaminatora **1/1 GREEN (osiem scenariuszy)**. Rzeczywisty
  Lab odróżnia fałszywe `100 passed; os._exit(0)`, SyntaxError i brak biblioteki egzaminatora:
  **1/1 GREEN, sześć komórek, Passed 1 / Judged 3**. Historia definicji nadal 10/10.
  Systemowa granica agentów i Check ma 3/3; dokładne pliki runtime i przygotowanie natywnego
  stanu miały kolejne dwa właściwe RED. Poprawki czekają na bieżący batch. Chroniony pomiar
  end-to-end pozostaje otwarty; nie nazywać samego parsera odpornym pomiarem.
- WF-19: wejście z edytora **1/1 GREEN**; osobne powtórzenia, czas, częściowy koszt i realne
  otwarcie zapisanego biegu z komórki **3/3 GREEN**. Formularze kolumn/cases mają proper
  browser RED i są w implementacji. Ścieżka open używa workspace i folderu z zapisu wyniku.
- WF-11: frontend opt-in 1/1, override 7/7, widoczny fakt zapisania wiadomości 1/1.
  Runtime test prawdziwego mostu do równoległych sesji czeka na RED, nie jest wdrożony.

Najbliższa ścieżka do odbioru: chroniony Lab + pełny formularz i preflight, skrzynka agentów,
spójne regresje, porządkowanie naruszeń bramek, dopiero potem WF-20. Nie dokładać nowych
projektów pobocznych. Wąskie przejścia nie zastępują kompletnego odbioru kart.

## Kolejny checkpoint: ochrona wykonania i skrzynka (2026-09-06)

Wyniki dotyczą nadal niezatwierdzonego worktree, nie `main` ani wydania. Główny checkout
pozostał czysty. Nowych płatnych sesji modelowych nie uruchamiano.

- Chroniony Lab: rzeczywisty Check oraz pełna droga `run_workflow_inner` przez oba
  produkcyjne drivery (tylko binarka modelowa jest dublem) → zwykły Check → zamrożony
  egzaminator **3/3 GREEN, 1,30 s**. Procesy mają systemową odmowę write/rename/chmod,
  odczytu nadrzędnego katalogu biegu i Git gospodarza; potomkowie zachowują ograniczenia.
  To nie jest odbiór płatnych CLI ani instalacji zależności dowolnego projektu.
- Kod egzaminatora jest atomowo publikowany poza drzewem subjectu; direct interpreter
  z `-I`, oddzielne cwd, zamrożone pliki wynikowe read-only. Tożsamość i bajty kodu oraz
  wynikowe dane Check są sprawdzane ponownie po zejściu procesu. Nowy test naturalnego
  zapisu bez martwego command/proof miał proper RED; walidacja jest poprawiona i czeka na GREEN.
- Wykryto realne zazielenianie `0 passed` w zwykłym Check. Testy procesowe miały
  **3 przejścia / 4 porażki**, po poprawce **7/7 GREEN**. Stare werdykty 3/3, bounded output
  6/6, pętle 9/9, execution facts 3/3 pozostały zielone.
- Skrzynka: rzeczywisty kanał do równoległych Claude/Codex, deduplikacja, odmowa cudzych
  adresów, trwała historia i brak hosta przy default-off mają przejścia. Rozszerzony moduł
  **3/5**, dwa limity timeout po 60 s: pełne przepisywanie index.json przy każdym send jest
  kwadratowe. Trwała publikacja pojedynczych wiadomości jest w naprawie; limitów nie zwiększono.
- Replay **4/4 GREEN, 1,00 s**: doszedł cienki `rerun_step` na wspólnym preview oraz
  zatwierdzany limit/ustawienia w rzeczywistym pytaniu do człowieka. Capture jawnego
  wyłączenia agentMessages ma osobny proper RED (pole false ginęło w serializacji).
- Usługi: granice zgody **13/13 GREEN**. Nowy odbiór Leada przez AppState nadal otwarty:
  poprawiono kolejność fixture i rozpoznano błędny klucz fixture `runId`; burst uruchomień
  ujawnił też prawdziwy `File exists` przy nazwie socketu opartej na czasowym prefiksie UUID.
- Lab UI: formularze, picker zapisanych wejść, zakres ochrony, kod egzaminatora widoczny
  przed osobnym Accept **11/11 browser GREEN**; wiring i wskazane regresje **122/122**.
  Backend zapisanych wejść **5/5 GREEN**. Setter ochrony miał proper RED 0/2; poprawka
  zachowująca wszystkie cases/statusy i CAS czeka na wspólny batch. Strict TypeScript przeszedł.
- Rozmiar Lab: dwa proper RED wykazały przepuszczenie 5914 rozwiniętych krawędzi oraz
  odrzucenie 112 sekwencyjnych kroków jako rzekomych 113 drzew. Wspólny kalkulator i aliasy
  rzeczywistych kopii są w budowie. Zachowanie source additionalInputs nadal w osobnym wycinku.

Nadal przed odbiorem: powyższe czerwienie, podgląd rozmiaru przed Startem, komplet regresji,
bramki i formatowanie, WF-20, desktop smoke oraz integracja. Brak finalnego SHA i pełnego CI.

## Checkpoint: domknięte czerwienie i odbiór złożonych ścieżek (2026-09-06)

Nadal worktree `workflow-reliability-build`, bez commita/integracji, `main` czysty.
Poniższe przejścia dotyczą bieżącego kodu, nie deklaracji o całej karcie:

- Skrzynka **5/5 GREEN**, w tym 1000 wiadomości, 8 MiB, paginacja i rzeczywiście zakończony
  adresat. Publikacja jednej wiadomości nie przepisuje wszystkich poprzednich; stały manifest
  RunRef i osobne niezmienne wiadomości mają produkcyjnych czytelników historii.
  Capture jawnego `agentMessages: false` **5/5 GREEN**.
- Burst **256/256** rzeczywistych kanałów bez kolizji; wcześniej przechodziły tylko 3/256.
  Step i Lead → rzeczywisty host usług → obaj producenci: **4/4 GREEN**.
- Kalkulator rozwinięcia **5/5 GREEN**, stare aliasy/fan-in/pętle **22/22 GREEN**.
  Dodatkowy właściwy RED Project i Pick wskazujących ten sam folder wymagał zachowania
  fizycznej równości ścieżek; poprawka czeka na GREEN.
- Lab additionalInputs: **4/4 GREEN + 1 ignorowany rzeczywisty program potomny**.
  Wspólna selekcja dociera do cwd, różne selekcje hosta odmawiają zamiast robić union,
  jawny zapisany seed zachowuje stare bajty, ochrona źródła nie jest po cichu osłabiana.
- Zapis zakresu ochrony zachowuje zaakceptowane przypadki i CAS: **2/2 GREEN**.
  Naturalny zamrożony egzaminator nie potrzebuje pustego shell command/proof:
  chroniony moduł **4/4 GREEN** razem z pełnymi ścieżkami obu driverów.
- Zmieniona mapa wyników dawała rzeczywiste fałszywe Passed 2/Judged 3: właściwy RED.
  Definicja wiąże teraz cały zmierzony graf, także mapę, wejścia i połączenia; po zmianie
  wynik jest NotJudged z powodem. Cały moduł plus historia definicji **11/11 GREEN**.
- Dodatkowe stare regresje skilli, przekazań, pętli i sterowania Leadem:
  **54/54 GREEN + 1 świadomie ignorowany płatny CLI**, 8,47 s.

W toku: read-only preflight i związanie Startu z pokazanymi źródłami, zapis historycznego
workflow jako nowej definicji, odświeżenie przyjętych notatek między turami Leada, pierwszy
złożony scenariusz WF-20. Nadal nie ma pełnego CI, finalnego SHA, desktop smoke ani
żywego płatnego odbioru. Nie nazywać powyższej sumy testów ukończonym WF-20.

## Checkpoint WF-20: ujawniona nieaktualna kopia po poprawce (2026-09-06)

- Preflight core **5/5 GREEN**, formularze Lab **13/13 GREEN**, ostatni wiring
  **102/102 GREEN**. Pełna gwarancja tylko-do-odczytu czeka: istniejące czytniki bibliotek
  wykonują recovery i mogą sprzątać tempy. Szósty test z przerwaną publikacją jeszcze
  nie był uruchomiony. Rozszerzenie o readonly adapter w `commands/workflows.rs`
  jest pytaniem do właściciela zgodnie z AGENTS; plik nie został zmieniony.
- Cały moduł ochrony **8/8 GREEN + 2 ignorowane**, w tym trzy nowe read-only readiness.
  Chroniony Lab nadal **4/4 GREEN**.
- Kopia historycznego workflow z nową tożsamością: replay **5/5 GREEN**, UI **4/4 GREEN**.
  Przyjęte notatki między turami Leada **2/2 GREEN** po właściwym RED ostatniej wycofanej
  notatki. Szósty złożony replay A1/A2/dwie kopie jest zapisany, jeszcze bez wykonania.
- Nowy browser WF-20 **1/1 GREEN**: równocześnie pracujące kafelki, przeplatane odpowiedzi,
  adresowany Lead przy aktywnym innym workspace, odpowiedź na dokładne pytanie, history
  i otwarcie zachowanego wyniku, brak fałszywego kosztu zero. Nie ma jeszcze mutacyjnego RED.
- Runtime WF-20 **właściwy RED**: dwa rzeczywiście nakładające się kroki otrzymały pełny
  skill, instrukcje repo, WIP i wybrany untracked plik; Lead odczytał dokładny żywy bieg.
  Po nieudanym judge oba kroki wykonały drugą rundę i zapisały marker `2`, lecz merge/judge
  oraz synteza nadal dostały `1`. `folded` i `FanInPreparation.input_ready` w `run.rs`
  cache'ują katalog bez rozróżnienia generacji rodziców. To blokuje odbiór pętli z fan-in;
  poprawka jeszcze nie powstała. Pierwsze błędy fixture (FreshCopy a pliki przygotowania,
  sesja kroku a run UUID) usunięto przed rozpoznaniem tej rzeczywistej regresji.

Nadal brak finalnego SHA, pełnego CI, odbioru desktopu i integracji. Płatne sesje nie
były uruchamiane; właściciel dostał osobne pytanie o jednorazowe fixture i limit 5 USD.

## Checkpoint: naprawa pętli, readonly Lab i wznowienie (2026-09-06)

Polecenie „to napraw te punkty” otworzyło omówiony readonly adapter bibliotek.
Kod nadal leży wyłącznie w `workflow-reliability-build`, baza `715567effb059ec3e684de2ba17d231b854d0b04`.
`main` jest czysty na tej samej bazie. Brak commita, merge/push i płatnych sesji.

- WF-20 runtime przeszedł po właściwym RED z wynikiem pierwszej rundy w drugiej.
  Zapadka `folded` usunięta. Marker przechowuje konsumenta, rzeczywiste kroki rodziców
  i poprzednią deltę importu; następna runda porównuje stare wejście, nowe wejście
  i własną pracę konsumenta. Konflikt odmawia przed zapisem. Pełna struktura wyniku
  uwzględnia własne dzieci usuwanego katalogu. Cel i źródła są chronione przed aktywnym
  użyciem, a manifesty sprawdzane ponownie przed publikacją.
- Nowy moduł odświeżenia miał **6/6 runtime RED** na skompilowanym szkielecie, następnie
  **6/6 GREEN**: aktualizacja, konflikt własnej zmiany, powrót do origin, własne dziecko
  usuwanego katalogu, niezmienione/zgodne wejście, zmiana celu po przygotowaniu.
- Wznowienie konsumenta miało właściwy RED: własna poprawka była uznawana za konflikt
  z niezmienionym importem. Nowy bieg przenosi zweryfikowaną bazę porównania, bez starej
  tożsamości przygotowanego kroku. Test mierzy plik w drugim rzeczywistym RunSpec przed
  pracą agenta. Nie wymaga nowej gałęzi, gdy Git poprawnie zachowuje niezmieniony wynik.
  Cały moduł przerwanego składania i wznowień **9/9 GREEN**.
- Preflight Lab **8/8 GREEN**: wspólne czytanie bibliotek nie wykonuje recovery; test
  przerwanej publikacji miał właściwy RED. Brakujący lub niewykonywalny interpreter
  jest odrzucany zarówno w preview, jak i przed rzeczywistym planowaniem. Dwa testy
  miały właściwy RED „preview accepted an unavailable examiner interpreter” przed
  poprawką. Readiness nie uruchamia interpretera ani kodu i nie potwierdza logowania.
- Historyczny replay **6/6 GREEN**. Złożony Current/Just dwóch kopii wymagał poprawy
  fixture, która niepotrzebnie zmieniała bazę projektu. Ochrona origin pozostała bez zmian;
  dwie kopie A1 nadal różnią się od A2 i trafiają do właściwego ponowienia.
- Ostatni batch po formatowaniu: **24/24 Rust GREEN, 10,45 s** (wznowienia/przerwania,
  pełny WF-20, preview i rozmiar Lab). Kolejny **63/63 GREEN + 3 ignorowane, 16,82 s**:
  rzeczywiste usługi i gotowość, skille, ochrona egzaminatora, wyniki bez Gita, odświeżenie.
  Wskazane testy UI **26/26 GREEN, 8,09 s**, strict TypeScript i `git diff --check` bez błędów.
  Nadal znany warning fixture z powtarzającym się kluczem `pipeline.json`.
- Bramka martwego API najpierw wskazała trzy funkcje bez produkcyjnego wywołania.
  Usunięto nieużywany zapis pliku i dwa zbędne wrappery; testy używają tych samych
  istniejących API co produkt. `wired` GREEN. `boundary`, `tokens`, `invoke-args`,
  `suppressions`, `tests-listed` GREEN. Rust i frontend sformatowane standardowymi narzędziami.

### Blokada wymagająca decyzji właściciela

`bash checks/vocabulary.sh` po jednej rundzie usunięcia rzeczywistego żargonu nadal
odmawia: **20 trafień, baseline 0**. Wśród nich fragment kodu
`) : snapshot !== '' && problem === null ? (` i interpolacja `source.digest?.slice(0, 8)`,
choć te identyfikatory nie są wyświetlane. Pozostałe obejmują opisy testów, przykładowe
polecenia i nazwy workflow pochodzące z danych fixture. Nie zmieniono nazw zmiennych
ani opisów testów po to, żeby ukryć błąd skanera; nie zmieniono baseline ani allowlisty.

Zgodnie z AGENTS §7 dalsza naprawa chronionego checka czeka na zgodę. Zakres proponowany:
poprawić wyłącznie rozpoznawanie tekstu widocznego, zachować wykrywanie prawdziwego
żargonu oraz wykazać czerwienią rzeczywiste naruszenie. `harness/`, `checks/`, `scripts/`,
`AGENTS.md` i zamknięte decyzje nadal bez zmian.

Nie ma jeszcze końcowego Clippy/pełnego CI, mutacyjnego odbioru całego WF-20,
desktop smoke ani integracji. Powyższe przejścia nie są deklaracją ukończenia całej aplikacji.

## Próba lądowania: check językowy zamknięty, Clippy blokuje (2026-09-06)

Właściciel odpowiedział „ok zmerguj wszystko” na pytanie o naprawę chronionego checka.
`checks/vocabulary.sh` zachowuje ten sam słownik, baseline 0 i pustą allowlistę.
Odczyt TypeScript/JSX korzysta z istniejącego parsera TypeScript w cienkim
`checks/vocabulary-ts.cjs`: oddziela rzeczywisty tekst od wyrażeń, identyfikatorów
i komentarzy. Pliki testów nie są tekstem produktu; importowane przez nie produkcyjne
komponenty nadal są skanowane we własnych plikach.

Nowy `src/vocabulary-check.test.ts` uruchamia rzeczywisty check w osobnym katalogu
tymczasowym. Pierwszy przebieg miał 3 właściwe porażki i 7 przejść, po poprawce 10/10.
Rozszerzenie o `aria-label={"Session"}` wykryło brak odczytu krótkiego atrybutu przez
wyrażenie; po poprawce **11/11 GREEN**. Prawdziwy żargon nadal powoduje exit 1,
także w ternary, szablonie, zwykłym stringu i komunikacie Rusta. Pełny skan:
**334 pliki, 0 trafień, baseline 0, allowlisted 0**. Strict TypeScript GREEN.

Pierwszy Clippy `--lib --tests -- -D warnings` odmówił ze 149 błędami produkcyjnymi.
Wykonano jedną rundę mechanicznych napraw `cargo clippy --fix --allow-dirty --allow-staged
--lib --tests` i formatowanie. Nie obniżono poziomów lintów i nie dodano wyjątków.
Pozostają **72 diagnostyki produkcyjne**; przebieg naprawczy zgłosił ponadto
**86 diagnostyk celu it**. Wśród nich są nadmiernie długie funkcje, zbędne kopie,
typy/konwersje oraz trzymany przez await zamek w fixture testowej.

Zgodnie z AGENTS §7 czerwona bramka po tej rundzie zatrzymuje lądowanie i wymaga
decyzji właściciela o dalszej naprawie. Nie wykonano commita, merge, push ani pełnego CI.
`main` pozostaje czysty na `715567effb059ec3e684de2ba17d231b854d0b04`; cała praca
pozostaje w worktree implementacyjnym. Wcześniejsze wyniki testów aplikacji są
historyczne względem tej ostatniej mechanicznej rundy i wymagają ponownego odbioru.

## Druga runda Clippy po zgodzie „ok popraw” (2026-09-06)

Usunięto produkcyjne diagnostyki typów, zbędnych przeniesień i kopii, konwersji,
formatowania, niepełnego Debug oraz nadmiarowych argumentów. Nie dodano wyciszeń.
Historia, przygotowanie chronionego sterownika, gotowość usług, zapis kroku do
run.json i kopiowanie grafu porównania mają wydzielone operacje. Zestaw rewizji
zatwierdzonych w Lab jest teraz jednym argumentem IPC `approval`; okno i test
prawdziwego kliknięcia wysyłają wspólnie `revision` oraz `sources`.

W testach zamek obserwacji kończy się leksykalnie przed await. Zakaz startu podczas
podglądu Lab ma licznik prób sprawdzany przez fixture: obsłużony przez runtime błąd
sterownika nie może ukryć próby startu. Asercje odmów zachowano przy usuwaniu expect_err.

Ponowny odbiór bieżących zmian:

- Rust: **75 przejść, 0 porażek, 1 ignored** w 12 zawężonych modułach: readonly
  preview Lab (8), izolacja kontekstu (2), zapisane wejścia replay (6), zamrożona
  pamięć replay (2), historia Leada (6), instrukcje repo (11), usługi agentów (14),
  komenda aplikacji z poprzedniego kroku (7), cały workflow w Lab (3 + 1 ignored),
  ochrona wyroczni (4), wyniki bez Git (6), fan-in każdej rundy (6).
- Vitest: check językowy **11/11**, rzeczywiste kliknięcie Run w Lab z nowym
  argumentem zatwierdzenia **1/1** (13 pozostałych przypadków tego pliku poza filtrem).
  Nadal występuje wcześniejsze ostrzeżenie React o powtórzonym kluczu pipeline.json;
  zielony wynik tego testu nie oznacza czystej konsoli.
- `cargo check --tests`, TypeScript, Prettier wskazanych plików i diff --check GREEN.
- Końcowy `cargo clippy --lib --tests -- -D warnings`: **13 błędów produkcyjnych**,
  wszystkie too_many_lines. Do dalszego podziału pozostają replay::prepare,
  result_restore::set_kept, dziewięć funkcji run.rs i compose_selected (101 linii).
  Trzynasty przypadek to command_handler (102 linie), czyli deklaratywna lista
  rejestracji, nie algorytm. Diagnostyki testów pozostają do domknięcia; ten końcowy
  przebieg odmówił już na bibliotece i nie stanowi ich ponownego odbioru Clippy.

Wysłano właścicielowi pytanie o wyjątek wyłącznie dla listy komend oraz chroniony
wpis w checks/suppressions-allowlist.json. Bez odpowiedzi niczego tam nie zmieniono.
AGENTS §7 zatrzymuje kolejną rundę po czerwonej bramce; potrzebna dalsza decyzja
właściciela. Nie wykonano commita, merge, push ani pełnego CI. Main nadal czysty
na 715567effb059ec3e684de2ba17d231b854d0b04; powyższe są wyniki brudnego worktree,
nie dowód integracji ani wydania.
