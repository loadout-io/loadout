/* Jedno zdanie na kafelku — i kto je powiedział.
 *
 * Cicha porażka, przed którą stoi ten plik: „latest note from this agent" karmione
 * czymkolwiek, co przyszło ostatnie. Agent pisze prozą, potem padają sprawdzenia, kafelek
 * pokazuje „3 of 40 tests failed" — i podaje to jako CYTAT AGENTA. Sprawdzenia to Loadout,
 * nie agent [00-SYNTHESIS §2.2]: to jest ten sam błąd co blok „co wyprodukował" karmiony
 * ostatnią wiadomością agenta, tylko mniejszą czcionką i dlatego trudniejszy do zauważenia.
 *
 * Stąd `who` obok tekstu, zawsze, a nie „gdy się przyda". Zdanie bez podpisu autorytetu
 * czyta się jak fakt niezależnie od tego, czym jest.
 */
import type { FeedLine } from '../../../state/run';
import type { Who } from '../../../state/run';
import type { Say } from './card';

/**
 * Trzy autorytety w całej aplikacji, nie osiem [00-SYNTHESIS §2.2].
 *
 * `Record<Who, true>`, nie tablica literałów, i to jest cała obrona: czwarty autorytet
 * dopisany kiedyś do `Who` przestaje TU się kompilować, zamiast po cichu wjechać na ekran
 * jako czwarte słowo, którego nikt nie zdefiniował. Ta sama sztuczka, co rejestr rodzajów
 * linii w `src/sections/run/feed/kinds.ts`.
 */
const AUTHORITY: Readonly<Record<Who, true>> = { agent: true, loadout: true, you: true };

/** Zamknięty zbiór autorytetów jako wartość — typ nie istnieje w czasie wykonania. */
export const AUTHORITIES: readonly Who[] = Object.keys(AUTHORITY) as Who[];

/**
 * Zdanie kafelka dla agenta, który nadał te linie.
 *
 * Wejściem są linie z drutu, nie wiersze historii: `ran` niesie `ok` tylko przed sklejeniem,
 * a bez `ok` nie da się odróżnić „agent coś powiedział" od „sprawdzenia coś powiedziały".
 */
export function sayFor(_lines: readonly FeedLine[]): Say {
  throw new Error('not implemented');
}
