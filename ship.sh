#!/usr/bin/env bash
# ship.sh — jeden bieg: PROMPT -> plan -> implementacja -> bramka zadania -> naprawa -> koniec.
#
#   ./ship.sh "dodaj przycisk Cancel do wiersza biegu"
#   ./ship.sh -f prompt.md --agent codex
#   ./ship.sh --review "..."          # druga opinia PO zielonym, jako raport, bez auto-naprawy
#   ./ship.sh --dry-run "..."         # wypisz, co by zrobil, i wyjdz
#
# CO ZASTAPILO I DLACZEGO (audyt 2026-08-28, liczby z runs/ i runs/build-loop.tsv).
#
# ship-task.sh bral ID zadania z katalogu tasks/ i biegl w osmiu etapach: kontrakt, naprawa
# kontraktu, implementacja, pelna bramka, druga opinia, naprawa, pelna bramka. Zmierzone na
# 121 biegach:
#
#   * 4,0 wywolania modelu na bieg i 4-5 przebiegow bramki, z tego DWA razy `full`;
#   * `full` to 319 s, z czego 280 s (88%) to suita CALEGO repo, a wszystkie czternascie
#     tanich sprawdzen razem to 9,6 s. Zadanie o trzech kryteriach placilo 640 s za
#     przebudowanie rzeczy, ktorych nikt nie tykal;
#   * druga opinia byla "doradcza" tylko na papierze: 97 recenzji na 105 zwrocilo uwage,
#     a warunek naprawy brzmial "bramka czerwona LUB jest uwaga", wiec runda naprawcza
#     odpalila sie w 98 biegach na 121 (81%) -- i regularnie trwala DLUZEJ niz implementacja
#     (T-103: 2 min implementacji, 45 min naprawy; T-119: 11 i 28; T-120: 9 i 20);
#   * pliki zadan to 26 617 linii, ktore czlowiek pisal RECZNIE przed biegiem.
#
# Wiec: kontrakt pisze etap planu (w worktree, dla jednego biegu), bramka biegu to `task`
# (16 s zamiast 319), naprawe prowadzi PARAGON a nie recenzent, a suita calego repo biegnie
# raz przy ladowaniu (integrate.sh) i w CI -- tam, gdzie jest o co pytac.
#
# CO ZOSTALO NIETKNIETE, bo to jest jedyny powod, dla ktorego zielone cokolwiek znaczy:
# graf biegu jest W KODZIE (model, ktory dostaje sekwencje w promptcie, pomija etap, kiedy
# uzna go za zbedny), `before` musi byc czerwone z wlasciwego powodu, specyfikacja moze
# zyskac asercje i nie moze zadnej stracic, a prompt idzie STDIN-em (niezmiennik 9).
#
# Kod wyjscia = kod bramki: 0 zielono · 1 sprawdzenie padlo · 2 harness zle skonfigurowany
# · 3 przerwane / limit czasu. Nigdy nie mieszamy 1 z 2.
set -euo pipefail

# Bash czyta ten plik PRZYROSTOWO, po offsetach bajtowych. Edycja w trakcie biegu przesuwa
# wszystko za kursorem i proces wykonuje smieci -- skladniowo poprawne, semantycznie losowe.
# Zdarzylo sie trzy razy 2026-08-15. Kopia jest niezmienna, wiec orchestrator moze naprawiac
# harness, kiedy petla chodzi. ROOT liczony PRZED exec: w kopii $0 wskazuje na mktemp.
# Nazwa sentinela jest WLASNA dla tego skryptu -- wspolna wyciekala przez srodowisko do
# dziecka, ktore pomijalo wlasne przypiecie i bralo katalog rodzica za korzen repo.
if [ -z "${LOADOUT_PINNED_SHIP:-}" ]; then
  LOADOUT_SELF_SHIP="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
  LOADOUT_SNAP="$(mktemp -t ship)"
  cat "${BASH_SOURCE[0]}" > "$LOADOUT_SNAP"
  export LOADOUT_PINNED_SHIP=1 LOADOUT_SELF_SHIP
  exec bash "$LOADOUT_SNAP" "$@"
fi

SELF_DIR="${LOADOUT_SELF_SHIP:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)}"
unset LOADOUT_PINNED_SHIP LOADOUT_SELF_SHIP

ROOT="$SELF_DIR"
cd "$ROOT"

# Jedyna polityka zycia procesow agenta. Zrodlowana TU, przed pulapka, bo pulapka jej uzywa.
[ -f "$ROOT/harness/process-group.sh" ] || {
  echo "harness/process-group.sh is missing — there is no policy for killing an agent." >&2
  exit 2
}
# shellcheck source=harness/process-group.sh
. "$ROOT/harness/process-group.sh"

# Przerwanie to 3, nie 130: "przerwane" musi dac sie odroznic od "sprawdzenie padlo"
# bez czytania logu.
#
# I nie wystarczy wyjsc. 2026-08-26: Ctrl-C zakonczyl repair.sh, ale pisarz Codeksa pracowal
# we wlasnym PGID, zostal adoptowany przez PID 1 i zdazyl ZACOMMITOWAC po tym, jak harness
# oddal kod 3. Powrot wymaga wiec dowodu ESRCH (niezmiennik 6), a brak dowodu to kod 2:
# "nie umiem udowodnic, ze nie zyje" to inna wiadomosc niz "przerwane".
ship_interrupted() {
  if loadout_agent_group_defer_interrupt; then return; fi
  trap '' INT TERM
  local rc=3
  loadout_agent_group_stop || rc=2
  if [ "${LOADOUT_AGENT_GROUP_PROOF_FAILED:-0}" = 1 ]; then
    printf '\ninterrupted; process-group death was not proved\n' >&2
  else
    printf '\ninterrupted\n' >&2
  fi
  exit "$rc"
}
trap ship_interrupted INT TERM

