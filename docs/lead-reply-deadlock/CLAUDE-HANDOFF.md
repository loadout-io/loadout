# Zlecenie dla Claude Code: naprawa blokady odpowiedzi Leada

Napraw w Loadout blokadę rozmowy z Leadem Claude, która pojawia się przy dziewiątej zakończonej turze. Zaimplementuj rozwiązanie, odtwórz błąd testem na starym kodzie, przeprowadź testy regresji i doprowadź poprawkę do pełnej integracji według bieżącego harnessu repozytorium. Nie kończ na diagnozie, planie, zwiększeniu bufora ani poprawce wyglądu UI.

Repozytorium: `/Users/jakubgawronski/Projects/Loadout`.
Dokumentacja po polsku, interfejs po angielsku.
Najpierw przeczytaj `AGENTS.md`, `docs/DECISIONS-LOCKED.md` i `harness/README.md`. Decyzje zablokowane mają pierwszeństwo przed tym dokumentem.

## 1. Potwierdzony problem

Użytkownik prowadził wieloturową rozmowę z Leadem Claude w Loadout. Po kilku odpowiedziach interfejs przestał pokazywać nowe zdarzenia i pozostawił stan pracy/Interrupt. Kolejne wiadomości nadal docierały do Claude'a. Model wykonał polecenie, uruchomił aplikację Murmur i zapisał końcową odpowiedź we własnym transkrypcie, lecz Loadout już tej odpowiedzi nie odebrał.

Diagnoza kodu i logów wskazuje następujący mechanizm:

1. `src-tauri/src/engine/drivers/claude.rs` definiuje `TURNS_IN_FLIGHT: usize = 8`.
2. Przy uruchomieniu sterownika powstaje `mpsc::channel(TURNS_IN_FLIGHT)` dla wyników tur. Odbiornik `outcomes` należy do `ClaudeHandle`.
3. Czytnik stdout `pump` zapisuje surową linię, dekoduje ją, następnie woła `emit`.
4. Dla `AgentEvent::Finished` funkcja `emit` najpierw wykonuje `outcomes.send(outcome.clone()).await`, a dopiero potem `events.send(decoded).await`.
5. Kolejkę `outcomes` opróżnia `ClaudeHandle::wait()`.
6. Rozmowa Leada z dostępnym `voice` wysyła kolejne wiadomości przez `voice.send(...)` i omija `handle.wait()`. Wyniki do historii/UI czyta `read_along` z osobnego kanału zdarzeń. Nikt nie opróżnia `outcomes`.
7. Po ośmiu zakończonych turach kolejka jest pełna. Dziewiąte `outcomes.send(...).await` zatrzymuje `pump` przed wysłaniem dziewiątego `Finished` i przed odczytaniem kolejnych linii stdout.
8. Pisarz stdin nadal działa, więc agent otrzymuje kolejne zadania, mimo że użytkownik nie widzi jego odpowiedzi. Kanał nie jest zamknięty, proces żyje, a otwarte tury pozostają `delivered`.

W badanej sesji pasują wszystkie punkty:

- tury 1–8 mają stan `succeeded`;
- w surowym `logs/lead.jsonl` jest dokładnie dziewięć rekordów `type: result`;
- dziewiąty jest ostatnią linią tego pliku, numer 316, zapisany 2026-09-07 o 22:55:43 czasu Europe/Warsaw;
- tury 9–11 pozostają `delivered`, bez `endedAt`;
- własny transkrypt Claude'a zawiera dalszą pracę i końcową odpowiedź o 23:09:49;
- podczas diagnozy uruchomiony Murmur rzeczywiście działał, mimo że Loadout nie pokazał odpowiedzi.

