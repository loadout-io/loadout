/* Zdanie, ktore tlumaczy „Learn from this run", ma SZEROKOSC — mierzona w chromium, nie
 * obecnosc w markupie.
 *
 * PO CO TO ISTNIEJE, ZMIERZONE 2026-09-01. `src/sections/run/reflection/reflection-explains-
 * itself.test.tsx` bylo ZIELONE nad zdaniem, ktore na ekranie mialo ZERO PIKSELI szerokosci.
 * Tamto kryterium pyta o wezel tekstowy w markupie i pyta slusznie — repo nie ma jsdom, wiec
 * nie ma tam czym zapytac o widocznosc. Ale `renderToStaticMarkup` nie ma szerokosci, wiec
 * `truncate` na napisie, ktory dostal 0 px, jest dla niego nieodrozniali od napisu czytelnego.
 * Rzad kontrolek paska chcial 1562 px, dostawal 1108, a caly niedobor pokrywaly dwa napisy tej
 * jednej kontrolki: to zdanie (0 z 400 px) i reszta jej wlasnej nazwy (57 ze 112).
 *
 * Zdanie o zerowej szerokosci jest gorsze niz jego brak: z markupu produkt wyglada na taki,
 * ktory tlumaczy, co robi, a czlowiek nie widzi ani litery. To jest ta sama roznica, co miedzy
 * „co agent powiedzial" a „co sie stalo" — i to jest jedyne pytanie, ktore ten plik zadaje.
 *
 * CZEGO TO NIE DOWODZI. Ze wybor JEDZIE do Rusta — na to odpowiada
 * `./t126-reflection-choice-real-routes.spec.ts`, tym samym przyrzadem i na tej samej granicy.
 * Tutaj nie ma ani jednej asercji o `run_workflow`.
 */
import { afterAll, describe, expect, it } from 'vitest';

import { REFLECTION_EXPLAINED, REFLECTION_LABEL } from '../../src/sections/run/reflection/toggle';
import type { RunningApp, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

/** Okno z makiety, i drugie — najwezsze, ktore ten produkt obsluguje (`./t161-…`). */
const SIZES = [
  { width: 1512, height: 950 },
  { width: 1100, height: 700 },
] as const;

const FOLDER = '/Users/somebody/Projects/client-website-redesign';
const WORKSPACE = { id: FOLDER, name: 'Client website redesign', folder: FOLDER };

const LEAD = {
  schema: 1,
  id: 'agent-builder',
  name: 'Builder',
  summary: 'Writes the change',
  color: 'green',
  instructions: 'Make the smallest change that makes the failing behaviour pass.',
  runsWith: 'claude',
  model: 'sonnet',
  thinking: 'balanced',
  fileAccess: 'edit-in-folder',
  giveUpAfterMinutes: 45,
  tools: 'all',
  reachesTheWeb: false,
  skills: [],
  connections: [],
  writeResultsTo: 'handoffs/patch-summary.md',
};

const WORKFLOW = {
  path: 'ship-a-feature.json',
  place: 'project',
  workflow: {
    format: 1,
    id: 'wf-ship',
    name: 'Ship a feature',
    steps: [
      {
        kind: 'agent',
        id: 's_build',
        name: 'Build',
        agent: LEAD.id,
        overrides: {},
        copies: 1,
        instructions: 'Build the requested change.',
        skills: 'all',
        folder: { use: 'project' },
        handover: 'notes',
        at: { x: 24, y: 24 },
      },
    ],
    links: [],
  },
};

function copies<T>(value: T, count = 20): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

const SCENE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_workspaces: copies([WORKSPACE]),
  list_agents: copies([LEAD]),
  list_workflows: copies([WORKFLOW]),
  list_skills: copies([]),
  list_notes: copies([]),
  list_runs: copies([]),
  load_workflow: copies({ workflow: WORKFLOW.workflow, revision: 'r1' }, 6),
  check_workflow: copies([], 8),
  read_settings: copies({ defaultLead: LEAD.id, defaultBudgetUsd: 75 }),
};

