/* T-161: a long, real workflow may overflow its own strip, never the Run screen.
 *
 * This mounts the production screen in Chromium and starts the workflow through the visible
 * button. Rust remains at the existing harness boundary, but the running step reaches React
 * through the real `Channel` callback recorded in the `run_workflow` call. That distinction is
 * the point: a hand-built Strip would not prove that the rail, command row and controls still
 * share one finite desktop viewport (invariant 29).
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import { LEAD_LABEL } from '../../src/sections/run/lead';
import { REFLECTION_LABEL } from '../../src/sections/run/reflection/toggle';
import { TASK_LABEL } from '../../src/sections/run/start';
import { STRIP_HEIGHT } from '../../src/sections/run/strip/strip';
import type { RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/Users/somebody/Projects/loadout-t161-long-run';
const WORKSPACE = { id: FOLDER, name: 'Long run', folder: FOLDER };
const AGENT = {
  id: 'agent-t161',
  name: 'Long workflow operator',
  summary: 'Keeps a long workflow moving',
  skills: [],
};
const STEP_COUNT = 32;
const RUNNING_AT = STEP_COUNT - 3;

const STEPS = Array.from({ length: STEP_COUNT }, (_, at) => ({
  kind: 'agent' as const,
  id: `long-step-${String(at + 1).padStart(2, '0')}`,
  name: `Phase ${String(at + 1).padStart(2, '0')} with a deliberately long descriptive name`,
  agent: AGENT.id,
  overrides: {},
  copies: 1,
  instructions: `Complete phase ${String(at + 1)}.`,
  skills: 'all' as const,
  folder: { use: 'project' as const },
  handover: 'notes' as const,
  at: { x: at * 24, y: 24 },
}));

const WORKFLOW = {
  path: 'long-workflow.json',
  workflow: {
    format: 1 as const,
    id: 'wf-t161-long-run',
    name: 'Long viewport proof',
    steps: STEPS,
    links: [],
  },
};

const RUN = 'main[data-section="run"]';
const STRIP = '[data-strip]';
const BLOCKS = '[data-step-list]';
const WORKFLOW_CONTROLS = '[data-workflow-controls]';
const WORK = '[data-work]';
const RAIL = '[data-plan-column]';
const COMMAND = 'input[aria-label="Command line"]';
const START = 'button[data-workflow-run="manual"]';
const LEAD = `select[aria-label="${LEAD_LABEL}"]`;
const TASK = `input[aria-label="${TASK_LABEL}"]`;
const AT_ONCE = 'input#at-once[type="range"]';
const BUDGET = 'input[data-budget]';
const COPY_DIAGNOSTICS = 'button[aria-label="Copy diagnostics"]';
const LEARN_FROM_THIS_RUN = `label:has-text("${REFLECTION_LABEL}") input[type="checkbox"]`;
const BEFORE_START_CONTROLS = [
  COPY_DIAGNOSTICS,
  LEARN_FROM_THIS_RUN,
  LEAD,
  TASK,
  START,
  AT_ONCE,
  BUDGET,
] as const;
const LOCKED_WHILE_RUNNING = [LEARN_FROM_THIS_RUN, TASK, AT_ONCE, BUDGET] as const;
const FIRST_CALL_LIMIT = 5_000;

function copies<T>(value: T, count = 24): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function scene(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workspaces: copies([WORKSPACE]),
    list_workflows: copies([WORKFLOW]),
    /* 2026-08-28: otwarcie oddaje plik RAZEM z rewizją, na której okno go czyta — bez niej
     * zapis nie ma czego porównać z dyskiem (`commands::workflows::OpenWorkflow`). */
    load_workflow: copies({ workflow: WORKFLOW.workflow, revision: 'r1' }, 4),
    check_workflow: copies([], 8),
    list_agents: copies([AGENT]),
    list_skills: copies([]),
    run_workflow: [{ deferred: 'long-run' }],
  };
}

async function runCall(app: RunningApp): Promise<TauriCall> {
  const deadline = Date.now() + FIRST_CALL_LIMIT;
  while (Date.now() < deadline) {
    const call = (await app.calls()).find((one) => one.cmd === 'run_workflow');
    if (call !== undefined) return call;
    await app.page.waitForTimeout(25);
  }
  throw new Error('the visible Run control never reached run_workflow');
}

