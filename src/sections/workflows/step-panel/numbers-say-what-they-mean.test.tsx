/* Dwie liczby w panelu kroku mówią, co znaczą — zamiast po cichu robić coś innego.
 *
 * # Pierwsza: „Give up after"
 *
 * Pole było gołym polem liczbowym BEZ JEDNOSTKI. Jednostkę znała wyłącznie funkcja `minutes()`,
 * używana w szarym wierszu „Agent uses: …", czyli widoczna tylko wtedy, gdy krok już się od
 * agenta różni. Gorzej: `minutesFrom('')` daje 0, a 0 znaczy „bez limitu" — pole pokazuje wtedy
 * „0", nie „no limit". Przypadkowe skasowanie zdejmowało agentowi limit czasu i ani jedno
 * zdanie na ekranie tego nie mówiło. Krok bez limitu jest różnicą między biegiem, który staje,
 * a biegiem, który pali pieniądze do rana.
 *
 * # Druga: „How many at once"
 *
 * `copiesFrom` przycinało do ośmiu po cichu: wpisanie „9" natychmiast zamieniało się w „8",
 * a ekran nie mówił, dlaczego. Człowiek widzi liczbę, której nie wpisał, i nie ma jak
 * rozstrzygnąć, czy to sufit, czy pomyłka jego własnych palców.
 *
 * # Dlaczego oba przypadki są mierzone W DWÓCH STANACH
 *
 * Zdanie wpisane na stałe pod polem przechodzi każde jednostanowe sprawdzenie (niezmiennik 20).
 * Zdanie o braku limitu ma stać przy zerze i NIE stać przy dwudziestu; zdanie o suficie — przy
 * ośmiu i nie przy dwóch.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Agent } from '../../../state/agents';
import type { AgentStep } from '../../../state/workflows';
import { PanelForStep } from './panel';

function jarvis(giveUpAfterMinutes: number): Agent {
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
    giveUpAfterMinutes,
    writeResultsTo: 'handoffs/build.md',
    tools: 'everything',
    reachesTheWeb: false,
    skills: [],
    connections: [],
  };
}

function step(copies: number): AgentStep {
  return {
    kind: 'agent',
    id: 's_build',
    name: 'Build',
    agent: jarvis(20).id,
    overrides: {},
    copies,
    instructions: 'Fix the failing parser tests.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y: 24 },
  };
}

function noop(): void {
  /* panel sterowany: statyczny render nic z tego nie woła */
}

function markup(giveUpAfterMinutes: number, copies: number): string {
  return renderToStaticMarkup(
    <PanelForStep
      step={step(copies)}
      agents={[jarvis(giveUpAfterMinutes)]}
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

/** Jeden wiersz panelu, po jego znaczniku — bez tagów, gotowy do czytania.
 *
 * Wiersz, nie cały panel: zdanie o braku limitu, które stoi gdziekolwiek indziej na ekranie,
 * nie jest odpowiedzią na liczbę, przy której ma stać. */
function row(html: string, name: string): string {
  const at = html.indexOf('data-row="' + name + '"');
  if (at === -1) return '';
  const from = html.lastIndexOf('<', at);
  /* Wiersz kończy się tam, gdzie zaczyna się następny — albo na końcu panelu. */
  const next = html.indexOf('data-row="', at + 1);
  const to = next === -1 ? html.length : html.lastIndexOf('<', next);
  return html
    .slice(from, to)
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Wartość, którą pole liczbowe pokazuje. */
function shows(html: string, id: string): string | null {
  const hit = new RegExp('<input[^>]*id="' + id + '"[^>]*value="([^"]*)"').exec(html);
  return hit?.[1] ?? null;
}

describe('the two numbers on a step panel say what they mean', () => {
  it('names the unit of the time limit, so a bare number is not left to guess at', () => {
    const said = row(markup(20, 1), 'give-up-after');

    expect(said, 'the panel has no row for the time limit at all').not.toBe('');
    expect(
      said,
      'the field holds a bare number and nowhere beside it says what that number counts. ' +
        'Twenty of what — minutes, tries, pounds? The only place this product ever spelled it ' +
        'out was the grey line under a row that had already been changed',
    ).toContain('minutes');
  });

  it('says out loud that an empty limit is no limit, and does not say it otherwise', () => {
    const none = row(markup(0, 1), 'give-up-after');

    expect(shows(markup(0, 1), 'step-give-up-after'), 'the field is not showing the value').toBe(
      '0',
    );
    expect(
      none,
      'the field reads "0" and nothing says that zero takes the time limit off. Clearing this ' +
        'field by accident hands the step an open-ended run, and the screen says nothing at all',
    ).toContain('no limit');
    expect(
      row(markup(20, 1), 'give-up-after'),
      'the same sentence stands at twenty minutes too, so it is a sentence about nothing. Two ' +
        'states, because one state passes for a line that was simply written under the field',
    ).not.toContain('no limit');
  });

  it('says eight is the most, at the moment a bigger number lands as eight', () => {
    expect(
      row(markup(20, 8), 'how-many-at-once'),
      'typing nine puts eight in the field and the screen keeps quiet about it. The person is ' +
        'left with a number they did not type and no way to tell a ceiling from a slip',
    ).toContain('eight');
    expect(
      row(markup(20, 2), 'how-many-at-once'),
      'the ceiling is announced at every count, so it is noise rather than an answer to what ' +
        'just happened',
    ).not.toContain('eight');
  });
});
