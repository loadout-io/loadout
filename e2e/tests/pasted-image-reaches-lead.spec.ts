/* T-34 AC-6: obraz jest prawdziwym File wklejonym do prawdziwego pola Command line.
 *
 * Ten plik nie importuje helpera obrazów ani komponentu podglądu. Robiłby wtedy dokładnie to,
 * czego zabrania niezmiennik 29: pytał producenta wartości i omijał ekran, na którym człowiek
 * ma zobaczyć podgląd, Remove, odmowę i zachowany szkic. Chromium tworzy ClipboardEvent, React
 * obsługuje go na produkcyjnej ścieżce, a harness ogląda dopiero ostatnią granicę do Rusta.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { OpenAppOptions, RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const WORK = 'main[data-section="run"]';
const FIELD = '[aria-label="Command line"]';
const PREVIEW = '[data-image-preview]';
const REMOVE = 'button[aria-label="Remove pasted image 1"]';
const FOLDER = '/Users/somebody/Projects/loadout-image-e2e';
const WORKSPACE = { id: FOLDER, name: 'Image fixture', folder: FOLDER };
const OTHER_FOLDER = '/Users/somebody/Projects/other-image-e2e';
const OTHER_WORKSPACE = { id: OTHER_FOLDER, name: 'Other image fixture', folder: OTHER_FOLDER };
const AGENT = {
  schema: 1,
  id: '0198a1f2-3b4c-7d5e-8f60-112233445566',
  name: 'Lead fixture',
  summary: 'Leads the test conversation',
  color: 'slate',
  instructions: 'Answer the person.',
  runsWith: 'claude-code',
  model: 'sonnet',
  thinking: 'balanced',
  fileAccess: 'look-only',
  giveUpAfterMinutes: 10,
  tools: 'everything',
  skills: [],
  connections: [],
  writeResultsTo: 'handoffs/lead.md',
};
const PNG_BASE64 =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9Z4pAAAAAASUVORK5CYII=';
const SECRET_NAME = 'customer-secret-screenshot.png';
const RETRY_TEXT = 'Please inspect this screenshot.';
const SEND_FAILURE = 'Loadout could not send that message.';
const PRIVATE_FAILURE = 'PRIVATE vendor failure must not replace the UI sentence';
const COMMAND_REFUSAL =
  'Images cannot be sent with commands. Remove them or send them to the Lead first.';
const STEP_REFUSAL = 'Images can only be sent to the Lead right now.';
const APPEARS = 4_000;
const SETTLE = 500;
const RUN_STATE = '/src/state/run.ts';
const RUN_HANDLE = '__LOADOUT_IMAGE_RUN__';
const TABS_STORE = '/src/sections/run/tabs/store.ts';
const TABS_HANDLE = '__LOADOUT_IMAGE_TABS__';

/* TA SAMA ODPOWIEDZ NA KAZDE PYTANIE, a nie jedna w kolejce.
 *
 * ZMIERZONE 2026-08-31, na jedenastu przypadkach tego pliku naraz. `openApp({replies})` trzyma
 * KOLEJKE na nazwe komendy i zdejmuje z niej po jednym (`e2e/harness.ts`, `replies[cmd].shift()`);
 * kiedy kolejka jest pusta, atrapa oddaje `[]` na kazde `list_*`. Kolejka dlugosci JEDEN opisuje
 * wiec dysk, ktory ma jednego agenta przy pierwszym pytaniu i zadnego przy drugim — a to nie jest
 * dysk, tylko licznik wywolan przebrany za fiksture.
 *
 * Zlamalo sie to w chwili, w ktorej boczne menu zaczelo liczyc te sama polke (`src/ui/shell/
 * what-you-have.ts`, kłódka i licznik przy pozycji): pierwsza odpowiedz szla do menu, a kontrolka
 * lidera w pasku dostawala juz pusta liste i malowala sie jako `disabled`. Kazdy z jedenastu
 * przypadkow padal potem na `locator.selectOption: element is not enabled` — czyli na fiksturze,
 * nie na zachowaniu, ktorego ten plik pilnuje.
 *
 * `say_to_orchestrator` zostaje kolejka i to jest cala roznica: tam KAZDA odpowiedz jest inna
 * i to ona jest tasma, ktora ten plik sadzi.
 */
