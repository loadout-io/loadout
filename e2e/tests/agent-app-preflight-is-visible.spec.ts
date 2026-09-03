/* Preflight aplikacji agentów: prawdziwy frontend, prawdziwa stopka i prawdziwy klik Retry.
 *
 * Niezmiennik 29 zabrania sądzić samego store'u albo funkcji składającej zdanie. Ten plik
 * zatrzymuje odpowiedź dokładnie na granicy Rusta i czyta to, co człowiek naprawdę widzi
 * w trwałej stopce SideNav. Pierwsze odroczone wywołanie oddziela stan `Checking` od szybkości
 * lokalnych CLI — bez tego poprawna sonda mogłaby skończyć się przed pierwszą asercją.
 */
import { afterAll, describe, expect, it } from 'vitest';

import { closeEverything, openApp } from '../harness';

const FOOTER = 'nav[data-chrome] > div.mt-auto';
const APPEARS = 4_000;

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('the agent app preflight in the persistent nav', () => {
  it('shows pending checks and retries both local apps through the real footer', async () => {
    const app = await openApp({
      replies: {
        check_agent_apps: [
          { deferred: 'first-agent-app-check' },
          {
            value: [
              { app: 'claude-code', state: 'found', version: 'claude-code 7.7.7' },
              { app: 'codex', state: 'found', version: 'codex-cli 8.8.8' },
            ],
          },
        ],
      },
    });
    try {
      const footer = app.page.locator(FOOTER);
      await footer.waitFor({ state: 'visible', timeout: APPEARS });
      const visible = (await footer.innerText()).trim();

      expect(
        visible,
        'the persistent footer still shows the unconditional "Claude · Codex ready" claim',
      ).not.toContain('Claude · Codex ready');
      expect(visible).toContain('Checking Claude Code…');
      expect(visible).toContain('Checking Codex…');
      expect(visible).toContain('Sign-in is checked when you first run an agent.');

      await app.settle('first-agent-app-check', {
        value: [
          { app: 'claude-code', state: 'not-found' },
          { app: 'codex', state: 'could-not-check' },
        ],
      });
      await footer.getByText("Claude Code wasn't found.", { exact: true }).waitFor({
        state: 'visible',
        timeout: APPEARS,
      });
      await footer.getByText("Loadout couldn't check Codex.", { exact: true }).waitFor({
        state: 'visible',
        timeout: APPEARS,
      });

      const retry = footer.getByRole('button', { name: 'Retry', exact: true });
      await retry.waitFor({ state: 'visible', timeout: APPEARS });
      await retry.click();

      await footer.getByText('Claude Code · claude-code 7.7.7', { exact: true }).waitFor({
        state: 'visible',
        timeout: APPEARS,
      });
      await footer.getByText('Codex · codex-cli 8.8.8', { exact: true }).waitFor({
        state: 'visible',
        timeout: APPEARS,
      });
      const afterRetry = (await footer.innerText()).trim();
      expect(afterRetry).not.toContain("wasn't found");
      expect(afterRetry).not.toContain("couldn't check");
      expect(await retry.count()).toBe(0);

      const checks = (await app.calls()).filter((call) => call.cmd === 'check_agent_apps');
      expect(checks).toEqual([
        { cmd: 'check_agent_apps', args: {} },
        { cmd: 'check_agent_apps', args: {} },
      ]);
    } finally {
      await app.close();
    }
  }, 180_000);
});
