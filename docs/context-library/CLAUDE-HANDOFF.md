Wykonaj pełną implementację opisanej poniżej funkcji Context w repozytorium `/Users/jakubgawronski/Projects/Loadout`, doprowadź ją do gotowości produkcyjnej i zintegruj zgodnie z procedurą repo.

To zlecenie wykonania, nie prośba o kolejny plan. Zakres i decyzje produktowe są już ustalone. Pracuj przez wszystkie etapy, naprawiaj wykryte problemy w tym zakresie i zakończ dopiero po spełnieniu kryteriów odbioru albo po wykazaniu konkretnego blokera, którego nie możesz usunąć w ramach uprawnień. Nie kończ na szkielecie, selektorze, częściowym MVP, mockach, zielonych jednostkach albo propozycji dalszych prac.

Masz w tej wiadomości cały potrzebny plan. Nie potrzebujesz poprzedniej rozmowy ani dostępu do plików pamięci Codexa. Jeśli `docs/context-library/PLAN.md` nie istnieje w Twoim checkoutcie, odtwórz go z pełnej specyfikacji zamieszczonej na końcu tej wiadomości. Ta specyfikacja jest projektem do wdrożenia, a nie stwierdzeniem, że wymienione funkcje już działają.

1. Najpierw przeczytaj aktualne `AGENTS.md`, `docs/DECISIONS-LOCKED.md` i `harness/README.md`. Zweryfikuj checkout, SHA, stan repo, otwarte biegi, worktree i aktywne procesy. Analizowano `main@315d209210dde34ae10ce92bd818bd22cfdd8d95`; jeżeli kod od tego czasu się zmienił, dostosuj miejsca integracji do bieżącego kodu, zachowując kontrakt produktu. Nie resetuj zmian użytkownika, nie przestawiaj istniejących biegów i nie zatrzymuj cudzych procesów.

2. Implementuj przez aktualny harness: `scripts/h run <id> --prompt ...`, a integruj przez `scripts/h land <id>`. Etapy CT-01–CT-09 wykonuj po kolei, na izolowanych worktree tworzonych przez harness. Używaj unikalnych ID i wznawiaj istniejący bieg zamiast uruchamiać duplikat. Nie przywracaj starych `ship-task.sh`, `integrate.sh`, ręcznych kontraktów `tasks/*.md` ani systemu `OWNS`. Nie uruchamiaj dwóch ciężkich Cargo/rustc równocześnie i nie współdziel targetu pomiędzy checkoutami.

3. Zlecenie obejmuje kod produktu, niezbędne zależności, typy, IPC, zapis na dysku, UI, oba adaptery i most narzędzi, testy, dokumentację oraz integrację. Nie pytaj ponownie o ustalone wybory: osobna sekcja `Context`, przypięcie przede wszystkim per krok, opcjonalne dziedziczenie workflow, Claude Code i Codex, tekst/Markdown/PDF/obrazy, trwałe źródła i niezmienne wersje. Rutynowe szczegóły implementacji rozstrzygaj zgodnie z aktualnym kodem i tym planem.

4. Nie rozszerzaj samodzielnie decyzji zablokowanych. Pliki `harness/`, `checks/`, `scripts/`, `AGENTS.md` i `docs/DECISIONS-LOCKED.md` pozostają chronione zgodnie z AGENTS.md §7. Jeżeli kompletna dostawa wymaga konkretnej zmiany takiego pliku, przygotuj jej zakres, pokaż powód i uzyskaj wymagane rozstrzygnięcie; nie omijaj bramki, nie podnoś baseline i nie przedstawiaj pominiętego sprawdzenia jako sukcesu. Tego wyjątku nie stosuj do zwykłych, już zamówionych zmian produktu i jego testów.

5. Każde nowe zachowanie dowiedź testem, który na starym zachowaniu uruchamia się i pada na asercji. Najpierw szkielet umożliwiający kompilację/import, później RED, potem implementacja i GREEN. Rust: wyłącznie moduły celu `src-tauri/tests/it/main.rs` i zawężone `cargo test --test it <modul>::`. Front: wskazane pliki vitest. Każde zaliczenie wymaga niezerowego licznika przejść. Nie utożsamiaj błędu zbierania testów, brakującej zależności ani niedostępnego vendora z poprawnym RED.

6. Sprawdzaj pełne drogi produkcyjne. W szczególności UI → IPC → AppState → resolver → konkretny krok/agenta, Cmd+V → trwały plik → restart → build → odczyt przez model oraz Lead → plan → Start z tymi samymi wersjami. Obraz musi dotrzeć jako obraz, nie jako nazwa ścieżki, podpis, OCR czy tekst zawierający base64. Wybrany kontekst nie może zniknąć w ponowieniu, kopii kroku, pętli, odtworzeniu historii ani Lab.

7. Zachowaj zakresy izolacji `executionInputs.contexts`. Nowe przypięcia materiałów nie mogą zmieniać grafu, otwierać cudzych źródeł ani nadawać krokowi możliwości Leada. Materiały użytkownika i złożony prompt są różnymi rzeczami: pierwsze zapisujemy prywatnie jako źródła, drugiego nie zapisujemy do logów ani argv. Rozdziel dane wejściowe, opracowanie AI, ważne wymagania użytkownika i instrukcje projektu. Samo udostępnienie źródła nie jest dowodem jego odczytania ani zrozumienia.

8. Uruchom rzeczywiste QA aplikacji na końcowym kodzie. Potrzebna jest izolowana prawdziwa aplikacja Tauri z backendem oraz żywe próby Claude Code i Codex. Przeglądarkowe testy z `e2e/harness.ts` są dodatkową warstwą i nie dowodzą natywnego schowka ani prawdziwego IPC. Brak potrzebnego narzędzia, logowania, kredytów, uprawnienia do sterowania oknem lub pomiaru oznacz jako `not-tested` i opisz konkretny bloker. Wymagane kryterium `not-tested` blokuje deklarację gotowości produkcyjnej. Nie wpisuj zaliczenia na podstawie intuicji ani zastępczego mocka.

9. Prowadź krótki dziennik `docs/context-library/IMPLEMENTATION.md`: etap, commit, RED/GREEN z liczbą wykonanych testów, natywne próby, wersje CLI/model, koszt jeśli znany, otwarte problemy oraz integracja. Dokument ma służyć kontynuacji po zmianie agenta lub utracie kontekstu. Statusy wymaganych kryteriów to `passed`, `failed` albo `not-tested`; nie przechodź do „gotowe”, dopóki istnieje wymagane kryterium w dwóch ostatnich stanach.

10. Po każdym etapie sprawdź kryteria właściwe dla zmiany, a pełne `scripts/ci.sh full` uruchamiaj przy lądowaniu przez harness. Po finalnych poprawkach wykonaj ponownie odpowiednie natywne próby na końcowym SHA. Końcowy raport ma zawierać: co działa z perspektywy użytkownika, końcowy SHA i stan integracji, liczniki testów i wynik pełnego CI, wynik natywnego QA osobno dla obu vendorów oraz wszystkie materialne ograniczenia. Zmierzona gotowość do wydania i publiczne opublikowanie wydania to osobne czynności; to zlecenie obejmuje implementację, weryfikację i integrację.

Nie zadawaj pytania „czy zaczynać” ani „czy kontynuować kolejny etap”. Zacznij od weryfikacji bieżącego checkoutu, następnie zrealizuj zakres. Jeżeli pojawi się rzeczywisty warunek STOP z AGENTS.md, wskaż konkretną regułę, wykonany test lub brakującą zdolność oraz najmniejsze rozstrzygnięcie potrzebne do dalszej pracy. Po jego usunięciu kontynuuj od zapisanych wyników.

Poniżej znajduje się pełna specyfikacja i plan wykonania.

---

**Context — pełny plan implementacji**

