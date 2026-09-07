# Zlecenie dla Claude Code: wspólny plan workflow

Zaimplementuj w Loadout kompletną, gotową do używania funkcję wspólnego planu między krokami workflow. Wykonaj kod, integrację, testy i weryfikację opisaną poniżej. Nie kończ na analizie, projekcie interfejsu, samych typach ani działającym prototypie. To jest samodzielne zlecenie: nie potrzebujesz wcześniejszej rozmowy z użytkownikiem.

Repozytorium: `/Users/jakubgawronski/Projects/Loadout`. Dokumentacja i objaśnienia po polsku, interfejs po angielsku. Przeczytaj najpierw `AGENTS.md`, następnie `docs/DECISIONS-LOCKED.md` i `harness/README.md`. Decyzje zablokowane mają pierwszeństwo przed tym dokumentem.

## 1. Cel i granice pracy

Użytkownik chce w panelu zwykłego kroku jedno ustawienie **Plan**, z opcjami **Off / Create / Update / Use**. Pierwszy agent tworzy plan, kolejni mogą go rozwijać, a implementer i QA pracują na jednoznacznie wskazanej, tej samej wersji. Funkcja działa zarówno z Claude Code, jak i z Codex, w dowolnych kombinacjach vendorów.

Przykład docelowy:

    Planner          Create   → version 1
    Final Plan       Update   → version 2
    Designer         Update   → version 3, zmiana sekcji Design
    Implementation   Use      → version 3
    QA               Use      → ta sama version 3
    Implementation   Use      → version 3 + aktualne uwagi QA
    QA               Use      → version 3 + wynik poprawki

Nazwy powyżej są przykładem konfiguracji. Silnik nie rozpoznaje ról po nazwach, instrukcjach ani modelu. Nie dodawaj rodzaju węzła Plan, obowiązkowego planowania, automatycznej recenzji ani etapu ukrytego w schedulerze. Jeden zwykły krok z Plan = Off nadal działa bez dodatkowej ceremonii.

**Context jest osobną funkcją, już budowaną.** Przeczytaj `docs/context-library/PLAN.md` i aktualny kod. Context przechowuje wielokrotnie używane materiały źródłowe. Plan jest dokumentem konkretnego biegu. Współdzielą składanie wejść, budżet i dostęp do źródeł; nie zastępują się nawzajem.

Stan sprawdzony przy przygotowaniu tego zlecenia, 2026-09-07: czysty main na `14a7b99a54bdd5867ce6c0a8e90a80df4878e12f`; CT-01 w historii main; osobny worktree `loadout-h-ct-02`. To historyczny punkt orientacyjny, nie gwarancja aktualnego stanu ani wynik testów tego zlecenia.

Przed pracą odśwież HEAD, stan drzewa, worktree i aktywne biegi. Nie resetuj zmian użytkownika, nie zatrzymuj cudzych procesów i nie uruchamiaj drugiej implementacji Context. Pracuj we własnym worktree, zgodnie z bieżącym harnessem. Zmiany we wspólnych plikach integruj po odpowiednich etapach CT; nie kopiuj implementacji z aktywnego, nieukończonego worktree. Możesz wcześniej wykonać niezależny model i testy planu. Brak gotowej zależności nie uzasadnia drugiego resolvera ani atrap w finalnej dostawie.

## 2. Problem, który ma rzeczywiście zniknąć

W zbadanym biegu Plan → Final Plan → Design → Implementation → QA implementer otrzymał odniesienie do Design, QA odniesienie do Implementation, a wspólny Final Plan nie był jawnym wejściem obu kroków. QA sam znalazł pełny plan na dysku; implementer początkowo korzystał ze starszej specyfikacji. Agenci oceniali pracę względem różnych ustaleń.

Zwykłe przekazania mogą być skracane do sekcji odsyłających do pełnego załącznika. Sama obecność ścieżki i rozmiaru pliku w manifeście nie dowodzi, że treść trafiła do promptu. Nie wolno też wnioskować, że agent pliku nie przeczytał tylko dlatego, że nie był wstawiony do promptu: w tym biegu Final Plan faktycznie odczytał pełny początkowy plan.

Naprawa ma zapewnić wspólną wersję ustaleń, pełną dostawę obowiązkowej treści, działające doczytywanie szczegółów i uczciwy podgląd wejść. Nie polega na podmianie modelu ani wrzucaniu całej historii wszystkich przodków do każdego promptu.

## 3. Zachowanie ustawienia Plan

| Opcja | Wejście i działanie | Wynik |
| --- | --- | --- |
| Off | Brak udziału w mechanizmie wspólnego planu. | Dotychczasowy wynik kroku. |
| Create | Agent dostaje zadanie i wybrane źródła, przygotowuje pierwszy plan. | Pierwsza kompletna wersja, publikowana przez Loadout. |
| Update | Agent dostaje konkretną wersję i zakres dozwolonych zmian. | Nowa wersja albo jawne „bez zmian”. |
| Use | Agent dostaje przypiętą wersję i wykonuje własne zadanie. | Wynik kroku z informacją, jakiej wersji użył; bez prawa publikowania planu. |

Create i Update nie zastępują instrukcji kroku ani zwykłego przekazania. Ten sam krok może przygotować projekt interfejsu oraz uaktualnić sekcję Design. Use nie jest synonimem QA: może oznaczać implementację, testowanie, analizę albo dowolną inną pracę.

Plan to **jeden logiczny dokument na bieg, z niezmiennymi wersjami**. Nie jest globalnym singletonem aplikacji, nie jest współdzielony automatycznie między biegami i nie jest jednym swobodnie nadpisywanym plikiem. Nazwa folderu, tytuł agenta czy najpóźniejszy timestamp nie określają wersji wejścia.

W panelu kroku umieść Plan w istniejących dodatkowych ustawieniach. Nie zwiększaj liczby podstawowych stale rozwiniętych pól. Zastosuj istniejące komponenty, tokeny, obsługę klawiatury i reguły gęstości. Zmienione ustawienie musi być widoczne w podsumowaniu panelu.

