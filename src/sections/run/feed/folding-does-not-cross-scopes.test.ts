/* Ta sama reguła przy przełączeniu zakresu: okno sklejania nie przechodzi ani przez koniec
 * biegu, ani przez granicę zakresu pracy.
 *
 * DRUGA TWARZ TEJ SAMEJ WADY. Rejestr `feeds` (`./live.ts`) trzyma JEDEN model na zakres i nie
 * ma usuwania — to jest wymóg właściciela z 2026-08-18 („jak się przełączam między workspace to
 * nie tracę sesji"), nie przeoczenie. Model żyje więc tak długo jak okno. Człowiek, który
 * puścił bieg w jednym zakresie, poszedł do drugiego i wrócił po godzinie, zastaje model
 * z otwartą grupą sklejania sprzed godziny — a pierwsza sklejalna linia nowego biegu, jeśli
 * trafi w okno liczone od TAMTEJ linii, dolicza się do wiersza sprzed godziny.
 *
 * DLACZEGO TO OSOBNY PLIK, A NIE CZWARTY PRZYPADEK W `./folding-does-not-cross-runs.test.ts`.
 * Tamten plik sądzi jeden model, zbudowany wprost przez `createFeed`. Ten sądzi drogę, którą
 * naprawdę jeździ produkcja: pompa pisze przez `feedFor(folder)`, bo bieg należy do folderu,
 * w którym idzie, a nie do tego, na który człowiek akurat patrzy, a ekran czyta przez `runFeed`,
 * czyli przez uchwyt do zakresu na wierzchu. Wada mieszka w modelu, ale widać ją dopiero
 * dlatego, że rejestr oddaje TEN SAM model po powrocie — więc test, który tego nie sprawdza,
 * przeszedłby także dla rejestru bijącego świeży model przy każdym pytaniu.
 *
 * SŁABA WERSJA, wypisana wprost, bo napisałby ją każdy w pośpiechu: sam przypadek „linia w A nie
 * dolicza się do wiersza w B". Przechodzi DZIŚ i przechodziłby przed tym zadaniem, bo dwa
 * zakresy to dwa osobne domknięcia — czyli nie mierzy niczego, co ta zmiana rusza. Rozróżnia to
 * przypadek pierwszy: powrót do TEGO SAMEGO zakresu po zejściu biegu.
 *
 * KONTROLA PRZECIW PUSTEMU PRZEJŚCIU stoi na końcu pliku i jest tą samą, co w pliku obok: ta
 * sama para znaczników czasu, w jednym zakresie, bez zejścia biegu między liniami, ma dać wiersz
 * JEDEN. Bez niej „dwa wiersze" jest zdaniem o dwóch znacznikach, które i tak nigdy by się nie
 * skleiły, a implementacja z wyłączonym sklejaniem przechodzi cały plik.
 */
import { describe, expect, it } from 'vitest';

import type { Workspace } from '../../../state/workspaces';
import { useWorkspaces } from '../../../state/workspaces';
import { line } from './fixtures/lines';
import { feedFor, runFeed } from './live';

const FORGE = 'Forge';

const FIRST_FILE = 'src/splitter.ts';
const SECOND_FILE = 'src/header.ts';
const THIRD_FILE = 'src/quotes.ts';

/* TA SAMA PARA ZNACZNIKÓW, CO W PLIKU OBOK — 500 ms od siebie, głęboko w oknie 2 s liczonym od
 * pierwszej linii grupy. Jedna para przez cały plik, bo dopiero ostatni przypadek nadaje
 * „dwóm wierszom" znaczenie: ta sama para bez granicy między liniami daje wiersz jeden. */
const OPENS_AT = 0;
const INSIDE_AT = 500;

/* Rejestr `feeds` jest stanem MODUŁU i nie ma usuwania, więc przypadki w tym pliku dzielą go
 * między sobą. Każdy bierze więc własne foldery: wspólny identyfikator wnosiłby do sceny
 * historię poprzedniego przypadku i „dwa wiersze" znaczyłoby wtedy coś innego w każdym z nich. */
function workspace(id: string): Workspace {
  return { id, name: id, folder: id };
}

/** Przestawia zakres, w którym pracujemy — dokładnie to, co robi kliknięcie w bocznym menu. */
function lookAt(all: readonly Workspace[], active: Workspace): void {
  useWorkspaces.setState({ all: [...all], activeId: active.id });
}