function copies<T>(value: T, count = 24): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function withReplies(
  replies: readonly TauriReply[],
  workspaces: readonly (typeof WORKSPACE)[],
): OpenAppOptions {
  return {
    replies: {
      list_workspaces: copies(workspaces),
      list_agents: copies([AGENT]),
      say_to_orchestrator: replies,
    },
  };
}

async function openConversation(
  replies: readonly TauriReply[] = [],
  workspaces: readonly (typeof WORKSPACE)[] = [WORKSPACE],
): Promise<RunningApp> {
  const app = await openApp(withReplies(replies, workspaces));
  await app.page.locator(WORK).waitFor({ state: 'attached', timeout: APPEARS });
  await app.page.locator(FIELD).waitFor({ state: 'attached', timeout: APPEARS });
  const lead = app.page.getByLabel('Lead agent');
  await lead.waitFor({ state: 'attached', timeout: APPEARS });
  await lead.selectOption(AGENT.id);
  return app;
}

async function switchTo(app: RunningApp, workspace: typeof WORKSPACE): Promise<void> {
  await app.page.click('[data-workspace-open]');
  await app.page.click(`[data-workspace-pick="${workspace.id}"]`);
  await app.page
    .locator('[data-workspace-open]')
    .filter({ hasText: workspace.name })
    .waitFor({ state: 'visible', timeout: APPEARS });
}

/** Wkleja prawdziwy `File`; nazwa jest sentinelem, który później nie może znaleźć się w invoke. */
async function pastePng(app: RunningApp): Promise<void> {
  await app.page.locator(FIELD).evaluate(
    (element, image) => {
      const bytes = Uint8Array.from(atob(image.base64), (character) => character.charCodeAt(0));
      const transfer = new DataTransfer();
      transfer.items.add(new File([bytes], image.name, { type: 'image/png' }));
      element.dispatchEvent(
        new ClipboardEvent('paste', {
          bubbles: true,
          cancelable: true,
          clipboardData: transfer,
        }),
      );
    },
    { base64: PNG_BASE64, name: SECRET_NAME },
  );
  await app.page.waitForTimeout(SETTLE);
}

/** Jeden paste z dwoma formatami, przy zaznaczeniu w środku istniejącego szkicu. */
async function pastePngWithText(app: RunningApp, text: string): Promise<void> {
  await app.page.locator(FIELD).evaluate(
    (element, image) => {
      const field = element as HTMLInputElement;
      field.setSelectionRange(5, 12);
      const bytes = Uint8Array.from(atob(image.base64), (character) => character.charCodeAt(0));
      const transfer = new DataTransfer();
      transfer.items.add(new File([bytes], image.name, { type: 'image/png' }));
      transfer.setData('text/plain', image.text);
      field.dispatchEvent(
        new ClipboardEvent('paste', {
          bubbles: true,
          cancelable: true,
          clipboardData: transfer,
        }),
      );
    },
    { base64: PNG_BASE64, name: SECRET_NAME, text },
  );
  await app.page.waitForTimeout(SETTLE);
}

/** Prawdziwy systemowy paste w Chromium — nie ręczne przypisanie `input.value`. */
async function pasteText(app: RunningApp, text: string): Promise<void> {
  await app.page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await app.page.evaluate(async (copied) => navigator.clipboard.writeText(copied), text);
  await app.page.locator(FIELD).focus();
  await app.page.keyboard.press(process.platform === 'darwin' ? 'Meta+V' : 'Control+V');
  await app.page.waitForTimeout(SETTLE);
}

function sentToLead(app: RunningApp): Promise<readonly TauriCall[]> {
  return app.calls().then((calls) => calls.filter((call) => call.cmd === 'say_to_orchestrator'));
}