/** Send one ordered Channel message exactly as the Tauri API wrapper receives it. */
async function markStepRunning(app: RunningApp, call: TauriCall): Promise<void> {
  const channel = call.args['lines'];
  const match = typeof channel === 'string' ? /^__CHANNEL__:(\d+)$/.exec(channel) : null;
  expect(match, 'run_workflow did not carry the real Tauri Channel handle').not.toBeNull();
  const id = match?.[1];
  if (id === undefined) throw new Error('the Channel handle had no callback id');

  await app.page.evaluate(
    ({ slot, stepId, agent }) => {
      const host = globalThis as unknown as Record<string, unknown>;
      const callback = host[slot];
      if (typeof callback !== 'function') {
        throw new Error(`the live Tauri Channel callback ${slot} is not registered`);
      }
      (callback as (payload: unknown) => void)({
        index: 0,
        message: [{ kind: 'stepState', agent, stepId, state: 'running' }],
      });
    },
    { slot: `_${id}`, stepId: STEPS[RUNNING_AT]?.id ?? '', agent: AGENT.name },
  );

  await app.page.locator(`${BLOCKS} [data-step="${STEPS[RUNNING_AT]?.id ?? ''}"]`).waitFor({
    state: 'visible',
    timeout: FIRST_CALL_LIMIT,
  });
}

interface Rect {
  readonly left: number;
  readonly right: number;
  readonly top: number;
  readonly bottom: number;
  readonly width: number;
  readonly height: number;
}

interface Geometry {
  readonly viewport: { readonly width: number; readonly height: number };
  readonly documentWidth: number;
  readonly bodyWidth: number;
  readonly runClientWidth: number;
  readonly runScrollWidth: number;
  readonly workClientWidth: number;
  readonly workScrollWidth: number;
  readonly stripClientWidth: number;
  readonly stripScrollWidth: number;
  readonly strip: Rect;
  readonly main: Rect;
  readonly rail: Rect;
  readonly command: Rect;
  readonly track: Rect;
  readonly running: Rect;
  readonly blockCount: number;
  readonly blockNames: readonly string[];
  readonly blockTitles: readonly string[];
  readonly trackClientWidth: number;
  readonly trackScrollWidth: number;
  readonly trackClientHeight: number;
  readonly trackScrollHeight: number;
  readonly trackScrollBehavior: string;
}

async function geometry(app: RunningApp): Promise<Geometry> {
  return app.page.evaluate(
    ({
      runSelector,
      stripSelector,
      blocksSelector,
      workSelector,
      railSelector,
      commandSelector,
      runningStepId,
    }) => {
      function required(selector: string): HTMLElement {
        const found = document.querySelector<HTMLElement>(selector);
        if (found === null) throw new Error(`the mounted Run screen is missing ${selector}`);
        return found;
      }
      function rect(element: Element): Rect {
        const box = element.getBoundingClientRect();
        return {
          left: box.left,
          right: box.right,
          top: box.top,
          bottom: box.bottom,
          width: box.width,
          height: box.height,
        };
      }

      const main = required(runSelector);
      const strip = required(stripSelector);
      const track = required(blocksSelector);
      const work = required(workSelector);
      const rail = required(railSelector);
      const command = required(commandSelector);
      const running = required(`${blocksSelector} [data-step="${runningStepId}"]`);
      const blocks = Array.from(track.querySelectorAll<HTMLElement>(':scope > [data-step]'));

      return {
        viewport: { width: window.innerWidth, height: window.innerHeight },
        documentWidth: document.documentElement.scrollWidth,
        bodyWidth: document.body.scrollWidth,
        runClientWidth: main.clientWidth,
        runScrollWidth: main.scrollWidth,
        workClientWidth: work.clientWidth,
        workScrollWidth: work.scrollWidth,
        stripClientWidth: strip.clientWidth,
        stripScrollWidth: strip.scrollWidth,
        strip: rect(strip),
        main: rect(main),
        rail: rect(rail),
        command: rect(command),
        track: rect(track),
        running: rect(running),
        blockCount: blocks.length,
        blockNames: blocks.map(
          (block) => block.querySelector<HTMLElement>('[title]')?.innerText.trim() ?? '',
        ),
        blockTitles: blocks.map(
          (block) => block.querySelector<HTMLElement>('[title]')?.getAttribute('title') ?? '',
        ),
        trackClientWidth: track.clientWidth,
        trackScrollWidth: track.scrollWidth,
        trackClientHeight: track.clientHeight,
        trackScrollHeight: track.scrollHeight,
        trackScrollBehavior: getComputedStyle(track).scrollBehavior,
      };
    },
    {
      runSelector: RUN,
      stripSelector: STRIP,
      blocksSelector: BLOCKS,
      workSelector: WORK,
      railSelector: RAIL,
      commandSelector: COMMAND,
      runningStepId: STEPS[RUNNING_AT]?.id ?? '',
    },
  );
}

