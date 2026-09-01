/* „Gdzie to biegnie" jest JEDNĄ decyzją, więc jest jedną kontrolką.
 *
 * # Co było zmierzone
 *
 * Trzy kopie tej samej decyzji, w trzech panelach jednego katalogu:
 *   `panel.tsx`        „Where it works", trzy radia, brzmienia „Work in the project folder" …
 *   `check-panel.tsx`  „Where it runs",  trzy radia, brzmienia „In your project folder" …
 *   `serve-panel.tsx`  „Where it runs",  dwa radia,  brzmienia jak wyżej.
 *
 * Dwie różne nazwy nad tym samym pytaniem i dwa różne brzmienia tej samej odpowiedzi. Człowiek,
 * który przeczytał jeden panel, musi przeczytać drugi od nowa, a rozjazd trzeciej kopii zauważy
 * dopiero recenzja — o ile spojrzy na oba pliki naraz.
 *
 * # Czego to kryterium pilnuje
 *
 * Że trzy panele mówią o tej samej wartości TYMI SAMYMI SŁOWAMI: jedno pytanie i jedna
 * odpowiedź na `use`. Słabą wersją byłoby `expect(html).toContain('Where it works')` w każdym
 * z trzech — przechodzi także wtedy, gdy trzy pliki trzymają trzy kopie tego samego napisu,
 * czyli dla dokładnie tego stanu, przed którym to kryterium stoi. Dlatego porównywane są
 * napisy WYJĘTE Z MARKUPU trzech paneli, między sobą.
 *
 * # Czego to kryterium NIE zmienia
 *
 * Nazw grup radiowych (`step-folder`, `check-where`, `serve-where`). Zawężają się do nich
 * kryteria spoza tego pliku — także jedno w prawdziwej przeglądarce — a nazwa grupy jest tym
 * kluczem, którym przeglądarka wiąże przyciski w jeden wybór. Wspólna kontrolka bierze ją
 * propsem i to jest cała różnica, którą zostawia.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Agent } from '../../../state/agents';
import type { AgentStep, CheckStep, Folder, ServeStep } from '../../../state/workflows';
import { CheckPanel } from './check-panel';
import { PanelForStep } from './panel';
import { ServePanel } from './serve-panel';

function noop(): void {
  /* sterowane panele: statyczny render nic z tego nie woła */
}

