/* Kafelek „sprawdź" da się WYPEŁNIĆ — inaczej przycisk, który go stawia, prowadzi donikąd.
 *
 * Kryterium jest napisane w kształcie `start-and-leave-has-a-panel.test.tsx` i z tego samego
 * powodu: ta wada zdarzyła się w tym repo już TRZY RAZY (krok bez wybranego agenta, punkt
 * kontrolny bez importera, kafelek „uruchom i zostaw" bez gałęzi) i za każdym razem wyglądała
 * identycznie — kafelek stoi na płótnie, a klik w niego wpada w cudzy panel albo w zdanie
 * o niezaznaczonym kroku. Tutaj wpada w „wybierz agenta", czyli w formularz, który pyta o rzecz,
 * której ten kafelek nie ma.
 *
 * SŁABĄ WERSJĄ jest wyrenderowanie `CheckPanel` wprost. To przechodzi w chwili, w której ten
 * plik zacznie cokolwiek rysować, i nie mówi ani słowa o tym, czy ekran go MONTUJE — a dokładnie
 * tego brakowało `checkpoint-panel.tsx` przez cały dzień, w którym miał komplet testów i zero
 * importerów. Renderujemy więc `PanelForStep`, czyli tę jedną funkcję, która rozstrzyga „jaki
 * panel dostaje ten kafelek", i pytamy o markup, który z niej wyszedł.
 *
 * DRUGĄ SŁABĄ WERSJĄ jest sam markup pól. Pole, które istnieje i nie oddaje wpisanej wartości,
 * wygląda na ekranie dokładnie tak samo jak działające — a przy tym kafelku pusty wzorzec jest
 * odmową zapisu. Dlatego każdy z czterech uchwytów jest WOŁANY i sprawdzamy, z jakim kluczem
 * wyszedł.
 *
 * ATRAPA JEST JEDNA I PRZEPUSZCZAJĄCA: `./check-panel` woła prawdziwy komponent i tylko zapisuje
 * po drodze jego drzewo. Bez niej nie da się dosięgnąć uchwytu — `renderToStaticMarkup` oddaje
 * napis, a napis nie ma handlerów.
 *
 * KAFELEK BIERZEMY Z `freshStep`, tej samej funkcji, którą woła przycisk płótna. Napisany tutaj
 * ręcznie byłby kafelkiem poprawnie wypełnionym, czyli dokładnie tym przypadkiem, który i tak
 * działa: plik z takim krokiem przyjeżdża dziś z importu. Ten wychodzi pusty, bo taki wychodzi
 * z przycisku.
 */
import { isValidElement } from 'react';
import type { ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Step, WorkflowFile } from '../../../state/workflows';
import { freshId, freshStep } from '../canvas/connect';
import type { CheckFields, CheckPanelProps } from './check-panel';
import { PanelForStep } from './panel';

const spy = vi.hoisted(() => ({
  /** Drzewa oddane przez panel — po jednym na jego zamontowanie. */
  shown: [] as ReactElement[],
  /** Co panel wysłał w górę, w kolejności wywołań. */
  edits: [] as CheckFields[],
}));

vi.mock('./check-panel', async (importOriginal) => {
  const real = await importOriginal<typeof import('./check-panel')>();
  return {
    CheckPanel: (props: CheckPanelProps): ReactElement => {
      const tree = real.CheckPanel(props);
      spy.shown.push(tree);
      return tree;
    },
  };
});

const START: WorkflowFile = {
  format: 1,
  id: 'wf_ship_a_feature',
  name: 'Ship a feature',
  steps: [],
  links: [],
};

/** Kafelek prosto z przycisku płótna — TĄ SAMĄ funkcją, którą woła przycisk. */
const CHECK = freshStep('check', freshId(START), { x: 24, y: 24 });

const COMMAND = './verify.sh full';
const PROOF = String.raw`(\d+) passed`;

const noop = () => undefined;

