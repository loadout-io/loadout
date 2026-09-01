/* T-163: lider wybrany raz w Settings stoi na pasku Run — także w oknie otwartym od nowa.
 *
 * PO CO PRAWDZIWA PRZEGLĄDARKA, skoro obok stoją kryteria na funkcjach. Bo one mierzą FUNKCJE.
 * Ten plik nie renderuje niczego ręcznie i nie woła ani jednej funkcji stanu: otwiera aplikację,
 * wchodzi do sekcji przełącznikiem, w który klika człowiek, wybiera agenta z nazwanej kontrolki
 * i sprawdza, co widać PO PRZEJŚCIU na Run. Sześć razy 2026-08-16 kryterium było zielone,
 * a produkt nie działał — mechanizm istniał i nikt go nie zamontował (niezmiennik 29).
 *
 * TRZY KONTROLE PRZECIW PUSTEJ ASERCJI, bo bez nich „na pasku stoi wybrany agent" przechodzi
 * na ekranie, który stawia go tam zawsze:
 *   przed czymkolwiek pasek Run pokazuje ZAPROSZENIE, nie agenta — więc „agent tam jest" po
 *     wyborze mówi o wyborze, a nie o ekranie;
 *   biblioteka ma DWÓCH agentów i wybierany jest DRUGI, więc implementacja biorąca „pierwszego
 *     z listy" nie przechodzi;
 *   wskazanie innego lidera NA PASKU nie ma prawa przepisać pliku — bez tego „jeden fakt, jedno
 *     miejsce" (niezmiennik 13) byłoby zdaniem o dwóch kopiach, które akurat się zgadzają.
 *
 * GRANICA ODPOWIADA KSZTAŁTEM, nie treścią (nagłówek `../harness.ts`). `read_settings` dostaje
 * własną odpowiedź, bo domyślne `null` znaczyłoby tu „nikt nie wybierał" w scenie, której całym
 * pytaniem jest to, co plik pamięta.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Page } from '@playwright/test';

import { LEAD_LABEL } from '../../src/sections/run/lead';
import { DEFAULT_LEAD_LABEL } from '../../src/sections/settings/index';
import type { RunningApp, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/Users/somebody/Projects/loadout-t163-default-lead';
const WORKSPACE = { id: FOLDER, name: 'Default lead', folder: FOLDER };

/** DWAJ zapisani agenci, i wybierany jest DRUGI: „pierwszy z listy" ma nie przechodzić. */
const SCOUT = {
  id: 'agent-t163-scout',
  name: 'Scout',
  summary: 'Reads the code first',
  skills: [],
};
const BUILDER = {
  id: 'agent-t163-builder',
  name: 'Builder',
  summary: 'Writes the change',
  skills: [],
};

const RUN_SWITCH = '[data-section-switch="run"]';
const SETTINGS_SWITCH = '[data-section-switch="settings"]';
const RUN_SCREEN = 'main[data-section="run"]';
const SETTINGS_SCREEN = 'main[data-section="settings"]';
const DEFAULT_LEAD = `select[aria-label="${DEFAULT_LEAD_LABEL}"]`;
const RUN_LEAD = `select[aria-label="${LEAD_LABEL}"]`;
/** Zaproszenie z paska Run: to ono ma zniknąć, kiedy wybór już jest. */
const INVITE = 'Pick a lead agent';

/** Ile czekamy na to, co ma przyjść po kliknięciu. Odpowiedź wraca w tej samej karcie. */
const APPEARS = 6_000;

function copies<T>(value: T, count = 12): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

/** `defaultLead` to wszystko, co plik pamięta — i jedyna rzecz, którą ta scena podstawia. */
function scene(defaultLead: string): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workspaces: copies([WORKSPACE]),
    list_agents: copies([SCOUT, BUILDER]),
    list_workflows: copies([]),
    list_skills: copies([]),
    read_settings: copies({ defaultLead }),
    save_settings: copies({ defaultLead: BUILDER.id }, 4),
  };
}

/** Czeka, aż kontrolka lidera wypełni się biblioteką — inaczej mierzymy pusty `<select>`. */
async function leadOptions(page: Page, selector: string): Promise<readonly string[]> {
  await page
    .locator(`${selector} option`)
    .nth(1)
    .waitFor({ state: 'attached', timeout: APPEARS })
    .catch(() => undefined);
  return page
    .locator(`${selector} option`)
    .evaluateAll((options) => options.map((option) => option.textContent?.trim() ?? ''));
}

/** Wszystko, co poleciało do Rusta pod tą nazwą, w kolejności wysłania. */
async function sentAs(app: RunningApp, command: string): Promise<readonly TauriCallArgs[]> {
  return (await app.calls()).filter((one) => one.cmd === command).map((one) => one.args);
}

type TauriCallArgs = Record<string, unknown>;

/** Czeka, aż wskazana kontrolka pokaże tę wartość, i oddaje to, co naprawdę pokazuje. */
async function settledValue(page: Page, selector: string, wanted: string): Promise<string> {
  const deadline = Date.now() + APPEARS;
  let showing = '';
  while (Date.now() < deadline) {
    showing = await page.locator(selector).inputValue();
    if (showing === wanted) return showing;
    await page.waitForTimeout(25);
  }
  return showing;
}

