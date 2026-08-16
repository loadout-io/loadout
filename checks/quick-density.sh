#!/usr/bin/env bash
# Sufit gęstości z docs/ARCHITECTURE.md §7, egzekwowany zamiast zadeklarowanego.
#
# Niezmiennik 18: sufit jest MIERZONY, nie oceniany okiem, a zapadka może tylko maleć.
# poprzedni prototyp ustawił swój próg po fakcie i zamarzł na 29 regionach przy limicie 12 — 2,4×
# wartości docelowej `[03 §4.1]`. Zapadka ustawiona po fakcie jest zawsze ustawiona tam,
# gdzie akurat jesteś.
#
# CZTERY STANY ŚWIATA, CZTERY KODY WYJŚCIA. To jest cała treść tego pliku, bo kod wyjścia
# jest jedyną rzeczą, którą bramka naprawdę czyta:
#
#   0  zmierzone, pod sufitem i pod zapadką
#   0  NIE MA CZEGO mierzyć — z nazwanym, mechanicznym warunkiem, nigdy w milczeniu
#   1  za gęsto (sufit) albo powyżej zapadki (regres) — dwie różne odmowy, dwa różne zdania
#   2  NIE DAŁO SIĘ zmierzyć
#
# Zlanie `0 (nie ma czego)` z `2 (nie dało się)` jest awarią, która wygląda jak sukces
# i utrzymuje się latami: bramka melduje zielono na maszynie, na której nic nie policzono.
# poprzedni prototyp opublikował dokładnie to — "czysty przebieg axe", który nie zmierzył niczego.
#
# NIEZMIENNIK 19: kod wyjścia to nie dowód. Każde zielone wyjście niżej wypisuje, CO zostało
# zmierzone i ILE tego było. `bash checks/quick-density.sh; echo $?` z zerem nie znaczy nic.
#
# NIEZMIENNIK 21: `checks/density-baseline.json` jest CZYTANY przy każdym biegu, nie tylko
# zapisywany przez `--update-baseline`. Plik zapisywany i nieczytany to artefakt, którego
# nikt nigdy nie otworzył.
#
# NIEZMIENNIK 23 / 18, druga połowa: siedmiu liczb sufitu NIE MA w tym pliku ani w
# `scripts/density-audit.mjs`. Są parsowane z docs/ARCHITECTURE.md §7 przy każdym wywołaniu.
# Druga kopia oznacza, że po pierwszej edycji dokumentu bramka pilnuje liczby, której już
# nikt nie deklaruje.
#
# Sędzia mieszka w `scripts/density-audit.mjs` i jest czystą funkcją nad zrzutem JSON. Ten
# skrypt jest adapterem: znajduje zrzut, czyta zapadkę, woła sędziego, tłumaczy werdykt na
# kod wyjścia. Pięć linii polityki tutaj byłoby przepisaniem polityki w adapterze.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

AUDIT="scripts/density-audit.mjs"
DOC="docs/ARCHITECTURE.md"
BASELINE="checks/density-baseline.json"
ENTRY="src/main.tsx"

update=0
for arg in "$@"; do
  case "$arg" in
    --update-baseline) update=1 ;;
    *)
      echo "quick-density: the only argument this check takes is --update-baseline" >&2
      echo "detail: got \"$arg\"" >&2
      exit 2
      ;;
  esac
done

# ── najpierw istnienie artefaktu, z WŁASNYM zdaniem ────────────────────────────────────────
#
# `ENOENT`, `No such file or directory`, `cannot find module` i `command not found` stoją na
# liście NOT_A_REAL_RED w harness/gate.py. Brakujący plik, o którym powie bash albo node,
# daje w tierze `before` czerwień, którą bramka odrzuca ze zdaniem "did not RUN" — czyli
# rundę, która nic nie poświadcza. Mówimy to więc sami, zdaniem bez ani jednego z tych podpisów.
if [ ! -f "$AUDIT" ]; then
  echo "the density judge is absent from this tree ($AUDIT), so there is nothing to judge with" >&2
  exit 2
fi
if [ ! -f "$DOC" ]; then
  echo "the only source of the seven density limits is absent from this tree ($DOC §7)" >&2
  exit 2
fi
if ! command -v node >/dev/null 2>&1; then
  echo "the density judge is a node module and node does not answer here" >&2
  exit 2
fi