Dodatkowe opcje ujawniaj tylko wtedy, gdy są potrzebne:

- Update: `Can update` z wyborem sekcji; domyślnie wszystkie edytowalne sekcje. Wymagania człowieka pozostają chronione niezależnie od tego wyboru.
- Use: opcjonalne `Focus on`, czyli preferowane sekcje szczegółowe. Wspólny obowiązkowy rdzeń zawsze pozostaje na wejściu.
- Źródło wersji domyślnie wynika z zależności. Gdy trzeba wskazać konkretną pracę, udostępnij `Same plan as` z wyborem kroku. Nie wymagaj ręcznego wybierania numeru wersji, która dopiero powstanie.
- Podgląd pokazuje źródło przed biegiem, a rzeczywistą wersję po jej powstaniu. Nie pokazuj fikcyjnego numeru „przewidywanej” wersji.

Nie dodawaj osobnej biblioteki Plan w nawigacji ani nowego dużego edytora dokumentów. Podgląd treści, historii wersji i różnic otwiera się z istniejącego panelu wejść/wyników biegu.

## 4. Treść planu i ochrona wymagań

Plan przechowuj jako walidowane dane, z których Loadout deterministycznie renderuje tekst dla agentów i UI. Nie parsuj znaczenia dowolnych nagłówków Markdown wyrażeniami regularnymi. Dobierz jeden mały schemat obejmujący:

- cel, zakres i rzeczy wyłączone z zakresu;
- wymagania ze stabilnymi identyfikatorami, np. R1, R2;
- kryteria akceptacji i oczekiwaną metodę sprawdzenia;
- ustalone decyzje i ograniczenia wpływające na zachowanie produktu;
- sekcje szczegółowe, np. Implementation, Design, Validation;
- propozycje, założenia, pytania i nierozstrzygnięte konflikty;
- odwołania do konkretnych źródeł i ich wersji.

Wersja ma identyfikator biegu i dokumentu, numer/ID wersji, rodzica, fizyczny krok i próbę autora oraz wersję schematu/renderera. Tożsamości, ścieżki, pochodzenie i aktualny wskaźnik nadaje aplikacja, nie model. Użyj istniejących typów identyfikatorów i publikacji tam, gdzie pasują; nie buduj nowej ogólnej platformy artefaktów.

Stabilne ID dotyczą tego samego wymagania w kolejnych wersjach. Aktualizacja jednej sekcji nie renumeruje pozostałych. Treść wymagań człowieka, liczby, jednostki, negacje i kryteria pozostają dokładne. Nowe wymaganie otrzymuje nowe ID. Nie deduplikuj semantycznie podobnych zdań o różnych warunkach.

Rozróżniaj pochodzenie od statusu: model może zaproponować wymaganie, ale nie może oznaczyć go jako zatwierdzone przez człowieka. W pierwotnym incydencie „Start ≤2 s” było propozycją w planie, a nie udokumentowaną decyzją użytkownika. Takiej propozycji nie wolno zamieniać w zatwierdzone kryterium przez sam fakt zapisania planu.

Jawne wymagania zadania i istniejące zatwierdzone `criteria` są obowiązkowym wejściem. Zachowaj je także niezależnie od tego, czy model dobrze wyekstrahował je do planu. Walidator może sprawdzić obecność znanych ID i dokładną treść; nie udawaj, że potrafi dowieść kompletności interpretacji dowolnego tekstu.

Update może doprecyzować rozwiązanie w powierzonym zakresie, zachowując wymagania i decyzje spoza tego zakresu. Próba usunięcia/osłabienia wymagania człowieka albo zmiany niedozwolonej sekcji jest odrzucana przez kod. Uzasadnioną zmianę takiego wymagania agent zapisuje jako propozycję lub konflikt, do rozstrzygnięcia istniejącą ścieżką interakcji. Nie dodawaj obowiązkowego zatwierdzania każdej zwykłej aktualizacji.

Plan nie staje się systemowym promptem i nie omija instrukcji repozytorium ani uprawnień. Materiał Context jest źródłem informacji; tekst dokumentu nie może sam przyznać sobie prawa aktualizacji planu lub zmienić grafu.

## 5. Jednoznaczne wersje, graf, kopie i pętle

Zaimplementuj jeden resolver zależności planu, wspólny dla zapisu/podglądu, Startu, wznowień i wykonania. Nie odczytuj globalnego `latest` przy każdym starcie konsumenta.

Zasady konfiguracji:

1. Nowy pełny bieg korzystający z planu ma dokładnie jeden Create. Może poprzedzać go dowolna praca bez planu. Update i Use muszą mieć osiągalne źródło planu.
2. Create i kolejne Update tworzą uporządkowany przez graf łańcuch. Dwóch nieuporządkowanych autorów odrzuć przed uruchomieniem pierwszego procesu, także gdy wybierają różne sekcje. Nie dodawaj cichej kolejki według czasu zakończenia.
3. Create/Update mają jedną kopię. Use może działać w wielu kopiach i równolegle, bez globalnego locka na cały czas pracy modelu.
4. Źródło automatyczne rozwiązuje się z grafu i rzeczywistych wyników jego przodków, a nie z wszystkich planów na dysku. Przy jednoznacznym łańcuchu użytkownik niczego dodatkowo nie ustawia.
5. Jawne `Same plan as` wskazuje ID kroku. Dla Create/Update oznacza wersję opublikowaną, dla Use wersję rzeczywiście przez niego używaną. Taka relacja musi być prawdziwą, widoczną zależnością wykonania w zapisanym grafie; sam picker nie może tworzyć ukrytej kolejności.
6. Join z różnymi odziedziczonymi wersjami wymaga jednoznacznego wyboru. Nie wybieraj wyższej wersji, pierwszej gałęzi ani późniejszego timestampu. Konflikt pokaż ze wskazaniem kroków i możliwej naprawy.
7. Nierozstrzygnięty szkic można zapisać z ostrzeżeniem. Start odmawia, gdy zależność, cykl, niedozwolony autor lub wymagany plan są nieprawidłowe. Waliduj też żądania omijające UI.

