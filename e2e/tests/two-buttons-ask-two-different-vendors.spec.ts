/* Prawdziwe kliknięcie w „Create with Codex" i „Create with Claude" prosi TEGO vendora,
 * którego nazwa obiecuje.
 *
 * # Dlaczego to musi być prawdziwa przeglądarka
 *
 * Bo pole opisu jest kontrolką sterowaną: jego treść żyje w stanie Reacta, a `renderToStatic
 * Markup` nie odpala ani jednego `setState`. Kryterium wypisane tam pytałoby, czy uchwyt
 * istnieje — a pytanie brzmi, czy WPISANY tekst dociera do właściwego vendora.
 *
 * # Czego pilnuje najmocniej
 *
 * Argumentu `runsWith` w żądaniu. Przycisk, który prosi drugiego vendora, wygląda na ekranie
 * dokładnie tak samo jak działający: różnica wychodzi w rachunku u dostawcy i w konfiguracji
 * zapisanego agenta, czyli dopiero wtedy, gdy jest już za późno.
 */
import { afterAll, expect, it } from 'vitest';

import { closeEverything, openApp } from '../harness';
import type { TauriReply } from '../harness';

const SWITCH = '[data-section-switch="agents"]';
const DESCRIBED = 'Checks that a recording survives Stop then Later on the running app';

const AGENT = {
  schema: 1,
  id: '01990000-0000-7000-8000-000000000001',
  name: 'QA',
  summary: 'Checks the work',
  color: 'slate',
  instructions: 'Verify the behaviour that was asked for.',
  runsWith: 'claude-code',
  model: '',
  thinking: 'balanced',
  fileAccess: 'look-only',
  giveUpAfterMinutes: 20,
  tools: 'everything',
  reachesTheWeb: true,
  skills: [],
  connections: [],
  serviceAccess: [],
  agentMessages: false,
  writeResultsTo: '',
  vendorOptions: {},
};

const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 12 }, () => ({ value }));

afterAll(closeEverything, 30_000);

it.each([
  { field: 'create-with-codex', vendor: 'codex' },
  { field: 'create-with-claude', vendor: 'claude-code' },
])(
  '$field asks $vendor and opens what came back',
  async ({ field, vendor }) => {
    const app = await openApp({
      replies: {
        list_agents: replies([AGENT]),
        generate_agent: replies({
          operation: 'op-1',
          agent: { ...AGENT, id: '01990000-0000-7000-8000-000000000002', name: 'Recording QA' },
          assumptions: ['You did not say which project.'],
          because: ['It only reads, because checking does not change the work.'],
          missing: ['the connection "screen-control", which is not available here'],
          refused: [],
        }),
      },
    });
    try {
      await app.page.locator(SWITCH).click();
      const described = app.page.locator('#agent-description');
      await expect.poll(async () => described.count()).toBe(1);
      await described.fill(DESCRIBED);
      await app.page.locator(`[data-field="${field}"]`).click();

      const asked = await app.page
        .waitForFunction(() => true)
        .then(async () => (await app.calls()).filter((call) => call.cmd === 'generate_agent'));
      expect(
        asked,
        'the button did not reach the backend at all, so the description never left the window',
      ).toHaveLength(1);
      expect(
        asked[0]?.args['runsWith'],
        'the button asked a different app than the one written on it',
      ).toBe(vendor);
      expect(
        asked[0]?.args['described'],
        'what the person typed did not travel with the request',
      ).toBe(DESCRIBED);

      // Szkic wraca do TEGO SAMEGO edytora, w którym powstają role ręczne.
      await expect
        .poll(async () => app.page.locator('body').innerText(), { timeout: 5_000 })
        .toContain('Recording QA');
      await expect
        .poll(async () => app.page.locator('body').innerText(), {
          message: 'what the draft cannot have here was never shown before Save',
        })
        .toContain('screen-control');
    } finally {
      await app.close();
    }
  },
  60_000,
);
