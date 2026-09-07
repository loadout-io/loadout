# Loadout: niezawodność workflow, QA na pełnej aplikacji i generator agentów

Data: 2026-09-06. Status: **plan do implementacji, nie raport wykonania**.

Dokument jest samodzielnym przekazaniem dla kolejnego agenta. Nie wymaga znajomości rozmowy,
w której powstał. Nazwy nowych typów i plików oznaczone jako propozycje nie opisują istniejącego API.
Dokumentacja po polsku; wszystkie nowe teksty produktu po angielsku.

## 1. Cel i definicja ukończenia

Loadout ma wykonywać konfigurowalne, wieloagentowe workflow w różnych repozytoriach. W tej
iteracji domykamy konkretne awarie ujawnione w prawdziwym biegu i dodajemy tworzenie agentów
z opisu użytkownika.

Użytkownik ma móc:

1. Doprecyzować zadanie przez Leada podczas pracy kroku bez zgubienia wiadomości i bez
   potraktowania dodatkowej tury jako zawieszenia procesu.
2. Odróżnić zakończenie procesu, deklarację agenta i faktycznie wykonane sprawdzenia.
3. Skonfigurować workflow, w którym wymagane testy oraz wymagane scenariusze użytkownika
   nie mogą zostać zastąpione ogólnym zdaniem `pass`.
4. Zlecić QA uruchomienie **pełnej aplikacji z prawdziwym backendem**, na końcowym wyniku
   połączenia prac, z osobnymi danymi testowymi i możliwością obsługi UI.
5. Napisać opis roli, kliknąć **Create with Codex** albo **Create with Claude**, otrzymać
   dopasowany szkic agenta, sprawdzić go i zapisać do istniejącej biblioteki.
6. Odtworzyć z historii: co zbudowano, co sprawdzono, czego nie sprawdzono, jakie wiadomości
   wpłynęły na pracę oraz dlaczego bieg zakończył się danym wynikiem.

„Perfekcyjny agent” oznacza tu: spójną rolę, poprawną konfigurację dla wybranego vendora,
rozwiązane odniesienia do skilli i narzędzi, jawne ograniczenia i możliwość próbnego uruchomienia.
**Nie oznacza gwarancji bezbłędnego zachowania modelu.** Generator nie ma prawa sam przyznać
etykiety „verified” na podstawie jakości własnego promptu.

### Granica zakresu

- Część L/V/P/G/E poniżej dotyczy **Loadouta**.
- Część M dotyczy **Murmura/meetnotes**, czyli wyniku badanego workflow. To osobna ścieżka
  naprawcza i osobne repo. Nie wbudowujemy reguł kolejki nagrań w silnik Loadouta.
- Ten dokument nie zleca uruchomienia nowych płatnych biegów, zmiany aktywnej biblioteki
  użytkownika, restartu aplikacji ani publikacji release'u podczas samego przygotowania planu.
- Agent implementujący ma przestrzegać uprawnień otrzymanych w swojej sesji. Dokument nie
  zastępuje zgody na chronione pliki, instalację połączeń, dostęp do mikrofonu czy publikację.

## 2. Baza kodu i bezpieczne przejęcie pracy

### Stan zastany przy pisaniu dokumentu

| Miejsce | Stan |
|---|---|
| `/Users/jakubgawronski/Projects/Loadout` | `main`, HEAD `715567effb059ec3e684de2ba17d231b854d0b04`; przed zapisaniem planu czysto |
| `/Users/jakubgawronski/Projects/loadout-workflow-reliability-build` | branch `workflow-reliability-build`, ten sam HEAD, duży niezacommitowany WIP |
| WIP, zmiany śledzone | 119 plików, 12618 dodanych i 1887 usuniętych linii; dodatkowo pliki nieśledzone |
| Badany wynik Murmura | zapisany commit `178cc24acc9817c95bdc54328a9e5d8299293e6e`, lokalny branch `feat/recording-processing-queue` |

**Sam SHA `715567ef` nie identyfikuje testowanej wersji Loadouta.** Zachowania opisane niżej
obejmują niezacommitowany WIP. Wiele wskazanych modułów istnieje tylko w tym worktree.
Nie przedstawiaj ich jako zmergowanych lub certyfikowanych na `main`.

W chwili audytu uruchomiona wersja Loadouta pochodziła z
`loadout-workflow-reliability-build/target/debug/loadout`. PID i porty są nietrwałe: ustal je
ponownie, nie kopiuj starych identyfikatorów procesów do komend zatrzymywania.

### L-00 — przyjęcie bazy i własności

**Priorytet P0. Zależności: brak.**

1. Przeczytaj `AGENTS.md`, `docs/DECISIONS-LOCKED.md` i instrukcje właściwego repo.
2. Read-only: sprawdź HEAD, `git status`, worktree, aktywne procesy, ewentualne `OWNS`
   i innych wykonawców. Zapisz krótką listę obszarów już zaimplementowanych w WIP.
3. Nie rób `reset`, `checkout --`, `clean`, masowego stashowania ani automatycznego
   commita całego cudzego WIP. Nie zakładaj, że to wszystko należy do tego planu.
4. Uzgodnij z właścicielem istniejącej pracy sposób zamrożenia/przeniesienia bazy.
   Preferowany wynik: uzgodniony snapshot istniejącej implementacji i osobny worktree
   `loadout-reliability-native-qa-generator` na gałęzi `feat/reliability-native-qa-generator`.
   To **proponowana, jeszcze nieutworzona** ścieżka obok repo.
5. Alternatywa: kontynuacja w istniejącym worktree tylko po przejęciu jego własności.
   Nie rozwijaj aktywnie obserwowanego przez Tauri kodu podczas sesji użytkownika.
6. Oddziel ograniczone commity funkcjonalne od integracji. Nie uruchamiaj dwóch ciężkich
   Cargo/full-CI jednocześnie. Testy w pętli są zawężone, pełna suita przy lądowaniu.

**Odbiór:** znana baza obejmująca wymagany WIP, lista zachowanych zmian, brak naruszonych
procesów/aktywnych danych i jednoznaczny katalog implementacji.

### Wiążące ograniczenia architektury

- `docs/DECISIONS-LOCKED.md` wygrywa z ogólnymi/starszymi zapisami `AGENTS.md`.
- D3: wybór vendora pozostaje dowolny; cross-vendor można zaproponować w szablonie.
  Doradcza recenzja kodu nie staje się ukrytym obowiązkowym wetem.
- D6/D7: żadnych zaszytych etapów QA/review/generation w schedulerze. Korzystamy z istniejących
  rodzajów kroków, warunków grafu, punktów kontrolnych i deklaracji uruchamiania usług.
  Generator agentów jest funkcją biblioteki, nie nowym rodzajem kafelka.
- Prosty workflow bez sprawdzeń nadal działa, ale nie jest prezentowany jako zweryfikowany.
- `engine/` bez Tauri. Procesy, środowisko i platformowe szczegóły przez supervisor.
  Pliki pozostają prawdą, SQLite indeksem zapisywanym tylko przez istniejącego writera.
- Nie wprowadzamy nowego daemona, event busa ani równoległego harnessu. Rozszerzamy małe
  istniejące kontrakty. Nowy zapis na dysku musi mieć wskazanego czytelnika.
