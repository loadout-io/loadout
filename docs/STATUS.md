# Stan budowy — 2026-08-18, 00:40

Ten plik jest **żywy**. Aktualizuje go orchestrator po każdym lądowaniu. Prawdą o biegu jest
jego `TASK.md` na gałęzi i `runs/<id>/`; tutaj jest wyłącznie to, czego z nich nie widać:
co już stoi w trunku, co stanęło i dlaczego.

> **2026-09-02:** wpisy starsze niż te niżej stoją w [`STATUS-ARCHIVE.md`](STATUS-ARCHIVE.md).
> Ten plik ma zostać krótki: co stoi w trunku, co otwarte, ostatnie sprostowania.

> **2026-09-04:** od pętli „prod-ready" postęp mieszka w
> [`docs/prod-ready/PLAN.md`](prod-ready/PLAN.md) — tabela „Stan" i „Dziennik" są tam jedynym
> źródłem prawdy, a ten plik nie jest już aktualizowany po każdym lądowaniu. Zostaje jako
> obraz tego, co stało w trunku przed pętlą.
> Bieżąca pętla i jej postęp: [`prod-ready/PLAN.md`](prod-ready/PLAN.md).

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

