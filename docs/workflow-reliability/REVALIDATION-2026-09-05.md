# Reweryfikacja main: historia Leada, odtwarzanie i uruchamianie projektu

Data: 2026-09-05. Odczytany, czysty `main`:
`715567effb059ec3e684de2ba17d231b854d0b04`.
Poprzednia baza planu: `1c6e579c94507cfaf701b16a32bfe35d99b78d05`.

To **aktualizacja projektu wykonania**, nie implementacja. Kod analizowano w
`/Users/jakubgawronski/Projects/Loadout`; dokumenty pozostają w
`/Users/jakubgawronski/Projects/loadout-workflow-reliability-plan`.
Nie przesuwano jego gałęzi, nie zmieniano main, nie uruchamiano płatnych agentów.

Ten dokument ma pierwszeństwo przed statusem i wskazanymi fragmentami
[TASKS.md](TASKS.md). Pozostałe kontrakty oraz reguły RED/GREEN i bezpieczeństwa obowiązują.
Nie przekazuj wykonawcy samego starego promptu bez tej aktualizacji.

## 1. Werdykt po zmianach

Main poprawił obsługę operacyjną, ale **nie zamknął głównych kontraktów poprzedniego planu**.
Nie wolno już twierdzić, że Lead w ogóle nie ma Stop albo że aplikacja nie potrafi uruchomić
projektu. Ma oba mechanizmy. Nadal brakuje dokładnego adresowania i potwierdzeń operacji,
świadomego odczytu historii przez Leada oraz pełnego lifecycle gotowej aplikacji w kopii.

Nowe wymagania rozdzielamy na konkretne obietnice:

| Potrzeba | Obietnica produktu |
|---|---|
| Lead wie, co robiono wcześniej | Krótki kontekst zapisanych faktów przy nowej rozmowie i narzędzia do wyszukania/odczytu źródeł |
| Lead wraca do dawnej pracy | Wskazuje konkretny bieg i wynik; rozróżnia stare ustalenia od aktualnych oraz materiał dostępny od usuniętego |
| Powtórzenie pracy | Nowy bieg ze wskazanej zapisanej konfiguracji i dostępnych wejść albo jawna odmowa; nie obietnica identycznego wyniku LLM |
| Przywrócenie rezultatu | Nowa izolowana kopia istniejących plików bez uruchamiania modelu i bez nadpisania aktywnego repo |
| Agent odpala projekt | Rozpoznaje komendy w repo, przygotowuje kopię i używa obecnego wykonawcy usług; nie musi dostać ręcznie wpisanego polecenia dla każdego repo |
| Inny agent sprawdza aplikację | Dostaje adres gotowej instancji właściwej kopii, może użyć skonfigurowanego narzędzia przeglądarki |
| Bezpieczny koniec | Proces, jego logi, port i katalog mają wspólnego właściciela; cleanup następuje po dowodzie śmierci, nie samym końcu grafu |

Nie budujemy drugiego systemu pamięci, schedulera ani menedżera procesów obok obecnego
`Processes`. Nie zamieniamy historii modelu w nadrzędne instrukcje ani w zgodę człowieka.

## 2. Co main już dostarczył

Z-36–Z-50 mają implementacje w historii main; nie przydzielać tych zakresów ponownie.
W `docs/prod-ready/PLAN.md` pozostała niespójna komórka TODO przy Z-43, mimo merge
`90742bba`, implementacji `0dab2f00` i zamknięcia fali. O stanie decyduje kod, nie ta komórka.
W chwili audytu były tylko dwa worktree: main i dokumentacyjny. Nie znaleziono aktywnego Cargo.

| Już istnieje | Dowód w aktualnym kodzie | Czego to nie dowodzi |
|---|---|---|
| Stop przez Leada | `bridge/verbs.rs:118`, `bridge/library.rs:569`; Z-39 | Hostowego dowodu zgody, adresu konkretnego run ID i potwierdzenia zakończonej operacji |
| Postęp narzędzia, kolejka, interrupt tury | `commands/chat.rs:2744`, `ipc.rs:3845`; Z-36/Z-40 | Continue konkretnego checkpointu i wiadomości do każdej sesji kroku |
| Prawdziwy opis możliwości Leada | `commands/chat.rs:758`, `what_the_lead_can_do_inner`; Z-50 | Zdolności odbioru wiadomości przez wszystkie kroki running |
| Zakończone kafelki, stronicowanie handoffów, retencja | Z-37/Z-49/Z-46, `commands/history.rs`, `handoffs.rs`, `sweep.rs` | Automatycznej znajomości historii przez nowego Leada i zachowania wszystkich wyników do odtworzenia |
| Sonda folderu i informacja o kontekście gospodarza | `ipc.rs:1494`, `history.rs:887`; Z-42/Z-47 | Kontrolowanego dostarczenia wszystkich AGENTS/CLAUDE i pełnych bundle skilli |
| Rachunek tokenów, odmowa nieznanej ceny, ciężkie kroki | Z-48/Z-44/Z-45 | Naprawy algorytmu pętli, fan-in albo całego workflow w Labie |
| Komenda uruchomienia od agenta | `workflow/mod.rs:392`, `run.rs:9491`, Serve.commandFrom | Gotowości aplikacji i dostarczenia właściwego URL następnemu agentowi |

Ścieżki Rust w tabelach są względne do `src-tauri/src/` w **odczytanym main**.

## 3. Reweryfikacja dotychczasowych kart

