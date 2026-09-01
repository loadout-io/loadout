/* T-164: the workflow library belongs to the workspace the run will happen in.
 *
 * WHY A REAL BROWSER, when a function-level criterion is cheaper. Because the defect §6d names
 * is not a wrong return value — it is a screen. You open project B and you are looking at the
 * workflows of project A. Nothing about that is visible from a function that was never called
 * with a second folder, and this repo has measured that gap four times on a green gate
 * (invariant 29).
 *
 * THREE CONTROLS AGAINST AN EMPTY ASSERTION, because "B does not show A" is true of a screen
 * that simply blanks itself, of a screen that never asked again, and of a harness whose queue
 * hands the second answer to an implementation that never sent a folder:
 *
 *   the tape — the listing that repaints after the switch must carry `{ folder: B }`, and the
 *     one before it `{ folder: A }`. Without this the queue below satisfies code that asks the
 *     same question twice and cannot tell the two projects apart;
 *   a shared row — B's answer keeps the library file `Shared cleanup`, which must STAY on the
 *     screen with the word `Shared`. "B does not show A" would otherwise pass on an empty list;
 *   the way back — switching to A again brings `Ship a feature` back, so the criterion does not
 *     praise code that just clears the screen on every switch.
 *
 * HOW THE DEFERRED REPLIES ARE ADDRESSED. `list_workflows` also runs when the window opens on
 * Run (`sections/run/index.tsx` and `sections/run/start.tsx` both read the folder once), so the
 * listing this spec cares about is NOT the first one. The queue is therefore a run of named
 * deferrals and the name is resolved from the tape: the i-th `list_workflows` of this tab holds
 * `listing-i`. That keeps the spec honest when a future section reads the same catalogue — it
 * counts what really crossed the boundary instead of assuming a number.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const A = '/Users/somebody/Projects/t164-a';
const B = '/Users/somebody/Projects/t164-b';
const WORKSPACE_A = { id: A, name: 'T164 A', folder: A };
const WORKSPACE_B = { id: B, name: 'T164 B', folder: B };

const WORKFLOWS_SWITCH = '[data-section-switch="workflows"]';
const WORKFLOWS_SCREEN = 'main[data-section="workflows"]';
const TILE = 'main [data-tile]';

/** How long we wait for what a click asks for. Generous: this is a real dev server. */
const APPEARS = 8_000;

/** Ile odroczonych odpowiedzi trzyma kolejka. Z zapasem na sekcje, które czytają katalog. */
const LISTINGS = 12;

interface WireStep {
  readonly kind: 'agent';
  readonly id: string;
  readonly name: string;
  readonly agent: string;
}

function step(id: string, name: string): WireStep {
  return { kind: 'agent', id, name, agent: 'agent-t164' };
}

/** Jedna zdrowa pozycja katalogu — lustro `Definition::Healthy` z `library::definition`. */
function listed(place: 'library' | 'project', path: string, id: string, name: string): unknown {
  return {
    kind: 'healthy',
    revision: 'revision-' + id,
    value: {
      path,
      place,
      workflow: {
        format: 1,
        id,
        name,
        steps: [step('one', 'First')],
        links: [],
      },
    },
  };
}

/** Plik biblioteczny — ten sam w obu projektach, bo biblioteka jest jedna. */
const SHARED = listed('library', 'shared-cleanup.json', 'wf-shared', 'Shared cleanup');
/** Tylko w A. Jego zniknięcie po przełączeniu jest całym zdaniem tego zadania. */
const ONLY_IN_A = listed('project', 'ship-a-feature.json', 'wf-a', 'Ship a feature');
/** Tylko w B, żeby „B nie pokazuje A" nie przechodziło na pustym ekranie. */
const ONLY_IN_B = listed('project', 'nightly-sweep.json', 'wf-b', 'Nightly sweep');

const CATALOG_A = [ONLY_IN_A, SHARED];
const CATALOG_B = [ONLY_IN_B, SHARED];

function copies<T>(value: T, count = 12): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function scene(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workspaces: copies([WORKSPACE_A, WORKSPACE_B]),
    list_agents: copies([]),
    list_skills: copies([]),
    /* Każde listowanie tej karty czeka pod własną nazwą; którą z nich domknąć, mówi taśma. */
    list_workflows: Array.from({ length: LISTINGS }, (_unused, index) => ({
      deferred: 'listing-' + String(index + 1),
    })),
  };
}

/** Ile razy ta nazwa przeszła przez granicę do tej pory. */
async function crossings(app: RunningApp, command: string): Promise<number> {
  return (await app.calls()).filter((one) => one.cmd === command).length;
}

/** Czeka, aż granicę przetnie `count`-te wywołanie tej komendy, i oddaje jego argumenty. */
async function nth(
  app: RunningApp,
  command: string,
  count: number,
): Promise<Record<string, unknown>> {
  const deadline = Date.now() + APPEARS;
  while (Date.now() < deadline) {
    const calls = (await app.calls()).filter((one) => one.cmd === command);
    if (calls.length >= count) return calls[count - 1]?.args ?? {};
    await app.page.waitForTimeout(20);
  }
  const calls = (await app.calls()).filter((one) => one.cmd === command);
  expect(
    calls.length,
    command +
      ' crossed the boundary ' +
      String(calls.length) +
      ' times, and this spec waited for ' +
      String(count) +
      '. A listing that never happens is a screen that shows the previous project.',
  ).toBeGreaterThanOrEqual(count);
  return calls[count - 1]?.args ?? {};
}

