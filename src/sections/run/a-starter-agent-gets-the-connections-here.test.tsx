/* Agent wzięty z pierwszego ekranu dostaje połączenia tego projektu (2026-09-13).
 *
 * PO CO TO ISTNIEJE. Scout, Builder i Needle z pierwszego otwarcia to trzecie miejsce, które
 * wpisywało pustkę (`SHARED` w `./starters.ts`) — obok `blankAgent` i poza zasięgiem kryterium
 * sekcji Agents. Człowiek, który zaczyna od gotowego agenta, ma dostać to samo, co człowiek,
 * który naciska `＋ Create`: połączenia włączone w bibliotece tego projektu.
 *
 * CO SĄDZIMY: napis w polu Connections tego agenta, otwartego w Agents po naciśnięciu Scout
 * (niezmiennik 29) — czyli dokładnie to, co trafiło na dysk.
 *
 * DLACZEGO `list_agents` NIE DOSTAJE `replies(...)`. Pierwszy ekran stoi wyłącznie przy
 * pustym projekcie, więc jedna powtarzana odpowiedź musiałaby być albo pustą listą (i Agents
 * nie pokazałby Scouta), albo Scoutem wpisanym w fiksturę z `figma` — a taki byłby zielony na
 * dzisiejszym kodzie, bo sądziłby fiksturę, nie przycisk. Każde czytanie dostaje więc własną
 * odroczoną odpowiedź (harness zdejmuje jedną na wywołanie), a test odpowiada na nie tym, co
 * W TEJ CHWILI leży na dysku: pustką przed naciśnięciem i agentem wziętym z wywołania
 * `save_agent` na taśmie po nim. Liczba czytań po drodze nie jest tu wpisana z palca — nowy
 * czytelnik biblioteki w sekcji Run nie przestawia przez to odpowiedzi na cudzy odczyt.
 */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../e2e/harness';
import type { RunningApp, TauriReply } from '../../../e2e/harness';

const PROJECT = '/work/starter-connections';
const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 12 }, () => ({ value }));

/** Z zapasem: pierwszy ekran i sekcja Agents czytają bibliotekę kilka razy, nie czterdzieści. */
const AGENT_READS = 40;

/** Nazwa odroczonej odpowiedzi na n-te czytanie agentów tej karty, licząc od jedynki. */
const agentRead = (nth: number): string => `list-agents-${String(nth)}`;

afterAll(closeEverything, 30_000);

/**
 * Odpowiada na każde czytanie agentów, które okno wysłało od ostatniego razu, tym, co teraz
 * leży na dysku. Oddaje, na ile czytań odpowiedziano w sumie.
 */
async function answerAgentReads(
  app: RunningApp,
  answered: number,
  onDisk: readonly unknown[],
): Promise<number> {
  const asked = (await app.calls()).filter((one) => one.cmd === 'list_agents').length;
  for (let nth = answered + 1; nth <= Math.min(asked, AGENT_READS); nth += 1) {
    await app.settle(agentRead(nth), { value: onDisk });
  }
  return Math.max(answered, Math.min(asked, AGENT_READS));
}

it('a ready-made agent taken on the first screen carries the connections this project has on', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Starter connections' }]),
      list_connections: replies(['figma']),
      list_agents: Array.from({ length: AGENT_READS }, (_, at) => ({
        deferred: agentRead(at + 1),
      })),
    },
  });
  try {
    /* Projekt musi już stać, zanim padnie naciśnięcie — inaczej zapis i odczyt połączeń
       jechałyby bez folderu, a to jest scena, której człowiek na tym ekranie nie ogląda. */
    await app.page
      .locator('[data-first-step="workspace"][data-step-state="done"]')
      .waitFor({ state: 'attached', timeout: 10_000 });
    let answered = await answerAgentReads(app, 0, []);

    await app.page.locator('[data-starter="Scout"]').click();
    await expect
      .poll(async () => (await app.calls()).filter((one) => one.cmd === 'save_agent'), {
        timeout: 10_000,
      })
      .toHaveLength(1);
    const saved = (await app.calls()).find((one) => one.cmd === 'save_agent')?.args['agent'] as
      { readonly id: string; readonly name: string } | undefined;
    expect(saved?.name, 'pressing Scout wrote something other than Scout').toBe('Scout');
    const id = saved?.id ?? '';
    const library = [{ kind: 'healthy', value: saved, path: 'scout.md', revision: 'r1' }];

    await app.page.locator('[data-section-switch="agents"]').click();
    const row = app.page.locator(`[data-agent="${id}"]`);
    await expect
      .poll(
        async () => {
          answered = await answerAgentReads(app, answered, library);
          return row.count();
        },
        { timeout: 10_000 },
      )
      .toBe(1);
    await row.click();
    await app.page.getByRole('button', { name: 'More settings', exact: true }).click();

    const field = app.page.locator('#agent-connections');
    expect(await field.count(), 'the Scout form has no Connections field at all').toBe(1);
    expect(
      await field.inputValue(),
      'Scout was taken in a project that has figma turned on and went to disk with an empty ' +
        'Connections field, so the first agent this person ever runs works without it',
    ).toBe('figma');
  } finally {
    await app.close();
  }
}, 90_000);