| Karta | Status po odczycie main | Wniosek dla wykonawcy |
|---|---|---|
| WF-01 | OTWARTE | HEAD/WIP nadal odczytywane per kopia; zachować już istniejące wznowienie per work key |
| WF-02 | OTWARTE | fan_in.rs bez zmian względem starej bazy; nadal brak pełnego delete/rename/mode/symlink |
| WF-03 | OTWARTE | folded nadal przed ukończeniem składania; brak trwałego inputReady |
| WF-04 | OTWARTE | nothing_to_judge nadal pomija ocenę wyniku bez zmian Git |
| WF-05 | OTWARTE | ostatni wynik nadal wybierany per kafelek, nie per kopia |
| WF-06 | OTWARTE, POSZERZ KRAWĘDZIE RETENCJI | Normalny cleanup non-git nadal usuwa wynik; nowe zabezpieczenia date-retention nie obejmują wszystkich dróg |
| WF-07 | OTWARTE | /run Leada nadal gubi folder przy przejściu do aktywnej karty; wynik narzędzia to zamiar, nie RunTicket |
| WF-08 | CZĘŚCIOWA BAZA GOTOWA | Z-50 opisuje Leada. Do zrobienia capabilities konkretnych sesji kroków i uczciwy wynik wysyłki |
| WF-09 | OTWARTE | Brak narzędzia statusu biegu dla Leada; rozszerza je WF-21 o historię |
| WF-10 | CZĘŚCIOWA BAZA GOTOWA | Rozbudować Z-39, nie implementować Stop od początku; nadal brak dokładnych zgód/ID/ack/continue/retry/send |
| WF-11 | OTWARTE | Role::Step nadal nie ma narzędzi mostu |
| WF-12 | OTWARTE | Z-42/Z-47 są gotowymi szwami, nie rozwiązaniem wspólnego kontekstu repo |
| WF-13 | OTWARTE | Borrow nadal przenosi SKILL.md; pełne zasoby i wszystkie sesje nadal do sprawdzenia/dostarczenia |
| WF-14 | OTWARTE | Rozszerzyć o agentowe rozpoznanie przygotowania i komendy; nie wymagać ręcznych poleceń dla każdego repo |
| WF-15 | OTWARTE | Historia Lab nadal oceniana przez aktualny EvalSet |
| WF-16 | OTWARTE | Brak zakresów per komórka; wspólne katalogi handoffów nadal udostępniane |
| WF-17 | OTWARTE | Subject nadal tylko Agent/Skill; Serve w Lab pozostaje jawnie ograniczonym zakresem |
| WF-18 | OTWARTE | Bez zmian w kontrakcie niezależnej wyroczni i jej ochrony |
| WF-19 | OTWARTE | Brak UI pomiaru całych workflow |
| WF-20 | NIE ODEBRANO | Poszerzyć odbiór o WF-21–WF-28; lokalne zielone testy nie zamykają tego kontraktu |

„Częściowa baza gotowa” nie znaczy, że cała karta spełnia kryteria. Na tym SHA nie
zakwalifikowano żadnego z pełnych dwudziestu kontraktów jako całkowicie zamknięty.

### Najważniejsze nadal obecne defekty

1. `fan_in.rs:230–254` odwiedza istniejące pliki i pomija linki, a `Change:211–217`
   zawiera bajty, nie operację usunięcia/tryb. `run.rs:9971–9977` wpisuje folded przed
   składaniem. Nowe zabezpieczenia przekazań nie naprawiają fizycznego składania plików.
2. `run.rs:9315–9330` ogląda pierwszą kopię entry i tylko Git touched;
   `9792–9802` pomija sędziego przed rozróżnieniem Agent/Check.
   `12476–12490` nadal bierze jeden wynik per tile, a `12555–12571` pierwszą kopię historii.
3. `run.rs:7897–7899` nadal bezwarunkowo usuwa non-git kopię. Z-46 nie naprawił końca biegu.
4. `bridge/library.rs:497–518` emituje /run i zwraca asked:true; `feed/suggested.ts:125–145`
   nie zachowuje folderu dla tej drogi, a `launch.ts:133` bierze activeWorkspace.
5. Stop Z-39 opiera zgodę na confirmed:true podanym przez model. Nie wiąże operacji z
   hostowym tokenem pytania i run ID, nie czeka na wynik. Test wymogu boola nie dowodzi
   pochodzenia zgody człowieka.
6. `lab/mod.rs:177–182` nie ma workflow subjectu; `commands/lab.rs:468` nadal wywołuje
   score_one przez open.set. Edycja kryteriów może zmienić interpretację starego pomiaru.

### Nowe istotne interakcje z retencją i integralnością

- `sweep.rs:543–578` chroni dirty worktree i niewlane commity przed sprzątaniem według
  daty. Jednak `reconcile.rs:379–400` (keepLastRuns) idzie bezpośrednio do
  `history::forget_run_inner`; jego guard sprawdza wyjęte gałęzie, nie każdy jedyny
  zachowany wynik. WF-06 obejmuje **oba** tryby retencji oraz ręczne Forget.
- `sweep.rs:402–411` rozpoznaje katalogi przez rejestr Git. Zachowany wynik non-git
  musi zostać dodany do wspólnej kwalifikacji zasobów, inaczej nadrzędny run folder
  może nadal zostać usunięty. Nie implementować drugiej polityki obok sweep/reconcile.
- Z-41 daje read-only i kontrolę brakującego/zmienionego pliku, ale
  `run.rs:12301–12329` porównuje długość, nie wszystkie bajty; odziedziczony attachment
  bywa bez oczekiwanej długości (`12417–12422`). Podmiana treści na inną o tej samej
  długości nie jest dowodem integralności. WF-16/18 muszą konsumować zweryfikowany
  snapshot/tożsamość, nie uznawać obecnego Z-41 za wystarczającą ochronę.

## 4. Obowiązujące poprawki do starych kart

- **WF-01:** zachować bieżące odtwarzanie gałęzi źródła per work key
  (`run.rs:6333–6347`). Przy wymaganym historycznym wejściu usunąć cichy fallback
  brakującej gałęzi/uszkodzonego run.json do HEAD (`6404–6414`). Odmowa nazywa brak.