# ── skąd bierze się zrzut ──────────────────────────────────────────────────────────────────
#
# `LOADOUT_DENSITY_SNAPSHOT` jest SZWEM między kolektorem a sędzią, nie furtką testową.
# Kolektor biegnie w przeglądarce i kryterium akceptacji mieć nie może: `Failed to launch`
# i `Executable doesn't exist` są na liście NOT_A_REAL_RED, więc na maszynie bez pobranych
# przeglądarek dałby w `before` czerwień, którą bramka odrzuca, a w `full` zieleń, która nic
# nie znaczy. Sędzia dostaje gotowy zrzut i to jego sądzą kryteria.
snapshot="${LOADOUT_DENSITY_SNAPSHOT:-}"
if [ -n "$snapshot" ]; then
  if [ ! -f "$snapshot" ]; then
    echo "LOADOUT_DENSITY_SNAPSHOT names a path this tree does not hold: $snapshot" >&2
    exit 2
  fi
elif [ ! -f "$ENTRY" ]; then
  # Warunek pominięcia jest MECHANICZNY i nazwany: pierwszy plik pod $ENTRY włącza
  # sprawdzenie z powrotem, bez niczyjej decyzji. To jest wyjście "nie ma czego mierzyć",
  # i musi się czytać inaczej niż "nie dało się" niżej.
  echo "density: nothing to measure — this tree has no $ENTRY, so it renders no default view"
  echo "density: 0 metrics measured, 0 compared against the ceiling in $DOC §7"
  exit 0
else
  # Jest co mierzyć i pomiaru NIE MA. To nie jest zieleń i nie jest czerwień o kodzie —
  # to jest brak wyniku, czyli dokładnie ten stan, który poprzedni prototyp opublikował jako
  # "czysty przebieg". Kod 2 mówi bramce: ten poziom nie osądził kodu.
  echo "density: could not measure — $ENTRY exists, so this tree renders a default view," >&2
  echo "detail: but no measurement of it reached this check. Two ways to give it one:" >&2
  echo "detail:   LOADOUT_DENSITY_SNAPSHOT=<plik.json> — a snapshot the collector already took" >&2
  echo "detail:   npm run build, then run the in-browser collector over dist/" >&2
  echo "detail: the collector leg is not wired into this check yet; until it is, this exit" >&2
  echo "detail: is the honest answer. A measurement nobody took is not a measurement of 0." >&2
  exit 2
fi