Prywatny materiał do lokalnego odczytu, jeżeli nadal istnieje:

    ../meetnotes/.loadout/conversations/01a07cef-6c5e-7180-bcf6-54b91bdc90ce/
      conversation.json
      turns/0008.json
      turns/0009.json
      turns/0010.json
      turns/0011.json
      logs/lead.jsonl
      claude/_lead/projects/-Users-jakubgawronski-Projects-meetnotes/
        01a07cef-6c5a-77d2-b4c8-b6571d399802.jsonl

Nie kopiuj prywatnych transkryptów, promptów ani sekretów do fixture'ów, repozytorium lub raportu. Do odtworzenia wystarczy syntetyczny proces emitujący poprawne zdarzenia protokołu.

Kod ponownie sprawdzono na `28b870a040b211d3c66b6529ca04e272c1f6c561`. To punkt odniesienia analizy, nie deklaracja SHA zainstalowanej aplikacji. Przed implementacją odśwież rzeczywisty stan repo i linii wskazanych niżej.

## 2. Granice pracy i ochrona istniejących zadań

Pracuj w osobnym worktree przez aktualny `scripts/h run`, zgodnie z zasadami repo. Najpierw sprawdź HEAD, zmiany użytkownika, worktree i aktywne biegi. Context oraz wspólny Plan workflow są osobnymi pracami; nie przejmuj ich i nie duplikuj zmian w tych samych plikach. Ten dokument dotyczy konkretnej blokady transportu i jej skutków dla cyklu życia rozmowy.

Nie resetuj cudzych zmian, nie przełączaj użytkownikowi głównego checkoutu, nie zamykaj działających sesji Claude/Codex, Loadout ani Murmura. Nie wysyłaj ponownie zadań ze starej rozmowy — mogły już zostać wykonane. Testy uruchamiaj na swoich procesach i izolowanych katalogach danych.

Naprawa kodu i uruchomienie nowej binarki są odrębne od odblokowania starego, żywego procesu. Nie obiecuj, że zbudowanie poprawki naprawi już działającą aplikację. Nie zastępuj zainstalowanej aplikacji ani nie restartuj sesji użytkownika bez osobnego polecenia. Możesz uruchomić własny izolowany egzemplarz do weryfikacji.

## 3. Wymagane rozwiązanie

Usuń zależność czytania stdout od kolejki wyników, której dany właściciel sesji nie konsumuje. Wskaż w kodzie jednoznacznie, kto odbiera zakończenia tur i odpowiada za ich rozliczenie.

Rozwiązanie ma rozróżniać rzeczywisty sposób konsumpcji wyników, a nie nazwę vendora, rolę nazwaną „Lead” w tekście promptu czy magiczną liczbę tur. Może to być jawny wybór trybu odbioru albo przekazanie własności odbiornika właściwej pętli. Wybierz najmniejszą zmianę, która spełnia cały kontrakt poniżej; nie buduj nowego ogólnego systemu zdarzeń.

Obowiązkowe własności:

