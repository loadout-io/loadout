/* Kryterium 6 dla T-17: wiersz notatki pokazuje dwa stany i ani jednego trzeciego.
 *
 * Słabą wersją tego kryterium jest test, który renderuje JEDEN wariant i szuka napisu
 * „Suggested". Przechodzi na komponencie, który ma ten tekst wpisany na sztywno — czyli na
 * wierszu, który dla notatki w użyciu pokazuje dokładnie to samo, co dla kandydatki.
 * Rozróżniają dwie rzeczy, obie tutaj: oba warianty z TEGO SAMEGO komponentu oraz lista słów,
 * których w markupie być nie może.
 *
 * Ta lista nie jest ozdobna. Trzeci stan wchodzi do interfejsu przez makietę (`confirmed`
 * z cyklu T6 §5.3), przez enum z drutu (`candidate`, `trusted`) i przez pole dopisane „na
 * wszelki wypadek" (`archived`, `replaced`). ARCHITECTURE §2 pyt. 5 zostawia DWA stany
 * i to jest cały model — a `promote` i `token` to słowa maszyny, nie człowieka
 * (niezmiennik 14, `checks/quick-vocabulary.sh` zna oba).
 *
 * Render jest statyczny (`renderToStaticMarkup`), bo w repo nie ma `jsdom` ani
 * `@testing-library/react` — `package.json` nie należy do T-17.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Note } from '../../state/memory';
import { NoteRow } from './note-row';

/* Każde z tych słów stoi tu jako DANE, nie jako copy: pojedyncze słowo bez spacji nie jest
 * prozą dla sprawdzacza słownictwa, więc lista nie musi być nigdzie wyjęta ani wyciszona. */
const NEVER_ON_SCREEN = [
  'candidate',
  'confirmed',
  'corroborated',
  'trusted',
  'archived',
  'replaced',
  'promote',
  'token',
];

const RULE = 'An unresolved tenant comes back as 401, not 400.';
const REASON = 'run 7f3a step 2 reproduced it in auth.e2e.spec.ts:88';

/** Wyróżniająca się liczba: przypadkowe trafienie w klasę albo w rok byłoby fałszywą zielenią. */
const LENGTH = 137;

function note(status: Note['status']): Note {
  return {
    id: 'tenant-before-guard',
    title: 'The tenant is resolved before the guard',
    rule: RULE,
    because: REASON,
    status,
    scope: 'this-project',
    length: LENGTH,
    occurrences: 2,
    modified: '2026-08-16T10:31:02Z',
  };
}

function noop(): void {
  /* sterowany wiersz: w statycznym renderze nic tego nie woła */
}

function markup(status: Note['status']): string {
  return renderToStaticMarkup(<NoteRow note={note(status)} onUse={noop} onStopUse={noop} />);
}

/**
 * Otwierający znacznik przycisku niosącego tę etykietę. Brak etykiety jest tu porażką,
 * a nie cichym `undefined`: napis w `<span>` wygląda w markupie tak samo jak przycisk,
 * a klika się zupełnie inaczej.
 */
function buttonFor(html: string, label: string): string {
  const at = html.indexOf(label);
  if (at < 0) {
    throw new Error('the row shows nothing labelled: ' + label);
  }
  const opens = html.lastIndexOf('<button', at);
  if (opens < 0) {
    throw new Error('this label is not inside a button: ' + label);
  }
  return html.slice(opens, html.indexOf('>', opens) + 1);
}

describe('a note row says which of the two states it is in, once', () => {
  it('offers to put a suggested note to use, and says nothing about being in use', () => {
    const html = markup('suggested');

    expect(
      html,
      'the state has exactly one live region in the row, and this is it. A person scanning the ' +
        'list has to be able to tell in one look which notes reach the model',
    ).toContain('Suggested');
    expect(
      buttonFor(html, 'Use this'),
      'and the button says what will HAPPEN, not what already is. A label that repeats the ' +
        'state is a second region for one fact (invariant 13)',
    ).toContain('<button');
    expect(
      html,
      'and the other state is nowhere in the row. Two words for one fact means the row answers ' +
        'the same question twice, and one of the two answers is wrong',
    ).not.toContain('In use');
  });

  it('offers to stop using a note that is in use, from the same component', () => {
    const html = markup('in-use');

    expect(
      html,
      'the same row, the other state. A row that shows one variant only is a row with the text ' +
        'written into it, and it looks right for exactly as long as nobody tries the other one',
    ).toContain('In use');
    expect(
      buttonFor(html, 'Stop using'),
      'and the way back is a real control, not a sentence about one',
    ).toContain('<button');
    expect(html, 'and the first state is gone from the row').not.toContain('Suggested');
  });

  it('keeps a third state and the words of the machine out of both variants', () => {
    for (const status of ['suggested', 'in-use'] as const) {
      expect(
        markup(status),
        'the row has to say what the note says, first. Without this line the whole check below ' +
          'is also passed by a row that renders nothing at all — which is exactly the failure ' +
          'a list of forbidden words cannot see on its own',
      ).toContain(RULE);

      const html = markup(status).toLowerCase();
      for (const word of NEVER_ON_SCREEN) {
        expect(
          html,
          'this word reached the screen. There are two states and there is no third one ' +
            '(ARCHITECTURE §2 q. 5), and the words a machine uses for them are not the words ' +
            'a person reads (invariant 14). The word was: ' +
            word +
            ', in the variant: ' +
            status,
        ).not.toContain(word);
      }
    }
  });

  it('shows the reason on the row, so nobody approves a note in the dark', () => {
    for (const status of ['suggested', 'in-use'] as const) {
      expect(
        markup(status),
        'the person is the only one who can say whether this is true, and they decide once. ' +
          'Without the reason in front of them they are deciding about a sentence they have ' +
          'not read — and a note that cannot say why it exists can never be safely retired ' +
          'either (T6 §5.1)',
      ).toContain(REASON);
    }
  });

  it('says how long the note is in the word a person uses for it', () => {
    const html = markup('in-use');

    expect(
      html,
      'the number is on the row, because the budget of a scope is the whole reason a person ' +
        'is ever asked to give a note up',
    ).toContain(String(LENGTH));
    expect(
      html.toLowerCase(),
      'and it is labelled with the plain word. The machine word for this is already in the ' +
        'list above, and it is banned for the same reason as the third state',
    ).toContain('length');
  });
});
