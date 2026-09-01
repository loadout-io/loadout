/* Kafelek „sprawdź" da się POSTAWIĆ, i wychodzi z przycisku pusty, z niczym rozstrzygniętym
 * za człowieka.
 *
 * PO CO TO KRYTERIUM ISTNIEJE. Rust ma ten rodzaj kroku w całości: walidator odmawia zapisu
 * bez wzorca, sterownik liczy wynik z kodu wyjścia ORAZ z dowodu w wyjściu, a wynik jedzie do
 * tras warunkowych i do pętli. Okno nie miało go wcale — płótno taki kafelek rysowało, ale
 * żaden przycisk go nie stawiał. Skutek jest większy, niż wygląda: dopóki tego kafelka nie ma,
 * KAŻDA pętla, jaką człowiek zbuduje, jest pętlą „co agent powiedział", a rozróżnienie
 * z `FOUNDATIONS.md` §2.1 — jedyny powód istnienia tego produktu — nie ma na płótnie żadnego
 * nośnika.
 *
 * SŁABĄ WERSJĄ jest napisanie kroku ręcznie i sprawdzenie, że mapper go przewozi. To przechodzi
 * dla kafelka POPRAWNIE WYPEŁNIONEGO, czyli dla przypadku, który i tak działa — plik z takim
 * krokiem przyjeżdża dziś z importu i przez płótno przechodzi. Kafelek bierzemy więc z tej samej
 * funkcji, którą woła przycisk płótna (`addStep`), bo dokładnie ten wychodzi pusty.
 *
 * DRUGĄ SŁABĄ WERSJĄ jest `expect(added.kind).toBe('check')` i koniec. Kafelek z właściwym
 * rodzajem i pustym kompletem pól wygląda na płótnie tak samo jak wypełniony, a odmawia dopiero
 * przy zapisie; kafelek z podpowiedzianą komendą (`npm test`) jest jeszcze gorszy, bo wygląda
 * jak decyzja człowieka, a ten kafelek URUCHAMIA to, co w nim stoi. Dlatego asercja stoi na
 * WSZYSTKICH czterech polach naraz.
 *
 * TRZECIĄ SŁABĄ WERSJĄ jest porównanie kafelka sprzed podróży z kafelkiem po podróży: dopóki
 * obie strony są `null`, taka asercja przechodzi na niczym. Podróż tam i z powrotem sprawdzamy
 * więc na WPISANYCH wartościach i porównujemy z literałem, nie z drugim odczytem.
 */
import { describe, expect, it } from 'vitest';
import type { AgentStep, Folder, Step, WhenItFails, WorkflowFile } from '../../../state/workflows';
import { addStep } from './connect';
import { toCanvas, toFile } from './map';

/** Wiersz powłoki, jaki człowiek naprawdę wpisuje w ten kafelek. */
const COMMAND = './verify.sh full';

/** Wzorzec z jedynym metaznakiem, jaki ten produkt zna — ta sama notacja, co w linii `expect:`
 * naszej własnej bramki. */
const PROOF = String.raw`(\d+) passed`;

/** Cztery pola, bez których ten kafelek nie znaczy nic.
 *
 * `whenItFails` jest tu WYMAGANE, choć w kroku jest opcjonalne: `toEqual` zrównuje brak klucza
 * z kluczem o wartości `undefined`, więc kształt bez tego pola przepuściłby kafelek, który
 * o porażce nie mówi nic. */
interface CheckFields {
  command: string;
  proof: string;
  folder: Folder;
  whenItFails: WhenItFails | undefined;
}

/** Cztery pola tego kafelka, czytane BEZ ani jednego rzutowania.
 *
 * `null` znaczy „to nie jest kafelek sprawdzenia" i jest odpowiedzią, nie awarią: rzutowanie
 * kroku innego rodzaju na ten kształt dałoby cztery `undefined` i asercję o niczym. */
function fieldsOf(step: Step | undefined): CheckFields | null {
  if (step === undefined || step.kind !== 'check') return null;
  return {
    command: step.command,
    proof: step.proof,
    folder: step.folder,
    whenItFails: step.whenItFails,
  };
}

function agentStep(id: string, name: string, at: { x: number; y: number }): AgentStep {
  return {
    kind: 'agent',
    id,
    name,
    agent: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    overrides: {},
    copies: 1,
    instructions: 'Do the work.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at,
  };
}

/** Dokument, w którym KTOŚ JUŻ NARYSOWAŁ strzałkę — bez niej „strzałki zostały nietknięte"
 * mierzy zero. */
function file(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_ship_a_feature',
    name: 'Ship a feature',
    steps: [
      agentStep('s_plan', 'Plan', { x: 24, y: 24 }),
      agentStep('s_build', 'Build', { x: 24, y: 168 }),
    ],
    links: [{ from: 's_plan', to: 's_build' }],
  };
}

