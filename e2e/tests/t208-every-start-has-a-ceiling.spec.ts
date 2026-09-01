/* T-208: żaden bieg nie zaczyna się bez sufitu wydatku, a bieg, któremu człowiek go zdjął,
 * mówi o tym na ekranie.
 *
 * PO CO PRAWDZIWA PRZEGLĄDARKA, skoro obok stoją kryteria na funkcjach. Bo one mierzą FUNKCJE.
 * Ten plik nie renderuje niczego ręcznie i nie woła ani jednej funkcji stanu: otwiera aplikację,
 * czyta pole, które widzi człowiek, wchodzi do sekcji przełącznikiem, w który on klika, i wpisuje
 * kwotę w nazwaną kontrolkę. Zielone kryterium nad martwą kontrolką jest klasą wady, dla której
 * to repo powstało (niezmiennik 29).
 *
 * TRZY KONTROLE PRZECIW PUSTEJ ASERCJI:
 *   scena odpowiada `defaultBudgetUsd: 25`, a nie liczbą, którą któraś ze stron ma zaszytą —
 *     więc „na pasku stoi 25" nie da się przejść własną stałą okna ani stałą Rusta;
 *   biblioteka agentów jest PUSTA, więc kontrolka sufitu ma się renderować także na maszynie,
 *     na której nie ma jeszcze kogo wskazać na lidera — sufit nie jest dodatkiem do lidera;
 *   samo otwarcie okna nie ma prawa nic zapisać: pokazanie zapamiętanego wyboru jest ODCZYTEM,
 *     a zapis przepisałby plik tym, co ekran akurat trzymał pierwszy.
 *
 * GRANICA ODPOWIADA KSZTAŁTEM, nie treścią (nagłówek `../harness.ts`). `read_settings` dostaje
 * własną odpowiedź, bo domyślne `null` znaczyłoby tu „plik nic nie pamięta" w scenie, której
 * całym pytaniem jest to, co plik pamięta.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import type { Page } from '@playwright/test';

import { NO_CEILING_SAID } from '../../src/sections/run/limits/budget';
import { DEFAULT_BUDGET_LABEL } from '../../src/sections/settings/index';
import type { RunningApp, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/Users/somebody/Projects/loadout-t208-every-start-has-a-ceiling';
const WORKSPACE = { id: FOLDER, name: 'Every start has a ceiling', folder: FOLDER };

/** Co pamięta plik. Ani okno, ani Rust nie mają tej liczby zaszytej — i o to chodzi. */
const SAVED = 25;

/** Co człowiek wpisuje w Settings. Inna liczba, żeby „zapisało się" znaczyło coś więcej niż nic. */
const TYPED = 40;

/** Workflow z jednym krokiem — bez niego przycisk Run jest wygaszony i nic nie da się zacząć. */
const WORKFLOW = {
  path: 'ship-it.json',
  workflow: {
    format: 1,
    id: 'wf-t208',
    name: 'Ship it',
    steps: [
      {
        kind: 'agent',
        id: 's_ship',
        name: 'Ship',
        agent: 'agent-t208',
        overrides: {},
        copies: 1,
        instructions: 'Do the work.',
        skills: 'all',
        folder: { use: 'project' },
        handover: 'notes',
        at: { x: 24, y: 24 },
      },
    ],
    links: [],
  },
};

const RUN_SWITCH = '[data-section-switch="run"]';
const SETTINGS_SWITCH = '[data-section-switch="settings"]';
const RUN_SCREEN = 'main[data-section="run"]';
const SETTINGS_SCREEN = 'main[data-section="settings"]';
/** Pole kwoty na pasku Run — ta sama kotwica, której używa kryterium T-94. */
const RUN_BUDGET = 'input[data-budget]';
const SETTINGS_BUDGET = `input[aria-label="${DEFAULT_BUDGET_LABEL}"]`;
/** Zdanie o zdjętym sufcie, w miejscu, w którym stoi na pasku. */
const NO_CEILING = '[data-no-ceiling]';
/** Przycisk, którym człowiek zaczyna bieg ręcznie. */
const MANUAL_RUN = 'button[data-workflow-run="manual"]';

/** Ile czekamy na to, co ma przyjść po kliknięciu. Odpowiedź wraca w tej samej karcie. */
const APPEARS = 6_000;

function copies<T>(value: T, count = 12): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

