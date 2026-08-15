#!/usr/bin/env bash
# ~0.3 s — niezmiennik 14: zero żargonu w tekście, który widzi użytkownik.
#
# CZYM TO NIE JEST. To nie jest ocena, czy zdanie jest dobre. Odpowiada na jedno wąskie
# pytanie: czy zakazany termin dociera na powierzchnię, którą użytkownik czyta. Wszystko
# subtelniejsze zostaje ludzkim osądem — i tak ma zostać.
#
# DLACZEGO SKANUJEMY TYLKO TEKST WIDOCZNY. Pierwsza wersja w meetnotes skanowała surowe
# źródło i dała 774 trafienia, z których prawie żadne nie było copy: identyfikatory,
# ścieżki importów, nazwy klas CSS. Sprawdzacz tak głośny uczy ludzi go ignorować, a to
# jest gorsze niż jego brak. Stąd: literały stringów ZE SPACJĄ, węzły tekstowe JSX,
# atrybuty aria-*/title/placeholder/alt, i ciała AppError::X("...") po stronie Rusta.
#
# DWIE LISTY, CELOWO OSOBNE:
#   checks/vocabulary-baseline.json   dług przejściowy. Może TYLKO maleć. To nie jest zgoda.
#   checks/vocabulary-allowlist.json  wyjątki trwałe. Każdy wpis ma zapisany powód.
# Zlanie ich w jeden plik jest dokładnie tym, jak baseline po cichu staje się pieczątką.
#
# Obie listy leżą w checks/, więc bieg ich nie edytuje. Baseline przesuwa człowiek:
#   bash checks/quick-vocabulary.sh --update-baseline
#
# Wybór wobec pustego drzewa: przechodzi z zerem trafień i tak ma być — nie ma jeszcze
# tekstu widocznego dla użytkownika. Nie jest to zieleń pusta, bo baseline wynosi 0:
# PIERWSZE trafienie, jakie kiedykolwiek powstanie, przewraca to sprawdzenie.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"
command -v python3 >/dev/null 2>&1 || { echo "python3 is not on PATH" >&2; exit 2; }

exec python3 - "$@" <<'PY'
import json, os, re, sys

ROOT = os.getcwd()
BASELINE = os.path.join(ROOT, "checks", "vocabulary-baseline.json")
ALLOWLIST = os.path.join(ROOT, "checks", "vocabulary-allowlist.json")

# Nazwa enuma błędów po stronie Rusta. Jeśli w src-tauri/src/error.rs nazywa się inaczej,
# to JEST miejsce, w którym się to mówi — inaczej połowa Rusta przestaje być skanowana
# i nikt tego nie zauważy, bo licznik dalej pokazuje zero.
RUST_ERROR_ENUMS = r"(?:AppError|LoadoutError|Error)"

