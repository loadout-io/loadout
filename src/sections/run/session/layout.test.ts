/* Kryterium 3: ekran agenta prowadzi dwoma blokami faktów, transkrypt jest trzeci.
 *
 * `expect(sections[0].id).toBe('given')` przechodzi dla bloku z pięcioma wierszami wpisanymi
 * na stałe za makietą, z których trzy są puste. Kolejność się zgadza, ekran wygląda jak
 * makieta, a trzy z pięciu wierszy nie niosą nic — dokładnie tak, jak poprzedni prototyp renderował
 * `SPEND: not reported` obok wierszy z prawdziwą liczbą, w tej samej siatce i tym samym
 * krojem, więc nie dało się ich odróżnić inaczej niż czytając.
 *
 * Rozróżniają to dwie asercje, druga ważniejsza:
 *   1. agent o minimalnym wejściu dostaje tylko te wiersze, które ma, i żaden z nich nie ma
 *      wartości pustej ani zastępczej,
 *   2. agent, którego ostatnia wiadomość brzmi „I fixed everything", a który nie zmienił ani
 *      jednego pliku, ma `produced` puste — a jego deklaracja pojawia się WYŁĄCZNIE
 *      w transkrypcie, jako linia `note` podpisana `agent`.
 *
 * Druga jest tą, dla której cały ten podział istnieje. Deklaracja agenta w rubryce faktów
 * czyta się jak fakt i nie ma na ekranie niczego, co by jej zaprzeczyło [FOUNDATIONS §2.2].
 */
import { describe, expect, it } from 'vitest';
import type { FeedLine } from '../../../state/run';
import { line } from '../feed/fixtures/lines';
import { sealedScroller } from '../feed/fixtures/scroller';
import type { FeedView } from '../feed/model';
import { createFeed } from '../feed/model';
import type { SessionInput } from './layout';
import { sessionSections } from './layout';

const FORGE = { id: 'Forge', name: 'Forge' };
const NEEDLE = { id: 'Needle', name: 'Needle' };

const GIVEN_KINDS = ['step', 'handoff', 'note', 'files'];
const PRODUCED_KINDS = ['changes', 'handoff'];

const BOAST = 'I fixed everything.';

function viewOf(lines: readonly FeedLine[]): FeedView {
  const feed = createFeed(sealedScroller());
  feed.appendLines(lines);
  return feed.view;
}

/** Wszystko, co Forge dostał i zostawił — pełna scena z makiety, linie 438–477. */
function fullRun(): SessionInput {
  return {
    view: viewOf([
      line.read(1, 0, 'Forge', 'src/parser.rs'),
      line.edit(2, 400, 'Forge', 'src/parser.rs', 42, 8),
      line.note(3, 800, 'Forge', 'Rewrote the field splitter as a three-state machine.'),
    ]),
    steps: [
      {
        agent: 'Forge',
        name: 'Build',
        brief: 'rewrite quote handling as a state machine',
        files: ['src/parser.rs', 'tests/parser.rs'],
      },
    ],
    handoffs: [
      {
        from: 'Orion',
        to: 'Forge',
        file: 'brief.md',
        summary: 'what to build and why',
        detailId: 11,
      },
      {
        from: 'Forge',
        to: 'Needle',
        file: '03__forge__patch-summary.md',
        summary: 'what changed and what to check',
        detailId: 12,
      },
    ],
    changes: [
      { agent: 'Forge', path: 'src/parser.rs', added: 42, removed: 8, detailId: 21 },
      { agent: 'Needle', path: 'tests/parser.rs', added: 6, removed: 0, detailId: 22 },
    ],
    notes: [
      {
        agent: 'Forge',
        text: 'Prefer small state machines over patterns for parsing',
        detailId: 31,
      },
    ],
  };
}

/** Agent o najuboższym możliwym wejściu: krok i nic poza nim. */
function bareRun(): SessionInput {
  return {
    view: viewOf([line.read(1, 0, 'Needle', 'tests/parser.rs')]),
    steps: [{ agent: 'Needle', name: 'Check', brief: 'run the checks and report', files: [] }],
    handoffs: [],
    changes: [],
    notes: [],
  };
}

