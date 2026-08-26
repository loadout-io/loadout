#!/usr/bin/env bash
# Druga opinia o zmianie, która JUŻ przeszła bramkę. Tylko do odczytu, w budżecie czasu.
#
#   ./review.sh                                    # domyślnie: pisał claude → recenzuje codex
#   ./review.sh --agent codex --reviewer claude
#   ./review.sh codex                              # forma pozycyjna z AGENTS.md §6
#
# Recenzent nie zatwierdza i nie blokuje (D3): schemat odpowiedzi strukturalnie nie ma czego
# zatwierdzić. Dlatego ten skrypt ZAWSZE kończy się 0 — także wtedy, gdy vendor jest
# niedostępny. Kod 2 jest zarezerwowany dla NASZEJ złej konfiguracji; w spreadsheet brakujący
# plik schematu przez dwie sesje udawał limit kredytów u vendora i tak został zdiagnozowany.
#
# Bramka dowodzi, że sprawdzenie przeszło. Nie dowodzi, że sprawdzenie było właściwe — i to
# jedyna luka, którą model potrafi zamknąć, a komenda powłoki nie.
set -euo pipefail
set +m                       # bez notek "Terminated", gdy stoper zdejmuje recenzenta

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
cd "$ROOT"

SCHEMA="harness/review-schema.json"
BUDGET="${LOADOUT_REVIEW_BUDGET:-900}"   # 240 s okazało się za mało: rc=143 to nasz własny
                                         # stoper zabijający recenzenta, który pracował.
DIFF_LINES="${LOADOUT_REVIEW_DIFF_LINES:-4000}"

# Modele pinujemy w repo, nie na maszynie. Powód z 06 §2: pierwsze zadanie w spreadsheet
# poszło na innym modelu tylko dlatego, że taki był domyślny na tym laptopie.
# Efortu NIE podajemy w ciemno: T1 §3.1-3.2 nie zweryfikował flagi --effort dla claude, a
# nieznana flaga kończy proces natychmiast, co przebiera się za "recenzent niedostępny".
CLAUDE_MODEL="${LOADOUT_CLAUDE_MODEL:-claude-opus-5[1m]}"
CODEX_MODEL="${LOADOUT_CODEX_MODEL:-gpt-5.6-sol}"
# Same-vendor: recenzent MUSI dostać inny model (D3). "sonnet" to zweryfikowany alias
# (T1 §3.1), "gpt-5.5" to model, który zna lokalny codex.
CLAUDE_REVIEW_MODEL="${LOADOUT_CLAUDE_REVIEW_MODEL:-sonnet}"
CODEX_REVIEW_MODEL="${LOADOUT_CODEX_REVIEW_MODEL:-gpt-5.5}"

usage() { echo "usage: ./review.sh [--agent claude|codex] [--reviewer claude|codex]" >&2; }
die2()  { printf 'review.sh is misconfigured: %s\n' "$1" >&2
          echo "That is our bug, not the reviewer's -- fix it rather than reading it as a limit." >&2
          exit 2; }
other() { case "$1" in claude) echo codex ;; codex) echo claude ;; esac; }

AGENT="${LOADOUT_AGENT:-}"
REVIEWER="${LOADOUT_REVIEWER:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    --agent)    AGENT="${2:-}";    [ -n "$AGENT" ]    || { usage; exit 2; }; shift 2 ;;
    --reviewer) REVIEWER="${2:-}"; [ -n "$REVIEWER" ] || { usage; exit 2; }; shift 2 ;;
    claude|codex) REVIEWER="$1"; shift ;;
    -h|--help)  usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
done
# Domyślnie cross-vendor: według researchu każdy realny defekt w spreadsheet znalazł
# recenzent INNEGO vendora, i to na zielonej bramce.
if [ -z "$AGENT" ]    && [ -n "$REVIEWER" ]; then AGENT="$(other "$REVIEWER")"; fi
if [ -z "$AGENT" ];    then AGENT=claude; fi
if [ -z "$REVIEWER" ]; then REVIEWER="$(other "$AGENT")"; fi
for v in "$AGENT" "$REVIEWER"; do
  case "$v" in claude|codex) ;; *) echo "vendor must be claude or codex, got: $v" >&2; usage; exit 2 ;; esac
