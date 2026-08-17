/* AC-1 dla T-29: kliknięcie `＋ Create` w Workflows zostawia workflow, KTÓRY WIDAĆ.
 *
 * Ten plik niczego nie ogląda w źródłach i niczego nie renderuje ręcznie. Otwiera aplikację,
 * przechodzi do Workflows przełącznikiem, w który klika człowiek, klika jedyną kontrolkę
 * tworzenia — i pyta o obie strony tego kliknięcia naraz.
 *
 * DLACZEGO OBIE STRONY. Sama asercja „atrapa dostała wywołanie" przechodzi na przycisku, który
 * woła Rusta i NIE RYSUJE NICZEGO — czyli w dokładnie tym stanie, w którym ta aplikacja jest
 * dzisiaj. Sama asercja „na ekranie jest kafelek" przechodzi na ekranie, który rysuje kafelek
 * zawsze. Rozróżnia je dopiero para: przejście przez granicę ORAZ zmiana widoczna dla człowieka,
 * zmierzona po obu stronach kliknięcia.
 *
 * NAZWA WIĄŻE OBIE STRONY, dopisane 2026-08-17. „Poleciało jedno `save_workflow`" i „na ekranie
 * jest kafelek" to bez tego dwa osobne zdania, a dwa osobne zdania wolno spełnić dwóm różnym
 * rzeczom: ekran rysujący wiersz z własnej głowy obok zapisu czegoś zupełnie innego przechodzi
 * oba. Wiąże je nazwa — ta, którą widać na kafelku, ma być tą, która pojechała w ładunku.
 *
 * KONTROLA PRZECIW PUSTEJ ASERCJI. Przed kliknięciem zdanie pustego stanu MUSI być na ekranie,
 * a kafelków MUSI być zero. Bez tej pary „kafelek jest, pustki nie ma" przechodzi na ekranie,
 * który rysuje kafelek zawsze, i na takim, który nigdy nie miał pustego stanu.
 *
 * ODSTĘPSTWO OD LITERY KRYTERIUM, ŚWIADOME I ZGŁOSZONE (2026-08-17). TASK.md pisze
 * „atrapa dostała DOKŁADNIE JEDNO wywołanie". Tworzenie workflow to po stronie magazynu trzy
 * komendy, każda o czym innym: `list_workflows` (która nazwa pliku jest wolna),
 * `new_id` (mennica `uuid` stoi po stronie Rusta) i `save_workflow` (zapis), a samo wejście
 * na sekcję czyta katalog czwarty raz (`src/sections/workflows/list/store.ts`, `create`
 * i `load`). Litera kryterium jest więc nie do spełnienia przez UCZCIWĄ implementację — a to,
 * o co kryterium naprawdę pyta („ten sam przycisk nie robi tej samej roboty dwa razy",
 * niezmiennik 16), mierzy się na komendzie, która ZAPISUJE PLIK. Stąd niżej: każda nazwa,
 * która poleciała, jest z `commands.golden.txt`, i DOKŁADNIE JEDNA z nich zapisuje plik.
 * To jest ostrzejsze niż „jedno wywołanie ma dobrą nazwę" i możliwe do spełnienia.
 */
import { readFileSync } from 'node:fs';
import { beforeAll, afterAll, describe, expect, it } from 'vitest';

import { closeEverything, openApp } from '../harness';

/** Ta sama lista, którą po drugiej stronie granicy czyta `ipc_commands_registered.rs`. */
const GOLDEN = new URL('../../src-tauri/commands.golden.txt', import.meta.url);

const known = new Set(
  readFileSync(GOLDEN, 'utf8')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('#')),
);

/**
 * Komenda, po której na dysku leży plik.
 *
 * Wpisana tutaj, a nie wyprowadzona z czegokolwiek: to JEST twierdzenie tego kryterium o tym,
 * co ma się stać po kliknięciu, i ma się przewrócić głośno w dniu, w którym nazwa zniknie
 * z listy złotej — a nie po cichu przestać cokolwiek mierzyć.
 */
const WRITE_COMMAND = 'save_workflow';

