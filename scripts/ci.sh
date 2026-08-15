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