# ---------------------------------------------------------------- argumenty --
usage() {
  cat >&2 <<'U'
usage: ship.sh [options] "<co ma zrobic>"
       ship.sh [options] -f <plik z promptem>

  --agent claude|codex      kto pisze (domyslnie claude)
  --reviewer claude|codex   kto daje druga opinie, jesli o nia poprosisz
  --review                  dodaj druga opinie PO zielonym; to raport, nie brama
  --rounds N                ile rund naprawczych po bramce zadania (domyslnie 2)
  --name <slug>             nazwa galezi i katalogu paragonow; domyslnie z promptu
  --dry-run                 wypisz plan biegu i wyjdz
U
}

AGENT=""; REVIEWER=""; REVIEW=0; ROUNDS=2; NAME=""; DRY=0; PROMPT_FILE=""; REQUEST=""
while [ $# -gt 0 ]; do
  case "$1" in
    --agent)    [ $# -ge 2 ] || { echo "--agent needs a value" >&2; exit 2; };    AGENT="$2";    shift 2 ;;
    --reviewer) [ $# -ge 2 ] || { echo "--reviewer needs a value" >&2; exit 2; }; REVIEWER="$2"; shift 2 ;;
    --rounds)   [ $# -ge 2 ] || { echo "--rounds needs a value" >&2; exit 2; };   ROUNDS="$2";   shift 2 ;;
    --name)     [ $# -ge 2 ] || { echo "--name needs a value" >&2; exit 2; };     NAME="$2";     shift 2 ;;
    -f|--file)  [ $# -ge 2 ] || { echo "-f needs a path" >&2; exit 2; };          PROMPT_FILE="$2"; shift 2 ;;
    --review)   REVIEW=1; shift ;;
    --dry-run)  DRY=1; shift ;;
    -h|--help)  usage; exit 0 ;;
    -*)         echo "unknown flag: $1" >&2; usage; exit 2 ;;
    *)          [ -z "$REQUEST" ] || { echo "unexpected argument: $1" >&2; usage; exit 2; }
                REQUEST="$1"; shift ;;
  esac
done

case "$ROUNDS" in ''|*[!0-9]*) echo "--rounds must be a number" >&2; exit 2 ;; esac

if [ -n "$PROMPT_FILE" ]; then
  [ -z "$REQUEST" ] || { echo "give a prompt OR -f <plik>, not both" >&2; exit 2; }
  [ -f "$PROMPT_FILE" ] || { echo "no such prompt file: $PROMPT_FILE" >&2; exit 2; }
  REQUEST="$(cat "$PROMPT_FILE")"
fi
[ -n "$REQUEST" ] || { usage; exit 2; }

AGENT="${AGENT:-claude}"
case "$AGENT" in claude|codex) ;; *) echo "--agent must be claude or codex" >&2; exit 2 ;; esac
# Domyslnie druga opinia od DRUGIEGO vendora (D3): w repo zrodlowym kazdy realny defekt na
# ZIELONEJ bramce znalazl recenzent innego vendora. Od 2026-08-28 nie biegnie bez --review.
if [ -z "$REVIEWER" ]; then
  case "$AGENT" in claude) REVIEWER=codex ;; codex) REVIEWER=claude ;; esac
fi
case "$REVIEWER" in claude|codex) ;; *) echo "--reviewer must be claude or codex" >&2; exit 2 ;; esac

# Kto mysli, jest czescia biegu. Prefiks LOADOUT_, a nie CLAUDE_, bo Claude Code eksportuje
# wlasne CLAUDE_* do kazdej powloki, ktora odpala: bieg wystartowany z wnetrza sesji po cichu
# dziedziczyl jej effort i ignorowal wartosc z repo.
export LOADOUT_CLAUDE_MODEL="${LOADOUT_CLAUDE_MODEL:-claude-opus-5[1m]}"
export LOADOUT_CLAUDE_EFFORT="${LOADOUT_CLAUDE_EFFORT:-max}"
export LOADOUT_CODEX_MODEL="${LOADOUT_CODEX_MODEL:-gpt-5.6-sol}"
export LOADOUT_CODEX_EFFORT="${LOADOUT_CODEX_EFFORT:-xhigh}"

# ------------------------------------------------------- warunki wstepne (2) --
have() { command -v "$1" >/dev/null 2>&1; }

have git     || { echo "ship.sh needs git on PATH." >&2; exit 2; }
have python3 || { echo "ship.sh needs python3 on PATH (the gate is python)." >&2; exit 2; }
# Te dwa sprawdzenia sa TAKZE testem przypiecia: w kopii z $TMPDIR ROOT bylby katalogiem
# tymczasowym, a wtedy verify.sh nie istnieje. scripts/ci.sh probuje dokladnie tego.
[ -f ./verify.sh ]   || { echo "verify.sh is missing — the gate IS the run." >&2; exit 2; }
[ -f ./worktree.sh ] || { echo "worktree.sh is missing." >&2; exit 2; }

# Brak PISARZA to nasza konfiguracja -> 2. Brak RECENZENTA to fakt o swiecie -> notatka
# i jedziemy dalej (D3: niedostepny recenzent nigdy nie jest czerwony).
have "$AGENT" || {
  echo "the writer '$AGENT' is not installed — that is our configuration, not a red gate." >&2
  exit 2
}
REVIEW_AVAILABLE=1
if ! have "$REVIEWER"; then REVIEW_AVAILABLE=0; fi

# --------------------------------------------------------- nazwa tego biegu --
# Slug z promptu, zeby galaz i katalog paragonow dalo sie rozpoznac po nazwie tygodnie
# pozniej. Znacznik czasu z przodu, bo dwa biegi tego samego dnia o tym samym prompcie sa
# normalne, a kolizja nazw galezi kosztowalaby cicho wznowienie CUDZEJ przestrzeni.
if [ -z "$NAME" ]; then
  NAME="$(printf '%s' "$REQUEST" | tr 'A-Z' 'a-z' \
          | tr -c 'a-z0-9' '-' | tr -s '-' | cut -c1-28 | sed 's/^-//; s/-$//')"
fi
[ -n "$NAME" ] || NAME="run"
ID="$(date +%m%d-%H%M)-$NAME"
BRANCH="run-$ID"