**Implementacja i QA:** wynik wykonawcy zapisuje ID planu użytego do pracy. QA powiązane z tym wynikiem otrzymuje ten sam plan. W zwykłym łańcuchu bez aktualizacji po drodze działa to automatycznie. Dla bardziej złożonego grafu służy jawne `Same plan as`; tam, gdzie istniejące dane pętli już jednoznacznie wskazują sprawdzaną pracę, wykorzystaj tę relację. Nie zgaduj, że kafelek jest QA, z jego nazwy.

Jeżeli między wykonaniem a jego oceną pojawiła się nowa wersja, nie podmieniaj podstawy oceny. Ocenę według nowych ustaleń trzeba skonfigurować jawnie, a jej wynik nie jest potwierdzeniem zgodności starej implementacji z poprzednim kontraktem. Pokaż tę różnicę w podglądzie wejść.

W tej dostawie Create/Update znajdują się poza pętlami naprawczymi; Use wewnątrz pętli dziedziczy przypiętą wersję na cały cykl implementacja–QA–poprawki. Walidacja przed Startem ma nazwać próbę umieszczenia autora planu w takiej pętli. Kolejna faza grafu może jawnie aktualizować plan po zakończeniu wcześniejszej pętli. Nie implementuj ukrytej przebudowy aktywnego grafu.

Równoległe propozycje są zwykłymi wynikami agentów Use. Użytkownik może połączyć je zwykłym krokiem Update. Nie dodawaj specjalnego węzła ani automatycznej syntezy w schedulerze.

Przetestuj rozwiązywanie po rozwinięciu kopii i pętli, wraz z fizycznymi ID kroków i numerami prób. Nie odwołuj się do „ostatniego kroku o takiej nazwie”. Nie wybieraj nieuruchomionej, pominiętej lub nieopublikowanej próby.

## 6. Tworzenie i aktualizacja przez rzeczywistego agenta

Agent proponuje dane, a Loadout je zapisuje. Do istniejącego mostu dodaj minimalną operację zgłoszenia planu, np. `submit_plan`, dostępną tylko w Create/Update. Lista narzędzi i egzekwowanie wywołań mają korzystać z tego samego uprawnienia przypisanego do fizycznego kroku/próby. Brak usług lub komunikacji między agentami nie może wyłączać potrzebnego mostu.

Create zgłasza dokument zgodny ze schematem. Update zgłasza `baseVersion` i zmiany wskazanych elementów/sekcji albo jawne `unchanged`. Preferuj stabilne ID elementów zamiast indeksów tablic. Nie zmuszaj agenta do przepisywania całego dokumentu przy zmianie jednego akapitu.

Backend:

- sprawdza zgodność wersji bazowej, zakres zmian, wymagane pola, odwołania, limity i chronione wymagania;
- stosuje poprawki do konkretnego rodzica, zachowując resztę;
- materializuje nowy kompletny dokument oraz jego deterministyczny widok;
- publikuje dopiero poprawny, kompletny wynik zakończonego kroku;
- nie tworzy nowej wersji dla rzeczywistego `unchanged`.

Zgłoszenie przez narzędzie jest kandydatem, a nie natychmiastową publikacją. Krok może poprawić odrzucone zgłoszenie w swojej istniejącej sesji i w swoim limicie. Nie uruchamiaj dodatkowego modelu do naprawy formatu. Brak poprawnego kandydata na końcu Create/Update jest nazwanym niepowodzeniem dostarczenia wyniku; `exit 0` procesu tego nie zmienia.

Kandydat nie może zostać opublikowany po Stopie, timeoutcie, nieudanym zakończeniu kroku albo przez spóźnioną próbę. Jeżeli skonfigurowany kontrakt wyniku kroku nie został spełniony, nie publikuj jego planu jako gotowego. Polityka kontynuacji może uruchomić niezależne kroki, ale nie zastępuje brakującego wymaganego planu starszą wersją.

Publikacja jest atomowa, warunkowa względem rodzica, idempotentna względem operacji oraz odporna na restart między zapisem dokumentu a zapisaniem wyniku kroku. Powtórzenie tej samej zakończonej publikacji nie tworzy v4 i v5 z tych samych danych. Nie ma `last writer wins`.

Następny Use dostaje kompletny aktualny dokument, nie łańcuch poprawek do samodzielnego odtwarzania. Historia zmian jest dodatkowym widokiem, nie podstawowym wejściem.

Zachowaj natywne wykonanie kroków obu vendorów. Nie przepinaj wszystkich kroków Codex na transport rozmowy tylko na potrzeby planu. Sam test JSON lub obecność `submit_plan` w liście MCP nie dowodzi, że funkcja działa w prawdziwym przebiegu.

## 7. Dostawa treści i efektywność tokenowa

Wprowadź jedną wspólną kompozycję wejścia dla Plan, Context i przekazań. Wykorzystaj docelowe moduły Context po ich integracji; nie pozwól, żeby każdy mechanizm osobno dokładał swój pełny limit. Polityka jest w jednym module Rusta, a adaptery vendorów tylko dostarczają wynik.

Wejście konsumenta ma trzy warstwy:

1. **Obowiązkowy rdzeń w całości:** cel, zakres, wiążące wymagania i kryteria, istotne decyzje/ograniczenia, nierozstrzygnięte konflikty wpływające na zadanie oraz jednoznaczna tożsamość wersji. Wymagania nie mogą istnieć wyłącznie w pominiętym szczególe.
2. **Szczegóły dobrane do kroku:** wskazane sekcje, ich konieczne zależności i aktualne informacje potrzebne do wykonania. Krótki plan, który mieści się w przydziale, podaj cały — bez obowiązkowych wywołań narzędzi po kilka akapitów.
3. **Indeks pozostałej treści i działający odczyt na żądanie:** stabilne identyfikatory, opis i zakres, a nie sama nic nieznacząca ścieżka. Dłuższe uzasadnienia, stare raporty, dokumenty i obrazy pozostają dostępne w przyznanym zakresie.

