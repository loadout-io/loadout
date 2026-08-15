# T-04 — `AgentDriver` i `ClaudeDriver`: jeden długo żyjący proces

Trzy sposoby, na jakie to zadanie wychodzi cicho źle, wszystkie zmierzone.
**Pierwszy: test, który czyta źródło zamiast zachowania.** Selftest w repo źródłowym asertował
`"--sandbox workspace-write" in ship-task.sh`, przechodził **na komentarzu**, a żywa flaga brzmiała
`danger-full-access` [raport 06 §2, niezmiennik 20]. Tutaj ten sam kształt kosztuje pieniądze: bez
`--strict-mcp-config --setting-sources ""` jeden bieg ładuje 73 narzędzia MCP z 9 serwerów i pali
36 870 tokenów tworzenia cache'u zamiast 4 725 [T1 §3.3, korekta 4]. Nic nie pęka — jest tylko drożej
i wolniej, na każdym kroku, na zawsze.
**Drugi: `subtype`.** Nieudany bieg przyszedł z `"subtype":"success"` przy `"is_error":true`
i `"terminal_reason":"api_error"` [T1 §4.4, potwierdzone ponownie]. Sterownik czytający `subtype`
melduje sukces kroku, który nie zrobił nic, a stożek poniżej rusza na pustym przekazaniu.
**Trzeci: `rate_limit_event`.** Pola są **zagnieżdżone** w `rate_limit_info`, a nie płaskie
[T1 korekta 3]. Parser napisany pod płaski kształt „nie widzi nic" — deserializacja się udaje,
zdarzenia nie ma, banner się nie pokazuje, bieg nie pauzuje i dowiadujesz się o tym z rachunku.

**Read first:**
`docs/research/topics/T1-agent-drivers.md` §8.3 (dokładne, zweryfikowane argv — to jest kontrakt
AC-1 i AC-2), §8.2 (kształt traitu i typów), §8.5 (kompletność z `is_error` + `terminal_reason`,
`#[serde(other)]`, trzystopniowa eskalacja anulowania), §4.4 (pułapka `subtype`), §4.6
(dwukierunkowy stdin `[ran]`, koperta wiadomości, protokół `control_request`), §3.3 (dlaczego
**nie** `--bare`: nigdy nie czyta OAuth ani keychaina i wywala subskrypcję na
`Not logged in · Please run /login`), sekcja Fact-check korekty 1, 3, 6 i 10 (co w raporcie jest
nieprawdą — czytaj je zanim skopiujesz cokolwiek z §4.4) oraz „Worth adding" (wolny konsument
opóźnia wyjście do 30 s; to nie jest zawieszenie).
`docs/ARCHITECTURE.md` §4 (linia argv w wersji wiążącej), §11 (`--max-turns` jest **nierozstrzygnięty**
— sprzeczność T1 vs T4, spike S-2; nie budujemy na tym).
`docs/PLAN.md` §2 (S-2) i §8 (założenie ryzyka 1: to zadanie jest jego sondą).
`docs/DECISIONS-LOCKED.md` D3 (dwie implementacje od początku; ta jest pierwszą z dwóch).
`AGENTS.md` §3 — niezmienniki 5, 9, 16, 20, 21, 23, 24.
Fikstura: `docs/research/fixtures/claude-stream.jsonl` — 16 prawdziwych linii z tej maszyny,
w tym `rate_limit_event`, `result/success` i `system/init` z `capabilities`. To jest złoty plik
tego zadania; nie pisz JSON-a ręcznie, bo ręczny zawsze dryfuje w stronę optymizmu [T7 §8.1].

## Kto to robi

- **Agent:** `rust-core` — pisze `claude`
- **Druga opinia:** `codex` (nigdy ten sam vendor; D3)
- **Artefakty biegu:** `runs/T-04/` — transkrypt, plik wyników, plan. Nigdy `$TMPDIR`.

## Co to zadanie posiada

