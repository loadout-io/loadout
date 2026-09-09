/* CT-02 w PRAWDZIWEJ przeglądarce: pięć wybranych plików daje pięć nazwanych wierszy, a jeden
 * zły plik nie unieważnia czterech dobrych.
 *
 * DLACZEGO TO KRYTERIUM STOI TUTAJ, A NIE W RUŚCIE. Po tamtej stronie granicy `import_*` już
 * dowodzi, że rejestr oddaje jeden wynik na każdą pozycję żądania (`context_source_import::`).
 * To jest dowód, że MECHANIZM istnieje. Kryterium 3 tego etapu mówi o czymś innym: że pięć
 * odpowiedzi dociera **na ekran**, z których dwie niosą powód po angielsku. Między jednym
 * a drugim mieszka klasa wady, dla której to repo powstało — kryterium zielone, funkcja martwa
 * (niezmiennik 29). Dlatego klika tu prawdziwy przycisk w prawdziwym Reakcie, a asercja stoi
 * na dokumencie.
 *
 * OKNO WYBORU PLIKU JEST WTYCZKĄ, nie komendą Loadouta, więc atrapa odpowiada na
 * `plugin:dialog|open` — to jest ta sama granica i ta sama taśma. Bez tej odpowiedzi przycisk
 * dostałby `null`, czyli „anulowano", i cały ten plik mierzyłby anulowanie.
 *
 * DRUGI PRZYPADEK SĄDZI MIESZANY PASTE. Wklejenie tekstu i obrazu naraz ma dojechać do granicy
 * jako JEDNA pozycja niosąca oba — implementacja, która bierze obraz i gubi podpis, przechodzi
 * każde kryterium mówiące „coś poleciało".
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriCall, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const TITLE = 'Checkout redesign';

/** Rewizja `draft.json`, którą okno przeczytało. Wraca w ładunku każdego żądania. */
const OPENED = 'rev-opened';
const AFTER_IMPORT = 'rev-imported';

/** Ten sam obraz, który czyta `context_source_import::`. Powód w `e2e/fixtures/context/README.md`. */
const SCREENSHOT = readFileSync(
  fileURLToPath(new URL('../fixtures/context/screenshot.png.b64', import.meta.url)),
  'utf8',
).trim();

/** Pięć ścieżek, które oddaje okno wyboru pliku. */
const PICKED = [
  '/Users/someone/Downloads/shot.png',
  '/Users/someone/Downloads/notes.md',
  '/Users/someone/Downloads/paper.pdf',
  '/Users/someone/Downloads/not-really.png',
  '/Users/someone/Downloads/enormous.png',
];

/* Zdania Rusta, słowo w słowo (`context::limits::Refusal`). Ekran nie ma prawa ich przepisać:
 * to jest jedyne miejsce, z którego człowiek dowie się, CZEGO ma poszukać w tych dwóch plikach. */
const WRONG_BYTES = 'What is inside this file is not what its name says it is, so it was left out.';
const TOO_MANY_PIXELS =
  'This image is 16008001 pixels, and Loadout reads images up to 16 million. Your file is ' +
  'untouched — add a smaller copy of it.';

const SET = {
  schema: 1,
  id: 'ct-checkout',
  title: TITLE,
  description: '',
  archived: false,
  draftRevision: 1,
  latestReadyRevision: null,
  createdAt: '2026-09-07T10:00:00Z',
  changedAt: '2026-09-07T10:00:00Z',
};

const EMPTY_DRAFT = {
  schema: 1,
  sources: [],
  excluded: [],
  howToPrepare: '',
  requirements: [],
};

/** Zestaw prosto po utworzeniu: jest nazwa, nie ma jeszcze ani jednego źródła. */
const MADE = { set: SET, draft: EMPTY_DRAFT, revision: OPENED };

function fileSource(id: string, name: string, kind: string): unknown {
  return {
    id,
    kind,
    name,
    description: '',
    text: '',
    file: {
      path: 'sources/' + id + '/r1/original.png',
      revision: 'r1',
      mime: 'image/png',
      bytes: 128,
      fingerprint: 'abc123',
      derived: 64,
      pages: null,
    },
    preparation: kind === 'pdf' ? { state: 'needs', pagesDone: 0 } : { state: 'notNeeded' },
    companionOf: null,
    notes: [],
  };
}