function expectContained(measured: Geometry, label: string): void {
  const slack = 1;
  expect(
    measured.documentWidth,
    `${label}: the document grew wider than the desktop viewport`,
  ).toBeLessThanOrEqual(measured.viewport.width + slack);
  expect(measured.bodyWidth, `${label}: the body hides horizontal overflow`).toBeLessThanOrEqual(
    measured.viewport.width + slack,
  );
  expect(
    measured.runScrollWidth,
    `${label}: Run owns horizontal overflow; strip ${String(measured.stripClientWidth)}/${String(measured.stripScrollWidth)}, work ${String(measured.workClientWidth)}/${String(measured.workScrollWidth)}, track ${String(measured.trackClientWidth)}/${String(measured.trackScrollWidth)}`,
  ).toBeLessThanOrEqual(measured.runClientWidth + slack);
  expect(
    measured.workScrollWidth,
    `${label}: the two-column work area widened`,
  ).toBeLessThanOrEqual(measured.workClientWidth + slack);
  expect(measured.stripScrollWidth, `${label}: the strip itself widened`).toBeLessThanOrEqual(
    measured.stripClientWidth + slack,
  );

  expect(measured.rail.width, `${label}: the plan column collapsed`).toBeGreaterThan(0);
  expect(
    measured.rail.left,
    `${label}: the plan column escaped left of Run`,
  ).toBeGreaterThanOrEqual(measured.main.left - slack);
  expect(
    measured.rail.right,
    `${label}: the plan column escaped the visible Run edge`,
  ).toBeLessThanOrEqual(Math.min(measured.main.right, measured.viewport.width) + slack);

  expect(measured.command.width, `${label}: the Command line collapsed`).toBeGreaterThan(0);
  expect(
    measured.command.top,
    `${label}: the Command line is above the viewport`,
  ).toBeGreaterThanOrEqual(-slack);
  expect(
    measured.command.bottom,
    `${label}: the Command line fell below the vertical viewport`,
  ).toBeLessThanOrEqual(measured.viewport.height + slack);

  expect(STRIP_HEIGHT, 'the named chrome budget changed').toBe(52);
  expect(measured.strip.height, `${label}: the strip created another chrome row`).toBe(
    STRIP_HEIGHT,
  );
  expect(measured.blockCount, `${label}: steps disappeared from the DOM`).toBe(STEP_COUNT);
  const expectedStepNames = STEPS.map((step) => step.name);
  expect(measured.blockNames, `${label}: visible step names changed or moved`).toEqual(
    expectedStepNames,
  );
  expect(measured.blockTitles, `${label}: full step titles changed or moved`).toEqual(
    expectedStepNames,
  );
  /* 2026-08-31 — LISTA PRZEWIJA SIĘ TERAZ W PIONIE, nie w poziomie: torek bloków w pasku zszedł
     razem z resztą drugiego rysunku planu, a kroki stoją w kolumnie pracy jedne pod drugimi.
     Pytanie zostało to samo — trzydzieści dwa kroki mają się PRZEWIJAĆ, a nie zostać przycięte
     — zmieniła się oś. */
  expect(
    measured.trackScrollHeight,
    `${label}: the long plan was clipped instead of scrollable`,
  ).toBeGreaterThan(measured.trackClientHeight);
  expect(
    measured.trackScrollWidth,
    `${label}: the plan column widened instead of keeping its cards inside`,
  ).toBeLessThanOrEqual(measured.trackClientWidth + slack);
  expect(
    measured.trackScrollBehavior,
    `${label}: following the running step must not animate`,
  ).toBe('auto');
  expect(
    measured.running.top,
    `${label}: the running step stayed above the visible part of the plan column`,
  ).toBeGreaterThanOrEqual(measured.track.top - slack);
  expect(
    measured.running.bottom,
    `${label}: the running step stayed below the visible part of the plan column`,
  ).toBeLessThanOrEqual(measured.track.bottom + slack);
}

