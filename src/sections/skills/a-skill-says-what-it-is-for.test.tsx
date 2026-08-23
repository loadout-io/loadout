/* Kafelek umiejętności mówi, PO CO ona jest — a kiedy nie ma czego powiedzieć, mówi i to.
 *
 * # Skarga
 *
 * Właściciel, 2026-08-23: „przebuduj ten widok skili bo to jak gówno wygląda cały ten UI, do
 * pełnej przebudowy".
 *
 * # Co było zepsute
 *
 * `InstalledWire` niósł NAZWĘ KATALOGU i znacznik pochodzenia — nic poza tym. Sekcja była więc
 * siatką gołych napisów, po której nie dało się poznać, co którakolwiek umiejętność robi; żeby
 * się dowiedzieć, trzeba było otworzyć `SKILL.md` poza aplikacją. Makieta ma zdanie opisu
 * w kafelku od początku (`docs/mockup/index.html`, panel `skills`), a komentarz w kodzie
 * przyznawał wprost, dlaczego go nie było: „`InstalledWire` nie niesie ani `summary`, ani
 * `description` — zdanie dopisane tutaj byłoby zmyślone. Zgłoszone człowiekowi."
 *
 * # Dlaczego DWA punkty, a nie jeden
 *
 * `expect(markup).toContain(SUMMARY)` przechodzi ekran, który przy braku opisu zostawia pustą
 * dziurę — a pusty prostokąt w miejscu zdania czyta się jak awaria wczytywania, nie jak plik
 * bez opisu. Człowiek wraca wtedy na sekcję i czeka, aż „się doładuje". Drugi punkt pilnuje, że
 * brak opisu też ma swoje zdanie, i że mówi ono, GDZIE ten opis dopisać.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import { useSkills } from '../../state/skills';
import SkillsScreen from './index';

/** Zdanie z `SKILL.md`. Brzmi inaczej niż nazwa, bo o to właśnie pytamy. */
const SAYS = 'Turns a screenshot into a written description of the layout';

const TOLD = { name: 'read-a-mockup', fromTheInternet: false, summary: SAYS };
const SILENT = { name: 'old-helper', fromTheInternet: false, summary: '' };

function markup(): string {
  return renderToStaticMarkup(<SkillsScreen store={useSkills} />);
}

describe('a skill tile says what the skill is for', () => {
  beforeEach(() => {
    useSkills.setState({ installed: [], pending: null, adding: null });
  });

  it('prints the sentence the skill file gives it', () => {
    useSkills.setState({ installed: [TOLD] });
    const html = markup();

    expect(
      html.includes(TOLD.name),
      'the tile is not even on the screen, so the point below would be about nothing',
    ).toBe(true);
    expect(
      html.includes(SAYS),
      'the tile shows a folder name and nothing else. That is the whole screen: a grid of bare ' +
        'words where the only way to learn what any of them does is to open a file outside ' +
        'this app. It rendered: ' +
        JSON.stringify(html.slice(0, 300)),
    ).toBe(true);
  });

  it('says so out loud when the skill file says nothing', () => {
    useSkills.setState({ installed: [SILENT] });
    const html = markup();
    const tile = /<li[^>]*data-skill="old-helper"[\s\S]*?<\/li>/.exec(html)?.[0] ?? '';

    expect(tile, 'the tile for a skill with no description was not drawn at all').not.toBe('');
    expect(
      /<p[^>]*>\s*<\/p>/.test(tile),
      'a skill with no description leaves an empty paragraph where the sentence goes. An empty ' +
        'box reads as loading that never finished, and the person waits for a screen that is ' +
        'already done. It rendered: ' +
        JSON.stringify(tile),
    ).toBe(false);
    expect(
      /SKILL\.md/.test(tile),
      'and the sentence has to name the file the description belongs in, because that is the ' +
        'one thing the person can go and change',
    ).toBe(true);
  });
});
