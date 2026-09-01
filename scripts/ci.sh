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
# Kody wyjścia — ten sam kontrakt, co `.loadout/h/h.py`:
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
# Regex jest ten sam, którego używa `.loadout/h/h.py` (PASS_COUNT), w składni ERE.
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

  # `--test-threads=1`. ZMIERZONE 2026-08-28: rownolegle 31 s, ale TRZY testy padaja na
  # `Elapsed(())` -- `driver_codex_finish`, `driver_codex_resume`
  # i `started_processes_die_with_the_window` spawnuja prawdziwe procesy i mierza czasy, wiec
  # pod obciazeniem maszyny wychodza na timeout. W izolacji przechodza wszystkie trzy.
  # Jednowatkowo: 88 s, 818 passed, 0 failed. Placimy 57 s za to, zeby zielone mowilo o kodzie,
  # a nie o tym, czy maszyna byla akurat wolna -- bramka mierzaca obciazenie nie jest bramka.
  step "cargo test" with_evidence cargo test "${locked[@]+"${locked[@]}"}" -- --test-threads=1

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
  # `cargo_release` odeszlo razem z muteksem z checks/_cargo-serialize.sh (2026-08-28).
  # Ta linia stala tu jako OSTATNIA instrukcja funkcji, wiec pod `set -e` nieistniejaca
  # funkcja dawala `declare -F` -> 1 -> rust_lane zwraca 1 -> caly skrypt wychodzi jedynka
  # BEZ ANI JEDNEJ LINII WYJSCIA. Log konczyl sie na "cargo build (5s)" i wygladalo to jak
  # zniknieciecie skryptu. Zmierzone przy pierwszym CI po przebudowie.
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

  # ── sufit gęstości: pomiar, potem sędzia (niezmiennik 18) ───────────────────────────────
  #
  # TUTAJ, A NIE W BRAMCE ZADANIA, i to jest cała decyzja o miejscu. Kolektor potrzebuje
  # zbudowanego `dist/` i Chromium: w pętli zadania kosztowałby build na każdy bieg, a na
  # maszynie bez pobranych przeglądarek dawałby czerwień, którą `NOT_A_REAL_RED` i tak odrzuca.
  # `vite build` stoi wiersz wyżej, więc tutaj kosztuje jedno uruchomienie przeglądarki
  # na lądowanie — i dopiero to czyni niezmiennik 18 egzekwowanym zamiast zadeklarowanego.
  #
  # Brak przeglądarki jest POMINIĘCIEM Z POWODEM, nigdy zielenią: pomiar, którego nikt nie
  # wziął, nie jest pomiarem zera, a sędzia i tak odmówiłby kodem 2 na braku zrzutu.
  if [ -f index.html ] && [ -f scripts/density-collect.mjs ]; then
    if node scripts/density-collect.mjs --out dist/density-snapshot.json >/dev/null 2>&1; then
      LOADOUT_DENSITY_SNAPSHOT=dist/density-snapshot.json \
        step "density" bash checks/density.sh
    else
      skip "density" "the in-browser collector did not run here (no Chromium?)"
    fi
  else
    skip "density" "no built app to measure"
  fi

  # D5 / niezmiennik 14: słownictwo widoczne dla użytkownika. Ciało sprawdzenia
  # jest w checks/, tutaj tylko wywołanie.
  run_check_if_present checks/quick-vocabulary.sh
}

# ── sprawdzenie sprawdzeń ─────────────────────────────────────────────────────
#
# Do 2026-08-28 pas guards miał trzynaście funkcji pilnujących `gate.py`, `ship-task.sh`,
# muteksu cargo i poziomów bramki. Wszystkie te rzeczy zniknęły, a strażnik pilnujący
# nieistniejącego kodu jest gorszy niż jego brak: przechodzi zawsze i czyta się jak dowód.
#
# Dwie klasy odeszły nie przez skasowanie, tylko przez KONSTRUKCJĘ, i to jest lepszy wynik:
# `prompt_backticks` i `prompt_dollars` pilnowały metaznaków w heredocach promptów. Prompty
# są teraz plikami `.md` w `.loadout/h/prompts/`, więc bash nigdy ich nie interpoluje — nie ma
# czego pilnować. Każda z tych klas kosztowała kiedyś bieg.
guards_lane() {
  echo
  echo "── guards (the check of checks) ──"
  checks_are_declared
  spine_merges_keep_both_declarations
  bash .loadout/h/guards.sh
}

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