- Zmiana `harness/`, `checks/`, `scripts/`, `AGENTS.md` lub decyzji zablokowanych wymaga
  osobnej zgody zgodnie z §7. Plan wskazuje takie bramki, nie udziela tej zgody.

## 3. Incydenty, które plan musi usunąć

Źródło lokalne, prywatne:

`/Users/jakubgawronski/Projects/meetnotes/.loadout/runs/20260906-165702__01a077a6-e8de-78e1-b480-62546b987624/`

W dalszej części skrót `RUN` oznacza ten katalog. Nie kopiuj całych prywatnych transkryptów
do publicznych fixture'ów; przygotuj minimalne syntetyczne odtworzenia.

| ID | Fakt / granica dowodu | Co musi go pokryć |
|---|---|---|
| I-01 | Architect przyjął wiadomość Leada; po pierwszym `result` zaczął następną turę; Loadout zamknął wejście i zatrzymał proces | L-01 |
| I-02 | Lokalny zapis miał `infrastructure-failed`, eksport pokazał `unknown` | L-02 |
| I-03 | Backend próbował dopisać 13 testów; `cd src-tauri` nie powiodło się; potem raportował testy jako istniejące | V-01 |
| I-04 | Filtry Backendu uruchomiły 0 albo 2 niepowiązane testy; samo końcowe zdanie nie ujawniło problemu | V-01, V-02 |
| I-05 | QA opisało braki obowiązkowych zachowań, ale dało `outcome: pass`; instrukcja mówiła „good enough to build on” | V-02, V-03 |
| I-06 | Wymagania dopuszczały mocked-IPC Playwright **albo** samo uruchomienie dev-app bez abortu | P-03, V-03 |
| I-07 | QA używało implementacyjnego Jarvisa; narzędzia sesji: Bash/Edit/Glob/Grep/Read/WebFetch/WebSearch/Write, brak MCP | G-03, V-03, P-03 |
| I-08 | Zależność Cargo od sąsiedniego `murmur-server` wymagała ręcznego ratowania środowiska | P-01 |

Miejsca dowodowe: `RUN/run.json`, log Architekta `agent-...e905-7c40-8d4f-2c99c2cad7bf.jsonl`,
log Backendu `agent-...e8f3-7ef1-b7e1-429ea67e7ae1.jsonl` (zapis testów: okolice linii 253),
pełne przekazanie QA `attachments/09__qa__findings__full.md`.

Co już działało: 9 rzeczywiście uruchomionych agentów, prawdziwe nakładanie się procesów,
łączenie prac i zapis wyników, 4 niewykorzystane iteracje pokazane jako `notRun` w eksporcie.
Końcowe QA rzeczywiście uruchomiło 3725 testów Rust i 533 testy Chromium; nie zastępuje to
sprawdzenia pełnego produktu. Nie implementuj ponownie fan-in ani równoległości tylko dlatego,
że wynik całego biegu był czerwony.

## 4. Minimalne kontrakty współdzielone

Rozszerz istniejące typy tam, gdzie już mają właściciela. Poniższe nazwy są kontraktem
semantycznym, a nie poleceniem dodania sześciu nowych frameworków.

### 4.1. Adres wiadomości i zakończenie kroku

Adres zawiera `runId`, identyfikator **wykonania** kroku, próbę/iterację i generację sesji.
Sam `node_key` nie wystarcza przy retry/pętli. Wiadomość ma ID, uporządkowanie lokalne
i odróżnione fakty: przyjęta przez Loadout, przekazana transportowi, obsłużona w zakończonej
turze, odrzucona albo z nieznanym wynikiem dostarczenia.

To nie jest deklaracja, że model semantycznie zastosował instrukcję. UI nie nazywa przyjęcia
przez kanał „przeczytaniem”. Restart po niepewnym wysłaniu nie może automatycznie ponowić
wiadomości ze skutkami ubocznymi. Nie obiecujemy exactly-once pomiędzy obcymi procesami.

### 4.2. Tożsamość sprawdzanego wyniku

Każdy wymagany wynik Check/QA odnosi się do: run, konkretnego wykonania kroku, kopii końcowej,
rewizji kryteriów i snapshotu istotnych plików. SHA nie wystarcza przy brudnym worktree.
Snapshot uwzględnia nowe nieśledzone źródła, konfigurację i lockfile, a pomija wyłącznie
zatwierdzone katalogi wynikowe/caches. Dla folderu bez Gita używa istniejącego manifestu plików.

Zamiast dodatkowego konkurencyjnego rejestru wykorzystaj istniejące saved inputs/results
oraz `evidence.rs`. Zapis czytają Check, widok wyników, Lead i Lab. Zmiana wejścia po teście
unieważnia aktualność wyniku, nie przepisuje historycznego sukcesu na porażkę.

### 4.3. Kryterium i wynik weryfikacji

Kryterium: stabilne ID, opis zachowania widocznego dla człowieka, czy wymagane, metoda
sprawdzenia, oczekiwany scenariusz/test, potrzeby środowiska. Metody odróżniają test
automatyczny, UI z mockiem, pełny runtime i potwierdzenie człowieka.

Wynik: `passed | failed | not-tested`, ID kryterium, powód, odwołanie do istniejącego
wyniku wykonania. Nieznane ID/duplikaty/brak wymaganych wyników nie mogą dać zielonego.

- Fail: zaobserwowano niespełnienie kryterium.
- Not tested: brak narzędzia, uprawnienia, zależności lub wiarygodnego pomiaru.
- Niesprawdzone wymagane kryterium nie jest ani zaliczeniem, ani dowodem wady produktu.

Istniejący `ExternalAssessmentV1` ma rozróżnienie `Passed/DidNotPass/NotJudged`. Zachowaj
jego semantykę i zgodność starych zapisów; nie interpretuj dowolnego stdout agenta jako paragonu.
Samo poprawne JSON od modelu nie dowodzi prawdziwości asercji.

### 4.4. Konfiguracja aplikacji do testów

Rozszerzenie istniejącego `LaunchDescription`, nie drugi launcher: komenda uruchomienia,
podkatalog względem przypisanej kopii, jawne wymagane zmienne, endpointy, readiness,
typ celu (`web`, `native`, `cli`), sposób izolacji danych i sposób obsługi/testowania.

Domyślnie nie zapisuj absolutnych ścieżek użytkownika w przenośnej definicji. Sekrety
pozostają referencjami do istniejącego mechanizmu Connections; nie treścią pliku launch.
Readiness HTTP/TCP nie dowodzi połączenia UI z prawdziwym backendem.

### 4.5. Generowanie: wykonawca a tworzony agent

**MVP przyjmuje jawną konwencję:** Create with Codex używa Codeksa do przygotowania agenta
`runsWith=codex`; Create with Claude używa Claude Code do przygotowania agenta
`runsWith=claude-code`. Nie ma ukrytego użycia Leada innego vendora.

W modelu żądania rozdziel `generatorVendor` od `targetVendor`, lecz w tym UI oba są równe.
Nie dodawaj na razie czterech kombinacji ani nowego selektora, jeśli użytkownik ich nie zamówi.