- Wieloturowa rozmowa otrzymuje każde `Finished` i kolejne zdarzenia bez względu na liczbę wcześniejszych tur. Nie odkłada kopii wyników do bezterminowo nieczytanej kolejki.
- Zwykłe kroki workflow nadal korzystają z `AgentHandle::wait()` zgodnie z jego kontraktem. Wynik jest dostępny również wtedy, gdy `wait()` zostanie wywołane po zdarzeniu zakończenia. Nie gub zakończenia w wyścigu „wynik przyszedł przed oczekiwaniem”.
- Wspierane sekwencje `send`/`wait` zachowują kolejność i tożsamość wyników. Nie zastępuj kolejki ostatnią wartością `watch`, jeśli mogłoby to zlać kilka rozliczanych tur w jedną.
- Jeżeli wprowadzasz jawny tryb bez osobnego odbiornika `wait`, jego kontrakt ma być czytelny i sprawdzony u wszystkich wołających. Nie zwracaj z `wait` wiecznie oczekującego future w ścieżce, która mogłaby go faktycznie wywołać.
- Sprawdź wszystkich użytkowników `start`, `start_conversation`, `voice` i `wait`. Samo wywołanie `start_conversation` nie jest wystarczającym dowodem, że nikt potem nie używa `wait` — mogą korzystać z niego inne operacje, w tym praca z obrazami lub Context.
- Nie dodawaj jednego odbiorcy wyników w sterowniku i drugiego konkurującego z nim w aktorze. Każdy wynik rozlicza się raz.
- Jeśli aktor oczekuje na zakończenie równolegle z wiadomościami/Stopem, sprawdź bezpieczeństwo anulowania tego future przez `select!`. Nie zgub wyniku ani mutowanego stanu sterownika po wygraniu innej gałęzi.
- Wysłanie kolejnej wiadomości podczas pracy i obsługa Interrupt zachowują dotychczasową semantykę. Opróżnianie wyników nie może wymusić czekania na koniec tury przed dostarczeniem wiadomości użytkownika.
- Odbiór zdarzeń, zapis dowodów, stan zajętości i zamknięcie tur pozostają spójne. Nie naprawiaj samej etykiety `active`, ukrywając nadal zablokowany transport.
- Kolejki i historia w pamięci mają uzasadnione ograniczenia. Sesja nie może rosnąć bez końca o nieodebrane kopie wyników.

Za naprawę nie uznajemy: zwiększenia 8 do 64/1024, kolejki bez limitu zamiast ustalenia odbiorcy, `try_send` ignorującego zagubione zakończenia, okresowego restartu po kilku wiadomościach, sztucznego timeoutu maskującego blokadę, zamiany całego Leada na innego vendora ani ręcznej edycji `conversation.json`.

## 4. Cykl życia, zamykanie i wolni odbiorcy

Prześledź także `finish_evidence`, `close`, `cancel`, EOF i zamknięcie odbiorcy. Usunięcie progu ośmiu tur nie wystarcza, jeżeli zamykanie nadal czeka bez końca na czytnik zatrzymany na wysyłce.

Zamknięcie sesji, deadline i Stop muszą korzystać z istniejącego supervisora i uzyskać dowód śmierci własnej grupy oraz właściwie rozliczanych potomków. Po udowodnionym zakończeniu procesu zebranie readerów, flush i zapis stanu końca muszą móc się zakończyć. Nie porzucaj dowodów ani readera tylko po to, żeby metoda zwróciła sukces.

Przy wolnym odbiorcy UI dopuszczalne jest istniejące, ograniczone w czasie buforowanie/backpressure. Niedopuszczalna jest trwała blokada odbioru wyniku lub zakończenia sesji po zniknięciu/odłączeniu tego odbiorcy. Nie usuwaj po cichu tekstów odpowiedzi i wyników narzędzi w celu podniesienia responsywności.

EOF, błąd protokołu, utrata czytnika i brak końcowego wyniku mają zachować istniejące, uczciwe rozróżnienia. Nie zamieniaj niepowodzenia w `succeeded`, a anulowania w zwykłą porażkę. Nie wprowadzaj globalnego boola anulowania ani `std::sync::Mutex` trzymanego przez await.

## 5. Mapa kodu i oczekiwany zakres

Potwierdź bieżące ścieżki. W przeanalizowanym kodzie:

