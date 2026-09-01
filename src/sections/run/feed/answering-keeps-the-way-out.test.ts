/* Odpowiedź na punkt kontrolny NIE ZABIERA drogi dalej — i naprawdę coś przewozi.
 *
 * ZMIERZONA WADA, KTÓRĄ TO KRYTERIUM ZAMYKA (2026-08-18). Kontrolka „dalej" renderowała się
 * dokładnie przy `pinned !== null`, a `answer()` zdejmuje przypięcie. Odpowiedź na pytanie
 * ODMONTOWYWAŁA więc jedyną kontrolkę wołającą `continue_run` i bieg parkował NA ZAWSZE: po
 * stronie Rusta (`commands::run::wait_for_a_person`) stoi on, dopóki nie podbije się licznik
 * zgód, a odpowiedź tego licznika nie rusza. Druga połowa tej samej wady: treść odpowiedzi nie
 * jechała nigdzie — człowiek pisał zdanie do agenta, który go nie przeczyta.
 *
 * SŁABA WERSJA: `expect(feed.view.pinned).toBeNull()` po odpowiedzi. Przechodzi dokładnie dla tej
 * zepsutej implementacji — bo o tym, czy droga dalej istnieje, nie mówi ani jedno pole, które ta
 * asercja czyta. Odróżnia to jedna rzecz: pytamy o `parked` PO odpowiedzi, czyli o fakt „bieg
 * dalej stoi i ktoś musi go puścić", i sprawdzamy, że dwa różne fakty gasną z dwóch różnych
 * powodów, a nie oba z jednego.
 */
import { describe, expect, it } from 'vitest';

import type { FeedLine } from '../../../state/run';
import { sealedScroller } from './fixtures/scroller';
import { createFeed } from './model';

const QUESTION = 'Does the plan look right?';
const TYPED = 'Yes, but keep the old parser behind a flag.';

/** Punkt kontrolny tak, jak wysyła go Rust: `options` jest ZAWSZE puste (`commands::run::ask`). */
function asked(id: number): FeedLine {
  return { kind: 'asked', agent: 'Plan', text: QUESTION, options: [], id, at: id * 1_000 };
}

function note(id: number, text: string): FeedLine {
  return { kind: 'note', agent: 'Plan', text, id, at: id * 1_000, body: [] };
}

function parked() {
  const feed = createFeed(sealedScroller());
  feed.appendLines([asked(1)]);
  return feed;
}

describe('answering a checkpoint leaves the way forward standing', () => {
  it('parks the run the moment the question lands, and says so in its own field', () => {
    const feed = parked();

    expect(feed.view.pinned?.text, 'the question is on screen').toBe(QUESTION);
    expect(
      feed.view.parked,
      'the run stands at the checkpoint, and that is a fact of its own — the whole run stops ' +
        'there, not just the step, so the window has to be able to say it without asking ' +
        'whether a question happens to be pinned right now.',
    ).toBe(true);
    expect(feed.view.toCarry, 'and nothing is waiting to be carried to the agent yet').toBe('');
  });

  it('keeps the run parked after the answer, so the control that lets it through survives', () => {
    const feed = parked();
    feed.answer(1, TYPED);

    expect(feed.view.pinned, 'the answered question comes off the screen').toBeNull();
    expect(
      feed.view.parked,
      'answering the question took away the fact that the run is standing. This is the defect ' +
        'that made every workflow with a checkpoint unfinishable: the continue control rendered ' +
        'on the pinned question, the answer unpinned it, and the run waited forever with ' +
        'nothing on screen able to release it.',
    ).toBe(true);
    expect(
      feed.view.toCarry,
      'and the sentence the person typed is queued for the agent. A question card that takes ' +
        'text nobody will ever read is a control without an effect (invariant 16), and it is ' +
        'worse than none because it looks like a conversation.',
    ).toBe(TYPED);
    expect(
      feed.view.answers.at(-1),
      'the answer is also kept as a record, signed by the person who gave it',
    ).toEqual({ questionId: 1, option: TYPED, who: 'you' });
  });

  it('lets the run go on exactly once, and does not spend that answer on the next question', () => {
    const feed = parked();
    feed.answer(1, TYPED);
    feed.carriedOn();

    expect(
      feed.view.parked,
      'the run was let through, so the control that let it through has no work left ' +
        '(invariant 16)',
    ).toBe(false);
    expect(
      feed.view.toCarry,
      'and the sentence has been carried, so it is off the queue. Left standing, it would ride ' +
        'along with the NEXT checkpoint as well — the person would see an agent answering a ' +
        'question with words meant for an earlier one.',
    ).toBe('');

    feed.appendLines([note(2, 'Carrying on with the state machine.'), asked(3)]);
    expect(feed.view.parked, 'the second checkpoint parks the run again').toBe(true);
    expect(feed.view.toCarry, 'with an empty queue of its own').toBe('');
  });

  it('clears both when the run goes down, answered or not', () => {
    const feed = parked();
    feed.answer(1, TYPED);
    feed.runEnded();

    expect(
      feed.view.parked,
      'a run that is gone is not standing on anyone’s question, so the continue control has to ' +
        'go with it — left on screen it would bump the go-ahead counter into a run that does ' +
        'not exist, and the NEXT run’s first checkpoint would fly past without asking',
    ).toBe(false);
    expect(feed.view.toCarry, 'and there is nobody left to carry the sentence to').toBe('');
  });
});
