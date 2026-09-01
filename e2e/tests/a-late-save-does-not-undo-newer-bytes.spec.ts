/* Spóźniony zapis nie ma prawa cofnąć nowszych bajtów — mierzone na PRAWDZIWYM oknie.
 *
 * PO CO TO ISTNIEJE. Do 2026-08-28 `save_workflow` publikował cały plik albo nic (to jest
 * dobre) i robił to BEZWARUNKOWO: ani jednej kontroli tego, co leży na dysku. Serializacja
 * zapisów z T-151 pilnuje wyłącznie kolejności zapisów TEGO okna i nie widzi zapisu, który
 * wystartował z nieaktualną rewizją — a taki zapis kasuje cudzą, nowszą pracę bez jednego
 * zdania na ekranie.
 *
 * Zamknięcie tej dziury ma DWIE połowy i obie muszą być prawdziwe naraz:
 *   okno niesie rewizję, na której czyta — inaczej Rust nie ma czego porównać;
 *   odmowa dochodzi na ekran — inaczej człowiek dalej nie wie, że jego zmiana nie weszła.
 * Ten plik sądzi obie w jednym przebiegu, bo zielona pierwsza przy martwej drugiej jest
 * dokładnie tą klasą wady, dla której to repo powstało (niezmiennik 29).
 *
 * Ani jedna funkcja magazynu nie jest tu wołana wprost: wejściem jest przełącznik sekcji,
 * kafelek z listy i litera wpisana w pole nazwy. Granica odpowiada odmową Rusta, tą samą,
 * którą produkuje `workflow::file::SaveError::Changed`.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const PATH = 'late-save.json';

/** Rewizja, którą okno przeczytało przy otwarciu. Wraca w ładunku każdego zapisu. */
const OPENED = 'r1';

/** Zdanie Rusta, słowo w słowo. Ekran nie ma prawa go przepisać ani zastąpić własnym. */
const REFUSED =
  'This workflow was not saved: it changed on disk after you opened it, so nothing was ' +
  'overwritten. Close it and open it again to see the newer one.';

const WORKFLOW = {
  format: 1 as const,
  id: 'wf-late-save',
  name: 'Late save',
  steps: [
    {
      kind: 'agent' as const,
      id: 's_build',
      name: 'Build',
      agent: '',
      overrides: {},
      copies: 1,
      instructions: 'Write the smallest change that works.',
      skills: 'all' as const,
      folder: { use: 'project' as const },
      handover: 'notes' as const,
      at: { x: 24, y: 24 },
    },
  ],
  links: [],
};

const ENTRY = { path: PATH, workflow: WORKFLOW };

function copies<T>(value: T, count: number): readonly TauriReply[] {
  return Array.from({ length: count }, () => ({ value }) as TauriReply);
}

/* Kolejki są zużywalne, a katalog i walidator odpowiadają po kilka razy — przy wejściu na
 * sekcję, po otwarciu pliku i po każdym zapisie. Odmowa stoi w kolejce kilka razy, bo
 * autosave ma prawo spróbować ponownie i za każdym razem ma usłyszeć to samo. */
const SCENE: Readonly<Record<string, readonly TauriReply[]>> = {
  list_workflows: copies([ENTRY], 12),
  load_workflow: copies({ workflow: WORKFLOW, revision: OPENED }, 4),
  check_workflow: copies([], 12),
  save_workflow: Array.from({ length: 6 }, () => ({ error: REFUSED })),
};

const SWITCH = '[data-section-switch="workflows"]';
const SCREEN = 'main[data-section="workflows"]';
const LIST_TILE = 'main [data-tile]';
const NAME = 'input[aria-label="Workflow name"]';
const SAID = '[data-could-not-save]';

/** Ile czekamy na to, co ma przyjść po wpisaniu litery. Odpowiedź wraca w tej samej karcie. */
const APPEARS = 6_000;

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku. */
beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('a late save does not undo newer bytes', () => {
  it('says the workflow was not saved because the file changed, and that nothing was overwritten', async () => {
    const app = await openApp({ replies: SCENE });
    try {
      const page = app.page;

      await page.locator(SWITCH).click();
      await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await page.locator(LIST_TILE).first().click();
      await page
        .locator(NAME)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);

      /* ── połowa zerowa: otwarcie w ogóle niesie rewizję ────────────────────────────────── */
      expect(
        await page.locator(NAME).count(),
        'the file came back together with the revision it was read at, and the editor could not ' +
          'open it: the window still takes the bare file, so a revision has nowhere to live and ' +
          'no save can carry one.',
      ).toBe(1);

      /* Jedna litera w nazwie — dokładnie ta czynność, po której autosave sięga na dysk. */
      await page.locator(NAME).fill('Late save!');

      const deadline = Date.now() + APPEARS;
      let saves = (await app.calls()).filter((call) => call.cmd === 'save_workflow');
      while (saves.length === 0 && Date.now() < deadline) {
        await page.waitForTimeout(25);
        saves = (await app.calls()).filter((call) => call.cmd === 'save_workflow');
      }

      expect(
        saves.length,
        'typing in the name reached no save at all, so nothing below would say anything about ' +
          'what the window sent or what it showed.',
      ).toBeGreaterThanOrEqual(1);

      /* ── połowa pierwsza: okno niesie rewizję, na której czyta ─────────────────────────── */
      expect(
        saves[0]?.args['expectedRevision'],
        'the save carried no revision of the file it was opened from, so Rust has nothing to ' +
          'compare and the older window still wins by writing last. The whole payload was: ' +
          JSON.stringify(saves[0]?.args ?? {}),
      ).toBe(OPENED);

      /* ── połowa druga: odmowa dochodzi tam, gdzie patrzy człowiek ──────────────────────── */
      await page
        .locator(SAID)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      const shown = (await page.locator(SAID).allInnerTexts()).join(' ').replace(/\s+/g, ' ');
      expect(
        shown,
        'the refusal never reached the screen. A save that was turned down in silence leaves a ' +
          'person believing the file holds what the canvas shows — and it holds somebody ' +
          "else's newer work instead. The screen said: " +
          shown,
      ).toContain('was not saved');
      expect(
        shown,
        'the screen does not say the file changed underneath, so the one thing a person can do ' +
          'about it — open it again — is not on offer. The screen said: ' +
          shown,
      ).toContain('changed on disk');
      expect(
        shown,
        'the screen never says that nothing was overwritten, so a person cannot tell a refused ' +
          'save from a save that took half. The screen said: ' +
          shown,
      ).toContain('nothing was overwritten');
    } finally {
      await app.close();
    }
  }, 90_000);
});
