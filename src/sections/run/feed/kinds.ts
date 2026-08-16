/* Rejestr rodzajów linii — zamknięty na czternastu [T2 §7.2].
 *
 * „Zamknięty" jest tu słowem operacyjnym. Rejestr, obok którego stoi gałąź `default`
 * drukująca to, czego nie rozpoznał, nie jest zamknięty: pierwszy nowy typ zdarzenia
 * od vendora wyświetla wtedy surowy enum z drutu na ekranie użytkownika (niezmiennik 14).
 * Dlatego rodzaj spoza tego zbioru nie ma renderera — jest PORZUCANY w modelu.
 *
 * Wpis niesie dwie rzeczy i ani jednej więcej:
 *   `route`     dokąd wiersz idzie. Dokładnie jeden rodzaj (`thinking`) trafia do strefy
 *               TERAZ, bo `Thinking…` to status, nie linia [T2 §7.3 reguła 5]; reszta
 *               dopisuje się do historii.
 *   `expanded`  czy jest rozwinięty domyślnie [T2 §7.3 reguła 2].
 *
 * Czego tu NIE ma: etykiety. Etykieta zależy od licznika (`Read 3 files`), więc jest funkcją,
 * nie stałą, i mieszka w modelu razem ze sklejaniem.
 */
import type { Line } from '../../../ipc/types';

/** Czternaście rodzajów, wyprowadzonych z lustra drutu — nie przepisanych obok niego. */
export type Kind = Line['kind'];

/** Dokąd idzie wiersz tego rodzaju. */
export type Route = 'now' | 'history';

export interface KindEntry {
  readonly route: Route;
  /** Rozwinięty domyślnie? Osiem tak, sześć nie [T2 §7.3 reguła 2]. */
  readonly expanded: boolean;
}

export type Registry = Readonly<Record<Kind, KindEntry>>;

/** Rejestr. Czytany, nigdy modyfikowany. */
export function kinds(): Registry {
  throw new Error('not implemented');
}