/** Playwright's trial click proves actionability without changing the running scene. */
async function expectControlsReachable(
  app: RunningApp,
  label: string,
  phase: 'before-start' | 'running',
): Promise<void> {
  if (phase === 'before-start') {
    const allControls = app.page.locator(
      `${WORKFLOW_CONTROLS} button, ${WORKFLOW_CONTROLS} input, ${WORKFLOW_CONTROLS} select`,
    );
    expect(await allControls.count(), `${label}: the real control set changed`).toBe(
      BEFORE_START_CONTROLS.length,
    );

    for (const selector of BEFORE_START_CONTROLS) {
      const control = app.page.locator(`${WORKFLOW_CONTROLS} ${selector}`);
      expect(await control.count(), `${label}: missing or duplicated ${selector}`).toBe(1);
      expect(await control.isVisible(), `${label}: ${selector} is not visible`).toBe(true);
      expect(await control.isEnabled(), `${label}: ${selector} is unexpectedly disabled`).toBe(
        true,
      );
    }
  }

  const controls = app.page.locator(
    `${WORKFLOW_CONTROLS} button:enabled, ${WORKFLOW_CONTROLS} input:enabled, ${WORKFLOW_CONTROLS} select:enabled`,
  );
  const count = await controls.count();
  expect(count, `${label}: the strip has no enabled controls`).toBeGreaterThan(0);

  for (let at = 0; at < count; at += 1) {
    const control = controls.nth(at);
    const identity = await control.evaluate((element) => ({
      tag: element.tagName.toLowerCase(),
      label: element.getAttribute('aria-label'),
      text: element.textContent?.trim() ?? '',
    }));
    const named = `${identity.tag} ${identity.label ?? identity.text}`.trim();
    await control.scrollIntoViewIfNeeded();
    await control.click({ trial: true });
    const box = await control.boundingBox();
    expect(
      box,
      `${label}: control ${String(at + 1)} (${named}) has no rendered box`,
    ).not.toBeNull();
    if (box !== null) {
      expect(box.width, `${label}: control ${String(at + 1)} (${named}) collapsed`).toBeGreaterThan(
        0,
      );
      expect(
        box.x,
        `${label}: control ${String(at + 1)} (${named}) is left of the viewport`,
      ).toBeGreaterThanOrEqual(-1);
      expect(
        box.x + box.width,
        `${label}: control ${String(at + 1)} (${named}) is cut off beyond the viewport`,
      ).toBeLessThanOrEqual((await app.page.evaluate(() => window.innerWidth)) + 1);
    }
  }

  if (phase === 'running') {
    for (const selector of LOCKED_WHILE_RUNNING) {
      const control = app.page.locator(`${WORKFLOW_CONTROLS} ${selector}`);
      expect(await control.count(), `${label}: missing or duplicated locked ${selector}`).toBe(1);
      expect(await control.isDisabled(), `${label}: ${selector} did not lock during the run`).toBe(
        true,
      );
      await control.scrollIntoViewIfNeeded();
      const box = await control.boundingBox();
      expect(box, `${label}: locked ${selector} has no rendered box`).not.toBeNull();
      if (box !== null) {
        expect(box.width, `${label}: locked ${selector} collapsed`).toBeGreaterThan(0);
        expect(
          box.x,
          `${label}: locked ${selector} is left of the viewport`,
        ).toBeGreaterThanOrEqual(-1);
        expect(
          box.x + box.width,
          `${label}: locked ${selector} is cut off beyond the viewport`,
        ).toBeLessThanOrEqual((await app.page.evaluate(() => window.innerWidth)) + 1);
      }
    }
  }
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('a long workflow stays inside the real Run viewport', () => {
  it('keeps the plan column, command row, running step and strip controls reachable at both supported sizes', async () => {
    const app = await openApp({ replies: scene() });
    try {
      await app.page.setViewportSize({ width: 1100, height: 700 });
      await app.page.locator(RUN).waitFor({ state: 'attached', timeout: FIRST_CALL_LIMIT });
      await app.page.locator(START).waitFor({ state: 'visible', timeout: FIRST_CALL_LIMIT });
      /* Before the run locks its inputs, every available control must have an actionable
       * position. During the run we repeat this for the controls that remain enabled. */
      await expectControlsReachable(app, '1100x700 before Start', 'before-start');
      await app.page.locator(START).click();
      const call = await runCall(app);
      await markStepRunning(app, call);

      for (const size of [
        { width: 1100, height: 700 },
        { width: 1440, height: 900 },
      ]) {
        await app.page.setViewportSize(size);
        await app.page.waitForTimeout(50);
        const label = `${String(size.width)}x${String(size.height)}`;
        expectContained(await geometry(app), label);
        await expectControlsReachable(app, label, 'running');
      }
    } finally {
      await app.close();
    }
  }, 90_000);
});
