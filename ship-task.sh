#!/usr/bin/env bash
# ship-task.sh — bieg jednego zadania zapisany jako GRAF W KODZIE.
#
#   ./ship-task.sh B1                              # claude pisze, codex recenzuje
#   ./ship-task.sh B1 --agent codex                # codex pisze, claude recenzuje
#   ./ship-task.sh B1 --agent claude --reviewer claude
#
# To jest najważniejszy pomysł strukturalny całego harnessu, i jedyny powód, dla którego
# ten plik nie jest promptem: model, który dostaje sekwencję w promptcie, pomija etap,
# kiedy uzna go za zbędny — a pomija najchętniej ten, który by go zdemaskował. W repo
# źródłowym „dowiedź, że kryteria są czerwone" mieszkało w SKILL.md i bywało pominięte.
# Tutaj pominięcie etapu wymaga edycji tego pliku, czego zabrania .claude/settings.json.
#
# Etapy:
#   worktree → TASK.md jako PIERWSZY commit gałęzi → verify.sh before (musi być czerwono
#   z właściwego powodu) → pisarz → verify.sh full → druga opinia → najwyżej JEDNA
#   naprawa → verify.sh full → komenda integrate.sh dla człowieka.
#
# Kod wyjścia = kod bramki: 0 zielono · 1 sprawdzenie padło · 2 harness źle skonfigurowany
# · 3 przerwane / limit czasu. Nigdy nie mieszamy 1 z 2.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
cd "$ROOT"

# Przerwanie to 3, nie 130. Orkiestrator czyta kod wyjścia i „przerwane" musi dać się
# odróżnić od „sprawdzenie padło" bez czytania logu.
trap 'printf "\ninterrupted\n" >&2; exit 3' INT TERM

# ---------------------------------------------------------------- argumenty --
usage() {
  cat >&2 <<'U'
usage: ship-task.sh <task-id> [--agent claude|codex] [--reviewer claude|codex]
       vendors are independent; the default is cross-vendor (decision D3)
U
}

ID=""; AGENT=""; REVIEWER=""
while [ $# -gt 0 ]; do
  case "$1" in
    --agent)    [ $# -ge 2 ] || { echo "--agent needs a value" >&2; exit 2; };    AGENT="$2";    shift 2 ;;
    --reviewer) [ $# -ge 2 ] || { echo "--reviewer needs a value" >&2; exit 2; }; REVIEWER="$2"; shift 2 ;;
    -h|--help)  usage; exit 0 ;;
    -*)         echo "unknown flag: $1" >&2; usage; exit 2 ;;
    *)          [ -z "$ID" ] || { echo "unexpected argument: $1" >&2; usage; exit 2; }
                ID="$1"; shift ;;
  esac
done
[ -n "$ID" ] || { usage; exit 2; }

AGENT="${AGENT:-claude}"
case "$AGENT" in claude|codex) ;; *) echo "--agent must be claude or codex" >&2; exit 2 ;; esac
# Domyślnie druga opinia od DRUGIEGO vendora (D3). Powód nie jest estetyczny: w repo
# źródłowym każdy realny defekt na ZIELONEJ bramce znalazł recenzent innego vendora.
if [ -z "$REVIEWER" ]; then
  case "$AGENT" in claude) REVIEWER=codex ;; codex) REVIEWER=claude ;; esac
fi
case "$REVIEWER" in claude|codex) ;; *) echo "--reviewer must be claude or codex" >&2; exit 2 ;; esac

# Kto myśli, jest częścią biegu. Prefiks LOADOUT_, a nie CLAUDE_, bo Claude Code eksportuje
# własne CLAUDE_* do każdej powłoki, którą odpala: bieg wystartowany z wnętrza sesji po cichu
# dziedziczył jej effort i ignorował wartość z repo. Stan maszyny nie może wygrywać ze stanem
# repo tylko dlatego, że zajął tę samą nazwę.
export LOADOUT_CLAUDE_MODEL="${LOADOUT_CLAUDE_MODEL:-claude-opus-5[1m]}"
export LOADOUT_CLAUDE_EFFORT="${LOADOUT_CLAUDE_EFFORT:-max}"
export LOADOUT_CODEX_MODEL="${LOADOUT_CODEX_MODEL:-gpt-5.6-sol}"
export LOADOUT_CODEX_EFFORT="${LOADOUT_CODEX_EFFORT:-xhigh}"

