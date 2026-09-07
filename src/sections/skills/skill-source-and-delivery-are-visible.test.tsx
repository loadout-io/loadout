/* WF-13: wybór źródła z prawdziwego formularza zapisuje źródło, nie samą nazwę. */
import { afterAll, expect, it } from 'vitest';
import { closeEverything, openApp } from '../../../e2e/harness';
import type { TauriReply } from '../../../e2e/harness';

const PROJECT = '/work/skill-choice';
const SKILL = 'bundle-reader';
const PROJECT_SOURCE = `${PROJECT}/.claude/skills/${SKILL}`;
const LIBRARY_SOURCE = `/library/skills/${SKILL}`;
const AGENT = {
  schema: 1,
  id: '01990000-0000-7000-8000-000000000013',
  name: 'Bundle reader',
  summary: 'Reads the selected bundle',
  instructions: 'Read the marker in the selected skill.',
  color: 'slate',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'look-only',
  reachesTheWeb: false,
  giveUpAfterMinutes: 20,
  writeResultsTo: '',
  tools: 'everything',
  skills: [SKILL],
  connections: [],
};
const replies = (value: unknown): readonly TauriReply[] =>
  Array.from({ length: 12 }, () => ({ value }));

afterAll(closeEverything, 30_000);

it('shows the two complete sources and persists the exact explicit source', async () => {
  const app = await openApp({
    replies: {
      list_workspaces: replies([{ id: PROJECT, folder: PROJECT, name: 'Skills' }]),
      list_agents: replies([
        { kind: 'healthy', value: AGENT, path: 'reader.md', revision: 'initial' },
      ]),
      save_agent: replies('saved'),
      /* 2026-09-07 — kopie przyjeżdżają wierszem listy umiejętności, nie drugą komendą. */
      list_skills: replies([
        {
          name: SKILL,
          fromTheInternet: false,
          summary: '',
          requiresChoice: true,
          sources: [
            { path: PROJECT_SOURCE, digest: 'a'.repeat(64), bytes: 170, files: 3, available: true },
            { path: LIBRARY_SOURCE, digest: 'b'.repeat(64), bytes: 190, files: 4, available: true },
          ],
        },
      ]),
    },
  });
  try {
    await app.page.locator('[data-section-switch="agents"]').click();
    await app.page.locator(`[data-agent="${AGENT.id}"]`).click();
    await app.page.getByRole('button', { name: 'More settings', exact: true }).click();
    const source = app.page.getByRole('combobox', { name: `Source for ${SKILL}`, exact: true });
    expect(await source.count(), 'the real agent form cannot distinguish same-name sources').toBe(
      1,
    );
    expect(
      await app.page.getByText('Choose which copy of this skill to use.', { exact: true }).count(),
    ).toBe(1);
    expect(await source.textContent()).toContain(PROJECT_SOURCE);
    expect(await source.textContent()).toContain(LIBRARY_SOURCE);
    await source.selectOption(LIBRARY_SOURCE);
    await app.page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect
      .poll(async () => (await app.calls()).filter((call) => call.cmd === 'save_agent'))
      .toHaveLength(1);
    const saved = (await app.calls()).find((call) => call.cmd === 'save_agent')?.args['agent'];
    expect(saved).toMatchObject({ skills: [SKILL], skillSources: { [SKILL]: LIBRARY_SOURCE } });
    expect((await app.calls()).some((call) => call.cmd === 'run_workflow')).toBe(false);
  } finally {
    await app.close();
  }
}, 90_000);
