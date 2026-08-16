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
import type { Step, StepState } from '../../../state/run';

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

/** Pasek dla tego workflow i tych kroków, w kolejności grafu. */
export function stripFor(workflow: string, steps: readonly Step[]): Strip {
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

  return { blocks, caption: captionFor(workflow, blocks) };
}