- `src-tauri/src/engine/drivers/mod.rs` — `trait AgentDriver`, `trait AgentHandle` oraz typy
  neutralne wobec vendora: `RunSpec`, `Policy`, `AgentEvent`, `Outcome`, `FinishReason`,
  `SessionRef`, `Tokens`, `Probe` [T1 §8.2]. Deklaracja `pub mod claude;`.
- `src-tauri/src/engine/drivers/claude.rs` — `ClaudeDriver`, budowa `Command`, koperta stdin,
  dekoder linii Claude → `AgentEvent`, eskalacja anulowania.
- Siedem plików testowych wymienionych w `check:` (blok OWNS na końcu).

**Czego NIE posiadasz, a czego potrzebujesz.** Jednego wiersza `pub mod drivers;` w
`src-tauri/src/engine/mod.rs`, który należy do T-02 (T-02 zostawia tam komentarz z listą takich
wierszy). To jest **pierwsza rzecz do zrobienia i jest poza twoim blokiem OWNS**: AGENTS.md §7,
zapytaj człowieka, zanim napiszesz choćby szkielet — bez tego wiersza żaden twój test się nie
skompiluje, a bramka odrzuci `unresolved import` jako fałszywą czerwień, więc `./verify.sh before`
nie powie ci niczego prawdziwego.

**Granica wobec T-05.** `claude.rs` posiada **wire enum Claude i mapowanie linia → `AgentEvent`**.
`stream.rs` (T-05) posiada pętlę czytającą, tee surowego logu na dysk i kurację `AgentEvent` → `Line`.
Ten podział jest jedynym, przy którym `CodexDriver` (T-10) powstaje **bez dotykania `stream.rs`** —
a jeśli go dotknie, trait nie jest abstrakcją i to jest sygnał, nie porażka [PLAN §8, ryzyko 5].

**Wszystko, czego dotyka test integracyjny, musi być `pub`.** Pliki w `src-tauri/tests/` to osobne
skrzynie; `pub(crate)` jest z nich niewidoczny, a „naprawa" przez przeniesienie testu do `#[cfg(test)]`
w module złamałaby regułę „jedno kryterium, jedna ścieżka pliku" z `AGENTS.md` §2a.

## Niezmienniki

- **1 — `engine/` nie importuje `tauri::*`.** `drivers/` leży pod `engine/`, więc obowiązuje i tu.
  Łamie się cicho przez **string**: `checks/quick-boundary.sh` grepuje `-i tauri` po niekomentowanych
  liniach, a literał ze ścieżką `src-tauri/...` przewraca granicę. Ścieżki przychodzą argumentem.
- **5 — nigdy nie wywalaj biegu na nieznanym zdarzeniu.** `#[serde(other)]` na wire enumie i
  `Option<T>` na każdym polu, które nie jest niezbędne. Cicha wersja złamania jest w **pętli**,
  nie w typie: `let ev = serde_json::from_str(&line)?;` — enum ma `Unknown`, a `?` i tak kończy
  krok na pierwszej linii, która nie jest JSON-em. Nieznaną linię logujemy i porzucamy.
- **9 — prompt i sekrety wyłącznie przez stdin.** Prompt jedzie kopertą
  `{"type":"user","message":{"role":"user","content":[{"type":"text","text":"…"}]}}` i **nigdy**
  w `argv`. Cicha wersja: `--append-system-prompt` z wklejoną treścią zadania — argumenty widzi
  `ps` każdego użytkownika maszyny.
- **16 — kontrolka bez handlera nie wchodzi do repo**, w wersji dla Rusta: metoda traitu bez
  wołającego i bez testu. Dlatego `probe()` ma kryterium (AC-6), a `--max-turns` i
  `--max-budget-usd` **nie wchodzą wcale**, dopóki S-2 ich nie rozstrzygnie [ARCHITECTURE §11].
- **20 — test sprawdza zachowanie, nie obecność stringa.** Żaden test w tym zadaniu nie czyta
  `claude.rs` z dysku. Argumenty asertujemy przez `Command::get_args()`, zdarzenia przez przepuszczenie
  prawdziwych linii przez dekoder, sygnały przez to, co zapisał uruchomiony proces.
