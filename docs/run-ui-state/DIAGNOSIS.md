# Rozbieżność kafelków i brak planu — diagnoza oraz naprawa 0.4.1

Zgłoszenie właściciela: 2026-09-09, Loadout 0.4.0. Status: przyczyny potwierdzone; naprawa zlecona przez właściciela 2026-09-10.

## Dowody

- Bieg: `20260909-212616__01a08810-7c87-7470-9557-a765e095fc58`, projekt meetnotes.
- Odczyt jego `run.json` o 23:40 CEST: `running`; „Plan implementation” jest
  `running`, pozostałe 14 instancji kroków jest `pending`. Wszystkie trzy instancje QA
  mają `started_at: null`, `ended_at: null`, `error: null`.
- Screenshot właściciela pokazuje pod tym biegiem dwa kafelki „Implementation” ze
  stanem `done` oraz co najmniej dwa „QA” ze stanem `failed`, podpisane `first step`.
  Bieżący graf nie ma kroków nazwanych „Implementation”; QA zależy od Combine.
- Sam licznik „step 1 of 11” nie dowodzi błędu: workflow ma 11 węzłów, a po
  rozwinięciu pętli bieg ma 15 instancji. Nie zmieniać jego znaczenia bez analizy.

## Miejsca do prześledzenia

- `src/sections/run/index.tsx`: `factsOf` i `planFor` łączą część danych strumienia
  z krokami przez nazwę. To potencjalne miejsce przenikania starych informacji,
  ale nie wyjaśnia jeszcze pochodzenia dodatkowych kafelków.
- `src/state/run.ts`: `nowRunning` zastępuje listę kroków; `withStepStates` dopasowuje
  zdarzenia po `stepId`. Sprawdzić wszystkie drogi odtwarzania stanu i ich wejścia.
- `src/sections/run/io.ts`, historia i magazyny kart: prześledzić wybór biegu,
  odtworzenie snapshotu, kolejkowanie zdarzeń i spóźnione odpowiedzi IPC.

## Kryteria naprawy

1. Odtworzyć rozbieżność na zamontowanym ekranie przed poprawką: wcześniejszy bieg
   ma zakończone Implementation i nieudane QA, nowy bieg dopiero wykonuje plan.
2. Lista i stany kroków odpowiadają wybranemu biegowi. Tożsamość obejmuje bieg
   oraz konkretną instancję/wykonanie kroku; nazwa wyświetlana nie jest kluczem.
3. Zdarzenia i odpowiedzi odczytu starego biegu nie zmieniają kafelków nowego.
   Sprawdzić też przełączanie kart, ponowne podłączenie i odtworzenie historii.
4. Powtarzające się nazwy oraz kolejne próby pętli zachowują własne stany i relacje.
5. Potwierdzić wynik w rzeczywistej aplikacji na izolowanych danych. Nie restartować
   ani nie modyfikować aktywnego biegu właściciela podczas diagnozy.

Pierwsza diagnoza była tylko odczytem. Naprawa 0.4.1 nadal nie zmienia danych ani procesów tego biegu.

## Drugi objaw po zakończeniu biegu — 23:46 CEST

Bieg zakończył się jako `failed`. Tym razem czerwone stany jego rzeczywistych kroków
potwierdza `run.json`; należy odróżnić je od dodatkowych kafelków widocznych wcześniej.

### Potwierdzona przyczyna braku planu

- „Plan implementation” miał `plan.mode: create`, ale jego zapisane efektywne ustawienie
  dostępu do projektu wynosiło `fileAccess: look-only`.
- `library/agents.rs::policy_of` mapuje ten dostęp na `Policy::ReadOnly`; adapter Codeksa
  przekazuje sandbox `read-only`.
- `commands/run.rs::plan_agent` wybiera kandydata w
  `.loadout/plan-candidates/<agent-session-id>.json` wewnątrz katalogu projektu.
- O 23:44:32 CEST stderr zawiera odmowę: `patch rejected: writing is blocked by read-only
  sandbox; rejected by user approval settings`.
- Po 19 min 28 s agent zakończył proces z `exit_code: 0`, ale pliku nie było. Loadout
  odrzucił wynik: `the candidate file is missing. No plan was published.`
- To nie błąd JSON ani brak instrukcji Create: konfiguracja wymagała zapisu, którego
  polityka wykonania zabraniała. Obecny preflight sprawdza pozostanie ścieżki wewnątrz
  projektu, ale nie wykrywa tej sprzeczności.
