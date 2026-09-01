#!/usr/bin/env bash
# ~0.2 s — DESIGN.md i src/styles/theme.css muszą mówić to samo, a komponenty nie mają prawa
# mówić nic własnego.
#
# DLACZEGO W OGÓLE. Tokeny w spreadsheet były przepisywane RĘCZNIE z dokumentu do arkusza
# i nic tego nie sprawdzało (raport 07: "hand-transcribing design tokens with no verification
# step"). Dokument projektowy, który rozjechał się z kodem, jest gorszy niż jego brak: dalej
# wygląda na źródło prawdy i dalej jest cytowany w recenzjach.
#
# DWIE POŁOWY, DWIE RÓŻNE AWARIE:
#   1. rozjazd DESIGN.md <-> theme.css — ktoś zmienił jedno z dwóch luster
#   2. literał w komponencie — ktoś ominął oba
#
# Wybór wobec pustego drzewa: połowa 1 działa OD ZARAZ (oba pliki istnieją i mają po 21
# tokenów koloru), więc to sprawdzenie nie jest puste ani przez chwilę. Połowa 2 nie ma
# jeszcze plików do przejrzenia i mówi to wprost, zamiast raportować "0 naruszeń".
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"
command -v python3 >/dev/null 2>&1 || { echo "python3 is not on PATH" >&2; exit 2; }

exec python3 - <<'PY'
import os, re, sys

ROOT = os.getcwd()
DESIGN = os.path.join(ROOT, "docs", "design", "DESIGN.md")
THEME = os.path.join(ROOT, "src", "styles", "theme.css")

for p in (DESIGN, THEME):
    if not os.path.isfile(p):
        print("%s is missing, so the two cannot be compared" % os.path.relpath(p, ROOT),
              file=sys.stderr)
        sys.exit(2)

read = lambda p: open(p, encoding="utf-8").read()

# DESIGN.md podaje tokeny na dwa sposoby: wierszem tabeli `| `--bg` | `#06090b` | ... |`
# oraz blokiem kodu `--accent-edge #3d8a70`. Jeden wzorzec obsługuje oba: nazwa, potem
# wyłącznie backticki / spacja / jedna pionowa kreska, potem hex. Zdanie prozą pomiędzy
# (`--accent` jest jedynym...) nie pasuje i o to chodzi.
TOKEN_HEX = re.compile(r"--([a-z][a-z0-9-]*)`?\s*\|?\s*`?(#[0-9a-fA-F]{6})\b")

design = {}
for name, hexv in TOKEN_HEX.findall(read(DESIGN)):
    design[name] = hexv.lower()

theme = {}
for name, hexv in re.findall(r"--color-([a-z][a-z0-9-]*)\s*:\s*(#[0-9a-fA-F]{6})\s*;",
                             read(THEME)):
    theme[name] = hexv.lower()

problems = []
for name in sorted(set(design) | set(theme)):
    d, t = design.get(name), theme.get(name)
    if d and t and d != t:
        problems.append("  --%-14s DESIGN.md says %s, theme.css says %s" % (name, d, t))
    elif d and not t:
        problems.append("  --%-14s only in DESIGN.md (%s) — theme.css never defines it" % (name, d))
    elif t and not d:
        problems.append("  --%-14s only in theme.css (%s) — DESIGN.md never documents it" % (name, t))

# ── Połowa 2: literały w komponentach. Tokeny albo nic. ────────────────────────────────────
HEX = re.compile(r"#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b")
SIZE = re.compile(r"(?:font-size|fontSize|border-radius|borderRadius)\s*:\s*[^,;}\n]*", re.I)
# Tailwind arbitrary value: text-[13px], rounded-[2px], bg-[#06090b]. To jest ta sama
# ucieczka co literał w CSS, tylko zapisana w klasie, więc omija oko recenzenta.
ARBITRARY = re.compile(r"\b(?:text|rounded|leading|tracking|bg|border|fill|stroke)-\[[^\]]+\]")

def strip_comments(src):
    src = re.sub(r"/\*[\s\S]*?\*/", " ", src)
    return re.sub(r"^\s*//.*$", " ", src, flags=re.M)

literals, files = [], 0
for base, dirs, names in os.walk(os.path.join(ROOT, "src")):
    dirs[:] = [d for d in dirs if d not in ("node_modules", "dist")]
    for n in sorted(names):
        if not n.endswith((".tsx", ".ts", ".css")):
            continue
        rel = os.path.relpath(os.path.join(base, n), ROOT)
        if rel == os.path.join("src", "styles", "theme.css"):
            continue          # jedyny plik, w którym hex JEST na miejscu
        files += 1
        src = strip_comments(open(os.path.join(base, n), encoding="utf-8",
                                  errors="replace").read())
        for i, line in enumerate(src.splitlines(), 1):
            for m in HEX.finditer(line):
                literals.append("  %s:%d  hex literal %s — use a token from theme.css"
                                % (rel, i, m.group(0)))
            for m in SIZE.finditer(line):
                val = m.group(0)
                if re.search(r"\d", val) and "var(" not in val:
                    literals.append("  %s:%d  %s — use --text-* or --radius-sq"
                                    % (rel, i, val.strip()[:60]))
            for m in ARBITRARY.finditer(line):
                literals.append("  %s:%d  %s — Tailwind arbitrary value, use a token class"
                                % (rel, i, m.group(0)))

if problems or literals:
    if problems:
        print("DESIGN.md and src/styles/theme.css disagree", file=sys.stderr)
        print("\n".join(problems[:30]), file=sys.stderr)
    if literals:
        print("a component states a colour or a size instead of naming a token",
              file=sys.stderr)
        print("\n".join(literals[:30]), file=sys.stderr)
        if len(literals) > 30:
            print("  ... %d more" % (len(literals) - 30), file=sys.stderr)
    print("detail: DESIGN.md is the source; theme.css is its mirror (DESIGN.md §9).",
          file=sys.stderr)
    sys.exit(1)

seen = "%d colour tokens agree" % len(theme)
where = "%d component files carry no literal" % files if files else \
        "no .tsx/.ts/.css under src/ besides theme.css yet"
print("tokens: %s, %s" % (seen, where))
PY