Wynik to **szkic istniejącego Agent**, nie surowy `.claude/settings.json` lub `config.toml`.
Generator nie zapisuje globalnych konfiguracji CLI, nie instaluje skilli/serwerów, nie
uruchamia workflow i nie modyfikuje repo na podstawie wygenerowanej treści.

## 5. Zadania L — komunikacja, historia i uczciwe statusy

### L-01 — jedna kolejka tur i atomowe zamknięcie

**P0; po L-00.**

Obszar: `commands/step_message.rs`, `commands/run.rs` (`one_turn`,
`finish_completed_agent_turn`, `Ended::Turn`), `engine/drivers/mod.rs`, `claude.rs`,
`codex.rs`; platformowe zamykanie nadal wyłącznie przez `engine/supervisor.rs`.

Implementacja:

1. Wyznacz jednego właściciela lifecycle sesji. Wysyłający nie klonuje swobodnie `Voice`
   i nie decyduje niezależnie, czy krok jeszcze przyjmuje pracę.
2. Kolejkuj doprecyzowania w Loadoucie. Po wyniku bieżącej tury wyślij kolejną, jeśli
   zaakceptowana wiadomość czeka. Nie polegaj wyłącznie na vendorowym `queued_turn_count`.
3. Sprawdzenie pustej kolejki i zamknięcie przyjmowania wykonaj atomowo, bez trzymania
   `std::sync::Mutex` przez await. Dopiero potem zamykaj stdin/transport.
4. Wynik końcowy/handoff pochodzi z ostatniej obsłużonej tury, nie pierwszej odpowiedzi.
   Zsumuj usage/koszt tur dokładnie raz; czas i budżet dotyczą całego kroku.
5. Ogranicz długość kolejki i rozmiar wiadomości; pełna kolejka ma jawną odmowę, nie
   nieograniczone oczekiwanie pod blokadą. Nie grupuj samowolnie kilku instrukcji w jedną.
6. Na anulowaniu przestań przyjmować pracę, rozlicz oczekujące wiadomości i zatrzymaj
   własną grupę procesu przez supervisor. Nie resetuj deadline po każdej wiadomości.
7. Dla vendora bez potwierdzonej obsługi następnej tury zwróć Unsupported. Sam kanał
   istniejący w pamięci nie dowodzi możliwości komunikacji.
8. Trwałe metadane wiadomości dołącz do istniejącej prywatnej historii. Nie dokładaj
   promptów do argv, plików tymczasowych lub logów diagnostycznych.

Testy: nowy moduł `step_turn_queue_is_atomic` w celu integracyjnym `it`, plus rozszerzenie
`step_message_capability_matches_the_session` i UI testu wiadomości.

**AC:** wszystkie harmonogramy wiadomość/result/close; dwie kolejne wiadomości; stale
generation; retry tego samego node; disconnect przed/po wysłaniu; restart po niepewnym
dostarczeniu; timeout podczas drugiej tury; cancel; brak cross-run delivery; suma usage;
zwykła jedno-turowa sesja nadal kończy się bez sztucznego opóźnienia. Użyj barier testowych,
nie szczęśliwych sleepów. Test I-01 pada na starym kodzie w wykonaniu.

### L-02 — przyczyna awarii i wynik widoczne także dla Leada

**P0; po L-01 w obszarach wspólnych.**

Obszar: `commands/diagnostics.rs`, `engine/line.rs`, `commands/lead_history.rs`,
`commands/history.rs`, `commands/replay.rs`, widoki `src/sections/run/`.

- Mapuj zapisane `end_cause` przed heurystyką opartą o `exitCode`/obecność artefaktów.
  Zachowaj fallback dla starych raportów bez nowego pola.
- Rozróżnij normalne zakończenie procesu, błąd infrastruktury, anulowanie, ocenę agenta,
  wynik sprawdzeń i brak pomiaru. Nie twórz dla tego sześciu równoległych maszyn stanów.
- `CarryOn` nie zmienia historycznej awarii na sukces. UI jednym zdaniem pokazuje,
  że bieg kontynuował mimo błędu konkretnego kroku.
- Lead otrzymuje źródłowy run/step i aktualność sprawdzenia, również po ponownym otwarciu
  rozmowy. Brak wyniku runtime ma być widoczny w jego briefing/history lookup.
- Replay zachowuje starą historię. Przy niepewnej wiadomości proponuje świadome ponowienie,
  nie uruchamia sam starej komendy. Nie zmienia już zapisanego biegu użytkownika.

**AC:** fixture I-02 nie daje `unknown`; stary raport nadal się otwiera; komunikat znajduje
się na rzeczywistym ekranie i w wyniku narzędzia Leada; brak duplikowania jednego faktu;
zero wycieku promptów/sekretów w eksporcie diagnostycznym.

## 6. Zadania V — sprawdzenia i akceptacja

### V-01 — Check potwierdza właściwe testy na właściwym kodzie

**P0; po L-00.**

Obszar: `engine/drivers/command.rs`, `engine/drivers/command/assessment.rs`,
`workflow/mod.rs`, `workflow/check.rs`, `evidence.rs`, `commands/run.rs`,
`commands/run/protection.rs`, formularz Check i widok wyniku.

1. Użyj istniejącego Check/assessment; nie twórz drugiego runnera w każdym vendorze.
2. Dla trybu testowego potrzebny jest nie tylko licznik >0, ale tożsamość wymaganych
   testów/scenariuszy i raport ich wykonania. Brak choć jednego wymaganego ID to brak
   potwierdzenia. Dodatkowe testy nie zastępują wymaganych.
3. Bazowe adaptery do struktur wynikowych Cargo oraz używanego runnera frontendowego
   mapują wyniki na jeden kontrakt. Discovery testów i ich wykonanie dotyczą tego samego
   snapshotu. Nie wystarczy istnienie funkcji lub nazwy w źródle.
4. Komenda ma jawny cwd od właściciela kopii; sprawdź istnienie katalogu przed spawn.
   Dla kontrolowanych komend preferuj executable/args/cwd. Zachowaj zgodność starego
   command-string, ale nie traktuj `test | tail` jako wiarygodnego kodu wyjścia testu.
5. Build/lint to osobny rodzaj sprawdzenia z właściwą asercją wyniku; nie udawaj, że
   musi wypisać licznik testów. Nie można nim zaspokoić kryterium „testy wykonane”.
6. Zapisz związek wyniku ze snapshotem (§4.2). Czytelnik nie może zaakceptować starego
   logu z `/tmp` ani pliku wyniku podstawionego przez inny krok.
7. Wymagania, zatwierdzone polecenia i zewnętrzny egzaminator pozostają niezapisywalne
   dla mierzonego biegu. Reużyj istniejącej ochrony; bez pozornego sandboxu z promptu.
8. Błąd w eksperymencie agenta nie kończy automatycznie zadania. Wymagany Check ocenia
   gotowy rezultat. Dzięki temu RED-before-GREEN nadal jest możliwe.

**AC:** exit 0 + 0 passed; 2 stare testy zamiast 13 wymaganych; brak pliku po błędnym cwd;
częściowy/uszkodzony raport; test przerwany sygnałem; stderr podszywający się pod wynik;
stary wynik z innej próby; usunięty test; źródło zmienione po zaliczeniu; poprawna suita
z rzeczywistymi ID przechodzi. UI rozróżnia Failed i Not tested.