| Miejsce | Znaczenie |
| --- | --- |
| `src-tauri/src/engine/drivers/claude.rs:273` | Limit `TURNS_IN_FLIGHT = 8`. |
| `claude.rs:2663`, `claude.rs:2766` | `pump`: czytanie stdout i oczekiwanie na `emit`. |
| `claude.rs:3012` | `emit`: blokująca wysyłka do `outcomes` przed kanałem zdarzeń. |
| `claude.rs:3109`, `claude.rs:3209` | Zbieranie readerów oraz odbiór przez `wait()`. |
| `claude.rs:3640` | Utworzenie kolejki i połączenie jej z readerem/uchwytem. |
| `src-tauri/src/commands/chat.rs:323` | Własność sesji, uchwytu, voice i czytnika. |
| `chat.rs:1160` | Aktor rozmowy, wiadomości, postęp, Stop i deadline. |
| `chat.rs:2082` | `await_next_delivery`: droga voice omija `wait()`. |
| `chat.rs:3027` | `read_along`: odbiór `Finished`, zamknięcie receiptu, widok rozmowy. |
| `chat.rs:3582` | `begin_thread`: produkcyjne zestawienie sesji Leada. |
| `src-tauri/src/engine/drivers/mod.rs` | Wspólny kontrakt sterownika/uchwytu, jeśli potrzebna jest minimalna zmiana. |
| `src-tauri/src/engine/drivers/codex.rs` | Sprawdzenie zgodności drugiego vendora, bez niepotrzebnej przebudowy. |
| `src-tauri/src/evidence.rs` | Kontrakt dowodów tur i rozmowy; zachowaj prywatność oraz trwałość. |

Pierwszy plan wykonawczy powinien objąć konieczne wpięcia, nowe testy w `src-tauri/tests/it/`, ich rejestrację w `main.rs` i ewentualny test rzeczywistego widoku rozmowy. Zmiana w interfejsie/typach wspólnych wymaga sprawdzenia wszystkich implementacji, w tym dublerów. Nie rozpraszaj tej polityki po osobnych wrapperach dla każdej ścieżki.

Przydatne istniejące testy i wzorce:

- `claude_session_process.rs`: prawdziwy proces testowy i dwie tury, ale test sam woła `wait()` — dlatego nie odtwarza incydentu Leada;
- `live_chat_goes_through_the_registry.rs`, `lead_thread_per_terminal.rs`, `lead_thread_per_scope.rs`: produkcyjna droga rozmowy i izolacja;
- `lead_evidence_is_durable.rs`, `lead_stall_is_visible.rs`: stan tur i sygnalizowanie braku odpowiedzi;
- `the_lead_can_be_interrupted.rs`, `claude_cancel_escalation.rs`, `supervisor_pipe_eof.rs`: przerwanie i zakończenie;
- `stream_raw_tee_live.rs`, `z14_rebuilt_history_keeps_what_the_tools_did.rs`: zapis strumienia i odtworzenie historii;
- `lead_image_reaches_both_vendors.rs`: dostawa obrazów i zgodność obu vendorów;
- `src/sections/run/lead-control-results-are-visible.test.tsx` i produkcyjny komponent rozmowy: wynik na ekranie.

## 6. Test, który najpierw ma wykazać błąd

Dodaj test regresji przez **rzeczywisty `ClaudeDriver` i produkcyjną drogę rozmowy `Threads`/aktora**, z kontrolowanym procesem udającym CLI po drugiej stronie stdin/stdout. Nie używaj testowego `AgentHandle`, który omija implementację kolejki. Nie odtwarzaj algorytmu własną kolejką w teście jako zamiennika produktu.

Proces testowy:

- nie wywołuje prawdziwego API i nie potrzebuje konta;
- czyta kolejne poprawne koperty wejścia;
- dla każdej odpowiada unikalnym tekstem i prawidłowym `result`;
- pozostaje żywy między turami;
- zapisuje wyłącznie syntetyczne znaczniki potrzebne do udowodnienia kolejności i liczby dostarczonych wiadomości.

Scenariusz podstawowy:

1. Uruchom sesję dokładnie drogą Leada z voice i odbiorcą zdarzeń.
2. Wyślij **co najmniej 24 kolejne tury**, wymagając odpowiedzi i zakończenia każdej.
3. Test nie może dodatkowo opróżniać `handle.wait()` za plecami aktora — to zamaskowałoby pierwotny błąd.
4. Zweryfikuj każdą unikalną odpowiedź, kolejność, dokładnie jedno zakończenie, terminalny receipt danej tury i możliwość wysłania następnej wiadomości.
5. Na starym kodzie test ma paść na braku zakończenia dziewiątej tury, z ograniczonym czasem oczekiwania. Musi wcześniej faktycznie wykonać pierwszych osiem.
6. Awaria testu sprząta jego własne procesy w ograniczonym czasie. Nie pozwól, aby reprodukcja deadlocka zawiesiła cały test runner przy `close()` lub Drop. W razie potrzeby wykorzystaj istniejący izolowany proces testowy i zewnętrzny watchdog należący do testu.

