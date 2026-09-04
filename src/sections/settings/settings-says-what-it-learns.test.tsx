/* Czy domyślna refleksja ma kontrolkę, opis i jedną drogę do pliku (2026-09, Z-18).
 *
 * SŁABA WERSJA: szukać samego napisu. Przechodzi ją martwa etykieta. Dlatego test łączy trzy
 * granice: ptaszek wskazuje na widoczne zdanie, zapis niesie nazwany klucz do Rusta, a świeży
 * wybór Run czyta tę samą potwierdzoną wartość zamiast własnego literału.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((command: string, sent?: unknown): Promise<unknown> => {
    if (command === 'save_settings') return Promise.resolve(sent);
    return Promise.resolve({
      defaultLead: '',
      defaultBudgetUsd: 75,
      navCollapsed: false,
      keepLastRuns: 0,
      learnFromRuns: false,
    });
  }),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const {
  default: SettingsScreen,
  LEARN_FROM_RUNS_LABEL,
  LEARN_FROM_RUNS_SAID,
} = await import('./index');
const { chooseLearnFromRuns, learnFromRuns, loadSettings } = await import('../../state/settings');
const {
  reflectionForRequestedRun,
  requestRun,
  requestedRun,
  subscribeToReflectionChoice,
  takeRequestedRun,
} = await import('../run/requested');

const markup = renderToStaticMarkup(<SettingsScreen />);
const field = /<input[^>]*id="learn-from-runs"[^>]*>/.exec(markup)?.[0] ?? '';
const describedBy = /aria-describedby="([^"]+)"/.exec(field)?.[1] ?? '';

const reflectionBeforeDisk = reflectionForRequestedRun();
let reflectionUpdates = 0;
const stopWatchingReflection = subscribeToReflectionChoice(() => {
  reflectionUpdates += 1;
});
await loadSettings();
stopWatchingReflection();
const reflectionAfterDisk = reflectionForRequestedRun();
requestRun('fresh-window.workflow.json');
const requestedAfterDisk = requestedRun();
takeRequestedRun();

await chooseLearnFromRuns(true);
invoked.mockClear();
await chooseLearnFromRuns(false);
const askedRust = invoked.mock.calls.at(0);

describe('Settings says what Loadout learns from runs', () => {
  it('draws an on-by-default checkbox with a visible description outside its label', () => {
    expect(
      field,
      'the setting has no checkbox, so a person cannot turn the private turn off',
    ).toContain('type="checkbox"');
    expect(field, 'the shipped default silently turns learning off').toContain('checked=""');
    expect(field, 'the checkbox does not point at the sentence that explains its effect').toContain(
      `aria-describedby="${describedBy}"`,
    );
    expect(markup).toContain(
      `<label class="label" for="learn-from-runs">${LEARN_FROM_RUNS_LABEL}</label>`,
    );
    expect(markup).toContain(
      `<p id="${describedBy}" data-learn-help="true" class="lead">${LEARN_FROM_RUNS_SAID}</p>`,
    );
    expect(
      /<label[^>]*for="learn-from-runs"[^>]*>[\s\S]*?<\/label>/.exec(markup)?.[0] ?? '',
      'the explanatory sentence became part of the checkbox name instead of its description',
    ).not.toContain(LEARN_FROM_RUNS_SAID);
  });

  it('updates a mounted Run when disk answers and carries that value into its request', () => {
    expect(reflectionBeforeDisk).toBe(true);
    expect(reflectionAfterDisk).toBe(false);
    expect(reflectionUpdates, 'the mounted Run did not hear the saved setting arrive').toBe(1);
    expect(requestedAfterDisk?.reflectionEnabled).toBe(false);
  });

  it('saves the choice and exposes that confirmed value', () => {
    expect(askedRust?.at(0)).toBe('save_settings');
    expect(askedRust?.at(1)).toMatchObject({
      defaultLead: '',
      defaultBudgetUsd: 75,
      navCollapsed: false,
      keepLastRuns: 0,
      learnFromRuns: false,
    });
    expect(learnFromRuns()).toBe(false);
    expect(reflectionForRequestedRun()).toBe(false);
  });
});