# Transkrypty do runs/<id>/ w GLOWNYM repo. Nigdy $TMPDIR: na maszynie zrodlowej kazdy
# katalog w $TMPDIR poza jednym zostal wyczyszczony przez system -- przezyly wylacznie
# paragony lezace w repo. A w glownym repo, nie w worktree, bo plik niesledzony w worktree
# czyta sie dla checks/quick-scope.sh jako zapis poza dozwolonym drzewem.
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

if [ "$DRY" = 1 ]; then
  printf 'root:     %s\n' "$ROOT"
  printf 'branch:   %s\n' "$BRANCH"
  printf 'receipts: %s\n' "$RUNDIR"
  printf 'writer:   %s · reviewer: %s (%s)\n' "$AGENT" "$REVIEWER" \
    "$([ "$REVIEW" = 1 ] && echo "asked for" || echo "off; pass --review")"
  printf 'rounds:   %s\n' "$ROUNDS"
  printf 'request:  %s\n' "$(printf '%s' "$REQUEST" | head -1 | cut -c1-70)"
  exit 0
fi

# ------------------------------------------------------------ narzedzia biegu --
gate() {                       # gate <tier>  -> kod bramki
  local rc=0
  ( cd "$WT" && bash ./verify.sh "$1" ) || rc=$?
  return "$rc"
}

# Powody Z PARAGONU, nie z domyslu. Model, ktory zna nazwe swojego ksztaltu czerwieni,
# nie zgaduje -- a to jest cala roznica miedzy runda naprawcza i druga runda implementacji.
gate_reasons() {
  python3 - "$WT/runs/last.json" <<'REASONS'
import json, sys
try:
    receipt = json.load(open(sys.argv[1], encoding="utf-8"))
except Exception:
    sys.exit(0)
for c in receipt.get("checks", []):
    if not c.get("ok"):
        print("  %s -- %s" % (c["id"], (c.get("reason") or "").replace("\n", " ")[:400]))
REASONS
}

# Praca, ktora istnieje, ale nie jest zacommitowana, jest niewidoczna dla integrate.sh
# i dla paragonu (pole `dirty`). Domykamy ja glosno, zamiast ja zgubic -- a jesli model
# napisal cos poza swoim drzewem, ten commit sprawia, ze sprawdzenie zakresu to ZOBACZY.
commit_leftovers() {           # commit_leftovers <etykieta>
  if [ -n "$(git -C "$WT" status --porcelain)" ]; then
    note "the $1 phase left uncommitted work — committing it so the gate can see it"
    git -C "$WT" add -A
    git -C "$WT" commit -q -m "chore(run): uncommitted work from the $1 phase"
  fi
}