- **21 — nie pisz artefaktu, którego żaden skrypt nie czyta.** `Outcome` niesie koszt i tury,
  bo T-05 i T-06 je czytają. Jeśli dołożysz pole, wskaż palcem, kto je czyta, albo je usuń.
- **23 — polityka mieszka w jednym rdzeniu, adaptery mają po pięć linii.** `Policy` ma trzy warianty
  po ludzku (`ReadOnly` / `EditInFolder` / `Unrestricted`); mapowanie na flagi vendora jest jedną
  tabelą w `claude.rs`. Cicha wersja złamania: `if agent == "claude" { … }` w miejscu wywołania.
- **24 — komentuj DLACZEGO, zwłaszcza incydent.** Przy `--strict-mcp-config`, przy braku `--bare`
  i przy czytaniu `is_error` zamiast `subtype` ma stać datowany powód z liczbą albo z cytatem.

## Kryteria akceptacji

Dwa ostatnie kryteria uruchamiają **prawdziwy proces** — nie `claude`, tylko skrypt `#!/bin/sh`
udający go, zapisany do `tempfile::tempdir()` i podany przez seam `ClaudeDriver::with_binary(PathBuf)`.
Skrypt loguje **obok siebie** (`"$(dirname "$0")/pid.log"`, `stdin.log`), nigdy przez zmienną
środowiskową: supervisor z T-03 robi `env_clear()`, więc fikstura sterowana envem po cichu przestanie
działać. Oba testy są `#[ignore]`, więc ich `check:` niesie `-- --include-ignored`; CI uruchamia je
przez `scripts/ci.sh` → `./verify.sh full` → bramkę, bo `checks/full-test.sh` woła tylko
`cargo test --lib` i celów integracyjnych nie widzi.

Zanim odpalisz `./verify.sh before`: kompilujący się szkielet (sygnatury + jawnie złe wartości
zwrotne) i raz `cargo test --no-run --tests`. W tierze `before` sprawdzenie ma 20 s.

Fiksturę wczytuj przez
`concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/research/fixtures/claude-stream.jsonl")`.

## AC-1 Zbudowana komenda niesie zweryfikowane argv transportu i kontekstu, a promptu w niej nie ma
check: cargo test --test claude_argv_transport

Asercje na `cmd.as_std().get_args()` zebranych do `Vec<&OsStr>`, **z porównaniem sąsiedztwa**
(flaga na indeksie `i`, wartość na `i + 1`), nie przez `contains`:
`-p`; `--output-format` → `stream-json`; `--input-format` → `stream-json`; `--verbose`
(bez niego CLI odmawia: `Error: When using --print, --output-format=stream-json requires --verbose`
[T1 §3.1]); `--strict-mcp-config` obecne; `--setting-sources` → argument o **długości zero**;
przy `resume: None` jest `--session-id` → `spec.run_id.to_string()`, a `--resume` nie ma;
przy `resume: Some(id)` jest `--resume` → `id`, a `--session-id` **nie ma**.
Nieobecne muszą być: `--bare` (nigdy nie czyta OAuth, wywala subskrypcję [T1 §3.3]),
`--max-turns` i `--max-budget-usd` (spike S-2 nierozstrzygnięty; limit czasu ściennego z T-03
robi to, co użytkownik ma na myśli [ARCHITECTURE §11]).
Prompt: `spec.prompt` z unikalnym znacznikiem nie występuje jako podciąg **żadnego** argumentu.

*Słaba asercja:* `assert!(std::fs::read_to_string(<plik sterownika>).unwrap().contains("--strict-mcp-config"))`
albo `assert!(args.contains(&OsStr::new("--setting-sources")))`. Pierwsza przechodzi na komentarzu
— to jest niezmiennik 20 dosłownie. Druga przechodzi, kiedy wartością jest `"user,project"`, czyli
wtedy, gdy izolacja kontekstu **nie działa** i bieg dalej ładuje 73 narzędzia MCP. Rozróżnia:
asercja na sąsiedztwie pary i na tym, że wartość `--setting-sources` ma **zero znaków**.