/* Rozruch vite i chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku:
 * `openApp()` jest leniwy, więc bez tego haka pierwszy `it` płaci cały rozruch pod swoim
 * limitem i pada na nim, mierząc start narzędzia zamiast produktu. */
beforeAll(async () => {
  const warm = await openApp();
  await warm.close();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('the lead chosen once in Settings is the lead the run strip shows', () => {
  it('carries the lead picked in Settings into the Run strip, and a fresh window opens already pointed at it', async () => {
    const app = await openApp({ replies: scene('') });
    try {
      const page = app.page;

      /* ── kontrola wstępna: pasek Run zaprasza, a nie wskazuje ──────────────────────────── */
      await page.locator(RUN_SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      const inviting = await leadOptions(page, RUN_LEAD);
      expect(
        inviting,
        'the run strip does not offer the invitation before anything was chosen, so its ' +
          'disappearance below would say nothing about the choice. It offers: ' +
          JSON.stringify(inviting),
      ).toContain(INVITE);
      expect(
        await page.locator(RUN_LEAD).inputValue(),
        'the run strip already points at somebody before a person chose anyone. Everything ' +
          'below would then be true of a screen that fills this control on its own.',
      ).toBe('');

      /* ── człowiek wchodzi do Settings i wybiera DRUGIEGO agenta ────────────────────────── */
      await page.click(SETTINGS_SWITCH);
      await page.locator(SETTINGS_SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await page.locator(DEFAULT_LEAD).waitFor({ state: 'visible', timeout: APPEARS });
      await leadOptions(page, DEFAULT_LEAD);
      await page.selectOption(DEFAULT_LEAD, BUILDER.id);

      const saved = await settledValue(page, DEFAULT_LEAD, BUILDER.id);
      expect(
        saved,
        'the choice did not stay in the control it was made in, so the disk answered and the ' +
          'screen kept its old answer.',
      ).toBe(BUILDER.id);
      const writes = await sentAs(app, 'save_settings');
      expect(
        writes,
        'choosing a default lead never reached Rust. The choice then lives in this window only ' +
          'and dies with it — which is the state this task exists to end.',
      ).toHaveLength(1);
      expect(
        writes[0]?.['defaultLead'],
        'the write reached Rust without the agent it was given. A call that arrives without its ' +
          'values is the same silence as no call at all.',
      ).toBe(BUILDER.id);

      /* ── i to samo widać na Run, bez wybierania drugi raz ──────────────────────────────── */
      await page.click(RUN_SWITCH);
      await page.locator(RUN_SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await leadOptions(page, RUN_LEAD);
      expect(
        await settledValue(page, RUN_LEAD, BUILDER.id),
        'the run strip does not show the lead that was just chosen in Settings. A choice a ' +
          'person makes once and has to make again on every run is the defect this task names.',
      ).toBe(BUILDER.id);
      expect(
        await leadOptions(page, RUN_LEAD),
        'the run strip still asks for a lead although one is chosen, so the person is invited ' +
          'to answer a question that already has an answer.',
      ).not.toContain(INVITE);

      /* ── wskazanie na PASKU nie przepisuje pliku (niezmiennik 13) ──────────────────────── */
      await page.selectOption(RUN_LEAD, SCOUT.id);
      expect(
        await settledValue(page, RUN_LEAD, SCOUT.id),
        'picking a lead in the run strip did nothing, so that control lies about what it does.',
      ).toBe(SCOUT.id);

      await page.click(SETTINGS_SWITCH);
      await page.locator(SETTINGS_SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await leadOptions(page, DEFAULT_LEAD);
      expect(
        await settledValue(page, DEFAULT_LEAD, BUILDER.id),
        'a lead pointed at for one run overwrote the saved default. Those are two different ' +
          'facts: one is about this run, the other is about every run that has not said otherwise.',
      ).toBe(BUILDER.id);
      expect(
        await sentAs(app, 'save_settings'),
        'the run strip wrote to the settings file. Its control changes this window and nothing ' +
          'else — a second writer of one fact is how two copies of it start to disagree.',
      ).toHaveLength(1);
    } finally {
      await app.close();
    }

    /* ── ŚWIEŻA KARTA: nowy magazyn, nowy stan, ani jednego kliknięcia ─────────────────────
     *
     * Magazyny żyją na poziomie modułu, czyli w kontekście strony, więc nowa karta to nowa
     * aplikacja. To jest jedyna droga, którą da się tu odróżnić „zapamiętane w oknie" od
     * „przeczytane z pliku". */
    const reopened = await openApp({ replies: scene(BUILDER.id) });
    try {
      const page = reopened.page;
      await page.locator(RUN_SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await leadOptions(page, RUN_LEAD);
      expect(
        await settledValue(page, RUN_LEAD, BUILDER.id),
        'a window opened straight onto Run does not show the saved lead, so the file is read by ' +
          'nobody until a person walks into Settings — and a person who never goes there picks ' +
          'the same agent before every run.',
      ).toBe(BUILDER.id);
      expect(
        await sentAs(reopened, 'save_settings'),
        'opening a window wrote to the settings file although nobody chose anything. Showing a ' +
          'saved choice is a read; a write here would rewrite the file from whatever the screen ' +
          'happened to hold first.',
      ).toEqual([]);
    } finally {
      await reopened.close();
    }
  }, 90_000);
});
