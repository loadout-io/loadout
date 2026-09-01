/* „Remove" pyta, zanim zdejmie pliki, i zdejmuje je STAMTĄD, CO NAPISAŁ NA EKRANIE.
 *
 * ZMIERZONA WADA, KTÓRĄ TEN PLIK ZAMYKA (2026-08-31). Do dziś `remove` w `src/state/skills.ts`
 * czytało `get().landing` — wybór z grupy radiowej, która renderuje się WYŁĄCZNIE wewnątrz karty
 * czekającego importu („Available in"). Bez czekającego importu tej kontrolki na ekranie NIE MA,
 * a wartość zostaje taka, jaką zostawił ostatni import: człowiek, który raz zapisał umiejętność
 * „w tym projekcie", od tej chwili każdym „Remove" celował w katalog WEWNĄTRZ swojego
 * repozytorium, patrząc na wiersz umiejętności leżącej w katalogu domowym. Po drugiej stronie
 * granicy stoi `fs::remove_dir_all` (`src-tauri/src/skills/place.rs`) — bez pytania i bez
 * cofnięcia.
 *
 * JAK BRZMIAŁABY SŁABA WERSJA I CO JĄ ODRÓŻNIA. Słaba wersja to `expect(io.remove).toHaveBeenCalled()`
 * — przechodzi na dokładnie tym ekranie, który tu naprawiamy, bo wywołanie było i jest. Odróżnia
 * je para: (1) licznik zerowy, dopóki pytanie nie stoi na ekranie — zgoda jest warunkiem
 * WYWOŁANIA, nie stanem widoku (nagłówek `src/state/skills.ts`); (2) porównanie CAŁEGO ładunku
 * przy ustawieniu `landing` przestawionym na drugą wartość — magazyn, który dalej czyta wybór
 * spoza ekranu, oddaje wtedy inne miejsce i przewraca się na tej jednej wartości.
 *
 * Zdanie pytania sądzimy tam, gdzie czyta je człowiek: w markupie prawdziwego ekranu
 * (`renderToStaticMarkup`, niezmiennik 29), a nie w wartości zwróconej przez funkcję.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { InstalledSkill, Landing } from '../../state/skills';
import { useSkills } from '../../state/skills';
import type { Workspace } from '../../state/workspaces';
import { useWorkspaces } from '../../state/workspaces';
import SkillsShelf from './shelf';
import * as io from './io';

/* Atrapa pokrywa CAŁĄ krawędź, także funkcje, których ten plik nie woła wprost: magazyn po
 * udanym zdjęciu czyta katalogi jeszcze raz i wołałby wtedy `undefined`, a test przewracałby
 * się na `TypeError` zamiast powiedzieć, co jest nie tak. */
vi.mock('./io', () => ({
  readLink: vi.fn(),
  authorSkill: vi.fn(),
  askAnAgent: vi.fn(),
  stopWriting: vi.fn(),
  install: vi.fn(),
  listSkills: vi.fn(),
  remove: vi.fn(),
}));

const removeFromDisk = vi.mocked(io.remove);
const listSkills = vi.mocked(io.listSkills);

const PDF: InstalledSkill = { name: 'pdf', fromTheInternet: true, summary: 'Reads a PDF' };

/** Zakres, w którym człowiek pracuje. Ścieżka jest fikcyjna i nigdy nie dotyka dysku. */
const OPEN_PROJECT: Workspace = {
  id: '/Users/somebody/Projects/Loadout',
  name: 'Loadout',
  folder: '/Users/somebody/Projects/Loadout',
};

/** Miejsce, które ekran nazywa człowiekowi przy wierszu listy. */
const ON_THIS_MACHINE: Landing = 'everywhere';

