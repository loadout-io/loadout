/* Port przewijania dla scen, które nie są o przewijaniu.
 *
 * Każda metoda RZUCA. Wygodniejsza byłaby atrapa, która po cichu nic nie robi — i byłaby
 * cichym przyzwoleniem: model, który sięgnął po port podczas sklejania linii albo przypinania
 * pytania, przechodziłby wtedy każdy plik poza `steady-state.test.ts`, a tam pękłby jednym
 * kryterium, o którym łatwo pomyśleć, że to ono jest przewrażliwione. Tutaj pęka na miejscu
 * i mówi, w którym pliku.
 *
 * Kryterium 1 ma własną atrapę — tamta ZAPISUJE zamiast rzucać, bo tam liczba wywołań jest
 * całą mierzoną wielkością.
 */
import type { Scroller } from '../model';

const REFUSAL = 'the run view reached for the scroll port; only jumpToNewest may do that';

/** Port, którego nie wolno dotknąć. */
export function sealedScroller(): Scroller {
  return {
    scrollTop(): number {
      throw new Error(REFUSAL);
    },
    scrollTo(): void {
      throw new Error(REFUSAL);
    },
    scrollIntoView(): void {
      throw new Error(REFUSAL);
    },
  };
}
