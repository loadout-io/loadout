import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ImportSetup, type ImportPreview } from './setup';

describe('Agent-assisted setup import', () => {
  it('shows the service choice, privacy boundary, and review step on the real dialog', () => {
    const preview: ImportPreview = {
      snapshot: {
        root: '/project',
        items: [
          {
            id: 'harness',
            source: 'agent_skills',
            kind: 'workflow',
            path: '.agents/harness/config.json',
            name: 'harness',
            summary: 'Custom routine',
          },
        ],
      },
      draft: {
        sourceHashes: { '.agents/harness/config.json': 'abc' },
        agents: [],
        skills: [],
        connections: [],
        workflows: [],
        report: {
          mappings: [
            {
              itemId: 'harness',
              compatibility: 'needs_choice',
              message: 'Choose how to reproduce this routine.',
            },
          ],
        },
      },
      analysis: {
        vendor: 'claude-code',
        sourceHashes: { '.agents/harness/config.json': 'abc' },
        agents: [],
        workflows: [],
      },
    };
    const html = renderToStaticMarkup(
      <ImportSetup
        initialPreview={preview}
        onClose={() => undefined}
        onImported={() => undefined}
      />,
    );
    expect(html).toContain('Analyze remaining setup with');
    expect(html).toContain('Claude');
    expect(html).toContain('Codex');
    expect(html).toContain('redacted, read-only copy');
    expect(html).toContain('Review analyzed routine');
  });
});
