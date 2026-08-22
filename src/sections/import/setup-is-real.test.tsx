import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it } from 'vitest';
import AgentsScreen from '../agents';
import { createAgentsStore } from '../../state/agents';
import { useWorkspaces } from '../../state/workspaces';
import { ImportSetup, type ImportPreview } from './setup';

/** Atrybuty przycisku o tym napisie, albo `null`, kiedy takiego przycisku nie ma. */
function attributesOf(html: string, label: string): string | null {
  for (const hit of html.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)) {
    if ((hit[2] ?? '').replace(/<[^>]*>/g, '').trim() === label) return hit[1] ?? '';
  }
  return null;
}

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

  it('is ready to import even when items cannot be brought in, and says how many stay out', () => {
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
        connections: [],
        workflows: [],
        report: {
          mappings: [
            { itemId: 'agent', compatibility: 'exact', message: 'Ready.' },
            { itemId: 'hook', compatibility: 'needs_choice', message: 'Choose how.' },
            { itemId: 'unknown', compatibility: 'unsupported', message: 'Not supported.' },
          ],
        },
      },
    };

    const html = renderToStaticMarkup(
      <ImportSetup
        initialPreview={preview}
        onClose={() => undefined}
        onImported={() => undefined}
      />,
    );

    expect(
      html,
      'the only answer this screen offers for an item that needs a choice IS Skip, so demanding ' +
        'the click was demanding the one possible answer, once per item. With 68 of them the ' +
        'import simply never happened',
    ).toContain('Ready to import. 2 item(s) will be left out.');
    expect(
      html,
      'and nothing is hidden by that: both items keep their row, their reason and their ticked Skip',
    ).toContain('Not supported.');
    expect(
      attributesOf(html, 'Import'),
      'the button has to be live, otherwise the sentence above is a promise the screen does not keep',
    ).not.toContain('disabled');
  });

  it('turns every connection on with one click, and still leaves each one visible', () => {
    const preview: ImportPreview = {
      snapshot: {
        root: '/project',
        items: [{ id: 'agent', path: '.claude/agents/build.md', name: 'build', summary: 'Agent' }],
      },
      draft: {
        sourceHashes: { '.claude/agents/build.md': 'abc' },
        agents: [{ id: 'a', name: 'Build' }],
        skills: [],
        connections: [
          { id: 'playwright', name: 'playwright', enabled: false, origin: 'project' as const },
          {
            id: 'linear-server',
            name: 'linear-server',
            enabled: false,
            origin: 'yours-here' as const,
          },
          { id: 'murmur', name: 'murmur', enabled: false, origin: 'yours-everywhere' as const },
        ],
        workflows: [],
        report: { mappings: [{ itemId: 'agent', compatibility: 'exact', message: 'Ready.' }] },
      },
    };

    const html = renderToStaticMarkup(
      <ImportSetup
        initialPreview={preview}
        onClose={() => undefined}
        onImported={() => undefined}
      />,
    );

    expect(
      attributesOf(html, 'Turn them all on'),
      'a project with seven servers is seven ticks today, and every one of them is the same ' +
        'decision. The button does not change the rule — a person still switches them on',
    ).not.toBeNull();
    expect(
      html,
      'and each connection keeps its own row: one click is a shortcut, never a way to hide what ' +
        'is being turned on',
    ).toContain('playwright');

    expect(
      html,
      'a server from the project and one from your own settings look identical without this — ' +
        'and the difference is who else has it, which is exactly what a person weighs before ' +
        'switching a tool server on',
    ).toContain('just you, in this project');
    expect(html, 'the third scope reads differently again').toContain('just you, everywhere');
    expect(html, 'and the ordinary case still says where it came from').toContain('in the project');
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