Tę samą obowiązkową treść tej samej wersji renderuj identycznie dla implementera i QA, niezależnie od vendora. Szczegóły roli mogą się różnić. Preferencja sekcji nie jest uprawnieniem do wycięcia wspólnego wymagania.

Nie dobieraj zakresu tylko po nazwie kafelka, nie uruchamiaj dodatkowego selektora LLM przed każdym krokiem i nie dodawaj bazy wektorowej. Zachowaj oznaczenie przydzielonej treści wymaganej i opcjonalnej.

Budżet obejmuje razem materiały składane przez Loadout, z uwzględnieniem zadania, instrukcji, pamięci, narzędzi oraz rezerwy na wynik i dalszą pracę. Rozróżniaj kontrolowany przez aplikację limit dodatku od całego okna CLI: vendor może dołożyć własne instrukcje i historię, których rozmiaru Loadout nie zna.

Domyślne wartości dobierz na podstawie istniejących limitów Context i pomiaru opisanej dalej próby. Nie przepisuj bezmyślnie 24 KiB jako osobnego dodatku dla każdego źródła. Limity należą do kodu/configu czytanego przez UI, z jednym źródłem wartości. Raportuj bajty albo rzeczywisty pomiar tokenów; szacunek oznacz jako szacunek.

Jeśli wymagany rdzeń się nie mieści, nie obcinaj końcówki i nie zastępuj go „Moved to …”. Pokaż nazwany błąd przed startem dotkniętego konsumenta, z informacją o konieczności zmniejszenia/podziału zakresu. Gdy cały materiał jest znany przed biegiem, sprawdź to przed pierwszym procesem. Plan generowany podczas biegu waliduj po powstaniu i przed jego użyciem; nie udawaj, że jego treść była dostępna w początkowym preflight.

Wykorzystaj czytelnik Context do odczytu szczegółów planu albo dodaj cienką operację nad tym samym rdzeniem dostępu. Odczyt zawsze dotyczy przypiętej wersji. Obsłuż limity, paginację i niepoprawne ID. Gdy plan odwołuje się do obrazu, agent ma móc otworzyć rzeczywisty obraz przez transport Context; nie duplikuj importu ani budowania obrazów.

Nie przyznawaj całej biblioteki Context tylko dlatego, że wersja planu wymienia jakieś źródło. Referencje muszą rozwiązywać się do istniejących przydziałów/snapshotów, a wymagane niedostępne źródło daje jawny problem. Nie poszerzaj po cichu zakresu chronionego kroku ani zakresu sędziego/subjecta w Lab.

Cache jest optymalizacją kosztu, nie pamięcią między agentami. Utrzymuj deterministyczną kolejność i stabilny tekst wspólnych fragmentów tam, gdzie natywny transport na to pozwala; informacje zmienne nie powinny bez potrzeby psuć całego prefiksu. Nie przenoś w tym celu treści źródeł do instrukcji systemowych. Nie obiecuj współdzielenia cache między vendorami, modelami, worktree czy sesjami. Nie zakładaj, że ustawienia API są dostępne w zainstalowanych CLI.

Po wznowieniu nowej sesji kroku rdzeń trzeba dostarczyć ponownie. Dla natywnej kompakcji zbadaj wspierane mechanizmy klienta, zachowaj możliwość odczytu przypiętego planu i sprawdź zachowanie w próbie. Nie twierdź, że Loadout kontroluje każdą wewnętrzną turę i kompakcję CLI, jeśli nie ma takiej integracji. Nie dodawaj kopii całego planu przed każdym wynikiem narzędzia.

## 8. Przekazania i wyniki QA

Plan nie jest zwykłą notatką podlegającą `memory::handoff::BODY_CAP`. Zwykłe przekazanie zawiera zwięzły wynik wykonanej pracy, odniesienie do użytej/powstałej wersji, istotne odchylenia i otwarte problemy. Szczegółowy log pozostaje załącznikiem.

Napraw również ujawniony przypadek, w którym bardzo krótki wstęp powoduje, że całe merytoryczne sekcje znikają z krótkiego przekazania na rzecz samych wskaźników. Zachowaj istniejący limit i pełny załącznik; deterministycznie zachowaj użyteczną treść w dostępnym budżecie, oznacz pominięcia i nie zgub wymaganych pól/werdyktu. Nie nazywaj zwykłego uciętego fragmentu semantycznym streszczeniem. Nie dodawaj ukrytego agenta streszczającego każde przekazanie.

To jest ograniczona poprawka przekazania. Nie zamieniaj jej w domyślną tranzytywną propagację wszystkich wcześniejszych raportów ani przebudowę całego systemu pamięci. Workflow bez Plan zachowują dotychczasowe zależności, narzędzia i semantykę wykonania.

Runda poprawki dostaje wspólny plan, aktualny stan pracy i aktualne ustalenia QA. Stare szczegółowe raporty nie mają narastać w promptach jako kolejne kopie. Zachowaj nierozwiązane uwagi z poprzednich rund: brak wzmianki w nowszym raporcie nie dowodzi naprawy. Historia pozostaje dostępna, a powiązania opierają się na ID ustaleń/kryteriów.

Wykorzystaj istniejące `workflow/criteria.rs` i mechanizm oceny kryteriów. Nie twórz drugiego enuma wyników i drugiej reguły wyboru gałęzi. Plan = Use samo w sobie nie zmienia agenta w weryfikatora. Pobranie kryteriów z planu ma być jawną konfiguracją istniejącego sprawdzania wymagań, widoczną w jego panelu, a nie skutkiem nazwy QA.

Przy sprawdzaniu kryteriów planu:

