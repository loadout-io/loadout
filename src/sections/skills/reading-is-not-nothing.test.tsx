/* Trzy stany pustego ekranu, nie dwa: czytam / nie ma nic / nie dało się przeczytać.
 *
 * ZMIERZONA WADA, KTÓRĄ TEN PLIK ZAMYKA (2026-08-31). Magazyn startował z `installed: []`,
 * a odczyt katalogów biegnie dopiero w efekcie po zamontowaniu — więc PIERWSZE, co widział
 * człowiek z dziesięcioma umiejętnościami na dysku, brzmiało „No skills yet.". Druga połowa
 * tej samej wady: kiedy odczyt ODMÓWIŁ, `installed` zostawało puste, więc ekran pokazywał
 * JEDNOCZEŚNIE „Loadout could not read the skills on this machine." i zaproszenie „No skills
 * yet. / Paste a link, or write one yourself." — dwa zdania, z których jedno musi być
 * nieprawdą, obok siebie, w jednym widoku.
 *
 * JAK BRZMIAŁABY SŁABA WERSJA I CO JĄ ODRÓŻNIA. Słaba wersja pyta magazyn o pole („czy
 * `folders` to `reading`") i przechodzi nad ekranem, który tego pola nigdy nie czyta — to jest
 * dokładnie ta klasa wady, dla której stoi niezmiennik 29. Tutaj sądzony jest MARKUP
 * prawdziwego ekranu, a trzeci przypadek trzyma kontrolę przeciw nadgorliwej poprawce:
 * kiedy katalogi naprawdę są puste, zaproszenie ma wrócić.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useSkills } from '../../state/skills';
import { useWorkspaces } from '../../state/workspaces';
import SkillsShelf from './shelf';
import * as io from './io';

vi.mock('./io', () => ({
  readLink: vi.fn(),
  authorSkill: vi.fn(),
  askAnAgent: vi.fn(),
  stopWriting: vi.fn(),
  install: vi.fn(),
  listSkills: vi.fn(),
  remove: vi.fn(),
}));

const listSkills = vi.mocked(io.listSkills);

/** Zdanie pustego ekranu i zaproszenie pod nim — oba z `src/sections/skills/shelf.tsx`. */
const NOTHING_YET = 'No skills yet.';
const INVITE = 'Paste a link, or write one yourself.';

/** Zdanie zapasowe odmowy odczytu, z `src/state/skills.ts`. */
const COULD_NOT_READ = 'Loadout could not read the skills on this machine.';

/** Element niosący ten atrybut, cały otwierający znacznik, albo pusty napis. */
function tagWith(markup: string, attribute: string): string {
  const at = markup.indexOf(attribute);
  if (at < 0) return '';
  const opens = markup.lastIndexOf('<', at);
  const closes = markup.indexOf('>', at);
  return opens < 0 || closes < 0 ? '' : markup.slice(opens, closes + 1);
}

function screen(): string {
  return renderToStaticMarkup(<SkillsShelf store={useSkills} />);
}

const BLANK = useSkills.getState();

beforeEach(() => {
  useSkills.setState(BLANK, true);
  useWorkspaces.setState({ all: [], activeId: null, said: null });
  vi.resetAllMocks();
  listSkills.mockResolvedValue([]);
});

describe('the empty skills screen tells reading, nothing and unreadable apart', () => {
  it('does not say the machine is bare before anything has looked at it', () => {
    /* Magazyn dokładnie taki, z jakim wstaje okno: odczyt jeszcze nie wrócił, bo biegnie
     * w efekcie po zamontowaniu. */
    const markup = screen();

    expect(
      markup,
      'the very first thing a person with ten skills on disk reads about their machine is that ' +
        'it holds none. The list is empty because nobody has looked yet, and "nobody looked" ' +
        'and "there is nothing" are two different statements',
    ).not.toContain(NOTHING_YET);
    expect(
      markup,
      'and the screen has to say what it is doing instead, or an empty rectangle reads as a ' +
        'section that failed to load',
    ).toContain('Reading the folders');
    expect(
      tagWith(markup, 'data-reading'),
      'the sentence says it once; the moving dots say it is still going. Without them a person ' +
        'cannot tell a slow read from a screen that stopped (DESIGN §7, the .thinking primitive)',
    ).not.toBe('');
  });

  it('invites once the folders really answered with nothing', async () => {
    /* KONTROLA PRZECIW NADGORLIWEJ POPRAWCE. Bez niej „nie mów, że pusto" przechodzi na
     * ekranie, który nie mówi już nigdy nic i nie ma jak zacząć. */
    await useSkills.getState().load();

    const markup = screen();
    expect(
      markup,
      'the folders answered and they hold nothing, so the empty screen is an invitation again ' +
        '(DESIGN §6)',
    ).toContain(NOTHING_YET);
    expect(markup, 'and the invitation says what to do next').toContain(INVITE);
    expect(
      markup,
      'and the sentence about reading is gone, because the reading is over',
    ).not.toContain('Reading the folders');
  });

  it('never stands the refusal and the invitation side by side', async () => {
    listSkills.mockRejectedValue(new Error(COULD_NOT_READ));

    await useSkills.getState().load();
    const markup = screen();

    expect(
      markup,
      'the refusal never reached the screen, so everything below would be a statement about ' +
        'a screen that says nothing at all',
    ).toContain(COULD_NOT_READ);
    expect(
      markup,
      'the screen says it could not read the folders AND that there is nothing in them, at the ' +
        'same time, one under the other. One of those two is false and a person has no way to ' +
        'tell which',
    ).not.toContain(NOTHING_YET);
    expect(
      markup,
      'and the invitation under it goes as well: an offer to paste a link into a section that ' +
        'just said it cannot read its own folders is an offer to write into the dark',
    ).not.toContain(INVITE);
    expect(
      markup,
      'and it must not say it is still reading either, because it is not: three states, and ' +
        'the third one has ended',
    ).not.toContain('Reading the folders');
  });
});