Wymagaj zdarzenia/receiptu właściwej tury, a nie samego obecnego tekstu: w incydencie treść dziewiątej odpowiedzi zdążyła dotrzeć, a blokada wystąpiła na jej zakończeniu. Używaj deadline'ów i synchronizacji po zdarzeniu zamiast wielosekundowych sleepów.

Zapisz rzeczywisty wynik RED przed implementacją i GREEN po poprawce. W Ruście test jest modułem `src-tauri/tests/it/<nazwa>.rs`, dopisanym do `src-tauri/tests/it/main.rs`, uruchamianym przez `cargo test --test it <nazwa>::`. Błąd kompilacji, nieznaleziony test oraz `0 passed` nie stanowią dowodu.

## 7. Pozostałe obowiązkowe kryteria

| Przypadek | Co musi zostać potwierdzone |
| --- | --- |
| Długi dialog | Co najmniej 24 tury kończą się poprawnie, a liczba nierozliczonych wyników nie rośnie z liczbą już zamkniętych tur. |
| Wiadomość podczas pracy | Wiadomość jest dostarczana zgodnie z istniejącym kontraktem, nie czeka na opróżnianie wyniku poprzedniej tury; brak pomylenia odpowiedzi/receiptów. |
| Zwykły workflow | `start` + `wait` zwraca właściwy wynik, także gdy wynik nadszedł wcześniej; kolejne wspierane tury nie gubią wyników. |
| Reużycie API rozmowy | Istniejący wołający `start_conversation`, który korzysta z `wait`, nadal działa; jawny nowy kontrakt ma testy wszystkich takich wołających. |
| Dwie rozmowy | Dwa terminale/workspace'y mają niezależne wyniki i stany; zajęcie/zamknięcie jednej nie blokuje drugiej. |
| Wolny/odłączony odbiorca | Strumień może się wznowić, a wynik i zamknięcie nie czekają bez końca po odłączeniu odbiorcy; brak cichego sukcesu z utraconymi dowodami. |
| Close/Stop/timeout | Własne procesy zostają rozliczone i uzyskują dowód śmierci, readery kończą pracę, operacja nie zawisa na pełnej/nieodbieranej kolejce. |
| EOF i błąd | Brak końcowego `result` lub przerwany strumień ma prawidłowy wynik i czytelny komunikat; nie pozostaje fałszywie trwającą odpowiedzią. |
| Interrupt | Zachowana jest semantyka przerwania aktualnej pracy i dalszej rozmowy; nie omijaj istniejącej detekcji możliwości protokołu. |
| Codex | Istniejąca droga Leada Codex, jej kończenie tur i przerwanie nie zostały uszkodzone zmianą wspólnego kontraktu. |
| Widok i historia | Ostatnia odpowiedź, koniec zajętości oraz wynik sterowania dochodzą do rzeczywistego widoku; historia i prywatne receipty zgadzają się z odebranymi zakończeniami. |
| Prywatność | Filtr prywatnych wejść, zapis surowych dozwolonych zdarzeń i allowlistowany raport diagnostyczny zachowują dotychczasowe własności. |

Nie wymagaj zakończenia całej konwersacji, aby zamknąć pojedynczą turę. `conversation.state = active` może oznaczać otwartą, bezczynną rozmowę; testuj stan odpowiedzi i jej receipt, nie wymuszaj fałszywego zamknięcia żywej sesji.

