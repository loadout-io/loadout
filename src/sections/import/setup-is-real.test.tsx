import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import AgentsScreen from '../agents';
import { createAgentsStore } from '../../state/agents';
import { ImportSetup, type ImportPreview } from './setup';

describe('Import setup', () => {
  it('is reachable from the real Agents screen', () => {
    const store = createAgentsStore({
      list: async () => [],
      newId: async () => '019b0000-0000-7000-8000-000000000075',
      save: async () => undefined,
      remove: async () => undefined,
    });
    const html = renderToStaticMarkup(<AgentsScreen store={store} usage={{}} />);
    expect(html).toContain('Import setup');
  });

  it('shows compatibility, blockers, and connections on the product dialog', () => {
    const preview: ImportPreview = {
      snapshot: {
        root: '/project',
        items: [
          { id: 'agent', path: '.claude/agents/build.md', name: 'build', summary: 'Agent' },
          { id: 'hook', path: '.claude/settings.json', name: 'settings', summary: 'Hook' },
        ],
      },
      draft: {
        sourceHashes: { '.claude/agents/build.md': 'abc' },
        agents: [{ id: 'a', name: 'Build' }],
        skills: [],
        connections: [{ id: 'browser', name: 'Browser', enabled: false }],
        workflows: [],
        report: {
          mappings: [
            { itemId: 'agent', compatibility: 'exact', message: 'Ready.' },
            {
              itemId: 'hook',
              compatibility: 'needs_choice',
              message: 'Choose how to reproduce this hook.',
            },
          ],
        },
      },
    };
    const html = renderToStaticMarkup(
      <ImportSetup
        initialPreview={preview}
        io={{
          scanSetup: async () => preview,
          applySetup: async () => ({ id: 'receipt', written: [], enabledConnections: [] }),
        }}
        onClose={() => undefined}
        onImported={() => undefined}
      />,
    );
    expect(html).toContain('Exact');
    expect(html).toContain('Needs a choice');
    expect(html).toContain('Connections stay off unless you enable them');
    expect(html).toContain('disabled=""');
    expect(html).toContain('Choose how to reproduce this hook.');
    expect(html).toContain('Leave this behavior out of the imported setup');
  });
});
