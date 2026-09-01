# Loadout — architektura

2026-08-15 · v1 · źródła: `docs/research/` (15 raportów), `docs/DECISIONS-LOCKED.md`

> **Uzgodnione z kodem 2026-08-24, po fazie 6** (`docs/PLAN-AGENTS-CONTEXT.md`). Ten dokument
> opisywał w kilku miejscach zamiar, nie stan: `INDEX.md` przekazań, `memory/agents/<slug>.md`,
> globalny semafor, `attempt += 1`, dwa rodzaje kafelka i nierozstrzygnięty spike S-2. Każde
> z tych miejsc jest niżej poprawione **z datą**, a nie po cichu — nieaktualna architektura
> uczy następnego czytelnika nieprawdy o systemie, który ma przed oczami.

Notacja cytowań: `[T1 §8.3]` = raport tematyczny, `[R06 §10]` = raport z rekonesansu repo,
`[ran]` = zweryfikowane uruchomieniem na tej maszynie.

---

## 1. Czym jest Loadout, w jednym akapicie

Aplikacja desktopowa, w której **budujesz graf agentów kodujących i go uruchamiasz**.
Definiujesz agentów raz (kto, jaki model, co wolno). Układasz z nich workflow metodą przeciągnij-upuść.
Naciskasz Start. Loadout odpala prawdziwe procesy `claude`, nadzoruje je, kuruje ich wyjście do czystego
strumienia i pokazuje po prawej listę agentów — klikasz jednego i widzisz jego sesję.
Agenci przekazują sobie wyniki przez pliki markdown, które widzisz i możesz przeczytać.

Zastępuje: Superset, Warp, ręcznie klejone harnessy z basha.

---

## 2. Rozstrzygnięcia otwartych pytań

Rekonesans przed budową zostawił dziewięć otwartych pytań.
Rozstrzygam je tutaj. Każde ma powód i koszt zmiany zdania później.

| # | Pytanie | Decyzja | Powód | Koszt zmiany później |
|---|---|---|---|---|
| 1 | Daemon czy in-process? | **In-process.** Silnik w module `engine/`, który nie zna ani jednego typu z Tauri. | 45 tras HTTP + token bearer + SSE to maszyneria, której desktop nie potrzebuje [R02 §4]. Granica modułu wystarczy, żeby daemon dał się dodać. | Niski, jeśli `engine/` nie importuje `tauri::*`. To jest wymuszone testem. |
| 2 | Trwały dziennik czy pamięć + JSONL? | **Oba, z jasnym podziałem: pliki są prawdą, SQLite jest indeksem.** Surowy `agent-<id>.jsonl` na dysku to zapis źródłowy; SQLite to zapytywalny widok, który wolno skasować. | Godzi T6 i T7. Kasujesz `loadout.db`, Loadout odbudowuje go w ~200 ms dla 5000 notatek `[ran]` [T6 §10.1]. To jedyna właściwość, która nie pozwala temu podsystemowi stać się poprzednim prototypem. | Zerowy — to jest właśnie ta ucieczka. |
| 3 | Jak głęboko sterujemy Claude? | **Jeden długo żyjący proces, dwukierunkowy stdin.** `--input-format stream-json` + `--session-id`, wiele tur w jednym procesie, przerwanie w paśmie. | Zweryfikowane end-to-end `[ran]` [T1 §2]. Daje „otwórz agenta i pogadaj z nim" za darmo, bo to ten sam proces. | Średni — tryb jednorazowy z `--resume` to fallback za tym samym traitem. |
| 4 | Co obiecujemy w kwestii izolacji? | **Mówimy prawdę, jednym zdaniem, w UI: „każdy krok dostaje własną kopię twoich plików".** To izolacja współbieżności, nie sandbox bezpieczeństwa. Sandboxa nie ma w v1. **Od 2026-08-19 (T-52) dowozi to `git worktree` na własnej gałęzi**, a kopia plików jest drogą dla folderu, który repozytorium nie jest. Zdanie zostaje to samo, bo mówi o tym, co człowiek dostaje; zmienia się to, czym jest dowożone — i dochodzi druga połowa, której kopia nie miała: praca jest po biegu **osiągalna z gita**, zamiast zostawać w katalogu, z którego nikt jej nie wyjmuje. | poprzedni prototyp to zapisał w `docs/security.md`, ale nie powiedział tego użytkownikowi. Obietnica bezpieczeństwa, której nie dowozimy, jest gorsza niż jej brak. Kopia plików pisana ręcznie przegrywała z systemem plików po kawałku: zmierzone 2026-08-19, dowiązanie do katalogu zatrzymywało bieg, a kolejka FIFO wieszała go bez słowa. | Niski — sandbox dokłada się później jako polityka na agencie. |
| 5 | Bramka promocji notatek? | **Dwa stany, nie trzy: „sugerowana" → „w użyciu".** Promuje **wyłącznie człowiek**. Bez człowieka notatka zostaje sugerowana i **nigdy nie trafia do promptu**. | poprzedni prototyp ma trzy stany i korroboratora, którym w praktyce nie ma kto być. Dwa stany są uczciwe: albo ktoś to zatwierdził, albo nie. | Niski — trzeci stan da się wcisnąć między te dwa. |
| 6 | Harness to też produkt? | **Tak, i to jest wiążące ograniczenie.** Graf, który buduje Loadout (workspace → implementacja → sprawdzenie → druga opinia → poprawka → wejście) **musi dać się wyrazić w edytorze workflow**. | To jest jedyny test, czy edytor jest wystarczająco ekspresyjny, i jedyny sposób, żeby harness nie zgnił [R06 §10]. | Wysoki. Dlatego decydujemy teraz. |
| 7 | Windows? | **macOS-first. Windows to v2, nie zobowiązanie v1.** Ale **cały kod platformowy siedzi w `engine/supervisor.rs`** i nigdzie indziej. | `process-wrap` daje `JobObject` w tym samym miejscu wywołania co `ProcessGroup::leader()` [T7 §9.2]. Port to gałąź `cfg`, nie przepisanie. | Niski, dopóki test pilnuje, że `#[cfg(windows)]` nie wycieka poza jeden plik. |
| 8 | Sprawdzacz słownictwa? | **Od pierwszego dnia.** ~100 linii, skanuje tylko tekst widoczny dla użytkownika, baseline może tylko maleć. | Prosty język to główna różnica wobec poprzedniego prototypu. Dyscyplina prozy nie działa — meetnotes ma na to dowód [R05 §7]. | Rośnie z każdym tygodniem. Najtaniej teraz. |
| 9 | Sufit gęstości? | **Ustalony poniżej, §7.** Liczby przed pierwszym ekranem, nie po. | poprzedni prototyp ustawił limity po fakcie i chybił czterech z ośmiu; wymuszany limit zamarzł na 2,4× wartości docelowej [R03 §4.1]. | Wysoki — sufit ustawiony po fakcie jest zawsze ustawiony tam, gdzie akurat jesteś. |