- **WF-06:** zakres plików obejmuje `commands/sweep.rs` oraz wspólną kwalifikację
  keepLastRuns/date-retention/Forget. Dodać RED: automatyczna retencja licznikowa nie
  kasuje jedynego wyniku; data-retention widzi zachowany non-git wynik. Respektować
  żywe zasoby WF-25 i przypięcie wyniku WF-24.
- **WF-08:** nie budować ponownie WhatTheLeadCanDo, opisu UI ani interrupt. Wykorzystać
  efektywną konfigurację już dostarczoną przez Z-50; nowa praca dotyczy sesji kroków.
- **WF-10:** rozszerzyć istniejący stop_run. Dokładne zgody, RunRef i wynik operacji
  są nadal konieczne. Określenie „wszystko zatrzymane” obejmuje run-owned usługi WF-25;
  window-owned usługi muszą być nazwane jako świadomie pozostawione, nie ukryte.
- **WF-12/13:** podpiąć fakty resolvera do istniejącej informacji o kontekście Z-47.
  Nie zastępować tych faktów heurystyką prefiksów pluginów i nie dublować panelu.
- **WF-14:** „komenda człowieka” jest jedną opcją, nie wymaganiem. Zwykły Agent/Bash
  może rozpoznać i przygotować repo, a commandFrom przenosi wynik. Przygotowanie
  nadal musi być jawnie skonfigurowaną pracą i odbywać się w kopii właściwych konsumentów.
- **WF-15:** biegi już zapisują workflow_snapshot i efektywnego agenta
  (`run.rs:13444–13451`, `13579–13581`). Rozszerzyć ten zapis i wspólny resolver
  materiałów, nie stworzyć równoległego archiwum tych samych ustawień dla Lab/replay.
- **WF-16:** include/read handoff/attachment ma sprawdzać tożsamość i zawartość
  opublikowanego materiału. Dodać RED z innymi bajtami o identycznej długości oraz
  odziedziczonym attachmentem. Scope nadal nie jest systemowym sandboxem.
- **WF-17/19:** w pierwszym odbiorze Lab nadal jawnie odmawia Serve, checkpointów
  i zewnętrznych mutacji. Nie uznać uruchomienia preview w normalnym Run za obsługę
  preview w porównaniu Lab. Jeżeli celem odbioru ma być także Lab web-app, wymagany
  jest opisany niżej dodatkowy zakres WF-20B; nie włączać go cicho.
- **WF-20:** odbiór normalnego produktu wymaga także WF-21–WF-28 i scenariuszy §8.

## 5. Nowe zadania: Lead zna historię i odtwarza pracę

### WF-21 — Ograniczone narzędzia historii dla Leada

Zależność: WF-09. Rozszerzenie, nie zastępstwo narzędzia aktywnego statusu.

**Dzisiaj:** Lead może w ramach narzędzi plikowych ręcznie szukać run.json; brief wręcz
mu to sugeruje (`chat.rs:164–167`). Istnieją list_runs_inner/read_run_inner dla UI,
ale most ich nie wystawia. Pełny read_run_inner wczytuje logi, więc nie może być hurtowym
czytnikiem kontekstu.

**Kontrakt:**

- Narzędzia: `list_runs({cursor?, limit?, state?, workflow_id?, query?})`,
  `read_run_summary({run_id})`, `list_handoffs({run_id,cursor?})`,
  `read_handoff({run_id,handoff_id,cursor?,max_bytes?})`.
- Workspace pochodzi z rozmowy, nie argumentu modelu. Run ID i handoff ID są
  walidowane w tym workspace; model nie podaje arbitralnej ścieżki.
- Domyślnie 20 wyników, maksymalnie 50; treść odczytu do 32 KiB, łączna odpowiedź do
  64 KiB. Truncation/cursor są jawne. Limity egzekwować przed pełnym odczytem pliku.
- Cursor zachowuje granicę czasową pierwszej strony i ostatnie stabilne ID. Dopisanie
  nowego biegu między stronami nie duplikuje wyników ani nie przesuwa już przeglądanej serii.
- Query przeszukuje ograniczone metadane/tytuły. Nie skanować wszystkich logów i źródeł
  przy każdym pytaniu. Wykorzystać czytniki history/handoffs i disposable indeks, nie
  nową bazę prawdy ani embeddings.
- Odpowiedź zawiera pochodzenie, czas, stan wyniku i możliwość dalszego odczytu. Brak
  pliku/usunięty bieg daje RunUnavailable, nie „nie było takiej pracy”.
- Historyczna treść jest materiałem do analizy, nigdy nową instrukcją systemową,
  komendą do wykonania ani potwierdzeniem Stop/restore. UI ma działający odnośnik źródła.

**Pliki:** `commands/{history,handoffs}.rs`, `bridge/{verbs,library}.rs`, istniejące
typy/mapowanie wyników do strumienia, potrzebne lustra IPC. Bez raw logs domyślnie.

**RED/AC:** moduł `lead_finds_and_reads_project_history`; rzeczywisty host mostu
znajduje starszy bieg B mimo nowszego A i odczytuje właściwy marker z handoffu.
Nowy bieg między stronami, brak/zmiana pliku, symlink, obcy workspace, limit przed
odczytem i historyczna treść „stop run” bez skutku. Front:
`src/sections/run/lead-history-has-openable-sources.test.tsx`.

### WF-22 — Nowa rozmowa zaczyna z zapisanym kontekstem projektu

Zależność: WF-21. Nie budujemy archiwum pełnych promptów.

**Dzisiaj:** powrót do tej samej żywej rozmowy zachowuje actor. Nowy Threads po restarcie
jest pusty; nowy thread dostaje nowy UUID i resume:None. Prywatne evidence rozmowy
zapisuje prompt_bytes, nie pełne wejście (`evidence.rs:51–60`). Nie ma z czego uczciwie
odtworzyć całej rozmowy modelu z samych receipts.

