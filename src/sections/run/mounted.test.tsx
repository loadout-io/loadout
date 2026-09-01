/* Kryterium 8: widok pracy naprawdę pojawia się w oknie, a nie tylko w swoim teście.
 *
 * Mechanizm montowania sekcji dowozi T-25 i dowodzi go przez WSTRZYKNIĘCIE mapy ekranów —
 * uczciwie zapisując, że dowodu z dysku nie ma wtedy czym zrobić, bo żadna sekcja nie ma
 * jeszcze swojego `index.tsx`. Tutaj jest: `run` jest pierwszą sekcją w kolejności budowy,
 * więc to jest pierwszy moment, w którym da się zapytać o odkrywanie prawdziwych plików.
 * Pozostałe cztery sekcje dostają ten dowód za darmo — ta sama ścieżka, ten sam wzorzec.
 *
 * Dwie słabe wersje tego sprawdzenia i dlaczego ich tu nie ma:
 *
 *   `expect(html).toContain('data-section="run"')` — przechodzi na powłoce, która nie montuje
 *   niczego. `data-section` stawia `<main>`, nie ekran, więc to zdanie jest prawdziwe od
 *   pierwszego dnia i będzie prawdziwe także wtedy, gdy wzorzec globa nie trafi w żaden plik.
 *
 *   `<App section="run" screens={{ run: … }} />` — wtedy nie sprawdzasz odkrywania, tylko
 *   własną atrapę. Dlatego mapa NIE jest tu podawana ani razu.
 *
 * Dyskryminuje BRAK zdania pustego ekranu w tym samym dokumencie. To jest dokładnie ten obraz,
 * który wychodzi, kiedy wzorzec globa nie trafia w plik: zielono, cicho i pusto.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import { useRun } from '../../state/run';
import { sectionEntry } from '../../ui/sections';

/**
 * Trzy oznaczone regiony, z których składa się widok pracy: pasek loadoutu, historia i obraz
 * planu [DESIGN §1 i §2]. Wypisane tutaj, bo to jest kontrakt między tym zadaniem a powłoką —
 * czytanie ich z komponentu byłoby pytaniem komponentu o zdanie na własny temat.
 *
 * 2026-08-31 — TRZECI REGION BYŁ DO DZIŚ BLOKIEM POD STRUMIENIEM (`data-now`) i zniknął razem
 * z nim: mówił słowo w słowo to samo, co wiersz strumienia nad nim i kafelek w kolumnie obok,
 * przy limicie jednego żywego regionu na fakt (niezmiennik 13). Pytanie zostało to samo — czy
 * powłoka naprawdę montuje widok pracy — zmienił się region, po którym je zadajemy.
 */
const REGIONS = ['data-strip', 'data-feed', 'data-plan-column'];

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/**
 * Jeden krok w magazynie biegu — i to NIE jest to wstrzyknięcie, którego ten plik odmawia.
 *
 * Odmawia mapy ekranów, bo mapa podana z testu zamienia pytanie „czy powłoka znajduje plik
 * sekcji" w pytanie o własną atrapę. Krok biegu nie odpowiada na nic z tego: powłoka dalej ma
 * znaleźć `src/sections/run/index.tsx` sama i sama go zamontować.
 *
 * DLACZEGO OD 2026-08-31 JEST POTRZEBNY. Ekran Run bez ani jednego kroku, agenta i workflow
 * rysuje pierwsze otwarcie — jedną taflę powitania, bez kolumny kroków (`./first-run.tsx`,
 * `welcomeIsTheWholeScreen`). Trzeci region z listy niżej istnieje wtedy i tylko wtedy, gdy
 * jest co w nim postawić, więc scena bez kroku pytałaby o region, którego ten ekran świadomie
 * nie ma — a to jest pytanie o pierwsze otwarcie, nie o montowanie sekcji.
 */
useRun.setState({ steps: [{ id: 's_build', name: 'Build', state: 'running' }] });

function markup(): string {
  return renderToStaticMarkup(<App section="run" />);
}

describe('the run view shows up in the window, not only in its own test', () => {
  for (const region of REGIONS) {
    it('draws the ' + region + ' region with no screen map handed in', () => {
      expect(
        occurrences(markup(), region),
        'asking the shell for the run section, with nothing injected, has to put the run view ' +
          'in the tree — found by the path src/sections/run/index.tsx and nothing else. Exactly ' +
          'once: a region drawn twice is a second live place for one fact (invariant 13)',
      ).toBe(1);
    });
  }

  it('leaves the empty sentence for run out of that same document', () => {
    const empty = sectionEntry('run').empty;
    expect(
      empty.length,
      'the sentence has to be a real one, otherwise "the document does not contain it" is free',
    ).toBeGreaterThan(0);
    expect(
      markup().includes(empty),
      'a section that has a screen shows THAT screen, never the invitation it falls back to. ' +
        'The invitation still standing there is what a glob pattern that matches no file looks ' +
        'like from the outside, and it looks exactly like a young app',
    ).toBe(false);
  });
});
