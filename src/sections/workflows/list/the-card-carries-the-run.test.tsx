/* Kompozycja listy workflow, mierzona w markupie — trzy rzeczy, które właściciel zobaczył na
 * zrzucie ekranu 2026-08-31 i nazwał wprost.
 *
 * Słaba wersja każdej z nich to `expect(markup).toContain('Run')`. Przechodzi dla napisu
 * wpisanego gdziekolwiek, także w nagłówku, także pod martwym przyciskiem. Dlatego każde
 * zdanie niżej pyta o POŁOŻENIE i o WAGĘ, a nie o obecność słowa:
 *
 *   1. uruchomienie stoi na KAŻDEJ czytelnej karcie, a nie za wejściem do edytora;
 *   2. czynność główna jest DOKŁADNIE JEDNA na całym ekranie i należy do karty bohatera —
 *      rząd jednakowo głośnych przycisków znaczy, że nikt nie rozstrzygnął, co jest ważne;
 *   3. `Duplicate` i `Delete` leżą WEWNĄTRZ karty, a nie pod nią, i mają dwie różne wagi,
 *      bo jedno z nich jest nieodwracalne.
 *
 * Bez DOM-u: `renderToStaticMarkup`. W repo nie ma `jsdom` ani `@testing-library/react`,
 * a `package.json` stoi na liście `DENIED` w checks/quick-scope.sh. Komponent jest sterowany,
 * więc kryterium nie potrzebuje myszy — pyta o to, co człowiek ma przed oczami.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { RunsBehindIt } from './history';
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

function entry(path: string, id: string, name: string): WorkflowEntry {
  return { path, place: 'project', workflow: file(id, name) };
}

/** Trzy pozycje, bo „dokładnie jedna czynność główna" mierzy się dopiero na kilku kartach. */
const THREE: readonly WorkflowEntry[] = [
  entry('architecture-review.json', 'wf-arch', 'Architecture review'),
  entry('ship-a-feature.json', 'wf-ship', 'Ship a feature'),
  entry('triage-the-inbox.json', 'wf-triage', 'Triage the inbox'),
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

function screen(runs?: ReadonlyMap<string, RunsBehindIt>): string {
  return renderToStaticMarkup(
    <WorkflowList
      workflows={THREE}
      runs={runs ?? new Map<string, RunsBehindIt>()}
      pendingDeleteId={null}
      actions={ACTIONS}
      onOpen={NOTHING_HAPPENS}
      onRun={NOTHING_HAPPENS}
    />,
  );
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Kawałki markupu, po jednym na pozycję listy. Pierwszy element to wszystko przed pierwszą. */
function cards(markup: string): string[] {
  return markup.split('<li ').slice(1);
}

describe('the list puts running a workflow on the card, and puts the tidying inside it', () => {
  it('gives every readable workflow its own way to run, without opening the editor first', () => {
    const markup = screen();

    expect(
      occurrences(markup, 'data-tile'),
      'three workflows, three cards — otherwise the count below is a statement about a list ' +
        'that is not there',
    ).toBe(3);
    expect(
      occurrences(markup, 'data-run'),
      'and three ways to run one. Before this, the card said "3 steps · 3 agents" and nothing ' +
        'else: the one thing a person comes to this screen to do lived behind opening the ' +
        'editor and finding it there',
    ).toBe(3);
  });

  it('makes exactly one of them the loud one, and it is the card standing first', () => {
    const markup = screen();

    expect(
      occurrences(markup, 'btn-primary'),
      'exactly one main action on the screen. Three equally loud Run buttons say that nobody ' +
        'decided which workflow this person came for',
    ).toBe(1);

    const loud = cards(markup).filter((card) => card.includes('btn-primary'));
    expect(loud, 'and the loud one belongs to a card, not to the header').toHaveLength(1);
    expect(
      loud[0]?.includes('Architecture review'),
      'with nothing run yet, the card standing first is the first readable one, and it is the ' +
        'one that carries the main action',
    ).toBe(true);
  });

  it('keeps Duplicate and Delete inside the card, at two different weights', () => {
    const markup = screen();
    const withTiles = cards(markup).filter((card) => card.includes('data-tile'));

    expect(withTiles, 'three cards to look inside').toHaveLength(3);
    for (const card of withTiles) {
      expect(
        card.includes('>Duplicate<'),
        'Duplicate has to live inside the card it belongs to. It used to hang under the frame, ' +
          'and at ten workflows that is twenty grey rectangles standing between a person and ' +
          'the names they came to read',
      ).toBe(true);
      expect(
        card.includes('>Delete<'),
        'and so does Delete, for the same reason and by the same measurement',
      ).toBe(true);
    }

    expect(
      occurrences(markup, 'btn-danger'),
      'one Delete per card wearing the weight of something that cannot be taken back',
    ).toBe(3);
    expect(
      occurrences(markup, 'btn-quiet'),
      'and neither of the two is the old equal-weight pair any more: Delete and Duplicate had ' +
        'the same button on them, though only one of them loses a file for good',
    ).toBe(0);
  });
});
