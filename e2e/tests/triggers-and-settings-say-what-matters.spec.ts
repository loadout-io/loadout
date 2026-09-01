/* Trzy zdania o kompozycji dwóch cichych ekranów, sprawdzone w prawdziwej przeglądarce.
 *
 * PO CO PRZEGLĄDARKA, A NIE STATYCZNY RYSUNEK. Wszystkie trzy pytania są pytaniami o to, co
 * człowiek WIDZI po wejściu na sekcję, a nie o wartość zwróconą przez funkcję (niezmiennik 29):
 *
 *   1. czy nazwa tego, co się odpali, mieści się na ekranie w całości — a przycięcie jest
 *      faktem o UKŁADZIE i istnieje wyłącznie tam, gdzie coś ten układ policzyło. Statyczny
 *      rysunek nie ma szerokości, więc o przycięciu nie wie nic;
 *   2. czy wybrany prowadzący mówi, kim jest — a lista agentów dojeżdża na ten ekran efektem,
 *      którego `renderToStaticMarkup` nie uruchamia (repo nie ma jsdom);
 *   3. czy zdanie „Add one in Agents" ma pod sobą drogę do Agents — czyli czy prawdziwe
 *      kliknięcie naprawdę przenosi sekcję.
 *
 * GRANICA ODPOWIADA KSZTAŁTEM (nagłówek `../harness.ts`). Podstawione są wyłącznie te odpowiedzi,
 * których SĄDZIMY treść: biblioteka triggerów, lista workflow, workspace'y, zapisani agenci
 * i wybór z pliku ustawień.
 */
import { afterAll, describe, expect, it } from 'vitest';

import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

/** Nazwy DŁUGIE, bo pytanie brzmi, co się dzieje, kiedy treści jest tyle, ile bywa naprawdę. */
const WORKFLOW_NAME = 'Reproduce, fix and ship a regression';
const WORKSPACE_NAME = 'Client website redesign';
const FOLDER = '/Users/somebody/Projects/client-website-redesign';
const SLUG = 'linear-assigned-to-me';

const WORKSPACE = { id: FOLDER, name: WORKSPACE_NAME, folder: FOLDER };

const SCOUT = { id: 'agent-scout', name: 'Scout', summary: 'Reads the code first', skills: [] };
const BUILDER = { id: 'agent-builder', name: 'Builder', summary: 'Writes the change', skills: [] };

const TRIGGER = {
  slug: SLUG,
  source: 'linear',
  condition: 'assigned-to-me',
  workflow: 'regression.json',
  workspace: FOLDER,
  enabled: true,
  pollEveryMinutes: 5,
  hasApiKey: true,
};

const WORKFLOWS = [
  {
    path: 'regression.json',
    workflow: { format: 1, id: 'wf-regression', name: WORKFLOW_NAME, steps: [], links: [] },
  },
];

/** Ile czekamy na to, co ma się pojawić po kliknięciu. Wszystko wraca w tej samej karcie. */
const APPEARS = 8_000;

function copies<T>(value: T, count = 20): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

function scene(agents: readonly unknown[]): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workspaces: copies([WORKSPACE]),
    list_agents: copies(agents),
    list_workflows: copies(WORKFLOWS),
    list_skills: copies([]),
    list_triggers: copies([TRIGGER]),
    read_settings: copies({ defaultLead: BUILDER.id, defaultBudgetUsd: 25 }),
  };
}

afterAll(async () => {
  await closeEverything();
});

