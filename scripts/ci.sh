#!/usr/bin/env bash
# scripts/ci.sh — JEDYNE źródło prawdy o tym, co znaczy „zielone".
#
#   bash scripts/ci.sh [rust|web|full]        (bez argumentu: full)
#
# `full` woła te same dwie funkcje pasów co `rust` i `web`. Nie ma trzeciej listy
# kroków, więc `full == rust ∪ web` PRZEZ KONSTRUKCJĘ — nie da się dodać kroku do
# jednego pasa i zgubić go w pełnej bramce. Powód jest konkretny: w meetnotes lista
# kroków żyła w ci.sh i drugi raz w .github/workflows/ci.yml, i te dwie listy się
# rozjechały. Workflow tego repo woła wyłącznie ten plik i nie wymienia ani jednego kroku.
#
# Kody wyjścia — ten sam kontrakt, co verify.sh i harness/gate.py:
#   0  pas przeszedł
#   1  sprawdzenie padło (uczciwa porażka)
#   2  to MY jesteśmy źle skonfigurowani: brak narzędzia, brak zależności, zły argument
#   3  przerwane albo limit czasu
# Nigdy nie mieszamy 1 z 2. „Nie umiem sprawdzić" to inna wiadomość niż „jest źle".
#
# PUSTE DRZEWO — wybór świadomy, bo repo nie ma jeszcze ani jednego .rs ani .tsx.
# Każdy krok ma przed sobą mechaniczny predykat („czy jest co sprawdzać"), a brak
# wejścia POMIJAMY Z WYPISANĄ LINIĄ, nigdy po cichu. Wybieram „pomiń i wypisz"
# zamiast „przewróć się", bo predykaty są czysto plikowe: pierwszy plik źródłowy
# włącza sprawdzenie sam, bez niczyjej decyzji i bez edycji tego skryptu.
# Czego ten wybór NIE obejmuje: kiedy testy JUŻ są, „zero uruchomionych" nie może
# udawać zielonego — dlatego licznik przejść jest wymagany (niezmiennik 19), nie sugerowany.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

# rustup instaluje się do ~/.cargo/bin, którego nieinteraktywny shell nie ma w PATH.
# Test `-f` NIE jest ozdobny: `source` nieistniejącego pliku ubija skrypt
# nieinteraktywny NATYCHMIAST, zanim dojdzie do `|| true` — z komunikatem na stderr,
# który typowe `2>/dev/null` zjada. Złapane 2026-08-15 na runnerze bez ~/.cargo/env:
# cały pas kończył się kodem 1 bez jednej linii wyjścia.
if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi

# Ctrl-C i SIGTERM to nie werdykt o kodzie. Kod 3, żeby nikt nie zapisał tego jako porażki.
trap 'printf "\n✗ interrupted\n" >&2; exit 3' INT TERM

STAGE="${1:-full}"
case "$STAGE" in
full | rust | web) ;;
*)
  printf 'usage: bash scripts/ci.sh [full|rust|web]\n' >&2
  exit 2
  ;;
esac

# ── pomocnicze ────────────────────────────────────────────────────────────────

die() { # die <kod> <wiadomość>
  printf '✗ %s\n' "$2" >&2
  exit "$1"
}

need() { # brak narzędzia to NASZA konfiguracja, nie czerwone sprawdzenie
  command -v "$1" >/dev/null 2>&1 || die 2 "required tool not found: $1"
}

skip() { printf '· %s — %s\n' "$1" "$2"; }

step() { # step <etykieta> <komenda...>
  local label="$1" rc=0 t0=$SECONDS
  shift
  printf '\n── %s ──\n' "$label"
  "$@" || rc=$?
  if [ "$rc" -ne 0 ]; then
    printf '✗ %s — rc %d (%ds)\n' "$label" "$rc" "$((SECONDS - t0))" >&2
    # 124 = timeout(1), 130/143 = INT/TERM. To przerwanie, nie werdykt o kodzie.
    case "$rc" in 124 | 130 | 143) exit 3 ;; esac
    exit 1
  fi
  printf '✓ %s (%ds)\n' "$label" "$((SECONDS - t0))"
}

# Niezmiennik 19: kod wyjścia to nie dowód. Testowany kod biegnie w tym samym
# procesie, którego rc czytamy — `os._exit(0)`/`process.exit(0)` na poziomie modułu
# zazielenia całą suitę. Wymagamy, żeby runner wypisał SWOJE podsumowanie z licznikiem.
# Regex jest ten sam, którego używa harness/gate.py (DEFAULT_EXPECT), w składni ERE.
EVIDENCE_RE='(Ran +[0-9]+ tests?|[0-9]+ (passed|tests? passed))'

with_evidence() { # with_evidence <komenda...>
  local log rc=0
  log="$(mktemp "${TMPDIR:-/tmp}/loadout-ci.XXXXXX")"
  "$@" 2>&1 | tee "$log" || rc=$?
  if [ "$rc" -eq 0 ] && ! grep -Eq "$EVIDENCE_RE" "$log"; then
    printf '\nexit 0 but no evidence of execution\n' >&2
    rc=1
  fi
  # Ostrzeżenie, nie porażka: suita z zerem przejść jest legalna, dopóki testów nie ma,
  # ale nie ma prawa przejść niezauważona, kiedy pliki testowe już leżą w repo.
  if [ "$rc" -eq 0 ] && ! grep -Eq '[1-9][0-9]* (passed|tests? passed)|Ran +[1-9][0-9]* tests?' "$log"; then
    printf '\n! no runner reported a non-zero passing count\n' >&2
  fi
  rm -f "$log"
  return "$rc"
}

has_any() { # has_any <katalog> <predykaty find...>
  local dir="$1"
  shift
  [ -d "$dir" ] || return 1
  [ -n "$(find "$dir" -type f \( "$@" \) -print -quit)" ]
}

CI_RAN_CHECKS=()

run_check_if_present() { # ciało sprawdzenia ma JEDNO miejsce — plik w checks/ (niezmiennik 23)
  local path="$1"
  if [ -x "$path" ]; then
    CI_RAN_CHECKS+=("$path")
    step "$path" bash "$path"
  elif [ -f "$path" ]; then
    die 2 "$path exists but is not executable — chmod +x it"
  else
    skip "$path" "not present yet"
  fi
}

# ── pas rustowy ───────────────────────────────────────────────────────────────