const CHECKBOX = `label:has-text("${REFLECTION_LABEL}") input[type="checkbox"]`;
const APPEARS = 8_000;

/** Co chromium naprawde narysowalo w miejscu opisu tej kontrolki. */
interface Painted {
  readonly found: boolean;
  readonly describedBy: string;
  readonly said: string;
  readonly width: number;
  readonly height: number;
  readonly wants: number;
  readonly given: number;
  readonly left: number;
  readonly right: number;
  readonly hidden: boolean;
  readonly insideTheStripRow: boolean;
  readonly viewport: number;
}

/**
 * Opis ptaszka wziety TAK, JAK BIERZE GO CZYTNIK EKRANU — przez `aria-describedby`, a nie po
 * napisie.
 *
 * Rozroznienie jest zmierzone, nie teoretyczne: wyszukanie po tresci przechodzi dowolny akapit
 * z tym samym zdaniem stojacy gdziekolwiek na ekranie, takze taki, ktory z ta kontrolka nie ma
 * nic wspolnego. Pytanie brzmi „czy TEN ptaszek ma widoczne wyjasnienie", wiec droga do zdania
 * musi byc ta sama, ktora ma czlowiek czytajacy ekranem.
 */
async function paintedExplanation(app: RunningApp): Promise<Painted> {
  /* PRZEZ `locator.evaluate`, NIE `page.evaluate` z napisem: `:has-text()` jest silnikiem
   * Playwrighta, a nie selektorem CSS — `document.querySelector` odrzuca go skladniowo. Element
   * rozwiazuje wiec ta sama warstwa, ktora klika w niego nizej. */
  return app.page.locator(CHECKBOX).evaluate((element) => {
    const box = element as HTMLInputElement;
    const id = box.getAttribute('aria-describedby') ?? '';
    const said = id === '' ? null : document.getElementById(id);
    if (said === null) {
      return {
        found: false,
        describedBy: id,
        said: '',
        width: 0,
        height: 0,
        wants: 0,
        given: 0,
        left: 0,
        right: 0,
        hidden: true,
        insideTheStripRow: false,
        viewport: window.innerWidth,
      };
    }
    const rect = said.getBoundingClientRect();
    const style = getComputedStyle(said);
    return {
      found: true,
      describedBy: id,
      said: (said.textContent ?? '').trim(),
      width: rect.width,
      height: rect.height,
      wants: said.scrollWidth,
      given: said.clientWidth,
      left: rect.left,
      right: rect.right,
      hidden: style.visibility === 'hidden' || style.display === 'none' || style.opacity === '0',
      insideTheStripRow: said.closest('[data-workflow-controls]') !== null,
      viewport: window.innerWidth,
    };
  });
}

afterAll(async () => {
  await closeEverything();
});