/** Agent, który powiedział, że skończył, i nie zmienił ani jednego pliku. */
function boastRun(): SessionInput {
  return {
    view: viewOf([line.read(1, 0, 'Forge', 'src/parser.rs'), line.note(2, 400, 'Forge', BOAST)]),
    steps: [{ agent: 'Forge', name: 'Build', brief: 'fix the quoted-comma case', files: [] }],
    handoffs: [],
    changes: [],
    notes: [],
  };
}

describe('what it was given and what it produced come before what it said', () => {
  it('lays out three blocks, in that order, headed with the agent name', () => {
    const sections = sessionSections(FORGE, fullRun());

    expect(sections.map((s) => s.id)).toEqual(['given', 'produced', 'transcript']);
    expect(
      sections.map((s) => s.heading),
      'the transcript is what we have to hand, so it is the version that writes itself. ' +
        'A person opens an agent to learn the other two things',
    ).toEqual(['What Forge was given', 'What Forge produced', 'What Forge said']);
  });

  it('puts the name of the agent it was asked about into the headings', () => {
    const headings = sessionSections(NEEDLE, fullRun()).map((s) => s.heading);

    expect(
      headings,
      'the name is read from the agent, not typed once into the layout behind the mockup',
    ).toEqual(['What Needle was given', 'What Needle produced', 'What Needle said']);
  });

  it('keeps each block to its own closed set of row kinds', () => {
    const sections = sessionSections(FORGE, fullRun());
    const given = sections.find((s) => s.id === 'given');
    const produced = sections.find((s) => s.id === 'produced');

    for (const row of given?.rows ?? []) {
      expect(GIVEN_KINDS, 'what an agent was given has four shapes and no others').toContain(
        row.kind,
      );
    }
    for (const row of produced?.rows ?? []) {
      expect(PRODUCED_KINDS, 'what is left behind has two, and both are facts').toContain(row.kind);
    }
    expect(given?.rows.length, 'a step, a handoff coming in, a note in use and the files').toBe(4);
    expect(
      produced?.rows.map((row) => row.kind),
      'one change of its own and one handoff going out. The change made by another agent ' +
        'belongs to that agent',
    ).toEqual(['changes', 'handoff']);
  });

  it('gives an agent with the barest input only the rows it really has', () => {
    const given = sessionSections(NEEDLE, bareRun()).find((s) => s.id === 'given');

    expect(
      given?.rows.map((row) => row.kind),
      'no handoff came in, no note was in use and no files were named, so those rows do ' +
        'not exist. Five rows behind the mockup with three of them blank is the version ' +
        'that looks finished and says less',
    ).toEqual(['step']);
    for (const row of given?.rows ?? []) {
      expect(row.value.trim(), 'no empty value').not.toBe('');
      expect(row.value, 'and no stand-in for one either').not.toBe('—');
    }
  });

  it('leaves "I fixed everything" out of the facts and in the record of what was said', () => {
    const sections = sessionSections(FORGE, boastRun());
    const produced = sections.find((s) => s.id === 'produced');
    const transcript = sections.find((s) => s.id === 'transcript');

    expect(
      produced?.rows.length,
      'the agent changed no file, so there is nothing under what it produced. Feeding this ' +
        'block the last message instead is the single failure this whole screen exists to ' +
        'prevent: what the agent said, printed where a person reads what happened',
    ).toBe(0);
    expect(
      produced?.empty,
      'and the block says so in one plain sentence rather than showing a blank frame',
    ).not.toBeNull();

    const said = transcript?.lines.filter((l) => l.label.includes('I fixed everything')) ?? [];
    expect(said.length, 'the sentence exists exactly once on this screen').toBe(1);
    expect(said[0]?.kind, 'as prose').toBe('note');
    expect(
      said[0]?.who,
      'and signed by the agent, which is the whole reason a person can read it as a boast',
    ).toBe('agent');

    const facts = [...(sections[0]?.rows ?? []), ...(produced?.rows ?? [])];
    expect(
      facts.filter((row) => row.value.includes('I fixed everything')),
      'and nowhere among the facts',
    ).toEqual([]);
  });
});
