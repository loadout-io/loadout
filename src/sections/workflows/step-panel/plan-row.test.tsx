import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Agent } from '../../../state/agents';
import type { AgentStep, StepPlan, WorkflowPlanView } from '../../../state/workflows';
import { PanelForStep } from './panel';

const AGENT: Agent = {
  schema: 1,
  id: 'worker',
  name: 'Worker',
  summary: 'Builds the requested change',
  color: 'clay',
  instructions: 'Do the requested work.',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'work-freely',
  giveUpAfterMinutes: 20,
  writeResultsTo: 'handoffs/result.md',
  tools: 'everything',
  reachesTheWeb: false,
  skills: [],
  connections: [],
};

function step(plan?: StepPlan): AgentStep {
  return {
    kind: 'agent',
    id: 'implementation',
    name: 'Implementation',
    agent: AGENT.id,
    overrides: {},
    copies: 1,
    instructions: 'Build the feature.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y: 24 },
    plan,
  };
}

const VIEW: WorkflowPlanView = {
  steps: [
    {
      stepId: 'implementation',
      mode: 'use',
      source: {
        stepId: 'planner',
        name: 'Planner',
        said: 'Takes the plan from Planner.',
      },
      earlier: [{ stepId: 'planner', name: 'Planner', said: 'Take the same plan as Planner.' }],
      said: null,
    },
  ],
  warnings: [],
};

function noop(): void {
  // Sterowany panel nie zmienia danych podczas statycznego renderu.
}

function panel(
  plan: StepPlan | undefined,
  view: WorkflowPlanView = VIEW,
  refusal: string | null = null,
): string {
  return renderToStaticMarkup(
    <PanelForStep
      step={step(plan)}
      agents={[AGENT]}
      skills={[]}
      plan={view}
      planRefusal={refusal}
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

describe('Plan in the actual step panel', () => {
  it('keeps all four modes behind More settings and names a changed mode in its summary', () => {
    const html = panel({ mode: 'create' });

    expect(html.match(/data-row="plan"/g)).toHaveLength(1);
    expect(html).toContain('Plan: Create');
    expect(html).toContain('Plan · Create');
    for (const label of ['Off', 'Create', 'Update', 'Use']) expect(html).toContain(label);
  });

  it('shows update scope only for Update and focus only for Use', () => {
    const update = panel({ mode: 'update', canUpdate: ['Design'] });
    const use = panel({ mode: 'use', focusOn: ['Implementation'] });

    expect(update).toContain('Can update');
    expect(update).not.toContain('Focus on (optional)');
    expect(use).toContain('Focus on (optional)');
    expect(use).not.toContain('Can update');
    expect(use).toContain('The complete shared core and all human requirements still apply.');
  });

  it('names the real source step without inventing a future version', () => {
    const html = panel({ mode: 'use' });

    expect(html).toContain('Takes the plan from Planner.');
    expect(html).toContain('Same plan as');
    expect(html).not.toMatch(/version\s+\d/i);
  });

  it('puts the resolver refusal beside the Plan control', () => {
    const refused = structuredClone(VIEW);
    const resolved = refused.steps[0];
    if (resolved === undefined) throw new Error('the fixture lost its Plan view');
    resolved.said =
      'Implementation can receive different plan versions from Planner and Designer. Choose Same plan as after drawing a dependency to that step.';

    expect(panel({ mode: 'use' }, refused)).toContain(resolved.said);
  });

  it('does not present a future mode as Off while naming its refusal', () => {
    const future = { mode: 'invented-by-a-newer-build' } as unknown as StepPlan;
    const refusal =
      'Implementation uses a Plan setting this Loadout does not know. Update Loadout or choose another setting.';

    const html = panel(future, { steps: [], warnings: [refusal] }, refusal);

    expect(html).toContain('Plan: Needs attention');
    expect(html).toContain('Plan · Needs attention');
    expect(html).toContain(refusal);
  });
});
