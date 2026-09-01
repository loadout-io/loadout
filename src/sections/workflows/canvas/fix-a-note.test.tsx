/* Auto-fix: uwaga, która niesie naprawę, dostaje przycisk — a przycisk naprawdę ją wykonuje.
 *
 * DLACZEGO TO ISTNIEJE. 2026-08-22, zgłoszenie właściciela: trzy odmowy pod rząd na jednym biegu,
 * każda po naciśnięciu Start, każda na innym kroku. „Powinniśmy to walidować na etapie budowania
 * workflow i mieć opcję auto-fixa" — uwagi liczy dziś `workflow::roster`, a to jest druga połowa:
 * zdanie na ekranie i jedno kliknięcie, które je gasi.
 *
 * NIEZMIENNIK 29 W PRAKTYCE. Słabą wersją tego kryterium jest sprawdzenie funkcji magazynu:
 * `applyFix` może działać bez zarzutu, a przycisku może nie być na ekranie ALBO może nie mieć
 * handlera — i to jest dokładnie ta klasa wady, dla której to repo powstało. Dlatego niżej
 * mierzone są obie połowy osobno: markup renderowany przez `ThingsToFix` (czy człowiek to widzi)
 * i skutek `applyFix` na magazynie (czy kliknięcie cokolwiek zmienia).
 *
 * DRUGĄ SŁABĄ WERSJĄ jest przycisk z napisem „Fix". Przechodzi ją implementacja, która zmienia
 * uprawnienia agenta bez powiedzenia, na co. Dlatego sądzony jest NAPIS.
 */
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { Agent } from '../../../state/agents';
import type { Fix, Note, WorkflowFile } from '../../../state/workflows';
import { createWorkflowStore } from '../../../state/workflows';
import { ThingsToFix } from './problems';

function noop(): void {
  /* sterowany pasek: w statycznym renderze nic tego nie woła */
}

function jarvis(): Agent {
  return {
    schema: 1,
    id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    name: 'design-qa',
    summary: 'Checks the work.',
    color: 'clay',
    instructions: 'Check it.',
    runsWith: 'claude-code',
    model: 'opus',
    thinking: 'balanced',
    fileAccess: 'ask-first',
    giveUpAfterMinutes: 20,
    writeResultsTo: '',
    tools: { only: ['Read', 'Bash', 'mcp__playwright'] },
    reachesTheWeb: false,
    skills: [],
    connections: ['playwright'],
  };
}

function file(): WorkflowFile {
  return {
    format: 1,
    id: 'wf',
    name: 'Ship a feature',
    steps: [
      {
        kind: 'agent',
        id: 's_check',
        name: 'Figma check',
        agent: jarvis().id,
        overrides: { fileAccess: 'look-only' },
        copies: 1,
        instructions: 'Check the work.',
        skills: 'all',
        folder: { use: 'fresh-copy' },
        handover: 'notes',
        at: { x: 24, y: 24 },
      },
    ],
    links: [],
  };
}

/** Atrybuty i napis przycisku o tym `data-`, albo `null`. */
function fixButton(html: string): { attributes: string; label: string } | null {
  for (const hit of html.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)) {
    const attributes = hit[1] ?? '';
    if (attributes.includes('data-fix'))
      return { attributes, label: (hit[2] ?? '').replace(/<[^>]*>/g, '').trim() };
  }
  return null;
}

function bar(notes: Note[], onApplyFix?: (fix: Fix) => void): string {
  return renderToStaticMarkup(
    createElement(ThingsToFix, {
      notes,
      onFocusNote: noop,
      ...(onApplyFix === undefined ? {} : { onApplyFix }),
    }),
  );
}

const DIAL_NOTE: Note = {
  level: 'problem',
  stepId: 's_check',
  message: 'design-qa is set to look only and asks for Bash.',
  fix: { kind: 'widenFileAccess', step: 's_check', to: 'ask-first', from: 'look-only' },
};

const PLAIN_NOTE: Note = {
  level: 'problem',
  stepId: 's_check',
  message: 'Two steps write to the same folder.',
};

describe('a note that carries a repair offers it, and the repair lands', () => {
  it('says what the button will DO, not the word "Fix"', () => {
    const button = fixButton(bar([DIAL_NOTE], noop));

    expect(
      button?.label,
      'this button changes what an agent may do to files, so it may not hide behind a generic ' +
        'word. A person has to know what they are agreeing to before the click, not after',
    ).toBe('Set this step to Ask first');
  });

  it('offers nothing on a note whose repair is a decision', () => {
    expect(
      fixButton(bar([PLAIN_NOTE], noop)),
      'two steps in one folder is fixed by drawing an arrow or by giving one its own copy — a ' +
        'button here would have to guess which, and a repair that guesses is worse than a ' +
        'sentence (invariant 16)',
    ).toBeNull();
  });

  it('offers nothing when the screen it is mounted on cannot repair anything', () => {
    expect(
      fixButton(bar([DIAL_NOTE])),
      'without the handler the button is a control with no effect, and those do not enter this repo',
    ).toBeNull();
  });

  it('raises the dial on the STEP and leaves the agent alone', async () => {
    const saved = vi.fn(async () => undefined);
    const store = createWorkflowStore(
      {
        save: async () => 'after-the-save',
        check: async () => [],
        saveAgent: saved,
      },
      file(),
    );

    await store
      .getState()
      .applyFix({ kind: 'widenFileAccess', step: 's_check', to: 'ask-first', from: 'look-only' }, [
        jarvis(),
      ]);

    const step = store.getState().document.steps[0];
    expect(
      step?.kind === 'agent' ? step.overrides : 'not an agent step',
      'the dial equal to what the agent already carries drops the override instead of writing ' +
        'the same value twice — that is what makes this repair reversible by hand',
    ).toEqual({});
    expect(
      saved,
      'the dial is a step override, so no agent file may be touched: the same role is used by ' +
        'other workflows and this repair was about one tile',
    ).not.toHaveBeenCalled();
  });

  it('takes the connection tools off the AGENT and leaves the document alone', async () => {
    const saved = vi.fn(async () => undefined);
    const store = createWorkflowStore(
      {
        save: async () => 'after-the-save',
        check: async () => [],
        saveAgent: saved,
      },
      file(),
    );
    const before = store.getState().document;

    await store.getState().applyFix(
      {
        kind: 'dropTools',
        agent: jarvis().id,
        agentName: 'design-qa',
        tools: ['mcp__playwright'],
      },
      [jarvis()],
    );

    expect(
      saved,
      'no dial covers a tool from a connection, so this repair has nowhere else to land',
    ).toHaveBeenCalledWith({ ...jarvis(), tools: { only: ['Read', 'Bash'] } });
    expect(
      store.getState().document,
      'and the workflow file is untouched: the tool list belongs to the role, not to this tile',
    ).toBe(before);
  });
});