---

## 3. Kształt systemu

```
┌──────────────────────────────────────────────────────────────────┐
│  React 19 · Vite · Tailwind v4 · Zustand · Base UI               │
│  Sześć sekcji, bez routera: Praca · Workflow · Agenci · Umiejętności     │
│  · Pamięć · Triggery                                                    │
└───────────────┬──────────────────────────────┬───────────────────┘
      invoke()  │  żądanie → odpowiedź         │  Channel<Vec<Line>>
                │                              │  sklejane 16 ms / 2000 linii
┌───────────────▼──────────────────────────────▼───────────────────┐
│  ipc.rs — JEDYNE miejsce, które zna słowo „Tauri"                │
│  cienkie #[tauri::command] → *_inner(&AppState, ..)              │
└───────────────┬──────────────────────────────────────────────────┘
                │
┌───────────────▼──────────────────────────────────────────────────┐
│  engine/ — nie importuje tauri::*. Testowalne bez okna.          │
│                                                                   │
│   scheduler.rs   zbiór gotowych (Kahn) + JoinSet + Semaphore      │
│                  + CancellationToken                    ~120 lin  │
│   dag.rs         graf, cykle, stopnie wejściowe          ~80 lin  │
│   step.rs        maszyna stanów kroku (tabela §5)                 │
│   supervisor.rs  grupy procesów, SIGTERM→SIGKILL, limit czasu     │
│                  ★ JEDYNY plik z #[cfg(unix)] / #[cfg(windows)]   │
│   stream.rs      NDJSON → AgentEvent → Line   ★ TU JEST KURACJA   │
│   drivers/       trait AgentDriver                                │
│                  ├ claude.rs  długo żyjący proces, dwukierunkowy  │
│                  ├ codex.rs   exec --json, resume co turę         │
│                  └ fake.rs    deterministyczny, do testów         │
└───────────────┬──────────────────────────────────────────────────┘
                │
┌───────────────▼──────────────────────────────────────────────────┐
│  store/ — rusqlite + WAL + JEDEN zadaniowy pisarz                │
│  Indeks, nie prawda. Wolno skasować.                             │
└───────────────┬──────────────────────────────────────────────────┘
                │
┌───────────────▼──────────────────────────────────────────────────┐
│  Pliki — TO JEST PRAWDA                                          │
│  ~/.loadout/{workflows,agents,skills,memory,triggers}/           │
│  <repo>/.loadout/runs/<ts>__<id>/{run.json,handoffs/,logs/}      │
└──────────────────────────────────────────────────────────────────┘
```

### Trzy granice, których nie wolno przekroczyć

1. **`engine/` nie zna Tauri.** Wymuszone testem: `grep -r 'tauri' src-tauri/src/engine/` musi być puste.
   Bez tego silnik nie da się przetestować bez okna i daemon nigdy nie powstanie.
2. **Tylko `store::writer` pisze do SQLite.** Drugie połączenie zapisujące = zakleszczenie [T7 §10.7].
3. **Kod platformowy tylko w `supervisor.rs`.** Wymuszone testem na `#[cfg(windows)]` poza tym plikiem.

---

## 4. Przepływ danych podczas biegu

```
użytkownik: Start
     │
     ▼
scheduler: policz stopnie wejściowe → zbiór gotowych → weź permit z semafora
     │
     ▼
supervisor: spawn w grupie procesów, zapisz pid+pgid do bazy
     │  claude -p --output-format stream-json --input-format stream-json --verbose
     │         --session-id <uuid> --strict-mcp-config --setting-sources ""
     │         --permission-mode <z polityki> --allowedTools <z polityki>
     │         --effort <z pola „thinking">                       (T-91)
     │         --settings <plik>  — przekierowanie auto-pamięci    (T-92)
     │         --max-budget-usd <reszta budżetu biegu>             (T-94)
     │         --plugin-dir <skille>  --mcp-config <połączenia>
     │         --tools <powierzchnia>  --model <jawny model>
     │         --append-system-prompt <konfiguracja agenta>
     │         --add-dir <handoffs/>  --add-dir <attachments/>
     ▼
stdout: NDJSON, linia po linii
     │
     ├──► tee do <run>/logs/agent-<id>.jsonl        ← surowe, nietknięte, prawda
     │
     ▼
stream.rs: parsuj permisywnie (#[serde(other)] — nigdy nie wywalaj biegu na nieznanym zdarzeniu)
     │
     ▼
AgentEvent (8 wariantów, neutralnych względem dostawcy)
     │
     ▼
mapowanie na Line (14 typów, §6) ← ★ TU POWSTAJE „CZYSTY TERMINAL", NIE W CSS
     │
     ├──► batch do SQLite (transakcje po ~100 wierszy)
     │
     ▼
sklejacz: 16 ms albo 2000 linii, co pierwsze
     │
     ▼
ipc::Channel<Vec<Line>> → React → wirtualizowana lista
```

