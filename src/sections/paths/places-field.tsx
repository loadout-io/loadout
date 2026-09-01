/* Pole tekstowe, w którym `@` otwiera listę miejsc.
 *
 * # Dlaczego opakowanie, a nie wpięcie w każdym polu z osobna
 *
 * `@` ma stanąć w trzech miejscach: w wierszu wejścia, w instrukcji kroku i w instrukcji agenta.
 * Trzy kopie tej samej obsługi klawiszy rozjechałyby się przy pierwszej poprawce (niezmiennik 13),
 * a każda z nich musiałaby osobno pamiętać, że strzałki należą do listy TYLKO wtedy, gdy jest
 * otwarta. Wiersz wejścia ma własne wpięcie, bo tam strzałki mają drugiego właściciela —
 * historię poleceń — i ta rozstrzygnięta kolizja nie ma prawa wyciec tutaj.
 */
import type { ReactElement, TextareaHTMLAttributes } from 'react';
import { useRef } from 'react';

import { AtPicker, optionId } from './at-picker';
import { useAt } from './use-at';

type Passed = Omit<
  TextareaHTMLAttributes<HTMLTextAreaElement>,
  'value' | 'onChange' | 'aria-controls' | 'aria-activedescendant'
>;

export interface PlacesFieldProps extends Passed {
  /** Musi być, bo lista wskazuje pole przez `aria-controls` i potrzebuje własnego adresu. */
  readonly id: string;
  readonly value: string;
  readonly onChange: (next: string) => void;
}

export function PlacesField({ id, value, onChange, ...rest }: PlacesFieldProps): ReactElement {
  const field = useRef<HTMLTextAreaElement>(null);
  const at = useAt();
  const listId = `${id}-places`;

  /** Wstawia wybraną ścieżkę i stawia kursor tam, gdzie człowiek pisze dalej. */
  function take(): void {
    const put = at.take(value);
    if (put === null) return;
    onChange(put.text);
    queueMicrotask(() => {
      const one = field.current;
      if (!one) return;
      one.focus();
      one.setSelectionRange(put.caret, put.caret);
    });
  }

  return (
    <div className="relative grid min-w-0">
      <AtPicker items={at.items} active={at.active} id={listId} onChoose={take} />
      <textarea
        {...rest}
        id={id}
        ref={field}
        value={value}
        aria-controls={at.open ? listId : undefined}
        aria-activedescendant={at.open ? optionId(listId, at.active) : undefined}
        onChange={(event) => {
          onChange(event.target.value);
          at.look(event.target.value, event.target.selectionStart);
        }}
        onKeyDown={(event) => {
          /* Lista bierze klawisze PIERWSZA, ale wyłącznie kiedy jest otwarta. Zamknięta nie ma
           * prawa odbierać polu ani strzałek, ani Entera — w obszarze tekstu Enter robi nową
           * linię i to zachowanie zostaje nietknięte. */
          if (!at.open) return;
          if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
            event.preventDefault();
            at.move(event.key === 'ArrowDown' ? 1 : -1);
            return;
          }
          if (event.key === 'Enter' || event.key === 'Tab') {
            event.preventDefault();
            take();
            return;
          }
          if (event.key === 'Escape') {
            event.preventDefault();
            at.shut();
          }
        }}
        /* Kliknięcie i strzałki przesuwają kursor bez zmiany treści, a lista ma wtedy zniknąć:
         * małpka, z której wyszedł kursor, przestała być tym, co człowiek wskazuje. */
        onClick={(event) => {
          at.look(event.currentTarget.value, event.currentTarget.selectionStart);
        }}
        onKeyUp={(event) => {
          if (event.key.startsWith('Arrow') && !at.open) {
            at.look(event.currentTarget.value, event.currentTarget.selectionStart);
          }
        }}
      />
    </div>
  );
}