rust_lane() {
  # Bez src-tauri/src/lib.rs `cargo metadata` nie rozwiązuje manifestu w ogóle
  # („can't find `loadout_lib` lib"), więc żaden krok nie ma na czym pracować.
  # Jeden predykat na cały pas: pierwszy lib.rs włącza wszystkie kroki naraz.
  if [ ! -f src-tauri/src/lib.rs ]; then
    skip "rust lane" "no src-tauri/src/lib.rs yet — nothing to check"
    return 0
  fi
  need cargo

  # Niezmiennik 26: jeden ciężki cargo naraz na tym Macu (dwa linki naraz przypinają
  # kompresor pamięci i zamrażają maszynę). Zamek ma JEDNO ciało — w checks/ — więc tu
  # go tylko bierzemy. Brak pliku nie przewraca bramki: to zamek, nie sprawdzenie.
  if [ -f checks/_cargo-serialize.sh ]; then
    # shellcheck source=checks/_cargo-serialize.sh
    . checks/_cargo-serialize.sh
    cargo_serialize || die 3 "timed out waiting for the cargo build lock"
  fi

  # --locked tylko w CI: lokalnie `cargo update` jest normalną częścią pracy,
  # a na wspólnym runnerze cicha zmiana wersji zależności to nie jest zielone.
  local locked=()
  if [ "${LOADOUT_CI:-0}" = "1" ] && [ -f Cargo.lock ]; then locked=(--locked); fi

  step "cargo fmt --check" cargo fmt --all --check

  # Pełna forma `--all-targets` mieszka WYŁĄCZNIE tutaj. W pętli wewnętrznej jest
  # zakazana (AGENTS.md §4): przestawia profil builda i potrafi przekroczyć limit czasu.
  step "cargo clippy --all-targets" cargo clippy --all-targets "${locked[@]+"${locked[@]}"}" -- -D warnings

  step "cargo test" with_evidence cargo test "${locked[@]+"${locked[@]}"}"

  ensure_cargo_deny
  # Zakres: advisories + licenses + bans + sources, wszystko z deny.toml.
  # cargo-audit celowo NIE jest tu drugi raz: [advisories] w deny.toml czyta tę samą
  # bazę RUSTSEC, a dwa narzędzia na jedną politykę to dwa miejsca do rozjechania się.
  step "cargo deny check" cargo deny check

  # Jedyny krok, który dowodzi, że to się LINKUJE. clippy kompiluje, ale nie linkuje.
  step "cargo build" cargo build "${locked[@]+"${locked[@]}"}"

  # ODDAJ ZAMEK TUTAJ, nie przy wyjściu z procesu. cargo_serialize wiesza `trap … EXIT`
  # na TEJ powłoce, więc bez tego wiersza ci.sh trzyma muteks do samego końca — a potem
  # pas guards odpala `bash checks/quick-clippy.sh`, które czeka 300 s na zamek trzymany
  # przez własnego rodzica i wychodzi kodem 2. Samozakleszczenie z konstrukcji.
  #
  # Zmierzone 2026-08-15: dokładnie jeden strażnik pudłował („RED WITH THE VIOLATION GONE"),
  # a reszta przechodziła — bo po ~600 s czekania zamek przekraczał próg 15 minut i zostawał
  # USUNIĘTY JAKO MARTWY, mimo że jego właściciel żył. Czyli objaw maskował się sam, płacąc
  # za to złamaniem niezmiennika 26 dokładnie wtedy, gdy maszyna była najbardziej obciążona.
  # Jeśli któryś krok padł przez `die`, do tego wiersza nie dojdziemy — i dobrze:
  # trap EXIT nadal zwolni zamek przy wyjściu procesu. To jest zwolnienie WCZEŚNIEJSZE,
  # nie jedyne.
  declare -F cargo_release >/dev/null && cargo_release
}

ensure_cargo_deny() {
  if command -v cargo-deny >/dev/null 2>&1 && [ "${LOADOUT_CI:-0}" != "1" ]; then return 0; fi
  if [ "${LOADOUT_CI_TOOLS_PREINSTALLED:-0}" = "1" ]; then
    command -v cargo-deny >/dev/null 2>&1 ||
      die 2 "cargo-deny is required by LOADOUT_CI_TOOLS_PREINSTALLED=1 but is not on PATH"
    return 0
  fi
  # Ścieżka awaryjna: bieg CI, który nie preinstalował narzędzia (workflow robi to sam,
  # z cache'em rotowanym co tydzień). Wtedy --force, bo przywrócony cache potrafi trzymać
  # binarkę sprzed miesięcy, a stare cargo-deny nie parsuje nowszych wpisów RUSTSEC
  # (meetnotes 2026-07-12: CVSS 4.0 wywróciło niezwiązane PR-y). Bez --force
  # `cargo install` po prostu odmawia nadpisania istniejącego pliku.
  printf '· installing cargo-deny\n'
  if [ "${LOADOUT_CI:-0}" = "1" ]; then
    cargo install --locked --force cargo-deny
  else
    cargo install --locked cargo-deny
  fi
}

# ── pas webowy ────────────────────────────────────────────────────────────────

web_lane() {
  need node
  need npm
  # Brak node_modules to nasza konfiguracja, nie czerwony frontend.
  [ -d node_modules ] || die 2 "node_modules is missing — run 'npm ci' (or 'npm install') first"

  if has_any src -name '*.ts' -o -name '*.tsx' -o -name '*.css'; then
    # Glob żyje w package.json (`fmt:check`), żeby edytor i bramka miały ten sam.
    step "prettier --check" npm run --silent fmt:check
  else
    skip "prettier --check" "no src/**/*.{ts,tsx,css} yet"
  fi

  if has_any src -name '*.ts' -o -name '*.tsx'; then
    if [ -f checks/tsconfig.strict.json ]; then
      # tsconfig w checks/ jest poza zasięgiem biegu — bieg nie rozluźni własnej bramki.
      step "tsc --noEmit (checks/tsconfig.strict.json)" npm run --silent typecheck
    else
      step "tsc --noEmit (tsconfig.json)" npx --no-install tsc --noEmit -p tsconfig.json
    fi
  else
    skip "tsc --noEmit" "no TypeScript sources yet"
  fi

  # vitest kończy się niezerowo na „No test files found", więc bez tego predykatu pas
  # byłby czerwony na pustym drzewie z powodu, który nie mówi nic o kodzie
  # (spreadsheet ma dokładnie ten komentarz w swoim ci.yml — i tam skończyło się
  # wyrzuceniem testów z CI, czego tutaj nie chcemy).
  if has_any src -name '*.test.ts' -o -name '*.test.tsx' -o -name '*.spec.ts' -o -name '*.spec.tsx'; then
    step "vitest run" with_evidence npx --no-install vitest run
  else
    skip "vitest run" "no test files yet"
  fi

  if [ -f index.html ]; then
    step "vite build" npx --no-install vite build
  else
    skip "vite build" "no index.html yet"
  fi

  # D5 / niezmiennik 14: słownictwo widoczne dla użytkownika. Ciało sprawdzenia
  # jest w checks/, tutaj tylko wywołanie.
  run_check_if_present checks/quick-vocabulary.sh
}

# ── co bramka widzi, a CI nie ─────────────────────────────────────────────────

