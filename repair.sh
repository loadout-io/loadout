#!/usr/bin/env bash
# DOKŁADNIE jedna runda poprawek. Recenzent planuje (tylko do odczytu), pisarz wykonuje plan,
# którego nie napisał. Potem bramka. Potem koniec — dalej decyduje człowiek.
#
#   ./repair.sh                                    # pisze claude, planuje codex
#   ./repair.sh --agent codex --reviewer claude
#
# Podział ról jest tym samym, co czyni recenzję wartą zachodu: kto napisał kod, nie decyduje,
# co jest z nim nie tak. Żaden z dwóch nie sprawdza własnej pracy.
#
# BOUNDED, i to jest cała rzecz. Razem z biegiem budującym i jedną rundą poprawek w
# ship-task.sh to CZTERY tury agenta na jedno zadanie. Piątej nie ma: zadanie, które przeszło
# przez to dwa razy, mówi coś, czego piąta tura nie naprawi — kryterium jest złe albo kontrakt.
set -euo pipefail
set +m

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
cd "$ROOT"

PLAN_BUDGET="${LOADOUT_PLAN_BUDGET:-900}"
EXEC_BUDGET="${LOADOUT_EXEC_BUDGET:-3600}"

CLAUDE_MODEL="${LOADOUT_CLAUDE_MODEL:-claude-opus-5[1m]}"
CODEX_MODEL="${LOADOUT_CODEX_MODEL:-gpt-5.6-sol}"
CLAUDE_REVIEW_MODEL="${LOADOUT_CLAUDE_REVIEW_MODEL:-sonnet}"
CODEX_REVIEW_MODEL="${LOADOUT_CODEX_REVIEW_MODEL:-gpt-5.5}"

usage() { echo "usage: ./repair.sh [--agent claude|codex] [--reviewer claude|codex]" >&2; }
die2()  { printf 'repair.sh is misconfigured: %s\n' "$1" >&2; exit 2; }
say()   { printf '\n\033[1m-- %s\033[0m\n' "$*"; }
other() { case "$1" in claude) echo codex ;; codex) echo claude ;; esac; }

AGENT="${LOADOUT_AGENT:-}"
REVIEWER="${LOADOUT_REVIEWER:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    --agent)    AGENT="${2:-}";    [ -n "$AGENT" ]    || { usage; exit 2; }; shift 2 ;;
    --reviewer) REVIEWER="${2:-}"; [ -n "$REVIEWER" ] || { usage; exit 2; }; shift 2 ;;
    claude|codex) AGENT="$1"; shift ;;
    -h|--help)  usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
done
if [ -z "$AGENT" ]    && [ -n "$REVIEWER" ]; then AGENT="$(other "$REVIEWER")"; fi
if [ -z "$AGENT" ];    then AGENT=claude; fi
if [ -z "$REVIEWER" ]; then REVIEWER="$(other "$AGENT")"; fi
for v in "$AGENT" "$REVIEWER"; do
  case "$v" in claude|codex) ;; *) echo "vendor must be claude or codex, got: $v" >&2; usage; exit 2 ;; esac
done

# ── NASZA konfiguracja: brak = 2 ─────────────────────────────────────────────────────────
# Na prawie pustym drzewie ten skrypt odmawia zamiast przejść cicho: naprawa bez kryteriów
# i bez paragonu bramki naprawiałaby cudzą wyobraźnię.
command -v python3 >/dev/null 2>&1 || die2 "python3 is not on PATH"
command -v git     >/dev/null 2>&1 || die2 "git is not on PATH"
[ -x ./verify.sh ]    || die2 "./verify.sh is missing or not executable -- there is no gate to repair against"
[ -f TASK.md ]        || die2 "there is no TASK.md here"
grep -qE '^##[[:space:]]+[A-Z]{2,8}-[0-9]+' TASK.md \
                      || die2 "TASK.md declares no acceptance criteria, so this can only repair itself"
[ -f runs/last.json ] || die2 "no gate receipt at runs/last.json -- run ./verify.sh full first"
mkdir -p runs

[ -f harness/process-group.sh ] || die2 "harness/process-group.sh is missing"
# shellcheck source=harness/process-group.sh
. harness/process-group.sh

