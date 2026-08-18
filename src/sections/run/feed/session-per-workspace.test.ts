/* Przełączenie zakresu NIE GUBI sesji — wymóg właściciela z 2026-08-18, słowo w słowo:
 * „jak się przełączam między workspace to nie tracę sesji".
 *
 * SŁABA WERSJA TEGO KRYTERIUM, i jest nią dokładnie to, co napisałby ktoś w pośpiechu:
 * `expect(feedFor('a')).not.toBe(feedFor('b'))`. Przechodzi dla rejestru, który oddaje dwa
 * osobne modele i którego NIKT nie pyta — czyli dla tej samej wady, tylko z mapą obok.
 * Przechodzi też dla rejestru, w którym `runFeed` dalej wisi na jednym, sztywnym modelu.
 * Odróżniają to trzy rzeczy, po których ten plik jedzie:
 *
 *   (a) do sesji zakresu B piszemy WTEDY, KIEDY OKNO PATRZY NA A — bo tak wygląda prawdziwa
 *       awaria: pompa dowozi linie biegu, który idzie w drugim folderze. Historia A nie ma
 *       prawa się o tym dowiedzieć;
 *   (b) po przełączeniu na B `runFeed` — czyli to, co czyta EKRAN — pokazuje historię B,
 *       a po powrocie na A znowu historię A, kompletną. Wersja gubiąca sesję wraca z pustą
 *       historią albo z „Thinking…" sprzed dwóch minut (nagłówek `src-tauri/src/workspace.rs`);
 *   (c) subskrybent `runFeed` DOSTAJE POWIADOMIENIE o przełączeniu. Bez tego React trzyma na
 *       ekranie migawkę poprzedniego zakresu, dopóki nie napłynie linia — czyli pokazuje cudzą
 *       pracę pod nazwą tego folderu, i to jest wersja, która wygląda na działającą.
 *
 * Kontrola przeciw pustej asercji: zanim cokolwiek porównamy, sprawdzamy, że obie sesje w ogóle
 * coś przyjęły. Porównanie dwóch pustych historii przechodzi na niczym.
 */
import { describe, expect, it } from 'vitest';

import type { FeedLine } from '../../../state/run';
import type { Workspace } from '../../../state/workspaces';
import { useWorkspaces } from '../../../state/workspaces';
import { feedFor, runFeed } from './live';

const MEETNOTES: Workspace = { id: '/w/meetnotes', name: 'meetnotes', folder: '/w/meetnotes' };
const SPREADSHEET: Workspace = {
  id: '/w/spreadsheet',
  name: 'spreadsheet',
  folder: '/w/spreadsheet',
};

/** Zakresy w magazynie. Aktywny przestawia się jak przy kliknięciu w bocznym menu. */
function workOn(workspace: Workspace | null): void {
  useWorkspaces.setState({
    all: [MEETNOTES, SPREADSHEET],
    activeId: workspace === null ? null : workspace.id,
  });
}

function note(id: number, agent: string, text: string): FeedLine {
  return { kind: 'note', agent, text, id, at: id * 1_000 };
}

/** Etykiety wierszy historii — to, co człowiek na ekranie naprawdę przeczyta. */
function historyOf(view: { history: readonly { label: string }[] }): readonly string[] {
  return view.history.map((row) => row.label);
}

const IN_MEETNOTES = 'Rewrote the field splitter as a three-state machine.';
const IN_SPREADSHEET = 'Renamed the quote column.';

describe('switching the workspace does not lose the run view behind it', () => {
  it('keeps two workspaces on two streams, and writes to the one the run belongs to', () => {
    workOn(MEETNOTES);

    /* Pompa pisze przez `feedFor(folder)` — bieg należy do folderu, w którym idzie, nie do tego,
     * na który człowiek akurat patrzy. Ta jedna linia jest różnicą między poprawną pompą
     * i tą, która przepisuje linie biegu z folderu B do sesji folderu A. */
    feedFor(MEETNOTES.id).appendLines([note(1, 'Build', IN_MEETNOTES)]);
    feedFor(SPREADSHEET.id).appendLines([note(2, 'Sweep', IN_SPREADSHEET)]);

    expect(
      historyOf(feedFor(MEETNOTES.id).view).length,
      'the first stream took nothing at all, so every comparison below would be a statement ' +
        'about two empty histories and would pass on nothing.',
    ).toBe(1);
    expect(historyOf(feedFor(SPREADSHEET.id).view).length, 'and so did the second').toBe(1);

    expect(
      historyOf(feedFor(MEETNOTES.id).view),
      'a line that belongs to the other folder landed in this stream. This is the one failure ' +
        'the registry exists to stop: the pump follows the view instead of the run, so the ' +
        'moment a person looks somewhere else the two runs read as one.',
    ).toEqual([IN_MEETNOTES]);
    expect(
      feedFor(SPREADSHEET.id),
      'and the two streams are two objects, not one map entry handed out twice',
    ).not.toBe(feedFor(MEETNOTES.id));
  });

  it('shows the stream of the workspace the person switched to, and gives the first one back', () => {
    workOn(MEETNOTES);
    expect(
      historyOf(runFeed.view),
      'the handle the screen reads is not showing the active workspace at all',
    ).toEqual([IN_MEETNOTES]);

    workOn(SPREADSHEET);
    expect(
      historyOf(runFeed.view),
      'after the switch the screen still shows the first workspace. A handle bound to one ' +
        'model at module load passes every test written on the active workspace and shows the ' +
        'wrong folder from the first switch on.',
    ).toEqual([IN_SPREADSHEET]);

    workOn(MEETNOTES);
    expect(
      historyOf(runFeed.view),
      'coming back to the first workspace lost its history. That is the exact sentence in the ' +
        'header of src-tauri/src/workspace.rs: it comes back empty, or with a Thinking from ' +
        'two minutes ago.',
    ).toEqual([IN_MEETNOTES]);
  });

  it('wakes the screen up on the switch itself, not on the next line', () => {
    workOn(MEETNOTES);
    let woken = 0;
    const drop = runFeed.subscribe(() => {
      woken += 1;
    });

    workOn(SPREADSHEET);
    expect(
      woken,
      'nobody told the screen that the workspace changed, so React keeps the previous ' +
        'view on screen until a line happens to arrive — and until then the window shows ' +
        'one folder’s work under another folder’s name.',
    ).toBe(1);

    /* Po przełączeniu subskrypcja ma wisieć na NOWEJ sesji. Wersja, która przewiązała uchwyt
     * i zapomniała o subskrypcji, budzi ekran raz i potem milczy o każdej nowej linii. */
    feedFor(SPREADSHEET.id).appendLines([note(3, 'Sweep', 'Checked the header row.')]);
    expect(woken, 'and a line in the workspace now on screen wakes it again').toBe(2);

    const quiet = woken;
    feedFor(MEETNOTES.id).appendLines([note(4, 'Build', 'Still working on the splitter.')]);
    expect(
      woken,
      'a line in the workspace nobody is looking at woke the screen. The stream keeps taking ' +
        'lines — that is the point — but the view it is not on has nothing to redraw.',
    ).toBe(quiet);

    drop();
    workOn(MEETNOTES);
    expect(woken, 'and unsubscribing really unsubscribes, switch included').toBe(quiet);
  });
});
