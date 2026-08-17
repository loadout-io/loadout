#!/usr/bin/env bash
# ~0,3 s. Funkcja, której nikt nie woła, i której żadne zadanie nie obiecuje zawołać.
#
# INCYDENT, po którym to powstało. Przegląd zewnętrzny 2026-08-16 znalazł siedem szwów tej
# samej rodziny. Wzorcowy: `engine::limits::Limiter` wylądował w T-21, ma własny test, jest
# poprawny — i **nie ma ani jednego produkcyjnego wołającego**. `run_workflow_inner` podaje
# `how_many_at_once` prosto do semafora per bieg, więc dwie karty dają `2 × limit`.
#
# DLACZEGO NIC TEGO NIE ŁAPAŁO, i to jest sedno. Element `pub` używany WYŁĄCZNIE z `tests/`
# **nie jest martwym kodem dla clippy**: testy integracyjne to osobne skrzynie, które linkują
# bibliotekę, więc `dead_code` widzi użycie i milczy. Mechanizm z testem i bez wołającego
# przechodzi więc przez każdą bramkę, jaką mamy — także przez `before`, bo `before` dowodzi
# tylko, że kryterium wykrywa „nic nie istnieje", a nie że wykrywa „istnieje i wisi luzem".
#
# ZAKRES JEST WĄSKI Z PREMEDYTACJĄ: wyłącznie `pub fn` na poziomie modułu.
# Zmierzone 2026-08-17 na 25 commitach trunka. Wariant obejmujący także typy dawał 5 fałszywych
# trafień na 9 (`ReviewWire`, `RunSink`, `GoOn` — typy poprawnie zamknięte we własnym module,
# odwoływane w swoim pliku), bo grep nie odpowiada na pytanie o OSIĄGALNOŚĆ, tylko o wystąpienia.
# Wariant „tylko funkcje" dał 16 trafień i **zero fałszywych**: 13 to skorupy `#[tauri::command]`
# (są w `commands.golden.txt`), a trzy pozostałe to `run_workflow_inner`, `stop_run_inner`,
# `continue_run_inner` — czyli dokładnie ten szew, który zamyka T-30. Check ma być precyzyjny;
# sprawdzenie, które hałasuje, jest obchodzone, a nie naprawiane.
#
# REGUŁA. Dla każdej `pub fn`, którą ta gałąź DOPISAŁA na poziomie modułu:
#   podłączona  = jest wywołanie `nazwa(` gdziekolwiek w `src-tauri/src/` poza jej deklaracją,
#                 albo nazwa stoi w `src-tauri/commands.golden.txt` (sama jest wejściem — woła
#                 ją Tauri, nie nasz kod),
#   zaplanowana = wymienia ją z nazwy któreś `tasks/*.md`.
# Ani jedno, ani drugie = zgnilizna, i to jest czerwone. Wywołanie z `tests/` NIE liczy się
# nigdy: to jest dokładnie ten podpis, który udawał użycie.
#
# „Zaplanowana" nie jest furtką, tylko przeniesieniem długu tam, gdzie ktoś go widzi: żeby
# przejść, trzeba NAPISAĆ ZADANIE, które tę funkcję zawoła. Plan świadomie rozdziela mechanizm
# od podpięcia (T-21 przed T-31) i ten check ma temu nie przeszkadzać — ma tylko zabronić
# rozdzielenia, o którym nikt nie wie.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

[ -d src-tauri/src ] || { echo "wired: no src-tauri/src yet, nothing to wire"; exit 0; }
command -v git >/dev/null 2>&1 || { echo "git is not on PATH" >&2; exit 2; }
git rev-parse --git-dir >/dev/null 2>&1 || { echo "not a git tree" >&2; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "python3 is not on PATH" >&2; exit 2; }

# Ta sama baza, co w checks/quick-scope.sh: `merge-base` odpowiada na pytanie „co ta gałąź
# dopisała" i odpowiada tak samo przed merge'em i po nim.
TRUNK="${LOADOUT_TRUNK:-main}"
TRUNK_REF=""
for cand in "$TRUNK" "refs/heads/$TRUNK" "origin/$TRUNK"; do
  git rev-parse --verify -q "$cand^{commit}" >/dev/null 2>&1 && { TRUNK_REF="$cand"; break; }
