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
import type { Step } from '../../../state/run';

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

/** Pasek dla tego workflow i tych kroków, w kolejności grafu. */
export function stripFor(workflow: string, steps: readonly Step[]): Strip {
  /* Zaślepka fazy kontraktu; implementacja zastępuje całe ciało. */
  void workflow;
  void steps;
  throw new Error('not implemented');
}
