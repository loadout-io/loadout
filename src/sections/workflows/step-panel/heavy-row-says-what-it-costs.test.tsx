/* Krok mówi człowiekowi, kiedy bierze jedyne miejsce dla pracy obciążającej maszynę.
 *
 * Zdanie jest sądzone w markupie prawdziwego panelu (niezmiennik 29), a brak pola przechodzi
 * tą samą drogą co każdy workflow zapisany przed Z-45 i ma wyglądać jak zwykła tura.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Agent } from '../../../state/agents';
import type { AgentStep, Weight } from '../../../state/workflows';
import { PanelForStep } from './panel';

const SENTENCE = 'Takes the heavy seat — builds, full test suites, browsers';

function agent(): Agent {
  return {
    schema: 1,
    id: '01990000-0000-7000-8000-000000000045',
    name: 'Builder',
    summary: 'Builds the project',
    color: 'clay',
    instructions: 'Do the work.',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'work-freely',
    giveUpAfterMinutes: 20,
    writeResultsTo: '',
    tools: 'everything',
    reachesTheWeb: false,
    skills: [],
    connections: [],
  };
}

function step(weight?: Weight): AgentStep {
  return {
    kind: 'agent',
    id: 's_build',
    name: 'Build',
    agent: agent().id,
    overrides: {},
    copies: 1,
    ...(weight === undefined ? {} : { weight }),
    instructions: 'Build everything.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y: 24 },
  };
}

function noop(): void {
  /* Panel sterowany: statyczny render nie woła handlerów. */
}

function markup(weight?: Weight): string {
  return renderToStaticMarkup(
    <PanelForStep
      step={step(weight)}
      agents={[agent()]}
      skills={[]}
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

function heavyRow(html: string): string {
  return /<label\b[^>]*data-row="heavy"[\s\S]*?<\/label>/.exec(html)?.[0] ?? '';
}

describe('the heavy row says what the choice costs', () => {
  it('shows the one sentence on the real panel', () => {
    expect(
      heavyRow(markup()),
      'the sentence is not mounted behind the panel fold, so the person cannot tell what the switch does',
    ).toContain(SENTENCE);
  });

  it('reads a missing field as ordinary and a heavy field as selected', () => {
    expect(
      heavyRow(markup()),
      'a workflow from before Z-45 looks heavy after being opened',
    ).not.toContain('checked=""');
    expect(heavyRow(markup('heavy')), 'the file says heavy but its switch does not').toContain(
      'checked=""',
    );
  });
});
