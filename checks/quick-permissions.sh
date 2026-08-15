#!/usr/bin/env bash
# quick-permissions — pytanie o OSIĄGALNOŚĆ, nie o kształt reguły.
#
# Dwa incydenty, jeden po drugim, obie w tym samym pliku konfiguracyjnym:
#
#   2026-08-15, pierwszy przejazd T-01: deny miało `Write(~/**)` i `Write(../**)`. Worktree leży
#   pod `~`, a jego katalog nadrzędny to `../`, więc obie reguły zabraniały CAŁEGO repo. Deny
#   wygrywa z allow, więc `Write(src/**)` nigdy nie zadziałało. 55 tur, 16 minut, $6,98, zero
#   plików. Każda reguła z osobna wyglądała rozsądnie.
#
#   2026-08-15, audyt (N-05): trzynaście ścieżek było zabronionych dla `Edit` i dla niczego
#   więcej. `Write` to inne narzędzie — całoplikowy Write Cargo.toml zdejmuje [workspace.lints],
#   po czym `clippy -D warnings` przestaje widzieć unwrapy, bo ta polityka nie mieszka nigdzie
#   indziej. Ta sama runda wykryła 19 kolizji deny × OWNS: wszystkie trzy spike'i nie mogły
#   zapisać własnego wyniku, a T-01 własnego tauri.conf.json.
#
# Wniosek, który ten plik egzekwuje: reguły nie czyta się z góry na dół. Zadaje się jej pytanie
# „czy TEN aktor może jeszcze zrobić TĘ rzecz", osobno dla każdego czasownika.
set -euo pipefail

SETTINGS=".claude/settings.json"
[ -f "$SETTINGS" ] || {
  echo "$SETTINGS is missing — the harness runs its writer with --setting-sources project," >&2
  echo "so without it the writer has no permissions at all." >&2
  exit 2
}

python3 - "$SETTINGS" <<'PY'
import fnmatch, json, os, pathlib, re, sys

settings = sys.argv[1]
root = os.path.realpath(os.getcwd())
home = os.path.expanduser("~")

try:
    cfg = json.load(open(settings))
except Exception as exc:
    print(f"{settings} is not valid JSON: {exc}", file=sys.stderr)
    sys.exit(2)

perms = cfg.get("permissions") or {}
deny = perms.get("deny") or []
allow = perms.get("allow") or []
if not isinstance(deny, list) or not isinstance(allow, list):
    print("permissions.deny / permissions.allow must be lists", file=sys.stderr)
    sys.exit(2)

MUTATING = ("Edit", "Write")   # dokładnie to, co harness przekazuje w --allowedTools; reszta jest nieosiągalna
RULE = re.compile(r"^(\w+)\((.*)\)$")

def parsed(entries):
    for e in entries:
        m = RULE.match(str(e).strip())
        if m and m.group(1) in MUTATING:
            yield str(e).strip(), m.group(1), m.group(2)

def absolute(pat: str) -> str:
    if pat.startswith("~/"):
        pat = home + pat[1:]
    if not pat.startswith("/"):
        pat = os.path.join(root, pat)
    return os.path.normpath(pat)

def matches(pattern: str, rel: str) -> bool:
    target = os.path.join(root, rel)
    p = absolute(pattern)
    return fnmatch.fnmatch(target, p) or fnmatch.fnmatch(target + "/x", p)

problems = []

# ── 1. Czy pisarz może zapisać pliki, które TO zadanie posiada? ─────────────────────────────
# Źródłem jest blok OWNS z TASK.md, nie zamrożona lista w tym pliku. Poprzednia wersja miała
# pięć ścieżek wpisanych na sztywno, starszych niż jakikolwiek plik zadania — i przepuściła
# `Edit(src-tauri/tauri.conf.json)` mimo że T-01 tę ścieżkę posiada, a AC-2 jest o niej.
owns = []
task = pathlib.Path("TASK.md")
if task.is_file():
    block = re.search(r"<!--\s*OWNS(.*?)-->", task.read_text(), re.S)
    if block:
        owns = [l.strip().rstrip("/") for l in block.group(1).strip().splitlines() if l.strip()]

must_write = [o if "." in os.path.basename(o) else f"{o}/probe.rs" for o in owns] or [
    "src/App.tsx", "src-tauri/src/lib.rs", "src-tauri/tests/probe.rs",
]
label = "this task's OWNS block" if owns else "the default set (no TASK.md here)"

for entry, verb, pattern in parsed(deny):
    for rel in must_write:
        if matches(pattern, rel):
            problems.append(f"  {entry:<34} blocks {rel}   [{label}]")
            break

# ── 2. Czy coś, czego NIKT nie posiada, jest wciąż osiągalne którymkolwiek czasownikiem? ────
# Write to inne narzędzie niż Edit. Zakaz na jeden czasownik to brak zakazu.
MUST_NOT_WRITE = [
    "harness/gate.py", "harness/guards.sh",
    ".claude/settings.json", ".claude/hooks/stop-gate.sh",
    "AGENTS.md", "docs/DECISIONS-LOCKED.md",
    "tasks/T-01.md", "verify.sh", "ship-task.sh",
    "Cargo.toml", "package.json", "rust-toolchain.toml",
]
owned_now = {o for o in owns}
for rel in MUST_NOT_WRITE:
    if any(rel == o or rel.startswith(o + "/") for o in owned_now):
        continue                      # to zadanie legalnie to posiada
    for verb in MUTATING:
        if not any(v == verb and matches(pat, rel) for _, v, pat in parsed(deny)):
            problems.append(f"  {rel:<34} is still reachable by {verb}")

# ── 3. Czy deny nie kłóci się z blokiem OWNS KTÓREGOKOLWIEK zadania? ────────────────────────
# Sprzeczność jest cicha: pisarz odkrywa ją metodą prób i czyta jako losową awarię narzędzia.
collisions = []
for f in sorted(pathlib.Path("tasks").glob("[ST]-*.md")):
    block = re.search(r"<!--\s*OWNS(.*?)-->", f.read_text(), re.S)
    if not block:
        continue
    for line in block.group(1).strip().splitlines():
        rel = line.strip().rstrip("/")
        if not rel:
            continue
        for entry, _verb, pattern in parsed(deny):
            if matches(pattern, rel):
                collisions.append(f"  {f.stem} owns {rel}, but {entry} forbids it")
                break

if problems or collisions:
    if problems:
        print("a permission rule and the work do not agree:", file=sys.stderr)
        for line in problems:
            print(line, file=sys.stderr)
    if collisions:
        print("a task owns a path its own permissions forbid:", file=sys.stderr)
        for line in collisions:
            print(line, file=sys.stderr)
    print("", file=sys.stderr)
    print("deny beats allow, and Write is a different tool from Edit. Name the directories you", file=sys.stderr)
    print("actually want closed, pair every verb, and never anchor at ~/ or ../ — the repository", file=sys.stderr)
    print("lives under both.", file=sys.stderr)
    sys.exit(1)

print(f"permissions: {len(deny)} deny rules · writable: {label} · nothing protected is reachable")
PY
