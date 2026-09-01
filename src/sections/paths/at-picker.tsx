/* Lista miejsc, która wychodzi spod małpki.
 *
 * # Dlaczego własna lista, a nie `<datalist>`
 *
 * Ta sama przyczyna, dla której wybór workflow przestał być `<select>`: macOS rysuje natywną
 * podpowiedź własnym krojem i własnym rozmiarem, więc lista wyglądałaby jak wyrwana z innego
 * programu, a my nie mielibyśmy jak pokazać, że wpis jest katalogiem.
 *
 * # Dlaczego to jest `listbox`, a nie menu
 *
 * Czytnik ekranu ma powiedzieć „lista, pozycja 2 z 7", bo dokładnie to widzi człowiek. Pole
 * zostaje ogniskiem — strzałki i Enter obsługuje ono, a nie ta lista, więc pisanie nigdy się
 * nie przerywa (`aria-activedescendant` wskazuje wybraną pozycję).
 */
import type { ReactElement } from 'react';

import type { Suggestion } from './io';

export interface AtPickerProps {
  /** Co pokazać. Pusta lista znaczy „nie ma czego pokazać" i nie rysuje ani jednego węzła. */
  readonly items: readonly Suggestion[];
  /** Która pozycja jest wybrana strzałkami. */
  readonly active: number;
  /** Identyfikator listy, żeby pole mogło ją wskazać. */
  readonly id: string;
  readonly onChoose: (path: string) => void;
}

/** Identyfikator jednej pozycji — ten sam po obu stronach `aria-activedescendant`. */
export function optionId(listId: string, index: number): string {
  return `${listId}-${String(index)}`;
}

export function AtPicker({ items, active, id, onChoose }: AtPickerProps): ReactElement {
  if (items.length === 0) return <></>;

  return (
    <ul
      id={id}
      data-at-picker
      role="listbox"
      aria-label="Places in this project"
      className="enter absolute bottom-full left-0 z-20 mb-1 max-h-64 w-96 max-w-full overflow-y-auto rounded-sm border border-line-strong bg-raised p-1 shadow-pane"
    >
      {items.map((one, index) => (
        <li key={one.path}>
          <button
            type="button"
            id={optionId(id, index)}
            role="option"
            aria-selected={index === active}
            data-at-option
            data-folder={one.folder ? 'yes' : 'no'}
            /* `onMouseDown` z `preventDefault`, nie `onClick`: klik zabrałby ognisko polu, a wtedy
               token, w który wstawiamy ścieżkę, przestałby istnieć razem z kursorem. */
            onMouseDown={(event) => {
              event.preventDefault();
              onChoose(one.path);
            }}
            className="flex w-full items-center gap-2 rounded-sm px-2 py-1 text-left aria-[selected=true]:bg-well"
          >
            <span aria-hidden="true" className="opacity-60">
              {one.folder ? '▸' : '·'}
            </span>
            <span className="truncate">{one.path}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}