async function previewUrl(app: RunningApp): Promise<string> {
  const url = await app.page.locator(`${PREVIEW} img`).getAttribute('src');
  if (url === null) throw new Error('the visible image preview has no src');
  return url;
}

/** Nowy odczyt blob URL: po `revokeObjectURL` ma się nie udać, niezależnie od zniknięcia img. */
function blobIsReadable(app: RunningApp, url: string): Promise<boolean> {
  return app.page.evaluate(async (address) => {
    try {
      const response = await fetch(address);
      await response.arrayBuffer();
      return response.ok;
    } catch {
      return false;
    }
  }, url);
}

async function waitForVisibleSentence(app: RunningApp, sentence: string): Promise<void> {
  const row = app.page.locator('[data-line]').filter({ hasText: sentence });
  await row.waitFor({ state: 'visible', timeout: APPEARS });
  expect(await row.innerText()).toContain(sentence);
}

/** Zasiewa jeden żywy krok w tym samym magazynie, z którego prawdziwy Run wylicza adresata. */
async function oneStepIsRunning(app: RunningApp): Promise<void> {
  await app.page.addScriptTag({
    type: 'module',
    content:
      "import { runFor } from '" +
      RUN_STATE +
      "';\nglobalThis[" +
      JSON.stringify(RUN_HANDLE) +
      '] = runFor;\n',
  });
  const problem = await app.page.evaluate(
    ({ handle, folder }) => {
      const host = globalThis as unknown as Record<string, unknown>;
      const runFor = host[handle] as
        | ((folder: string) => {
            getState(): {
              nowRunning(
                workflow: string,
                steps: readonly {
                  readonly id: string;
                  readonly name: string;
                  readonly state: string;
                }[],
                folder: string,
              ): void;
            };
          })
        | undefined;
      if (runFor === undefined) return 'runFor did not reach the page';
      runFor(folder)
        .getState()
        .nowRunning('fixture', [{ id: 'worker', name: 'Worker', state: 'running' }], folder);
      return null;
    },
    { handle: RUN_HANDLE, folder: FOLDER },
  );
  if (problem !== null) throw new Error(problem);
  await app.page.getByText('Start the line with Worker to reach it.', { exact: false }).waitFor({
    state: 'visible',
    timeout: APPEARS,
  });
}