## AC-2 Trzy polityki mapują się na dokładnie te flagi, a `Unrestricted` nie udaje, że coś ogranicza
check: cargo test --test claude_argv_policy

Pary sąsiadujące, po jednym `RunSpec` na wariant [T1 §8.3]:
`Policy::ReadOnly` → `--permission-mode dontAsk` + `--allowedTools Read,Grep,Glob`;
`Policy::EditInFolder` → `--permission-mode acceptEdits` + `--allowedTools Read,Grep,Glob,Edit,Write,Bash(git *)`;
`Policy::Unrestricted` → `--permission-mode bypassPermissions` i **brak `--allowedTools`** —
lista dozwolonych nie ogranicza `bypassPermissions`, wszystko jest zatwierdzone niezależnie od niej
[T1 §5.2], więc wysłanie obu naraz to kłamstwo o tym, co jest ograniczone.
Dodatkowo: wartość `--permission-mode` nigdy nie brzmi `default` — CLI jej nie wymienia w komunikacie
odrzucenia, aliasem jest `manual` [T1 korekta 10].

*Słaba asercja:* `assert!(args.iter().any(|a| *a == "--permission-mode"))` dla każdej polityki.
Przechodzi implementacja, która wypisuje `dontAsk` dla wszystkich trzech — czyli agent, któremu
obiecano `No limits`, nie może nic napisać, a agent, któremu obiecano `Read only`, nie jest
ograniczony żadnym testem. Rozróżnia: porównanie **wartości** na indeksie `i + 1` dla każdego
z trzech wariantów i asercja o **nieobecności** `--allowedTools` przy `Unrestricted`.

## AC-3 Nieznany typ zdarzenia i linia, która nie jest JSON-em, nie kończą biegu
check: cargo test --test claude_unknown_events

Sekwencja podana do dekodera linia po linii: wszystkie 16 linii fikstury, a między `assistant`
a `result` wstrzyknięte trzy linie — `{"type":"quantum_flux","payload":{"a":1}}`,
`{"type":"system","subtype":"init","brand_new_key":42}` i `not json at all`.
Asercje: `push()` dla nieznanego typu zwraca **pusty** wektor zdarzeń i nie zwraca błędu;
`push()` dla linii niebędącej JSON-em zwraca pusty wektor, a `unparsed()` rośnie o 1 i tylko o 1;
po całej sekwencji dekoder wypuścił zdarzenie `Finished` z fikstury; łączna liczba `Finished`
wynosi dokładnie 1; `system/init` z nieznanym kluczem dalej daje `Started` z
`capabilities` zawierającymi `interrupt_receipt_v1`.

*Słaba asercja:* `assert!(serde_json::from_str::<ClaudeLine>(unknown).is_ok())`. Przechodzi ją sam
`#[serde(other)]` i nie mówi nic o **biegu**: prawdziwą regresją jest `?` w pętli czytającej, który
kończy krok na pierwszej linii spoza schematu, a vendorzy dokładają typy co tydzień [niezmiennik 5,
T7 ryzyko 4]. Rozróżnia: przepuszczenie **całej sekwencji** i asercja, że `Finished` z linii
*po* śmieciu wciąż przyszło.

## AC-4 Zakończenie czyta `is_error` i `terminal_reason`, nigdy `subtype`
check: cargo test --test claude_completion

Pięć linii `result`, każda dająca inny `FinishReason`:
`{"subtype":"success","is_error":true,"terminal_reason":"api_error"}` → `Failed(_)`, `ok == false`
— to jest prawdziwa linia z nieudanego biegu `--bare` [T1 §4.4];
`{"subtype":"success","is_error":false,"terminal_reason":"completed"}` → `Completed`, `ok == true`;
`{"subtype":"error_during_execution","is_error":true,"terminal_reason":"cancelled"}` → `Cancelled`;
`{"subtype":"error_max_turns","is_error":true,…}` → `LimitReached`;
strumień, który **kończy się bez linii `result`** przy kodzie wyjścia 0 → `Failed`, a powód mówi
o braku zdarzenia wyniku (wyjście procesu jest sygnałem drugorzędnym [T1 §8.5]).
W tym samym pliku wartości z fikstury: `session_id == "d24ee572-640c-4442-9c15-587dff952b98"`,
`turns == 2`, `cost_usd == 0.1483629`, `tokens.input == 4`, `tokens.cached == 65403`,
`tokens.output == 336`.