# ------------------------------------------------------- warunki wstępne (2) --
have() { command -v "$1" >/dev/null 2>&1; }

have git     || { echo "ship-task.sh needs git on PATH." >&2; exit 2; }
have python3 || { echo "ship-task.sh needs python3 on PATH (the gate is python)." >&2; exit 2; }
[ -f ./verify.sh ]   || { echo "verify.sh is missing — the gate IS the run." >&2; exit 2; }
[ -f ./worktree.sh ] || { echo "worktree.sh is missing." >&2; exit 2; }
# Wszystkie skrypty harnessu wołamy przez `bash <plik>`, nie `./<plik>`. Powód jest nudny
# i kosztował już jeden bieg: bit wykonywalności ginie przy kopiowaniu, przy checkoucie na
# systemie bez uprawnień POSIX i przy rozpakowaniu z archiwum — a „Permission denied" na
# skrypcie bramki wygląda w logu identycznie jak bramka, która coś odrzuciła.

# Brak PISARZA to nasza konfiguracja → 2. Brak RECENZENTA to fakt o świecie → notatka i
# jedziemy dalej (D3: niedostępny recenzent nigdy nie jest czerwony).
have "$AGENT" || {
  echo "the writer '$AGENT' is not installed — that is our configuration, not a red gate." >&2
  exit 2
}
REVIEW_AVAILABLE=1
if ! have "$REVIEWER"; then REVIEW_AVAILABLE=0; fi

TASK_FILE="tasks/$ID.md"
if [ ! -f "$TASK_FILE" ]; then
  echo "no such task: $TASK_FILE" >&2
  ls tasks/ >&2 2>/dev/null || echo "(tasks/ is empty)" >&2
  exit 2
fi

# Transkrypty do runs/<id>/ w GŁÓWNYM repo. Nigdy $TMPDIR: na maszynie źródłowej każdy
# katalog ship-<id> w $TMPDIR poza jednym został już wyczyszczony przez system — przeżyły
# wyłącznie paragony leżące w repo. A w głównym repo, nie w worktree, bo plik nieśledzony
# w worktree czyta się dla checks/quick-scope.sh jako zapis poza dozwolonym drzewem, czyli
# bieg brudziłby drzewo, które sam każe ocenić. /runs/* jest w .gitignore.
RUNDIR="$ROOT/runs/$ID"
mkdir -p "$RUNDIR"
LOG="$RUNDIR/ship.log"

say() {
  printf '\n\033[1m── %s\033[0m\n' "$*"
  printf '\n== %s\n' "$*" >> "$LOG"
}
note() {
  printf '   %s\n' "$*"
  printf '   %s\n' "$*" >> "$LOG"
}

say "task $ID — $AGENT writes, $REVIEWER gives the second opinion"
note "claude: $LOADOUT_CLAUDE_MODEL (effort $LOADOUT_CLAUDE_EFFORT) · codex: $LOADOUT_CODEX_MODEL (effort $LOADOUT_CODEX_EFFORT)"
note "transcripts: $RUNDIR"
[ "$REVIEW_AVAILABLE" = 1 ] || note "note: '$REVIEWER' is not installed — the review stage will be skipped, not failed"

# ------------------------------------------------------------ 1. przestrzeń --
BRANCH="task-$ID"
say "workspace"
# Ścieżki NIE zgadujemy. worktree.sh sam decyduje o nazwie katalogu (m.in. zamienia ją na
# małe litery) i echo tej ścieżki jest całym jego interfejsem — druga kopia tej reguły tutaj
# rozjeżdżała się z pierwszą przy każdej zmianie nazewnictwa.
WT="$(bash ./worktree.sh "$BRANCH" | tail -1)" || {
  echo "could not cut a workspace for $BRANCH" >&2; exit 2; }
[ -d "$WT" ] || { echo "worktree.sh printed '$WT', which is not a directory" >&2; exit 2; }
note "$WT"