done

# ── to, co jest NASZĄ konfiguracją: brak = 2, nigdy 1 ────────────────────────────────────
# Świadomy wybór na prawie pustym drzewie: bez TASK.md ten skrypt NIE przechodzi cicho.
# Zielone "nothing to add" bez kryteriów czytałoby się jak zatwierdzenie, a to jedyna rzecz,
# której recenzent nigdy nie może wybić.
[ -f "$SCHEMA" ]      || die2 "$SCHEMA is missing"
command -v python3 >/dev/null 2>&1 || die2 "python3 is not on PATH"
command -v git     >/dev/null 2>&1 || die2 "git is not on PATH"
[ -f TASK.md ]        || die2 "there is no TASK.md here -- a second opinion needs the criteria it judges against"
# To nie jest drugi parser kontraktu (nim jest bramka) — tylko sprawdzenie obecności.
if ! grep -qE '^##[[:space:]]+[A-Z]{2,8}-[0-9]+' TASK.md; then
  die2 "TASK.md declares no acceptance criteria, so a review can only report on itself"
fi
[ -f runs/last.json ] || die2 "no gate receipt at runs/last.json -- run ./verify.sh full first;
  a second opinion on an ungated tree reviews the wrong thing"

[ -f harness/process-group.sh ] || die2 "harness/process-group.sh is missing"
# shellcheck source=harness/process-group.sh
. harness/process-group.sh

# Od tego miejsca "zawsze 0" jest strukturalne, nie deklaratywne. Powód: obcinanie diffu
# wywracało skrypt SIGPIPE-em (rc=141) i awaria recenzji czytałaby się jak awaria bramki.
# Nasza zła konfiguracja (kod 2) jest już za nami, więc nic poniżej nie ma prawa być czerwone.
ALWAYS_ZERO=1
finish() {
  local rc=$?
  trap - EXIT
  trap '' INT TERM
  if [ -n "${LOADOUT_AGENT_GROUP_PID:-}" ]; then
    loadout_agent_group_stop || { rc=2; ALWAYS_ZERO=0; }
  fi
  if [ "${ALWAYS_ZERO:-0}" = 1 ] && [ "$rc" -ne 0 ]; then
    echo "review.sh hit an internal error (rc=$rc) -- advisory only, so this is not a red." >&2
    exit 0
  fi
  exit "$rc"
}
trap finish EXIT

review_interrupted() {
  local rc=3
  if loadout_agent_group_defer_interrupt; then return; fi
  # Przerwanie nie jest niedostepnoscia recenzenta. Nie wolno pozwolic finish() zamienic 3 na 0.
  ALWAYS_ZERO=0
  trap '' INT TERM
  loadout_agent_group_stop || rc=2
  if [ "$rc" = 3 ]; then
    printf '\nreview interrupted; active agent process group is dead\n' >&2
  else
    printf '\nreview interrupted; process-group death was not proved\n' >&2
  fi
  exit "$rc"
}
trap review_interrupted INT TERM

mkdir -p runs
RAW="runs/review.raw"; LOG="runs/review.log"; OUT="runs/review.json"
# Stary werdykt kasujemy ZANIM cokolwiek odpalimy: nieaktualny runs/review.json podszyłby
# się pod dzisiejszą opinię w repair.sh.
rm -f "$OUT" "$RAW" "$LOG"

