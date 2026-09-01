/* T-131 AC-2: the real Memory screen shows current reach, omissions and typed provenance.
 *
 * The future wire fields are added by structural spread to today's Note. This keeps collection
 * honest before implementation: the production modules load, the real screen renders, and the
 * assertions fail only because those fields do not affect visible markup yet.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import type { Note } from '../../state/memory';
import { useMemory } from '../../state/memory';
import NotesShelf from './shelf';

const OPAQUE_ORIGIN = '019b0131-aaaa-7bbb-8ccc-0123456789ab';

type AcceptanceNote = Note & {
  readonly project?: string | null;
  readonly leftOut?: boolean;
};

function note(id: string, rule: string, fields: Partial<AcceptanceNote>): AcceptanceNote {
  const current: Note = {
    place: 'project',
    id,
    title: id,
    rule,
    because: 'T131 fixture reason',
    status: 'in-use',
    scope: 'this-project',
    length: 37,
    occurrences: 1,
    modified: '2026-08-26T10:00:00Z',
  };
  return { ...current, ...fields };
}

const EVERYWHERE = note('everywhere', 'T131 every project rule', {
  place: 'library',
  scope: 'everywhere',
  leftOut: false,
});
const THIS_PROJECT = note('this-project', 'T131 this project rule', {
  scope: 'this-project',
  leftOut: false,
});
const ONLY_FORGE = note('only-forge', 'T131 only Forge rule', {
  place: 'library',
  scope: 'this-agent',
  agent: 'Forge',
  leftOut: false,
});
const LEGACY = note('legacy', 'T131 earlier project rule', {
  place: 'library',
  scope: 'this-project',
  status: 'suggested',
  leftOut: false,
});
const IMPORTED = note('imported', 'T131 typed imported origin', {
  project: OPAQUE_ORIGIN,
  from: null,
  leftOut: false,
});
const AFTER_RUN = note('after-run', 'T131 typed run origin', {
  status: 'suggested',
  project: null,
  from: OPAQUE_ORIGIN,
  leftOut: false,
});
const LEFT_OUT = note('left-out', 'T131 over the current length limit', {
  place: 'library',
  scope: 'everywhere',
  leftOut: true,
});

const NOTES: readonly AcceptanceNote[] = [
  EVERYWHERE,
  THIS_PROJECT,
  ONLY_FORGE,
  LEGACY,
  IMPORTED,
  AFTER_RUN,
  LEFT_OUT,
];

function visible(markup: string): string {
  return markup
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function row(markup: string, one: AcceptanceNote): string {
  const marker = `data-note-address="${one.place}:${one.id}"`;
  const start = markup.indexOf(marker);
  if (start < 0) return '';
  const end = markup.indexOf('</li>', start);
  return visible(end < 0 ? markup.slice(start) : markup.slice(start, end));
}

function zone(markup: string, id: string): string {
  const marker = `data-zone="${id}"`;
  const start = markup.indexOf(marker);
  if (start < 0) return '';
  const next = markup.indexOf('data-zone="', start + marker.length);
  return next < 0 ? markup.slice(start) : markup.slice(start, next);
}

function renderMemory(): string {
  return renderToStaticMarkup(<NotesShelf store={useMemory} />);
}

beforeEach(() => {
  useMemory.setState({
    notes: [...NOTES],
    passed: [],
    message: null,
    passedProblem: null,
    choice: null,
  });
});

describe('the current Memory screen tells the truth without guessing from UUID shape', () => {
  it('control: mounts every fixture through the real notes shelf and production NoteRow', () => {
    const markup = renderMemory();

    for (const one of NOTES) {
      expect(
        row(markup, one),
        `the real screen did not mount the production row at ${one.place}:${one.id}`,
      ).toContain(one.rule);
    }
  });

  it('labels the three current reaches on their actual rows', () => {
    const markup = renderMemory();

    expect(row(markup, EVERYWHERE)).toContain('Every project');
    expect(row(markup, THIS_PROJECT)).toContain('This project');
    expect(row(markup, ONLY_FORGE)).toContain('Only Forge');
  });

  it('keeps a library legacy note in Earlier project notes without claiming this project', () => {
    const markup = renderMemory();
    const earlier = zone(markup, 'earlier-project');

    expect(earlier).toContain(LEGACY.rule);
    expect(row(markup, LEGACY)).not.toContain('This project');
  });

  it('keeps the In use lead neutral when the zone mixes reaches and omissions', () => {
    const lead = visible(zone(renderMemory(), 'in-use')).toLowerCase();

    expect(lead).toContain('in use');
    expect(lead).not.toContain('every agent');
    expect(lead).not.toContain('last run');
  });

  it('keeps a left-out note visible and says why it is not in prompts right now', () => {
    const words = row(renderMemory(), LEFT_OUT).toLowerCase();

    expect(words).toContain(LEFT_OUT.rule.toLowerCase());
    expect(words).toContain('not in prompts');
    expect(words).toContain('right now');
    expect(words).toContain('length limit');
    expect(words).not.toContain('last run');
  });

  it('uses typed fields to give the same opaque UUID two exact meanings', () => {
    const markup = renderMemory();
    const imported = row(markup, IMPORTED);
    const afterRun = row(markup, AFTER_RUN);

    expect(imported).toContain(`Imported from ${OPAQUE_ORIGIN}`);
    expect(imported).not.toContain('Suggested after run');
    expect(afterRun).toContain(`Suggested after run ${OPAQUE_ORIGIN}`);
    expect(afterRun).not.toContain('Imported from');
    expect(markup).not.toContain('Imported from another project');
  });
});