**Dlaczego sklejanie, a nie emit-per-event.** Zmierzone na tej maszynie stanowiskiem pomiarowym IPC:

| Sposób | µs/wiadomość | wiadomości/s | najgorsza przerwa klatki |
|---|---|---|---|
| `emit` na zdarzenie | 13,2–14,4 | ~70 000 | 0–1 ms |
| `Channel` bez sklejania | 10,6–11,0 | ~92 000 | 1 ms |
| `Channel` batch 50 | 1,0–1,8 | ~1 000 000 | 1–23 ms |
| **sklejanie 16 ms, limit 2000** | **0,18–0,20** | **~5 000 000** | **0–1 ms** |

Sklejanie jest **~70× tańsze na wiadomość** od `emit` i jednocześnie ma **najlepszą** najgorszą przerwę klatki.
To nie jest kompromis — to jest ścisłe zwycięstwo.

---

## 5. Maszyna stanów kroku

Siedem stanów. `paused` jest stanem **biegu**, nigdy kroku — to usuwa całą ćwiartkę stanów [T7 §9.3].

| Z | Wyzwalacz | Do | Efekt uboczny |
|---|---|---|---|
| — | bieg utworzony | `pending` | wiersz w bazie, `session_id` przydzielony z góry |
| `pending` | stopień wejściowy spadł do 0 | `ready` | do zbioru gotowych |
| `pending` | krok wyżej `failed` | `skipped` | końcowy; przejście po stożku w dół |
| `pending` | krok wyżej `cancelled` | `cancelled` | końcowy — **nie `skipped`**, bo UI kłamałby o powodzie |
| `ready` | permit semafora | `running` | spawn w grupie procesów, zapis `pid`, `pgid` |
| `running` | wyjście 0 **i** `result.is_error == false` | `succeeded` | koszt, podsumowanie, dekrementacja stopni potomków |
| `running` | niezerowe wyjście **albo** `result.is_error` | `failed` | zapis powodu, pominięcie stożka |
| `running` | limit czasu | `failed` | SIGTERM grupy → łaska → SIGKILL |
| `running` | anulowanie przez użytkownika | `cancelled` | jw. |
| `running` | **crash aplikacji** | `failed` (`interrupted`) | ustawiane przy starcie przez `recovery.rs`, sprzątanie po `pgid` |
| `running` | limit zapytań u dostawcy | zostaje `running` | pauza **biegu**, wznowienie o `resetsAt` |
| `failed` / `cancelled` | ponowienie przez użytkownika | `pending` | `attempt += 1`, nowy `session_id`, `skipped` niżej → `pending` |

**Pułapka, którą trzeba pokryć testem:** `tokio::time::timeout` wokół kroku anuluje zadanie Rusta,
**nie proces systemowy**. Każda ścieżka limitu czasu musi przejść przez eskalację zabijania w supervisorze [T7 §10.8].

**Dwa sufity dowodu śmierci, nie jeden.** Żywy Stop człowieka wykonuje najwyżej trzy pełne
próby `cancel`/eskalacji i czeka sekundę tylko między próbami. Jeśli grupa nadal odpowiada,
krok kończy się jako `failed`, z jawnym powodem i `death_proof: false` — nie udaje
`cancelled`. Ścieżka timeoutu używa osobnego `prove_agent_dead`: zachowuje uchwyt i ponawia
bezterminowo, bo zwrot bez dowodu zostawiłby proces naliczający koszt. Sufit trzech prób
dotyczy więc responsywności przycisku Stop, nie pozwolenia na porzucenie procesu.

### Czego ta tabela nie mówi, a kod robi (uzupełnione 2026-08-24)

- **`attempt += 1` i „nowy `session_id`" w ostatnim wierszu nie istnieją.** Ponowienie kroku
  wewnątrz biegu nie jest zbudowane; `run.json` ma `attempt: 0` na sztywno. Powtórzenie to
  **osobny bieg** (`commands/rerun.rs`), a mechanizmem „spróbuj jeszcze raz" wewnątrz grafu jest
  pętla `max_turns`, rozwinięta na literalne rundy **przed** biegiem (`workflow/unroll.rs`).
- **`FailedAndCarriedOn`** (T-87) — krok jest `failed`, ale jego potomkowie **ruszają**. To jest
  ustawienie „co, kiedy ten nie przejdzie" z D7, a nie ósmy stan.
- **Trasa zablokowana** — krok, który zameldował sukces, dostaje `Failed`, kiedy żaden warunek
  krawędzi wychodzącej nie pasuje (`Route::Blocked`). Tabela nie zna tego wyzwalacza.
- **`settle_leftovers`** — po pętli planisty każdy krok wciąż `ready`/`running` schodzi jako
  `failed`, a `pending` jako `skipped` albo `cancelled`. To domknięcie, nie przejście.
- **Sufit budżetu** (T-94) — krok zatrzymany sufitem czyta się jako `skipped`, nigdy `cancelled`:
  na ekranie „cancelled" znaczy „nacisnąłeś Stop".