**Kontrakt:**

- Host składa `LeadBriefing` przy rozpoczęciu nowej sesji: aktywny RunRef, do pięciu
  ostatnich biegów z krótkim stanem i źródłem, ograniczony katalog przyjętych notatek
  projektowych oraz informację, jak użyć WF-21. Budżet do 12 KiB, reszta na żądanie.
- Aktualizować skrót między turami, nie w środku trwającej tury. Nie dopisywać w kółko
  całych run.json/logów. Każdy fakt ma źródło i datę; stary plan nie jest aktualnym wynikiem.
- Wykorzystać obecną pamięć projektową (`commands/memory.rs`), jej katalog i mechanizm
  przyjmowania sugestii. Model może zaproponować ustalenie, ale sam nie awansuje go do
  zatwierdzonej decyzji człowieka. Nie tworzyć drugiej półki „pamięci Leada”.
- Po restarcie UI i brief mówią „new conversation with saved project context”, nie
  „resumed all messages”. Natywne wznowienie konkretnej sesji vendora może być późniejszą
  funkcją, ale ta karta go nie wymaga i nie symuluje przez receipts.
- Nie utrwalać pełnych nowych wiadomości użytkownika, sekretów ani złożonych promptów.
  Własne słowa człowieka mogą zostać notatką tylko przez istniejące jawne przyjęcie.
- Stare zgody, checkpoint tokens, narzędzia w toku i niepotwierdzone twierdzenia modelu
  nigdy nie są przywracane jako aktywny stan. Dane sąsiedniego workspace są wykluczone.

**Pliki:** `commands/{chat,memory}.rs`, wspólne czytniki WF-21, bridge notatek jeśli
potrzebny, typy/brief i istniejący panel kontekstu. Nie zmieniać polityki evidence.

**RED/AC:** `a_new_lead_knows_saved_project_facts`: nowy obiekt Threads po zamknięciu
poprzedniego dostaje w rzeczywistym RunSpec identyfikator starego biegu i przyjęte
ustalenie; nie dostaje odrzuconej sugestii, cudzych danych ani starej zgody.
Edycja/odrzucenie notatki jest widoczne w następnej turze. Odczyt skasowanego źródła
daje niepewność. Front: `a-new-conversation-says-what-it-remembers.test.tsx` w run/.

### WF-23 — Powtórzenie wskazanej konfiguracji z historii

Zależności: WF-01, WF-07, WF-10, WF-13, WF-15. Współdzieli obecny rerun/resume.

**Kontrakt:**

- `prepare_replay({source_run_id, selection, mode:"recorded"|"current"})` zwraca
  ReplayPreview: źródło, zakres kafelków/kopii, graf/ustawienia, dostępność wejść,
  różnice względem current, wymagane dzisiejsze uprawnienia i ograniczenie kosztu.
- `recorded` bierze zapisany workflow_snapshot, efektywnych agentów oraz zweryfikowane
  materiały źródłowe z istniejącego zapisu, uzupełnionego WF-01/13/15. Nie czyta w ich
  miejsce bieżącej biblioteki i nie spada do HEAD przy brakującej gałęzi.
- `current` zachowuje dzisiejszy wariant istniejącego rerun, ale różnice są jawne.
  Nie nazywać go odtworzeniem dawnej konfiguracji.
- `start_replay({preview_id, confirmation_token})` używa wspólnego Start/ack WF-07.
  Preview wiąże RunRef, materiały i wersję żądania; zmiana źródła/polityki po preview
  unieważnia zgodę. Zawsze powstaje nowy run ID, źródło pozostaje niezmienne.
- Brak historycznych bajtów skilla/instrukcji/wejść daje RecordedInputsUnavailable
  ze wskazaniem braków. Nie traktować odcisku dzisiejszego skilla jako archiwum dawnego.
- Zachować odrębność work key i prób. Przy ponowieniu całego kafelka podać liczbę kopii;
  pojedyncza próba nie może po cichu oznaczać wszystkich kopii.
- Dawne uprawnienia nie podnoszą obecnej polityki. Sekrety trzeba ponownie dostarczyć
  zatwierdzoną drogą; nie odtwarzamy wartości z archiwum i nie rozszerzamy wyjątku argv.
- „Recorded” oznacza zapisane, kontrolowane wejścia Loadouta, nie identyczne środowisko
  całego świata ani identyczny output LLM. Nieznany dawny kontekst natywny/wersja jest
  widocznym ograniczeniem; ścisły tryb nie udaje pełnego odtworzenia przy brakach.
- Przywrócenie starego workflow do biblioteki zapisuje nową kopię z nową tożsamością,
  nie nadpisuje aktualnej definicji i nie mutuje żywego grafu.

**Pliki:** `commands/{rerun,run}.rs`, wspólny resolver zapisanych wejść z WF-15,
`bridge/{verbs,library}.rs`, history/IPC i istniejące akcje odtwarzania w run/past/.

**RED/AC:** `replay_uses_the_recorded_run_inputs`: kompletna fixture z zachowanymi
wejściami; zmienić workflow i model po źródłowym biegu, uruchomić recorded przez
produkcyjny start. Wymagany SUKCES: rzeczywisty RunSpec i wykonany graf mają historyczne
ustawienia. Odmowa nie zalicza tego przypadku. Osobne niekompletne fixture sprawdzają
konkretną odmowę brakującego materiału. Brak gałęzi/skilla nigdy nie daje current/HEAD.
Dwie kopie z różnymi wejściami, zmiana po preview, nieaktualna zgoda, niezmienny source
run.json i nowy RunRef. Front: `recorded-and-current-replay-are-different.test.tsx` w run/past/.

