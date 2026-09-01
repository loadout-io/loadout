/* Historia biegów dochodzi na kartę — i milczy, kiedy jej nie ma.
 *
 * DWIE POŁOWY I ANI JEDNEJ MNIEJ, bo każda osobno przechodzi na wadzie, którą druga łapie.
 *   (a) z historią: karta mówi, ile razy ten workflow ruszał i czym skończył się ostatni bieg.
 *       Bez tej połowy `runsBehindThem` jest funkcją z testem i bez ani jednego piksela na
 *       ekranie — dokładnie ta klasa wady, dla której powstał niezmiennik 29.
 *   (b) bez historii: karta nie mówi o biegach ANI SŁOWA — żadnego `—`, `never` ani
 *       `not reported`. Bez tej połowy pierwsza przechodzi także dla karty, która zawsze
 *       rysuje komórkę i tłumaczy się w niej z własnej pustki (00-SYNTHESIS §6).
 *
 * Plus trzecia rzecz, bez której obie byłyby prawdą o ozdobie: pierwsze miejsce na ekranie
 * bierze workflow uruchamiany NAJPÓŹNIEJ, a nie pierwszy alfabetycznie. To jest cała różnica
 * między listą, która wie, co ten człowiek robił wczoraj, a sześcioma jednakowymi kartami.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { PastRunRow } from '../../run/io';
import { runsBehindThem } from './history';
import type { Step, WorkflowEntry, WorkflowListActions, WorkflowFile } from './store';
import { WorkflowList } from './workflow-list';

function step(id: string, name: string, agent: string): Step {
  return {
    kind: 'agent',
    id,
    name,
    agent,
    overrides: {},
    copies: 1,
    instructions: '',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 0, y: 0 },
  };
}

function file(id: string, name: string): WorkflowFile {
  return { format: 1, id, name, steps: [step('s_one', 'Do the thing', 'forge')], links: [] };
}

const TWO: readonly WorkflowEntry[] = [
  {
    path: 'architecture-review.json',
    place: 'project',
    workflow: file('wf-arch', 'Architecture review'),
  },
  { path: 'ship-a-feature.json', place: 'project', workflow: file('wf-ship', 'Ship a feature') },
];

/** Wiersz historii dokładnie w kształcie, w którym oddaje go `list_runs`. */
function ran(when: string, title: string, state: string): PastRunRow {
  return {
    folder: when.replace(/[^0-9]/g, '') + '__x',
    when,
    title,
    state,
    steps: 1,
    costUsd: null,
    said: null,
  };
}

/* `Ship a feature` biegł trzy razy i ostatni raz PÓŹNIEJ niż jedyny bieg `Architecture review`
 * — a alfabetycznie stoi drugi. Bez tej różnicy „bohater jest tym, który biegł ostatni" byłoby
 * prawdą także o wyborze pierwszej pozycji z listy. */
const ROWS: readonly PastRunRow[] = [
  ran('2026-08-20 16:00', 'Architecture review', 'cancelled'),
  ran('2026-08-28 11:30', 'Ship a feature', 'failed'),
  ran('2026-08-29 09:45', 'Ship a feature', 'failed'),
  ran('2026-08-30 18:12', 'Ship a feature', 'succeeded'),
];

const NOTHING_HAPPENS = (): void => undefined;

const ACTIONS: WorkflowListActions = {
  create: () => Promise.resolve(),
  duplicate: () => Promise.resolve(),
  requestDelete: NOTHING_HAPPENS,
  cancelDelete: NOTHING_HAPPENS,
  confirmDelete: () => Promise.resolve(),
  /* Odczyt katalogu jest od 2026-08-31 czescia akcji ekranu: `Try again` na odmowie wola
   * dokladnie to. Te kryteria nie dotykaja tamtego stanu, wiec fikstura go nie robi. */
  load: () => Promise.resolve(),
};

function screen(rows: readonly PastRunRow[]): string {
  return renderToStaticMarkup(
    <WorkflowList
      workflows={TWO}
      runs={runsBehindThem(rows)}
      pendingDeleteId={null}
      actions={ACTIONS}
      onOpen={NOTHING_HAPPENS}
      onRun={NOTHING_HAPPENS}
    />,
  );
}

/** To, co czyta człowiek: bez znaczników, z rozwiniętymi encjami, bez nadmiarowych odstępów. */
function visibleText(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, ' ')
    .replaceAll('&quot;', '"')
    .replaceAll('&#x27;', "'")
    .replaceAll('&lt;', '<')
    .replaceAll('&gt;', '>')
    .replaceAll('&amp;', '&')
    .replace(/\s+/g, ' ')
    .trim();
}

function cards(markup: string): string[] {
  return markup.split('<li ').slice(1);
}

describe('a workflow card says what it has behind it, and stays quiet when it has nothing', () => {
  it('says how often it ran and how the last one ended', () => {
    const text = visibleText(screen(ROWS));

    expect(
      text,
      'three runs under that name, three on the card. The mockup promised "used 12x" and the ' +
        'card carried two numbers counted out of the file and nothing else',
    ).toContain('used 3×');
    expect(text, 'and one run under the other name').toContain('used 1×');
    expect(
      text,
      'plus the day the last one happened, so a person can tell yesterday from March',
    ).toContain('Last run 2026-08-30 18:12');
    expect(
      text,
      'and how it ended, in the English word this application uses for it — never the raw wire ' +
        'word (invariant 14). "succeeded" is what Rust sends; "done" is what a person reads',
    ).toContain('Last run 2026-08-30 18:12 · done');
    expect(text, 'and the wire word itself never reaches the screen').not.toContain('succeeded');
    expect(
      text,
      'the other card ended differently and has to say so, or one sentence is being written ' +
        'for every card regardless of what happened',
    ).toContain('Last run 2026-08-20 16:00 · stopped');
  });

  it('puts the one that ran last on top, not the one that sorts first', () => {
    const first = cards(screen(ROWS))[0] ?? '';

    expect(
      first.includes('Ship a feature'),
      'the card standing first is the workflow this person ran most recently. Sorted by name, ' +
        '"Architecture review" wins — and that is the screen giving its best place to the ' +
        'alphabet instead of to the work',
    ).toBe(true);
    expect(
      first.includes('btn-primary'),
      'and that card is the one carrying the main action, so the biggest thing on the screen ' +
        'is the thing this person came here to do',
    ).toBe(true);
  });

  it('says nothing at all about runs for a workflow that never ran', () => {
    const markup = screen([]);
    const text = visibleText(markup);

    expect(
      text,
      'no run history, no sentence about run history. A card that always draws the cell and ' +
        'writes "never" or "not reported" in it is a place on screen taken by a field ' +
        'explaining its own emptiness',
    ).not.toMatch(/used|Last run|never|not reported/i);
    expect(
      markup,
      'and not hidden in an attribute either — these words cannot occur in a class name here',
    ).not.toMatch(/used |never|not reported/i);
    expect(text, 'while everything the file itself says is still on the card').toContain('1 step');
  });
});