describe('the window that folds neighbouring lines does not reach across a workspace', () => {
  it('gives a workspace its own row when a run went down and the person came back to it', () => {
    const alpha = workspace('/w/back-to-alpha');
    const beta = workspace('/w/back-to-beta');
    const both = [alpha, beta];
    lookAt(both, alpha);

    expect(
      feedFor(alpha.id),
      'the registry has to hand the SAME model back for the same folder, or nothing below is ' +
        'measured: one that mints a fresh model per question passes this whole file while ' +
        'losing the run a person walked away from, which is the requirement the registry ' +
        'exists to keep',
    ).toBe(feedFor(alpha.id));

    /* Pompa pisze przez `feedFor(folder)`, nie przez uchwyt ekranu — bieg należy do folderu,
     * w którym idzie. Ta linia zostawia otwartą grupę sklejania. */
    feedFor(alpha.id).appendLines([line.read(1, OPENS_AT, FORGE, FIRST_FILE)]);
    feedFor(alpha.id).runEnded();

    /* Człowiek idzie popatrzeć na drugi folder, tam też coś się dzieje, i wraca. */
    lookAt(both, beta);
    feedFor(beta.id).appendLines([line.read(2, 250, FORGE, THIRD_FILE)]);
    lookAt(both, alpha);

    feedFor(alpha.id).appendLines([line.read(3, INSIDE_AT, FORGE, SECOND_FILE)]);

    expect(
      runFeed.view.history.map((row) => row.ids),
      'the folder a person came back to grew the row of the run that had already gone down. ' +
        'The model outlives the run by the whole life of the window, so the window left open by ' +
        'the last foldable line is still open an hour later — and read through the handle the ' +
        'screen reads, that is two runs standing in the transcript as one row. Two rows here, ' +
        'one line each.',
    ).toEqual([[1], [3]]);
    expect(
      runFeed.view.history.at(-1)?.label,
      'and the row of the new run counts from itself: one file read is "Read 1 file", never ' +
        '"Read 2 files" because a run that ended earlier also read one',
    ).toBe('Read 1 file');
  });

  it('never grows a row in one workspace with a line that arrived in another', () => {
    const alpha = workspace('/w/apart-alpha');
    const beta = workspace('/w/apart-beta');
    const both = [alpha, beta];
    lookAt(both, alpha);

    /* Ten sam agent, ten sam rodzaj, TEN SAM znacznik czasu — czyli wszystko, czym sklejanie
     * kluczuje grupę, zgadza się co do joty. Różni je wyłącznie folder. */
    feedFor(alpha.id).appendLines([line.read(1, OPENS_AT, FORGE, FIRST_FILE)]);
    feedFor(beta.id).appendLines([line.read(2, OPENS_AT, FORGE, SECOND_FILE)]);

    expect(
      runFeed.view.history.length,
      'the folder on screen took nothing at all, so every comparison below would be a ' +
        'statement about two empty transcripts and would pass on nothing',
    ).toBe(1);
    expect(
      runFeed.view.history.map((row) => row.ids),
      'the folder on screen has to show ITS OWN line and no other. A line from the other ' +
        'folder folded into this row would hand one workspace the work done in another, and it ' +
        'reads as true because nothing on the screen contradicts it.',
    ).toEqual([[1]]);

    lookAt(both, beta);

    expect(
      runFeed.view.history.length,
      'and the second folder took its line too — otherwise the assertion below compares an ' +
        'empty list with an empty list',
    ).toBe(1);
    expect(
      runFeed.view.history.map((row) => row.ids),
      'and after the switch the screen shows the second folder its own line, with the same ' +
        'agent, the same kind and the same stamp as the first one and no trace of it',
    ).toEqual([[2]]);
  });

  it('still folds two neighbouring reads inside one workspace, so two rows means something', () => {
    const solo = workspace('/w/folds-inside');
    lookAt([solo], solo);

    feedFor(solo.id).appendLines([line.read(1, OPENS_AT, FORGE, FIRST_FILE)]);
    feedFor(solo.id).appendLines([line.read(2, INSIDE_AT, FORGE, SECOND_FILE)]);

    expect(
      runFeed.view.history.map((row) => row.ids),
      'THE SAME PAIR OF STAMPS the two cases above lean on, in one folder, with no end of a run ' +
        'and no switch between them. This is what makes "two rows" a result: those two lines ' +
        'really do sit inside the fold window, and folding still happens. An implementation ' +
        'that repairs the defect by switching folding off passes both cases above and turns six ' +
        'reads in one second back into six rows and a wall of text (DESIGN §1).',
    ).toEqual([[1, 2]]);
    expect(runFeed.view.history.at(-1)?.label, 'and the folded row counts both of them').toBe(
      'Read 2 files',
    );
  });
});