report_gate_only_checks() {
  # verify.sh biegnie w worktree z TASK.md; CI biegnie na trunku, gdzie TASK.md nie ma.
  # Te dwa zbiory nigdy nie będą równe i nie mają być. Wypisujemy różnicę, bo „którego
  # pliku z checks/ CI nie woła" to pytanie, które inaczej zadaje się po incydencie.
  # To notatka, nie werdykt: część z tych sprawdzeń CI pokrywa MOCNIEJSZĄ formą tej samej
  # rzeczy (clippy --all-targets zamiast --lib), a część nie ma tu sensu w ogóle
  # (quick-scope pilnuje niezacommitowanych zapisów w worktree, których na trunku nie ma).
  local f rest=()
  for f in checks/quick-*.sh checks/full-*.sh; do
    [ -e "$f" ] || continue
    case " ${CI_RAN_CHECKS[*]-} " in *" $f "*) continue ;; esac
    rest+=("$f")
  done
  [ ${#rest[@]} -gt 0 ] || return 0
  printf '\n· not invoked by CI (the gate runs these): %s\n' "${rest[*]}"
}

# ── sprawdzenie sprawdzeń ─────────────────────────────────────────────────────
#
# N-12 (audyt 2026-08-15): harness/guards.sh nie był wołany przez NIC. Występował
# w harness/README.md, w docs/, w tasks/T-22.md i w samym sobie — ale nie w verify.sh,
# nie w gate.py, nie w ship-task.sh, nie tutaj i nie w haku Stop. Sprawdzenie sprawdzeń
# było komendą, którą człowiek musiał pamiętać. 00-SYNTHESIS §4.1 przewidywał je „przy pięciu
# sprawdzeniach, obowiązkowo przy dziesięciu" — mamy jedenaście.
#
# Uruchamiamy je TUTAJ, a nie jako checks/full-guards.sh w bramce, z dwóch powodów: guards
# odpala każde sprawdzenie dwa razy (to podwoiłoby najdroższą warstwę), i odmawia na brudnym
# drzewie kodem 2 — co bramka słusznie eskaluje na MISCONFIGURED dla całej warstwy. Drzewo CI
# jest zawsze czyste, czyli dokładnie to, czego guards wymaga.
#
# Bramkowane ścieżką, nigdy oceną: „czy ten diff dotyka harnessu" to test ścieżki
# (spreadsheet/checks/fast-selftest.sh:5), a nie decyzja do podjęcia.
guards_lane() {
  local base="${LOADOUT_CI_BASE:-origin/main}"
  local touched=""
  if git rev-parse --verify -q "$base" >/dev/null 2>&1; then
    touched="$(git diff --name-only "$base"...HEAD 2>/dev/null | grep -E '^(harness/|checks/|verify\.sh$)' || true)"
  else
    touched="baza nieznana — uruchamiam"
  fi
  if [ -z "$touched" ]; then
    echo "guards: the diff does not touch harness/, checks/ or verify.sh — skipped by path"
    return 0
  fi
  prompt_backticks
  pinned_scripts_find_the_repo
  task_spine_declarations
  cargo_lock_exit_code
  cargo_lock_reclaims_dead_owner
  hung_check_reads_as_red_not_as_a_slow_gate
  spec_assertions_may_grow_never_shrink
  branch_is_judged_by_the_trunks_oracle
  spine_merges_keep_both_declarations
  contract_freeze_touches_only_its_own_task
  one_clippy_at_the_full_tier
  queueing_never_lands_in_the_waiters_budget
  echo "── guards (the check of checks) ──"
  bash harness/guards.sh
}


# ── prompty pisarza nie mogą zawierać nieescapowanych backticków ──────────────
# Zmierzone 2026-08-15: wstawka do promptu w ship-task.sh niosła `;` i `&&` w backtickach.
# Heredoc <<PROMPT jest NIECYTOWANY (celowo — podstawiamy $ID i $AGENT), więc bash wykonał
# je jako komendy, prompt dojechał do modelu zniekształcony, a `bash -n` nic nie zgłosił:
# składniowo to poprawne podstawienie komendy, tylko robi coś zupełnie innego.
prompt_backticks() {
  local bad
  bad="$(awk '/<<PROMPT/,/^PROMPT$/' ship-task.sh repair.sh review.sh 2>/dev/null \
         | grep -n '[^\\]`' || true)"
  if [ -n "$bad" ]; then
    echo "unescaped backtick inside a writer prompt heredoc -- bash will RUN it:" >&2
    printf '%s\n' "$bad" | head -10 | sed 's/^/  /' >&2
    echo "escape them as \\\` — the heredoc is unquoted on purpose." >&2
    return 1
  fi
  echo "prompts: no unescaped backticks"
}

# ── przeterminowany muteks cargo to 2, nigdy 1 ────────────────────────────────
# Zmierzone 2026-08-15: `cargo_serialize || exit 1` sprawiał, że sprawdzenie, które NIC nie
# uruchomiło, meldowało „twój kod jest zepsuty". gate.py zna na poziomie sprawdzenia tylko
# dwie kategorie — 2 to `misconfigured`, wszystko inne niezerowe to `failed` — więc jedyną
# poprawną odpowiedzią jest 2. Fałszywa czerwień uzbraja się dopiero przy zadaniach
# równoległych, czyli tam, gdzie nikt jej nie będzie szukał.
#
# Nie jest to strażnik w harness/guards.sh: tamten framework daje jedną funkcję na sprawdzenie
# i sadzi naruszenie W KODZIE. Tutaj naruszeniem jest STAN MASZYNY (zajęty muteks), więc
# asercja mieszka tam, gdzie już mieszka prompt_backticks — obok, nie w środku.
cargo_lock_exit_code() {
  local sandbox rc
  # WŁASNY katalog tymczasowy, nie współdzielony. Pierwsza wersja zajmowała prawdziwy zamek
  # i pomijała się, gdy był zajęty — a wewnątrz ci.sh był zajęty ZAWSZE, więc asercja nie
  # wykonała się ani razu i meldowała to jednym wierszem, który wyglądał jak sukces.
  # Asercja, która się pomija, nie jest asercją.
  sandbox="$(mktemp -d)" || { echo "nie mogę zrobić katalogu na zamek" >&2; return 1; }
  mkdir "$sandbox/loadout-cargo.lock"
  # Właściciel musi ŻYĆ, inaczej nowa reguła odzyskania skasuje zamek i zmierzymy
  # zupełnie inną ścieżkę niż tę, o którą pytamy. $$ to ci.sh, czyli na pewno żywy.
  echo "$$" > "$sandbox/loadout-cargo.lock/pid"

  rc=0
  TMPDIR="$sandbox" LOADOUT_CARGO_LOCK_WAIT=2 bash checks/quick-clippy.sh >/dev/null 2>&1 || rc=$?
  rm -rf "$sandbox"

  if [ "$rc" != 2 ]; then
    echo "quick-clippy wyszedł $rc na zajętym muteksie, a ma wyjść 2" >&2
    echo "detail: nic się nie wykonało, więc to nie jest twierdzenie o kodzie (Q-3)." >&2
    return 1
  fi
  echo "cargo lock: zajęty muteks daje 2, nie 1"
}

# ── zadanie tworzace modul Rusta musi miec w OWNS plik z jego deklaracja ──────
# Cialo w harness/task-spine.py (niezmiennik 23: jedna polityka, jedno miejsce).
# Ta klasa zatrzymala petle cztery razy 2026-08-15 -- za kazdym razem z innym objawem,
# wiec za kazdym razem diagnozowalem ja od zera. Bez `pub mod x;` w rodzicu modul nie
# wchodzi do skrzyni, test integracyjny sie nie kompiluje, a bramka odrzuca to jako
# falszywa czerwien (NOT_A_REAL_RED). Zadania agent nie moze wtedy wykonac ani obejsc.
task_spine_declarations() {
  python3 harness/task-spine.py || return 1
}

# ── przypięte skrypty muszą nadal znajdować korzeń repo ───────────────────────
# Regresja z 2026-08-15, wprowadzona przez samą poprawkę przeciw edycji w trakcie biegu:
# po `exec bash "$snap"` ${BASH_SOURCE[0]} wskazuje plik w $TMPDIR, więc każde
# `dirname "${BASH_SOURCE[0]}"` liczyło korzeń repo jako /var/folders/… i oba skrypty
# padały natychmiast. `bash -n` tego nie widzi — składnia jest bez zarzutu.
#
# Drugi, gorszy wariant tej samej wady: LOADOUT_PINNED jest wyeksportowany, więc
# ship-task.sh odpalony przez build-loop.sh dziedziczyłby cudzy sentinel, pomijał własne
# przypięcie i brał katalog scripts/ za korzeń. Poprawka wyłączałaby się dokładnie tam,
# gdzie pętla jej najbardziej potrzebuje. Stąd `unset` w obu skryptach i drugi wiersz niżej.
pinned_scripts_find_the_repo() {
  local out
  # ship-task.sh z nieistniejącym zadaniem ma dojść do listowania tasks/ — czyli musi
  # najpierw poprawnie ustalić korzeń. To najtańsze wywołanie, które tego dowodzi.
  out="$(bash ship-task.sh __ci_probe__ 2>&1 || true)"
  if ! printf '%s' "$out" | grep -q 'no such task: tasks/__ci_probe__.md'; then
    echo "ship-task.sh nie znajduje korzenia repo po przypięciu:" >&2
    printf '%s\n' "$out" | head -3 | sed 's/^/  /' >&2
    return 1
  fi

  out="$(bash scripts/build-loop.sh --dry-run 2>&1 || true)"
  if printf '%s' "$out" | grep -q 'run me from the repo root'; then
    echo "build-loop.sh nie znajduje korzenia repo po przypięciu" >&2
    return 1
  fi

  # Sentinel build-loopa NIE MOŻE wyłączyć przypięcia ship-taskowi. To druga wada, nie ta
  # sama: przy wspólnej nazwie dziecko dziedziczyło sentinel rodzica i brało jego katalog
  # za korzeń repo. Podrzucamy tu dokładnie taką parę zmiennych i wymagamy, żeby nic się
  # nie zmieniło — bo nazwy są rozłączne.
  out="$(env LOADOUT_PINNED_BUILD_LOOP=1 LOADOUT_SELF_BUILD_LOOP=/tmp \
             bash ship-task.sh __ci_probe__ 2>&1 || true)"
  if ! printf '%s' "$out" | grep -q 'no such task: tasks/__ci_probe__.md'; then
    echo "cudzy sentinel wyłączył przypięcie ship-task.sh:" >&2
    printf '%s\n' "$out" | head -3 | sed 's/^/  /' >&2
    return 1
  fi

  echo "pinned scripts: oba znajdują korzeń repo, cudzy sentinel ich nie rusza"
}

# ── zamek po martwym właścicielu ma być odzyskany, nie odczekany ──────────────
# Druga połowa tej samej reguły. Bez niej „daje 2" byłoby prawdą także wtedy, gdyby zamek
# nigdy nie dał się odzyskać — czyli pierwsze zabite cargo blokowałoby maszynę na 5 minut
# przy każdym kolejnym sprawdzeniu.
cargo_lock_reclaims_dead_owner() {
  local sandbox rc dead
  sandbox="$(mktemp -d)" || return 1
  mkdir "$sandbox/loadout-cargo.lock"
  # PID, który na pewno nie żyje: odpal cokolwiek i poczekaj, aż skończy.
  ( exit 0 ) & dead=$!; wait "$dead" 2>/dev/null || true
  echo "$dead" > "$sandbox/loadout-cargo.lock/pid"

  rc=0
  TMPDIR="$sandbox" LOADOUT_CARGO_LOCK_WAIT=5 bash checks/quick-clippy.sh >/dev/null 2>&1 || rc=$?
  rm -rf "$sandbox"

  if [ "$rc" != 0 ]; then
    echo "quick-clippy wyszedł $rc na zamku po MARTWYM właścicielu, a ma przejść" >&2
    echo "detail: zamek po trupie ma być odzyskany od razu, nie odczekany do sufitu." >&2
    return 1
  fi
  echo "cargo lock: zamek po martwym właścicielu odzyskany"
}

# ── zawieszone kryterium ma się czytać jako CZERWONE, nie jako wolna bramka ───
# Zmierzone na T-06 (2026-08-16). AC-2 zawisło na zakleszczeniu kanału tokio, zjadło swój
# budżet 420 s, `run_one` spytał drugi raz — to jest zamierzone, bo timeout mówi „nie
# skończyło", nie mówi dlaczego — więc razem 840 s. Bramka zapisała je uczciwie jako
# `failed` z powodem „did not FINISH", po czym zwróciła **3**, bo sufit poziomu liczył
# JEDEN budżet zamiast dwóch, które sam przyznał. Kod 3 znaczy „przerwane albo maszyna"
# i wysyła orchestratora po osierocone procesy; prawdą był defekt kontraktu, leżący
# w paragonie. Kosztowało to bieg i noc na hipotezie o zamku SQLite.
#
# Strażnik stoi tutaj, a nie w harness/guards.sh: tamten framework sadzi naruszenie w pliku
# checks/*.sh, a to naruszenie mieszka w arytmetyce oracle'a. Odtworzenie go end-to-end
# wymagałoby biegu 840 s, więc pytamy funkcję wprost — jej własnymi liczbami z paragonu.
hung_check_reads_as_red_not_as_a_slow_gate() {
  python3 - <<'PY' || return 1
import importlib.util, sys

spec = importlib.util.spec_from_file_location("gate", "harness/gate.py")
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)
per = g.CHECK_TIMEOUT["before"]

# Paragon T-06 co do liczby: szesc kryteriow czerwonych w ~1 s, AC-2 wisialo 420 + 420.
argv = {"AC-%d" % i: ["cargo", "test", "--test", "store_x%d" % i] for i in range(1, 8)}
hung = [{"id": "AC-%d" % i, "seconds": s, "retried": "", "rc": rc} for i, s, rc in
        [(1, 11.26, 101), (2, 840.39, 124), (3, 1.10, 101), (4, 1.02, 101),
         (5, 1.11, 101), (6, 1.00, 101), (7, 0.95, 101)]]
if 840.40 > g.ceiling_for("before", hung, argv, per):
    sys.exit("zawieszone kryterium przewraca poziom na 3 -- czerwien chowa sie za sufitem, "
             "ktory sama wypelnila, a orchestrator dostaje diagnoze 'maszyna' zamiast 'kontrakt'")

# Kontrola przeciw pustej asercji. Bez niej ta asercja przechodzilaby takze wtedy, gdyby
# poprawka po prostu skasowala sufit: poziom wolny BEZ ani jednego sprawdzenia dotknietego
# timeoutem musi NADAL wychodzic trojka.
slow = [{"id": "P-%d" % i, "seconds": 5.0, "retried": "", "rc": 0} for i in range(40)]
if 200.0 <= g.ceiling_for("before", slow, {}, per):
    sys.exit("sufit przestal lapac bramke wolna bez powodu -- poprawka zjadla go w calosci")
PY
  echo "gate: zawieszone kryterium czyta się jako RED, wolna bramka nadal jako 3"
}