Planowane testy: `required_checks_run_the_requested_tests`, `check_results_bind_to_source`;
rozszerz istniejące `checks_evidence_*`, jeżeli pokrywają tę samą granicę.

### V-02 — obowiązkowe kryteria nie znikają w podsumowaniu

**P0; po V-01.**

Obszar: istniejące workflow/handoff/assessment/Lab oraz mały wspólny moduł normalizacji
kryteriów, jeśli obecne typy nie wystarczą. Nie rozrzucaj tej samej polityki po adapterach.

- Użytkownik zatwierdza listę wymagań przy przygotowaniu workflow/przed pracą; agent może
  ją zaproponować. Domyślny prosty workflow nadal nie wymaga listy.
- Dodaj kontrakt §4.3. Oddziel obowiązkowe zachowanie od uwagi doradczej o stylu kodu.
- Weryfikator nie może usunąć wymagania, obniżyć metody z native do mock ani samodzielnie
  zwolnić obowiązkowego sprawdzenia bezpieczeństwa.
- PASS jest wyliczany z kompletności i wyników zatwierdzonych kryteriów, a nie przez
  przeszukiwanie prozy po słowach „non-blocking”. Parser prozy nie rozwiązuje semantyki.
- Niesprawdzone kryterium wymaga jawnego statusu i wyjścia do człowieka/uzupełnienia
  środowiska. Nie dokręca nieskończonej pętli naprawczej kodu.
- Przeprowadź to rozróżnienie przez istniejące `RouteEvidence`/`Condition` i routing:
  obecne `CheckOutcome` ma tylko Passed/Failed. Dodaj wstecznie zgodne reprezentowanie
  NotJudged dla nowych grafów, nie mapuj go na Failed ani na zaliczenie. Brak jawnie
  skonfigurowanej drogi dla takiego wyniku ma zatrzymać wykonanie z wyjaśnieniem, nie
  wybrać przypadkową gałąź. Stare pliki zachowują dotychczasowe znaczenie swoich pól.
- Zmiana wymagań przez użytkownika podczas biegu tworzy rewizję. Graf pozostaje
  zamrożony. Zmiana zakresu/metody wymagająca nowego etapu zatrzymuje się na decyzji
  właściciela i nowym biegu; agent nie przerabia aktywnego grafu.
- Wczesne niepowodzenia historycznych prób nie są kasowane, a końcowa poprawna próba
  może spełnić kryterium. Zachowaj pochodzenie wyniku próby.

**AC:** missing mandatory, duplicate ID, unknown ID, pass + failed item, pass + not-tested
item, runtime zastąpiony mockiem, kryterium z wcześniejszej rewizji; poprawna kompletna
lista; stary workflow bez kryteriów nadal działa jako niesprawdzony. Testy obejmują widok
użytkownika, a nie tylko funkcję wyliczającą wartość.

### V-03 — workflow implementacja → sprawdzenia → funkcjonalne QA

**P0; po V-02 i P-03.**

Obszar: konfiguracja/szablon workflow, definicja roli QA, `workflow/unroll*`, routing
warunkowy i ewentualnie protokół wyniku pętli. Nie dodawaj gałęzi `if qa` w schedulerze.

Docelowe relacje (ich zakodowanie musi pozostać danymi grafu):

```text
Plan → [Backend || Frontend] → Combine → wymagane Check → uruchomienie aplikacji → QA
                                ↑           │                                  │
                                └── naprawa po potwierdzonym niespełnieniu ───────┘
                                            brak pomiaru → decyzja człowieka
```

- Gdzie to bezpieczne, niezależne gałęzie zachowują równoległość. Ciężkie Cargo są
  szeregowane zgodnie z polityką hosta, nie przez wyłączenie całej współbieżności.
- QA używa odrębnej definicji roli: testuje zachowanie, nie dostaje bazowej instrukcji
  „implementuj”. Nie zmienia kodu, kryteriów ani scenariuszy podczas werdyktu.
- Dla testów mutacyjnych używa osobnej, odtwarzalnej kopii, nigdy żywego badanego wyniku.
- Usuń zachętę „just pass next”. Wzmocnij instrukcję **tego workflow**, nie wszystkie
  użycia uniwersalnego `OUTCOME_ASKED_FOR`: kryterium weryfikacji to wykonanie wymagań,
  a nie tylko „good enough to build on”.
- Maksymalnie dwie poprawki po początkowej implementacji. Sprawdź rzeczywiste znaczenie
  `max_turns`, nie zakładaj, że wartość 2 oznacza dwie poprawki.
- Powrót kieruje do właściwego wykonawcy i właściwej kopii. Jeśli wznowienie tej samej
  sesji jest obsługiwane i bezpieczne, wykorzystaj je; w innym przypadku nowa sesja
  dostaje kompletny handoff i jawne oznaczenie kontynuacji. Nie używaj ślepo `--continue`.
- Doradcza recenzja kodu pozostaje doradcza (D3). Weryfikacja konkretnego wymagania
  funkcjonalnego jest osobną rolą i jawnym elementem grafu.
- Nie edytuj istniejącego `Murmur-1` ani globalnego Jarvisa w miejscu. Przygotuj nową
  wersję do obejrzenia/importu. Stare runy mają zachować swój snapshot.

**AC:** rzeczywisty fail → poprawka → ponowne wykonanie wymaganych Check/QA → pass;
dwie nieudane poprawki → stop; missing runtime → Not tested, nie pass ani kolejna
bezsensowna poprawka; brak dostępnego vendora nie udaje defektu funkcjonalności.

## 7. Zadania P — przenośność i QA na prawdziwej aplikacji

### P-01 — kontrola środowiska i dziedziczenie konwencji

**P1; po L-00.**

Obszar: `inherit/`, `skills/`, `commands/skills.rs`, `commands/run_inputs.rs`,
`workspace_inputs.rs`, `input_snapshot.rs`, istniejący Project Guide/instructions resolver.

- Jeden resolver instrukcji i referencji dla Leada, generatora i kroków. Nie kopiuj
  reguł repo do długiego stałego promptu agenta.
- Rozpoznaj katalog repo/workspace, wersje lockfile, lokalne/path dependencies,
  komendy build/test/run i wymagane narzędzia. Preferuj zatwierdzone deklaracje repo;
  auto-detekcja jest propozycją, nie zgodą na uruchomienie dowolnego skryptu.
- Przy zależności od sąsiedniego repo pokaż konkretny brak i sposób rozwiązania:
  zatwierdzone dołączenie zależności/read scope albo przygotowanie osobnej kopii.
  Nie twórz sam symlinków do dowolnych prywatnych katalogów.
- Źródła skilli mają pochodzenie i kolejność dziedziczenia. Niedostępny skill/connection
  nie może zostać zamieniony na twierdzenie, że agent go posiada.
- Cache dotyczy konkretnego snapshotu konfiguracji. Przeniesienie agenta do innego repo
  wymusza ponowne rozwiązanie referencji, nie ponowne generowanie całej roli.