repair_interrupted() {
  local rc=3
  if loadout_agent_group_defer_interrupt; then return; fi
  # Kolejny Ctrl-C nie moze zabic powloki w trakcie laski TERM i odtworzyc sieroty.
  trap '' INT TERM
  loadout_agent_group_stop || rc=2
  if [ "$rc" = 3 ]; then
    printf '\nrepair interrupted; active agent process group is dead\n' >&2
  else
    printf '\nrepair interrupted; process-group death was not proved\n' >&2
  fi
  exit "$rc"
}
trap repair_interrupted INT TERM

repair_finish() {
  local rc=$?
  trap - EXIT
  trap '' INT TERM
  if [ -n "${LOADOUT_AGENT_GROUP_PID:-}" ]; then
    loadout_agent_group_stop || rc=2
  fi
  exit "$rc"
}
trap repair_finish EXIT

FAILED="$(python3 -c "
import json
d = json.load(open('runs/last.json'))
print(' '.join(d.get('failed') or []))
" 2>/dev/null)" || die2 "runs/last.json is not readable JSON -- re-run ./verify.sh full"

CONCERNS=0
if [ -f runs/review.json ]; then
  CONCERNS="$(python3 -c "
import json
print(len(json.load(open('runs/review.json')).get('findings') or []))
" 2>/dev/null || echo 0)"
fi

if [ -z "$FAILED" ] && [ "$CONCERNS" -eq 0 ]; then
  echo "nothing to repair: the gate is green and no second opinion raised a concern."
  exit 0
fi

# Same-vendor: planista MUSI dostać inny model niż wykonawca (D3), plus jawną rolę recenzenta.
SAME=0
if [ "$AGENT" = "$REVIEWER" ]; then SAME=1; fi
if [ "$AGENT" = claude ]; then WRITER_MODEL="$CLAUDE_MODEL"; else WRITER_MODEL="$CODEX_MODEL"; fi
if [ "$REVIEWER" = claude ]; then
  if [ "$SAME" -eq 1 ]; then PLANNER_MODEL="$CLAUDE_REVIEW_MODEL"; else PLANNER_MODEL="$CLAUDE_MODEL"; fi
else
  if [ "$SAME" -eq 1 ]; then PLANNER_MODEL="$CODEX_REVIEW_MODEL"; else PLANNER_MODEL="$CODEX_MODEL"; fi
fi
ROLE=""
if [ "$SAME" -eq 1 ]; then
  echo "repair: planner and writer are both $REVIEWER -- THE WEAKER MODE."
  echo "  The planner runs a different model ($PLANNER_MODEL vs $WRITER_MODEL) and an explicit"
  echo "  planner role, but one vendor's blind spot tends to be the same blind spot twice."
  ROLE="WHO YOU ARE. The same vendor wrote this code. You are running as a different model with
one job: to find what is actually wrong with it. Assume the author was you an hour ago and that
you were wrong.
"
fi

