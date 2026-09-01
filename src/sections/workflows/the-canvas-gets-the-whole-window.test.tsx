/* Edytor przestaje trzymać otwartą kolumnę na panel, którego nie ma — i dalej nie rusza płótna.
 *
 * WADA, zgłoszona przez właściciela 2026-08-31, ze zrzutu z okna 1512 px. Ciało ekranu stało na
 * `grid-cols-[minmax(0,1fr)_330px]`, czyli kolumna panelu była STAŁA. Bez zaznaczonego kroku
 * rysowała jedno zdanie i 300 px pustki pod nim, a płótno dostawało 974 px z 1304 dostępnych.
 * Piąta część okna szła na powierzchnię, na której nic nie stało.
 *
 * DRUGA POŁOWA JEST RÓWNIE WIĄŻĄCA I TO ONA BRONIŁA TAMTEJ PUSTKI. Komentarz przy tej kolumnie
 * mówił prawdę: kolumna, która znika po kliknięciu w kafelek, przesuwa płótno POD KURSOREM
 * dokładnie w chwili, w której człowiek w nie celuje. Kryterium, które żąda tylko zniknięcia
 * pustki, zamawia tamtą wadę z powrotem — więc oba pytania stoją tu razem.
 *
 * DLACZEGO TA ODPOWIEDŹ. Panel leży NAD płótnem, a nie obok niego: `absolute` wewnątrz
 * `relative`, czyli poza układem. Pudełko płótna jest wtedy tą samą rzeczą przy zaznaczonym
 * kroku i bez niego — nie „prawie tą samą", tylko tym samym elementem o tych samych klasach —
 * więc nie ma czego przeliczać ani dopasowywać po zmianie szerokości. Dwie pozostałe drogi,
 * które rozważałem, są słabsze z mierzalnego powodu: zwijanie kolumny do paska ZMIENIA
 * szerokość (mniej, ale zmienia — a warunek brzmi „nie drgnie"), a `fitView` po zmianie
 * szerokości przesuwa TREŚĆ, żeby zrekompensować ruch ramki, czyli goni ruch zamiast go nie robić.
 *
 * CZEGO TO KRYTERIUM NIE UMIE i dlatego stoi obok niego `e2e/tests/canvas-keeps-its-width.spec.ts`:
 * w repo nie ma jsdom, więc `renderToStaticMarkup` nie zmierzy ani jednego piksela. Tutaj
 * dowodzimy MECHANIZMU (panel poza układem, ramka płótna niezmieniona co do bajta), tam —
 * SKUTKU, na prawdziwej szerokości w chromium. Żadna z tych połówek sama nie wystarcza:
 * mechanizm bez pomiaru to klasa „kryterium zielone, funkcja martwa" (niezmiennik 29).
 *
 * TRZECIE `it` jest o nagłówku i to ta sama rodzina wady: nazwa dokumentu — najważniejszy napis
 * na tym ekranie — była jedyną rzeczą w wierszu BEZ obrysu, obok wyjścia, które obrys miało.
 * Kryterium pyta o oba naraz, bo cicha nazwa jest faktem WZGLĘDNYM: nazwa z obrysem obok
 * wyjścia, które też go dostało, jest dalej najcichsza z trzech.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import type { WorkflowFile } from '../../state/workflows';
import { WorkflowEditor } from './editor';

/* Granica Tauriego: w vitest nie ma okna, a magazyn naprawdę zapisuje i naprawdę sprawdza. */
vi.mock('./io', () => ({
  write: () => Promise.resolve(),
  check: () => Promise.resolve([]),
}));

const PATH = 'ship-a-feature.json';

/** Zdanie, którym ekran odpowiada, kiedy NIC nie jest zaznaczone. Kontrakt tego kryterium —
 * wpisany ręcznie, nie zaimportowany z `editor.tsx`: zaimportowany zgadzałby się z ekranem
 * zawsze, także wtedy, gdyby ekran przestał je w ogóle pokazywać. */
const HINT = 'Pick a step to set up what it does.';

/** Brzmienie, które tu stało do 2026-08-31. Obiecywało PODGLĄD („see what it was given"),
 * a ta kolumna jest EDYTOREM: ustawia się w niej, co krok ma robić. */
const A_PROMISE_OF_A_LOOK = 'Pick a step to see what it was given.';

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

function editorWith(openStep?: string): string {
  return renderToStaticMarkup(
    <WorkflowEditor
      path={PATH}
      document={DOC}
      revision="r1"
      agents={[]}
      onClose={noop}
      onRun={noop}
      onCreateAgent={noop}
      {...(openStep === undefined ? {} : { openStep })}
    />,
  );
}

/** Znacznik OTWIERAJĄCY element, w którym stoi `needle` — atrybut albo tekst tuż za nim.
 *
 * Oddaje `null`, kiedy takiego elementu nie ma, i wołający MA to sprawdzić osobno: element
 * nieznaleziony i element bez szukanej klasy dają w teście identyczną pustą listę klas. */