# ── kontekst dla recenzenta ──────────────────────────────────────────────────────────────
# Baza diffu: commit, który wniósł TASK.md — pierwszy commit gałęzi, czyli kontrakt.
BASE=""; FROM_WHAT=""
if git rev-parse --verify -q HEAD >/dev/null 2>&1; then
  BASE="$(git log --diff-filter=A --format=%H -- TASK.md 2>/dev/null | head -1 || true)"
  FROM_WHAT="the commit that added TASK.md, i.e. the contract"
  if [ -z "$BASE" ]; then
    BASE="$(git merge-base HEAD main 2>/dev/null || true)"; FROM_WHAT="where this branch left main"
  fi
  if [ -z "$BASE" ]; then
    BASE="HEAD"; FROM_WHAT="HEAD; nothing links this tree to a contract commit, so only uncommitted work is visible"
  fi
fi
if [ -n "$BASE" ]; then
  DIFF="$( { git diff --stat "$BASE" -- .; echo; git diff "$BASE" -- .; } 2>/dev/null || true )"
  DIFF_FROM="git diff $BASE -- $FROM_WHAT, to the working tree, uncommitted work included"
else
  DIFF=""; DIFF_FROM="no commits yet in this repository"
fi
TOTAL="$(printf '%s\n' "$DIFF" | wc -l | tr -d ' ')"
if [ "$TOTAL" -gt "$DIFF_LINES" ]; then
  # `|| true` nie jest ozdobą: head zamyka rurę po N liniach, printf dostaje SIGPIPE, a
  # pipefail + set -e wywracały przez to CAŁY skrypt kodem 141 — i to dokładnie na dużym
  # diffie, czyli w jedynym przypadku, dla którego to obcinanie istnieje.
  DIFF="$(printf '%s\n' "$DIFF" 2>/dev/null | head -n "$DIFF_LINES" || true)
[... diff truncated at $DIFF_LINES of $TOTAL lines. Read the files directly for the rest.]"
fi
[ -n "$DIFF" ] || DIFF="(the diff is empty -- say so under what_i_could_not_verify)"
UNTRACKED="$(git status --porcelain=v1 -uall 2>/dev/null | grep '^??' || true)"
[ -n "$UNTRACKED" ] || UNTRACKED="(none)"

# ── kto recenzuje i czym ─────────────────────────────────────────────────────────────────
SAME=0
if [ "$AGENT" = "$REVIEWER" ]; then SAME=1; fi
if [ "$REVIEWER" = claude ]; then
  if [ "$SAME" -eq 1 ]; then MODEL="$CLAUDE_REVIEW_MODEL"; else MODEL="$CLAUDE_MODEL"; fi
else
  if [ "$SAME" -eq 1 ]; then MODEL="$CODEX_REVIEW_MODEL"; else MODEL="$CODEX_MODEL"; fi
fi

ROLE=""
if [ "$SAME" -eq 1 ]; then
  echo "second opinion: writer and reviewer are both $REVIEWER -- THE WEAKER MODE."
  echo "  Cross-vendor is the default because every real defect in the source project was found"
  echo "  by the other vendor on a green gate. Here the reviewer only gets a different model"
  echo "  ($MODEL, writer runs $([ "$AGENT" = claude ] && echo "$CLAUDE_MODEL" || echo "$CODEX_MODEL")) and an explicit reviewer role."
  ROLE="WHO YOU ARE. The same vendor wrote this code. You are running as a different model with
one job: to disagree with it. Assume the author was you an hour ago and that you were wrong.
This is the weaker of the two review modes -- a model's blind spot tends to be the same blind
spot twice -- so raise the thing you would rather let pass.
"
fi

if ! command -v "$REVIEWER" >/dev/null 2>&1; then
  echo "no second opinion: $REVIEWER is not installed -- advisory only, carrying on"
  exit 0
fi

PROMPT="You are the second opinion on a change that has ALREADY passed its automated gate.
You cannot approve it and you cannot block it. Your reply is advisory; the gate decides.

$ROLE
Do not re-run the tests. Do not report style, naming, formatting or structure. Do not report
anything the gate would already catch. You may read files in this repository; you may not
change any of them.