describe('the one control on this screen that spends money after the run says so out loud', () => {
  for (const size of SIZES) {
    const where = `${String(size.width)}x${String(size.height)}`;

    it(`paints the whole explanation at ${where}`, async () => {
      const app = await openApp({ replies: SCENE });
      try {
        await app.page.setViewportSize(size);
        await app.page.locator(CHECKBOX).waitFor({ state: 'visible', timeout: APPEARS });
        await app.page.waitForTimeout(200);

        const painted = await paintedExplanation(app);

        expect(
          painted.found,
          `${where}: the Learn from this run checkbox points at no description at all ` +
            `(aria-describedby=${JSON.stringify(painted.describedBy)}). A person reading this ` +
            'screen has no way to find out what Loadout does with the run, and a person ' +
            'reading it aloud has nothing to read',
        ).toBe(true);

        expect(
          painted.said,
          `${where}: the description beside the checkbox is not the sentence this product ` +
            'means to say',
        ).toBe(REFLECTION_EXPLAINED);

        expect(
          painted.hidden,
          `${where}: the sentence is in the markup and painted by nobody`,
        ).toBe(false);

        /* PIERWSZE Z DWOCH PYTAN O SZEROKOSC: czy to w ogole zajmuje miejsce. Napis ucisniety
         * do zera stoi w markupie tak samo, jak napis czytelny — i wlasnie tak przeszedl
         * przez trzy dni zielonej bramki. */
        expect(
          painted.width,
          `${where}: the sentence explaining ${JSON.stringify(REFLECTION_LABEL)} is ` +
            `${String(Math.round(painted.width))} pixels wide and ` +
            `${String(Math.round(painted.height))} pixels tall. It is in the markup and a ` +
            'person sees none of it. A screen that pretends to explain itself in a string of ' +
            'zero width is worse than one that says nothing: from the outside it reads as a ' +
            'product that tells you what it is about to do with your money',
        ).toBeGreaterThan(0);

        /* DRUGIE: czy pudelko nie chowa wlasnej tresci. `truncate` daje szerokosc wieksza od
         * zera i trzy kropki zamiast konca zdania — a koncem tego zdania jest jedyne miejsce,
         * w ktorym pada slowo „Knowledge", czyli odpowiedz na pytanie, gdzie te notatki
         * wyladuja. */
        expect(
          painted.wants,
          `${where}: the sentence is cut by its own box — it wants ` +
            `${String(painted.wants)} pixels and was given ${String(painted.given)}. The tail ` +
            'it loses is where the sentence says the notes wait for you to approve them',
        ).toBeLessThanOrEqual(painted.given + 1);

        expect(
          painted.left,
          `${where}: the sentence starts left of the window at ` + String(Math.round(painted.left)),
        ).toBeGreaterThanOrEqual(-1);
        expect(
          painted.right,
          `${where}: the sentence runs past the right edge of the window (${String(
            Math.round(painted.right),
          )} of ${String(painted.viewport)})`,
        ).toBeLessThanOrEqual(painted.viewport + 1);

        /* GDZIE TEGO ZDANIA NIE MA PRAWA BYC, i to jest asercja o wadzie, nie o guscie. Rzad
         * kontrolek paska jest jednym wierszem wysokim na 52 px, ktory przy oknie z makiety
         * chce o 454 px wiecej, niz dostaje. Kazdy napis, ktory tam wroci, wroci ucisniety —
         * a nastepny czlowiek zobaczy dokladnie to, co ten plik wlasnie zmierzyl. */
        expect(
          painted.insideTheStripRow,
          `${where}: the sentence is back inside the row of controls in the loadout strip. ` +
            'That row is one line 52 pixels tall and it is already short of room at the ' +
            'window this product is drawn for, so anything long standing in it is squeezed ' +
            'to nothing without a word to anybody',
        ).toBe(false);
      } finally {
        await app.close();
      }
    }, 120_000);
  }

  it('lets a person reach and turn the choice the sentence explains', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      await app.page.setViewportSize(SIZES[0]);
      const box = app.page.locator(CHECKBOX);
      await box.waitFor({ state: 'visible', timeout: APPEARS });

      expect(await box.count(), 'the Run screen carries no reflection choice at all').toBe(1);
      expect(await box.isEnabled(), 'the choice is dead before the run even starts').toBe(true);
      expect(await box.isChecked(), 'a fresh Run screen must opt in by default').toBe(true);

      /* BEZ `scrollIntoViewIfNeeded`, i to jest asercja, nie oszczednosc jednej linii:
       * kontrolka ustawiana PRZED startem, do ktorej trzeba sie doprzewijac przez cala liste
       * krokow, jest kontrolka, ktorej czlowiek nie znajdzie (niezmiennik 16). */
      await box.click({ trial: true });
      await box.click();
      expect(await box.isChecked(), 'the click reached nothing').toBe(false);
    } finally {
      await app.close();
    }
  }, 120_000);
});
