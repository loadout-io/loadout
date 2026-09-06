/* WF-11 RED: prawdziwy kanał historii nie może nazwać zapisu przeczytaną wiadomością. */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';

const FOLDER = '/work/step-mail';
const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 8 }, () => ({ value }));
afterAll(closeEverything, 30_000);

it('shows who stored a message for whom, keeps its body collapsed, and never claims it was read', async () => {
  const app = await openApp({
    replies: { list_workspaces: replies([{ id: FOLDER, folder: FOLDER, name: 'Step messages' }]) },
  });
  try {
    await expect
      .poll(async () => (await app.calls()).some((one) => one.cmd === 'open_chat'))
      .toBe(true);
    const raw = (await app.calls()).filter((one) => one.cmd === 'open_chat').at(-1)?.args['lines'];
    const match = typeof raw === 'string' ? /^__CHANNEL__:(\d+)$/.exec(raw) : null;
    expect(match).not.toBeNull();
    await app.page.evaluate(
      (slot) => {
        const callback = (globalThis as unknown as Record<string, (value: unknown) => void>)[slot];
        if (!callback) throw new Error('the production chat channel is missing');
        callback({
          index: 0,
          message: [
            {
              kind: 'messageStored',
              agent: 'Builder',
              text: 'Builder stored a message for Reviewer.',
              runId: '019b0006-0000-7000-8000-000000000011',
              sequence: 1,
              fromNode: 'builder#2',
              toNode: 'reviewer#2',
              body: 'Inspect the exact saved boundary.',
            },
          ],
        });
      },
      '_' + (match?.[1] ?? ''),
    );
    await expect
      .poll(async () =>
        app.page.getByText('Builder stored a message for Reviewer.', { exact: true }).count(),
      )
      .toBe(1);
    expect(await app.page.locator('body').innerText()).not.toContain(
      'Inspect the exact saved boundary.',
    );
    await app.page
      .locator('article')
      .filter({ hasText: 'Builder stored a message for Reviewer.' })
      .getByRole('button', { name: 'Show more', exact: true })
      .click();
    await expect
      .poll(async () => app.page.locator('body').innerText())
      .toContain('Inspect the exact saved boundary.');
    expect(await app.page.locator('body').innerText()).not.toContain('Reviewer read');
    expect(
      (await app.calls()).some((one) =>
        ['start_replay', 'continue_run', 'run_workflow'].includes(one.cmd),
      ),
    ).toBe(false);
  } finally {
    await app.close();
  }
}, 90_000);