# 2026-08-28: `contract_freeze_touches_only_its_own_task` odszedl razem z katalogiem tasks/.
# Pilnowal, zeby zamrozenie kontraktu (N-08) cofalo na galezi WYLACZNIE jej wlasny plik
# zadania, a nie cale `tasks/` -- bo szersza wersja rewertowala cudza prace, a ladowanie
# wnosilo ten revert na trunk PO CICHU, przy zielonej bramce. Ta klasa wady nie ma juz
# wejscia: kontrakt nie mieszka na trunku, wiec nie ma czego cofac. Ze odswiezenie oracle'a
# faktycznie nie rusza kontraktu galezi, sprawdza teraz asercja w
# `branch_is_judged_by_the_trunks_oracle` wyzej.

# ── w `full` biegnie JEDNO clippy, nie dwa bijące się o muteks ────────────────
# Zmierzone 2026-08-16 przy lądowaniu T-27, na PUSTEJ maszynie: `verify.sh full` odkrywał
# `quick-clippy` i `full-clippy`, oba brały muteks cargo (niezmiennik 26), drugie czekało 300 s
# i oddawało 2 — więc trunk świecił „MISCONFIGURED" przez własną kolejkę bramki, a nie przez kod.
# Podnoszenie sufitu czekania nie pomaga: przy 2400 s to `full-test` ginął na swoim budżecie.
# Jedyna naprawa bez wady jest taka, że zbędne sprawdzenie się nie odpala.
#
# Strażnik pilnuje OBU stron: że w `full` schodzi z drogi, i że poza `full` biegnie normalnie.
# ── każdy check z checks/ musi być ZADEKLAROWANY w checks.json ────────────────
# To rola, którą do 2026-08-28 pełnił `checks/MANIFEST`, i jest to blizna (N-13,
# audyt 2026-08-15): bramka odkrywała checki po nazwie pliku, więc plik, który zgubił
# prefiks warstwy, dostawał notkę — ale plik SKASOWANY nie produkował nic. Zmierzone:
# usunięcie `checks/quick-permissions.sh` dało „7 checks, 0 failed" i exit 0. Zniknęło
# sprawdzenie napisane po incydencie za 6,98 USD, a bramka tego nie zauważyła, bo iterowała
# po plikach, które istnieją.
#
# Teraz checki wybiera `.loadout/h/checks.json` po zmienionych ścieżkach, więc pytanie jest
# dwustronne: czy każdy plik w `checks/` jest zadeklarowany, i czy każda deklaracja wskazuje
# na plik, który istnieje. Cichy rozjazd w obie strony to ten sam brak sprawdzenia.
checks_are_declared() {
  python3 - <<'PY' || return 1
import json, os, re, sys

cfg = json.load(open(".loadout/h/checks.json", encoding="utf-8"))
declared, missing = set(), []
for group in ("checks", "manual_only"):
    for cid, spec in cfg[group].items():
        if cid.startswith("_"):
            continue
        for m in re.finditer(r"checks/([\w.-]+\.sh)", spec["cmd"]):
            declared.add(m.group(1))
            if not os.path.isfile(os.path.join("checks", m.group(1))):
                missing.append("%s deklaruje checks/%s, ktorego nie ma" % (cid, m.group(1)))

on_disk = {f for f in os.listdir("checks") if f.endswith(".sh") and not f.startswith("_")}
orphans = sorted(on_disk - declared)
if orphans:
    missing += ["checks/%s istnieje, ale zaden check w checks.json go nie wola" % f for f in orphans]
if missing:
    sys.stderr.write("checks/ i checks.json sie rozjechaly:\n")
    for m in missing:
        sys.stderr.write("  %s\n" % m)
    sys.stderr.write("Skasowany check nie produkuje nic i czyta sie jak zdany (N-13).\n")
    raise SystemExit(1)
print("checks: %d plikow, kazdy zadeklarowany w checks.json" % len(on_disk))
PY
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
  guards_lane
  ;;
esac

printf '\n✅ CI green (stage: %s, %ds)\n' "$STAGE" "$SECONDS"
