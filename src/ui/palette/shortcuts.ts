/* Lista skrótów, którą pokazuje `?` — wyprowadzona z tej samej mapy, którą czyta klawiatura.
 *
 * DLACZEGO WYPROWADZONA, A NIE SPISANA. Spisana lista skrótów jest dokumentacją, a dokumentacja
 * rozjeżdża się z kodem po cichu: sekcja dopisana do rejestru dostaje literę od `./keys.ts`
 * i NIE dostaje wiersza tutaj, więc człowiek czyta listę, która milczy o skrócie, który działa.
 * Wersja odwrotna jest gorsza — wiersz obiecujący skok, którego klawiatura nie zna.
 *
 * KRÓTKA, NIE ŚCIANA. Trzy wiersze o samej palecie, dwa o poruszaniu się po niej i po jednym
 * na sekcję, która ma literę. Przy siedmiu sekcjach w rejestrze daje to jedenaście wierszy —
 * i to jest sufit, którego pilnuje kryterium, a nie uprzejmość autora. Dodatkowo lista jedzie
 * przez to samo pole wpisywania, co reszta palety, więc jest przeszukiwalna tym samym słowem.
 */
import { JUMPS } from './keys';
import { sectionEntry } from '../sections';

/** Jeden wiersz listy: co nacisnąć i co się wtedy stanie. */
export interface Shortcut {
  /** Klawisze, znak w znak tak, jak człowiek ma je nacisnąć. */
  readonly press: string;
  /** Zdanie o tym, co robią. */
  readonly does: string;
}

/* Skróty, które nie są skokiem do sekcji. Stoją pierwsze, bo są jedyną drogą DO tej listy
 * i jedyną drogą z niej z powrotem. */
const ABOUT_THE_PALETTE: readonly Shortcut[] = [
  { press: '⌘K', does: 'Open this list of things to do' },
  { press: '↑ ↓', does: 'Move through the list' },
  { press: 'Enter', does: 'Pick what is highlighted' },
  { press: '?', does: 'Show these shortcuts' },
  { press: 'Esc', does: 'Close' },
];

/** Wszystkie skróty, jakie ta aplikacja ma — w kolejności, w jakiej warto je poznać. */
export function shortcuts(): Shortcut[] {
  const rows = [...ABOUT_THE_PALETTE];
  for (const [letter, section] of JUMPS) {
    rows.push({
      press: 'G ' + letter.toUpperCase(),
      /* Etykieta z rejestru, nie z identyfikatora: `sectionEntry(id).label` jest tym samym
       * napisem, który stoi w nawigacji, więc wiersz mówi o tym, co człowiek widzi. */
      does: 'Go to ' + sectionEntry(section).label,
    });
  }
  return rows;
}

/** To, co zostaje po wpisaniu słowa — po klawiszach i po zdaniu, bo szuka się i tak, i tak. */
export function matchingShortcuts(rows: readonly Shortcut[], typed: string): Shortcut[] {
  const wanted = typed.trim().toLowerCase();
  if (wanted === '') return [...rows];
  return rows.filter(
    (row) => row.press.toLowerCase().includes(wanted) || row.does.toLowerCase().includes(wanted),
  );
}