**AC:** repo z instrukcjami lokalnymi; monorepo; brak Gita; repo ze spacjami w ścieżce;
brak zależności `../...`; niezatwierdzony skrypt setup; błędny cwd; zmieniony lockfile;
brak skilla; oba vendory dostają właściwe, aktualne instrukcje bez mieszania projektów.

### P-02 — izolowana instancja aplikacji należąca do QA

**P0 dla native QA; po P-01.**

Obszar: `workflow::LaunchDescription`, `commands/processes.rs`, `processes/launch.rs`,
`readiness.rs`, `services.rs`, `bridge/library/services.rs`, `engine/supervisor.rs`,
uprawnienia aplikacji w formularzu agenta/kroku, istniejący widok podglądu.

1. Reużyj istniejących `ServiceGrant`, `ServiceRef`, generacji, CopyLease i procesu
   zarządzanego. QA dostaje read/start/restart/stop tylko dla wskazanej usługi testowej.
2. Instancja jest uruchamiana z końcowej kopii Combine i przypisanej wersji źródeł.
   Przy poprawce poprzednią instancję należy zakończyć i zweryfikować nową wersję.
3. Osobne katalogi danych/cache, testowe nagrania i porty. Nie nadpisuj `HOME` ani
   globalnych konfiguracji użytkownika. Używaj jawnego, wspieranego przez aplikację
   testowego app-data override. Jeśli aplikacja go nie ma, to brak adaptera, nie izolacja.
4. Nie stosuj `open -a` wskazującego zainstalowany stary release. Potwierdź executable,
   cwd, wersję/snapshot, tożsamość instancji i prawdziwy backend.
5. Gotowy port nie wystarcza: odpowiedź kontrolna musi identyfikować instancję testową.
   Zajętego portu nie odzyskuj zabijaniem jego nieznanego właściciela.
6. Dla native app potwierdź także okno/proces. Sam działający `ng serve` spełnia tylko web.
7. Anulowanie/awaria/restart kończą tylko własne procesy; brak dowodu śmierci pozostaje
   widoczny i blokuje start kolidującej instancji. Domyślny lifetime QA to run, nie okno
   użytkownika. Pozostawienie podglądu po biegu jest osobną świadomą decyzją.
8. Rejestr zapisuje wystarczającą tożsamość do bezpiecznego recovery, ale nie sekrety
   środowiska. UI pokazuje jedną kartę aplikacji, nie powiela jej stanu w kilku panelach.

**AC:** zajęty port; obca instancja z tym samym tytułem; dwa QA różnych runów; brak
app-data override; readiness z obcego serwera; stale ServiceRef; cleanup po cancel;
przeżywający potomek; start po poprawce nie testuje poprzedniej wersji.

### P-03 — narzędzie obsługi UI i scenariusze native

**P0; po P-02. To zadanie ma jawny punkt rozpoznania możliwości, nie wolno go pominąć.**

#### P-03a: wybór działającej drogi automatyzacji

- Sprawdź rzeczywiście dostępne lokalne połączenia/narzędzia do sterowania UI macOS
  i ograniczenia aplikacji docelowej. Flaga CLI, nazwa narzędzia lub deklaracja modelu
  nie dowodzą, że połączenie działa.
- Preferuj istniejące, zatwierdzone połączenie z możliwością wskazania konkretnego PID/
  okna. Nie pisz własnego ogólnego systemu Computer Use jako skutku ubocznego.
- W minimalnej próbie trzeba uruchomić testową natywną instancję, znaleźć jej UI,
  wykonać jedną akcję i potwierdzić efekt z prawdziwego backendu.
- Jeśli nie ma odpowiedniego połączenia, przedstaw dokładnie: brakujący adapter,
  potrzebne uprawnienia i minimalny zakres integracji. Instalacja/nowe uprawnienia
  wymagają zgody. Do ich zapewnienia status native QA pozostaje Not tested.
- Nie zastępuj tej bramki Chromium, Safari/WebKit w Playwright, samym HTTP ani
  frontendowym `window.__TAURI_INTERNALS__.invoke` podmienionym na mock.

**Odbiór P-03a:** wskazana i sprawdzona droga od narzędzia vendora do własnego okna i
realnego backendu, albo jawny blocker właścicielski. Bez tego nie oznaczaj P-03 ukończonego.

#### P-03b: integracja z krokiem QA

- Podłącz zatwierdzone narzędzie przez istniejące Connections/bridge. Sprawdź uprawnienia
  zarówno w katalogu konfiguracji, jak i podczas inicjalizacji sesji obu vendorów.
- Przy każdej akcji ogranicz odbiorcę do tożsamości aplikacji testowej. Utrata okna/PID
  lub przeniesienie fokusu nie może skierować kliknięcia/pisania do aplikacji użytkownika.
- Testy korzystają z danych syntetycznych. Mikrofon, system audio, Touch ID, Accessibility
  i Screen Recording są realnymi uprawnieniami; model ich nie nadaje. Nie nagrywaj
  prywatnej rozmowy w ramach automatycznego testu.
- QA wykonuje scenariusze, a nie tylko otwiera okno. Każdy ma ID kryterium, kroki, efekt
  widoczny dla człowieka i potwierdzenie skutku po stronie prawdziwego backendu.
- Screeny i logi są dowodami pomocniczymi, nie jedyną wyrocznią. Zapis prywatny i ograniczony
  rozmiarem, referencje czytane przez widok wyniku/Leada. Bez automatycznej publikacji.
- Brak narzędzia/zgody, awaria automatyzacji i defekt aplikacji to różne wyniki.
- QA nie jest autorem naprawy, a źródła są zamrożone na czas werdyktu; testowe dane i
  pliki wynikowe mogą być zapisywane w osobnych katalogach.

**AC native Murmur:** Stop → Later → audio zachowane, brak startu ASR; start następnego
nagrania; restart wyłącznie testowej instancji i brak auto-drain; Run/Run all sekwencyjnie;
retry/cancel; locked recording i relock. Nieobecny scenariusz obowiązkowy daje Not tested.

**AC Loadout:** agent QA naprawdę dostaje i używa narzędzia; test trafia do prawidłowej
instancji; mock UI nie zaspokaja native criterion; brak dostępu jest pokazany na ekranie;
sprzątanie nie dotyka użytkownika. Testy deterministyczne adaptera nie zastępują live smoke.

## 8. Zadania G — generowanie agentów z opisu

### UX obowiązujący dla MVP

W istniejącej sekcji Agents pozostaje ręczne tworzenie. Obok dodaj wejście z polem
**Describe what this agent should do** i przyciskami:

- **Create with Codex**
- **Create with Claude**

Przepływ:

```text
opis → wybór przyciskiem → Generating… → szkic w istniejącym edytorze
                                              │
                            popraw / Regenerate / Save agent
                                              │
                                 opcjonalnie Test agent
```

- Domyślnie aktualny projekt może dostarczyć kontekst tylko w jawnie pokazanym zakresie;
  użytkownik widzi, że wybrany vendor otrzyma opis i wskazane instrukcje/metadane.
- Zakres biblioteki/rola pozostaje przenośny. Nie wklejamy całego repo ani historii
  użytkownika do promptu generatora. Sekrety Connections nigdy nie są jego wejściem.