- **Sufit jest miękki przy równoległości.** Scheduler liczy ceny wyłącznie zakończonych kroków;
  kroki już uruchomione kończą swoje tury. Przy `N` startach naraz rachunek może więc dojść
  do około `N ×` ustawionej kwoty. Claude dostaje w `--max-budget-usd` pozostałą kwotę, ale
  pojedyncza tura Codeksa jest wyceniana dopiero po zakończeniu. Po T-149 końcowe
  `run.json.spent_usd` obejmuje także koszt udanej refleksji; decyzje schedulera nadal liczą
  tylko kroki, ponieważ refleksja biegnie dopiero po grafie.

---

## 6. Kuracja: zdarzenie → linia

To jest miejsce, w którym powstaje wartość produktu. Nie w CSS.

Tabela pokazuje zdarzenia Claude. Codex ma **inne nazwy, ten sam kształt** — jego model JSON to już
właściwa taksonomia (`command_execution` z `exit_code`, `file_change` z `changes[].path/.kind`,
`reasoning`, `agent_message`, `todo_list`, `web_search`, `mcp_tool_call`) [T2 §7.1]. Oba strumienie
lądują w tym samym enumie `AgentEvent`, więc reguły zwijania niżej są wspólne.

| Zdarzenie z `claude` | Co widać |
|---|---|
| `system/init` | *nic* — kropka agenta robi się aktywna |
| `thinking`, `thinking_tokens` | *nic w strumieniu* — stały slot na dole, nadpisywany |

> **Codex nie wysyła `reasoning` w trybie `exec`.** Zmierzone 2026-08-24 na `codex-cli 0.148.0`
> trzema drogami: sześć prawdziwych biegów, sonda z siecią i sonda z `model_reasoning_effort=high`
> **plus** `model_reasoning_summary=detailed` — ani razu. Pozycja `reasoning` w taksonomii wyżej
> pochodzi z raportu T2 i zestarzała się. Odwzorowanie na slot myślenia istnieje i jest sprawdzone
> (T-97), żeby zadziałało, gdyby vendor zaczął je wysyłać; dziś slot przy krokach Codeksa jest
> pusty **z powodu vendora, nie z powodu wady**.
| blok `text` | proza agenta, maks. 3 linie, dalej „więcej" |
| `tool_use` Read/Grep/Glob | `Przeczytał 6 plików` — sklejone w oknie 2 s |
| `tool_use` Edit/Write | `Zmienił src/auth.rs  +12 −4` → klik otwiera panel zmian |
| `tool_use` Bash | `Uruchomił testy` — z pola `description`, które model sam pisze `[ran]` |
| `tool_result` | jednolinijkowe podsumowanie; pełne wyjście za kliknięciem |
| `rate_limit_event` | `Limit Claude wyczerpany — wraca o 5:30` |
| `result` | `Gotowe · 2 tury · 12 s · $0,012` |

Wszystko inne jest **odrzucane**. Pole `description` w `tool_use.input` to prezent: model sam pisze
czytelną etykietę własnego działania, więc dostajemy ludzkie linie za darmo [T1 §8.6].

### Pięć reguł zwijania — to jest produkt [T2 §7.3]

1. **Jedna czynność, jedna linia.** Bez wyjątków. Jeśli coś ma treść, treść jest za linią.
2. **Domyślnie zwinięte** — poza prozą, pytaniami, błędami i strukturą.
3. **Błąd rozwija się sam** i pokazuje ostatnie 20 linii. To jedyne miejsce, gdzie ściana tekstu jest pożądana.
4. **Sklejanie sąsiednich linii tego samego typu w oknie 2 s** w licznik.
5. **`Myśli…` to status, nie linia.** Stały slot na dole, nadpisywany, **nigdy nie wchodzi do historii.**
   Ta jedna reguła usuwa większość wrażenia ściany tekstu.

Plus jeden globalny skrót: **`Cmd+O` — „Pokaż wszystkie szczegóły"**. Nigdy „verbose".

---

## 6a. Workspace'y i karty

*Dodane 2026-08-15 na prośbę użytkownika: kilka kart, kilka terminali w karcie, wybór folderu,
w którym pracuje AI, i przełączanie bez utraty sesji.*

### Model

**Karta = workspace.** Workspace to folder, w którym pracuje AI (`~/Projects/meetnotes`).
W jednej karcie żyje jeden bieg i tyle „terminali", ilu agentów ten bieg uruchomił —
każdy agent w szynie po prawej jest terminalem, który otwierasz kliknięciem.

```
┌────────────────────────────────────────────────────────────┐
│ ● meetnotes   ○ Loadout   ● spreadsheet   ＋               │  karty = workspace'y, 34px
├────────────────────────────────────────────────────────────┤
│ ▓▓ ▓▓ ░░ ░░   Fix the CSV parser · step 3 of 4             │  pasek loadoutu, 56px
├──────────────────────────────────────────┬─────────────────┤
│  historia / teraz / wejście              │  agenci = term.  │
└──────────────────────────────────────────┴─────────────────┘
```

Reguły, które to trzymają w ryzach:

1. **Jeden workspace = jedna karta.** Otwarcie folderu, który już ma kartę, przełącza na nią,
   nie tworzy drugiej. Dwa biegi w tym samym katalogu kolidowałyby na plikach, a kopia per krok
   chroni tylko kroki między sobą, nie biegi między sobą.
2. **Przełączenie karty to wyłącznie zmiana widoku.** Nic się nie pauzuje, nie odłącza i nie ginie.
   Silnik nie wie o kartach — zna tylko biegi. Karta to zapytanie: „pokaż mi bieg dla tego workspace'a".