*Słaba asercja:* `match subtype { "success" => Completed, _ => Failed(_) }` z testem, w którym
przypadek nieudany ma `subtype != "success"`. Przechodzi wszystko i **odwraca wynik dokładnie tam,
gdzie to boli**: krok, który padł na błędzie API, jedzie dalej jako `succeeded`, a jego przekazanie
jest puste. Rozróżnia: pierwszy przypadek z listy — `subtype` mówi `"success"`, a kryterium żąda
`ok == false`.

## AC-5 `rate_limit_event` jest czytany w swoim prawdziwym, zagnieżdżonym kształcie
check: cargo test --test claude_rate_limit

Prawdziwa linia z fikstury (indeks 12): pola siedzą w `rate_limit_info`. Asercje: powstaje jedno
zdarzenie z `status == "allowed"`, `resets_at == 1786800600`, `rate_limit_type == "five_hour"`.
Wariant **płaski** (te same klucze bez `rate_limit_info`) **nie** produkuje zdarzenia limitu z
wartościami domyślnymi — albo nie produkuje nic, albo liczy się jako nierozpoznana linia; w żadnym
razie nie wolno mu dać `resets_at == 0`. Trzeci przypadek: `status != "allowed"` daje zdarzenie
oznaczone jako wymagające pauzy biegu (samą pauzę robi T-21).

*Słaba asercja:* `assert!(matches!(ev, AgentEvent::Notice { .. }))`. Przechodzi sterownik, który
na każde nierozpoznane zdarzenie systemowe wypuszcza `Notice` z pustym tekstem, i przechodzi parser
napisany pod płaski kształt, który „po cichu nie widzi nic" [T1 korekta 3] — a skutkiem jest bieg,
który nigdy nie pauzuje na wyczerpanym limicie. Rozróżnia: asercja na **dokładnej** wartości
`resets_at == 1786800600` oraz przypadek płaski, który musi **nie** dać zera.

## AC-6 Jeden proces obsługuje wiele tur w jednej sesji, a `probe()` odróżnia brak CLI od awarii
check: cargo test --test claude_session_process -- --include-ignored

Skrypt-atrapa odczytuje wartość `--session-id` z `argv`, wypisuje `system/init` z tą sesją i z
`capabilities` z fikstury, po czym czyta stdin linia po linii i na każdą kopertę użytkownika
odpowiada jednym `result`. Dopisuje `$$` do `pid.log` przy każdym uruchomieniu.
Asercje: `start()` + `send()` dają **dwa** `Outcome`; oba mają ten sam `session.id`, równy UUID-owi
wygenerowanemu przez nas przed uruchomieniem [T7 §6.2]; `pid.log` ma **dokładnie jedną** linię;
zamknięcie stdin kończy proces kodem 0 [T1 §2].
`probe()`: przy atrapie zwraca `found == true` i `version` z `Some`; przy ścieżce do nieistniejącego
pliku zwraca `Ok(Probe { found: false, .. })` — **nie `Err`**, bo brak CLI to ekran ustawień,
a nie awaria startu aplikacji.

*Słaba asercja:* „oba `Outcome` mają ten sam `session.id`". Spełnia to wariant awaryjny B z
T1 §8.1 — nowy proces na turę z `--resume` — który jest legalnym fallbackiem, ale płaci zimny start
i odbudowę cache'u przy każdej turze, czyli dokładnie ten koszt, którego to zadanie ma uniknąć.
Rozróżnia: `pid.log` z jedną linią. Dla `probe()` słabą asercją jest `assert!(probe().is_ok())` —
rozróżnia sprawdzenie pola `found` przy ścieżce, której nie ma.