function jarvis(): Agent {
  return {
    schema: 1,
    id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    name: 'Jarvis',
    summary: 'Implements',
    color: 'clay',
    instructions: 'Write the smallest change that makes the checks pass.',
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
}

function agentStep(folder: Folder): AgentStep {
  return {
    kind: 'agent',
    id: 's_build',
    name: 'Build',
    agent: jarvis().id,
    overrides: {},
    copies: 1,
    instructions: 'Fix the failing parser tests.',
    skills: 'all',
    folder,
    handover: 'notes',
    at: { x: 24, y: 24 },
  };
}

function checkStep(folder: Folder): CheckStep {
  return {
    kind: 'check',
    id: 's_check',
    name: 'Run the checks',
    command: './verify.sh',
    proof: String.raw`(\d+) passed`,
    folder,
    at: { x: 24, y: 168 },
  };
}

function serveStep(folder: Folder): ServeStep {
  return {
    kind: 'serve',
    id: 's_serve',
    name: 'Run frontend',
    command: 'npm run dev',
    folder,
    at: { x: 24, y: 312 },
  };
}

function agentPanel(folder: Folder): string {
  return renderToStaticMarkup(
    <PanelForStep
      step={agentStep(folder)}
      agents={[jarvis()]}
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

function checkPanel(folder: Folder): string {
  return renderToStaticMarkup(<CheckPanel step={checkStep(folder)} onEditStep={noop} />);
}

function servePanel(folder: Folder): string {
  return renderToStaticMarkup(<ServePanel step={serveStep(folder)} onEditStep={noop} />);
}

/** Pytanie postawione nad grupą wyboru. */
function question(html: string): string {
  const hit = /<legend\b[^>]*>([\s\S]*?)<\/legend>/.exec(html);
  return (hit?.[1] ?? '').replace(/<[^>]*>/g, '').trim();
}

/** Znaczniki wszystkich przycisków tej grupy. */
function buttons(html: string, group: string): string[] {
  return [...html.matchAll(/<input\b[^>]*>/g)]
    .map((hit) => hit[0])
    .filter((tag) => tag.includes('name="' + group + '"'));
}

function attribute(tag: string, name: string): string {
  const hit = new RegExp(name + '="([^"]*)"').exec(tag);
  return hit?.[1] ?? '';
}

/** Odpowiedzi tej grupy: wartość zapisywana w pliku → zdanie, które człowiek czyta. */
function answers(html: string, group: string): Record<string, string> {
  const found: Record<string, string> = {};
  for (const tag of buttons(html, group)) {
    found[attribute(tag, 'value')] = attribute(tag, 'aria-label');
  }
  return found;
}

const AGENT_GROUP = 'step-folder';
const CHECK_GROUP = 'check-where';
const SERVE_GROUP = 'serve-where';

describe('where a step works is one control, worded once', () => {
  it('asks the same question on all three kinds of tile', () => {
    const asked = question(agentPanel({ use: 'project' }));

    expect(asked, 'the agent panel does not ask where its step works at all').not.toBe('');
    expect(
      question(checkPanel({ use: 'project' })),
      'a check asks the same thing under a different name, so the person who learned one panel ' +
        'has to read the next one from scratch',
    ).toBe(asked);
    expect(
      question(servePanel({ use: 'project' })),
      'and the tile that starts something asks it under a third wording',
    ).toBe(asked);
  });

  it('gives the same value the same words, whatever kind of tile is asking', () => {
    const agent = answers(agentPanel({ use: 'project' }), AGENT_GROUP);
    const check = answers(checkPanel({ use: 'project' }), CHECK_GROUP);
    const serve = answers(servePanel({ use: 'project' }), SERVE_GROUP);

    expect(
      Object.keys(agent).sort(),
      'the agent panel no longer offers the three places the file can hold',
    ).toEqual(['fresh-copy', 'project', 'same-copy']);
    for (const [use, said] of Object.entries(agent)) {
      expect(said, 'the answer for ' + use + ' reaches nobody: it has no wording').not.toBe('');
    }

    expect(
      check,
      'a check spells the same three places differently from an agent step. One of the two ' +
        'wordings will drift, and the drift shows up as two screens that seem to offer ' +
        'different things while writing the same value',
    ).toEqual(agent);
    expect(
      Object.keys(serve).sort(),
      'a tile that starts something has two honest answers, not three: a copy of its own would ' +
        'serve code nobody in this run has touched',
    ).toEqual(['project', 'same-copy']);
    for (const [use, said] of Object.entries(serve)) {
      expect(said, 'and its wording for ' + use + ' has to be the one everywhere else').toBe(
        agent[use],
      );
    }
  });

  it('marks the place the file holds, on every kind of tile', () => {
    for (const use of ['project', 'fresh-copy', 'same-copy'] as const) {
      const picked = buttons(agentPanel({ use }), AGENT_GROUP).filter((tag) =>
        /\bchecked(?:="")?/.test(tag),
      );
      expect(picked.length, 'the agent panel marks none or many for ' + use).toBe(1);
      expect(attribute(picked[0] ?? '', 'value'), 'and it marked the wrong one').toBe(use);
    }
    const marked = buttons(checkPanel({ use: 'same-copy' }), CHECK_GROUP).filter((tag) =>
      /\bchecked(?:="")?/.test(tag),
    );
    expect(attribute(marked[0] ?? '', 'value'), 'a check does not show the place it holds').toBe(
      'same-copy',
    );
  });
});