/** Trzy pliki, które weszły — i dwa wiersze odmowy obok nich. */
const REPORT = {
  operationId: 'op-e2e',
  results: [
    { name: 'shot.png', added: ['s-shot'], refused: null, notes: [] },
    { name: 'notes.md', added: ['s-notes'], refused: null, notes: [] },
    { name: 'paper.pdf', added: ['s-paper'], refused: null, notes: [] },
    { name: 'not-really.png', added: [], refused: WRONG_BYTES, notes: [] },
    { name: 'enormous.png', added: [], refused: TOO_MANY_PIXELS, notes: [] },
  ],
  read: {
    set: SET,
    draft: {
      ...EMPTY_DRAFT,
      sources: [
        fileSource('s-shot', 'shot.png', 'image'),
        fileSource('s-notes', 'notes.md', 'document'),
        fileSource('s-paper', 'paper.pdf', 'pdf'),
      ],
    },
    revision: AFTER_IMPORT,
  },
};

function copies<T>(value: T, count: number): readonly TauriReply[] {
  return Array.from({ length: count }, () => ({ value }) as TauriReply);
}

/** Miniatura, którą granica oddaje na `Preview`. */
const THUMBNAIL = { kind: 'image', image: { mime: 'image/png', base64: SCREENSHOT } };

/* Pierwsza odpowiedź katalogu jest PUSTA, bo biblioteka na tej scenie zaczyna bez zestawów.
 * Kolejne są już z zestawem i jest ich kilka, bo katalog odpowiada przy każdym wejściu. */
const SCENE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_context_sets: [{ value: [] }, ...copies([SET], 8)],
  create_context_set: copies(MADE, 4),
  read_context_set: copies(MADE, 4),
  'plugin:dialog|open': [{ value: PICKED }, ...copies(PICKED, 3)],
  import_context_sources: copies(REPORT, 4),
  read_context_source: copies(THUMBNAIL, 4),
};

/** Ten sam zestaw, tylko dokument stoi w połowie przygotowania: dwie strony z trzech. */
const HALF_PREPARED = {
  set: SET,
  draft: {
    ...EMPTY_DRAFT,
    sources: [
      {
        ...(fileSource('s-paper', 'paper.pdf', 'pdf') as Record<string, unknown>),
        file: {
          path: 'sources/s-paper/r1/original.pdf',
          revision: 'r1',
          mime: 'application/pdf',
          bytes: 4096,
          fingerprint: 'abc123',
          derived: 512,
          pages: 3,
        },
        preparation: { state: 'needs', pagesDone: 2 },
      },
    ],
  },
  revision: OPENED,
};

/** Scena powrotu do zestawu, w którym przygotowanie przerwano po drugiej stronie. */
const COMING_BACK: Readonly<Record<string, readonly TauriReply[]>> = {
  list_context_sets: [{ value: [] }, ...copies([SET], 8)],
  create_context_set: copies(HALF_PREPARED, 4),
  read_context_set: copies(HALF_PREPARED, 4),
};

/** Podpis, który człowiek wkleił razem ze screenshotem. */
const CAPTION = 'This is the total after the quantity changes.';

/** Prawdziwy trzystronicowy dokument. Ten JEST otwierany przez `pdf.js` w tej przeglądarce. */
const THREE_PAGES = readFileSync(
  fileURLToPath(new URL('../fixtures/context/three-pages.pdf', import.meta.url)),
).toString('base64');

/** Dokument w bibliotece, z podaną liczbą gotowych stron. */
function paperSource(pagesDone: number): unknown {
  return {
    id: 's-paper',
    kind: 'pdf',
    name: 'three-pages.pdf',
    description: '',
    text: '',
    file: {
      path: 'sources/s-paper/r1/original.pdf',
      revision: 'r1',
      mime: 'application/pdf',
      bytes: 1400,
      fingerprint: 'abc123',
      derived: 0,
      pages: pagesDone === 0 ? null : 3,
    },
    preparation: { state: 'needs', pagesDone },
    companionOf: null,
    notes: [],
  };
}

function paperSet(pagesDone: number): unknown {
  return {
    set: SET,
    draft: { ...EMPTY_DRAFT, sources: [paperSource(pagesDone)] },
    revision: OPENED,
  };
}

/**
 * Bajty dokumentu tak, jak oddaje je biblioteka lokalnemu workerowi — razem ze stemplami.
 *
 * To jest jedyna droga, którą `pdf.js` w ogóle dostaje plik: okno nie czyta dysku.
 */