### WF-24 — Przywrócenie wyniku bez modelu i bez nadpisania projektu

Zależności: WF-01, WF-06, wspólny odczyt i zgody WF-21/23. To nie jest rerun.

**Kontrakt:**

- Przy finalizacji zapisać powiązanie wyniku z niezmiennym commit OID albo kompletnym
  manifestem non-git. Branch jest nazwą pomocniczą; przesunięcie ref nie podmienia
  historycznego wyniku. Wykorzystać istniejące finalize/isolate, nie osobny system backupu.
- `prepare_result_restore({source_run_id, result_id})` pokazuje dostępność, OID/manifest,
  listę/zakres plików, koszt miejsca i nowy kontrolowany katalog docelowy.
  `restore_result({preview_id, confirmation_token})` materializuje dokładnie to źródło.
- Domyślnie nowy izolowany worktree albo nowy katalog eksportu. Nie checkout/reset,
  merge ani zapis do brudnego aktywnego repo. Przywrócenie nie uruchamia żadnego modelu,
  hooka, instalatora ani serwera; uruchomienie kopii jest osobną operacją.
- Usunięty obiekt/niekompletny manifest oznacza ResultUnavailable. Nie ma fallbacku
  do HEAD, bieżącej gałęzi czy wygenerowania „podobnych” plików przez agenta.
- Dodać jawne `Keep result`/unpin do istniejącej historii i wspólnej kwalifikacji retencji.
  Utrzymać obiekt Git własnym zweryfikowanym refem, jeśli ma pozostać dostępny mimo retencji
  zwykłego refa wyniku. Manifest/pin jest plikową prawdą; SQLite pozostaje indeksem.
- Aktywne restore i żywe usługi trzymają czasową ochronę źródła/katalogu. Retencja nie
  usuwa go między preview i operacją. Po zakończeniu zwalniać tylko własną ochronę.
  Pin nie jest obietnicą przetrwania ręcznego usunięcia dysku; brak nadal ma być jawny.
- Usuwanie przypiętego wyniku wymaga osobnej rzeczywistej zgody i listy tego, co znika.
  Walidacja containment, no-follow i tożsamość właściciela obowiązują również dla celu.

**Pliki:** `commands/{history,finalize,isolate,run,reconcile,sweep}.rs`, wspólna warstwa
plików/supervisor, bridge i run/past/. Nie dodawać automatycznej retencji poza tymi ścieżkami.

**RED/AC:** `a_saved_result_can_be_restored_without_an_agent`: kompletna fixture z
przypiętym OID; zmienić pomocniczy branch po zapisaniu wyniku. Wymagany SUKCES:
nowy katalog zawiera dokładnie historyczne pliki z właściwego OID, zero wywołań drivera.
Odmowa nie zalicza tego przypadku. Osobna fixture z rzeczywiście usuniętym źródłem
sprawdza odmowę. Aktywne dirty repo identyczne bajtowo. Non-git, niepełny manifest,
zajęty/symlinkowy cel, data/count
retention, pin/unpin, źródło usunięte po preview. Front klika rzeczywiste Open restored folder.

## 6. Nowe zadania: agenci uruchamiają i sprawdzają projekt

### WF-25 — Usługa ma własną kopię aż do dowodu śmierci

Priorytet: wysoki, przed budowaniem kolejnych obietnic preview. Zależności integracyjne:
WF-01/WF-03; współdzieli cleanup z WF-06. Zwykły Serve już istnieje i zostaje wykonawcą.

**Dzisiaj:** `run.rs:9441–9460` przekazuje proces do Processes. Po zakończeniu grafu
`2162 → 7834–7838` sprząta drzewa. Isolate.finish usuwa Git worktree po commit,
a close_one_copy usuwa non-git. Cleanup nie sprawdza, czy pozostawiony Serve nadal
czyta ten katalog. Dotychczasowy test żywej usługi używa Project, nie izolowanej kopii.

**Kontrakt:**

- Rozszerzyć istniejący HeldProcess o stabilne `ServiceRef { RunRef?, node_key?,
  service_id, generation }`, workspace ownership i sposób zakończenia. PGID to adres
  procesu, nie tożsamość pozwalająca rozstrzygać spóźnione operacje.
- Przed spawn przejąć prawo utrzymania własnej kopii; rollback oddaje je tylko po
  odmowie bez procesu albo dowodzie śmierci. Alive/unproven zawsze blokuje cleanup.
- Kilka usług może utrzymywać jedną kopię. Ostatni potwierdzony koniec zwalnia ją do
  istniejącej finalizacji/ochrony wyniku i cleanup dokładnie raz. Katalog nie znika
  po śmierci tylko jednej usługi, końcu agenta albo samego grafu.
- Jawne `lifetime: window | run`. Stare pliki zachowują window; nowe workflow preview
  może wybrać run. Run-owned usługi kończą się przy końcu/Stopie tego biegu. Window-owned
  pozostają widoczne z informacją o katalogu i osobnym Stop, aż do zamknięcia/świadomego Stop.
- Stop konkretnej usługi/run nie iteruje globalnie po wszystkich cudzych procesach.
  Stop run nie może mówić „nothing left running”, jeżeli zostawił window-owned preview.
- Rerun, recovery i retencja nie mogą podmienić ani skasować katalogu wciąż żywej usługi.
  Brak dowodu śmierci pozostawia również jej diagnostykę i własność katalogu.
- Żywy dev server może zapisywać cache w swoim cwd. Nie traktować takiego drzewa jako
  zamrożonego rodzica fan-in. Źródło do złożenia/archiwizacji wymaga dowodu stabilnego
  snapshotu albo zakończenia piszących usług; nie ignorować ich, bo Serve step już succeeded.

**Pliki:** `commands/{processes,run,isolate,reconcile,sweep}.rs`, `workflow/mod.rs`,
history/IPC/Started UI. Własność nadal w Processes, platforma nadal w supervisorze.