## AC-7 Anulowanie eskaluje: `control_request` tylko pod zdolnością, potem zabicie grupy
check: cargo test --test claude_cancel_escalation -- --include-ignored

Atrapa A ogłasza w `init` `capabilities: ["interrupt_receipt_v1","interrupt_cancel_queued_v1","msg_lifecycle_v1"]`,
zapisuje każdą linię ze stdin do `stdin.log`, na `control_request` odpowiada
`{"type":"control_response","response":{"subtype":"success","request_id":"…","response":{"still_queued":[]}}}`
i dopiero potem `result` z `subtype":"error_during_execution"`, po czym wychodzi sama.
Asercje: `stdin.log` zawiera **dokładnie jedną** linię z `"subtype":"interrupt"`; `Outcome.reason`
to `Cancelled`; proces zakończył się **sam**, nie sygnałem (sesja zostaje wznawialna [T1 §8.5]).
Atrapa B ogłasza `capabilities: []` i ignoruje stdin. Asercje: `stdin.log` **nie zawiera** żadnego
`control_request`; po `cancel()` `libc::kill(-pgid, 0)` daje `ESRCH` (ścieżka z T-03); całość poniżej 15 s.

*Słaba asercja:* `assert!(handle.cancel().await.is_ok())`. Prawdziwe jest to również dla sterownika,
który od razu wysyła SIGKILL — a wtedy tracimy wznawialność sesji, transkrypt nie zostaje dosypany
i hooki `SessionEnd` nie biegną [T1 §4.6]. Przechodzi też sterownik, który wysyła `control_request`
**zawsze**, także tam, gdzie CLI go nie obsługuje, i wisi 5 s na odpowiedzi, która nie przyjdzie.
Rozróżnia: treść `stdin.log` w obu atrapach — jedna linia interruptu w A, zero w B.

## Świadomie poza zakresem

- **`CodexDriver`** — T-10. Ono jest testem, czy ten trait jest abstrakcją, czy fikcją
  [PLAN §8, ryzyko 5]. Nie „przygotowuj" tu pod niego niczego.
- **Pętla czytająca, tee surowego `agent-<id>.jsonl` na dysk, sklejanie, kuracja `AgentEvent` → `Line`**
  — T-05. Ty produkujesz `AgentEvent`, nie linie ekranu.
- **`--max-turns`, `--max-budget-usd`** — spike S-2. Do rozstrzygnięcia nie ma ich w `RunSpec` ani
  w argv; sufit egzekwuje limit czasu ściennego z T-03.
- **Podzbiór umiejętności dla sesji (`--disable-slash-commands`, `--plugin-dir`)** — spike S-1 i T-13.
- **`--include-partial-messages`** (strumień delt tokenów) — odłożone: trzykrotnie więcej zdarzeń
  i powrót w stronę ściany logów [T1 §9].
- **Interaktywne zatwierdzanie (`canUseTool`, `--permission-prompt-tool`)** — wymaga hostowania
  serwera MCP albo hooka; listy dozwolonych narzędzi wystarczają na v1 [T1 §5.2].
- **Zmiana trybu uprawnień w trakcie sesji** — niezweryfikowana [T1 §11.1].
- **Ekran „check setup"** — T-01/T-11 rysują; ty dajesz `probe()`.
- **Zapis kosztu i sesji do SQLite** — T-06.
- **Prawdziwe wywołanie `claude` end-to-end** — świadomie nie jest kryterium: jest sieciowe, płatne
  i niedeterministyczne. Jedno takie wywołanie (`--model haiku`, ~3 s) należy do bramki manualnej
  przed wydaniem [T7 §8.2], nie do pętli.

<!-- OWNS
src-tauri/src/engine/mod.rs
src-tauri/src/lib.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/src/engine/drivers/claude.rs
src-tauri/tests/claude_argv_transport.rs
src-tauri/tests/claude_argv_policy.rs
src-tauri/tests/claude_unknown_events.rs
src-tauri/tests/claude_completion.rs
src-tauri/tests/claude_rate_limit.rs
src-tauri/tests/claude_session_process.rs
src-tauri/tests/claude_cancel_escalation.rs
-->