const WHOLE_PAPER = {
  kind: 'whole',
  mime: 'application/pdf',
  base64: THREE_PAGES,
  operationId: 'op-pdf',
  fingerprint: 'abc123',
};

/** Zdanie Rusta o pliku, którego nie da się otworzyć (`context::limits::Unopenable::Damaged`). */
const DAMAGED =
  'This file could not be opened, so nothing was prepared from it. Save it again from the app ' +
  'that made it and add that copy.';

/** Zestaw po tym, jak biblioteka zapisała porażkę — stan TRWAŁY, nie zdanie na ekranie. */
const PAPER_FAILED = {
  set: SET,
  draft: {
    ...EMPTY_DRAFT,
    sources: [
      {
        ...(paperSource(0) as Record<string, unknown>),
        preparation: { state: 'failed', said: DAMAGED },
      },
    ],
  },
  revision: AFTER_IMPORT,
};

/** Scena, w której `Prepare` dostaje bajty, które dokumentem nie są. */
const UNOPENABLE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_context_sets: [{ value: [] }, ...copies([SET], 8)],
  create_context_set: copies(paperSet(0), 4),
  read_context_set: copies(paperSet(0), 4),
  read_context_source: copies(
    {
      kind: 'whole',
      mime: 'application/pdf',
      base64: Buffer.from('This is not a document at all.').toString('base64'),
      operationId: 'op-pdf',
      fingerprint: 'abc123',
    },
    4,
  ),
  complete_context_source_preparation: copies(PAPER_FAILED, 4),
};

/** Dokument, który OTWIERA się i pada dopiero przy drugiej stronie. */
const BREAKS_LATER = readFileSync(
  fileURLToPath(new URL('../fixtures/context/breaks-on-page-two.pdf', import.meta.url)),
).toString('base64');

/** Scena, w której dokument daje jedną stronę, a przy następnej się rozsypuje. */
const BREAKS_MIDWAY: Readonly<Record<string, readonly TauriReply[]>> = {
  list_context_sets: [{ value: [] }, ...copies([SET], 8)],
  create_context_set: copies(paperSet(0), 4),
  read_context_set: copies(paperSet(0), 4),
  read_context_source: copies(
    {
      kind: 'whole',
      mime: 'application/pdf',
      base64: BREAKS_LATER,
      operationId: 'op-pdf',
      fingerprint: 'abc123',
    },
    4,
  ),
  complete_context_source_preparation: copies(PAPER_FAILED, 6),
};

/** Scena, w której `Prepare` naprawdę mieli dokument: nic nie jest jeszcze gotowe. */
function preparing(pagesDone: number): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_context_sets: [{ value: [] }, ...copies([SET], 8)],
    create_context_set: copies(paperSet(pagesDone), 4),
    read_context_set: copies(paperSet(pagesDone), 4),
    read_context_source: copies(WHOLE_PAPER, 6),
    complete_context_source_preparation: copies(paperSet(pagesDone), 8),
  };
}

/**
 * Zestaw po mieszanym wklejeniu: DWA źródła, a obraz wie, przy którym tekście stanął.
 *
 * Tak odpowiada Rust (`context::sources`, `take_in`): jedna pozycja importu, dwa źródła i szew
 * `companionOf` między nimi.
 */
const AFTER_PASTE = {
  set: SET,
  draft: {
    ...EMPTY_DRAFT,
    sources: [
      {
        id: 's-caption',
        kind: 'text',
        name: 'Pasted material',
        description: '',
        text: CAPTION,
        file: null,
        preparation: { state: 'notNeeded' },
        companionOf: null,
        notes: [],
      },
      {
        ...(fileSource('s-picture', 'Pasted material', 'image') as Record<string, unknown>),
        companionOf: 's-caption',
      },
    ],
  },
  revision: AFTER_IMPORT,
};

/** Scena mieszanego wklejenia: import oddaje dwa źródła, a katalog oddaje je znowu po powrocie. */
const PASTING: Readonly<Record<string, readonly TauriReply[]>> = {
  list_context_sets: [{ value: [] }, ...copies([SET], 8)],
  create_context_set: copies(MADE, 4),
  read_context_set: copies(AFTER_PASTE, 4),
  import_context_sources: copies(
    {
      operationId: 'op-paste',
      results: [
        { name: 'Pasted material', added: ['s-caption', 's-picture'], refused: null, notes: [] },
      ],
      read: AFTER_PASTE,
    },
    4,
  ),
};

