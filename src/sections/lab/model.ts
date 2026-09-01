/* Czysty model tabeli: wszystko, co da się rozstrzygnąć bez ekranu, rozstrzyga się TUTAJ.
 *
 * DLACZEGO OSOBNY PLIK, A NIE CIAŁO KOMPONENTU. To repo nie ma jsdom, więc kliknięcia nie da
 * się odpalić w teście, a `renderToStaticMarkup` nie uruchamia efektów. Reguła zamknięta
 * w komponencie byłaby regułą, której żadne kryterium nie umie dotknąć — a to jest ta sama
 * rodzina wad, z której wzięło się siedemnaście kłamiących kontrolek. Tutaj test woła to samo,
 * co rysuje ekran.
 *
 * CZEGO TU NIE MA: liczenia wyniku. Kto przeszedł, a kto nie, rozstrzyga `lab::results` po
 * stronie Rusta, na plikach biegu. Druga odpowiedź na to samo pytanie, wyliczona w oknie,
 * byłaby tą, która rozjeżdża się po pierwszej zmianie po tamtej stronie — i nikt by tego nie
 * zauważył, bo obie wyglądają jak tabela.
 */
import type { CellOutcome, EvalBoard, EvalCase, EvalCell, EvalSet, PastEval } from './io';

/** Znak, którym komórka mówi, jak się skończyła. */
export const MARKS: Readonly<Record<CellOutcome, string>> = {
  passed: '✓',
  'did-not-pass': '✗',
  'not-judged': '·',
};

/** Jedna komórka gotowa do narysowania. */
export interface CellView {
  readonly caseId: string;
  readonly variantId: string;
  readonly outcome: CellOutcome;
  readonly mark: string;
  /** Zdanie za kliknięciem: dlaczego tak. Puste przy przejściu. */
  readonly said: string;
  /** `$0.42`, albo pusty napis, kiedy nikt nie podał ceny. */
  readonly spend: string;
}

/** Jeden wiersz tabeli. */
export interface RowView {
  readonly caseId: string;
  readonly name: string;
  readonly cells: readonly CellView[];
}

/** Cała tabela. */
export interface TableView {
  readonly columns: readonly { readonly id: string; readonly name: string }[];
  readonly rows: readonly RowView[];
}

/**
 * Cena po ludzku. Pusty napis, kiedy nikt jej nie podał — **nigdy `$0.00`**.
 *
 * Zero jest liczbą i czyta się jak „nic nie kosztowało"; brak jest brakiem. To samo
 * rozróżnienie stoi po stronie Rusta przy `engine::drivers::Outcome::cost_usd`, i musi stać
 * po obu stronach granicy, bo inaczej jedna z nich zmyśla drugiej odpowiedź.
 */
export function spendOf(costUsd: number | null): string {
  if (costUsd === null) return '';
  return '$' + costUsd.toFixed(2);
}

/** Wiersze, które naprawdę mierzą: kandydatki czekają obok tabeli, nie w niej. */
export function runningCases(set: EvalSet): readonly EvalCase[] {
  return set.cases.filter((one) => one.status === 'in-use');
}

/** Kandydatki czekające na człowieka. */
export function suggestedCases(set: EvalSet): readonly EvalCase[] {
  return set.cases.filter((one) => one.status === 'suggested');
}

/**
 * Klucz komórki w mapie wyników: wiersz i kolumna razem.
 *
 * `JSON.stringify` NAD PARĄ, a nie sklejenie separatorem, i to nie jest ostrożność na zapas.
 * Sklejenie wymaga znaku, którego w żadnym identyfikatorze nie ma — czyli reguły, która żyje
 * w innym pliku (`lab::plan::APART` po stronie Rusta) i której ten kod nie egzekwuje.
 * Zakodowana para nie potrzebuje żadnej reguły: dwie różne pary dają dwa różne napisy,
 * cokolwiek w nich stoi.
 *
 * 2026-08-31 — POWSTAŁO Z WADY, KTÓRA TU STAŁA. Separatorem był bajt zerowy, wpisany do
 * źródła przez pomyłkę zamiast spacji. Działało, bo obie strony używały tego samego — i to
 * jest dokładnie powód, dla którego żadne kryterium tego nie zobaczyło. Zobaczył git: plik
 * z bajtem zerowym przestaje być tekstem, więc nie ma diffu, nie ma scalania i nie ma recenzji.
 */