/** Panel zaznaczonego kafelka, wyrenderowany tak, jak renderuje go ekran. */
function panelFor(step: Step): string {
  return renderToStaticMarkup(
    <PanelForStep
      step={step}
      agents={[]}
      skills={[]}
      onChooseAgent={noop}
      onCreateAgent={noop}
      onEdit={noop}
      onEditStep={noop}
      onEditCheckpoint={noop}
      onEditServe={noop}
      onEditCheck={(fields) => {
        spy.edits.push(fields);
      }}
      onReset={noop}
      onChooseSkills={noop}
      wayBack={null}
      onEditWayBack={noop}
    />,
  );
}

type Change = (event: { target: { value: string } }) => void;

/** Uchwyt `onChange` pierwszego elementu, który pasuje — wyjęty z drzewa Reacta.
 *
 * Drzewo, nie markup: `renderToStaticMarkup` oddaje napis, a napis nie niesie handlerów.
 * `null`, kiedy takiego elementu nie ma — i wołający MA to sprawdzić, bo pole nieznalezione
 * i pole, które nic nie robi, wyglądają w teście identycznie. */
type Props = Record<string, unknown>;

function changeIn(part: unknown, matches: (props: Props) => boolean): Change | null {
  if (Array.isArray(part)) {
    for (const one of part) {
      const hit = changeIn(one, matches);
      if (hit !== null) return hit;
    }
    return null;
  }
  if (typeof part !== 'object' || part === null) return null;
  if (!isValidElement<Props>(part)) return null;

  const handler = part.props['onChange'];
  if (typeof handler === 'function' && matches(part.props)) {
    return (event) => {
      handler(event);
    };
  }
  return changeIn(part.props['children'], matches);
}

function changeOf(part: unknown, id: string): Change | null {
  return changeIn(part, (props) => props['id'] === id);
}

/** Propsy pierwszego elementu, który pasuje — ten sam spacer po drzewie, co wyżej. */
function propsIn(part: unknown, matches: (props: Props) => boolean): Props | null {
  if (Array.isArray(part)) {
    for (const one of part) {
      const hit = propsIn(one, matches);
      if (hit !== null) return hit;
    }
    return null;
  }
  if (typeof part !== 'object' || part === null) return null;
  if (!isValidElement<Props>(part)) return null;
  if (matches(part.props)) return part.props;
  return propsIn(part.props['children'], matches);
}

/** Uchwyt wyboru miejsca pracy.
 *
 * 2026-08-31 — SZUKAMY GO NA KONTROLCE, NIE NA PRZYCISKU. Wybór „gdzie to biegnie" jest od tego
 * dnia jedną wspólną kontrolką dla wszystkich trzech rodzajów kafelka (`./where-it-works.tsx`),
 * więc w drzewie TEGO panelu stoi jako jeden element z `onChoose`, a trzy przyciski powstają
 * dopiero w jej własnym renderze. Kryterium dalej pyta o to samo: czy wybór wychodzi z panelu
 * pod kluczem, którego używa plik. */
function chooseFolder(part: unknown): ((folder: { use: string }) => void) | null {
  const props = propsIn(part, (one) => typeof one['onChoose'] === 'function');
  const handler = props?.['onChoose'];
  return typeof handler === 'function' ? (handler as (folder: { use: string }) => void) : null;
}

beforeEach(() => {
  spy.shown.length = 0;
  spy.edits.length = 0;
});

