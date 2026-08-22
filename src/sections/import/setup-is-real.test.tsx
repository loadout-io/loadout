import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it } from 'vitest';
import AgentsScreen from '../agents';
import { createAgentsStore } from '../../state/agents';
import { useWorkspaces } from '../../state/workspaces';
import { ImportSetup, type ImportPreview } from './setup';

describe('Import setup', () => {
  afterEach(() => {
    useWorkspaces.setState({ all: [], activeId: null, said: null });
  });

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
          { id: 'unknown', path: '.claude/future.json', name: 'future', summary: 'Unknown' },
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
            {
              itemId: 'unknown',
              compatibility: 'unsupported',
              message: 'This setting is not supported.',
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
    expect(html).toContain('Import without this behavior');
    expect(html).toContain('Leave this item out of the import');
    expect(html).toContain('Leave out all unresolved items');
    expect(html).toContain('<table');
    expect(html).toContain('Type');
    expect(html).toContain('Source');
    expect(html).toContain('Needs attention');
  });

  it('starts with the folder of the workspace open in the side menu', () => {
    const folder = '/Users/somebody/Projects/Current';
    useWorkspaces.setState({
      all: [{ id: folder, name: 'Current', folder }],
      activeId: folder,
      said: null,
    });

    const html = renderToStaticMarkup(
      <ImportSetup onClose={() => undefined} onImported={() => undefined} />,
    );

    expect(html).toContain(`value="${folder}"`);
  });

  it('does not offer an empty import as ready', () => {
    const preview: ImportPreview = {
      snapshot: { root: '/project', items: [] },
      draft: {
        sourceHashes: {},
        agents: [],
        skills: [],
        connections: [],
        workflows: [],
        report: { mappings: [] },
      },
    };

    const html = renderToStaticMarkup(
      <ImportSetup
        initialPreview={preview}
        onClose={() => undefined}
        onImported={() => undefined}
      />,
    );

    expect(html).toContain('No setup files were found in this project.');
    expect(html).not.toContain('Ready to import.');
  });
});