# Odcisk asercji w specyfikacjach: sciezka -> ile linii niesie asercje.
#
# Grube narzedzie i ma takie byc. Nie mierzy jakosci asercji, tylko odpowiada na jedno
# pytanie, na ktore inaczej nie odpowiada nikt: czy faza, ktora wlasnie biegla, ZABRALA
# specyfikacji asercje. Liczba moze rosnac dowolnie; spadek na dowolnym pliku zatrzymuje bieg.
assertion_fingerprint() {
  python3 - "$WT" <<'FINGERPRINT'
import os, re, sys

root = sys.argv[1]
carries = re.compile(r"\bassert\w*!|\bassert\b|\bexpect\(|\.toBe|\.toThrow|\.toEqual|\bdebug_assert")
rust_fail_path = re.compile(r"\breturn\s+Err\s*\(")
skip = {".git", "node_modules", "target", "dist", ".loadout", "refs"}

for base, dirs, files in os.walk(root):
    dirs[:] = [d for d in dirs if d not in skip]
    for name in files:
        if not name.endswith((".rs", ".ts", ".tsx", ".js", ".jsx")):
            continue
        rel = os.path.relpath(os.path.join(base, name), root)
        # Specyfikacja poznaje sie po miejscu albo po nazwie -- tak samo, jak poznaje ja
        # `check:` w TASK.md. Kod produkcyjny nas tu nie interesuje: tam asercji ubywa
        # legalnie, bo todo!() znika razem ze szkieletem.
        if "tests/" not in rel.replace(os.sep, "/") and ".test." not in name and ".spec." not in name:
            continue
        try:
            body = open(os.path.join(base, name), encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        # 2026-08-27, T-134: full-clippy wymusil zamiane expect() na jawna, warunkowa
        # sciezke bledu. To nadal asercja specyfikacji, nie jej ubytek.
        n = sum(
            1
            for line in body.split("\n")
            if carries.search(line)
            or (name.endswith(".rs") and rust_fail_path.search(line))
        )
        if n:
            print("%s\t%d" % (rel, n))
FINGERPRINT
}

# Ktory plik specyfikacji STRACIL asercje miedzy dwoma odciskami. Cisza znaczy "zaden".
# Przyrost jest legalny i niewidoczny tutaj z premedytacja.
assertions_lost() {             # assertions_lost <przed.tsv> <po.tsv>
  python3 - "$1" "$2" <<'COMPARE'
import sys

def load(path):
    out = {}
    for line in open(path, encoding="utf-8"):
        if "\t" in line:
            name, count = line.rstrip("\n").split("\t")
            out[name] = int(count)
    return out

was, now = load(sys.argv[1]), load(sys.argv[2])
for name in sorted(was):
    # Plik SKASOWANY liczy sie jako strata wszystkich swoich asercji, a nie jako brak
    # wpisu -- inaczej najprostsza droga na skroty (usun specyfikacje) byla niewidoczna.
    if now.get(name, 0) < was[name]:
        print("  %s: %d assertion lines -> %d" % (name, was[name], now.get(name, 0)))
COMPARE
}

# Porownaj biezacy odcisk z BAZA i zatrzymaj bieg, jesli czegokolwiek ubylo.
# Baza jest zawsze stanem, ktory bramka juz OSADZILA -- nie stanem poprzedniej fazy:
# przy porownywaniu z poprzednia faza dwa male ubytki po sobie przechodza.
assertions_must_not_shrink() {  # assertions_must_not_shrink <baza.tsv> <etykieta>
  local lost
  assertion_fingerprint > "$RUNDIR/assertions-now.tsv"
  lost="$(assertions_lost "$1" "$RUNDIR/assertions-now.tsv")"
  [ -n "$lost" ] || return 0
  echo >&2
  echo "the $2 phase REMOVED assertions from specs:" >&2
  printf '%s\n' "$lost" >&2
  echo >&2
  echo "A spec may gain assertions; it may never lose one. This is the one way a green" >&2
  echo "gate can be a lie: the criterion still runs, still reports passing, and no longer" >&2
  echo "asks the question it was written to ask. Nothing downstream would catch it." >&2
  echo "workspace kept for inspection: $WT" >&2
  exit 1
}

# Podciagnij ORACLE z trunka do galezi. Galaz niesie WLASNA kopie harness/, checks/
# i verify.sh -- bo worktree.sh wycina caly katalog roboczy -- wiec bieg wznowiony godziny
# po wycieciu jest sadzony przez bramke sprzed poprawek, ktore w miedzyczasie naniosl
# orchestrator. Zmierzone dwa razy: 2026-08-15 trzy z pierwszych czterech zatrzyman petli
# byly falszywymi alarmami z nieaktualnej kopii checks/; 2026-08-16 T-06 wznowiony po
# poprawce sufitu dostal exit 3 od WLASNEJ, starej kopii bramki.
#
# Merge, nie rebase: nie przepisujemy historii galezi, ktora zaraz laduje. Tylko gdy czysto --
# konfliktu nie zgadujemy, tylko go zglaszamy i jedziemy na kopii galezi.
#
# Do 2026-08-28 ta funkcja przywracala tez zamrozony `tasks/$ID.md` po merge'u (N-08).
# Ta polowa odeszla razem z katalogiem tasks/: kontrakt zyje teraz jako TASK.md na galezi,
# trunk go nie niesie, wiec merge nie ma czym go nadpisac. Zamrozenie pilnuja dwie reguly
# o tej samej tresci -- checks/quick-scope.sh (kod 1) i harness/gate.py (kod 2) -- obie
# porownujace TASK.md z wersja z commita, ktory go dodal.
refresh_harness_from_trunk() {   # refresh_harness_from_trunk <etykieta>
  local label="$1" before_merge blocked
  before_merge="$(git -C "$WT" rev-parse HEAD)"
  if git -C "$WT" merge --no-edit -q "${LOADOUT_TRUNK:-main}" >/dev/null 2>&1; then
    if [ "$before_merge" = "$(git -C "$WT" rev-parse HEAD)" ]; then
      note "harness is already current with the trunk ($label)"
    else
      note "harness refreshed from the trunk $label"
    fi
  else
    # NAZWIJ, co blokuje. "Nie udalo sie" bez podania pliku to dokladnie ten rodzaj nadzoru,
    # za ktory to repo skasowalo wave.sh: 444 razy "drzewo brudne" bez powiedzenia, CO.
    blocked="$(git -C "$WT" diff --name-only --diff-filter=U 2>/dev/null | paste -sd" " -)"
    git -C "$WT" merge --abort >/dev/null 2>&1 || true
    note "could not merge the trunk cleanly — judging against the branch's own harness copy"
    note "conflicting: ${blocked:-<git refused before touching a file>}"
  fi
}

# Prompt idzie STDIN-em, nigdy w argv (niezmiennik 9): argv widzi kazdy `ps`, a prompt niesie
# tresc zadania i bywa, ze sciezki. Oba CLI to obsluguja -- claude -p czyta prompt ze stdin,
# codex exec czyta go po myslniku.
#
# Pisarz biegnie we WLASNEJ grupie procesow, a harness/process-group.sh jest jedyna polityka
# jej zycia: SIGTERM -> laska -> SIGKILL, a powrot wymaga dowodu ESRCH (niezmiennik 6).
#
# Do 2026-08-28 pod ta polityka biegl WYLACZNIE recenzent i naprawiacz (review.sh, repair.sh).
# Pisarz -- czyli najdluzszy i najdrozszy proces w calym biegu -- biegl bez niej, bo
# ship-task.sh wolal go zwyklym podshellem. Ctrl-C w fazie implementacji zostawial wiec
# `claude` adoptowanego przez PID 1, palacego limit w tle: blad finansowy, nie higieniczny.
# Nowa petla ma o jeden skrypt mniej i o jeden proces wiecej pod polityka, i to nie jest
# przypadek -- to ta sama polityka w jednym rdzeniu, z adapterem na pare linii (niezmiennik 23).
LOADOUT_WRITER_OUT=""
LOADOUT_WRITER_TURNS=""

_loadout_spawn_writer() {
  cd "$WT" || exit 2
  case "$AGENT" in
    claude)
      # --setting-sources project, NIE "". Flaga "" tnie koszt kontekstu ~6x, ale wycina tez
      # .claude/settings.json — czyli NASZ hak Stop i NASZA liste permissions. Bieg bez
      # naglowka sesji nie ma kto zatwierdzic, wiec "nie zabronione" znaczy w praktyce
      # "zablokowane na zawsze": w repo zrodlowym 28 tur i 4,65 $ na zbudowanie niczego.
      local mcp=()
      if [ -f "$WT/.mcp.json" ]; then mcp=(--mcp-config .mcp.json); fi
      # bash 3.2 (macOS) przewraca sie na "${a[@]}" przy pustej tablicy pod set -u.
      exec claude -p \
        --output-format stream-json --verbose \
        ${mcp[@]+"${mcp[@]}"} --strict-mcp-config --setting-sources project \
        --disable-slash-commands \
        --permission-mode acceptEdits \
        --model "$LOADOUT_CLAUDE_MODEL" --effort "$LOADOUT_CLAUDE_EFFORT" \
        --max-turns "$LOADOUT_WRITER_TURNS" > "$LOADOUT_WRITER_OUT" 2>&1
      ;;
    codex)
      # -s workspace-write, nie danger-full-access. Repo zrodlowe eskalowalo piaskownice
      # wylacznie dlatego, ze Chromium nie startuje pod workspace-write; Loadout nie ma
      # sprawdzenia przegladarkowego w bramce, wiec ten powod tu nie istnieje.
      exec codex exec --json --skip-git-repo-check -C "$WT" \
        -s workspace-write \
        -c "sandbox_workspace_write.writable_roots=[\"$GIT_COMMON\"]" \
        -m "$LOADOUT_CODEX_MODEL" -c "model_reasoning_effort=$LOADOUT_CODEX_EFFORT" \
        - > "$LOADOUT_WRITER_OUT" 2>&1
      ;;
  esac
}