/** Drugie miejsce — osiągalne dopiero wtedy, gdy jakiś projekt jest otwarty. */
const IN_THE_PROJECT: Landing = 'this-project';

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Kawałek markupu od znacznika tej umiejętności do znacznika następnej. */
function rowFor(markup: string, name: string): string {
  const start = markup.indexOf('data-skill="' + name + '"');
  if (start < 0) return '';
  const next = markup.slice(start + 1).search(/data-skill="/);
  return next < 0 ? markup.slice(start) : markup.slice(start, start + 1 + next);
}

/** Sam tekst kawałka markupu, bez znaczników i bez nadmiarowych odstępów. */
function words(part: string): string {
  return part
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function screen(): string {
  return renderToStaticMarkup(<SkillsShelf store={useSkills} />);
}

const BLANK = useSkills.getState();

beforeEach(() => {
  /* Oba magazyny są singletonami, więc zasianie jednego testu dojechałoby do następnego. */
  useSkills.setState(BLANK, true);
  useWorkspaces.setState({ all: [], activeId: null, said: null });
  vi.resetAllMocks();
  removeFromDisk.mockResolvedValue(undefined);
  listSkills.mockResolvedValue([]);
});

describe('taking a skill back out asks first and strikes where the screen said', () => {
  it('takes nothing off the disk while no question about it stands on the screen', async () => {
    useSkills.setState({ installed: [PDF], folders: 'read' });

    /* Wywołanie WPROST, z pominięciem widoku — bo o to właśnie chodzi. Wyłączony albo
     * niewyrenderowany przycisk jest sugestią: zostaje klawiatura, zostaje skrót i zostaje
     * druga ścieżka w interfejsie. Zgoda ma być warunkiem WYWOŁANIA. */
    await useSkills.getState().remove(ON_THIS_MACHINE);

    expect(
      removeFromDisk.mock.calls,
      'a skill left the folders the agent apps of this person read and nobody was asked about ' +
        'it. On the far side of this call stands a recursive folder delete with no undo, so ' +
        '"one click" and "gone for good" are the same thing here. It handed over: ' +
        JSON.stringify(removeFromDisk.mock.calls),
    ).toEqual([]);
  });

  it('asks by name and says what exactly goes, in the row a person is looking at', () => {
    useSkills.setState({ installed: [PDF], folders: 'read', removing: PDF.name });

    const said = words(rowFor(screen(), PDF.name));

    expect(
      said,
      'the row of this skill is not on the screen at all, so everything below would be a ' +
        'statement about nothing',
    ).toContain(PDF.name);
    expect(
      said,
      'the question a person reads before the files go has to name the skill. "Are you sure?" ' +
        'names nothing, and this grid holds ten of them. The row said: ' +
        said,
    ).toContain('Remove ' + PDF.name + '?');
    expect(
      said,
      'and it has to say what goes away. A question that does not name the folders is a ' +
        'question about nothing a person can picture. The row said: ' +
        said,
    ).toContain('on this machine');
    expect(
      said,
      'and it has to say that nothing brings it back, because nothing does: the far side is a ' +
        'recursive folder delete. The row said: ' +
        said,
    ).toContain('brings it back');
    expect(
      said,
      'and the way out that changes nothing has to stand next to the one that does, or the ' +
        'only answer to the question is yes',
    ).toContain('Keep it');
  });

  it('strikes the place the question named, never the choice that is off the screen', async () => {
    /* Człowiek zapisał kiedyś umiejętność „w tym projekcie". Karta przeglądu zniknęła razem
     * z grupą radiową, a wybór został w magazynie — i to on rozstrzygał, gdzie uderza Remove. */
    useSkills.setState({
      installed: [PDF],
      pending: null,
      folders: 'read',
      landing: IN_THE_PROJECT,
    });

    const markup = screen();
    expect(
      markup,
      'the row with its way back is not on the screen, so the point below would be about nothing',
    ).toContain('data-remove="' + PDF.name + '"');
    expect(
      markup,
      'the choice that decides where Remove strikes is not in the document at all: it renders ' +
        'inside the card of a waiting import and there is no waiting import. A person cannot ' +
        'read it, cannot change it and is about to lose a folder because of it',
    ).not.toContain('data-landing');

    useSkills.setState({ removing: PDF.name });
    await useSkills.getState().remove(ON_THIS_MACHINE);

    expect(
      removeFromDisk.mock.calls,
      'the delete went somewhere else than the sentence on the screen said. The value that ' +
        'decided it was last touched during an import that is long gone from the screen. It ' +
        'handed over: ' +
        JSON.stringify(removeFromDisk.mock.calls),
    ).toEqual([[PDF.name, ON_THIS_MACHINE, null]]);
  });

  it('offers one place with no project open and both when one is open, each named', () => {
    useSkills.setState({ installed: [PDF], folders: 'read', removing: PDF.name });

    const alone = words(rowFor(screen(), PDF.name));
    expect(
      alone,
      'with no project open there is exactly one place a skill can be, so the question has to ' +
        'offer exactly one — and name it. The row said: ' +
        alone,
    ).toContain('Remove from this machine');
    expect(
      alone,
      'and it must not offer a place that has no folder behind it: with nothing open, "inside ' +
        'the project" resolves to a relative path and the far side refuses. The row said: ' +
        alone,
    ).not.toContain('Remove from this project');

    useWorkspaces.setState({ all: [OPEN_PROJECT], activeId: OPEN_PROJECT.id, said: null });
    const both = rowFor(screen(), PDF.name);

    expect(
      occurrences(both, 'data-goes-from='),
      'with a project open the same name means two different folders, and a person has to say ' +
        'which one goes. One control here is the defect this file is about, wearing a question ' +
        'mark',
    ).toBe(2);
    expect(words(both), 'and the second one has to say where it strikes as well').toContain(
      'Remove from this project',
    );
  });

  it('a refused delete says what Rust said and puts the question away', async () => {
    const SAID = 'pdf is not in any of the folders Loadout writes to.';
    useSkills.setState({ installed: [PDF], folders: 'read', removing: PDF.name });
    removeFromDisk.mockRejectedValue(SAID);

    await useSkills.getState().remove(ON_THIS_MACHINE);

    expect(
      useSkills.getState().message,
      'the sentence from the far side reaches the screen word for word: "it was never there" ' +
        'and "that folder would not let me write" are two different things to go and do',
    ).toBe(SAID);
    expect(
      screen(),
      'and the question is put away, so the next press is a fresh decision rather than a ' +
        'second answer to a question a person already answered',
    ).not.toContain('data-goes-from');
  });
});