- zamroź wybraną listę ID i treść z wersji użytej przez ocenianą pracę;
- zachowaj osobno wcześniej zatwierdzone kryteria workflow; konflikt nie może zostać cicho nadpisany planem;
- wymagaj przyporządkowania wyników do ID i dowodu/metody, zgodnie z istniejącym kontraktem;
- `passed`, `failed`, `not-tested` pozostają odrębne; brak pomiaru i brak odpowiedzi nie są zaliczeniem;
- nie utożsamiaj sukcesu procesu ze spełnieniem kryteriów;
- powiąż ocenę również z rzeczywistym wynikiem pracy/snapshotem/commitem, wykorzystując istniejący mechanizm pochodzenia plików. To samo ID planu przy innym kodzie nie dowodzi sprawdzenia właściwego produktu.

Kompletność raportu można sprawdzać mechanicznie. Prawdziwości dowodu i poprawnego zrozumienia wymagań nie dowodzi sam poprawny JSON ani odczyt narzędzia.

## 9. Trwałość, uprawnienia i błędy

Pliki są źródłem prawdy, SQLite odtwarzalnym indeksem. Plan i wersje przechowuj przy biegu, wykorzystując istniejące prywatne publikacje/snapshoty. Każdy dodany artefakt musi mieć konkretnego czytelnika: runtime, replay, UI albo test. Nie przechowuj drugiej niezależnej kopii aktualnego stanu wyłącznie w bazie.

Zadbaj o awarię zapisu, pełny dysk, uszkodzony plik, przerwanie między etapami publikacji, duplikat zgłoszenia, spóźniony wynik i restart. Poprzednia wersja pozostaje ważna po nieudanym Update, ale odbiorca wymagający wyniku tego Update nie dostaje jej jako cichego zastępstwa.

Use/Off nie mają uprawnienia do operacji publikacji. Update nie może zmienić wersji bazowej po stronie hosta ani pola pochodzenia. Agent nie wybiera ścieżki zapisu. Ścieżki/ID/kursory waliduj w backendzie, także przy bezpośrednim wywołaniu mostu.

Kanonu planu nie udostępniaj jako roboczego pliku do edycji. Wykorzystaj istniejące granice procesu i publikacji. Przy szerokim dostępie do dysku sam chmod lub brak narzędzia MCP nie stanowi pełnej granicy bezpieczeństwa: sprawdzaj integralność opublikowanych materiałów i nie uznawaj arbitralnej edycji pliku za dozwolone Update. Zamrożone wejście aktywnego konsumenta nie może się zmienić przez taką edycję. W chronionych krokach przetestuj także odmowę przez drogę plikową.

Anulowanie zachowuje istniejącą semantykę generacji i nadzoru procesów. Nie dodawaj globalnego boola, nowego supervisora ani platformowego kodu poza `engine/supervisor.rs`. Nie trzymaj `std::sync::Mutex` przez await. Timeout musi zakończyć własną grupę procesu i uzyskać dowód śmierci, zanim aplikacja ogłosi zakończone anulowanie.

Prompt runtime i sekrety dostarczaj zgodnie z AGENTS.md, bez logowania finalnego promptu. Trwały plan jest celowo zapisanym dokumentem użytkownika, a nie pretekstem do archiwizowania całej złożonej wiadomości do modelu.

## 10. Zapis workflow, zgodność i wszystkie ścieżki uruchomienia

Dodaj opcjonalną konfigurację kroku przez jeden typowany parser Rust i jego odpowiednik TypeScript. Brak pola oznacza Off. Dokumenty bez funkcji nie otrzymują sztucznie nowego pola przy odczycie/zapisie. Zachowaj nieznane pola zgodnie z otwartym formatem workflow.

**Starszy build nie może wykonać dokumentu, ignorując Plan.** Na sprawdzonym main format workflow wynosił 1; Context planuje wersję 2. Ustal rzeczywistą wersję po integracji CT. Jeżeli wydany czytnik tego formatu nie zna Plan, dokument z Plan musi wymagać kolejnego obsługiwanego formatu/już istniejącego skutecznego mechanizmu odmowy. Nie zakładaj, że format 2 automatycznie chroni dwie niezależnie dodane funkcje.

Zastosuj addytywne, idempotentne przejście i istniejącą kopię przed zmianą formatu. Nie migruj masowo starych dokumentów. Nieznany tryb Plan albo nowszy schemat może pozostać zachowany w dokumencie, ale nie może uruchomić się jako Off.

Sprawdź pełną drogę: edytor → stan frontu → IPC → zapis/odczyt → walidacja → zamrożony graf → fizyczny krok → stdin/MCP → publikacja → następny krok → historia. Uwzględnij duplikowanie kroku/workflow, remapowanie ID, import i spłaszczanie workflow. Podworkflow nie może wprowadzić niezauważenie drugiego Create.

Te same reguły muszą działać dla Startu z UI, Startu z Leada i uruchomienia w Lab. Lead zachowuje konfigurację w proponowanym grafie, a walidacja jej nie gubi. Nie dodawaj autonomicznego przepisywania planu przez każdą rozmowę Leada.

Wznowienie aktywnego biegu zachowuje opublikowane wersje i przypięcia. `Pick up here`, ponowienie konsumenta i odtworzenie jego zapisanych wejść korzystają z zachowanego planu, także po edycji/usunięciu biblioteki Context. Brak lub uszkodzenie wymaganej wersji daje jawną odmowę, bez odczytu „podobnego” planu z repo.

Rozróżnij dwa przypadki replay:

- odtworzenie zapisanych wejść konkretnego konsumenta ma przywrócić tę samą treść planu i przypięte źródła;
- ponowne wykonanie całego grafu z Create/Update ponownie generuje wyniki tych kroków. Może utworzyć inny plan mimo tych samych początkowych źródeł. Nie nazywaj tego odtworzeniem identycznego planu ani nie pomijaj autorów po cichu.

Wykorzystaj istniejący model replay i pokazuj ten wybór w jego obecnym podglądzie. Zmiana planu nie może zmienić trwającego biegu. Retencja nie może skasować wersji lub źródła używanego przez aktywny krok, rozpoczęte kopiowanie do replay czy przypadek Lab.

