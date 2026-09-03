/* Zamknięta karta zwalnia oba rejestry, których kluczem jest jej terminal.
 *
 * Kontrola fikstury stoi przed zamknięciem: każdy z dwudziestu modeli naprawdę powstaje. Test
 * bez tej kontroli przechodziłby dla drogi, która niczego nie otworzyła, więc nie dowodziłby
 * zwalniania. Magazyn biegu ma odwrotną kontrolę: samo pytanie, czy na karcie coś biegnie, nie
 * może go zakładać — wpis bez biegu jest właśnie śmieciem mierzonym przez ten przypadek. */
import { expect, it, vi } from 'vitest';

const { closed, stopped } = vi.hoisted(() => ({
  closed: vi.fn(() => Promise.resolve()),
  stopped: vi.fn(() => Promise.resolve()),
}));

vi.mock('../io', () => ({
  closeTerminal: closed,
  stop: stopped,
}));

const { createFeed } = await import('../feed/model');
const { feedsAlive, feedFor } = await import('../feed/live');
const { runTabs } = await import('./store');
const { createRunStore, LINE_LIMIT, sessionsAlive } = await import('../../../state/run');

it('closing twenty terminals leaves the two registries where they were', async () => {
  const feedsBefore = feedsAlive();
  const runsBefore = sessionsAlive();
  const terminals = Array.from({ length: 20 }, (_, index) => `terminal-z26-${String(index + 1)}`);

  for (const id of terminals) {
    runTabs.getState().open({ id, name: id, path: '/w/ledger-ui', agents: 0 });
    feedFor(id);
  }

  expect(
    feedsAlive().filter((id) => terminals.includes(id)).length,
    'all twenty streams have to exist before closing them, or this test measures an empty fixture',
  ).toBe(20);
  expect(
    sessionsAlive(),
    'opening a terminal must not create a run entry before anything has started',
  ).toEqual(runsBefore);

  for (const id of terminals) {
    /* 2026-09: jeden pracujący agent prowadzi przez potwierdzoną drogę `stopRunOf`; zero
     * ominęłoby dokładnie pytanie, które wcześniej zakładało zbędny wpis. */
    runTabs.getState().setAgents(id, 1);
    runTabs.getState().requestClose(id);
    await runTabs.getState().confirmClose();
  }

  expect
    .soft(
      feedsAlive(),
      'closing the cards left their streams in memory although no screen can reach them',
    )
    .toEqual(feedsBefore);
  expect
    .soft(
      sessionsAlive(),
      'asking whether a closing terminal owns work created run entries that nobody wrote',
    )
    .toEqual(runsBefore);
});

it('keeps only the answers that can still point into the line window', () => {
  const run = createRunStore();
  const feed = createFeed({
    scrollTop: () => 0,
    scrollTo: () => {},
    scrollIntoView: () => {},
  });

  for (let id = 1; id <= LINE_LIMIT + 1; id += 1) {
    run.getState().answer(id, `run answer ${String(id)}`);
    feed.answer(id, `stream answer ${String(id)}`);
  }

  expect
    .soft(
      run.getState().answers.length,
      'the run answer list grew past the same ceiling as its line window',
    )
    .toBe(LINE_LIMIT);
  expect
    .soft(
      run.getState().answers[0]?.questionId,
      'the run answer list dropped its newest item instead of the oldest one',
    )
    .toBe(2);
  expect
    .soft(
      feed.view.answers.length,
      'the stream answer list grew past the same ceiling as its line window',
    )
    .toBe(LINE_LIMIT);
  expect
    .soft(
      feed.view.answers[0]?.questionId,
      'the stream answer list dropped its newest item instead of the oldest one',
    )
    .toBe(2);
});