/** Wszystko, co plik pamięta — i jedyne, co ta scena podstawia. */
function scene(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workspaces: copies([WORKSPACE]),
    /* PUSTA BIBLIOTEKA AGENTÓW. Sufit biegu nie jest dodatkiem do wyboru lidera i ma stać na
     * ekranie także wtedy, kiedy nie ma jeszcze kogo wskazać. */
    list_agents: copies([]),
    list_workflows: copies([WORKFLOW]),
    list_skills: copies([]),
    read_settings: copies({ defaultLead: '', defaultBudgetUsd: SAVED }),
    save_settings: copies({ defaultLead: '', defaultBudgetUsd: TYPED }, 6),
    /* Bieg kończy się od razu, bo to nie o jego przebieg tu chodzi: `run_workflow` po tamtej
     * stronie trwa tyle, co bieg, a nam potrzebne są TRZY Starty jeden po drugim. */
    run_workflow: copies(null, 6),
  };
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

/**
 * Naciska Run i czeka, aż bieg dojdzie do Rusta i zejdzie.
 *
 * Czekanie jest treścią, nie ostrożnością: zapadka `going` w `run/io.ts` odrzuca drugi Start,
 * dopóki pierwszy nie zszedł, więc bez tego trzy kliknięcia dałyby jedno wywołanie i kryterium
 * mierzyłoby zapadkę zamiast sufitu.
 */