Lab ma dać porównywanym konsumentom identyczny zapisany plan i źródła. Jeżeli przedmiotem pomiaru jest sam generator planu, jego wynik jest zmienną pomiaru i musi być tak oznaczony. Zachowaj izolację odpowiedzi referencyjnych sędziego.

## 11. Podgląd wejścia i diagnostyka

Rozszerz istniejący panel `What this agent was told`, zamiast tworzyć drugi monitor tego samego faktu. Użytkownik ma zobaczyć:

- źródło planu, jego wersję i krok, który ją opublikował;
- sekcje faktycznie włączone do wejścia oraz ich rozmiar;
- szczegóły dostępne do odczytu;
- materiały skutecznie zwrócone przez kontrolowane narzędzia;
- pominięcia z powodu zakresu/budżetu i brak wymaganych materiałów;
- używaną wersję implementera obok wersji podstawy oceny, gdy ogląda powiązany wynik QA.

`Included`, `Available`, `Opened` to różne fakty. Otwarcie znaczy skuteczne zwrócenie danych, nie zrozumienie. Odczyt przez szeroki Bash poza instrumentowanym mostem może nie być obserwowalny; brak wpisu nie dowodzi braku odczytu. Historyczne manifesty bez informacji o dostawie pokazuj jako nieznaną metodę dostarczenia, a nie jako rzekomo pełne wstrzyknięcie treści.

Zachowaj kontrakt `SafeInputManifest`: dopuszczone rodzaje, względne referencje/ID, rozmiary i stan dostawy. Bez treści źródeł, pełnego promptu, hashów całego promptu, sekretów i absolutnych ścieżek gospodarza w bezpiecznym eksporcie. Integralność prywatnego dokumentu może korzystać z istniejącego mechanizmu prywatnych snapshotów; nie przenoś tego automatycznie do raportu diagnostycznego.

Odmowy są zrozumiałymi angielskimi zdaniami wskazującymi krok i naprawę, np. brak źródła, niejednoznaczna wersja, za duży wymagany plan, niedozwolona aktualizacja. Test musi asertować zdanie na rzeczywistej ścieżce UI, nie tylko wartość zwróconą przez niepodłączoną funkcję.

## 12. Miejsca integracji

To mapa odczytanego kodu, nie nakaz zmiany wszystkich plików. Potwierdź aktualne nazwy przed edycją.

| Obszar | Istniejące miejsca |
| --- | --- |
| Schemat, format i walidacja | `src-tauri/src/workflow/{mod,file,check,execution,unroll,criteria}.rs` |
| Stan i edytor | `src/state/workflows.ts`, `src/sections/workflows/{editor.tsx,io.ts,canvas/map.ts,step-panel/panel.tsx,step-panel/more-settings.tsx,step-panel/criteria-row.tsx}` |
| Wejście i przekazania | `src-tauri/src/commands/run.rs`: `handed_before`, `index_of_what_came_before`, przygotowanie fizycznego kroku |
| Skracanie przekazań | `src-tauri/src/memory/handoff.rs`: `BODY_CAP`, `cap`, publikacja pełnego załącznika |
| Context | aktualne `src-tauri/src/context/`; docelowe `commands/context_inputs.rs`, `commands/context_sources.rs`, `bridge/context.rs` z CT |
| Most i native CLI | `src-tauri/src/bridge/{mod,host,serve,verbs,messages}.rs`, `src-tauri/src/engine/drivers/{mod,claude,codex}.rs` |
| Snapshoty i historia | `commands/{run_inputs,input_snapshot,memory_sources,replay,history}.rs`, `commands/lab/workflow_sources.rs`, `lab/workflow_inputs.rs` |
| Publikacja i ochrona | `durable_file.rs`, `engine/supervisor.rs`, `commands/run/protection.rs` |
| Manifest i IPC | `src-tauri/src/evidence.rs`, `ipc.rs`, rejestry komend oraz produkcyjny czytelnik panelu wejść |
| Testy | `src-tauri/tests/it/main.rs`, istniejące moduły przekazań/replay/criteria, pliki testów panelu kroków, `e2e/harness.ts` |

Nową logikę trzymaj w niewielkim konkretnym module, np. `work_plan/` oraz `workflow/work_plan.rs`. Nie myl dokumentu roboczego z istniejącym wewnętrznym typem `Plan` w `commands/run.rs` albo planem Lab. W `run.rs` pozostaw cienkie wpięcia. Wspólne składanie wejść rozszerz w miejscu dostarczonym przez Context. Nie twórz traitu z jedną implementacją ani ogólnego systemu ośmiu rodzajów autorytetu.

## 13. Kolejność wykonania

Najpierw zapisz w tym katalogu krótki plan wykonawczy z rzeczywistymi ścieżkami, zależnościami CT i testem dla każdego etapu. To krok do wykonania implementacji, nie finalna odpowiedź. Oznaczenia WP poniżej porządkują zakres; nie przywracają starego systemu ręcznych `tasks/*.md`/OWNS.

| Etap | Zakres i wynik | Główna weryfikacja |
| --- | --- | --- |
| WP-01 | Model dokumentu, walidacja zmian, atomowa publikacja, historia wersji i czytelnik. | Aktualizacja Design zachowuje wymagania; konflikt rodzica i awaria nie niszczą starej wersji; restart nie wymaga DB. |
| WP-02 | Konfiguracja Plan w workflow i panelu, zgodność formatu, resolver zależności. | Round-trip przez prawdziwy IPC; stare dokumenty; odmowa starszego czytnika; konflikt autorów, kopie, pętle i wybór źródła widoczne na ekranie. |
| WP-03 | Create/Update przez istniejący most i zakończenie zwykłego kroku; Use z przypiętą wersją. | Pełny obrót przez oba produkcyjne adaptery, staging, brak poprawnego wyniku, anulowanie, podwójna publikacja. |
| WP-04 | Integracja z gotowym Context, wspólny budżet, rdzeń i odczyt szczegółów; poprawka cap. | Obowiązkowy tekst na stdin; szczegół przez rzeczywisty most; brak wycieku zakresu i braku mostu przy wyłączonych usługach. |
| WP-05 | Wspólny plan implementacji i oceny, istniejące criteria, historia uwag w pętli. | QA sprawdza tę samą wersję i wynik pracy; brak kryterium nie daje pass; stara nierozwiązana uwaga nie znika. |
| WP-06 | Historia, preview, resume/replay, import/duplikowanie, Lead i Lab. | Odtworzenie konsumenta po usunięciu biblioteki; zapisane wersje i źródła; prawdziwe komunikaty UI; bezpieczna diagnostyka. |
| WP-07 | Natywne scenariusze obu vendorów, pomiar jakości/kosztu, dokumentacja i finalna integracja. | MacOS/Tauri z prawdziwym backendem, zgodność przypadków odbioru, CI dla końcowego zintegrowanego kodu. |