- Nie obiecuj „perfect” w UI. Wynik jest edytowalnym szkicem.
- Save jest momentem zapisu i zatwierdzenia pokazanych uprawnień. Kliknięcie Create
  uruchamia generowanie, nie samodzielną pracę nowego agenta.
- Generator nie tworzy automatycznie workflow ani nie zastępuje istniejącego agenta.

### G-01 — kontrakt szkicu i walidacja dopasowania

**P1; po L-00, może powstawać niezależnie od L-01 przy rozłącznej własności plików.**

Obszar istniejący: `library/agents.rs`, `commands/agents.rs`, `inherit/`, `skills/`,
`connections/`, `src/state/agents.ts`, `src/sections/agents/capabilities.ts`.
Propozycja nowego modułu: `library/agent_generation.rs` (konkretne typy i walidacja).

Żądanie zawiera: ID operacji, opis roli, generatorVendor/targetVendor, jawny kontekst
projektu, wybrany model generatora lub jego istniejące ustawienie domyślne, limit czasu
i snapshot dostępnych możliwości. Nie czytaj arbitralnych ścieżek podanych przez model.

Propozycja odpowiedzi: `draft` z polami istniejącego `Agent`, `assumptions`,
`missingCapabilities` i krótkie uzasadnienia istotnych ustawień. To nie jest chain-of-thought,
tylko użytkowe uzasadnienie „QA needs app access to test the real application”.

Walidator:

1. Backend wybija tożsamość. Model nie wybiera istniejącego ID, rewizji, ścieżki zapisu
   ani nowej wersji schematu. Zapis korzysta z obecnego `save_agent_inner`.
2. Jeden typ Agent/Overrides/capabilities i obecne adaptery. Nie duplikuj modelu danych
   dla generacji Claude i Codex. Uwzględnij `serviceAccess` i `agentMessages` z WIP.
3. Wygenerowany szkic ma poprawne `runsWith`, model/thinking, instrukcje, scope plikowy,
   timeout, tools, network, skills, connections, service grants i politykę komunikacji.
   Ustawienia niedostępne dla vendora nie mogą udawać działających.
4. Modele/opcje pochodzą ze zweryfikowanego katalogu lub jawnego ustawienia użytkownika;
   stara statyczna lista z formularza nie jest dowodem bieżącej dostępności. Nie odpalaj
   promptu, aby zgadywał najnowsze modele/flagi. Nie wymyślaj nieistniejących poleceń CLI.
5. Skill, connection i service muszą rozwiązać się w zatwierdzonym kontekście. Brak
   przekształca się w czytelny problem. Generator nie instaluje braków po cichu.
6. Proponowane poszerzenie uprawnień pokaż przed Save; generator nie przyznaje sobie
   tych uprawnień. Nie rozszerzaj niejawnie sieci tylko dlatego, że dany vendor tego
   wymaga przy innym trybie. Zachowaj istniejące decyzje domyślnych ustawień produktu.
7. `vendorOptions` może zawierać tylko opcje niesprzeczne z istniejącą walidacją. Model
   nie omija dialu przez flagi resume/session/cwd/sandbox/env. Nieznana wygenerowana
   opcja wymaga sprawdzenia, nie automatycznego uznania za prawidłową.
8. Zachowaj kompatybilność ręcznej przelotki D6. Odrzucenie niezweryfikowanej propozycji
   generatora nie oznacza globalnego usunięcia surowych opcji dla użytkownika.
9. Nowy szkic ma ścisły schema/limity rozmiaru. Nie reinterpretuj `Agent.extra` jako zgody
   na dowolne wygenerowane pola. Stare pliki nowszych wersji nadal zachowują swoje pola.

**AC:** role developer/research/QA dla obu vendorów; nieistniejący skill/model/connection;
kolizja flags; prompt żądający zapisu globalnego configu; próba nadpisania ID; QA bez
możliwości UI; nieobsługiwane tools u vendora; zbyt długi/niepoprawny wynik; ręczna edycja
i serializacja nie tracą poprawnych pól.

### G-02 — izolowane wykonanie generowania przez wybranego vendora

**P1; po G-01.**

Obszar: istniejące `AgentDriver`/`RunSpec`, driver factory, supervisor i IPC.
Propozycja cienkiej komendy: `commands/agent_generation.rs`. Backend odpowiada za
lifecycle, frontend wyłącznie za żądanie, prezentację i anulowanie.

- Nie korzystaj z aktywnej rozmowy Leada. Osobne ID, anulowanie i sesja; brak możliwości
  zatrzymania workflow przez Cancel generatora. Brak globalnego AtomicBool.
- Wybrany przycisk dociera do fabryki drivera: test musi wyłapać hardcoded Claude/Codex.
- Generator otrzymuje zatwierdzony, ograniczony pakiet kontekstu oraz specyfikację
  docelowego Agent. Jego zadaniem jest napisanie konfiguracji, nie wykonanie opisanej roli.
- Proces generatora nie dostaje praw do repo, zapisu biblioteki, usług testowych ani
  narzędzi wykonawczych tylko dlatego, że tworzony agent będzie ich potrzebował.
  Dane kontekstu przekazuj kontrolowanie; nie polegaj na miękkiej prośbie „nie edytuj”.
- Sprawdź, czy aktualne API drivera potrafi wymusić generowanie wyłącznie odpowiedzi,
  bez narzędzi. Jeżeli walidator odrzuca pustą listę narzędzi, dodaj jawny, testowalny
  tryb tej operacji w istniejących driverach; **nie stosuj fallbacku do Everything**.
  Ograniczenie musi działać u obu vendorów. Niewspierana możliwość daje odmowę, nie
  deklarację bezpieczeństwa opartą tylko na instrukcji systemowej.
- Sekrety i opis wyłącznie przez dozwolony transport stdin. Brak tajnych wartości w
  generowanym JSON, argv i eksportowanych logach. Nie wysyłaj pełnej konfiguracji MCP.
- Jedna generacja, najwyżej jedna ograniczona korekta niepoprawnego formatu na podstawie
  błędów walidatora. Brak nieskończonej samonaprawy. Semantyczne braki pokazujemy człowiekowi.
- Domyślny proponowany deadline: 180 s na całą operację, w tym korektę; ustawienie
  musi być jawne i testowalne. Raportuj koszt/usage, a brak pomiaru nie jest kosztem zero.
- Timeout, cancel, zamknięcie UI i awaria transportu przechodzą przez supervisor i dowód
  zakończenia. Zachowaj wpisany opis do Retry. Spóźniony wynik nie nadpisuje nowszego szkicu.
- Generation nie zapisuje agenta. Krótkie metadane pochodzenia można zapisać dopiero
  z zaakceptowanym szkicem, jeśli czyta je edytor/history; nie twórz osobnego dziennika bez UI.

**AC:** oba przyciski uruchamiają właściwego drivera; generation równolegle z Leadem nie
miesza sesji; cancel jednej operacji nie rusza drugiej; brak CLI/login; partial stdout;
result bez poprawnego Agent; korekta przekracza budżet; pending result po zmianie opisu;
zero zapisów repo/biblioteki przed Save. Testy z kontrolowanym driverem, potem live smoke.

### G-03 — formularz generowania i zapis bez utraty ręcznej pracy