/** Nazwy workflow, które człowiek naprawdę widzi na ekranie, w kolejności z DOM-u. */
async function onScreen(app: RunningApp): Promise<readonly string[]> {
  return app.page.locator(TILE).evaluateAll((tiles) => tiles.map((tile) => tile.textContent ?? ''));
}

/** Czeka, aż ekran pokaże dokładnie te workflow — i oddaje to, co naprawdę pokazuje. */
async function settledScreen(
  app: RunningApp,
  wanted: readonly string[],
): Promise<readonly string[]> {
  const deadline = Date.now() + APPEARS;
  let showing: readonly string[] = [];
  while (Date.now() < deadline) {
    showing = await onScreen(app);
    if (wanted.every((name) => showing.some((tile) => tile.includes(name)))) {
      if (showing.length === wanted.length) return showing;
    }
    await app.page.waitForTimeout(25);
  }
  return showing;
}

async function switchTo(app: RunningApp, workspace: string): Promise<void> {
  await app.page.locator('[data-workspace-open]').click();
  await app.page.locator('[data-workspace-pick="' + workspace + '"]').click();
}

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku —
 * ten sam powód i ten sam kształt, co w `t163-settings-default-lead-reaches-run.spec.ts`. */
beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('a workflow belongs to the workspace it will be run in', () => {
  it("does not show the first workspace's workflow after switching to the second", async () => {
    const app = await openApp({ replies: scene() });
    try {
      const page = app.page;

      /* ── ekran Workflows w A ────────────────────────────────────────────────────────────── */
      await page.locator(WORKFLOWS_SWITCH).click();
      await page.locator(WORKFLOWS_SCREEN).waitFor({ state: 'attached', timeout: APPEARS });

      const forA = await crossings(app, 'list_workflows');
      expect(
        forA,
        'opening Workflows asked for no catalogue at all, so nothing below could tell the two ' +
          'projects apart.',
      ).toBeGreaterThan(0);
      await app.settle('listing-' + String(forA), { value: CATALOG_A });

      expect(
        await settledScreen(app, ['Ship a feature', 'Shared cleanup']),
        'the workflows of the open workspace never reached the screen, so the switch below ' +
          'would prove nothing.',
      ).toHaveLength(2);
      expect(
        (await nth(app, 'list_workflows', forA))['folder'],
        'the catalogue was read without saying which folder it belongs to. One global folder ' +
          'is exactly the defect this task closes: open B and you are reading A.',
      ).toBe(A);

      /* ── człowiek przełącza się na B ─────────────────────────────────────────────────────── */
      await switchTo(app, B);
      const forB = forA + 1;
      const asked = await nth(app, 'list_workflows', forB);
      expect(
        asked['folder'],
        'switching the workspace did not ask the disk again for THAT folder. The screen then ' +
          'keeps showing the previous project and nothing says so.',
      ).toBe(B);
      await app.settle('listing-' + String(forB), { value: CATALOG_B });

      const showing = await settledScreen(app, ['Nightly sweep', 'Shared cleanup']);
      expect(
        showing.join(' | '),
        'the workflow saved in the first workspace is still on the screen of the second. This ' +
          'is the whole defect: one global workflow library leaking between workspaces.',
      ).not.toContain('Ship a feature');
      expect(
        showing.join(' | '),
        'the second workspace shows nothing of its own, so "A is gone" is true of a screen ' +
          'that simply went blank.',
      ).toContain('Nightly sweep');

      /* ── plik biblioteczny zostaje, i mówi o sobie, że jest wspólny ──────────────────────── */
      const shared = page.locator('[data-workflow-place="library"]');
      expect(
        await shared.count(),
        'the shared library file vanished with the switch, or it is on the screen without ' +
          'saying which shelf it comes from — and then Delete quietly removes it from the ' +
          'project next door.',
      ).toBe(1);
      expect(
        await shared.textContent(),
        'the shared file does not carry the word every workspace needs to read before pressing ' +
          'Delete on it.',
      ).toContain('Shared');

      /* ── droga powrotna: kryterium nie chwali kodu, który po prostu czyści listę ─────────── */
      await switchTo(app, A);
      const backToA = forB + 1;
      expect(
        (await nth(app, 'list_workflows', backToA))['folder'],
        'switching back did not read the first folder again.',
      ).toBe(A);
      await app.settle('listing-' + String(backToA), { value: CATALOG_A });
      expect(
        (await settledScreen(app, ['Ship a feature', 'Shared cleanup'])).join(' | '),
        'coming back to the first workspace does not bring its workflow back, so the screen ' +
          'forgets instead of scoping.',
      ).toContain('Ship a feature');
    } finally {
      await app.close();
    }
  }, 90_000);
});
