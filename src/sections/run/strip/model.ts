/* Pasek loadoutu: workflow jako ciąg bloków, jeden na krok [DESIGN §2].
 *
 * Dwie rzeczy są tu wiążące i obie łamią się po cichu.
 *
 * Bloków jest DOKŁADNIE tyle, ile bieg ma kroków. Cztery na stałe, „bo makieta ma cztery",
 * to interfejs rysujący relację, której nie ma w danych (niezmiennik 17).
 *
 * Bloków `now` może być KILKA. Jeden kursor `currentIndex` przechodzi każdy test na biegu
 * sekwencyjnym i kłamie w pierwszym biegu równoległym — a równoległość jest całą przesłanką
 * tego produktu (niezmiennik 11). Stan bloku jest więc funkcją stanu kroku, nie pozycji.
 *
 * Mapowanie jest TOTALNE na siedmiu stanach [ARCHITECTURE §5] i żaden z trzech stanów
 * końcowych bez sukcesu (`failed`, `cancelled`, `skipped`) nie ma prawa dać `done`: blok
 * wypełniony to obietnica, że krok się udał, a pominięty krok pokazany jako zrobiony jest
 * kłamstwem, które użytkownik odkrywa dopiero po wyniku.
 */
import type { FeedLine, Step, StepState } from '../../../state/run';

/** Trzy stany bloku [DESIGN §2]: wypełniony, akcent, obrys. */
export type BlockState = 'done' | 'now' | 'todo';

export interface Block {
  readonly id: string;
  readonly name: string;
  readonly state: BlockState;
  /** Krok się skończył, ale nie sukcesem. Blok zostaje `todo` i mówi to osobno. */
  readonly ended: boolean;
}

export interface Strip {
  readonly blocks: readonly Block[];
  /** `<nazwa> · step N of M` / `<nazwa> · N of M running` / `<nazwa> · M steps`. */
  readonly caption: string;
  /**
   * Chip z prawej: ile ten bieg zajął agentom i ile kosztował. Puste, dopóki nie wiadomo.
   *
   * TO JEST JEDYNE DOZWOLONE MIEJSCE NA TE DWIE LICZBY w całej aplikacji (niezmiennik 13) —
   * więc ich brak tutaj znaczył, że nie ma ich NIGDZIE, i tak było do 2026-08-18: makieta ma
   * chip `4m 12s · $0.31`, a pasek nie miał ani jednej cyfry.
   *
   * Puste, nie „—" i nie „0.0s · $0.00": bieg, po którym nie skończył się ani jeden krok,
   * nie ma jeszcze czego podać, a zero wygląda jak pomiar (`SPEND: not reported` z poprzedniego prototypu
   * stało w tej samej siatce, co wiersz z prawdziwą liczbą).
   */
  readonly spend: string;
}

/**
 * Stan bloku dla każdego z siedmiu stanów kroku [ARCHITECTURE §5].
 *
 * `Record`, nie `switch` z gałęzią `default`: mapowanie ma być TOTALNE, a gałąź domyślna
 * zamienia ósmy stan dodany kiedyś po stronie Rusta w cichy `todo` zamiast w błąd kompilacji.
 * Trzy stany końcowe bez sukcesu celowo lądują w `todo`, nie w `done` — patrz `ENDED`.
 */
const BLOCK: Readonly<Record<StepState, BlockState>> = {
  succeeded: 'done',
  running: 'now',
  pending: 'todo',
  ready: 'todo',
  failed: 'todo',
  cancelled: 'todo',
  skipped: 'todo',
};

/**
 * Kroki, które się skończyły, ale nie sukcesem.
 *
 * Blok wypełniony jest obietnicą, że krok się udał. Pominięty krok pokazany jako zrobiony jest
 * kłamstwem, które użytkownik odkrywa dopiero po wyniku całego biegu — a wtedy nie ma już czego
 * naprawić. Stąd osobna flaga zamiast czwartego stanu bloku: DESIGN §2 zna trzy i tyle ich jest.
 */
const ENDED: ReadonlySet<StepState> = new Set<StepState>(['failed', 'cancelled', 'skipped']);

/**
 * Podpis paska.
 *
 * Trzy zdania, bo bieg równoległy jest zwykłym biegiem, nie wyjątkiem (niezmiennik 11):
 * „krok 2 z 4" ma sens dokładnie wtedy, kiedy biegnie jeden krok, a przy dwóch jest już
 * wyborem, który z nich nazwać ważniejszym. Bez ani jednego biegnącego kroku nie ma numeru,
 * na który można wskazać, więc podpis mówi tylko, ile kroków ma workflow.
 */