For every acceptance criterion in TASK.md, ask exactly one question:

    does the implementation satisfy the CRITERION, or only the ASSERTION written for it?

Ask it the hard way: what does the laziest implementation that passes this check look like,
and would it be wrong? Report only:
  - an assertion weak enough that a wrong implementation would also pass it;
  - a value hard-coded to the exact input the test uses;
  - a branch the test never reaches;
  - an edge case the criterion implies and the test ignores;
  - a criterion identity satisfies (f(f(x)) == x is met by returning the argument);
  - a check that passes without running anything.

WHAT I COULD NOT VERIFY -- required. Every finding must fill what_i_could_not_verify: what you
could NOT check from here (a file outside the diff, behaviour that needs the app running, a
number you could not reproduce). Write \"nothing\" only when that is true. If something material
was unverifiable and belongs to no finding, raise it as its own finding with severity \"low\"
and a claim beginning \"not verified:\".

If you find nothing of that kind, return verdict \"none\" with an empty findings list. Do not
invent concerns to look thorough. At most 6 findings: if you have more, rank them and keep six.

Reply with ONLY a JSON object matching this schema -- no prose, no code fence:
$(cat "$SCHEMA")

===== TASK.md =====
$(cat TASK.md)

===== the gate receipt (runs/last.json) =====
$(cat runs/last.json)

===== the change ($DIFF_FROM) =====
$DIFF

===== files git does not track, so they are not in the diff above =====
$UNTRACKED
"

# Prompt idzie STDIN-em, nigdy w argv (niezmiennik 9). Funkcja kończy się `exec`, więc $!
# to pid samego CLI — stoper zabija recenzenta, a nie powłokę, która go trzyma.
spawn_reviewer() {
  if [ "$REVIEWER" = codex ]; then
    # -o zapisuje ostatnią wiadomość (nasz JSON); strumień zdarzeń idzie do logu.
    exec codex exec --json --skip-git-repo-check -C "$ROOT" -s read-only --ignore-user-config \
      -m "$MODEL" --output-schema "$SCHEMA" -o "$RAW" >"$LOG" 2>&1
  fi
  # --strict-mcp-config --setting-sources "" tną koszt kontekstu ~6x (T1 §3.3) i nie psują
  # OAuth, w przeciwieństwie do --bare. Recenzent dostaje wyłącznie narzędzia do czytania.
  exec claude -p --output-format stream-json --verbose \
    --strict-mcp-config --setting-sources "" \
    --model "$MODEL" --permission-mode dontAsk --allowedTools "Read,Grep,Glob" \
    --max-turns "${LOADOUT_REVIEW_MAX_TURNS:-40}" >"$RAW" 2>"$LOG"
}

echo "second opinion by $REVIEWER ($MODEL), ${BUDGET}s budget"
# Wspolny rdzen daje recenzentowi osobny PGID, ogranicza czas i dowodzi ESRCH po kazdym
# zakonczeniu. Podstawienie procesu utrzymuje prompt wylacznie na stdin (niezmienniki 6 i 9).
started=0
loadout_agent_group_start spawn_reviewer < <(printf '%s\n' "$PROMPT") || started=$?
if [ "$started" = 3 ]; then review_interrupted; fi
if [ "$started" != 0 ]; then
  ALWAYS_ZERO=0
  exit 2
fi
rc=0; loadout_agent_group_wait "$BUDGET" || rc=$?
if [ "$LOADOUT_AGENT_GROUP_PROOF_FAILED" = 1 ]; then
  ALWAYS_ZERO=0
  exit 2
fi

# Walidacja PRZECIW plikowi schematu, nie przeciw kształtowi przepisanemu tutaj — inaczej
# schemat i jego czytelnik rozjeżdżają się po cichu (niezmiennik 23).
# `|| echo` trzyma obietnicę "zawsze 0" nawet wtedy, gdy to nasz walidator się wywróci:
# druga opinia, która przewraca bieg, jest gorsza niż jej brak.
python3 - "$SCHEMA" "$RAW" "$REVIEWER" "$OUT" "$rc" "$BUDGET" <<'PY' || echo "no second opinion: the review validator itself failed -- advisory only, carrying on"
import json, sys

