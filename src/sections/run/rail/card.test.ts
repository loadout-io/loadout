/* Kryterium 1: kafelek ma cztery sloty tekstu i ani jednego więcej.
 *
 * `expect(card.say.text).not.toContain('\n')` przechodzi dla implementacji, która dokłada
 * piąty slot — „12 files · 2m 04s" jako wiersz metadanych pod stanem. Kafelek ma wtedy pięć
 * linii, wygląda dobrze na jednym agencie i rozjeżdża listę przy czterech, a żadna asercja
 * o jednolinijkowości tego nie widzi.
 *
 * Rozróżnia to porównanie PEŁNEGO posortowanego zbioru kluczy z literałem, powtórzone dla
 * sześciu stanów agenta. Nowe pole nie ma jak się prześlizgnąć: musiałoby powstać dla
 * żadnego z sześciu, a wtedy nie ma po co istnieć. To samo w środku `say` — piąty slot
 * schowany jako `say.meta` byłby dokładnie tym samym błędem o jeden poziom niżej.
 *
 * Skracaniem zajmuje się CSS (`text-overflow: ellipsis`, makieta linia 185), nie kod, więc
 * długa notatka ma wrócić W CAŁOŚCI. Obcięcie do stałej liczby znaków jest tu osobnym
 * błędem: wygląda jak troska o układ, a gubi zdanie, którego nikt już nie odzyska, bo
 * kafelek jest jedynym miejscem, w którym to zdanie widać.
 */
import { describe, expect, it } from 'vitest';
import type { FeedLine } from '../../../state/run';
import { line } from '../feed/fixtures/lines';
import type { AgentInRun, AgentStatus } from './card';
import { railCard } from './card';

/** Komplet kluczy kafelka. Literał, nie import: import z modułu sprawdzałby sam siebie.
 *
 * 2026-08-23 — DOSZEDŁ `stepId` I TO NIE JEST ROZLUŹNIENIE TEJ REGUŁY. Sufit z `ARCHITECTURE`
 * §7 mówi o SLOTACH TEKSTU na kafelku, a nie o liczbie pól w strukturze; liczba pól była tylko
 * jego tanim przybliżeniem, dopóki każde pole coś rysowało. `stepId` nie rysuje nic — jest
 * kluczem kafelka z pliku workflow, po którym ekran otwartego agenta powtarza ten krok
 * (`rail/again.ts`), i nie ma jak wyciec na listę.
 *
 * Regułę, o którą tu naprawdę chodzi, pilnuje dalej i bez zmian kryterium obok
 * (`rail-shows-agents.test.tsx`): kafelek pokazuje cztery wiersze tekstu i ani jednego więcej,
 * mierzone na markupie. Gdyby ktoś dołożył piąty napis, tamto padnie — i to ono jest tu
 * wyrocznią, nie ta lista. */
const KEYS = ['id', 'name', 'role', 'say', 'square', 'status', 'stepId'];

const A = 'Forge';

function agentWith(status: AgentStatus, lines: readonly FeedLine[]): AgentInRun {
  return { id: A, name: 'Forge', role: 'writes code', status, lines };
}

/**
 * Sześć stanów, sześć różnych zestawów linii.
 *
 * Kryterium wymienia dwa z nich po imieniu, bo to one kuszą do piątego slotu: `failed`
 * chce dopisać, ile sprawdzeń padło, a `needs you` — treść pytania.
 */
const SCENES: readonly (readonly [AgentStatus, readonly FeedLine[]])[] = [
  ['working', [line.read(1, 0, A, 'src/parser.rs'), line.edit(2, 400, A, 'src/parser.rs', 42, 8)]],
  ['waiting', [line.handoff(3, 800, A, 'Orion → Forge')]],
  [
    'needs you',
    [
      line.asked(4, 1_200, A, 'The row has more columns than the header. What should it do?', [
        'Drop the extra columns',
        'Fail the whole file',
        'Keep them, unnamed',
      ]),
    ],
  ],
  [
    'failed',
    [
      line.note(
        5,
        1_600,
        A,
        'The quote handling only looks at the first character of a field, so an embedded ' +
          'comma inside a quoted value splits the row in two. I am rewriting it as a small ' +
          'state machine with three states and re-running the checks after that.',
      ),
      line.ran(6, 2_000, A, "Ran tests — didn't work", false, ['3 of 40 failed']),
    ],
  ],
  ['done', [line.done(7, 2_400, A, 'Finished in 4m 12s')]],
  ['stopped', [line.note(8, 2_800, A, 'Stopping here; the header row is malformed.')]],
];

describe('the agent card carries four slots of text and not one more', () => {
  it('carries the same fields in every one of the six states an agent can be in', () => {
    for (const [status, lines] of SCENES) {
      const card = railCard(agentWith(status, lines));

      expect(
        Object.keys(card).sort(),
        'the full sorted key set, compared with a literal, for the state "' +
          status +
          '". A weaker check on one field lets a fifth TEXT slot in — a metadata row under ' +
          'the state looks fine on one agent and breaks the layout at four. The rendered rule ' +
          'is measured next door, on the markup; this list only keeps the shape honest',
      ).toEqual(KEYS);
    }
  });

  it('keeps the sentence and who said it together, with nothing else beside them', () => {
    const card = railCard(agentWith('working', [line.read(1, 0, A, 'src/parser.rs')]));

    expect(
      Object.keys(card.say).sort(),
      'a fifth slot hidden one level down is the same defect, and the top-level key set ' +
        'cannot see it',
    ).toEqual(['text', 'who']);
  });

  it('carries the name of a colour in the square, never a literal value', () => {
    const card = railCard(agentWith('working', [line.read(1, 0, A, 'src/parser.rs')]));

    expect(
      card.square.startsWith('--'),
      'the square carries a name from the theme; a literal value in component code is ' +
        'forbidden [DESIGN §9]',
    ).toBe(true);
    expect(card.square).not.toContain('#');
  });

  it('folds a note that spans two lines into exactly one', () => {
    // Znak nowej linii siedzi na pozycji 10, w środku zdania — dokładnie tam, gdzie łamie
    // go agent piszący prozą, a nie tam, gdzie ktoś by go w teście wygodnie postawił.
    const note = 'The parser\nsplits   on   the first comma,   which is wrong.  ';
    expect(note.indexOf('\n'), 'the break sits inside the sentence, not at its edge').toBe(10);

    const card = railCard(agentWith('working', [line.note(1, 0, A, note)]));

    expect(
      card.say.text,
      'runs of whitespace collapse to a single space and the result is trimmed on both ' +
        'ends. A card line that can become two lines breaks the four-line ceiling without ' +
        'a single new field being added',
    ).toBe('The parser splits on the first comma, which is wrong.');
    expect(card.say.text).not.toContain('\n');
  });

  it('gives back a long note whole, because shortening belongs to the stylesheet', () => {
    const long =
      'The quote handling only looks at the first character of a field, so an embedded ' +
      'comma inside a quoted value splits the row in two. I am rewriting it as a small ' +
      'state machine with three states and re-running the checks after that.';
    expect(long.length).toBeGreaterThan(200);

    const card = railCard(agentWith('working', [line.note(1, 0, A, long)]));

    expect(
      card.say.text,
      'no cut to a fixed number of characters. The stylesheet ends the line with an ' +
        'ellipsis and the full sentence is still there when the layout gets wider; a cut ' +
        'in code throws it away for good',
    ).toBe(long);
  });
});
