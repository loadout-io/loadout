/* WF-20: jedna karta prawdziwej aplikacji od Startu do zachowanego wyniku.
 * Jedyną atrapą jest kabel Tauri istniejącego harnessu. Kolejność pakietów opisuje
 * współbieżny bieg; rzeczywiste nakładanie procesów sądzi partnerski test rustowy.
 */
import { afterAll, expect, it } from 'vitest';

import { closeEverything, openApp } from '../../../e2e/harness';
import type { RunningApp, TauriCall, TauriReply } from '../../../e2e/harness';

const A = {
  id: '/work/workflow-acceptance-a',
  folder: '/work/workflow-acceptance-a',
  name: 'Project A',
};
const B = {
  id: '/work/workflow-acceptance-b',
  folder: '/work/workflow-acceptance-b',
  name: 'Project B',
};
const RUN_ID = '019b0020-0000-7000-8000-000000000020';
const RUN_FOLDER = `20260906-120000__${RUN_ID}`;
const QUESTION_ID = '019b0020-0000-7000-8000-000000000021';
const RESULT = `${A.folder}/.loadout/runs/${RUN_FOLDER}/work/synthesis`;
const TITLE = 'Build and judge';
const STATUS = 'Project A: both branches are working; the judge is waiting for their results.';
const QUESTION = 'Keep both branches running in Project A?';
const STEPS = [
  { id: 'prepare', name: 'Prepare', kind: 'agent', at: { x: 0, y: 80 } },
  { id: 'branch-a', name: 'Branch A', kind: 'agent', at: { x: 260, y: 0 } },
  { id: 'branch-b', name: 'Branch B', kind: 'agent', at: { x: 260, y: 160 } },
  { id: 'judge', name: 'Judge', kind: 'check', at: { x: 520, y: 80 } },
  { id: 'synthesis', name: 'Synthesis', kind: 'agent', at: { x: 780, y: 80 } },
];
const LINKS = [
  { from: 'prepare', to: 'branch-a' },
  { from: 'prepare', to: 'branch-b' },
  { from: 'branch-a', to: 'judge' },
  { from: 'branch-b', to: 'judge' },
  { from: 'judge', to: 'synthesis' },
];
const ROW = {
  folder: RUN_FOLDER,
  when: '2026-09-06 12:00',
  title: TITLE,
  workflowFile: 'build-and-judge.json',
  state: 'succeeded',
  steps: STEPS.length,
  costUsd: null,
  said: null,
};
const OPENED = {
  ...ROW,
  steps: STEPS.map((step) => ({
    id: `finished-${step.id}`,
    tile: step.id,
    name: step.name,
    agent: step.name,
    state: 'succeeded',
    summary:
      step.id === 'synthesis'
        ? 'The saved result contains the outputs of both branches.'
        : `${step.name} finished.`,
    error: '',
    costUsd: null,
    lines: [],
  })),
  handoffs: [],
  branches: [],
  resultFolders: [{ workKey: 'synthesis', step: 'Synthesis', path: RESULT, state: 'changed' }],
};

const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 24 }, () => ({ value }));

async function chooseProject(app: RunningApp, folder: string): Promise<void> {
  await app.page.locator('[data-workspace-open]').click();
  await app.page.locator(`[data-workspace-pick="${folder}"]`).click();
}

async function sent(app: RunningApp, command: string, folder: string): Promise<TauriCall> {
  await expect
    .poll(
      async () =>
        (await app.calls()).filter((call) => call.cmd === command && call.args['folder'] === folder)
          .length,
    )
    .toBeGreaterThan(0);
  const call = (await app.calls())
    .filter((one) => one.cmd === command && one.args['folder'] === folder)
    .at(-1);
  if (!call) throw new Error(`the actual ${command} call was not recorded for ${folder}`);
  return call;
}

async function deliver(
  app: RunningApp,
  call: TauriCall,
  index: number,
  message: unknown[],
): Promise<void> {
  const raw = call.args['lines'];
  const match = typeof raw === 'string' ? /^__CHANNEL__:(\d+)$/.exec(raw) : null;
  if (!match) throw new Error(`${call.cmd} did not create its actual Tauri Channel`);
  await app.page.evaluate(
    ({ slot, index, message }) => {
      const callback = (globalThis as unknown as Record<string, unknown>)[slot];
      if (typeof callback !== 'function') throw new Error('the original Tauri Channel disappeared');
      (callback as (payload: unknown) => void)({ index, message });
    },
    { slot: '_' + match[1], index, message },
  );
}

const state = (stepId: string, agent: string, state: string): unknown => ({
  kind: 'stepState',
  agent,
  stepId,
  state,
});
const note = (agent: string, text: string): unknown => ({ kind: 'note', agent, text, body: [] });

afterAll(closeEverything, 30_000);