done
base=""
[ -n "$TRUNK_REF" ] && base="$(git merge-base HEAD "$TRUNK_REF" 2>/dev/null || true)"
if [ -z "$base" ] && [ -f TASK.md ]; then
  base="$(git log --diff-filter=A --format=%H -- TASK.md 2>/dev/null | head -1 || true)"
fi
if [ -z "$base" ]; then
  echo "wired: no branch point to compare against — what this branch added is not knowable"
  exit 0
fi

exec python3 - "$base" <<'PY'
import os, re, subprocess, sys

base = sys.argv[1]
SRC = "src-tauri/src"

def sh(*a):
    return subprocess.run(a, capture_output=True, text=True).stdout

# Zatwierdzone i niezatwierdzone razem: pisarz ma to zobaczyć w swojej pętli, a nie dopiero
# przy lądowaniu. Nowe, jeszcze nieśledzone pliki czytamy w całości — dla nich każda linia
# jest linią dodaną.
diff = sh("git", "diff", "-U0", "--no-renames", "%s..HEAD" % base, "--", SRC)
diff += sh("git", "diff", "-U0", "--no-renames", "HEAD", "--", SRC)
for f in sh("git", "ls-files", "--others", "--exclude-standard", "--", SRC).split():
    if f.endswith(".rs") and os.path.isfile(f):
        with open(f, encoding="utf-8", errors="replace") as fh:
            diff += "".join("+" + l for l in fh)

DECL = re.compile(r"^\+pub\s+(?:async\s+)?fn\s+([A-Za-z_]\w*)")
symbols = sorted({m.group(1) for m in (DECL.match(l) for l in diff.splitlines()) if m})
if not symbols:
    print("wired: this branch added no module-level pub fn to %s" % SRC)
    raise SystemExit(0)

files = []
for root, _, names in os.walk(SRC):
    files += [os.path.join(root, n) for n in names if n.endswith(".rs")]
body = {f: open(f, encoding="utf-8", errors="replace").read() for f in files}

golden = set()
gp = "src-tauri/commands.golden.txt"
if os.path.isfile(gp):
    golden = {l.strip() for l in open(gp, encoding="utf-8") if l.strip()
              and not l.lstrip().startswith("#")}

tasks = ""
for root, _, names in os.walk("tasks"):
    for n in names:
        if n.endswith(".md"):
            tasks += open(os.path.join(root, n), encoding="utf-8", errors="replace").read()

orphans = []
for s in symbols:
    if s in golden:
        continue
    decl = re.compile(r"^pub\s+(?:async\s+)?fn\s+%s\b" % re.escape(s), re.M)
    definer = next((f for f in files if decl.search(body[f])), None)
    call = re.compile(r"\b%s\s*\(" % re.escape(s))
    is_decl = re.compile(r"\s*pub\s+(?:async\s+)?fn\s")
    called = False
    for f in files:
        text = body[f]
        for m in call.finditer(text):
            line = text[text.rfind("\n", 0, m.start()) + 1:m.end()]
            # Wlasna deklaracja nie jest wywolaniem samej siebie.
            if f == definer and is_decl.match(line):
                continue
            called = True
            break
        if called:
            break
    if called:
        continue
    if re.search(r"\b%s\b" % re.escape(s), tasks):
        continue
    orphans.append((s, definer or "?"))

if orphans:
    sys.stderr.write("these pub fn have no caller in %s and no task promises one:\n" % SRC)
    for s, d in orphans:
        sys.stderr.write("  %-28s %s\n" % (s, d))
    sys.stderr.write(
        "\na function used only from tests/ is NOT dead code to clippy -- integration tests\n"
        "are separate crates, so dead_code stays silent and the thing rots green. That is how\n"
        "engine::limits::Limiter landed with a passing test and zero production callers.\n"
        "Wire it, or write the task that will -- naming the function in tasks/*.md.\n")
    raise SystemExit(1)

print("wired: %d new pub fn, every one called from %s, registered as a command, or owned "
      "by a task" % (len(symbols), SRC))
PY
