/* WF-08: prawdziwy Enter → IPC → odmowa w rozmowie, bez utraty szkicu.
 * To jest druga połowa kryterium Rustowego: runtime nie powiela odmowy w strumieniu biegu,
 * bo nadawcą może być inny terminal, a jego Entry jest jedynym właścicielem odpowiedzi.
 */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';

const FOLDER = '/work/message-refusal';
const RUN = '01980000-0000-7000-8000-000000000001';
const TYPED = '@builder#2 explain the result';
const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 12 }, () => ({ value }));

afterAll(closeEverything, 30_000);

it.each([
  { result: 'unsupportedDuringRun', said: 'Builder does not accept messages while it is running.' },
  {
    result: 'fixedInputs',
    said: 'This comparison uses fixed inputs. Start a new run to change them.',
  },
  // L-01: kolejka jest po stronie Loadouta, więc jej sufity mają swoje zdania na ekranie.
  {
    result: 'queueFull',
    said: 'Builder already has 8 messages waiting. Wait for it to answer.',
  },
  {
    result: 'tooLong',
    said: 'That message is too long to send. Keep it under 8192 bytes.',
  },
])(
  'shows one $result refusal beside the real Entry and retains the exact unsent draft',
  async ({ result, said }) => {
    const app = await openApp({
      replies: {
        list_workspaces: replies([{ id: FOLDER, folder: FOLDER, name: 'Messages' }]),
        step_message_recipients: replies([
          {
            runId: RUN,
            nodeKey: 'builder#2',
            agent: 'Builder',
            canReceive: false,
            finished: false,
          },
        ]),
        send_to_step: replies({
          runId: RUN,
          nodeKey: 'builder#2',
          result,
          said,
        }),
      },
    });
    try {
      const field = app.page.getByRole('textbox', { name: 'Command line', exact: true });
      await expect
        .poll(async () =>
          (await app.calls()).some((call) => call.cmd === 'step_message_recipients'),
        )
        .toBe(true);
      await field.fill(TYPED);
      await field.press('Enter');
      await expect
        .poll(async () => (await app.calls()).filter((call) => call.cmd === 'send_to_step'), {
          message: await app.page.locator('body').innerText(),
        })
        .toHaveLength(1);
      // The production row includes its timestamp in the same text node.
      await expect.poll(async () => app.page.getByText(said, { exact: false }).count()).toBe(1);
      expect(await field.inputValue()).toBe(TYPED);
      const sent = (await app.calls()).filter((call) => call.cmd === 'send_to_step');
      expect(sent).toHaveLength(1);
      expect(sent[0]?.args).toEqual({
        folder: FOLDER,
        runId: RUN,
        nodeKey: 'builder#2',
        text: 'explain the result',
      });
      expect((await app.calls()).some((call) => call.cmd === 'say_to_orchestrator')).toBe(false);
    } finally {
      await app.close();
    }
  },
  90_000,
);