schema_p, raw_p, vendor, out_p, rc, budget = sys.argv[1:7]

def unavailable(msg):
    # Niedostępny recenzent to fakt o świecie, nie o kodzie. Zawsze 0.
    print("no second opinion: %s (rc=%s, %ss budget) -- advisory only, carrying on" % (msg, rc, budget))
    raise SystemExit(0)

try:
    raw = open(raw_p, encoding="utf-8", errors="replace").read()
except OSError:
    unavailable("the reviewer wrote nothing")

if vendor == "claude":
    # stream-json: interesuje nas wyłącznie zdarzenie `result`.
    text = None
    for line in raw.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            ev = json.loads(line)
        except ValueError:
            continue
        if ev.get("type") == "result":
            # Nigdy nie rozgałęziaj po samym subtype: nieudany bieg potrafi mieć
            # subtype "success" z is_error=true (T1 §4.4).
            if ev.get("is_error"):
                unavailable("claude ended with %s" % (ev.get("terminal_reason") or "an error"))
            text = ev.get("result") or ""
    if text is None:
        unavailable("claude produced no result event")
else:
    text = raw

i, j = text.find("{"), text.rfind("}")
if i < 0 or j <= i:
    unavailable("the reply carried no JSON object")
try:
    doc = json.loads(text[i:j + 1])
except ValueError as exc:
    unavailable("the reply was not valid JSON (%s)" % exc)

def check(node, sch, path):
    t = sch.get("type")
    if t == "object":
        if not isinstance(node, dict):
            return "%s is not an object" % path
        props = sch.get("properties", {})
        if sch.get("additionalProperties") is False:
            for k in node:
                if k not in props:
                    return "%s has an unexpected key %r" % (path, k)
        for k in sch.get("required", []):
            if k not in node:
                return "%s is missing %r" % (path, k)
        for k, v in node.items():
            if k in props:
                bad = check(v, props[k], "%s.%s" % (path, k))
                if bad:
                    return bad
    elif t == "array":
        if not isinstance(node, list):
            return "%s is not an array" % path
        cap = sch.get("maxItems")
        if cap is not None and len(node) > cap:
            return "%s carries %d items, the schema allows %d" % (path, len(node), cap)
        for n, item in enumerate(node):
            bad = check(item, sch.get("items", {}), "%s[%d]" % (path, n))
            if bad:
                return bad
    elif t == "string" and not isinstance(node, str):
        return "%s is not a string" % path
    elif t == "integer" and (isinstance(node, bool) or not isinstance(node, int)):
        return "%s is not an integer" % path
    if "enum" in sch and node not in sch["enum"]:
        return "%s is not one of %s" % (path, sch["enum"])
    return None

bad = check(doc, json.load(open(schema_p)), "review")
if bad:
    # Proza zamiast schematu to niedostępność, nie werdykt: pół-zrozumiana opinia jest
    # gorsza niż żadna.
    unavailable("the reply does not match harness/review-schema.json (%s)" % bad)

with open(out_p, "w", encoding="utf-8") as fh:
    json.dump(doc, fh, indent=2, ensure_ascii=False)
    fh.write("\n")

# Liczymy findings, nie verdict: gdyby model zwrócił "none" z uwagami, uwagi i tak wychodzą.
findings = doc["findings"]
if not findings:
    print("second opinion: nothing to add")
    raise SystemExit(0)
print("second opinion -- %d concern(s). ADVISORY: the gate still decides.\n" % len(findings))
for f in findings:
    print("  [%s] %s:%s  %s" % (f["severity"], f["file"], f["line"], f["claim"]))
    print("      %s" % f["why_it_matters"])
    print("      could not verify: %s\n" % f["what_i_could_not_verify"])
PY
exit 0