# worktree.sh świadomie PONOWNIE UŻYWA istniejącej przestrzeni. Dla niego to zaleta; dla
# tego biegu to koniec: przestrzeń z poprzedniego podejścia ma już implementację, więc
# „before" byłoby zielone i nikt by się nie dowiedział, że kryterium niczego nie sprawdza.
if [ -f "$WT/TASK.md" ]; then
  echo >&2
  echo "$WT already carries a TASK.md, so ship-task.sh has run for $BRANCH before." >&2
  echo "A second run there cannot prove the criteria red. Finish or discard it first:" >&2
  echo "  git worktree remove '$WT' && git branch -D '$BRANCH'" >&2
  exit 2
fi

# Podpięty worktree trzyma metadane gita w GŁÓWNYM .git/worktrees/<nazwa>, czyli POZA
# katalogiem, który przepuszcza `-C` w piaskownicy codeksa. Zmierzone w repo źródłowym:
# każdy `git commit` w biegu codeksa umierał na
#     fatal: Unable to create '<root>/.git/worktrees/<n>/index.lock': Operation not permitted
# — model napisał sześć specyfikacji, nie zacommitował ani jednej i stanął. Odmowa
# środowiska nie do odróżnienia od poddania się modelu, jeśli nie czyta się logu.
GIT_COMMON="$(cd "$WT" && cd "$(git rev-parse --git-common-dir)" && pwd -P)"

cp "$TASK_FILE" "$WT/TASK.md"
git -C "$WT" add TASK.md
git -C "$WT" commit -q -m "docs(task): $ID — the contract this branch is judged against"
note "TASK.md committed as the branch's first commit"

# ------------------------------------------------------------ narzędzia biegu --
gate() {                       # gate <tier>  → kod bramki
  local rc=0
  ( cd "$WT" && bash ./verify.sh "$1" ) || rc=$?
  return "$rc"
}

# Praca, która istnieje, ale nie jest zacommitowana, jest niewidoczna dla integrate.sh
# i dla paragonu (pole `dirty`). Domykamy ją głośno, zamiast ją zgubić — a jeśli model
# napisał coś poza swoim drzewem, ten commit sprawia, że sprawdzenie zakresu to ZOBACZY,
# zamiast żeby zniknęło razem z worktree.
commit_leftovers() {           # commit_leftovers <etykieta>
  if [ -n "$(git -C "$WT" status --porcelain)" ]; then
    note "the $1 phase left uncommitted work — committing it so the gate can see it"
    git -C "$WT" add -A
    git -C "$WT" commit -q -m "chore(run): uncommitted work from the $1 phase"
  fi
}

# Prompt idzie STDIN-em, nigdy w argv (niezmiennik 9): argv widzi każdy `ps`, a prompt
# niesie treść zadania i bywa, że ścieżki. Oba CLI to obsługują — claude -p czyta prompt
# ze stdin, codex exec czyta go po `-`.
write_with() {                 # write_with <transkrypt> <max-turns>  < prompt
  local out="$1" turns="$2" rc=0
  case "$AGENT" in
    claude)
      # --setting-sources project, NIE "". Flaga "" tnie koszt kontekstu ~6x, ale wycina
      # też .claude/settings.json — czyli NASZ hak Stop i NASZĄ listę permissions. Bieg
      # bez nagłówka sesji nie ma kto zatwierdzić, więc „nie zabronione" znaczy w praktyce
      # „zablokowane na zawsze": w repo źródłowym 28 tur i 4,65 $ na zbudowanie niczego.
      # `project` ładuje wyłącznie ustawienia z repo — user/local zostają za drzwiami, więc
      # reprodukowalność zostaje, a znika tylko śmieć z maszyny operatora.
      local mcp=()
      if [ -f "$WT/.mcp.json" ]; then mcp=(--mcp-config .mcp.json); fi
      # bash 3.2 (macOS) przewraca się na "${a[@]}" przy pustej tablicy pod set -u.
      ( cd "$WT" && claude -p \
          --output-format stream-json --verbose \
          ${mcp[@]+"${mcp[@]}"} --strict-mcp-config --setting-sources project \
          --disable-slash-commands \
          --permission-mode acceptEdits \
          --model "$LOADOUT_CLAUDE_MODEL" --effort "$LOADOUT_CLAUDE_EFFORT" \
          --max-turns "$turns" ) > "$out" 2>&1 || rc=$?
      ;;
    codex)
      # -s workspace-write, nie danger-full-access. Repo źródłowe eskalowało piaskownicę
      # wyłącznie dlatego, że Chromium nie startuje pod workspace-write (macOS odmawia
      # rejestracji portu Macha) — a Loadout nie ma sprawdzenia przeglądarkowego w bramce
      # (00-SYNTHESIS §4.2, „Nx/Angular/Playwright — dropped"). Ten powód tu nie istnieje,
      # więc granica zostaje.
      #
      # Brak --ignore-user-config jest świadomy: flaga wycina też trust_level z konfiguracji
      # operatora i w repo źródłowym pierwszy bieg pod nią stracił worker builda i stanął
      # bez implementacji. Recenzent jej potrzebuje, pisarz nie.
      ( cd "$WT" && codex exec --json --skip-git-repo-check -C "$WT" \
          -s workspace-write \
          -c "sandbox_workspace_write.writable_roots=[\"$GIT_COMMON\"]" \
          -m "$LOADOUT_CODEX_MODEL" -c "model_reasoning_effort=$LOADOUT_CODEX_EFFORT" \
          - ) > "$out" 2>&1 || rc=$?
      ;;
  esac
  return "$rc"
}

