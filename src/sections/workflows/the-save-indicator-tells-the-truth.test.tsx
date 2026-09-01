/* Wskaźnik zapisu w nagłówku edytora ma TRZECI stan, i mówi go po odmowie dysku.
 *
 * WADA, zgłoszona przez właściciela 2026-08-31. Nagłówek liczył stan z jednego porównania:
 * `state.document === state.savedDocument ? 'saved' : 'saving…'`. Po ODMOWIE zapisu
 * `savedDocument` się nie zmienia — ustawia je wyłącznie gałąź sukcesu w `state/workflows.ts` —
 * więc nagłówek pokazywał „saving…" już na zawsze, choć nic się nie zapisywało i nie miało
 * zapisać. Czerwony pasek obok mówił prawdę, a nagłówek kłamał: dwa zdania o jednym fakcie,
 * sprzeczne (niezmiennik 13). Do tego pasek nieokreślony `.working` chodził w nieskończoność
 * pod czymś, co nie trwało.
 *
 * DLACZEGO TO KRYTERIUM CZYTA MARKUP, A NIE MAGAZYN. Wartość `couldNotSave` istniała w stanie
 * od 2026-08-18 i była renderowana; wadą było ZDANIE, które człowiek czyta w nagłówku
 * (niezmiennik 29). Asercja na magazynie przeszłaby nad nią bez mrugnięcia.
 *
 * JAK TO W OGÓLE DA SIĘ WYRENDEROWAĆ, skoro w repo nie ma jsdom. Magazyn dokumentu powstaje
 * WEWNĄTRZ edytora (`useState(() => createWorkflowStore(...))`), więc nie da się go podać
 * z zewnątrz ani z zewnątrz popchnąć. Atrapa `../../state/workflows` jest PRZEPUSZCZAJĄCA
 * i ZAPAMIĘTUJĄCA: woła prawdziwe `createWorkflowStore`, oddaje prawdziwy magazyn, zapisuje go
 * po drodze i przy drugim renderze oddaje TEN SAM. Dzięki temu drugi render jest tym samym
 * ekranem po odmowie, a nie nowym ekranem obok. Ten sam wzorzec, co atrapa `checkpoint-panel`
 * w `./every-tile-opens-a-panel.test.tsx`.
 *
 * Odmowa jedzie NAPISEM, bo tak odrzuca Tauri: skorupy komend robią `to_string()`, więc
 * `error instanceof Error` po tej stronie jest zawsze fałszywe.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { WorkflowFile } from '../../state/workflows';
import { WorkflowEditor } from './editor';

const spy = vi.hoisted(() => ({
  /** Magazyny oddane edytorowi — po jednym na otwarty plik, i to jest cała treść atrapy. */
  made: new Map<string, { getState: () => { commit: (next: never) => void } }>(),
}));

/* Granica Tauriego: `save_workflow` odmawia. To jest ta odmowa, o której mówi to kryterium. */
const REFUSED =
  'This workflow was not saved: it changed on disk after you opened it, so nothing was ' +
  'overwritten.';

vi.mock('./io', () => ({
  write: () => Promise.reject(REFUSED),
  check: () => Promise.resolve([]),
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

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Magazyn, który ekran naprawdę zbudował dla tego dokumentu. */
function storeOfTheOpenEditor() {
  const made = spy.made.get(DOC.id);
  if (made === undefined) {
    throw new Error('the editor never built a document store, so nothing below means anything');
  }
  return made;
}

describe('the save indicator in the editor header has a third state and uses it', () => {
  it('says saved while the screen really is the file, and does not run the bar', () => {
    const markup = editor();

    expect(
      visibleText(markup),
      'a freshly opened file IS the file on disk, so the header says so. Without this line the ' +
        'assertions below also pass on a header that says "not saved" always',
    ).toContain('1 step · saved');
    expect(
      occurrences(markup, 'class="working'),
      'and nothing is in flight, so the indeterminate bar has no business being on screen',
    ).toBe(0);
  });

  it('says the save did not go through, in the header, where "saved" stands', async () => {
    const first = editor();
    expect(
      first,
      'the store was not built at all, so the rest of this case is meaningless',
    ).toContain('workflow-name');

    /* JEDNA ZMIANA I JEDEN ZAPIS — dokładnie ta droga, którą chodzi autosave (`commit` →
     * `saveNow`). Odmowa wraca do wołającego i JEST ROZLICZONA: `saveNow` świadomie jej nie łyka. */
    const store = storeOfTheOpenEditor();
    store.getState().commit({ ...DOC, name: 'Ship it' } as never);
    await expect(
      (store.getState() as unknown as { saveNow: () => Promise<void> }).saveNow(),
      'the save was supposed to be turned down here; if it went through, the state this ' +
        'criterion is about never happens',
    ).rejects.toBeTruthy();

    const after = visibleText(editor());

    expect(
      after,
      'the header still says the save is under way, minutes after the disk turned it down. ' +
        'Nothing is being written and nothing will be: `savedDocument` only moves on success. ' +
        'The red bar says one thing and the header says the opposite, about the same file. ' +
        'The header read: ' +
        after.slice(0, 200),
    ).not.toContain('· saving…');
    expect(
      after,
      'and the third state has to be a SENTENCE where "saved" stands, not the absence of one: ' +
        'a header that just drops the word leaves a person with no answer at all',
    ).toContain('· not saved');
    expect(
      occurrences(editor(), 'class="working'),
      'the indeterminate bar keeps running under something that is not running. That bar is ' +
        'the answer to "is this taking a while", and here the answer is no — it is over, and ' +
        'it failed',
    ).toBe(0);
  });

  it('leaves the reason on screen next to the state, so the two are one story', async () => {
    editor();
    const store = storeOfTheOpenEditor();
    store.getState().commit({ ...DOC, name: 'Ship it again' } as never);
    await expect(
      (store.getState() as unknown as { saveNow: () => Promise<void> }).saveNow(),
    ).rejects.toBeTruthy();

    const markup = editor();

    expect(
      visibleText(markup),
      'the header names the STATE and the bar names the REASON — one is useless without the ' +
        'other. "not saved" with no reason sends a person looking; a reason with a header ' +
        'saying "saving…" tells them to wait for something that will never come.',
    ).toContain('changed on disk');
    expect(occurrences(markup, 'data-could-not-save'), 'and the reason stands exactly once').toBe(
      1,
    );
  });
});