write_with() {                 # write_with <transkrypt> <max-turns>  < prompt
  local rc=0 started=0 prompt
  LOADOUT_WRITER_OUT="$1"
  LOADOUT_WRITER_TURNS="$2"
  # Prompt do zmiennej, potem jawnym przekierowaniem na start grupy. Bez tego stdin
  # asynchronicznego polecenia w nieinteraktywnym bashu to /dev/null, a agent dostaje
  # PUSTY prompt i konczy sie zerem -- czyli faza "przeszla", nie robiac nic.
  prompt="$(cat)"
  loadout_agent_group_start _loadout_spawn_writer < <(printf '%s\n' "$prompt") || started=$?
  if [ "$started" != 0 ]; then
    # 3 z loadout_agent_group_start znaczy "przerwanie przyszlo w trakcie startu i zostalo
    # odroczone". Nie ma czego czekac; oddajemy sterowanie pulapce.
    ship_interrupted
  fi
  # Pusty budzet znaczy BRAK watchera, i to jest domyslna odpowiedz: pisarz ma pracowac
  # tak dlugo, jak trzeba, a limitem jest --max-turns i limit vendora. LOADOUT_WRITER_BUDGET
  # istnieje dla selftestu i dla czlowieka, ktory chce twardego sufitu; nazwa jest wlasna,
  # jak LOADOUT_REVIEW_BUDGET w review.sh, bo jeden wspolny "EXEC" mylil sie o adapter.
  loadout_agent_group_wait "${LOADOUT_WRITER_BUDGET:-}" || rc=$?
  return "$rc"
}

say "run $ID — $AGENT writes"
note "claude: $LOADOUT_CLAUDE_MODEL (effort $LOADOUT_CLAUDE_EFFORT) · codex: $LOADOUT_CODEX_MODEL (effort $LOADOUT_CODEX_EFFORT)"
note "receipts: $RUNDIR"
if [ "$REVIEW" = 1 ] && [ "$REVIEW_AVAILABLE" = 0 ]; then
  note "note: '$REVIEWER' is not installed — the second opinion will be skipped, not failed"
fi
printf '%s\n' "$REQUEST" > "$RUNDIR/request.txt"

# ------------------------------------------------------------ 1. przestrzen --
say "workspace"
# Sciezki NIE zgadujemy. worktree.sh sam decyduje o nazwie katalogu (m.in. zamienia ja na
# male litery) i echo tej sciezki jest calym jego interfejsem.
WT="$(bash ./worktree.sh "$BRANCH" | tail -1)" || {
  echo "could not cut a workspace for $BRANCH" >&2; exit 2; }
[ -d "$WT" ] || { echo "worktree.sh printed '$WT', which is not a directory" >&2; exit 2; }
note "$WT"

RESUMED=0
[ -f "$WT/TASK.md" ] && RESUMED=1
# Bieg zabity w polowie zostawia prace niezacommitowana. Domykamy ja GLOSNO i PIERWSZA,
# przed odswiezeniem oracle'a: `git merge` odmawia na brudnym drzewie, wiec bez tego
# odswiezenie po kazdym przerwanym biegu cicho degraduje sie do starej kopii bramki.
[ "$RESUMED" = 1 ] && commit_leftovers "interrupted"

refresh_harness_from_trunk "before this workspace is judged"

# Podpiety worktree trzyma metadane gita w GLOWNYM .git/worktrees/<nazwa>, czyli POZA
# katalogiem, ktory przepuszcza -C w piaskownicy codeksa. Zmierzone w repo zrodlowym:
# kazdy `git commit` w biegu codeksa umieral na "Unable to create index.lock" -- model
# napisal szesc specyfikacji, nie zacommitowal ani jednej i stanal.
GIT_COMMON="$(cd "$WT" && cd "$(git rev-parse --git-common-dir)" && pwd -P)"

# ------------------------------------------------------------- 2. plan + oracle --
# JEDNO wywolanie modelu produkuje kontrakt tego biegu: TASK.md (plan, kryteria, OWNS),
# specyfikacje i szkielet, ktory pozwala im PADAC na asercji.
#
# DLACZEGO to osobne wywolanie, a nie pierwszy akapit promptu implementacji: "before musi
# byc czerwone" da sie WYEGZEKWOWAC tylko wtedy, gdy istnieje moment, w ktorym specyfikacje
# sa, a implementacji nie ma. Jedno wywolanie nie daje harnessowi takiego momentu -- zostaje
# prosba w promptcie, czyli dokladnie to, czego ten plik ma nie robic.
if [ "$RESUMED" = 1 ] && [ -s "$WT/TASK.md" ]; then
  note "this workspace already carries a contract — resuming, the plan phase is skipped"
else
  say "plan — $AGENT turns your request into a contract, then makes it fail"
  write_with "$RUNDIR/plan.jsonl" 80 <<PROMPT || note "the plan phase exited nonzero; the gate decides what that was worth"
Read AGENTS.md in this directory first. It wins over anything below.

THE REQUEST, verbatim:
---
$REQUEST
---

Do two things, in this order, and nothing else.

