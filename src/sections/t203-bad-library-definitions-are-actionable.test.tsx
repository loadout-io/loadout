/* T-203 AC-3: wadliwy plik jest prawdziwym, nieinteraktywnym wierszem na obu ekranach.
 *
 * Test ładuje produkcyjne magazyny przez ich IO, a potem montuje prawdziwe ekrany. Problem nie
 * przychodzi propsem do komponentu. Markup dowodzi, że człowiek widzi bezpieczną nazwę oraz
 * ręczną drogę naprawy, ale nie dostaje destrukcyjnej kontrolki o nieuczciwym kontrakcie.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { Agent, AgentsIo } from '../state/agents';
import { createAgentsStore } from '../state/agents';
import type { Definition, DefinitionProblem } from '../state/library';
import { problemSays } from '../state/library';
import AgentsScreen from './agents';
import type { WorkflowEntry, WorkflowFile, WorkflowListIo } from './workflows/list/store';
import { createWorkflowListStore } from './workflows/list/store';
import WorkflowsScreen from './workflows';
import * as AgentDisk from './agents/io';
import * as WorkflowDisk from './workflows/io';

const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((_command: string): Promise<unknown> => Promise.resolve([])),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

function agent(): Agent {
  return {
    schema: 1,
    id: 'agent-healthy',
    name: 'Healthy agent',
    summary: 'Stays usable beside a bad file.',
    color: 'clay',
    instructions: 'Keep working.',
    runsWith: 'codex',
    model: 'gpt-5',
    thinking: 'deep',
    fileAccess: 'ask-first',
    giveUpAfterMinutes: 20,
    tools: 'everything',
    reachesTheWeb: false,
    skills: [],
    connections: [],
    writeResultsTo: '',
  };
}

function workflow(): WorkflowEntry {
  const document: WorkflowFile = {
    format: 1,
    id: 'workflow-healthy',
    name: 'Healthy workflow',
    steps: [],
    links: [],
  };
  return { path: 'healthy.json', workflow: document };
}

const AGENT_PROBLEM: DefinitionProblem = {
  kind: 'definitionProblem',
  shelf: 'agents',
  fileName: 'broken-agent.md',
  problem: 'malformed',
};

const WORKFLOW_PROBLEM: DefinitionProblem = {
  kind: 'definitionProblem',
  shelf: 'workflows',
  fileName: 'broken-workflow.json',
  problem: 'malformed',
};

function region(markup: string, fileName: string): string {
  const at = markup.indexOf(`data-definition-problem="${fileName}"`);
  if (at < 0) return '';
  const start = markup.lastIndexOf('<li', at);
  const end = markup.indexOf('</li>', at);
  return start < 0 || end < 0 ? '' : markup.slice(start, end + 5);
}

function agentsIo(listed: Definition<Agent>[]): AgentsIo {
  return {
    list: () => Promise.resolve(structuredClone(listed)),
    newId: () => Promise.resolve('agent-new'),
    save: () => Promise.resolve(),
    remove: () => Promise.resolve(),
  };
}

function workflowsIo(listed: Definition<WorkflowEntry>[], writes: string[] = []): WorkflowListIo {
  return {
    list: () => Promise.resolve(structuredClone(listed)),
    newId: () => Promise.resolve('workflow-new'),
    write: (path) => {
      writes.push(path);
      return Promise.resolve();
    },
    remove: () => Promise.resolve(),
  };
}

describe('bad library definitions stay actionable beside healthy ones', () => {
  it('shows each problem on its real screen without healthy controls attached to it', async () => {
    const healthyAgent: Definition<Agent> = { kind: 'healthy', value: agent() };
    const healthyWorkflow: Definition<WorkflowEntry> = { kind: 'healthy', value: workflow() };
    const agentStore = createAgentsStore(agentsIo([healthyAgent, AGENT_PROBLEM]));
    const workflowStore = createWorkflowListStore(workflowsIo([healthyWorkflow, WORKFLOW_PROBLEM]));
    await agentStore.getState().load();
    await workflowStore.getState().load();

    const agentsMarkup = renderToStaticMarkup(
      <AgentsScreen store={agentStore} usage={{ 'agent-healthy': 1 }} />,
    );
    const workflowsMarkup = renderToStaticMarkup(<WorkflowsScreen store={workflowStore} />);

    expect(problemSays(AGENT_PROBLEM)).toBe(
      '“broken-agent.md” is not an agent Loadout can read. Open your Agents folder to repair or remove it, then reload.',
    );
    expect(problemSays(WORKFLOW_PROBLEM)).toBe(
      '“broken-workflow.json” is not a workflow Loadout can read. Open your Workflows folder to repair or remove it, then reload.',
    );

    expect(agentsMarkup, 'the healthy agent keeps its real Edit/open control').toContain(
      'data-agent="agent-healthy"',
    );
    expect(workflowsMarkup, 'the healthy workflow keeps its real Open control').toContain(
      'data-tile="true"',
    );

    for (const [markup, problem] of [
      [agentsMarkup, AGENT_PROBLEM],
      [workflowsMarkup, WORKFLOW_PROBLEM],
    ] as const) {
      const row = region(markup, problem.fileName);
      expect(
        row,
        `${problem.fileName} came through the production store but never reached the real screen`,
      ).not.toBe('');
      expect(row).toContain(problem.fileName);
      expect(row).toContain(problemSays(problem));
      expect(row, 'a problem must not expose the removed destructive Delete flow').not.toContain(
        'data-delete-problem',
      );
      expect(
        row,
        'the problem row is informational, not a definition-shaped control',
      ).not.toContain('<button');
      expect(row, 'a malformed definition cannot be opened as if it had parsed').not.toContain(
        'data-agent=',
      );
      expect(row).not.toContain('data-tile');
      expect(row).not.toContain('Duplicate');
      expect(row).not.toContain('Run');
      expect(row).not.toContain('Edit');
    }
  });

  it('keeps legacy healthy-only IPC arrays usable through both production wrappers', async () => {
    invoked.mockImplementation((command: string) => {
      if (command === 'list_agents') return Promise.resolve([agent()]);
      if (command === 'list_workflows') return Promise.resolve([workflow()]);
      return Promise.resolve([]);
    });

    await expect(AgentDisk.list()).resolves.toEqual([agent()]);
    await expect(WorkflowDisk.list()).resolves.toEqual([workflow()]);
  });

  it('counts an unreadable workflow file name as occupied when creating', async () => {
    const writes: string[] = [];
    const store = createWorkflowListStore(
      workflowsIo(
        [
          {
            kind: 'definitionProblem',
            shelf: 'workflows',
            fileName: 'New-Workflow.JSON',
            problem: 'malformed',
          },
        ],
        writes,
      ),
    );
    await store.getState().load();
    await store.getState().create('New workflow');

    expect(
      writes,
      'Create must not overwrite a file merely because its parser cannot produce a workflow id',
    ).toEqual(['new-workflow-2.json']);
  });
});