Pracuj etapami przez bieżący `scripts/h run`, zgodnie z repo. Nie uruchamiaj równoległych etapów modyfikujących te same pliki ani dwóch ciężkich Cargo/rustc. Nie współdziel targetu między worktree. Pełne CI należy do lądowania; w pętli stosuj zawężone testy i wymagane checki.

Nie edytuj `harness/`, `checks/`, `scripts/`, `AGENTS.md` ani `docs/DECISIONS-LOCKED.md` w ramach zwykłego biegu. Jeżeli istniejąca wyrocznia rzeczywiście wymaga zmiany, przygotuj konkretny diff i uzasadnienie zgodnie z AGENTS.md §7; nie osłabiaj kryterium ani limitu gęstości. Rutynowe decyzje w powierzonym zakresie rozstrzygaj sam; nie pytaj po każdym etapie, czy kontynuować. Nie przypisuj dawnym zgodom na zmianę checków zakresu tego zlecenia.

## 14. Obowiązkowe scenariusze odbioru

Testy nowego zachowania mają najpierw uruchomić się i paść na starej implementacji/szkielecie, następnie przejść po zmianie. Nie zaliczaj błędu importu, kompilacji ani `0 passed` jako czerwieni/zieleni. Rust: moduły w `src-tauri/tests/it/`, dopisane do `main.rs`; wywołania `cargo test --test it <modul>::`. Front: konkretna ścieżka pliku Vitest. Nie twórz osobnych ciężkich binariów integracyjnych.

1. **Incydent end-to-end:** Plan → Update → Design Update → Implementation Use → QA Use. Krytyczne wymaganie backendowe występuje w rdzeniu, a nie w opisie Design. Oba ostatnie kroki dostają dokładnie to samo wymaganie i wersję. Nie muszą odkrywać pełnego planu po losowej ścieżce.
2. **Konflikt starej specyfikacji:** repo zawiera starsze ustalenie, plan jawnie wskazuje aktualne wymaganie. Wykonawca i ocena używają tej samej podstawy; niezatwierdzona propozycja pozostaje propozycją.
3. **Duży materiał:** drobny wstęp i długa sekcja nie powodują przekazania samych wskaźników. Wymaganie umieszczone na końcu planu nie znika. Nadmierny rdzeń odmawia startu konsumenta z rzeczywistym komunikatem UI.
4. **Update:** częściowa zmiana, `unchanged`, błędny schemat, nieaktualny rodzic, zmiana chronionego wymagania, zmiana niedozwolonej sekcji, powtórzone ID. Poprawna aktualizacja nie zmienia reszty.
5. **Uprawnienia:** bezpośrednie wywołanie publikacji z Use/Off jest odrzucone. Próba podmiany prywatnego pliku nie staje się nową wersją. Chroniony krok nie otwiera obcego źródła inną drogą.
6. **Graf:** brak Create, drugi Create, dwaj nieuporządkowani autorzy, brakujące/niejednoznaczne źródło, niedozwolona pętla autora, remapowanie ID przy kopiowaniu. Szkic zapisuje się, niedozwolony Start nie uruchamia modeli.
7. **Równoległość i pętla:** dwie kopie Use naprawdę nakładają się w czasie i korzystają z tej samej wersji. Naprawy dostają przypięty plan i właściwe uwagi, bez wzrostu pełnych starych raportów w każdej rundzie.
8. **Zmiana w trakcie:** nowy Update lub edycja źródeł nie zmienia już przygotowanego wejścia ani odczytu przypiętej wersji. Ocena starej pracy nie przełącza się na nowy plan.
9. **Awaria:** Stop/timeout przed zgłoszeniem, po zgłoszeniu i przy finalizacji, restart przy publikacji, brak miejsca, spóźniona próba, ponowione żądanie. Brak osieroconego procesu i fałszywie opublikowanej wersji.
10. **Wznowienie:** ponowienie konsumenta po restarcie i usunięciu Context nadal dostaje zapisane materiały. Uszkodzenie wymaganego snapshotu jest odmową. Pełne ponowne generowanie planu nie jest opisane jako identyczny replay.
11. **Właściwa ocena:** `exit 0` z niespełnionym wymaganiem nie daje zaliczenia; `not-tested` zachowuje istniejącą obsługę. Wynik wskazuje faktycznie oceniany plan i kod, nie aktualny HEAD innego worktree.
12. **Prawdziwe wejście:** asercje na bajtach odebranych przez kontrolowany proces za rzeczywistym adapterem i treści zwróconej przez most. Sam `input.json`, mock wyniku resolvera albo tekst w wygenerowanym pliku nie wystarcza.
13. **UI i trasy:** kliknięcie Plan, zapis, ponowne otwarcie, Start, wersja w panelu i odmowa na ekranie. Konfiguracja działa również z Leada i w Lab. Każdy nowy IPC ma wołającego i test pełnego obrotu.
14. **Zgodność i prywatność:** stare workflow Off działają; starszy czytnik odmawia nowego dokumentu; przyszły tryb nie wykonuje się jako Off; diagnostyka nie zawiera prywatnej treści/promptu. Legacy manifest nie udaje dowodu pełnej dostawy.