describe('a saved trigger says what will run without losing half of the name', () => {
  it('shows the whole line naming the workflow, the workspace and how often it checks', async () => {
    const app = await openApp({ replies: scene([BUILDER]) });
    try {
      /* Szerokość okna z makiety. Przycięcie jest funkcją miejsca, więc miejsce musi być znane. */
      await app.page.setViewportSize({ width: 1512, height: 950 });
      await app.page.locator('[data-section-switch="triggers"]').click();
      const line = app.page.locator(`[data-trigger-row="${SLUG}"] [data-trigger-workflow]`);
      await line.waitFor({ state: 'visible', timeout: APPEARS });
      await app.page
        .locator(`[data-trigger-row="${SLUG}"]`)
        .getByText(WORKFLOW_NAME)
        .waitFor({ state: 'visible', timeout: APPEARS });

      const cut = await line.evaluate((element) => element.scrollWidth - element.clientWidth);
      expect(
        cut,
        'the line naming what this trigger runs is cut off by ' +
          String(cut) +
          ' pixels. A person reads "' +
          WORKFLOW_NAME +
          '" as something ending in three dots, because the row hands a third of its width to ' +
          'the connector and a third to the condition — two facts which read the same on every ' +
          'single row. Text a person wrote gives way only to other text a person wrote: the ' +
          'name takes the room it needs and the rest of the line wraps, shortens or goes away',
      ).toBeLessThanOrEqual(1);

      const said = await line.innerText();
      expect(
        said,
        'the line stopped naming the workflow altogether, so nothing on this row answers the ' +
          'one question this screen exists for: what will run',
      ).toContain(WORKFLOW_NAME);
      expect(said).toContain(WORKSPACE_NAME);
    } finally {
      await app.close();
    }
  }, 120_000);
});

describe('the default lead on Settings', () => {
  it('says who that agent is, not only what the file is called', async () => {
    const app = await openApp({ replies: scene([SCOUT, BUILDER]) });
    try {
      await app.page.setViewportSize({ width: 1512, height: 950 });
      await app.page.locator('[data-section-switch="settings"]').click();
      /* Czekamy na LISTĘ, nie na napis, którego sądzimy istnienie: dopiero wypełniona kontrolka
         wyboru dowodzi, że biblioteka agentów dojechała z dysku. Bez tego brak zdania o roli
         znaczyłby „jeszcze nie wróciło", a nie „nie ma go na ekranie". */
      await app.page
        .locator('select#default-lead option')
        .nth(1)
        .waitFor({ state: 'attached', timeout: APPEARS });
      const role = app.page.locator('[data-lead-summary]');
      expect(
        await role.count(),
        'with the library loaded and a lead chosen, nothing on this screen says what that ' +
          'agent does. The sentence is in the very file this screen already reads',
      ).toBe(1);
      expect(
        await role.innerText(),
        'the screen names the agent leading every run and says nothing about who that is. ' +
          'A month later "' +
          BUILDER.name +
          '" is a filename, and the sentence answering it already sits in the very file this ' +
          'screen reads to fill that list',
      ).toBe(BUILDER.summary);
    } finally {
      await app.close();
    }
  }, 120_000);

  it('offers a way into Agents when there is nobody to lead yet', async () => {
    const app = await openApp({ replies: scene([]) });
    try {
      await app.page.setViewportSize({ width: 1512, height: 950 });
      await app.page.locator('[data-section-switch="settings"]').click();
      /* Kotwicą jest zdanie o pustej bibliotece — stoi na ekranie w obu wersjach, więc brak
         kontrolki obok niego jest odpowiedzią, a nie brakiem odpowiedzi. */
      await app.page
        .locator('[data-settings-screen] [data-empty]')
        .waitFor({ state: 'visible', timeout: APPEARS });
      const into = app.page.locator('[data-open-agents]');
      expect(
        await into.count(),
        'the screen says to add an agent in Agents and gives a person nothing to press',
      ).toBe(1);
      await into.click();
      await app.page
        .locator('main[data-section="agents"]')
        .waitFor({ state: 'attached', timeout: APPEARS });
      expect(
        await app.page.locator('main[data-section]').getAttribute('data-section'),
        'the sentence tells a person to add an agent in Agents and then leaves them to find ' +
          'their own way there. Words which name a place a person has to reach are a control, ' +
          'or they are an instruction the screen refuses to carry out',
      ).toBe('agents');
    } finally {
      await app.close();
    }
  }, 120_000);
});
