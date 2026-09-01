/* Run jest główną akcją tego ekranu i wygląda na główną — a liczba rzeczy do poprawienia stoi
 * w JEDNYM miejscu.
 *
 * WADA, zgłoszona przez właściciela 2026-08-31, ze zrzutu z okna 1512 px. `Run` siedział
 * w `<Panel position="top-right">` płótna, czyli był małą nakładką w rogu; „Tidy up" — czynność
 * porządkowa, której człowiek używa raz na dziesięć razy — miał pełnowymiarowy przycisk w rzędzie
 * na dole. Odwrócona waga, i to nie w drobiazgu: to jest przycisk, po który cały ten ekran
 * istnieje.
 *
 * DRUGA POŁOWA JEST O NIEZMIENNIKU 13 i bez niej pierwsza jest niebezpieczna. Zdanie
 * „N things to fix" powstawało DWA RAZY, w dwóch niezależnych kawałkach kodu: raz w plakietce
 * nagłówka (odmiana wpisana wprost w JSX edytora), raz w pasku nad przyciskiem Run. Przeniesienie
 * `Run` do nagłówka bez zdjęcia jednej z kopii postawiłoby je OBOK SIEBIE, w odległości dwóch
 * centymetrów — czyli zamieniłoby cichą duplikację w głośną.
 *
 * DLACZEGO TO KRYTERIUM RENDERUJE CAŁY EKRAN, a nie `RunButton` wprost. Bo pytanie brzmi „gdzie
 * ten przycisk stoi", a nie „czy taki komponent istnieje" (niezmiennik 29). `RunButton`
 * wyrenderowany wprost przechodzi każdą asercję o `.btn-primary` i o `disabled` także wtedy,
 * gdy nikt go nigdzie nie zamontował — a dokładnie tak wyglądały cztery wady złapane przez
 * recenzenta na ZIELONEJ bramce (AGENTS.md §3, niezmiennik 29). Podział z tamtym plikiem jest
 * ostry: `canvas/problems.test.tsx` sądzi POLITYKĘ (co gasi start), ten plik sądzi MIEJSCE
 * (gdzie człowiek to widzi).
 *
 * JAK TU W OGÓLE TRAFIAJĄ UWAGI, skoro `renderToStaticMarkup` nie odpala efektów. Magazyn
 * powstaje WEWNĄTRZ edytora (`useState(() => createWorkflowStore(...))`), więc nie da się go
 * podać z zewnątrz. Atrapa `../../state/workflows` jest PRZEPUSZCZAJĄCA i ZAPAMIĘTUJĄCA: woła
 * prawdziwe `createWorkflowStore`, oddaje prawdziwy magazyn, zapisuje go po drodze i przy drugim
 * renderze oddaje TEN SAM. Między renderami wołamy `recheck()` ręcznie — czyli robimy to, czego
 * `useEffect` w statycznym renderze nie zrobi. Ten sam wzorzec, co w
 * `./the-save-indicator-tells-the-truth.test.tsx`.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { Note, WorkflowFile } from '../../state/workflows';
import { WorkflowEditor } from './editor';

const spy = vi.hoisted(() => ({
  /** Magazyny oddane edytorowi — po jednym na otwarty plik, i to jest cała treść atrapy. */
  made: new Map<string, { getState: () => { recheck: () => Promise<void> } }>(),
}));

/** Zdanie z `workflow::check`, słowo w słowo. Wagę `problem` niesie po to, żeby zgasić start. */
const BLOCKER = 'These steps point back at each other in a circle. Work would never finish.';

/** Ostrzeżenie. NIE gasi startu — i to jest cały powód, dla którego stoi tu obok tamtego. */
const WARNING = '"Build" is not connected to the rest of the workflow.';

const NOTES: Note[] = [
  { level: 'problem', stepId: 's_build', message: BLOCKER },
  { level: 'warning', stepId: 's_build', message: WARNING },
];

vi.mock('./io', () => ({
  write: () => Promise.resolve(),
  check: () => Promise.resolve(NOTES),
}));

vi.mock('../../state/workflows', async (importOriginal) => {
  const real = await importOriginal<typeof import('../../state/workflows')>();
  return {
    ...real,
    createWorkflowStore: (
      io: Parameters<typeof real.createWorkflowStore>[0],
      open: WorkflowFile,
      revision?: string | null,
    ) => {
      const standing = spy.made.get(open.id);
      if (standing !== undefined) return standing;
      const made = real.createWorkflowStore(io, open, revision);
      spy.made.set(open.id, made as never);
      return made;
    },
  };
});

const PATH = 'ship-a-feature.json';

const DOC: WorkflowFile = {
  format: 1,
  id: 'wf_ship_a_feature',
  name: 'Ship a feature',
  steps: [
    {
      kind: 'agent',
      id: 's_build',
      name: 'Build',
      agent: '',
      overrides: {},
      copies: 1,
      instructions: 'Write the smallest change that works.',
      skills: 'all',
      folder: { use: 'project' },
      handover: 'notes',
      at: { x: 24, y: 24 },
    },
  ],
  links: [],
};

const noop = () => undefined;

function editor(): string {
  return renderToStaticMarkup(
    <WorkflowEditor
      path={PATH}
      document={DOC}
      revision="r1"
      agents={[]}
      onClose={noop}
      onRun={noop}
      onCreateAgent={noop}
    />,
  );
}

/** Ekran, który zdążył już usłyszeć odpowiedź walidatora.
 *
 * Pierwszy render buduje magazyn, `recheck` przynosi uwagi, drugi render pokazuje TEN SAM ekran
 * z nimi. Bez tej ręcznej tury uwagi nie dojechałyby nigdy: `useEffect`, który je zamawia, pod
 * `renderToStaticMarkup` nie biegnie. */