**RED/AC:** `a_live_preview_keeps_its_working_copy`: realny proces w fresh/same-copy
odczytuje plik po końcu grafu; katalog istnieje. Po ostatnim deathproof normalna
finalizacja/cleanup raz. Dwie usługi w jednej kopii, non-git, naturalny koniec, run Stop,
window lifetime, nieudana eskalacja, restart aplikacji i podmiana cudzej ścieżki.
Test nie może poprzestać na alive bool ani Project folder.

### WF-26 — Gotowość i adres konkretnej instancji

Zależność: WF-25. Rozszerzenie Serve, nie nowy kafelek.

**Dzisiaj:** Serve daje Succeeded zaraz po spawn (`run.rs:9453–9460`).
StartedProcess ma tylko command/pgid/alive (`processes.rs:63–77`), bez URL/readiness.

**Kontrakt:**

- Opcjonalne `ReadinessSpec`: rodzaj HTTP/TCP, adres względny do wybranego endpointu,
  timeout 1–120 s (domyślnie 30), oczekiwany wynik. Brak pola zachowuje legacy „started”,
  nigdy nie opisuje go jako potwierdzone „ready”. Próby nie zajmują miejsca agenta LLM.
- Z readiness krok zwalnia zależnych konsumentów dopiero po rzeczywistym probe.
  Exit przed ready, timeout i Stop nie dają sukcesu. Każdy timeout przechodzi supervisor.
- Endpoint: service ID/generation, nazwa, host, port, URL, stan, workspace/node źródła.
  Może być kilka nazwanych endpointów (web/api); nie globalne last_url.
- Przy automatycznym porcie host przydziela kandydat i przekazuje go jawną niesekretną
  konfiguracją. Weryfikować, że listener należy do właściwej grupy/instancji albo
  wskazana aplikacja zwraca sprawdzony znacznik instancji. Sam „wolny przed spawn”
  i późniejszy HTTP 200 nie wystarczają przez race. Operacje platformowe w supervisorze.
- Obcy zajęty port nie jest gotowością naszej aplikacji i nie uprawnia do kill.
  Nieobsługiwany sposób identyfikacji dynamicznego endpointu daje konkretną odmowę,
  nie zgadywanie z dowolnego logu. Nie wymagać zmiany kodu każdego repo tylko dla nonce,
  jeśli ownership listenera daje wystarczający dowód.
- Endpoint trafia przez strukturalny wynik usługi do przekazań/readiness context
  następników; QA dostaje URL i generation jako dane. Restart unieważnia stary uchwyt.
- Usługa, która umrze po ready, aktualizuje stan i diagnostykę. Nie utrzymywać zielonego
  „ready” dlatego, że readiness kiedyś przeszło; konsument musi móc odczytać bieżący stan.

**Pliki:** `workflow/{mod,check}.rs`, `commands/{run,processes}.rs`, CommandDriver,
supervisor, panel Serve i istniejąca kuracja/odczyt historii.

**RED/AC:** `a_preview_is_ready_before_its_consumer_starts`: serwer celowo opóźnia
gotowość, agent QA nie startuje wcześniej i dostaje własny URL. Obcy listener, early
exit, timeout, Stop, dwie kopie z różnymi portami, restart i crash po ready. Test
systemowego ownership nie jest mockiem boolean. Front pokazuje różnicę started/ready/failed.

### WF-27 — Agent rozpoznaje sposób przygotowania i uruchomienia repo

Zależności: WF-14, WF-26. To rozwinięcie commandFrom, nie obowiązkowy ręczny profil.

**Kontrakt:**

- Zachować podstawową ścieżkę: agent czyta repo i jego instrukcje, wykonuje dozwolone
  przygotowanie w swoim Agent/Bash albo proponuje jawne Check, przekazuje plan startu.
  Ten sam workflow ma działać w repo z różnymi poleceniami bez ręcznej zmiany frontend/backend.
- Dodać typowany `LaunchDescription`: command, względny subdirectory, niesekretne env,
  nazwane endpointy/readiness. Producent/pole/kopia/próba są wskazane jednoznacznie.
  Nie wybierać ostatniego dowolnego pasującego pola wśród wielu przodków.
- Sam graf i dopuszczony zakres wykonania nadal są zamrożone. Runtime wartość komendy
  może pochodzić od wskazanego agenta, jak dzisiaj commandFrom; to nie zgoda na dodanie
  nowej gałęzi, rozszerzenie sandboxu lub pobranie sekretu z sąsiedniego projektu.
- Walidować strukturę, containment cwd, konflikt producentów, wymagane zmienne,
  limity długości, skaner sekretów i politykę procesu przed spawn.
- Zachować obecny jawny wybór źródła: włączone commandFrom wybiera komendę agenta,
  wyłączone wybiera ręczną komendę. UI zachowuje nieaktywną ręczną wartość, ale brak
  wymaganego pola agenta nie może po cichu uruchomić tej starej komendy. Tak działa
  wykonanie `run.rs:9426–9432`; komentarz o pierwszeństwie ręcznej komendy przy
  `run.rs:9423` jest z nim sprzeczny i nie jest kontraktem kompatybilności.
- Env przekazywać wspólną drogą supervisora z env_clear i jawną listą, bez wrappera
  `env -i`. Nazwy sekretów mogą być wymaganiami, ale wartości nie trafiają do command,
  argv, handoffu ani logu. Nieobsługiwana droga dostarczenia sekretu to jawna blokada.
- Zależności/cache po fan-in nie są automatycznie obecne. Przygotowanie musi zostać
  wykonane i sprawdzone w kopii rzeczywistego startu. Usunięcie przygotowania z grafu
  usuwa to zachowanie; nie ma ukrytej instalacji przy otwarciu repo.