# ── specyfikacji wolno zyskiwać asercje, nigdy żadnej zgubić ──────────────────
# Obrona rundy naprawczej kontraktu (ship-task.sh, etap 3a). Ta faza dostaje instrukcję
# „spraw, żeby kryterium padało INACZEJ", a najtańsza droga do tego jest asertować mniej —
# i jest to jedyna faza, w której „asertuj mniej" jest wiarygodnym ODCZYTEM instrukcji,
# a nie jawnym oszustwem. Dlatego dostała obronę mechaniczną, a nie zdanie w promptcie
# (niezmiennik 28). Obrona bez strażnika jest obroną, o której nikt nie wie, czy strzela.
#
# Strażnik wycina OBIE funkcje z ship-task.sh i uruchamia je naprawdę. Kopia asercji
# wklejona tutaj testowałaby kopię, nie mechanizm (niezmiennik 20).
spec_assertions_may_grow_never_shrink() {
  local sandbox fns wt src
  sandbox="$(mktemp -d)" || return 1
  fns="$sandbox/fns.sh"
  wt="$sandbox/wt"

  python3 - ship-task.sh "$fns" <<'EXTRACT' || { rm -rf "$sandbox"; return 1; }
import io, sys

lines = io.open(sys.argv[1], encoding="utf-8").read().split("\n")
out = []
for want in ("assertion_fingerprint()", "assertions_lost()"):
    head = [k for k, l in enumerate(lines) if l.startswith(want)]
    if len(head) != 1:
        sys.exit("%s wystepuje %d razy w ship-task.sh" % (want, len(head)))
    i = head[0]
    j = next(k for k in range(i + 1, len(lines)) if lines[k] == "}")
    out.extend(lines[i:j + 1])
if len(out) < 20:
    sys.exit("wyciety kod jest podejrzanie krotki -- ksztalt pliku sie zmienil")
io.open(sys.argv[2], "w", encoding="utf-8").write("\n".join(out) + "\n")
EXTRACT

  src="source '$fns'"
  mkdir -p "$wt/src-tauri/tests" "$wt/src-tauri/src"
  # Trzy linie z asercją i jedna bez — plus PRODUKCYJNY plik, którego odcisk ma nie widzieć:
  # tam asercji ubywa legalnie, bo szkielet znika razem z implementacją.
  printf 'assert_eq!(a, b);\nlet z = 1;\nassert!(x);\nassert_ne!(c, d);\n' \
    > "$wt/src-tauri/tests/spec_one.rs"
  printf 'assert!(internal);\nassert!(other);\n' > "$wt/src-tauri/src/thing.rs"

  WT="$wt" bash -c "$src; assertion_fingerprint" > "$sandbox/a.tsv"
  if [ "$(cat "$sandbox/a.tsv")" != "$(printf 'src-tauri/tests/spec_one.rs\t3')" ]; then
    echo "odcisk asercji nie mierzy tego, co ma mierzyć:" >&2
    sed 's/^/  /' "$sandbox/a.tsv" >&2
    echo "  oczekiwano dokładnie: src-tauri/tests/spec_one.rs<TAB>3" >&2
    echo "  (plik produkcyjny z dwiema asercjami NIE ma się tu pojawić)" >&2
    rm -rf "$sandbox"; return 1
  fi

  # (a) ubyło jednej asercji → strata musi być nazwana z pliku i obiema liczbami
  printf 'assert_eq!(a, b);\nlet z = 1;\nassert!(x);\n' > "$wt/src-tauri/tests/spec_one.rs"
  WT="$wt" bash -c "$src; assertion_fingerprint" > "$sandbox/b.tsv"
  if ! bash -c "$src; assertions_lost '$sandbox/a.tsv' '$sandbox/b.tsv'" | grep -q 'spec_one.rs: 3 assertion lines -> 2'; then
    echo "specyfikacja straciła asercję, a porównanie tego nie zgłosiło" >&2
    rm -rf "$sandbox"; return 1
  fi

  # (b) przybyło → cisza. Bez tego strażnik przechodziłby także wtedy, gdyby porównanie
  #     krzyczało zawsze, czyli gdyby nie mierzyło niczego.
  printf 'assert_eq!(a, b);\nassert!(x);\nassert_ne!(c, d);\nassert!(more);\nassert!(yet);\n' \
    > "$wt/src-tauri/tests/spec_one.rs"
  WT="$wt" bash -c "$src; assertion_fingerprint" > "$sandbox/c.tsv"
  if [ -n "$(bash -c "$src; assertions_lost '$sandbox/a.tsv' '$sandbox/c.tsv'")" ]; then
    echo "specyfikacja ZYSKAŁA asercje, a porównanie zgłosiło stratę" >&2
    rm -rf "$sandbox"; return 1
  fi

  # (c) skasowany plik to strata wszystkich jego asercji, a nie brak wpisu. Bez tego
  #     najprostsza droga na skróty — usuń specyfikację — byłaby niewidoczna.
  rm "$wt/src-tauri/tests/spec_one.rs"
  WT="$wt" bash -c "$src; assertion_fingerprint" > "$sandbox/d.tsv"
  if ! bash -c "$src; assertions_lost '$sandbox/a.tsv' '$sandbox/d.tsv'" | grep -q 'spec_one.rs: 3 assertion lines -> 0'; then
    echo "skasowana specyfikacja nie liczy się jako strata asercji" >&2
    rm -rf "$sandbox"; return 1
  fi

  rm -rf "$sandbox"
  echo "specs: asercji może przybyć, ubyć nie może — odcisk widzi ubytek i skasowanie"
}