describe('the panel of a check tile edits every field the tile has', () => {
  it('opens the moment the tile is picked, instead of asking who does this', () => {
    const markup = panelFor(CHECK);

    expect(
      markup,
      'picking this tile answers with the form for picking an agent. This tile has no agent — ' +
        'it runs a command and reads the answer out of what came back — so that form asks about ' +
        'something it does not have, and the two fields that make the tile mean anything are ' +
        'nowhere on the screen.',
    ).not.toContain('data-needs-agent');
    expect(markup, 'no panel of any kind came out for this tile').toContain('data-step-panel');
    expect(
      spy.shown.length,
      'the screen never mounted the panel for this kind of tile. A file with zero importers is ' +
        'exactly how the checkpoint panel sat in this repo until 2026-08-18: written, correct, ' +
        'and unreachable from the window.',
    ).toBe(1);
    expect(
      markup,
      'this tile has no agent, so it inherits nothing and must not be shown the rows an agent ' +
        'step inherits: half of them would answer a question nobody asked.',
    ).not.toContain('id="step-give-up-after"');
  });

  it('carries a field for the name, the command, the pattern and where it runs', () => {
    const markup = panelFor(CHECK);

    expect(markup, 'no field for the name of the tile').toContain('id="check-name"');
    expect(
      markup,
      'the name the canvas gave this tile has to reach its panel — a panel showing somebody ' +
        "else's tile is worse than no panel.",
    ).toContain(`value="${CHECK.name}"`);

    expect(
      markup,
      'the panel carries no field for the command, which is the one thing this tile runs.',
    ).toContain('id="check-command"');
    expect(markup, 'and it has to say so in the words the person reads elsewhere').toContain(
      'Command to run',
    );

    expect(
      markup,
      'the panel carries no field for what the output has to say. Without it the answer would ' +
        'be read out of nothing but whether the command came back, and a suite that ran no ' +
        'tests at all comes back happy — which is the whole reason this field exists.',
    ).toContain('id="check-passes-when"');
    expect(
      markup,
      'and the field has to be named on screen the way this product names it, not the way the ' +
        'file names it.',
    ).toContain('Counts as passed when the output contains');
    expect(
      markup,
      'one sentence of help, and it has to carry the only stand-in this pattern knows. Without ' +
        'it the person has no way to guess that a number is written this way, and every ' +
        'pattern they write with a real number in it stops matching the next time the count ' +
        'changes.',
    ).toContain(String.raw`(\d+)`);

    expect(
      markup,
      'the panel does not say where this runs. For a check that is not a detail: run in the ' +
        'project folder it looks at code WITHOUT the work the step before it just wrote, and ' +
        'passes on the old version. Since 2026-08-31 the question is worded once for all three ' +
        'kinds of tile, and its own criterion holds the three panels to that one wording.',
    ).toContain('Where it works');
    for (const use of ['same-copy', 'project', 'fresh-copy']) {
      expect(
        markup,
        'the choice of where it runs is missing the answer "' +
          use +
          '". A choice with one answer left out is a choice the person cannot make, and the ' +
          'file already carries all three.',
      ).toContain(`value="${use}"`);
    }

    expect(
      markup,
      'the panel does not say what happens when the check does not pass. That is the blind spot ' +
        'this field exists to end: without it a failing check silently wipes out every step ' +
        'after it, and nobody chose that.',
    ).toContain('If this check does not pass');
    for (const answer of ['stop', 'carry-on', 'ask-me']) {
      expect(
        markup,
        'the answer "' + answer + '" is not among the ones offered for a check that does not pass',
      ).toContain(`value="${answer}"`);
    }
  });

  it('sends every change up with the key it belongs to', () => {
    panelFor(CHECK);
    const tree = spy.shown.at(0);
    expect(
      tree,
      'the screen mounted no panel for this tile, so there is no field to type into.',
    ).toBeDefined();

    const typeCommand = changeOf(tree, 'check-command');
    const typePattern = changeOf(tree, 'check-passes-when');
    const pickFolder = chooseFolder(tree);
    const pickFailure = changeOf(tree, 'check-when-it-fails');
    const typeName = changeOf(tree, 'check-name');

    expect(
      [typeName, typeCommand, typePattern, pickFolder, pickFailure].filter((one) => one === null)
        .length,
      'one of the five controls in the rendered panel has no handler, so the rest of this test ' +
        'would assert nothing at all about it. A control with nothing behind it does not go ' +
        'into this repo (invariant 16), and it looks on screen exactly like one that works.',
    ).toBe(0);

    typeName?.({ target: { value: 'Run the checks' } });
    typeCommand?.({ target: { value: COMMAND } });
    typePattern?.({ target: { value: PROOF } });
    pickFolder?.({ use: 'fresh-copy' });
    pickFailure?.({ target: { value: 'carry-on' } });

    expect(
      spy.edits,
      'what the person typed and picked did not come back out of the panel under the keys the ' +
        'file uses. A field that accepts text and hands it up under the wrong name is a field ' +
        'whose work lands on some other setting, or on none.',
    ).toEqual([
      { name: 'Run the checks' },
      { command: COMMAND },
      { proof: PROOF },
      { folder: { use: 'fresh-copy' } },
      { whenItFails: 'carry-on' },
    ]);
  });
});
