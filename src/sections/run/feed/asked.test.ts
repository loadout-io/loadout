/* Kryterium 7: pytania do człowieka nie da się przegapić i nie da się obsłużyć nie po kolei.
 *
 * `expect(view.pinned).not.toBeNull()` tuż po pytaniu przechodzi dla implementacji, która
 * przypina po prostu OSTATNIĄ linię. Pierwszy `note`, który po niej przyjdzie, zasłania wtedy
 * pytanie, a bieg stoi, dopóki człowiek nie przewinie — czyli dokładnie ta awaria, którą
 * „przyklejone" ma wykluczyć. Rozróżniają to dwie rzeczy:
 *
 *   - `pinned.id` po WSZYSTKICH 160 dalszych zdarzeniach skryptu (są wśród nich i proza,
 *     i błąd — każde z nich jest kandydatem na „ostatnią linię"),
 *   - dwa nieodpowiedziane pytania naraz: przypięte musi być STARSZE, a odpowiedź na młodsze
 *     nie ma prawa zdjąć przypięcia ze starszego. Kolejka, w której odpowiadasz na to, co
 *     akurat widać, gubi pytanie, na którym bieg naprawdę stoi.
 */
import { describe, expect, it } from 'vitest';
import type { FeedLine } from '../../../state/run';
import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { ASKED_AT, run200 } from './fixtures/run-200';
import { createFeed } from './model';

/** Pytanie ze skryptu, zwężone do wariantu, który naprawdę ma tekst i opcje. */
function questionIn(script: readonly FeedLine[]) {
  const row = script[ASKED_AT - 1];
  if (row === undefined || row.kind !== 'asked') {
    throw new Error('the script carries no question where this check expects one');
  }
  return row;
}

describe('a question to a human is impossible to miss and impossible to answer out of turn', () => {
  it('runs on a script that really does keep talking after the question', () => {
    const after = run200().slice(ASKED_AT);

    expect(after.length, 'a hundred and sixty events come after the question').toBe(160);
    expect(
      after.filter((row) => row.kind === 'note').length,
      'and at least two of them are prose — prose is the cheapest thing to mistake for the ' +
        'line worth pinning',
    ).toBeGreaterThanOrEqual(2);
    expect(
      after.filter((row) => row.kind === 'problem').length,
      'and one of them is a failure, which is the other one',
    ).toBeGreaterThanOrEqual(1);
  });

  it('keeps the question pinned through every one of the 160 events that follow', () => {
    const script = run200();
    const question = questionIn(script);
    const feed = createFeed(sealedScroller());

    feed.appendLines(script.slice(0, ASKED_AT));
    expect(feed.view.pinned?.id, 'the question is pinned the moment it lands').toBe(question.id);
    expect(
      feed.view.attention,
      'and the view says whose turn it is, so the accent colour has something true to follow ' +
        '[DESIGN §3]',
    ).toBe('you');

    for (let i = ASKED_AT; i < script.length; i += 1) {
      feed.appendLines(script.slice(i, i + 1));
      expect(
        feed.view.pinned?.id,
        'still the question after event ' +
          String(i + 1) +
          '. Pinning the newest line instead lets the first note that arrives cover it, and ' +
          'the run then waits on somebody who cannot see what it is waiting for',
      ).toBe(question.id);
    }
    expect(feed.view.attention, 'and it is still your turn at the end of the script').toBe('you');
  });

  it('carries the text and the options the buttons need', () => {
    const script = run200();
    const question = questionIn(script);
    const feed = createFeed(sealedScroller());
    feed.appendLines(script.slice(0, ASKED_AT));

    expect(feed.view.pinned?.text, 'the pinned question says what it asks').toBe(question.text);
    expect(
      feed.view.pinned?.options,
      'and carries its options: a question rendered without them is a control with no handler ' +
        '(invariant 16)',
    ).toEqual(question.options);
  });

  it('takes the answer down and writes down who gave it', () => {
    const script = run200();
    const question = questionIn(script);
    const feed = createFeed(sealedScroller());
    feed.appendLines(script.slice(0, ASKED_AT));

    const chosen = question.options[0] ?? '';
    feed.answer(question.id, chosen);

    expect(feed.view.pinned, 'answered, so it stops blocking the view').toBeNull();
    expect(feed.view.attention, 'and the run goes back to being the agents turn').toBe('agents');
    expect(
      feed.view.answers,
      'the answer is written down with who gave it — three of those in the whole app, not ' +
        'eight [00-SYNTHESIS §2.2]',
    ).toEqual([{ questionId: question.id, option: chosen, who: 'you' }]);
  });

  it('pins the older of two unanswered questions and keeps it there', () => {
    const feed = createFeed(sealedScroller());
    feed.appendLines([
      line.asked(1, 0, 'Forge', 'Which database should I use?', ['Postgres', 'SQLite']),
      line.note(2, 500, 'Needle', 'The header row carries a stray quote.'),
      line.asked(3, 1_000, 'Needle', 'Should I keep the header row?', ['Keep', 'Drop']),
    ]);

    expect(
      feed.view.pinned?.id,
      'the OLDER question is the one the run is standing on. Showing the newest is showing the ' +
        'one that matters least',
    ).toBe(1);

    feed.answer(3, 'Keep');
    expect(
      feed.view.pinned?.id,
      'answering the younger one leaves the older one pinned. Otherwise the question the run ' +
        'is actually waiting on disappears the moment you deal with a newer one',
    ).toBe(1);

    feed.answer(1, 'Postgres');
    expect(feed.view.pinned, 'and only both answers together clear the pin').toBeNull();
  });
});