# Lewa kolumna z docs/research/projects/00-SYNTHESIS.md §2.2 (wiążąca dla UI) plus tabela
# z docs/design/DESIGN.md §8 plus lista zakazana z docs/DECISIONS-LOCKED.md.
# Para: (wzorzec, czym to zastępujemy). Prawa kolumna jest częścią komunikatu błędu —
# sprawdzenie, które mówi tylko "nie wolno", zostawia autorowi zgadywanie.
BANNED = [
    # ── §2.2, kolejność tabeli ─────────────────────────────────────────────────────────
    (r"control plane",                       "Loadout"),
    (r"\bobjectives?\b",                     "goal"),
    (r"work items?\b",                       "step"),
    (r"\battempts?\b",                       "try — 'try 2 of 3'"),
    (r"\bDAGs?\b",                           "workflow"),
    (r"\b(?:plan|loop) nodes?\b",            "step"),
    (r"\b(?:dependency|loop) edges?\b",      "runs after"),
    (r"\bledgers?\b|event stream|DomainEvent", "activity"),
    (r"\bprojections?\b|\breducers?\b",      "(never shown — say what is true now)"),
    (r"authority facts?|\bauthority\b|AuthorityKind", "who said it"),
    (r"\bclaims?\b|\bclaimed\b",             "agent said"),
    (r"agent_finished",                      "agent says done"),
    (r"verification gate|\bgates?\b",        "check"),
    (r"gate_passed|gate_failed",             "checks passed / checks failed"),
    (r"NoTestsExecuted",                     "nothing ran"),
    (r"InfrastructureError",                 "could not run"),
    (r"\bverifiers?\b",                      "the checks"),
    (r"evidence receipts?|artifact receipts?|\breceipts?\b", "results"),
    (r"advisory verdict|\bverdicts?\b",      "second opinion"),
    (r"review_unavailable",                  "no second opinion configured"),
    (r"\bartifacts?\b",                      "file"),
    (r"\bsnapshots?\b",                      "saved copy"),
    (r"content-addressed",                   "(never shown)"),
    (r"\bsha-?256\b|\bdigests?\b|\bbindings?\b", "fingerprint — 8 chars, full one click away"),
    (r"\bworktrees?\b",                      "workspace"),
    (r"max_parallel",                        "how many at once"),
    (r"resource lanes?|ResourceDemand",      "slot"),
    (r"\bleases?\b|\bleased\b",              "(never shown — say 'in use by step 3')"),
    (r"\boutbox\b|idempotenc",               "(never shown)"),
    (r"FailureClass",                        "why it failed"),
    (r"RetryDisposition",                    "retry a few times / ask me / stop"),
    (r"circuit breaker",                     "same error again — stopping"),
    (r"plan_binding_stale|plan binding",     "the plan changed"),
    (r"gate\.decision_recorded|plan\.approval_requested", "(a wire enum never reaches a screen)"),
    (r"memory records?|MemoryStatus",        "note"),
    (r"context manifest|\bretrievals?\b",    "what this agent was told"),
    (r"\bMCP\b",                             "tool server"),
    (r"capability grants?|CapabilityProfile", "permissions"),
    (r"\bhandshakes?\b|doctor\(\)",          "check setup"),
    (r"\badapters?\b|\bproviders?\b",        "agent app — say 'Claude Code' or 'Codex'"),
    (r"agent rail|\brails?\b",               "the agents list"),
    (r"session inspector",                   "open this agent"),
    (r"EventFidelity|DegradedProcess",       "raw output only"),
    (r"acceptance criteri|\bAC-\d+\b",       "check"),
    (r"\bred tier\b|\bfast tier\b",          "before / quick"),
    (r"\bintegrate\b|\bintegration\b",       "land"),
    (r"repair rounds?",                      "fix round"),
    (r"\bprobes?\b",                         "measurement"),
    # ── DECISIONS-LOCKED, lista zakazana ───────────────────────────────────────────────
    (r"policy kernel",                       "(never shown)"),
    (r"durable records?",                    "(never shown)"),
    (r"\bWI-\d+\b|\bA#\d+\b",                "(never shown — name the step)"),
    # ── DESIGN.md §8 ───────────────────────────────────────────────────────────────────
    (r"\bsubmit\b|execute workflow",         "Run"),
    (r"\bconfiguration\b",                   "Settings"),
    (r"\binitiali[sz]e\b",                   "Create"),
    (r"\bterminate\b",                       "Stop"),
    (r"exit code",                           "didn't work"),
    (r"no records found",                    "Nothing here yet. Type /plan to start."),
    (r"tool[ _]use|tool calls?\b",           "name the action: Read, Edited, Ran"),
    (r"\bstdout\b|\bstderr\b",               "output"),
    (r"\btokens?\b|context window|compaction", "length / started a fresh page"),
    (r"\bPTY\b|\bspawn(?:ed|ing|s)?\b|\bprocess(?:es)?\b|\bsessions?\b",
                                             "terminal / agent / started"),
    (r"\borchestrators?\b",                  "lead agent"),
    (r"\bnodes?\b",                          "step"),
    (r"\bdiffs?\b|\bhunks?\b",               "changes"),
    (r"\breasoning\b",                       "Thinking…"),
]
BANNED = [(re.compile(p, re.I), p, r) for p, r in BANNED]

FE_EXT = (".ts", ".tsx")
RS_DIR = os.path.join("src-tauri", "src")
FE_DIR = "src"

_STR = re.compile(r"""(['"`])((?:\\.|(?!\1)[^\\])*)\1""")
_JSX_TEXT = re.compile(r">([^<>{}]{3,})<")
_ATTR = re.compile(
    r"""\b(?:aria-[a-z]+|title|placeholder|alt|label)\s*=\s*(?:\{\s*)?["']([^"']+)["']""", re.I)
_RS_MSG = re.compile(RUST_ERROR_ENUMS + r"::\w+\(\s*(?:format!\()?\s*\"([^\"]{4,})\"")


def prose(value):
    """Czy ten string może być zdaniem dla człowieka, a nie identyfikatorem/ścieżką/klasą."""
    if not re.search(r"\s", value):
        return False              # jedno słowo: nazwa klasy, klucz, event name
    if re.match(r"^[./#@]", value):
        return False              # ścieżka, selektor, import
    if not re.search(r"[a-z]{3}", value, re.I):
        return False
    return True


def visible_ts(source):
    """Tekst, który użytkownik może zobaczyć w pliku .ts/.tsx."""
    # Komentarze najpierw. Apostrof w polskim komentarzu otwiera fikcyjny literał
    # i połyka kilkaset znaków kodu, po czym cały komentarz czyta się jak copy.
    text = re.sub(r"/\*[\s\S]*?\*/", " ", source)
    text = re.sub(r"^\s*//.*$", " ", text, flags=re.M)
    out = [m.group(2) for m in _STR.finditer(text) if prose(m.group(2))]
    out += [m.group(1).strip() for m in _JSX_TEXT.finditer(text) if prose(m.group(1))]
    out += [m.group(1) for m in _ATTR.finditer(text)]
    return out


