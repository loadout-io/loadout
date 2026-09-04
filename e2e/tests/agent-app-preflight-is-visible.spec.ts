/* Stopka bocznego menu mówi, co NAPRAWDĘ odpowiedziało — w prawdziwej przeglądarce.
 *
 * PO CO TO ISTNIEJE, i to jest niezmiennik 29 wzięty dosłownie. Do 2026-09 w tym miejscu stała
 * stała `READY = 'Claude · Codex ready'`: zdanie wpisane w kod, które nie pytało niczego.
 * Człowiek bez zainstalowanego Codeksa czytał w stopce obietnicę gotowości, a pierwszy krok
 * biegu mówił „nie" — czyli kontrolka bez skutku (niezmiennik 16) na tej jednej powierzchni,
 * która w ogóle mówi cokolwiek o otoczeniu aplikacji.
 *
 * Czysty render dowiódłby TREŚCI zdania, a moduł stanu — że mechanizm istnieje. Ani jedno, ani
 * drugie nie dotyka linii, na której to się psuje naprawdę: odczytu przy montażu okna, kliknięcia
 * Retry i zwinięcia menu. Ten plik zatrzymuje odpowiedź dokładnie na granicy Rusta (odroczone
 * wywołanie), czyta to, co widać w trwałej stopce, i składa menu PRAWDZIWYM kliknięciem.
 *
 * Pierwsze wywołanie jest odroczone z premedytacją: bez tego stan „Checking…" mógłby minąć,
 * zanim padnie pierwsza asercja, i punkt o nim nie mierzyłby niczego.
 */
import { afterAll, describe, expect, it } from 'vitest';

import { closeEverything, openApp } from '../harness';

/** Stopka: jedyny blok nawigacji przypięty do dołu (`margin-top:auto` z makiety). */
const FOOTER = 'nav[data-chrome] > div.mt-auto';

/** Kontrolka zwijania — ta sama, którą klika `the-side-nav-folds-on-a-real-click`. */
const FOLD = '[data-nav-fold]';

/** Ile czekamy, aż React przerysuje stopkę. Render i mikrozadanie, nie sieć. */
const APPEARS = 4_000;

const CHECKING_CLAUDE = 'Checking Claude Code…';
const CHECKING_CODEX = 'Checking Codex…';
const NO_CLAUDE = "Claude Code wasn't found.";
const NO_CODEX_ANSWER = "Loadout couldn't check Codex.";
const CLAUDE_VERSION = 'Claude Code · claude-code 7.7.7';
const CODEX_VERSION = 'Codex · codex-cli 8.8.8';
const SIGN_IN = 'Sign-in is checked when you first run an agent.';

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('the agent app preflight in the persistent nav', () => {
  it(
    'shows probe results in expanded nav and the same sentences in the collapsed tooltip ' +
      'after Retry and the real fold click',
    async () => {
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
          'the persistent footer still shows the unconditional "Claude · Codex ready" claim. ' +
            'Nothing asked the two local apps anything, so the sentence promises readiness the ' +
            'first step of a run then denies (invariant 16).',
        ).not.toContain('Claude · Codex ready');
        expect(
          visible,
          'the window mounted and the footer says nothing about Claude Code being looked at. ' +
            'The read has to start with the window, not with a visit to some screen.',
        ).toContain(CHECKING_CLAUDE);
        expect(visible).toContain(CHECKING_CODEX);
        expect(
          visible,
          'the footer never admits that being signed in is a different question, answered only ' +
            'when an agent really runs. Without that line a found version reads as "ready".',
        ).toContain(SIGN_IN);

        await app.settle('first-agent-app-check', {
          value: [
            { app: 'claude-code', state: 'not-found' },
            { app: 'codex', state: 'could-not-check' },
          ],
        });
        await footer.getByText(NO_CLAUDE, { exact: true }).waitFor({
          state: 'visible',
          timeout: APPEARS,
        });
        await footer.getByText(NO_CODEX_ANSWER, { exact: true }).waitFor({
          state: 'visible',
          timeout: APPEARS,
        });

        const retry = footer.getByRole('button', { name: 'Retry', exact: true });
        await retry.waitFor({ state: 'visible', timeout: APPEARS });
        await retry.click();

        await footer.getByText(CLAUDE_VERSION, { exact: true }).waitFor({
          state: 'visible',
          timeout: APPEARS,
        });
        await footer.getByText(CODEX_VERSION, { exact: true }).waitFor({
          state: 'visible',
          timeout: APPEARS,
        });
        const afterRetry = (await footer.innerText()).trim();
        expect(
          afterRetry,
          'one app answered and the other one still shows its old refusal, so the two reads are ' +
            'not one read: ' +
            afterRetry,
        ).not.toContain("wasn't found");
        expect(afterRetry).not.toContain("couldn't check");
        expect(
          await retry.count(),
          'both local apps answered with a version and Retry is still standing. A control that ' +
            'has nothing left to do is furniture.',
        ).toBe(0);

        const asked = (await app.calls()).filter((call) => call.cmd === 'check_agent_apps');
        expect(
          asked,
          'the window and Retry did not each cost exactly one read of BOTH apps. Two commands, ' +
            'or one per vendor, would be two answers to one question (invariant 13).',
        ).toEqual([
          { cmd: 'check_agent_apps', args: {} },
          { cmd: 'check_agent_apps', args: {} },
        ]);

        /* ZWINIĘCIE PRAWDZIWYM KLIKNIĘCIEM, nie wołaniem `collapseNav` z testu: sądzimy to, co
           człowiek dostaje po naciśnięciu kontrolki, którą ta nawigacja rysuje. */
        await app.page.click(FOLD);
        await footer.locator('[class*="rounded-full"]').first().waitFor({
          state: 'visible',
          timeout: APPEARS,
        });

        const folded = (await footer.innerText()).trim();
        expect(
          folded,
          'the narrowed footer still writes the sentences out. A list of app names cut to fit a ' +
            '64 px column promises readiness for whoever is no longer on it: ' +
            folded,
        ).toBe('');

        const dots = await footer
          .locator('[class*="rounded-full"]')
          .evaluateAll((nodes) => nodes.map((node) => node.getAttribute('class') ?? ''));
        expect(
          dots.length,
          'the folded footer leaves ' +
            String(dots.length) +
            ' dots, not one. Three states of two apps on one dot is one fact in one place ' +
            '(invariant 13); two dots side by side is a second carrier nobody can read.',
        ).toBe(1);
        expect(
          dots[0],
          'the readiness dot is not the muted one. The accent means "this is interactive" and ' +
            'the live colour means "this is happening now"; whether a local app answers is ' +
            'neither (DESIGN §3).',
        ).toContain('bg-muted');
        expect(
          /animate-/.test(dots[0] ?? ''),
          'the readiness dot pulses. Movement is kept for the one fact that something is ' +
            'happening now, and the two moving regions ARCHITECTURE §7 allows are both spent.',
        ).toBe(false);

        const said = await footer.locator('[class*="rounded-full"]').getAttribute('title');
        expect(
          said,
          'the folded dot carries no tooltip, so folding the nav threw both answers away. The ' +
            'rule for this column is that a thing disappears only when its absence does not lie.',
        ).not.toBeNull();
        expect(said ?? '').toContain(CLAUDE_VERSION);
        expect(said ?? '').toContain(CODEX_VERSION);
        expect(said ?? '').toContain(SIGN_IN);
      } finally {
        await app.close();
      }
    },
    180_000,
  );
});
