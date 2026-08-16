/* Wiersz notatki: dwa stany, jeden żywy region na fakt i jedna akcja.
 *
 * NIEZMIENNIK 13 RZĄDZI TYM PLIKIEM. Stan notatki ma dokładnie JEDEN żywy region w wierszu:
 * chip. Etykieta przycisku jest *akcją* („Use this"), nie powtórzonym stanem — chip
 * „Suggested", obok tekst „not in use yet", a na dole licznik „3 suggested" to trzy regiony
 * na jeden fakt i dokładnie to, przez co poprzedni prototyp pokazywał stan połączenia w sześciu
 * miejscach [ARCHITECTURE §7: żywe regiony na jeden fakt = 1].
 *
 * NIEZMIENNIK 14: w tym wierszu istnieją wyłącznie słowa `Suggested`, `In use`, `Use this`,
 * `Stop using` i `length`. Trzeci stan (`candidate`, `confirmed`, `corroborated`, `trusted`,
 * `archived`, `replaced`) i żargon (`promote`, `token`) wchodzą właśnie tędy — z makiety,
 * z enuma z drutu albo z pola, które ktoś wypisał „na wszelki wypadek".
 *
 * UZASADNIENIE JEST WIDOCZNE, NIE ZA KLIKNIĘCIEM. Człowiek jest jedyną osobą, która może
 * powiedzieć „tak, to jest prawda", a klika w to raz — bez powodu na ekranie klika w ciemno
 * i cała bramka promocji staje się rytuałem [T6 §5.1].
 *
 * Czysta funkcja propsów na markup, jak `ReviewCard`: bez własnego stanu i bez `invoke()`.
 * Odmowa i wymuszony wybór mieszkają w magazynie (`src/state/memory.ts`), nie tutaj —
 * wyłączony przycisk jest sugestią, a nie mechanizmem.
 *
 * Ciało jest jeszcze puste. Szkielet ma się WCZYTAĆ i paść w czasie wykonania — komponent,
 * którego nie ma, daje „Cannot find module", czyli czerwień, której bramka nie liczy
 * (AGENTS.md §2a).
 */
import type { ReactElement } from 'react';
import type { Note } from '../../state/memory';

export interface NoteRowProps {
  note: Note;
  /** „Use this". Handler jest wymagany, bo kontrolka bez handlera nie wchodzi do repo
   * (niezmiennik 16) — a wiersz nie zna magazynu i nie ma jak zawołać go sam. */
  onUse: (id: string) => void;
  /** „Stop using". Ta sama reguła. */
  onStopUse: (id: string) => void;
}

export function NoteRow(_props: NoteRowProps): ReactElement {
  return <></>;
}