Data: 2026-09-07. Podstawa analizy: czysty `main` na `315d209210dde34ae10ce92bd818bd22cfdd8d95`.
Status: plan do wykonania; funkcjonalność i opisane niżej testy nie zostały jeszcze zaimplementowane ani uruchomione.

Uzgodniony produkt: biblioteka nazwanych zestawów tekstów, dokumentów i screenshotów; budowanie opracowania przez Claude Code albo Codex; **wybór per krok jako podstawowy sposób użycia**; opcjonalne wspólne zestawy workflow. Etykiety interfejsu są po angielsku. Ten dokument jest samodzielnym planem, nie promptem uruchamiającym agenta.

1. **Rezultat widoczny dla użytkownika**

   Użytkownik otwiera `Context`, tworzy np. `Checkout redesign`, wpisuje tekst, wkleja screenshoty przez Cmd+V albo dodaje pliki z dysku. Dopisuje cel materiałów i ważne wymagania. `Build context` przygotowuje krótkie opracowanie, tematy, odwołania do źródeł oraz pytania i sprzeczności. Użytkownik może je przeczytać, poprawić i używać w wielu workflow.

   W edytorze otwiera krok, wybiera `Context` i przypina jeden lub kilka zestawów, opcjonalnie tylko wybrane tematy. Frontend może dostać referencje wizualne, backend kontrakt API, a QA wymagania akceptacyjne. Wspólny brief można przypiąć na poziomie workflow i wyłączyć w konkretnym kroku.

   Agent od razu dostaje ograniczony objętościowo brief, ważne wymagania i indeks. Pełne treści oraz obrazy czyta na żądanie przez istniejący most Loadouta. W historii można odróżnić materiały podane na wejściu, dostępne do odczytu i faktycznie zwrócone przez narzędzie. Żaden z tych stanów nie oznacza automatycznie, że model poprawnie zrozumiał materiał.

   Samo przypięcie nie uruchamia dodatkowego agenta. Budowanie opracowania jest jawną operacją biblioteki, poza wykonaniem grafu. Jeżeli użytkownik chce analizować materiały podczas workflow, dodaje zwykły krok agenta z odpowiednim zadaniem. Nie powstaje typ kafelka `context` ani `enhance`.

2. **Zakres kompletnej dostawy**

   Dostawa obejmuje tworzenie, zmianę nazwy, wyszukiwanie, edycję, archiwizację i usuwanie zestawów; trwały import tekstu, Markdown, PNG, JPEG, WebP i PDF; tekst ze screenshotów oraz skanów rozpoznawany podczas budowania przez model widzący obrazy; podgląd źródeł; wersje opracowania; wybór vendora; anulowanie i ponowienie budowania; przypięcia per krok i workflow; Lead i planowanie; wykonanie, powtórzenie, historia oraz wejścia Lab.

   Obsługa GIF, HEIC, dokumentów Office, stron internetowych, synchronizacji między komputerami i automatycznego skanowania repozytorium nie jest częścią tej dostawy. Nieobsługiwany plik dostaje nazwany wynik importu. Nie wystarczy pokazać go jako dodany i pominąć podczas budowania. Zachowanie istniejącej rozmowy z obrazami, w tym jej obecnych formatów, pozostaje osobnym kontraktem.

   Biblioteka jest lokalna i dostępna w całej aplikacji. Sam zapis i podgląd działają bez vendora. `Build context` wymaga dostępnego agenta; brak logowania albo limit usługi jest wynikiem operacji, nie pustym opracowaniem. Biblioteka niczego nie wysyła automatycznie przy imporcie. Kliknięcie budowania albo uruchomienie kroku korzystającego z kontekstu przekazuje odpowiednie materiały wybranemu vendorowi; interfejs pokazuje, kto będzie je przetwarzał.

3. **Co już istnieje i gdzie należy się podłączyć**

   Poniższe ustalenia wynikają z odczytu aktualnego kodu, nie z historycznych paragonów CI.

   | Obszar                   | Obecne pliki                                                                                         | Znaczenie dla implementacji                                                                                                             |
   | ------------------------ | ---------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
   | Nawigacja                | `src/ui/sections.tsx`, `src/ui/screens.ts`, `src/ui/shell/nav-icons.tsx`, `src/ui/palette/`          | Rejestr sekcji, odkrywanie ekranów, ikony i skróty; nowy ekran musi być osiągalny naprawdę.                                             |
   | Obrazy z rozmowy         | `src/sections/run/entry/entry.tsx`, `images.ts`, `src-tauri/src/ipc.rs`                              | Istnieją paste, walidacja i wysyłka do Leada; brak trwałej biblioteki załączników.                                                      |
   | Transport obrazów        | `src-tauri/src/engine/drivers/{mod,claude,codex}.rs`                                                 | Claude i Codex mają obrazy w rozmowie; Codex używa tam App Servera, a zwykły krok używa `exec`. Nie utożsamiać tych dróg.               |
   | Most narzędzi            | `src-tauri/src/bridge/{mod,serve,host,verbs,messages}.rs`                                            | Most już istnieje dla Leada i kroków. `tool_result` obecnie emituje wyłącznie tekst; trzeba dodać typowany wynik obrazu.                |
   | Generowanie przez agenta | `src-tauri/src/commands/agent_generation.rs`, `library/agent_generation.rs`                          | Przykład rozdziału operacji, formatu wyniku i walidacji. Nie kopiować jego deklaracji izolacji: pusty cwd sam nie jest granicą odczytu. |
   | Dane workflow            | `src-tauri/src/workflow/{mod,execution,check,file}.rs`, `src/state/workflows.ts`                     | `executionInputs.contexts` oznacza izolację wykonania. Nowa biblioteka dostaje odrębne pole i parser.                                   |
   | Edytor                   | `src/sections/workflows/{editor,io}.tsx/ts`, `canvas/map.ts`, `step-panel/{panel,more-settings}.tsx` | Zachować zapis, duplikowanie i pięć podstawowych pól panelu.                                                                            |
   | Składanie wejścia        | `src-tauri/src/commands/run.rs`                                                                      | `prompt_for`, przygotowanie planu, `service_bridge_for`, `evidence_for_agent`; nowa logika ma żyć w małych modułach obok.               |
   | Zamrożone źródła         | `commands/memory_sources.rs`, `commands/replay.rs`, `commands/lab/workflow_sources.rs`               | Aktualny kod utrwala prywatne źródła i odtwarza je. Kontekst powinien zachować te same własności.                                       |
   | Lead i Start             | `commands/chat.rs`, `commands/lead_start.rs`, `bridge/library.rs`, `src/sections/run/io.ts`          | Przypięcie podczas rozmowy musi dotrzeć również do żądania uruchomienia.                                                                |
   | Trwały zapis             | `src-tauri/src/durable_file.rs`, `engine/supervisor.rs`                                              | Wykorzystać istniejące bezpieczne otwieranie plików i atomową publikację.                                                               |
   | Weryfikacja UI           | `e2e/harness.ts`, `src/sections/commands-wired.test.ts`                                              | Browser testuje prawdziwy frontend z atrapą IPC. Potrzebne są też backend i natywna próba macOS.                                        |

   `docs/PLAN-AGENTS-CONTEXT.md` jest historycznym planem przekazań i pamięci z sierpnia. Nie zastępuje tego zakresu. `Knowledge` nadal przechowuje umiejętności i pamięć; `Context` jest zamówioną osobną biblioteką materiałów wybieranych do pracy.

