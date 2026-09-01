/* Jedno wejście do stawiania rzeczy na płótnie, i pięć zdań, które mówią, co powstanie.
 *
 * CO SĄDZI TEN PLIK, A CZEGO NIE. Sądzi BRZMIENIA i to, że lista jest jedną kontrolką z pięcioma
 * pozycjami, a nie pięcioma kontrolkami — czyli tę połowę zgłoszenia, która jest o słowach.
 * Drugiej połowy — że ktoś tę listę naprawdę montuje, że przycisk ją otwiera i że wybór stawia
 * kafelek — TEN PLIK NIE DOWODZI I NIE UDAJE, że dowodzi: `renderToStaticMarkup` nie odpala ani
 * jednego `onClick`. Dowodzi tego prawdziwym kliknięciem `e2e/tests/the-canvas-reads-as-a-board
 * .spec.ts`, i to jest ten sam podział, który niezmiennik 29 nazywa po imieniu.
 *
 * OSTATNI PRZYPADEK JEST JEDYNYM, KTÓREGO PRZEGLĄDARKA NIE ZŁAPIE. Powrót ma w tym produkcie
 * TRZY powierzchnie: pozycję na tej liście, podpis na strzałce (`triesLabel`) i wiersz w panelu
 * kroku („Try again up to"). Do 2026-08-31 każda z nich nazywała go inaczej — przycisk mówił
 * „loop", strzałka „tries", panel „try again" — a trzy nazwy jednej rzeczy to trzy rzeczy dla
 * kogoś, kto widzi ją pierwszy raz. Kryterium wiąże pierwsze dwie ze sobą, więc powrót do
 * słowa „loop" w liście przewraca je, choć wygląda wtedy dokładnie tak samo jak dziś.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { ADD_MENU, AddMenu } from './add-menu';
import { triesLabel } from './canvas';

/** Sam tekst, bez znaczników — to jest to, co czyta człowiek. */
function plain(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, ' ')
    .replace(/&#x27;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/\s+/g, ' ')
    .trim();
}

function nothing(): void {
  /* Kryterium sądzi kształt i słowa; handlery sądzi prawdziwe kliknięcie w przeglądarce. */
}

const CLOSED = renderToStaticMarkup(
  <AddMenu open={false} onToggle={nothing} onPick={nothing} onDismiss={nothing} />,
);
const OPEN = renderToStaticMarkup(
  <AddMenu open onToggle={nothing} onPick={nothing} onDismiss={nothing} />,
);

describe('one way in to putting something on the board', () => {
  it('offers a single control until somebody asks for the list', () => {
    expect(
      (CLOSED.match(/<button/g) ?? []).length,
      'the board offers more than one control for putting something down. Six of them stood ' +
        'here in a row until 2026-08-31, and four of the six put a tile down while the fifth ' +
        'drew an arrow and the sixth rearranged everything — one family name over three ' +
        'different jobs.',
    ).toBe(1);

    expect(plain(CLOSED), 'the one control does not say what it is for.').toBe('＋ Add');

    for (const group of ADD_MENU) {
      for (const pick of group.picks) {
        expect(
          plain(CLOSED),
          'the whole list is spelled out on the board before anybody asked for it, so the ' +
            'row grows by one line with every kind of step this product ever learns.',
        ).not.toContain(pick.says);
      }
    }
  });

  it('says what each pick will make, never what the variant is called in the code', () => {
    const said = plain(OPEN);

    expect(
      said,
      'the list does not say that a step made this way is a step an AGENT does. That is the ' +
        'whole content of that step and the only reason this product exists; "Add step" left ' +
        'it out entirely.',
    ).toContain('A step an agent does');

    expect(
      said,
      'the list still does not say that this pick makes a STEP, nor what that step does. ' +
        '"Start something" said neither, and the tile it left behind then called itself ' +
        'something else again.',
    ).toContain('A step that leaves a command running');

    expect(
      said,
      'the pick that makes a checking step reads like an order to check something now, ' +
        'rather than the name of a step that will check something later.',
    ).toContain('A step that runs a check');

    expect(
      said,
      'the pick that makes a step which stops and asks a person still carries a word that ' +
        'says nothing about what will happen.',
    ).toContain('A step that asks you');
  });

  it('groups the picks by what a person came for, not by shape', () => {
    const said = plain(OPEN);
    const order = ADD_MENU.flatMap((group) => [group.goal, ...group.picks.map((one) => one.says)]);

    let read = -1;
    for (const line of order) {
      const found = said.indexOf(line, read + 1);
      expect(
        found,
        `"${line}" is missing from the list, or stands somewhere other than where the ` +
          'grouping puts it. A heading that does not stand above its own picks is a heading ' +
          'a person has to check twice.',
      ).toBeGreaterThan(read);
      read = found;
    }

    expect(
      ADD_MENU.map((group) => group.goal),
      'the groups are named after the shape of the thing that comes out — which is the same ' +
        'defect one storey up. A person does not arrive wanting a shape; they arrive wanting ' +
        'work done, or wanting to know whether it worked.',
    ).toEqual(['Getting work done', 'Checking the work', 'When a check says no']);
  });

  it('tells a tile apart from an arrow by the words alone', () => {
    const tiles = ADD_MENU.flatMap((group) => group.picks).filter(
      (pick) => pick.choice !== 'way-back',
    );
    for (const pick of tiles) {
      expect(
        pick.says,
        `"${pick.says}" makes a tile and does not say so, so the only way to find out what ` +
          'a pick leaves behind is to press it and look.',
      ).toContain('A step');
    }

    const back = ADD_MENU.flatMap((group) => group.picks).find(
      (pick) => pick.choice === 'way-back',
    );
    expect(back, 'there is no way to send work back to an earlier step at all.').toBeDefined();
    expect(
      back?.says ?? '',
      'the way back is worded like the four picks that make a tile, and it makes no tile — ' +
        'it draws an arrow between two that already stand there.',
    ).not.toContain('A step');
  });

  it('calls the way back the same thing the arrow and the step panel call it', () => {
    const back = ADD_MENU.flatMap((group) => group.picks).find(
      (pick) => pick.choice === 'way-back',
    );

    expect(
      back?.says ?? '',
      'the list names the way back with a word that neither the arrow nor the step panel ' +
        'uses. The panel row reads "Try again up to", the arrow on the board reads ' +
        `"${triesLabel(3)}", and the button used to read "Add loop" — three names for one ` +
        'thing, which is three things to anybody meeting it for the first time.',
    ).toContain('try again');

    expect(
      triesLabel(3),
      'the arrow stopped saying what the list promises, so the two drifted apart the other way.',
    ).toContain('tries');
  });
});