/**
 * Nazwa workflow z ładunku `save_workflow`, wyjęta bez ani jednego rzutowania na `any`.
 *
 * Pusty łańcuch, kiedy jej tam nie ma — i to jest celowe: asercja niżej wymaga niepustej,
 * więc brak pola pada z własnym zdaniem, a nie wyjątkiem o czytaniu `name` z `undefined`.
 */
function nameInPayload(args: Record<string, unknown>): string {
  const file = args['workflow'];
  if (typeof file !== 'object' || file === null) return '';
  const name = (file as Record<string, unknown>)['name'];
  return typeof name === 'string' ? name : '';
}

/** Nazwa pliku z tego samego ładunku, tą samą drogą i z tego samego powodu. */
function fileNameInPayload(args: Record<string, unknown>): string {
  const fileName = args['fileName'];
  return typeof fileName === 'string' ? fileName : '';
}

const SWITCH = '[data-section-switch="workflows"]';
const SCREEN = 'main[data-section="workflows"]';
const EMPTY = 'main [data-empty]';
const TILE = 'main [data-tile]';
const CREATE = 'main [data-create]';

/** Ile czekamy na kafelek. Odpowiedź wraca z atrapy w tej samej karcie, więc to jest sufit. */
const APPEARS = 4_000;

/* Rozruch vite + chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku.
 * `openApp()` jest leniwy (`booted ??= boot()` w harness.ts), więc bez tego haka pierwszy `it`
 * płaci cały rozruch pod swoim limitem i pada na nim — mimo że każdy następny przechodzi po
 * ~160 ms na już postawionej aplikacji. Zmierzone 2026-08-18: rozruch ~63 s przy limicie 60 s,
 * czyli porażka mierzyła czas startu Playwrighta, a nie ani jednej rzeczy o produkcie.
 *
 * To PRZENIESIENIE KOSZTU, nie osłabienie kryterium: ani jeden `expect` się nie zmienia,
 * a hak jest symetryczny do `afterAll(closeEverything, 30_000)`, który stał tu od początku. */