# ── gałąź ma być sądzona przez ORACLE Z TRUNKA, nie przez własną starą kopię ──
# Zmierzone dwa razy. 2026-08-15: trzy z pierwszych czterech zatrzymań pętli były fałszywymi
# alarmami z nieaktualnej kopii `checks/` na gałęzi. 2026-08-16: poprawka sufitu w gate.py,
# napisana dokładnie dla biegu T-06, była dla niego **nieosiągalna** — `worktree.sh` wycina
# cały katalog roboczy, więc gałąź niesie własne `harness/`, i ta stara kopia oddała exit 3
# tam, gdzie nowa oddaje 1. Routing wznowienia zobaczył 3, odmówił, bieg skończył się dwójką.
#
# Strażnik pyta o dwie rzeczy, bo defekt miał dwie połowy: funkcja musi działać, i musi być
# wołana ZANIM cokolwiek osądzi. Druga połowa jest asercją o kolejności w pliku — i tak ma
# być: `ship-task.sh` JEST grafem biegu, więc kolejność etapów to jego zachowanie, nie jego
# formatowanie.
branch_is_judged_by_the_trunks_oracle() {
  local sandbox g wt wt2
  sandbox="$(mktemp -d)" || return 1
  g="git -c user.email=ci@loadout -c user.name=ci -C $sandbox/repo"

  # ── połowa pierwsza: czy funkcja podciąga oracle ──
  mkdir -p "$sandbox/repo/harness" "$sandbox/repo/tasks"
  $g init -q -b main "$sandbox/repo" 2>/dev/null || { rm -rf "$sandbox"; return 1; }
  echo "old oracle" > "$sandbox/repo/harness/gate.py"
  echo "contract v1" > "$sandbox/repo/tasks/T-99.md"
  $g add -A && $g commit -q -m "trunk v1"
  $g branch task-T-99
  # trunk idzie do przodu: NOWY oracle i NOWY plik zadania
  echo "new oracle" > "$sandbox/repo/harness/gate.py"
  echo "contract v2" > "$sandbox/repo/tasks/T-99.md"
  $g add -A && $g commit -q -m "trunk v2"
  wt="$sandbox/wt"
  $g worktree add -q "$wt" task-T-99

  python3 - ship-task.sh "$sandbox/fn.sh" <<'EXTRACT' || { rm -rf "$sandbox"; return 1; }
import io, sys
lines = io.open(sys.argv[1], encoding="utf-8").read().split("\n")
head = [k for k, l in enumerate(lines) if l.startswith("refresh_harness_from_trunk()")]
if len(head) != 1:
    sys.exit("refresh_harness_from_trunk() wystepuje %d razy" % len(head))
i = head[0]
j = next(k for k in range(i + 1, len(lines)) if lines[k] == "}")
io.open(sys.argv[2], "w", encoding="utf-8").write(
    "note() { printf '   %s\\n' \"$*\"; }\n" + "\n".join(lines[i:j + 1]) + "\n")
EXTRACT

  # $1 = 0: bieg dopiero się zaczyna, więc chcemy BIEŻĄCEGO kontraktu razem z oracle'em
  WT="$wt" bash -c "source '$sandbox/fn.sh'; refresh_harness_from_trunk 0 'w tescie'" >/dev/null
  if [ "$(cat "$wt/harness/gate.py")" != "new oracle" ]; then
    echo "gałąź po odświeżeniu NADAL sądzi się starą bramką" >&2
    rm -rf "$sandbox"; return 1
  fi
  if [ "$(cat "$wt/tasks/T-99.md")" != "contract v2" ]; then
    echo "odświeżenie na starcie biegu ma przynieść także bieżący kontrakt" >&2
    rm -rf "$sandbox"; return 1
  fi

  # $1 = 1: kontrakt ZAMROŻONY (N-08). Bieg nie może zmieniać warunków własnego zaliczenia,
  # więc `tasks/` ma wrócić do wersji gałęzi, a oracle ma mimo to zostać nowy.
  #
  # DRUGI worktree, nie recykling pierwszego. Pierwsza wersja tego strażnika kasowała
  # i odtwarzała tamten — `worktree remove` odmawiał, `worktree add` mówił „already exists",
  # a asercja mierzyła stan, którego nikt nie zaplanował. Trafiła dobrze przypadkiem, co jest
  # gorsze niż porażka: przechodziłaby też wtedy, gdyby zamrażanie w ogóle nie działało.
  wt2="$sandbox/wt-frozen"
  $g branch task-T-99f main~1
  $g worktree add -q "$wt2" task-T-99f
  WT="$wt2" bash -c "source '$sandbox/fn.sh'; refresh_harness_from_trunk 1 'w tescie'" >/dev/null
  if [ "$(cat "$wt2/harness/gate.py")" != "new oracle" ]; then
    echo "zamrożony kontrakt zablokował odświeżenie ORACLE'a, a miał zamrozić tylko tasks/" >&2
    rm -rf "$sandbox"; return 1
  fi
  if [ "$(cat "$wt2/tasks/T-99.md")" != "contract v1" ]; then
    echo "kontrakt NIE został zamrożony: bieg zmienił warunki własnego zaliczenia (N-08)" >&2
    rm -rf "$sandbox"; return 1
  fi
  rm -rf "$sandbox"

  # ── połowa druga: czy odświeżenie wyprzedza pierwszy osąd ──
  python3 - ship-task.sh <<'ORDER' || return 1
import io, sys

lines = io.open(sys.argv[1], encoding="utf-8").read().split("\n")

def first(pred, what):
    for k, l in enumerate(lines):
        if pred(l):
            return k
    sys.exit("nie znalazlem %s w ship-task.sh" % what)

refresh = first(lambda l: l.strip().startswith("refresh_harness_from_trunk 0"),
                "wywolania refresh_harness_from_trunk 0")
judges = first(lambda l: ("verify.sh before" in l or l.strip().startswith("gate before")
                          or "gate before ||" in l) and not l.lstrip().startswith("#"),
               "pierwszego etapu, ktory sadzi")
if judges < refresh:
    sys.exit("pierwszy osad (linia %d) wyprzedza odswiezenie oracle'a (linia %d): galaz "
             "bylaby sadzona przez wlasna, stara kopie bramki -- dokladnie to zatrzymalo "
             "T-06 2026-08-16" % (judges + 1, refresh + 1))
ORDER

  echo "oracle: gałąź sądzi się bramką z trunka, i to zanim cokolwiek osądzi"
}

