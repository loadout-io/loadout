/* WF-10 RED: backend-bound question → actual button → original question id on the IPC wire.
 * The displayed local row number and the model's confirmed flag are not human consent.
 */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';

const FOLDER = '/work/lead-control';
const RUN = '01980000-0000-7000-8000-000000000001';
const QUESTION = '01980000-0000-7000-8000-000000000010';
const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 8 }, () => ({ value }));

afterAll(closeEverything, 30_000);

it('a reply through the Lead closes only the exact question in the live run view', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: FOLDER, folder: FOLDER, name: 'Control' }]),
    },
  });
  try {
    await expect
      .poll(async () => (await app.calls()).some((one) => one.cmd === 'open_chat'))
      .toBe(true);
    const raw = (await app.calls()).filter((one) => one.cmd === 'open_chat').at(-1)?.args['lines'];
    const match = typeof raw === 'string' ? /^__CHANNEL__:(\d+)$/.exec(raw) : null;
    expect(match).not.toBeNull();
    const slot = '_' + (match?.[1] ?? '');
    await app.page.evaluate(
      ({ slot, run, question }) => {
        const callback = (globalThis as unknown as Record<string, (value: unknown) => void>)[slot];
        if (!callback) throw new Error('real conversation Channel unavailable');
        callback({
          index: 0,
          message: [question, 'second-question'].map((id) => ({
            kind: 'asked',
            agent: 'Decision',
            text: id === question ? 'First decision' : 'Second decision',
            options: [],
            question: { questionId: id, runId: run, checkpointId: id, operation: 'continue_run' },
          })),
        });
      },
      { slot, run: RUN, question: QUESTION },
    );
    await expect
      .poll(async () => app.page.locator('[data-asked]').innerText())
      .toContain('First decision');
    await app.page.evaluate(
      ({ slot, run, question }) => {
        const callback = (globalThis as unknown as Record<string, (value: unknown) => void>)[slot];
        if (!callback) throw new Error('real conversation Channel unavailable');
        callback({
          index: 1,
          message: [
            {
              kind: 'questionAnswered',
              agent: 'Decision',
              runId: 'older-run',
              checkpointId: question,
              answer: 'Not this run',
            },
          ],
        });
        callback({
          index: 2,
          message: [
            {
              kind: 'questionAnswered',
              agent: 'Decision',
              runId: run,
              checkpointId: question,
              answer: 'The original human answer',
            },
          ],
        });
      },
      { slot, run: RUN, question: QUESTION },
    );
    await expect
      .poll(async () => app.page.locator('[data-asked]').innerText())
      .toContain('Second decision');
    expect(await app.page.locator('body').innerText()).toContain('The original human answer');
    expect(
      (await app.calls()).filter(
        (one) => one.cmd === 'answer_checkpoint' || one.cmd === 'continue_run',
      ),
    ).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('a checkpoint without options sends the original human text only to its exact checkpoint', async () => {
  const original = '  Keep the left path in my own words.  ';
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: FOLDER, folder: FOLDER, name: 'Control' }]),
      answer_the_lead: replies(false),
      answer_checkpoint: replies({
        runId: RUN,
        checkpointId: QUESTION,
        result: 'answerAccepted',
        said: 'The answer was accepted for this checkpoint.',
      }),
    },
  });
  try {
    await expect
      .poll(async () => (await app.calls()).some((one) => one.cmd === 'open_chat'))
      .toBe(true);
    const chat = (await app.calls()).filter((one) => one.cmd === 'open_chat').at(-1);
    const raw = chat?.args['lines'];
    const match = typeof raw === 'string' ? /^__CHANNEL__:(\d+)$/.exec(raw) : null;
    expect(match).not.toBeNull();
    await app.page.evaluate(
      ({ slot, run, question }) => {
        const callback = (globalThis as unknown as Record<string, unknown>)[slot];
        if (typeof callback !== 'function')
          throw new Error('real conversation Channel unavailable');
        (callback as (payload: unknown) => void)({
          index: 0,
          message: [
            {
              kind: 'asked',
              agent: 'Decision',
              text: 'Which path should this run take?',
              options: [],
              question: {
                questionId: question,
                runId: run,
                checkpointId: question,
                operation: 'continue_run',
              },
            },
          ],
        });
      },
      { slot: '_' + (match?.[1] ?? ''), run: RUN, question: QUESTION },
    );
    const field = app.page.getByRole('textbox', { name: 'Your answer', exact: true });
    await field.fill(original);
    await field.press('Enter');
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'answer_checkpoint'))
      .toHaveLength(1);
    expect((await app.calls()).find((one) => one.cmd === 'answer_checkpoint')?.args).toEqual({
      folder: FOLDER,
      runId: RUN,
      checkpointId: QUESTION,
      answer: original,
    });
    expect(
      (await app.calls()).filter(
        (one) => one.cmd === 'answer_the_lead' || one.cmd === 'continue_run',
      ),
    ).toHaveLength(0);
    await expect.poll(async () => app.page.locator('[data-asked]').count()).toBe(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('an old automatic suggestion is shown but cannot bypass addressed human confirmation', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: FOLDER, folder: FOLDER, name: 'Control' }]),
      stop_run: replies(true),
    },
  });
  try {
    await expect
      .poll(async () => (await app.calls()).some((one) => one.cmd === 'open_chat'))
      .toBe(true);
    const chat = (await app.calls()).filter((one) => one.cmd === 'open_chat').at(-1);
    const raw = chat?.args['lines'];
    const match = typeof raw === 'string' ? /^__CHANNEL__:(\d+)$/.exec(raw) : null;
    expect(match).not.toBeNull();
    await app.page.evaluate(
      (slot) => {
        const callback = (globalThis as unknown as Record<string, unknown>)[slot];
        if (typeof callback !== 'function')
          throw new Error('real conversation Channel unavailable');
        (callback as (payload: unknown) => void)({
          index: 0,
          message: [
            {
              kind: 'suggested',
              agent: 'Lead',
              text: 'Old stop request',
              command: '/stop',
              auto: true,
            },
          ],
        });
      },
      '_' + (match?.[1] ?? ''),
    );
    await expect
      .poll(async () => app.page.getByText('Old stop request', { exact: false }).count())
      .toBeGreaterThan(0);
    expect((await app.calls()).filter((one) => one.cmd === 'stop_run')).toHaveLength(0);
  } finally {
    await app.close();
  }
}, 90_000);