3. **Karty istnieją tylko w sekcji Praca.** Agenci i workflow są globalne (`~/.loadout/`), więc
   karta nic by tam nie znaczyła. Pamięć podąża za aktywnym workspace'em.

   *Poprawione 2026-08-19, po decyzji użytkownika o wyborze „ten projekt / wszędzie" przy
   dodawaniu umiejętności (T-44).* Umiejętności były w tym zdaniu wymienione razem z agentami
   i workflow, i połowa tego jest dalej prawdą: **biblioteka** jest globalna — kopia kanoniczna
   leży w `~/.loadout/skills/<name>/` niezależnie od zakresu, sekcja pokazuje jedną listę, a karta
   jej nie filtruje. Nieprawdą przestaje być zdanie o **miejscu docelowym**: `Scope::Project`
   i `Roots.project` istnieją w rdzeniu od T-18, a od T-44 człowiek wybiera przy dodawaniu, czy
   plik ląduje w katalogu domowym, czy pod korzeniem otwartego projektu — i wtedy jedzie z repo
   do zespołu. Konsekwencja, której nie wolno rozdzielić od tej zmiany: **lista i „Remove" muszą
   widzieć oba korzenie**, bo droga zapisu bez drogi odczytu jest gorsza niż brak funkcji.
   `.gitignore` w repo człowieka pozostaje nietknięty (T5 §12 pyt. 6 rozstrzygnięte tak, że
   pliki zapisujemy, a o ignorowaniu decyduje właściciel repo).
4. **Karta w tle z żywym biegiem ma kropkę.** To jedyna rzecz, jaką karta w tle o sobie mówi.
   Bez tego zapominasz, że coś ci chodzi w innym folderze, i płacisz za to limitem.

### Konsekwencja, którą trzeba nazwać teraz

**Limit „ile naraz" musi być globalny, nie per bieg.** Trzy karty po trzech agentach to dziewięciu
agentów na jednej maszynie. Przy ~583 MB na agenta [T7 ryzyko 3] to jest zamrożony laptop, a nie
szybsza praca.

Więc: **jeden semafor na całą aplikację**, dzielony przez wszystkie biegi we wszystkich kartach.
*Prawdziwe od 2026-08-24 (T-94): `AppState` trzyma jeden `Limiter`, a każdy świeży uchwyt biegu
dostaje jego klon. Do tego dnia pula powstawała **na każdy bieg osobno**, więc dwie karty dawały
`2 × limit` agentów naraz — i to była wada, nie wygoda.*
Kiedy karta czeka na wolne miejsce, ma to powiedzieć — „czeka na wolne miejsce (2 z 3 zajęte
w innych folderach)" — a nie milczeć i wyglądać na zawieszoną. Milczące czekanie jest nieodróżnialne
od zepsucia i to jest dokładnie ten rodzaj cichej porażki, którego ten dokument pilnuje.

### Trwałość

Otwarte karty i ich workspace'y to stan UI, nie stan biegu: `~/.loadout/ui.json`, zapisywany
z debounce'em. Biegi żyją w SQLite i w plikach, jak dotąd. Restart aplikacji odtwarza karty
i podpina je do biegów, które przetrwały — a te, które nie przetrwały, przechodzą przez
zwykłą ścieżkę odzyskiwania z §2 pyt. 1 (`interrupted`, pytamy, nie zgadujemy).

### Wybór workspace'a

`tauri-plugin-dialog` (już w zależnościach) na wskazanie folderu, plus lista ostatnio używanych
w tym samym menu. Folder bez repozytorium git jest dozwolony, ale wtedy **mówimy wprost**,
że kopia plików per krok jest niedostępna i kroki będą pracować w tym samym katalogu —
bo izolacja kroków stoi na `git worktree`.

## 6b. Edytor workflow: warstwa orkiestracji, nie powtórka vendora

Decyzja i uzasadnienie: `docs/DECISIONS-LOCKED.md` §D6. Tutaj jest to, co z niej wynika dla modelu danych.

### Pięć zadań i jak każde jest wyrażone

| Zadanie | W modelu danych | W UI |
|---|---|---|
| Kolejność i zależności | krawędzie grafu, jedno znaczenie: „po" | strzałka |
| Który model pracuje | `model` w definicji agenta, nadpisywalny w węźle | pole w modalu kroku |
| Kilku agentów naraz | `copies: n` na węźle | `×3` na kafelku |
| **Synteza wyników** | krok z **wieloma krawędziami wchodzącymi** czyta wiele plików przekazań | `czyta: 4 przekazania` na kafelku |
| Kontekst i analiza u orchestratora | orchestrator dostaje w prompcie indeks przekazań, nie ich treść | panel „co ten agent dostał" |

**Synteza nie jest nowym typem węzła.** To zwykły krok, do którego wchodzą trzy strzałki zamiast
jednej. Model przekazań (§8) już to obsługuje — front-matter każdego pliku ma `from` i `to`, więc
„zsyntetyzuj wyniki czterech researcherów" to krok, którego `reads` wskazuje na cztery pliki.
Jedyne, co dochodzi, to **widoczność tego faktu na kafelku** — bo krok czytający cztery wejścia
zachowuje się inaczej niż czytający jedno i użytkownik musi to widzieć bez otwierania modalu.

**Orchestrator dostaje indeks, nie treść.** Wrzucenie czterech pełnych raportów do promptu
orchestratora to najprostsza droga do przepełnienia kontekstu i do rachunku, którego nikt się nie
spodziewał. Dostaje **indeks w prompcie** — po jednym wierszu na przekazanie: kto je zostawił,
ścieżka i jedna z siedmiu angielskich **etykiet roli** — i czyta pełny plik wtedy, kiedy
zdecyduje:

1. `what the step before left`;
2. `the step before did not pass; this is what it said`;
3. `what you were given at the start`;
4. `your own earlier answer, try N of M`;
5. `an earlier answer from the work you are checking, try N of M`;
6. `what the tester said last time, try N of M`;
7. `what an earlier run left here`.