- Treść analizy pozostała w `handoffs/00__plan-implementation__findings.md` (8014 bajtów).
  Nie jest to opublikowany dokument planu; nie uznawać jej automatycznie za taki dokument.

### Skutki

- Pozostałych 13 instancji agentów ma `process_started: false`. Ich komunikaty o braku
  planu są skutkiem pierwszej porażki; nie doszło do 13 osobnych awarii modeli.
- Polityka kontynuowania dopuściła przejście przez zależne kroki i próby pętli. Każda
  odmowa wygenerowała własny handoff, stąd 14 handoffów przy tylko jednym uruchomionym agencie.
- Osobny krok `serve` uruchomił `npm run dev`. Sam start serwera nie dowodzi realizacji zadania.
- Backend zapisuje cały bieg jako `failed`, ale nagłówek ekranu pokazuje zielone `Finished`.
  `strip/headline.ts::headlineFor` rozróżnia tylko Running / Finished / Ready to run;
  `state/run.ts::EndedRun` przechowuje nazwę i początek, bez wyniku zakończenia.

### Zakres dalszej naprawy

1. Rozdzielić prawo do publikacji wyniku planowania od prawa do modyfikacji projektu.
   Create/Update muszą mieć kontrolowany kanał dostarczenia dokumentu także dla planisty
   czytającego projekt. Niewykonalna konfiguracja musi być wykryta przed płatną pracą.
2. Pokazać jedną pierwotną przyczynę i odróżnić zależne kroki, których agent nie wystartował,
   od kroków, w których agent pracował i poniósł porażkę. Nie uruchamiać bezcelowych prób
   naprawczych dla brakującego wymaganego planu.
3. Doprowadzić zapisany wynik biegu do nagłówka oraz poprawnie rozróżniać porażkę,
   anulowanie i sukces także po powrocie z historii.
4. Weryfikacja regresji ma objąć Create + look-only na obu adapterach, publikację i odczyt
   tej samej wersji przez następny krok, odmowę przed startem przy niedostępnym kanale
   zapisu oraz widoczny wynik nieudanego biegu. Sama obecność handoffu lub exit 0 nie wystarcza.

## Rozstrzygnięcie i implementacja 0.4.1

Właściciel wybrał blokadę sprzecznej konfiguracji: Create/Update nie mogą pracować z
Look only. Wspólny walidator w `workflow/roster.rs` jest używany przez panel oraz Start;
rozstrzyga efektywne nadpisania i narzędzia, które dany vendor rzeczywiście ogranicza.
Szkic pozostaje zapisywalny; przycisk Ask first jawnie zmienia tylko wskazany krok.

Historia używała logicznego identyfikatora kafelka dla każdej próby QA, co dawało powielone
klucze Reacta. Teraz klucze obejmują fizyczne próby, a obraz remountuje się dla nowego biegu.
Stan magazynu, źródła kart i kanały są ograniczone do bieżącego uruchomienia. Wybór nowego
workflow usuwa też zapamiętaną migawkę poprzedniego wyniku.

Księga silnika wysyła kompletną migawkę `RunProgress`, zawierającą wynik, czasy, konkretne
próby i rzeczywiste zależności. Zakończenie IPC nie oznacza sukcesu. Końcowa migawka czeka
na miejsce w kolejce zamiast przepadać razem z nadmiarem transkryptu. Brak wymaganego planu
przerywa zależną gałąź przez istniejące pomijanie schedulera, mimo Carry on. Pominięte kroki
i serwery nie startują; niezależna gałąź może się skończyć.

## Weryfikacja przed lądowaniem

Czerwone asercje przed poprawką: niedozwolony autor planu akceptowany przez Start;
brak planu powodujący Failed zamiast Skipped u następcy; powielone identyfikatory historii;
zielony Finished po porażce; stare QA w nowym biegu; końcowy wynik gubiony przy pełnej kolejce.
Dodatkowo odtworzono powrót starego wyniku po zmianie workflow i błędną odmowę dla listy
narzędzi, której Codex nie stosuje.

Testy natywne używają izolowanych katalogów i uruchamiają rzeczywiste procesy atrap CLI
przez oba adaptery. Dowodzą też publikacji Create/Update, użycia właściwej wersji planu,
anulowania oraz limitu czasu. Testy UI sprawdzają prawdziwy renderer, a test przeglądarkowy
klika Run dwukrotnie i dostarcza migawki przez rzeczywisty obiekt Channel z atrapą Tauri.
To nie jest płatny test modeli ani automatyczny test kliknięć w WKWebView.