ONE. Write CONTRACT.md in this directory, with the Write tool. Exactly this shape, because
the gate parses it after the harness renames it to TASK.md:

  # <one line naming what this run delivers>

  ## Plan

  Two to four sentences: the shape of the change and which files carry it. No essay.

  ## AC-1 <what a human would check>

  check: <one command that proves it>

  ## AC-2 ...

  <!-- OWNS
  TASK.md
  <every path this run will create or modify, one per line>
  -->

Rules for the criteria, all of them enforced mechanically:

- ONE to THREE criteria. Not more. If the request is bigger than three criteria, cover its
  core and say in your final message what you left out. A run with eight criteria is a run
  that finishes none of them.
- Numbering is AC-1..AC-n with no gaps.
- Each criterion has exactly ONE check line naming exactly ONE spec path.
- Rust criteria are MODULES of the single integration target, never new files directly in
  src-tauri/tests/. Write the spec as src-tauri/tests/it/<module>.rs, declare it with
  mod <module>; in src-tauri/tests/it/main.rs, and write the check line as
      check: cargo test --test it <module>::
  This is measured, not stylistic: every file placed directly in src-tauri/tests/ becomes
  a SEPARATE binary that statically links the whole library with 527 Tauri crates, about
  60 s each, while the tests themselves execute in 6,0 s total.
- Frontend criteria name the file by path:
      check: npx --no-install vitest run <path>/<name>.test.tsx
- A criterion asserts the sentence a HUMAN sees, not the value a function returns
  (AGENTS.md invariant 29). A green criterion over a dead function is the defect this
  whole repo exists to prevent.

TWO. Write those spec files, plus the SMALLEST SKELETON that lets each spec run and FAIL
at runtime.

Two kinds of stub, and the difference is the whole point:

  FORBIDDEN — a stub that makes a criterion PASS. Returning the expected value, asserting
  something weaker, hard-coding the answer. That is the failure this phase exists to prevent.

  REQUIRED — the skeleton that lets the spec COMPILE and then FAIL. Function signatures with
  todo!() bodies in Rust, modules that reach them, and for the frontend an empty component or
  a function that throws "not implemented". A spec that fails with "module not found",
  "command not found", "no test files found" or "N skipped (N)" proves NOTHING, and
  ./verify.sh before refuses it by name.

Everything you create must be listed in the OWNS block you just wrote (list TASK.md, not
CONTRACT.md -- the harness renames it before anything is judged).

Never touch TASK.md, verify.sh, harness/, checks/, .claude/ or any config file. They are the
oracle. If a criterion cannot be written without one of them, say so in your final message.

WRITE FILES WITH THE WRITE AND EDIT TOOLS. Never through Bash. Measured 2026-08-28, and this
cost a whole run: python3 -c and python3 with a heredoc are REFUSED in an unattended run even
though Bash(python3:*) sits in the allow list -- Claude Code does not honour allow rules for
interpreters and for local scripts. Simple Bash (grep, ls, sed, cat, find, git status) works.
So does Write. A phase that tries to write a file through python3 burns its whole turn budget
on approval prompts nobody can answer: 81 turns and $10,40 for zero files.

For the same reason: do NOT try to run ./verify.sh, cargo or vitest yourself -- all three are
refused. The harness runs the gate between phases and hands you the reasons. You are not
expected to see it here; you are expected to write a contract that CAN be judged.

One shell command per Bash call. Never chain with a semicolon or with two ampersands: Claude
Code splits a compound command and asks approval for each part.

Commit everything you wrote as ONE commit with git add and git commit (both allowed),
subject "docs(run): the contract this run is judged against".
PROMPT
  # TASK.md materializuje HARNESS, nie agent. Powod jest mierzony i kosztowal cala fazę:
  # `.claude/settings.json` ma `Write(TASK.md)` i `Edit(TASK.md)` w `deny` -- slusznie, bo
  # kontrakt jest tym, po czym bieg jest sadzony. Etap planu prosil wiec o plik, ktorego nie
  # wolno mu napisac, dostawal "File is in a directory that is denied by your permission
  # settings", probowal to obejsc heredociem w Bashu (tez odmowa) i spalil 81 tur i 10,40 $
  # nie zapisujac ani jednego pliku.
  #
  # Rozwiazaniem nie jest zdjecie zakazu, tylko przeniesienie momentu: agent pisze CONTRACT.md
  # (nie jest zakazany), a harness zmienia mu nazwe. Dzieki temu plik, ktory czyta bramka,
  # jest dla KAZDEGO agenta niezapisywalny na poziomie uprawnien -- niemozliwy, nie tylko
  # wykrywany (niezmiennik 28). `checks/quick-permissions.sh` dalej wymaga tego zakazu.
  if [ -f "$WT/CONTRACT.md" ] && [ ! -f "$WT/TASK.md" ]; then
    mv "$WT/CONTRACT.md" "$WT/TASK.md"
    note "the plan wrote CONTRACT.md; the harness materialised it as TASK.md"
  elif [ -f "$WT/CONTRACT.md" ]; then
    # Oba naraz znaczy, ze ktos wznowil bieg albo agent obszedl zakaz. Nie zgadujemy ktore.
    echo "both CONTRACT.md and TASK.md exist in $WT -- the harness will not guess which one" >&2
    echo "is the contract. Remove one and rerun." >&2
    exit 2
  fi
  commit_leftovers plan
fi

if [ ! -s "$WT/TASK.md" ]; then
  echo >&2
  echo "the plan phase wrote no contract: neither CONTRACT.md nor TASK.md is in $WT." >&2
  echo "read $RUNDIR/plan.jsonl -- the usual cause is a phase that spent its turns asking for" >&2
  echo "a permission nobody could grant. Nothing was implemented." >&2
  exit 2
fi

# I DOPIERO TERAZ bramka `before` jest egzekwowalna. To jest ten jeden warunek, ktorego bieg
# nie moze obejsc: jezeli kryteria nie sa czerwone z wlasciwego powodu, dalej nie ma po co
# isc -- implementacja przeciwko sprawdzeniu, ktore nic nie sprawdza, jest drozsza niz jej
# brak, bo zostawia zielone, ktoremu ktos uwierzy.
say "before — the criteria must be red, and red for the right reason"
RED=0; gate before || RED=$?

