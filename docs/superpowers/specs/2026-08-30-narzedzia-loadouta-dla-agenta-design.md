# Narzędzia Loadouta dla agenta — rozmowa, która działa

*Specyfikacja. Data: 2026-08-30. Autor rozstrzygnięć: Jakub, w rozmowie tego samego dnia.*

Dokument opisuje jeden mechanizm, który zamyka dwie luki zgłoszone przez właściciela, i nazywa
wprost, którą wcześniejszą decyzję cofa.

---

## 1. Po co to istnieje — zgłoszenie, dosłownie

> „brakuje mi w apce takiego bardziej flow rozmowy z agentem ai bo aktualnie to sie opiera do
> dispatch workflow itp ale nie ma zbytnio rozmowy ze
> - nie mam po prostu opcji pogadania z agentem i potem np claude odpala nasze workflow ktore
>   mamy zbudowane w apce
> - w trakcie planowania nie mamy wgl pytan od agentow"

Doprecyzowane w tej samej rozmowie:

> „wazne jest aby lider uzywal naszych workflow po prostu, cel apki to aby nasz np claude uzywla
> tego co mamy i aby apka wlasnie byla bardziej interaktywna, nie chce tez aby na sztywno bylo
> zeby agent lub ktowliwek zadawal 2-3 pytania, wszystko zalezy od analiz i potrzeb"

Trzy zdania, trzy wiążące wymagania:

1. **Lider ma używać biblioteki, którą człowiek już zbudował** — nie wymyślać pracy od zera.
2. **Aplikacja ma być interaktywna** — rozmowa prowadzi do pracy, praca odzywa się w rozmowie.
3. **Żadnej wymuszonej ceremonii.** Pytanie ma padać wtedy, kiedy agent naprawdę nie wie. Zero
   pytań przy jasnym zleceniu, pięć przy zagmatwanym. Reguła „zadaj 2–3 pytania" jest zakazana.

