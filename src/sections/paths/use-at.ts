/* Stan listy miejsc dla jednego pola.
 *
 * Mieszka osobno, bo `@` ma stanąć w TRZECH miejscach — wiersz wejścia, pola kroku w edytorze
 * i pola agenta — a trzy kopie tej samej obsługi klawiszy rozjechałyby się przy pierwszej
 * poprawce (niezmiennik 13). Hak nie wie nic o wyglądzie; rysuje `AtPicker`.
 */
import { useRef, useState } from 'react';

import type { AtMention } from './at-mention';
import { chosen, mentionAt } from './at-mention';
import type { Suggestion } from './io';
import { suggestPaths } from './io';

/** Co pole dostaje do ręki. */
export interface At {
  readonly items: readonly Suggestion[];
  readonly active: number;
  /** Czy lista jest otwarta — czyli czy strzałki należą do niej, a nie do pola. */
  readonly open: boolean;
  /** Powiedz, co jest w polu i gdzie stoi kursor. */
  readonly look: (text: string, caret: number) => void;
  /** Przesuń wybór; zawija się, bo lista bywa dłuższa niż ekran. */
  readonly move: (by: number) => void;
  /** Wstaw wybraną ścieżkę. `null`, kiedy nie ma czego wstawić. */
  readonly take: (text: string) => { readonly text: string; readonly caret: number } | null;
  readonly shut: () => void;
}

export function useAt(): At {
  const [items, setItems] = useState<readonly Suggestion[]>([]);
  const [active, setActive] = useState(0);
  const mention = useRef<AtMention | null>(null);
  /* Numer ostatniego pytania. Odpowiedzi z dysku wracają w dowolnej kolejności, a lista pokazana
   * dla `@sr` po tym, jak człowiek dopisał `c`, to lista dla zapytania, którego już nie ma. */
  const asked = useRef(0);

  function shut(): void {
    mention.current = null;
    asked.current += 1;
    setItems([]);
    setActive(0);
  }

  function look(text: string, caret: number): void {
    const found = mentionAt(text, caret);
    if (found === null) {
      shut();
      return;
    }
    mention.current = found;
    asked.current += 1;
    const mine = asked.current;
    void suggestPaths(found.typed).then((answer) => {
      if (asked.current !== mine) return;
      setItems(answer);
      setActive(0);
    });
  }

  function move(by: number): void {
    setActive((now) => {
      if (items.length === 0) return 0;
      return (now + by + items.length) % items.length;
    });
  }

  function take(text: string): { readonly text: string; readonly caret: number } | null {
    const found = mention.current;
    const one = items[active];
    if (found === null || one === undefined) return null;
    const put = chosen(text, found, one.path);
    /* Katalog NIE zamyka listy: człowiek wszedł piętro niżej i chce zobaczyć, co tam jest.
     * Plik zamyka, bo wskazywanie się skończyło. */
    if (one.folder) {
      look(put.text, put.caret);
    } else {
      shut();
    }
    return put;
  }

  return { items, active, open: items.length > 0, look, move, take, shut };
}