4. **Trwały model danych i wersje**

   Nowy konkretny moduł `src-tauri/src/context/` odpowiada za zestawy i ich treść. Nie powstaje drugi silnik pamięci ani baza wektorowa. Początkowo listowanie i wyszukiwanie korzystają z manifestów i przygotowanych tekstów. SQLite nie jest potrzebne do działania tej funkcji; ewentualny późniejszy indeks musi być odtwarzalny przez istniejący `store::writer`.

   Katalog biblioteki bierze się z `AppState.home`, a nie z odczytanego samodzielnie `$HOME`. Typowa ścieżka to `~/.loadout/contexts/context-<slug>-<id>/`. Slug jest pomocniczy, stały identyfikator powstaje w aplikacji. Zmiana tytułu nie zmienia tożsamości ani katalogu. Listowanie nie zakłada, że nazwa katalogu jest nazwą wyświetlaną.

   ```text
   contexts/context-checkout-<id>/
     manifest.json
     draft.json
     sources/<source-id>/<source-revision>/original.<ext>
     sources/<source-id>/<source-revision>/prepared.json
     sources/<source-id>/<source-revision>/pages/...
     versions/<revision-id>/manifest.json
     versions/<revision-id>/index.md
     versions/<revision-id>/topics/<topic-id>.md
     versions/<revision-id>/findings.json
     builds/<operation-id>/state.json
     builds/<operation-id>/results/...
   ```

   Każdy artefakt ma czytelnika: manifesty czyta resolver i UI; źródła oraz strony czyta podgląd i narzędzia kontekstu; `findings.json` czyta edytor, walidator i generator następnej wersji; stan budowania czyta odzyskiwanie oraz ekran postępu; wyniki partii czyta wznowienie budowania. `index.md` i tematy są renderowane przez aplikację z zaakceptowanych danych, nie pisane dowolnymi ścieżkami przez model.

   Minimalne byty:

   - `ContextSet`: schema, id, title, description, archived, draftRevision, latestReadyRevision, daty.
   - `ContextSource`: id, sourceRevision, rodzaj, bezpieczna nazwa wyświetlana, lokalna ścieżka względna, rozmiar i odcisk, status przygotowania, relacja tekst–obraz. Oryginalna ścieżka importu nie jest potrzebna do dalszego działania.
   - `ContextDraft`: kolejność źródeł, opisy, wyłączenia, instrukcje budowania, własne wymagania użytkownika. Edycje mają numer rewizji i zapis warunkowy.
   - `ContextRevision`: niezmienny identyfikator wersji, dokładne rewizje źródeł, wersja ekstraktora i instrukcji budowania, vendor/model użyty faktycznie, tematy, ustalenia, pokrycie oraz pytania.
   - `ContextFinding`: stabilny identyfikator, typ `requirement / fact / visual-reference / assumption / question / conflict`, treść, odwołania do źródeł, pochodzenie `human / generated`. Wymagania użytkownika zachowują dokładne brzmienie.

   Oryginały są niezmienne; podmiana źródła tworzy jego nową rewizję. Nowa wersja jest publikowana dopiero po sprawdzeniu kompletności wszystkich wskazanych plików. Zapis manifestu gotowej wersji i aktualizacja wskaźnika `latestReadyRevision` następują po publikacji treści. Awaria w połowie nie zabiera poprzedniej gotowej wersji.

   `Build context` automatycznie publikuje kompletny, poprawny strukturalnie wynik. Nie dokładamy obowiązkowego drugiego przycisku zatwierdzania. `Ready` oznacza gotowe opracowanie, a nie niezależnie zweryfikowaną prawdziwość zdań. Użytkownik może dopisać korektę lub edytować ustalenie; zapis tworzy kolejną wersję z oznaczeniem pochodzenia. Takie korekty pozostają wejściem następnych przebudów.

