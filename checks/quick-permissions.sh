#!/usr/bin/env bash
# quick-permissions — czy pisarz w ogóle może zapisać własny kod?
#
# Incydent, który to zrodził (2026-08-15, pierwszy przejazd ship-task.sh na T-01):
# .claude/settings.json miał w deny `Write(~/**)` i `Write(../**)`. Worktree leży pod `~`,
# a jego katalog nadrzędny to `../` — więc obie reguły zabraniały CAŁEGO repo. Deny wygrywa
# z allow, więc `Write(src/**)` z listy allow nigdy nie zadziałało. Pisarz spalił 55 tur,
# 16 minut i $6,98, nie zapisując ani jednego pliku, i dopiero bramka to zatrzymała.
#
# Ten check NIE szuka zakazanych napisów — to byłby test obecności stringa (niezmiennik 20).
# Bierze konkretne ścieżki, które pisarz MUSI umieć zapisać, i sprawdza, czy któraś reguła
# deny je łapie. Reguła może wyglądać dowolnie; liczy się, co realnie zasłania.
set -euo pipefail

SETTINGS=".claude/settings.json"

if [ ! -f "$SETTINGS" ]; then
  echo "$SETTINGS is missing -- the harness runs its writer with --setting-sources project," >&2
  echo "so without it the writer has no permissions at all." >&2
  exit 2
fi

python3 - "$SETTINGS" <<'PY'
import fnmatch, json, os, re, sys

settings = sys.argv[1]
root = os.path.realpath(os.getcwd())
home = os.path.expanduser("~")

try:
    cfg = json.load(open(settings))
except Exception as exc:                       # niepoprawny JSON to NASZ błąd, nie czerwone
    print(f"{settings} is not valid JSON: {exc}", file=sys.stderr)
    sys.exit(2)

deny = (cfg.get("permissions") or {}).get("deny") or []
if not isinstance(deny, list):
    print("permissions.deny is not a list", file=sys.stderr)
    sys.exit(2)

# Ścieżki, bez których żadne zadanie w tasks/ nie da się wykonać.
MUST_WRITE = [
    "src/App.tsx",
    "src/ui/shell/window.test.tsx",
    "src-tauri/src/lib.rs",
    "src-tauri/src/engine/scheduler.rs",
    "src-tauri/tests/engine_scheduler.rs",
]

RULE = re.compile(r"^(Edit|Write|MultiEdit|NotebookEdit)\((.*)\)$")

def expand(pat: str) -> str:
    """Do postaci bezwzględnej, tak jak widzi ją warstwa uprawnień."""
    if pat.startswith("~/"):
        pat = home + pat[1:]
    if not pat.startswith("/"):
        pat = os.path.join(root, pat)
    return os.path.normpath(pat)

blockers = []
for entry in deny:
    m = RULE.match(str(entry).strip())
    if not m:
        continue
    pattern = expand(m.group(2))
    for rel in MUST_WRITE:
        target = os.path.join(root, rel)
        # fnmatch traktuje * jako pasujące także przez ukośniki — dokładnie tak działa
        # `~/**`, i dokładnie dlatego zasłoniło całe repo.
        if fnmatch.fnmatch(target, pattern):
            blockers.append((str(entry), rel))
            break

if blockers:
    print("the writer cannot write its own code -- a deny rule shadows the repository:", file=sys.stderr)
    for entry, rel in blockers:
        print(f"  {entry:<24} blocks {rel}", file=sys.stderr)
    print("", file=sys.stderr)
    print("deny beats allow, so an entry like Write(src/**) never gets a chance.", file=sys.stderr)
    print("Name the directories you actually want closed; never anchor at ~/ or ../,", file=sys.stderr)
    print("because the repository itself lives under both.", file=sys.stderr)
    sys.exit(1)

print(f"permissions: {len(deny)} deny rules, none of them shadows the repository")
PY
