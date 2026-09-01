/* Kryterium 7: ekran agenta mieści się w suficie gęstości [ARCHITECTURE §7].
 *
 * Licznik chodzący po kluczach najwyższego poziomu modelu zwraca trzy i będzie zielony na
 * zawsze — przy każdym ekranie, jaki ktokolwiek kiedykolwiek napisze, łącznie z takim,
 * który ma czterysta wierszy. Zielone bez dowodu wykonania jest czerwone (niezmiennik 19),
 * a licznik, którego nie da się przewrócić, nie jest pomiarem, tylko ozdobą stojącą w
 * miejscu pomiaru — czyli gorszą wersją braku pomiaru.
 *
 * Dlatego w tym pliku są DWA przypadki i drugi jest ważniejszy: ta sama scena z rozwiniętym
 * transkryptem MUSI przekroczyć baseline. Licznik, który zdał pierwszy i nie zdał drugiego,
 * niczego nie mierzy; licznik, który zdał oba, mierzy dokładnie to, co miał.
 *
 * Scena jest ustalona i nie ma się zmieniać przy kolejnych zmianach ekranu: jedno przekazanie
 * na wejściu, jedna notatka „w użyciu", dwie zmienione ścieżki, jedno przekazanie na wyjściu
 * i dwanaście wierszy transkryptu w stanie domyślnym. Jedenaście z nich to zwinięte grupy po
 * sześć odczytów — zwinięta grupa liczy się JAKO JEDEN wiersz i o to w suficie chodzi.
 */
import { describe, expect, it } from 'vitest';
import type { FeedLine } from '../../../state/run';
import { line } from '../feed/fixtures/lines';
import { sealedScroller } from '../feed/fixtures/scroller';
import type { Feed, FeedView } from '../feed/model';
import { createFeed } from '../feed/model';
import { countRegions, countTextNodes } from './density';
import type { SessionInput } from './layout';
import { sessionSections } from './layout';

/**
 * Limit z tabeli [ARCHITECTURE §7]. Baseline może TYLKO maleć.
 *
 * Stoi na suficie, bo w fazie kontraktu nie ma jeszcze czego zmierzyć. Pierwszy pomiar na
 * gotowym ekranie ma go opuścić do wartości rzeczywistej — podniesienie go kiedykolwiek
 * później jest tym, jak poprzedni prototyp doszedł do 2,4× własnego limitu.
 */
const DENSITY_BASELINE = 60;

/** Ile oznaczonych regionów wolno mieć ekranowi [ARCHITECTURE §7]. */
const REGION_CEILING = 8;

const A = 'Forge';
const GROUPS = 11;
const PER_GROUP = 6;

/** Jedenaście zwiniętych grup po sześć odczytów plus jedna notatka — dwanaście wierszy. */
function twelveRows(): Feed {
  const feed = createFeed(sealedScroller());
  const lines: FeedLine[] = [];
  let id = 0;
  for (let group = 0; group !== GROUPS; group += 1) {
    for (let n = 0; n !== PER_GROUP; n += 1) {
      id += 1;
      // Grupy odległe o 3 s, więc okno sklejania zamyka się między nimi i otwiera w środku.
      lines.push(line.read(id, group * 3_000 + n * 100, A, 'src/parser.rs'));
    }
  }
  lines.push(line.note(id + 1, GROUPS * 3_000, A, 'Rewrote the field splitter.'));
  feed.appendLines(lines);
  return feed;
}

function sceneFor(view: FeedView): SessionInput {
  return {
    view,
    steps: [
      { agent: A, name: 'Build', brief: 'rewrite quote handling as a state machine', files: [] },
    ],
    handoffs: [
      { from: 'Orion', to: A, file: 'brief.md', summary: 'what to build', detailId: 11 },
      { from: A, to: 'Needle', file: 'patch-summary.md', summary: 'what changed', detailId: 12 },
    ],
    changes: [
      { agent: A, path: 'src/parser.rs', added: 42, removed: 8, detailId: 21 },
      { agent: A, path: 'tests/parser.rs', added: 6, removed: 0, detailId: 22 },
    ],
    notes: [{ agent: A, text: 'Prefer small state machines for parsing', detailId: 31 }],
  };
}

describe('the screen for one agent stays under the density ceiling', () => {
  it('fits inside the baseline in its default state', () => {
    const feed = twelveRows();
    expect(feed.view.history.length, 'twelve rows, as the fixed scene says').toBe(12);

    const sections = sessionSections({ id: A, name: 'Forge' }, sceneFor(feed.view));

    expect(
      DENSITY_BASELINE,
      'the ceiling from the table is 60 and may only come down',
    ).toBeLessThanOrEqual(60);
    expect(
      countTextNodes(sections),
      'every element of the model that carries text, counted all the way down: section ' +
        'heading, row label, row value, chip, and one closed line as one',
    ).toBeLessThanOrEqual(DENSITY_BASELINE);
    expect(
      countRegions(sections),
      'and eight named regions on a screen, which the earlier prototype answered with thirty',
    ).toBeLessThanOrEqual(REGION_CEILING);
  });

  it('goes over the baseline once all twelve rows are opened', () => {
    const feed = twelveRows();
    const closed = countTextNodes(sessionSections({ id: A, name: 'Forge' }, sceneFor(feed.view)));

    // Rozwijamy KAŻDY wiersz, nie tylko zwinięte: notatka jest rozwinięta domyślnie, więc
    // ślepe przełączenie wszystkiego zamknęłoby ją i zaniżyło pomiar o jeden.
    for (const row of feed.view.history) {
      if (!row.expanded) feed.toggle(row.id);
    }
    for (const row of feed.view.history) {
      expect(row.expanded, 'the whole transcript is open now').toBe(true);
    }

    const opened = countTextNodes(sessionSections({ id: A, name: 'Forge' }, sceneFor(feed.view)));

    expect(
      opened,
      'sixty-six read lines are behind those eleven closed rows. A counter that walks only ' +
        'the top level of the model answers three here as well, and would be green for ' +
        'every screen anyone ever writes',
    ).toBeGreaterThan(DENSITY_BASELINE);
    expect(opened, 'and opening a row can only add to the count').toBeGreaterThan(closed);
  });
});
