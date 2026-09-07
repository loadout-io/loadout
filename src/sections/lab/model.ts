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
  readonly elapsed?: string;
  readonly costNote?: string;
  readonly source?: { readonly workspace: string; readonly runFolder: string };
}

/** Jedna rzecz, której ten wiersz żąda: podpis i treść. */
export interface AsksFor {
  readonly label: string;
  readonly value: string;
}

/** Jeden wiersz tabeli. */
export interface RowView {
  readonly caseId: string;
  readonly repeat?: number;
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
  return (
    runningCases(set).reduce((sum, one) => sum + repetitions(set, one), 0) * set.variants.length
  );
}

function repetitions(set: EvalSet, one: EvalCase): number {
  return set.subject.kind === 'workflow' &&
    Number.isInteger(one.repeats) &&
    (one.repeats ?? 0) >= 1 &&
    (one.repeats ?? 0) <= 20
    ? one.repeats!
    : 1;
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
 * Pod jakimi warunkami zmierzono TEN przebieg — albo pusty napis, gdy nie ma z czego to wziąć.
 *
 * # Po co, skoro obok stoi panel o ochronie
 *
 * Panel czyta DZISIEJSZY zapis zestawu, więc mówi o biegu, którego jeszcze nie było. Po
 * przestawieniu ochrony jego zdanie stawało nad macierzą zmierzoną na odwrotnych warunkach:
 * „File access is restricted…" nad wynikiem policzonym bez żadnej granicy, i odwrotnie.
 * „Przeszło" pod pilnowanym dostępem do plików i „przeszło" bez niego to nie jest ten sam
 * fakt, a człowiek nie miał przy wyniku ani jednego zdania, które by je rozróżniało.
 *
 * PUSTY NAPIS, GDY PRZEBIEG NIE NIESIE DEFINICJI: starszy zapis nie wie, jak go mierzono,
 * a zdanie zgadnięte z dzisiejszego zestawu jest dokładnie tą wadą, którą to naprawia.
 */
export function howItWasJudged(run: PastEval | null): string {
  const measured = run?.definition ?? null;
  if (measured === null) return '';
  if (measured.subject.kind !== 'workflow') return '';
  if (measured.protected === true) {
    return (
      'This run was measured with file access restricted; trusted external checks judged the ' +
      'result.'
    );
  }
  return (
    'This run was measured as a diagnostic comparison: the checks ran outside the workflow, ' +
    'but file access was not restricted.'
  );
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
function keyOf(row: string, column: string, repeat: number): string {
  return JSON.stringify([row, column, repeat]);
}

/**
 * Definicja, KTÓRĄ ZMIERZONO ten przebieg — a dla zestawu bez przebiegu dzisiejszy formularz.
 *
 * Jedno miejsce na całą sekcję, bo tę samą odpowiedź czyta tabela i lista pod tabelą. Póki
 * regułę znała wyłącznie tabela, jeden wynik miał na jednym ekranie dwa podpisy: macierz
 * nazywała komórkę tak, jak nazywała się w chwili pomiaru, a lista „What did not pass"
 * dzisiejszym formularzem. Po przemianowaniu kolumny człowiek czytał dwie nazwy tej samej
 * rzeczy w odległości trzech centymetrów, a po jej usunięciu — surowy identyfikator z drutu.
 */
export function definitionOf(set: EvalSet, run: PastEval | null): EvalSet {
  return run?.definition ?? set;
}

/**
 * Składa tabelę z zestawu i JEDNEGO przebiegu.
 *
 * 2026-09-05: wynik historyczny musi pokazywać historyczne kryteria. Dzisiejszy formularz
 * jest wejściem przyszłego pomiaru, nie opisem pomiaru już wykonanego.
 *
 * `null` w miejscu przebiegu jest normalnym stanem świeżego zestawu i daje tabelę pustych
 * komórek. Pusta tabela jest lepsza od jej braku: pokazuje, o co Loadout zapyta po Run.
 */
export function tableFor(set: EvalSet, run: PastEval | null): TableView {
  const measured = definitionOf(set, run);
  const found = new Map<string, EvalCell>(
    (run?.cells ?? []).map((cell) => [
      keyOf(cell.case, cell.variant, cell.execution?.repeat ?? 0),
      cell,
    ]),
  );
  return {
    columns: measured.variants.map((one) => ({ id: one.id, name: one.name })),
    rows: runningCases(measured).flatMap((one) =>
      Array.from({ length: repetitions(measured, one) }, (_, repeat) => ({
        caseId: one.id,
        repeat,
        name: rowNameOf(measured, one, repeat),
        asks: whatItAsks(one),
        cells: measured.variants.map((variant) => {
          const cell = found.get(keyOf(one.id, variant.id, repeat)) ?? null;
          const outcome: CellOutcome = cell?.outcome ?? 'not-judged';
          return {
            caseId: one.id,
            variantId: variant.id,
            outcome,
            mark: MARKS[outcome],
            said: cell?.said ?? '',
            spend:
              (cell?.execution?.costPartial === true && cell.costUsd !== null ? 'At least ' : '') +
              spendOf(cell?.costUsd ?? null),
            elapsed:
              cell?.execution?.elapsedMs == null
                ? ''
                : (cell.execution.elapsedMs / 1000).toFixed(1) + ' s',
            costNote:
              cell?.execution?.costPartial === true ? 'Some agents did not report a price.' : '',
            ...(cell?.execution != null && run?.workspace
              ? {
                  source: { workspace: run.workspace, runFolder: run.folder },
                }
              : {}),
          };
        }),
      })),
    ),
  };
}

/**
 * Podpis wiersza: nazwa przypadku, a przy powtórzeniach także numer powtórzenia.
 *
 * Numer dokleja się WYŁĄCZNIE tam, gdzie powtórzeń jest więcej niż jedno. „· Repeat 1" przy
 * przypadku, który biegł raz, jest rozróżnieniem, które niczego nie rozróżnia.
 */
function rowNameOf(measured: EvalSet, one: EvalCase, repeat: number): string {
  return one.name + (repetitions(measured, one) > 1 ? ' · Repeat ' + String(repeat + 1) : '');
}

/**
 * Jak nazywa się TA komórka poza tabelą: wiersz, powtórzenie i kolumna, jednym napisem.
 *
 * TĄ SAMĄ FUNKCJĄ, CO WIERSZ MACIERZY, i to jest cały powód, dla którego nazwa powstaje tutaj,
 * a nie w komponencie listy. Lista „What did not pass" miała własną kopię tej reguły i kopia
 * nie znała powtórzeń: dwie nieudane próby tego samego przypadku w tej samej kolumnie
 * dostawały ten sam podpis dwa razy, podczas gdy tabela nad nimi mówiła „· Repeat 1"
 * i „· Repeat 2". Z takiej listy nie da się dojść, które powtórzenie padło.
 *
 * `measured` jest definicją Z CHWILI POMIARU (`definitionOf`), nie dzisiejszym formularzem:
 * identyfikatory w komórkach są historyczne, więc szukanie ich w dzisiejszym zestawie mówi
 * nową nazwę pod starym wynikiem, a po usunięciu kolumny nie znajduje nic i spada na
 * identyfikator, którego człowiek nigdy nie napisał.
 */
export function nameOfCell(measured: EvalSet, cell: EvalCell): string {
  const repeat = cell.execution?.repeat ?? 0;
  const one = measured.cases.find((row) => row.id === cell.case);
  const row = one === undefined ? cell.case : rowNameOf(measured, one, repeat);
  const column = measured.variants.find((it) => it.id === cell.variant)?.name ?? cell.variant;
  return row + ' · ' + column;
}

/**
 * Czym ta komórka różni się od pozostałych — ten sam napis, którym tabela szuka jej w mapie.
 *
 * Lista pod tabelą kluczowała wiersze samą parą przypadek-kolumna, więc dwa powtórzenia tej
 * samej pary były dla Reacta jednym wierszem. Powtórzenie jest częścią tożsamości komórki
 * wszędzie indziej (`keyOf`, `lab::results`), więc jest nią i tutaj.
 */
export function keyOfCell(cell: EvalCell): string {
  return keyOf(cell.case, cell.variant, cell.execution?.repeat ?? 0);
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
  const fingerprint = runs[0]?.comparisonFingerprint;
  if (fingerprint == null) return [];
  return [...runs]
    .reverse()
    .filter((run) => run.judged > 0 && run.comparisonFingerprint === fingerprint)
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