# ----------------------------------------------- 2a. naprawa kontraktu x1 --
# DOKLADNIE JEDNA runda, i wylacznie na jedynce. Dwojka ("bramka zle skonfigurowana")
# i trojka ("sufit") nie sa dla modelu.
#
# Zmierzone: ta runda odpalila sie w 23 biegach na 121 i za kazdym razem ratowala bieg,
# ktory inaczej trafial do kosza. Wzorcowy przypadek to T-06: siedem poprawnych
# specyfikacji i szkielet, w ktorym JEDNA funkcja sie zakleszcza, wiec kryterium WISIALO
# zamiast padac. Diagnoza kosztowala noc, naprawa -- jedno wywolanie modelu.
if [ "$RED" = 1 ]; then
  say "contract repair — one round, then stop"
  WHY="$(gate_reasons)"
  printf '%s\n' "$WHY"

  # Odcisk asercji PRZED runda. Ta faza dostaje instrukcje "spraw, zeby kryterium padalo
  # INACZEJ", a najtansza droga do tego jest asertowac mniej -- i jest to jedyna faza,
  # w ktorej "asertuj mniej" jest wiarygodnym ODCZYTEM instrukcji, a nie jawnym oszustwem.
  # Dlatego dostaje obrone mechaniczna, a nie zdanie w promptcie (niezmiennik 28).
  assertion_fingerprint > "$RUNDIR/assertions-before.tsv"

  write_with "$RUNDIR/plan-repair.jsonl" 80 <<PROMPT || note "the contract repair exited nonzero; the gate decides what that was worth"
Read AGENTS.md and TASK.md in this directory.

Every criterion has to FAIL when ./verify.sh before runs, and each failure has to be a
MISSING BEHAVIOUR that shows up at runtime. These did not qualify:

$WHY

Fix the SPECS AND THE SKELETON so each criterion fails at runtime, on an assertion.

Three shapes of false red and the repair for each:

  did not RUN — the spec file or the test target does not exist. Create it, and for Rust
  declare the module with mod <name>; in src-tauri/tests/it/main.rs.
  did not COMPILE / module not found — the skeleton is missing. Add the signature with a
  todo!() body, or the empty component that the spec imports. The import must resolve.
  did not FINISH — the skeleton HANGS instead of failing. A todo!() body cannot block; a
  channel, a lock or a sleep can. Replace it with todo!() and nothing else.

Do NOT weaken any assertion, do NOT delete a spec, and do NOT make a criterion pass. The
run measures whether the assertion count in every spec file went DOWN, and stops if it did.

Never touch verify.sh, harness/, checks/ or TASK.md. Commit with one conventional-commit
subject.
PROMPT
  commit_leftovers "contract repair"
  assertions_must_not_shrink "$RUNDIR/assertions-before.tsv" "contract repair"

  say "before — again, after the contract repair"
  RED=0; gate before || RED=$?
fi

if [ "$RED" -ne 0 ]; then
  echo >&2
  case "$RED" in
    2) echo "the gate says this run has no usable contract (exit 2). Read the reason above:" >&2
       echo "the criteria disagree with themselves, or the plan phase wrote none." >&2 ;;
    3) echo "the before tier hit its ceiling (exit 3) — a spec is hanging, not failing." >&2 ;;
    *) echo "the criteria are still not honestly red after one repair round." >&2 ;;
  esac
  echo "AGENTS.md §7 says that is a human's call. Nothing was implemented." >&2
  echo "workspace kept for inspection: $WT" >&2
  echo "receipts: $RUNDIR" >&2
  exit "$RED"
fi
note "red for the right reason — the oracle is certified"

# ------------------------------------------------------------ 3. implementacja --
# Odcisk kontraktu W CHWILI, W KTOREJ BRAMKA GO OSADZILA. Od tego miejsca zadna faza nie ma
# prawa zabrac specyfikacji asercji -- ani pisarz, ani runda naprawcza. To jedyny sposob,
# w jaki zielona bramka moze klamac bez sladu: kryterium dalej biegnie, dalej melduje
# "1 passed" i po prostu przestaje pytac o to, po co je napisano.
assertion_fingerprint > "$RUNDIR/assertions-certified.tsv"

say "implementing with $AGENT"
write_with "$RUNDIR/build.jsonl" 250 <<PROMPT || note "the writer exited nonzero; the gate decides what that was worth"
Read AGENTS.md and TASK.md in this directory. The acceptance specs already exist and are
already proven red for the right reason — that is the contract, and it is frozen.

Implement it. One criterion at a time: implement, commit that criterion, move on.

You cannot run the gate yourself, and do not try: ./verify.sh, cargo and vitest are all
REFUSED in an unattended run (measured 2026-08-28 -- Claude Code does not honour allow rules
for local scripts and interpreters, whatever .claude/settings.json says). What you get instead
is mechanical and already wired: the Stop hook runs the gate when you try to end your turn and
hands you its output, and the harness runs it again between phases. Write files with the Write
and Edit tools, never through python3 or a Bash heredoc -- those are refused too.

Rules that do not bend:

- Never edit TASK.md, verify.sh, harness/, checks/ or any config. They are the oracle. If a
  criterion cannot be met without touching one, that is a finding — say it in your final
  message instead of doing it.
- Never weaken or delete an assertion in an existing spec. A spec may gain assertions; it
  may never lose one. This is measured between phases, not trusted.
- If a criterion can only be passed in a way you consider cheating, say so plainly and
  change nothing. AGENTS.md §7 names that as the most valuable thing you can report.
- Three attempts at one criterion, then move on and commit what exists, with the commit
  message saying what is still red.
- One shell command per Bash call. Never chain with a semicolon or with two ampersands.

Write only under the paths in the OWNS block of TASK.md. Commit subjects are
"<type>(<scope>): <what changed>".
PROMPT
commit_leftovers implementation
assertions_must_not_shrink "$RUNDIR/assertions-certified.tsv" "implementation"