Punkt 3 nie jest preferencją stylistyczną — jest tą samą regułą, co **D7** („domyślnie: nic")
i **niezmiennik 27** („żaden etap biegu nie jest zaszyty w Ruście"), tylko wypowiedzianą
o pytaniach.

---

## 2. Co jest dziś — zmierzone, nie założone

### 2.1 Rozmowa istnieje i jest celowo odgrodzona od biegu

`commands/chat.rs` prowadzi rozmowę z liderem: jeden wątek na terminal, wiersze w tym samym
strumieniu, co bieg. Nagłówek modułu mówi wprost, że **nie ma ani jednej drogi do uruchomienia
biegu**: nie zna `RunDeps`, nie importuje `super::run`, nie widzi `RunControl`. Pilnuje tego
`tests/it/chat_never_starts_a_run.rs`.

Podstawa: rozstrzygnięcie właściciela z 2026-08-19 — *„nie, tylko komendy determinują akcje
workflow"* — po tym, jak wcześniejsza wersja po cichu startowała workflow na każdą prozę
(*„jak piszę bez komendy… to się na nowo całe workflow odpala"*).

### 2.2 Połowa drogi „rozmowa → bieg" jest zbudowana i prawie nigdy się nie zapala

`engine::line::suggested` (`line.rs:618`) zamienia wiersz prozy lidera w `Line::Suggested`,
jeśli **linia zaczyna się** od `/run <nazwa>`. `src/sections/run/feed/suggested.ts` rysuje pod
nią przycisk wołający `startFromLine` — tę samą politykę startu, co Enter.

Trzy powody, dla których przycisk się nie pokazuje:

| # | Luka | Miejsce |
|---|---|---|
| 1 | `BRIEF` mówi liderowi tylko *„say plainly that they start it with /run"* — nie mówi, że komenda ma stać w **osobnej linii**. Model wplata ją w zdanie, a `command_in` wymaga początku linii. | `chat.rs:98`, `line.rs:654` |
| 2 | Lider dostaje prawo **czytania** `~/.loadout/workflows/`, ale nie dostaje spisu ani nazw do wpisania. `typable()` żyje wyłącznie w TypeScripcie. | `chat.rs:1440`, `run-command.ts:37` |
| 3 | `/ask <agent>` nie jest proponowalny — `names_a_workflow` zna tylko `/run`. | `line.rs:668` |

### 2.3 Pytania od agentów są niemożliwe — i to nie jest nasza wada

Zmierzone 2026-08-29 na `claude 2.1.251`, tymi samymi flagami, których używa Loadout:

```
tryb -p, domyślnie:                27 narzędzi, AskUserQuestion NIE MA
--tools "...,AskUserQuestion":     system/init melduje 3 narzędzia, dalej NIE MA
odpowiedź modelu:  "I don't have an AskUserQuestion tool available in this session"
```

Skutki w drzewie:

- `stream.rs:594` (`"AskUserQuestion" => Action::Asked`) jest **martwy w produkcji**;
- `tools_for()` (`claude.rs:322`) i tak nie wymienia tej nazwy w żadnej z trzech polityk;
- `Line::Asked.options` jest zaszyte jako `Vec::new()` w obu miejscach, które ten wiersz tworzą
  (`line.rs:978`, `run.rs:10563`).

**Ekran na to jest gotowy.** Makieta ma blok pytania z przyciskami (`docs/mockup/index.html:527`);
`feed/model.ts` ma `Question`, `pinned`, kolejkę od najstarszego i `parked`. Jedyne, co dziś
potrafi to wypełnić, to `Job::Ask` — **kafelek kontrolny postawiony wcześniej przez człowieka**.
Jego odpowiedź idzie **przekazaniem do następnego kroku** (`run.rs:10513`), nie do pytającego —
bo kafelek kontrolny nie ma żadnego agenta.

### 2.4 Fazy planowania w aplikacji nie ma wcale

D4 obiecuje `❯ /plan zbuduj parser CSV`; makieta na pustym ekranie obiecuje *„Type `/plan` and say
what you want done. Loadout turns it into steps and shows you before anything runs"*.
`KNOWN` (`entry.tsx:79`) ma `/run /ask /start /history /open /stop`. Research T2 §10 planował
`/plan` przez `--permission-mode plan`; `Policy` ma trzy wartości i żadna nią nie jest.

### 2.5 Nośnik już leży w repo

`connections/runtime.rs` pisze konfigurację MCP dla obu vendorów i podaje ją flagą
(`--mcp-config` dla Claude, `-c mcp_servers.*` / `config` w `thread/start` dla Codeksa).
`claude.rs:1239` dokłada `mcp__<serwer>` do `--allowedTools`.

---

## 3. Rozstrzygnięcia człowieka z 2026-08-30

Trzy, wszystkie podjęte w rozmowie poprzedzającej ten dokument.

### D-A. Lider odpala bieg sam

Na pytanie „po rozmowie z liderem — klikasz przycisk, czy bieg rusza sam?" odpowiedź brzmi
**„rusza samo"**.

**To cofa rozstrzygnięcie z 2026-08-19 w części dotyczącej tego, KTO może zacząć bieg.** Ryzyko
zostało nazwane przed decyzją: lider, który źle zrozumie, puszcza pracę kosztującą minuty
i pieniądze, a człowiek dowiaduje się po fakcie. Właściciel je przyjął.

Co z tamtego rozstrzygnięcia **zostaje w mocy**:

- proza bez ukośnika **nie jest** komendą i nadal nie uruchamia niczego sama z siebie —
  uruchomienie jest **jawną decyzją modelu**, wyrażoną wywołaniem narzędzia, a nie skutkiem
  ubocznym tego, że człowiek coś napisał. To jest dokładnie ta awaria, którą właściciel odrzucił
  w sierpniu, i ona nie wraca;
- `commands/chat.rs` nadal nie importuje `commands::run` — nowa władza mieszka gdzie indziej
  (§6.5) i przysługuje **wyłącznie liderowi rozmowy**, nigdy krokowi w biegu (§5.2).

### D-B. Pytania działają w obu miejscach

Zarówno u lidera w rozmowie (dopytuje, zanim cokolwiek ruszy), jak i w kroku biegu (staje
w połowie i pyta). Jeden mechanizm obsługuje oba.

### D-C. Budowanie nowych workflow wchodzi w zakres, ale jako ostatni etap

Priorytetem jest **używanie tego, co człowiek ma**. Składanie nowego workflow to wyjście na
wypadek, gdy nic nie pasuje — etap 3, wycinalny bez szkody dla dwóch pierwszych.

---

## 4. Mechanizm: jeden rdzeń, dwa adaptery

**Loadout daje agentowi własne narzędzia** — tą samą drogą, którą człowiek podpina Figmę.

Do dziś agent dostaje wyłącznie narzędzia vendora (czytaj, pisz, uruchom). Dokładamy do jego
zestawu **czasowniki Loadouta**. Agent nie musi ich użyć; są dostępne, nigdy wymagane — i to
jest cała treść wymagania 3 z §1.

```
┌──────────────┐   narzędzie vendora    ┌──────────────────┐
│  agent       │ ─────────────────────► │  most (bridge)   │
│ (claude/     │ ◄───────────────────── │  proces potomny  │
│  codex)      │      wynik/odpowiedź   └────────┬─────────┘
└──────────────┘                                 │ gniazdo unixowe
                                                 │ (JSON, linia = wiadomość)
                                        ┌────────▼─────────┐
                                        │  Loadout         │
                                        │  bridge::verbs   │  ← jedyna tabela czasowników
                                        └────────┬─────────┘
                                                 │
                        ┌────────────────────────┼───────────────────┐
                        ▼                        ▼                   ▼
                  pytanie na ekran        spis z biblioteki    prośba do okna
                  (Line::Asked)           (commands::workflows) o start biegu
```

### 4.1 Dlaczego to, a nie umowa w prozie

Rozważona i odrzucona alternatywa: agent pisze `?? pytanie` w osobnej linii, Loadout to
rozpoznaje — jak dziś rozpoznaje `/run`. Tańsza o kilka dni. Odrzucona z trzech powodów, każdy
wystarczający sam:

1. **Tura agenta kończy się przed odpowiedzią.** Agent pyta i nie czeka — robi swoje, a odpowiedź
   dostaje jako nową turę, kiedy praca jest już zrobiona. Pytanie, na które nikt nie czeka, nie
   jest pytaniem.
2. **Prompt jest miękki** (niezmiennik 28). Agent może umowę zignorować i nikt się o tym nie
   dowie, bo nie ma kto sprawdzić.
3. **Opcje odpowiedzi trzeba by parsować z prozy.** Ekran chce listy; proza daje akapit.

### 4.2 Dlaczego to nie łamie D6

D6 zabrania **kafelków, które powtarzają funkcje vendorów**, i każe konfigurować per agent
wszystko, co vendorzy dowożą. Ten mechanizm:

- **nie dokłada ani jednego rodzaju kafelka.** Rodzajów zostaje trzy: krok, punkt kontrolny,
  sprawdzenie;
- **nie powtarza funkcji vendora.** Żaden vendor nie dostarcza „zapytaj człowieka i zaczekaj"
  w trybie bez terminala — zmierzone w §2.3. I żaden nie dostarcza „uruchom workflow Loadouta";
- **jest polem w definicji agenta** (§5.2), dokładnie jak każe reguła wynikowa D6.

### 4.3 Dlaczego to nie łamie D7 ani niezmiennika 27

W `scheduler.rs` nie przybywa ani jeden warunek. Silnik nie wie, że coś takiego jak „pytanie"
albo „start z rozmowy" istnieje jako pojęcie — dla niego to jest wywołanie narzędzia jak każde
inne. Ceremonia zostaje konfiguracją, a nie kodem.

---

## 5. Czasowniki

### 5.1 Tabela

| Czasownik | Wejście | Wyjście | Etap |
|---|---|---|---|
| `ask_the_person` | `question`, `options[]?` | tekst odpowiedzi człowieka | 2 |

*Etap 1 dowozi trzy dolne czasowniki; `ask_the_person` jest całą treścią etapu 2.*

| `list_workflows` | — | `[{ name, title, does, steps, shelf }]` | 1 |
| `list_agents` | — | `[{ name, title, summary }]` | 1 |
| `start_workflow` | `workflow`, `task?` | `{ started, run }` albo `{ refused: "<zdanie>" }` | 1 |
| `propose_workflow` | szkic | `{ shown: true }` — do obejrzenia, nie do biegu | 3 |

`name` jest **zawsze postacią do wpisania** (`typable`), bo to jest ta sama nazwa, którą człowiek
wpisuje po `/run`. Rozjazd między tym, co lider proponuje, a tym, co przyjmuje wiersz wejścia,
byłby liderem odsyłającym do komendy, która odmawia.

`shelf` mówi, z której półki jest workflow — z projektu czy z biblioteki (T-164 wprowadziło ten
podział i kafelek już to pokazuje).

### 5.2 Kto co dostaje — z ROLI, nie z pola w formularzu

*Poprawione 2026-08-30, po napisaniu pierwszej wersji tego paragrafu. Stał tu przełącznik
per agent („What this agent can do in Loadout", trzy pola). Wycofany z dwóch powodów, oba twarde:*

*(a) **Właściciel go odrzucił.** W rozwidleniu z §3 wariant „Zależy od lidera" — czyli dokładnie
ten przełącznik — był jedną z trzech odpowiedzi i wybrana została inna („Rusza samo"). Wpisanie go
i tak było przemyceniem odrzuconej opcji.*

*(b) **`tests/it/agents_wire_shape.rs:68` sądzi, że zapisany agent ma DOKŁADNIE szesnaście
kluczy**, a jego komunikat nazywa siedemnasty wprost: „how the form starts growing towards the
settings page nobody fills in". Pole kosztowałoby też dopisanie klucza w ~20 plikach fikstur.*

Zamiast pola — **rola w chwili wywołania**:

| Kto woła | Co dostaje | Dlaczego tyle |
|---|---|---|
| **lider rozmowy** — agent wskazany przez człowieka na pasku pracy albo w Settings | `list_workflows`, `list_agents`, `start_workflow` | wskazanie lidera **jest** zgodą człowieka, wyrażoną tam, gdzie już mieszka (T-163) |
| **krok w biegu** | w etapie 1: nic; w etapie 2: `ask_the_person` | krok startujący drugi bieg jest awarią, nie funkcją — a przy roli jest **strukturalnie niemożliwy**, nie „domyślnie wyłączony" |

Trzy reguły wiążące zostają:

1. **Nieobecne, nie „odmówi".** Czasownika, którego rola nie daje, nie ma w `tools/list` — więc
   model o nim nie wie i nie obieca człowiekowi czegoś, czego nie zrobi. To ta sama zasada, co
   przy `ToolsRefused`: cicha odmowa wygląda z zewnątrz jak agent, który „nie umiał".
2. **Rola jest liczona po stronie Loadouta**, przy składaniu listy narzędzi — nigdy przez agenta
   i nigdy z argv mostu. Most pyta o listę przez gniazdo (§6.2) i dostaje tę, która mu przysługuje.
3. **Nowa rola wymaga rozstrzygnięcia człowieka**, nie wygody implementacji — dokładnie tak, jak
   czwarty rodzaj kafelka w D6.

**Co to zostawia nierozstrzygniętym, świadomie:** nie da się mieć dwóch liderów o różnej władzy
(np. lidera do researchu, który nie odpala biegów). Jeśli okaże się potrzebne, to jest osobna
decyzja z własnym nośnikiem — a nie pole dołożone przy okazji. Etap 2 wraca do tego pytania przy
„kto może pytać", bo tam ma ono drugą twarz: bieg bez nadzoru nie chce stanąć na pytaniu.

### 5.3 Czego na tej liście NIE MA i dlaczego

- **`stop_run`.** Zatrzymanie jest odpowiedzią człowieka na to, co widzi; agent, który zatrzymuje
  cudzą pracę, jest ósmym rodzajem autorytetu, przed którym stoi całe to repo.
- **`edit_agent` / `edit_settings`.** Zmiana konfiguracji jest zapisem w bibliotece człowieka,
  a nie czynnością w rozmowie. Do rozważenia osobno, nigdy przy okazji.
- **`answer_for_the_person`.** Nie istnieje i nie powstanie.

---

## 6. Kształt techniczny

### 6.1 Most jest tym samym programem, odpalonym z flagą

`main.rs` ma dziś cztery linie i woła `loadout_lib::run()`. Dostaje rozgałęzienie **przed**
startem Tauri:

```
loadout --bridge <ścieżka-gniazda>   →  pętla MCP po stdio, bez okna
loadout                              →  aplikacja
```

Powody, dla których to nie jest osobny plik wykonywalny ani serwer HTTP:

- **zero nowych zależności.** `serde_json` i `tokio` już są; gniazdo unixowe to cecha `net`
  w `tokio`, nie nowa skrzynia. Serwer HTTP oznaczałby `axum`/`hyper` i kilkadziesiąt skrzyń
  w drzewie, które już ma 527 i mierzy czas kompilacji w minutach;
- **nic nie dochodzi do bundla.** Program już tam jest;
- **niezmiennik 6 zostaje spełniony bez ani jednej linii kodu.** Most startuje **`claude`**,
  a `claude` stoi w naszej grupie procesów — więc most też w niej stoi, ginie razem z nią
  i wchodzi do dowodu śmierci. Osobny serwer nasłuchujący po stronie aplikacji stałby poza tym
  dowodem.

### 6.2 Gniazdo i protokół

- **Jedno gniazdo na proces agenta**, nie jedno na aplikację. Tożsamością wołającego jest **samo
  gniazdo**, więc nie ma tokenów do porównywania ani sposobu, żeby krok A odpowiedział na pytanie
  kroku B.
- **Gdzie leży:**
  - krok biegu — `<katalog biegu>/<krok>/bridge.sock`;
  - rozmowa — `<folder>/.loadout/connections/<id agenta>/bridge.sock`, czyli w katalogu, który
    rozmowa już zakłada na swoją konfigurację MCP (`chat.rs:1661`).
- **Prawa `0600`.** Ścieżka gniazda **nie jest sekretem** i wolno jej stać w argv mostu
  (niezmiennik 9 dotyczy promptu i sekretów). Zdolnością jest prawo do pliku, nie znajomość
  ścieżki. Ograniczenie do odnotowania: proces tego samego użytkownika może się podłączyć — ale
  taki proces równie dobrze czyta `~/.loadout`, więc to nie jest nowa dziura.
- **Protokół:** JSON, jedna wiadomość na linię. Ten sam kształt, co strumienie vendorów, więc
  czyta się go tym samym nawykiem.

### 6.3 Adapter Claude — zmierzony, działa

Sonda z 2026-08-29, dokładnie flagami Loadouta (`-p`, `--strict-mcp-config`,
`--setting-sources ""`, `--permission-mode dontAsk`, `--output-format stream-json`):

```
init tools:  ['Glob', 'Grep', 'Read', 'mcp__loadout__ask_the_person']
mcp_servers: [{'name': 'loadout', 'status': 'connected'}]

TOOL_USE   mcp__loadout__ask_the_person
           {"question": "A CSV row has more columns than the header. What would
                         you like me to do about it?",
            "options": ["Ignore/drop the extra columns",
                        "Treat it as an error and stop",
                        "Show me the row so I can decide",
                        "Something else (please specify)"]}
TOOL_RESULT  ->  'The person answered: "Keep them, unnamed."'
TEXT:            You answered: **"Keep them, unnamed."**
RESULT success   duration_ms = 9589      (z czego 6000 ms to celowy sen serwera)
```

Cztery fakty, każdy wiążący dla implementacji:

1. **`--tools` nie rządzi narzędziami MCP.** W tej sondzie `--tools` wymieniało `Read,Grep,Glob`,
   a narzędzie MCP i tak weszło do zestawu. Wystarczy `mcp__loadout` w `--allowedTools`, czyli
   **dokładnie ten szew, który już istnieje** (`claude.rs:1239`). Tabela polityk (`tools_for`)
   zostaje nietknięta.
2. **Serwer łączy się w trybie `-p`.**
3. **Model sam produkuje pytanie z listą opcji** — kształt, którego chce gotowy ekran.
4. **Wywołanie blokuje turę**, a odpowiedź wraca do kontekstu agenta.

Konfiguracja mostu dopisuje się do tego samego pliku `claude-mcp.json`, który
`connections::runtime::for_driver` już pisze, obok zatwierdzonych połączeń człowieka.

### 6.4 Adapter Codex — protokół znaleziony, sonda do wykonania

Sterownik Codeksa **nie używa `codex exec`** — używa `codex app-server --listen stdio://`
i sam jest klientem JSON-RPC (`codex.rs:1810`). To zmienia obraz na korzyść.

**Zmierzone 2026-08-29 na `codex-cli 0.150.1`:**

Pod `codex exec` z konfiguracją MCP Codex **wywołał narzędzie**, ale odbił się o politykę:

```
mcp_tool_call  server=loadout  tool=ask_the_person  status=failed
error: "MCP tool call requires approval, but approval policy is never"
```

`"approvalPolicy": "never"` stoi wprost w `thread/start` (`codex.rs:1285`).

`codex app-server generate-json-schema --experimental` ujawnia **jedenaście żądań serwer→klient**,
w tym trzy istotne:

```
item/permissions/requestApproval     ← tędy przechodzi zgoda na wywołanie MCP
item/tool/requestUserInput           ← NATYWNY kanał „zapytaj człowieka"
mcpServer/elicitation/request        ← standardowa elicytacja MCP
```

`ToolRequestUserInputParams` niesie `questions[]` z polami `header`, `question`,
`options[] {label, description}`, `isOther`, `isSecret` oraz `isBlocking`; odpowiedź mapuje id
pytania na `answers[]`. To jest **ten sam kształt**, którego chce nasz ekran.

**Luka po naszej stronie, precyzyjnie:** pętla czytająca w `codex.rs:1099` bierze każdą wiadomość
z `id`, szuka jej wśród **naszych** oczekujących żądań i — jeśli nie znajdzie — **porzuca ją
w ciszy** (`state.decoder.dropped += 1`). Żądanie przychodzące od serwera nie ma więc dziś jak
dostać odpowiedzi, a Codex czekałby na nią bez końca.

**Szew:** gałąź w tej pętli dla wiadomości, która ma **i `id`, i `method`** — czyli żądania, nie
odpowiedzi. Rozgałęzienie idzie do tej samej tabeli czasowników, co adapter Claude.

**Sonda, którą trzeba wykonać PRZED implementacją etapu 2 dla Codeksa** (najtańsza wersja):
podnieść `codex app-server --listen stdio://`, wystartować wątek z `approvalPolicy` innym niż
`never`, poprosić model o wywołanie narzędzia z serwera `loadout` i zapisać, **które** z trzech
żądań przyjdzie. Wynik rozstrzyga między dwiema drogami:

- **droga A** — odpowiadamy `item/permissions/requestApproval` zgodą **wyłącznie** dla serwera
  `loadout`, a `approvalPolicy` dla wszystkiego innego zostaje `never`. Piaskownica bez zmian;
- **droga B** — jeśli `item/tool/requestUserInput` odpala się bez MCP, Codex dostaje pytania
  **natywnie**, a MCP zostaje mu tylko dla czasowników bibliotecznych.

**Obie drogi są dopuszczalne i obie mieszczą się w niezmienniku 23**, bo rdzeń („czym jest
pytanie, kto pyta, dokąd idzie odpowiedź") zostaje jeden, a różni się wyłącznie adapter. Czego
robić NIE WOLNO: podnieść `approvalPolicy` globalnie i zacząć zatwierdzać wszystko, co przyjdzie —
to oddaje piaskownicę w zamian za jedno narzędzie.

### 6.5 Gdzie mieszka polityka — niezmiennik 23 rozpisany

| Fakt | Jedyny dom | Kto go czyta |
|---|---|---|
| lista czasowników: nazwa, opis, schemat, wymagane pozwolenie | `bridge/verbs.rs` | odpowiedź `tools/list` **i** rozdzielnik wywołań |
| co znaczy „ta rola może" | `bridge/verbs.rs` (tabela ról) | odpowiedź `tools/list`, przy składaniu listy |
| jak nazywa się workflow do wpisania | `typable` — Rust, lustrzane wobec TS (§9.4) | `list_workflows`, wiersz wejścia |
| **który workflow, ile naraz, w którym folderze** | `launchRun` / `startFromLine` (TypeScript) | **niezmienione** — patrz niżej |
| czy w tym zakresie coś już biegnie | `AppState::begin_run` (`ipc.rs:1056`) | jak dziś |

**Start biegu przechodzi przez okno i nie ma innej drogi — to jest wymuszone architekturą, nie
wybrane.** Kanał wierszy do okna umie zbudować **wyłącznie okno** (`ARCHITECTURE.md` §3–§4;
potwierdzają to nagłówki `openChat` i `start` w `io.ts`). Rust nie ma jak sam zacząć biegu, który
byłoby widać. Więc `start_workflow`:

1. most przyjmuje wywołanie i **zawiesza je**;
2. Loadout prosi okno o start — okno wykonuje **ten sam** `startFromLine`, co Enter i co przycisk
   propozycji;
3. wynik (`null` albo zdanie odmowy) wraca do agenta jako wynik narzędzia.

Dzięki temu polityki startu nie przybywa ani o jedną kopię, a lider dostaje **dokładnie to samo
zdanie odmowy**, które zobaczyłby człowiek — łącznie z „w tym zakresie już coś biegnie".

---

## 7. Pytanie: droga tam i z powrotem

### 7.1 Ścieżka

```
agent woła ask_the_person
   → most → gniazdo → Loadout
   → Line::Asked { agent, text, options }   ← wiersz w tym samym strumieniu, co reszta
   → ekran: pytanie przypięte, opcje jako przyciski   (JUŻ ZBUDOWANE)
   → człowiek odpowiada
   → answer_question(id, odpowiedź)
   → Loadout → gniazdo → most → wynik narzędzia
   → agent pracuje dalej, znając odpowiedź
```

`Line::Asked` istnieje i ma pole `options` — dziś zawsze puste. Ta zmiana jest **pierwszym
wypełniaczem tego pola**.

### 7.2 Kanał per pytanie — zamyka H24

`PLAN-HARDENING.md:100` notuje: *„Dwa równoległe pytania do człowieka dzielą jeden slot
odpowiedzi"*. Dziś odpowiedź idzie przez `RunControl::take_answer` — jeden slot na cały bieg.
Przy dwóch krokach pytających naraz odpowiedź trafia w losowe pytanie.

**Wymóg:** odpowiedź jest kluczowana **identyfikatorem pytania**, którym jest identyfikator
wywołania narzędzia. Front już rysuje kolejkę od najstarszego (`feed/model.ts:655`), więc widok
się nie zmienia — zmienia się to, że odpowiedź trafia tam, gdzie ma.

### 7.3 Pytanie przeżywa zamknięcie aplikacji

Niezmiennik 4: pliki są prawdą. Pytanie bez odpowiedzi zapisuje się do `run.json` razem
z opcjami; odpowiedź zapisuje się obok. Bez tego bieg wznowiony po awarii stoi na pytaniu,
którego nikt już nie widzi.

### 7.4 Stop

Stop na przypiętym pytaniu **anuluje wywołanie**: most oddaje agentowi wynik mówiący, że nikt nie
odpowiedział i praca jest zatrzymana, a grupa procesów schodzi z dowodem (niezmiennik 6). To ta
sama odpowiedź, którą Stop znaczy wszędzie indziej.

### 7.5 Bez sufitu czasu — świadomie

Pytanie czeka tak długo, jak trzeba. Powody: zablokowane wywołanie narzędzia **nie pali tokenów**
(zmierzone w sondzie — 6 sekund snu nie kosztowało nic poza czasem), a kafelek kontrolny parkuje
bieg bezterminowo już dziś. Sufit czasu wprowadzałby trzeci sposób kończenia pytania obok
odpowiedzi i Stopu, a każdy z nich musi mieć zdanie na ekranie.

**Praca bez nadzoru potrzebuje własnej odpowiedzi i etap 2 musi ją dać:** agent, który nie ma
`ask_the_person` w zestawie, nie ma jak zapytać ani na czym stanąć — więc pytanie „kto może pytać"
jest zarazem pytaniem „co się dzieje, kiedy nikogo nie ma przy klawiaturze". Rola tego nie
rozstrzyga, bo krok biegu bywa i nadzorowany, i nie. **To jest jedyne rozstrzygnięcie, które
etap 2 musi wziąć od człowieka, zanim ruszy** (§5.2, akapit końcowy).

---

## 8. Etapy

Kolejność jest **kolejnością ryzyka**, nie ważności: najpierw dowodzimy, że rura działa, na
czasownikach, których odpowiedź przychodzi sama — bez czekania na człowieka, czyli bez
najtrudniejszej części (§7).

### Etap 1 — lider używa Twojej biblioteki i sam odpala

Most, gniazdo, `bridge/verbs.rs`, adapter Claude, trzy czasowniki, z których żaden nie czeka na
człowieka (`list_workflows`, `list_agents`, `start_workflow`), rola lidera jako źródło pozwoleń.

**Most wchodzi wyłącznie do ROZMOWY, nie do kroków biegu.** Krok nie ma w etapie 1 ani jednego
czasownika, więc jego argv zostaje co do bajtu takie, jak dziś — a przypięte kryteria argv biegu
nie mają się o co przewrócić.

Po tym etapie: mówisz „zrób mi X", lider patrzy, co masz, wybiera i odpala. Bez klikania.

### Etap 2 — pytania

`ask_the_person`, wypełnienie `Line::Asked.options`, kanał per pytanie (H24), zapis pytania do
`run.json`, Stop na pytaniu, adapter Codeksa (po sondzie z §6.4).

Po tym etapie: agent — w rozmowie i w biegu — staje i pyta, kiedy nie wie.

### Etap 3 — gdy nic nie pasuje

`propose_workflow`: lider składa nowy workflow z **Twoich** agentów i pokazuje go do obejrzenia,
zanim ruszy. To jest obietnica z pustego ekranu makiety.

Etap 3 jest wycinalny. Etapy 1 i 2 są niezależne od siebie w tę stronę, że każdy sam w sobie
domyka jedno ze zgłoszeń z §1.

---

## 9. Co się przy tym zepsuje — nazwane, nie odkryte później

### 9.1 `tests/it/chat_never_starts_a_run.rs`

Ten plik sądzi rozstrzygnięcie, które D-A cofa. **Nie kasujemy go i nie osłabiamy po cichu.**

- **Zostaje** kryterium skanujące zależności modułu: `commands/chat.rs` nadal nie ma drogi do
  biegu i po tej zmianie to jest **dalej prawdą** — nowa władza mieszka w `bridge/`, nie tam.
- **Zmienia się nagłówek**: dopisujemy, że reguła produktowa została cofnięta 2026-08-30 decyzją
  właściciela, i wskazujemy ten dokument. Plik, którego „po co to istnieje" kłamie, jest gorszy
  niż jego brak.
- **Dochodzą nowe kryteria**, w pliku mostu: krok biegu **nie ma tego narzędzia w liście**;
  lider rozmowy startuje i dostaje z powrotem zdanie odmowy, kiedy start się nie uda.

### 9.2 Czwarta droga do uruchomienia biegu

Dziś są trzy: przycisk Start, `/run` w wierszu, wyzwalacz. Dochodzi czwarta. Ochrona przed
rozjazdem jest w §6.5: wszystkie cztery kończą w `startFromLine` → `launchRun` → `begin_run`.
**Kryterium ma tego pilnować wprost**, bo cichy rozjazd tutaj wygląda tak: liczba „ile naraz"
jest wczytywana, logowana i inna.

### 9.3 Martwa gałąź `AskUserQuestion`

`stream.rs:594` zostaje, **z dopisanym powodem**: vendor nie daje tego narzędzia w trybie `-p`
(pomiar w §2.3), więc gałąź nie odpali — a bez tej notatki ktoś ją „naprawi", dokładając nazwę
do `tools_for()` i dostając agenta, który obiecuje pytanie i go nie zadaje.

### 9.4 `typable` po obu stronach granicy

Nazwa do wpisania musi znaczyć to samo w Ruście (`list_workflows`) i w TypeScripcie (wiersz
wejścia). Dwie implementacje jednej reguły rozjeżdżają się cicho: lider proponuje nazwę, której
wiersz wejścia nie zna.

**Rozwiązanie idiomem tego repo:** wspólna fikstura par `nazwa → typable`, czytana przez
kryterium rustowe **i** przez spec vitesta — ten sam zabieg, co lustro drutu w `src/ipc/types.ts`.

### 9.5 `commands.golden.txt`

`answer_question` jest nową komendą IPC, a `ipc_commands_registered.rs` porównuje listę handlera
z `src-tauri/commands.golden.txt` co do sztuki. Plik golden trzeba dopisać w tym samym commicie,
inaczej bramka jest czerwona z powodu, który nie ma nic wspólnego z pracą.

### 9.6 `BRIEF` lidera

Zdanie *„You cannot start a workflow run… Only the person can start work, by typing /run"*
przestaje być prawdą dla lidera, który ma czasowniki. `BRIEF` musi zależeć od roli — tak jak dziś
zależy od dialu plików (`MAY_WRITE_DRAFTS` → `LOOK_ONLY`). Lider, który ma narzędzie i prompt
mówiący, że go nie ma, jest najgorszą z możliwych kombinacji.

**Czego w `BRIEF` być NIE MOŻE:** ani jednego zdania każącego zadać pytanie. To jest wprost
zakazane wymaganiem 3 z §1.

---

## 10. Czego ta specyfikacja świadomie nie zawiera

- **`/plan` jako komendy.** Etap 3 daje `propose_workflow` jako czasownik, nie komendę w wierszu.
  Komenda dochodzi wtedy, kiedy czasownik się sprawdzi.
- **`--permission-mode plan`.** Research T2 §10 planował tędy `/plan`; ta droga nie odpowiada na
  żadne ze zgłoszeń z §1 i nie wchodzi do zakresu.
- **Zatwierdzania akcji agenta w locie.** `PLAN.md` §7 odkłada to poza v1. Most **jest** nośnikiem,
  którego to wymagało, ale sama funkcja to osobna decyzja.
- **PTY.** D4, bez zmian.
- **Sufitu czasu na pytanie.** Powód w §7.5.
- **Prawa agenta do zatrzymywania biegu ani do zmiany konfiguracji.** §5.3.

---

## 11. Ryzyka i najtańsza sonda na każde

| # | Ryzyko | Jeśli się sprawdzi | Najtańsza sonda |
|---|---|---|---|
| 1 | Codex nie da się nakłonić do wywołania MCP bez otwierania piaskownicy | Codex dostaje pytania natywnym `item/tool/requestUserInput`, a czasowniki biblioteczne dopiero po rozstrzygnięciu | §6.4 — jeden wątek `app-server`, zapisać, które żądanie przyjdzie |
| 2 | Vendor zmienia zestaw narzędzi `-p` i `AskUserQuestion` wraca | Nic nie pęka; nasz czasownik jest niezależny | powtórzyć pomiar z §2.3 po każdej aktualizacji CLI |
| 3 | Lider odpala nie ten workflow, co trzeba | Ryzyko przyjęte przez właściciela (D-A). Łagodzi je odmowa drugiego biegu w zakresie i widoczny wiersz startu | obserwacja na prawdziwym biegu |
| 4 | Most nie wstaje, bo `claude` nie ma `PATH` do binarki Loadouta | Agent traci czasowniki po cichu | ścieżka bezwzględna w konfiguracji MCP, nigdy nazwa — i kryterium na to |
| 5 | Blokujące wywołanie zatrzymuje pętlę czytającą strumień | Ekran zamiera przy pytaniu | pytanie z kroku, który w tym samym czasie coś pisze; wiersze mają dochodzić |

---

## Załącznik — dowody pomiarowe

Wszystkie pomiary wykonane 2026-08-29 na tej maszynie, prawdziwymi binarkami
(`/opt/homebrew/bin/`, nie opakowaniami Supersetu z `PATH`).

| # | Co | Wynik |
|---|---|---|
| P1 | `claude 2.1.251 -p`, domyślny zestaw | 27 narzędzi, bez `AskUserQuestion` |
| P2 | to samo z `--tools "…,AskUserQuestion"` | `init` melduje 3 narzędzia; model: *„I don't have an AskUserQuestion tool available in this session"* |
| P3 | serwer MCP po stdio + `--allowedTools "…,mcp__loadout"`, `--tools` bez niego | `mcp__loadout__ask_the_person` **w zestawie**, serwer `connected`, wywołanie z opcjami, blokada 6 s, odpowiedź w kontekście, `duration_ms 9589` |
| P4 | `codex 0.150.1 exec --json` + MCP | wywołanie doszło do serwera; `status=failed`, `"MCP tool call requires approval, but approval policy is never"` |
| P5 | `codex app-server generate-json-schema --experimental` | 11 żądań serwer→klient, w tym `item/tool/requestUserInput`, `item/permissions/requestApproval`, `mcpServer/elicitation/request` |

Skrypty sond leżą poza repo (katalog roboczy sesji). P3 warto **odtworzyć jako kryterium**
w etapie 1 — jest najtańszym dowodem, że most stoi.