# ── werdykt ────────────────────────────────────────────────────────────────────────────────
RUNNER=$(cat <<'JS'
import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const root = process.env.LOADOUT_DENSITY_ROOT;
const snapshotPath = process.env.LOADOUT_DENSITY_SNAPSHOT_FILE;
const docPath = process.env.LOADOUT_DENSITY_DOC;
const baselinePath = process.env.LOADOUT_DENSITY_BASELINE;
const update = process.env.LOADOUT_DENSITY_UPDATE === '1';

/** Nasza konfiguracja jest zepsuta, nie mierzony kod. 2 nigdy nie jest czerwienią. */
function cannotMeasure(message) {
  process.stderr.write(`${message}\n`);
  process.exit(2);
}

const { judge, readCeiling } = await import(
  pathToFileURL(`${root}/scripts/density-audit.mjs`).href
);

let snapshot;
try {
  snapshot = JSON.parse(readFileSync(snapshotPath, 'utf8'));
} catch (error) {
  cannotMeasure(`density: the snapshot at ${snapshotPath} is not readable JSON (${error.message})`);
}

let ceiling;
try {
  ceiling = readCeiling(docPath);
} catch (error) {
  // readCeiling odmawia PO NAZWIE brakującego wiersza i nigdy nie zwraca wartości domyślnej.
  // Domyślna wartość w tym miejscu to ta sama cicha awaria, tylko o warstwę wyżej.
  cannotMeasure(error.message);
}

// NIEZMIENNIK 21: zapadka jest czytana przy KAŻDYM biegu. Brak pliku to pusta zapadka —
// każda metryka jest wtedy pierwszym pomiarem, a pierwszy pomiar zawsze wolno przyjąć.
let baseline = {};
if (existsSync(baselinePath)) {
  try {
    baseline = JSON.parse(readFileSync(baselinePath, 'utf8'));
  } catch (error) {
    cannotMeasure(`density: the ratchet at ${baselinePath} is not readable JSON (${error.message})`);
  }
}

const verdict = judge(snapshot, ceiling, baseline);
const widths = (Array.isArray(snapshot.widths) ? snapshot.widths : [])
  .map((at) => at?.width)
  .filter((w) => typeof w === 'number');

// NIEZMIENNIK 19: co zmierzone i ile tego było — na KAŻDEJ ścieżce, także odmownej.
const seen = Object.keys(verdict.measured).length;
process.stdout.write(
  `density: measured ${seen} of ${ceiling.length} metrics at ` +
    `${widths.length > 0 ? widths.map((w) => `${w} px`).join(' and ') : 'no width at all'}` +
    ', taking the worse of the two per metric\n',
);
if (seen > 0) {
  process.stdout.write(
    `density: ${ceiling
      .filter((entry) => entry.key in verdict.measured)
      .map((entry) => `${entry.key} ${verdict.measured[entry.key]}/${entry.limit}`)
      .join(', ')}\n`,
  );
}
// Powód zapisany, nie zero. "Osie nawigacji" są osądem człowieka i nigdy nie będą liczbą —
// to ma być powiedziane przy KAŻDYM biegu, a nie ukryte pod wartością wyglądającą jak pomiar.
for (const [metric, reason] of Object.entries(verdict.reasons)) {
  process.stdout.write(`not measured: ${metric} — ${reason}\n`);
}

// ── odmowy: trzy różne, bo są to trzy różne rzeczy do zrobienia przez człowieka ──
if (verdict.verdict === 'over') {
  process.stderr.write(`density: over the ceiling declared in ${docPath} §7\n`);
  for (const entry of verdict.over) {
    process.stderr.write(
      `  ${entry.metric} measured ${entry.measured}, ceiling ${entry.limit}` +
        ` (over by ${entry.measured - entry.limit})\n`,
    );
  }
  process.stderr.write('detail: this one does not enter the product. The limit is not the\n');
  process.stderr.write('detail: thing to negotiate — poprzedni prototyp raised its own to 2.4x and\n');
  process.stderr.write('detail: ended with 149 px of chrome on every screen [03 §4.1].\n');
  process.exit(1);
}

if (verdict.verdict === 'regressed') {
  process.stderr.write(`density: the baseline may only shrink, never grow (invariant 18)\n`);
  for (const entry of verdict.regressed) {
    process.stderr.write(
      `  ${entry.metric} measured ${entry.measured}, baseline ${entry.baseline}\n`,
    );
  }
  process.stderr.write('detail: this is NOT the "too dense" refusal — every metric above is\n');
  process.stderr.write('detail: still under its ceiling. You went backwards from the last\n');
  process.stderr.write('detail: measurement. Lower it again, or lower the baseline on purpose\n');
  process.stderr.write(`detail: with --update-baseline once the number really came down.\n`);
  process.exit(1);
}

if (verdict.verdict === 'unmeasured') {
  process.stderr.write('density: a metric was not measured and the collector stated no reason\n');
  for (const metric of verdict.unexplained) {
    process.stderr.write(`  ${metric}\n`);
  }
  process.stderr.write('detail: a metric nobody measured, written as 0 and compared against a\n');
  process.stderr.write('detail: ceiling, is green forever. Either measure it, or have the\n');
  process.stderr.write('detail: collector say in notMeasured why it cannot be measured.\n');
  process.exit(1);
}

// ── zapadka schodzi w dół i nigdy w górę ──
//
// Zapis WYŁĄCZNIE stąd, czyli wyłącznie po werdykcie `pass`. Wszystkie trzy odmowy wyżej
// kończą proces, więc żadna z nich nie może zapisać pliku: odmowa, która mimo wszystko
// zapisuje, zabetonowałaby regres i wyglądałaby przy tym jak działająca obrona.
if (update) {
  const next = {};
  for (const entry of ceiling) {
    const fresh = verdict.measured[entry.key];
    const kept = baseline?.[entry.key];
    const value = typeof fresh === 'number' ? fresh : kept;
    if (typeof value !== 'number') continue;
    // Zapadka nie ma prawa zabetonować stanu GORSZEGO niż deklarowany. Ta gałąź jest
    // nieosiągalna przez `verdict === 'pass'` i stoi tu jako własność pliku, nie ścieżki kodu.
    if (value > entry.limit) {
      cannotMeasure(
        `density: refusing to write ${entry.key}=${value} into the ratchet — ` +
          `${docPath} §7 declares ${entry.limit}`,
      );
    }
    next[entry.key] = value;
  }
  // Atomowo: bramka bywa ubijana sygnałem w połowie zapisu, a zapadka przycięta w połowie
  // nie jest JSON-em i następny bieg wyjdzie na niej dwójką.
  const tmp = `${baselinePath}.tmp`;
  writeFileSync(tmp, `${JSON.stringify(next, null, 2)}\n`, 'utf8');
  renameSync(tmp, baselinePath);
  process.stdout.write(`density: baseline rewritten at ${baselinePath}\n`);
}

process.stdout.write(
  `density: under the ceiling in ${docPath} §7 and under the ratchet in ${baselinePath}\n`,
);
JS
)

LOADOUT_DENSITY_ROOT="$ROOT" \
LOADOUT_DENSITY_SNAPSHOT_FILE="$snapshot" \
LOADOUT_DENSITY_DOC="$DOC" \
LOADOUT_DENSITY_BASELINE="$BASELINE" \
LOADOUT_DENSITY_UPDATE="$update" \
  node --input-type=module -e "$RUNNER"