# ── konflikt na kręgosłupie rozwiązuje się ZACHOWANIEM OBU DEKLARACJI ─────────
# `src-tauri/src/lib.rs` zbiera po jednym `pub mod` od każdego zadania tworzącego moduł, więc
# przy dwóch gałęziach naraz konflikt jest PEWNY — zdarzył się przy T-11 i T-12, a potem
# zablokował odświeżanie harnessu na T-06, przez co gałąź sądziła się własną, starą bramką.
# `.gitattributes` zapisuje jedyne poprawne rozwiązanie jako regułę (`merge=union`), zamiast
# kazać je powtarzać ręcznie i ryzykować, że ktoś kiedyś „wybierze stronę".
#
# Kontrola przeciw pustej asercji jest tu obowiązkowa: bez niej ten strażnik przechodziłby
# także wtedy, gdyby git scalał te wiersze sam z siebie, i nie mierzyłby reguły.
spine_merges_keep_both_declarations() {
  local sandbox g rule
  sandbox="$(mktemp -d)" || return 1
  g="git -c user.email=ci@loadout -c user.name=ci -C $sandbox/repo"
  rule="$(grep -v '^[[:space:]]*#' .gitattributes | grep 'src-tauri/src/lib.rs' || true)"
  if [ -z "$rule" ]; then
    echo ".gitattributes nie ma już reguły dla src-tauri/src/lib.rs" >&2
    rm -rf "$sandbox"; return 1
  fi

  mkdir -p "$sandbox/repo/src-tauri/src"
  $g init -q -b main "$sandbox/repo" 2>/dev/null || { rm -rf "$sandbox"; return 1; }
  printf 'pub mod engine;\n' > "$sandbox/repo/src-tauri/src/lib.rs"
  printf '%s\n' "$rule" > "$sandbox/repo/.gitattributes"
  $g add -A && $g commit -q -m "spine v0"
  $g branch task-alpha
  # trunk dopisuje swoją deklarację…
  printf 'pub mod engine;\npub mod beta;\n' > "$sandbox/repo/src-tauri/src/lib.rs"
  $g add -A && $g commit -q -m "trunk adds beta"
  # …a gałąź swoją, w tym samym miejscu
  $g worktree add -q "$sandbox/wt" task-alpha
  printf 'pub mod engine;\npub mod alpha;\n' > "$sandbox/wt/src-tauri/src/lib.rs"
  $g -C "$sandbox/wt" add -A
  $g -C "$sandbox/wt" commit -q -m "branch adds alpha"

  if ! $g -C "$sandbox/wt" merge --no-edit -q main >/dev/null 2>&1; then
    echo "merge kręgosłupa nadal konfliktuje mimo reguły union w .gitattributes" >&2
    rm -rf "$sandbox"; return 1
  fi
  if ! grep -q 'pub mod alpha;' "$sandbox/wt/src-tauri/src/lib.rs" \
     || ! grep -q 'pub mod beta;' "$sandbox/wt/src-tauri/src/lib.rs"; then
    echo "merge kręgosłupa zgubił deklarację — wolno tylko ZACHOWAĆ OBIE:" >&2
    sed 's/^/  /' "$sandbox/wt/src-tauri/src/lib.rs" >&2
    rm -rf "$sandbox"; return 1
  fi

  # Kontrola przeciw pustej asercji: bez reguły ten sam merge MUSI konfliktować.
  $g -C "$sandbox/wt" reset -q --hard HEAD~1
  rm "$sandbox/wt/.gitattributes"
  $g -C "$sandbox/wt" add -A
  $g -C "$sandbox/wt" commit -q -m "drop the rule"
  if $g -C "$sandbox/wt" merge --no-edit -q main >/dev/null 2>&1; then
    echo "bez reguły union ten merge też przeszedł — strażnik nie mierzy reguły, tylko szczęście" >&2
    rm -rf "$sandbox"; return 1
  fi
  $g -C "$sandbox/wt" merge --abort >/dev/null 2>&1 || true

  rm -rf "$sandbox"
  echo "spine: konflikt w lib.rs rozwiązuje się zachowaniem obu deklaracji"
}