/** Nadaje domyślnej rozmowie jej kartę folderu; tożsamość feedu przed i po pozostaje ta sama. */
async function sameConversationGetsItsFolderCard(app: RunningApp): Promise<void> {
  await app.page.addScriptTag({
    type: 'module',
    content:
      "import { runTabs } from '" +
      TABS_STORE +
      "';\nglobalThis[" +
      JSON.stringify(TABS_HANDLE) +
      '] = runTabs;\n',
  });
  const problem = await app.page.evaluate(
    ({ handle, folder }) => {
      const host = globalThis as unknown as Record<string, unknown>;
      const tabs = host[handle] as
        | {
            getState(): {
              open(tab: {
                readonly id: string;
                readonly name: string;
                readonly path: string;
                readonly agents: number;
              }): void;
            };
          }
        | undefined;
      if (tabs === undefined) return 'runTabs did not reach the page';
      tabs.getState().open({ id: folder, name: 'Fixture run', path: folder, agents: 0 });
      return null;
    },
    { handle: TABS_HANDLE, folder: FOLDER },
  );
  if (problem !== null) throw new Error(problem);
  await app.page
    .locator(`[data-tab="${FOLDER}"][aria-current="true"]`)
    .waitFor({ state: 'visible', timeout: APPEARS });
}

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('pasted images on the real Lead composer', () => {
  it('shows a preview, removes it, and leaves ordinary pasted text as text', async () => {
    const app = await openConversation();
    try {
      await pastePng(app);
      expect(
        await app.page.locator(PREVIEW).count(),
        'a PNG File was pasted into Command line and no preview appeared on the real screen.',
      ).toBe(1);

      expect(
        await app.page.locator(REMOVE).count(),
        'the preview has no Remove control, so a mistaken paste cannot be undone before sending.',
      ).toBe(1);
      const removedUrl = await previewUrl(app);
      expect(await blobIsReadable(app, removedUrl)).toBe(true);
      await app.page.locator(REMOVE).click();
      expect(await app.page.locator(PREVIEW).count()).toBe(0);
      expect(
        await blobIsReadable(app, removedUrl),
        'Remove hid the preview but left its local blob alive in the webview.',
      ).toBe(false);

      await pasteText(app, 'plain clipboard text');
      expect(
        await app.page.inputValue(FIELD),
        'ordinary clipboard text stopped behaving like text after image paste support was added.',
      ).toBe('plain clipboard text');

      await pastePng(app);
      const unmountedUrl = await previewUrl(app);
      await app.page.click('[data-section-switch="agents"]');
      await app.page
        .locator('main[data-section="agents"]')
        .waitFor({ state: 'attached', timeout: APPEARS });
      expect(
        await blobIsReadable(app, unmountedUrl),
        'leaving the Run screen kept a pasted image blob alive after its owner unmounted.',
      ).toBe(false);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('keeps both halves of a clipboard item that contains an image and plain text', async () => {
    const app = await openConversation();
    try {
      await app.page.fill(FIELD, 'keep replace tail');
      await pastePngWithText(app, 'caption');
      expect(
        await app.page.inputValue(FIELD),
        'the image arrived, but the simultaneous text/plain flavor was silently dropped.',
      ).toBe('keep caption tail');
      expect(await app.page.locator(PREVIEW).count()).toBe(1);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('sends an image-only message as MIME plus base64, without its filename, then clears it', async () => {
    const app = await openConversation([{ value: null }]);
    try {
      await pastePng(app);
      const sentUrl = await previewUrl(app);
      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);

      const calls = await sentToLead(app);
      expect(
        calls.length,
        'Enter on an image-only draft never reached say_to_orchestrator. Requiring placeholder ' +
          'text would make screenshots impossible to send on their own.',
      ).toBe(1);

      const call = calls[0];
      expect(call).toEqual({
        cmd: 'say_to_orchestrator',
        args: {
          terminal: FOLDER,
          folder: FOLDER,
          lead: AGENT.id,
          text: '',
          images: [{ mime: 'image/png', base64: PNG_BASE64 }],
        },
      });
      expect(JSON.stringify(call)).not.toContain(SECRET_NAME);
      expect(await app.page.locator(PREVIEW).count()).toBe(0);
      expect(await app.page.inputValue(FIELD)).toBe('');
      expect(
        await blobIsReadable(app, sentUrl),
        'a successfully sent image disappeared from the draft but its blob URL stayed alive.',
      ).toBe(false);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('keeps image and text after refusal, shows the safe sentence, and retries the same draft', async () => {
    const app = await openConversation([{ error: PRIVATE_FAILURE }, { value: null }]);
    try {
      await app.page.fill(FIELD, RETRY_TEXT);
      await pastePng(app);
      const retriedUrl = await previewUrl(app);
      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);

      expect(
        await app.page.inputValue(FIELD),
        'the rejected message disappeared from Command line, so retry requires typing it again.',
      ).toBe(RETRY_TEXT);
      expect(
        await app.page.locator(PREVIEW).count(),
        'the rejected image disappeared, so the retry cannot send the draft that was refused.',
      ).toBe(1);
      await waitForVisibleSentence(app, SEND_FAILURE);
      expect(await app.page.locator('body').innerText()).not.toContain(PRIVATE_FAILURE);
      expect(
        await blobIsReadable(app, retriedUrl),
        'the refusal preserved the preview markup but revoked the image needed for retry.',
      ).toBe(true);

      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);

      const calls = await sentToLead(app);
      expect(calls.length).toBe(2);
      expect(calls[0]).toEqual(calls[1]);
      expect(await app.page.locator(PREVIEW).count()).toBe(0);
      expect(await app.page.inputValue(FIELD)).toBe('');
      expect(await blobIsReadable(app, retriedUrl)).toBe(false);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('latches a pending send and never clears edits made while its snapshot is in flight', async () => {
    const app = await openConversation([{ deferred: 'first-image-send' }, { value: null }]);
    try {
      await app.page.fill(FIELD, 'First draft');
      await pastePng(app);
      const pendingUrl = await previewUrl(app);

      await app.page.press(FIELD, 'Enter');
      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);
      expect(
        (await sentToLead(app)).length,
        'two Enter presses while one invoke is pending bought two Lead turns.',
      ).toBe(1);

      await app.page.fill(FIELD, 'Newer draft written while waiting');
      await app.settle('first-image-send', { value: null });
      await app.page.waitForTimeout(SETTLE);
      expect(await app.page.inputValue(FIELD)).toBe('Newer draft written while waiting');
      expect(await app.page.locator(PREVIEW).count()).toBe(1);
      expect(
        await blobIsReadable(app, pendingUrl),
        'the old reply revoked the image still owned by the newer visible draft.',
      ).toBe(true);

      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);
      const calls = await sentToLead(app);
      expect(calls.length).toBe(2);
      expect(calls[0]?.args['text']).toBe('First draft');
      expect(calls[1]?.args['text']).toBe('Newer draft written while waiting');
      expect(await app.page.inputValue(FIELD)).toBe('');
      expect(await app.page.locator(PREVIEW).count()).toBe(0);
      expect(await blobIsReadable(app, pendingUrl)).toBe(false);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('clears and revokes an unsent image draft when the conversation identity changes', async () => {
    const app = await openConversation([], [WORKSPACE, OTHER_WORKSPACE]);
    try {
      await app.page.fill(FIELD, 'Draft owned by the first workspace');
      await pastePng(app);
      const firstUrl = await previewUrl(app);

      await switchTo(app, OTHER_WORKSPACE);
      expect(
        await app.page.inputValue(FIELD),
        'the second workspace inherited text typed in the first workspace.',
      ).toBe('');
      expect(
        await app.page.locator(PREVIEW).count(),
        'the second workspace inherited an image pasted in the first workspace.',
      ).toBe(0);
      expect(
        await blobIsReadable(app, firstUrl),
        'switching conversations hid the first draft but kept its blob alive.',
      ).toBe(false);

      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);
      expect(
        (await sentToLead(app)).length,
        'Enter in the empty second workspace sent the hidden draft from the first workspace.',
      ).toBe(0);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('does not let a late completion from one workspace clear the next workspace draft', async () => {
    const app = await openConversation(
      [{ deferred: 'first-workspace-send' }, { value: null }],
      [WORKSPACE, OTHER_WORKSPACE],
    );
    try {
      await app.page.fill(FIELD, 'Message from the first workspace');
      await pastePng(app);
      const firstUrl = await previewUrl(app);
      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);
      expect((await sentToLead(app))[0]?.args['folder']).toBe(FOLDER);

      await switchTo(app, OTHER_WORKSPACE);
      expect(await app.page.locator(PREVIEW).count()).toBe(0);
      expect(await blobIsReadable(app, firstUrl)).toBe(false);
      await app.page.fill(FIELD, 'Message owned by the second workspace');

      await app.settle('first-workspace-send', { value: null });
      await app.page.waitForTimeout(SETTLE);
      expect(
        await app.page.inputValue(FIELD),
        'the old workspace completion cleared the visible draft owned by the new workspace.',
      ).toBe('Message owned by the second workspace');

      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);
      const calls = await sentToLead(app);
      expect(calls.length).toBe(2);
      expect(calls[1]?.args).toMatchObject({
        folder: OTHER_FOLDER,
        terminal: OTHER_FOLDER,
        text: 'Message owned by the second workspace',
        images: [],
      });
    } finally {
      await app.close();
    }
  }, 90_000);

  it('keeps the draft when the default folder conversation receives its folder card', async () => {
    const app = await openConversation([{ value: null }]);
    try {
      await app.page.fill(FIELD, 'The same conversation keeps this draft');
      await pastePng(app);
      const sameUrl = await previewUrl(app);

      await sameConversationGetsItsFolderCard(app);
      expect(
        await app.page.inputValue(FIELD),
        'null-to-folder tab normalization remounted the composer for the same conversation.',
      ).toBe('The same conversation keeps this draft');
      expect(await app.page.locator(PREVIEW).count()).toBe(1);
      expect(
        await blobIsReadable(app, sameUrl),
        'the same feed identity kept its text but revoked its visible image.',
      ).toBe(true);

      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);
      expect((await sentToLead(app))[0]?.args).toMatchObject({
        folder: FOLDER,
        terminal: FOLDER,
        text: 'The same conversation keeps this draft',
        images: [{ mime: 'image/png', base64: PNG_BASE64 }],
      });
    } finally {
      await app.close();
    }
  }, 90_000);

  it('clears the draft when closing the folder card ends that conversation', async () => {
    const app = await openConversation();
    try {
      await sameConversationGetsItsFolderCard(app);
      await app.page.fill(FIELD, 'This draft belongs to the card being closed');
      await pastePng(app);
      const closedUrl = await previewUrl(app);

      await app.page.getByLabel('Close Fixture run').click();
      await app.page
        .locator(`[data-tab="${FOLDER}"]`)
        .waitFor({ state: 'detached', timeout: APPEARS });
      expect(
        await app.page.inputValue(FIELD),
        'closing the conversation left its text in the fresh default composer.',
      ).toBe('');
      expect(
        await app.page.locator(PREVIEW).count(),
        'closing the conversation left its image in the fresh default composer.',
      ).toBe(0);
      expect(
        await blobIsReadable(app, closedUrl),
        'the closed conversation lost its card but kept the pasted image blob alive.',
      ).toBe(false);
      expect(
        (await app.calls()).filter((call) => call.cmd === 'close_terminal'),
        'the card disappeared without ending the conversation this lifecycle test describes.',
      ).toEqual([{ cmd: 'close_terminal', args: { terminal: FOLDER } }]);

      await app.page.press(FIELD, 'Enter');
      await app.page.waitForTimeout(SETTLE);
      expect(
        (await sentToLead(app)).length,
        'Enter in the fresh composer sent the hidden draft from the closed conversation.',
      ).toBe(0);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('refuses images beside slash commands before any command or conversation IPC', async () => {
    const app = await openConversation();
    try {
      await app.page.fill(FIELD, '/run Easy inspect this');
      await pastePng(app);
      await app.page.press(FIELD, 'Enter');
      await waitForVisibleSentence(app, COMMAND_REFUSAL);

      expect(await app.page.inputValue(FIELD)).toBe('/run Easy inspect this');
      expect(await app.page.locator(PREVIEW).count()).toBe(1);
      const forbidden = new Set([
        'run_workflow',
        'run_agent',
        'start_process',
        'say_to_orchestrator',
        'say_to_agent',
      ]);
      expect((await app.calls()).filter((call) => forbidden.has(call.cmd))).toEqual([]);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('refuses an image addressed to a live step and preserves the draft without IPC', async () => {
    const app = await openConversation();
    try {
      await oneStepIsRunning(app);
      await app.page.fill(FIELD, 'Worker inspect this screenshot');
      await pastePng(app);
      await app.page.press(FIELD, 'Enter');
      await waitForVisibleSentence(app, STEP_REFUSAL);

      expect(await app.page.inputValue(FIELD)).toBe('Worker inspect this screenshot');
      expect(await app.page.locator(PREVIEW).count()).toBe(1);
      const conversationCalls = (await app.calls()).filter(
        (call) => call.cmd === 'say_to_agent' || call.cmd === 'say_to_orchestrator',
      );
      expect(conversationCalls).toEqual([]);
    } finally {
      await app.close();
    }
  }, 90_000);
});