**P1; po G-01/G-02.**

Obszar: `src/sections/agents/index.tsx`, `agent-form.tsx`, `io.ts`, `app-permissions.tsx`,
`src/state/agents.ts`, istniejące IPC typy i komendy. Nowy mały komponent
`generate-agent.tsx` jest propozycją; nie powiększaj dalej monolitycznego index bez potrzeby.

- Dodaj opis i dwa przyciski przy istniejącym ręcznym Create. Widoczne stany: idle,
  generating, draft, failed, cancelled. Przy generowaniu działający Cancel.
- Po wyniku użyj istniejącego AgentForm. Nie twórz drugiego pełnego edytora agenta.
- Pokaż runtime vendor/model, co agent może zrobić, brakujące możliwości i kontekst
  dziedziczenia. QA nie może wyglądać na gotowe do native testów przy pustej liście narzędzi.
- Rola QA ma spójne instrukcje weryfikacyjne, nie odziedziczone „implementuj”. Generator
  wybiera rolę z opisu, nie z samej nazwy. Niejednoznaczność trafia do assumptions.
- Regenerate nie nadpisuje ręcznych zmian bez potwierdzenia. Zmiana vendora tworzy
  nową walidowaną propozycję, nie podmienia tylko jednego selecta w starej konfiguracji.
- Save idzie przez istniejący store/IO/Rust writer z optimistic concurrency, walidacją
  nazwy i ID. Kolizja nazwy daje wybór/edycję, nigdy nadpisanie obcego agenta.
- Jeśli opis pusty lub vendor niedostępny, pokaż przyczynę przy kontrolce. Ręczne
  tworzenie nadal dostępne. Nie usuwaj szkicu po błędzie zapisu.
- Bez automatycznej podmiany agentów używanych w aktywnych workflow. Zapisany agent
  jest dostępny dla następnego wyboru w edytorze; aktywny bieg zachowuje snapshot.

**AC UI:** oba rzeczywiste kliknięcia → poprawny vendor w żądaniu → draft w formularzu →
Save → agent widoczny po ponownym otwarciu biblioteki → wybór do workflow. Obsłuż klawiaturę,
cancel, brak CLI, edit/regenerate, duplicate name, odmowę zapisu i opóźnioną odpowiedź.
Testy muszą obejmować produkcyjny komponent/handler/IPC, nie tylko helper generujący JSON.

### G-04 — opcjonalna próba wygenerowanego agenta

**P1; po G-03 i P-01, native role po P-03.**

Obszar: istniejący Lab i zwykłe wykonanie workflow/agentów. Nie dodawaj drugiego executora.

- Przycisk **Test agent** uruchamia dopiero po świadomej akcji użytkownika krótki scenariusz
  w izolowanej kopii z podanym budżetem. Pokazuje, co sprawdzi i czego nie obejmuje.
- Przed próbą prezentuj osobno poprawność konfiguracji i status „Not tested”. Dopiero
  wynik niezależnego sprawdzenia daje „Test passed”, z zakresem, vendorem i rewizją.
- Przykłady: developer zmienia mały plik i przechodzi zewnętrzny test; research korzysta
  z kontrolowanego źródła; QA znajduje zasadzony błąd; native QA uruchamia i obsługuje fixture.
- Przeniesienie do innego repo lub zmiana istotnej konfiguracji oznacza utratę aktualności
  poprzedniej próby. Nie jest to globalna certyfikacja agenta na wszystkie projekty.
- Generator nie jest swoim egzaminatorem. Reużyj chronionego zewnętrznego assessment/Lab.

**AC:** dobry szkic przechodzi wskazany scenariusz; celowo wadliwy nie dostaje zieleni;
brak natywnego połączenia jest Not tested; koszt i cleanup rozliczone; nic nie uruchamia
się wskutek samego otwarcia lub zapisu szkicu.

### Co świadomie pozostaje zadaniem modelu

Synteza opisu roli, wybór właściwych instrukcji i semantyczna ocena scenariusza nie są
samym stanem plików, więc wymagają modelu. Natomiast poprawność konfiguracji, dostępność
narzędzi, zgody, adres sesji, wynik komendy, licznik/ID testów i zgodność snapshotu mają
być egzekwowane kodem. Nie dokładaj promptów jako zamiennika tych mechanizmów. Jeśli
aktualne reguły repo wymagają odnotowania takiego wyboru w chronionej dokumentacji
harnessu, uzyskaj zgodę przed jej zmianą.

## 9. Zadania M — osobna naprawa wyniku Murmura

Nie mieszaj tych zmian z Loadoutem. Przed pracą ponownie sprawdź lokalny stan meetnotes,
jego instrukcje, aktywne nagrywanie i właścicieli plików. Badany commit nie musi już być HEAD.

### M-01 — własność kolejki obejmuje Retry i odzyskiwanie

Najpierw zbuduj odtwarzalny test dwóch podejrzanych scenariuszy: legacy retry dla nagrania
Queued i startup po częściowo nieudanym sprzątaniu archiwum. W audycie były wnioskami z
kodu, nie wykonanymi reprodukcjami.

Obszar: `commands/mod.rs::retry_transcription_prep`, `commands/processing_queue.rs`,
`storage/processing_queue_store.rs`, `lib.rs`, `audio/spill.rs::claim_inflight`.

Wszystkie wejścia przetwarzania respektują queue ownership. Stary Retry odmawia albo
świadomie deleguje do kolejki; nie zostawia Queued podczas pracy poza nią. Startup
może odtworzyć metadane, ale nie rozpoczyna odłożonego ASR. Wykluczenie kolejki musi dotyczyć
każdej ścieżki odzyskiwania, nie tylko `claim_disk_salvage`.

**AC:** Later → restart, także po zasadzonym błędzie cleanup, nie uruchamia pipeline;
legacy retry nie omija stanu/limitów; audio pozostaje zachowane.

### M-02 — jedna rezerwacja ciężkiej pracy i bezpieczne Cancel

Jeden wspólny mechanizm dopuszczenia pracy obejmuje kolejkę i pozostałe wejścia pipeline,
z rozstrzygnięciem wyścigu Start recording/Run. Nie wystarczy odczyt boola przed await.

Cancel musi mieć zdefiniowaną, widoczną semantykę. Jeśli bieżąca operacja FFI nie może
być bezpiecznie przerwana, pokazuj „Stopping…” i zatrzymuj na najbliższej bezpiecznej
granicy; nie zgłaszaj „Cancelled”, gdy ciężka praca nadal trwa. Bez niebezpiecznego
przerywania natywnych wątków i bez usuwania audio.

**AC:** dwa Run, Run all + Retry, Start podczas claim, Cancel w każdej fazie; realny
limit jednego ciężkiego zadania i przewidywalny priorytet nagrywania.

### M-03 — prawdziwy postęp i spójne stany UI

Zasil etapy/counters zdarzeniami istniejącego pipeline. Eksport nie może po raz pierwszy
pojawiać się jako trwający dopiero po zakończeniu. Tam, gdzie brak licznika, UI pokazuje
postęp nieokreślony, nie fałszywy procent. Library/detail/queue muszą zgadzać się w tej
samej sesji, nie dopiero po następnym uruchomieniu.