*Poprawione 2026-08-24: osobny plik `INDEX.md` nie powstaje i nie powstawał nigdy; indeks żyje
w prompcie kroku (`Live::index_of_what_came_before`, T-87).* To ta sama dyscyplina, co
progresywne ujawnianie w umiejętnościach.

### Przelotka na opcje vendora

Loadout **nie modeluje** funkcji vendorów. Modeluje agenta i przepuszcza resztę:

```rust
pub struct AgentDef {
    // ...pola, które Loadout rozumie i tłumaczy na oba vendory...
    pub vendor_options: BTreeMap<String, BTreeMap<String, String>>,  // "claude" -> {flaga: wartość}
}
```

`BTreeMap`, nie `serde_json::Value`: chcemy deterministycznej kolejności przy serializacji, żeby
zapis workflow nie produkował fałszywych różnic w gicie.

Dwie reguły walidacji — obie **przy zapisie**, nie w trakcie biegu (należą do `T-12`):

1. **Kolizja z flagą, którą ustawiamy sami, to odmowa zapisu** z nazwaniem flagi.
   Lista zarezerwowanych jest jedna, w jednym miejscu, obok budowniczego komendy:
   `--session-id`, `--output-format`, `--input-format`, `--verbose`, `--permission-mode`,
   `--strict-mcp-config`, `--setting-sources`, `-C`, `-s`, `--json`.
   Cicha wygrana którejkolwiek strony jest gorsza niż odmowa.
2. **Przelotka nie podnosi dialu bezpieczeństwa.** Pole „co agent może zrobić z plikami" jest
   tłumaczone przez nas na flagi vendora. Przelotka, która próbuje ustawić `bypassPermissions`
   albo `danger-full-access`, jest odrzucana — dial jest jedyną drogą.

### Co z tego wynika dla planu

Liczba rodzajów kafelka zostaje **stała** niezależnie od tego, ile funkcji dowiozą vendorzy —
dziś są **trzy** (krok, punkt kontrolny, sprawdzenie; trzeci dopisany decyzją człowieka
2026-08-20, `DECISIONS-LOCKED.md` §D6) plus `serve` jako rodzaj **sterownika**, nie etapu. Nowa
funkcja Claude'a to nowe pole w kreatorze agenta albo wpis w przelotce — nigdy nowy typ węzła.
To jest jedyna reguła, która utrzyma płótno czytelne przez rok.

## 6c. Triggery: trwała dostawa do istniejącej drogi Startu

*Dodane 2026-08-21 decyzją właściciela w T-65. Zakres kończy się na jednym biegu w otwartej
aplikacji: nie powstaje daemon, rejestr wielu biegów ani `stop_run(id)`.*

**Rust rozstrzyga zajętość.** Zegar żyje przy korzeniu okna, a nie przy ekranie Triggers, lecz
każdy jego tik pyta `AppState.live` przed siecią i zapisem. `RunState.workflow` pozostaje lustrem
prezentacji i nie jest autorytetem: odmowa drugiego Startu może je wyzerować, kiedy pierwszy bieg
nadal trwa. Rustowy zamek nie przechodzi przez `await`.

**Kursor jest skrótem, ledger jest prawdą.** Pod `~/.loadout/triggers/` każdy slug ma atomowo
zapisywany, ukryty ledger identyfikatorów spraw i dostaw. Pierwsze odpytanie tylko uzbraja
trigger na istniejącym backlogu. Każde późniejsze nowe `Issue.id` dostaje trwały delivery,
prealokowany UUID v7 przyszłego biegu, workflow i czas. Zmiana `updatedAt` tej samej sprawy nie
tworzy drugiej dostawy, a restart oddaje ten sam pending z tym samym UUID.

**Okno nie buduje drugiej ścieżki uruchomienia.** Pending przechodzi przez istniejące
`launchRun` i `run_workflow`; zwykły Start niesie jawne `claim: null`. Pod rustowym zamkiem
claim musi nadal pasować do sluga, `delivery_id`, workflow i UUID. Wyścig z ręcznym Startem
kończy się zwykłym `ALREADY_GOING` i zostawia dostawę pending, zamiast przesuwać kursor i zgubić
sprawę.

**Pierwszy `run.json` jest granicą akceptacji.** Plan biegu używa UUID i czasu z delivery, a
pierwszy atomowy paragon zapisuje przed procesem wyłącznie zredagowane pochodzenie: slug,
`delivery_id` i `issue_id`. Dopiero ten plik zmienia dostawę z bound na accepted. Po awarii
pasujący paragon domyka ledger bez drugiego katalogu i drugiego startu; brak paragonu pozwala
ponowić to samo wiązanie. SQLite nie uczestniczy w rozstrzygnięciu (niezmiennik 4).

### Linear konfiguruje się w oknie

*Dodane 2026-08-21 decyzją właściciela w T-74. Pierwszy prawdziwy connector to Linear; ekran
nie pokazuje nazw integracji, których produkt jeszcze nie wykonuje.*

**Formularz zapisuje konfigurację przez Rust.** Człowiek wybiera Linear, istniejący workflow
oraz sprawdzanie co 1, 5, 15 albo 60 minut. Warunek `An issue is assigned to you` jest tekstem,
bo rustowe zapytanie nie obsługuje innego filtra. Rust wybija niezmienny slug, waliduje workflow
i publikuje nowy plik bez nadpisania istniejącego. Edycja porównuje zredagowaną migawkę pól
niesekretnych; pusty klucz zachowuje najnowszy sekret z pliku, a wpisany klucz jawnie go
zastępuje.

