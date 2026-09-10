import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';
import { AgentForm } from './agent-form';
import type { Agent } from '../../state/agents';
it('does not pretend a hardcoded model list is current', () => {
  const value: Agent = {
    schema: 1,
    id: 'a',
    name: 'Planner',
    summary: '',
    instructions: 'Plan',
    color: 'slate',
    runsWith: 'codex',
    model: 'gpt-6',
    thinking: 'deep',
    fileAccess: 'look-only',
    giveUpAfterMinutes: 0,
    tools: 'everything',
    reachesTheWeb: false,
    skills: [],
    connections: [],
    writeResultsTo: '',
  };
  const html = renderToStaticMarkup(
    <AgentForm
      value={value}
      expanded={false}
      brainOpen
      onChange={() => {}}
      onToggleMore={() => {}}
      onSave={() => {}}
    />,
  );
  expect(html).toContain('Refresh models');
  expect(html).not.toContain('<option value="gpt-5.6-sol"');
});
