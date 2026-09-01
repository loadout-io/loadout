# Stan budowy — 2026-08-18, 00:40

Ten plik jest **żywy**. Aktualizuje go orchestrator po każdym lądowaniu. Prawdą o biegu jest
jego `TASK.md` na gałęzi i `runs/<id>/`; tutaj jest wyłącznie to, czego z nich nie widać:
co już stoi w trunku, co stanęło i dlaczego.

## 2026-08-30 — SPROSTOWANIE: chrome mieści się w suficie; 137 px było pomiarem pierwszego startu

Wpis niżej twierdzi, że aplikacja przekracza sufit gęstości o 41 px. **To było nieprawdą, i wina
leży w kolektorze, nie w produkcie.** Kolektor odpowiadał aplikacji pustą listą na
`list_workspaces`, więc mierzył ekran, na którym stoi `[data-add-workspace]` — zaproszenie do
wskazania pierwszego folderu. Ten przycisk znika po pierwszym wskazaniu i nikt go więcej nie
widzi. Zmierzone obie sceny, 1512 px:

```
bez workspace   chrome = 137 px   (zaproszenie na ekranie)
z workspace     chrome =  93 px   (zaproszenia nie ma)     ← sufit 96
```

**Aplikacja, której używa właściciel, mieści się w suficie z trzema pikselami zapasu.** Rachunki
w §7 („karty 34 + pasek 56 = 90 z 96") były przez cały czas poprawne.

Rozbicie zmierzone przy okazji, bo bez niego „napraw chrome" nie było planem: odstęp kontenera
8 px, karty workspace 33, pasek loadoutu 52 — razem 93. Komentarz przy `StripProps.controls`
mówi, że przed przeniesieniem kontrolek do paska było **189 px**; ta praca została wykonana
wcześniej i to ona kupiła dzisiejszy zapas.

**Czego to uczy o samym pomiarze.** Kolektor bez opisanej sceny mierzy stan, którego nikt nie
widzi, i melduje naruszenie, którego nie ma — czyli robi dokładnie to, przed czym stoi
niezmiennik 18, tylko w drugą stronę. Scena jest teraz wypowiedziana w nagłówku
`scripts/density-collect.mjs`, a kontrola sceny odmawia pomiaru, kiedy zaproszenie stoi na
ekranie, zamiast oddać większą liczbę.

Zapadka ustawiona na 93/26/3/0 — legalnie, bo POD sufitem. Sprawdzenie wpięte w `scripts/ci.sh`
w pasie `full`, zaraz za `vite build`: kolektor potrzebuje `dist/` i Chromium, więc w pętli
zadania kosztowałby build na każdy bieg, a brak przeglądarki jest tam pominięciem z powodem,
nigdy zielenią.

## 2026-08-29 — kolektor gęstości istnieje i przy pierwszym pomiarze znalazł 41 px za dużo

`checks/density.sh` był odstawiony od 2026-08-16 z jednym brakującym ogniwem: kolektorem.
Sędzia (`scripts/density-audit.mjs`), zapadka i parser sufitu były przetestowane siedmioma
kryteriami T-22 i działały — nie było czego mierzyć. Kolektor stoi teraz w
`scripts/density-collect.mjs`.

**Pierwszy prawdziwy pomiar, na zbudowanej aplikacji, w Chromium, przy 1100 i 1512 px:**

```
labelledRegions 3/8 · chromePixels 137/96 · textElements 26/60 · animatedRegions 0/2
over the ceiling: chromePixels measured 137, ceiling 96 (over by 41)
```

**To jest prawdziwe naruszenie niezmiennika 18, nie wada pomiaru.** Sam §7 liczy sobie
„Karty 34 px + pasek loadoutu 56 px = 90 z 96" i pisze „Zostało sześć pikseli" — a zmierzone
jest 137. Czterdzieści siedem pikseli weszło nad treść, nie zauważone przez nikogo, bo jedyne
sprawdzenie, które mogło to zobaczyć, nie miało pomiaru.

Geometria, zmierzona przy 1512 px: `main` zaczyna się na 8 px (odstęp kontenera), pasek kart
około 33 px, `[data-strip]` na 41 px, a pierwsza treść (`[data-work]`, `[data-stream-column]`)
dopiero na **137 px**.

### Czego świadomie NIE zrobiono

- **Nie ustawiono zapadki.** `--update-baseline` zapisałby 137 jako punkt odniesienia, czyli
  dokładnie „zapadka ustawiona po fakcie jest zawsze ustawiona tam, gdzie akurat jesteś" —
  zdanie z nagłówka tego samego pliku, o poprzednim prototypie, który tak skończył ze 149 px.
- **Nie podłączono sprawdzenia do bramki.** To jest „JEDEN ruch" opisany w nagłówku
  `checks/density.sh`, ale wykonany dziś zamieniłby każdy bieg w czerwień do czasu naprawy UI.
  Decyzja należy do człowieka, a nie do commita, który przy okazji przynosi kolektor.
- **Nie naciągnięto pomiaru.** Pierwsza wersja liczyła „pierwszy element z tekstem wewnątrz
  `main`" i dała 11 px, bo trafiła w przycisk `＋` na pasku kart. Wyglądało to jak zieleń.
  Treść jest teraz wskazana kotwicą `[data-work]`, a jej brak jest powodem ODMOWY pomiaru.

### Cztery z siedmiu metryk, i dlaczego nie siedem

Mierzone: `labelledRegions`, `chromePixels`, `textElements`, `animatedRegions`.
Niemierzone Z POWODEM: `liveRegionsPerFact` (to, który fakt niesie region, nie jest zapisane
w DOM), `agentCardLines` (widok domyślny nie ma kafelka agenta, bo kolektor odpowiada
aplikacji pustymi listami), `navigationAxes` (§7 stawia limit jako „2, i muszą być
prostopadłe" — prostopadłość jest odczytem człowieka).

Nagłówek `checks/density.sh` nazywa wprost pułapkę, w którą tu nie wpadamy: „zrzut z siedmioma
metrykami »niezmierzone, powód: kolektor nie biegł« — sędzia by to przepuścił, i byłaby to
zieleń kupiona za zdanie". Cztery liczby i trzy nazwane granice to nie to samo.

## 2026-08-29 — audyt fazy 8 i 9 po mechanizmach; faza 8 stoi na 16 z 18

Audyt na życzenie właściciela, robiony **po mechanizmach w kodzie, nie po nazwach zadań**:
dla każdej pozycji szukałem konkretu, który musiałby istnieć, gdyby wylądowała. Pierwsza wersja
tego audytu była **za pesymistyczna o cztery zadania** — szukanie po identyfikatorze `T-2xx`
w gicie i w tym pliku daje zero trafień dla rzeczy, które stoją w trunku od tygodnia.

| ID | Wyrok | Mechanizm, który to rozstrzyga |
|---|---|---|
| T-152 | zrobione | `PrestartFaultInjector`, `PrestartFaultPoint`, trzy odmowy „nothing ran" |
| T-202 | zrobione | `src-tauri/src/durable_file.rs` wołany przez workflow, agentów, handoff, run, reconcile |
| T-203 | zrobione | `t203-bad-library-definitions-are-actionable.test.tsx` + `state/library.ts` |
| T-204 | zrobione | `feed/session-per-terminal`, `session-per-workspace`, `folding-does-not-cross-runs` |
| T-205 | zrobione | kanały ograniczone z uzasadnieniem ×3, plus T-157 i T-159 |
| T-207 | zrobione | `ExecutionFacts { executed, process_started }` — „nie wynika z PID-u ani statusu kroku" |
| T-209 | zrobione | `reclaimed_run_directory`, `owns_reclaimed_run_directory`, `block_reclaimed_parent_cleanup` |
| T-210 | zrobione | nazwa tempa `.loadout-writing-<uuid v7>.tmp` plus `is_owned_temp` |
| **T-206** | **brak** | jedyny `preflight` w drzewie jest w `import/apply.rs` i nie ma z tym nic wspólnego |
| **T-208** | **połowa** | próg dysku `25e5de5`; sufitu kosztu nie ma |

**W całym `src-tauri/src` nie ma ani jednego `todo!()` ani `unimplemented!()`.** Trzy trafienia
to komentarze o dawnych fazach kontraktowych. Jeden z nich wprowadza w błąd i został:
`run.rs` przy `run_workflow_with_prestart_faults` mówi „właściwa implementacja zastąpi `todo!()`",
a funkcja od dawna deleguje do prawdziwej drogi.

### Trzy błędy w samym planie, znalezione przy okazji

1. `Gotowe: T-150, T-151, T-157` w §6c było nieaktualne od dziewięciu lądowań. Poprawione.
2. Kolejność fal mówi „T-162 po T-156 i **przed T-204**". T-204 wylądował dawno, T-162 dopiero
   dziś. Nic się nie zepsuło, ale ograniczenie było martwe.
3. **Kolektora `density` nie da się zrobić biegiem zadaniowym.** §6c mówi „Żaden task nie zmienia
   `harness/`, `checks/`, `verify.sh`", a kolektor musi wejść do `checks/`. Albo ręka właściciela,
   albo świadomy wyjątek od tej reguły.

### Co ta weryfikacja rozstrzygnęła w T-208

Plan każe T-163 zależeć od T-208 i wygląda to dziwnie, dopóki nie zobaczy się, że to **jedna
powierzchnia**: domyślny sufit kosztu jest USTAWIENIEM. „Każdy start ma jawny cost limit" nie
znaczy więc stałej w kodzie, tylko liczbę, którą człowiek widzi i ustawia — a to zdejmuje
sprzeczność z istniejącym, celowym testem `a_run_without_a_ceiling_is_untouched`. Sufit jest
jawny, bo pochodzi od człowieka. Kosztowa połowa T-208 idzie więc PO T-163, do Settings.

## 2026-08-29, 04:00 — faza 8 domknięta produktowo; workflowy naprawione, Urc dawał się zapisać i nie dawał uruchomić

Finalny SHA: **`0fbebf8`**. Dziewięć biegów, dziewięć zielonych pełnych CI na dokładnym SHA po
merge'u, zero commitów po ostatnim lądowaniu — więc to lądowanie (452 s) certyfikuje ten SHA
bez powtórki.

| Bieg | SHA | Rundy | Koszt |
|---|---|---|---|
| `p8-t158-trigger-quarantine` | `137e0ca` | 1 | $25,19 |
| `p8-t201-process-proof` | `9d7a423` | 2 | $67,78 |
| `p8-t155-workspace-runs` | `3ff9b31` | 1 | $21,82 |
| `p8-t151-newer-truth` | `3d9c3f0` | 3 podejścia | $50,44 |
| `p8-t157-literal-secret-refused` | `9834ad6` | 1 | $11,18 |
| `p8-t154-skill-frozen-once` | `b2d50eb` | limit konta | $18,76 |
| `p8-t153-physical-file-fanin` | `4306159` | 1 | $?? |
| `p8-t156-bounded-lifecycle` | `fe05e2c` | 2 podejścia | $?? |
| `p8-t159-copy-lineage` | `0fbebf8` | 1 | $?? |

**21 mutacji, 21 prawdziwych czerwieni.** Osiem z nich to strażnicy przeciw „odmawiaj
wszystkiemu", którzy słusznie **zostali zieloni** — to mocniejszy dowód niż sama czerwień, bo
pokazuje, że testy wiążą różne rzeczy, nie jedną.

### §8: pięć workflowów, jedna transakcja, dwanaście zmian i ani jednej więcej

Backup: `~/.loadout-workflows-backup-20260829-020902`, zweryfikowany haszami przed i po.

| Workflow | Zmiana | Co naprawia |
|---|---|---|
| Murmur-1 | `Combine`, `QA` → `same-copy` | `Combine` dostawał ŚWIEŻĄ kopię i nie widział pracy `Backend` ani `Frontend`; `QA` siedział na głównym projekcie |
| Reaserch + implement | `C1`, `C2` → `fresh-copy`; `Implement` → `same-copy` | wszystkie cztery kroki były na `project`, czyli nie było czego składać |
| Deep reaserch | `Synteze` → `same-copy` | trzech rodziców na kopiach, `Synteze` na głównym projekcie |
| Urc | `Learings` i Serve → `same-copy`; nowy krok `Run the checks`; Serve przestaje poprzedzać całą pracę | patrz niżej |
| Easy | **bez zmian** | „Check dochodzi wyłącznie przy dostarczaniu kodu" czytane jako „nie na stałe" |

**Dlaczego to nie było kosmetyką, zmierzone w logu aplikacji.** `~/.loadout/loadout.log`,
2026-08-27 22:40–22:47: trigger Urc odpalał się **co minutę** (`poll_every_minutes: 1`)
i **co minutę był odrzucany**:

    WARN Loadout turned down a run said="Plan" and "Start and leave running" can run at
         the same time and both work in the project folder. Give one of them a fresh copy.

Workflow Urc był więc niewykonalny — Serve stał PRZED całą pracą (`Start and leave running →
Final implementation plan`) i ścigał się z `Planem` w folderze projektu, co niezmiennik 12
słusznie odrzuca. Potwierdzone eksperymentem przed/po na produkcyjnym `check_workflow_inner`:
**stara wersja daje 1 uwagę o tej kolizji, nowa daje 0.** Zgadza się to z AGENTS.md co do słowa:
kolizja widoczna z pliku jest przy zapisie ostrzeżeniem, a przed biegiem problemem — dlatego
plik dawał się zapisać, a bieg nie dawał się uruchomić.

**Jak walidowałem, nie ruszając repo.** Odczepiony worktree `loadout-wf-preflight` z tymczasową
sondą, która przepuszcza kandydatów przez PRODUKCYJNĄ drogę: `check_workflow_inner` →
`save_workflow_inner` → `load_workflow_inner`, do katalogu tymczasowego, nie do `~/.loadout`.
`jq -e .` mówi tylko, że plik jest JSON-em; o tym, czy Loadout go przyjmie, decyduje ta droga,
razem ze wszystkimi odmowami, które weszły 2026-08-28/29 — kolizja flag D6, literalny sekret,
obowiązkowy `proof` przy kroku „sprawdź". Sonda **nigdy nie dotknęła `main`**, sprawdzone.

`Urc` dostał `make check` jako komendę, bo jego własny `CLAUDE.md` nazywa to „the canonical
full-verification gate", a nie bo tak wybrałem. Wzorzec dowodu `passed, (\d+) total` (licznik
Jesta) wybrał właściciel, po tym jak pokazałem, że linia sukcesu nx dopasowałaby też
`0 projects` — czyli dziurę, przed którą stoi niezmiennik 19.

### §9: co jest certyfikowane, a co nie

- **Certyfikowane na `0fbebf8`:** pełne CI (452 s, 63 targety, 0 failed, strażnicy 8/8),
  aplikacja startuje i wczytuje bibliotekę, wszystkie pięć workflowów przechodzi produkcyjną
  drogę zapisu z zerem problemów.
- **Trigger Urc: `enabled: false`** — był wyłączony przed operacją i zostaje wyłączony (§9.6).
- **Cztery worktree brudne** (1, 2, 10 i 2 zmiany) — zostają nietknięte zgodnie z §9.7,
  w tym `loadout-wf-preflight` z sondą. `git worktree remove` jest w `deny`, więc usunięcie
  należy do właściciela.
- **NIE certyfikowane:** disposable smoke Murmur-1 na prawdziwych agentach (§9.4) i rotacja
  ujawnionego poświadczenia Linear (§9.5). Pierwsze kosztuje pieniądze i czeka na decyzję;
  drugie jest czynnością właściciela i nie tykam wartości sekretów.

## 2026-08-29, 00:35 — faza 8: pięć zakresów w trunku, pięć napraw harnessu

Licznik: **`b2d50eb`**. Sześć biegów, pięć wylądowanych zielono z pełnym CI na dokładnym SHA
po merge'u, jeden (`t153`) w toku. Razem **$151,25**.

| Bieg | SHA | Czas | Rundy | Koszt |
|---|---|---|---|---|
| `p8-t158-trigger-quarantine` | `137e0ca` | 2106 s | 1 | $25,19 |
| `p8-t201-process-proof` | `9d7a423` | 6047 s | 2 | $67,78 |
| `p8-t155-workspace-runs` | `3ff9b31` | 1778 s | 1 | $21,82 |
| `p8-t151-newer-truth` | `3d9c3f0` | 3 podejścia | 1 | $50,44 |
| `p8-t157-literal-secret-refused` | `9834ad6` | 1198 s | 1 | $11,18 |
| `p8-t154-skill-frozen-once` | `b2d50eb` | limit konta | — | $18,76 |

### Najważniejsza rzecz z tej fazy: zielona bramka nie widzi niezabezpieczonej bramy

`p8-t151-newer-truth` dowiózł działający mechanizm i przeszedł **wszystko**: 13 checków,
`tsc`, 826 testów rustowych, spec e2e. Rustowa odmowa spóźnionego zapisu **nie miała ani
jednego testu**. Trzy powody naraz:

- nowy spec szedł przez atrapę IPC, więc przy wyłączonej bramie rustowej pozostawał zielony;
- istniejący `workflow_save_refuses.rs` dostał `Some(&revision_of(ON_DISK.as_bytes()))`, ale
  to **poprawna** rewizja — czyli sama ścieżka szczęśliwa;
- `SaveError::Changed` nie był asertowany nigdzie w repo.

Złapała to mutacja, i to **druga** sonda. Pierwsza była zła: podmieniłem `expected` na `None`,
sądząc że wyłączam sprawdzanie. `None` znaczy tam „tego pliku ma jeszcze nie być", więc brama
się **zaostrzyła**, padł test *sukcesu* (`a_warning_still_saves…`), a ja odczytałem to jako
„oracle działa". Właściwą sondą było zepsucie samego porównania w `durable_file.rs`:
`revision_of(&found) == revision_of(&found)`. Pod nim **826 passed, 0 failed**.

Reguła na przyszłość: **mutuj porównanie, nie argument.** Padający test *sukcesu* pod mutacją
„wyłączającą odmowę" znaczy, że sonda jest zła, a nie że oracle działa.

Jedna runda z promptem nazywającym tę lukę pomiarowo dała moduł
`a_late_save_does_not_undo_newer_bytes.rs` z dwiema stronami bramy. Wszystkie biegi tej fazy
sprawdzone mutacją; łącznie **13 mutacji, 13 prawdziwych czerwieni**, w tym trzy takie, gdzie
strażnik przeciw „odmawiaj wszystkiemu" słusznie **został zielony**.

### Pięć napraw harnessu, każda osobnym commitem z incydentem

| Commit | Co | Incydent |
|---|---|---|
| `8526559` | wysiłek per faza | plan 10–12 min, implementacja 26–49 min; plan w OBU pierwszych biegach poprawił przesłankę zlecenia, więc taniejemy na implementacji |
| `13a9b88` | deny `Bash(cargo test --tests:*)` | agent odpalił pełną suitę **7 razy** (~35 min budowania); niezmiennik 28: hak odpada (brak stanu), check odpada (identyczne drzewo) → uprawnienie |
| `44197f8` | sufit tur to kod **3**, nie 1 | bieg zjadł 250 tur na 145 edycjach wachlarza, skończył z `tsc rc=0`; kod 1 kazał mi szukać defektu kodu, którego nie było |
| `39c4382` | model per rola | właściciel wyczerpał tokeny Codeksa; `--verifier claude` bez tego dawałby ten sam model dwa razy, czego D3 nie uznaje za drugą opinię |
| `e1da96a` | `.h-plan.md` odśledzony | lądowanie padło na konflikcie w brudnopisie, przy `ipc.rs` z +331 liniami zmergowanym automatycznie; komentarz w `h.py` **kłamał**, że plik jest w `.gitignore` |

Ostatnia jest niezmiennikiem 20 zastosowanym do komentarza: zdanie, które opisuje stan
zamiast go wymuszać, kłamało tygodniami i nikt tego nie sprawdził.

### Co zrobiłem źle, dla następnej sesji

1. **Druga sesja pisała do tego samego `main`.** Sesja `9334477f` biegła 29-gałęziowym
   `triage.sh` (merge → bramka → `reset --hard` na czerwieni) i przesunęła HEAD dwa razy pod
   moim pomiarem bazy. Podpisy widoczne w `ps`, żaden w `git status`: **dwa procesy
   `tee /var/folders/.../loadout-ci.*`** oraz cel testowy z **0,35 s CPU przy 4 min elapsed**
   (zagłodzony, nie zawieszony). Ubita na polecenie właściciela, od dołu do góry, osiem PID-ów
   z dowodem ESRCH. Koszt: dwa spalone `ci.sh full`.
2. **`task-T-105` wszedł na `main` bez bramki i ją zawieszał.**
   `tests/it/a_turned_down_lead_says_why.rs` nie ma ani jednego deadline'u i `await`-uje
   atrapę `codex app-server`, która stoi z `0:00.00` CPU. Cofnięte revertem jako `db79576`;
   drzewo wróciło bajt w bajt do zielonego `e9c7b89`. Relanding wymaga naprawy tego testu,
   bo w obecnym kształcie **żadna** bramka repo się nie skończy.
3. **Źle oceniłem rozłączność plikową** przy równoległości — wypisałem pliki biegu z pamięci,
   pomijając `ipc.rs`, który widziałem we własnym logu clippy. Lądowanie padło. Reguła:
   drugą gałąź przed lądowaniem merguj z `main` **w jej worktree**, nie na trunku.
4. **Commitowałem do `main`, gdy `h land` był w środku CI.** Skutek był ograniczony (tylko
   `harness/**`, którego żaden krok bramki nie czyta), ale reguła jest prosta: naprawy
   harnessu idą przed lądowaniem albo po nim.
5. **`git diff main..<gałąź>` obwinia gałąź o moje własne commity.** Gałąź wyglądała na taką,
   która tknęła `harness/h.py` — czyli łamie najostrzejszą regułę repo. Tknąłem go ja.
   O winę biegu pytaj od `git merge-base`.
6. **Backticki w promptcie podanym przez shell zostają wykonane**, a stary `<log>.rc` po
   nieudanej próbie natychmiast odpala czuwanie i melduje żywy bieg jako padnięty. Prompty
   idą teraz z plików, paragony mają świeże nazwy na każdą próbę.

### Długi otwarte, nazwane

- **`curl --fail` — T-158 nie jest domknięte.** `linear_curl_config` ma `fail`, więc HTTP ≥ 400
  ginie jako `CurlFailed` (kubełek chwilowy), a kwarantanna wymaga **200 z niepustym `errors`**.
  Jeśli Linear odrzuca zły klucz kodem 4xx, produkcja dalej puka bez końca — mechanizm zielony
  w testach, martwy w produkcie. Wymaga pomiaru żywym kluczem, czyli decyzji właściciela.
- **FNV-1a jest teraz w trzech kopiach** (`commands/run.rs`, `import/apply.rs`,
  `skills/place.rs`) — niezmiennik 23 mówi o jednym rdzeniu.
- **59 standalone targetów** w `src-tauri/tests/`, po ~60 s linku każdy przy każdym lądowaniu.
- **26 gałęzi z listy triage** nadal nie zlandowanych, `task-T-105` z revertem do odwrócenia.
- `p8-t154-skill-frozen-once` **nie był oglądany przez niezależnego weryfikatora** — limit
  konta ubił bieg przed tą fazą. Oparty wyłącznie na 12 zielonych checkach i trzech mutacjach
  (brama, licznik uruchomień, droga zdania na ekran).

## 2026-08-28, 19:05 — p8-t158-trigger-quarantine w trunku; kwarantanna po odrzuconym kluczu

**`p8-t158-trigger-quarantine` · zielone / WYLĄDOWANE jako `137e0ca` · 2106 s biegu (1 runda)
· pełne CI przy lądowaniu 494 s, 63 targety, 0 failed, strażnicy 8/8 · $25,19.**

Pierwszy bieg nowego harnessu z prawdziwym zakresem produktowym. Wszystkie trzynaście checków
zielone w pierwszym podejściu, weryfikacja Codeksem oddała `DZIALA` bez rundy naprawczej.

**Co dowozi.** Deterministyczna odmowa Lineara wstrzymuje obserwację triggera trwale, w pliku,
a nie w stanie okna: `Ledger.paused: Option<PausedReason>` (dopisek addytywny, niezmiennik 25),
piąty wariant drutu `TriggerPoll::Refused { sentence }`, `resume_with`/`resume` w Ruście oraz
komenda `resume_trigger`. Klasyfikacja siedzi w jednym `const fn lasting_refusal`:
`Api | ConnectionRefused | MissingViewer | InvalidKey | MissingKey` → pauza,
wszystko inne → dzisiejsze zachowanie, czyli błąd do okna i normalny następny tick.
Wiersz triggera mówi po angielsku „Linear refused this key, so Loadout stopped checking.
Replace the key, then press Retry." i daje kontrolkę `Retry`, która puka **dokładnie raz**.

**Planista poprawił przesłankę zlecenia, i miał rację.** `poll_with` nie zwracał
`ConnectionRefused` — ten wariant powstaje wyłącznie w `parse_connection_response`, czyli
w probie „Test connection". Deterministyczną odmową na ścieżce pollu jest `TriggerError::Api`
z `parse_response` przy niepustym `errors`. To jest dokładnie ta klasa poprawki, po której
poznaje się, że etap planu czytał kod, a nie prompt.

**Zieleń sprawdzona mutacją, nie przyjęta na słowo.** Dwie mutacje, obie z prawdziwą czerwienią:

| Mutacja | Wynik |
|---|---|
| `Api` wyjęte z kubełka w `lasting_refusal` | `FAILED`: „a refused key ended the tick with an error, not a hold" |
| `row.tsx:53` oddaje inne zdanie dla `paused` | `FAILED` na markupie z `renderToStaticMarkup`: brak zdania i „Retry" |

Obie przywrócone bajt w bajt. Sprawdzone też, że helper `a_repaired_key_gets_the_rhythm_back`
jest wołany z linii 133 (nie martwy kod) i że `rust-clippy (0s)` / `rust-test (0s)` w logu biegu
to **ciepłe drzewo** po własnych przebiegach agenta, a nie pominięty check: worktree miał
prawdziwy własny `target/`, `LOADOUT_SHARE_TARGET` nie było ustawione.

**DŁUG, który ten bieg zostawia otwarty — T-158 nie domyka się nim.** `linear_curl_config`
zawiera `fail`, więc curl przy HTTP ≥ 400 wychodzi niezerowo i **odrzuca ciało**, a
`triggers.rs:1281` mapuje to na `CurlFailed`, który ten plan klasyfikuje jako chwilowy.
`TriggerError::Api` wymaga odpowiedzi **200 z niepustym `errors`**. Jeśli więc Linear odrzuca
zły klucz kodem 4xx, produkcja nigdy nie zobaczy `Api` i dalej będzie pukać bez końca —
mechanizm zielony w testach, martwy w produkcie. Planista biegu nazwał to sam jako
`POZA ZAKRESEM`, zamiast po cichu przepuścić. Uczciwe rozstrzygnięcie wymaga dwóch rzeczy:
zmierzenia prawdziwej odpowiedzi Lineara na odrzucony klucz i przeniesienia kodu HTTP przez
`%{http_code}`. Samo `fail-with-body` nie wystarczy — zamieniłoby 5xx z ciałem GraphQL
w fałszywą kwarantannę. Pomiar wymaga żywego klucza, więc to decyzja właściciela, tym
bardziej że klucz z planu fazy 8 jest tym przeznaczonym do rotacji.

**Koszt: zmierzony, nie przeczuwany.** $25,19 przekroczyło próg $25 z karty orchestratora, więc
sprawdziłem, czy to pętla: **142 wywołania narzędzi** (71 Bash, 44 Edit, 25 Read, 2 Write),
najczęściej powtórzona komenda to 7× bezczynne `cd`, a polecenia testowe poszły po 3 razy —
czerwień, implementacja, zieleń. Pętli nie ma; pieniądze poszły w 88,3 mln tokenów **z cache**
przy 297 turach, czyli w czytanie 2252-linijkowego `triggers.rs` i ~900-linijkowego
`state/triggers.ts` przy `--effort max` na Opusie z 1M kontekstu. Dźwignia istnieje
(`LOADOUT_CLAUDE_EFFORT`, `LOADOUT_CLAUDE_MODEL` czytane przez `h.py` ze środowiska), ale
zjeżdżanie z `max` przy sześciu pozostałych zakresach — wszystkie niemechaniczne — kupuje
~$15 za ryzyko STOP-u kosztującego cały bieg.

**Decyzja o rozmiarze na dalej:** §6.1 planu idzie jako **dwa** biegi, nie jeden. Podstawa jest
teraz zmierzona: jeden mechanizm kosztował 142 wywołania i całą rundę, a §6.1 wymienia pięć
niezależnych zachowań przy sufcie trzech tur (`MAX_FIX_ROUNDS = 2`).

## 2026-08-28, 18:05 — dwie sesje pisały do jednego `main`; `task-T-105` cofnięty

Właściciel podał plan dokończenia produkcyjnego (osiem sekcji, siedem biegów fazy 8).
Pierwsza rzecz, którą plan zakładał — „nie ingerować w aktywny `run-0828-1343-d7v4`" — była
już nieaktualna: ten bieg wylądował jako `c04c559`, a po nim weszły `4945972` i `615338b`.
`.git/h/` było puste, czyli zero otwartych biegów.

**Prawdziwy problem był inny i niewidoczny z gita.** Nad tym samym `main`, w tym samym
katalogu roboczym, pracowała **druga sesja Claude Code** (`9334477f`, od ~11:03,
`--dangerously-skip-permissions`) własnym `triage.sh`: lista 29 gałęzi, `merge --no-ff` →
bramka → `git reset --hard HEAD~1` na czerwieni. Moja baza `ci.sh full` startowała na
`615338b` i skończyła mierząc `df8f104`.

Podpisy, po których to poznać — wszystkie w `ps`, żaden w `git status` (triage commituje
każdy krok, więc brud nigdy nie wychodzi na wierzch):

- **dwa procesy `tee /var/folders/.../loadout-ci.*`** — `ci.sh` tee'uje do tempa, więc dwa
  tee to dwa równoległe `ci.sh` w jednym repo;
- cel testowy z **0,35 s CPU przy 4 min elapsed** — zagłodzony, nie zawieszony; odróżnisz
  go po świeżym dziecku (`ps --ppid`) z elapsed 0–2 s;
- `git rev-parse HEAD` inne niż zapisane na starcie pomiaru.

Kosztowało to dwa spalone `ci.sh full` i zatruło bramkę tamtej sesji: jej werdykt dla
`task-T-73` zapadł przy `load 11` z moim cargo obok. Na polecenie właściciela („wywal tę
sesję i leć") ubiłem ją od dołu do góry — `ci.sh` → `triage.sh` → `claude`, osiem PID-ów,
każdy z dowodem ESRCH. Osierocony `npm run dev` na porcie 5273 też.

**Co triage zdążył zrobić, i co z tego zostaje:**

| Gałąź | Bramka | Decyzja |
|---|---|---|
| `wip-T-152-final` | KONFLIKT w `src-tauri/src/commands/run.rs` | nie ląduje — i nie musi, patrz niżej |
| `T-203` | zielona, 368 s | zostaje (`615338b`) |
| `task-T-73` | zielona, 894 s | zostaje (`e9c7b89`) — dwa pliki testowe strumienia |
| `task-T-105` | **nie zdążyła** | **cofnięte** revertem (`db79576`) |

`task-T-105` weszło jako `df8f104` (merge + amend zdejmujący `TASK.md`) i bramka nigdy się
nie uruchomiła. Kiedy ją uruchomiłem, **zawisła**: `src-tauri/tests/it/a_turned_down_lead_says_why.rs`
nie ma ani jednego deadline'u (`grep timeout|Duration|deadline` → pusto) i `await`-uje
atrapę `codex app-server`, która stała jako `/bin/sh` z `0:00.00` CPU. To ta sama klasa,
co „niedokończone zadanie tokio wiesza bramkę": bez warunku zakończenia w pierwszym
przyroście cel `it` nie kończy się nigdy, a `cargo test` nie ma jak zameldować czerwieni.

Cofnięcie musiało pójść revertem, bo `git reset --hard` jest zablokowany. Drzewo `db79576`
jest **bajt w bajt** równe `e9c7b89` (ten sam `tree 668820b`), czyli baza wróciła do stanu,
który przeszedł bramkę zielono. Konsekwencja do zapamiętania: git uznaje `task-T-105` za
wmergowaną na zawsze, więc ponowne lądowanie wymaga revertu revertu — a przed nim naprawy
tamtego testu, bo w obecnym kształcie **żadna** bramka repo się nie skończy.

**Pozostałe 26 gałęzi z listy triage nie są zlandowane** i nie ma planu ich landować hurtem.
Lista jest w `triage-order.txt` tamtej (już martwej) sesji; jeśli wróci, to po jednej,
z bramką, i bez drugiego pisarza na `main`.

**Dwie sekcje planu właściciela okazały się już zrobione, sprawdzone przeciwko kodowi:**

- **§4, ręczna korekta T-152 — nie ma czego naprawiać.** Unikalny commit `wip-T-152-final`
  (`f1514d9`, kanonizacja trwałego roota) jest w treści na `main`: `run.rs:955`
  `let parent = fs::canonicalize(parent)?` i `durable_run_location()` w 7511.
  `wip-T-152-review` jest 0 ahead. Plan chciał też „usunąć fałszywą obietnicę atomowego
  identity-check i unlink" — takiej obietnicy tam nie ma: `remove_owned_run_file` jest
  fail-closed na brak zapisanej tożsamości, niezgodność inode'a, brak dokładnych bajtów,
  zmianę bajtów i zmianę tożsamości przy unlinku, a komentarz **wprost** mówi, że POSIX nie
  daje compare-and-unlink dla nazwy i obcy proces może trafić między kontrolę i `unlinkat`.
  Scena „podmiana po publikacji, cleanup nie usuwa nowszej prawdy" jest przetestowana:
  `t152_prestart_transaction_rolls_back.rs`, test
  `a_replaced_git_identity_and_its_wip_are_never_removed_by_parent_cleanup`.
  Został jeden dług: ten cel jest **standalone**, nie modułem `tests/it/` (AGENTS.md §2a.1).

- **§5, ręczne dokończenie T-158 — nie kwalifikuje się jako małe ani odziedziczone.**
  `T-158-repair` jest 5 commitów ahead i **81 behind**. Z sześciu pozycji zakresu dowozi
  **jedną** (bounded rotacja logu), jej test to standalone target, a
  `src-tauri/src/logging.rs` **nie istnieje na `main`** — czyli te 1144 linie to nowy
  subsystem, nie wąska korekta. `grep -rn quarantine src-tauri/src` → zero trafień.
  Zgodnie z warunkiem §5.7 planu idzie to jako bieg, nie ręcznie.

**Gdzie naprawdę jest luka T-158, zmierzone.** Odmowa triggera żyje **wyłącznie w stanie
okna**: `refused` i `retryable` są syntetyzowane w `src/state/triggers.ts` z przechwyconego
błędu (`retryable: true` w pięciu miejscach), a drut `TriggerPoll` w `src/sections/triggers/io.ts`
ma cztery warianty — `busy | armed | pending | accepted` — i **nie ma `refused`**.
`poll_with` zwraca `Err(TriggerError::ConnectionRefused)` i nie tyka ledgera, więc następny
tick puka do Lineara tym samym kluczem, który został już odrzucony; przeładowanie okna gubi
ten stan w całości. To jest jeden spójny mechanizm z widocznym zdaniem — dobry rozmiar na
jeden bieg i uczciwie czerwony na starym kodzie.

**Ocena §6 planu: siedem zakresów jest za dużych na jeden bieg każdy.** Nowy harness ma
`MAX_FIX_ROUNDS = 2`, czyli trzy tury łącznie, a każda pozycja wymienia 4–6 niezależnych
zachowań (§6.3 siedem scen w jednym „odchudzonym" teście). Tnę przy pierwszym STOP-ie
zamiast puszczać drugą rundę tego samego promptu.

**Poprawka do `.claude/commands/build.md`, której nie naniosłem:** jego §4 każe puszczać
wachlarz sześciu biegów i podnosić `LOADOUT_CARGO_LOCK_WAIT=2400`. Ta zmienna **nie istnieje**
w nowym harnessie — muteks cargo zniknął przy przebudowie, więc dla Rusta obowiązuje
szeregowo (niezmiennik 26). Plan właściciela ma to poprawnie w §7.

## 2026-08-28, później — harness ścięty z 9323 linii do 861, na wzór Murmura

Polecenie właściciela: „lekki mały harness bez overheadu, zobacz jak w murmur przerobiłem".
Wzorzec: `~/Projects/meetnotes`, `.agents/h/` — 1523 linie razem z promptami, po przebudowie
z 38 226.

Wejście: `scripts/h run <id> --prompt "co ma powstać"`. Pętla:
`worktree → plan → implementacja → checki + weryfikacja → max 2 poprawki`.

**Co zniknęło i za ile:** `gate.py` 1072 · `ship.sh` 837 · `ci.sh` −639 · `checks/` −1425
(9 plików + MANIFEST) · `review.sh` 365 · `process-group*.sh` 348 · `integrate.sh` 179 ·
`verify.sh` 17 · guardy skasowanych checków 138. Razem **−5859 linii**, +1256.

**Co zastąpiło:** `harness/` — `h.py` 587, `checks.json` 89, trzy prompty po ~30 linii,
`README.md` 88, `guards.sh` 378, `trust-workspace.py` 105. Plus osiem **własnych** checków
w `checks/`, bo pilnują niezmienników produktu, nie ceremonii.

**Trzy rzeczy warte zapamiętania z tej przebudowy:**

1. **Checki są deklaratywne.** `checks.json` mapuje glob ścieżki na komendę i budżet.
   Check biegnie tylko wtedy, gdy jego ścieżki się zmieniły, i jest zawężany do zmienionych
   modułów. To zastąpiło poziomy `before/quick/task/full`, odkrywanie po prefiksie nazwy pliku
   i `MANIFEST`.
2. **Symlinku `target/` NIE skopiowałem z Murmura, mimo że tam jest lewarem nr 1.**
   Powód jest o poprawności i odtworzony właśnie w meetnotes: jeden `CARGO_TARGET_DIR` dla
   dwóch checkoutów o tej samej nazwie pakietu daje jeden odcisk metadanych, więc
   `build A; build B; build A` melduje A jako `Fresh`, choć rlib zbudowano ze źródeł B.
   Check osądza wtedy cudzy kod i świeci zielono. Plus zmierzone tu 2026-08-17: 24 worktree
   na jeden `target/` = 66 GB i 886 645 plików.
3. **Dwie klasy wad zniknęły przez konstrukcję, nie przez strażnika.** Prompty są plikami
   `.md`, więc bash ich nie interpoluje — `prompt_backticks` i `prompt_dollars` nie mają czego
   pilnować, a każda z tych klas kosztowała kiedyś bieg. Podobnie przypinanie skryptu:
   python czyta plik w całości, więc edycja w trakcie biegu nie przewraca procesu.

Z całej dawnej maszynerii dowodowej zostało **15 linii**: `PASS_COUNT` w `h.py`
(niezmiennik 19). To jednocześnie jest to, co czyni zawężanie checka bezpiecznym — filtr,
który nic nie dopasował, daje `0 passed` i pada.

Strażnicy: **8 wystrzeliło, 0 misfire'ów, 0 bez strażnika, 1 nieadekwatny z nazwanym powodem**
(`density`, manualny). Nowy `checks_are_declared` przejął rolę `MANIFEST`-u i w pierwszej
minucie życia złapał mój własny błąd po zmianie nazwy pliku.

## 2026-08-28 — stary harness usunięty, pętla przepisana na prompt

Decyzja właściciela po audycie: bieg trwał za długo, a narzut nie zwracał się w niczym, co
widać w produkcie. Zmierzone na 121 biegach z `runs/`:

- **4,0 wywołania modelu i 4–5 przebiegów bramki** na bieg, z tego **dwa razy `full`**;
- `verify.sh full` = **319 s**, z czego `full-test` **280 s (88%)**, `full-clippy` 38 s,
  a wszystkie czternaście tanich sprawdzeń razem **9,6 s** — czyli sprawdzenia nigdy nie
  były problemem czasowym, mimo intuicji, że są;
- **97 recenzji na 105** zwróciło uwagę, a warunek naprawy brzmiał „bramka czerwona LUB
  jest uwaga", więc runda „doradcza" odpaliła się w **98 biegach na 121 (81%)** i regularnie
  trwała dłużej niż implementacja (T-103: 2 min implementacji, 45 min naprawy);
- `tasks/*.md` to było **26 617 linii** kontraktów pisanych ręcznie **przed** biegiem.

Co się zmieniło:

| Było | Jest |
|---|---|
| `./ship-task.sh <ID>` + `tasks/<ID>.md` napisane przez człowieka | `./ship.sh "<prompt>"`; kontrakt pisze etap planu w worktree |
| faza kontraktowa + implementacja + recenzja + naprawa (4,0 wywołania) | plan + implementacja + naprawa z paragonu (~2,2 wywołania) |
| `verify.sh full` dwa razy na bieg (640 s) | `verify.sh task` — nowy poziom, **16 s** na trunku |
| recenzja w każdym biegu, jej uwaga odpalała naprawę | recenzja na `--review`, jako raport |
| zamrożenie kontraktu = porównanie z `tasks/<ID>.md` | porównanie z wersją z **commita planu** |
| pisarz **poza** polityką grup procesów | pisarz pod `harness/process-group.sh`, z dowodem ESRCH |

Usunięte: `ship-task.sh`, `repair.sh`, `scripts/build-loop.sh`, `harness/task-spine.py`,
cały katalog `tasks/` (179 plików). Historia zostaje w gicie.

Zamknięte tą zmianą: **Q-7** z `docs/HARNESS-QUEUE.md`. Przyczyną 462 celów testowych była
reguła kontraktu („globalnie unikalna ścieżka pliku na kryterium"), nie budżet — bramka
umiała czytać moduł jedynego celu `it` od 2026-08-17, brakowało reguły, która każe tak pisać.
`AGENTS.md` §2a mówi to teraz wprost, a etap planu dostaje tę instrukcję z pomiarem.

Do zrobienia raz, na spokojną głowę: przenieść 60 istniejących celów z `src-tauri/tests/*.rs`
do `src-tauri/tests/it/` jako moduły. Każdy przeniesiony cel to ~60 s mniej w `full-test`
przy każdym lądowaniu.

## 2026-08-27, 11:24 — T-149 w trunku; faza 7 domknięta produktowo

**T-149 · zielone / WYLĄDOWANE · 1 h 02 min 34 s od commita kontraktu do merge'u ·
$0,00 raportowanego kosztu.** Na jawne polecenie właściciela zadanie ominęło wyłącznie
`ship-task.sh`; zachowało osobny worktree, uczciwe czerwone `before`, pełne bramki, niezależną
recenzję, jedną rundę naprawy i pojedyncze `integrate.sh`. `before` wykonało 4/4 sprawdzenia
i padło na dokładnych sentinelach w 2,68 s, quick przeszło 17/17 w 8,15 s, a pełne bramki
gałęzi przed i po naprawie przeszły 19/19 w 61,64 s oraz 66,53 s.

Kod produkcyjny rozdziela teraz koszt używany przez scheduler od końcowego rachunku biegu:
`run.json.spent_usd` dolicza koszt udanej prywatnej refleksji, ale nie zmienia decyzji o
starcie kroków. Offline oracle dowodzi prawdziwego grafu, routingu obu vendorów, historii
pętli i refleksji; osobny test Stop uruchamia rzeczywistą grupę `/bin/sh` i wymaga produkcyjnej
ścieżki Stop, trwałego `death_proof` oraz czystej sondy ESRCH. Normalny target zakończył się
**4 passed / 2 ignored**.

Recenzent znalazł pięć rzeczywistych luk płatnego oracle: planowany identyfikator udawał start,
routing był czytany z zamiaru zamiast drivera, blokada nie dowodziła wyłączności, cleanup mógł
zamaskować żywy proces, a refleksja nie dowodziła odczytania konkretnego handoffu. Jedyna runda
naprawy zamknęła wszystkie pięć w `c6e82ce`; all-targets clippy miał zero ostrzeżeń. T-149
wylądowało pojedynczo jako **`e01be73`**. Bramki integracyjne przeszły 16/16 w 64,06 s przed
merge'em i 16/16 w 104,94 s po nim; `TASK.md` nie przeżył lądowania.

Dokładne płatne polecenie po lądowaniu odmówiło w 0,06 s, zanim dotknęło stanu gospodarza,
sieci albo vendora: wykryło aktywne zewnętrzne procesy Claude'a/Codeksa. Wynik to 0/2 testów
live i **$0,00 wydatku**, ale poprawna odmowa bezpieczeństwa, nie porażka produktu. Nie wolno
zabijać cudzych sesji ani osłabiać preflightu; oba kierunki live czekają na ciche okno bez
zewnętrznych procesów vendorów.

**Liczniki fazy 7:** 52 numery T-98…T-149; 19 lądowań; 31 zamknięć „stój i zgłoś" bez
lądowania; 2 historyczne kontrakty zastąpione przed uruchomieniem (T-107 i T-108); 40 rund
naprawczych (39 zachowanych artefaktów Harnessu + ręczna runda T-149). Widoczny koszt zadań
to co najmniej **$98,59**; większość tur Codeksa, recenzje i ręczne T-149 nie zapisały ceny,
więc nie jest to pełny rachunek. `docs/ARCHITECTURE.md` zostało uzgodnione z kodem po ostatnim
lądowaniu: argv, dwa różne sufity dowodu śmierci, siedem etykiet indeksu, pełne attachments,
dwa korzenie pamięci, prywatny stan Claude'a i miękki budżet przy równoległości.

## 2026-08-27, 05:38 — T-148 ZAMKNIĘTE 15/18; płatny oracle nie ruszył

**T-148 · czerwone / ZAMKNIĘTE, NIE LĄDOWAĆ · 21 min 12 s Harnessu · $0,00 raportowanego
kosztu.** Enforced `before` uruchomiło oba standalone targety i uczciwie padło na dokładnym
sentinelu `T-148 oracle not authored` w 0,86 s. Kontrakt testowy powstał jako `7a2f11b`, ale
implementer nie usunął sentineli. Pierwsza oraz końcowa pełna bramka miały **15/18**; czerwone
pozostały `full-test`, AC-1 i AC-2, wszystkie na tym samym niewdrożonym oracle.

Recenzent wskazał trzy luki, których nie wolno zaliczyć danymi skonstruowanymi przez sam test:
atrapa offline ignorowała wybranego vendora, `spent_usd` obejmuje kroki, lecz nie prywatną
refleksję, a liczniki Stop/death-proof nie były powiązane z prawdziwą ścieżką timeout/błędu.
Planner naprawy potwierdził te luki. Jedyna runda naprawcza odmówiła usunięcia sentineli i nie
zmieniła żadnego pliku; końcowa bramka pozostała czerwona, więc zgodnie z AGENTS.md nie ma
piątej tury.

Gałąź `task-T-148` i jej testowy commit **nie lądują**. Płatnego polecenia `--ignored` nie
uruchomiono: testy live nie są zaimplementowane ani w trunku. Paragony kontraktu/build pokazują
co najmniej 10 331 900 tokenów wejścia (10 013 696 z cache) i 35 153 wyjścia; review/repair
nie mają osobnych liczników. Dalszy uczciwy oracle wymaga świeżego kontraktu z nowymi,
globalnie unikalnymi targetami, vendor-aware fake driverem i realnym probe Stop. Właściciel
polecił kontynuować bez `ship-task.sh`; T-149 przejmuje ten zakres ręczną pętlą i jawnie
obejmuje `src-tauri/src/commands/run.rs`, aby końcowe `spent_usd` liczyło także udaną refleksję
bez zmiany rachunku schedulera.

## 2026-08-27, 05:16 — T-147 w trunku; startup reaper ma deterministyczny dowód

**T-147 · zielone / WYLĄDOWANE · 12 min 34 s Harnessu + 3 min 22 s lądowania ·
$0,00 raportowanego kosztu.** Enforced `before` wykonało oba standalone targety i padło na
runtime szkielecie w 0,31 s. Każdy spec został certyfikowany z ośmioma liniami asercji i
zachował pełny odcisk do końca. Wspólny neutralny rdzeń zatrzymuje się na `Refused`, wymaga
sondy także przy zerowym limicie oraz nie uznaje dostarczonego KILL za śmierć bez późniejszego
`NoSuchGroup`. Produkcyjny `reap_group` jest cienkim adapterem tego samego rdzenia; tylko ESRCH
mapuje się na brak grupy.

Pierwsza pełna bramka przeszła **18/18 w 68,33 s**, a recenzent Codeks 5.5 odpowiedział
`nothing to add`. `integrate.sh` wylądował wyłącznie `task-T-147` jako **`2b65b2e`**; pełne
bramki przed i po merge'u przeszły **16/16 w 59,21 s** oraz **16/16 w 114,86 s**. `TASK.md`
nie przeżył, `main` jest czysty.

Paragony kontraktu/build pokazują co najmniej 2 932 680 tokenów wejścia (2 785 024 z cache)
i 24 082 wyjścia; review nie ma osobnego licznika, Harness nie podał ceny dolarowej. Następne
i ostatnie jest T-148, a po jego lądowaniu osobny, jawnie uzbrojony płatny oracle.

## 2026-08-27, 04:57 — T-143 ZAMKNIĘTE na odcisku asercji; T-147 przejmuje dowód

**T-143 · czerwone / ZAMKNIĘTE, NIE LĄDOWAĆ · 12 min 28 s Harnessu · $0,00 raportowanego
kosztu.** Enforced `before` wykonało oba standalone targety i uczciwie padło na runtime
`todo!()` w 0,81 s. Implementacja wydzieliła jeden neutralny rdzeń, mapowała wyłącznie ESRCH
na brak grupy, zatrzymywała KILL po odmowie oraz wymagała sondy po dostarczonym KILL. Niezmieniony
prawdziwy target T-135 przeszedł 2/2 w 5,40 s.

Harness zatrzymał bieg jeszcze przed pierwszą pełną bramką i recenzją: implementacja
zmniejszyła oba nowe specy z 7 do 6 linii asercji. To poprawna odmowa ochrony oracle;
`task-T-143` nie ląduje. Paragony kontraktu/build pokazują co najmniej 2 403 049 tokenów
wejścia (2 243 072 z cache) i 27 124 wyjścia; brak ceny dolarowej.

**T-147** odtwarza ten sam dowód w nowych targetach, wymaga minimum 7/7 od certyfikacji i po
własnym czerwonym `before` może przejąć wyłącznie produkcyjne commity `277d0c9` i `64915e0`.
T-146 zostaje zamknięte bez uruchomienia, bo zależy od T-143. Świeże T-148 zależy od
wylądowanego T-147 i pozostaje ostatnim oracle. Kolejność: T-147 → T-148.

## 2026-08-27, 04:42 — T-145 w trunku; osobny commit usuwa jednosekundowy flake

**T-145 · zielone / WYLĄDOWANE · 38 min 18 s Harnessu + 10 min 26 s integracji i naprawy
infrastruktury · $0,00 raportowanego kosztu.** Enforced `before` wykonało oba standalone
targety i uczciwie padło na zachowaniu w 0,75 s. Pierwsza bramka przeszła **18/18 w 68,99 s**.
Recenzent wykazał realny brak `ready` w `paused` zarówno w SQL, jak i lustrzanym kolektorze
plikowym; jedyna naprawa domknęła oba wejścia i dodała kontrolę do AC-1. Bramki po repair
oraz końcowa przeszły odpowiednio **18/18 w 65,93 s** i **18/18 w 53,74 s**.

Pierwsze `integrate.sh` miało zielone **16/16 w 58,66 s** przed merge'em i wylądowało T-145
jako `d974613`, lecz post-merge ponownie ujawnił niezależny jednosekundowy timeout
`recovery_waits_for_the_slug_that_owns_an_active_ledger_temp`. Była to trzecia reprodukcja
tego samego flaka poza diffem recovery. Osobny test-only commit **`415f730`** zastąpił trzy
jednosekundowe deadline'y jednym udokumentowanym sufitem 10 s, bez `sleep`, zmian asercji i
bez kodu produktu; cały target miał 794 passed / 0 failed / 14 ignored w 20,39 s. Ponowne
`integrate.sh task-T-145` przeszło **16/16 w 79,56 s** i **16/16 w 54,18 s**. `TASK.md` nie
przeżył, `main` jest czysty.

Paragony kontraktu/build pokazują co najmniej 18 978 210 tokenów wejścia (18 529 792 z cache)
i 65 202 wyjścia; review/repair nie mają osobnych liczników. Harness nie podał ceny dolarowej.
Następne jest T-143. T-142 zostaje zamknięte bez uruchomienia: wymaga nieistniejącego
lądowania T-141 i twardego sufitu pojedynczej tury Codeksa, którego test-only OWNS nie może
wdrożyć. Świeże T-146 po T-143 zachowuje pełny oracle i uczciwie nazywa 8 USD miękkim limitem
schedulera; pozostaje ostatnim zadaniem fazy.

## 2026-08-27, 03:44 — T-144 ZAMKNIĘTE 17/18; T-145 zachowa stare regresje

**T-144 · czerwone / ZAMKNIĘTE, NIE LĄDOWAĆ · 40 min 58 s Harnessu · $0,00 raportowanego
kosztu.** Enforced `before` uruchomiło oba nowe targety i uczciwie padło na zachowaniu w
0,56 s. Implementacja domknęła semantykę T-141, a recenzent wskazał dwie luki wyroczni:
Codeks był sądzony przez helper zamiast pełnego `exec_argv`, a zakaz martwych pól planu
sprawdzał tylko najwyższy poziom JSON. Jedyna naprawa domknęła obie uwagi i zaktualizowała
stare moduły recovery; pełny clippy oraz oba AC były zielone.

Końcowa bramka pozostała **17/18 w 15,71 s** na niezależnym teście
`recovery_waits_for_the_slug_that_owns_an_active_ledger_temp`, który skończył się
`RecvError`/timeoutem. Nie uznajemy deklaracji wykonawcy o ograniczeniu środowiska za dowód:
paragon pokazuje pojedynczy timeout. Harness słusznie odmówił również dlatego, że naprawa
zmniejszyła liczbę linii asercji w trzech istniejących specach: 16→10, 13→10 i 10→9.
Gałąź `task-T-144` nie ląduje.

Paragony kontraktu/build pokazują co najmniej 8 227 627 tokenów wejścia (7 922 176 z cache)
i 38 137 wyjścia; review/repair nie mają osobnych liczników. **T-145** przejmuje wyłącznie
produkcyjne commity `d8e5ca4` i `7fef6fc` po własnym czerwonym `before`, odtwarza oba nowe
oracle i zachowuje certyfikowane minima wszystkich trzech starych speców. Nie przejmuje
testowych commitów T-144 ani nie maskuje niezależnego timeoutu. Następna kolejność:
T-145 → T-143 → T-142.

## 2026-08-27, 03:01 — T-141 ZAMKNIĘTE 17/18; świeży T-144 przejmuje recovery

**T-141 · czerwone / ZAMKNIĘTE, NIE LĄDOWAĆ · 31 min 32 s Harnessu · $0,00 raportowanego
kosztu.** Enforced `before` wykonało oba targety i uczciwie padło na zachowaniu w 0,35 s.
Pierwsza bramka miała zielone AC, ale `full-clippy` oraz kompilacja pełnej suity ujawniły
zbędny raw-string hash i nieaktualne stare fixture'y recovery.

Recenzent znalazł dwa realne defekty: run był oznaczany przed potwierdzeniem przerwanego kroku,
a stary boot nadal wymagał używalnego PGID mimo braku prawa do reap. Wykazał też, że AC-2 nie
sądziło komentarza transportu resume. Jedyna naprawa domknęła wszystkie trzy punkty i stare
testy; końcowy `full-test` oraz oba AC były zielone. Pozostał jednak drugi deterministyczny
`needless_raw_string_hashes` w `t141_recovery_only_cleans_and_marks.rs`: końcowa bramka była
**17/18 w 55,95 s**. Nie ma piątej tury i gałąź nie ląduje.

Paragony kontraktu/build pokazują co najmniej 6 481 867 tokenów wejścia (6 129 152 z cache)
i 34 805 wyjścia; review/repair nie mają osobnych liczników. **T-144** ma dwa nowe, globalnie
unikalne targety, obejmuje oba defekty recenzenta i po własnym czerwonym `before` może przejąć
wyłącznie `bd5e42b`, `9ec7025`, `7ad1b70`, `36507aa`. Nie przejmuje speców, `TASK.md`, całej
gałęzi ani mieszanego commita `d5f2797`. Następna kolejność: T-144 → T-143 → T-142.

## 2026-08-27, 02:28 — T-140 w trunku; nowe indeksy mają tylko cztery żywe tabele

**T-140 · zielone / WYLĄDOWANE · 17 min 38 s Harnessu + 2 min 32 s lądowania ·
$0,00 raportowanego kosztu.** Enforced `before` uruchomiło oba standalone targety i padło
na zachowaniu w 0,48 s. Świeży oraz odbudowany `loadout.db` zawiera teraz wyłącznie `runs`,
`steps`, `events` i `artifacts`; plikowy zapis notatki nie tworzy bazy ani cienia treści, a
nagłówek `memory::notes` mówi prawdę o obu korzeniach plikowych.

Zgodnie z niezmiennikiem 25 implementacja nie zawiera `DROP` ani przepisywania: historyczna
tabela w istniejącym indeksie pozostaje bajtowo nietknięta do skasowania odtwarzalnego DB.
Nowy target niezależnie dowodzi, że po odbudowie fakty biegu wracają, a martwa tabela nie.
Pierwsza bramka przeszła **18/18 w 66,78 s**, recenzent Codeks 5.5 odpowiedział `nothing to
add`. `integrate.sh` wylądował tylko `task-T-140` jako **`d43182c`**; pełne bramki przed i po
merge'u przeszły **16/16 w 58,78 s** i **16/16 w 88,35 s**. `TASK.md` nie przeżył, `main`
jest czysty.

Paragony pokazują co najmniej 6 828 843 tokeny wejścia (6 524 160 z cache) i 30 288 wyjścia;
review nie zapisał osobnego licznika, więc to dolna granica. Następne jest T-141.

## 2026-08-27, 02:00 — cztery świeże kontrakty domykają H21, H22 i oracle fazy

Równoległe, wyłącznie odczytowe audyty przygotowały cztery standalone zadania. **T-140**
usuwa martwą tabelę `memory` ze świeżego i odbudowanego indeksu oraz prostuje nagłówek
notatek. Starego indeksu nie migruje destrukcyjnie: niezmiennik 25 w `AGENTS.md` wprost
zakazuje `DROP` i przepisywania wierszy, więc wcześniejsze żądanie T-108 „migracja ją zdejmuje"
było niewykonalne pod aktualnym kontraktem repo. Stara tabela jest tolerowana bez pisarza i
czytelnika do skasowania odtwarzalnego `loadout.db`; pliki pozostają prawdą.

**T-141** usuwa `RecoveryPlan.ask` i martwą zależność decyzji od sesji/próby. Nie usuwa
`RunSpec.resume`: aktualny trunk ma 49 wystąpień pola w 47 plikach, pięć produkcyjnych konstrukcji,
a oba adaptery naprawdę czytają pole. Kontrakt zapisuje więc prawdziwą granicę: recovery nie
wznawia, ale jawny wołający nadal może uruchomić istniejącą sesję.

**T-143** zamyka dwie luki recenzji T-135 przez jeden produkcyjny rdzeń z kontrolowanym
signalerem: nie-ESRCH/EPERM nie prowadzi do KILL, a `Dead` po KILL wymaga wewnętrznej sondy
ESRCH. **T-142** jest świeżym następcą zamkniętego T-107: offline kompiluje i sądzi wspólny
fixture, dwa testy live pozostają `#[ignore]` i wymagają jawnego
`LOADOUT_PAID_ORACLE=phase7`. Po lądowaniu oba kierunki vendorów biegną szeregowo, z sufitem
8 USD na bieg. Kolejność jest celowo **T-140 → T-141 → T-143 → T-142**, więc oracle pozostaje
ostatni; ze względu na zakaz dwóch ciężkich Cargo zadania nie biegną równolegle.

## 2026-08-27, 01:52 — T-135 w trunku; startup cleanup eskaluje i pamięta survivora

**T-135 · zielone / WYLĄDOWANE · 27 min 10 s Harnessu + 2 min 22 s lądowania ·
$0,00 raportowanego kosztu.** Enforced `before` uruchomiło oba standalone targety i padło
na brakującym zachowaniu w 0,85 s. Produkcja prowadzi osieroconą grupę przez TERM, stałą
łaskę i KILL, a `StillAlive` zapisuje do właściwego kroku trwałe, angielskie zdanie z PID i
PGID. Jedyna runda naprawcza dopisała ważny przypadek: survivor warning bezwarunkowo zastępuje
stary błąd kroku zamiast znikać pod nim (`86d3a4b`).

Pierwsza hostowa bramka była 17/18 przez E2E, w którym aplikacja nie osiągnęła
`main[data-section]`. Bramka po naprawie była 17/18 na innym, niezwiązanym teście
`recovery_waits_for_the_slug_that_owns_an_active_ledger_temp` (`RecvError`/timeout). Końcowa
bramka powtórzyła całość i przeszła **18/18 w 69,40 s**. `integrate.sh` wylądował wyłącznie
`task-T-135` jako merge **`fc09cc8`**; pełne bramki przed i po merge'u przeszły **16/16 w
46,19 s** oraz **16/16 w 89,73 s**. `TASK.md` nie przeżył, `main` jest czysty.

Recenzent Codeks 5.5 ujawnił dwie nierozstrzygnięte luki dowodu AC-1. Prawdziwe fixture'y
nie wymuszają nie-ESRCH/EPERM, a zewnętrzny probe po powrocie nie dowodzi, że sama produkcja
czekała na ESRCH po KILL. Odczyt kodu potwierdził obie właściwości implementacji, lecz obecny
target da się przejść leniwą wersją zwracającą `Dead` zaraz po KILL. **Nie uznajemy tych dwóch
własności za dowiedzione**; świeży standalone następca ma je zamknąć przed końcowym oracle.

Paragony kontraktu i pierwszej implementacji pokazują co najmniej 7 515 132 tokeny wejścia
(7 236 096 z cache) oraz 37 587 wyjścia. Review i repair nie zapisały osobnych liczników,
więc to dolna granica; Harness nie podał ceny dolarowej. Następne są rozdzielone kontrakty
H21/H22, a po nich świeży następca T-107 uruchamiany jako ostatni.

## 2026-08-27, 01:20 — T-134 w trunku, Live Stop ma uczciwy sufit

**T-134 · zielone / WYLĄDOWANE · 34 min 53 s Harnessu + 2 min 15 s lądowania ·
$0,00 raportowanego kosztu.** Enforced `before` uruchomiło dwa testy, z których uporczywy
`GroupProof::Alive` przekraczał 15-sekundowy limit po piętnastu próbach, więc czerwień była
brakiem zachowania. Implementacja ogranicza żywy Stop do trzech pełnych prób, kończy krok
jako `failed`, zachowuje PID/PGID oraz pozwala temu samemu `AppState` przyjąć i rzeczywiście
ukończyć drugi Start. Kontrola `Dead` nadal kończy krok jako `cancelled` z dowodem.

Pierwszy przebieg zatrzymała mechaniczna ochrona asercji: pełny clippy wymusił zamianę
testowego `expect()` na warunkowe `return Err(...)`, którego odcisk nie liczył. Osobny commit
Harnessu **`d3b96f5`** dodał tę rustową ścieżkę błędu do odcisku i wykonywany selftest; drugi
Codeks dał `verdict: none`. To naprawa Harnessu, nie rozluźnienie T-134: ubytek `expect`,
ubytek fail-path oraz skasowanie pliku nadal są czerwone.

Hostowa bramka po wznowieniu przeszła **17/17 w 59,20 s**. Recenzent znalazł jedną zasadną
uwagę medium: test dopuszczał brak `death_proof`, choć kontrakt wymagał jawnego `false`.
Jedyna runda naprawcza usunęła `skip_serializing_if` i wzmocniła asercję; jej bramka oraz
końcowa bramka przeszły odpowiednio **17/17 w 54,61 s** i **17/17 w 46,76 s**.
`integrate.sh` wylądował tylko `task-T-134` jako merge **`13d49fc`**; pełne bramki przed i po
merge'u przeszły **16/16 w 44,40 s** oraz **16/16 w 80,31 s**. `TASK.md` nie przeżył, `main`
jest czysty.

Zachowane paragony pokazują co najmniej 9 338 788 tokenów wejścia (9 014 016 z cache) i
42 097 wyjścia. Review i repair nie zapisały osobnych liczników, więc jest to dolna granica;
Harness nie podał ceny dolarowej. Następne jest T-135, wyłącznie startup cleanup.

## 2026-08-27, 00:40 — świeże kontrakty T-134 i T-135 zastępują T-106

Na jawne polecenie właściciela powstały dwa rozdzielone, standalone zadania. **T-134** sądzi
wyłącznie żywy Stop: trzy nieudowodnione próby `cancel`, krok `failed` z zachowanym PID/PGID,
ten sam błąd na drucie historii oraz faktycznie przyjęty i ukończony drugi Start na tym samym
`AppState`. **T-135** idzie dopiero po jego lądowaniu i sądzi startup cleanup: prawdziwe grupy
TERM → łaska → KILL → ESRCH oraz osobne, trwałe zdanie dla procesu, który nadal żyje.

Stare T-106 pozostaje zamkniętym kontraktem historycznym: łączyło obie domeny, filtrowało
funkcje wspólnego `tests/it` i wskazywało nieaktualny szew zapisu. Nowe targety mają globalnie
unikalne ścieżki, a `before` pada na zachowaniu bez brakujących symboli. Kolejność to T-134,
po zielonym pojedyncze `integrate.sh`, potem T-135; oba biegi używają właścicielsko
zatwierdzonej pary Codex + Codex.

## 2026-08-27, 00:32 — T-133 w trunku, próba IO refleksji jest obserwowalna

**T-133 · zielone / WYLĄDOWANE · 24 min 38 s Harnessu + 2 min 23 s lądowania · $0,00
raportowanego kosztu.** Enforced `before` uruchomiło dwa testy: kontrolny przeszedł, a drugi
padł na brakującym `reflection.discardedAgain`, więc czerwień była zachowaniem, nie brakiem
targetu albo kompilacji. Następnie zadanie selektywnie przejęło cztery dozwolone commity
T-132, bez jego kontraktu, `TASK.md` ani całej gałęzi.

Nowy target wykazał drugą, wcześniej niewidoczną granicę: warning niezależnego błędu IO
znikał pod równoległymi dispatcherami przez współdzielony cache callsite'u, chociaż pojedynczy
target przechodził. Dwa kolejne `quick` były czerwone; dopiero produkcyjna emisja do aktywnego
dispatchera zachowała tę samą treść, UUID biegu i błąd oraz dała deterministyczne **15/15**.
Nie serializowano testu i nie rozluźniono asercji. Commit implementacji to `8d71c2d`.

Pierwsza pełna bramka hosta nie osądziła kodu, bo dysk miał 184 MiB wolnego i zakończyła się
`No space left on device` przed recenzją. `cargo clean` usunął wyłącznie odtwarzalne `target/`
pięciu starych worktree i odzyskał 28 GiB; źródła, gałęzie i paragony pozostały. Wznowiony
Harness przeszedł **17/17 w 54,43 s**, a recenzent Codeks 5.5 odpowiedział `nothing to add`.
`integrate.sh` wylądował tylko `task-T-133` jako merge **`dc8df68`**, usunął branchowy
`TASK.md`; pełne bramki przed i po merge'u przeszły odpowiednio **16/16 w 43,16 s** oraz
**16/16 w 93,97 s**. `main` jest czysty.

Cztery zachowane paragony Codeksa pokazują co najmniej 8 268 763 tokeny wejścia (7 857 024
z cache) i 44 494 wyjścia. Pierwsza tura implementacyjna straciła terminalny paragon przy
wyczerpaniu dysku, więc są to liczniki dolne, nie pełny rachunek; Harness nie podał ceny
dolarowej. Następne są świeże, rozdzielone zadania Stop/startup — stary T-106 nie będzie
wznawiany.

## 2026-08-27, 00:05 — T-133 przejmuje receipt z obserwowalną próbą IO

Na właścicielskie polecenie dalszej jazdy powstało **T-133**, pełny następca zamkniętego
T-132. Ma świeży, globalnie unikalny target uruchamiający prawdziwy workflow i refleksję z
trzema kandydatkami: udanym zapisem, dokładnym tombstonem oraz niezależnym błędem IO. Lokalny
subscriber `tracing` ma zobaczyć dokładnie jeden produkcyjny warning z UUID faktycznego biegu;
równocześnie receipt musi mieć `kept == 1` i `discardedAgain == 1`. Sam preutworzony katalog,
mtime ani stan fixture nie są dowodem próby.

Dopiero po własnym enforced `before` T-133 może selektywnie zastosować trzy commity
produkcyjne T-132 oraz jego trzy pełne regresje: `5932154`, `ab71cfc`, `635d6f1` i `48a7fed`.
Nie przejmuje kontraktu, `TASK.md` ani całej gałęzi. Następne jedyne wejście to
`./ship-task.sh T-133 --agent codex --reviewer codex`.

## 2026-08-26, 23:57 — T-132 WSTRZYMANE mimo zielonej bramki: nieudowodniona próba IO

**T-132 · zielona bramka / WSTRZYMANE / NIEWYLĄDOWANE · 27 min 14 s · $0,00
raportowanego kosztu.** Enforced `before` certyfikowało trzy prawdziwe czerwienie w 3,13 s.
Mocniejszy fake naprawdę wywołuje `AgentDriver::start -> Err`, wiąże każdą próbę z fizycznym
UUID i odróżnia ją od porażki po uchwycie oraz kroku pominiętego przez graf. Prawdziwy E2E
wpisuje `/history`, naciska Enter, klika wyrenderowany wiersz i czyta zamrożony receipt spod
właściwego kroku. Pierwsza hostowa bramka przeszła **19/19 w 52,78 s**.

Recenzent znalazł jednak jedną zasadną lukę medium w AC-1. Fixture z góry tworzy katalog pod
ścieżką trzeciej kandydatki refleksji, a końcowa asercja sprawdza tylko, że katalog istnieje.
Implementacja, która przetworzyłaby jedynie dwie pierwsze kandydatki, nadal mogłaby zachować
jedną notatkę, policzyć tombstone i przejść bez dowodu, że niezależna gałąź błędu IO została
w ogóle wywołana. Plan naprawy poprawnie uznał to za wadę oracle, ale wykonawca nie zmienił
testu ani produkcji. Bramka po rundzie naprawczej przeszła **19/19 w 46,68 s**, a końcowa
**19/19 w 45,17 s**; zielony wynik nie odpowiada więc na uwagę recenzenta. `task-T-132`
pozostaje czystym dowodem na `48a7fed` i nie wolno jej lądować.

Pięć tur Codeksa zużyło 12 905 584 tokeny wejścia (12 281 728 z cache) i 59 275 wyjścia;
Harness nie podał ceny dolarowej. Uczciwa kontynuacja wymaga świeżego, globalnie unikalnego
targetu, który obserwuje faktyczną próbę trzeciego zapisu IO, a nie tylko stan fixture. Dopiero
po własnym czerwonym `before` następca może selektywnie przejąć commity T-132; zielone testy
T-132 ani cała gałąź nie są zgodą na lądowanie.

## 2026-08-26, 23:29 — T-132 przejmuje receipt z mocnym oracle

Na jawne właścicielskie polecenie kontynuacji z godmode powstało **T-132**, pełny następca
zamkniętego T-130. Ma trzy nowe, globalnie unikalne targety. Rustowy workflow wymusza osobno
udaną turę, porażkę po uchwycie, kontrolowane `AgentDriver::start -> Err` i krok pominięty
przez graf; UUID dwóch ostatnich nie może pojawić się w `recipients` ani `leftOutFor`.
Historyczny E2E używa niezmienionego `e2e/harness.ts`, wpisuje `/history`, naciska Enter i
klika prawdziwy `data-history-row`; bezpośrednie akcje store i handlery są zakazane.

Dopiero po własnym enforced `before` T-132 może selektywnie zastosować trzy commity
produkcyjne T-130: `442ce94`, `72dec4c` i `6663e2b`. Nie przejmuje słabego kontraktu
`674b9e9`, starych targetów ani całej gałęzi. Następne jedyne wejście to
`./ship-task.sh T-132 --agent codex --reviewer codex`.

## 2026-08-26, 23:23 — T-130 WSTRZYMANE mimo zielonej bramki: dwa słabe oracle

**T-130 · zielona bramka / WSTRZYMANE / NIEWYLĄDOWANE · 41 min 45 s · $0,00
raportowanego kosztu.** Enforced `before` certyfikowało trzy prawdziwe czerwienie w 1,42 s.
Implementacja zapisała rzeczywistych odbiorców dopiero po `AgentDriver::start`, prowadzi
zamrożony receipt przez `run.json` i odczyt historii, pokazuje go pod krokiem oraz liczy
`discardedAgain`. Pierwsza bramka Harnessu przeszła **19/19 w 54,99 s**, a gałąź jest czysta
na `6663e2b`; trzy commity produkcyjne to `442ce94`, `72dec4c` i `6663e2b`.

Recenzent znalazł jednak dwie luki medium w nowych kryteriach. AC-1 nie sadzi
`AgentDriver::start -> Err`, więc nie chroni granicy przed przyszłym przesunięciem zapisu
odbiorcy przed uzyskaniem uchwytu. AC-3 otwiera historię bezpośrednimi akcjami store zamiast
wysłaniem formularza i kliknięciem wyrenderowanego `data-history-row`, więc martwa kontrolka
mogłaby przejść. Plan naprawy uznał oba problemy za wady oracle, a wykonawca nie zmienił ani
testów, ani produkcji. Bramka po naprawie przeszła **19/19 w 41,77 s**, końcowa **19/19 w
42,72 s**, ale Harness sam zaznaczył, że człowiek nadal musi rozstrzygnąć odpowiedź na review.
Dlatego `task-T-130` nie wolno lądować mimo kodu 0.

Pięć tur Codeksa zużyło 27 055 670 tokenów wejścia (26 399 360 z cache) i 83 524 wyjścia;
Harness nie podał ceny dolarowej. Uczciwa kontynuacja wymaga świeżego zadania z nowymi,
globalnie unikalnymi targetami: fake `start` zwracający kontrolowane `Err` oraz interakcja przez
prawdziwe kontrolki historii. Dopiero po własnym czerwonym `before` wolno selektywnie przejąć
trzy commity produkcyjne T-130; kontrakt, stare targety i cała gałąź nie są wejściem.

## 2026-08-26, 22:39 — T-131 w trunku, prawdziwe nazwy zachowane

**T-131 · zielone / WYLĄDOWANE · 27 min 12 s Harnessu + 2 min 9 s lądowania · $0,00
raportowanego kosztu.** Enforced `before` certyfikowało trzy prawdziwe czerwienie w 1,57 s.
Implementacja zachowała identyczny literal UUID jako legalną nazwę projektu w polu `project`
i jako identyfikator biegu wyłącznie w polu `from`; nie przejęła heurystyki ani commita
`6b8ad1d` z zamkniętego T-129. Katalog pokazuje bieżący `leftOut`, prawdziwy zasięg i typowane
pochodzenie, a panel agenta nie nazywa pominiętej notatki wiedzą, którą dostał.

Pierwsza bramka była zielona **19/19 w 54,68 s**. Recenzent znalazł zasadny brak zgodności
filtra UI z rustową macierzą `place × scope`; jedyna runda naprawcza dodała tę samą granicę i
dwa przypadki regresyjne w `286dd20`. Jeden istniejący test recovery raz zakończył się
`RecvError`/timeoutem, po czym przewidziana grafem końcowa bramka przeszła **19/19 w 43,80 s**.
`integrate.sh` wylądował wyłącznie `task-T-131` jako `4189789`, usunął branchowy `TASK.md`, a
pełna bramka po merge'u przeszła **16/16 w 76,75 s**. Pięć tur Codeksa zużyło łącznie
12 683 244 tokeny wejścia (12 185 344 z cache) i 55 046 wyjścia; Harness nie podał ceny
dolarowej. T-130 jest następne po doprecyzowaniu własnego oracle historii.

## 2026-08-26, 22:01 — T-129 zamknięte, Harness naprawiony, T-131 gotowe

**T-129 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 28 min 30 s · $0,00 raportowanego
kosztu.** Właściciel przyjął rekomendację zachowania prawdziwych nazw projektów. Gałąź
`task-T-129` pozostaje wyłącznie dowodem; nie wolno lądować ani wznawiać jej oraz commita
`6b8ad1d`, który maskuje legalną nazwę projektu heurystyką UUID. Trzy wcześniejsze commity
produkcyjne są dopuszczone do selektywnego użycia dopiero po uczciwym `before` następcy.

Osobny commit `ba3d8be` naprawia defekt Harnessu ujawniony przez przerwanie T-129.
`review.sh` i `repair.sh` używają jednej polityki aktywnego PGID: SIGTERM → łaska → SIGKILL →
dowód ESRCH. Selftest na atrapach ignorujących TERM przeszedł trzy scenariusze: interrupt
naprawy, interrupt recenzji oraz timeout; w każdym proces był martwy i nie pisał po wyjściu.

**T-131 zakontraktowane.** Trzy nowe, globalnie unikalne targety zachowują cały prawdziwy
zakres T-129 i dodają rozstrzygający przypadek: identyczny UUID jest legalną nazwą projektu
oraz identyfikatorem biegu, a znaczenie wynika wyłącznie z pól `project`/`from`. T-130 zostało
przepięte na zielone lądowanie T-131 i nadal nie ruszyło. Następne jedyne wejście to
`./ship-task.sh T-131 --agent codex --reviewer codex`.

## 2026-08-26, 20:07 — T-129 WSTRZYMANE przed nieuczciwą naprawą

**T-129 · czerwone / WSTRZYMANE / NIEWYLĄDOWANE · 28 min 30 s do przerwania · $0,00
raportowanego kosztu.** Enforced `before` certyfikowało wszystkie trzy nowe targety w 1,34 s.
Implementacja rozdzieliła `project/from`, wylicza bieżące `leftOut`, pokazuje prawdziwy zasięg
i wyklucza pominięte notatki z bieżącego panelu agenta. Gałąź ma trzy commity produkcyjne
`dca0c89`, `7939635` i `d38cbde`; AC-1, AC-2, AC-3, Clippy, typy i wszystkie quick-checki
przeszły.

Pełna bramka implementera miała dwa razy 18/19 na niedostępnych w jego sandboxie testach
grup procesów i `kern.boottime`. Następna bramka Harnessu miała również 18/19, lecz z innym
podpisem poza `OWNS`: `copy-diagnostics-is-real` nie zobaczył nawet startowego
`main[data-section]`. Recenzent zgłosił niski brak fixture projektu o nazwie wyglądającej jak
UUID. Odczyt produkcji rozstrzyga tę niewiadomą: import używa surowego basename katalogu, więc
UUID jest legalną nazwą projektu. Plan naprawy mimo to polecił heurystykę zamieniającą taką
realną nazwę na `another project` i test wymuszający to zachowanie. Byłoby to zazielenienie
nadmiernie literalnego zdania kosztem kłamstwa w UI, więc orchestrator przerwał Harness kodem
3 przed zmianą produkcji.

Worktree `task-T-129` pozostaje dowodem. Mimo kodu 3 proces pisarza uruchomiony przez Harness
nie zszedł razem ze skryptem: dokończył w tle i zatwierdził nieuczciwy test oraz heurystykę UI
jako `6b8ad1d fix(memory): hide UUID-shaped imported project labels`. Gałąź jest czysta, ale
nie wolno jej resume'ować w ciemno ani lądować. To osobny defekt sterowania Harnessu: przerwany
bieg nie może zostawić żywego pisarza, który dalej zmienia repo. Dwie wcześniejsze zapisane
tury pisarza zużyły co najmniej 14,23 mln tokenów wejścia (13,84 mln z cache) i 48,1 tys.
wyjścia; recenzja i naprawa nie mają wspólnego paragonu tokenów. Potrzebna jest decyzja
właściciela: zachować prawdziwe nazwy projektów i zamknąć T-129 na rzecz świeżego następcy z
poprawnym zdaniem, albo jawnie przyjąć maskowanie projektów o nazwie UUID. Rekomendowany jest
pierwszy wariant. T-130 nie ruszyło.

## 2026-08-26, 19:35 — kontrakty T-129 i T-130 gotowe

Na jawne polecenie właściciela powstały dwa świeże kontrakty po T-139, bez kodu
produkcyjnego i bez uruchamiania bramki. **T-129** ma trzy globalnie unikalne targety i
rozstrzyga wyłącznie bieżący katalog: `Block::dropped`, prawdziwy zasięg, rozdzielone
pochodzenie projektu/biegu oraz widoczny ekran Memory i bieżącego agenta. Jego `OWNS`
obejmuje z góry pięć historycznych wyroczni, które przy zmianie lustra lub tekstu muszą
pozostać kompilowalne — to wprost eliminuje klasę zamknięcia T-128.

**T-130** ma trzy inne, globalnie unikalne targety i rusza dopiero po wylądowaniu T-129.
Zamraża pełny adres i pochodzenie, zapisuje `recipients` oraz `leftOutFor` po UUID dopiero po
udanym starcie procesu, prowadzi receipt przez `run.json` do historii i prawdziwego ekranu
oraz domyka licznik ponownie odrzuconych propozycji refleksji. Nie dodaje kopii do SQLite:
historia czyta plik biegu bezpośrednio, więc kolumna bez produkcyjnego czytelnika byłaby
martwym indeksem sprzecznym z niezmiennikiem 21. Operacyjnie oba biegi pozostają Codex +
Codex; następne wejście Harnessu to T-129.

## 2026-08-26, 19:04 — T-139 w trunku po usunięciu presji dysku

**T-139 · zielone / WYLĄDOWANE · 34 min 49 s Harnessu + 4 min 12 s lądowania · $0,00
raportowanego kosztu.** Właściciel jawnie wybrał `integrate.sh task-T-139`. Pierwsza próba
integracji poprawnie odmówiła przed merge'em: main miał czerwone `full-test`, a paragon nie
zachował powodu. W tym momencie wolne miejsce spadło z 2,5 GiB do 105 MiB. Zamknięty worktree
T-136 trzymał 72 GiB regenerowalnego `target/`; `cargo clean` usunął wyłącznie ten cache,
pozostawiając gałąź, źródła i paragony nietknięte, i odzyskał 25 GiB.

Po usunięciu konkretnego blockera środowiskowego ponowiona operacja lądowania przeszła pełną
bramkę main **16/16 w 91,31 s**, wylądowała wyłącznie `task-T-139` jako merge
**`0fb49a4`**, usunęła branchowy `TASK.md`, a pełna bramka po merge'u przeszła **16/16 w
103,27 s**. Main jest czysty i nie zawiera `TASK.md`; H16/H18 są domknięte, następne jest
T-129, potem T-130.

Odczytowy audyt potwierdził znany flake starego testu trigger recovery: sekundowy watchdog
może podczas przeciążenia wywołać wtórny `RecvError`; identyczny podpis wystąpił przy T-101 i
zniknął bez zmiany kodu. Nie zmieniono testu ani kryterium przy lądowaniu T-139. Jeśli podpis
wróci przy normalnej przestrzeni dyskowej, wymaga osobnego zadania stabilizującego domeny
zamków triggerów, nie rozszerzenia OWNS kolejnego zadania pamięci.

## 2026-08-26, 17:55 — T-139 ZAMKNIĘTE: finalny timeout spoza OWNS po zielonej naprawie

**T-139 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 34 min 49 s Harnessu · $0,00
raportowanego kosztu.** Enforced `before` certyfikowało 3/3 prawdziwe czerwienie w 6,53 s.
Pierwsza pełna bramka była zielona 19/19 w 51,48 s. Recenzent znalazł realny błąd: po
przełączeniu B → C nadal widoczny wiersz B brał globalny folder C. Jedyna runda naprawcza
związała cztery mutacje z `notesFolder`, dodała opóźnioną odpowiedź C i commit
`c3e2096 fix(memory): bind actions to rendered catalog`.

Ten commit przeszedł pełne 19/19 w 45,70 s podczas wykonania naprawy. Natychmiastowa końcowa
bramka miała jednak 18/19: tylko istniejący
`trigger_editor_writes_safe_file::recovery_waits_for_the_slug_that_owns_an_active_ledger_temp`
zakończył się `cleanup oracle releases fetch: RecvError` oraz `Error: Timeout`. Wszystkie AC,
Clippy, quick-checki i pozostała pełna suita były zielone. Wadliwy target leży poza `OWNS`
T-139, więc kontraktu nie rozszerzono, testu nie poprawiono i bramki nie ponowiono „żeby
sprawdzić”. Zgodnie z regułą wyniku spoza OWNS zadanie jest zamknięte, nie wylądowane.

Gałąź `task-T-139`, czysty worktree i `runs/T-139/gate-final.json` pozostają dowodem związanym
z commitem `c3e20969414199e8b8d3b806f9fe065eed2e4c73`. Zarejestrowane tury pisarza zużyły co
najmniej 14 548 218 tokenów wejścia (13 962 240 z cache) i 56 391 wyjścia; Harness nie podał
dolarowego kosztu Codeksa. Dalsze H16/H18 wymaga decyzji właściciela: jawnego lądowania przez
`integrate.sh task-T-139` z jego własną pełną bramką albo świeżego następcy z nowymi targetami.

## 2026-08-26, 17:08 — T-139 przejmuje działający kod z lądowalnym oracle

Właściciel polecił kontynuować po obowiązkowym postoju T-138. Świeże **T-139** startuje z
czystego `main`, ma trzy nowe standalone targety i od początku wymaga funkcji testowych do 90
linii. Dopiero po uczciwym `before` wolno mu zastosować siedem commitów produkcyjnych T-138;
kontrakt, targety, pusty commit, naprawa testu i cała gałąź nie są wejściem.

Pełne pokrycie T-138 pozostaje: realna refleksja z evidence, niekanoniczny snapshot, oba
korzenie, exact prefix, trwały Move i cztery kliknięcia pełnego adresu. Nowa obowiązkowa scena
sadzi tombstone wyłącznie w bibliotece i przez `record_project_candidate_from_run` dowodzi
odmowy automatycznego zapisu do projektu; prefix-extra jest kontrolą negatywną. Operacyjnie
bieg pozostaje Codex + Codex, a T-129/T-130 czekają na zielone lądowanie tego bloku.

## 2026-08-26, 16:56 — T-138 ZAMKNIĘTE: 18/19 po naprawie i niezałatana luka tombstone'a

**T-138 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 44 min 11 s Harnessu · $0,00
widoczne.** Dwie zapisane tury Codeksa zużyły 26,06 mln tokenów wejścia (25,54 mln z cache)
i 80,1 tys. wyjścia; recenzja, plan i wykonanie naprawy nie mają wspólnego paragonu tokenów.
Enforced `before` certyfikowało wszystkie trzy nowe targety w 7,06 s.

Implementacja przejęła pięć dozwolonych commitów T-137 i domknęła pełny workflow dwóch
korzeni, działającą refleksję z wrapperami i evidence, bajtowy snapshot, trwały Move oraz
adresowane akcje okna. Pierwsza pełna bramka miała **18/19** w 54,71 s: pełna suita i AC-1,
AC-2, AC-3 przeszły, a `full-clippy` znalazł identyczne ramiona `match` w nowym oracle Move.

Recenzent zgłosił jedną zasadną lukę medium. Oracle tombstone'a wołał drogę jednego korzenia,
więc nie dowodził, że dokładny tombstone w bibliotece blokuje automatyczny zapis do korzenia
projektu. Plan naprawy wskazał właściwą scenę przez `record_project_candidate_from_run`, lecz
wykonawca uznał zmianę oracle za wymagającą decyzji człowieka i jej nie wykonał. Połączył tylko
ramiona `match` w `b997767`; kolejna pełna bramka ujawniła drugi lint w tym samym nowym
targetcie: funkcja snapshot/reflection ma 131 linii przy limicie 100.

Końcowa bramka na czystym `b997767` ponownie miała **18/19** w 45,81 s. Wszystkie testy i
trzy AC są zielone; jedyną formalną czerwienią jest `full-clippy`, ale luka bibliotecznego
tombstone'a pozostaje prawdziwa niezależnie od wyniku bramki. Po jednej rundzie nie ma piątej
tury, ręcznego refaktoru ani lądowania.

Gałąź `task-T-138`, czysty worktree i `runs/T-138/` pozostają dowodem. Świeży następca musi
mieć trzy nowe globalnie unikalne targety, od początku rozbić długą funkcję oracle i dodać
wielokorzeniową scenę tombstone'a. Dopiero po własnym czerwonym `before` może jawnie przejąć
produkcyjne commity T-138: `705f433`, `7d9bbc9`, `124cc46`, `6642567`, `5ceea68`, `d439a25`
i `3dba18d`. Nie przenosi `f782ef9`, `330a49d`, `9e8ec91`, `b997767`, targetów T-138 ani
całej gałęzi.

## 2026-08-26, 14:18 — T-138 przejmuje H16/H18 bez powtarzania implementacji

Właściciel polecił przyspieszyć po obowiązkowym postoju T-137. Świeże **T-138** startuje z
czystego `main` i ma trzy nowe standalone targety. Dopiero po uczciwym `before` wolno mu
zastosować pięć commitów implementacyjnych T-137; jego kontrakt, specy i cała gałąź nie są
wejściem.

AC-1 prowadzi atrapę refleksji przez prawdziwe wrappery ustawień, evidence i budżetu, wymaga
receipt oraz fizycznej notatki tylko we właściwym projekcie, a stempel porównuje na
niekanonicznym front matterze bajt po bajcie po normalizacji wyłącznie `last_used_at`. AC-2
uznaje wpis Move dopiero po udanym delegowaniu rzeczywistej operacji i wymaga, żeby odmowa nie
udawała wykonanego fsync/publish/unlink. AC-3 przypina legacy do widocznej strefy
`earlier-project`, wyklucza je z `suggested` i zachowuje prawdziwe kliknięcia Move, Use, Stop
oraz Discard. Operacyjnie bieg pozostaje Codex + Codex; T-129/T-130 czekają na wylądowanie
pełnego adresu z T-138.

## 2026-08-26, 14:05 — T-137 ZAMKNIĘTE: AC-1 zatrzymała wadliwa atrapa refleksji

**T-137 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 55 min 36 s Harnessu · $0,00
widoczne.** Dwie zapisane tury Codeksa zużyły 27,12 mln tokenów wejścia (26,50 mln z cache)
i 92,2 tys. wyjścia; recenzja, plan i wykonanie naprawy nie mają wspólnego paragonu tokenów.
Enforced `before` certyfikowało trzy nowe standalone targety. Pierwsza bramka miała 17/19:
AC-2 i AC-3 przeszły, a `full-test` oraz AC-1 zatrzymały się na tym samym teście snapshotu.

Jedyna naprawa poprawiła prawdziwy defekt produkcji w `8345b02`: stempel `last_used_at`
zastępuje teraz wyłącznie tę linię w bajtach zamrożonego snapshotu, bez ponownego odczytu i bez
kanonizowania pozostałego front mattera. Końcowa bramka na czystym `8345b02` ponownie miała
**17/19** w 23,72 s. Pada jednak wcześniej niż dowód stempla: testowy `FakeDriver` zwraca
sterownik z `reflecting()`, ale nie implementuje wymaganych `with_settings`, `with_evidence`
i `with_budget`. Produkcyjny `reflection_driver` słusznie odmawia tak nieopakowanej atrapie,
więc notatka `T137-REFLECTION-A` nigdy nie powstaje i asercja katalogu projektu A jest czerwona.
To wada oracle do naprawienia w świeżym zadaniu, nie powód do zmiany polityki produktu.

Druga opinia pozostawiła jeszcze dwa zasadne braki dowodu. `RecordingMoveIo` zapisuje część
operacji przed delegowaniem, więc ślad może opisywać próbę zamiast wykonanego fsync/unlink.
Browserowy test znajduje legacy row globalnie, zamiast dowodzić, że stoi w strefie
`earlier-project` i nie stoi w `suggested`. Naprawa bajtów usuwa produkcyjną część trzeciej uwagi
o niekanonicznym front matterze, ale świeży oracle powinien użyć właśnie takiego fixture.

Gałąź `task-T-137`, czysty worktree na `8345b02` i `runs/T-137/` pozostają dowodem; nic nie
wylądowało. Po jednej rundzie nie ma piątej tury ani ręcznego łatania. Uczciwa kontynuacja to
świeży następca z globalnie unikalnymi targetami: po własnym czerwonym `before` może jawnie
przejąć wyłącznie commity implementacyjne T-137, lecz musi od zera postawić działający szew
refleksji, ślad Move rejestrowany dopiero po sukcesie oraz locator legacy przywiązany do
widocznej strefy.

## 2026-08-26, 12:52 — T-137 przejmuje H16/H18 z trzema obserwowalnymi wyroczniami

Właściciel polecił kontynuować po obowiązkowym postoju T-136. Świeże **T-137** startuje z
czystego `main` i ma trzy nowe standalone targety. Dopiero po uczciwym `before` wolno mu
wykorzystać trzy commity implementacyjne T-136; jego kontrakt, specy i jedyna naprawa nie są
wejściem.

AC-1 porównuje literalny multizbiór adresów, zmienia niesioną notatkę po przechwyceniu
`RunSpec.prompt`, lecz przed stemplem, i używa tombstone'a `similar-slug-extra__…` przeciw
`similar-slug`. AC-2 wykonuje ten sam rdzeń Move przez produkcyjny adapter oraz modelujące IO,
więc mierzy temp, zapis, oba fsync, no-clobber i unlink w kolejności zamiast ufać nazwom w
źródle. AC-3 zachowuje prawdziwe kliknięcia Move, Use, Stop i Discard oraz pełne katalogi po
każdej odpowiedzi. Operacyjnie bieg pozostaje Codex + Codex; T-129/T-130 czekają na
wylądowanie pełnego adresu z T-137.

## 2026-08-26, 11:44 — T-136 ZAMKNIĘTE: czerwona bramka i trzy luki wyroczni AC-1

**T-136 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 11 min 25 s Harnessu
(1 godz. 21 min 27 s od pierwszego startu wraz z odzyskiwaniem miejsca) · $0,00 widoczne.**
Dwie porównywalnie zapisane tury Codeksa zużyły co najmniej 27,36 mln tokenów wejścia
(26,85 mln z cache) i 80,3 tys. wyjścia; niedokończona naprawa kontraktu, recenzja, plan oraz
wykonanie naprawy nie mają wspólnego paragonu tokenów. Pierwszy start zatrzymał hostowy
`ENOSPC`; po usunięciu wyłącznie odbudowywalnych cache bieg wznowił ten sam certyfikowany
kontrakt i nie powtarzał `before`.

Implementacja w `8cca95a`, `eb09306` i `33d84f3` domknęła produkcyjny adres dwóch korzeni,
historyczne fixture oraz prawdziwe akcje okna. Pierwsza pełna bramka miała **15/18**:
AC-2 przeszło, AC-1 odrzuciło kolejność katalogu, a full Clippy znalazł podobne nazwy w nowym
teście. Druga opinia zgłosiła trzy zasadne luki AC-1: brak interleavingu zmieniającego notatkę
między przechwyceniem promptu a stemplem, brak obserwowalnego dowodu temp/no-clobber/fsync
w Move oraz tombstone, który nie rozróżnia dokładnego `<id>__` od dłuższego prefiksu.

Plan naprawy poprawnie rozpoznał, że kontrakt wymaga multizbioru, podczas gdy oracle
porównuje uporządkowany `Vec`; nie wolno naginać produkcyjnego sortowania do fixture.
Wykonawca zmienił jednak tylko pierwszą nazwę w `2c2b389`. Końcowa bramka na czystym drzewie
`f1b8d6b` ponownie miała **15/18** w 172,49 s: `full-clippy` znalazł nieużywane `&self`,
`full-test` oraz AC-1 nadal padały na katalogu, a samodzielny AC-1 raz ujawnił też niestabilny
scenariusz stempli. Po jednej rundzie nie ma piątej tury, ręcznego łatania ani lądowania.

Gałąź `task-T-136`, worktree i `runs/T-136/` pozostają dowodem. Świeży następca musi startować
z `main`, dostać nowe globalnie unikalne targety i dopiero po uczciwym `before` może jawnie
przejąć trzy commity implementacyjne T-136. Musi porównywać literalny multizbiór, usunąć lint
bez suppression oraz uczynić trzy uwagi recenzenta obserwowalnymi; samo zazielenienie obecnych
dwóch błędów byłoby słabsze od kontraktu i nie może być podstawą integracji.

## 2026-08-26, 10:18 — T-136 przejmuje pełny zakres zamkniętego T-128

Właściciel zatwierdził świeży kontrakt i przyszłe rutynowe decyzje wykonawcze fazy. **T-136**
startuje z czystego `main`, ma dwa nowe standalone targety oraz od początku posiada oba stare
pliki, które zamknęły T-128, i pozostałe fixture ujawnione przez pełną suitę. Po uczciwym
`before` wolno mu wykorzystać wyłącznie dwa produkcyjne commity T-128; stare commity
kontraktowe i testowe pozostają dowodem, nie wejściem.

Nowy Rust oracle rozróżnia stemple projektu B od stanu pozostawionego przez A i przypina
historyczne wyrocznie do właściwych fizycznych korzeni. Prawdziwy browserowy oracle klika
Move, Use, Stop oraz Discard i wymaga, żeby usunięcie projektowego duplikatu nie usunęło
bibliotecznej notatki o tym samym `id`. Operacyjnie bieg pozostaje Codex + Codex; następne
T-129/T-130 czekają na wylądowanie pełnego adresu z T-136.

## 2026-08-26, 09:28 — T-128 ZAMKNIĘTE: dwa stare testy są poza OWNS

**T-128 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 50 min 01 s Harnessu · $0,00
widoczne.** Enforced `before` certyfikowało oba standalone targety. Implementacja dwóch
korzeni, pełnego adresu `{ catalogFolder, place, id }`, Move, tombstone'ów i ochrony przed
spóźnioną odpowiedzią przeszła własne AC-1 i AC-2. Pierwsza pełna bramka miała 17/18:
789 testów projektu przeszło, pięć starych fixture nadal zakładało, że `this-project` leży
w bibliotece.

Recenzent znalazł jedną lukę medium i trzy niskie luki dowodu: E2E klikało Use, ale nie
Stop/Discard, projekt B nie dowodził własnych stempli, React key był sprawdzany po atrybucie,
a sekwencję trwałego Move dało się rozstrzygnąć dopiero inspekcją. Plan naprawy poprawnie
odmówił przywrócenia legacy do promptu, lecz wykonawca nie zmienił fixture i druga bramka
powtórzyła 17/18.

Po jawnej zgodzie właściciela test-only repair wzmocnił oba nowe oracle i przeniósł stare
fixture do korzenia projektu. `cargo check --all-targets --keep-going` przeszedł w 12,04 s,
ale pełna bramka mechanicznie odmówiła dwóm koniecznym plikom spoza `OWNS`:
`a_suggestion_can_be_discarded.rs` oraz `a_suggestion_needs_a_because.rs`; ujawniła też
kolejny historyczny adres w należącym do zadania `run_evidence_reaches_the_product.rs`.
To jest dokładny wynik z instrukcji fazy: nie rozszerzać OWNS, nie łatać kontraktu i zapisać
zadanie jako zamknięte. Gałąź `task-T-128` pozostaje niewylądowana; następca musi od początku
posiadać implementację dwóch korzeni oraz pełny zestaw starych wyroczni.

## 2026-08-26, 03:42 — T-104 ZAMKNIĘTE; T-128 przejmuje pełny adres dwóch korzeni

**T-104 · ZAMKNIĘTE / NIEURUCHOMIONE / NIEWYLĄDOWANE · $0,00.** Cztery rustowe `check:`
filtrują funkcje wspólnego targetu `tests/it`, więc nie mają globalnie unikalnej ścieżki i
mogą zazielenić pusty wybór. Kontrakt nie posiada też `AppState::project_for`, pełnego adresu
`{ place, id }`, konsumentów prawdziwego promptu ani regresji T-126. Uruchomienie go mimo
znanej wady byłoby spaleniem `before`, nie pomiarem zachowania.

Świeże **T-128** startuje z wylądowanego T-127 i ma dwa globalnie unikalne targety. Rozdziela
bibliotekę (`everywhere`, `this-agent`) od `<projekt>/.loadout/memory` (`this-project`),
adresuje każdą akcję przez zamrożone `{ catalogFolder, place, id }`, daje wcześniejszym
bibliotecznym notatkom projektowym trwały Move przed użyciem i nie pozwala spóźnionej
odpowiedzi poprzedniego workspace nadpisać ekranu. Ten sam runtime oracle dowodzi promptu,
stempla, dokładnego tombstone i braku wycieku między dwoma projektami. T-129 i T-130 przejmą
osobno bieżący stan budżetu/pochodzenia oraz zamrożony receipt rzeczywistych odbiorców.

## 2026-08-26, 03:17 — T-127 w trunku: prywatny stan każdego procesu Claude'a

**T-127 · zielone · 47 min 30 s do końca lądowania · $0,00 widoczne.** Pięć tur Codeksa
(kontrakt, implementacja, druga opinia, plan naprawy i wykonanie naprawy) zużyło łącznie
co najmniej 16,73 mln tokenów wejścia (16,10 mln z cache) i 66,4 tys. wyjścia. Uczciwe
`before` uruchomiło trzy samodzielne targety i padło na brakującym zachowaniu. Produkcja
izoluje każdą kopię pod `<run>/claude/<work-key>`, refleksję pod `_reflection`, nadpisuje
hostile `CLAUDE_CONFIG_DIR` po `env_clear` i odmawia widocznym zdaniem przed spawnem, gdy
prywatnego katalogu nie da się przygotować; stan gospodarza pozostaje nietknięty.

Pierwsza pełna bramka po implementacji miała zielone AC-1/2/3, ale globalny `full-test`
dwukrotnie trafił w hostowe `Operation not permitted` i brak boot-time. Druga opinia zgłosiła
dwie zasadne luki dowodu: brak późniejszej rundy tej samej kopii i brak jawnej ścieżki
`AgentEvent::Notice` → `Line::Problem`. Jedyna naprawa domknęła je w `285d112` i `c1b5ab7`;
dwa końcowe przebiegi gałęzi przeszły 19/19 w 43,00 s i 41,86 s. `integrate.sh` wylądował
wyłącznie `task-T-127` jako **`c5bcc5c`**; bramki `main` przed i po merge'u przeszły 16/16
w 115,57 s oraz 236,36 s. `TASK.md` nie przeżył lądowania, a trunk jest czysty.

## 2026-08-26, 02:24 — T-109 ZAMKNIĘTE; T-127 przejmuje izolację stanu przed spawnem

**T-109 · ZAMKNIĘTE / NIEURUCHOMIONE / NIEWYLĄDOWANE · $0,00.** Wszystkie trzy `check:`
filtrują funkcje we wspólnym targecie `tests/it`, więc łamią globalnie unikalną ścieżkę
`AGENTS.md` §2a i mogą zazielenić pusty wybór. Kontrakt wymaga ponadto zmian
`commands/run.rs` oraz vendor-neutralnego `drivers/mod.rs` poza swoim `OWNS`. Nie wolno
naprawiać go rozszerzeniem własności ani uruchamiać po to, żeby zobaczyć znaną wadę.

Świeże **T-127** startuje z wylądowanego T-126 i ma trzy nowe standalone targety. Dodaje
vendor-neutralny `work_key`, izoluje zwykłe kopie pod `<run>/claude/<work-key>`, refleksję pod
`_reflection`, nadpisuje hostile `CLAUDE_CONFIG_DIR` po `env_clear` i dowodzi trzema
prawdziwymi spawnami bez serializacji. Awaria przygotowania katalogu ma odmówić przed
pierwszym procesem, pokazać dokładne zdanie człowiekowi i zapisać je w `run.json`.

## 2026-08-26, 02:16 — T-126 w trunku: prywatna refleksja, prawdziwy Stop i trwały receipt

**T-126 · zielone · 1 godz. 12 min 08 s do końca lądowania · $0,00 widoczne.** Dwie
porównywalnie zapisane tury Codeksa zużyły łącznie co najmniej 38,75 mln tokenów wejścia
(37,85 mln z cache) i 109,4 tys. wyjścia; recenzja, plan oraz wykonanie naprawy nie mają
wspólnego paragonu tokenów. Enforced `before` uczciwie uruchomiło cztery standalone targety:
5/5 kontroli w 4,61 s, czerwone na brakującym zachowaniu.

Pierwsza pełna bramka przeszła **20/20** w 52,68 s. Refleksja dostała osobne evidence,
ustawienia i receipt, wybór w UI doszedł trzema rzeczywistymi drogami, pusty handoff nie
uruchamiał tury, późny Stop sprzątał prawdziwą grupę, a budżet należał do skonfigurowanego
klona zamiast nazwy modelu. Recenzent znalazł dwie zasadne uwagi medium. Pierwsza była luką
wyroczni AC-4: test dowodził stanu ręcznie złożonego klona, lecz nie wykonywał produkcyjnej
funkcji `reflection_driver`; inspekcja potwierdziła bieżące
`with_budget(REFLECTION_BUDGET_USD)` i wartość `0.08`, ale końcowy oracle fazy musi jeszcze
zmierzyć tę prawdziwą ścieżkę. Druga była wadą kodu: wynik `GroupProof::Alive` był ignorowany.

Jedyna naprawa w `4251ff6` rozróżnia `Dead` i `Alive`: bez `ESRCH` Stop nie udaje
zakończenia, nie zamyka potencjalnie żywego uchwytu i nie zapisuje fikcyjnej refleksji ani
kosztu. Dwie końcowe bramki gałęzi przeszły **20/20** w 52,44 s oraz 47,36 s.
`integrate.sh` wylądował wyłącznie `task-T-126` jako **`b232a580`**; pełna bramka `main`
przed merge'em przeszła 16/16 w 119,31 s, a po merge'u 16/16 w 237,02 s. Trunk jest czysty,
`TASK.md` nie przeżył lądowania. T-109 pozostaje historycznym, niewykonalnym kontraktem;
następne jest świeże T-127, które przejmuje wyłącznie izolację prywatnego stanu Claude'a.

## 2026-08-26, 00:57 — T-125 ZAMKNIĘTE; T-126 przejmuje H14 bez pięciosekundowej pułapki

**T-125 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 01 min 49 s · $0,00
widoczne.** Dwie porównywalnie zapisane tury Codeksa zużyły co najmniej 28,80 mln tokenów
wejścia i 96,8 tys. wyjścia; recenzja, plan i wykonanie naprawy nie mają wspólnego paragonu
tokenów. Enforced `before` uczciwie certyfikowało cztery nowe targety: 5/5 kontroli w 11,30 s,
a AC-2 padło dopiero po zamontowaniu prawdziwej aplikacji w Chromium.

Pierwsza pełna bramka miała **16/20** w 27,11 s. AC-2 i AC-4 były zielone, ale pełny clippy
odrzucił podobne nazwy, AC-1 nie znalazło oczekiwanej kandydatki, a AC-3 zakończyło się
`ENOENT`. Recenzent znalazł cztery zasadne luki: exact-once wracało przed oknem na opóźniony
duplikat, Stop ufał fikcyjnemu `Dead` bez supervisora, budżet można było rozpoznać po promptcie,
a kompatybilność starego `run.json` sprawdzała tylko brak błędu.

Jedyna naprawa w `6b72e1a` dodała okno obserwacji, neutralne prompty, supervisor-backed Stop
i pełniejsze asercje historii. Autorytatywna bramka pozostała **15/20** w 60,14 s. AC-3 i
AC-4 były zielone. AC-1 skanowało `<home>/memory/notes`, choć `scan_notes` samo dopisuje
`notes/`, więc test czytał `notes/notes`; receipt produktu już miał `kept:1`,
`dropped_without_reason:1` i koszt. Wszystkie sześć scen AC-2 czekało stałe 6 sekund przy
domyślnym limicie Vitest 5 sekund. Pełny clippy znalazł ścisłe porównanie `f64`, a formatter
złą kolejność importów. To są błędy wyroczni po jedynej rundzie, nie mandat do piątej tury.

Gałąź `task-T-125` jest czysta na `6b72e1a`, lecz nie wolno jej lądować ani wznawiać.
Świeże **T-126** startuje z `main`, nie przenosi kodu, commitów, speców ani testów T-125.
Skanuje prawidłowy korzeń `<home>/memory`, polluje pierwsze IPC najwyżej 4 sekundy, potem
obserwuje co najmniej 300 ms ciszy przy jawnym limicie testu ≥15 s, używa prawdziwego PGID i
`ESRCH`, identycznych neutralnych promptów, tolerancji `f64` oraz zachowuje id, status i kroki
starego biegu. T-109 i następca pamięci czekają na wylądowanie T-126; operacyjnie dalej
Codex + Codex.

## 2026-08-25, 23:46 — T-123 ZAMKNIĘTE; T-125 przejmuje H14 z prawdziwym efektem przeglądarki

**T-123 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 53 min 51 s · $0,00 widoczne.**
Dwie porównywalnie zapisane tury Codeksa zużyły co najmniej 40,65 mln tokenów wejścia i
91,7 tys. wyjścia; recenzja, plan i wykonanie naprawy nie mają wspólnego paragonu tokenów.
To jest wolumen, który przy płatnym cenniku zewnętrznym przekroczyłby próg ręcznej uwagi;
bieżący harness raportuje jednak koszt widoczny $0,00, więc nie wolno dopisać zmyślonej kwoty.
Enforced `before` uczciwie certyfikowało cztery nowe targety: 5/5 kontroli w 1,21 s.

Pierwsza pełna bramka przeszła **20/20** w 49,48 s. Prywatna refleksja miała osobne evidence,
ustawienia i budżet; zwykły krok zachował fizyczny UUID, rachunek był addytywny, puste
przekazanie nie uruchamiało tury, a tryb klona — nie nazwa modelu — przydzielał limit.
Recenzent znalazł jednak dwie zasadne luki medium. Stop był sprawdzany wyłącznie zanim zwykły
krok skończy scheduler, więc nie dowodził anulowania już żywej refleksji. Frontendowy AC-2
bezpośrednio wołał `requestRun()` i `launchRequested()`: `renderToStaticMarkup` nie uruchamia
efektów, zatem jedyny produkcyjny konsument żądania edytora w
`useSyncExternalStore`/`useEffect` pozostawał niewykonany mimo zielonego testu.

Jedyna naprawa w commicie `98945a2` poprawiła pierwszy defekt: późny Stop anuluje
`AgentHandle` prywatnej tury i nowy scenariusz AC-3 przeszedł 5/5. Nie wolno było uczciwie
załatać drugiego bez efekt-capable przyrządu. Naprawa rozbudowała ponadto produkcyjne
`a_short_turn_about` do 116 wierszy. Ostatnia, autorytatywna bramka miała **19/20** w 41,78 s:
wszystkie AC, pełne testy, scope, format i typy były zielone, ale `full-clippy` odrzucił limit
100 wierszy. Po jednej rundzie nie ma kolejnej tury ani ręcznej poprawki.

Gałąź `task-T-123` jest czysta na `98945a2`, lecz nie wolno jej lądować ani wznawiać.
Odbudowywalny target usunięto (5,1 GiB według Cargo); źródła, branch i paragony pozostały.
Świeże **T-125** startuje z `main`, nie przenosi kodu/speców T-123 i przejmuje H14. Nowe AC-2
używa istniejącego `e2e/harness.ts`, prawdziwego Chromium, widocznego checkboxa, Entera w
`/run` oraz prawdziwego kliknięcia Run w edytorze; dopiero zamontowany efekt może wyemitować
`run_workflow`. AC-3 czeka ze Stopem na żywy proces refleksji i dowodzi jego śmierci, a kontrakt
od początku ogranicza dotknięte funkcje produkcyjne do 100 wierszy. T-109 i T-104 czekają na
wylądowanie T-125; operacyjnie dalej Codex + Codex.

## 2026-08-25, 22:45 — T-124 w trunku: pełna auto-pamięć i trwały atomowy persist

**T-124 · zielone · 24 min 12 s · $0,00 widoczne.** Dwie porównywalnie zapisane tury
Codeksa zużyły co najmniej 6,93 mln tokenów wejścia i 41,3 tys. wyjścia; recenzja, plan i
wykonanie naprawy nie mają wspólnego paragonu tokenów. Enforced `before` uczciwie
certyfikowało trzy nowe targety: 4/4 kontroli w 0,57 s.

Pierwsza pełna bramka przeszła 19/19 w 45,48 s. Auto-pamięć skończonego kroku zachowuje
`ThisAgent`, nazwę właściciela, cały pierwszy akapit, dokładne źródłowe body i `Why`; writer
składa front matter z body w jednym same-directory tempie, a błąd nie zmienia ani starego
pliku, ani pełnego listingu. Read-only plik docelowy przy zapisywalnym katalogu dowodzi, że
zwykły copy-over nie może udawać podmiany.

Recenzent znalazł dwie zasadne granice. Medium: stan końcowy AC-3 nie obserwuje hipotetycznego
mikro-okna `unlink → create`, choć bieżący kod naprawdę używa `NamedTempFile::persist` i nie
ma jawnego remove. Low: `sync_all` tempa plus rename nie utrwala jeszcze wpisu katalogowego
po crashu. Planner zaklasyfikował pierwsze jako ograniczenie wyroczni, nie defekt bieżącej
implementacji; naprawa nie maskowała go grepem ani nową final-state asercją. Drugi problem
naprawił commit `da7827f`: po udanym `persist` otwiera rodzica i propaguje `sync_all` katalogu.
Dwie końcowe bramki przeszły 19/19 w 41,86 s oraz 39,85 s. Ograniczenie AC-3 pozostaje jawne:
test odrzuca copy-over, ale sam nie jest pełnym dowodem braku chwilowego unlinku.

`integrate.sh` wylądował wyłącznie `task-T-124` jako **`63f4246`**. Pełna bramka `main`
przed merge'em przeszła 16/16 w 96,68 s, a po merge'u 16/16 w 176,61 s. Trunk jest czysty,
`TASK.md` nie przeżył lądowania. Odbudowywalny target worktree usunięto (4,9 GiB według
Cargo). Następne jest T-123 przez Codex + Codex.

## 2026-08-25, 22:08 — T-122 ZAMKNIĘTE; T-124 przejmuje H15 z mocniejszą wyrocznią

**T-122 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 23 min 17 s · $0,00 widoczne.**
Dwie porównywalnie zapisane tury Codeksa zużyły co najmniej 7,44 mln tokenów wejścia i
38,3 tys. wyjścia; recenzja, plan i wykonanie naprawy nie mają wspólnego paragonu tokenów.
Enforced `before` uczciwie certyfikowało oba nowe targety: 3/3 kontroli w 0,39 s.

Pierwsza pełna bramka miała **17/18** w 46,57 s. Oba AC, pełne testy, scope, format i typy
były zielone; `full-clippy` odrzucił infallible `draft() -> Result<NoteDraft>` w nowym teście.
Recenzent zgłosił jedną zasadną uwagę medium: AC-2 dowodziło błędu, retry, bajtów i listingu,
ale mogło przepuścić temp-then-copy-over zamiast atomowego rename. Inspekcja bieżącej
implementacji potwierdziła prawidłowe same-directory temp + `sync_all` + `persist`, lecz luka
wyroczni pozostała prawdziwa.

Jedyna naprawa usunęła pierwszy lint bez zmiany produkcji lub asercji, po czym pełny clippy
odsłonił drugi taki sam defekt: infallible `fake_drivers() -> Result<Drivers>` w drugim nowym
teście. Pierwsza bramka po naprawie miała 17/18 w 41,31 s i zielony `full-test`. Ostatnia,
autorytatywna bramka miała **16/18** w 40,54 s: ten sam lint oraz wtórne `ENOSPC` frontendu.
Brak miejsca nie zmienia diagnozy, bo wcześniejsza pełna suita przeszła; po jedynej rundzie
nie ma piątej tury.

Gałąź `task-T-122` jest czysta na `b8f01ca`, ale nie wolno jej lądować ani wznawiać. Usunięto
wyłącznie odbudowywalne targety Cargo czystych, zakończonych worktree (31,0 GiB według Cargo);
źródła, gałęzie i paragony zostały zachowane, a wolne miejsce wzrosło z 123 MiB do 23 GiB.
Świeże **T-124** przejmuje cały H15 bez przenoszenia kodu/speców: fallible helpery mają
`Result`, infallible konkretny typ, a osobna mutacja read-only target + writable parent
odróżnia atomowy persist/rename od copy-over. T-123 zależy teraz od T-124. Operacyjnie dalej
Codex + Codex.

## 2026-08-25, 21:24 — T-121 w trunku: dokładny i atomowy snapshot Store

**T-121 · zielone · 25 min 24 s · $0,00 widoczne.** Dwie porównywalnie zapisane tury
Codeksa zużyły co najmniej 5,33 mln tokenów wejścia i 37,2 tys. wyjścia; recenzja, plan i
wykonanie naprawy nie mają wspólnego paragonu tokenów. Enforced `before` uczciwie
certyfikowało oba nowe targety: 3/3 kontroli w 0,36 s.

Pierwsza pełna bramka przeszła 18/18 w 45,35 s. Recenzent znalazł jednak dwie zasadne luki
oracle: fixture exact-multiset nie zawierała dwóch identycznych eventów, a rollback był
obserwowany dopiero po zwróceniu błędu. Jedyna naprawa była test-only: dodała identyczny
duplikat pełnej krotki oraz współbieżnego czytelnika, który podczas zablokowanej wymiany może
widzieć wyłącznie cały stary snapshot. Produkcyjna implementacja pozostała jednym jobem
jedynego writera i jedną transakcją. Dwie końcowe bramki przeszły 18/18 w 39,26 s oraz
44,02 s.

`integrate.sh` wylądował wyłącznie `task-T-121` jako **`968d239`**. Pełna bramka `main`
przed merge'em przeszła 16/16 w 92,93 s, a po merge'u 16/16 w 171,96 s. Trunk jest czysty,
`TASK.md` nie przeżył lądowania. Następne jest T-122 przez Codex + Codex.

## 2026-08-25, 20:45 — T-120 ZAMKNIĘTE; niezależne cele rozdzielone na T-121…T-123

**T-120 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 55 min 12 s · $0,00 widoczne.**
Dwie porównywalnie zapisane tury Codeksa zużyły co najmniej 26,27 mln tokenów wejścia i
72,3 tys. wyjścia; recenzja, plan i wykonanie naprawy nie mają wspólnego paragonu tokenów.
Enforced `before` uczciwie certyfikowało sześć nowych targetów: 7/7 kontroli w 1,27 s.

Pierwsza pełna bramka miała **15/22** w 48,96 s. Zielone były AC-1 i AC-5, cały scope,
format, typy i granice. Czerwone pokazywały niewykonaną atomową wymianę Store, martwy toggle,
brak semantycznego `left_nothing`, pozostawiony `todo!` atomowego writera, dwa test-only szwy
oraz regresje istniejących dubli refleksji. Recenzent zgłosił trzy uwagi: low o czerwonym
paragonie było wyłącznie findingiem procesu; medium o braku prawdziwego rerenderu checkboxa
i medium o braku końcowych bajtów po udanym retry były zasadne. Planner uwzględnił obie.

Naprawa wylądowała w czterech commitach gałęzi: atomowa wymiana snapshotu, prawdziwy stan
Startu, semantyczne pominięcie pustego handoffu oraz atomowy writer pełnego Markdownu.
Końcowa, autorytatywna bramka na czystym `bde6e8a` miała **19/22** w 17,19 s. Zielone były
AC-1 i AC-3…AC-6, pełny clippy, format, typy, wiring i wszystkie pozostałe quick checks.

Pozostałe trzy czerwienie rozstrzygają kontrakt, nie uzasadniają piątej tury. AC-2 sortowało
odczytane eventy po `body`, ale expected zachowywało kolejność wejściową — poprawny dokładny
snapshot przegrywał na odwróconych dwóch wierszach. `quick-scope` wykazało prawdziwego
właściciela wspólnego stanu `/run`: `src/sections/run/index.tsx` było poza `OWNS`. `full-test`
ujawniło dwa osobne problemy: dozwolone duble z `reflecting()` nie dostały wymaganych
`with_settings`/`with_evidence`/budżetu, a auto-pamięć pojedynczego kroku została błędnie
awansowana z zamrożonego `ThisAgent + agent` do `ThisProject`. Refleksja całego biegu jest
projektowa; prywatna notatka jednego agenta nie.

Gałęzi `task-T-120` nie wolno lądować ani wznawiać; `main` nie dostał jej kodu. Żeby kolejne
zadanie nie mieszało trzech niezależnych domen w jednej rundzie naprawy, jawny mandat
właściciela na cały plan rozdziela świeże zastępstwo: **T-121** ląduje atomowy Store z
order-independent exact multiset, **T-122** zachowuje pełny Markdown i `ThisAgent` atomowo,
a **T-123** ląduje prywatną refleksję, prawdziwy rerender i komplet tras z `index.tsx` w
`OWNS`. Każde startuje ze świeżego trunka i nie przenosi kodu, commitów ani speców T-120.
Kolejność: T-121 → T-122 → T-123 → T-109 → T-104; operacyjnie Codex + Codex.

## 2026-08-25, 19:43 — T-119 ZAMKNIĘTE: fizyczny UUID, zakres i higiena pełnej bramki

**T-119 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 06 min 19 s · $0,00
widoczne.** Dwie porównywalnie zapisane części biegu zużyły co najmniej 21,12 mln tokenów
wejścia i 78,8 tys. wyjścia; recenzja, plan i wykonanie naprawy nie zostawiły kompletnego,
porównywalnego paragonu. Enforced `before` uczciwie certyfikowało sześć nowych targetów:
7/7 kontroli w 1,29 s.

Pierwsza pełna bramka miała **12/22** w 53,16 s: szkielety zachowania pozostały niewykonane.
Recenzent znalazł trzy zasadne luki wyroczni: AC-3 nie dowodziło domyślnego zaznaczenia przed
pierwszą zmianą, AC-2 wymagało nowego artefaktu bez jawnej nieobecności starego, a AC-6
zgadywało nazwy plików tymczasowych zamiast porównać pełny listing katalogu. Planner poprawnie
przeniósł te uwagi do jedynej naprawy, lecz wskazał też trzy produkcyjne trasy Startu poza
zamrożonym `OWNS`; orchestrator jawnie ostrzegł, że nie wolno ich dotknąć.

Naprawa doprowadziła AC-2…AC-6 do zieleni. Końcowa bramka na czystym `dcf0d5c` miała
**17/22** w 20,70 s. AC-1 użyło logicznego klucza workflow `build` jako nazwy pliku evidence,
choć wylądowany kontrakt `run_evidence_reaches_the_product.rs` używa fizycznego UUID kroku z
`run.json`. Pełny clippy odrzucił 123-wierszową funkcję nowego testu, formatter odrzucił nowy
test TS, a `quick-scope` wykazał zmiany `launch.ts`, `requested-launch.ts` i `run-command.ts`
poza `OWNS`. `full-test` agregował błąd AC-1; pozostałe testy systemu były zielone.

Gałęzi `task-T-119` nie wolno lądować ani wznawiać; `main` nie dostał jej kodu. Jawna zgoda
właściciela na cały plan autoryzuje świeże **T-120** z sześcioma globalnie unikalnymi
targetami. Nowy kontrakt wyprowadza oczekiwane evidence z fizycznego UUID w prawdziwym
`run.json`, obejmuje wszystkie trzy trasy Startu w `OWNS`, wymaga formattera, ogranicza każdą
funkcję nowego testu rustowego do 90 wierszy i od początku zamyka wszystkie trzy uwagi
recenzenta. Nie przenosi commitów, implementacji, speców ani testów z T-119. T-120 musi
wylądować przed T-109 i T-104; operacyjna para pozostaje Codex + Codex.

## 2026-08-25, 18:35 — właściciel zatwierdził cały pozostały plan i zastępstwo T-118

Jawne „wykonaj cały plan, możesz modyfikować zadania, masz god mode” uruchamia wyjątek
authoringu dla **T-119** i wszystkich koniecznych świeżych zastępstw. Kod produkcyjny nadal
idzie wyłącznie przez `ship-task.sh`, a każda zielona gałąź osobno przez `integrate.sh`.
Operacyjna para pozostaje Codex + Codex.

T-119 startuje z czystego `main` i nie przenosi commitów, implementacji, speców ani testów z
`task-T-118`. Sześć nowych targetów wpisuje od początku cztery uwagi recenzenta T-118 oraz
poprawia jego błędną wyrocznię: AC-2 zmienia snapshot i wymusza rollback w połowie wymiany;
AC-3 używa handlera toggle'a znalezionego w prawdziwym drzewie `Start`, a następnie uruchamia
przycisk, `/run` i żądanie edytora; AC-4 składa kanoniczne nagłówki wyłącznie przez
`Section::name()`; AC-5 daje zwykłemu krokowi dokładnie model refleksji; AC-6 dowodzi
atomowości przez awarię tworzenia pliku tymczasowego i nietknięte stare bajty. T-119 musi
wylądować przed T-109 i T-104.

## 2026-08-25, 18:24 — T-118 ZAMKNIĘTE: druga czerwień na błędnej wyroczni AC-4

**T-118 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 12 min 07 s · $0,00
widoczne.** Dwie strukturalnie zapisane tury Codeksa zużyły co najmniej 30,66 mln tokenów
wejścia i 92,9 tys. wyjścia; recenzja, plan i wykonanie naprawy nie zostawiły porównywalnego
paragonu tokenów. Enforced `before` uczciwie certyfikowało sześć nowych, samodzielnych plików
testowych: 7/7 kontroli, 1,29 s.

Pierwsza pełna bramka miała **16/22** w 22,27 s. Zielone były AC-1, AC-2 i AC-5; czerwone
wykazały niewpięty atomowy writer, martwy `ReflectionToggle`, brak semantycznego filtrowania
`left_nothing` i regresje istniejących dubli refleksji. Recenzent znalazł dodatkowo cztery
zasadne luki wyroczni: rebuild mógł być no-opem, test Startu nie obejmował `/run` i edytora,
budżet można było przypisać do modelu zamiast trybu refleksji, a końcowe bajty notatki nie
dowodziły atomowego zapisu. Planner przekazał wszystkie te punkty jedynej naprawie.

Naprawa wzmocniła AC-2, AC-3, AC-5 i AC-6, podpięła produkcyjne drogi i usunęła wszystkie
pierwotne czerwienie. Końcowa bramka na czystym `c3ff52f` miała **20/22** w 19,74 s:
zielone były pełny clippy, wszystkie quick checks oraz AC-1, AC-2, AC-3, AC-5 i AC-6.
`full-test` i AC-4 powtarzają ten sam pojedynczy defekt nowego testu.

Wyrocznia AC-4 żąda dosłownie nagłówków `What changed / Decisions / Open questions`, choć
autorytatywny, wylądowany format handoffu w `memory/handoff.rs` to
`Answer / Evidence / Open`. Sam kontrakt T-118 wymagał trwałej nazwanej informacji i
semantyki `left_nothing`, nie tych starych nazw. Zmiana produkcji pod błędną asercję byłaby
oszustwem; zmiana autorytatywnego formatu wymagałaby pliku poza `OWNS`. Po zużyciu jedynej
rundy nie wolno też ręcznie poprawić testu i ponowić bramki. Branch `task-T-118` pozostaje
czysty i **nie może wylądować**; `main` nie dostał jego kodu. Kontynuacja wymaga decyzji
właściciela o świeżym zastępstwie z nowymi globalnie unikalnymi wyroczniami opartymi o
kanoniczne `Answer / Evidence / Open`.

## 2026-08-25, 17:08 — właściciel zatwierdził pełne zastępstwo T-117

Po zamknięciu T-117 właściciel polecił kontynuować. Nowy kontrakt **T-118** startuje ze
świeżego `main`; nie przenosi commitów, implementacji, speców ani testów z
`task-T-117`. Ma sześć nowych, globalnie unikalnych ścieżek kryteriów i musi wylądować
przed T-109 oraz T-104.

Kontrakt zamyka wszystkie trzy wady pierwszej bramki T-117 i uwagę recenzenta. Tryb klona
drivera ma być jawny, więc refleksji nie wolno rozpoznawać po tekście promptu, modelu,
kolejności ani nazwie pliku; zwykły krok i refleksja zachowują równocześnie własne dowody.
Wyrocznia frontendu przechodzi przez prawdziwy element `ReflectionToggle` znaleziony w
drzewie zwróconym przez produkcyjny `Start`, a nie przez osobny helper lub setter. Udany
pusty krok nadal zapisuje prawdziwy handoff `left_nothing`; dopiero refleksja rozpoznaje tę
semantykę i nie uruchamia płatnego procesu. Zapis pełnego Markdownu pamięci ma należeć do
`memory::notes` i być atomowy razem z front matter, bez ponownego otwierania pliku w
`run.rs`. Operacyjna para pozostaje **Codex + Codex**.

## 2026-08-25, 16:43 — T-117 ZAMKNIĘTE: naprawę przerwał pełny dysk

**T-117 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 04 min 13 s · $0,00
widoczne.** Cztery ukończone tury Codeksa zapisały co najmniej 33,73 mln tokenów wejścia
i 109,2 tys. wyjścia; piąta, przerwana tura wykonawcza nie zdążyła zapisać paragonu. Enforced
`before` uczciwie certyfikowało wszystkie sześć nowych AC jako runtime-red: 7/7 kontroli,
1,38 s.

Pierwsza pełna bramka miała **19/22** w 70,31 s. Zielone były AC-2…AC-6, pełny zakres, typy
i wszystkie quick checks. AC-1 wykryło realną regresję: zwykły krok stracił dotychczasową
ścieżkę `logs/agent-<step>.*`. `full-clippy` wykrył zakazane `panic!` w helperze nowego testu
AC-6. `full-test` niosło oba problemy i regresję T-114 wprowadzoną przez T-117: udany pusty
krok przestał zostawiać prawdziwy handoff `left_nothing`.

Recenzent zgłosił dwie uwagi. Zasadna uwaga medium wykazała, że AC-3 woła handler z osobno
wyeksportowanego `reflectionCheckbox()`, nie z kontrolki należącej do drzewa `Start`; martwy
checkbox na ekranie mógłby więc przejść. Uwaga low o braku refleksji w SQLite nie zmienia
kontraktu: prawdą jest blok w `run.json`, SQLite pozostaje odtwarzalnym indeksem, a wyrocznia
już pilnuje bajtów pliku i stabilności wszystkich czterech tabel.

Planner jedynej naprawy prawidłowo rozpisał trzy defekty kodu/testu oraz wzmocnienie AC-3.
Wykonawca zaczął od odczytu kontraktu i źródeł, ale o 16:39 dysk osiągnął `ENOSPC`. Codex nie
mógł zapisać własnego rollouta ani stdout i zakończył paniką `StorageFull` **przed pierwszą
zmianą**. Końcowa próba bramki nie jest wynikiem produktu: nie mogła utworzyć heredoców,
artefaktów Cargo, pliku Vitesta, `runs/.last.tmp` ani `assertions-now.tsv`. Branch pozostał
czysty na `04d0801`; nie ma commita naprawy. Harness zużył jednak cztery tury i jawnie odmawia
piątej, więc T-117 nie wolno wznawiać ani lądować.

Po biegu `cargo clean` usunęło wyłącznie odbudowywalne cache `target/` z zamkniętych worktree
T-117, T-116 i T-103 (raportowane łącznie 15,7 GiB); źródła i paragony pozostały. Wolne miejsce
wzrosło ze 119 MiB do 12 GiB. `main` nie dostał kodu T-117 i pozostaje czysty. Uczciwa
kontynuacja wymaga świeżego zastępstwa z nowymi globalnie unikalnymi specami; nie wolno
przenosić commitów, implementacji ani testów z `task-T-117`.

## 2026-08-25, 15:29 — właściciel zatwierdził uczciwe zastępstwo T-116

Po zamknięciu T-116 właściciel polecił kontynuować. Nowy kontrakt **T-117** startuje ze
świeżego `main` i nie przenosi commitów, implementacji, speców ani testów z `task-T-116`.
`OWNS` obejmuje teraz dokładnie brakującą drogę idempotentnej odbudowy przez
`src-tauri/src/store/mod.rs` i jedynego pisarza w `src-tauri/src/store/writer.rs`.

Sześć nowych, globalnie unikalnych wyroczni usuwa wadliwy setup zwykłego Markdownu i od
początku zamyka cztery uwagi recenzenta: każdy błąd twardego opakowania musi zostawić
`reflection.ran == false`; frontend woła prawdziwy `onChange` widocznego checkboxa; brak
przekazania jest udanym pustym krokiem, nie krokiem `failed`; katalog dowodów odmawia także
dodatkowych plików `reflection*`. Ponowny `Store::rebuild_from` tego samego biegu musi przejść
dwa razy bez duplikatów, bez zmiany `run.json` i bez `INSERT OR IGNORE`. Operacyjna para
pozostaje Codex + Codex na osobnych modelach. T-117 musi wylądować przed T-109 i T-104.

## 2026-08-25, 15:05 — T-116 ZAMKNIĘTE po drugiej czerwieni

**T-116 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 02 min 43 s · $0,00 widoczne.**
Etapy Codeksa nie zapisały kompletnej wyceny, więc to wyłącznie koszt widoczny. Enforced
`before` uczciwie certyfikowało sześć nowych AC jako runtime-red. Pierwsza i druga pełna
bramka miały **19/22**; końcowa trwała 19,97 s. Zielone były AC-1, AC-3, AC-4 i AC-5,
pełny clippy, zakres, typy oraz wszystkie pozostałe szybkie kontrole. Czerwone pozostały
AC-2, AC-6 i agregujący je `full-test`.

AC-2 ujawniło kolejny rzeczywisty brak zakresu. Wyrocznia odbudowuje ten sam bieg ponownie,
a `Store::rebuild_from` próbuje drugi raz wstawić ten sam `runs.id` i dostaje SQLite 1555
`UNIQUE constraint failed: runs.id`. Planner naprawy uznał idempotentną odbudowę indeksu za
defekt kodu i wskazał najmniejszą zmianę w `src-tauri/src/store/mod.rs`. Tego pliku nie ma w
`OWNS` T-116; zmiana identyfikatora, przepisywanie `run.json` albo `INSERT OR IGNORE` bez
rozstrzygnięcia zdarzeń byłyby obejściem pod test.

AC-6 ma niezależny defekt zamrożonej wyroczni. Helper `body_of_text` woła
`FrontMatter::split` na zwykłym pliku Markdown bez front matter i kończy się
`NoFrontMatter { path: "" }`, choć produkcyjny kolektor poprawnie traktuje taki plik jako
ciało od bajtu zero. Dodanie sztucznego front matter zmieniłoby przypadek, a poprawa helpera
po certyfikacji `before` byłaby zmianą kryterium. Planner nazwał to wprost defektem testu;
wykonawca zgodnie z regułą nie zrobił częściowej naprawy ani commita.

Recenzent wskazał ponadto cztery nierozstrzygnięte luki oracle: brak asercji `ran:false` dla
każdego brakującego wrappera, ominięcie prawdziwego handlera widocznego checkboxa, przypadek
„bez handoffu” zbudowany z porażki zamiast udanego pustego wyniku oraz brak odmowy dodatkowych
plików refleksji. Wszystkie uwagi są zasadne i muszą wejść do następnego kontraktu od początku.

Gałąź `task-T-116` jest czysta na `af18576`; `quick-scope` było zielone, lecz gałęzi nie wolno
lądować. `main` nie dostał jej kodu i pozostaje czysty. Faza 7 stoi przed T-109. Uczciwa
kontynuacja wymaga kolejnego pełnego zastępstwa z `store/mod.rs` w `OWNS`, sześcioma nowymi
ścieżkami testów, poprawną fiksturą Markdown przed `before` i czterema domkniętymi lukami
recenzenta; nie wolno przenosić commitów, implementacji ani speców z `task-T-116`.

## 2026-08-25, 13:59 — właściciel zatwierdził pełne zastępstwo T-103

Jawne „rób, masz pozwolenie na wszystko” właściciela uruchamia wyjątek authoringu dla
**T-116**. Nowy kontrakt startuje z czystego `main`, nie przenosi commitów, implementacji ani
speców z `task-T-103` i obejmuje oba brakujące pliki: `src-tauri/src/evidence.rs` oraz
`src/sections/run/io.ts`. Ma sześć nowych globalnie unikalnych ścieżek testów.

T-116 wzmacnia też dwie luki ujawnione przez drugą czerwień: prawdziwy panel Startu pokazuje
działający, domyślnie włączony przełącznik refleksji, a oba istniejące opt-in duble zachowują
swoje stare asercje po dołożeniu prywatnych ustawień, dowodów i sufitu ceny. Operacyjna para
pozostaje Codex + Codex na osobnych modelach. T-116 musi wylądować przed T-109 i T-104.

## 2026-08-25, 13:37 — T-103 ZAMKNIĘTE: kontrakt wymaga dwóch plików poza OWNS

**T-103 · czerwone / ZAMKNIĘTE / NIEWYLĄDOWANE · 1 godz. 15 min 25 s · $0,00 widoczne.**
Etapy Codeksa nie zapisały kompletnej wyceny, więc to wyłącznie koszt widoczny. Enforced
`before` uczciwie certyfikowało pięć nowych AC jako runtime-red. Po jedynej rundzie naprawy
końcowa pełna bramka miała **19/21 w 15,93 s**: wszystkie AC, clippy, typy i kontrole
podpięcia były zielone; czerwone pozostały `full-test` i `quick-scope`.

Kontraktu nie da się uczciwie wykonać w zamrożonym `OWNS`. AC-1 wymaga dokładnych artefaktów
`logs/reflection.jsonl`, `logs/reflection.stderr.log` i `logs/reflection.input.json`, ale
istniejący `EvidenceTarget` umie nazwać tylko dowody kroku grafu albo rozmowy Lead. Jawna
tożsamość i konstruktor refleksji muszą powstać w `src-tauri/src/evidence.rs`, którego nie ma
w `OWNS`. Pierwszy wykonawca wykrył to przed implementacją i poprawnie odmówił kopiowania,
symlinków oraz pustych plików jako obejścia pod test. AC-3 niezależnie wymaga, żeby produkcyjny
Start wysłał nazwany klucz `reflectionEnabled`: Tauri deserializuje argumenty przed wejściem
do komendy i pominięty `Option<bool>` odrzuca wywołanie. Ta krawędź mieszka w
`src/sections/run/io.ts`, również poza `OWNS`.

Wykonawca naprawy mimo tego zmienił oba pliki poza zakresem. Dodatkowo nowe obowiązkowe
opakowania `with_settings`, `with_evidence` i limitu ceny sprawiły, że istniejący dubler,
który jawnie podaje wyłącznie szew `reflecting()`, przestał wykonywać turę. Stary oracle
`a_run_that_handed_nothing_on_is_never_asked` dostał zero wywołań zamiast jednego. To jest
rzeczywista regresja implementacji, ale po drugiej czerwieni nie ma kolejnej rundy naprawy.

Zgodnie z regułą fazy zakresu nie rozszerzono i zadania nie przepisano. Gałąź `task-T-103`
pozostaje czysta na `6db6091`, lecz nie wolno jej lądować; `main` nie dostał żadnego jej kodu
i pozostaje czysty. Faza 7 stoi przed T-104. Uczciwa kontynuacja wymaga nowego zadania
zastępczego z pełnym `OWNS` i nowymi, globalnie unikalnymi ścieżkami testów.

## 2026-08-25, 12:00 — T-115 w trunku po dwóch jawnie autoryzowanych poprawkach testowych

**T-115 · zielone · 57 min 05 s właściwego biegu harnessu + ręczne domknięcie lintów ·
$0,00 widoczne.** Etapy Codeksa nie zapisały kompletnej wyceny ani księgi użycia, więc to
wyłącznie koszt widoczny. Mocne cztery AC pozostały bez zmian: nierówne kolumny cen są
sprawdzane osobno dla Sol/Terra/Luna, ekran sumuje dwa różne koszty, nieznany model zachowuje
tokeny bez `$0.00`, a Codex dostaje otwieralne pełne ścieżki handoffów.

Po zielonej pierwszej bramce 20/20 recenzent znalazł dwa średnie defekty, które jedyna runda
naprawy domknęła z regresjami: model dociera teraz także przez App Server do wspólnego dekodera
cen (`9573327`), a uwaga o nieznanej cenie jest kojarzona po stabilnym kluczu kroku zamiast po
nieunikalnej nazwie (`3396e6f`). Naprawa zostawiła jedynie deterministyczne lity we własnym
nowym teście. Właściciel dwa razy jawnie dopuścił test-only domknięcie poza zakończonym grafem:
przeniesienie stałej przed instrukcje (`0c79213`) i zamianę dwóch `assert!(false)` na
równoważne `Result::Err` (`9c6635d`). Nie zmieniono produkcji, kryterium ani siły asercji.

Po pierwszym ręcznym commicie `cargo check --all-targets --keep-going` był zielony, a pełna
bramka miała 19/20 i ujawniła drugi lint. Po drugim commitcie `cargo check` znów był zielony,
a pełna bramka przeszła **20/20 w 65,73 s**. `integrate.sh` wylądował wyłącznie
`task-T-115` jako **`118c876`**: bramka main przed merge'em przeszła **16/16 w 90,33 s**,
a po merge'u **16/16 w 171,77 s**. Drzewo jest czyste, `TASK.md` nie przeżył lądowania.
Następne jest T-103, przez Codex + Codex.

## 2026-08-25, 11:08 — T-115 czerwone po naprawie; wyłącznie testowy lint wymaga decyzji

**T-115 · czerwone / NIEWYLĄDOWANE · 57 min 05 s · $0,00 widoczne.** Etapy Codeksa nie
zapisały kompletnej wyceny ani księgi użycia, więc to wyłącznie koszt widoczny. Enforced
`before` uczciwie certyfikowało cztery nowe AC jako runtime-red. Odczyt zamrożonych speców
potwierdził, że naprawiają obie luki T-102: każdy znany model dostaje nierówne 10k/5k/20k,
a prawdziwy ekran odróżnia sumę `$1.20` od obu operandów `$0.41` i `$0.79`.

Pierwsza pełna bramka przeszła **20/20 w 42,39 s**. Recenzent tego samego vendora na osobnym
modelu gpt-5.5 znalazł jednak dwa średnie defekty kodu: ścieżka App Servera nie niosła modelu
do wspólnego dekodera ceny, a uwagi o nieznanej cenie były kojarzone po nieunikalnej nazwie
agenta zamiast po stabilnym kluczu kroku. Jedyna runda naprawy przeniosła model przez App
Server i dodała regresję (`9573327`) oraz przypisała uwagę do prawdziwego klucza kroku z
regresją dwóch równoległych kroków o tej samej nazwie (`3396e6f`). Wszystkie AC i `full-test`
są po naprawie zielone.

Wykonawca naprawy wprowadził jedną deterministyczną czerwień wyłącznie w nowym teście:
`const UNKNOWN_MODEL` w `codex.rs` stoi po instrukcjach i uruchamia
`clippy::items_after_statements`. Dwie końcowe bramki miały przez to **19/20 w 45,26 s** i
**19/20 w 40,88 s**; jedyną porażką był ten sam lint, bez porażki zachowania. Przesunięcie
deklaracji przed instrukcje nie zmienia kryterium ani kodu produkcyjnego, lecz byłoby piątą,
ręczną turą po zamknięciu grafu Harnessu. Zgodnie z AGENTS.md §7 orchestrator zatrzymał się
zamiast robić ją po cichu.

Gałąź `task-T-115` jest czysta na `3396e6f`; `main` nie dostał jej kodu. Faza 7 stoi przed
T-103. Potrzebna jest jawna decyzja właściciela: dopuścić audytowalną, test-only poprawkę
poza grafem, potem `cargo check --all-targets --keep-going` i pełną bramkę, albo zamknąć
T-115 i pisać kolejne zastępstwo.

## 2026-08-25, 02:25 — właściciel zatwierdził pełne zastępstwo T-102

Jawne „ok” właściciela uruchamia wyjątek authoringu dla **T-115**. Nowy kontrakt startuje
z czystego `main`, nie przenosi gałęzi, implementacji ani speców T-102 i ma cztery globalnie
unikalne ścieżki testów. Cennik każdego znanego modelu dostaje nierówne liczniki 10k/5k/20k,
więc zamiana wejścia, cache lub wyjścia jest czerwona; prawdziwy ekran dostaje co najmniej
dwa niezerowe koszty różnych vendorów i musi pokazać ich sumę, nie jeden operand. T-115 musi
wylądować przed T-103. Operacyjna para pozostaje Codex + Codex na osobnych modelach.

## 2026-08-25, 02:24 — T-102 zielone, lecz NIEWYLĄDOWANE; dwie uwagi wyroczni zostały otwarte

**T-102 · formalnie zielone 20/20, lecz NIEWYLĄDOWANE · 38 min 53 s · $0,00 widoczne.**
Etapy Codeksa nie zapisały kompletnej wyceny ani księgi użycia, więc to wyłącznie koszt
widoczny. Enforced `before` uczciwie certyfikowało wszystkie cztery AC jako runtime-red.
Implementacja na czystej gałęzi `task-T-102` kończy się w `7a22629`: wycenia znane modele
Codeksa jako szacunek, zachowuje same tokeny dla nieznanego modelu, pokazuje wydatki na pasku
i wyjaśnia krokom Codeksa, że pliki handoffów leżą poza katalogiem pracy.

Pierwsza pełna bramka miała **19/20**: wszystkie AC i `full-test` były zielone, a jedyną
czerwienią był `clippy::doc_markdown` w komentarzu nowej wyroczni. Recenzent tego samego
vendora na osobnym modelu gpt-5.5 znalazł jednak dwie niezależne luki asercji. Średnia uwaga:
test tabeli daje Terra i Luna dokładnie po milionie tokenów wejścia, cache i wyjścia, więc
zamiana stawek między kolumnami zachowuje tę samą sumę i nadal przechodzi. Niska uwaga: test
prawdziwego ekranu zasila pasek jedną płatną linią, więc implementacja pokazująca pierwszy
albo ostatni koszt zamiast sumy obu vendorów także przechodzi.

Planista poprawnie zaproponował nierówne próbki per model oraz dwa płatne kroki na ekranie,
ale nazwał je „criterion/test defect”. Wykonawca zinterpretował regułę zamrożonego oracle
dosłownie: poprawił wyłącznie lint w `7a22629` i odmówił zmiany obu asercji. Dwie końcowe pełne
bramki były przez to zielone **20/20 w 40,15 s** i **20/20 w 38,42 s**, lecz nie odpowiadają
na drugą opinię. Odczyt pozostałych testów potwierdził, że żadna niezałączona wyrocznia nie
zamyka luk: nierówne tokeny są sprawdzone tylko dla Sol, a wszystkie testy paska używają
jednego płatnego wiersza.

Gałęzi nie wylądowano mimo kodu 0, ponieważ zielone kryterium da się przejść dokładnie tymi
dwoma błędnymi implementacjami. Zmiana zamrożonych speców po certyfikacji `before` albo druga
runda naprawy łamałyby kontrakt Harnessu. `main` pozostaje czysty i bez kodu T-102; faza 7
stoi przed T-103. Uczciwe wyjście to nowe zadanie zastępcze z nowymi globalnie unikalnymi
ścieżkami testów, nierównymi tokenami dla każdego modelu i ekranową sumą co najmniej dwóch
płatnych kroków.

## 2026-08-25, 01:44 — T-101 w trunku

**T-101 · zielone · 47 min 38 s biegu harnessu · $0,00 widoczne.** Etapy Codeksa nie
zapisały kompletnej wyceny ani księgi użycia, więc to wyłącznie koszt widoczny. Enforced
`before` uczciwie certyfikowało wszystkie cztery AC jako runtime-red. Pierwsza tura
implementacji nie zostawiła zmian i pierwsza bramka miała **15/20**: czerwone były cztery AC
oraz agregujący je `full-test`.

Planista naprawy potwierdził trzy rzeczywiste boczne drzwi omijające wspólną politykę porażki.
Jedyna runda naprawy skierowała odmowę kontekstu (`bfeeec9`), zablokowaną trasę (`0fc04c5`)
i sufit budżetu (`b5bf409`) przez `when_this_one_fails`. W rezultacie ustawienia `carry-on`,
`ask-me` i `stop` działają na tych ścieżkach tak samo jak na zwykłej porażce, strumień zgadza
się z książką, potomkowie zatrzymani budżetem mówią o budżecie zamiast o Stopie człowieka,
a `carry-on` przekazuje prawdziwe ostatnie słowa.

Recenzent tego samego vendora na osobnym modelu gpt-5.5 zgłosił tylko niską uwagę o braku
zielonego paragonu przed naprawą. Pierwsza pełna bramka po naprawie miała **19/20**: wszystkie
AC były zielone, a stary test `trigger_editor_writes_safe_file` spoza OWNS dostał przejściowy
`RecvError`/timeout w cleanupie. Końcowa bramka Harnessu, bez żadnej zmiany kodu lub testu,
przeszła **20/20 w 43,07 s**; flake nie został zamaskowany ani naprawiony w tym zadaniu.

`integrate.sh` wylądował wyłącznie `task-T-101` jako **`73ec11c`**. Pełna bramka main przed
merge'em przeszła **16/16 w 88,81 s**, a po merge'u **16/16 w 171,25 s**. Drzewo jest czyste,
`TASK.md` nie przeżył lądowania. Następne jest T-102, przez Codex + Codex.

## 2026-08-25, 00:50 — T-100 w trunku

**T-100 · zielone · 36 min 18 s biegu harnessu · $0,00 widoczne.** Etapy Codeksa nie
zapisały kompletnej wyceny ani księgi użycia, więc `$0,00` oznacza wyłącznie koszt widoczny,
nie oszacowanie ceny. Wymuszone `before` uczciwie certyfikowało cztery kryteria jako runtime-red.
Pierwsza pełna bramka gałęzi przeszła **20/20 w 42,83 s**, a dwie pełne bramki po jedynej
rundzie naprawy przeszły **20/20 w 42,60 s** i **20/20 w 37,23 s**.

Tester pętli dostaje wymagane pole `outcome`; ustrukturyzowana wartość rozstrzyga przed
zgodnościową linią prozy, tester widzi wszystkie wcześniejsze próby implementera, a `run.json`
addytywnie zapisuje wynik każdej rundy. Recenzent tego samego vendora na osobnym modelu gpt-5.5
znalazł rzeczywistą lukę mimo zielonej bramki: parser pola czytał je tylko wewnątrz
`## Answer`, choć wspólny nośnik `key: value` nie ma takiego ograniczenia. Jedyna naprawa
rozszerzyła odczyt na całe ciało i dodała regresję, w której kanoniczne `outcome: pass` poza
Answer wygrywa ze sprzecznym późniejszym markerem (`aacd038`).

`integrate.sh` wylądował wyłącznie `task-T-100` jako **`18e0cd3`**. Pełna bramka main przed
merge'em przeszła **16/16 w 88,58 s**, a po merge'u **16/16 w 162,37 s**. Drzewo jest czyste,
`TASK.md` nie przeżył lądowania. Następne jest T-101, przez Codex + Codex.

## 2026-08-25, 00:09 — T-114 w trunku

**T-114 · zielone · 42 min 32 s biegu harnessu · $0,00 widoczne.** Zapisane tury kontraktu
i implementacji Codeksa zużyły łącznie co najmniej **25,54 mln tokenów wejścia i 68,7 tys.
wyjścia**; osobna recenzja oraz plan i wykonanie naprawy nie zapisały kompletnego użycia ani
wyceny. Wymuszone `before` uczciwie certyfikowało sześć runtime-red speców, a końcowa bramka
gałęzi przeszła dwukrotnie **22/22** (44,11 s i 39,29 s).

Kopie `fresh-copy` mają osobne poprawne refy, a kolizja zakodowanych ogonów jest widocznym
ostrzeżeniem przy zapisie i Problemem przy Starcie przed katalogiem biegu, worktree i spawnem.
Prompt podaje otwieralny adres pełnej kopii bieżącego biegu, zachowując prawdziwą etykietę
zwykłego poprzednika albo pliku przeniesionego z wcześniejszego biegu. Ostatnie `outcome:`
przeżywa limit dokładnie raz, pusta udana odpowiedź jest nazwana, a tylko źródło strzałki
powrotnej musi mieć jedną kopię.

Recenzent samego vendora (osobny model gpt-5.5) znalazł rzeczywistą lukę: rozdęta preambuła
przed `## Answer` nie dzieliła budżetu 8 KB z nagłówkami, wskaźnikami i końcową decyzją.
Jedyna naprawa dodała mocną regresję wymagającą limitu, wszystkich nagłówków, wskaźnika,
jednej decyzji i pełnej kopii bajt w bajt (`43c3a4c`). Pierwsza oficjalna bramka miała
niezależny timeout gotowości starego E2E; `e2e/harness.ts` poza OWNS pozostał nietknięty,
a dwa następne pełne przebiegi były zielone.

`integrate.sh` wylądował wyłącznie `task-T-114` jako **`50ad074`**. Pełna bramka main przed
merge'em przeszła **16/16 w 127,84 s**, a po merge'u **16/16 w 160,77 s**. Drzewo jest czyste,
`TASK.md` nie przeżył lądowania. Następne jest T-100, przez Codex + Codex.

## 2026-08-24, 21:24 — właściciel zatwierdził T-114 i Codex + Codex

Jawne „ok” właściciela uruchamia wyjątek authoringu dla **T-114**, pełnego zastępstwa
zamkniętych T-99/T-112/T-113. Nowy kontrakt startuje z `main`, ma sześć globalnie unikalnych
ścieżek i nie przenosi starej implementacji. Poprawione AC-3 osobno wymaga etykiety zwykłego
poprzednika oraz prawdziwej etykiety pliku przeniesionego z wcześniejszego biegu; obie ścieżki
muszą wskazywać pełną kopię pod katalogiem bieżącego biegu. T-114 musi wylądować przed T-100.

Właściciel jawnie polecił używać dalej **Codex + Codex**, ponieważ kończy się budżet Claude'a.
Recenzja pozostaje osobnym wywołaniem Harnessu, w roli tylko do odczytu i na innym modelu;
ograniczenie samego vendora ma być raportowane, ale nie zastępowane Claude'em bez nowej decyzji.

## 2026-08-24, 21:21 — T-113 czerwone po naprawie; błędny spec AC-3 wymaga decyzji człowieka

**T-113 · czerwone / NIEWYLĄDOWANE · 47 min 12 s od pierwszego startu · $0,00 widoczne.**
Po przełączeniu właściciela na Codex + Codex kontrakt i implementacja zapisały łącznie co
najmniej **22,86 mln tokenów wejścia i 69,0 tys. wyjścia**; recenzja oraz plan i wykonanie
naprawy nie zapisały osobnej ceny ani kompletnego użycia. Wymuszone `before` uczciwie
certyfikowało wszystkie sześć AC jako runtime-red. Implementacja pozostała w OWNS i kończy się
na czystej gałęzi `task-T-113` w `9bd71fa`; trunk nie dostał żadnej jej zmiany.

Pierwsza i druga pełna bramka po jedynej rundzie naprawczej miały **20/22**. Zielone są AC-1,
AC-2, AC-4, AC-5 i AC-6, pełny clippy oraz wszystkie szybkie kontrole. Czerwone są wyłącznie
AC-3 i agregujący tę samą porażkę `full-test`. Działający kod daje czytelnikowi bezwzględny,
otwieralny adres pełnej kopii w katalogu bieżącego biegu, zachowuje względny wskaźnik na dysku,
przenosi adres przy wznowieniu i nie tworzy attachmentu dla krótkiej odpowiedzi.

Powód czerwieni jest w zamrożonym specu kontraktowym, nie w tym zachowaniu. Ten sam helper
asercji wymaga po wznowieniu dokładnej etykiety `what the step before left`, choć istniejące
wyrocznie i produkcyjny model pochodzenia wymagają wtedy `what an earlier run left here`.
Zadanie wymaga dla wznowienia nowego adresu, nie fałszywej informacji o pochodzeniu. Obu
dokładnych równości nie da się spełnić równocześnie. Uzależnienie etykiety od długości tekstu,
przeklasyfikowanie przeniesionego pliku na zwykłego poprzednika albo zmiana speca po
certyfikacji `before` byłyby oszustwem pod test.

Recenzent zgłosił jedną niską uwagę proceduralną: czerwony paragon nie pozwala zweryfikować
zmiany jako gotowej. Planista naprawy wskazał błędne kryterium i zalecił decyzję człowieka;
wykonawca poprawnie zatrzymał się bez zmian i commita. Zgodnie z AGENTS.md §7 oraz regułą
drugiej czerwieni faza 7 stoi przed T-100. T-113 nie wolno landować ani wznawiać bez jawnej
decyzji, czy zastąpić błędny spec nowym kontraktem.

## 2026-08-24, 20:30 — oracle `before` naprawiony; T-113 ma zgodę i nowy kontrakt

Właściciel jawnie autoryzował osobną naprawę harnessu oraz T-113. Commit **`5604c3d`**
zamyka wadę z biegu T-112: `NOT_A_REAL_RED` rozpoznaje każdą numerowaną diagnostykę
kompilatora Rusta oraz końcowe `could not compile`, więc E0308 we wspólnym targetcie nie może
już udawać czerwonego zachowania. Automatyczny selftest pyta funkcję werdyktu, nie tekst regexu:
reprezentatywny E0308 musi dostać „did not RUN”, a runtime'owa panika testu nadal certyfikuje
uczciwe `before`. Nie rozluźniono kryterium ani żadnego istniejącego wyjątku.

Składnia obu plików przeszła, nowy selftest przeszedł dwukrotnie, a pełne pasy Rust i Web były
zielone. Końcowy `harness/guards.sh` na czystym commicie ujawnił osobny, istniejący stan własnej
księgi: **11 strażników zadziałało, 1 (`quick-scope`) chybił, 4 odkryte checki nie mają funkcji
guard** (`before-spec-owns`, `quick-invoke-args`, `quick-tests-listed`, `quick-wired`). Tego
wyniku nie zamaskowano ani nie dopisano wyjątków do commita naprawiającego `before`; wymaga
osobnego rozstrzygnięcia harnessu.

Nowe **T-113** jest pełnym zastępstwem T-99/T-112 z sześcioma globalnie unikalnymi ścieżkami
speców. Zachowuje poprawne refy kopii, żywy adres załącznika, końcowe `outcome:`, sygnał pustej
odpowiedzi i jednoznacznego sędziego. Dodane kryterium liczy planowane klucze `fresh-copy`
tym samym kodowaniem co Git i wymaga widocznej odmowy kolizji `s_2~2` z literalnym `s_2-2`
przez prawdziwy Start — przed katalogiem biegu, drzewem roboczym i pierwszym procesem. Hashowanie
lub losowe przemianowanie nie jest dopuszczonym obejściem. T-99/T-112 pozostają tylko dowodem;
T-100 czeka na wylądowanie T-113.

## 2026-08-24, 19:04 — T-112 ZAMKNIĘTE bez lądowania; zielona bramka była nieważna

**T-112 · formalnie zielone 21/21, lecz ZAMKNIĘTE / NIEWYLĄDOWANE · 1 h 09 min 09 s ·
co najmniej $34,78 widoczne.** Kontrakt Claude'a kosztował **$14,52** i doszedł do limitu
81 tur; implementacja kosztowała **$20,25** w 139 turach. Recenzja Codeksa i wykonanie
naprawy przez Claude'a nie zapisały osobnej ceny. Końcowa bramka przy czystym drzewie miała
21/21 w 36,76 s, a jedyna runda naprawcza mechanicznie rozbiła 102-liniowy test bez zmiany
asercji (`0234a26`). Gałąź nie została wylądowana mimo kodu 0.

Pierwszy powód jest kontraktowy. `branch_for(run, "s_2~2") → s_2-2` oraz gwarancja, że
prawidłowy klucz `s_2-2` przechodzi niezmieniony, wybierają ten sam ref. Loadout akceptuje
ręcznie zapisane identyfikatory bez ograniczenia znaków; workflow z `s_2` w dwóch kopiach i
osobnym `s_2-2` zapisuje się, przechodzi `check_to_run` i odmawia dopiero podczas drugiego
`git worktree add -b` — po rozpoczęciu pracy, wbrew niezmiennikowi 12. Zielone AC-1 nie ćwiczy
tej kolizji. Uczciwe wyjście wymaga nowego kontraktu: rekomendowana jest widoczna odmowa
kolizji zakodowanych refów przed pierwszym procesem, zamiast zmiany istniejących nazw gałęzi.

Drugi powód unieważnia cały paragon `before`. Kontraktowy test AC-3 destrukturyzował
3-elementowy wynik jako dwie wartości. Między commitem kontraktu `cabbfc4` a implementacją
`6820eec` agent musiał poprawić dokładnie dwa takie miejsca. Każde z pięciu AC kompiluje wspólny
target `it`, więc wszystkie wymuszone `before` padły na ten sam E0308, nie na brak zachowania.
Harness nie rozpoznał podpisu błędu kompilacji i błędnie wypisał „red for the right reason”.
To defekt warstwy zaufania: dopóki nie powstanie osobny, autoryzowany commit harnessu z
selftestem, następny kontrakt Rust może dostać ten sam fałszywy certyfikat.

Dodatkowe znalezisko spoza OWNS: `commands/history.rs::branches_of_run` dopasowuje ogon refa
do `tile_key`, więc nową gałąź drugiej kopii `s_2-2` pokaże bez etykiety kroku. Jest to mniejsza
resztka produktu, nie powód do cichego rozszerzenia T-112.

Gałąź `task-T-112` pozostaje niewylądowana na `0234a26`; trunk nie dostał kodu produkcyjnego.
Faza 7 zatrzymuje się przed T-100. Potrzebna jest jawna zgoda właściciela na osobną naprawę
harnessu oraz na nowy kontrakt zastępczy z walidacją kolizji refów.

## 2026-08-24, 17:53 — właściciel zatwierdził zastępcze T-112

Jawne „ok” właściciela na rekomendowany task zastępczy uruchamia wyjątek authoringu wyłącznie
dla kontraktu i dokumentacji. **T-112** zastępuje zamknięte T-99 i nie bierze z jego gałęzi
commitów ani speców. Rozstrzygnięcia są trzy: ref drugiej kopii to poprawne dla Gita `s_2-2`,
trwały handoff zachowuje względny i przenośny wskaźnik, natomiast zmontowany prompt odbiorcy
dostaje bezwzględny adres kopii z bieżącego biegu; sędzią pętli jest źródło strzałki powrotnej.
Pięć nowych ścieżek wyroczni jest globalnie unikalnych. T-112 musi wylądować przed T-100.

## 2026-08-24, 17:31 — T-99 ZAMKNIĘTE, sprzeczne wyrocznie i dwa błędy kontraktu

**T-99 · czerwone / ZAMKNIĘTE · 1 h 04 min 26 s · co najmniej $35,23 widoczne.** Kontrakt
Claude'a kosztował **$13,50** i doszedł do limitu 81 tur; implementacja kosztowała **$21,72**
w 156 turach. Recenzja Codeksa i wykonanie naprawy przez Claude'a nie zapisały osobnej ceny.
Mimo niezerowego wyjścia fazy kontraktu wymuszone `before` uczciwie certyfikowało wszystkie
cztery AC jako czerwone z właściwego powodu.

Pierwsza pełna bramka była czerwona na AC-2, tych samych dwóch przypadkach w `full-test` oraz
pięciu lintach w nowych testach. Jedyna runda naprawcza poprawiła linty bez suppressions
(`225ee2e`) i zapisała bezwzględny wskaźnik pełnej kopii (`777c041`). Ostatnia bramka przy
czystym drzewie miała **19/20 w 14,51 s**: AC-1/AC-2/AC-3/AC-4, clippy i wszystkie szybkie
sprawdzenia były zielone, lecz `full-test` nadal miał dokładnie dwie porażki.

To nie jest zaproszenie do ręcznej naprawy. `src-tauri/tests/it/memory_handoff_cap.rs`, którego
nie ma w OWNS T-99, asertuje dokładną relatywną linię `Moved to attachments/<name>`. AC-2
asertuje dla tej samej linii ścieżkę bezwzględną i otwieralną z dowolnego katalogu. Jedna
wartość nie może spełnić obu równości. Istniejący zielony test wznowienia dodatkowo staje się
fałszywy: `new_run.join(absolute_path)` ignoruje `new_run` i sprawdza plik starego biegu.
Naprawa wymagałaby pliku spoza OWNS oraz decyzji, co bezwzględny adres ma znaczyć po usunięciu
poprzedniego biegu. Zgodnie z regułą fazy nie rozszerzono OWNS i nie osłabiono żadnej wyroczni.

Recenzent wykrył też dwa niezależne błędy tekstu zadania. AC-1 wymaga dokładnej gałęzi
`loadout/<run>/s_2~2`, ale Git zabrania `~` w refach; produkcja poprawnie koduje ją jako
`s_2-2`, a zielony test sprawdza tylko różność i prefiks, więc nie certyfikuje literalnego AC.
AC-4 mówi o **celu** strzałki powrotnej (`link.to`), podczas gdy opis zadania, model pętli,
produkcja i test wskazują kafelek zamykający pętlę (`link.from`); wymuszenie celu zakazałoby
legalnych wielokrotnych wejść do pętli. Zielone AC nie naprawiają tych sprzeczności.

Gałąź `task-T-99` pozostaje niewylądowana na `777c041`; trunk nie dostał żadnej jej zmiany.
Faza 7 zatrzymuje się tutaj zgodnie z regułą drugiej czerwieni i nierozstrzygniętych uwag
recenzenta. T-100 nie został rozpoczęty.

## 2026-08-24, 16:25 — T-111 w trunku

**T-111 · zielone · 41 min 56 s biegu harnessu + jawnie zatwierdzone domknięcie testowe ·
koszt widoczny: brak wyceny.** Zapisane etapy kontraktu i implementacji Codeksa zużyły łącznie
**13,32 mln tokenów wejścia i 41,7 tys. wyjścia**; artefakty naprawy i recenzji Claude'a nie
zapisały kompletnej ceny. Lead Codeksa czyta teraz efektywną konfigurację przed `thread/start`,
wyłącza prywatne MCP w konfiguracji wątku, ponownie włącza wyłącznie zatwierdzone Connections
i odmawia startu, gdy konfiguracji nie da się bezpiecznie zinterpretować. Ten sam encoder klucza
TOML obsługuje identyfikatory z cudzysłowem i znakami sterującymi, a dane prywatnych serwerów
nie trafiają do argv ani evidence.

Recenzent cross-vendor zgłosił pięć uwag. Cztery zakończyły się poprawkami: brak lub `null`
`mcp_servers` oznacza pustą kolekcję, wyrocznia wykrywa też escaped identyfikator, odmowa zatruwa
evidence, a encoder ucieka znaki sterujące. Piątą — czy nakładka per-thread odpowiada żywej
semantyce App Servera — rozstrzygnęły oficjalne typy protokołu i implementacja `ConfigManager`:
mapa `ThreadStartParams.config` jest konwertowana do par TOML i dokładana po nakładkach CLI,
a `mcp_servers.<id>.enabled` jest oficjalnym przełącznikiem. Nie przyjęto jej ani nie odrzucono
na intuicję.

Po jedynej rundzie naprawczej pełna bramka była czerwona wyłącznie dlatego, że nowy test miał
112 linii przy limicie clippy 100. Zgodnie z osobną zgodą właściciela mechanicznie rozbito jego
niezmienione asercje na helpery (`5bee49a`), bez dotknięcia produkcji lub kryteriów. Pełna bramka
gałęzi przeszła **19/19 w 49,35 s**, a `integrate.sh` zakończył lądowanie `6926cb3` pełną bramką
trunka **16/16 w 154,37 s**. Drzewo jest czyste i `TASK.md` nie przeżył lądowania. T-105 i T-110
pozostają zamknięte; następnym zadaniem jest T-99.

## 2026-08-24, 15:25 — T-110 ZAMKNIĘTE, pełny zakres przejmuje T-111

**T-110 · czerwone / ZAMKNIĘTE · około 1 h 11 min do kontrolowanego przerwania · koszt
widoczny: brak kwoty w paragonie.** Dwie tury Codeksa zużyły łącznie **14,47 mln tokenów
wejścia i 51,4 tys. wyjścia**; artefakty recenzji i naprawy Claude'a nie zapisały kwoty.
Kontrakt uczciwie certyfikował 3 AC, implementacja dostała jedną opinię cross-vendor i dokładnie
jedną rundę naprawczą. Po niej AC-1/AC-2/AC-3 były zielone, a pełna bramka doszła do starego
`lead_evidence_is_durable.rs` i nie skończyła się w swoim suficie: jego atrapa App Servera nie
odpowiadała na nowe `config/read`. Proces testu i jego grupa zostały zatrzymane po rozpoznaniu
dokładnych pgid; ponowna sonda drzewa procesów była pusta. Gałąź nie została wylądowana.

To jest wynik granicy, nie zaproszenie do poprawienia speca. Sam kontrakt wskazał przed
implementacją, że ta pełna fikstura wymaga zmiany, ale pliku nie było w OWNS. Po jednej rundzie
naprawy kryteria zadania przeszły, lecz produktowa suita zawisła dokładnie na tym brakującym
ogniwie. Zgodnie z regułą fazy „wykonalne tylko plikiem spoza OWNS = ZAMKNIĘTE” T-110 nie jest
wznawiane i żaden jego commit nie trafia do main.

Recenzent zgłosił pięć uwag. Utratę zatwierdzonych Connections naprawił jeszcze bieg, ale nie
wylądowała z zamkniętą gałęzią. Dwie uwagi o semantyce nakładki rozstrzygnęły później źródła
OpenAI: `ThreadStartParams.config` jest mapą, `config/read` ma oficjalny parametr i kształt,
a `ConfigManager::load_with_cli_overrides` konwertuje pary żądania do TOML i dokłada je po
nakładkach CLI; `mcp_servers.<id>.enabled=false` jest oficjalnym przełącznikiem. Uwaga o obrazie
nie jest defektem: T-34 świadomie nie pokazuje arbitralnego błędu vendora przy załączniku i ma
na to żywe E2E; pełna treść pozostaje dla wiadomości tekstowej. Rozjazd web-dialu jest osobnym,
niezweryfikowanym znaleziskiem i nie został ukryty w tym zadaniu.

Właścicielski wyjątek authoringu tworzy **T-111** z nowymi, globalnie unikalnymi ścieżkami.
Obejmuje oba stare serwery-atrapy, autorytatywną listę Connections i wspólny encoder klucza,
więc nie powtarza granicy T-110. T-111 jest następnym i ostatnim zastępstwem przed T-99;
T-105 ani T-110 nie wolno wznawiać.

## 2026-08-24, 14:04 — T-105 ZAMKNIĘTE, cel przejmuje T-110

**T-105 · czerwone / ZAMKNIĘTE · 14 min ostatniego przebiegu · $0,00 widoczne.** Kontrakt
Codeksa zużył łącznie 7,29 mln tokenów wejścia i 26,4 tys. wyjścia w fazie specyfikacji oraz
jednej naprawy; księga nie wycenia Codeksa. AC-1 i AC-2 dostały uczciwe czerwone testy, lecz
AC-3 po dwóch wymuszonych `before` nadal miało exit 0 bez dowodu wykonania. Gałąź nie została
wylądowana.

Powód nie jest brakiem pracy agenta. Wymagane `--ignore-user-config` działa w `codex exec`,
ale zainstalowany `codex-cli 0.149.1` odrzuca je przed i po `app-server`; pomoc App Servera
tej flagi nie wystawia. Dodanie asercji na argv zazieleniłoby atrapę i zepsuło prawdziwego
leada — byłoby oszustwem. Również `-c 'mcp_servers={}'` nie jest zastępstwem: pusta tabela
scala się niedestrukcyjnie, więc istniejące serwery pozostają włączone.

Właścicielska zgoda na pełne domknięcie fazy została użyta wyłącznie do authoringu nowego,
globalnie unikalnego `T-110`, bez łatania T-105. T-110 zachowuje dwa działające cele i przed
`thread/start` pobiera efektywną konfigurację przez `config/read`, po czym dla każdego
znalezionego serwera ustawia osobne `mcp_servers.<id>.enabled=false` w konfiguracji wątku.
Błąd odmawia startu; nie ma cichego powrotu do prywatnych MCP. T-110 musi wylądować przed
T-102 i jest następnym biegiem.

## 2026-08-24, 13:36 — faza 7: T-98 w trunku, pierwszy żywy bieg zmienił mapę

**T-98 · zielone · 1 h 52 min 26 s · $28,58 widoczne.** Przelotka obu vendorów nie może już
nadpisać transportu, polityki, połączeń, modelu ani limitu wydatku ustawianych przez Loadout;
podniesienia uprawnień są rozpoznawane po kluczu i wartości w obu drogach — przy zapisie oraz
przy Starcie. Po ręcznym domknięciu integracji pełna bramka trunka przeszła **16/0 w 57,64 s**,
a `TASK.md` nie przeżył lądowania (`3700831`, poprawka integracyjna `e175860`).

Próg $25 został przekroczony z konkretnych powodów. Pierwszy start wykrył zduplikowaną globalnie
ścieżkę wyroczni AC-4. Drugi ujawnił wyścig dwóch `worktree.sh`: równoległe zapisy uszkodziły
zarówno `~/.codex/config.toml`, jak i tymczasowy plik `~/.claude.json`. Naprawa harnessu używa
teraz jednej blokady, unikalnych plików tymczasowych i atomowej publikacji dla obu konfiguracji
(`465ec3e`, strażnik zarejestrowany w `6e78c7b`). Recenzent zgłosił cztery słuszne uwagi o sile
wyroczni; wszystkie cztery zostały zamknięte testami zachowania. Pełny clippy po merge'u złapał
jeszcze podwójne włączenie starego modułu T-36 — dlatego ostatni dowód był wykonany po ręcznej
poprawce integracyjnej, nie przed nią.

Otwarte znaleziska z T-98, poza jego kontraktem: rodzina `sandbox_workspace_write.*` rezerwuje
dziś tylko `network_access`, a nie np. `writable_roots`; goły klucz `mcp_servers` nie wpada pod
prefiks `mcp_servers.`; zdanie odmowy limitu wydatku mówi o uprawnieniach do plików. Zostają
tu jako fakty do osobnego kontraktu, nie jako ciche rozszerzenie wylądowanego zadania.

### Pierwszy prawdziwy bieg po fazie 6

Bieg `20260824-091300__01a0330b-6690-7eb2-a156-5613c14d0c9d` trwał **97,5 min**, wykonał
28 kroków przy trzech naraz i zakończył 26 sukcesami oraz dwiema porażkami. Widoczny koszt
Claude'a to **$26,86**; 15 kroków Codeksa zużyło 45,2 mln tokenów wejścia i 218 tys. wyjścia,
ale stara księga pokazała dla nich $0. Raport produktu powstał i przeszedł własne testy.

Żywy przebieg potwierdził obietnice fazy 6, których atrapy nie mogły dowieść: runda trzecia
dostała własne wcześniejsze próby i oba werdykty, fan-in dostał sześć przekazań, wszystkie
osiem tur sprawdzających wystawiło `outcome:`, kopie pracowały osobno, a pamięć przeszła pełne
koło produkcja → promocja człowieka → konsumpcja → `last_used_at`.

Jednocześnie dał trzy nowe dowody, włączone do fazy 7 przed następnym biegiem:

1. **N1 → T-109.** Sześć równoległych procesów Claude'a dzieliło `HOME` i zapisywało ten sam
   `~/.claude.json`. Jeden z nich dostał błąd parsowania JSON i padł po 273 ms z kodem 1;
   CLI zrobiło kopię i odbudowało plik, więc późniejsze kroki ruszyły. Ten `processExit` nadał
   całemu biegowi stan `failed`, mimo że nie był porażką pracy agenta. Kroki dostaną prywatny
   katalog stanu bez utraty równoległości; gospodarz nie może być ich wspólnym plikiem zapisu.
2. **N2 → T-99 AC-2.** W 20 z 28 przekazań pełna kopia była w `attachments/`. Limit 8 KB
   systematycznie usuwał końcową linię `outcome:` z pliku czytanego przez syntezę, chociaż
   silnik rozstrzygnął ją z surowej odpowiedzi. Ucięta kopia ma zachować tę jedną linię
   dokładnie raz, niezależnie od jej położenia, obok bezwzględnego adresu pełnej kopii.
3. **N3 → T-99 AC-3.** Martwy krok wszedł do następnych rund jako 434-bajtowe przekazanie
   z trzema pustymi nagłówkami. To żywe potwierdzenie istniejącego kryterium „left nothing",
   nie nowy zakres.

Druga porażka biegu była prawdziwym wynikiem: sprawdzający ostatniej rundy nie przepuścił pracy.
Mechanizm zadziałał, lecz `carry-on` pozwolił iść dalej; naprawę tej klasy prowadzą T-100/T-101.

## 2026-08-24, 01:40 — FAZA 6 ZAMKNIĘTA: dwanaście z dwunastu w trunku

Wszystkie zadania `T-86`…`T-97` wylądowały, bramka trunka zielona po każdym. Plan i mapa
znalezisk: `docs/PLAN-AGENTS-CONTEXT.md`. `ARCHITECTURE.md` uzgodniony z kodem tego samego dnia.

### Liczniki, z realnych danych

| | |
|---|---|
| Zadania | **12 z 12**, 47 kryteriów |
| Commity fazy | 105, w tym 12 lądowań |
| Koszt z transkryptów `ship-task` (6 zadań) | **$168,61** |
| Tryb szybki (6 zadań) | koszt w sesji orchestratora, nieliczony osobno |
| Decyzje oddane człowiekowi | **4** — i wszystkie cztery były prawdziwymi rozwidleniami |
| Rundy naprawcze / restarty | 5 (T-86, T-90 ×2, T-92 ×2, T-94) |
| Konflikty scalania rozwiązane ręcznie | 6 |
| Defekty złapane dopiero **pełną bramką na trunku** | **3** |

### Trzy rzeczy, które ta faza udowodniła o samym harnessie

1. **Brak konfliktu w gicie nie znaczy poprawności.** Trzy razy scalenie dwóch zielonych gałęzi
   dało drzewo, które się nie kompilowało albo nie przechodziło typów: przeniesiony wektor
   pożyczony osiemnaście linii niżej (T-92 × T-94), funkcja bez klamry zamykającej, bo git
   przyciął hunk na sygnaturze (T-90 × T-97), literał linii bez pól, które właśnie doszły do
   drutu (T-94 × T-97). **Jedynym świadkiem był kompilator i pełna bramka po ręcznym scaleniu.**
   Wniosek operacyjny: po każdym ręcznym rozwiązaniu konfliktu `cargo check --all-targets
   --keep-going`, a potem `./verify.sh full` — nigdy sam commit.
2. **`TASK.md` przeżywa ręczne scalenie.** `integrate.sh` kasuje go tylko na własnej ścieżce
   commita. Zostawiony sprawia, że każdy nowy worktree rodzi się z cudzym kontraktem, a
   `ship-task.sh` odmawia startu. Zdejmowany trzy razy w tej fazie.
3. **Zadanie o pięciu kryteriach dotykające czterech warstw nie mieści się w fazie kontraktu.**
   T-94 spaliło 81 tur i $12,06, nie napisawszy ani jednego pliku (`error_max_turns`). To samo
   zadanie w trybie szybkim, gdzie specyfikacja i implementacja dzielą jeden kontekst, przeszło
   za pierwszym podejściem. **Nie jest to wada modelu, tylko kształtu wywołania.**

### Cross-vendor zarobił na siebie trzy razy

Recenzent Codeksa zgłosił łącznie 12 uwag na zielonych kryteriach. Dwie były prawdziwymi
defektami kontraktu (`giveUpAfterMinutes: 0` obiecywane jako brak limitu przy silniku robiącym
`.max(1)`; AC-3 z T-90 rzekomo sprawdzające odmowę po turze agenta), sześć dotyczyło **siły
wyroczni**, a cztery obaliłem czytając kod. **Ani jednej nie przyjąłem na słowo** — każda
kosztowała 3–5 minut sprawdzenia i to jest właściwa cena.

### Co zostaje otwarte dla człowieka

- **`--settings` nie jest flagą zarezerwowaną**, a od T-92 Loadout ustawia ją sam.
  `agents_vendor_args_filtered.rs` używa jej wprost jako przykładu flagi **nie**zarezerwowanej,
  więc dopisanie jej do listy zmienia przesłankę tamtego testu — **decyzja, nie poprawka.**
- **Trzy długi z T-94, wszystkie po jednej linii w cudzym pliku:** kolizja przelotki
  z `--max-budget-usd` (jedna pozycja w `FORBIDDEN_ESCALATIONS`); pasek `$3.41 of $20` jest
  **liczony i nigdy nie pokazany**, bo `index.tsx` woła `stripFor` bez trzeciego argumentu;
  szew sterownika na flagę budżetu obok `effort_argv`.
- **Chip `12k tokens` jest niebudowalny**: słowo „tokens" jest zakazane przez sprawdzacz
  słownictwa, a `checks/` jest poza zasięgiem biegu. Pisarz T-97 cofnął zmianę zamiast walczyć
  z bramką — słusznie.
- **Kryterium, które liczy elementy, dryfuje po cichu.** `agent-form.test.tsx` miał stałą
  `THREE` z czterema pozycjami. Nota w `tasks/T-11.md`.
- **Pamięć per projekt** (`<repo>/.loadout/memory/`) dalej nie istnieje — zostaje globalnie,
  zgodnie z domyślną decyzją z planu §6.

### Czego ta faza NIE dowiodła

Ani jedno kryterium nie uruchomiło prawdziwego biegu z prawdziwymi agentami. Wszystkie dowody
stoją na `FakeDriver`, złotych plikach i `renderToStaticMarkup`. **Pierwszy prawdziwy bieg
workflow po tej fazie jest testem, którego bramka nie umie zrobić** — i to jest najbliższa
rzecz do zrobienia, zanim dołoży się cokolwiek nowego.

## 2026-08-24, 00:40 — faza 6: dziewięć zadań w trunku, tryb szybki się sprawdził

Kolejność lądowań po pierwszym wpisie: T-91, T-96, T-95, T-88, T-93, T-92. Trunk zielony po
każdym (`integrate.sh`, pełna bramka 15/0). Zostały T-90, T-94, T-97.

**Co realnie dostał produkt.** Pętla pamięta swoje rundy i oddaje dalej to, co przeszło (T-87).
Wznowienie niesie przekazania poprzedniego biegu razem z załącznikami (T-88). Poziom myślenia
dociera wreszcie do obu vendorów — `--effort` u Claude'a, `-c model_reasoning_effort` u Codeksa
(T-91). Katalog roboczy znika po biegu, praca zostaje na gałęzi, a historia umie te gałęzie zdjąć
(T-95). Krok pożycza z repo gospodarza to, co człowiek zaznaczył, i wybór jest własnością kafelka
(T-93). Ekran agenta przestał kłamać o tym, co dostał, a powtórzenie kroku ma wreszcie drogę
na ekran (T-96). **Pamięć ma producenta** — jedna tura refleksji po biegu, najwyżej trzy
kandydatki, każda z uzasadnieniem; auto-pamięć Claude'a pisze do katalogu biegu zamiast do
wspólnego katalogu projektu (T-92).

### Tryb szybki: co zdjęte, co zostaje

Właściciel zdjął pętlę zadaniową dla prostych zadań. Zdjęte: faza kontraktu jako osobne płatne
wywołanie, druga opinia i runda naprawcza. **Zostaje dowód** — własny worktree, kontrakt
zamrożony jako pierwszy commit gałęzi, `./verify.sh before` czerwony NA ASERCJI przed
implementacją, pełna bramka przed lądowaniem i druga pełna bramka na trunku po merge'u.
Tak wylądowały T-91, T-96, T-95 i T-93; recenzenta zostawiono przy zadaniach ruszających silnik.

### Trzy rzeczy, które ta faza kosztowała i których nie było w planie

1. **Trunk był czerwony w warstwie `full` od `905ef9e`** i przewrócił oba zadania pierwszej fali,
   zanim ktokolwiek napisał linijkę. `quick-clippy` jest `--lib`, `full-clippy` `--all-targets`,
   więc quick świecił 13/0. Naprawione `4fcab5c`.
2. **Nowe pole w strukturze przewraca każdy jej literał** — trzy razy z rzędu zadanie stanęło
   na tym samym. Zmierzone: `AgentStep` ma pięć literałów, `RunRequest` **55** i nie ma `Default`,
   `Line::Done` pięć (wszystkie w kryteriach T-05, T-07, T-10). Od T-94 liczę to **gerpem przed
   odpaleniem biegu**, nie po czerwonej bramce.
3. **`commands.golden.txt` musi być w OWNS każdego zadania dokładającego komendę** — trzy
   przeoczenia tego samego kształtu (T-93, T-95, T-92). Rejestracja bez wiersza to czerwień,
   wiersz bez rejestracji to martwa kontrolka.

### Cztery decyzje właściciela, wszystkie z dowodem mechanicznym

Każde poszerzenie zakresu szło z porównaniem linii `## AC-`, `check:` i `expect:` przed i po —
za każdym razem 0 różnic. Rozstrzygnięcia: asercja równości promptu zamieniona na „zawiera raz,
na początku" (T-86); zero minut znaczy w silniku brak limitu, nie jedną minutę (T-86, znalazł
recenzent Codeksa na zielonych kryteriach); obserwacja w cudzym instrumencie przeniesiona tam,
gdzie fakt nadal jest — treść z gałęzi, rejestracja drzewa w trakcie kroku (T-95, kryterium T-65);
refleksja jedzie własnym szwem z domyślnym `None`, nie fabryką sterowników (T-92).

**Ta ostatnia była jedyną zmianą TREŚCI kryterium w całej fazie** i warto wiedzieć dlaczego:
moje AC-1 kazało wołać fabrykę sterowników, czyli tę samą, którą podstawiają wszystkie testy —
28 testów w 20 plikach zobaczyło jedno wywołanie więcej. Tych liczb nie wolno było podnieść:
pilnują, żeby bieg nie odpalił więcej procesów, niż miał.

### Znaleziska, które zostają otwarte

- **Kryterium, które liczy elementy, dryfuje po cichu.** `agent-form.test.tsx` (kryterium T-11)
  ma stałą nazwaną `THREE` trzymającą **cztery** pozycje, a tekst kryterium mówi „dokładnie trzy".
  Ktoś dołożył wiersz i nie tknął ani nazwy, ani tekstu. Nota dopisana do `tasks/T-11.md`.
- **`--settings` nie jest flagą zarezerwowaną**, a od T-92 Loadout ustawia ją sam (przekierowanie
  auto-pamięci). `agents_vendor_args_filtered.rs` używa jej wprost jako przykładu flagi
  **niezarezerwowanej**, więc dopisanie jej do listy zmienia przesłankę tamtego testu —
  **to jest decyzja, nie poprawka.**
- **T-94 spaliło 81 tur i $12,06 w fazie kontraktu, nie pisząc ani jednego pliku** (`error_max_turns`).
  Zadanie o pięciu kryteriach dotykające `AppState`, `limits`, argv i frontu nie mieści się
  w budżecie tur jednej fazy kontraktowej. Przeniesione do trybu szybkiego, gdzie specyfikacja
  i implementacja dzielą jeden kontekst.
- Trzy uwagi recenzenta o **sile wyroczni**, nie o kodzie: AC-5 z T-87 nie przechodzi gałęzią
  „zapytaj mnie"; AC-1 z T-92 nie sprawdza limitu czasu refleksji (sam limit i zdejmowanie grupy
  procesów są w kodzie i sprawdziłem je); AC-2 z T-92 nie sprawdza licznika odrzuconych par.

## 2026-08-23, 20:45 — faza 6 ruszyła: T-89 w main, T-86 stoi na kolizji, trunk był czerwony od rana

Plan fazy i mapa 38 znalezisk: [`docs/PLAN-AGENTS-CONTEXT.md`](PLAN-AGENTS-CONTEXT.md).
Dwanaście kontraktów T-86…T-97 wylądowało jako `7206f4b` (wyjątek właściciela, AGENTS.md §2).
Fala 1 = T-86 + T-89 równolegle, cross-vendor (`--reviewer codex`, kredyty wróciły).

**Trunk był czerwony w warstwie `full` od `905ef9e`, i nikt tego nie widział.** `quick-clippy`
biegnie `--lib`, a `--all-targets` jest dopiero w `full-clippy`, więc trunk pokazywał 13/0 na
quick i przewracał każde zadanie, które doszło do pełnej bramki. Zmierzone: **oba** zadania fali 1
dostały tę samą czerwień w `runs_left_over_are_reconciled.rs:387` — pliku, którego żadne z nich nie
posiada (`git log <baza>..<gałąź> -- <plik>` pusty dla obu). Kosztowało to dwie rundy recenzji
i dwie rundy naprawcze. Naprawione osobnym commitem `4fcab5c` (stała przed instrukcjami);
`clippy --all-targets clean over 68 Rust files`. **Wniosek dla pętli: commit wchodzący na main
poza `integrate.sh` nie przechodzi warstwy `full` i trunk może być czerwony przy zielonym quick.**

**T-89 w main** (`integrate.sh`, bramka 15/0 w 45,8 s). Kafelek „sprawdź" da się wreszcie postawić
z płótna: przycisk, własny panel (komenda, wzorzec przejścia, folder, co po porażce), czerwona
kropka przy braku wzorca, plus dowód po prawdziwym kliknięciu w `e2e/`. Do dziś ten rodzaj kroku
istniał wyłącznie w Ruście i przychodził tylko z importu — czyli jedyny węzeł, który mówi
**co się stało** zamiast **co agent powiedział** (D6, `00-SYNTHESIS` §2.1), nie miał jak trafić
na płótno.

**T-86 w main** (`integrate.sh`, bramka 15/0 w 154 s). Stanął był na dwóch sprawach, obie
rozstrzygnął właściciel tego samego wieczoru; opis obu niżej, bo mechanizm powtórzy się w tej
fazie jeszcze nieraz. Zapłacone: jedna runda kontraktu i dwie implementacje, bo pierwszy bieg
skończył się kodem 1 na czerwieni spoza własnego zakresu.

**Wznowienie było certyfikowane, nie darmowe.** `ship-task.sh` sam orzekł, że kryteria
przechodzą, a specyfikacje niosą komplet asercji z chwili, w której bramka udowodniła je
czerwonymi — czyli „to działająca implementacja, nie rozluźniony kontrakt" — i przeszedł prosto
do fazy implementacji. Drugi kontrakt nie został napisany ani opłacony.

### T-86, sprawa pierwsza: asercja równości kontra nowy blok

`product_path_end_to_end.rs:164` żąda, żeby prompt kroku był **równy** instrukcji, słowo w słowo:

```rust
assert_eq!(prompts, vec![WHAT_TO_DO.to_owned()],
    "the step's instructions have to reach the driver, once, word for word. …");
```

T-86 AC-1 żąda, żeby prompt **każdego** kroku agenta kończył się blokiem mówiącym, że ostatnia
wypowiedź jest tym, co krok przekazuje dalej. Oba zdania nie mogą być prawdziwe naraz.

Co jest ważne przy tej decyzji, i co sprawdziłem, zamiast zgadywać:

1. **Żadne kryterium nie woła tego pliku.** `grep "check:" tasks/*.md` nie wymienia go ani razu —
   to test regresyjny żyjący w scalonym celu `it`, sądzony wyłącznie przez `full-test`.
   Kolizja jest więc między **kryterium T-86** a **asercją bez kryterium**, nie między dwoma
   kryteriami.
2. **Zdanie, które ta asercja niesie, jest po T-86 nadal prawdziwe.** Instrukcja człowieka
   dociera do sterownika dosłownie i dokładnie raz — stoi na początku promptu, przed blokiem.
   Nieprawdziwa robi się wyłącznie **forma** asercji (równość całego promptu), nie jej treść.
3. **Defekt, który ta asercja złapała, zostaje złapany po każdej możliwej zmianie:** pusty
   `instructions` dalej daje prompt bez zdania człowieka.

Trzy wyjścia, w kolejności, którą rekomenduję:

- **(a)** Zamienić równość na „zawiera dosłownie, dokładnie raz, na początku" — zdanie asercji
  zostaje bez zmiany, defekt pustego promptu dalej czerwony. Wymaga dopisania
  `src-tauri/tests/it/product_path_end_to_end.rs` do OWNS T-86 z **wąskim mandatem** (skill §5c
  pozwala poszerzyć uprawnienia, nigdy kryteria; porównanie linii `## AC-`/`check:`/`expect:`
  przed i po jest wtedy obowiązkowe).
- **(b)** Zostawić asercję i zwęzić AC-1 do „krok, który ma następnika" — słabsze, bo krok
  końcowy też oddaje przekazanie, a to on najczęściej niesie wynik całego biegu.
- **(c)** Uznać, że blok nie wchodzi do promptu, tylko do `--append-system-prompt` — nie działa
  u Codeksa, który nie ma takiej flagi i dostaje system prompt doklejony do stdin.

Nie wybrałem sam, bo (a) rozluźnia asercję, którą napisano po prawdziwym incydencie, a §5 karty
orchestratora zabrania mi rozluźniać oracle, żeby przepuścić własną falę.

**Rozstrzygnięte: (a).** Poszerzenie zakresu, nigdy kryteriów, z dowodem mechanicznym w commicie
`6398ea5`: `diff` linii `## AC-`, `check:` i `expect:` między kontraktem certyfikowanym na gałęzi
a nowym dał **0 różnic** przy 9 liniach kryterialnych po obu stronach. W kontrakcie stoi wąski
mandat i **wypisane wprost obejście, którego nie wolno zrobić** — gołe `contains()` bez
`starts_with` i bez liczby wystąpień. Pisarz wykonał mandat co do joty: trzy warunki naraz
(`len() == 1`, `starts_with`, `matches().count() == 1`), zdanie asercji nietknięte słowo w słowo,
plus komentarz nazywający ten sam defekt, po którym asercję napisano.

### T-86, sprawa druga: `giveUpAfterMinutes: 0` nie znaczy „bez limitu"

Znalazł to **recenzent Codeksa** (`gpt-5.6-sol`), na zielonych kryteriach — dokładnie ten
mechanizm, dla którego D3 wymaga cross-vendora:

> AC-2's assertion accepts a false promise: `giveUpAfterMinutes: 0` is described to the agent as
> having no time limit, but the execution timer converts it to one minute with `.max(1)`.

Ma rację i to jest **defekt kontraktu, który napisałem**, nie implementacji. `plan_agent` liczy
`give_up_after_minutes.max(1) * 60`, więc `0` to dziś **jedna minuta**, a nie brak limitu.
Prompt mówiący „nie masz limitu" przy kroku ubijanym po 60 s jest gorszy niż brak zdania.

Dwa wyjścia: albo silnik zaczyna traktować `0` jako brak limitu (`run.rs` **jest** w OWNS T-86,
więc mieści się w zakresie, ale to zmiana zachowania poza literą kryterium), albo AC-2 przestaje
obiecywać brak limitu i prompt mówi prawdę o jednej minucie. Pierwsze jest lepsze dla produktu
(pole „0" w formularzu agenta oznacza dla człowieka „bez limitu"), drugie mieści się w kontrakcie
bez jego zmiany.

**Rozstrzygnięte: silnik.** `plan_agent` daje dziś `0 => Duration::MAX`, a liczba minut jedzie do
promptu **osobnym polem**, nie wyjęta z `Duration` — pisarz zauważył sam, że zdanie zbudowane
z `Duration::MAX` obiecywałoby agentowi pięćset osiemdziesiąt cztery tysiące lat. AC-2 nie
zmieniło się ani o słowo; zmieniło się to, czy jego zdanie jest prawdziwe.

**Recenzent Codeksa zgłosił przy drugim biegu dwie dalsze uwagi i obie są o SILE WYROCZNI, nie
o kodzie** — zapisuję je, bo są prawdziwe i nikt ich dziś nie egzekwuje: AC-2 dowodzi wyłącznie,
że krok bez limitu przeżywa jedną wirtualną godzinę, więc implementacja zamieniająca zero na
dowolny skończony limit powyżej godziny też by przeszła (zbudowana jest `Duration::MAX` —
sprawdzone w kodzie, nie w teście); a AC-1 czyta blok jako „wszystko od pierwszego znacznika do
końca promptu", więc nie wykryłaby tekstu doklejonego ZA blokiem.

## 2026-08-22, 18:20 — T-79 w main: skille docierają do vendora, potwierdzone przez vendora

`131d214`. Bramka gałęzi 20/0, bramka trunka po lądowaniu 15/0 w 110 s.

Zbiór efektywny liczy się z agenta złożonego z nadpisaniem kroku; brak klucza znaczy „weź to,
co ma agent", `[]` znaczy żadnych, lista znaczy podzbiór skilli tego agenta. Nazwa spoza
zbioru zatrzymuje bieg **przed pierwszym procesem**, z nazwą brakującego skilla w zdaniu.
`RunSpec` nietknięty zgodnie z rozstrzygnięciem właściciela — wybór jedzie istniejącym szwem
dziedziczenia.

**Najmocniejszy dowód w tym biegu**: AC-3 uruchamia PRAWDZIWE Claude Code z tym samym
fragmentem argv, bierze linię `system`/`init` z transkryptu i przepuszcza ją przez
`place::discovery_from_init` — `Seen` dla obu wybranych, `NotSeen` dla trzeciego. Odpalone na
żywym CLI: 3,75 s, vendor ogłasza `<plugin>:alpha` i `<plugin>:beta`. To nie jest „napisaliśmy
pliki do katalogu"; to vendor mówi, że je widzi.

Cztery biegi zamiast jednego. Pierwszy padł, bo faza kontraktu napisała 33 KB specyfikacji bez
ani jednego `mod` w `tests/it/main.rs`; drugi i trzeci dowiozły resztę; czwarty przeszedł po
naprawie `before-spec-owns`. Obie przyczyny opisane niżej.

### Dwie rzeczy, które T-79 zostawia człowiekowi

1. **AC-3 jest naprawione w połowie.** Wyrocznia sięga do konta i sieci, więc musi być
   `#[ignore]`, a linia `check:` tego kryterium nie ma `--include-ignored`. Wzór: T-04 AC-6.
   Do czasu dopisania bramka dowodzi z dysku i manifestu, a dowód od vendora przechodzi się
   ręcznie: `cargo test --test it skills_reach_claude:: -- --ignored`. Cena dopisania jest
   realna: każdy bieg bramki zaczyna kosztować wywołanie vendora i wymagać sieci.

2. **AC-5 dowodzi, że callback działa, gdy się go zawoła — nie że Start go woła.** Pisarz
   odmówił naprawy przez skrót i nazwał powody: `go()` czyta `choices` wypełniane przez
   `useEffect`, którego `renderToStaticMarkup` nie uruchamia; DOM-u nie ma, bo vitest biegnie
   w `node`, a jsdom, happy-dom, `@testing-library` i `react-test-renderer` nie leżą
   w `node_modules`; Playwright odpada, bo `e2e/` jest poza OWNS tego zadania. Wybór: devDependency
   na środowisko DOM plus zmiana linii `check:`, albo przeniesienie kryterium do `e2e/`.

## Szósty defekt harnessu: `before-spec-owns` nie umiał rozwiązać celu `it`

`e9ddaae`. `CARGO_TARGET` w tym pliku był jednogrupowy, więc `--test it <modul>::`
rozwiązywało się na `src-tauri/tests/it.rs` — plik, który nie istnieje. `harness/gate.py:326`
ma na tę samą składnię regex dwugrupowy. Piąty konsument tej składni czytał ją inaczej niż
cztery pozostałe.

Skutek: składni używa **56 plików zadań**. Przy zadaniu czysto rustowym check wypadał przez
furtkę „the specs do not exist yet" z kodem 0 — milczał tam, gdzie miał sądzić. Przy mieszanym
sądził sam front i oskarżał kontrakt na pusto.

Kontrola pozytywna po naprawie: przegląd wszystkich kontraktów — 81 sądzonych i zielonych,
6 milczących, **0 czerwonych**. Kontrola negatywna: OWNS na `engine/limits.rs` z kryterium
w `store_pragmas::` daje 1 i wypisuje poprawnie rozwiązaną ścieżkę.

**Znalezisko przy okazji, nienaprawione:** rozróżnianie tego checku jest słabe. Pierwsza wersja
kontroli negatywnej PRZESZŁA, bo spec magazynu trafił w symbol `Result` z plików OWNS. Filtr
odrzuca nazwy do trzech znaków, więc zwykłe angielskie słowa przeciekają. Zaostrzenie wymaga
pomiaru na 81 kontraktach — **czeka na człowieka**.

## T-77 stoi na jednej decyzji projektowej

Oba własne kryteria zielone: Import JEST siódmą sekcją, otwiera się, Agenci przestali być drogą
do niego. Padło `shell-matches-mockup`: powłoka ma siedem przełączników, `docs/mockup/index.html`
ma sześć, a makieta jest wyrocznią nawigacji („a different set here is a different product,
not a different style"). **Czeka na człowieka: czy makieta dostaje siódmą pozycję „Import".**
Gałąź `task-T-77` gotowa, dwanaście plików, nic poza OWNS — pisarz uderzył w ścianę i stanął,
zamiast sięgnąć poza zakres.

Mój błąd w autorstwie kontraktu: naliczyłem pięć plików kodujących listę sekcji, bo znalazłem
je gerpem po nazwach. Szósty, `shell-matches-mockup.test.tsx`, nie wymienia ich wcale —
wyprowadza je z makiety.

## 2026-08-22, 15:13 — T-75 w main, T-76 cofnięte pomiarem, cztery defekty harnessu, osiem nowych kontraktów

Właściciel polecił zacommitować zastaną pracę, wyładować gałęzie importu i zacząć budowę
domknięcia importu setupu. Wykonane wszystko poza wyładowaniem T-76, które **cofnięte**.

**Zastana praca T-34 zacommitowana bez pętli.** 62 pliki (+7939/-793) leżały na main
niezacommitowane: dowody biegu, allowlistowany raport diagnostyczny i obrazy wklejane do
rozmowy Lead. Powstały bezpośrednio na trunku — bez worktree, bez czerwonego `before`, bez
drugiej opinii. Nie da się tego odtworzyć wstecz, więc jest to zapisane w commicie `800ebc3`
zamiast udawać zwykłą drogę. Jedyny dowód jest zewnętrzny wobec kryteriów T-34: pełna bramka
zielona. **Nikt nie sprawdził, że sześć kryteriów T-34 jest czerwonych bez implementacji** —
czyli nie wiadomo, czy cokolwiek mierzą. To zostaje otwarte.

**T-75 wylądowane** (`9564616`). Cztery konflikty, wszystkie tego samego kształtu: T-34 i T-75
dokładają do tych samych typów dwa równoległe, dyn-safe szwy z domyślnym `None` —
`with_evidence` i `configured`. Rozwiązane sumą. Jedno miejsce wymagało decyzji: w
`commands/run.rs` oba opakowania oddają KLON sterownika, więc kolejność (Connections →
dziedziczenie → dowody) jest wymuszona i milcząca; odwrócenie kompiluje się i cicho gubi
`--mcp-config` albo plik dowodu. Powód stoi w komentarzu przy tych liniach.

**Dwie wady, których git nie zgłosił jako konflikt.** `lib.rs:299` — automatyczny merge skleił
dwa ogony jednego komentarza blokowego i plik przestał się parsować, z meldunkiem
„Auto-merging". Trzy literały struktur w testach `codex.rs` bez nowego pola (E0063). Obie
znalazł dopiero `cargo check --all-targets --keep-going`; bez `--keep-going` druga wyszłaby
po naprawie pierwszej.

**T-76 wylądowane i cofnięte** (`bdc622b`, revert `7e77548`). Bramka po merge'u czerwona:
`full-test` 15/1, dwa testy z `setup-is-real.test.tsx`. Przyczyna to kolizja kontraktów, nie
wada merge'a: T-75 AC-10 obiecuje „człowiek uruchamia Scan, widzi cztery statusy, wszystkie
blockery", a T-76 zamknął całą tabelę za `preview.analysis === undefined ? null :`. Kryterium
T-75 uruchomione NA GAŁĘZI T-76 daje `2 failed | 2 passed` — regresja przyjechała z gałęzią,
a bramka gałęzi jej nie zobaczyła, bo tam biegł tylko `verify.sh quick`.

>>> T-76 WYMAGA RUNDY NAPRAWCZEJ: tabela ma być widoczna po Scan, a sekcja analizy ma się do
niej DOKŁADAĆ. I uwaga przy ponownym lądowaniu: git uznaje T-76 za wmergowany, więc samo
`./integrate.sh T-76` wciągnie wyłącznie commity po reverke i cicho cofnie resztę. Najpierw
`git revert 7e77548`, dopiero potem merge. <<<

## Cztery defekty harnessu znalezione po drodze

1. **`quick-permissions` wychodziło 2 na czystym main** — `T-75 owns AGENTS.md, but
   Edit(AGENTS.md) forbids it`. Deklaracja była martwa (zero plików przez dwanaście commitów
   gałęzi), zdjęta w `a8818ce`. `integrate.sh` ma jawną obronę przed lądowaniem na kodzie 2,
   więc T-75 i tak by nie weszło — z komunikatem wyglądającym na winę gałęzi.

2. **Strażnik N-08 był czerwony od 2026-08-16** (`abe8f02`). Wołał
   `refresh_harness_from_trunk` bez `ID`, a ta funkcja mrozi `tasks/$ID.md` — przy pustym `ID`
   mroziła `tasks/.md`, czyli nic, i to cicho, bo `git diff --quiet` na nieistniejącej ścieżce
   jest prawdą. Zmierzone na wyekstrahowanej funkcji: bez ID `contract v2`, z ID `contract v1`,
   oracle `new oracle` w obu. Mechanizm produkcyjny był sprawny; nieaktualne było wywołanie.
   Skąd: `caf976c` zawęził zamrożenie do własnego pliku zadania i tknął wyłącznie ship-task.sh.

3. **Strażniki biegną wyłącznie w `scripts/ci.sh`, a `integrate.sh` woła `verify.sh`.** Bramka
   gałęzi i bramka lądowania ich nie znają, więc każde lądowanie przechodziło ponad czerwienią,
   której żadna z nich nie widzi. To jest decyzja o tym, gdzie mają mieszkać strażniki —
   **czeka na człowieka**.

4. **`quick-scope` ma strażnika, który pudłuje, i cztery sprawdzenia nie mają go wcale.**
   Po naprawie N-08 bramka doszła wreszcie do etapu guards: 10 strzeliło poprawnie, 1 spudłował,
   4 bez strażnika (`before-spec-owns`, `quick-invoke-args`, `quick-tests-listed`,
   `quick-wired`). Pudło: po zdjęciu zasadzonego naruszenia `quick-scope` nadal świeci przez
   `.claude/settings.local.json` i `.claude/worktrees/` — nieśledzone, sprzed tej sesji.
   **Łatwa naprawa jest oszustwem i nie została wykonana**: dopisanie ich do `GENERATED`
   oślepia sprawdzenie na plik, który NADAJE UPRAWNIENIA (`allow: Bash(ps -eo pid,command)`),
   czyli osiąga od drugiej strony dokładnie to, przed czym broni wyłączony `.gitignore`.
   Właściwa naprawa: strażnik ma dowodzić, że sprawdzenie REAGUJE na zasadzone naruszenie,
   a nie że jest zielone w tym środowisku. To zmiana w `harness/guards.sh` dotykająca
   wszystkich jedenastu strażników — **czeka na człowieka**.

Piąte, drobniejsze: `integrate.sh` umie rozwiązać konflikt TREŚCI `TASK.md`, ale nie
SKASOWANIA — a skasowanie robi sam, trzydzieści linii niżej. Każde lądowanie po tym, w którym
TASK.md zniknął z trunka, trafi w `error: path 'TASK.md' does not have our version`.

## Osiem kontraktów na domknięcie importu (`c05bb6b`)

T-77 ekran importu jako sekcja paska · T-78 typowany model i receipt · T-79 skille docierają
do vendora · T-80 pamięć per agent · T-81 MCP: parsery i pętla zwrotna · T-82 rekonstrukcja
workflow · T-83 reimport i naprawa · T-84 tabela Skills.

Trzy rzeczy zmierzone w kodzie, które zmieniły podział wobec planu właściciela:
`connections::runtime` już odmawia startu dla wyłączonego połączenia (dwa kryteria z planu
byłyby zielone w `before`); `RunSpec` nie ma `Default` i konstruuje go 31 plików, więc nowe
pole to fala, a nie linia; siódma sekcja paska kosztuje pięć plików powłoki, z czego trzy są
cudzymi kryteriami.

Stan na teraz: main zielony (15/0, 92,71 s), T-77 biegnie przez `ship-task.sh`.

## 2026-08-21, 13:57 — T-74 w main i uruchomione; Linear ma pełną drogę konfiguracji

Właściciel odrzucił ręczne tworzenie JSON-u po T-65 i polecił zbudować najpierw prawdziwy
connector Lineara. Ekran Triggers prowadzi teraz przez Create/Edit/Delete: wybór Lineara,
jednokierunkowe podanie klucza, prawdziwą listę workflow oraz sprawdzanie co 1, 5, 15 albo
60 minut. Przy cadence ekran mówi wprost, że sprawdzanie działa tylko przy otwartym Loadoucie.
`Test connection` wykonuje osobne zapytanie `viewer`; nie uzbraja triggera i nie zapisuje
kursora, kolejki ani biegu.

**Granica sekretu i zapisu.** Okno nigdy nie dostaje klucza ani jego pochodnej. Rust tworzy
plik jako 0600 przed pierwszym bajtem, publikuje Create bez nadpisania i odmawia stale Edit.
Puste pole edycji zachowuje najnowszy klucz z pliku, wpisany zastępuje go jawnie. Pliki T-65
z `condition: "assigned to me"` i bez cadence nadal się ładują, ale nowe zapisy używają wyłącznie
`assigned-to-me`. Nie twierdzimy, że to Keychain albo szyfrowanie at rest.

**Delete nie ściga się ze Startem.** Pending jest trwale kończone jako Cancelled przed ukryciem
konfiguracji. Bound oznacza, że Start już wiąże bieg, więc Delete odmawia przed jakąkolwiek
mutacją i pokazuje człowiekowi, żeby poczekał; rozpoczęty bieg zatrzymuje Stop. Crashowe pliki
tymczasowe i tombstone mają czytelnika, blokadę per katalog+slug oraz bariery fsync. Niezależny
audyt zakończył się `none` osobno dla frontendu i Rusta.

**Paragon.** Formalne `before` uruchomiło 8 kryteriów i wszystkie były czerwone z właściwego
powodu w 3,60 s. Późniejsze wzmocnienia miały własne celowane czerwienie: między innymi Delete
dla Bound 5/1, symlink korzenia 5/1, współbieżność curl 3/2, publish ledger-temp 7/1 i legacy
condition 0/1. Końcowe kryteria: frontend 31/31, Rust AC-3 10/10 po mechanicznym podziale testu,
AC-4 5/5, AC-7 7/7; sąsiedzi T-65 3/3, 7/7 i 27/27. Pełny rerun miał zielone wszystkie
21 sprawdzeń kodu w 23,79 s, w tym `full-clippy` i `full-test`.

Właściciel jawnie zezwolił usunąć sprzeczny deny dla posiadanego `src-tauri/Cargo.toml`;
`quick-permissions` wróciło do zieleni w `c484b6f`, a T-74 weszło do main jako `81337c2`.
Przed merge'em trunk przeszedł 15/0. Po merge'u bramka dwa razy zatrzymała się wyłącznie na
starszym `workspace_global_slots`: w pełnej równoległej suicie zmierzył peak 2 zamiast 3.
Ten sam test przechodzi osobno (1/1), przeszedł w bramce gałęzi i trunka przed merge'em, a pełne
`cargo test -- --nocapture` po merge'u także przeszło; zwykłe `cargo test` odtwarza peak 2.
Test i jego kontrakt leżą poza OWNS T-74, więc nie zostały cicho osłabione. Merge pozostaje
w main zgodnie z zachowaniem `integrate.sh`, a aplikacja została zbudowana i uruchomiona z tego
SHA. Druga opinia Claude była niedostępna (`api_error`), więc `review.sh` zwrócił 0 jako advisory.
Żywego wywołania Lineara nie wykonano, bo w bramce nie ma klucza; produkcyjny przycisk jest
gotowy do takiego testu.

## 2026-08-21, 09:22 — T-65 gotowe na gałęzi, pełna bramka zielona

Właściciel polecił zaplanować i wykonać T-65 oraz oddał wybór rozwiązania agentowi. Powstał
trwały ledger dostaw pod `~/.loadout/triggers/`, UUID v7 przydzielony przed Startem i pierwszy
atomowy oraz zsynchronizowany `run.json` jako chwila akceptacji. Rozwiązanie nie ufa
`RunState.workflow`: o zajętości decyduje `AppState.live`, a wyścig z ręcznym Startem zostawia
ten sam pending i UUID do ponowienia. SQLite pozostaje indeksem; nadal nie ma daemona, wielu
żywych biegów ani `stop_run(id)` z Etapu B T-71.

**Paragon kontraktu.** Pierwsze `before`: 9/9 kryteriów czerwonych z właściwego powodu w 6,68 s;
po wzmocnieniu recovery AC-8 osobny `before` dał 2/2 w 4,17 s. Końcowy `quick` po aktualizacji
makiety: 21/0 w 9,64 s. Pełna bramka nie została uznana po dwóch częściowych przebiegach: pierwszy
znalazł lint w teście i utratę komunikatu T-64, drugi stare pięciosekcyjne lustro makiety. Po
naprawach oraz jawnej zgodzie właściciela na dodanie `docs/mockup/index.html` do OWNS finalny
`full` dał 23/0 w 29,81 s.

Niezależne audyty przed bramką znalazły i dostały regresje między innymi dla: wszystkich nowych
spraw zamiast tylko najnowszej, local Pending przy niedostępnym Linearze, crashy przed
`run.json`, osieroconej administracji worktree, symlinków w artefaktach biegu oraz ponownego
`fsync` pliku i katalogu przed recovery-acceptance. AC-8 kończy z 27 testami. Druga opinia Claude
była niedostępna (`api_error`); `review.sh` zgodnie z kontraktem zwrócił 0 jako advisory, więc nie
powstał plan rundy naprawczej. Pozostało lądowanie i pełna bramka na trunku.

## 2026-08-21, 01:53 — trzy urwane sesje rozliczone: T-71 i T-64 w trunku, T-40 wycofane, T-65 uczciwie wstrzymane

**Wyladowane: T-71 i T-64.** T-71 przeszlo 20/0 na galezi i 15/0 na trunku; po znalezieniu
urwanej uwagi recenzenta jego AC-4 zostalo w tej samej rundzie wzmocnione o drugie klikniecie
`+` przy juz otwartym terminalu, potem ponownie 20/0 na galezi i 15/0 po ladowaniu. To odroznia
„0 kart → 1" od prawdziwej obietnicy wlasciciela: kolejne terminale w tym samym zakresie nie
podmieniaja poprzednich.

W T-64 wszystkie szesc kryteriow bylo w `before` czerwonych z wlasciwego powodu; potem `quick`
dal 19/0, a `full` 21/0. Klucz Lineara jedzie w konfiguracji `curl --config -` na stdin,
srodowisko jest wyczyszczone do `PATH`, odpowiedz GraphQL jest deserializowana permisywnie wobec
obcych pol, a kursor pod `~/.loadout/triggers/` jest zapisany atomowo przed oddaniem trafienia.
Awaria zapisu jest odmowa, nie trafieniem. Druga opinia Claude byla niedostepna (`api_error`),
co zgodnie z kontraktem review jest notatka advisory i `exit 0`, nie blokada. Po nalozeniu na
T-71 pelna bramka zostala powtorzona (21/0), a trunk po ladowaniu dal 15/0.

**T-40 wycofane pomiarem.** AC-1 przeszlo mimo dwoch celowych martwych handlerow zasadzonych
w produkcji, a AC-2 nie skonczylo sie w 40,42 s. Pierwsze nie widzi obiecanego naruszenia,
drugie nie uruchamia sadu; oba sa zakazanymi pseudo-czerwieniami z AGENTS.md §2a. Galaz
`task-T-40` zostaje jako paragon i nie jest integrowana — niesie mutacje kontrolne, nie naprawy.

**T-65 wstrzymane przed `before`.** `RunState.workflow` klamie po odmowie drugiego startu, ale
zastapienie go samym `ALREADY_GOING` zostawia wyscig: T-64 przesuwa kursor przed Startem, wiec
odmowa po trafieniu zjada sprawe. Przesuniecie kursora po Starcie dubluje ja po awarii miedzy
akceptacja a zapisem. Rozstrzygniecie: potrzebna jest trwala tozsamosc i chwila akceptacji biegu
po stronie Rusta, z ktora da sie zwiazac trafienie i odtworzyc decyzje po restarcie. Etap B jest
nazwany w T-71, ale nie ma pliku zadania; T-65 nie obchodzi tej luki stanem okna. AC-2 i AC-6
zostaly juz poprawione pod niezmiennik 29: przyszly sad ma renderowac prawdziwy ekran.

**Brudny trunk zachowany, nie przepchniety.** Dziewiec plikow z urwanych sesji lezy w commicie
`1fdbefd` na `rescue/2026-08-21-three-sessions`. Czesc rozmowy byla starsza i wezsza od T-71
(watek per zakres zamiast per terminal), wiec zostala zastapiona przez wyladowane T-71. Cztery
pliki wlaczajace `CodexDriver` w aplikacji sa sensowne, ale nie naleza do OWNS T-10 ani zadnego
innego istniejacego zadania; pozostaja zachowane, nie w trunku.

**Trzy luki runtime z pierwszej sesji pozostaja znaleziskami, nie cichymi poprawkami:** produkcja
nie wola `ClaudeDriver::with_transcript`, `copies > 1` nadal nie rozwija krokow, a limit czasu nie
jest widoczny agentowi i przy ubiciu gubi `cost_usd`/`summary`. Zadnego odpowiadajacego pliku
`tasks/<ID>.md` nie ma. AGENTS.md §2 zabrania wymyslenia zadan w przelocie, a §7 zabrania wejscia
w szwy przypisane innym taskom. Handoff zalacznika, ktory ujawnil te luki, jest juz w trunku
(`693f894`, poprawka full-clippy `209ba7f`).

**Dowod, ktorego T-64 swiadomie nie ma:** nie wykonano zywego zapytania do Lineara, bo w repo i
w bramce nie ma klucza. Do sprawdzenia reka po skonfigurowaniu pierwszego triggera: czy zapytanie
GraphQL jest przyjmowane przez aktualne API. Drugi jawny dlug T-64: budowniczy bezpiecznego `curl`
jest teraz drugi obok `skills::ingest`; wspolny rdzen wymaga osobnego zadania z OWNS obu stron.

## 2026-08-20, 07:10 — biurko rozliczone: trzy zadania w trunku, niezmiennik 29, trzy decyzje w kontraktach

**Wyladowane: T-68, T-69, T-70.** Pelna bramka po kazdym, 15/0. Do tego **niezmiennik 29**
w karcie pracy, **trzy decyzje produktowe** zamienione w kontrakty (T-70, T-71, T-72)
i **T-73 wycofane po pomiarze**.

### Niezmiennik 29 — kryterium asertuje zdanie tam, gdzie czlowiek je widzi

Wszedl na wyrazne polecenie wlasciciela, po tym jak recenzent zlapal te klase CZTERY RAZY na
zielonej bramce w jednej fali. Regula nie zada niemozliwego w repo bez jsdom i mowi to wprost:
czysty modul dowodzi TRESCI, `renderToStaticMarkup` obecnosci na prawdziwej sciezce,
`e2e/harness.ts` dojscia po prawdziwym kliknieciu. Wolno wybrac jedno z trzech; nie wolno
poprzestac na wartosci zwroconej przez funkcje, ktorej nikt nie wola.

**Regula od razu zaczela pracowac.** Recenzent T-70 zlapal, ze kryteria wolaja `Threads::say`
wprost, a **zywa aplikacja `Threads` nie konstruuje w ogole** — `AppState.chat` to nadal
`Mutex<Option<Chat>>`. Biblioteka dla lidera byla wiec dowiedziona na typie, ktorego produkt
nie wola.

### Blokada, ktora postawil orchestrator, i ktora zdejmuje T-71 AC-5

Przyczyna tamtego stanu NIE jest wada pisarza i to jest wazniejsze niz sama naprawa. Pisarz T-60
opisal go co do zdania (`ipc.rs`, „WATEK PER ZAKRES ISTNIEJE I NIE STOI TUTAJ"): `Threads::say`
wymaga wskazanego lidera, wskazania nie ma czym dowiezc z okna, bo wymagaloby klucza obok
`folder` w `io.ts` — a **moj mandat na tamten plik pozwalal dopisac wylacznie `folder`**.
Odmowil podstawienia polowy i mial racje: rozmowa zakladajaca nowy watek przy kazdym zdaniu
bylaby gorsza od tej, ktora stoi.

Blokada jest wiec granica orchestratora, nie modelu, i dlatego zdejmuje ja zadanie, ktore posiada
wszystkie trzy pliki. **Nauka: waski mandat na cudzy plik potrafi zablokowac podpiecie, ktore
jest CALYM sensem zadania. Kiedy go stawiasz, sprawdz, czy zadanie da sie wtedy skonczyc.**

### T-73 wycofane, bo wada byla zamknieta I PILNOWANA

Kontrakt na sklejanie wierszy przechodzacych przez koniec biegu zeszl z „PASSES before
implementation" na obu kryteriach. Zamiast zgadywac, zmierzylem mutacja: zdjecie `groups.clear()`
z `runEnded` zapala `nothing-live-survives-the-run.test.ts > closes the open fold windows, so the
next run cannot grow the last row of this one`; po przywroceniu 7 passed. Czyli pisarz T-68
przewidzial te wade i pokryl ja kryterium **w tym samym biegu**, a recenzent czytal kod, ktory
juz ja zamykal.

**Wzor do zapamietania:** „zielone before" nie odroznia „zachowanie istnieje" od „test jest
zepsuty". Kiedy oba kryteria swieca zielono przed implementacja, mutacja odpowiada w 30 sekund,
a lektura nie odpowiada wcale.

### Trzeci raz: limit konta wyglada jak zly kontrakt

T-72 zeszlo rc=1 z „did not RUN" na wszystkich czterech kryteriach i galezia zawierajaca
**wylacznie commit kontraktowy**. To ten sam podpis, co dwa razy wczesniej tej nocy.
Rozpoznanie jednolinijkowe: `git log main..HEAD` na galezi pokazuje jeden commit zamiast kilku.
Po resecie wznowione bez zmiany ani jednego znaku w kontrakcie.

### Co czeka

| co | stan |
|---|---|
| **T-72** — procesy, ktore Loadout trzyma (`/start`, kafelek w szynie, kill z dowodem) | wznowione |
| **T-71** — plusik otwiera terminal + AC-5 (zywa komenda przez rejestr watkow) | po T-72, dzieli `ipc.rs` i `io.ts` |
| T-40, T-41, T-45, T-56 | starsza kolejka, nietkniete |
| T-64, T-65 | triggery Lineara, druga fala |

**Etap B dla terminali** (biegi rownolegle: tozsamosc biegu na drucie, `stop_run(id)`, rejestr
zamiast jednego `AppState.live`) nie ma jeszcze kontraktu. Jego warunkiem wstepnym byl T-69
i ten juz stoi w trunku.

## 2026-08-20, 05:40 — terminal, lider i siedem zadan w trunku

**Wyladowane: T-58, T-66, T-67, T-60, T-61, T-62, T-63.** Pelna bramka po KAZDYM ladowaniu,
15/0 za kazdym razem; na galeziach przed ladowaniem T-58 20/0, T-60 19/0, T-61 19/0, T-62 18/0,
T-63 19/0, T-66 17/0, T-67 17/0.
**T-59 wycofane w trakcie.** Fala wziela sie z rozmowy z wlascicielem, nie z planu.

### Ladowanie stalo godziny na CUDZEJ niezacommitowanej pracy, i jak zostalo zdjete

`integrate.sh` odmawia lądowania na brudnym drzewie i ma racje. W drzewie glownym leza od
kilku godzin trzy pliki CUDZEJ, niezacommitowanej pracy (`commands/run.rs`,
`memory/handoff.rs`, nowy `tests/handoff_attachment_is_openable.rs` — zalaczniki przekazan).
Rozwiazanie: **zmierzyc, zanim sie ruszy.** `./verify.sh quick` dalo 13/0, a `cargo test
--test handoff_attachment_is_openable` 1 passed — praca byla wiec SKONCZONA i dala sie
zacommitowac jako wlasny commit. Nic nie zginelo: `git reset --soft HEAD~1` cofa ja jednym
ruchem. `git stash` bylby gorszy, bo znika wtedy z drzewa robota, ktorej autor jest w trakcie
zadania.

**I tu wpadla pulapka warta zapisania.** Ta praca przechodzila `quick` (`--lib`) i swoj wlasny
test, a mimo to zostawiala trunk CZERWONY: `full-clippy` sadzi `--all-targets`, czyli takze
`tests/`, i jedno `redundant closure` przy `-D warnings` zatrzymalo cala fale. `integrate.sh`
zameldowal to dokladnie tak, jak trzeba — czerwien na main PRZED jakimkolwiek merge'em, nic nie
wyladowane, zeby wina nie spadla na pierwsza galaz. Naprawa: jedna linia,
`.filter_map(Result::ok)`. **Zielony `quick` plus zielony wlasny test NIE znaczy, ze trunk
przyjmie.**

**Nauka operacyjna:** drugi agent pracowal NA TRUNKU, nie w worktree. Przy dwoch agentach na
jednym repo to zatrzymuje lądowanie calej fali. Kazda praca — takze jego — potrzebuje pliku
zadania z blokiem OWNS, bo blok OWNS jest jedynym zamkiem, jaki to repo ma.

### Stos zamiast czekania — brudny trunk nie musi zatrzymywac budowy

Repo ma na to gotowy mechanizm i tej nocy zostal uzyty pierwszy raz na serio: `FROM=` w
`worktree.sh` odbija galaz od wskazanej bazy, a `LOADOUT_TRUNK=` ustawia zakres, po ktorym
sadzi `quick-scope`. Trzy fale poszly na stosie:

    main -- task-T-58 -- task-T-66 -- task-T-67          (front)
    main -- task-T-60 -+- task-T-61                      (lider)
                       +- task-T-62
                       +- stack-T-63 (T-60+T-61+T-62) -- task-T-63

**Trzy pulapki stosu, kazda zmierzona:**

1. **Worktree z bazy nie widzi plikow zadan zacommitowanych na main.** Kontrakt trzeba najpierw
   domergowac do bazy, inaczej bieg nie ma czego zamrozic.
2. **Rozszerzenie kontraktu wciagniete do galezi merge'em z main wyglada dla bramki jak zapis
   poza zakresem.** `quick-scope` sadzi CALA galaz wzgledem bazy, wiec zmieniony `tasks/<ID>.md`
   jest „plikiem spoza OWNS", choc zmienil go orchestrator. Harness robi to u siebie poprawnie
   (`refresh_harness_from_trunk` przywraca po merge'u wylacznie wlasny plik zadania) — recznym
   merge'em ten krok sie pomija. Poprawna kolejnosc: **baza do trunku, galaz do bazy, plik
   zadania z bazy, dopiero potem `TASK.md`**.
3. **Baza zlozona z dwoch galezi konfliktuje o `TASK.md`** — kazda niesie swoj zamrozony
   kontrakt pod ta sama sciezka. W bazie `TASK.md` musi ZNIKNAC, inaczej swiezy worktree rodzi
   sie w trybie wznowienia i sadzi sie cudzym kontraktem.

### T-59: kontrakt byl zly i wykrylo to dopiero uruchomienie

Mial wpuscic `WebSearch`/`WebFetch` na kazdy szczebel `Policy`, zeby lider do researchu nie
wymagal oddania calej maszyny. Zapowiedziana cena byly dwa napisy w `claude_argv_policy.rs`.
Prawdziwa: `driver_claude_policy_surface.rs:171` trzyma `editing.is_subset(&unlimited) &&
editing != unlimited`, a po przeniesieniu sieci w dol `Unrestricted` nie dokłada do `--tools`
niczego wlasnego — obie listy sie zrownuja. Zmierzone: **401 passed / 3 failed**, czerwien poza
OWNS. Kryterium T-53 jest DOBRE (ostre zawieranie lapie adapter drukujacy jedna liste dla trzech
polityk), wiec bieg zatrzymany, grupa ubita z dowodem ESRCH, specyfikacje (818 linii) zachowane.
Zamiennik — **T-63** — robi to per agent, wiec agent domyslny sklada argv co do bajtu jak dzis
i zaden wyladowany straznik nie przestaje byc prawdziwy.

### Recenzent w SLABSZYM trybie zlapal szesc defektow na ZIELONEJ bramce

Ten sam vendor, inny model, rola recenzenta. Zaden z tych szesciu nie byl widoczny dla
zadnego z moich kryteriow:

1. **Widmowy agent w szynie.** `roster.ts` bije kafelek na kazde odrebne `row.agent`; po T-58
   kazda komenda sklada wiersz podpisany oknem, wiec pierwsze `/stop` sadza agenta „working"
   na zawsze. -> **T-66, zielone.**
2. **Widmowy wiersz w strefie TERAZ.** Ta sama linia idzie do mapy `doing`, a `now.tsx` nie
   bramkuje listy wierszy propsem `live`. -> **T-67, zielone.**
3. **Przypiete pytanie przezywa bieg.** `runEnded` nie gasi `waiting`, wiec karta „Needs your
   answer" wisi po biegu i dalej daje sie kliknac. -> **T-68, napisane.**
4. **Druga tabela `FileAccess` -> `Policy`.** T-60 nie posiadalo `run.rs`, wiec lider dostal
   reczna kopie tabeli, a pisarz ZAPISAL w komentarzu, ze wymog jest niespelniony. -> **T-63 AC-4.**
5. **Przycisk propozycji martwy w aplikacji.** Renderowal sie tylko z propsem `command`, ktorego
   `HistoryRow` nie mial, a produkcyjni wolajacy nie podawali. Kryterium zielone, funkcja
   nieistniejaca. -> naprawione w T-61 po rozszerzeniu OWNS.
6. **Start osieroca agenta z `/ask`.** `begin_a_run` dostalo warunek, `begin_run` nie — a wola
   je Start, `/run` i zielony Run. Osierocony agent pracuje i placi, Stop go nie dosiega.
   Zgloszone niezaleznie przez DWA rozne biegi recenzji. -> **T-69, napisane.**

### Wzor, ktory kosztowal trzy rozszerzenia OWNS

Pisalem bloki OWNS pod pliki, ktore zadanie ZMIENIA, i nie pod **lustra**, ktore o tej zmianie
musza sie dowiedziec. Trzy razy: nowy rodzaj wiersza przewrocil `feed/collapse.test.ts`
(dziewiec rozwinietych), nowy wariant na drucie tablice `KINDS: [LineKind; 16]`, nowa komenda
`commands-wired.test.ts`. Kazde lustro zachowalo sie poprawnie — wymusilo swiadoma decyzje
zamiast przepuscic ja po cichu.

**Regula na nastepne kontrakty:** zadanie dotykajace drutu (nowy rodzaj wiersza, nowa komenda,
nowe pole w `RunSpec`) dostaje swoje lustro w OWNS od razu, z mandatem waskim do jednego wiersza.

Wszystkie trzy rozszerzenia poszly procedura §5c z dowodem mechanicznym: linie `## AC-`,
`check:` i `expect:` porownane miedzy zamrozonym `TASK.md` i nowym kontraktem, za kazdym razem
identyczne co do znaku.

### Limit uzycia konta wyglada jak zly kontrakt

Trzy biegi zeszly naraz z „did not RUN (No test files found)" i galeziami zawierajacymi WYLACZNIE
commit kontraktowy. Bramka nazwala to wada kontraktu, bo nie ma czym odroznic „kontrakt jest zly"
od „agent nigdy nie odpowiedzial". Rozpoznanie: zero plikow specyfikacji na trzech galeziach
jednoczesnie. Po resecie limitu te same kontrakty przeszly bez zmiany ani jednego znaku.
**Wniosek operacyjny:** nie wiecej niz dwie fazy kontraktu naraz.

### `scripts/detach.py` jest w repo

Zginal dwa razy (19.08 i 20.08), za kazdym razem kosztem sesji, ktora go potrzebowala.
Zmierzone tej nocy: dziewiec biegow w czterech falach, zero zgubionych na granicy tury.

### Konflikt przy ladowaniu, ktory byl prawdziwy

`task-T-62` zderzyl sie z `entry/entry.tsx` przepisanym przez T-58: jedno zadanie przebudowalo
wiersz wejscia (historia strzalka, echo do strumienia, ognisko), drugie dolozylo do niego `/ask`.
Trzy hunki, rozwiazane addytywnie z zachowaniem architektury MLODSZEJ, bo ona jest na trunku.

Ostatnia pozostalosc znalazl `tsc`, nie ja: dwa wywolania `setSaid` przezyly merge, bo lezaly
POZA znacznikami konfliktu — T-58 skasowal ten stan, przenoszac odpowiedzi wiersza do strumienia.
Wniosek na przyszlosc: po recznym rozwiazaniu konfliktu w pliku, ktory ktos przepisal, `tsc`
jest tania kontrola przeciw pozostalosciom, ktorych `git` nie pokazal.

Drugi wniosek, tanszy: **kazda galaz stosu nosi swoj `TASK.md`**, a `integrate.sh` kasuje go przy
ladowaniu — wiec druga galaz w kolejce konfliktuje o ten plik. Zdejmuj `TASK.md` z galezi
PRZED ladowaniem, jednym commitem na kazda.

### Co czeka

| co | stan |
|---|---|
| **T-68** — koniec biegu gasi wszystko, co opisywalo zywy bieg (2) | napisane |
| **T-69** — zaden start nie osieroca poprzednika (2) | napisane, niezmiennik 6 |
| T-40, T-41, T-45, T-56 | starsza kolejka, nietkniete |
| T-64, T-65 | triggery Lineara, druga fala; dziela `ipc.rs` z T-60 i T-62 |

**Luka wymieniona, nie zamknieta:** AC-4(c) w T-61 wymaga, zeby zdanie odmowy „wracalo i bylo
pokazane", a testowana jest tylko polowa „wracalo" — bez jsdom `onClick` nie odpala sie w zadnym
tescie. Prawdziwe klikniecie sadzi wylacznie harness e2e (tak zrobilo T-58 AC-5). Ta sama luka
dotyczy `start-invokes.test.tsx` i jest w tym repo strukturalna, nie swieza.

## 2026-08-20, 00:20 — D6 ma trzeci rodzaj kafelka, i to byla decyzja czlowieka

**Wyladowane tej nocy: T-53, T-10, T-54, T-55, T-57.** Pelna bramka po kazdym, 15/0.
Strategia „harness jest nasz, dziedziczymy tekst" stoi w trunku w calosci:
`drivers/{codex,command,host}.rs` i `inherit/{scan,rewrite,wire}.rs`, plus `Step::Check`
w schemacie.

### Blokada, ktora zatrzymala T-55, i jak zostala zdjeta

T-55 skonczylo 5/5 kryteriow zielonych i utknelo na `harness_workflow_two_kinds` — wyroczni
AC-2 z T-23, ktora asertuje **rownosc** zbioru rodzajow, nie zawieranie, z komentarzem
napisanym wprost: *„trzeci rodzaj po cichu dolozony, zeby graf sie zmiescil, jest dokladnie
ta awaria, ktora to zadanie ma lapac"*. Krok „sprawdz" JEST trzecim rodzajem. Wyrocznia
zadzialala dokladnie tak, jak zaprojektowano.

**Pisarz nie oslabil asercji** — zostawil plik nietkniety i pozwolil mu pasc, a piec innych
plikow dostalo po JEDNEJ linii ramienia `match`, ktorej wymaga kompilator. To jest zachowanie,
o ktore chodzi w AGENTS.md par. 7, i dlatego zostaje odnotowane.

Rozstrzygnal czlowiek: **zmieniamy D6** (`94a0d23`). Regula „nie powtarzamy funkcji vendorow"
zostaje w mocy bez jednej zmiany — zaden vendor nie dostarcza „uruchom komende i sam orzeknij,
czy przeszla". Zmienil sie tylko limit liczbowy, ktory tej reguly nie wyrazal.

**Czego to nie otwiera, zapisane w D6, zeby nie stalo sie precedensem:** nie ma i nie bedzie
kafelka „recenzja" — etap nazwany w kodzie JEST domyslny i nie da sie go wylaczyc konfiguracja
(D7, niezmiennik 27). Wyrocznia T-23 dostala wlasnie ten rodzaj jako swoj nowy przypadek
odmowy, wiec regula jest **egzekwowana mechanicznie**, a nie tylko napisana.

### Jedna stala odpowiadala na dwa pytania

Przy okazji wyszlo, ze `KNOWN` w tej wyroczni znaczylo naraz „co zna schemat" i „czego uzywa
mierzony plik" — i moglo, dopoki odpowiedz byla ta sama. Po dolozeniu `check` przestala:
schemat zna trzy, a `ship-task.json` uzywa dwoch, bo etapy sprawdzenia i wejscia na trunk stoja
w nim NADAL na kafelku kontrolnym. Stala nazywa sie teraz `IN_THE_FILE` i pilnuje pliku, bo
asercja od poczatku byla o pliku. **Przepisanie `s_gate` i `s_land` na kroki sprawdzenia jest
osobna praca** i tak stoi w komentarzu.

### T-57: dlug po T-54 splacony, nie zamieciony

T-54 wyladowalo z czterema funkcjami bez konsumenta produkcyjnego (`plugin_dir`, `plugin_argv`,
`recurring_patterns`, `agent_body`) — wolanymi wylacznie z `tests/`, czyli z osobnych skrzyn,
w ktorych `dead_code` milczy. `quick-wired` zlapal to i zaoferowal dwa wyjscia; wybrane zostalo
drugie, ktore sam check opisuje jako „przeniesienie dlugu tam, gdzie ktos go widzi": napisane
**T-57** z czterema prawdziwymi kryteriami, ktore te funkcje wolaja. Wyladowalo tej samej nocy.

### Falszywa czerwien, ktora kosztowala jedno przejscie

T-57 zglosilo `full-test` czerwone z „vitest exited 0 and reports no passing tests / no Tests
line at all", przy 4/4 kryteriach zielonych. To bylo obciazenie maszyny (rownolegly bieg T-55),
nie defekt: ta sama galaz na spokojnej maszynie daje **152 pliki / 817 testow**. Rozpoznanie
jest jednolinijkowe — odpal `npx --no-install vitest run` na galezi i na trunku i porownaj.

### Dwa biegi zginely na granicy tury — i to jest naprawione

T-10 i T-54 zostaly ubite na twardym suficie 3600 s tla, oba w fazie recenzji albo poprawek,
czyli PO wykonaniu pracy. Zero osieroconych procesow (sprawdzone `ps` po `claude -p`).
Rozwiazanie: `scratchpad/detach.py` — podwojny fork + `setsid`, kod wyjscia do `<log>.rc`.
T-55 i T-57 poszly odczepione i przezyly. **Helper nie jest w repo** i przy nastepnej sesji
trzeba go napisac od nowa albo wpiac na stale.

## 2026-08-19, 22:20 — harness jest NASZ: dziedziczymy tekst, nigdy maszynerie

**Wyladowane: T-53 (4 kryteria) i T-10 (6).** Pelna bramka po kazdym: 15 sprawdzen, 0 czerwonych.
Do tego zamkniety spike **S-3** i **trzy naprawy harnessu**, kazda z kontrola w obie strony.

Pytanie wlasciciela brzmialo: co sie stanie, gdy Loadout odpali agentow w repo, ktore ma juz
WLASNY harness (mierzone na `../meetnotes`, ale to tylko przyklad). Odpowiedz jest zmierzona,
nie zalozona, i odwrotna do pierwszej hipotezy.

### Kierunek „wczytaj ustawienia gospodarza, odejmij haki" NIE ISTNIEJE

Zmierzone na 11 biegach `claude -p`: kazdy z `--setting-sources project` odpalil hak gospodarza
(7/7); `--settings <plik>` SUMUJE sie z projektowym i nie gasi hakow nawet podana pusta lista
`PreToolUse`; `--bare` gasi je kosztem OAuth (`Not logged in`), wiec na subskrypcji jest
bezuzyteczny. Zostaje kierunek odwrotny: **odetnij wszystko, potem odbuduj wiedze po swojemu.**

Cena wczytania jest twarda, nie estetyczna: **hak PreToolUse gospodarza startuje proces we
WLASNEJ grupie procesow, a jego dziecko dostaje ppid=1 i przezywa wyjscie `claude`.** Zmierzone:
jeden bieg zostawil 14 sierot, eksperymenty lacznie 30 zywych procesow ubitych recznie. Przy
zaladowanych ustawieniach gospodarza **niezmiennik 6 jest nie do utrzymania** — zabicie naszej
grupy nie dotyka ani jednej z tamtych.

Zmierzone ryzyko, ktore ta fala zamyka: nasz agent wywolal projektowego podagenta gospodarza
(`release-engineer`), ktory wystartowal jako osobny proces i spalil **38-41 tys. tokenow
calkowicie poza widokiem i rozliczeniem Loadouta**.

### Dwie rzeczy, w ktorych mylil sie research po drodze

1. **`--allowedTools` to lista AUTO-ZATWIERDZANIA, nie filtr dostepnosci.** `Task`/`Agent`
   i `Skill` sa dostepne w KAZDEJ z trzech polityk. Filtrem jest `--tools` — twarda biala lista, i to
   ona wchodzi do sterownika (T-53 AC-1). Czarna lista nie wystarcza: domyslna powierzchnia ma
   osiem sciezek startu procesu (Task, Workflow, SendMessage, CronCreate, RemoteTrigger,
   ScheduleWakeup, EnterWorktree, Monitor) i cicho urosnie przy nastepnym wydaniu CLI.
2. **`init.tools` nie jest powierzchnia uprawnien.** Lista pod `ReadOnly` zawiera `Bash`.
   Porownywanie polityk przez dlugosc tej listy to blad kategorii — 27 pozycji to BAZA CLI,
   a wymienienie `Glob` albo `Grep` w `--allowedTools` odslania oba, dajac 29.

### Ustawienia gospodarza moga nas ROZSZERZYC, nie tylko zawezic

`sandbox.autoAllowBashIfSandboxed: true` przepuszcza dowolna komende mimo naszego
`--allowedTools`. Blok `env` gospodarza nadpisuje srodowisko podane przez Loadouta (jego haki
czytaja wlasne zmienne, wiec haki i `env` to jedna calosc). Dlatego przepisujemy **wylacznie
`permissions.deny`** — `src-tauri/src/engine/drivers/host.rs`, T-53 AC-4.

### Trzy naprawy harnessu, kazda po prawdziwym incydencie

- **`ac30479` — cztery konsumenty OWNS czytaly ten blok na trzy rozne sposoby.** 42 z 60 plikow
  zadan konczy blok bajtami `...cancel.rs-->`, bez nowej linii. `quick-scope.sh` kasowal `sed '$d'`
  CALA ostatnia linie (ginela ostatnia sciezka), a `before-spec-owns.sh` z regexem `\n-->`
  **nie dopasowywal wcale** i wychodzil zerem z napisem „nothing to judge" — czyli NIE SADZIL
  NICZEGO na 42 zadaniach. To niezmiennik 19 zlamany po cichu wewnatrz bramki. T-10 wpadl przez
  to w pelne zakleszczenie: napiszesz plik -> `quick-scope` czerwony, nie napiszesz -> AC-6
  czerwone, TASK.md zablokowany.
- **`04a346e` — kanarek `tasks/T-01.md` pilnowal polityki, ktora wlasciciel cofnal** commitem
  `533eab8`. T-53 skonczylo 4/4 zielone i utknelo na czerwieni, ktorej zadna dozwolona sciezka
  nie gasi. Zdjecie jest bezpieczne: `Edit/Write(TASK.md)` zostaja w `deny`, wiec pisarz dalej
  nie tknie wlasnego kontraktu.
- **`699ef25` — kod 2 znaczy „nie twoje" na calej dlugosci.** `quick-permissions` oddawal 1 przy
  sprzecznosci konfiguracji, choc CALY jego material (`.claude/settings.json`, blok OWNS, on sam)
  lezy poza zasiegiem pisarza. Teraz oddaje 2. Razem z tym **zawezona karta w `integrate.sh`**:
  stara wersja wybaczala KAZDY kod 2 na trunku, wiec sama pierwsza naprawa otworzylaby dziure.
  Wybacza teraz wylacznie przy SWIEZYM paragonie z pusta lista `misconfigured` (nowe pole w
  `runs/last.json`); brak paragonu i paragon o innym commicie znacza odmowe.

**Zasada dla nastepnych sprawdzen:** sprawdzenie, ktorego caly material lezy poza zasiegiem
pisarza, oddaje 2, nie 1.

### S-3 zamkniety, T-10 odblokowane — ale pokrycie parsera jest zdegradowane

`docs/research/fixtures/codex-stream.jsonl` pochodzi z PRAWDZIWEGO biegu `codex exec --json`
(commit `7a24fd4`), ale zawiera wylacznie **koperte awaryjna**: cztery zdarzenia
(`thread.started`, `turn.started`, `error`, `turn.failed`), bo konto Codeksa jest bez kredytow
**do 2026-08-20 05:30**. Ani jednego `item.*`. T-10 AC-2 przewidzialo ten przypadek i wymaga
oznaczenia mapowan `item.*` komentarzem `[3p]`. **Po 5:30 S-3 leci ponownie i ten plik ma sie
POWIEKSZYC** — to jest zaplanowane, nie regresja.

Dwa pomiary przy okazji: stdout Codeksa jest czystym JSONL, a stderr niesie `Reading additional
input from stdin...` (potwierdza T2 §9.3: nigdy `2>&1`). `--ignore-user-config` USUWA ladowanie
cudzych serwerow MCP — bieg bez tej flagi probowal odswiezyc OAuth dla figma, notion i linear,
zanim ruszyla tura. Codex nie ma `--strict-mcp-config`, wiec to jedyny znany srodek.

### Codex jest slabszym adapterem i to trzeba zapisac, a nie zalozyc symetrie

Nie ma odpowiednika `--tools`, `--disallowedTools` ani `--setting-sources`. `--ignore-user-config`
tyka WYLACZNIE `$CODEX_HOME/config.toml`, a `--ignore-rules` tylko pliki `.rules` — **zadna flaga
nie wylacza projektowego `.codex/hooks.json` gospodarza** (meetnotes ma tam te same trzy straze
co po stronie Claude'a). Jedyna obrona to zaufanie hakow po haszu tresci, czyli obrona MASZYNY,
nie Loadouta: hak raz zatwierdzony wystartuje. Dla adaptera: piaskownica (`-s read-only` /
`workspace-write`) jest glowna dzwignia, `--ephemeral` bez zapisu sesji, i **nigdy**
`--dangerously-bypass-hook-trust`.

### Co czeka

| co | stan |
|---|---|
| **T-54** — dziedziczenie wiedzy (5 kryteriow) | **w biegu**, faza kontraktu |
| **T-55** — krok „sprawdz" (5 kryteriow) | napisane, czeka na wolna maszyne |
| **T-56** — jedna kopia dla lancucha + krok ciezki (2) | napisane, **czeka na T-52** |
| **T-52** — izolacja jako drzewo gita | napisane przez wlasciciela, galaz `T-52`, niezlandowane |
| S-3 ponownie + przeglad cross-vendor | po 2026-08-20 05:30 |

**Wada, ktorej ta fala NIE zamyka:** bramka dalej nie odroznia „czerwien z mojego zakresu" od
„czerwien odziedziczona z trunku w trakcie biegu". `refresh_harness_from_trunk` jest projektowane
i moze wniesc czerwien, ktorej zadanie nie spowodowalo — T-53 musialo zglosic defekt konfiguracji
(semantyka kodu 2) pod kodem 1, bo nie ma czym powiedziec tego inaczej. `699ef25` zamyka tylko te
klase, w ktorej sprawdzenie SAMO wie, ze sadzi nasza konfiguracje.

## 2026-08-19 — sekcja Skills umie przyjac tresc, nie tylko adres

**Wyladowane: T-42 (4 kryteria) i T-43 (3).** Pelna bramka po kazdym: 15 sprawdzen, 0 czerwonych.
Zamowienie czlowieka brzmialo „chce napisac jakiego chce skilla, a program buduje z niego skilla
kompatybilnego z claude/codex", z wyborem „opis -> agent pisze". Rozbite na trzy kontrakty, bo to
sa trzy rozne dowody: **T-42** droga wejscia dla TRESCI (trzy pytania -> `place::emit` -> zapis ->
`ingest::from_folder`, ten sam skan co przy linku), **T-43** jedna tura agenta POZA grafem
(`AgentDriver::start` -> `Outcome.text` -> trzy pola formularza), **T-44** wybor „ten projekt /
wszedzie" (w toku).

### Co z tego wynika dla produktu

Zlota lista komend: 24 -> 29 (`author_skill`, `draft_skill`, `stop_draft` z tej fali, `open_chat`
i `say_to_orchestrator` z pracy wlasciciela). Karta przegladu przestala twierdzic, ze wie, skad
przyszla umiejetnosc: plakietka „From the internet" byla wpisana NA SZTYWNO i ignorowala
`item.fromTheInternet` -- prawdziwa przez konstrukcje, dopoki jedyna droga byl link. Pochodzenie
lezy teraz w plikach (`~/.loadout/skills/origins.json`), a nie w domysle z istnienia kopii
kanonicznej, i ma ostrozny domyslny: kopia bez zapisu pochodzenia jest „z internetu", bo do tej
fali tylko taka droga tworzyla kopie.

### Trzy znaleziska, ktorych ta fala NIE zamiata (AGENTS.md §7)

1. **Utrata danych osiagalna z okna, naprawiona po drodze w T-42 AC-1(c).** `review_skill_inner`
   liczyl sciezke kopii kanonicznej z pola `name` front-mattera i robil na niej `remove_dir_all`
   (`commands/skills.rs:350-351`); `from_folder` nie waliduje nazwy, a `Skill::default()` daje
   `name: ""`. Sprawdzone `rustc`: `PathBuf::from("/a/b").join("")` to `"/a/b/"`. Link do dowolnego
   `SKILL.md` BEZ pola `name:` kasowal `~/.loadout/skills/` razem z `installed.json`.
2. **Globalny limit „ile naraz" nie jest podpiety w produkcji.** `run_workflow_with_slots(…, slots)`
   nie ma wolajacego poza testami, a `run_workflow_inner:237` zaklada wlasny `Limiter` na kazdy
   bieg. Kryterium T-31 dowodzi globalnosci, bo podaje pule argumentem. Trzy karty po trzech
   agentach to dziewieciu agentow przy suwaku na 3 (niezmiennik 11). Dlatego T-43 nie udaje, ze
   bierze slot -- ma jawna granice „jeden draft naraz".
3. **Lista pol zdjetych przez `emit` nie ma konsumenta** (`let (doc, _) = emit(skill)`,
   `place.rs:545`). `hooks:` znika z pliku bez ani jednego zdania na ekranie. Do tego
   `allowed-tools` jest w `SPEC_FIELDS`, wiec JEDZIE do obu katalogow vendorow z samym `Warn` --
   umiejetnosc moze przydzielic sobie narzedzia, a przy tekscie pisanym przez model przestaje to
   byc rzadkie.

### Dwa defekty harnessu, naprawione osobnymi commitami

Odslonil je stos galezi (T-43 odbity od niewyladowanego `task-T-42`, bo trunk byl brudny). Oba
mialy ten sam ksztalt: pytanie o stan dysku rozstrzygane po BRZMIENIU komunikatu.

- `0140979` -- `exit 0 but no evidence` bylo liczone jako „kryterium przechodzi", wiec kazdy
  wznowiony bieg z kryterium rustowym konczyl sie kodem 2 przy uczciwie czerwonych kryteriach.
- `c696fc0` -- „czy sa specyfikacje" rozstrzygane po napisie `did not RUN`; kryterium rustowe bez
  modulu udawalo istniejacy plik, wiec bieg szedl NAPRAWIAC pliki, ktorych nie ma. Teraz pyta
  dysku przez `gate.spec_tokens` -- ten sam parser, ktory sadzi kontrakt.

Oba z kontrola w obie strony na prawdziwych bajtach funkcji; grozny przypadek („PASSES before
implementation") dalej odmawia.

### Cena infrastruktury, zmierzona

T-42 kosztowalo **~$36,50**, z czego **$12,15 to strata na infrastrukturze**: limit sesji (429 po
811 ms, faza pisarza nie ruszyla) i ubicie biegu na granicy tury (7 minut pisania, `result:
error_during_execution`, $8,44 za prace, ktorej nikt nie odebral). Zamkniete przez
`scratchpad/detach.py` (podwojny fork + `setsid`, kod wyjscia do `runs/<ID>/wave.rc`) -- ten sam
bieg odczepiony przezyl cztery granice tury. Do czekania na wynik uzywaj `Monitor` z
`persistent: true`, nie `run_in_background`: czekacz ginie na kazdej granicy tury, praca nie.

**Falszywa czerwien, ktorej nie warto szukac drugi raz:** `product_path_end_to_end`,
`run_reaches_the_pump`, `runcmd_snapshot` i `runcmd_parallel` wieszaja sie na ZAJETEJ maszynie --
mierza nakladanie sie na prawdziwym zegarze i maja limit 20 s w sobie, wiec
`CHECK_TIMEOUT_OVERRIDE` ich nie podniesie. Przy siedmiu agentach w tle: cztery czerwone.
Na bezczynnej maszynie ta sama migawka: 15 sprawdzen, 0 czerwonych, 16 s.

## 2026-08-18, 05:30 — pietnascie kryteriow jednego dnia i aplikacja, ktora naprawde chodzi

**Suita jednostkowa: 88 plikow / 440 testow zielonych. E2E w prawdziwym chromium: 13/13.**
Dowiezione tego dnia: T-37 (3 kryteria), T-38 (8), T-39 (7). Kontroli negatywnych: **101 w piecu
rownoleglych pasach plus 3 moje**, wszystkie czerwone, wszystkie przywrocone po md5.

### Aplikacja dziala — zmierzone, nie zadeklarowane

Zrzut zywego okna 05:24 pokazuje menu 196 px ze znakiem i `LOADOUT`, piec sekcji, stopke
`Claude · Codex ready`, pasek kart z `＋`, wybor workflow, **wlaczony** `Start`, suwak „ile
naraz", pusty stan z zaproszeniem, **szyne agentow** po prawej i **wiersz wejscia** na dole.

**Dowod, ze to nie atrapa:** w wyborze stoi `New workflow 2`, a na dysku lezy
`~/.loadout/workflows/new-workflow-2.json` z polem `"name": "New workflow 2"`. Lancuch
plik → `list_workflows` → `invoke` → okno jest prawdziwy w obie strony — te pliki powstaly
wczesniej przyciskiem `Create`.

### Biale okno przy starcie — przyczyna zamknieta, NIE jest defektem produktu

Dwie przyczyny, obie srodowiskowe. (1) `tauri dev` obserwuje `src-tauri/` i **restartuje
aplikacje po kazdym zapisie** — przy pieciu agentach piszacych rownolegle okno ginelo co
kilkadziesiat sekund, a czlowiek widzial „szary ekran na chwile". (2) vite pre-bunduje
zaleznosci na zadanie i pierwsze wejscie po zmianie ich zestawu blokuje `/src/main.tsx`
na **32 s**; webview trzyma wtedy polaczenia i pokazuje pusta strone.
**Rozpoznanie jest jednolinijkowe:** `curl -o /dev/null -w '%{time_total}' /src/main.tsx`
mierzy ten czas wprost. Okno maluje sie natychmiast po tym, jak serwer zaczyna oddawac modul.

### Trzy rzeczy, ktore znalazl dopiero sprawdzajacy

Pieciu niezaleznych sprawdzajacych z poleceniem „domyslaj sie na niekorzysc pasa". Kazdy
odtworzyl po jednej kontroli negatywnej SAM i przeszukal pliki pod katem zaslepek.
**Atrap nie znalezli zadnych.** Znalezli trzy rzeczy, ktorych nie widzialo zadne kryterium:

1. **Zamkniecie karty z zywym biegiem nie anulowalo biegu.** `WorkspaceTab.agents` bylo pisane
   tylko przy zakladaniu karty i zawsze zerem, wiec `requestClose` zawsze wchodzil w galaz
   „nic tu nie chodzi": karta znikala bez pytania i bez `cancel()`. Osierocony agent dalej palil
   limit (niezmiennik 6 — blad finansowy). `CloseConfirm` byl przez to kodem NIEOSIAGALNYM.
   Naprawione, kryterium T-39 AC-7 z trzema sondami.
2. **`useMemory.load` i `useSkills.load` nie mialy wolajacego** — sciezka odczytu byla zbudowana
   i martwa, wiec obie sekcje dalej nie czytaly dysku. Naprawione.
3. **`commands-wired.test.ts` byl czerwony**: doszly dwie krawedzie bez wiersza w tabeli strazy.
   Dopisane, 16 → 18.

### Co zostalo do prod-ready

- **T-41 (napisane)** — odpowiedz czlowieka NIE dochodzi do agenta. `answer()` jest czysto
  lokalne: pytanie znika z ekranu, agent dalej czeka. To jedyna znana martwa kontrolka i jedyna,
  ktora **klamie**. Nie jest to podpiecie kabla — `RunControl` nie ma uchwytu do zywej sesji,
  wiec trzeba przeciagnac kanal przez granice. `AgentDriver::send` juz istnieje.
- **T-40 (napisane)** — wyrocznia „kazda kontrolka cos robi" poza pieciu ekranami: stany
  zagniezdzone, pola i selecty, oraz dowod, ze skutek jest TYM skutkiem.
- **`quick-types` nie umie byc czerwony na kodzie zadania** — prawdziwy blad typow melduje jako
  „our TypeScript configuration is broken — this is not your code", kodem 2, o ktorym bramka
  sama pisze „never a red". Trafilo mnie dwa razy jednego dnia.
- **`tests/it/main.rs` to nowy kregoslup bez `merge=union` i bez wlasciciela** — dwa zadania
  dodajace test naraz dadza pewny konflikt.

## Liczby

| | |
|---|---|
| commitów lądowania | **34** |
| trunk | **ZIELONY** — 14 sprawdzeń, 0 porażek, 390 s (`runs/last.json`, 2026-08-18 00:31) |
| zadań w `tasks/` | 42 |
| żywe gałęzie | **cztery**: T-38 · T-29 · T-28 · T-37 (T-32 wylądowało, worktree do sprzątnięcia) |
| zablokowane kalendarzem | **S-3, T-10** — kredyty Codeksa wracają 2026-08-20 |

## Co się dziś zmieniło w tempie pracy — i dlaczego to jest najważniejszy wpis

| | było | jest |
|---|---|---|
| `checks/full-test.sh` | do 3600 s, czyli **timeout** | **224 s** |
| `cargo clippy --all-targets` | 455–1200 s | **6 s** na ciepłym drzewie |
| `./verify.sh quick` | ~300 s | **37 s** |
| `./verify.sh full` na trunku | nie kończyło się | **390 s** |
| lądowanie gałęzi | ~2 h | **9–11 min** |

**Przyczyna była jedna: `src-tauri/tests/` miało 122 pliki, a Rust robi z każdego pliku osobne
binarium** linkujące całą bibliotekę z 527 skrzyniami. Same testy wykonują się w **6,0 s**;
reszta była składaniem i pierwszym uruchamianiem 122 programów. Dwie niezależne miary tego samego:

- linkowanie — kontrolowany pomiar jednego celu po dotknięciu `commands/run.rs`: **60 s i 62 s**;
- **pierwsze** uruchomienie świeżej, niepodpisanej binarki debug — `store_strict_schema` **36 s**,
  `workflow_check_ids` **59 s**, przy **0 s** za drugim razem i teście trwającym 0,01 s. To jest
  skanowanie macOS (`syspolicyd`, `XprotectService`), zapamiętywane per plik.

Obie miary mnożyły się przez 122. Pliki są teraz **modułami jednego celu** (`tests/it/main.rs`),
czyli jeden link i jedno skanowanie. Tak samo robią ripgrep (`autotests = false` + jeden
`[[test]]`) i cargo (`tests/testsuite/main.rs`, ~150 linii `mod`). `src-tauri/Cargo.toml` sam to
deklarował od pierwszego dnia — „`cargo test --lib` jest CAŁĄ powierzchnią testową" — a kod łamał
tę deklarację 122 razy.

Dla skali, zmierzone na tej maszynie: `../meetnotes` ma **950** skrzyń (prawie dwa razy więcej
niż my) i **jedno** binarium testowe — 19 835 plików w `target/debug/deps` wobec naszych 886 645.

### Trzy rzeczy, które z tego wynikają dla piszącego

1. **Kryterium woła `cargo test --test it <moduł>::`**, nie `cargo test --test <moduł>`. Filtr
   z dwukropkami, nie sam podciąg: `--test it store` łapie także `store_pragmas` i `storage_x`.
2. **Nowy plik w `tests/it/` wymaga linii `mod` w `main.rs`.** Bez niej nie kompiluje się, nie
   uruchamia ani jednego testu i **wygląda jak zestaw, który przeszedł**. Pilnuje tego
   `checks/quick-tests-listed.sh` — mechaniczny, bez kompilacji, więc działa też wtedy, gdy
   drzewo się nie buduje.
3. **Test mierzący albo zmieniający stan CAŁEGO PROCESU zostaje osobnym celem w `tests/`.**
   Dziś dwa: `shell_logging` (liczy deskryptory przez `/dev/fd`, instaluje globalny hak paniki)
   i `supervisor_env_hygiene` (`env::set_var`). W scalonym binarium mierzyłyby 285 cudzych
   testów — `shell_logging` dostał 96 zamiast swojej liczby przy pierwszym lądowaniu po scaleniu.

## Praca, która weszła z pominięciem pętli — i co to kosztuje

Cztery zadania weszły **wprost na trunk**, bez gałęzi i bez tieru `before`: **T-28, T-33, T-35,
T-37**. Powód był policzalny — fala kosztowała ~2 h przy około 40% skuteczności za pierwszym
razem — ale konsekwencja jest realna i zostaje zapisana:

**Te cztery nie mają dowodu, że ich kryteria były najpierw CZERWONE**, ani drugiej opinii.
Kryterium za wąskie od urodzenia jest w nich niewykryte.

**Powód tego skrótu zniknął.** Pełna bramka idzie 9 minut. Cztery przebiegi to niecała godzina
i to jest najtańszy sposób odzyskania tego dowodu.

Najlepszy argument za tym stoi w `f35466f`: pełna bramka na trunku złapała **prawdziwe
naruszenie projektu** we wczorajszej pracy — `libc::getpgrp()` w `lib.rs` zamiast w
`supervisor.rs` (niezmiennik 3, dwa sprawdzenia naraz). Bez niej nikt by tego nie zobaczył.

## Co stoi w trunku, a czego nie widać z plików zadań

- **Aplikacja się uruchamia i zapisuje.** Cztery wady widoczne dopiero z prawdziwego okna:
  białe okno od IPv6 (`host: false` wiązało serwer na `::1`, WKWebView pyta o IPv4 i **nie
  zgłasza żadnego błędu**), brak `.manage()` (trzy komendy biegu padały „state not managed"),
  katalog projektu wskazujący na `src-tauri/`, oraz `Store::open` poza runtime'em tokio.
- **Sekcje są podpięte do prawdziwych adapterów.** Do 2026-08-17 wszystkie pięć `io.ts` istniało
  i **żadnego nie wołał kod produkcyjny** — jedynym importerem był test. Ekrany były trwale puste,
  a Create odmawiał pod palcem.
- **Edytor workflow jest osiągalny.** Płótno i panel kroku miały testy i **ani jednego miejsca
  montowania**. Siedem takich komponentów znalazł jeden pomiar; `checks/quick-wired.sh` pilnuje
  teraz strony Rusta, strona TS została jako dług.
- **„Własna kopia twoich plików" znaczy kopię** (T-33). Wcześniej `fresh-copy` dawał pusty
  katalog, więc krok pracował na pustce — gorzej niż kolizja, bo agent nie widzi plików.
- **Krok ma limit czasu** (T-35 AC-1), egzekwowany **przez sterownik**, nie przez
  `tokio::time::timeout` — tamto anuluje zadanie Rusta i zostawia żywy proces (niezmiennik 10).
- **Odzyskiwanie po awarii biegnie przy starcie okna** (T-35 AC-2/AC-3). Wymagało zbudowania
  **sześciu** brakujących ogniw, z których **pięć było w komentarzach opisanych jako istniejące**:
  odczyt `kern.boottime`, kolumna `boot_id`, `add_column_if_missing`, `reap_group`
  (`unimplemented!()`), odczyt wierszy i zapis znacznika przy starcie biegu.

## Wada, która wraca — nazwana, bo trafiona ponad dziesięć razy

**Kryterium sprawdza coś węższego niż niezmiennik, którego pilnuje.** Wzorcowy przykład: asercja
`TITLEBAR_HEIGHT <= 96` była **zielona przy 138 px** realnego chrome, bo mierzyła jeden pasek
z trzech. Drugi: „strzałka znaczy po" porównywała chwile odbioru paczek, więc padała losowo,
gdy dwa kroki trafiły w to samo 16-milisekundowe okno pompy.

Trzy rzeczy, które to rozróżniają, i wszystkie trzy są w tym repo sprawdzone:

1. **Wartość oczekiwaną czytaj z pliku, nie przepisuj.** Test wpisujący `196` z palca przechodzi
   też wtedy, gdy makieta mówi 220.
2. **Kontrola negatywna do każdego kryterium.** Dwie moje dzisiejsze sondy przechodziły **także
   przed poprawką** — dowiedziałem się tego wyłącznie dlatego, że je zasadziłem.
3. **Napisz w nagłówku, jaka byłaby SŁABA wersja tej asercji i co ją odróżnia.**

## Co dalej, po właścicielu

| co | kto | stan |
|---|---|---|
| T-38 — szew front↔Rust, klucze argumentów | agent redesignu | 8 kryteriów, gałąź `T-38` |
| T-37 — trzy testy kryteriów układu | agent redesignu | **kod w trunku, testów nie ma** |
| T-29 — e2e w przeglądarce | agent redesignu | odłożone świadomie do po redesignie |
| S-3, T-10 — drugi vendor | czeka | kredyty Codeksa 2026-08-20; `drivers/absent.rs` odmawia głośno zamiast udawać Claude'a |
| przepuścić T-28/T-33/T-35/T-37 przez pętlę | orchestrator | ~1 h, odzyskuje dowód czerwieni |
| Q-6 — zegar ścienny nie odróżnia „wolne" od „wisi" | kolejka | `docs/HARNESS-QUEUE.md` |
| Q-7 — liczba celów testowych | **zamknięte** | 122 → 1, opisane wyżej |

## Mina, o której trzeba pamiętać przed lądowaniem T-28

`a7a2d87` dodał oba pliki testów szkieletowych **wprost na main**, a gałąź `task-T-28` niesie
**własną, rozjechaną kopię tych samych plików**. Różnicą jest dokładnie `#[ignore]` z `6e55daf`,
czyli ogrodzenie płatnych testów uruchamiających prawdziwe procesy `claude`.

**Lądowanie T-28 bez uzgodnienia po cichu cofnie to ogrodzenie** — a bramka po takim lądowaniu
będzie **zielona**, bo cofnięte kryterium nie psuje testów, tylko je osłabia.