function captionFor(workflow: string, blocks: readonly Block[]): string {
  const total = blocks.length;
  /* Bieg, którego nie ma, nie ma czego podpisywać. „· 0 steps" opisywałoby workflow o zerowej
   * długości, czyli rzecz, której nie da się zbudować. */
  if (total === 0) return '';

  const running = blocks.filter((block) => block.state === 'now').length;
  if (running === 1) {
    /* Numer kroku jest jego pozycją w grafie, nie liczbą tych, które się skończyły: przy
     * biegu, który przeskoczył krok, „step 2 of 4" i „drugi blok" muszą być tym samym blokiem. */
    const at = blocks.findIndex((block) => block.state === 'now') + 1;
    return `${workflow} · step ${at} of ${total}`;
  }
  if (running > 1) {
    return `${workflow} · ${running} of ${total} running`;
  }
  return `${workflow} · ${total} steps`;
}

/** Sekundy w milisekundzie — jedyne miejsce, w którym ta zamiana tu żyje. */
const MS_PER_SECOND = 1_000;
/** Sekund w minucie. */
const SECONDS_PER_MINUTE = 60;

/**
 * Czas tak, jak zapisuje go strona Rusta (`engine/line.rs`, `took_text`): `6.2s` pod minutą,
 * `4m 12s` powyżej.
 *
 * Przepisany kształt, nie przepisana liczba: te dwa napisy muszą się czytać identycznie, bo
 * ten sam bieg widać i w linii `done` w strumieniu, i w chipie na pasku. Dwie różne konwencje
 * na jeden pomiar to dwa różne odczyty tej samej rzeczy na jednym ekranie.
 */
function tookText(ms: number): string {
  if (ms < SECONDS_PER_MINUTE * MS_PER_SECOND) {
    const tenths = Math.round(ms / 100) / 10;
    return tenths.toFixed(1) + 's';
  }
  const seconds = Math.floor(ms / MS_PER_SECOND);
  return (
    String(Math.floor(seconds / SECONDS_PER_MINUTE)) +
    'm ' +
    String(seconds % SECONDS_PER_MINUTE) +
    's'
  );
}

/**
 * Chip paska: ile czasu zebrało się na agentach i ile to kosztowało — z tego, co PRZYSZŁO.
 *
 * SKĄD TE LICZBY. Wiersz `done` zamyka turę agenta i niesie `durationMs` oraz `costUsd`
 * (`engine/line.rs`, `done_line`) — surowo, nie zaokrąglone do wyświetlenia, właśnie po to,
 * żeby suma biegu dała się policzyć. Sumujemy więc to, co dostaliśmy, i ani jednej liczby
 * więcej: bieg bez ani jednej skończonej tury daje pusty napis, a nie zero.
 *
 * CZEGO TA FUNKCJA NIE UDAJE. Suma czasów tur NIE JEST czasem zegarowym biegu — przy dwóch
 * agentach pracujących równolegle jest większa. Zegara ściennego biegu okno dziś nie ma
 * (`run_workflow` oddaje `()`, a `RunReport` nie jest `Serialize`), więc chip mówi to, co
 * naprawdę wiemy, a podpowiedź nad nim nazywa tę wielkość słowami. Wpisanie tu „elapsed"
 * byłoby liczbą, która wygląda na zegar i nim nie jest.
 *
 * Koszt pomijany, kiedy ŻADNA tura go nie podała: `costUsd` jest `Option<f64>` i dostawca bez
 * cenniku (albo tryb, w którym go nie ma) oddaje `null`. `$0.00` przy biegu, który kosztował
 * nieznane pieniądze, jest gorsze niż brak liczby.
 */
export function spendFor(lines: readonly FeedLine[]): string {
  let ms = 0;
  let cost = 0;
  let turns = 0;
  let priced = false;
  for (const line of lines) {
    if (line.kind !== 'done') continue;
    turns += 1;
    ms += line.durationMs;
    if (line.costUsd !== null) {
      cost += line.costUsd;
      priced = true;
    }
  }
  if (turns === 0) return '';
  return priced ? tookText(ms) + ' · $' + cost.toFixed(2) : tookText(ms);
}

/**
 * Pasek dla tego workflow i tych kroków, w kolejności grafu.
 *
 * `spend` wchodzi gotowe, a nie liczy się tutaj z linii: pasek jest funkcją PLANU, a wydatek
 * jest funkcją STRUMIENIA, i wołający ma pod ręką jedno i drugie. Trzeci argument jest
 * opcjonalny, bo cudze kryterium (`strip/strip.test.ts`) woła tę funkcję dwoma i nie wolno
 * go tknąć — a bieg bez ani jednej skończonej tury i tak nie ma czego pokazać.
 */
export function stripFor(workflow: string, steps: readonly Step[], spend = ''): Strip {
  /* Jeden blok na jeden krok, w kolejności grafu — długość bierze się z danych. Cztery bloki
   * „bo makieta ma cztery" to interfejs rysujący relację, której nie ma (niezmiennik 17). */
  const blocks: Block[] = steps.map((step) => ({
    id: step.id,
    name: step.name,
    /* Stan bloku jest funkcją stanu KROKU, nigdy jego pozycji. Jeden kursor `currentIndex`
     * przechodzi każdy bieg sekwencyjny i kłamie w pierwszym równoległym. */
    state: BLOCK[step.state],
    ended: ENDED.has(step.state),
  }));

  return { blocks, caption: captionFor(workflow, blocks), spend };
}