**Sekret przekracza IPC tylko w jedną stronę.** Pole klucza jest hasłem i przy edycji zawsze
pozostaje puste. Lista zwraca wyłącznie fakt, że klucz zapisano. Plik powstaje z prywatnymi
prawami przed pierwszym bajtem, lecz T-74 nie nazywa tego szyfrowaniem ani Keychainem. Osobna
akcja `Test connection` wykonuje stałe zapytanie `viewer` przez tę samą politykę HTTPS, stdin,
wyczyszczonego środowiska i limitu czasu co watcher. Nie przyjmuje sprawy, nie rusza kursora,
ledgeru ani biegu.

**Cadence należy do pliku, heartbeat do korzenia okna.** Jeden minutowy heartbeat wylicza
osobny termin dla każdego sluga; wolniejszy trigger nie pyta wcześniej, a dwa pytania tego
samego sluga nie nakładają się. Zmiana cadence przelicza kolejny termin dopiero po potwierdzeniu
zapisu. Plik sprzed T-74 bez pola cadence zachowuje jedną minutę.

**Delete najpierw kończy pracę, potem chowa plik.** Widoczne, dwustopniowe potwierdzenie mówi,
że zapisane sprawy czekające na Start zostaną odrzucone. Rust trwale zmienia Pending na
Cancelled, a dopiero potem atomowo ukrywa konfigurację. Bound oznacza już rozpoczęty Start,
więc Delete odmawia bez mutacji i każe poczekać; zatrzymanie biegu pozostaje odpowiedzialnością
Stop. Tombstone jest czytany i sprzątany przy następnym listowaniu; po awarii zostaje więc albo
widoczny trigger bez pracy udającej Pending, albo uczciwie usunięty trigger — nigdy aktywny
plik nad skasowaną kolejką.

## 7. Sufit gęstości

Liczby ustalone **przed** pierwszym ekranem. Mierzone skryptem, nie okiem. Baseline może tylko maleć.

| Miara | Limit | Poprzedni prototyp dla porównania |
|---|---|---|
| Oznaczone regiony na ekranie | **8** | 30 [R03 §4.1] |
| Piksele chrome nad pierwszą treścią | **96** | 149 |
| Elementy niosące tekst w widoku domyślnym | **60** | 142 |
| Żywe regiony na jeden fakt | **1** | 6 (stan połączenia) |
| Linie tekstu w kafelku agenta | **4** | — |
| Regiony animujące się od jednego zdarzenia | **2** | 10–14 |
| Osie nawigacji na ekranie | **2**, i muszą być prostopadłe | 3, zachodzące na siebie |

**Budżet chrome jest już prawie wydany.** Karty 34 px + pasek loadoutu 56 px = **90 z 96**.
Zostało sześć pikseli. Każdy kolejny pasek nad treścią wymaga usunięcia innego, nie negocjacji
limitu — poprzedni prototyp podniósł swój wymuszany limit do 2,4× wartości docelowej i tak właśnie
skończył ze 149 px chrome na każdym ekranie.

**O dwóch osiach nawigacji.** Pierwotnie limit brzmiał „jedna metafora" i zarzucałem poprzedniemu prototypowi
trzy. Karty go zmieniają, ale nie łamią: warunkiem jest **prostopadłość**. Boczne menu odpowiada
na „co robię" (Praca / Workflow / Agenci / Umiejętności / Pamięć), karty odpowiadają na „w którym
folderze". Te pytania się nie przecinają, więc żadne miejsce w aplikacji nie ma dwóch odpowiedzi
na to samo pytanie. Wada poprzedniego prototypu była inna: boczne menu, pasek kart i przełącznik trybu
w strumieniu odpowiadały **na to samo pytanie** trzema różnymi sposobami. Trzecia oś wymaga
usunięcia którejś z tych dwóch.

---

## 8. Warstwa plików

```
~/.loadout/                          # globalne, między projektami
  agents/<slug>.json                 # definicja agenta — 11 pól, 9 widocznych [T4 §3]
  workflows/<slug>.json              # graf; pozycje przyciągane do 24 px [T3 §8]
  skills/<slug>/SKILL.md             # kanoniczna umiejętność
  triggers/<slug>.json               # konfiguracja; sekret nie przekracza granicy IPC
  triggers/.*                        # kursory i trwałe ledgery dostaw, niewidoczne w bibliotece
  memory/notes/<slug>.md             # jedna notatka, jeden plik; ZAKRES JEST WE FRONT-MATTERZE,
                                     # nie w katalogu (`memory/notes.rs`)
  memory/discarded/<slug>.md         # odrzucona ręką człowieka; nic nie ginie twardo (T-92)

<repo>/.loadout/                     # projektowe, bezpieczne do commitowania
  memory/…                            # wyłącznie zakres ThisProject
  runs/<ts>__<id>/
    run.json                         # workflow, kroki, status, sumy; opcjonalne redagowane pochodzenie
    handoffs/01__orchestrator__brief.md
             02__research-auth__findings.md      ← to widzisz w UI jako „co przekazał"
    attachments/02__…__full.md       # CAŁA znormalizowana odpowiedź, gdy przekazanie przekracza 8 KB
    logs/agent-<id>.jsonl            # surowe, nierenderowane domyślnie (pisze `evidence.rs`)
    mem/<krok>/                      # auto-pamięć Claude'a, przekierowana z katalogu domowego (T-92)
    claude/<work-key>/               # prywatny CLAUDE_CONFIG_DIR; refleksja używa _reflection
    claude-settings-<work-key>.json  # prywatna pamięć i deny dla jednego fizycznego spawnu
    work/<krok>/                     # kopia plików kroku — ZNIKA po biegu, praca zostaje
                                     # na gałęzi `loadout/<bieg>/<kafelek>` (T-95)
  loadout.db                         # indeks SQLite — DO SKASOWANIA BEZ STRATY
```