Użyj syntetycznych danych. Testy transportu mogą mieć kontrolowany proces vendora, ale osobno wykonaj natywną próbę macOS z prawdziwym Tauri, backendem oraz oboma CLI. Przeglądarkowy E2E z atrapą IPC nie jest natywnym testem produktu.

Próby żywych vendorów mają pokryć tworzenie, aktualizację, użycie i doczytywanie u każdego z nich oraz przekazanie Claude → Codex i Codex → Claude. Łącz scenariusze, żeby nie płacić wielokrotnie za ten sam dowód. Jeśli nie ma dostępnego vendora albo automatyzacji natywnego okna, oznacz konkretną lukę jako `not-tested`; nie zamieniaj jej w zaliczenie. Test ręczny musi podać wersję aplikacji/binarkę, SHA, CLI, model i wynik.

## 15. Pomiar oszczędności bez pogorszenia jakości

Przygotuj mały powtarzalny zestaw syntetycznych zadań z odpowiedzią referencyjną poza zakresem plików agenta. Uwzględnij: krótki plan, duży plan, ważne wymaganie pod koniec, sprzeczną starą specyfikację, aktualizację Design i rundę poprawki po QA. Nie kopiuj prywatnego biegu meetnotes do publicznych fixture'ów.

Porównaj na tych samych zamrożonych danych dwa warianty: cały plan oraz obowiązkowy rdzeń + dobrane szczegóły + odczyt na żądanie. Dla krótkiego planu rozsądny wynik kompozytora to cały dokument w obu wariantach. Przypadek przekraczający okno w wariancie pełnym oznacz jako niewykonalny, nie jako porażkę modelu.

Kontroluj vendor/model/ustawienia, wersje źródeł, konfigurację narzędzi i stan plików. Koszt mierz dla zakończonego zadania razem z doczytywaniem i poprawkami, a nie dla jednej wiadomości. Zapisz liczbę przypadków i powtórzeń; nie przedstawiaj jednego przejścia jako statystycznej gwarancji.

Mierz:

- poprawność wyniku i zachowanie obowiązkowych wymagań;
- pominięte warunki, fałszywe twierdzenia i liczbę rund naprawy;
- bajty wejścia, liczbę i objętość odczytów;
- rzeczywiste uncached input, cache read/write i output, gdy są raportowane;
- czas i koszt, gdy można go wiarygodnie ustalić.

Brak metryki to brak danych, nie zero. Nie dodawaj cache read drugi raz do wejścia, jeśli licznik vendora już je zawiera. Nie przeliczaj kosztu samodzielnie z jednej stawki dla wszystkich modeli. Cache nie zmniejsza rozmiaru materiału w oknie modelu.

Próby muszą respektować istniejące limity operacji/kosztów; nie uruchamiaj nieograniczonej macierzy wariantów. Niedokończony pomiar pozostaje jawnie niedokończony. Nie obiecuj procentu oszczędności przed pomiarem. Zmniejszenie promptu kosztem dodatkowych błędów lub rund naprawy nie jest optymalizacją.

Warunek odbioru: zero zgubionych obowiązkowych fragmentów w deterministycznej dostawie; wszystkie krytyczne scenariusze odbioru przechodzą; żywe próby potwierdzają pracę obu vendorów; brak niewyjaśnionej powtarzalnej regresji jakości względem pełnego planu na porównywalnych przypadkach. Raportuj wyniki jako pomiar tego zestawu, nie gwarancję dla każdego przyszłego zadania.

## 16. Podstawa doboru kontekstu

Poniższe materiały wyjaśniają kierunek. Specyfikacja produktu i wymienione kryteria są naszymi decyzjami, nie wymaganiami narzuconymi przez vendorów. Zweryfikuj aktualne możliwości zainstalowanych CLI przed dodaniem zależnych od nich ustawień.

- Anthropic opisuje mały, informacyjny kontekst, połączenie materiału dostarczonego z góry z doczytywaniem oraz ryzyko utraty istotnych informacji podczas kompakcji. To podstawa rdzenia, szczegółów na żądanie i pomiaru jakości: [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents).
- Dokumentacja Claude Code opisuje automatyczny prompt cache i znaczenie zgodności kontekstu. Nie oznacza to trwałej pamięci agenta ani gwarantowanego współdzielenia między uruchomieniami: [How Claude Code uses prompt caching](https://code.claude.com/docs/en/prompt-caching).
- OpenAI opisuje cache zgodnych prefiksów i korzyść ze stabilnego układu wspólnych materiałów. Możliwości API nie są automatycznie opcjami Codex CLI: [Prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching).
- Czytelne rozdzielenie instrukcji, materiału i oczekiwanego wyniku wspiera jednoznaczność wejścia: [Prompt engineering](https://developers.openai.com/api/docs/guides/prompt-engineering).

## 17. Kiedy zadanie jest skończone

Użytkownik może skonfigurować Create → Update → Use w edytorze, naprawdę wykonać taki workflow oboma vendorami, sprawdzić wersję i treść dostarczoną krokowi, przeprowadzić ocenę względem tej samej podstawy, bezpiecznie ponowić pracę i odtworzyć zapisane wejście. Context jest zintegrowany, a zwykłe workflow pozostają lekkie.

Zaktualizuj dokumentację zgodnie z rzeczywistym zachowaniem. Usuń szkielety, martwe kontrolki, nieużywane endpointy i doraźne obejścia zależności. Zakończ wszystkie WP, wykonaj wymagane checki oraz pełne CI przy finalnym lądowaniu przez istniejący harness. Nie publikuj wydania ani nie restartuj użytkownikowi jego aktywnej aplikacji w ramach samej implementacji.

Raport końcowy ma krótko podać: zachowanie dostępne w UI, commit/gałąź i stan integracji, testy z liczbą przejść, natywne scenariusze obu vendorów, rzeczywiste wyniki pomiaru oraz konkretne pozostałe ograniczenia. Rozróżniaj kod napisany, kod zintegrowany, funkcję sprawdzoną i wydaną. Jeżeli wymagany scenariusz pozostaje nieprzetestowany, nie nazywaj całej dostawy gotową produkcyjnie.