function tagAround(markup: string, needle: string): string | null {
  const at = markup.indexOf(needle);
  if (at === -1) return null;
  const opens = markup.lastIndexOf('<', at);
  if (opens === -1) return null;
  const closes = markup.indexOf('>', opens);
  return closes === -1 ? null : markup.slice(opens, closes + 1);
}

/** Klasy jako LISTA, nie jako napis. `toContain('border-line')` na napisie przechodzi także
 * na `hover:border-line`, czyli na obrysie, którego przy spoczynku nie widać — a to jest
 * dokładnie ta wada, o którą pyta trzecie `it`. */
function classesOf(tag: string): string[] {
  const found = /class="([^"]*)"/.exec(tag);
  /* `?? ''` zamiast `!`: `noUncheckedIndexedAccess` w `checks/tsconfig.strict.json` traktuje
   * grupę wyrażenia jako możliwie nieobecną, choć ta akurat istnieje zawsze, gdy `exec` trafił.
   * Pusty napis daje tu pustą listę, czyli tę samą odpowiedź co brak dopasowania — asercja
   * nie zmienia się o jotę. */
  return found === null ? [] : (found[1] ?? '').split(/\s+/).filter((one) => one !== '');
}

describe('the editor gives the canvas the whole window and lays the step editor over it', () => {
  it('answers "nothing is picked" without holding a column open for it', () => {
    const markup = editorWith();

    expect(
      markup,
      'with nothing picked the screen still built the side column. That column is 330 px wide ' +
        'whatever is in it, so on the 1512 px window it spent a fifth of the room on one ' +
        'sentence and 300 px of nothing underneath it.',
    ).not.toContain('<aside');
    expect(
      markup,
      'the sentence for "nothing is picked" is gone as well. Losing the column is not a licence ' +
        'to lose the one line that says what picking a step is for.',
    ).toContain(HINT);
    expect(
      markup,
      'the screen still promises a look at what the step was given. That column is where a ' +
        'person sets up what the step DOES, so the old wording named the wrong half of it.',
    ).not.toContain(A_PROMISE_OF_A_LOOK);
  });

  it('lays the step editor over the canvas, so picking a step cannot resize it', () => {
    const picked = editorWith('s_build');

    const frame = tagAround(picked, 'data-step-editor');
    expect(
      frame,
      'picking a step opened no step editor at all, so there is nothing here to ask about.',
    ).not.toBeNull();
    expect(
      classesOf(frame ?? ''),
      'the step editor is still a second track of the layout rather than a surface above the ' +
        'canvas. Anything that takes width from the canvas when it opens moves the canvas out ' +
        'from under the pointer at the exact moment a person is aiming at it.',
    ).toContain('absolute');

    const holder = tagAround(picked, 'data-canvas-area');
    expect(
      holder,
      'the canvas has no box of its own to hold still, so nothing here can be compared.',
    ).not.toBeNull();
    expect(
      classesOf(tagAround(picked, 'data-step-editor') ?? '').includes('absolute') &&
        classesOf(tagAround(picked, 'data-canvas-body') ?? '').includes('relative'),
      'the surface above the canvas is placed against a box that is not a positioning context, ' +
        'so it would hang off the whole screen instead of the canvas.',
    ).toBe(true);

    /* STRAŻNIK, nie sterownik: ta równość jest prawdziwa także w starym układzie, bo tam ramka
     * płótna też nie zmieniała klas — zmieniała się TOR SIATKI pod nią. Stoi tu, żeby następna
     * osoba nie dopisała szerokości zależnej od `open`, co jest najkrótszą drogą z powrotem
     * do drgającego płótna. */
    expect(
      tagAround(editorWith(), 'data-canvas-area'),
      'the canvas box is described differently depending on whether a step is picked. Whatever ' +
        'the difference is, it is a relayout under the pointer at click time.',
    ).toBe(holder);
  });

  it('makes the workflow name the loudest thing in the header, not the quietest', () => {
    const markup = editorWith();

    const name = tagAround(markup, 'id="workflow-name"');
    expect(name, 'the header carries no field for the workflow name at all.').not.toBeNull();
    expect(
      classesOf(name ?? ''),
      'the name of the document draws no outline until the pointer finds it, so the most ' +
        'important text on this screen looks like a caption nobody can change. On disk that ' +
        'reads as folders full of "New workflow" and "New workflow 2".',
    ).toContain('border-line');

    const wayOut = tagAround(markup, 'All workflows');
    expect(wayOut, 'the header lost the way back to the list of workflows.').not.toBeNull();
    expect(
      classesOf(wayOut ?? ''),
      'the way back is still drawn with a full outline, which makes it louder than the name of ' +
        'the document standing next to it. A quiet name is a relative fact: giving the name an ' +
        'outline while the way out keeps its own leaves the name third of three.',
    ).toContain('btn-bare');
  });
});