# ── zamrożenie kontraktu nie ma prawa cofać CUDZYCH plików zadań ──────────────
# Zmierzone na T-20 (2026-08-16). `refresh_harness_from_trunk` zamrażał całe `tasks/`, więc
# podciągnięcie trunka przed rundą naprawczą **rewertowało** na gałęzi pliki zadań zmienione
# w międzyczasie. Dwa skutki: `quick-scope` świecił na plikach spoza OWNS (czerwień nie do
# odróżnienia od winy pisarza — poprzedni bieg zapisał ją nawet jako „commit człowieka"),
# a lądowanie wniosłoby revert na trunk i **po cichu** skasowało cudzą pracę. Bramka po takim
# lądowaniu jest zielona, bo cofnięte kryterium nie psuje testów — ono je osłabia.
#
# N-08 chroni przed jedną rzeczą: biegiem, który zmienia warunki WŁASNEGO zaliczenia. To jest
# pytanie o `tasks/$ID.md` i o nic więcej; potwierdza to `gate.py`, który porównuje TASK.md
# wyłącznie z plikiem o tym samym identyfikatorze.
contract_freeze_touches_only_its_own_task() {
  local sandbox g wt
  sandbox="$(mktemp -d)" || return 1
  g="git -c user.email=ci@loadout -c user.name=ci -C $sandbox/repo"

  mkdir -p "$sandbox/repo/tasks" "$sandbox/repo/harness"
  $g init -q -b main "$sandbox/repo" 2>/dev/null || { rm -rf "$sandbox"; return 1; }
  echo "kontrakt T-99, wersja zamrozona" > "$sandbox/repo/tasks/T-99.md"
  echo "kontrakt T-88, wersja stara"     > "$sandbox/repo/tasks/T-88.md"
  echo "oracle stary"                    > "$sandbox/repo/harness/gate.py"
  $g add -A && $g commit -q -m "v1"
  $g branch task-T-99
  # Trunk idzie do przodu we WSZYSTKICH trzech plikach naraz.
  echo "kontrakt T-99, wersja ULEPSZONA" > "$sandbox/repo/tasks/T-99.md"
  echo "kontrakt T-88, wersja NOWA"      > "$sandbox/repo/tasks/T-88.md"
  echo "oracle nowy"                     > "$sandbox/repo/harness/gate.py"
  $g add -A && $g commit -q -m "v2"
  wt="$sandbox/wt"
  $g worktree add -q "$wt" task-T-99

  python3 - ship-task.sh "$sandbox/fn.sh" <<'EXTRACT' || { rm -rf "$sandbox"; return 1; }
import io, sys
lines = io.open(sys.argv[1], encoding="utf-8").read().split("\n")
head = [k for k, l in enumerate(lines) if l.startswith("refresh_harness_from_trunk()")]
if len(head) != 1:
    sys.exit("refresh_harness_from_trunk() wystepuje %d razy" % len(head))
i = head[0]
j = next(k for k in range(i + 1, len(lines)) if lines[k] == "}")
io.open(sys.argv[2], "w", encoding="utf-8").write(
    "note() { :; }\n" + "\n".join(lines[i:j + 1]) + "\n")
EXTRACT

  WT="$wt" ID=T-99 bash -c "source '$sandbox/fn.sh'; refresh_harness_from_trunk 1 'w tescie'" >/dev/null

  # (1) oracle MA sie odswiezyc -- po to cale odswiezenie istnieje
  if [ "$(cat "$wt/harness/gate.py")" != "oracle nowy" ]; then
    echo "zamrożenie kontraktu zablokowało odświeżenie oracle'a" >&2
    rm -rf "$sandbox"; return 1
  fi
  # (2) WŁASNY kontrakt ma zostać zamrożony -- bieg nie zmienia warunków swojego zaliczenia
  if [ "$(cat "$wt/tasks/T-99.md")" != "kontrakt T-99, wersja zamrozona" ]; then
    echo "własny kontrakt biegu NIE został zamrożony (N-08)" >&2
    rm -rf "$sandbox"; return 1
  fi
  # (3) CUDZY plik zadania ma iść za trunkiem. To jest ta asercja, której brakowało.
  if [ "$(cat "$wt/tasks/T-88.md")" != "kontrakt T-88, wersja NOWA" ]; then
    echo "zamrożenie COFNĘŁO cudzy plik zadania do wersji sprzed trunka:" >&2
    echo "  tasks/T-88.md = $(cat "$wt/tasks/T-88.md")" >&2
    echo "  oczekiwano wersji z trunka. Wylądowanie takiej gałęzi kasuje cudzą pracę po cichu," >&2
    echo "  a bramka po tym lądowaniu jest ZIELONA, bo cofnięte kryterium nie psuje testów." >&2
    rm -rf "$sandbox"; return 1
  fi

  rm -rf "$sandbox"
  echo "kontrakt: zamrożony jest własny plik zadania, cudze idą za trunkiem"
}

# ── w `full` biegnie JEDNO clippy, nie dwa bijące się o muteks ────────────────
# Zmierzone 2026-08-16 przy lądowaniu T-27, na PUSTEJ maszynie: `verify.sh full` odkrywał
# `quick-clippy` i `full-clippy`, oba brały muteks cargo (niezmiennik 26), drugie czekało 300 s
# i oddawało 2 — więc trunk świecił „MISCONFIGURED" przez własną kolejkę bramki, a nie przez kod.
# Podnoszenie sufitu czekania nie pomaga: przy 2400 s to `full-test` ginął na swoim budżecie.
# Jedyna naprawa bez wady jest taka, że zbędne sprawdzenie się nie odpala.
#
# Strażnik pilnuje OBU stron: że w `full` schodzi z drogi, i że poza `full` biegnie normalnie.
# Sama pierwsza połowa przechodziłaby na checku, który nie robi już nic nigdzie.
one_clippy_at_the_full_tier() {
  local out sandbox
  out="$(LOADOUT_TIER=full bash checks/quick-clippy.sh 2>&1)"
  if ! printf '%s' "$out" | grep -q 'superseded'; then
    echo "quick-clippy nie schodzi z drogi w tierze full — dwa clippy będą się biły o muteks" >&2
    printf '%s\n' "$out" | head -3 | sed 's/^/  /' >&2
    return 1
  fi

  # Druga połowa: poza `full` ma biec normalnie. Piaskownica BEZ zrodel Rusta, zeby nie
  # odpalac prawdziwego clippy — check ma wtedy wlasna, nazwana galaz „nie ma czego lintowac".
  sandbox="$(mktemp -d)" || return 1
  mkdir -p "$sandbox/checks" "$sandbox/src-tauri/src"
  cp checks/quick-clippy.sh checks/_cargo-serialize.sh "$sandbox/checks/"
  out="$(cd "$sandbox" && LOADOUT_TIER=before bash checks/quick-clippy.sh 2>&1)"
  rm -rf "$sandbox"
  if printf '%s' "$out" | grep -q 'superseded'; then
    echo "quick-clippy schodzi z drogi także POZA tierem full — czyli nie sprawdza już nic" >&2
    return 1
  fi
  if ! printf '%s' "$out" | grep -q 'nothing to lint'; then
    echo "quick-clippy poza tierem full nie doszedł do swojej właściwej gałęzi:" >&2
    printf '%s\n' "$out" | head -3 | sed 's/^/  /' >&2
    return 1
  fi

  echo "clippy: jedno w tierze full, normalne poza nim"
}

