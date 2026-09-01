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

/** Jedna rzecz, której ten wiersz żąda: podpis i treść. */
export interface AsksFor {
  readonly label: string;
  readonly value: string;
}

/** Jeden wiersz tabeli. */
export interface RowView {
  readonly caseId: string;
  readonly name: string;
  /**
   * Czego ten wiersz żąda — do przeczytania z tabeli, bez otwierania pliku zestawu.
   *
   * Do 2026-08-31 `task`, `expect`, `command` i `proof` leżały w modelu i NIE MIAŁY DROGI NA
   * EKRAN: wiersz był `<th>` z samą nazwą, bez handlera i bez `title`. Człowiek patrzył na
   * `✗` i nie miał jak sprawdzić, czego właściwie ta komórka chciała.
   *
   * WYŁĄCZNIE POLA, KTÓRE COŚ MÓWIĄ (niezmiennik 17). Przypadek bez komendy nie dostaje wiersza
   * „Command: —"; kreska w miejscu wartości jest kształtem faktu, którego nie ma.
   */
  readonly asks: readonly AsksFor[];
  readonly cells: readonly CellView[];
}

/** Cała tabela. */
export interface TableView {
  readonly columns: readonly { readonly id: string; readonly name: string }[];
  readonly rows: readonly RowView[];
}

/** `1 case`, `3 cases` — liczebnik przy rzeczowniku, w jednym miejscu na całą sekcję. */
export function count(how: number, one: string, many: string): string {
  return String(how) + ' ' + (how === 1 ? one : many);
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
 * Ile komórek zamawia ten zestaw dzisiaj: wiersze razy kolumny.
 *
 * Z ZESTAWU, nie z przebiegu, bo pytanie pada wtedy, gdy przebiegu jeszcze nie ma — w chwili
 * naciśnięcia `Run`. Obie liczby stoją w pliku zestawu, więc żadna z nich nie jest zmyślona.
 */
export function howManyCells(set: EvalSet): number {
  return runningCases(set).length * set.variants.length;
}

/**
 * Ile komórek tego przebiegu NIKT nie zmierzył.
 *
 * ARYTMETYKA NA ODPOWIEDZI RUSTA, nie druga odpowiedź na to samo pytanie. `judged` liczy
 * `lab::results` na plikach biegu, a `cells` jest całą macierzą, którą stamtąd dostaliśmy —
 * więc różnica jest tym, co Rust już powiedział, tylko wypowiedzianym wprost. Policzenie tego
 * po `outcome` w oknie byłoby drugim licznikiem, który rozjedzie się po pierwszej zmianie tamtej
 * strony i nikt tego nie zauważy, bo obie liczby wyglądają jak liczba.
 */
export function notMeasured(run: PastEval): number {
  return Math.max(0, run.cells.length - run.judged);
}

/**
 * Zdanie o tym, jak skończył się CAŁY przebieg — albo pusty napis, gdy nie ma czego mówić.
 *
 * # Po co to istnieje: „0 of 3 passed" nad sześcioma wierszami
 *
 * Zmierzone na zrzucie właściciela 2026-08-31. Aplikacja zginęła w połowie biegu, uzgodnienie
 * wpisało trzem pracującym krokom `failed`, `lab::results` nie ma `failed` na liście stanów
 * nieosądzonych — i Loadout policzył WŁASNE ZAMKNIĘCIE jako trzy porażki agenta, po czym
 * wystawił za to zero procent. Liczba była prawdziwa wobec swojej definicji i bezużyteczna
 * wobec pytania, które człowiek zadał.
 *
 * NOŚNIK BYŁ NA MIEJSCU OD POCZĄTKU i nie miał ani jednego czytelnika: `PastEval.state` niesie
 * słowo o całym biegu, a odzyskiwanie po awarii wpisuje tam `interrupted`. Tłumaczenie tego
 * słowa na zdanie należy do okna (niezmiennik 14), więc stoi tutaj, a nie po tamtej stronie.
 *
 * CZEGO TO NIE ROBI: nie zmienia wyniku ani jednej komórki. Kto przeszedł, rozstrzyga Rust;
 * to zdanie mówi tylko, czy przebieg, który tak policzono, w ogóle dobiegł końca.
 */
export function howItEnded(run: PastEval): string {
  switch (run.state) {
    case 'interrupted':
      return (
        'Loadout closed while this run was still going, so it never finished. ' +
        'Press Run to measure the whole set again.'
      );
    case 'cancelled':
      return 'You stopped this run before it finished. Press Run to measure the whole set again.';
    case 'running':
    case 'paused':
      return 'This run is still going. Pick this set again on the left to see how far it got.';
    default:
      return '';
  }
}

/**
 * Czego ten przypadek żąda, w kolejności czytania i bez pól, których nie ma.
 *
 * `expect` schodzi do jednego wiersza, bo jego rolą jest powiedzieć, CO ma paść w odpowiedzi —
 * a nie odtworzyć kształt pliku. Oczekiwanie bez `contains` mówi wyłącznie „to pole ma być"
 * i tak też się je pisze.
 */
export function whatItAsks(one: EvalCase): readonly AsksFor[] {
  const said = one.expect
    .map((want) => (want.contains.trim() === '' ? want.field : want.field + ': ' + want.contains))
    .filter((line) => line.trim() !== '')
    .join(' · ');
  return [
    { label: 'Asks the agent to', value: one.task.trim() },
    { label: 'And to answer with', value: said },
    { label: 'Then runs', value: one.command.trim() },
    { label: 'And looks for', value: one.proof.trim() },
    { label: 'Drafted from', value: one.because.trim() },
  ].filter((row) => row.value !== '');
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
      asks: whatItAsks(one),
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
  /* ILE KOMÓREK NIKT NIE ZMIERZYŁ — obok wyniku, nie zamiast niego. Bez tego członu „0 of 3"
   * nad sześcioma wierszami czyta się jako rozmiar tego, co widać, a trzy kropki obok trzech
   * krzyżyków jako „nic tam nie ma". Człon znika, kiedy zmierzono wszystko: zero pisane wprost
   * jest liczbą, która niczego nie dodaje, a zabiera miejsce w wierszu czytanym jednym rzutem. */
  const missed = notMeasured(newest);
  if (missed > 0) parts.push(String(missed) + ' not measured');
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

/**
 * Co jest teraz jedyną rzeczą do zrobienia — i co ma nieść akcent.
 *
 * # Zmierzone na człowieku, 2026-08-31
 *
 * Właściciel trzy razy pod rząd napisał „nie kumam, jak to działa", stojąc nad ekranem,
 * który mówił mu wprost, co nacisnąć. Zdanie było, tylko akcent leżał na `Run` — dużym,
 * kolorowym i **wygaszonym** — a jedyna możliwa czynność stała obok jako cichy obrys.
 *
 * Ekran krzyczał o rzeczy niemożliwej i szeptał o jedynej możliwej. To ta sama rodzina, co
 * cała reszta wad tej sekcji: kontrolka mówiąca co innego, niż jest prawdą — tylko wyrażona
 * wagą, a nie słowem.
 */
export function theNextMoveIs(cannotRun: string | null): 'write-cases' | 'run' {
  return cannotRun === null ? 'run' : 'write-cases';
}
