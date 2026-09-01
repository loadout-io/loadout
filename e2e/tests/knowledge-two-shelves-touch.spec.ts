/* Dwie półki sekcji Knowledge stoją OBOK SIEBIE i dotykają się — mierzone w przeglądarce.
 *
 * PO CO OSOBNY PLIK OBOK KRYTERIUM STATYCZNEGO. `renderToStaticMarkup` widzi kolejność
 * w dokumencie i nic poza nią: półka pod półką i półka obok półki mają w markupie ten sam
 * porządek. Różnicę widać wyłącznie w układzie, a układ liczy chromium.
 *
 * ZMIERZONA WADA (2026-08-31, zrzut przy 1512×950). Treść całej sekcji siedziała w kolumnie
 * 640 px przy lewej krawędzi, a prawa połowa ciała ekranu była pusta na całą wysokość. Obie
 * półki stały jedna POD drugą, więc żeby zobaczyć drugą, trzeba było przewinąć — a różnica
 * między nimi jest jedyną rzeczą, którą człowiek musi w tej sekcji zrozumieć, i nie da się jej
 * przeczytać z rzeczy, których nie widać naraz.
 *
 * SŁABA WERSJA: sprawdzić, że w dokumencie jest pojemnik z dwiema kolumnami. Przechodzi na
 * siatce, której druga kolumna jest pusta, i na siatce zwiniętej z powrotem do jednej kolumny
 * regułą, której nikt nie zauważył. Tutaj pytamy o PROSTOKĄTY, które chromium naprawdę
 * policzyło: pasy pionowe muszą się pokrywać, pasy poziome nie, a szczelina między nimi ma być
 * szczeliną, nie ekranem.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { RunningApp, TauriReply } from '../harness';
import { closeEverything, openApp } from '../harness';

const FOLDER = '/Users/somebody/Projects/knowledge-shelves';
const WORKSPACE = { id: FOLDER, name: 'Shelves', folder: FOLDER };
const SCREEN = 'main[data-section="knowledge"]';
const READY = 8_000;

/** Ile razy ta sama odpowiedź ma stać w kolejce — odczyt biegnie przy każdym przełączeniu. */
function copies<T>(value: T, count = 8): readonly { readonly value: T }[] {
  return Array.from({ length: count }, () => ({ value }));
}

const NOTE = {
  place: 'project',
  id: 'always-on',
  title: 'One command per shell call',
  rule: 'Put one command in each shell call, never three joined by semicolons.',
  because: 'Thirteen refusals in a single phase came from joined commands.',
  status: 'in-use',
  scope: 'everywhere',
  agent: null,
  project: null,
  from: null,
  length: 69,
  occurrences: 4,
  modified: '2026-08-30T17:40:00Z',
};

const SKILL = {
  name: 'pdf',
  fromTheInternet: false,
  summary: 'Reads a PDF and pulls out its text',
};

function scene(): Readonly<Record<string, readonly TauriReply[]>> {
  return {
    list_workspaces: copies([WORKSPACE]),
    list_notes: copies([NOTE]),
    list_handoffs: copies([]),
    list_skills: copies([SKILL]),
    list_agents: copies([]),
  };
}

let app: RunningApp;

/** Prostokąt, który chromium naprawdę policzył dla tej strefy. */
async function boxOf(selector: string): Promise<{ x: number; y: number; w: number; h: number }> {
  const found = await app.page.locator(selector).boundingBox();
  if (found === null) {
    throw new Error('nothing was laid out for ' + selector + ', so there is no shelf to measure');
  }
  return { x: found.x, y: found.y, w: found.width, h: found.height };
}

beforeAll(async () => {
  app = await openApp({ replies: scene() });
  await app.page.setViewportSize({ width: 1512, height: 950 });
  await app.page.locator('[data-section-switch="knowledge"]').click();
  await app.page.locator(SCREEN).waitFor({ state: 'attached', timeout: READY });
  /* Obie strefy muszą już stać, zanim cokolwiek mierzymy: odczyt katalogów biegnie w efekcie
     po zamontowaniu, więc przed odpowiedzią na ekranie jest zaproszenie, a nie półki. */
  await app.page.locator('[data-zone="in-use"]').waitFor({ state: 'visible', timeout: READY });
  await app.page.locator('[data-zone="skills"]').waitFor({ state: 'visible', timeout: READY });
}, 180_000);

afterAll(async () => {
  await closeEverything();
});

describe('the two shelves of the knowledge screen stand side by side and touch', () => {
  it('lays the shelf of notes and the shelf of skills in one horizontal band', async () => {
    const notes = await boxOf('[data-zone="in-use"]');
    const skills = await boxOf('[data-zone="skills"]');

    expect(
      notes.h,
      'the shelf of notes was laid out with no height at all, so every measurement below is ' +
        'about a box nobody can see',
    ).toBeGreaterThan(0);
    expect(skills.h, 'and so was the shelf of skills').toBeGreaterThan(0);

    expect(
      skills.x,
      'skills stand to the RIGHT of notes, not underneath them. One under the other means a ' +
        'person scrolls past the first to find the second, and the difference between the two ' +
        'is the one thing this section exists to say — it cannot be read from things that are ' +
        'never on screen together',
    ).toBeGreaterThan(notes.x + notes.w - 1);

    const bandsOverlap = notes.y < skills.y + skills.h && skills.y < notes.y + notes.h;
    expect(
      bandsOverlap,
      'and their horizontal bands overlap, so both headings are readable without moving. ' +
        'notes: ' +
        JSON.stringify(notes) +
        ' skills: ' +
        JSON.stringify(skills),
    ).toBe(true);
  });

  it('leaves a gap between them, not a screen', async () => {
    const notes = await boxOf('[data-zone="in-use"]');
    const skills = await boxOf('[data-zone="skills"]');
    const gap = skills.x - (notes.x + notes.w);

    expect(
      gap,
      'the two shelves touch: they sit on one dividing line, so the gap between them is ' +
        'breathing room and nothing more. Measured gap: ' +
        String(Math.round(gap)) +
        'px',
    ).toBeLessThan(80);
    expect(gap, 'and they do not run into each other either').toBeGreaterThanOrEqual(0);
  });

  it('uses the width of the window instead of leaving half of it dark', async () => {
    const body = await boxOf(SCREEN + ' .screen-body');
    const notes = await boxOf('[data-zone="in-use"]');
    const skills = await boxOf('[data-zone="skills"]');
    const used = skills.x + skills.w - notes.x;

    expect(
      used / body.w,
      'the two shelves together fill the body of the screen. Before this wave everything lived ' +
        'in a 640px column against the left edge and the right half of the window was black ' +
        'from the header to the footer — emptiness under the last line is a defect, not room ' +
        'to breathe. Used ' +
        String(Math.round(used)) +
        'px of ' +
        String(Math.round(body.w)),
    ).toBeGreaterThan(0.8);
  });
});