def visible_rs(source):
    return [m.group(1) for m in _RS_MSG.finditer(source)]


def walk(root, exts):
    for base, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d not in ("node_modules", "target", "gen", "dist")]
        for f in sorted(files):
            if f.endswith(exts):
                yield os.path.relpath(os.path.join(base, f), ROOT)


def scan():
    hits, scanned, seen = [], 0, set()
    sources = []
    if os.path.isdir(FE_DIR):
        sources += [(p, visible_ts) for p in walk(FE_DIR, FE_EXT)]
    if os.path.isdir(RS_DIR):
        sources += [(p, visible_rs) for p in walk(RS_DIR, (".rs",))]
    for path, extract in sources:
        scanned += 1
        with open(os.path.join(ROOT, path), encoding="utf-8", errors="replace") as fh:
            lines = extract(fh.read())
        for line in lines:
            for rx, pat, repl in BANNED:
                if rx.search(line):
                    # Ten sam napis bywa złapany dwa razy: raz jako literał stringa, raz jako
                    # wartość atrybutu aria-*. To jedno naruszenie, nie dwa — podwójne
                    # liczenie zawyżałoby baseline i psuło jego jedyną własność (maleje).
                    key = (path, pat, line.strip())
                    if key in seen:
                        continue
                    seen.add(key)
                    hits.append({"file": path, "term": pat, "instead": repl,
                                 "sample": line.strip()[:100]})
    return hits, scanned


def load(path, fallback):
    if not os.path.isfile(path):
        return fallback
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


args = set(sys.argv[1:])
hits, scanned = scan()
allow = load(ALLOWLIST, {"entries": []})
base = load(BASELINE, {"count": 0, "files": {}})
allowed = {"%s::%s" % (e["file"], e["term"]) for e in allow.get("entries", [])}
live = [h for h in hits if "%s::%s" % (h["file"], h["term"]) not in allowed]

if "--update-baseline" in args:
    # Zapadka (N-10, audyt 2026-08-15, zmierzone). `.claude/settings.json` przyznaje
    # `Bash(bash checks/:*)`, co czyta się jako „niech pisarz uruchamia sprawdzenia" — i łapie
    # także TĘ komendę, jedyną w checks/, która ZAPISUJE. Zmierzone: 3 trafienia na czerwono,
    # `--update-baseline`, `baseline written: 3 hits`, rc 0. Plik, którego `_comment` mówi
    # „ta liczba nigdy nie może rosnąć", urósł z wnętrza biegu, a N-06 ukrywał potem diff.
    # Baseline wolno tylko OPUSZCZAĆ.
    prev = int(base.get("count", 0))
    if len(live) > prev:
        print("refusing to raise the baseline: %d -> %d" % (prev, len(live)), file=sys.stderr)
        print("This file records debt that may only shrink. Rewrite the copy, or add an entry",
              file=sys.stderr)
        print("to the allowlist with a written reason — those are the only two ways out.",
              file=sys.stderr)
        sys.exit(1)
    files = {}
    for h in live:
        files[h["file"]] = files.get(h["file"], 0) + 1
    with open(BASELINE, "w", encoding="utf-8") as fh:
        json.dump({"_comment": "Transitional debt only. This number must never grow. "
                               "Entries leave by rewriting the copy, never by moving to "
                               "the allowlist without a reason.",
                   "count": len(live), "files": files}, fh, indent=2)
        fh.write("\n")
    print("baseline written: %d hits" % len(live))
    sys.exit(0)

ceiling = int(base.get("count", 0))
if len(live) > ceiling:
    print("jargon reached text a user can read (%d hits, baseline %d)"
          % (len(live), ceiling), file=sys.stderr)
    by_file = {}
    for h in live:
        by_file.setdefault(h["file"], []).append(h)
    for f in sorted(by_file):
        print("\n  %s" % f, file=sys.stderr)
        for h in by_file[f][:6]:
            print("    %-34s -> %s" % (h["term"], h["instead"]), file=sys.stderr)
            print("      %s" % h["sample"], file=sys.stderr)
        if len(by_file[f]) > 6:
            print("    ... %d more" % (len(by_file[f]) - 6), file=sys.stderr)
    print("\ndetail: the binding table is 00-SYNTHESIS.md §2.2 plus DESIGN.md §8.",
          file=sys.stderr)
    print("detail: a permanent exception goes in checks/vocabulary-allowlist.json "
          "WITH a reason.", file=sys.stderr)
    sys.exit(1)

note = ""
if len(live) < ceiling:
    note = " — baseline may shrink to %d" % len(live)
print("vocabulary: %d file%s scanned, %d user-visible hit%s (baseline %d, allowlisted %d)%s"
      % (scanned, "" if scanned == 1 else "s",
         len(live), "" if len(live) == 1 else "s",
         ceiling, len(allowed), note))
PY
