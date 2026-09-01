import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ImportSetup, type ImportPreview } from './setup';

const preview: ImportPreview = {
  snapshot: {
    root: '/project',
    items: [
      {
        id: 'builder',
        source: 'claude',
        kind: 'agent',
        path: '.claude/agents/builder.md',
        name: 'Builder',
        summary: 'Builds the project.',
      },
      {
        id: 'project-guide',
        source: 'agent_skills',
        kind: 'skill',
        path: '.agents/skills/project-guide/SKILL.md',
        name: 'Project guide',
        summary: 'Explains the project.',
      },
    ],
  },
  draft: {
    sourceHashes: {
      '.claude/agents/builder.md': 'source-agent',
      '.agents/skills/project-guide/SKILL.md': 'source-skill',
    },
    items: [
      {
        id: 'builder',
        kind: 'agent',
        sources: [
          {
            provider: 'claude',
            path: '.claude/agents/builder.md',
            hash: 'source-agent',
            role: 'definition',
          },
        ],
        target: 'agents/builder.md',
        dependencies: ['skill:project-guide'],
        status: 'missing_dependencies',
        statusMessage: 'Blocked because skill project-guide will not be imported.',
        generatedHash: 'generated-agent',
      },
      {
        id: 'project-guide',
        kind: 'skill',
        sources: [
          {
            provider: 'agent_skills',
            path: '.agents/skills/project-guide/SKILL.md',
            hash: 'source-skill',
            role: 'definition',
          },
        ],
        target: 'skills/project-guide/SKILL.md',
        dependencies: [],
        status: 'needs_choice',
        statusMessage: 'Choose whether to import this skill without its coordinating behavior.',
        generatedHash: 'generated-skill',
      },
    ],
    agents: [{ id: 'agent-id', name: 'Builder' }],
    skills: [{ name: 'project-guide' }],
    connections: [],
    workflows: [],
    report: {
      mappings: [
        { itemId: 'builder', compatibility: 'exact', message: 'The format can be reproduced.' },
        {
          itemId: 'project-guide',
          compatibility: 'needs_choice',
          message: 'The format needs a decision.',
        },
      ],
    },
  },
};

function markup(value: ImportPreview = preview): string {
  return renderToStaticMarkup(
    <ImportSetup initialPreview={value} onClose={() => undefined} onImported={() => undefined} />,
  );
}

describe('Import item rows', () => {
  it('shows the full typed plan and the reason a dependency blocks it', () => {
    const html = markup();

    expect(html).toContain('Builder');
    expect(html).toContain('Agent');
    expect(html).toContain('Claude');
    expect(html).toContain('agents/builder.md');
    expect(html).toContain('skill:project-guide');
    expect(html).toContain('Blocked because skill project-guide will not be imported.');
  });

  it('offers two independently named decisions and never calls either one Skip', () => {
    const html = markup();

    expect(html).toContain('aria-label="Import this item"');
    expect(html).toContain('aria-label="Import without this behavior"');
    expect(html).not.toContain('Skip');
  });

  it('keeps the three filters and states exact targets and counts before the action', () => {
    const importable: ImportPreview = {
      ...preview,
      draft: {
        ...preview.draft,
        items: preview.draft.items.map((item) =>
          item.id === 'builder'
            ? { ...item, dependencies: [], status: 'ready', statusMessage: 'Ready to import.' }
            : item,
        ),
      },
    };
    const html = markup(importable);
    const importButton = html.lastIndexOf('>Import</button>');
    const counters = html.slice(html.indexOf('grid grid-cols-4'), html.indexOf('>Show</span>'));
    const proposed = html.slice(html.indexOf('>Proposed files</h3>'), importButton);

    expect(html).toContain('All');
    expect(html).toContain('Ready');
    expect(html).toContain('Needs attention');
    expect(counters).toMatch(/>1<\/b><small[^>]*>Agents/);
    expect(counters).toMatch(/>0<\/b><small[^>]*>Skills/);
    expect(proposed).toContain('1 agent');
    expect(proposed).not.toContain('1 skill');
    expect(proposed).toContain('agents/builder.md');
    expect(proposed).not.toContain('skills/project-guide/SKILL.md');
  });
});