# ------------------------------------------------------------- 4. bramka zadania --
# `task`, nie `full`. Ten poziom pyta o TO zadanie: jego kryteria plus tanie sprawdzenia
# projektowe (14 sztuk, 9,6 s razem). Suita calego repo -- 280 s z 319 -- nalezy do
# ladowania i do CI, gdzie jest o co pytac; tutaj przebudowywalaby rzeczy, ktorych ten
# bieg nie tknal, i robila to DWA razy.
say "gate — this run's criteria"
GATE=0; gate task || GATE=$?
note "gate: $GATE"

# ----------------------------------------------------------- 5. naprawa z paragonu --
# Naprawe prowadzi BRAMKA, nie recenzent. Do 2026-08-28 runda naprawcza odpalala sie takze
# na uwage recenzenta -- 97 recenzji na 105 zwracalo uwage, wiec byla obowiazkowa w 81%
# biegow i mieszala dwie rozne rzeczy: "sprawdzenie padlo" i "ktos ma zdanie".
ROUND=0
while [ "$GATE" = 1 ] && [ "$ROUND" -lt "$ROUNDS" ]; do
  ROUND=$((ROUND + 1))
  # Podciagnij ORACLE z trunka PRZED runda. Galaz wycieta godziny temu biegnie ze
  # sprawdzeniami sprzed poprawek, ktore orchestrator nanosil w miedzyczasie -- i trzy
  # z pierwszych czterech zatrzyman petli (2026-08-15) to byly wlasnie falszywe alarmy
  # z nieaktualnej kopii checks/. Moment jest bezpieczny, bo zaden agent nie pracuje.
  refresh_harness_from_trunk "before repair round $ROUND"

  say "repair $ROUND of $ROUNDS — from the receipt, by the writer"
  WHY="$(gate_reasons)"
  printf '%s\n' "$WHY"
  write_with "$RUNDIR/repair-$ROUND.jsonl" 120 <<PROMPT || note "the repair round exited nonzero; the gate decides what that was worth"
Read AGENTS.md and TASK.md in this directory. Your implementation is in place and the gate
came back red. This is what it said, from its own receipt:

$WHY

Fix exactly that. Nothing else — no refactors, no cleanups, no improvements you noticed on
the way. You cannot run the gate yourself (./verify.sh, cargo and vitest are refused in an
unattended run); the Stop hook runs it when you end your turn and the harness runs it after
this round.

Rules that do not bend:

- Never weaken or delete an assertion, and never edit TASK.md. If the criterion is wrong,
  say so in your final message and change nothing — the run measures assertion counts and
  stops when one goes down.
- Never touch verify.sh, harness/, checks/ or any config to make a check pass. A check you
  silenced is a check nobody will run again.
- If you cannot fix it, say which criterion and why, in one paragraph. A human reads this.
- One shell command per Bash call. Never chain with a semicolon or with two ampersands.
PROMPT
  commit_leftovers "repair $ROUND"
  assertions_must_not_shrink "$RUNDIR/assertions-certified.tsv" "repair $ROUND"

  say "gate — after repair $ROUND"
  GATE=0; gate task || GATE=$?
  note "gate: $GATE"
done

if [ "$GATE" -eq 2 ]; then
  echo >&2
  echo "the gate is MISCONFIGURED (exit 2), not red. That is ours to fix, not the model's:" >&2
  echo "read the reason above (a missing tool, or a contract defect in TASK.md)." >&2
  echo "workspace kept for inspection: $WT" >&2
  exit 2
fi

# ------------------------------------------------------------ 6. druga opinia --
# Wylacznie na --review, i wylacznie jako RAPORT. Nie zatwierdza, nie blokuje i nie odpala
# naprawy: schemat odpowiedzi ma verdict w {concern, none}, wiec strukturalnie nie ma czego
# zatwierdzic (D3). Recenzent niedostepny to notatka, nigdy czerwien.
if [ "$REVIEW" = 1 ]; then
  REVIEW_OUT="$RUNDIR/review.txt"
  : > "$REVIEW_OUT"
  if [ "$REVIEW_AVAILABLE" = 1 ] && [ -f "$WT/review.sh" ]; then
    say "second opinion by $REVIEWER — a report for you, not a gate"
    RV=0
    # Obie flagi, nie samo --reviewer: review.sh po tej parze poznaje tryb same-vendor
    # i dopiero wtedy daje recenzentowi INNY model (D3).
    ( cd "$WT" && bash ./review.sh --agent "$AGENT" --reviewer "$REVIEWER" ) \
      | tee "$REVIEW_OUT" || RV=$?
    if [ "$RV" = 2 ]; then
      echo "review.sh reports OUR misconfiguration, not an unavailable reviewer." >&2
      exit 2
    fi
  elif [ "$REVIEW_AVAILABLE" = 1 ]; then
    note "review.sh is missing — no second opinion (advisory, never a red)"
  else
    note "'$REVIEWER' unavailable — no second opinion (advisory, never a red)"
  fi
fi

# ------------------------------------------------------------ 7. co zostaje --
# Paragon przezywa worktree. To jedyny artefakt, ktory na maszynie zrodlowej przetrwal
# wszystkie biegi, i to on jest naturalnym wejsciem dla widoku sesji w samym Loadout.
if [ -f "$WT/runs/last.json" ]; then
  cp "$WT/runs/last.json" "$RUNDIR/gate-final.json"
fi

say "run $ID: gate $([ "$GATE" -eq 0 ] && echo GREEN || echo "RED (exit $GATE)")"
( cd "$WT" && bash ./verify.sh --report ) || true

echo
echo "branch $BRANCH is ready in $WT"
if [ "$GATE" -eq 0 ]; then
  echo "land it with:  ./integrate.sh $BRANCH"
  echo "the whole-repo suite runs there, once — that is where it belongs."
else
  echo "the gate is red after $ROUND repair round(s) — AGENTS.md §7 says that is a human's call."
  echo "read $RUNDIR/, then either fix it by hand or change the criterion deliberately."
fi
exit "$GATE"
