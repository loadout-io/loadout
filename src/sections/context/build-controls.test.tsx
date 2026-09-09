import { renderToStaticMarkup } from 'react-dom/server';
import { readFileSync } from 'node:fs';

import { describe, expect, it, vi } from 'vitest';

import { BUILD_BEFORE_ADDING, whatIsNextForTheSet } from '../../state/context';
import type {
  ContextApp,
  ContextBuild,
  ContextDraft,
  ContextRevision,
  ContextSet,
} from '../../state/context';
import BuildControls from './build-controls';
import ContextOverview from './overview';

const RUNNING: ContextBuild = {
  operationId: 'build-1',
  setId: 'set-1',
  generation: 3,
  draftRevision: 2,
  stage: 'extracting',
  end: 'running',
  app: 'claude-code',
  requestedModel: null,
  model: 'sonnet',
  batchesDone: 1,
  batchesTotal: 2,
  sources: [
    { sourceId: 'notes', part: 'fragment 1', outcome: 'processed', said: 'Processed.' },
    { sourceId: 'screen', part: 'whole', outcome: 'unknown', said: '' },
  ],
  said: 'Claude Code is reading batch 2 of 2.',
  revisionId: null,
  startedAt: '2026-09-08T10:00:00Z',
  changedAt: '2026-09-08T10:00:01Z',
};

function markup(
  build: ContextBuild | null,
  hasVersion = false,
  app: ContextApp = 'claude-code',
): string {
  return renderToStaticMarkup(
    <BuildControls
      build={build}
      app={app}
      model=""
      claudeCode={{ state: 'found', version: '2.1.263' }}
      codex={{ state: 'not-found' }}
      hasVersion={hasVersion}
      onChooseApp={vi.fn()}
      onModel={vi.fn()}
      onBuild={vi.fn()}
      onStop={vi.fn()}
      sourceNames={{ notes: 'Project notes', screen: 'Checkout screen' }}
    />,
  );
}

describe('context build controls', () => {
  it('shows who will read the material before the first build', () => {
    const html = markup(null);
    expect(html).toContain('Claude Code');
    expect(html).toContain('Codex');
    expect(html).toContain('Sign-in is checked when the build starts.');
    expect(html).toContain('Build context');
    expect(html).toMatch(/<input(?=[^>]*value="codex")(?=[^>]*disabled="")[^>]*>/);
  });

  it('says try again after a build that produced nothing, and rebuild only when there is a version', () => {
    const cases = [
      [markup(null), 'Build context'],
      [markup(RUNNING), 'Stop'],
      [markup({ ...RUNNING, end: 'failed' }), 'Try building again'],
      [markup({ ...RUNNING, end: 'cancelled' }), 'Try building again'],
      [markup({ ...RUNNING, end: 'ready' }, true), 'Rebuild context'],
      [markup(null, true), 'Rebuild context'],
    ] as const;
    for (const [html, label] of cases) {
      expect(html.match(/data-build-action/g)).toHaveLength(1);
      expect(html).toContain(`>${label}</button>`);
    }
  });

  it('shows a valid model example for either app while an empty value keeps its meaning', () => {
    expect(markup(null)).toMatch(
      /<input(?=[^>]*id="context-build-model")(?=[^>]*placeholder="sonnet")[^>]*>/,
    );
    expect(markup(null, false, 'codex')).toMatch(
      /<input(?=[^>]*id="context-build-model")(?=[^>]*placeholder="gpt-5\.6-sol")[^>]*>/,
    );
    expect(markup(null)).toContain('Model (empty means this app&#x27;s own model)');
  });

  it('says what comes next for an empty draft, material, and a ready version', () => {
    const set: ContextSet = {
      schema: 1,
      id: 'set-1',
      title: 'Checkout',
      description: '',
      archived: false,
      draftRevision: 2,
      latestReadyRevision: null,
      createdAt: '2026-09-08T10:00:00Z',
      changedAt: '2026-09-08T10:00:00Z',
    };
    const draft: ContextDraft = {
      schema: 1,
      sources: [],
      excluded: [],
      howToPrepare: '',
      requirements: [],
    };
    const withMaterial: ContextDraft = {
      ...draft,
      sources: [
        {
          id: 'typed',
          kind: 'text',
          name: 'Typed material',
          description: '',
          text: 'Keep the total visible.',
        },
      ],
    };

    expect(whatIsNextForTheSet(set, draft)).toBe(
      'Add material to this set before it can be built.',
    );
    /* PORÓWNANIE DWÓCH PLIKÓW, NIE TRZECI EGZEMPLARZ NAPISU. To zdanie mieszka w Ruście
       (`catalog_choice`) i tu; wpisane trzeci raz w teście przechodziłoby także wtedy, gdy
       Rust mówi już co innego — czyli dokładnie w awarii, którą ma wykluczyć (niezmiennik 13).
       Czytamy więc bajty z pliku Rusta. Brak trafienia jest PORAŻKĄ, nie pominięciem: zero
       dopasowań z regexa to zwykle zły regex, nie zniknięcie faktu. */
    const rust = readFileSync(
      new URL('../../../src-tauri/src/commands/workflow_context.rs', import.meta.url),
      'utf8',
    );
    const said = /Some\("([^"]*before adding it to a workflow[^"]*)"\.to_owned\(\)\)/.exec(rust);
    expect(
      said?.[1],
      'the Rust side no longer carries a sentence about building before adding; if the wording ' +
        'moved, this comparison has to follow it, not be deleted',
    ).toBeDefined();
    expect(
      whatIsNextForTheSet(set, withMaterial),
      'the set screen and the step panel must say the same thing about the same fact',
    ).toBe(said?.[1]);
    expect(whatIsNextForTheSet(set, withMaterial)).toBe(BUILD_BEFORE_ADDING);
    expect(whatIsNextForTheSet({ ...set, latestReadyRevision: 'revision-1' }, withMaterial)).toBe(
      'This context is built, so a workflow step can add it.',
    );
  });

  it('renders saved progress and every source outcome', () => {
    const html = markup(RUNNING);
    expect(html).toContain('1 of 2 batches');
    expect(html).toContain('Project notes · fragment 1');
    expect(html).toContain('processed');
    expect(html).toContain('Checkout screen · whole');
  });

  it('keeps two conflicting findings and their exact sources visible', () => {
    const revision: ContextRevision = {
      id: 'revision-1',
      setId: 'set-1',
      draftRevision: 2,
      app: 'claude-code',
      requestedModel: null,
      model: 'sonnet',
      createdAt: '2026-09-08T10:01:00Z',
      origin: 'generated',
      topics: [{ id: 'checkout', title: 'Checkout' }],
      findings: [
        {
          id: 'first',
          kind: 'requirement',
          text: 'Keep the total visible.',
          condition: 'before payment',
          sources: [{ sourceId: 'alpha', part: 'fragment 1' }],
          topic: 'checkout',
          conflictsWith: ['second'],
          origin: 'generated',
        },
        {
          id: 'second',
          kind: 'requirement',
          text: 'Keep the total visible.',
          condition: 'after payment',
          sources: [{ sourceId: 'beta', part: 'fragment 1' }],
          topic: 'checkout',
          conflictsWith: ['first'],
          origin: 'generated',
        },
      ],
      questions: [],
      conflicts: ['Keep the total visible.'],
      sources: [],
    };
    const html = renderToStaticMarkup(<ContextOverview revision={revision} onSave={vi.fn()} />);
    expect(html.match(/data-context-finding=/g)).toHaveLength(2);
    expect(html).toContain('Sources: alpha fragment 1');
    expect(html).toContain('Sources: beta fragment 1');
  });
});