# F-1 z 06 §2: w podpiętym worktree `.git` jest PLIKIEM wskazującym na katalog w repo głównym,
# poza korzeniem -C, który przepuszcza sandbox workspace-write codeksa. Każdy `git commit`
# padał wtedy na "Unable to create index.lock: Operation not permitted", co wygląda dokładnie
# jak model, który się poddał. Grant tylko dla tego jednego katalogu i tylko gdy leży na zewnątrz.
GITDIR="$(cd "$(git rev-parse --git-common-dir 2>/dev/null || echo .)" && pwd -P)"
CODEX_WRITABLE=()
case "$GITDIR" in
  "$ROOT"/*|"$ROOT") ;;
  *) CODEX_WRITABLE=(-c "sandbox_workspace_write.writable_roots=[\"$GITDIR\"]") ;;
esac

# Prompt zawsze STDIN-em (niezmiennik 9). Funkcja kończy się exec-em, więc $! to pid samego
# CLI i stoper zdejmuje agenta, a nie powłokę, która go trzyma.
run_boxed() {                       # run_boxed <budget> <funkcja>
  local budget="$1" fn="$2" rc=0 started=0
  # Wspolny rdzen zapisuje aktywny PGID PRZED wait, zeby pulapka INT/TERM mogla go sprzatnac.
  # Podstawienie procesu, nie potok: prompt nadal idzie wylacznie przez stdin (niezmiennik 9).
  loadout_agent_group_start "$fn" < <(printf '%s\n' "$PROMPT") || started=$?
  if [ "$started" = 3 ]; then repair_interrupted; fi
  [ "$started" = 0 ] || return "$started"
  loadout_agent_group_wait "$budget" || rc=$?
  if [ "$LOADOUT_AGENT_GROUP_PROOF_FAILED" = 1 ]; then
    exit 2
  fi
  return "$rc"
}

# codex oddaje ostatnią wiadomość przez -o, claude nie ma odpowiednika — trzeba ją wyłuskać
# ze strumienia stream-json.
claude_final_text() {               # claude_final_text <plik z stream-json>
  python3 - "$1" <<'PY'
import json, sys
try:
    raw = open(sys.argv[1], encoding="utf-8", errors="replace").read()
except OSError:
    raw = ""
out = None
for line in raw.splitlines():
    line = line.strip()
    if not line.startswith("{"):
        continue
    try:
        ev = json.loads(line)
    except ValueError:
        continue
    # Nigdy po samym subtype: nieudany bieg bywa "success" z is_error=true (T1 §4.4).
    if ev.get("type") == "result" and not ev.get("is_error"):
        out = ev.get("result") or ""
sys.stdout.write(out or "")
PY
}

# ── tura 3 z 4: RECENZENT PLANUJE, tylko do odczytu ──────────────────────────────────────
# Planista, który zaczyna edytować, przestaje być drugą opinią i staje się drugim autorem
# z mniejszym kontekstem.
say "planning with $REVIEWER ($PLANNER_MODEL) — read-only, ${PLAN_BUDGET}s"
REPORT="$(./verify.sh --report 2>&1 || true)"
REVIEW_JSON="(no second opinion recorded)"
if [ -f runs/review.json ]; then REVIEW_JSON="$(cat runs/review.json)"; fi

PROMPT="A task failed its gate, or a second opinion raised concerns about it. Plan the fix.
Do NOT edit any file: you are read-only, and a different model will execute what you write.

$ROLE
Here is what the gate says:

$REPORT

Failing check ids: ${FAILED:-(none — the gate is green)}

Here is the second opinion, verbatim (harness/review-schema.json):

$REVIEW_JSON

Read TASK.md, the tests its criteria name, and the implementation. Then answer three questions
for EACH failing criterion and EACH concern, in plain text, under 400 words in total:

1. Is this a defect in the code, or is the CRITERION wrong? Say which, and how you know. If the
   criterion is wrong, say so plainly -- that is a finding for a human, not a fix, and the next
   step is to stop rather than to make it pass.
2. If it is the code: which file, which function, and the smallest change that makes the
   criterion true. Not a rewrite.
3. What would make the fix WRONG -- the shortcut that would turn it green without meeting the
   criterion. Name it, so the executor does not take it.

Do not restate the report. Do not suggest improvements nobody asked for.
"

PLAN_RAW="runs/repair-plan.raw"; PLAN="runs/repair-plan.txt"; PLAN_LOG="runs/repair-plan.log"
rm -f "$PLAN_RAW" "$PLAN" "$PLAN_LOG"
spawn_planner() {
  if [ "$REVIEWER" = codex ]; then
    exec codex exec --json --skip-git-repo-check -C "$ROOT" -s read-only --ignore-user-config \
      -m "$PLANNER_MODEL" -o "$PLAN" >"$PLAN_LOG" 2>&1
  fi
  exec claude -p --output-format stream-json --verbose \
    --strict-mcp-config --setting-sources "" \
    --model "$PLANNER_MODEL" --permission-mode dontAsk --allowedTools "Read,Grep,Glob" \
    --max-turns "${LOADOUT_PLAN_MAX_TURNS:-40}" >"$PLAN_RAW" 2>"$PLAN_LOG"
}
if command -v "$REVIEWER" >/dev/null 2>&1; then
  run_boxed "$PLAN_BUDGET" spawn_planner || true
  if [ "$REVIEWER" = claude ]; then claude_final_text "$PLAN_RAW" > "$PLAN" || true; fi
fi

if [ ! -s "$PLAN" ]; then
  # Niedostępny planista to fakt o vendorze, nie o kodzie: nie wymyślamy czerwonego i nie
  # zgłaszamy tego jako naszej złej konfiguracji. Zwracamy werdykt, który bramka JUŻ wydała.
  echo "the planner returned nothing -- $REVIEWER is unavailable, so nothing was repaired."
  echo "The gate verdict is unchanged; run ./verify.sh full yourself, or try --reviewer $(other "$REVIEWER")."
  if [ -z "$FAILED" ]; then exit 0; else exit 1; fi
fi
sed 's/^/  /' "$PLAN"

# ── tura 4 z 4: PISARZ WYKONUJE plan, którego nie napisał ────────────────────────────────
say "executing with $AGENT ($WRITER_MODEL) — ${EXEC_BUDGET}s"
if ! command -v "$AGENT" >/dev/null 2>&1; then
  # Symetrycznie do planisty: niedostępny vendor to fakt o maszynie, nie o naszej konfiguracji
  # i nie o kodzie. Zwracamy werdykt, który bramka JUŻ wydała — plan zostaje na dysku.
  echo "$AGENT is not installed, so the plan in $PLAN was not executed. Nothing changed."
  if [ -z "$FAILED" ]; then exit 0; else exit 1; fi
fi

PROMPT="A second opinion has planned a repair for this task. Execute it. You did not write this
plan, and that is the point: whoever wrote the code does not decide what is wrong with it.

$(cat "$PLAN")

Rules that do not bend:
- Write only under the paths TASK.md's OWNS block names, plus src/ and src-tauri/src/.
- Never edit ./verify.sh, harness/, checks/, tasks/, TASK.md, docs/ or any config file. They are
  the oracle. If a criterion cannot be met without touching one, that is a finding to report,
  not a fix to make.
- Never remove or weaken an assertion in an existing test. A test may gain assertions, never
  lose them.
- If the plan says the CRITERION is wrong rather than the code, do not make it pass. Say so in
  your final message and change nothing. That is the correct outcome, and it is worth more than
  a green gate.
- One commit per fix, subject '<type>(<scope>): <what changed>'. Then run ./verify.sh full.
"

EXEC_RAW="runs/repair-exec.raw"; EXEC_LOG="runs/repair-exec.log"; EXEC_MSG="runs/repair-exec.txt"
rm -f "$EXEC_RAW" "$EXEC_LOG" "$EXEC_MSG"
spawn_writer() {
  if [ "$AGENT" = codex ]; then
    exec codex exec --json --skip-git-repo-check -C "$ROOT" -s workspace-write \
      "${CODEX_WRITABLE[@]+"${CODEX_WRITABLE[@]}"}" -m "$WRITER_MODEL" \
      -o "$EXEC_MSG" >"$EXEC_RAW" 2>&1
  fi
  # --setting-sources project, NIE "" (N-02, audyt 2026-08-15). Tu stało "", z uzasadnieniem,
  # że broni checks/quick-scope.sh — a N-06 pokazał, że to sprawdzenie było ślepe po commicie.
  # Efekt: runda uruchamiana PO tym, jak recenzent powiedział, że coś jest nie tak, była rundą
  # z najmniejszą liczbą zabezpieczeń: bez Write(harness/**), bez Write(TASK.md), bez haka Stop.
  # Najmniej ograniczony pisarz i najbardziej ślepe sprawdzenie, w tym samym miejscu.
  # ship-task.sh:181 opisuje ten sam błąd i go nie popełnia; ten plik go popełniał.
  exec claude -p --output-format stream-json --verbose \
    --strict-mcp-config --setting-sources project \
    --model "$WRITER_MODEL" --permission-mode acceptEdits \
    --allowedTools "Read,Grep,Glob,Edit,Write,Bash" \
    --max-turns "${LOADOUT_EXEC_MAX_TURNS:-120}" >"$EXEC_RAW" 2>"$EXEC_LOG"
}
run_boxed "$EXEC_BUDGET" spawn_writer || true
# Ostatnia wiadomość pisarza jest tym miejscem, w którym pada "kryterium jest złe, nic nie
# zmieniłem" — dlatego wychodzi na ekran, a nie tylko do logu.
if [ "$AGENT" = claude ]; then claude_final_text "$EXEC_RAW" > "$EXEC_MSG" || true; fi
tail -n 40 "$EXEC_MSG" 2>/dev/null || echo "(the writer left no final message -- see $EXEC_RAW)"

# ── bramka decyduje. Nie model, nie recenzent. ───────────────────────────────────────────
say "gate"
GATE=0
./verify.sh full || GATE=$?

say "repair: gate $([ "$GATE" -eq 0 ] && echo GREEN || echo RED)"
./verify.sh --report || true
echo
if [ "$GATE" -eq 0 ]; then
  echo "Green. A human still owes this one read: the gate cannot tell whether the repair"
  echo "answered the second opinion, ignored it, or wrote a new weak assertion in its place."
else
  echo "That is four agent turns on this task: the build, one repair round, this plan and this"
  echo "execution. There is no fifth. Read $PLAN: if it says the CRITERION is wrong rather than"
  echo "the code, that is a finding for a human, and TASK.md is not yours to change."
fi
exit "$GATE"