function keyOf(row: string, column: string): string {
  return JSON.stringify([row, column]);
}

/**
 * Składa tabelę z zestawu i JEDNEGO przebiegu.
 *
 * Kształt bierze się z ZESTAWU, nie z przebiegu, i to jest treść: tabela ma tyle wierszy, ile
 * ma zestaw dzisiaj. Przebieg sprzed dopisania wiersza pokazuje w nim „nie zmierzono" zamiast
 * znikać — a wtedy człowiek czyta prawdę: ten wiersz jest nowszy niż tamten przebieg.
 *
 * `null` w miejscu przebiegu jest normalnym stanem świeżego zestawu i daje tabelę pustych
 * komórek. Pusta tabela jest lepsza od jej braku: pokazuje, o co Loadout zapyta po Run.
 */
export function tableFor(set: EvalSet, run: PastEval | null): TableView {
  const found = new Map<string, EvalCell>(
    (run?.cells ?? []).map((cell) => [keyOf(cell.case, cell.variant), cell]),
  );
  return {
    columns: set.variants.map((one) => ({ id: one.id, name: one.name })),
    rows: runningCases(set).map((one) => ({
      caseId: one.id,
      name: one.name,
      cells: set.variants.map((variant) => {
        const cell = found.get(keyOf(one.id, variant.id)) ?? null;
        const outcome: CellOutcome = cell?.outcome ?? 'not-judged';
        return {
          caseId: one.id,
          variantId: variant.id,
          outcome,
          mark: MARKS[outcome],
          said: cell?.said ?? '',
          spend: spendOf(cell?.costUsd ?? null),
        };
      }),
    })),
  };
}

/**
 * Nagłówek nad tabelą: wynik, różnica wobec poprzedniego przebiegu i wydatek.
 *
 * Jeden napis, składany raz. Trzy osobne pola rysowane w trzech miejscach byłyby trzema
 * odpowiedziami na jedno pytanie „jak poszło" (niezmiennik 13), a rozjechałoby się to, które
 * ktoś zapomni przestawić.
 */
export function scoreOf(board: EvalBoard): string {
  const [newest] = board.runs;
  if (newest === undefined) return 'Not run yet';
  const parts = [String(newest.passed) + ' of ' + String(newest.judged) + ' passed'];
  const movement = board.movement;
  if (movement !== null && (movement.gained > 0 || movement.lost > 0)) {
    const said: string[] = [];
    if (movement.gained > 0) said.push('+' + String(movement.gained));
    if (movement.lost > 0) said.push('−' + String(movement.lost));
    parts.push(said.join(' ') + ' since the run before');
  }
  const spend = spendOf(newest.costUsd);
  if (spend !== '') parts.push(spend);
  return parts.join(' · ');
}

/**
 * Linia trendu: po jednym udziale przejść na przebieg, od najstarszego.
 *
 * Osobno od tabeli, bo odpowiada na inne pytanie. Tabela mówi „jak jest teraz", trend mówi
 * „czy się poprawia" — i tylko drugie z nich odpowiada na pytanie, dla którego ta sekcja
 * powstała. Przebieg, w którym nic nie zmierzono, nie ma udziału i nie ma go w linii: zero
 * z zera narysowane jako zero byłoby spadkiem, którego nie było.
 */
export function trendOf(runs: readonly PastEval[]): readonly number[] {
  return [...runs]
    .reverse()
    .filter((run) => run.judged > 0)
    .map((run) => run.passed / run.judged);
}
