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

# Bash czyta ten plik PRZYROSTOWO, po offsetach bajtowych. Edycja w trakcie biegu przesuwa
# wszystko za kursorem i proces wykonuje smieci -- skladniowo poprawne, semantycznie losowe.
# Zdarzylo sie trzy razy 2026-08-15, za kazdym razem po moim wlasnym ostrzezeniu, i za
# kazdym razem kosztowalo diagnostyke "czy ten bieg jeszcze jest wazny".
# Kopia jest niezmienna, wiec orchestrator moze naprawiac harness, kiedy petla chodzi.
# ROOT liczony PRZED exec: w kopii $0 wskazuje na mktemp, a nie na repo.
# Nazwa sentinela jest WŁASNA dla tego skryptu, nie wspólna. Wspólna („LOADOUT_PINNED")
# wyciekała przez środowisko: build-loop.sh odpala ship-task.sh, więc dziecko widziało
# cudzy sentinel, pomijało własne przypięcie i brało katalog rodzica za korzeń repo —
# czyli obrona przed edycją w trakcie biegu wyłączała się dokładnie w pętli, gdzie jest
# najpotrzebniejsza. Osobne nazwy czynią ten wyciek niemożliwym, a nie tylko posprzątanym.
if [ -z "${LOADOUT_PINNED_SHIP_TASK:-}" ]; then
  LOADOUT_SELF_SHIP_TASK="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
  LOADOUT_SNAP="$(mktemp -t ship-task)"
  cat "${BASH_SOURCE[0]}" > "$LOADOUT_SNAP"
  export LOADOUT_PINNED_SHIP_TASK=1 LOADOUT_SELF_SHIP_TASK
  exec bash "$LOADOUT_SNAP" "$@"
fi

# W kopii ${BASH_SOURCE[0]} wskazuje na plik w $TMPDIR, więc katalog repo trzeba WZIĄĆ
# z góry, nie policzyć od nowa. Fallback jest na wypadek, gdyby mktemp odmówił.
SELF_DIR="${LOADOUT_SELF_SHIP_TASK:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)}"
unset LOADOUT_PINNED_SHIP_TASK LOADOUT_SELF_SHIP_TASK   # higiena; poprawność daje sama nazwa

ROOT="$SELF_DIR"
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