it('keeps parallel work and its Lead in Project A, then opens that exact retained result from history', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([A, B]),
      list_workflows: replies([
        {
          path: ROW.workflowFile,
          place: 'project',
          workflow: {
            name: TITLE,
            steps: STEPS,
            links: LINKS,
          },
        },
      ]),
      list_runs: replies([ROW]),
      read_run: replies(OPENED),
      run_workflow: [{ deferred: 'workflow-a' }],
      say_to_orchestrator: [{ deferred: 'status-a' }],
      answer_the_lead: replies(true),
    },
  });
  try {
    await chooseProject(app, A.folder);
    const entry = app.page.locator('[aria-label="Command line"]');
    await entry.fill('/run build-and-judge Keep both branch results');
    await entry.press('Enter');
    const run = await sent(app, 'run_workflow', A.folder);
    expect(run.args).toMatchObject({
      fileName: ROW.workflowFile,
      task: 'Keep both branch results',
    });
    const chatA = await sent(app, 'open_chat', A.folder);

    await deliver(app, run, 0, [
      { kind: 'run', agent: '', text: TITLE },
      state('prepare', 'Prepare', 'succeeded'),
      state('branch-a', 'Branch A', 'running'),
      note('Branch A', 'A started its isolated work.'),
      state('branch-b', 'Branch B', 'running'),
      note('Branch B', 'B started before A finished.'),
      note('Branch A', 'A is still working while B is active.'),
    ]);
    const stream = app.page.locator('[data-stream-column]');
    await expect
      .poll(async () => stream.innerText())
      .toContain('A is still working while B is active.');
    const parallel = await stream.innerText();
    expect(parallel.indexOf('A started its isolated work.')).toBeGreaterThanOrEqual(0);
    expect(parallel.indexOf('B started before A finished.')).toBeGreaterThan(
      parallel.indexOf('A started its isolated work.'),
    );
    expect(parallel.indexOf('A is still working while B is active.')).toBeGreaterThan(
      parallel.indexOf('B started before A finished.'),
    );
    for (const branch of ['branch-a', 'branch-b']) {
      expect(await app.page.locator(`[data-step="${branch}"]`).first().innerText()).toContain(
        'working',
      );
    }

    await entry.fill(`What is happening in Project A run ${RUN_ID}?`);
    await entry.press('Enter');
    const statusCall = await sent(app, 'say_to_orchestrator', A.folder);
    expect(statusCall.args['text']).toContain(RUN_ID);
    await chooseProject(app, B.folder);
    const chatB = await sent(app, 'open_chat', B.folder);
    expect(chatB.args['terminal']).not.toBe(chatA.args['terminal']);
    // Odpowiedź dochodzi do starego kanału DOPIERO przy aktywnym projekcie B.
    await deliver(app, chatA, 0, [
      {
        kind: 'runSource',
        agent: 'Lead',
        text: 'The status is from this saved run.',
        workspace: A.folder,
        runId: RUN_ID,
        runFolder: RUN_FOLDER,
        observedAt: '2026-09-06T12:00:02Z',
      },
      note('Lead', STATUS),
      {
        kind: 'asked',
        agent: 'Lead',
        text: QUESTION,
        options: ['Keep running', 'Stop this run'],
        question: {
          questionId: QUESTION_ID,
          runId: RUN_ID,
          checkpointId: null,
          operation: 'stop_run',
        },
      },
    ]);
    expect(await stream.innerText()).not.toContain(STATUS);
    expect(await stream.innerText()).not.toContain('A is still working while B is active.');
    expect(await app.page.locator('[data-asked]').count()).toBe(0);

    await chooseProject(app, A.folder);
    await expect.poll(async () => stream.innerText()).toContain(STATUS);
    await expect.poll(async () => app.page.locator('[data-asked]').innerText()).toContain(QUESTION);
    await app.page.getByRole('button', { name: /Keep running/ }).click();
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'answer_the_lead'))
      .toHaveLength(1);
    const answer = (await app.calls()).find((one) => one.cmd === 'answer_the_lead');
    expect(answer?.args).toEqual({
      terminal: chatA.args['terminal'],
      agent: 'Lead',
      answer: 'Keep running',
      questionId: QUESTION_ID,
    });
    expect(
      (await app.calls()).filter((one) => one.cmd === 'stop_run' || one.cmd === 'continue_run'),
    ).toHaveLength(0);
    await app.settle('status-a', { value: null });

    await deliver(app, run, 1, [
      state('branch-b', 'Branch B', 'succeeded'),
      note('Branch B', 'B handed over its result.'),
      state('branch-a', 'Branch A', 'succeeded'),
      note('Branch A', 'A handed over its result.'),
      state('judge', 'Judge', 'succeeded'),
      state('synthesis', 'Synthesis', 'succeeded'),
      note('Synthesis', 'The saved result contains the outputs of both branches.'),
      {
        kind: 'done',
        agent: 'Synthesis',
        text: 'Finished',
        vendorTurns: null,
        durationMs: 2500,
        costUsd: null,
        uncachedInput: 0,
        cacheRead: 0,
        cacheWrite: 0,
        output: 0,
        ended: 'well',
      },
    ]);
    await app.settle('workflow-a', { value: null });
    await expect
      .poll(async () => app.page.locator('[data-run-head]').innerText())
      .toMatch(/finished/i);
    expect(await stream.innerText()).not.toMatch(/\$0(?:[.,]0+)?\b/u);

    await entry.fill('/history');
    await entry.press('Enter');
    const historyRow = app.page.locator(`button[data-history-row="${RUN_FOLDER}"]`);
    await historyRow.waitFor({ state: 'visible' });
    expect(await historyRow.innerText()).not.toMatch(/\$0(?:[.,]0+)?\b/u);
    await historyRow.click();
    const history = app.page.locator(`[data-past-run="${RUN_FOLDER}"]`);
    await history.waitFor({ state: 'visible' });
    expect(await history.innerText()).toContain(
      'The saved result contains the outputs of both branches.',
    );
    expect(await history.innerText()).toContain(RESULT);
    expect(await history.innerText()).not.toMatch(/\$0(?:[.,]0+)?\b/u);
    expect((await sent(app, 'read_run', A.folder)).args).toEqual({
      folder: A.folder,
      run: RUN_FOLDER,
    });
    await history.getByRole('button', { name: 'Open result folder', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'open_result_folder'))
      .toHaveLength(1);
    expect((await app.calls()).find((one) => one.cmd === 'open_result_folder')?.args).toEqual({
      folder: A.folder,
      run: RUN_FOLDER,
      workKey: 'synthesis',
    });
  } finally {
    await app.close();
  }
}, 90_000);