5. **Import, schowek i PDF**

   Wszystkie drogi importu kończą się tym samym rejestrem źródeł w Ruście. Cmd+V przechwytujemy tylko dla materiałów przeznaczonych do tego pola; zwykły tekst zachowuje normalną edycję. Wklejenie tekstu i obrazu razem zachowuje oba oraz ich powiązanie. Nie monitorujemy schowka w tle.

   Pliki wybrane z dysku są kopiowane do biblioteki, nie tylko linkowane. Usunięcie pliku z Downloads nie może zepsuć zestawu. Duże pliki są przesyłane lub kopiowane partiami; całej biblioteki nie przechowujemy w Zustand ani w pojedynczym base64 przez IPC. UI otrzymuje identyfikatory, metadane i podglądy ładowane na żądanie.

   Rust sprawdza limity, typ, granice ścieżek i rewizję operacji przed publikacją. Obrazy wymagają prawidłowego dekodowania i limitu wymiarów, nie tylko pasujących magic bytes. Przydatny będzie wąsko skonfigurowany dekoder `image` dla wspieranych formatów; jego wersję przypiąć w lockfile i sprawdzić koszt zależności. Oryginał zostaje zachowany, miniatura i wariant do modelu są oddzielnymi pochodnymi. Nadmierne zmniejszenie, które uniemożliwia odczyt detalu, jest widocznym ograniczeniem; nie oznaczamy takiego źródła jako rozpoznanego bez uwag.

   Do lokalnego przygotowania PDF proponujemy pakiet `pdfjs-dist`, dołączony wraz z workerem i wymaganymi zasobami do aplikacji. PDF.js udostępnia ekstrakcję tekstu i renderowanie stron przez publiczne API; nie trzeba instalować użytkownikowi zewnętrznego programu. [PDF.js: warstwy i pakowanie](https://mozilla.github.io/pdf.js/getting_started/), [API stron PDF](https://mozilla.github.io/pdf.js/api/draft/module-pdfjsLib-PDFPageProxy.html).

   Przebieg PDF: najpierw zapis oryginału w Ruście; następnie lokalny worker czyta zatwierdzone bajty, przygotowuje tekst z numerami stron oraz obrazy stron; Rust przyjmuje ograniczone wyniki związane z identyfikatorem importu i odciskiem źródła. Jedna strona jest renderowana naraz, a pamięć strony zwalniana. Kuracja, selekcja i publikacja opracowania pozostają w Ruście. PDF nie uruchamia osadzonych skryptów, załączników ani zewnętrznych odnośników. Worker i zasoby nie są pobierane z CDN.

   Przygotowanie jest osobną fazą importu: po zamknięciu okna niedokończony PDF pozostaje zapisany i ma stan `Needs preparation`. Przy ponownym otwarciu można dokończyć go od brakującej strony. Budowanie w backendzie korzysta wyłącznie z gotowych pochodnych; nie zależy od otwartego ekranu. Skan otrzymuje obrazy stron do analizy przez model. PDF mieszany zachowuje tekst i wygląd stron, żeby tekstowa ekstrakcja nie zgubiła diagramów. Zaszyfrowany lub uszkodzony plik dostaje precyzyjny stan, bez pozorowania pustego dokumentu.

   Import wielu plików pokazuje wynik każdego z nich. Identyczne bajty można deduplikować w obrębie zestawu, ale dwa różne podpisy albo różny cel użycia pozostają oddzielnymi powiązaniami. Niezapisywalny dysk, anulowanie, ponowione żądanie i spóźniony wynik muszą być rozliczane po identyfikatorze operacji.

6. **Budowanie opracowania przez agenta**

   Backend posiada operację, jej źródła, postęp i anulowanie. Przejście do innej sekcji nie kończy budowania. Zamknięcie aplikacji korzysta z istniejącego nadzoru procesów; po restarcie niedomknięta operacja staje się `Interrupted`, a nie `Ready`.

   Wybór vendora: ostatni jawny wybór, jeżeli nadal jest dostępny; jeżeli jest tylko jeden, wybieramy go; przy pierwszym użyciu i obu dostępnych domyślnie wskazujemy aplikację wybranego Leada, a przy braku Leada Claude Code. Selektor zawsze pokazuje wynik przed startem. Brak instalacji, nieudana sonda i brak logowania są różnymi stanami. `--version` nie dowodzi logowania, kredytów ani obsługi obrazów. Nie przełączamy vendora po cichu w połowie budowania.

   Model jest wybieralny w ustawieniach budowania; puste pole oznacza domyślny model danej aplikacji. Aktualny kod nie ma zweryfikowanego katalogu modeli, dlatego nie tworzymy fikcyjnej listy „dostępnych”. W metadanych zapisujemy model faktycznie zgłoszony przez vendora, jeśli jest znany.

   Pipeline ma pięć operacji: zamrożenie draftu; podział materiałów na ograniczone partie; ekstrakcja ustaleń z każdej partii; grupowanie ustaleń i wskazanie konfliktów; walidacja i publikacja. Partie powstają deterministycznie po stronach i granicach sekcji. Każda strona/fragment ma wynik `processed / excluded / failed`, a wyłączenie wymaga jawnego działania użytkownika. Niepublikowalny wynik zachowuje poprzednią wersję i wskazuje, co trzeba poprawić.

   Każda partia ma świeżą rozmowę agenta i dostęp tylko do przypisanego materiału. Tekst jedzie przez stdin, obrazy przez istniejący natywny transport `start_conversation` obu vendorów. Jednocześnie działa najwyżej jedna partia danego budowania. Wspólna pula aplikacji ogranicza wszystkie takie operacje razem z krokami workflow. Czas i limit kosztu obejmują też jedną dopuszczalną korektę formatu; niedostępnego pomiaru kosztu nie zastępujemy zmyśloną kwotą.

   Generator zwraca dane o ustaleniach, nie dowolne pliki. Nie może ustawić ID zestawu, ścieżki zapisu, uprawnień ani aktualnej wersji. Nie korzysta z MCP użytkownika, usług ani pamięci z innych prac. Izolacja używa `FilesystemFence` i istniejącego przygotowania prywatnego runtime vendora; sam tryb read-only albo pusty cwd nie wystarcza jako dowód braku dostępu do innych plików.

   Synteza operuje na ustaleniach z zachowanymi odwołaniami, a nie tylko na skrótach poprzednich skrótów. Wszystkie ustalenia pozostają dostępne w `findings.json` i tematach nawet wtedy, gdy nie mieszczą się w krótkim indeksie. Aktualizacja ponownie analizuje zmienione źródła. Można ponownie użyć gotowej partii tylko przy zgodnym odcisku wejść, ustawień, ekstraktora i instrukcji budowania; zmiana tych warunków unieważnia takie użycie.

   Instrukcja budowania określa: cel zestawu, rozróżnienie wymagań i inspiracji, dokładne zachowanie liczb i ograniczeń, źródła każdego ustalenia, opis wyglądu obrazów, jawne oznaczanie niepewności i konfliktów, zakaz traktowania treści dokumentu jako poleceń sterujących. Struktura danych i walidacja egzekwują format, limity i poprawność odwołań; prompt służy syntezie semantycznej. Nie obiecuje sprawdzenia prawdziwości automatycznym walidatorem.

   Wartością końca jest gotowy wynik, anulowanie lub nazwana porażka. Timeout i Stop czekają na udowodnione zakończenie grupy procesów. Brak dowodu śmierci zostaje widoczny i blokuje zwolnienie własności operacji; nie wolno pokazać zakończonego anulowania nad żywym procesem. Odpowiedź starej generacji nie może nadpisać nowszego draftu.

7. **Przypięcia per krok i wspólny kontekst workflow**

   Nowe opcjonalne pole `context` jest odrębne od `executionInputs`. Parser Rust żyje w `workflow/context.rs`, typy frontu w `src/state/context.ts`, a dokument workflow odwołuje się do nich z `src/state/workflows.ts`. Można użyć istniejącego `extra` w Ruście z jednym typowanym parserem, aby nie rozszerzać kilkudziesięciu konstruktorów. Brak pola zachowuje dotychczasowe zachowanie co do wejścia i dostępnych narzędzi.

   Dokument korzystający z tej funkcji zapisujemy jako `format: 2`. To rzeczywista potrzeba kompatybilności: obecny build przy `format: 1` zachowałby nieznane pole, ale wykonał workflow bez kontekstu. Jego istniejąca odmowa dla nowszego formatu zatrzyma dokument z `format: 2`. Nowy czytnik obsługuje formaty 1 i 2; stare dokumenty bez kontekstu pozostają w formacie 1 i nie wymagają masowej migracji. Rozdzielić najwyższy obsługiwany format od formatu potrzebnego konkretnemu dokumentowi. Aktualizacja zapisywanego dokumentu jest addytywna i atomowa, z kopią przed pierwszym faktycznym podniesieniem formatu. Dotyczy to także odczytu przez `load_snapshot` i walidacji grafów przekazanych bezpośrednio przez IPC, nie tylko przycisku Save.

   Przykładowy zapis fragmentów dokumentu:

   ```json
   {
     "format": 2,
     "context": {
       "schema": 1,
       "sets": [{ "id": "<brief-id>", "revision": "<revision-id>", "topics": "all" }]
     },
     "steps": [
       {
         "kind": "agent",
         "id": "frontend",
         "context": {
           "schema": 1,
           "inheritWorkflow": true,
           "exclude": [],
           "sets": [
             { "id": "<design-id>", "revision": "<revision-id>", "topics": ["<screens-topic-id>"] }
           ]
         }
       }
     ]
   }
   ```

   To fragment ilustrujący nowe pola, nie kompletny wykonywalny workflow.

   Reguły resolvera są jednoznaczne. Najpierw bierze zestawy workflow, jeśli dziedziczenie jest włączone, następnie usuwa wpisy z `exclude`, następnie nakłada wybory lokalne. Lokalny wpis tego samego zestawu zastępuje odziedziczony wybór tematów. Nie wykonuje ukrytej sumy tematów, która uniemożliwiałaby zawężenie. Dwa wpisy z tym samym ID w jednej liście są błędem. W jednej pracy różne wersje tego samego zestawu są odmawiane przed startem, aby plan i wykonanie nie opierały się na sprzecznych źródłach.

   Przypięcie wskazuje dokładną gotową wersję. Nowy build wyświetla `Update available`, ale nie zmienia zapisanych workflow ani rozpoczętych rozmów. `Update` pokazuje zmienione/usunięte tematy; utrata wybranego tematu wymaga nowego wyboru, nie automatycznego rozszerzenia na `all`. Tematy mają ID związane z wersją; nazw nie używamy do zgadywania równoważności.

   Wspólny kontekst dziedziczą wyłącznie kroki agentów. `Check`, punkt kontrolny i `Serve` nie otrzymują promptu ani nowych danych przez to pole. Nie zmieniamy semantyki ich stdin i przekazań. W chronionych `ContextScope` wspólne zestawy nie są dziedziczone automatycznie: przypięcia lokalne działają, a dziedziczenie trzeba włączyć w tym kroku jawnie. UI pokazuje efektywny wybór i przyczynę różnicy.

   Walidacja zapisu sprawdza kształt danych, ID, duplikaty i znane konflikty. Szkic z chwilowo niedostępnym zestawem można zachować, z ostrzeżeniem. Start odmawia przed pierwszym płatnym procesem, jeśli materiał wymagany przez uruchamianą część grafu jest brakujący, uszkodzony, niegotowy lub nie mieści się w limicie. Odmowa wskazuje krok i zestaw oraz drogę poprawy.

8. **Dostarczenie materiałów w trakcie pracy**

   `commands/context_inputs.rs` jest jednym miejscem rozwiązywania przypięć, zamrażania wersji i składania dodatku do promptu. `commands/context_sources.rs` zapisuje i odtwarza prywatny pakiet źródeł konkretnego biegu, zgodnie z wzorcem `memory_sources.rs`. Nie dokładać osobnych compositorów dla Leada, Codexa, Claude'a i Lab.

   Przed startem powstaje mapa `node_key -> wybrane wersje, tematy, źródła`. `copies` i rundy zachowują wybór kafelka, ale każdy fizyczny odbiorca ma własny adres odczytu i rachunek dostarczenia. Bieg otrzymuje własną kopię potrzebnych, niezmiennych materiałów. Nie zależy później od obecnej biblioteki. Kopiujemy strumieniowo, sprawdzając odciski i dostępne miejsce; nie używamy zapisywalnych hardlinków do źródeł.

   Do promptu wchodzą: cel zestawu, wybrane wymagania oznaczone jako ważne, krótki indeks tematów i źródeł oraz instrukcja doczytywania materiałów potrzebnych do zadania. Ta część ma jawny nagłówek materiałów referencyjnych i nie trafia do `system_append`. Polecenie użytkownika, instrukcje projektu i kontrakt odpowiedzi zachowują własne role. Sprawdzać końcowy prompt, nie tylko funkcję produkującą dodatek.

   Dostęp na żądanie realizuje istniejący most `mcp__loadout`, rozszerzony o cztery ograniczone narzędzia: `list_context`, `search_context`, `read_context`, `view_context_image`. Wszystkie korzystają z tego samego `ContextAccess`, związanego przez hosta z konkretnym odbiorcą i wersją pakietu. Model podaje tylko ID tematu/źródła i kursor; nie może podać dowolnego katalogu, innego kroku ani obcego zakresu.

   `read_context` zwraca ograniczony fragment tekstu, adres źródła i kursor dalszego odczytu. `search_context` szuka tylko w przydzielonych tematach i tekstach źródłowych, z deterministyczną kolejnością i ograniczoną liczbą wyników. Na początek wystarczy lokalne wyszukiwanie tekstowe. Wybór materiałów nie uruchamia kolejnego modelu wyszukującego.

   `view_context_image` zwraca rzeczywisty blok obrazu MCP z MIME i bajtami, razem z podpisem i odniesieniem do źródła. Specyfikacja MCP definiuje taki typ wyniku; dotychczasowy tekstowy `Answer::Ok` trzeba rozszerzyć addytywnie o typowany wynik multimedialny. Nie wolno serializować obrazu do zwykłego tekstu JSON i nazwać tego obsługą obrazu. [MCP: wynik narzędzia z obrazem](https://modelcontextprotocol.io/specification/2025-06-18/server/tools).

   To jest wybrana droga obrazów kontekstu w krokach: Codex zachowuje `exec`, Claude swój dotychczasowy transport; obrazy docierają jako wynik odczytu narzędzia. Nie przepinamy całego workflow na transport rozmowy. Oba warianty trzeba udowodnić żywymi CLI z obrazem, którego istotnego szczegółu nie ma w podpisie. Rozmowa Leada i budowanie mogą nadal korzystać z istniejącej natywnej pierwszej wiadomości z obrazami.

   Przyznanie kontekstu nie przyznaje narzędzi Leada, uruchamiania workflow, usług ani wiadomości do innych agentów. W `service_bridge_for` trzeba usunąć założenie, że brak usług i wiadomości oznacza brak mostu: krok posiadający wyłącznie kontekst także potrzebuje swoich narzędzi odczytu. Lista narzędzi i dispatcher muszą wynikać z tego samego zestawu uprawnień.

   Pełna biblioteka i cały pakiet biegu nie są podawane jako globalne `extra_dirs`. W chronionych krokach odczyt przez inne narzędzia jest ograniczony istniejącą granicą procesu. W zwykłym trybie przypięcie kontroluje dostarczanie materiałów przez Loadout; nie jest obietnicą, że agent z szerokimi uprawnieniami do dysku nie może sam odnaleźć innych plików. Wybór tematów w narzędziach nigdy nie odsłania całego PDF zawierającego również nieprzydzielone strony: serwujemy wybrane pochodne i zakresy, bez obchodzenia wyboru przez odczyt oryginału.

9. **Budżety i ochrona jakości**

   Limity poniżej są wartościami startowymi do zmierzenia, nie deklaracją optymalnej jakości wszystkich modeli. Rust jest źródłem konfiguracji limitów dla UI. Nie przeliczamy bajtów na tokeny z pozorną dokładnością.

   | Obszar                       | Wartość startowa                                                             | Zachowanie po przekroczeniu                                                    |
   | ---------------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
   | Zestaw                       | 200 źródeł, 512 MiB oryginałów, osobno 512 MiB pochodnych                    | Nazwana odmowa dodania; istniejące źródła zostają.                             |
   | Pojedynczy plik              | 50 MiB; wklejony tekst 2 MiB                                                 | Można podzielić materiał lub wybrać mniejszy plik.                             |
   | PDF                          | 200 stron, jedna renderowana naraz                                           | Wybór zakresu przed przygotowaniem; wyłączone strony widoczne.                 |
   | Obraz do analizy             | Obecny sufit 5 MiB/obraz; maks. 16 mln pikseli przy dekodowaniu              | Mniejszy wariant albo jawna odmowa, z zachowaniem oryginału.                   |
   | Partia budowania             | Do 24 KiB tekstu i limitów `ValidatedImages`: 4 obrazy / 12 MiB łącznie      | Kolejna partia, bez odcinania końcówki.                                        |
   | Jedno uruchomienie budowania | Domyślnie 20 minut całej operacji; limit kosztu użytkownika, jeśli mierzalny | Udowodniony Stop i możliwość kontynuowania gotowych partii.                    |
   | Zestawy jednego kroku        | Maks. 8 po rozwiązaniu dziedziczenia                                         | Wybór mniejszej liczby zestawów.                                               |
   | Dodatek do promptu           | Łącznie 24 KiB dla wszystkich zestawów danego kroku                          | Najpierw wymagania; reszta jako indeks. Zbyt duży blok wymagań odmawia startu. |
   | Odczyt tekstu                | Do 16 KiB na wynik, z kursorem                                               | Dalszy odczyt po kursorze.                                                     |
   | Wyszukiwanie                 | Do 10 wyników i 8 KiB odpowiedzi                                             | Kolejna strona wyników.                                                        |
   | Odczyt obrazu                | Jeden obraz do 5 MiB na odpowiedź                                            | Kolejne obrazy osobnymi odczytami.                                             |

   Krótki indeks zestawu ma docelowo do 2 KiB, a pełna treść zawsze zostaje dostępna. Ważne wymagania nie są wycinane, parafrazowane ani degradowane do ścieżki tylko po to, żeby zmieścić się w budżecie. Deduplikacja używa tożsamości ustalenia i dokładnych źródeł; podobne zdania o różnych warunkach nie są automatycznie jednym wymaganiem.

   Limit dodatku nie jest limitem całego okna modelu. Preflight uwzględnia już obecne instrukcje, pamięć, zadanie i przekazania; przy znanym limicie modelu zostawia rezerwę na narzędzia i odpowiedź. Przy nieznanym pokazuje rozmiary bez twierdzenia, że cały prompt na pewno się zmieści. Narzędzia ograniczają pojedyncze odpowiedzi, ale wiele odczytów nadal zużywa kontekst; rejestrujemy łączny odczyt, a nie obiecujemy rozwiązania problemu samą paginacją.

   Krótkie wejście i doczytywanie szczegółów są zgodne z opisywanym przez Anthropic podejściem do zarządzania kontekstem. Mają jednak koszt kolejnych odczytów, dlatego trafność, czas i zużycie trzeba zmierzyć na zadaniach Loadouta. [Anthropic: context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents).

10. **Lead, plan i uruchomienie**

    W polu rozmowy Leada pojawia się ten sam picker zestawów. Wybrane materiały należą do konkretnej rozmowy/terminala oraz workspace, nie do globalnego stanu wszystkich czatów. Można przygotowywać plan z zestawów przed stworzeniem workflow.

    Lead domyślnie widzi wyłącznie jawnie przypięte zestawy i materiały wskazanego workflow. Otwarcie listy workflow nie uprawnia go do czytania wszystkich kontekstów biblioteki. Właściciel wskazuje, czy zestawy rozmowy mają stać się wspólnym kontekstem uruchamianego workflow, czy trafić do wybranych kroków. Propozycja Leada nie aktualizuje na własną rękę trwałego pliku workflow; ewentualne przypięcia tylko dla tego uruchomienia są widoczne w istniejącym podglądzie Startu i utrwalone w zapisie biegu.

    Gdy plan powstaje jako zwykły krok grafu, jego przypięcia są widoczne jak w każdym kroku. Preflight zestawia jego materiały z materiałami wykonawców. Nie wprowadza się heurystyki opartej na nazwie `Plan` ani specjalnej gałęzi w schedulerze. Jeśli ktoś skonfigurował planowanie bez wymagań potrzebnych wykonawcom, UI pokazuje tę różnicę; nie twierdzimy, że system zna semantycznie każdy etap.

    Żądanie Startu z Leada wiąże rewizję workflow i mapę wersji kontekstów, na której pracowała rozmowa. Zmiana wyboru między podglądem i startem unieważnia ten podgląd. Nowa wersja w bibliotece nie jest takim problemem, jeżeli wybrane stare wersje nadal istnieją i są poprawne. Brak możliwości odczytu wybranej wersji daje odmowę, nigdy automatyczne `latest`.

    Zmiana przypięć w istniejącej rozmowie nie usuwa starych wiadomości z kontekstu vendora. UI jasno to zaznacza i udostępnia `Start a new conversation` z aktualnym wyborem. Przed Startem po zmianie materiałów pokazuje `Context changed since this plan`; można zachować poprzedni wybór lub ponownie omówić plan z nowym. Nie udajemy mechanicznego dowodu, że swobodny tekst planu został semantycznie zaktualizowany.

11. **Historia, ponowienia, Lab i sprzątanie**

    Run zapisuje wybrane wersje, odbiorców, rozmiary oraz odnośnik do prywatnego pakietu źródeł. Źródła są danymi użytkownika celowo utrwalonymi do pracy. Finalny złożony prompt nadal nie jest archiwizowany ani hashowany w `SafeInputManifest`. Ten podział jest zgodny z obecnym zapisem prywatnych źródeł pamięci.

    `ContextKind` dostaje addytywny wariant dla opracowania, a historia odczytów osobny ograniczony zapis metadanych. Do UI trafiają nazwa zestawu, wersja, źródło/temat, liczba bajtów i rzeczywisty stan: `Included`, `Available`, `Opened`, `Not included`. `Opened` wolno wystawić po skutecznym zwróceniu treści/obrazu, nie po samym zamiarze odczytu. Panel `What this agent was told` pokazuje te dane w jednym miejscu. Nie przechowujemy kopii każdego odczytanego tekstu w dodatkowych logach.

    Powtórzenie z zapisanych wejść bierze historyczny pakiet, także po zmianie/usunięciu biblioteki. Powtórzenie z obecnych ustawień stosuje obecny dokument workflow, w tym jego jawnie przypięte wersje; nie oznacza niejawnego przejścia na najnowszy kontekst. Częściowe ponowienie, pętle, `copies`, `Pick up here` i wznowienie po restarcie przechodzą przez ten sam resolver i zachowują adresy fizycznych kroków. Uszkodzony pakiet historyczny uniemożliwia powtórzenie z tych wejść; nie staje się pustym kontekstem.

    Lab zamraża materiał na poziomie przypadku i przekazuje wariantom tę samą wersję źródeł. Zmiana porównywanego modelu nie może pośrednio zmienić wejścia. Sędzia i subject zachowują istniejące zakresy; zestawy subjecta nie rozszerzają automatycznie dostępu do odpowiedzi referencyjnych sędziego.

    `Archive` ukrywa zestaw z domyślnej listy, zachowując przypięcia. `Delete` pokazuje użycia w workflow i informuje o zachowanych kopiach historycznych. Usunięcie nie może uszkodzić aktywnego budowania ani rozpoczętego biegu; aktywny czytelnik korzysta z własnych zamrożonych danych. Po usunięciu zestawu przyszły zwykły Start z nieaktualnym przypięciem odmawia i wskazuje naprawę.

    Pierwsza wersja nie usuwa samodzielnie gotowych rewizji biblioteki. Tymczasowe, nieopublikowane dane budowania można sprzątać po udowodnionym zakończeniu operacji. Pakiety biegów podlegają retencji biegów; aktywne użycie i kopiowanie do Lab/powtórzenia chronią je przed wyścigiem z usuwaniem. Standardowy raport diagnostyczny zawiera tylko dopuszczone metadane, nigdy oryginały dokumentów, obrazy czy pełne fragmenty. Osobny eksport całej biblioteki nie jest wymagany do tej dostawy.

12. **Interfejs i IPC**

    `Context` dostaje jedną pozycję w grupie `Know`, obok `Knowledge`, bez blokady zależnej od posiadania agentów. Lista ma wyszukiwanie, filtr aktywnych/archiwalnych, nazwę, krótki opis i jeden stan pozycji. Nie pokazujemy pełnej zawartości wszystkich zestawów naraz. Zmiana liczby sekcji musi obejmować skróty numeryczne, paletę, makietę i istniejące testy nawigacji; liczby siedem nie podmieniamy tylko w jednym komponencie.

    Edytor zestawu ma `Sources` i `Overview`. Źródła, notatki i pole `How should this context be prepared?` są edytowalne przed pierwszym buildem. Jedno główne działanie odpowiada stanowi: `Build context`, `Stop`, `Rebuild context`. Stan przygotowania plików jest odrębny od dostępności ostatniej gotowej wersji, np. można mieć gotową wersję 2 i nowe źródła czekające na przebudowę. Błąd nowego buildu nie zmienia starej wersji na zepsutą.

    W panelu kroku `Context` jest zwijanym wierszem w istniejących dodatkowych ustawieniach, z efektywnym wyborem `From workflow` / `Added to this step`. Nie dodajemy szóstego stale rozwiniętego pola obok istniejących pięciu. Wiersz pokazuje wybór bez otwierania wielkiego edytora; picker ma wyszukiwanie i możliwość zawężenia tematów. Workflow ma analogiczny picker wspólnego kontekstu. Żadne strzałki ani dodatkowe kafelki nie reprezentują samych załączników.

    Rust zwraca gotowe angielskie komunikaty dotyczące stanu i odmów. Front nie wyciąga sensu z surowego błędu vendora. Używamy istniejących tokenów, komponentów i zasad gęstości; nowe ekrany mają sceny pomiarowe, a baseline nie rośnie.

    Przewidywane komendy IPC: `list_context_sets`, `read_context_set`, `create_context_set`, `save_context_draft`, `import_context_sources`, `complete_context_source_preparation`, `read_context_source`, `remove_context_source`, `build_context`, `stop_context_build`, `read_context_build`, `save_context_revision`, `archive_context_set`, `delete_context_set`, `resolve_workflow_context`. To lista kontraktów do wdrożenia wraz z czytelnikami UI, nie zgoda na wystawienie martwych endpointów. Zapis przyjmuje oczekiwaną rewizję; długie operacje mają ID i kanał zdarzeń oraz odczyt stanu po ponownym wejściu. Podgląd pliku używa ID zatwierdzonego źródła, nie arbitralnej ścieżki od frontendu.

13. **Kolejność implementacji i kryteria poszczególnych etapów**

    Oznaczenia CT służą tylko temu planowi. Nie przywracają starego systemu `tasks/*.md` ani `OWNS`. Każdy etap ma wąski bieg `scripts/h run`, test zachowania uruchomiony przed poprawką i kompletną ścieżkę produkcyjną. Nie uruchamiać etapów równolegle bez osobnego polecenia właściciela. Integracja i ciężkie Cargo są zawsze pojedyncze.

    | Etap  | Zależności   | Konkretny wynik                                                                   |
    | ----- | ------------ | --------------------------------------------------------------------------------- |
    | CT-01 | —            | Trwały zestaw tekstowy dostępny przez nowy ekran.                                 |
    | CT-02 | CT-01        | Wklejanie obrazów, import z dysku, przygotowanie PDF i podgląd.                   |
    | CT-03 | CT-02        | Ograniczony czytelnik kontekstu i rzeczywiste obrazy przez most MCP obu vendorów. |
    | CT-04 | CT-02, CT-03 | Budowanie, postęp, anulowanie, korekty i publikacja wersji.                       |
    | CT-05 | CT-04        | Zapis i edycja przypięć workflow/per krok, jeden resolver.                        |
    | CT-06 | CT-03, CT-05 | Zamrożone materiały w prawdziwych krokach, kopiach i pętlach.                     |
    | CT-07 | CT-06        | Lead planuje z kontekstem i przekazuje wersje do Startu.                          |
    | CT-08 | CT-06, CT-07 | Historia, powtórzenia, Lab, archiwizacja i retencja.                              |
    | CT-09 | CT-08        | Natywne QA, pomiar jakości i odbiór kompletnej funkcji.                           |

    **CT-01.** Nowe `context/{mod,files}.rs`, `commands/context.rs`, `src/state/context.ts`, `src/sections/context/{index,io,editor}.tsx/ts`; rejestracja w `commands/mod.rs`, `lib.rs`, `ipc.rs`, nawigacji i palecie. Test `context_library_survives_restart::`: prawdziwy zapis tekstu, odtworzenie bez SQLite, zmiana nazwy bez zerwania ID, odrzucony spóźniony zapis, niekompletna publikacja niewidoczna jako gotowa. Browser `context-library.spec.ts`: nowa pozycja menu, utworzenie, powrót do zapisanego materiału. RED ma dotyczyć utraty/nieobecności zachowania, nie brakującego importu modułu.

    **CT-02.** `context/{sources,limits}.rs`, `src/sections/context/{source-editor,source-preview,pdf-preparation}.tsx/ts`, wąski wspólny helper schowka jeżeli potrzebny, `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `Cargo.lock`, w razie potrzeby `vite.config.ts`, `src-tauri/tauri.conf.json` i odpowiednie capabilities. Test `context_source_import::` i `context-sources.spec.ts`: mieszany paste, prawdziwy obraz, usunięcie oryginału z miejsca importu, błędne MIME, za duże wymiary, kilka plików z jednym błędem, PDF tekstowy/skan/mieszany, przerwanie i dokończenie przygotowania. Natywna próba musi dowieść rzeczywistego Cmd+V z macOS, a build produkcyjny działania workera bez sieci.

    **CT-03.** `context/access.rs`, `bridge/context.rs`, rozszerzenie `bridge/{mod,serve,verbs,host}.rs`, połączenie czytelnika z podglądem źródeł w UI, następnie z izolowaną operacją agenta. Testy `context_reader_is_scoped::` oraz `context_image_reaches_vendor::`: tekst z kursorem bez utraty/duplikacji, obraz jako blok obrazu na końcu pełnego obrotu przez most, odmowa sfałszowanego ID/kursora/zakresu, wygasłe uprawnienie, limit ramki i zerwany odbiorca. Żywe próby obu CLI muszą rozpoznać szczegół obrazu nieobecny w nazwie i tekście. Jeżeli wykryta konfiguracja vendora nie przyjmuje obrazu, poprawić w tym zakresie transport/konfigurację; nie zastępować zdjęcia opisem i nie zaliczać etapu samym `tools/list`.

    **CT-04.** `context/{build,findings,prompt}.rs`, `commands/context_build.rs`, `ipc.rs`/`AppState`, `src/sections/context/{build-controls,overview}.tsx`, korzystanie z istniejących `Limiter`, `AgentDriver`, `AgentHandle`, `FilesystemFence` i publikacji. Test `context_build_publishes_complete_version::`: dwa vendory przez rzeczywiste adaptery z kontrolowanym procesem testowym, kompletność źródeł, błędny JSON i jedna korekta, konflikt zachowany, opóźniony wynik, restart, Stop/timeout z dowodem śmierci, brak vendora. Żywy test obu vendorów obejmuje tekst i obrazy. Strukturalnie poprawny fikcyjny JSON nie dowodzi jakości syntezy.

    **CT-05.** `workflow/context.rs`, parser w `workflow/{check,file}.rs`, `src/state/workflows.ts`, `step-panel/{panel,more-settings,context-row}.tsx`, `editor.tsx`, `canvas/map.ts`, testy zapisu. Test `workflow_context_selection::` i `context-selection.spec.ts`: per krok, wspólny zestaw, zastąpienie tematów lokalnym wyborem, wyłączenie odziedziczonego, konflikt wersji, scope chroniony, duplikowanie workflow/kroku i round-trip. Brak pola ma zachować stare dokumenty. Sprawdzić odmowę dokumentu z kontekstem w starszym czytniku, odczyt obu formatów przez nowy `load_snapshot` i brak podniesienia formatu dokumentów bez kontekstu. Odmowy są asertowane w faktycznym panelu, w tym zachowanie szkicu z brakującą wersją.

    **CT-06.** `commands/{context_inputs,context_sources}.rs`, cienkie wpięcia w `run.rs`, `run/protection.rs`, `bridge/messages.rs`, `evidence.rs`, mapowanie stanów w `engine/line.rs` i panelu wejść. Testy `step_receives_selected_context::`, `context_does_not_change_during_run::`, `context_budget_preserves_requirements::`: produkcyjny Start, dwa kroki o różnych przydziałach, `copies`, runda ponowienia, działający most mimo wyłączonych usług/wiadomości, zachowany fan-in i kontrakt odpowiedzi. Dwa równoległe kroki muszą nakładać się w czasie. Edycja/usunięcie biblioteki po starcie nie zmienia odczytów. Przekroczenie ważnych wymagań albo brak źródła zatrzymuje start przed pierwszym procesem i daje widoczne zdanie. Chroniony krok nie czyta cudzych źródeł ani przez most, ani przez dysk.

    **CT-07.** `commands/{chat,lead_start}.rs`, `bridge/library.rs` i `bridge/library/context.rs`, `ipc.rs`, `src/sections/run/{io,index}.ts/tsx`, picker przy polu rozmowy oraz podgląd Startu. Test `lead_context_reaches_run::` i `lead-context.spec.ts`: UI → IPC → `AppState` → rozmowa → żądanie Startu → konkretny krok; dwie rozmowy/workspace'y z różnym wyborem; zmiana wersji po planowaniu; nowe przypięcie tylko dla tego uruchomienia; brak samowolnego zapisu workflow. Stary transcript nie jest przedstawiany jako oczyszczony po odpięciu materiału.

    **CT-08.** `commands/{history,replay,context_sources,sweep}.rs`, `commands/lab/workflow_sources.rs`, `lab/workflow_inputs.rs`, `bridge/library/replay.rs`, istniejący `run::retention_blocker` oraz panele historii/Lab. Test `recorded_replay_uses_frozen_context::`: powtórzenie bez biblioteki, uszkodzony lub podmieniony plik, częściowe powtórzenie, materiał skopiowany do Lab przed retencją, równoczesny odczyt i usuwanie. Test `context_history_reports_delivery::`: dostępne ≠ otwarte, obraz rzeczywiście zwrócony, limitowane metadane, brak treści w raporcie diagnostycznym. Archiwizacja zachowuje działające przypięcia; usuwanie przyszłego wejścia daje czytelną odmowę.

    **CT-09.** Nowe moduły testów w `src-tauri/tests/it/`, przypadki w `e2e/tests/`, syntetyczne źródła w `e2e/fixtures/context/`, uzupełnienie tego planu rzeczywistymi wynikami oraz dokumentacji użytkowej/architektury. Przejść scenariusze z punktu 14, sprawdzić gęstość na prawdziwych ekranach i wykonać pełne CI przy lądowaniu. Zmiany `docs/ARCHITECTURE.md`, `docs/FOUNDATIONS.md`, `docs/design/DESIGN.md` i `docs/mockup/index.html` mają opisywać rzeczywiście dostarczony produkt.

    W każdym etapie dodającym IPC aktualizować `src/sections/commands-wired.test.ts`, `src-tauri/commands.golden.txt` i odpowiednie kryteria obrotu, które czytają aktualny rejestr. Dokładne istniejące ścieżki potwierdzić przed biegiem; nie dopisywać drugiego rejestru. Każdy nowy rustowy moduł testowy dopisać w `src-tauri/tests/it/main.rs`.

14. **Odbiór całej funkcji**

    | Scenariusz            | Wymagany dowód                                                                                                                                               |
    | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
    | Trwały paste          | W prawdziwym oknie macOS wklejenie tekstu i screenshotu, restart, obraz i powiązanie nadal dostępne.                                                         |
    | Wiele źródeł          | Minimum 50 mieszanych źródeł, w tym duży tekst; każde ma wynik przetwarzania, nie ma cichej końcówki poza limitem.                                           |
    | PDF                   | Tekst, skan, tabela i diagram na różnych stronach; odwołania wskazują właściwe strony, a obraz jest dostępny modelowi.                                       |
    | Obaj vendorzy         | Claude i Codex każdy buduje kontekst oraz każdy wykonuje krok czytający źródło i obraz przez produkcyjny most.                                               |
    | Dobór per krok        | Frontend dostaje UI, backend API, QA kryteria; asercje na faktycznie dostarczonych danych i wyniku pracy.                                                    |
    | Plan przed wykonaniem | Lead lub jawnie skonfigurowany krok planujący korzysta z przypiętych wersji, wykonawcy z tych samych wersji; nowy build niczego nie podmienia.               |
    | Izolacja              | Dwa adresy kroków/rozmów nie zamieniają źródeł; próba odczytu obcego ID jest odmową. Ochrona procesu testowana osobno.                                       |
    | Równoległość          | Dwa kroki naprawdę pracują równocześnie; czytanie wspólnego zestawu nie wprowadza globalnej blokady całej pracy.                                             |
    | Anulowanie            | Stop podczas importu, oczekiwania na slot, analizy i publikacji; brak późnego zapisu i potwierdzenie zakończenia własnych procesów.                          |
    | Powtarzalność wejść   | Zmiana/usunięcie biblioteki, restart i powtórzenie historyczne zachowują stare źródła; brakujący pakiet daje odmowę.                                         |
    | Uczciwy ekran         | Loading, partial import, failed build, ready z pytaniami, update available, missing source i exceeded budget są osiągalne po prawdziwej akcji.               |
    | Prywatność danych     | Materiały tylko w bibliotece/prywatnych wejściach i uprawnionym transporcie; brak pełnego promptu, base64 obrazu i treści źródeł w standardowej diagnostyce. |

    Testy deterministyczne mają kontrolowane procesy vendora i sprawdzają dane na prawdziwej granicy stdin/MCP, zapis na dysku, wykonanie oraz komunikaty. Próby żywych modeli sprawdzają dodatkowo rozumienie materiału, którego dubler nie dowodzi. Obraz testowy musi zawierać fakt niedostępny w podpisie, nazwie, OCR i wygenerowanym skrócie; w innym razie poprawna odpowiedź nie dowodzi odczytu obrazu.

    Browser E2E z `e2e/harness.ts` to prawdziwy React z atrapą IPC. Osobno uruchomić backend w izolowanym katalogu danych oraz prawdziwą aplikację Tauri z tym backendem i vendorami. Zapisać ścieżkę binarki, SHA, wersje CLI, model, scenariusz i wynik. Jeżeli automatyzacja natywnego okna nie jest dostępna, wykonać udokumentowany test ręczny albo oznaczyć go `not-tested`; test przeglądarkowy nie zastępuje natywnego Cmd+V.

    Do pomiaru jakości przygotować poza katalogiem dostępnym agentowi 12 syntetycznych zadań: cztery z wymaganiami tekstowymi, cztery wizualne, dwa ze sprzecznościami i dwa łączące UI/API/QA. Każde ma konkretne wymagane fakty, wynik zadania oraz informacje, których nie wolno domyślać. Porównać brak dodatkowego kontekstu, surowe źródła i opracowany zestaw, używając tego samego modelu, ustawień i plików wejściowych. Dla wariantów mieszczących się w oknie wykonać po trzy powtórzenia u każdego vendora; koszt pomiaru pozostaje objęty osobnym limitem.

    Mierzyć: zachowane ważne wymagania, poprawność wyniku, błędne twierdzenia, rozpoznanie detalu obrazu, liczbę odczytów, wejście/wyjście/cache jeśli vendor raportuje, czas i koszt jeśli znany. Warunki odbioru: wszystkie deterministyczne kontrakty przechodzą; żaden zatwierdzony ważny warunek nie ginie w składaniu wejścia; na każdym wspieranym vendorze obrazy i tekst działają w natywnych próbach; opracowany kontekst nie powoduje powtarzalnej regresji względem surowych źródeł na zestawie kontrolnym. Każda regresja ma nazwany przypadek i poprawkę lub pozostaje blokerem publikacji funkcji. Nie deklarować procentowych oszczędności przed pomiarem ani gwarancji braku degradacji poza sprawdzonym zestawem.

15. **Wykonanie w repo i warunek zakończenia**

    Przed rozpoczęciem implementacji odświeżyć SHA, stan checkoutu, istniejące biegi, worktree i procesy. Ten plan nie autoryzuje zatrzymywania cudzych procesów ani czyszczenia innych prac. Każdy bieg korzysta z `scripts/h run <id> --prompt ...`; polecenie zawiera zakres etapu i odsyła do tego dokumentu. Treść promptu przygotować w sposób, który nie wykonuje backticków ani podstawień powłoki.

    Dla nowego zachowania najpierw szkielet pozwalający uruchomić test, potem rzeczywista czerwień, następnie implementacja. Rust: `cargo test --test it <modul>:: -- --test-threads=1`. Front: `npx --no-install vitest run <konkretny-plik>`. Każde zaliczenie ma niezerowy licznik przejść. W pętli etapu uruchamiać zawężone kryteria i wymagane checki; `scripts/h land <id>` wykonuje pełne CI przy integracji. Nie współdzielić targetu między worktree i nie uruchamiać ciężkich Cargo/rustc jednocześnie.

    Plan nie wymaga zmiany `harness/`, `checks/`, `scripts/`, `AGENTS.md` ani `docs/DECISIONS-LOCKED.md`. Istniejące reguły obejmują nowe testy. Jeżeli pomiar nowej sceny gęstości albo brakująca bramka rzeczywiście wymaga edycji chronionej wyroczni, przygotować konkretną zmianę i zgłosić ją osobno zgodnie z AGENTS.md §7; nie usuwać sprawdzenia ani podnosić baseline. Przy czerwonej bramce po dozwolonej rundzie naprawczej obowiązuje stop i opis konkretnego problemu.

    Funkcja jest zakończona dopiero po CT-09: można zapisać i odtworzyć materiały, zbudować je oboma vendorami, wybrać per krok, zaplanować i wykonać pracę z właściwymi wersjami, sprawdzić ich rzeczywiste odczyty oraz powtórzyć bieg z zachowanych źródeł. Raport końcowy oddziela plan, zaimplementowany commit, wyniki testów, integrację i wydanie. Zielony selektor, pozytywny test formattera albo sama obecność plików nie stanowią odbioru.