# Podciagnij ORACLE z trunka do galezi. Galaz niesie WLASNA kopie harness/, checks/
# i verify.sh -- bo `worktree.sh` wycina caly katalog roboczy -- wiec bieg wznowiony
# godziny po wycieciu jest sadzony przez bramke sprzed poprawek, ktore w miedzyczasie
# naniosl orchestrator.
#
# Zmierzone dwa razy, z tego raz dzisiaj. 2026-08-15: trzy z pierwszych czterech zatrzyman
# petli byly falszywymi alarmami z nieaktualnej kopii checks/. 2026-08-16: T-06 wznowiony
# po poprawce sufitu w gate.py dostal exit 3 od WLASNEJ, starej kopii bramki, przez co
# routing wznowienia zobaczyl "3" zamiast "1", odmowil i zakonczyl bieg kodem 2 -- czyli
# poprawka, ktora powstala dokladnie dla tego biegu, byla dla niego nieosiagalna.
#
# Merge, nie rebase: nie przepisujemy historii galezi, ktora zaraz laduje. Tylko gdy
# czysto -- konfliktu nie zgadujemy, tylko go zglaszamy i jedziemy na kopii galezi.
#
# $1 = 1 znaczy "kontrakt jest juz ZAMROZONY, przywroc tasks/ po merge'u" (N-08). Bieg nie
# moze zmieniac warunkow wlasnego zaliczenia -- ulepszony plik zadania obowiazuje NASTEPNY
# bieg. Na poczatku biegu jest odwrotnie: tam wlasnie chcemy biezacego kontraktu, wiec 0.
refresh_harness_from_trunk() {   # refresh_harness_from_trunk <zamrozony-kontrakt:0|1> <etykieta>
  local frozen="$1" label="$2" before_merge blocked
  before_merge="$(git -C "$WT" rev-parse HEAD)"
  if git -C "$WT" merge --no-edit -q "${LOADOUT_TRUNK:-main}" >/dev/null 2>&1; then
    # WYLACZNIE wlasny plik zadania, nigdy cale `tasks/`. Zamrozenie ma chronic przed jedna
    # rzecza -- biegiem, ktory zmienia warunki WLASNEGO zaliczenia (N-08) -- a to jest pytanie
    # o `tasks/$ID.md` i o nic wiecej. Cudze pliki zadan sa dla tego biegu obojetne i maja
    # isc za trunkiem.
    #
    # Szersza wersja miala dwie wady i obie sie zmaterializowaly 2026-08-16 na T-20:
    #   1. `quick-scope` slusznie zapalal sie na galezi, ktora rozni sie od trunka w plikach
    #      spoza swojego OWNS -- czerwien nie do odroznienia od winy pisarza, ktory tego nie
    #      napisal. Kosztowalo bieg i runde naprawcza szukajaca winy u siebie.
    #   2. Grozniejsza: ten commit REWERTOWAL cudze zmiany. Gdyby galaz wyladowala, merge
    #      wniosl by revert na trunk i po cichu skasowal prace zrobiona w miedzyczasie --
    #      w tym wypadku wzmocnione kryterium T-24 i wiersz T-26 w INDEX.md. Cicha utrata
    #      cudzej pracy jest najgorszym trybem, jaki ten harness moze miec.
    if [ "$frozen" = 1 ] && ! git -C "$WT" diff --quiet "$before_merge" -- "tasks/$ID.md"; then
      git -C "$WT" checkout -q "$before_merge" -- "tasks/$ID.md"
      git -C "$WT" commit -q -m "chore(contract): keep $ID's frozen contract across the trunk refresh" -- "tasks/$ID.md" \
        || true
      note "trunk brought a newer tasks/$ID.md — restored this run's frozen contract (N-08)"
    fi
    if [ "$before_merge" = "$(git -C "$WT" rev-parse HEAD)" ]; then
      note "harness is already current with the trunk ($label)"
    else
      note "harness refreshed from the trunk $label"
    fi
  else
    # NAZWIJ, co blokuje. "Nie udalo sie" bez podania pliku to dokladnie ten rodzaj
    # nadzoru, za ktory to repo skasowalo wave.sh: 444 razy "drzewo brudne" bez powiedzenia,
    # CO jest brudne. Zmierzone tego samego dnia na T-06: komunikat bez nazwy pliku kazal mi
    # diagnozowac merge recznie, a odpowiedz -- konflikt w lib.rs -- jest z gory znana.
    blocked="$(git -C "$WT" diff --name-only --diff-filter=U 2>/dev/null | paste -sd" " -)"
    git -C "$WT" merge --abort >/dev/null 2>&1 || true
    note "could not merge the trunk cleanly — judging against the branch's own harness copy"
    note "conflicting: ${blocked:-<git refused before touching a file>}"
  fi
}

say "task $ID — $AGENT writes, $REVIEWER gives the second opinion"
note "claude: $LOADOUT_CLAUDE_MODEL (effort $LOADOUT_CLAUDE_EFFORT) · codex: $LOADOUT_CODEX_MODEL (effort $LOADOUT_CODEX_EFFORT)"
note "transcripts: $RUNDIR"
[ "$REVIEW_AVAILABLE" = 1 ] || note "note: '$REVIEWER' is not installed — the review stage will be skipped, not failed"

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

