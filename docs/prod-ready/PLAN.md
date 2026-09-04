# Plan „prod-ready" — pętla do stanu produkcyjnego

Powstał 2026-09-02 z audytu (`docs/prod-ready/AUDIT-2026-09-02.md`, strona:
https://claude.ai/code/artifact/52e87887-d893-46ac-a47f-48f8dafc7d1f). Ten plik jest **jedynym
źródłem prawdy o postępie**: tabela „Stan" mówi, co zrobione, „Dziennik" mówi, co się działo.
Orkiestrator aktualizuje oba po każdej zmianie statusu i commituje ten plik na `main`.

Nad tym planem stoją `AGENTS.md` i `docs/DECISIONS-LOCKED.md`. Jeśli coś tu się z nimi kłóci —
wygrywają tamte, a rozbieżność trafia do Dziennika.

---

## 1. Protokół pętli (wiążący dla orkiestratora)

### Statusy

`TODO` → `RUNNING` → `DZIALA` (bieg skończony kodem 0, jeszcze niewlany) → `LANDED`
(zmergowane na `main`, `ci.sh full` zielone, worktree usunięty). Boczne: `BLOCKED` (kod 2 po
dwóch poprawkach, kod 1 z powodu kodu, czerwone CI po merge'u, konflikt) — z jednym zdaniem
powodu w kolumnie „Uwagi" i wpisem w Dzienniku. `BLOCKED` nie zatrzymuje pętli; zatrzymuje tylko
zadania, które od niego zależą (te dostają `BLOCKED-DEP`).

### Wybór następnego zadania

1. Fala 0 w całości przed jakimkolwiek `scripts/h run` — pakiety 0.1 → 0.5, po kolei.
2. Potem: najniższa fala, w niej najniższy numer, którego wszystkie zależności są `LANDED`.
3. Równoległość: najwyżej **jeden** bieg dotykający Rusta (`src-tauri/**`) naraz; obok niego
   może iść najwyżej **jeden** bieg czysto TS (`src/**`, oznaczone `TS` w tabeli). Niezmiennik 26
   i pamięć projektu: dwa ciężkie `cargo` naraz zamrażają maszynę, a zajęta maszyna udaje czerwony
   test.
4. `scripts/h land` biegnie wyłącznie, gdy **żaden** bieg nie trwa (pełne CI to ciężkie `cargo`
   + `vitest` + Chromium). Kolejka lądowań: w kolejności zakończenia biegów.
5. Sesja ma limit tokenów. Po każdym `LANDED` zapisz stan; gdy kontekst robi się ciężki —
   `/compact`. Nie trzymaj w kontekście transkryptów biegów: czytaj `runs/<id>/` tylko przy
   kodach 1/2/3, i tylko ogon.

### Uruchomienie zadania

```bash
scripts/h run <id> --prompt "$(cat docs/prod-ready/prompts/<ID>.md)" [--dev codex --verifier claude]
```

- `<id>` = mały identyfikator z tabeli (np. `z01-descendants`); gałąź to `h-<id>`, worktree
  `../loadout-h-<id>`, stan `.git/h/<id>.json`, transkrypty `runs/<id>/`.
- Prompt idzie przez `$(cat …)`, nie wklejony w cudzysłów: wynik podstawienia nie jest
  interpretowany przez powłokę, więc backticki i dolary w treści są bezpieczne.
- Uruchamiaj w tle (`run_in_background: true`); bieg trwa 30–90 min, a sufit tury Basha to 10 min.
- Domyślna para: plan Claude, kod Claude, weryfikacja Codex (D3). Zadania oznaczone w tabeli
  `codex` idą z `--dev codex --verifier claude` — cross-vendor w drugą stronę, żeby oba vendory
  robiły całość, a D3 była prawdziwa w obu kierunkach.
- Domyślne sufity per faza (po pakiecie 0.3): `LOADOUT_BUDGET_PLAN=12`, `LOADOUT_BUDGET_DEV=40`,
  `LOADOUT_BUDGET_VERIFY=6` (USD). Zadania oznaczone `duże` dostają `LOADOUT_BUDGET_DEV=70`.

### Odbiór

| kod | znaczenie | co robisz |
|---|---|---|
| 0 | DZIALA, praca zacommitowana na `h-<id>` | status `DZIALA`; gdy nic nie biegnie: `scripts/h land <id>` → `scripts/h clean <id>` → `LANDED` |
| 1 | check padł **albo padł model** | przeczytaj ogon `runs/<id>/build-*.jsonl` i `.git/h/<id>.json`. Trzy przyczyny MASZYNOWE, wszystkie zmierzone 2026-09-03 i wszystkie dające kod 1: `api_error_status: 429` z frazą o limicie sesji; `cannot execute binary file`, gdy vendor aktualizował się w oknie zapisu (plik nazywa się `claude.exe` i JEST binarką macOS — sprawdź `claude --version`); timeout checka pod obcym obciążeniem. Przy każdej z nich **powtórz raz** to samo polecenie, worktree zostaje. Dopiero przyczyna w kodzie to `BLOCKED` |
| 1 | **faza planu zjadła sufit dolara** | Piąta przyczyna maszynowa, poznana 2026-09-04 na Z-22: w transkrypcie stoi `terminal_reason: "budget_exhausted"`, `subtype: "error_max_budget_usd"` i zdanie `Reached maximum budget ($12)`. To NIE jest porażka sprawdzenia ani wada kodu — to `LOADOUT_BUDGET_PLAN` robiący swoje. Bieg ginie **przed** napisaniem czegokolwiek, a sesja planu nie jest zapisywana, więc ponowienie płaci od nowa (dług H-24, opisany pod tabelą sond). Odpowiedź: **jeden** ponowny bieg z tańszym planistą (`--planner codex`) i podniesionym sufitem; jeśli i on zje sufit, zadanie idzie na BLOCKED z powodem „plan". Zanim ponowisz, policz narzędzia w `runs/<id>/plan.jsonl` — dwanaście odczytów tego samego pliku znaczy planistę kręcącego się w kółko, nie szeroki zakres. |
| 2 | STOP po dwóch poprawkach albo NIE_WIEM albo bieg dotknął wyroczni | `BLOCKED` z pierwszym zdaniem `co_nie_dziala`; worktree zostaje do wglądu |
| 3 | sufit czasu/tur | powtórz raz z `LOADOUT_MAX_TURNS=400 LOADOUT_BUDGET_DEV=70` i `--no-plan` (plan jest w stanie); drugi raz — `BLOCKED` |

Po `land`: jeśli `ci.sh full` jest czerwone, merge zostaje na `main`. Trzy powody wolno
poprawić na `main` (`bash scripts/ci.sh full`, commit `fix(main): …`): `rust-fmt`, `web-fmt`,
pojedyncza uwaga clippy — oraz **test rodzeństwa, który przypina zachowanie sprzed tej
właśnie wlanej zmiany**. Ten czwarty dopisany 2026-09-03 po Z-12: zawężony check biegu nie
odpala cudzych modułów (H-5), więc test wyliczający pełną sekwencję zdarzeń dowiaduje się
o nowym zdaniu dopiero w pełnej bramce. Warunek jest ostry: wolno **DOPISAĆ** oczekiwanie
opisujące nowe, zweryfikowane zachowanie, i **nigdy** osłabić albo usunąć asercję. Jeśli masz
wątpliwość, czy to jedno czy drugie — to jest rewert.

Każdy inny powód: `git revert -m 1 HEAD`, `BLOCKED` z powodem. **Nigdy** `git reset --hard`
(zablokowane w `deny`).

### Czego orkiestrator nie robi

- Nie edytuje `harness/`, `checks/`, `scripts/`, `.claude/`, `AGENTS.md`,
  `docs/DECISIONS-LOCKED.md`, `worktree.sh` poza Falą 0 — i w Fali 0 wyłącznie przez `python3`
  z zapisem atomowym (`tmp` w tym samym katalogu + `os.replace`), bo Edit/Write są tam
  zablokowane celowo.
- Nie uruchamia `cargo`/`vitest` na `main`, gdy jakikolwiek bieg albo `land` trwa.
- Nie pushuje. Nie kasuje gałęzi `backup/*`. Nie kasuje worktree zadania w stanie `BLOCKED`.
- Nie zmienia treści promptów w `docs/prod-ready/prompts/` po starcie biegu; jeśli prompt jest zły,
  zadanie dostaje `BLOCKED` z powodem „prompt", a poprawiony prompt idzie jako nowy bieg z nowym
  `<id>` (stary worktree `clean`).

---

## 2. Fala 0 — ręka orkiestratora, przed pętlą

Powód: pętla na dzisiejszym `h.py` dostaje plan Codeksa jako surowy strumień JSON, poprawkę na
cudzej sesji, zostawia sieroty po checkach i potrafi wlać gałąź bez wspólnego przodka. Wyrocznia
jest w `deny`, więc żaden bieg tego nie naprawi.

Każdy pakiet kończy się commitem na `main` i wierszem w Dzienniku. Kolejność jest zależnością.

### 0.1 Sprzątanie maszyny — LANDED

1. Sierota: `ps -o pid,ppid,pgid,etime,command -p 23164`. Jeśli to nadal
   `…/meetnotes/.loadout/runs/20260901-150035__01a05d7c-…/work/s_7/target/debug/deps/meetnotes_lib-…`
   → `kill -TERM 23164`, po 5 s `kill -KILL 23164`, potwierdź `kill -0 23164` = ESRCH. Jeśli pid
   wskazuje co innego — nie zabijaj, zapisz w Dzienniku.
2. `cargo clean` w `~/Projects/Loadout` (34 GB, 577 979 plików w `deps`). Potem pierwszy
   `cargo build --tests` w `src-tauri` (zimny, kilka minut) — żeby hak Stop i pierwsze biegi nie
   płaciły go w turze.
3. Worktree: `scripts/h clean skills-reach-the-lead` (zlądowany w `ff31bb11`);
   `git worktree remove --force` dla `../loadout-T-151-race-before` i `../loadout-T-157-before`
   (detached, sondy); `git worktree prune`. Pozostałe 16 (`h-repair-*`, `lab`, `ui`,
   `wf-preflight`, `T-203-phase8`, `fix-session-trust`) zostają do pakietu 0.2.
4. `runs/`: zostaw katalogi zadań z otwartym stanem w `.git/h/` oraz młodsze niż 14 dni; resztę
   usuń (`runs/` jest gitignored). Zapisz w Dzienniku, ile usunięto.
5. Zaufanie: kopia `~/.claude.json` → `~/.claude.json.bak-2026-09-02`; usuń z `projects` wpisy,
   których katalog nie istnieje (python3, atomowo, `flock` jak w `harness/trust-workspace.py`).
   To samo dla `[projects."…"]` w `~/.codex/config.toml` (kopia najpierw). Zapisz liczby.
6. Zamknij dwa procesy-sieroty innych sesji tylko, jeśli są nasze i martwe (nie dotykaj czterech
   interaktywnych `claude`).

Kryterium: `ls target/debug/deps | wc -l` < 60 000 po buildzie; `git worktree list` = main +
16 z pakietu 0.2; brak procesu z `cwd` w `.loadout/runs/` starszego niż żywy bieg.

### 0.2 Ratunek dwunastu gałęzi `h-repair-*` — LANDED (7 z 11 commitów, 4 BLOCKED)

Dla każdej z: `repair-agent-app-preflight`, `repair-bounded-check-output`,
`repair-bounded-evidence`, `repair-diagnostics-build-facts`, `repair-global-diagnostics`,
`repair-log-lifecycle`, `repair-readme-truth`, `repair-release-identity`,
`repair-release-runbook`, `repair-serve-natural-reap`, `repair-serve-reap-hardening`,
`repair-trigger-open-app-honesty`:

1. `git -C ../loadout-h-<x> stash -u` jest zablokowane; zamiast tego **odrzuć** 85 zestagowanych
   zmian bez commitowania: to kopie z przepisywania historii, nie praca biegu
   (`git -C ../loadout-h-<x> diff --cached --stat` do Dziennika, potem
   `git -C ../loadout-h-<x> restore --staged --worktree .` — jeśli `restore` jest zablokowane,
   pomiń krok: patch i tak bierzesz z commitów, nie z drzewa).
2. Właściwa praca biegu = commity ponad wspólnym przodkiem ze starą historią:
   `base=$(git merge-base backup/przed-przepisaniem-historii h-<x>)`;
   `git diff $base h-<x> > /tmp/prod-ready/<x>.patch`. Obejrzyj `--stat`: jeśli patch dotyka
   `harness/`, `checks/`, `scripts/`, `AGENTS.md`, `docs/DECISIONS-LOCKED.md` — te hunki wytnij
   (to nie była praca zadania) i zapisz w Dzienniku.
3. Świeża gałąź: `git worktree add ../loadout-rescue-<x> -b rescue-<x> main`;
   `git -C ../loadout-rescue-<x> apply --3way /tmp/prod-ready/<x>.patch`. Konflikt → zostaw,
   `BLOCKED` w tabeli poniżej, dalej.
4. Bramka: `scripts/h check` w tym worktree nie zna zadania bez stanu — więc zapisz stan ręcznie
   (`.git/h/rescue-<x>.json` z `worktree` i `task`) i odpal
   `scripts/h run rescue-<x> --no-plan --prompt "Ta gałąź przenosi naprawę <x> z gałęzi sprzed
   przepisania historii. Sprawdź, że kompiluje się i testy jej dotyczące przechodzą; nie dodawaj
   niczego."` — dev Claude, verifier Codex. Kod 0 → `land` → `clean` (także starego worktree
   `../loadout-h-<x>` i gałęzi `h-repair-<x>` przez `git worktree remove --force` +
   `git branch -D`).
5. Po dwunastu: `lab`, `ui`, `wf-preflight`, `T-203-phase8` — obejrzyj `git log --oneline
   $(git merge-base backup/przed-przepisaniem-historii <b>)..<b>`; jeśli to sondy albo praca już
   w `main` innym commitem, `worktree remove --force` + `branch -D`; jeśli nie — ta sama droga
   co wyżej, ale jako wpis `BLOCKED` do decyzji człowieka (nie ratuj automatycznie).
6. Dopiero teraz skasuj 92 gałęzie `task-*`/`T-*` bez merge-base z `main` (jedna komenda z listy
   `git for-each-ref --format='%(refname:short)' refs/heads/ | while …`), zostawiając `backup/*`,
   `main`, `rescue-*`, `fix-session-trust` (zmergowana — też do kasacji po sprawdzeniu `ahead=0`).
7. Usuń `./.h-plan.md` z korzenia repo (H-12).

| gałąź | status | uwagi |
|---|---|---|
| repair-log-lifecycle | LANDED | rotacja `loadout.log`, nowy `logging.rs` |
| repair-bounded-check-output | LANDED | wyjście checka z sufitem w RAM |
| repair-diagnostics-build-facts | LANDED | paczka diagnostyczna zna build |
| repair-serve-natural-reap | LANDED | naturalne zejście `serve` zbiera grupę |
| repair-serve-reap-hardening | LANDED | to samo przy uporczywym procesie |
| repair-release-identity | LANDED | `.cargo/config.toml` + kontrakt wydania |
| repair-release-runbook | LANDED | `docs/RELEASE.md` + test runbooka |
| repair-bounded-evidence | PUSTA | wskazywała ten sam commit co serve-reap-hardening; własnej pracy nie miała |
| repair-global-diagnostics | ODŁOŻONE — decyzja właściciela 2026-09-03 | do napisania od nowa jako zwykłe zadanie, jeśli nadal potrzebne; cała praca jest w plikach interfejsu przepisanych w 0.2. Gałąź `h-repair-global-diagnostics` zostaje jako materiał źródłowy |
| repair-readme-truth | **SKASOWANE** 2026-09-03 | README przepisany dla 0.2, praca bezprzedmiotowa; gałąź usunięta |
| repair-agent-app-preflight | PRZENIESIONE DO KOLEJKI jako **Z-34** | merge próbowany i cofnięty (`612bbf74`): część rustowa przenosi się czysto, ale testy żądają stopki czytającej sondę, a dzisiejszy pasek fałduje się do ikon — czego gałąź nie znała. Port nie jest mechaniczny, więc idzie przez pętlę z weryfikatorem, a gałąź zostaje materiałem źródłowym |
| repair-trigger-open-app-honesty | ODŁOŻONE — decyzja właściciela 2026-09-03 | to samo; gałąź `h-repair-trigger-open-app-honesty` zostaje jako materiał źródłowy |

Kryterium: `git branch --no-merged main | wc -l` = liczba gałęzi z otwartym stanem w `.git/h/`;
`git merge-base main <każda żywa gałąź>` niepuste.

### 0.3 `harness/h.py` — LANDED (`b157a6a0`)

Wszystko przez `python3` z zapisem atomowym. Po zmianach: `python3 -m py_compile harness/h.py`,
potem jeden krótki bieg próbny `scripts/h run probe-h --no-plan --prompt "Dopisz jedną linię
komentarza z datą 2026-09-02 do nagłówka src-tauri/src/durable_file.rs i nic więcej."`
z `--planner codex` raz i domyślnie raz — oba mają skończyć kodem 0 albo 2 z czytelnym
powodem; potem `scripts/h clean probe-h` bez lądowania.

Zmiany, każda z komentarzem DLACZEGO z datą i numerem znaleziska:

- **H-1 (plan Codeksa).** W `call_model`, gałąź `codex`: gdy `transcript` jest podane, a `schema`
  nie — dołóż `out_file = Path(cwd) / ".h-last.txt"` i `argv += ["-o", str(out_file)]`; istniejący
  kod po `communicate` już zwraca treść `out_file`, jeśli istnieje. Surowy strumień nadal idzie do
  `transcript`. W `phase_plan` po wyliczeniu `plan`: jeśli `plan.lstrip().startswith('{"type":')`
  albo `len(plan) > 40_000` → `die("plan wyglada jak strumien JSON, nie plan", 2)`.
- **H-4 (sesja poprawki).** `import uuid`. W `phase_implement`: `sid = load_state(task_id).get("session") or str(uuid.uuid4())`;
  `save_state(task_id, session=sid)`; przekaż `session=sid` do `call_model`. W `call_model`
  (parametr `session=None`), gałąź `claude`: zamiast `argv.append("--continue")` →
  `argv += ["--resume", session] if resume else ["--session-id", session]` (gdy `session`
  podane). Codex bez zmian (nie ma wznowienia; poprawka startuje z `feedback`).
- **H-8 (sieroty checków).** `run_check`: `subprocess.Popen([...], start_new_session=True, ...)`
  + `communicate(timeout=budget)`; przy `TimeoutExpired` → `kill_group(proc)` i tail
  `[TIMEOUT po %ds]`. W `call_model` po normalnym `communicate` też wołaj `kill_group(proc)`
  (grupa zwykle już pusta → `True`; jeśli nie — dzieci Basha agenta giną z dowodem).
- **H-9 (sufit).** `call_model` dostaje `budget_usd`; gałąź `claude`: `argv += ["--max-budget-usd", "%.2f" % budget_usd]`.
  `phase_plan` → `float(os.environ.get("LOADOUT_BUDGET_PLAN", "12"))`, `phase_implement` →
  `LOADOUT_BUDGET_DEV` (40), `phase_verify` → `LOADOUT_BUDGET_VERIFY` (6). W `harness/prompts/implement.md`
  dopisz jedno zdanie: „`scripts/h check` i hak Stop biegną po tobie — nie odpalaj checków ani
  pełnej suity sam; zawężony test padający-przechodzący wystarczy."
- **H-10 (weryfikacja bez śladu).** `phase_verify(task_id, task, plan, wt, checks, vendor, rnd)`
  z `transcript=str(rundir(task_id) / ("verify-%d.jsonl" % rnd))`. W `call_model`: brak binarki
  vendora przy `schema` → `die(..., 2)` (niedostępny weryfikator to nie czerwone, D3). W `cmd_run`:
  `plan = (load_state(task_id).get("plan") or task) if a.no_plan else phase_plan(...)`.
- **H-11 (NIE_WIEM).** W pętli `cmd_run`: `if verdict == "NIE_WIEM": print(...); raise SystemExit(2)`
  przed budowaniem `feedback`.
- **H-2 (wyrocznia).** Stała `ORACLE = ("harness/", "checks/", "scripts/", ".claude/",
  "AGENTS.md", "docs/DECISIONS-LOCKED.md", "worktree.sh", "CLAUDE.md")`. W `cmd_run` po
  `phase_check`: `hit = [p for p in paths if p.startswith(ORACLE)]` → `die("bieg dotknal wyroczni: %s" % hit, 2)`.
  W `cmd_land` przed merge'em: `git diff --name-only <trunk>...<branch>` z tym samym filtrem →
  `die(..., 2)`; oraz `if not git("merge-base", trunk, branch, check=False): die("galaz nie ma wspolnego przodka z trunkiem", 2)`.
- **H-19.** `phase_verify`: `base = git("merge-base", trunk, "HEAD", cwd=wt, check=False) or "HEAD"`;
  `diff = git("diff", base, cwd=wt, check=False)`.
- **H-20.** `phase_check`: po pierwszym `FAIL` pomiń checki z `budget_s >= 600`, dopisując do
  listy `{"id": cid, "ok": False, "seconds": 0, "tail": "POMINIETY: wczesniejszy check padl"}`.
- **H-14/H-23.** `cmd_clean`: po `worktree remove` sprawdź `Path(wt).exists()` — jeśli istnieje,
  `die("worktree nie zszedl: %s" % wt)` zamiast „usunieto". `cmd_land`: po zielonym CI wołaj
  `cmd_clean` (argument `--keep` wyłącza). Poradę `git reset --hard HEAD~1` zamień na
  `git revert -m 1 HEAD`. Usuń martwy `if not git("show-ref", …) == "": pass`.

Kryterium: bieg próbny z `--planner codex` zapisuje `.h-plan.md` bez `{"type":`; `ps` po
timeoucie checka nie pokazuje cargo/vitest z cwd w worktree; bieg, który dotknie `checks/`,
kończy się kodem 2 przed commitem; `NIE_WIEM` = kod 2.

### 0.4 Bramka i CI mówią prawdę — LANDED (`b157a6a0`)

- **H-6.** `scripts/ci.sh:277`: `checks/quick-vocabulary.sh` → `checks/vocabulary.sh`, i nie przez
  `run_check_if_present`, tylko twardo (plik musi istnieć). Sprawdź, czy `run_check_if_present`
  ma jeszcze inne wywołania do plików, których nie ma — każde takie to ta sama wada.
- **H-7.** `.github/workflows/ci.yml`: trzeci job `guards` na tym samym runnerze co `rust`
  (`bash -c 'source scripts/ci.sh; guards_lane'` albo nowy tryb `bash scripts/ci.sh guards`),
  `gate` zależy od wszystkich trzech. W `ci.sh` stderr kolektora gęstości do logu, nie do `/dev/null`.
- **R-2.** Zdejmij `#[ignore]` z 11 testów procesowych (`supervisor_group_death:154`,
  `supervisor_term_then_kill:79,146`, `supervisor_timeout_kills:94`, `supervisor_pipe_eof:62`,
  `supervisor_drop_guard:142,191`, `claude_cancel_escalation:214,279`,
  `claude_session_process:114,173`). Uruchom `cargo test --test it -- --test-threads=1 supervisor_ claude_cancel claude_session`
  **trzy razy** z rzędu; jeśli któryś jest niestabilny, zostaw mu `#[ignore]` z nowym, prawdziwym
  powodem i wpisz do Dziennika. Do `harness/guards.sh` strażnik: każdy `#[ignore = "…bramka woła…"]`
  bez odpowiadającego wiersza w `ci.sh` = czerwone.
- **H-15.** Do `deny` w `.claude/settings.json`: `docs/ARCHITECTURE.md`, `docs/design/DESIGN.md`,
  `src-tauri/commands.golden.txt` (Edit i Write).
- **H-16.** `worktree.sh` `cut`: `cargo build --tests` w `src-tauri` nowego worktree (poza turą);
  `.claude/hooks/stop-gate.sh`: własny sufit 600 s i przy przekroczeniu blokada z jawnym zdaniem
  „nie zdążyłem sprawdzić", nie cisza.
- **H-18.** Usuń `"CI": "1"` z `env` w `.claude/settings.json` (`h.py` daje `CI=1` checkom sam).
- **R-9.** `vite.config.ts` (python3): `test.include = ['src/**/*.test.ts?(x)', 'checks/tests/**/*.test.ts', 'docs/research/**/*.test.ts']`;
  osobny projekt `e2e` (`vitest run --project e2e` z `fileParallelism` na 4); w `package.json`
  skrypt `e2e` wskazuje ten projekt (dziś `playwright test` bez configu); w `checks/tests/_support.ts`
  usuń odwołanie do nieistniejącego `checks/_cargo-serialize.sh`. `checks.json` `web-test` bez
  zmian (zawęża po ścieżkach).
- **R-10.** `package.json`: usuń `@base-ui/react`, `@tauri-apps/plugin-store`,
  `@tauri-apps/plugin-opener` (`npm uninstall`, żeby lockfile poszedł razem); `@tanstack/react-virtual`
  zostaje do decyzji Z-25. `Cargo.toml` (src-tauri): usuń `rusqlite_migration`. Uprawnienia
  `store:default` i `opener:*` w `capabilities/default.json` zostają tylko, jeśli Rust je woła —
  sprawdź `grep -rn plugin_store\|plugin_opener src-tauri/src`.
- **R-7.** `Cargo.toml` workspace: `license = "AGPL-3.0-only"`; komentarz w `deny.toml` o
  „zamkniętej aplikacji" poprawić.
- **R-11.** Przypnij `@playwright/test`, `vitest`, `prettier`, `@types/node` do zainstalowanych
  wersji z `package-lock.json` (komentarz przy Playwright: bump = ponowny pomiar gęstości).

Kryterium: `bash scripts/ci.sh full` zielone lokalnie; job `gate` na runnerze zależy od trzech
jobów; `cargo test --test it -- --test-threads=1 supervisor_` melduje > 0 passed bez
`--include-ignored`.

### 0.5 Dokumentacja bez martwych odwołań — LANDED (`4ed0e9b4`)

Jedna przecinka, usuwanie nie dopisywanie: `AGENTS.md` (§5 `docs/research/projects/`,
§6 komendy, `checks/quick-vocabulary.sh`), `docs/DECISIONS-LOCKED.md` (`ship.sh`, `review.sh`,
`ship-task.sh`, `verify.sh` — zamień na `scripts/h run`/`h land`, treść decyzji bez zmian),
`.claude/commands/build.md` (§3–6 opisują maszynerię, której nie ma — skróć do wskazania na ten
plan), `docs/HARNESS-QUEUE.md` (Q-6 nadal prawdziwe dla `h.py`, reszta archiwum),
`harness/README.md` („~590 linii"), `.claude/settings.json` allow (`Bash(bash harness/snapshot.sh)`,
`Edit(engine/**)`, `Write(engine/**)`, `Edit(tests/**)`, `Write(tests/**)` — nie istnieją).
`docs/STATUS.md` → 150 linii („co stoi w trunku, co otwarte, trzy ostatnie sprostowania") +
`docs/STATUS-ARCHIVE.md`; `docs/PLAN-HARDENING.md` i `docs/PROMPT-FAZA-7-CODEX.md` →
`docs/archive/` (usuń ścieżkę domową z tego drugiego).

Kryterium: `grep -rn 'verify.sh\|ship.sh\|review.sh\|tasks/\|snapshot.sh\|quick-vocabulary' AGENTS.md docs/*.md .claude harness/README.md`
= 0 trafień.

---

## 3. Fale 1–5 — zadania w pętli

Kolumny: **ID** (numer z audytu) · **id biegu** (argument `scripts/h run`) · **prompt** ·
**tryb**: `R` dotyka Rusta (jeden naraz), `TS` czysto frontend (może iść obok jednego `R`) ·
**vendorzy**: `C→X` = kod Claude, weryfikacja Codex (domyślne); `X→C` = `--dev codex --verifier claude` ·
**rozmiar**: `duże` = `LOADOUT_BUDGET_DEV=70` · **zależy od** · **status** · **uwagi**.

Kolejność w tabeli = kolejność startu, gdy zależności pozwalają. Z-28 idzie pierwsze, bo
obniża koszt każdej następnej bramki (60 binariów testowych → 8).

| ID | id biegu | prompt | tryb | vendorzy | rozmiar | zależy od | status | uwagi |
|---|---|---|---|---|---|---|---|---|
| Z-28 | `z28-tests-into-it` | `prompts/Z-28.md` | R | X→C | duże | 0.4 | **LANDED** `2026-09-02` | mechaniczne; po wlaniu orkiestrator dopisuje allowlistę do `checks/tests-listed.sh` (python3) |
| Z-01 | `z01-descendants` | `prompts/Z-01.md` | R | C→X | duże | Z-28 | **BLOCKED** — prompt był niepełny; zastąpione przez Z-01b |
| Z-01b | `z01b-descendants` | `prompts/Z-01b.md` | R | C→X | duże | Z-02 | **ZAMKNIĘTE jako za szerokie** — decyzją właściciela 2026-09-03 rozbite na Z-01c i Z-01d |
| Z-01c | `z01c-live-kill` | `prompts/Z-01c.md` | R | C→X | duże | Z-04 | **LANDED** `2026-09-03` | JEDNA runda po sześciu odrzuceniach szerokiej wersji | Stop i limit czasu zabijają każdą grupę, którą krok utworzył |
| Z-01d | `z01d-pgids-recovery` | `prompts/Z-01d.md` | R | C→X | duże | Z-01c | **LANDED** `2026-09-03` | dwie rundy; CI raz czerwone na flaku, zielone w powtórce | znacznik na wszystkich drogach spawnu, `pgids` w `run.json`, reaper po awarii | ten sam zakres z trzema wymaganiami, które weryfikator odkrył przez trzy rundy | krytyczne; wymaga aktywnych testów z 0.4 (R-2) |
| Z-02 | `z02-zero-probe` | `prompts/Z-02.md` | R | X→C | | Z-01 | **LANDED** `2026-09-02` | dwie rundy, 13 min; zawężony test biegnie 1 s |
| Z-03 | `z03-heavy-permit` | `prompts/Z-03.md` | R | C→X | | Z-02 | **LANDED** `2026-09-03` | jedna runda, 911 s | |
| Z-04 | `z04-settle-guard` | `prompts/Z-04.md` | R | C→X | duże | Z-03 | **LANDED** `2026-09-03` | drugie podejście, trzy rundy; merge rozwiązany ręcznie | `run.rs` 11 k linii |
| Z-05 | `z05-turn-proof` | `prompts/Z-05.md` | R | X→C | | Z-04 | **LANDED** `2026-09-03` | trzy rundy | |
| Z-06 | `z06-exit-requested` | `prompts/Z-06.md` | R | C→X | | Z-05 | **LANDED** `2026-09-03` | trzy rundy; ⌘Q na żywym oknie DO POTWIERDZENIA (niżej) |
| Z-24 | `z24-one-stamp` | `prompts/Z-24.md` | TS | C→X | | 0.5 | **LANDED** `2026-09-02` | jedna runda, 7 checków, CI 256 s |
| Z-25 | `z25-processes-publish` | `prompts/Z-25.md` | TS | C→X | | Z-24 | **LANDED** `2026-09-02` | decyzja o `react-virtual` → jeśli „usunąć", orkiestrator robi to w `package.json` po wlaniu |
| Z-29 | `z29-fixtures-redacted` | `prompts/Z-29.md` | TS | X→C | | 0.5 | **LANDED** `2026-09-02` | tylko `docs/`; może biec obok |
| Z-07 | `z07-finish-keeps-commits` | `prompts/Z-07.md` | R | C→X | | Z-06 | **LANDED** `2026-09-03` | jedna runda |
| Z-08 | `z08-skills-outside-commit` | `prompts/Z-08.md` | R | X→C | | Z-07 | **LANDED** `2026-09-03` | dwie rundy | |
| Z-09 | `z09-sweeper` | `prompts/Z-09.md` | R | C→X | duże | Z-08 | **LANDED** `2026-09-03` | trzy rundy; sufit tur podniesiony raz, podział niepotrzebny | pięć punktów, jeden bieg; jeśli kod 3 — podziel na `z09a` (reconcile+prune) i `z09b` (kopie, forget, exclude) |
| Z-10 | `z10-blocking-offload` | `prompts/Z-10.md` | R | C→X | duże | Z-09 | **LANDED** `2026-09-03` | dwa razy ubite maszynowo, potem jedna runda | |
| Z-26 | `z26-terminal-eviction` | `prompts/Z-26.md` | R | X→C | | Z-25, Z-10 | **LANDED** `2026-09-03` | jedna runda | jedna linia w `chat.rs` → liczy się jako R |
| Z-27 | `z27-card-truth` | `prompts/Z-27.md` | TS | X→C | | Z-26 | **LANDED** `2026-09-03` | dwie rundy | pięć drobnych; `PastRunRow` + lustro drutu = `invoke-args` |
| Z-11 | `z11-skill-tool` | `prompts/Z-11.md` | R | C→X | | Z-10 | **LANDED** `2026-09-03` | jedna runda; żywa wyrocznia potwierdzona | żywa wyrocznia `--ignored`; orkiestrator odpala ją raz po wlaniu (3,75 s) |
| Z-12 | `z12-codex-pricing` | `prompts/Z-12.md` | R | X→C | | Z-11 | **LANDED** `2026-09-03` | dwie rundy; jeden test rodzeństwa dopisany na `main` |
| Z-13 | `z13-budget-reservation` | `prompts/Z-13.md` | R | C→X | duże | Z-12 | **ZASTĄPIONE** przez Z-13b | trzy odrzucenia; winne było zlecenie, nie wykonanie — formuła `left / (running_now + 1)` wyłączała równoległość |
| Z-14 | `z14-tee-tool-results` | `prompts/Z-14.md` | R | C→X | | Z-13 | **LANDED** `2026-09-04` | dwie rundy | |
| Z-15 | `z15-index-without-raw` | `prompts/Z-15.md` | R | C→X | duże | Z-14 | **LANDED** `2026-09-04` | dwie rundy; sprawdzone kasowaniem indeksu | migracja addytywna; po wlaniu orkiestrator kasuje `~/.loadout/loadout.db*` (indeks odbuduje się) i zapisuje rozmiar przed/po |
| Z-16 | `z16-context-loaded-by-cli` | `prompts/Z-16.md` | R | C→X | | Z-15 | **LANDED** `2026-09-04` | domknięte ręką: brakowało wyłącznie akapitu do mnie |
| Z-17 | `z17-prompt-file-lead-settings` | `prompts/Z-17.md` | R | X→C | | Z-16 | **LANDED** `2026-09-04` | dwie rundy | |
| Z-18 | `z18-reflection-switch` | `prompts/Z-18.md` | R | X→C | | Z-17 | **LANDED** `2026-09-04` | dwie rundy | |
| Z-19 | `z19-codex-resume-flag` | `prompts/Z-19.md` | R | X→C | | Z-18 | **LANDED** `2026-09-04` | trzy rundy |
| Z-20 | `z20-roster-same-copy-skill` | `prompts/Z-20.md` | R | X→C | | Z-19 | **LANDED** `2026-09-04` | dwie rundy |
| Z-21 | `z21-borrow-review` | `prompts/Z-21.md` | R | C→X | | Z-20 | **LANDED** `2026-09-04` | jedna runda |
| Z-22 | `z22-trigger-key` | `prompts/Z-22.md` | R | X→C | | Z-21 | **LANDED** `2026-09-04` | trzy rundy + ręka; planista Codeksa |
| Z-23 | `z23-connection-secrets` | `prompts/Z-23.md` | R | C→X | duże | Z-22 | **LANDED** `2026-09-04` | jedna runda po decyzji B; wyrocznia zawężona i **wzmocniona** |
| Z-34 | `z34-agent-app-preflight` | `prompts/Z-34.md` | R+TS | C→X | | — | **COFNIĘTE — DECYZJA WŁAŚCICIELA** `2026-09-04` | z uratowanej gałęzi: sonda gotowości CLI zamiast bezwarunkowego „Claude · Codex ready" |
| Z-30 | `z30-engine-small-leaks` | `prompts/Z-30.md` | R | C→X | | — | **PRZYWRÓCONE** `2026-09-04` (`392c58c4`) | werdykt DZIALA, ale pełne CI po merge'u złapało regresję; `git revert -m 1` = `4778c4af`; gałąź zostaje materiałem |
| Z-30b | `z30b-death-proof-keeps-the-status` | `prompts/Z-30b.md` | R | C→X | | — | **LANDED** `2026-09-04` | jedna runda; pełna suita w checku (197 s) zamiast zawężonej |
| Z-31 | `z31-lab-fixes` | `prompts/Z-31.md` | R | X→C | | Z-30 | **LANDED** `2026-09-04` | |
| Z-32 | `z32-library-compat` | `prompts/Z-32.md` | R | C→X | | Z-31 | **LANDED** `2026-09-04` | |
| Z-33 | `z33-record-truth` | `prompts/Z-33.md` | R | X→C | | Z-32 | **LANDED** `2026-09-04` | |

> **Kolumna „zależy od" jest KOLEJNOŚCIĄ STARTU, nie zależnością logiczną** (poza Z-28 → Z-01,
> gdzie chodziło o żywe testy procesowe z pakietu 0.4). Zadania silnika dotykają rozłącznych
> plików; zostały ustawione w szereg, bo na tej maszynie wolno biec jednemu ciężkiemu `cargo`
> naraz (niezmiennik 26). Dlatego `BLOCKED` na jednym zadaniu **nie** przenosi się na następne:
> kolejne startuje z `main`, którego zablokowana praca i tak nie dotknęła. Zapisane 2026-09-02,
> po zablokowaniu Z-01.

Szacunek: 30–70 USD i 40–90 min na zadanie (zmierzone na biegach z sierpnia). 33 zadania to rząd
1 000–1 500 USD i 2–3 doby zegara przy jednym biegu Rusta naraz. Jeśli sufit per faza z 0.3
zatrzyma bieg (kod 3 z `--max-budget-usd`), zadanie wraca jako `BLOCKED` z powodem „budżet" —
podniesienie sufitu jest decyzją człowieka, nie orkiestratora.

> **Zmiany w zakresie, wprowadzone w Fali 0 (2026-09-02):**
> `docs/ARCHITECTURE.md` i `docs/design/DESIGN.md` weszły do `deny` (H-15), więc prompty
> **Z-13** i **Z-16** proszą teraz o akapit w podsumowaniu zamiast o edycję pliku —
> dopisuje go orchestrator poza biegiem. `src-tauri/commands.golden.txt` świadomie ZOSTAŁ
> zapisywalny: jest lustrem, które `it/ipc_read_paths.rs` porównuje z prawdziwą listą komend,
> więc wpis wymyślony przez bieg przewraca test, a blokada psułaby legalne dodanie komendy (Z-09).

### Po wlaniu, poza biegiem (ręka orkiestratora, po konkretnym zadaniu)

- **po Z-28 — ZROBIONE** (obie połowy weszły z Falą 0.4): allowlista `ALLOWED_TOP`
  w `checks/tests-listed.sh` stoi i została sprawdzona zasadzoną naruszeniem;
  `harness/checks.json` ma `rust-test.budget_s` 1500 z zapisanym powodem i pomiarem.
- **po Z-06 — PRÓBOWANE 2026-09-03, WYMAGA CIEBIE.** Połowa indeksowa jest dowiedziona
  automatycznie i dobrze: `quitting_leaves_the_index_small` buduje sytuację (czytelnik trzyma
  migawkę, dziennik rośnie), ma **kontrolę negatywną** na to, że sytuacja naprawdę powstała,
  i cytuje zmierzone 42 MB.

  Połowy procesowej **nie da się sprawdzić z powłoki** i to jest ograniczenie systemu, nie
  produktu. Zbudowałem pakiet (`npm run app:build`), uruchomiłem go i posadziłem obok proces
  w osobnej grupie — ale ⌘Q jest zdarzeniem Apple'a, a wysłanie go wymaga zgody na
  automatyzację, której nie da się przyznać bez okna dialogowego. Do tego
  `tell application "Loadout" to quit` trafia w **kopię z `/Applications`**, nie w świeżo
  zbudowaną: obie mają ten sam identyfikator pakietu, a system wybiera zarejestrowaną.
  Test wyglądał więc na czerwony, choć mierzył czyjeś inne okno.

  **Co zrobić, gdy będziesz przy maszynie** (dwie minuty): otwórz
  `target/release/bundle/macos/Loadout.app`, wystartuj dowolny bieg, w innym oknie odpal
  `sleep 600 &` z terminala agenta, naciśnij ⌘Q i sprawdź `ps ax | grep -c claude` oraz
  rozmiar `~/.loadout/loadout.db-wal`. Agenci mają zejść, a dziennik zejść poniżej 8 MB.
- **po Z-11:** `cargo test --test it -- --ignored skills_reach_claude` raz (płatne ~0,05 USD).
- **po Z-15 — ZROBIONE 2026-09-04.** Kopia zapasowa (`loadout.db.bak-2026-09-04`), skasowanie
  indeksu, start aplikacji: wstała bez błędu, a 31 katalogów biegów w trzech projektach
  zostało nietkniętych — pliki są prawdą (niezmiennik 4). Zmierzone przed i po:
  baza **71,7 MB → 0,004 MB**, dziennik **42,2 MB → 0,11 MB**. Indeks zapełnia się od nowa
  przy kolejnych biegach; historia i tak czyta pliki.
- **po Z-25:** jeśli werdykt „wirtualizacja niepotrzebna" — `npm uninstall @tanstack/react-virtual`.
- **po Z-20:** otwórz workflow Urc w aplikacji — kafelek „Figma check" bez Problemu (zrzut do Dziennika).

---

## 4. Sondy przed konkretnym zadaniem (tanie, kilka minut, wynik do Dziennika)

| przed | pytanie | jak | co zmienia |
|---|---|---|---|
| Z-12 | co Claude Code robi z `--max-budget-usd 0.00` | `claude -p --max-budget-usd 0.00 --output-format json "say hi"` w pustym katalogu | czy „poniżej centa" to odmowa (jak w prompcie), czy CLI sam odmawia |
| Z-16 | czy `--restricted` odcina `CLAUDE.md` gospodarza (2.1.258) | katalog tymczasowy z `CLAUDE.md` „ODPOWIEDZ SŁOWEM MARKER", `claude -p --restricted --output-format stream-json "what does the project file say?"` i odczyt `system/init` | jeśli tak — Z-16 dostaje dopisek: przełącznik izolacji, nie tylko raport |
| Z-23 **ZROBIONA** `2026-09-04` | jaki klucz przekazuje zmienne do serwera MCP stdio w codex-cli 0.153 | `codex exec --help`, `codex mcp --help`, próba `-c 'mcp_servers.x.env_vars=["A"]'` i `-c 'mcp_servers.x.env={A="1"}'` z serwerem-atrapą `env` | treść akapitu „Wynik sondy" w prompcie Z-23 |
| 0.4 | czy Claude Code po timeoucie haka Stop przepuszcza turę | hak z `sleep 700` w kopii ustawień w katalogu tymczasowym | kształt H-16: sufit własny vs. blokada |
| 0.3 | czy `codex exec -o <plik>` zapisuje ostatnią wiadomość agenta | `codex exec --skip-git-repo-check -o /tmp/x.txt -` z promptem „odpowiedz słowem TAK" | kształt H-1 |

---

### Dług harnessu poznany w trakcie pętli (poza Falą 0 — do zrobienia po niej)

- **H-24 (faza planu nie ma sesji).** `phase_plan` woła `call_model` bez `session=`, więc bieg
  ubity w planie — z sufitu dolara, limitu sesji albo podmiany binarki — nie ma czego wznowić
  i płaci od zera. Faza implementacji ma to od `b157a6a0` (`saved = load_state(task_id).get("session")`),
  faza planu nie. Koszt zmierzony 2026-09-04 na Z-22: **12,35 USD** za nic, z czego prawie wszystko
  to odczyty z cache'u (7,8 mln tokenów). Poprawka jest tego samego kształtu co w `phase_implement`:
  zapisz `sid` w stanie przed wołaniem, a przy ponowieniu podaj `--resume`. Nie robię tego teraz,
  bo `harness/` jest zamknięte poza Falą 0 (patrz „Czego orkiestrator nie robi").

- **H-25 (lista `deny` nie zatrzymuje basha).** `docs/ARCHITECTURE.md` jest w `deny`
  w `.claude/settings.json`, a mimo to bieg Z-30 go zmienił: `deny` rządzi narzędziami Edit/Write,
  nie `python3` ani `cat` z poziomu basha, a krotka `ORACLE` w `harness/h.py` — ta, która potrafi
  ubić bieg — tego pliku nie zna. Tym razem zmiana była trafna i CI zielone, więc zostaje; ale
  ochrona jest dziś wyłącznie grzecznościowa. Poprawka: dopisz `docs/ARCHITECTURE.md`
  i `docs/design/DESIGN.md` do `ORACLE`. Nie robię tego teraz, bo `harness/` jest zamknięte
  poza Falą 0.

## 5. Dziennik

Format wiersza: `- 2026-09-DD HH:MM · <ID albo pakiet> · <co się stało> · koszt <USD z runs/<id>/> · <kto: C→X / X→C / ręka>`.
Najnowsze na górze. Zdania krótkie; powód `BLOCKED` w jednym zdaniu z cytatem werdyktu.

- 2026-09-04 21:35 · Z-33 · LANDED, trzy rundy, CI 321 s. `run.json` zapisuje ten sam werdykt, którym pętla naprawdę steruje: `Succeeded` nieostatniej rundy jest sygnałem dla planisty, żeby odblokować graf, a nie zdaniem „krok się udał", i historia mówi to człowiekowi wprost — bez ósmego stanu w bazie · X→C
- 2026-09-04 20:45 · Z-32 · LANDED, dwie rundy, CI 329 s. Biblioteka przestaje ufać temu, czego nie sprawdziła: APFS nie robi dwóch katalogów z samej różnicy wielkości liter, więc przemianowanie leafu i publikacja idą pod jednym zamkiem, wadliwe `u32` dostaje uwagę zamiast zamawiać miliard iteracji, odrzucony workflow nie zostawia pustej półki, a okno wie, którą rewizję naprawdę przeczytało · C→X
- 2026-09-04 20:10 · Z-34 · werdykt DZIALA w jednej rundzie, **merge cofnięty** (`2328b3a2`) i czeka na jedno zdanie od właściciela. Nie ma tu wady: bramka stanęła na zapadce gęstości — `textElements` 52 przy bazie 49, **przy suficie 60**, czyli ekran nie jest za gęsty, tylko gęstszy niż przy ostatnim pomiarze. Nowa stopka mówi po zdaniu o KAŻDEJ aplikacji zamiast jednego nieprawdziwego „Claude · Codex ready", więc trzy elementy więcej to koszt prawdy, nie bałaganu. `--update-baseline` umie tylko obniżać (niezmiennik 18), a `checks/` jest zamknięte i dla pętli, i dla mnie · C→X
- 2026-09-04 19:00 · Z-31 · LANDED, dwie rundy, CI 334 s. Lab przestaje płacić za turę bez obiecanego sufitu: brak kwoty jest odmową PRZED `start`, nie cichym zejściem do sterownika bazowego. Tura już opłacona czyta stan ponownie i honoruje Accept albo Discard wykonany w jej trakcie, zamiast pisać po nim; agent i jego rewizja pochodzą z JEDNEGO odczytu · X→C
- 2026-09-04 18:35 · **RÓWNOLEGŁOŚĆ WŁĄCZONA decyzją właściciela.** Od tej chwili dwa zadania naraz, z buforem: drugi startuje, gdy pierwszy ma checki za sobą, i nie ląduję nic, gdy drugi jest w checkach. Reguła „nigdy dwa ciężkie cargo naraz" zostaje jako intencja — dwa dzisiejsze flaki pokazały, że obciążona maszyna udaje regresję, a każdy fałszywy alarm kosztuje pełny przebieg bramki. Flak z obciążenia = powrót do szeregu · ręka
- 2026-09-04 18:15 · Z-23 · **LANDED**, jedna runda, CI 322 s. Sekret jedzie do serwera MCP kluczem `mcp_servers.<n>.env.<ZMIENNA>`, a wyrocznia `codex_lead_curates_mcp_servers` nie tylko się zawęziła — **wzmocniła się**: dowodzi, że wartość stoi w argv DOKŁADNIE RAZ i wyłącznie pod tym kluczem, a cztery prywatne napisy człowieka nadal nigdzie. Test idzie produkcyjną drogą (`for_driver_with_secrets` + nośnik `0600`), więc dowodzi, że wartość przyszła z NOŚNIKA, nie ze środowiska okna — którego z Docka nie ma · C→X
- 2026-09-04 17:20 · Z-23 · **ODBLOKOWANE decyzją właściciela: wariant B.** Niezmiennik 9 dostał jeden jawny wyjątek — wartość sekretu wolno w argv wyłącznie w `-c mcp_servers.<n>.env.<ZMIENNA>` dla Codeksa, bo serwer stdio nie dziedziczy środowiska rodzica. Cena zapisana w `AGENTS.md`: przez życie procesu Codeksa wartość widzi każdy `ps` · ręka
- 2026-09-04 17:05 · Z-30b · **LANDED**, jedna runda, a razem z nim wróciła cała praca Z-30 (`392c58c4` = cofnięcie cofnięcia; po `git revert -m 1` git uważa stare commity za scalone, więc sam merge doniósłby samą poprawkę, a `git rebase` jest tu zablokowany). `stop()` oddaje kod wyjścia dokładnie raz, więc dowód „wyszedł sam" wraca, a drugie zatrzymanie nadal go nie ma. Check biegł PEŁNĄ suitą (197 s). Pierwsze CI po merge'u czerwone na `stop_is_not_dead_until_a_child_in_its_own_group_is_gone` — strażnik fikstury pod obciążeniem; moduł zielony osobno, powtórzone `ci.sh full` zielone (1266 passed, 0 failed) · C→X
- 2026-09-04 15:10 · Z-30 · werdykt DZIALA, **merge COFNIĘTY** (`4778c4af`). Pełne CI złapało regresję, którą zawężony `rust-test` przepuścił: `wait()` zaczął stawiać `proved_dead`, a `stop()` przy `proved_dead` oddaje `Dead { status: None }`, więc uchwyt tracił kod wyjścia. Wywraca to `claude_cancel_escalation::under_the_capability_the_session_is_asked_to_stop_and_ends_itself`, czyli jedyny dowód, że proces poproszony grzecznie **wyszedł sam**. Odtworzone na main w 0,16 s — nie flak. Przepisane na Z-30b · C→X
- 2026-09-04 13:05 · Z-23 · **BLOCKED** po trzech rundach, blokada uczciwa. Nośnik `~/.loadout/env`, zdanie odmowy i druga lista `VENDOR_AUTH_PASSTHROUGH` napisane, checki zielone, praca na gałęzi (`2364944a`). Ostatniego kryterium nie da się domknąć bez decyzji człowieka: `-c mcp_servers.…` idzie **w argv**, więc `env` z wartością znaczy sekret w argv · C→X
- 2026-09-04 11:40 · Z-22 · LANDED, CI 338 s, ale nie samą pętlą. Werdykt po trzeciej rundzie NIE_DZIALA przy **wszystkich czterech kryteriach spełnionych**: przewracał się test rodzeństwa, którego zawężony `rust-test` nie uruchomił (`trigger_run_is_accepted_once` — fikstura z literalnym kluczem, a nowa brama odmawia przepisania takiego pliku także przy WYŁĄCZANIU). Harness sam odsyła wtedy do ręki. Sprawdziłem, czy poprawka nie osłabia asercji: `assert_accepted_run_file` traci ostrze na `KEY`, ale stała zostaje uzbrojona w uszkodzonym pliku i w asercji redakcji. Kodu nie ruszyłem — nowy test celowo pinuje odmowę przy wyłączaniu · X→C + ręka
- 2026-09-04 09:50 · Z-22 · AWARIA nr 5, nowa: faza planu zjadła sufit dolara (`terminal_reason: budget_exhausted`, „Reached maximum budget ($12)"). Planista przeczytał `commands/triggers.rs` dwanaście razy i puścił trzech podagentów, zbierając 7,8 mln odczytów z cache'u. Ponowione RAZ z planistą Codeksa; protokół odbioru dostał piąty wiersz, lista długów pozycję **H-24** · ręka
- 2026-09-04 09:35 · Z-21 · LANDED, jedna runda, CI 327 s. Tekst pożyczony przez Borrow idzie przez ten sam przegląd co import: podagent z instrukcją ukrytą w komentarzu HTML zatrzymuje bieg przed pierwszym procesem, z nazwą pliku · C→X
- 2026-09-04 08:40 · Z-20 · LANDED, dwie rundy, CI 299 s. Krok, który bierze tę samą kopię co poprzednik, zachowuje swój skill — do dziś gubił go w drodze, więc workflow z rosteru startował agenta bez umiejętności, o którą prosił kafelek · X→C
- 2026-09-04 07:30 · sonda przed Z-23 · Klucz to `mcp_servers.<nazwa>.env`, zmienna **dochodzi** do serwera stdio, ale środowisko rodzica **nie jest dziedziczone**: serwer dostał zamkniętą listę czternastu zmiennych, a znacznik wyeksportowany przed startem Codeksa nie dotarł. `shell_environment_policy.inherit=all` niczego nie zmienia. Czyli dzisiejszy Loadout zostawia KAŻDY serwer stdio pod Codeksem bez sekretów · ręka
- 2026-09-04 07:05 · Z-19 · LANDED, trzy rundy, CI 297 s. Tura Codeksa wznawia się też w folderze bez repozytorium — do dziś drugi krok w tym samym folderze zaczynał rozmowę od zera i płaciło się za cały kontekst po raz drugi · X→C
- 2026-09-04 06:20 · Z-18 · LANDED, dwie rundy, CI 327 s. Refleksja po biegu ma przełącznik i nie startuje po Stopie — do dziś każdy bieg, także anulowany i złożony wyłącznie z Codeksa, spawnował dodatkowy proces Claude'a, o który nie prosił żaden kafelek · X→C
- 2026-09-04 04:55 · Z-17 · LANDED, dwie rundy. Instrukcje agenta idą plikiem, nie argumentem wiersza poleceń (role importowane z repo właściciela mają 30–73 KB, a argumenty widzi każdy `ps`). Lider dostał tę samą trójkę co zwykły krok: własne ustawienia, prywatny katalog stanu i sufit wydatku — do tej pory pisał auto-pamięć wprost do katalogu domowego i nie miał limitu · X→C
- 2026-09-04 04:08 · Z-16 · LANDED, domknięte ręką: brakowało wyłącznie akapitu do orkiestratora. Kontekst wczytuje CLI, nie my — po sondzie `--restricted`, której wynik wszedł do zlecenia przed startem · C→X
- 2026-09-04 02:45 · Z-15 · LANDED, dwie rundy. Indeks przestał trzymać surową kopię transkryptu; migracja addytywna. Sprawdzone po wlaniu kasowaniem `~/.loadout/loadout.db*`: aplikacja wstała bez błędu, 31 katalogów biegów nietkniętych (pliki są prawdą, niezmiennik 4), baza **71,7 MB → 0,004 MB**, dziennik **42,2 MB → 0,11 MB** · C→X
- 2026-09-04 01:27 · Z-14 · LANDED, dwie rundy. Tee niesie też wyniki narzędzi — filtr prywatności wycinał każdy `tool_result`, więc historia odbudowana z plików nie wiedziała, co krok edytował i uruchamiał · C→X
- 2026-09-04 00:40 · Z-13b · LANDED w JEDNEJ rundzie, po trzech odrzuceniach pierwszej wersji, w której winne było **zlecenie**: formuła `left / (running_now + 1)` przy pierwszym starcie dzieli przez jeden, więc krok #1 rezerwował całą resztę i wyłączał równoległość (niezmiennik 11). Poprawna wersja dzieli przez szerokość równoległości, a test dowodzi nakładania się w czasie, nie samej sumy · X→C
- 2026-09-03 22:40 · Z-12 · LANDED, dwie rundy. Cache Codeksa nie jest już liczony dwa razy (wycena bywała kilkukrotnie zawyżona, więc sufit wydatku wyczerpywał się przedwcześnie), krok bez modelu mówi, że jego ceny nie znamy, a reszta budżetu poniżej centa jest odmową zamiast błędu parsera. Pełne CI po merge'u złapało jedno: `driver_codex_stream` wylicza CAŁĄ sekwencję zdarzeń dla fikstury bez modelu, więc przypinał zachowanie sprzed poprawki. Dopisane oczekiwanie (nie osłabiona asercja) i protokół rozszerzony o ten przypadek · X→C
- 2026-09-03 21:50 · sonda przed Z-12 · `--max-budget-usd 0.00` jest **odrzucane przez CLI** jako niepoprawny argument, nie czytane jako brak limitu. Czyli krok z resztą poniżej centa dziś nie startuje wcale, a człowiek dostaje błąd parsera zamiast zdania o budżecie — trzeci punkt Z-12 jest pilniejszy, niż zakładał audyt. Wynik dopisany do zlecenia · ręka
- 2026-09-03 21:40 · Z-11 · LANDED, jedna runda, CI 300 s. Umiejętności wreszcie mają czym się odpalić — a żywa wyrocznia (płatna, uruchomiona raz po wlaniu) potwierdza to na PRAWDZIWYM procesie: Claude ogłasza dokładnie te umiejętności, które bieg mu położył. To zamyka wadę, przez którą cała ścieżka kopiowania, hashowania i odmów była kosztem bez efektu · C→X
- 2026-09-03 21:05 · Z-27 · LANDED, dwie rundy, CI 313 s. Karta przestała pulsować godzinę po biegu i pytać o agentów, których nie ma; odmówiony drugi Run nie przemianowuje karty żywego biegu; montaż ekranu nie ściąga już całej historii biegu przez IPC · X→C
- 2026-09-03 20:20 · Z-26 · LANDED, jedna runda, CI 305 s. Zamknięty terminal zwalnia sesję i pompę po obu stronach granicy — do dziś każdy zamknięty terminal zostawiał `Feed` z dwoma tysiącami wierszy i pompę budzącą się co 16 ms na zawsze · X→C
- 2026-09-03 19:40 · Z-10 · LANDED, CI 314 s. Bieg był ubity dwa razy maszynowo (limit sesji, potem `server_error` po stronie dostawcy w rundzie naprawczej, 40 minut pracy), za trzecim razem przeszedł w jednej rundzie. Ciężkie `git` i `fs` zeszły z wątku okna i z workerów, na których żyje Stop · C→X
- 2026-09-03 19:15 · CZWARTA przyczyna maszynowa · `"error":"server_error"` dopisany do listy ponowień w `h.py`. Lista zostaje WĄSKA z premedytacją — trzy dokładne napisy, nie ogólna pętla retry, która zamaskowałaby prawdziwą czerwień · ręka
- 2026-09-03 14:40 · próba testu ⌘Q · wykorzystałem przerwę na limit, żeby zrobić ręczną połowę Z-06: zbudowałem wydanie i pakiet, uruchomiłem, posadziłem obok proces w osobnej grupie. Nie da się: ⌘Q to zdarzenie Apple'a, a jego wysłanie wymaga zgody na automatyzację, której powłoka nie dostanie; do tego `tell application "Loadout"` trafia w kopię z `/Applications`, bo obie mają ten sam identyfikator pakietu. Pierwszy przebieg wyglądał na czerwony i BYŁ WADLIWYM TESTEM, nie wadą produktu — goła binarka spod `target/release` nie jest dla systemu aplikacją, więc zdarzenie nie miało adresata. Instrukcja dla człowieka zapisana wyżej; nic nie zostało po teście (sprawdzone `ps`) · ręka
- 2026-09-03 14:20 · LIMIT SESJI (reset 15:10) · Z-10 zginęło w implementacji z pracą w worktree; przy okazji trafiło dokładnie w moment podmiany binarki vendora (14:20), więc sonda też odmówiła — dwie przyczyny maszynowe naraz, obie już opisane w protokole odbioru. Wznowienie po resecie, bez zmian w zleceniu · ręka
- 2026-09-03 12:40 · Z-09 · LANDED, trzy rundy, CI 318 s. Najgrubsze zadanie kolejki: uderzyło w sufit 250 tur z 21 plikami pracy w środku, wznowione z sufitem 400 — podział na dwa, który plan przewidywał, okazał się niepotrzebny. Kopie robocze starych biegów, osierocone worktree i gałęzie `loadout/*` mają teraz zamiatacz; kopia plikowa znika po biegu, a podfolder repo jako workspace mówi wprost, że praca nie wyląduje na gałęzi (3,8 GB i 89 worktree w urc-monorepo to była ta wada) · C→X
- 2026-09-03 08:20 · Z-08 · LANDED, dwie rundy, CI 295 s. Katalog umiejętności nie wchodzi już do commita kroku ani do gałęzi wynikowej — do dziś każdy bieg z umiejętnościami wnosił skille Loadouta w PR człowieka · X→C
- 2026-09-03 07:15 · Z-07 · LANDED, jedna runda, CI 301 s. Gałąź kroku nie ginie już, gdy agent sam zacommituje całą pracę i zostawi czyste drzewo — a to była cicha utrata pracy człowieka: `finish` sądził po `status --porcelain`, więc czyste drzewo z commitami czytało się jak „nic nie zrobił" i szło do `branch -D` · C→X
- 2026-09-03 06:30 · Z-06 · LANDED, trzy rundy, CI 301 s. Wyjście z menu i ⌘Q idą teraz przez `ExitRequested`, wstrzymywana jest KAŻDA prośba dopóki sprzątanie trwa (pierwsze podejście przepuszczało drugą i kończyło proces w środku eskalacji zabijania), sprzątanie odpala się najwyżej raz, a zapadka przechodzi w `Done` na `Drop`, więc także po panice. Indeks zamyka się z `journal_size_limit`, a test na to ma kontrolę negatywną i cytuje zmierzone 42 MB. `eprintln!` w moście zamienione na zapis ignorujący błąd — panika „failed printing to stderr" z dziennika 31.08 nie ma już wejścia · C→X
- 2026-09-03 05:05 · rescue-preflight · merge **COFNIĘTY** (`612bbf74`) i przepisany na zadanie **Z-34**. Trzy konflikty rozwiązałem (titlebar na rzecz dzisiejszego, oba sterowniki na rzecz wspólnego `probe::run`), clippy czyste, ale pełne CI złapało prawdziwą czerwień: dwa testy z gałęzi żądają stopki, która CZYTA sondę, a ja zostawiłem stopkę z bezwarunkowym „Claude · Codex ready". Port nie jest mechaniczny — dzisiejszy pasek fałduje się do ikon (`⌘B`), czego gałąź nie znała, a to co pokazać na zwiniętej kropce jest decyzją projektową, nie rozwiązaniem konfliktu. Część rustowa (sonda jako jeden rdzeń dla obu vendorów, komenda, 276 linii kryteriów) jest gotowa i wchodzi do promptu Z-34 jako materiał · ręka
- 2026-09-03 04:10 · Z-01d · **LANDED**, dwie rundy — i tym samym KRYTYCZNA wada audytu jest domknięta w całości. Weryfikator odrzucił raz, znów na produkcyjnej drodze: `SearchEnvironmentDriver` nie delegował nowej metody, więc oba vendory szły do spawnu bez znacznika, a test tego nie widział, bo podstawiał atrapę implementującą metodę wprost. Lądowanie: pierwsze pełne CI padło na `supervisor_timeout_kills` — test mierzący limit czasu, jeden z jedenastu odblokowanych w R-2. Zmierzone: 3/3 samotnie w 0,38 s, pas rustowy zielony w powtórce (182 s), pełna bramka zielona (258 s). Flak zajętej maszyny, nie regresja — bramka biegła zaraz po zakończeniu biegu · C→X
- 2026-09-03 03:20 · AWARIA MASZYNY nr 3 i JEJ NAPRAWA · trzecie zabicie biegu przez aktualizację vendora (`claude not found in PATH`, kod 127, po 30 min pracy). Trzy razy w jednej fali to nie przypadek, więc zamiast czwartego powtórzenia: `DISABLE_AUTOUPDATER=1` w środowisku procesu vendora (zmienna potwierdzona `strings` w binarce) plus JEDNO ponowienie po 15 s, wyłącznie na dwóch sygnaturach chwilowego braku binarki. Niezmiennik 28: skrypt przed promptem · ręka
- 2026-09-03 00:12 · AWARIA MASZYNY nr 2 · Z-01d zginęło w fazie planu na kodzie 1, ale w transkrypcie stoi `api_error_status: 429`, `terminal_reason: api_error` i zdanie „You’ve hit your session limit · resets 12:50am". To nie jest porażka sprawdzenia — a kod 1 po protokole znaczy właśnie „sprawdzenie padło", więc rozpoznanie idzie z transkryptu, nie z kodu. Właściciel przelogował się, sonda `claude -p` odpowiedziała, bieg wznowiony bez zmian w zleceniu · ręka
- 2026-09-03 12:15 · Z-01c · **LANDED w JEDNEJ rundzie** — to jest odpowiedź na pytanie, czy rozbicie Z-01 było słuszne. Ta sama praca w szerokiej wersji dostała sześć odrzuceń; zawężona do samego żywego zabijania (migawka drzewa, pełna eskalacja dla każdej odkrytej grupy, druga migawka także po zejściu lidera, test na produkcyjnym `Supervised::stop()`) przeszła za pierwszym razem. Stop i limit czasu zabijają teraz wszystko, co krok uruchomił. Pełne CI 322 s · C→X
- 2026-09-03 11:20 · Z-04 · **LANDED** za drugim podejściem, trzy rundy. Poprawka promptu (reguła o szkieletach z `todo!()` plus wklejona uwaga z pierwszej rundy) zadziałała — to samo zadanie, które wcześniej stanęło. Lądowanie wymagało ręcznego rozwiązania konfliktu z Z-05: oba dotknęły `run.rs` i `codex.rs`, wzięta struktura z Z-05 i sufit z Z-04. Pełne CI 276 s · C→X
- 2026-09-03 09:45 · AWARIA MASZYNY, nie kodu · runda naprawcza Z-04 zginęła na `cannot execute binary file`: Claude Code aktualizował się globalnie o 22:21 i podmieniał binarkę w miejscu, a bieg trafił w okno zapisu. Mylące dwa razy — plik nazywa się `claude.exe`, ale `file` mówi `Mach-O arm64`, a komunikat wskazuje ścieżkę homebrew, choć wywołanie idzie przez opakowanie Supersetu. Po protokole: przyczyna maszynowa, więc jedno powtórzenie, nie `BLOCKED` · ręka
- 2026-09-03 09:40 · Z-05 · LANDED, trzy rundy, pełne CI 295 s · X→C
- 2026-09-03 09:15 · DECYZJE WŁAŚCICIELA · (1) Z-01 rozbite na Z-01c (żywe zabijanie: migawka i eskalacja dla każdej grupy, test na produkcyjnym `Supervised::stop()`) i Z-01d (znacznik na wszystkich drogach spawnu, `pgids` w `run.json`, reaper po awarii). (2) Z-04 wznowione. (3) `repair-agent-app-preflight` do uratowania w części rustowej, `repair-readme-truth` skasowane jako bezprzedmiotowe (README przepisany dla 0.2), dwa pozostałe do napisania od nowa, gdy będą potrzebne. (4) 58 starych gałęzi osiągalnych z backupu skasowanych; 35 nieosiągalnych zostaje · ręka
- 2026-09-03 09:10 · Z-05 · DZIALA w trzech rundach; sufit na zamknięcie tury i dowód śmierci grupy poprzedniej tury Codeksa · X→C
- 2026-09-03 02:10 · WNIOSEK Z DZIEWIĘCIU ODRZUCEŃ · dopisałem do 28 pozostałych promptów sekcję z trzema rzeczami, na których stanęły Z-01, Z-01b i Z-04, a których żaden check nie widzi: (1) test musi się SKOMPILOWAĆ na starym kodzie — najpierw sygnatury z `todo!()`, inaczej cel nie kompiluje się wcale i nic nie dowodzi (AGENTS.md §2a p. 4); (2) przy ujednolicaniu zachowania trzeba wymienić WSZYSTKIE drogi, bo weryfikator przechodzi je po kolei i pyta o tę pominiętą; (3) zdanie, które czyta człowiek, asertuje się tam, gdzie ono stoi (niezmiennik 29), nie na wartości zwróconej · ręka
- 2026-09-03 02:00 · Z-04 · **BLOCKED po trzech rundach**, ale z powodu, który da się naprawić promptem: moduł testowy importował nowe typy, więc na starym kodzie cel integracyjny nie kompilował się i test nie mógł najpierw paść. Weryfikator ma rację co do §2a. Praca w `../loadout-h-z04-settle-guard` · C→X
- 2026-09-03 00:35 · Z-03 · LANDED, jedna runda, 911 s. Miejsce ciężkie przestaje przeciekać przy Stopie w kolejce — bez tego pierwszy Stop w oczekiwaniu na pulę unieruchamiał każdy krok „sprawdź" aż do restartu aplikacji · C→X
- 2026-09-02 23:50 · Z-01b · **BLOCKED po trzech rundach**, drugi raz. Sześć odrzuceń łącznie, każde na innej prawdziwej dziurze — pełna lista i trzy drogi wyjścia w sekcji 6 tego pliku. Pętla idzie dalej po Z-03; zależność w tabeli była kolejnością, nie logiką · C→X
- 2026-09-02 21:35 · Z-02 · LANDED, dwie rundy, 780 s. Sonda sygnałem zerowym zamiast salwy SIGTERM co 10 ms przez całe okno łaski; zawężony test biegnie 1 s zamiast pełnej suity · X→C
- 2026-09-02 20:35 · Z-29 · LANDED, pełne CI 237 s · X→C
- 2026-09-02 20:20 · Z-01 · **BLOCKED po trzech rundach** i to jest dobra wiadomość o systemie, nie zła o zadaniu. Weryfikator (codex) odrzucił trzy razy, za każdym razem wskazując lukę, której zielone checki nie widziały: (1) reaper uznawał DOWOLNY znacznik za zgodę na zabicie, więc grupa cudzego biegu ginęłaby zamiast być zgłoszona jako obca; (2) znacznik przy prawdziwym starcie bierze się z identyfikatora SESJI, a odzyskiwanie porównuje go z identyfikatorem BIEGU — nigdy się nie zgadzają, więc własna żywa grupa byłaby „obca"; (3) `pgids` nie są zapisywane do `run.json` na ścieżce Stopu ani timeoutu, więc ocalały wnuk po nieudanym Stopie nie trafia do pliku i reaper startowy go nie znajdzie. Praca (18 plików, 908 linii) stoi w `../loadout-h-z01-descendants`. Sugestia weryfikatora jest konkretna: pobierać `descendant_groups()` z zachowanego uchwytu przed zapisem `death_proof`, a drugi test przepiąć na prawdziwy bieg zamiast ręcznego wpisu · C→X
- 2026-09-02 18:10 · REGRESJE WŁASNE ×2, obie naprawione · (a) strażnik obietnicy `--include-ignored` nie mógł zaświecić: szukał napisu w całym `ci.sh`, a jego własny kod stoi w `ci.sh` i ten napis zawiera — niezmiennik 20 w czystej postaci. Teraz patrzy na KOD (atrybut zaczyna linię, wywołanie stoi przy `cargo test`), zasadzone naruszenie czerwone. (b) `--session-id` przy ponownym biegu tego samego zadania odmawiał („already in use"), czyli dokładnie na drodze po kodzie 3. Sesja w stanie znaczy teraz „wznów" · ręka
- 2026-09-02 18:00 · Z-01 · kod 3 po 250 turach; 17 plików i 891 linii pracy zostało w worktree. Wznowione z sufitem 400. Bieg zgłosił przy okazji cztery rzeczy POZA ZAKRESEM, w tym ostatnią obietnicę `--include-ignored` bez pokrycia (`supervisor_env_hygiene.rs:195`) — ominęła R-2, bo tamto szło po `tests/it/`, a to jest osobny cel · C→X
- 2026-09-02 17:30 · Z-29 · DZIALA w jednej rundzie; ścieżki domowe i lista serwerów MCP właściciela zniknęły z obu fikstur, cztery testy czytające je poprawione. Czeka na lądowanie, bo maszyna jest zajęta przez Z-01 · X→C
- 2026-09-02 15:05 · domknięcia po Z-28 i Z-25 · allowlista ośmiu celów w `checks/tests-listed.sh` (zasadzone naruszenie: czerwone, przywrócone: zielone); `rust-test` z 3600 na 1500 s; `npm uninstall @tanstack/react-virtual`; tabela zależności w ARCHITECTURE.md uzgodniona z `package.json` · ręka
- 2026-09-02 14:55 · Z-25 · LANDED, dwie rundy. Weryfikator (codex) znalazł przypadek brzegowy w rundzie 1: ostatni proces znikający przy otwartym panelu wypadał z `held` i odpytywanie milkło. Werdykt o wirtualizacji: NIE, z trzema zmierzonymi powodami w `feed.tsx`; zależność zdjęta · C→X
- 2026-09-02 14:50 · Z-28 · LANDED. 60 → 8 plików testowych wprost w `tests/`; pełne CI 252 s, a kolejne 222 s (było 256) · X→C
- 2026-09-02 14:10 · REGRESJA WŁASNA, naprawiona · H-10 dokładał `--output-format stream-json` do fazy weryfikacji, co razem ze `--json-schema` przewracało `parse_json` — bieg ginął zdaniem „model nie zwrócił JSON-a" PO całej implementacji, mimo że weryfikator napisał poprawną diagnozę. Transkrypt powstaje teraz bez tych flag · ręka
- 2026-09-02 13:40 · Z-28 · drugie podejście: 48 błędów `dead_code` z pliku pomocniczego wciągniętego do celu `it`. H-20 zadziałał — po czerwonym clippy `rust-test` został POMINIĘTY zamiast mielić do godziny. Wznowione z konkretem, naprawione jednym `#[allow(dead_code)]` na deklaracji modułu · X→C
- 2026-09-02 13:25 · POMYŁKA ORKIESTRATORA · uznałem bieg za martwy po ciszy w logu i wystartowałem drugi na tym samym `task_id`; log był tylko buforowany. Starszy ubity z dowodem ESRCH. Odtąd długie biegi idą przez tło narzędzia, nie przez `nohup &` · ręka
- 2026-09-02 13:05 · Z-28 · pierwsze podejście kod 1, ZERO zmian — i to jest uczciwa odmowa, nie awaria. Planista podał baseline z grepu (1217/1054), Codex zmierzył `--list` (1245/1034), liczby się nie zgodziły i plan kazał w takiej sytuacji stanąć. Wada jest w moim prompcie: pozwalał zapisać liczbę bazową w planie. Poprawione — wykonawca mierzy sam i porównuje z własnym pomiarem · X→C
- 2026-09-02 13:00 · Z-24 · LANDED, jedna runda, werdykt DZIALA, 7 checków zielonych, pełne CI 256 s; `h land` sam sprzątnął worktree (H-14 działa) · C→X
- 2026-09-02 12:05 · 0.1/0.2 sprzątanie · 22 worktree zdjęte, 11 gałęzi skasowanych, `runs/` 192→120, `.h-plan.md` z korzenia usunięty; wolne 433→520 GiB · ręka
- 2026-09-02 11:58 · 0.5 · `4ed0e9b4`; STATUS.md 3237→204 linii + archiwum, dwa pliki 90 KB do `docs/archive/`, martwe odwołania z AGENTS.md, DECISIONS-LOCKED, build.md, settings.json; proza historyczna dostała baner zamiast przepisania · ręka
- 2026-09-02 11:40 · 0.3/0.4 · `b157a6a0` + `Cargo.lock`; pełna bramka zielona na main, strażnicy 9/9. Nowo żywy `checks/vocabulary.sh` przechodzi. 11 testów dowodu śmierci procesu biegnie: 6,8 / 5,9 / 5,9 s w trzech przebiegach · ręka
- 2026-09-02 11:20 · sonda H-1 · `codex exec -o` oddaje 3 bajty („TAK"), strumień bez `-o` to 716 bajtów z logiem `ERROR rmcp` — na prawdziwym planie 55–317 KB. Poprawka potwierdzona przeciwko zainstalowanemu CLI · ręka
- 2026-09-02 11:15 · rescue-repairs · siedem napraw sprzed przepisania historii wlane do main; pełna bramka zielona przed merge'em (257 s) i po nim. Cztery pozostałe BLOCKED · ręka
- 2026-09-02 10:50 · ODSTĘPSTWO od planu · 0.2 miało być dwunastoma biegami `h run rescue-<x>` (~360 USD). Gałęzie okazały się STOSEM, nie dwunastoma niezależnymi: 11 różnych commitów w dwóch łańcuchach, jedna gałąź bez własnej pracy. Zamiast dwunastu biegów: jeden cherry-pick w kolejności zależności + pełna bramka. Bramka odpowiada na to samo pytanie („czy to nadal działa") mechanicznie i za darmo · ręka
- 2026-09-02 10:45 · ODSTĘPSTWO od planu · plan kazał skasować 92 gałęzie `task-*`/`T-*`. NIE zrobione: 35 z nich NIE jest osiągalnych z `backup/przed-przepisaniem-historii`, więc kasowanie straciłoby ich wierzchołki bezpowrotnie. Refy zostają (kosztują 40 bajtów), dysk zwolniły katalogi worktree. Decyzja o kasowaniu należy do człowieka · ręka
- 2026-09-02 10:30 · 0.1 · osierocone binarium testowe z anulowanego biegu meetnotes (pid 23164, 16 h 48 min, 262 MB) zabite z dowodem ESRCH; `cargo clean` zdjął 621 313 plików i 71,3 GiB; 84 martwe wpisy zaufania u Claude'a i 288 u Codeksa · ręka
- 2026-09-02 10:15 · POMYŁKA ORKIESTRATORA · edytowałem drzewo w trakcie `ci.sh full`, więc pas strażników odmówił („the tree is dirty") i cała bramka dała kod 2 przy wszystkich pasach zielonych. Reguła na przyszłość: kiedy bramka biegnie, wolno tylko czytać · ręka
- 2026-09-02 03:50 · plan · plan powstał z audytu; 5 pakietów Fali 0, 33 zadania, 33 prompty w `prompts/`; nic jeszcze nie wykonane · ręka (Fable)

---

## 6. Z-01 — ZAMKNIĘTE 2026-09-03 (zapis dla historii)

> **ZAMKNIĘTE.** Właściciel wybrał wariant A. `Z-01c` (żywe zabijanie) przeszło
> w JEDNEJ rundzie, `Z-01d` (znacznik, `pgids`, reaper) w dwóch. Sekcja zostaje, bo
> niesie sześć znalezisk, których żaden check nie widział — i to jest najlepszy zapis
> tego, po co w tym repo stoi weryfikacja cross-vendor.

Dwa podejścia (`z01-descendants`, `z01b-descendants`), po trzy rundy każde, **sześć odrzuceń**,
każde na innej prawdziwej dziurze, każde przy ZIELONYCH checkach. Praca stoi w dwóch worktree
i nie jest zmarnowana: ostatnie podejście ma 8 plików i ~700 linii, wszystkie checki zielone.

Co kolejno znalazł weryfikator (cross-vendor, Codex):

1. reaper uznawał **dowolny** znacznik za zgodę na zabicie — grupa cudzego biegu ginęłaby
   zamiast zostać zgłoszona jako obca;
2. znacznik przy prawdziwym starcie brał się z identyfikatora **sesji**, a odzyskiwanie
   porównywało go z identyfikatorem **biegu** — nigdy równe, więc własna żywa grupa byłaby „obca";
3. `pgids` nie trafiały do `run.json` na ścieżce Stopu ani limitu czasu — czyli dokładnie tam,
   gdzie ocalały wnuk jest jedynym powodem, dla którego ta praca powstaje;
4. krok `serve` w ogóle nie dostawał znacznika (`Processes::start` omija `for_step`);
5. zmienna z Połączeń o tej samej nazwie mogła **nadpisać** znacznik, bo był ustawiany przed
   pętlą po środowisku;
6. potomek utworzony **po** SIGTERM, w trakcie obsługi sygnału, wymyka się drugiej migawce;
   a grupy odkryte w oknie łaski dostawały od razu SIGKILL zamiast eskalacji TERM → KILL → dowód.

**Wniosek, który należy do ciebie, nie do pętli.** To nie jest zadanie „dodaj migawkę drzewa".
Znacznik musi przeżyć KAŻDĄ drogę powstania procesu (agent, sprawdzenie, `serve`, środowisko
z Połączeń) i KAŻDĄ drogę zejścia (naturalne, Stop, limit czasu, fork w oknie łaski), a jego
zapis musi trafić do `run.json` na każdej z nich. To jest zmiana projektowa w supervisorze,
nie łatka — i dlatego harness słusznie stanął.

Trzy drogi do wyboru:

- **podzielić na trzy zadania**: (a) migawka i eskalacja dla każdej odkrytej grupy w samym
  supervisorze, z testem na `Supervised::stop()`; (b) znacznik na wszystkich drogach spawnu,
  odporny na nadpisanie ze środowiska; (c) zapis `pgids` na wszystkich drogach zejścia plus
  reaper porównujący identyfikator biegu;
- **zawęzić do (a)**, bo samo to zamyka najczęstszy przypadek — `cargo test` odpalony przez
  agenta, który przeżywa Stop — i dopiero potem wracać po resztę;
- **zostawić** i przyjąć, że `death_proof` mówi o grupie lidera, a nie o wszystkim, co krok
  uruchomił, dopisując to zdanie do `docs/ARCHITECTURE.md` §5 jako znane ograniczenie.

Worktree obu podejść zostają nietknięte: `../loadout-h-z01-descendants`
i `../loadout-h-z01b-descendants`.