- Ręcznie zatwierdzone ustawienia startu mogą być ponownie użyte, ale są opcją.
  Nie budować uniwersalnego instalatora package managerów ani uprzywilejowanego bootstrapu.

**Pliki:** zakres WF-14 plus `workflow/{mod,check}.rs`, `commands/run.rs`,
`engine/drivers/command.rs`, wspólna konfiguracja supervisora, panel Serve/preparation.

**RED/AC:** `agents_discover_how_different_repositories_start`: dwa fixture repo
o różnych manifestach/komendach, ten sam graf, różne poprawne LaunchDescription.
Proces Serve i konsument pracują w przygotowanej kopii. Monorepo subdir, brak dependency,
brak env, sekret w komendzie, niejednoznaczny producent, zmiana tylko drugiej kopii.
Przy zapisanej starej ręcznej komendzie włączenie commandFrom uruchamia wyłącznie
komendę wskazanego agenta; brak jego wymaganego pola daje odmowę i zero procesów,
a wyłączenie commandFrom przywraca ręczne źródło.
Pełny żywy smoke rozpoznania przez model osobno od deterministycznego testu kontraktu.

### WF-28 — Narzędzia agentów do usług i dojście do przeglądarki

Zależności: WF-08, WF-16, WF-25–27. Rola Step nadal nie dostaje zarządzania całymi biegami.

**Kontrakt:**

- Dodać wyłączone domyślnie capability pracy z usługami w definicji agenta/nadpisaniu
  kroku. Host wiąże tożsamość, scope, dopuszczone usługi i katalogi z zamrożoną konfiguracją.
- Narzędzia nad tym samym Processes: `service_status`, `service_logs`, `service_start`,
  `service_restart`, `service_stop`. Start odnosi się do skonfigurowanej usługi i
  zweryfikowanego LaunchDescription WF-27, nie dowolnego argv/PGID/path z modelu.
- Użytkownik może skonfigurować usługę do agentowego startu, bez nowego kafelka i bez
  zmiany grafu w locie. Wybór serwisu, producenta konfiguracji, workspace i praw do
  restartu jest uprzedni; sam moment wywołania może należeć do uprawnionego agenta.
- Restart: atomowe przejęcie wskazanej generation → Stop i deathproof starej → spawn
  nowej → readiness → nowa generation. Dwa restart nie tworzą dwóch grup. Spóźniony
  Stop starej generation nie dotyka nowej ani obcej usługi.
- Shared preview udostępnia wybranym konsumentom status/URL/logi; prawo do odczytu
  nie daje automatycznie prawa do zatrzymania. Lead widzi te same źródła i capabilities.
- Logi są ograniczone i stronicowane, a bezpieczny końcowy ogon i exit reason zostają
  po usunięciu żywego wpisu. Nie utrwalać sekretów/pełnego dowolnego stdout poza istniejącą
  polityką evidence. Filtr w UI nie zastępuje kontroli odczytu backendu.
- QA z już skonfigurowanym natywnym MCP przeglądarki dostaje własny gotowy URL i może
  wykonać rzeczywistą nawigację/interakcję. Nie instalować MCP automatycznie i nie
  nazywać samego osiągalnego URL wykonanym testem UI.
- Rozróżnić dostęp do lokalnej aplikacji od dostępu do Internetu. Jeżeli dana polityka
  vendora nie pozwala przyznać żądanego dostępu bez szerszych uprawnień, zgłosić ograniczenie
  i potrzebną zgodę; nie podnosić po cichu reaches_the_web/sandboxu.
- Weryfikacja UI może czytać przygotowaną kopię, ale nie może startować na częściowym
  fan-in. Żywy Serve nie staje się uprawnieniem do równoległych kolidujących zapisów.

**Pliki:** `bridge/{verbs,library,host}.rs` zgodnie z rzeczywistym dispatch,
`commands/{processes,run,chat}.rs`, definicja capabilities/typy, istniejący panel Started,
handoff context i adapter konfiguracji narzędzi. Nie nowy shell/runner.

**RED/AC:** `agents_manage_only_their_project_services`: oba vendory jako dublerowane
sesje przez rzeczywisty most wykonują start/status/logs/restart/stop własnej usługi.
Odmowa obcego run/scope, starej generation i braku capability. Log błędu dostępny po
śmierci. Żywy odbiór dwóch vendorów + wybrane MCP: QA otwiera własny URL i sprawdza marker
oraz interakcję w aplikacji, nie tylko odczytuje tekst linku. Limit kosztu uzgodnić przed smoke.

## 7. Zależności i priorytety po rozszerzeniu

Stare ID zachowują znaczenie; nie przenumerowujemy kart. Pełny plan ma 28 kart.

| Karta nowa | Poprzedniki |
|---|---|
| WF-21 historia | WF-09 |
| WF-22 kontekst nowej rozmowy | WF-21 |
| WF-23 replay konfiguracji | WF-01, WF-07, WF-10, WF-13, WF-15 |
| WF-24 restore istniejącego wyniku | WF-01, WF-06, WF-21, WF-23 |
| WF-25 lifecycle usługi/kopii | WF-01, WF-03; wspólny cleanup WF-06 |
| WF-26 readiness/endpoint | WF-25 |
| WF-27 agentowe przygotowanie/start | WF-14, WF-26 |
| WF-28 narzędzia usług/QA | WF-08, WF-16, WF-25, WF-26, WF-27 |
| WF-20 odbiór końcowy | Dotychczasowi poprzednicy oraz WF-22, WF-24, WF-28 |

Kolejność priorytetów:

1. Integralność pracy: WF-04/05, WF-01/02/03, WF-06 oraz WF-25. Nie dokładać obietnicy
   „przywróć rezultat”, dopóki wynik i katalog mogą zniknąć.