# Odcisk asercji w specyfikacjach: sciezka -> ile linii niesie asercje.
#
# Grube narzedzie i ma takie byc. Nie mierzy jakosci asercji, tylko odpowiada na jedno
# pytanie, na ktore inaczej nie odpowiada nikt: czy faza, ktora wlasnie biegla, ZABRALA
# specyfikacji asercje. Liczba moze rosnac dowolnie; spadek na dowolnym pliku zatrzymuje bieg.
# Formatowanie jest bez znaczenia, bo liczymy linie niosace asercje, nie znaki.
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
        # legalnie, bo `todo!()` znika razem ze szkieletem.
        if "tests/" not in rel.replace(os.sep, "/") and ".test." not in name and ".spec." not in name:
            continue
        try:
            body = open(os.path.join(base, name), encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        # 2026-08-27, T-134: full-clippy wymusił zamianę `expect()` na jawną,
        # warunkową ścieżkę błędu. To nadal asercja specyfikacji, nie jej ubytek.
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
# Przyrost jest legalny i niewidoczny tutaj z premedytacja: specyfikacja moze zyskiwac
# asercje dowolnie, nie moze zadnej zgubic.
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
#
# Baza jest zawsze stanem, ktory bramka juz OSADZILA -- nie stanem poprzedniej fazy.
# Roznica jest istotna: przy porownywaniu z poprzednia faza dwa male ubytki po sobie
# przechodza, bo kazdy z osobna jest maly.
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

# Czy ta przestrzen juz cos niosla? Pytamy TERAZ, bo za chwile skopiujemy tam kontrakt
# i `-f TASK.md` przestanie odrozniac swieza przestrzen od wznawianej.
RESUMED=0
[ -f "$WT/TASK.md" ] && RESUMED=1

# ORACLE z trunka PRZED czymkolwiek, co sadzi. Bez tego wznowiony bieg jest sadzony
# przez bramke sprzed poprawek, ktore powstaly wlasnie dla niego.
# Bieg zabity w polowie zostawia prace niezacommitowana. Domykamy ja GLOSNO i PIERWSZA,
# przed odswiezeniem oracle'a: `git merge` odmawia na brudnym drzewie, wiec bez tego
# odswiezenie po kazdym przerwanym biegu cicho degraduje sie do starej kopii bramki.
# Zmierzone 2026-08-16: trzy biegi zabite naraz, jeden z nich w polowie naprawy
# szkieletu -- praca byla dobra i nie mial jej kto zobaczyc.
[ "$RESUMED" = 1 ] && commit_leftovers "interrupted"

refresh_harness_from_trunk 0 "before this workspace is judged"

# Podpięty worktree trzyma metadane gita w GŁÓWNYM .git/worktrees/<nazwa>, czyli POZA
# katalogiem, który przepuszcza `-C` w piaskownicy codeksa. Zmierzone w repo źródłowym:
# każdy `git commit` w biegu codeksa umierał na
#     fatal: Unable to create '<root>/.git/worktrees/<n>/index.lock': Operation not permitted
# — model napisał sześć specyfikacji, nie zacommitował ani jednej i stanął. Odmowa
# środowiska nie do odróżnienia od poddania się modelu, jeśli nie czyta się logu.
GIT_COMMON="$(cd "$WT" && cd "$(git rev-parse --git-common-dir)" && pwd -P)"

cp "$TASK_FILE" "$WT/TASK.md"
git -C "$WT" add TASK.md
# Przy wznowieniu kontrakt jest juz zacommitowany i identyczny — `git commit` bez zmian
# konczy sie jedynka i pod `set -e` wywraca caly bieg. Pusty commit tez nie: historia
# galezi ma miec dokladnie jeden commit kontraktowy, bo to on jest baza zakresu.
if git -C "$WT" diff --cached --quiet; then
  note "TASK.md unchanged — the contract commit is already the branch's first"
else
  git -C "$WT" commit -q -m "docs(task): $ID — the contract this branch is judged against"
  note "TASK.md committed as the branch's first commit"
fi

# worktree.sh świadomie PONOWNIE UŻYWA istniejącej przestrzeni. Pytanie brzmi, czy ta
# przestrzeń ma już IMPLEMENTACJĘ — bo wtedy „before" byłoby zielone i nikt by się nie
# dowiedział, że kryterium niczego nie sprawdza.
#
# Odpowiada na to ZACHOWANIE, nie obecność pliku (niezmiennik 20 zastosowany do samego
# harnessu). Wcześniej stało tu `if [ -f "$WT/TASK.md" ]` i to odmawiało także wtedy, gdy
# kontrakt był gotowy, a implementacji nie było — czyli w jedynym przypadku, w którym
# wznowienie jest i bezpieczne, i oszczędza pół godziny. Zmierzone na T-02: kontrakt
# certyfikowany, siedem kryteriów uczciwie czerwonych, a skrypt kazał wyrzucić całą pracę.
#
# `before` == 0 znaczy dokładnie „specyfikacje są, implementacji nie ma" — czyli stan,
# z którego wolno wystartować. Cokolwiek innego to odmowa jak dotąd.
# Czy ktores kryterium PRZESZLO, zanim powstala implementacja? To jedyny stan, w ktorym
# drugi bieg nie ma jak dowiesc czerwieni -- wiec jedyny, w ktorym odmowa jest tansza niz
# wznowienie. Czytamy paragon, bo to on niesie werdykt POZIOMU: `before` odwraca kryteria,
# wiec samo `ok: false` nie mowi, czy kryterium przeszlo, czy nie ruszylo. Nazwa powodu mowi.
# Exit 0 znaczy "tak, takie kryterium jest".
# Czy faza kontraktu napisala CHOC JEDNA specyfikacje? Kryterium, ktorego plik nie istnieje,
# bramka melduje jako "did not RUN". Jesli tak wygladaja WSZYSTKIE, to nie ma czego naprawiac
# -- trzeba napisac kontrakt od zera, a nie posylac naprawiacza do pliku, ktorego nie ma.
# Zmierzone na T-19 (2026-08-16): bieg zabity w fazie kontraktu, osiem razy
# "did not RUN (error: no test target)". Exit 0 znaczy "nie ma ani jednej specyfikacji".
contract_has_no_specs_at_all() {
  # 2026-08-19 -- PYTA DYSK, NIE PARAGONU, i to jest naprawa tej samej klasy co w funkcji
  # nizej. Wersja poprzednia uznawala „nie ma ani jednej specyfikacji" wtedy i tylko wtedy,
  # gdy KAZDE kryterium meldowalo `did not RUN`. Kryterium rustowe, ktorego modul jeszcze nie
  # istnieje, melduje jednak `exit 0 but no evidence of execution`: po scaleniu celow testowych
  # w jeden (`tests/it/`) cel `it` ISTNIEJE i biegnie z filtrem, ktory nic nie lapie. Wiec dla
  # kazdego zadania z kryterium rustowym ten test byl falszem takze wtedy, gdy plikow nie bylo
  # ani jednego -- a bieg szedl wtedy do RUNDY NAPRAWCZEJ ze zdaniem „the specs are already
  # written and already judged". Komentarz nizej ostrzega przed dokladnie tym: naprawiacz nie ma
  # czego naprawic, a kosztuje cale wywolanie modelu.
  #
  # Zmierzone 2026-08-19 na T-43: trzy kryteria, ZERO plikow specyfikacji, paragon 2x
  # „exit 0 but no evidence" + 1x „did not RUN" -- stara wersja oddawala 1 („specyfikacje sa").
  #
  # Dlaczego dysk, a nie drugi napis w warunku: „czy plik specyfikacji istnieje" jest pytaniem
  # o PLIK, a nie o brzmienie komunikatu (niezmiennik 20 zastosowany do harnessu). Sciezki
  # liczy `gate.spec_tokens` -- ten sam parser, ktory sadzi kontrakt, wiec nie powstaje tu
  # druga tabela odwzorowan `--test it <modul>:: -> plik` (niezmiennik 23). Rozne od zera
  # trafienie znaczy „jest co naprawiac", zero znaczy „pisz od nowa".
  python3 - "$WT" <<'SPECS'
import os, sys

wt = sys.argv[1]
sys.path.insert(0, os.path.join(wt, "harness"))
try:
    import gate
except Exception:
    sys.exit(1)                      # brak parsera to brak dowodu, a nie dowod

criteria = gate.read_task(os.path.join(wt, "TASK.md"))
named = [path for c in criteria if c["check"] for path in gate.spec_tokens(c["check"])]
if not named:
    sys.exit(1)
sys.exit(1 if any(os.path.isfile(os.path.join(wt, path)) for path in named) else 0)
SPECS
}



contract_has_a_passing_criterion() {
  python3 - "$WT/runs/last.json" <<'RECEIPT'
import json, sys
try:
    receipt = json.load(open(sys.argv[1], encoding="utf-8"))
except Exception:
    sys.exit(1)                      # brak paragonu to brak dowodu, a nie dowod
if receipt.get("tier") != "before":
    sys.exit(1)
for c in receipt.get("checks", []):
    reason = c.get("reason") or ""
    # 2026-08-19 -- „exit 0 but no evidence" ZNIKNELO Z TEGO WARUNKU, i to jest naprawa
    # KLASYFIKACJI, nie rozluznienie bramki. Ta funkcja odpowiada na jedno pytanie: „czy
    # jakies kryterium JEST ZIELONE przed implementacja", bo tylko wtedy wznowienie bywa
    # oszustwem. „exit 0 bez licznika przejsc" znaczy dokladnie odwrotnie -- `cargo test
    # --test it <nieistniejacy_modul>::` nie uruchomil ANI JEDNEGO testu, bramka liczy to jako
    # CZERWONE (`not c.get("ok")` stoi w tym samym warunku) i jest to normalny stan przed faza
    # kontraktu. Ten sam podpis dostal pre-flight pierwszego biegu T-42 i harness SLUSZNIE
    # poslal go wtedy do fazy kontraktu.
    #
    # Zmierzone 2026-08-19 na T-43 odbitym od `task-T-42`: TASK.md przyniesiony z bazy ustawia
    # RESUMED=1, wiec kazde swieze kryterium rustowe wpadalo tutaj i bieg konczyl sie kodem 2
    # przy TRZECH uczciwie czerwonych kryteriach (`verify.sh before` w tym samym drzewie:
    # „4 checks, 3 failed"). Sonda na tych samych bajtach, przed i po: „PASSES before
    # implementation" -> rc=0 (dalej odmawia), „exit 0 but no evidence" -> rc=1 (wznawia).
    if (c.get("kind") == "acceptance" and not c.get("ok")
            and "PASSES before implementation" in reason):
        sys.stderr.write("   %s passes before implementation -- it certifies nothing\n" % c["id"])
        sys.exit(0)
sys.exit(1)
RECEIPT
}

# Trzy flagi wznowienia. Ustawione JAWNIE, bo `set -u` zamienia niezainicjowana zmienna
# w blad dopiero w tej galezi, ktora akurat nie biegla w testach.
PRE_RESUME=0          # kod wyjscia `before` ze sprawdzenia wznowienia
PRE_RESUME_RAN=0      # czy to sprawdzenie w ogole sie odbylo
RESUME_WITH_SPECS=0   # czy zastalismy napisane specyfikacje, ktore nie certyfikuja
RESUME_CERTIFIED=0    # czy kontrakt byl juz certyfikowany, a kryteria przechodza
if [ "$RESUMED" = 1 ]; then
  ( cd "$WT" && bash ./verify.sh before >/dev/null 2>&1 ) || PRE_RESUME=$?
  PRE_RESUME_RAN=1
  # Cztery stany, w jakich zastaje sie istniejaca przestrzen. Rozroznia je PARAGON, nie
  # ksztalt historii gita (niezmiennik 20 zastosowany do harnessu): wczesniej stalo tu
  # "policz commity nad trunkiem", proxy, ktore mylilo sie w obie strony. Zmierzone na T-06
  # (2026-08-16): `commit_leftovers` domknal prace fazy kontraktu DRUGIM commitem, licznik
  # pokazal 2, i skrypt kazal wyrzucic prace, w ktorej nie bylo ani jednej linii implementacji.
  #
  # Wszystkie cztery zdarzyly sie tego samego dnia, na trzech galeziach naraz, kiedy trzy biegi
  # zostaly zabite w trzech roznych fazach. To nie sa przypadki teoretyczne.
  if [ "$PRE_RESUME" = 0 ]; then
    # (1) Kontrakt certyfikowany, implementacji nie ma. Stan, z ktorego wolno wystartowac.
    note "$WT already has a certified contract -- resuming from the implementation phase"

  elif [ "$PRE_RESUME" = 1 ] && contract_has_no_specs_at_all; then
    # (2) Faza kontraktu zginela, zanim napisala cokolwiek: kazde kryterium melduje
    # "did not RUN". Naprawiacz nie ma czego naprawic -- posylanie go do pliku, ktory nie
    # istnieje, spalilo by cale wywolanie modelu. Kontrakt powstaje od zera.
    note "$WT has a contract but not one spec -- the contract phase never got that far"
    note "writing the specs from scratch; there is nothing here to repair"

  elif [ "$PRE_RESUME" = 1 ] && ! contract_has_a_passing_criterion; then
    # (3) Specyfikacje SA, ale nie certyfikuja: cos wisi, cos sie nie kompiluje, cos nie
    # ruszylo. Zadne kryterium nie przechodzi, wiec nie ma tu implementacji do stracenia.
    # Przepisywanie specyfikacji od zera kosztowaloby drugie pelne wywolanie pisarza za
    # wiedze, ktora juz lezy w paragonie -- idziemy prosto do rundy naprawczej.
    note "$WT carries a contract that does not certify -- no criterion passes, so there is"
    note "no implementation here to lose; the contract repair round takes it from here"
    RESUME_WITH_SPECS=1

  elif [ "$PRE_RESUME" = 1 ] \
    && [ -f "$RUNDIR/assertions-certified.tsv" ] \
    && assertion_fingerprint > "$RUNDIR/assertions-now.tsv" \
    && [ -z "$(assertions_lost "$RUNDIR/assertions-certified.tsv" "$RUNDIR/assertions-now.tsv")" ]; then
    # (4) Kryteria PRZECHODZA -- i to jest jedyny stan, w ktorym wznowienie bywa oszustwem,
    # bo kryterium przechodzace przed implementacja nie sprawdza niczego. Rozstrzyga to
    # odcisk asercji z chwili, w ktorej bramka udowodnila te specyfikacje CZERWONYMI:
    # jesli niosa dzis tyle samo asercji, co wtedy, to przechodza dlatego, ze dziala KOD.
    # Nikt ich nie rozluznil, bo rozluznienie widac jako ubytek.
    #
    # Bez tego bieg zabity PO implementacji byl nie do wznowienia i cala jego praca szla do
    # kosza. Zmierzone na T-13 (2026-08-16): osiem kryteriow przechodzilo, osiem plikow
    # specyfikacji bajt w bajt takich, jakie certyfikowala bramka, i skrypt kazal to wyrzucic.
    note "every criterion passes and the specs still carry every assertion they had when the"
    note "gate proved them RED -- that is a working implementation, not a weakened contract"
    RESUME_CERTIFIED=1

  else
    echo >&2
    echo "$WT already carries a TASK.md and its criteria are not provably red." >&2
    if [ "$PRE_RESUME" = 1 ]; then
      echo "A criterion PASSES with no proof that it was ever red -- either the implementation" >&2
      echo "is already here without a certified contract, or an assertion was weakened." >&2
      echo "Both are a human's call (AGENTS.md section 7), and neither is safe to resume:" >&2
      echo "a second run cannot prove these criteria red." >&2
    else
      echo "The before tier exited $PRE_RESUME, which is ours to read, not the model's." >&2
    fi
    echo "Finish or discard it first:" >&2
    echo "  git worktree remove '$WT' && git branch -D '$BRANCH'" >&2
    exit 2
  fi
fi



# ------------------------------------------------- 2. bramka „before", część 1 --
# Odpalona ZANIM cokolwiek kosztuje pieniądze — bo to jedyny moment, w którym wyłapie
# zadanie bez kontraktu. Kod 2 („no acceptance criteria" / „no checks discovered") znaczy,
# że nie ma czego pilnować, i to jest nasz błąd, nie modelu: stajemy tutaj.
say "before (pre-flight)"
# Przy wznowieniu ten sam poziom przebiegl chwile temu, na TYM SAMYM drzewie i TYM SAMYM
# kontrakcie -- powtorzenie go nie jest ostroznoscia, tylko podwojna cena. Zmierzone na
# T-06 (2026-08-16): warstwa `before` kosztowala tam 840 s, bo jedno kryterium wisialo do
# konca budzetu razy dwa; potrojenie tego (wznowienie + pre-flight + `before` po zbednym
# przepisaniu kontraktu) to 42 minuty czekania na wiedze, ktora byla znana po pierwszych
# czternastu. Ponowne uzycie jest bezpieczne, bo sprawdzenie wznowienia biegnie PO kopii
# kontraktu i po odswiezeniu oracle'a -- to ten sam kontrakt, to samo drzewo, ta sama bramka.
PRE=0
# Kontrakt certyfikowany w POPRZEDNIM biegu jest certyfikowany -- dowodem jest odcisk asercji
# z tamtej chwili, sprawdzony wyzej. Bez tego bieg zabity po implementacji nie mial jak wrocic:
# jego kryteria przechodza, wiec `before` nigdy juz nie odda zera.
if [ "$RESUME_CERTIFIED" = 1 ]; then
  PRE=0
  note "the contract was certified on an earlier run of this branch -- taking it as certified"
elif [ "$PRE_RESUME_RAN" = 1 ]; then
  PRE="$PRE_RESUME"
  note "reusing the before run from the resume check (same tree, same contract): exit $PRE"
else
  gate before || PRE=$?
fi
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
 if [ "$RESUME_WITH_SPECS" = 1 ]; then
  note "the specs are already written and already judged -- skipping the contract phase"
  RED="$PRE"
 else
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
 fi

  # ---------------------------------------------- 3a. naprawa kontraktu x1 --
  # DOKLADNIE JEDNA runda, i wylacznie na jedynce. Dwojka ("bramka zle skonfigurowana")
  # i trojka ("sufit") nie sa dla modelu -- ich naprawa nalezy do orchestratora.
  #
  # DLACZEGO ten etap w ogole istnieje. Strona implementacyjna ma jedna runde naprawcza
  # od poczatku; strona kontraktowa nie miala ZADNEJ, i to nie byla niczyja decyzja, tylko
  # sposob, w jaki to uroslo. Skutek zmierzony na T-06 (2026-08-16): faza kontraktu napisala
  # siedem poprawnych specyfikacji i szkielet, w ktorym JEDNA funkcja sie zakleszcza, przez
  # co AC-2 wisialo zamiast padac. Caly bieg poszedl do kosza, worktree trzeba bylo skasowac
  # recznie, a diagnoza kosztowala noc -- za defekt, ktory bramka NAZWALA po imieniu
  # ("did not FINISH") w paragonie, i ktory da sie naprawic jednym wywolaniem modelu.
  #
  # Ta runda dostaje powody Z PARAGONU, nie z domyslu. Bramka rozroznia trzy ksztalty
  # falszywej czerwieni i kazdy ma inna naprawe; model, ktory zna nazwe swojego ksztaltu,
  # nie zgaduje.
  if [ "$RED" = 1 ]; then
    say "contract repair -- one round, then stop"
    WHY="$(python3 - "$WT/runs/last.json" <<'REASONS'
import json, sys
try:
    receipt = json.load(open(sys.argv[1], encoding="utf-8"))
except Exception:
    sys.exit(0)
for c in receipt.get("checks", []):
    if not c.get("ok"):
        print("  %s -- %s" % (c["id"], (c.get("reason") or "").replace("\n", " ")[:300]))
REASONS
)"
    printf '%s\n' "$WHY"

    # Odcisk asercji PRZED runda. Ta faza dostaje instrukcje "spraw, zeby kryterium padalo
    # INACZEJ", a najtansza droga do tego jest asertowac mniej -- i jest to jedyna faza,
    # w ktorej "asertuj mniej" jest wiarygodnym ODCZYTEM instrukcji, a nie jawnym oszustwem.
    # Dlatego dostaje obrone mechaniczna, a nie zdanie w promptcie (niezmiennik 28).
    assertion_fingerprint > "$RUNDIR/assertions-before.tsv"

    write_with "$RUNDIR/contract-repair.jsonl" 80 <<PROMPT || note "the contract repair exited nonzero; the gate decides what that was worth"