/** Ten sam dokument z jednym krokiem podmienionym — tą drogą panel zapisuje wpisane pola. */
function withStep(doc: WorkflowFile, id: string, next: Step): WorkflowFile {
  return { ...doc, steps: doc.steps.map((step) => (step.id === id ? next : step)) };
}

describe('a tile that runs a check can be put on the canvas', () => {
  /* 2026-08-31 — TO KRYTERIUM ZMIENIŁO ZDANIE, na polecenie właściciela. Do tego dnia żądało
   * kafelka PUSTEGO („a made-up example like npm test reads on the canvas exactly like a
   * decision the person made"). Argument był prawdziwy i kosztu, o którym milczał, nie
   * przeważał: pusty kafelek nie zapisywał się wcale — `workflow::file::save` odmawia na
   * pierwszym problemie, więc 400 ms po kliknięciu stał czerwony pasek, a razem z tym kafelkiem
   * na dysk przestawała docierać cała reszta pracy. Powód zmiany w całości stoi przy `freshStep`
   * w `./connect.ts`; nowa asercja pilnuje jej dokładnie tak samo ostro, jak stara pilnowała
   * pustki. */
  it('comes out of the add button ready to be saved, in a folder it can be in', () => {
    const before = file();

    const { file: next, step: added } = addStep('check', before);

    expect(next.steps, 'the button put down no tile at all').toHaveLength(before.steps.length + 1);
    expect(
      next.steps.at(-1)?.id,
      'the new tile has to arrive at the END of the list: that order is the order things were ' +
        'put down and it is never sorted, so inserting in the middle rewrites the tail of the ' +
        'file in git for a tile nobody moved.',
    ).toBe(added.id);
    expect(
      added.kind,
      'the button handed back a tile of a different kind, so there is still no way to put a ' +
        'check on the canvas. Every loop a person builds is then a loop about what an agent ' +
        'said, and the one thing this product exists to tell apart has no place to live.',
    ).toBe('check');
    expect(
      fieldsOf(added),
      'the tile does not come out of the button as a document that can be saved. Both text ' +
        'fields carry the very value the panel already shows in grey under the cursor, and the ' +
        'folder is one a tile with nothing in front of it can actually be in — an empty field ' +
        'or "the same copy as the step before it" is a refusal from the disk 400 ms after the ' +
        'click, and everything else on the canvas stops landing with it. And it stops the work ' +
        'when it does not pass: carrying on past a check that said no is the one answer that ' +
        'makes the tile pointless.',
    ).toEqual({
      command: 'npm test',
      proof: PROOF,
      folder: { use: 'project' },
      whenItFails: 'stop',
    });
  });

  it('leaves the arrows the person already drew exactly as they were', () => {
    const before = file();

    const { file: next } = addStep('check', before);

    expect(
      next.links,
      'putting down a tile is not an opinion about the arrows. An implementation that rebuilds ' +
        'them here loses connections the person drew by hand, which is worse than the gap this ' +
        'tile fills.',
    ).toEqual(before.links);
  });

  it('is a tile the canvas knows how to draw', () => {
    const { file: next, step: added } = addStep('check', file());

    const drawn = toCanvas(next).nodes.find((tile) => tile.id === added.id);

    expect(
      drawn?.type,
      'the canvas mapper does not know what to draw for this tile, so it would land on the ' +
        'board as something else — or as nothing. A tile a person cannot see is a tile they ' +
        'cannot pick, and picking it is the only way to fill it in.',
    ).toBe('check');
  });

  it('keeps the command, the pattern, the folder and the answer to failure on the way to the file and back', () => {
    const { file: put, step: added } = addStep('check', file());

    /* Wypełniamy kafelek tak, jak wypełnia go panel: to jest jedyna wersja tej podróży, która
     * mierzy przewożenie WARTOŚCI, a nie samych domyślnych. */
    const filled: Step =
      added.kind === 'check'
        ? {
            ...added,
            command: COMMAND,
            proof: PROOF,
            folder: { use: 'project' },
            whenItFails: 'carry-on',
          }
        : added;

    const doc = withStep(put, added.id, filled);
    const view = toCanvas(doc);
    /* Płótno → plik → JSON → plik. Ostatni krok nie jest ozdobą: na dysku leży JSON, więc pole,
     * którego nie da się zapisać i odczytać, ginie po pierwszym zamknięciu okna. */
    const written = toFile(doc, view.nodes, view.edges);
    const reopened = JSON.parse(JSON.stringify(written)) as WorkflowFile;

    expect(
      fieldsOf(reopened.steps.find((step) => step.id === added.id)),
      'a field the window does not carry is a field the window quietly drops: the person types ' +
        'it, the canvas shows it, and the file on disk never had it. That is worse than a ' +
        'refusal, because everything on screen says it worked.',
    ).toEqual({
      command: COMMAND,
      proof: PROOF,
      folder: { use: 'project' },
      whenItFails: 'carry-on',
    });
  });
});