# ------------------------------------------------- 2. bramka „before", część 1 --
# Odpalona ZANIM cokolwiek kosztuje pieniądze — bo to jedyny moment, w którym wyłapie
# zadanie bez kontraktu. Kod 2 („no acceptance criteria" / „no checks discovered") znaczy,
# że nie ma czego pilnować, i to jest nasz błąd, nie modelu: stajemy tutaj.
say "before (pre-flight)"
PRE=0; gate before || PRE=$?
CONTRACT_READY=0
case "$PRE" in
  0) CONTRACT_READY=1
     note "the criteria are already red for the right reason — the oracle is certified" ;;
  2) echo >&2
     echo "the gate says this task has no contract (exit 2). Nothing to prove, nothing to" >&2
     echo "spend a model's budget on. Fix tasks/$ID.md, not the harness." >&2
     exit 2 ;;
  *) note "the criteria are not provable yet (exit $PRE) — the contract phase must fix that" ;;
esac

# ------------------------------------------------------- 3. faza kontraktowa --
# Osobne wywołanie pisarza, które pisze WYŁĄCZNIE pliki wskazane przez `check:`.
#
# DLACZEGO osobne: „before musi być czerwone" da się wyegzekwować tylko wtedy, gdy istnieją
# pliki, które kryterium wskazuje, a nie istnieje jeszcze implementacja. Jedno wywołanie
# pisarza nie daje harnessowi żadnego momentu, w którym ten stan da się sprawdzić — zostaje
# prośba w promptcie, czyli dokładnie to, czego ten plik ma nie robić. Dwa wywołania
# kosztują jedno uruchomienie modelu i zamieniają prośbę w bramkę.
if [ "$CONTRACT_READY" = 0 ]; then
  say "contract — $AGENT writes the acceptance specs and the skeleton that makes them fail"
  write_with "$RUNDIR/contract.jsonl" 80 <<PROMPT || note "the contract phase exited nonzero; the gate decides what that was worth"
Read AGENTS.md and TASK.md in this directory.