it('the actual Stop confirmation sends its backend question id and cannot become Continue', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: FOLDER, folder: FOLDER, name: 'Control' }]),
      answer_the_lead: replies(true),
    },
  });
  try {
    await expect
      .poll(async () => (await app.calls()).some((one) => one.cmd === 'open_chat'))
      .toBe(true);
    const chat = (await app.calls()).filter((one) => one.cmd === 'open_chat').at(-1);
    const channel = chat?.args['lines'];
    const match = typeof channel === 'string' ? /^__CHANNEL__:(\d+)$/.exec(channel) : null;
    expect(match).not.toBeNull();
    await app.page.evaluate(
      ({ slot, run, question }) => {
        const callback = (globalThis as unknown as Record<string, unknown>)[slot];
        if (typeof callback !== 'function')
          throw new Error('real conversation Channel unavailable');
        (callback as (value: unknown) => void)({
          index: 0,
          message: [
            {
              kind: 'asked',
              agent: 'Lead',
              text: 'Stop Review the result? Unfinished work will end.',
              options: ['Stop run', 'Keep running'],
              question: {
                questionId: question,
                runId: run,
                checkpointId: null,
                operation: 'stop_run',
              },
            },
          ],
        });
      },
      { slot: '_' + (match?.[1] ?? ''), run: RUN, question: QUESTION },
    );
    const confirm = app.page.getByRole('button', { name: /Stop run/ });
    await expect
      .poll(async () => confirm.count(), {
        message: await app.page.locator('body').innerText(),
      })
      .toBe(1);
    await confirm.click();
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'answer_the_lead'))
      .toHaveLength(1);
    const answer = (await app.calls()).find((one) => one.cmd === 'answer_the_lead');
    expect(answer?.args).toMatchObject({ questionId: QUESTION, agent: 'Lead', answer: 'Stop run' });
    expect((await app.calls()).filter((one) => one.cmd === 'continue_run')).toHaveLength(0);
    expect(await app.page.getByRole('button', { name: 'Continue', exact: true }).count()).toBe(0);
  } finally {
    await app.close();
  }
}, 90_000);