**AC:** obserwacja prawdziwego transcribe → summarize → export, późne zdarzenia starej
próby, retry i cancel nie cofają/nie nadpisują nowego wyniku.

### M-04 — pełny odbiór produktu

Wymagany według aktualnych reguł Murmura przegląd lock/security i scenariusze P-03 na
osobnej instancji. Testy mockowane pozostają szybką warstwą, nie substytutem backendu.
Nie twierdź „brak utraty audio / brak wycieku” wyłącznie na podstawie zielonych testów UI.

Decyzje produktowe o domyślnym Ask i API-only Queued pokaż właścicielowi; nie przedstawiaj
obecnego wyboru agenta jako jego wcześniejszej zgody. Zachowaj zgodność starych baz.

## 10. Kolejność realizacji i zależności

| Fala | Zadania | Warunek przejścia |
|---|---|---|
| 0 | L-00 | ustalona i bezpieczna baza obejmująca istniejący WIP |
| 1 | L-01, L-02, V-01 | odtworzone incydenty komunikacji i fałszywych testów już nie przechodzą |
| 2 | V-02, P-01, P-02 | kryteria i wynik związane z kopią, izolowana aplikacja |
| 3 | P-03, V-03 | QA rzeczywiście testuje natywny cel; brak pomiaru nie daje pass |
| G | G-01 → G-02 → G-03 → G-04 | generacja, zapis i próba dla obu vendorów |
| M | M-01 → M-02/M-03 → M-04 | oddzielnie domknięta funkcjonalność Murmura |
| Final | E-01 | jeden pełny workflow na dokładnej wersji po integracji |

Tor G może powstawać równolegle do L/V/P wyłącznie przy uzgodnionej własności plików.
`commands/run.rs`, `library/agents.rs`, IPC, rejestracja komend i `tests/it/main.rs` wymagają
koordynacji. Sam plan nie nakazuje uruchamiania subagentów. Nigdy równoległe ciężkie Cargo.

## 11. E-01 — regresje harnessu i odbiór całego przepływu

### Tanie, deterministyczne regresje

Każdy nowy moduł rustowy pod `src-tauri/tests/it/`, deklaracja w `it/main.rs`.
Przykładowy **planowany**, nie istniejący jeszcze, kontrakt polecenia:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test it step_turn_queue_is_atomic::
```

Front: pojedynczy plik `npx --no-install vitest run <ścieżka>.test.tsx`. Jeśli trzeba
dopisać mapowanie do chronionych checków, najpierw zgoda właściciela; nie obchodź guardów.

Każdy bugfix: kompilowalny test padający na zachowaniu starego kodu przed poprawką.
Nie zaliczaj błędu importu/kompilacji jako RED. Nowe funkcje: cienki szkielet pozwalający
uruchomić test i uzyskać błąd asercji, potem implementacja.

Minimalna macierz regresji:

| Próba | Oczekiwany wynik |
|---|---|
| Wiadomość dokładnie przy pierwszym result | obsłużona kolejna tura albo jawna odmowa przed przyjęciem |
| Wiadomość do wcześniejszej próby/node w innym runie | brak dostarczenia do nowej sesji |
| 0 testów / niewłaściwe testy / stary wynik | brak zaliczenia wymagania |
| QA pisze pass przy brakującym obowiązkowym scenariuszu | brak końcowego potwierdzenia |
| UI działa na mocku, kryterium wymaga backendu | Not tested |
| Obca aplikacja zajmuje port/okno | brak przejęcia lub zatrzymania jej |
| Poprawka zmienia źródła po QA | poprzedni wynik historyczny, nie aktualna akceptacja |
| Generowanie dla dwóch vendorów | właściwy driver i właściwe ustawienia docelowe |
| Model żąda zapisu configu, sekretów lub większych praw | brak wykonania/poszerzenia bez zatwierdzenia |
| Cancel generation równolegle do workflow | kończy wyłącznie operację generatora |
| Brak CLI, loginu, GUI connection lub uprawnienia | czytelny brak możliwości, nie fałszywy sukces |
| Próba zmiany egzaminatora przez mierzony bieg | rzeczywista odmowa |

### Pełny odbiór produktu — dopiero po tanich testach

Za zgodą na płatne próby i po zamknięciu wymaganych bramek:

1. Utwórz jednego agenta każdym z dwóch przycisków. Edytuj, zapisz, ponownie otwórz,
   wybierz do workflow i potwierdź faktyczne użycie właściwego vendora.
2. Uruchom mały wieloagentowy workflow na repo fixture. Dostarcz doprecyzowanie w trakcie
   kroku i potwierdź ostatnią turę, wynik oraz brak ubitego poprawnie pracującego procesu.
3. Zasiej jeden kontrolowany defekt. Wymagany Check/QA wykrywa go, następuje jedna
   naprawa i ponowna weryfikacja właściwej kopii. Dodatkowe iteracje mają Not run.
4. Native QA uruchamia własną pełną aplikację i wykonuje zatwierdzone scenariusze.
   Nie używa zainstalowanego release'u ani danych/okna użytkownika.
5. Lead z nowo otwartej rozmowy potrafi odczytać źródła: wynik, niepowodzenie wcześniejszej
   próby, wiadomość, snapshot i zakres testów. Nie wymyśla „wszystko gotowe”.
6. Po zakończeniu brak własnych osieroconych procesów; zapisany wynik można otworzyć
   bez działającego agenta. Eksport diagnostyczny pozostaje prywatnościowo ograniczony.
7. Osobno raportuj pozytywne i negatywne wyniki, nie sumuj kilku starych prób jako jednej.

Pełne CI przy integracji zgodnie z `scripts/ci.sh`/aktualnym procesem repo, jeden ciężki
writer. Zielony harness nie zastępuje smoke aplikacji na dokładnym integrowanym snapshotcie.
Brak potrzebnego środowiska jest blockerem, nie powodem obniżenia kryterium w trakcie.

## 12. Co ma zawierać końcowe przekazanie implementującego agenta

1. Wykonane ID zadań i niewykonane ID z konkretną przyczyną.
2. Repo, branch/worktree, dokładny commit oraz informacja o niezacommitowanych zmianach.
3. Wyniki zawężonych testów z licznikami, potem wynik integracyjny na właściwej wersji.
4. Dla runtime: faktyczna aplikacja/backend, własna instancja, scenariusze, dowody i cleanup.
5. Dla generatora: oba przyciski, vendor wykonujący, szkic, zapis i opcjonalna próba;
   rozróżnienie konfiguracji poprawnej od zachowania sprawdzonego.
6. Lista wymaganych zgód/niezapewnionych narzędzi. Nie chowaj ich pod „non-blocking”.
7. Co trafiło tylko do lokalnego kodu, co zmergowano, co opublikowano. Release/push
   wymagają osobnego zakresu; ten plan nie nakazuje publikacji.

**Zakaz końcowego skrótu:** nie wolno zamknąć całości zdaniem „wszystko gotowe”, jeśli
native QA nie wykonało scenariuszy, generator obsługuje tylko jednego vendora, aktualny
kod różni się od sprawdzonego albo otwarte są obowiązkowe wymagania bezpieczeństwa.