### Kontrakt przekazania

Front-matter pisze **Loadout**, nie agent. Agent dostarcza tylko treść [T6 §10.2].
Agent, który wymyśla własne metadane, to agent, który je zmyśli.

Silnik zapisuje każdy runtime handoff jako `findings`, niezależnie od roli kroku. Gdy ciało
przekracza 8 KB, zwykły plik dostaje wersję skróconą, a `attachments/` przechowuje **całą**
znormalizowaną odpowiedź sprzed `reshape` i cięcia — nie sam ogon.

Pamięć ma dwa korzenie i rozłączne znaczenia. `~/.loadout/memory` jest biblioteką dla zakresów
`Everywhere` i `ThisAgent`; `<repo>/.loadout/memory` zawiera wyłącznie `ThisProject`. Źle
położona notatka legacy pozostaje widoczna do ręcznego Move, ale nie wchodzi do promptu jako
wiedza z innego zakresu. Prywatny stan Claude'a nie dziedziczy `CLAUDE_CONFIG_DIR`: każda
fizyczna kopia dostaje `<run>/claude/<work-key>`, własny plik settings, a refleksja osobny klucz
`_reflection`.

---

## 9. Umiejętności: nie ma kompilatora

Najważniejsze odkrycie researchu, bo kasuje cały podsystem z planu.

Format Agent Skills jest **otwartym standardem** (agentskills.io), przyjętym przez ~45 produktów.
Wszystkie czytają **ten sam `SKILL.md` z tym samym front-matterem**. Różni je **wyłącznie katalog,
w który zaglądają** [T5 §0].

Zamiast kompilatora:

```
jeden kanoniczny folder umiejętności → zapis/symlink do 2 nazw katalogów → 6 vendorów widzi
```

Te dwa katalogi to `.claude/skills/` i `.agents/skills/`. To jest cały backend na MVP: **~300 linii Rusta.**

Wysiłek przenosi się tam, gdzie trudność jest prawdziwa:
- **P3 — wklejanie linku.** Nieufna treść, wstrzykiwanie promptu. **Wysoka trudność.**
- **P4 — dowód, że działa.** Żaden vendor nie daje testera end-to-end. **Średnio-wysoka.**

---

## 10. Stos, z wersjami

Wszystkie zweryfikowane na crates.io / npm 2026-08-15 [T7 §9.1, T8 §10].

```
Powłoka     tauri 2.11.5 · tauri-build 2.6.3 · @tauri-apps/api 2.11.1
Wtyczki     opener 2.5.4 · dialog 2.7.2 · store 2.4.4 · single-instance 2.4.3 · window-state 2.4.1
            BEZ shell, BEZ fs w webview
Rust        rustc 1.96 · tokio 1.53 · tokio-util 0.7.19 · process-wrap 9.1.0
            rusqlite 0.40.2 (bundled) · rusqlite_migration 2.6.0 · serde_json 1.0.151
            thiserror 2.0.20 · uuid 1.24.1 (v7) · tracing 0.1.44
Frontend    react 19.2.8 · vite 8.2.1 · typescript ~6.0.3 (pinowany dokładnie)
Stan        zustand 5.0.15
Style       tailwindcss 4.3.3 — tokeny w jednym bloku @theme, lustrzane wobec DESIGN.md
Prymitywy   @base-ui/react 1.7.0 · @tanstack/react-virtual 3.14.9
Graf        @xyflow/react 12.11.3
Testy       proptest 1.11.0 · vitest · playwright (tylko gesty przeciągania)
```

**Świadomie nieobecne:** `petgraph`/`daggy` (listy sąsiedztwa wystarczą), `sqlx` (rusqlite jest prostsze
w Tauri), `tauri-plugin-sql` (przecieka SQL do UI), Temporal/Restate (wymagają serwerów),
`@xterm/xterm` (odłożone razem z PTY, D4).

---

## 11. Sprzeczność do rozstrzygnięcia spike'em

**T1 twierdzi, że `--max-turns` istnieje** (sonda: flaga bez wartości zwraca `option '--x <y>' argument missing`,
a nie `unknown option`) `[ran]`.
**T4 twierdzi, że `--max-turns` nie istnieje jako flaga CLI** (sprawdzone w `--help`).

> **ROZSTRZYGNIĘTE 2026-08-23 pomiarem, `claude --help` 2.1.241.** Istnieją i są używane:
> `--effort <low|medium|high|xhigh|max>` (T-91 wozi tam pole `thinking`) oraz
> `--max-budget-usd <kwota>`, działające wyłącznie z `--print`, czyli w trybie, w którym
> Loadout i tak biegnie (T-94 wozi tam resztę budżetu biegu). `--max-turns` pozostaje
> niesprawdzone i nieużywane — limit czasu ściennego dalej jest tym, co egzekwuje Loadout.
> Spike `S-2` jest tym samym zamknięty; akapit niżej zostaje jako zapis, czym była ta niepewność.

Metoda T1 jest mocniejsza, ale to nie jest rozstrzygnięte. **Nie budujemy na tym.**
Limit czasu ściennego, który egzekwuje sam Loadout zabiciem procesu, działa u każdego dostawcy
i jest tym, co użytkownik ma na myśli mówiąc „nie mielże w nieskończoność" [T4 §3.3].
`--max-turns` wchodzi dopiero po spike'u `S-2`.

Druga niepewność: **podzbiór umiejętności dla sesji nie ma flagi CLI** [T3 §10.1].
`--disable-slash-commands` jest wszystko-albo-nic. Jeśli spike `S-1` wypadnie źle,
modal kroku degraduje się do przełącznika Wszystkie / Żadne — i tak trzeba to sprawdzić przed budową UI.
