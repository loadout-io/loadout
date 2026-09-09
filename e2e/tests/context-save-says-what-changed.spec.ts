/* Udany zapis MÓWI, co się zmieniło — i mówi to, o co człowiek naprawdę pyta.
 *
 * 2026-09-09 (CT-09, znalezisko z natywnego QA) — właściciel kliknął `Save` osiem razy
 * w prawdziwym oknie. Każdy zapis SIĘ UDAŁ: `draftRevision` doszedł do ośmiu, na dysku
 * wylądowały trzy źródła, a Rust nie odrzucił niczego. Ekran przy tym milczał, bo handler
 * ustawiał zdanie wyłącznie przy PORAŻCE. Zapis udany bez śladu jest nieodróżnialny od zapisu,
 * który nic nie zrobił — i wniosek „Save nie działa" był w pełni racjonalny.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: sprawdzić, że po kliknięciu stoi napis „Saved". Przechodzi ją
 * ekran, który mówi „Saved" TAKŻE po odmowie — a wtedy dwa zdania o dwóch różnych wynikach stoją
 * obok siebie i człowiek wierzy temu pierwszemu. Dlatego trzeci przypadek sądzi odmowę i wymaga,
 * żeby zdania o sukcesie nie było WCALE.
 *
 * Drugi przypadek pilnuje, żeby to nie było zdanie ozdobne: zestaw z gotową wersją ma inną
 * następną czynność niż zestaw bez niej, więc dostaje inne zdanie.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const TITLE = 'Save says so';

const BASE = {
  schema: 1,
  id: 'ct-save',
  title: TITLE,
  description: '',
  archived: false,
  draftRevision: 1,
  latestReadyRevision: null as string | null,
  createdAt: '2026-09-09T00:00:00Z',
  changedAt: '2026-09-09T00:00:00Z',
};

const DRAFT = { schema: 1, sources: [], excluded: [], howToPrepare: '', requirements: [] };

/** Zdanie Rusta przy spóźnionym zapisie, słowo w słowo (`context::Error::Changed`). */
const REFUSED =
  'This context set was not saved: it changed on disk after you opened it, so nothing was ' +
  'overwritten. Open it again to see the newer one.';

function copies<T>(value: T, count: number): readonly TauriReply[] {
  return Array.from({ length: count }, () => ({ value }) as TauriReply);
}

function scene(
  ready: string | null,
  refuse: boolean,
): Readonly<Record<string, readonly TauriReply[]>> {
  const set = { ...BASE, latestReadyRevision: ready };
  const read = { set, draft: DRAFT, revision: 'rev-opened' };
  return {
    list_context_sets: copies([set], 8),
    read_context_set: copies(read, 4),
    save_context_draft: refuse
      ? Array.from({ length: 4 }, () => ({ error: REFUSED }))
      : copies({ set: { ...set, draftRevision: 2 }, draft: DRAFT, revision: 'rev-saved' }, 4),
  };
}

const SWITCH = '[data-section-switch="context"]';
const SCREEN = 'main[data-section="context"]';
const CARD = 'main [data-context-set]';
const SAVE = 'main [data-save]';
const SAVED = 'main [data-saved]';
const REFUSAL = 'main [data-refusal]';
const APPEARS = 6_000;

beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

async function openTheOnlySet(replies: Readonly<Record<string, readonly TauriReply[]>>) {
  const app = await openApp({ replies });
  const page = app.page;
  await page.locator(SWITCH).click();
  await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
  await page.locator(CARD).first().click();
  await page.locator(SAVE).waitFor({ state: 'attached', timeout: APPEARS });
  return app;
}

describe('a successful save says what it changed', () => {
  it('tells a set with no ready version that it still needs preparing', async () => {
    const app = await openTheOnlySet(scene(null, false));
    try {
      await app.page.locator(SAVE).click();
      await app.page.locator(SAVED).waitFor({ state: 'attached', timeout: APPEARS });
      expect(
        await app.page.locator(SAVED).innerText(),
        'the save succeeded and the screen has to say so, or it is indistinguishable from a ' +
          'save that did nothing at all',
      ).toContain('needs preparing before a step can use it');
      expect(await app.page.locator(REFUSAL).count()).toBe(0);
    } finally {
      await app.close();
    }
  }, 60_000);

  it('tells a ready set to prepare it again, because that is a different next move', async () => {
    const app = await openTheOnlySet(scene('rev-1', false));
    try {
      await app.page.locator(SAVE).click();
      await app.page.locator(SAVED).waitFor({ state: 'attached', timeout: APPEARS });
      expect(
        await app.page.locator(SAVED).innerText(),
        'a set that steps already use needs preparing AGAIN, and a single decorative "Saved" ' +
          'would hide exactly that',
      ).toContain('Prepare this set again');
    } finally {
      await app.close();
    }
  }, 60_000);

  it('says only the refusal when Rust turns the save down', async () => {
    const app = await openTheOnlySet(scene(null, true));
    try {
      await app.page.locator(SAVE).click();
      await app.page.locator(REFUSAL).waitFor({ state: 'attached', timeout: APPEARS });
      expect(await app.page.locator(REFUSAL).innerText()).toContain('was not saved');
      expect(
        await app.page.locator(SAVED).count(),
        'a refused save that also claims it saved puts two answers about one action on one ' +
          'screen, and the person believes the first one',
      ).toBe(0);
    } finally {
      await app.close();
    }
  }, 60_000);
});