async function editorWithNotes(): Promise<string> {
  editor();
  const store = spy.made.get(DOC.id);
  if (store === undefined) throw new Error('the screen built no store to ask for notes');
  await store.getState().recheck();
  return editor();
}

/** Wszystko do zamykającego `</header>` — czyli dokładnie ten pasek, który człowiek czyta u góry.
 *
 * Asercja „Run jest gdzieś w markupie" przechodziła także wtedy, gdy stał w rogu płótna, więc
 * całe to kryterium sprowadzałoby się do niczego. */
function head(markup: string): string {
  const closes = markup.indexOf('</header>');
  return closes === -1 ? '' : markup.slice(0, closes);
}

/** Znaczniki OTWIERAJACE wszystkie przyciski o tej nazwie dostepnej. Biora atrybuty, nie tresc. */
function buttonsNamed(markup: string, label: string): string[] {
  const found: string[] = [];
  for (const hit of markup.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)) {
    const inside = (hit[2] ?? '').replace(/<[^>]*>/g, '').trim();
    if (inside === label) found.push(hit[1] ?? '');
  }
  return found;
}

/** Pierwszy z nich, albo `null`. */
function buttonNamed(markup: string, label: string): string | null {
  return buttonsNamed(markup, label)[0] ?? null;
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Wartość atrybutu `title`, albo `null`. Znak cytowania budowany z kodu, nie wpisany: parser
 * `checks/vocabulary.sh` paruje cudzysłowy po kolei i wpisany wprost rozjeżdża mu resztę pliku
 * (powód w całości stoi w `canvas/problems.test.tsx`). */
const MARK = String.fromCharCode(34);

function titleOf(attributes: string): string | null {
  const opens = ' title=' + MARK;
  const at = attributes.indexOf(opens);
  if (at < 0) return null;
  const rest = attributes.slice(at + opens.length);
  const closes = rest.indexOf(MARK);
  return closes < 0 ? null : rest.slice(0, closes);
}

describe('the editor puts Run where the main action of a screen goes', () => {
  it('stands in the screen header, filled, not in a corner of the canvas', async () => {
    const markup = await editorWithNotes();

    expect(
      buttonsNamed(markup, 'Run').length,
      'the screen carries a number of Run buttons other than one. Two of them are two ways to ' +
        'start the same work \u2014 the one in the header and the small overlay in the corner of ' +
        'the canvas it was meant to replace \u2014 and the two can disagree about whether a ' +
        'problem stops the start, so which one a person happens to press decides what happens. ' +
        'Zero is a screen nobody can start anything from.',
    ).toBe(1);

    const inTheHeader = buttonNamed(head(markup), 'Run');
    expect(
      inTheHeader,
      'Run is on the screen but not in its header. It sat in the top-right corner of the ' +
        'canvas, as a small overlay, while "Tidy up" — a tidying job used once in ten sittings ' +
        '— had a full-size button in the row underneath. That is the weight of this screen ' +
        'upside down: the one control the screen exists for was the smallest thing on it.',
    ).not.toBeNull();
    expect(
      inTheHeader ?? '',
      'the main action of the screen is not drawn as the main action. Every other screen in ' +
        'this product ends its header with a filled button (the list of workflows ends its own ' +
        'with Create), and a Run that looks like everything else around it is a Run nobody ' +
        'reaches for first.',
    ).toContain('btn-primary');
  });

  it('says how many things there are to fix once, and only once', async () => {
    const markup = await editorWithNotes();

    expect(
      occurrences(markup, 'things to fix'),
      'the count of things to fix is written on this screen more than once. It was: the badge ' +
        'in the header spelled it out in the JSX of the editor, and the bar above Run counted ' +
        'the same list again in another file. Two pieces of code answering one question drift ' +
        'apart at the first change of wording, and nobody finds out (invariant 13).',
    ).toBe(1);
    expect(
      head(markup),
      'and the one copy of it belongs in the header, beside Run, because it is the reason Run ' +
        'may be refusing to start.',
    ).toContain('2 things to fix');
  });

  it('makes that count the way to the sentences, not a label that only counts', async () => {
    const markup = await editorWithNotes();

    const badge = buttonNamed(head(markup), '2 things to fix');
    expect(
      badge,
      'the count is a caption, so a person reading "2 things to fix" has nowhere to press and ' +
        'no way to learn what the two things are. The sentences used to be spelled out over ' +
        'Run; with Run in the header they have to be one press from the count that names them.',
    ).not.toBeNull();
    expect(
      badge ?? '',
      'the count says nothing about whether the sentences under it are showing, so it reads ' +
        'the same open as it does shut.',
    ).toContain('aria-expanded');
  });

  it('goes dim in the header while a problem stands, and says which one', async () => {
    const markup = await editorWithNotes();
    const run = buttonNamed(head(markup), 'Run');

    expect(run, 'there is no Run in the header to ask about.').not.toBeNull();
    expect(
      /\bdisabled\b/.test(run ?? ''),
      'a problem means this workflow would not finish, so it does not start — and the button ' +
        'that has to say so is the one in the header now. Moving a control is exactly when its ' +
        'refusal gets left behind in the place it moved out of.',
    ).toBe(true);
    expect(
      titleOf(run ?? ''),
      'the reason is the note itself, straight from the checker. A person who cannot press Run ' +
        'and is told nothing has to go looking for the cause; anything written here by hand ' +
        'instead goes stale the day the checker changes its wording.',
    ).toBe(BLOCKER);
  });
});