Nie utożsamiaj ogólnej obsługi wiadomości wysłanych podczas pracy z założeniem „jedna wiadomość zawsze daje jeden osobny result”: zachowaj rzeczywisty kontrakt klienta dla sterowania/łączenia wiadomości. Sekwencyjny test 24 tur ma jednoznaczne parowanie i musi przechodzić bez tego uproszczenia.

## 8. Weryfikacja produktu i integracja

Po deterministycznej regresji wykonaj mały test żywego Claude'a w izolowanym profilu: **co najmniej 12 krótkich tur w tej samej sesji**, przeprowadzonych przez Loadout, z obserwacją kolejnych zakończeń i widoku. Ten test ma przekroczyć dawny próg, nie generować kosztownej pracy implementacyjnej. Korzystaj z istniejących limitów; nie uruchamiaj 24 dużych zadań u modelu.

Sprawdź rzeczywistą aplikację Tauri z backendem, a nie tylko React z atrapą IPC. Podaj SHA, uruchomioną binarkę, wersję CLI, model i wynik. Jeżeli konto, narzędzie natywnego sterowania lub środowisko nie pozwala tego wykonać, zgłoś konkretny brak jako `not-tested`; nie opisuj mocka jako żywego testu. Próba żywego Codex jest potrzebna, jeżeli zmiana dotyka wspólnej ścieżki odbioru; w przeciwnym razie uruchom jej odpowiednie istniejące regresje i jasno opisz zakres weryfikacji.

Nie uruchamiaj równolegle ciężkich Cargo/rustc i nie współdziel targetu między worktree. W pętli uruchamiaj zawężone testy oraz wymagane checki. Pełna suita należy do lądowania przez `scripts/h land`, które uruchamia `scripts/ci.sh full`. Każde zaliczenie ma niezerowy licznik przejść.

Nie edytuj wyroczni w `harness/`, `checks/`, `scripts/`, `AGENTS.md` lub `docs/DECISIONS-LOCKED.md`, aby uzyskać zielony wynik. Jeżeli okaże się potrzebna rzeczywista zmiana chronionego pliku, przygotuj konkretny diff i uzasadnienie zgodnie z AGENTS.md §7. Przy wymaganym przez repo zatrzymaniu zgłoś nazwany problem i gotową do oceny zmianę. Nie pytaj o rutynowe decyzje już objęte tym zleceniem.

## 9. Warunek zakończenia i raport

Poprawka jest zakończona, gdy źródło blokady jest usunięte, regresja rzeczywiście pada na starym kodzie i przechodzi na nowym, zwykłe workflow nadal otrzymują poprawne wyniki, a wieloturowa rozmowa oraz jej zamykanie działają przez produkcyjne ścieżki. Samo zwiększenie liczby tur, które „jeszcze działają”, nie spełnia zadania.

Dodaj krótki komentarz wyjaśniający incydent i własność odbioru wyników. Dokumentacja ma mówić, dlaczego nieodebrany kanał nie może blokować stdout. Nie dopisuj ostrzeżenia „nie rozmawiaj więcej niż osiem tur” do promptu agenta.

Raport końcowy ma zawierać:

1. Jednozdaniową przyczynę i sposób usunięcia niekonsumowanej kolejki/zależności.
2. Zmienione pliki i commit/gałąź oraz rzeczywisty stan integracji.
3. RED i GREEN regresji, liczbę wykonanych tur i zaliczonych testów.
4. Wynik sprawdzenia zwykłych workflow, zamykania, UI i żywych vendorów.
5. Wynik pełnego CI dla zintegrowanej poprawki oraz ewentualne konkretne luki.
6. Jasne rozróżnienie między gotową poprawką a nadal działającą starszą aplikacją użytkownika.

Nie wdrażaj w tym zleceniu wspólnego planu, nowej biblioteki Context ani zmian w Murmurze. Wykonaj w pełni tę naprawę Leada i jej konieczne regresje.