Write the files named by the \`check:\` lines under the \`## AC-n\` headings — the acceptance
specs — plus the SMALLEST SKELETON that lets each spec run and fail at runtime.

Two kinds of stub, and the difference is the whole point:

  FORBIDDEN — a stub that makes a criterion PASS. Returning the expected value, asserting
  something weaker, hard-coding the answer. That is the failure this phase exists to prevent.

  REQUIRED — the skeleton that lets the spec COMPILE and then FAIL. Function signatures with
  \`todo!()\` bodies, the module declarations that reach them, and in Rust the crate root
  itself. A test under \`src-tauri/tests/\` links against the library crate: without
  \`src-tauri/src/lib.rs\` cargo cannot even load the manifest, and the criterion proves
  nothing. \`todo!()\` is transient — the implementation phase replaces it, and
  \`clippy::todo = deny\` in Cargo.toml makes sure none survives to the full gate.

Every file you create must be inside this task's \`<!-- OWNS -->\` block. If a skeleton would
need a path you do not own, stop and say so — that is a finding for a human (AGENTS.md §7).

Each spec must fail because the BEHAVIOUR is missing, not because the file cannot load.
A spec that fails with "module not found", "command not found", "no test files found" or
"N skipped (N)" proves nothing, and \`./verify.sh before\` refuses it by name. If a
criterion cannot be given such a spec, say so in your final message and leave it out —
that is a finding for a human, not a file to invent.

If you need scratch space outside your own files — a throwaway project, a probe directory —
use \`.loadout/scratch/\` inside this worktree. Paths outside the worktree are refused by the
sandbox unpredictably: measured on S-1, \`mkdir /tmp/s1-only-two\` was blocked in one phase and
\`/tmp/s1-run\` succeeded in another. Inside the worktree it always works.

One shell command per Bash call. Never chain with \`;\` or \`&&\`: Claude Code splits a compound
command and asks approval for each part, and in an unattended run there is nobody to give it —
every chained command is a lost turn. Measured: 7 lost turns in one phase.

Never edit TASK.md, verify.sh, harness/, checks/ or tasks/. Commit what you write with
one conventional-commit subject, e.g. "test(<scope>): acceptance specs for $ID".
PROMPT
  commit_leftovers contract

  # I dopiero teraz bramka „before" jest egzekwowalna. To jest ten jeden warunek, którego
  # bieg nie może obejść: jeżeli kryteria nie są czerwone z właściwego powodu, dalej nie ma
  # po co iść — implementacja przeciwko sprawdzeniu, które nic nie sprawdza, jest droższa
  # niż jej brak, bo zostawia zielone, któremu ktoś uwierzy.
  say "before (enforced)"
  RED=0; gate before || RED=$?
  if [ "$RED" -ne 0 ]; then
    echo >&2
    case "$RED" in
      1) echo "the criteria are NOT red for the right reason. Either a criterion already" >&2
         echo "passes (it checks nothing), or its check failed without running at all." >&2
         echo "Read the reasons above; both are contract bugs, not implementation bugs." >&2 ;;
      2) echo "the gate is misconfigured (exit 2). That is ours to fix, not the model's." >&2 ;;
      3) echo "the before tier hit its ceiling (exit 3). A check that cannot finish cannot" >&2
         echo "certify anything." >&2 ;;
    esac
    echo >&2
    echo "workspace kept for inspection: $WT" >&2
    exit "$RED"
  fi
  note "red for the right reason — the oracle is certified"
fi

# --------------------------------------------------------------- 4. pisarz --
say "implementing with $AGENT"
write_with "$RUNDIR/build.jsonl" 250 <<PROMPT || note "the writer exited nonzero; the gate decides what that was worth"
Read AGENTS.md and TASK.md in this directory. The acceptance specs already exist and are
already proven red for the right reason — that is the contract, and it is frozen.

Implement the task. One criterion at a time: implement, run \`./verify.sh quick\` (about
20 seconds), commit that criterion, move on. Run \`./verify.sh full\` before you finish.

Rules that do not bend:

- Never edit TASK.md, verify.sh, harness/, checks/, tasks/ or any config. They are the
  oracle. If a criterion cannot be met without touching one, that is a finding — say it
  in your final message instead of doing it.
- Never weaken or delete an assertion in an existing spec. A spec may gain assertions;
  it may never lose one.
- If a criterion can only be passed in a way you consider cheating, say so plainly and
  change nothing. AGENTS.md §7 names that as the most valuable thing you can report.
- Three attempts at one criterion, then move on and commit what exists, with the commit
  message saying what is still red.

Write under src/, src-tauri/, engine/ and tests/. Commit subjects are
"<type>(<scope>): <what changed>".
PROMPT
commit_leftovers implementation

# ---------------------------------------------------------------- 5. bramka --
say "gate"
GATE=0; gate full || GATE=$?
note "gate: $GATE"

# 2 to NASZA konfiguracja (brakujący prettier, brakujące cargo, defekt kontraktu), a nie
# czerwony kod. Model ma harness zabroniony do edycji, więc runda naprawcza nie ma czego
# naprawić — kosztowałaby dwie tury i skończyła się tą samą dwójką. Stajemy tutaj.
if [ "$GATE" -eq 2 ]; then
  echo >&2
  echo "the gate is MISCONFIGURED (exit 2), not red. That is ours to fix, not the model's:" >&2
  echo "read the reason above (a missing tool, or a contract defect in TASK.md)." >&2
  echo "workspace kept for inspection: $WT" >&2
  exit 2
fi

# ----------------------------------------------------------- 6. druga opinia --
# Doradcza. Nie zatwierdza i nie blokuje — schemat odpowiedzi ma verdict ∈ {concern, none},
# więc strukturalnie nie ma czego zatwierdzić (D3). Recenzent niedostępny to notatka.
REVIEW_OUT="$RUNDIR/review.txt"
: > "$REVIEW_OUT"
if [ "$REVIEW_AVAILABLE" = 1 ] && [ -f "$WT/review.sh" ]; then
  say "second opinion by $REVIEWER"
  RV=0
  # Obie flagi, nie samo --reviewer: review.sh po tej parze poznaje tryb same-vendor i
  # dopiero wtedy daje recenzentowi INNY model (D3). Bez --agent para codex/codex wygląda
  # dla niego jak cross-vendor i recenzent dostaje ten sam model, co pisarz.
  ( cd "$WT" && bash ./review.sh --agent "$AGENT" --reviewer "$REVIEWER" ) \
    | tee "$REVIEW_OUT" || RV=$?
  # 2 z review.sh znaczy „NASZA konfiguracja jest zepsuta" (np. brak schematu) i tylko to.
  # Rozróżnienie jest tu, bo w repo źródłowym brakujący plik schematu przez dwie sesje
  # meldował się jako „reviewer unavailable" i był diagnozowany jako limit kredytów.
  if [ "$RV" = 2 ]; then
    echo "review.sh reports OUR misconfiguration, not an unavailable reviewer." >&2
    exit 2
  fi
elif [ "$REVIEW_AVAILABLE" = 1 ]; then
  note "review.sh is missing — skipping the second opinion (advisory, never a red)"
else
  note "'$REVIEWER' unavailable — skipping the second opinion (advisory, never a red)"
fi

# ------------------------------------------------------------- 7. naprawa x1 --
# Dokładnie jedna runda. Nieograniczona pętla recenzji to sposób, w jaki jedno zadanie
# zjada cały dzień; po tej rundzie decyduje człowiek.
CONCERNS=0
if grep -q '^second opinion -- ' "$REVIEW_OUT" 2>/dev/null; then CONCERNS=1; fi

if [ "$GATE" -ne 0 ] || [ "$CONCERNS" = 1 ]; then
  if [ -f "$WT/repair.sh" ]; then
    # repair.sh nie bierze id zadania: czyta TASK.md, runs/last.json i runs/review.json
    # z katalogu, w którym stoi. Dlatego uruchamiamy go W WORKTREE — z korzenia naprawiałby
    # trunk przeciwko paragonowi z innej gałęzi.
    say "repair — one round, then stop"
    RP=0
    ( cd "$WT" && bash ./repair.sh --agent "$AGENT" --reviewer "$REVIEWER" ) 2>&1 \
      | tee -a "$RUNDIR/repair.txt" || RP=$?
    if [ "$RP" = 2 ]; then
      echo "repair.sh reports OUR misconfiguration." >&2
      exit 2
    fi
    say "gate, after the repair round"
    GATE=0; gate full || GATE=$?
    note "gate: $GATE"
  else
    note "repair.sh is missing — no repair round was run"
  fi
fi

# ------------------------------------------------------------ 8. co zostaje --
# Paragon przeżywa worktree. To jedyny artefakt, który na maszynie źródłowej przetrwał
# wszystkie biegi, i to on jest naturalnym wejściem dla widoku sesji w samym Loadout.
if [ -f "$WT/runs/last.json" ]; then
  cp "$WT/runs/last.json" "$RUNDIR/gate-final.json"
fi

say "task $ID: gate $([ "$GATE" -eq 0 ] && echo GREEN || echo "RED (exit $GATE)")"
( cd "$WT" && bash ./verify.sh --report ) || true

echo
echo "branch $BRANCH is ready in $WT"
if [ "$GATE" -eq 0 ]; then
  echo "land it with:  ./integrate.sh $BRANCH"
else
  echo "the gate is red after one repair round — AGENTS.md §7 says that is a human's call."
  echo "read $RUNDIR/, then either fix it by hand or change the criterion deliberately."
fi
exit "$GATE"