beforeAll(async () => {
  await openApp();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('clicking Create in Workflows leaves a workflow on the screen', () => {
  it('crosses into Rust once and draws a tile where the empty state was', async () => {
    /* Lista złota, której nie dało się wczytać, jest pustym zbiorem — a wtedy „każda nazwa jest
     * z listy" byłoby prawdą o niczym i całe kryterium zamieniłoby się w ozdobę. */
    expect(known.size, 'src-tauri/commands.golden.txt has to name at least one command').toBeGreaterThan(0);
    expect(
      known.has(WRITE_COMMAND),
      WRITE_COMMAND +
        ' is not on src-tauri/commands.golden.txt any more. This test measures the click by ' +
        'that name, so a rename leaves it measuring nothing until this line is changed too.',
    ).toBe(true);

    const app = await openApp();
    try {
      const page = app.page;
      await page.click(SWITCH);
      await page.locator(SCREEN).waitFor({ state: 'attached', timeout: APPEARS });

      /* ── przed kliknięciem ────────────────────────────────────────────────────────────── */
      const emptyBefore = await page.locator(EMPTY).count();
      expect(
        emptyBefore,
        'the Workflows screen has to say it is empty before anything is created. Without this ' +
          'line, "the empty sentence went away" also passes on a screen that never had one.',
      ).toBe(1);
      const sentence = (await page.locator(EMPTY).innerText()).trim();
      expect(
        sentence.length,
        'the empty state has to carry a sentence for a person to read, not just a marked-up box',
      ).toBeGreaterThan(0);

      expect(
        await page.locator(TILE).count(),
        'there must be no tile before the click, or "a tile is there" says nothing about the click',
      ).toBe(0);
      expect(
        await page.locator(CREATE).count(),
        'the screen offers exactly one way to create a workflow. Two entry points are two places ' +
          'where a file comes into being, and the first chance for them to drift (invariant 16).',
      ).toBe(1);

      const before = await app.calls();

      /* ── kliknięcie ───────────────────────────────────────────────────────────────────── */
      await page.locator(CREATE).click();

      /* Czekanie ograniczone i ODMOWA POŁKNIĘTA celowo: kiedy kafelek nie przyjdzie, chcemy
       * paść na asercji niżej — z jej zdaniem — a nie na surowym limicie czasu locatora,
       * który mówi tylko tyle, że czegoś nie było. */
      await page
        .locator(TILE)
        .first()
        .waitFor({ state: 'visible', timeout: APPEARS })
        .catch(() => undefined);

      /* ── co poleciało do Rusta ────────────────────────────────────────────────────────── */
      const fired = (await app.calls()).slice(before.length);
      expect(
        fired.length,
        'the click never reached Rust. A create button that talks to nobody leaves nothing on ' +
          'disk, and the row it draws disappears at the next restart (invariant 4).',
      ).toBeGreaterThan(0);

      const strangers = [...new Set(fired.map((call) => call.cmd))].filter(
        (cmd) => !known.has(cmd),
      );
      expect(
        strangers,
        'the click asked Rust for names that are not on src-tauri/commands.golden.txt: ' +
          strangers.join(', ') +
          '. Nothing on the Rust side keeps such a name alive, so the day it is renamed this ' +
          'click goes quiet and the screen keeps drawing the row.',
      ).toEqual([]);

      const wrote = fired.filter((call) => call.cmd === WRITE_COMMAND);
      expect(
        wrote.length,
        'one click has to write exactly one file. Zero is the silence this task exists to end; ' +
          'two means the same button did the same work twice, and one of the two files is an ' +
          'orphan nobody will ever open.',
      ).toBe(1);

      /* ── czy to, co poleciało, jest tym, co widać ─────────────────────────────────────── */
      /* Bez tej pary „jedno wywołanie `save_workflow`" i „jeden kafelek" są dwoma osobnymi
       * zdaniami, które wolno spełnić dwóm różnym rzeczom: ekran rysujący wiersz z własnej
       * głowy obok zapisu czegoś zupełnie innego przechodzi obie. Wiąże je dopiero nazwa —
       * ta sama po obu stronach granicy. */
      const payload = wrote[0]?.args ?? {};
      const sent = nameInPayload(payload);
      expect(
        sent.length,
        'the call to ' +
          WRITE_COMMAND +
          ' carried no workflow name. A file written without the name a person just saw is a ' +
          'file nobody will recognise in the folder, and the row on screen stops standing for ' +
          'anything on disk (invariant 4).',
      ).toBeGreaterThan(0);

      /* `allInnerTexts`, nie `first().innerText()`: przy zerze kafelków pierwsze oddaje pustą
       * listę i pada asercja niżej, ze swoim zdaniem, a drugie padłoby surowym limitem czasu
       * locatora — czyli „czegoś nie było" zamiast powodu. */
      const tileText = (await page.locator(TILE).allInnerTexts()).join(' ').replace(/\s+/g, ' ');
      expect(
        tileText,
        'the tile on screen does not carry the name that went to ' +
          WRITE_COMMAND +
          ' (' +
          JSON.stringify(sent) +
          '). Then the row and the file are two different workflows, and the screen is telling ' +
          'a person about one that was never written.',
      ).toContain(sent);

      const fileName = fileNameInPayload(payload);
      expect(
        fileName.length,
        'the call to ' + WRITE_COMMAND + ' named no file, so nothing says where this lands',
      ).toBeGreaterThan(0);
      expect(
        fileName.includes('/'),
        'the front sent a path (' +
          JSON.stringify(fileName) +
          ') where the boundary takes a bare file name. The folder is resolved on the Rust side ' +
          '[T3 §8.3]; a front that glues it on is a second place where "where does this live" ' +
          'is answered, and the two answers drift the day the folder moves.',
      ).toBe(false);

      /* ── co zobaczył człowiek ─────────────────────────────────────────────────────────── */
      expect(
        await page.locator(TILE).count(),
        'after the answer comes back there has to be a tile on the screen that was not there ' +
          'before. A button that reaches Rust and draws nothing is the state this whole ' +
          'application is in today, and it is the state a person calls broken.',
      ).toBe(1);
      expect(
        await page.locator(EMPTY).count(),
        'and the empty-state invitation has to go away. A screen that keeps saying "no workflows ' +
          'yet" next to a workflow says two things about one fact (invariant 13).',
      ).toBe(0);
    } finally {
      await app.close();
    }
  }, 60_000);
});