async function pressRun(app: RunningApp, howManyBefore: number): Promise<TauriCallArgs> {
  await app.page.locator(MANUAL_RUN).click();
  const deadline = Date.now() + APPEARS;
  while (Date.now() < deadline) {
    const calls = await sentAs(app, 'run_workflow');
    if (calls.length > howManyBefore) {
      /* Chwila ciszy: `finally` w `run/io.ts` zdejmuje zapadkę i przywraca pasek dopiero po
       * powrocie komendy, a bez tego następne kliknięcie trafiłoby w bieg, który jeszcze idzie. */
      await app.page.waitForTimeout(120);
      return calls[calls.length - 1] ?? {};
    }
    await app.page.waitForTimeout(25);
  }
  return {};
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

describe('every run starts under a ceiling somebody can see', () => {
  it('starts every run under the ceiling set in Settings, and says on screen when a person takes it off', async () => {
    const app = await openApp({ replies: scene() });
    try {
      const page = app.page;

      /* ── pasek Run pokazuje zapisany sufit, bez ani jednego kliknięcia ─────────────────── */
      await page.locator(RUN_SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await page.locator(RUN_BUDGET).waitFor({ state: 'attached', timeout: APPEARS });
      expect(
        await settledValue(page, RUN_BUDGET, String(SAVED)),
        'the run strip opens with no ceiling in it, so a run started right now is capped by ' +
          'nothing — and nothing on the screen says so. That silent uncapped run is the whole ' +
          'of what this task removes.',
      ).toBe(String(SAVED));
      expect(
        await sentAs(app, 'save_settings'),
        'opening a window wrote to the settings file although nobody chose anything. Showing a ' +
          'saved ceiling is a read; a write here would rewrite the file from whatever the ' +
          'screen happened to hold first.',
      ).toEqual([]);

      /* ── ta sama liczba stoi w Settings, i to tam się ją zmienia ───────────────────────── */
      await page.click(SETTINGS_SWITCH);
      await page.locator(SETTINGS_SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      await page.locator(SETTINGS_BUDGET).waitFor({ state: 'visible', timeout: APPEARS });
      expect(
        await settledValue(page, SETTINGS_BUDGET, String(SAVED)),
        'Settings does not show the ceiling the file remembers, so the number every run takes ' +
          'is one nobody can see or change — which is what "explicit" was supposed to end.',
      ).toBe(String(SAVED));

      /* WPISZ I ODEJDŹ, bo tak kończy się pisanie liczby. Zapis po każdym znaku odrzucałby „0"
       * w drodze do „0.5" i zabierał człowiekowi to, co właśnie napisał (`sections/settings`,
       * `spendAtMost`), więc kryterium ma iść tą samą drogą, co palce. */
      await page.fill(SETTINGS_BUDGET, String(TYPED));
      await page.locator(SETTINGS_BUDGET).blur();
      /* Najpierw czekamy, aż kontrolka pokaże potwierdzoną kwotę, i dopiero potem czytamy taśmę:
       * pole wraca do wartości z magazynu DOPIERO po powrocie zapisu, więc ta jedna asercja
       * ustawia obie następne za tym, co naprawdę przeszło przez granicę. */
      expect(
        await settledValue(page, SETTINGS_BUDGET, String(TYPED)),
        'the amount did not stay in the control it was typed into, so either nothing was saved ' +
          'or the disk answered and the screen kept its old answer.',
      ).toBe(String(TYPED));
      const writes = await sentAs(app, 'save_settings');
      expect(
        writes,
        'changing the default ceiling never reached Rust. The choice then lives in this window ' +
          'only and dies with it, so the next window starts runs under the old number.',
      ).toHaveLength(1);
      expect(
        writes[0]?.['defaultBudgetUsd'],
        'the write reached Rust without the amount it was given. Tauri matches invoke arguments ' +
          'by name, so a value under another key is not a smaller call — it is a rejected one.',
      ).toBe(TYPED);

      /* ── i to samo bierze pasek Run, bez wpisywania drugi raz ──────────────────────────── */
      await page.click(RUN_SWITCH);
      await page.locator(RUN_SCREEN).waitFor({ state: 'attached', timeout: APPEARS });
      expect(
        await settledValue(page, RUN_BUDGET, String(TYPED)),
        'the run strip does not show the ceiling just set in Settings, so a limit a person sets ' +
          'once has to be typed again before every run — which is the same defect the default ' +
          'lead had.',
      ).toBe(String(TYPED));

      /* ── i naciśnięty Run naprawdę niesie tę liczbę ────────────────────────────────────── */
      const first = await pressRun(app, 0);
      expect(
        first['budgetUsd'],
        'pressing Run without touching the amount started a run under a different ceiling than ' +
          'the one on the screen. Showing a number and sending another is worse than showing ' +
          'none.',
      ).toBe(TYPED);

      /* ── zdjęcie sufitu jest wolno, ale nie po cichu ───────────────────────────────────── */
      await page.fill(RUN_BUDGET, '');
      await page
        .locator(NO_CEILING)
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);
      expect(
        (await page.locator(RUN_SCREEN).textContent()) ?? '',
        'clearing the amount left the screen saying nothing, so a run with no ceiling at all ' +
          'looks exactly like a run whose amount has not been typed yet. An empty field is not ' +
          'a sentence, and the placeholder is read once, when nobody is looking.',
      ).toContain(NO_CEILING_SAID);
      expect(
        await page.locator(NO_CEILING).isVisible(),
        'the sentence is in the markup and not on the screen, which is the difference this ' +
          'file exists to measure.',
      ).toBe(true);

      const uncapped = await pressRun(app, 1);
      expect(
        uncapped['budgetUsd'],
        'a person who cleared the field still has to be able to run without a ceiling. That ' +
          'door stays open — it is only the silence around it that this task closes.',
      ).toBeNull();

      /* ── ZDJĘCIE SUFITU DOTYCZYŁO JEDNEGO BIEGU, i to jest sedno tego przypadku ─────────
       *
       * Bez tego trzeciego Startu całe kryterium przechodzi także dla okna, które zdejmuje sufit
       * RAZ I NA ZAWSZE: każdy następny bieg leci wtedy bez ograniczenia, nikt tego nie zamawia
       * i nic tego nie mówi — czyli ta sama wada, tylko przesunięta o jeden bieg dalej. */
      expect(
        await settledValue(page, RUN_BUDGET, String(TYPED)),
        'after the uncapped run ended the strip is still empty, so the next run is uncapped too ' +
          'and nobody asked for that. Taking the ceiling off is a decision about ONE run.',
      ).toBe(String(TYPED));
      expect(
        await page.locator(NO_CEILING).count(),
        'and the screen still says this run has no spending limit although the next one has ' +
          'one. A sentence that outlives the run it describes is a sentence about nothing.',
      ).toBe(0);

      const third = await pressRun(app, 2);
      expect(
        third['budgetUsd'],
        'the run after the uncapped one went out uncapped as well. A ceiling taken off for one ' +
          'run has to come back, or "every start has a ceiling" is true exactly until the first ' +
          'time somebody takes one off.',
      ).toBe(TYPED);

      /* ── kwota, która nie jest kwotą, jest ODRZUCANA, a nie czytana jako „bez sufitu" ──── */
      await page.fill(RUN_BUDGET, '0');
      expect(
        await settledValue(page, RUN_BUDGET, String(TYPED)),
        'typing 0 was taken for an amount. A run allowed to spend nothing can never start, so ' +
          'the field must refuse it and keep what it had.',
      ).toBe(String(TYPED));
      expect(
        await page.locator(NO_CEILING).count(),
        'typing 0 turned into "no spending limit". That is the second silent door out of every ' +
          'ceiling in this product: an amount that is not an amount must not mean no amount.',
      ).toBe(0);

      expect(
        await sentAs(app, 'save_settings'),
        'taking the ceiling off one run rewrote the saved default. Those are two different ' +
          'facts: one is about this run, the other is about every run that has not said otherwise.',
      ).toHaveLength(1);
    } finally {
      await app.close();
    }
  }, 90_000);
});