const SWITCH = '[data-section-switch="context"]';
const SCREEN = 'main[data-section="context"]';
const NAME = '#context-new';
const CREATE = 'main [data-create]';
const MATERIAL = '#context-material';
const ADD_FILES = 'main [data-add-files]';
const RESULT_ROW = 'main [data-import-result]';
const SOURCE_ROW = 'main [data-source-row]';
const PASTED_WITH = 'main [data-pasted-with]';
const BACK = 'main [data-back]';
const CARD = 'main [data-context-set]';

/** Ile czekamy na to, co ma przyjść po kliknięciu. Odpowiedź wraca w tej samej karcie. */
const APPEARS = 6_000;

/** Ile czekamy na przemielony dokument. Hojnie: `pdf.js` podnosi własny worker i rysuje strony. */
const PREPARES = 60_000;

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku. */
beforeAll(async () => {
  const warm = await openApp();
  /* ZMIERZONE 2026-09-07: pierwsze doczytanie sterownika dokumentów kazało vite przemielić
   * `pdfjs-dist` i trwało ponad 40 s; każde następne — pół sekundy. Policzone w pierwszym
   * przypadku dawało czerwień o produkcie, a mierzyło rozruch narzędzia (ta sama klasa, co
   * limit `page.goto` w `e2e/harness.ts`). Tutaj płaci to `beforeAll`, który ma na to budżet.
   *
   * `addScriptTag`, NIE `page.evaluate(() => import(...))`: vitest transformuje ten plik przed
   * uruchomieniem i przepisuje `import()` na `__vite_ssr_dynamic_import__`, którego w
   * przeglądarce nie ma. Zmierzone: taki rozgrzew wywalał się w karcie i — obsłużony po cichu —
   * wyglądał na wykonany. Znacznik z adresem jedzie do strony jako napis i tej podmiany nie ma. */
  /* 2026-09-08 — TEN ROZGRZEW WYWRACAŁ SIĘ O WŁASNY SKUTEK, i to jest cała treść tej pętli.
   * Ściągnięcie sterownika każe vite'owi zoptymalizować `pdfjs-dist`, a odkrycie nowej
   * zależności wymusza PRZEŁADOWANIE strony — czyli `Execution context was destroyed`
   * dokładnie w wywołaniu, które to przeładowanie wywołało. Pod małym obciążeniem reload
   * trafiał już po powrocie z `addScriptTag` i nikt tego nie widział; w pełnej suicie, gdzie
   * ośmiu workerów trzyma po chromium, trafiał w środku i przewracał CAŁY plik — 2100 testów
   * zielonych, jeden plik czerwony (zmierzone przy lądowaniu CT-02).
   *
   * Druga próba jest tania i wystarcza: po pierwszej zależność jest już zoptymalizowana, więc
   * drugiego przeładowania nie ma. Milczącego `catch` tu nie ma z rozmysłu — gdy obie próby
   * padną, plik ma się przewrócić z prawdziwym powodem, bo wtedy nie chodzi już o reload. */
  for (let attempt = 0; ; attempt += 1) {
    try {
      await warm.page.addScriptTag({
        type: 'module',
        url: '/src/sections/context/pdf-preparation.ts',
      });
      break;
    } catch (error) {
      const destroyed = String(error).includes('Execution context was destroyed');
      if (attempt >= 1 || !destroyed) throw error;
      await warm.page.waitForLoadState('domcontentloaded');
    }
  }
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

/**
 * Sądzi to, co po mieszanym wklejeniu widzi CZŁOWIEK: dwa wiersze i powiązanie między nimi.
 *
 * `where` wchodzi w komunikat, bo ta sama para asercji stoi dwa razy — raz zaraz po wklejeniu
 * i raz po powrocie do zestawu — a „nie ma powiązania" bez powiedzenia KTÓRY raz kazałoby
 * zgadywać, czy zgubiło się rysowanie, czy odczyt z dysku.
 */
async function twoRowsThatKnowEachOther(page: RunningApp['page'], where: string): Promise<void> {
  const rows = await page.locator(SOURCE_ROW).allInnerTexts();
  const said = rows.join(' | ').replace(/\s+/g, ' ');
  expect(
    rows.length,
    'one paste carrying a screenshot AND its caption has to leave two rows ' +
      where +
      ', and the set shows ' +
      String(rows.length) +
      '. Keeping one of the two is the half a person notices tomorrow, when a note explains a ' +
      'picture that is not there. The list said: ' +
      said,
  ).toBe(2);
  expect(
    await page.locator(PASTED_WITH).count(),
    'nothing on the screen says which text the pasted picture came in with ' +
      where +
      '. The link is saved in the file and shown nowhere, so two sources that were one paste ' +
      'no longer know it as far as a person can tell. The list said: ' +
      said,
  ).toBeGreaterThanOrEqual(1);
  expect(
    (await page.locator(PASTED_WITH).allInnerTexts()).join(' ').replace(/\s+/g, ' '),
    'the link names no text at all ' +
      where +
      ', so it points at a row a person cannot identify — both rows of one paste carry the same ' +
      'name. The list said: ' +
      said,
  ).toContain(CAPTION);
}

/** Pierwsze bajty każdego PNG, zapisane base64. Krótsze niż nagłówek, ale własne dla PNG. */
const PNG_HEADER = 'iVBORw0KGgo';

/** Jedna strona tak, jak przyjechała do granicy z lokalnego workera. */
interface PageOnTheWire {
  readonly number: number;
  readonly pagesTotal: number;
  readonly text: string;
  readonly image: { readonly base64: string } | null;
}

/**
 * Czeka, aż `pdf.js` przemieli dokument, i oddaje to, co doszło do granicy.
 *
 * Czekamy na LICZBĘ wywołań, nie na stały czas: rozruch workera i renderowanie trwają na
 * zimnej przeglądarce dłużej niż na ciepłej, a limit dobrany na oko dawałby czerwień od pogody
 * maszyny zamiast od kodu.
 */
async function preparationCalls(app: RunningApp, howMany: number): Promise<readonly TauriCall[]> {
  const deadline = Date.now() + PREPARES;
  const sent = async (): Promise<readonly TauriCall[]> =>
    (await app.calls()).filter((call) => call.cmd === 'complete_context_source_preparation');
  let calls = await sent();
  while (calls.length < howMany && Date.now() < deadline) {
    await app.page.waitForTimeout(50);
    calls = await sent();
  }
  expect(
    calls.length,
    'the local worker reached the boundary ' +
      String(calls.length) +
      ' times out of the ' +
      String(howMany) +
      ' this document takes. A Prepare that reaches no boundary leaves the file unfinished ' +
      'forever, and one that stops halfway leaves it worse than untouched.',
  ).toBe(howMany);
  return calls;
}

/** Same strony, dla przypadków, w których dokument przechodzi w całości. */
async function pagesPrepared(app: RunningApp, howMany: number): Promise<PageOnTheWire[]> {
  return (await preparationCalls(app, howMany)).map((call) => call.args['page'] as PageOnTheWire);
}

/** Wchodzi do sekcji i otwiera świeżo utworzony zestaw. Wspólny początek obu przypadków. */
async function openTheSet(replies: Readonly<Record<string, readonly TauriReply[]>>) {
  const app = await openApp({ replies });
  await app.page.locator(SWITCH).click();
  await app.page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
  await app.page.locator(NAME).fill(TITLE);
  await app.page.locator(CREATE).click();
  await app.page
    .locator(MATERIAL)
    .waitFor({ state: 'visible', timeout: APPEARS })
    .catch(() => undefined);
  return app;
}

describe('files picked from disk reach the library one named result at a time', () => {
  it('five files land as five named results and one refusal does not void the rest', async () => {
    const app = await openTheSet(SCENE);
    try {
      const page = app.page;

      expect(
        await page.locator(ADD_FILES).count(),
        'the editor has no way to add a file at all, so everything below would be about a ' +
          'control a person cannot reach.',
      ).toBe(1);
      await page.locator(ADD_FILES).click();

      await page
        .locator(RESULT_ROW)
        .first()
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);

      // 2026-09-09: sukces stoi raz, przy źródle; osobny wynik zostaje dla odmowy.
      const rows = await page.locator(`${RESULT_ROW}, ${SOURCE_ROW}`).allInnerTexts();
      expect(await page.locator(RESULT_ROW).count()).toBe(2);
      const said = rows.join(' | ').replace(/\s+/g, ' ');
      expect(
        rows.length,
        'five files were picked and the screen shows ' +
          String(rows.length) +
          ' of them. An import that reports fewer results than a person chose is the silent ' +
          'tail this whole feature exists to end. The screen said: ' +
          said,
      ).toBe(5);

      for (const name of ['shot.png', 'notes.md', 'paper.pdf', 'not-really.png', 'enormous.png']) {
        expect(
          said,
          'the results never name ' +
            name +
            ', so a person reading five lines cannot tell which file each one is about. The ' +
            'screen said: ' +
            said,
        ).toContain(name);
      }

      expect(
        said,
        'the file whose bytes do not match its name was turned down in silence. A person then ' +
          'believes it is in the set and finds out when an agent works without it. The screen ' +
          'said: ' +
          said,
      ).toContain('not what its name says it is');
      expect(
        said,
        'the oversized image was turned down without saying what the ceiling is, so a person ' +
          'cannot tell how much smaller a copy has to be. The screen said: ' +
          said,
      ).toContain('16 million');

      const kept = (await page.locator(SOURCE_ROW).allInnerTexts()).join(' | ');
      expect(
        await page.locator(SOURCE_ROW).count(),
        'three of the five were readable and the set shows ' +
          String(await page.locator(SOURCE_ROW).count()) +
          ' sources. One bad file may not take the good ones with it. The list said: ' +
          kept,
      ).toBe(3);

      /* ── i widać je w podglądzie ────────────────────────────────────────────────────────
         Wiersz na liście dowodzi, że plik wszedł. Kryterium 1 mówi o czymś dalszym: że da się
         na niego POPATRZEĆ. Między jednym a drugim mieszka źródło zapisane i nieoglądalne. */
      await page.locator('main [data-preview="s-shot"]').click();
      await page
        .locator('main [data-preview-image]')
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      const shown = await page
        .locator('main [data-preview-image]')
        .getAttribute('src')
        .catch(() => null);
      expect(
        (shown ?? '').startsWith('data:image/png;base64,'),
        'the preview drew no picture, so a file a person added is in the set and cannot be ' +
          'looked at. It drew: ' +
          String(shown).slice(0, 60),
      ).toBe(true);
    } finally {
      await app.close();
    }
  }, 90_000);

  it('a paste of a screenshot and its caption leaves two rows that still know each other', async () => {
    const app = await openTheSet(PASTING);
    try {
      const page = app.page;
      const caption = CAPTION;

      await page.locator(MATERIAL).evaluate(
        (element, image) => {
          const bytes = Uint8Array.from(atob(image.base64), (character) => character.charCodeAt(0));
          const transfer = new DataTransfer();
          transfer.items.add(new File([bytes], 'screenshot.png', { type: 'image/png' }));
          transfer.setData('text/plain', image.caption);
          element.dispatchEvent(
            new ClipboardEvent('paste', {
              bubbles: true,
              cancelable: true,
              clipboardData: transfer,
            }),
          );
        },
        { base64: SCREENSHOT, caption },
      );

      const deadline = Date.now() + APPEARS;
      let sent = (await app.calls()).filter((call) => call.cmd === 'import_context_sources');
      while (sent.length === 0 && Date.now() < deadline) {
        await page.waitForTimeout(25);
        sent = (await app.calls()).filter((call) => call.cmd === 'import_context_sources');
      }
      expect(
        sent.length,
        'pasting a screenshot into the material field reached no command at all, so the picture ' +
          'never left the window.',
      ).toBeGreaterThanOrEqual(1);

      const payload = JSON.stringify(sent[0]?.args ?? {});
      expect(
        payload,
        'the paste carried the picture and dropped the caption written with it. Two halves of ' +
          'one paste, one of which survives, leaves a note explaining a picture that is not ' +
          'there. The whole payload was: ' +
          payload,
      ).toContain(caption);
      expect(
        payload,
        'the paste carried no image bytes, so the screenshot a person pasted never left the ' +
          'window. The whole payload was: ' +
          payload,
      ).toContain(SCREENSHOT.slice(0, 24));

      /* ── DWA WIERSZE I WIDOCZNE POWIĄZANIE ────────────────────────────────────────────────
         Ładunek dowodzi wyłącznie tego, że oba kawałki przeszły przez granicę. Kryterium 1
         mówi o czymś, co widzi CZŁOWIEK: że po wklejeniu stoją dwa wiersze i że przy obrazie
         widać, do którego tekstu należy. Powiązanie zapisane w pliku i niepokazane nigdzie
         jest dokładnie tą wadą, dla której to repo powstało (niezmiennik 29). */
      await twoRowsThatKnowEachOther(page, 'right after the paste');

      /* ── i po wyjściu z zestawu ─────────────────────────────────────────────────────────
         Powrót idzie przez listę i przez `read_context_set`, więc wiersze wracają Z DYSKU,
         a nie z pamięci okna — powiązanie trzymane wyłącznie w zustandzie zniknęłoby tutaj. */
      await page.locator(BACK).click();
      await page.locator(CARD).first().click();
      await page
        .locator(SOURCE_ROW)
        .first()
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      await twoRowsThatKnowEachOther(page, 'after leaving the set and opening it again');
    } finally {
      await app.close();
    }
  }, 90_000);

  it('a file left half prepared says how far it got and carries on from there', async () => {
    const app = await openTheSet(COMING_BACK);
    try {
      const page = app.page;

      await page
        .locator(SOURCE_ROW)
        .first()
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      const said = (await page.locator(SOURCE_ROW).allInnerTexts()).join(' ').replace(/\s+/g, ' ');
      expect(
        said,
        'coming back to a file whose preparation was interrupted says nothing about it being ' +
          'unfinished, so a person has no reason to finish it and no idea it is not ready. The ' +
          'row said: ' +
          said,
      ).toContain('Needs preparation');
      expect(
        said,
        'the row does not say how far the preparation got, so carrying on reads exactly like ' +
          'starting again. The row said: ' +
          said,
      ).toContain('2 of 3 pages');

      /* Przycisk musi DOJŚĆ do granicy po plik. Co zrobi z nim lokalny worker, sądzi po tamtej
         stronie `context_source_import::` — tu chodzi o to, czy kontrolka żyje (niezmiennik 16). */
      await page.locator('main [data-prepare="s-paper"]').click();
      const deadline = Date.now() + APPEARS;
      let asked = (await app.calls()).filter((call) => call.cmd === 'read_context_source');
      while (asked.length === 0 && Date.now() < deadline) {
        await page.waitForTimeout(25);
        asked = (await app.calls()).filter((call) => call.cmd === 'read_context_source');
      }
      expect(
        asked.length,
        'Prepare reached no command at all, so the button that finishes an interrupted file is ' +
          'dead and the file stays unfinished forever.',
      ).toBeGreaterThanOrEqual(1);
      expect(
        JSON.stringify(asked[0]?.args ?? {}),
        'Prepare asked for something other than the saved source, so it is not the file in the ' +
          'library that gets prepared. It asked with: ' +
          JSON.stringify(asked[0]?.args ?? {}),
      ).toContain('s-paper');
    } finally {
      await app.close();
    }
  }, 90_000);

  it('Prepare reads a real document and keeps the words and the look of every page', async () => {
    const app = await openTheSet(preparing(0));
    try {
      const page = app.page;
      await page.locator('main [data-prepare="s-paper"]').click();
      const pages = await pagesPrepared(app, 3);

      expect(
        pages.map((one) => one.number),
        'a three page document did not come back as pages 1, 2 and 3. What reached the boundary ' +
          'was: ' +
          JSON.stringify(pages.map((one) => one.number)),
      ).toEqual([1, 2, 3]);
      expect(
        pages[0]?.pagesTotal,
        'nobody counted the pages, so the file never learns how long it is and the progress on ' +
          'screen has nothing to count towards.',
      ).toBe(3);

      /* ── strona tekstowa ──────────────────────────────────────────────────────────────── */
      expect(
        pages[0]?.text,
        'the words on the first page were not read out of the document. Text extraction that ' +
          'returns nothing is indistinguishable from a scan, and the whole point of preparing a ' +
          'text file is that its words become searchable. It read: ' +
          JSON.stringify(pages[0]?.text),
      ).toContain('Page one, in words.');

      /* ── skan ─────────────────────────────────────────────────────────────────────────── */
      expect(
        pages[1]?.text,
        'a page with no text on it came back with text anyway, so page numbers and page contents ' +
          'do not line up. It read: ' +
          JSON.stringify(pages[1]?.text),
      ).toBe('');

      /* ── strona mieszana: TEKST I WYGLĄD ──────────────────────────────────────────────── */
      expect(
        pages[2]?.text,
        'the mixed page lost its words. It read: ' + JSON.stringify(pages[2]?.text),
      ).toContain('Page three, with a diagram.');

      for (const one of pages) {
        expect(
          (one.image?.base64 ?? '').startsWith(PNG_HEADER),
          'page ' +
            String(one.number) +
            ' came back without a picture of itself, so a diagram between the paragraphs is gone ' +
            'the moment the text is extracted (PLAN §5). It carried: ' +
            String(one.image?.base64).slice(0, 24),
        ).toBe(true);
      }
      expect(
        pages[0]?.image?.base64 === pages[1]?.image?.base64,
        'two different pages rendered to byte-identical pictures, so nothing was actually drawn ' +
          'and every page carries the same blank sheet.',
      ).toBe(false);
    } finally {
      await app.close();
    }
  }, 120_000);

  it('a file that will not open is written down as such, not just complained about', async () => {
    const app = await openTheSet(UNOPENABLE);
    try {
      const page = app.page;
      await page.locator('main [data-prepare="s-paper"]').click();

      const told = await preparationCalls(app, 1);
      expect(
        JSON.stringify(told[0]?.args ?? {}),
        'nothing in what reached the library says the file could not be opened, so there is no ' +
          'way for it to write that down. The refusal then lives on the screen until a person ' +
          'looks somewhere else, and the file comes back tomorrow as "needs preparation" — so ' +
          'they press Prepare again, forever. It carried: ' +
          JSON.stringify(told[0]?.args ?? {}),
      ).toContain('damaged');

      /* I to, co biblioteka odesłała, ma stanąć przy wierszu: zdanie pisze Rust, ekran je
         wyłącznie pokazuje (D5, niezmiennik 14). */
      await page
        .locator(SOURCE_ROW)
        .first()
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      const said = (await page.locator(SOURCE_ROW).allInnerTexts()).join(' ').replace(/\s+/g, ' ');
      expect(
        said,
        'the row says nothing about the file being unreadable, so a person sees a file waiting ' +
          'to be prepared and no reason it never is. The row said: ' +
          said,
      ).toContain('could not be opened');
    } finally {
      await app.close();
    }
  }, 120_000);

  it('a document that opens and then breaks is written down, not just complained about', async () => {
    const app = await openTheSet(BREAKS_MIDWAY);
    try {
      const page = app.page;
      await page.locator('main [data-prepare="s-paper"]').click();

      /* Ten plik ODDAJE pierwszą stronę i dopiero potem się rozsypuje, więc czekamy na dwa
         wywołania: gotową stronę i porażkę. Jedno wywołanie znaczyłoby, że albo nic się nie
         przygotowało, albo porażka nigdzie nie dotarła. */
      const told = await preparationCalls(app, 2);
      const pages = told.map((call) => call.args['page'] as { failed?: string; number?: number });
      expect(
        pages[0]?.number,
        'the page this document really has was not prepared before it fell apart, so work that ' +
          'was possible was thrown away with the part that was not. It sent: ' +
          JSON.stringify(pages[0]),
      ).toBe(1);
      expect(
        pages.at(-1)?.failed,
        'the document opened, fell apart on the next page, and the library was told nothing ' +
          'about it. The refusal then lives on the screen until a person looks away, and the ' +
          'file comes back tomorrow as "needs preparation" with one page of three. It sent: ' +
          JSON.stringify(pages.at(-1)),
      ).toBe('damaged');

      await page
        .locator(SOURCE_ROW)
        .first()
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      const said = (await page.locator(SOURCE_ROW).allInnerTexts()).join(' ').replace(/\s+/g, ' ');
      expect(
        said,
        'the row still invites a person to prepare a file that cannot be prepared. The row ' +
          'said: ' +
          said,
      ).toContain('could not be opened');
    } finally {
      await app.close();
    }
  }, 120_000);

  it('Prepare on a half finished document starts at the missing page and repeats none', async () => {
    const app = await openTheSet(preparing(1));
    try {
      const page = app.page;
      await page.locator('main [data-prepare="s-paper"]').click();
      const pages = await pagesPrepared(app, 2);

      expect(
        pages.map((one) => one.number),
        'carrying on re-did work that was already on disk. Pages 2 and 3 were missing and what ' +
          'came back was: ' +
          JSON.stringify(pages.map((one) => one.number)),
      ).toEqual([2, 3]);
      expect(
        pages.some((one) => one.number === 1),
        'page one was prepared again although it was already finished before the window closed. ' +
          'Redoing finished pages is what a person calls hanging.',
      ).toBe(false);
    } finally {
      await app.close();
    }
  }, 120_000);
});