The acceptance specs in this worktree do not work as an oracle yet. Every criterion must
FAIL when \`./verify.sh before\` runs, and each failure has to be a MISSING BEHAVIOUR that
shows up at runtime. These did not qualify:

$WHY

Read runs/last.json for the full output behind each line.

Three shapes of false red, and the only repair each one allows:

  "did not FINISH -- it hung or could not start". The spec, or the skeleton it calls,
  BLOCKS. A skeleton must fail fast. The usual cause is a wait that nothing will ever
  satisfy: a channel whose receiver never sees its last sender dropped, a lock nobody
  releases, a task awaited while a live handle to it is still in scope. Fix the SKELETON
  so the call returns. Never delete the wait from the spec -- a caller that deadlocks is
  a defect of the design under test, and the spec is right to sit on it.

  "PASSES before implementation" / "exit 0 but no evidence of execution". The criterion is
  green with no implementation, so it measures nothing. If the skeleton returns the value
  the spec expects, make it return a deliberately wrong one. If instead the ASSERTION is
  too weak to tell the difference, stop and say so -- a weak assertion is a finding for
  a human (AGENTS.md section 7), never something to patch quietly.

  "did not RUN (...)". The check never executed at all: a missing module, a missing test
  target, a spec that does not compile. Add the smallest skeleton that makes it load and
  run, and nothing more.

Rules that do not bend:

- Never delete or weaken an assertion. A spec may GAIN assertions; it may never lose one.
  Making a criterion fail faster by asserting less is the exact failure this stage exists
  to prevent, and it is worse than the hang, because nothing downstream would ever notice.
  This is checked mechanically after you finish, per spec file.
- Never implement the behaviour under test. Every criterion must still be RED when you are
  done -- red because the behaviour is missing, at runtime.
- Never edit TASK.md, verify.sh, harness/, checks/ or tasks/. They are the oracle.
- Stay inside this task's \`<!-- OWNS -->\` block.
- If a criterion cannot be made to fail fast without implementing it, say so in your final
  message and change nothing. That is a finding, and it is worth more than a guess.

One shell command per Bash call. Never chain with \`;\` or \`&&\`: Claude Code splits a compound
command and asks approval for each part, and in an unattended run there is nobody to give it.

Commit what you change with one conventional-commit subject, e.g.
"fix(test): make the $ID skeleton fail fast instead of hanging".
PROMPT
    commit_leftovers "contract repair"

    assertions_must_not_shrink "$RUNDIR/assertions-before.tsv" "contract repair"

    say "before (enforced, after the contract repair)"
    RED=0; gate before || RED=$?
  fi

  if [ "$RED" -ne 0 ]; then
    echo >&2
    case "$RED" in
      1) echo "the criteria are NOT red for the right reason. Either a criterion already" >&2
         echo "passes (it checks nothing), or its check failed without running at all." >&2
         echo "Read the reasons above; both are contract bugs, not implementation bugs." >&2
         echo "One repair round already ran against exactly these reasons and did not" >&2
         echo "settle them, so this is a human's call now (AGENTS.md section 7)." >&2 ;;
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
# Odcisk kontraktu W CHWILI, W KTOREJ BRAMKA GO OSADZILA. Od tego miejsca zadna faza nie
# ma prawa zabrac specyfikacji asercji -- ani pisarz, ani runda naprawcza. To jedyny sposob,
# w jaki zielona bramka moze klamac bez sladu: kryterium dalej biegnie, dalej melduje
# "1 passed" i po prostu przestaje pytac o to, po co je napisano.
assertion_fingerprint > "$RUNDIR/assertions-certified.tsv"

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
assertions_must_not_shrink "$RUNDIR/assertions-certified.tsv" "implementation"

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
    # Podciągnij harness z trunka PRZED rundą naprawczą. Gałąź wycięta godziny temu biegnie
    # ze sprawdzeniami sprzed poprawek, które orchestrator nanosił w międzyczasie — i trzy
    # z pierwszych czterech zatrzymań pętli (2026-08-15) to były właśnie fałszywe alarmy
    # z nieaktualnej kopii checks/. Żadne nie było defektem zadania, każde kończyło się tym
    # samym ręcznym rebase i ponowną bramką.
    #
    # Merge, nie rebase: nie przepisujemy historii gałęzi, która zaraz ląduje. Tylko gdy
    # czysto — konfliktu nie zgadujemy, tylko go zgłaszamy. Moment jest bezpieczny, bo żaden
    # agent już nie pracuje: recenzent skończył, naprawiacz jeszcze nie wystartował.
    # PRZED naprawą, nie po niej: naprawiacz odpowiada na to, co powiedziała bramka, więc
    # jest tym, który najbardziej potrzebuje jej aktualnej wersji. Pracując przeciwko
    # nieaktualnej kopii może trafić w stare sprawdzenie i przewrócić się na nowym.
    # HARNESS, nie KONTRAKT. To rozróżnienie kosztowało jeden bieg (T-04, 2026-08-15, 82 minuty
    # i 36 dolarów). Merge zaciągnął `tasks/T-04.md` poprawiony w międzyczasie na trunku, podczas
    # gdy `TASK.md` gałęzi jest zamrożony przy commicie kontraktowym — i N-08 słusznie zatrzymał
    # bramkę kodem 2 na rozjeździe, którego nie zrobił ani pisarz, ani naprawiacz.
    #
    # Zamrożenie kontraktu nie jest formalnością: bieg nie może zmieniać warunków własnego
    # zaliczenia. Ulepszony plik zadania obowiązuje NASTĘPNY bieg, nie ten. Więc po merge'u
    # przywracamy `tasks/` do wersji gałęzi — trunk daje nam sprawdzenia, nie nową umowę.
    refresh_harness_from_trunk 1 "before the repair round"

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

    assertions_must_not_shrink "$RUNDIR/assertions-certified.tsv" "repair"
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