# ── czekanie na muteks nie ma prawa ladowac w budzecie czekajacego ────────────
# Zmierzone 2026-08-17 na T-36: `full-test` trzymal muteks cargo 512 s, a `full-clippy`,
# puszczony przez bramke w tej samej fali, PRZESPAL na nim 242,88 s na WLASNYM zegarze,
# oddal 2 i cala bramka zaswiecila sie "MISCONFIGURED". Kod byl poprawny, oba kryteria
# przeszly -- bramka nie osadzila go przez wlasna kolejke.
#
# Sufitem czekania tego nie da sie naprawic: zeby full-clippy doczekal, cap musi byc >=512 s,
# a zeby zmiescil sie we wlasnym budzecie (600 s) razem z zimnym buildem -- <=360 s. Warunki
# sprzeczne, wiec zmienna do ruszenia jest ROWNOLEGLOSC, a nie cap.
#
# Ten straznik pyta o BUDZET, nie o nakladanie sie. Pierwsza wersja pytala o nakladanie
# i przechodzila TAKZE przed poprawka -- bo nakladania i tak zawsze bronil sam muteks.
queueing_never_lands_in_the_waiters_budget() {
  local t work=3
  t="$(mktemp -d)"
  mkdir -p "$t/harness" "$t/checks"
  # Podmiana bramki istnieje po to, zeby dalo sie POKAZAC, ze ten straznik ma zeby (sadzimy
  # nim bramke sprzed poprawki i musi zaswiecic). Mowi o tym glosno: straznik sadzacy po cichu
  # nie ten plik, o ktory go pytano, czyta sie identycznie jak straznik, ktory przeszedl.
  if [ -n "${LOADOUT_GUARD_GATE:-}" ]; then
    echo "UWAGA: straznik sadzi $LOADOUT_GUARD_GATE, a nie harness/gate.py" >&2
  fi
  cp "${LOADOUT_GUARD_GATE:-harness/gate.py}" "$t/harness/gate.py"
  cp checks/_cargo-serialize.sh "$t/checks/_cargo-serialize.sh"

  local n
  for n in heavya heavyb lighta lightb; do
    {
      echo '#!/usr/bin/env bash'
      echo 'set -euo pipefail'
      echo 'cd "$(dirname "${BASH_SOURCE[0]}")/.."'
      case "$n" in heavy*)
        echo '. checks/_cargo-serialize.sh'
        echo 'cargo_serialize || exit 2' ;;
      esac
      echo "printf '%s IN %s\n' \"\$(date +%s.%N)\" 'quick-$n' >> \"\$LANE_LOG\""
      echo "sleep $work"
      echo "printf '%s OUT %s\n' \"\$(date +%s.%N)\" 'quick-$n' >> \"\$LANE_LOG\""
      echo "echo '$n done'"
    } > "$t/checks/quick-$n.sh"
  done
  printf 'quick-heavya\nquick-heavyb\nquick-lighta\nquick-lightb\n' > "$t/checks/MANIFEST"

  # TMPDIR wlasny: zamek straznika nie ma prawa dotknac zamka prawdziwego biegu obok.
  LANE_LOG="$t/lane.log" ; : > "$LANE_LOG" ; export LANE_LOG
  ( cd "$t" && TMPDIR="$t" python3 harness/gate.py quick --jobs 4 ) >"$t/out.txt" 2>&1 || true

  python3 - "$t/out.txt" "$t/lane.log" "$work" <<'PY'
import sys, re, collections
measured, span = {}, collections.defaultdict(dict)
for line in open(sys.argv[1]):
    m = re.match(r"\s+(ok|FAIL|MISC)\s+(\S+)\s+([0-9.]+)s", line)
    if m:
        measured[m.group(2)] = float(m.group(3))
for line in open(sys.argv[2]):
    t, ev, name = line.split()
    span[name]["in" if ev == "IN" else "out"] = float(t)
work = float(sys.argv[3])

need = ["quick-heavya", "quick-heavyb", "quick-lighta", "quick-lightb"]
missing = [n for n in need if n not in measured or len(span[n]) < 2]
if missing:
    sys.exit("straznik nic nie zmierzyl dla: %s -- bramka nie odkryla sprawdzen sondy"
             % ", ".join(missing))

for n in ("quick-heavya", "quick-heavyb"):
    queued = measured[n] - work
    if queued > 1.2:
        sys.exit("%s przesiedzialo %.2fs cudzego czasu na WLASNYM zegarze -- dokladnie tak "
                 "full-clippy oddal 2 przy T-36, majac poprawny kod" % (n, queued))

# Kontrola negatywna. Bez niej asercja wyzej przechodzi takze wtedy, gdy poprawka zabila
# rownoleglosc CALEJ bramki: nikt nigdy nie czeka, bo nikt nigdy nie biegnie obok.
A, B = span["quick-lighta"], span["quick-lightb"]
if min(A["out"], B["out"]) - max(A["in"], B["in"]) <= 0:
    sys.exit("lekkie sprawdzenia tez sie nie nakladaja -- bramka jest szeregowa CALA, wiec "
             "asercja wyzej nie dowodzi niczego o lane szeregowym")
PY
  local rc=$?
  if [ "$rc" != 0 ]; then
    # Straznik, ktory mowi tylko "czerwone", jest tym monitoringiem, ktory to repo skasowalo.
    echo "kolejka po muteks cargo lezy w budzecie czekajacego sprawdzenia" >&2
    echo "--- co zobaczyla bramka sondy ---" >&2
    sed -n '1,20p' "$t/out.txt" >&2 || true
    rm -rf "$t"
    return 1
  fi
  rm -rf "$t"
  echo "gate: czekanie na muteks poza budzetem, rownoleglosc reszty zyje"
}

# ── dyspozytor ────────────────────────────────────────────────────────────────

# Pasy są sekwencyjne LOKALNIE świadomie (niezmiennik 26: dwa ciężkie cargo/rustc naraz
# na tym Macu przypinają kompresor pamięci i zamrażają maszynę). Równoległość jest
# w CI — dwie osobne maszyny — i to jest jedyny powód, dla którego te podkomendy istnieją.
case "$STAGE" in
rust) rust_lane ;;
web) web_lane ;;
full)
  rust_lane
  web_lane
  report_gate_only_checks
  guards_lane
  ;;
esac

printf '\n✅ CI green (stage: %s, %ds)\n' "$STAGE" "$SECONDS"
