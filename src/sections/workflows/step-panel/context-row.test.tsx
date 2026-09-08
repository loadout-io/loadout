import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Agent } from '../../../state/agents';
import type { WorkflowContextView } from '../../../state/context';
import type { AgentStep } from '../../../state/workflows';
import { PanelForStep } from './panel';

const AGENT: Agent = {
  schema: 1,
  id: 'frontend-agent',
  name: 'Frontend agent',
  summary: 'Builds the interface',
  color: 'clay',
  instructions: 'Implement the requested interface.',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'work-freely',
  giveUpAfterMinutes: 20,
  writeResultsTo: 'handoffs/build.md',
  tools: 'everything',
  reachesTheWeb: false,
  skills: [],
  connections: [],
};

const STEP: AgentStep = {
  kind: 'agent',
  id: 'frontend',
  name: 'Frontend',
  agent: AGENT.id,
  overrides: {},
  copies: 1,
  instructions: 'Build checkout.',
  skills: 'all',
  folder: { use: 'project' },
  handover: 'notes',
  at: { x: 24, y: 24 },
  context: {
    schema: 1,
    inheritWorkflow: true,
    exclude: [],
    sets: [{ id: 'S', revision: 'r1', topics: ['checkout'] }],
  },
};

function noop(): void {
  // Sterowany panel niczego nie zmienia podczas statycznego renderu.
}

function panel(
  context: WorkflowContextView,
  refusal: string | null = null,
  step: AgentStep = STEP,
): string {
  return renderToStaticMarkup(
    <PanelForStep
      step={step}
      agents={[AGENT]}
      skills={[]}
      context={context}
      contextRefusal={refusal}
      onChooseAgent={noop}
      onCreateAgent={noop}
      onEdit={noop}
      onEditStep={noop}
      onEditCheckpoint={noop}
      onEditServe={noop}
      onReset={noop}
      onChooseSkills={noop}
      wayBack={null}
      onEditWayBack={noop}
    />,
  );
}

const NARROWED: WorkflowContextView = {
  catalog: [
    {
      id: 'S',
      title: 'Store rules',
      description: 'Checkout and returns',
      revision: 'r1',
      topics: [
        { id: 'checkout', title: 'Checkout' },
        { id: 'returns', title: 'Returns' },
      ],
      said: null,
    },
  ],
  workflow: [],
  steps: [
    {
      stepId: 'frontend',
      sets: [
        {
          id: 'S',
          title: 'Store rules',
          revision: 'r1',
          selectedTopics: ['checkout'],
          topics: [
            { id: 'checkout', title: 'Checkout' },
            { id: 'returns', title: 'Returns' },
          ],
          source: 'step',
          update: null,
          said: null,
        },
      ],
      omitted: [],
      inheritsWorkflow: true,
      protectedScope: false,
      said: null,
    },
  ],
  warnings: [],
};

describe('Context in the actual step panel', () => {
  it('says that a narrowed shared set was added to this step', () => {
    const html = panel(NARROWED);

    expect(html.match(/data-row="context"/g)).toHaveLength(1);
    expect(html).toContain('Added to this step');
    expect(html).toContain('1 topic');
    expect(html).not.toContain('From workflow');
  });

  it('keeps a draft visible when its pinned version is temporarily unavailable', () => {
    const said =
      'Context set S version r1 is not available now. This draft can still be saved; open Context and choose or build a ready version before starting.';
    const missing = structuredClone(NARROWED);
    const selected = missing.steps[0]?.sets[0];
    if (selected === undefined) throw new Error('the fixture lost its selected context');
    selected.said = said;

    expect(panel(missing)).toContain(said);
  });

  it('puts a resolver refusal beside Context instead of hiding the panel', () => {
    const refusal =
      'Context set S is selected more than once in the same list. Keep one selection.';

    expect(panel(NARROWED, refusal)).toContain(refusal);
  });

  it('shows the refusal even when the hand-edited selection has the wrong shape', () => {
    const malformed = structuredClone(STEP);
    (malformed as unknown as { context: unknown }).context = {
      schema: 1,
      sets: 'not a list',
    };
    const refusal =
      "This step's context selection cannot be read. Open Context and choose the sets again.";

    const html = panel(NARROWED, refusal, malformed);
    expect(html).toContain('context selection cannot be read');
    expect(html).toContain('Open Context and choose the sets again.');
  });

  it('names why a protected step did not inherit the workflow set', () => {
    const protectedView = structuredClone(NARROWED);
    const step = protectedView.steps[0];
    if (step === undefined) throw new Error('the fixture lost its step');
    step.sets = [];
    step.omitted = [
      {
        id: 'S',
        title: 'Store rules',
        said: 'Not used in this step — protected steps require an explicit choice.',
      },
    ];
    step.inheritsWorkflow = false;
    step.protectedScope = true;
    step.said =
      'Workflow context is off because this step uses a protected context. Turn it on here to include shared sets.';

    const html = panel(protectedView);

    expect(html).toContain('protected steps require an explicit choice');
    expect(html).toContain('Turn it on here to include shared sets');
  });

  it('does not turn a removed chosen topic into all topics during an update', () => {
    const changed = structuredClone(NARROWED);
    const catalog = changed.catalog[0];
    const selected = changed.steps[0]?.sets[0];
    if (catalog === undefined || selected === undefined) {
      throw new Error('the fixture lost its context version');
    }
    catalog.revision = 'r2';
    catalog.topics = [
      { id: 'payment', title: 'Payment' },
      { id: 'returns', title: 'Returns policy' },
    ];
    selected.update = 'Update available';

    const html = panel(changed);

    expect(html).toContain('Removed: Checkout.');
    expect(html).toContain('Changed: Returns → Returns policy.');
    expect(html).toContain('Choose topics for the new version before updating.');
    expect(html).toContain('Payment');
    expect(html).toContain('Returns policy');
    expect(html).not.toMatch(/<button[^>]*>Update<\/button>/);
  });
});