2. Adresowany Start/status/sterowanie: WF-07/08/09/10; równolegle tylko rozłączne prace
   projektowe nad WF-21/22. Lead ma wiedzieć i umieć wskazać źródło przed rozbudową automatyzacji.
3. Instrukcje/skille/wejścia WF-12/13/14, gotowość i plan startu WF-26/27.
4. Wspólne snapshoty i scope WF-15/16; historia replay/restore WF-23/24;
   wiadomości WF-11 i usługi WF-28 na tych samych tożsamościach.
5. Lab WF-17/18/19 i końcowy WF-20. Nie rozbudowywać niezależnych implementacji
   snapshotu, zgód, processes i context scope dla każdego z tych ekranów.

To priorytety, nie zgoda na równoczesną edycję wspólnych plików. Rust i integracja
pozostają szeregowane. Nie ma potrzeby ponownie odpalać zakończonych Z-36–Z-50.

## 8. Poszerzony odbiór WF-20 i świadoma granica Labu

Do istniejących scenariuszy dodać:

- Zamknąć sesję, utworzyć nowy Threads: Lead zna zapisane, przyjęte ustalenie i potrafi
  znaleźć wskazany dawny bieg. Nie twierdzi, że pamięta niespisaną prywatną rozmowę.
- Zmienić workflow, agenta i ref wyniku po starym biegu. Recorded replay bierze
  zapisane źródła albo odmawia; restore materializuje właściwy OID bez wywołania modeli.
  Aktywne dirty repo jest nietknięte. Missing source nigdy nie staje się HEAD.
- Dwa repo o różnych komendach, ten sam workflow: agent rozpoznaje setup/start,
  przygotowuje własną kopię, uruchamia usługę, QA czeka na readiness i odwiedza właściwy URL.
- Dwie kopie/two endpoints: brak pomyłki portu/źródła. Restart i spóźniona operacja
  nie zatrzymują nowej instancji. Crash pokazuje końcowy log i stan, nie wieczne ready.
- Koniec grafu nie kasuje kopii window-owned preview. Stop run kończy tylko jego
  run-owned usługi; pozostałe są jawnie wskazane. Śmierć ostatniej usługi pozwala
  finalizować/sprzątnąć własny katalog dokładnie raz, z ochroną wyników/pinów.

**WF-20B — dodatkowy zakres tylko jeśli odbiór ma obejmować preview w Labie.**
Nie jest automatycznie częścią zamknięcia zwykłego Serve/QA. Wymaga rozszerzenia WF-17/19:
wyłącznie run-owned usługi z readiness, izolowane namespace portów/workspaces/contexts,
brak zewnętrznych mutacji, własne browser sessions, read-only dostęp wyroczni do wyniku,
przyczyny błędu rozróżniające aplikację i infrastrukturę. Egzaminator kończy się przed
run-owned cleanupem usług. Każda komórka ma własny endpoint związany z ServiceRef,
nie współdzielony port gospodarza. Bez tych warunków Lab ma odmówić pomiaru, nie
uruchomić pozornie niezależne warianty nad jedną aplikacją.

## 9. Weryfikacja wykonana podczas tej reweryfikacji

Na powyższym SHA uruchomiono **31 testów Rust i 39 testów frontendu — wszystkie przeszły**.
To zawężone regresje istniejących ścieżek, nie dowód ukończenia nowych kart.

Rust, każdy oddzielnie przez
`cargo test --manifest-path src-tauri/Cargo.toml --test it <moduł>:: -- --test-threads=1`:

| Moduł | Passed |
|---|---:|
| z39_the_lead_stops_a_run | 5 |
| lead_evidence_is_durable | 11 |
| serve_takes_its_command_from_the_step_before | 7 |
| serve_step_does_not_block_the_graph | 1 |
| parents_fold_into_one_copy | 2 |
| a_cell_needs_three_things_to_pass | 5 |

Frontend, każdy jako konkretny plik przez `npx --no-install vitest run <plik>`:

| Plik pod src/sections/ | Passed |
|---|---:|
| run/feed/a-suggested-stop-goes-the-one-way-a-run-is-stopped.test.ts | 3 |
| run/the-row-says-what-the-lead-can-do.test.tsx | 4 |
| workflows/step-panel/serve-takes-the-command-from-before.test.tsx | 8 |
| run/past/leftovers-reach-the-screen.test.tsx | 8 |
| lab/a-cut-off-run-is-not-the-agents-score.test.tsx | 5 |
| run/interrupting-a-long-tool-is-offered.test.tsx | 7 |
| run/a-resumed-run-can-be-stopped.test.ts | 4 |

Przeczytano dodatkowo produkcyjne ścieżki i testy nowych Z, ale nie przypisuje się im
wyniku wykonania w tej sesji. Nie uruchomiono pełnego CI ani płatnego smoke Claude/Codex.
Nie wprowadzono nowych reproduktorów awarii w main: wskazane luki są aktualnym dowodem
z kodu i niepokrytych scenariuszy, a opisane RED są zadaniami do wykonania.

## 10. Miejsce rozpoczęcia implementacji

Dokumentacyjny worktree nadal ma stary kod bazowy. Nie wolno uruchomić implementacji
na nim w przekonaniu, że zawiera powyższy main. Przed przydzieleniem pierwszej karty:
sprawdzić ponownie aktualny SHA i drzewo, zachować oba dokumenty, a kod implementacji
wyciąć od sprawdzonego main albo świadomie zintegrować tę bazę w osobnej gałęzi.
Ta reweryfikacja nie wykonała takiej operacji.

W harness/h.py nadal jest `STATE_DIR = ROOT / ".git" / "h"`. Samo przejście do
linked worktree nie rozwiązuje obsługi stanu przez harness; wcześniejsze ostrzeżenie
operacyjne pozostaje aktualne. Nie naprawiać chronionego harnessu przy okazji tych kart.
