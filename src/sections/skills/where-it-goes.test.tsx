/* Trzecie kryterium T-44: wybór miejsca stoi tam, gdzie zapada decyzja, i jedzie razem
 * z zapisem.
 *
 * CO JEST DZIŚ ZEPSUTE. Cały mechanizm zakresu jest napisany po stronie Rusta i NIEOSIĄGALNY
 * z okna. `src/state/skills.ts` niesie już `landing` i `chooseLanding`, `src/sections/skills/io.ts`
 * przyjmuje już wybór i folder — a `add()` wpisuje w oba argumenty stałe (`'everywhere'`, `null`)
 * i ekran nie ma ani jednej kontrolki, którą dałoby się ten wybór zmienić. Umiejętność ląduje
 * więc zawsze w katalogach domowych człowieka, niezależnie od tego, w którym zakresie pracuje.
 *
 * DLACZEGO TO NIE JEST DRUGORZĘDNE. Ta sekcja jako jedyna w Loadoucie pisze POZA własną
 * bibliotekę: cel to katalogi, do których zaglądają narzędzia agentowe człowieka
 * (`DESTINATION_DIRS` w `src-tauri/src/skills/mod.rs`). Wybór, który nigdzie nie jedzie, jest
 * więc kontrolką bez skutku (niezmiennik 16) dokładnie tam, gdzie skutkiem jest zapis do żywej
 * konfiguracji cudzych narzędzi — a zdanie na ekranie mówiące „w tym projekcie", kiedy plik
 * ląduje u człowieka, jest gorsze niż brak wyboru, bo jest nieprawdą, którą człowiek przeczytał
 * i której uwierzył.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: asercja, że w markupie stoją dwa `<input type="radio">`.
 * Przechodzi na wyborze, który nigdzie nie jedzie — czyli dokładnie na dzisiejszym defekcie,
 * tylko z dorysowaną kontrolką. Rozstrzyga odczyt ARGUMENTÓW z atrapy granicy: co naprawdę
 * pojechało do Rusta, kiedy człowiek nacisnął „Add this skill".
 *
 * FOLDER NIE JEST WPISANY W ASERCJĘ. Bierze się go z `activeWorkspace()` — tej samej funkcji,
 * którą pyta Start biegu (`src/sections/run/launch.ts`). Wartość wpisana tu z palca przechodziłaby
 * na implementacji, która zgaduje katalog roboczy albo czyta zmienną środowiskową, czyli na
 * drugiej odpowiedzi na pytanie „który to projekt" (niezmiennik 13) — a dwie odpowiedzi rozjeżdżają
 * się pierwszego dnia, w którym ktoś przełączy zakres.
 *
 * NAZWA KOMENDY I NAZWY JEJ ARGUMENTÓW TEŻ NIE SĄ TU WPISANE. Nazwa jest czytana
 * z `src-tauri/commands.golden.txt`, zbiór nazw argumentów z `src-tauri/src/ipc.rs`, w tym samym
 * biegu testu. Tauri dopasowuje argumenty PO NAZWIE i deserializuje je, ZANIM wejdzie w ciało
 * komendy, więc klucz, który się nie zgadza, nie daje mniejszego wywołania: daje odrzucone,
 * z odmową w postaci surowego napisu, którego nikt nie widzi.
 *
 * RENDER JEST STATYCZNY (`renderToStaticMarkup`), bo w repo nie ma `jsdom`. Stąd dwie
 * konsekwencje, obie widoczne niżej: wybór musi dać się ZASIAĆ w magazynie, a „naciśnięcie Add"
 * jest wywołaniem akcji magazynu, nie kliknięciem. Pierwsza z nich nie jest ustępstwem na rzecz
 * testu — wybór i tak ma mieszkać w magazynie, bo to z niego bierze się JEDNOCZEŚNIE zaznaczona
 * pozycja, zdanie na ekranie i to, co jedzie na dysk (niezmiennik 13).
 *
 * KONTRAKT NA MARKUP, żeby następny czytelnik nie musiał go zgadywać:
 *   data-pick-where            kontrolka, w której człowiek wybiera miejsce. Jedna na ekran.
 *   data-landing="<wartość>"   jedna pozycja na każdą wartość `Landing`; niosąca `disabled`,
 *                              kiedy nie da się jej wybrać.
 *   data-where-it-goes         element niosący JEDNO zdanie o tym, gdzie to wyląduje.
 * `data-add` i `data-review-card` zostają tam, gdzie są (`review-card.tsx`) — wybór stoi NAD
 * kartą, w ekranie, więc propsy karty się nie zmieniają.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Import, Landing } from '../../state/skills';
import { useSkills } from '../../state/skills';
import type { Workspace } from '../../state/workspaces';
import { activeWorkspace, useWorkspaces } from '../../state/workspaces';
import { FIRST_INVITE } from '../../ui/shell/workspace-switcher';
import { ipcSource, windowSideArguments } from '../ipc-signature';
import SkillsScreen, { WHERE_IT_LANDS } from './index';

/* Atrapa granicy: rozwiązuje się zawsze i zawsze tą samą wartością. Ta droga nie mierzy
 * odpowiedzi Rusta, tylko to, co w jego stronę pojechało. Żadnego żywego Tauri — kryterium,
 * które go wymaga, nie umie być czerwone z właściwego powodu, bo „Failed to launch" jest jednym
 * z podpisów, których warstwa `before` nie liczy jako czerwień. */
const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((..._sent: unknown[]) => Promise.resolve(undefined)),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');

/** Pliki czytamy tak, żeby test padał na asercji o treści, nigdy na otwarciu pliku. */
function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Ta sama lista, którą po drugiej stronie granicy czyta `ipc_commands_registered.rs`. */
const known = new Set(
  fileText(resolve(ROOT, 'src-tauri/commands.golden.txt'))
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('#')),
);

/** `ipc.rs` w całości — jedyne miejsce, w którym stoją nazwy argumentów komend. */
const rust = ipcSource();

/* Dwie wartości `Landing` i ani jednej więcej. Typ jest tu z rozmysłu: dzień, w którym ta unia
 * zmieni kształt, ma zaczerwienić ten plik na kompilacji, a nie po cichu przepuścić asercję
 * o napisie, którego nikt już nie wysyła. */
const EVERYWHERE: Landing = 'everywhere';
const THIS_PROJECT: Landing = 'this-project';

const NAME = 'review-pull-requests';

/* Ani jednego apostrofu i ani jednego cudzysłowu w tekstach, które ekran renderuje: React ucieka
 * `'` na `&#x27;`, więc `toContain` na zdaniu z apostrofem byłoby czerwone także wtedy, gdy ekran
 * pokazuje dokładnie to, co trzeba — czyli mierzyłoby kodowanie, nie zachowanie. */
const REVIEWED: Import = {
  name: NAME,
  summary: 'Use this when somebody asks for a second look at a pull request.',
  reviewed: {
    body: 'Read the change first, then say in one paragraph what to fix.',
    findings: [],
    verdict: 'clean',
  },
  scripts: 0,
  fromTheInternet: true,
};

/* Zakres, w którym człowiek pracuje. Ścieżka jest fikcyjna i nigdy nie dotyka dysku — jedzie
 * wyłącznie do atrapy granicy. */
const OPEN_PROJECT: Workspace = {
  id: '/Users/somebody/Projects/Loadout',
  name: 'Loadout',
  folder: '/Users/somebody/Projects/Loadout',
};

function withAWorkspace(): void {
  useWorkspaces.setState({ all: [OPEN_PROJECT], activeId: OPEN_PROJECT.id, said: null });
}

function withoutAWorkspace(): void {
  useWorkspaces.setState({ all: [], activeId: null, said: null });
}

function screenWith(landing: Landing): string {
  useSkills.setState({ landing });
  return renderToStaticMarkup(<SkillsScreen store={useSkills} />);
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/**
 * Otwierający znacznik elementu niosącego ten atrybut, albo `''`.
 *
 * Pusty napis, a nie wyjątek: wołający ma o niego zapytać JAWNIE i powiedzieć, czego zabrakło.
 * `expect(undefined).not.toContain(…)` przechodzi dla elementu, którego nie ma.
 */
function tagWith(markup: string, attribute: string): string {
  const at = markup.indexOf(attribute);
  if (at < 0) return '';
  const opens = markup.lastIndexOf('<', at);
  const closes = markup.indexOf('>', at);
  return opens < 0 || closes < 0 ? '' : markup.slice(opens, closes + 1);
}

/** Tekst wewnątrz elementu niosącego ten atrybut, bez odstępów po brzegach. */
function textOf(markup: string, attribute: string): string {
  const at = markup.indexOf(attribute);
  if (at < 0) return '';
  const opens = markup.indexOf('>', at);
  const closes = markup.indexOf('<', opens);
  return opens < 0 || closes < 0 ? '' : markup.slice(opens + 1, closes).trim();
}

/**
 * Każda wartość prosta w ładunku, na dowolnym poziomie zagnieżdżenia.
 *
 * Pyta o WARTOŚCI, bo o klucze pyta osobno porównanie z `ipc.rs` niżej: nazw argumentów nie ma
 * prawa być w tym pliku wpisanych z palca, więc nie da się tu powiedzieć „pod kluczem `landing`".
 * Wywołanie, które doszło bez tego, co miało nieść, jest tą samą ciszą, co brak wywołania.
 */
function carriedValues(value: unknown, into: unknown[]): unknown[] {
  if (Array.isArray(value)) {
    for (const item of value as unknown[]) carriedValues(item, into);
  } else if (typeof value === 'object' && value !== null) {
    for (const item of Object.values(value as Record<string, unknown>)) carriedValues(item, into);
  } else if (value !== undefined && value !== null) {
    into.push(value);
  }
  return into;
}

beforeEach(() => {
  /* Oba magazyny są singletonami, więc zasianie ich w jednym teście dojechałoby do następnego.
   * `installed: []` i `adding: null` są tu warunkiem sensu asercji o POŁOŻENIU: wiersz listy
   * niesie własne `data-skill`, a otwarty panel niesie `data-add-panel`, w którym siedzi napis
   * `data-add`. */
  useSkills.setState({
    pending: REVIEWED,
    acknowledged: [],
    message: null,
    installed: [],
    adding: null,
    landing: EVERYWHERE,
  });
  withAWorkspace();
  invoked.mockClear();
});

describe('the choice of where a skill lands stands by the decision and travels with the save', () => {
  it('really read both oracles, so nothing below compares against nothing', () => {
    expect(
      known.size,
      'src-tauri/commands.golden.txt could not be read, so "the window asked for a name off the ' +
        'list" would pass for every name there is.',
    ).toBeGreaterThan(0);
    expect(
      rust,
      'src-tauri/src/ipc.rs could not be read, so the expected set of argument names would come ' +
        'from nowhere and the comparison would pass on two empty lists.',
    ).not.toBe('');
    expect(
      WHERE_IT_LANDS,
      'the section exports no sentence about where a skill lands, so every comparison against ' +
        'it below would be a comparison against an empty string, and an empty string is inside ' +
        'every markup there is.',
    ).not.toBe('');
    expect(
      FIRST_INVITE,
      'the side menu exports no name for the control that adds a workspace, so the sentence ' +
        'this screen has to point at would be checked against nothing.',
    ).not.toBe('');
  });

  it('puts the choice inside the card section and ABOVE the control that carries it out', () => {
    const markup = screenWith(EVERYWHERE);

    /* KONTROLA PRZECIW PUSTEMU EKRANOWI, i bez niej wszystkie porównania położenia niżej są
     * o dokumencie, w którym nie ma nic. */
    expect(
      occurrences(markup, 'data-review-card'),
      'the review card is not in the document, or it is there twice. Everything below compares ' +
        'positions, and positions mean nothing on a screen that shows nothing',
    ).toBe(1);
    expect(
      occurrences(markup, 'data-add'),
      'the screen offers no single way to add this skill, or offers two. The positions compared ' +
        'below only mean something when there is exactly one decision on the page',
    ).toBe(1);

    const sectionAt = markup.indexOf('data-skill="' + NAME + '"');
    expect(
      sectionAt,
      'the section holding the skill waiting for a decision is missing from the document',
    ).toBeGreaterThanOrEqual(0);

    expect(
      occurrences(markup, 'data-pick-where'),
      'there is no control for choosing where this skill goes, or there are two. The whole scope ' +
        'mechanism is written and tested on the Rust side and cannot be reached from the window: ' +
        'the store hard-codes one value and the screen offers no way to change it, so a skill ' +
        'always lands in the home folders whatever the person is working on. Two controls would ' +
        'be two answers to one question (invariant 13), and this is the one screen in Loadout ' +
        'that writes into the folders other agent apps read',
    ).toBe(1);

    const pickAt = markup.indexOf('data-pick-where');
    const addAt = markup.indexOf('data-add');
    expect(
      pickAt,
      'the choice stands OUTSIDE the section holding the card, so it is a setting somewhere on ' +
        'the page rather than part of this decision about this skill',
    ).toBeGreaterThan(sectionAt);
    expect(
      pickAt,
      'the choice stands BELOW the control that carries it out. A choice about where something ' +
        'goes, offered after the button that puts it there, is not a choice — it is the same ' +
        'shape as a warning read after the decision, and this section writes into the folders ' +
        'every later run of the agent apps on this machine will read',
    ).toBeLessThan(addAt);
  });

  it('says where it goes in ONE sentence, and that sentence changes with the choice', () => {
    const everywhere = screenWith(EVERYWHERE);
    const project = screenWith(THIS_PROJECT);

    for (const [which, markup] of [
      ['everywhere', everywhere],
      ['this project', project],
    ] as const) {
      expect(
        occurrences(markup, 'data-where-it-goes'),
        'with "' +
          which +
          '" chosen the screen carries no sentence about where this skill goes, or carries two. ' +
          'One fact, one place (invariant 13): two sentences about one destination are how a ' +
          'person ends up reading the one that is no longer true',
      ).toBe(1);
    }

    expect(
      textOf(everywhere, 'data-where-it-goes'),
      'with "everywhere" chosen the sentence on screen is no longer the one this section exports ' +
        'and src/sections/skills/origin-is-not-a-guess.test.tsx already stands on. That sentence ' +
        'is the only warning a person gets that this write leaves Loadout',
    ).toContain(WHERE_IT_LANDS);

    const said = textOf(project, 'data-where-it-goes');
    expect(
      said,
      'with "this project" chosen the sentence about where this goes is empty. A destination that ' +
        'changed and a screen that says nothing about it is worse than no choice at all: the ' +
        'person reads the old sentence and believes it',
    ).not.toBe('');
    expect(
      said,
      'the sentence did not change with the choice: both destinations are described by the same ' +
        'words. Then the choice is a control with no visible effect, and the effect it does have ' +
        'is a write into the folders this persons agent apps read',
    ).not.toBe(WHERE_IT_LANDS);

    expect(
      project,
      'with "this project" chosen the screen STILL carries the sentence about landing in the ' +
        'folders on this machine, next to the new one. Two sentences about one fact is exactly ' +
        'invariant 13, and here the older of the two is the one that is now false',
    ).not.toContain(WHERE_IT_LANDS);
    expect(
      everywhere,
      'with "everywhere" chosen the screen ALSO carries the sentence written for the other ' +
        'choice. Both sentences standing at once is the same defect read from the other side: ' +
        'whichever one the person believes, half the screen disagrees',
    ).not.toContain(said);
  });

  for (const landing of [EVERYWHERE, THIS_PROJECT]) {
    it(
      'sends "' + landing + '" and the folder of the open workspace when the skill is added',
      async () => {
        useSkills.setState({ landing });

        /* FOLDER CZYTANY Z `activeWorkspace()`, NIE WPISANY TUTAJ. To jest ta sama funkcja, którą
         * pyta Start biegu, i jedyna odpowiedź na pytanie „gdzie pracujemy" w tym repo. Wartość
         * wpisana z palca przechodziłaby na implementacji, która zgaduje katalog roboczy. */
        const folder = activeWorkspace()?.folder ?? null;
        expect(
          folder,
          'the fixture did not manage to open a workspace, so the assertion about the folder below ' +
            'would be about nothing',
        ).not.toBeNull();

        await useSkills.getState().add();

        expect(
          invoked.mock.calls.length,
          'adding the skill reached Rust ' +
            String(invoked.mock.calls.length) +
            ' times instead of once. Zero is silence after a control; more than one writes the ' +
            'same skill twice',
        ).toBe(1);

        const sent = invoked.mock.calls.at(0);
        if (sent === undefined) {
          throw new Error('adding the skill never reached Rust at all');
        }

        const asked = sent.at(0);
        expect(
          typeof asked === 'string' && known.has(asked),
          'the window asked Rust for ' +
            String(asked) +
            ', which is not on src-tauri/commands.golden.txt — so nothing on the Rust side keeps ' +
            'that name alive, and the day it is renamed this call goes quiet. The name is read out ' +
            'of that file here, never typed into this test',
        ).toBe(true);

        const payload = sent.at(1);
        const carried =
          typeof payload === 'object' && payload !== null
            ? (payload as Record<string, unknown>)
            : {};
        const wanted = [...windowSideArguments(rust, String(asked))].sort();
        expect(
          wanted.length,
          'no signature for ' +
            String(asked) +
            ' could be parsed out of src-tauri/src/ipc.rs, so the key comparison below would be ' +
            'nothing against nothing — the exact shape of green this criterion exists to end',
        ).toBeGreaterThan(0);
        expect(
          Object.keys(carried).sort(),
          'the window sends ' +
            JSON.stringify(Object.keys(carried).sort()) +
            ' and the command takes ' +
            JSON.stringify(wanted) +
            ' (read out of src-tauri/src/ipc.rs in this run). Tauri matches invoke arguments BY ' +
            'NAME and deserializes them before the command body runs, so a key that does not line ' +
            'up is not a smaller call — it is a rejected one, and the refusal arrives as a raw ' +
            'string nobody sees',
        ).toEqual(wanted);

        const values = carriedValues(carried, []);
        expect(
          values,
          'the person chose "' +
            landing +
            '" and that choice did not leave the window: what went to Rust was ' +
            JSON.stringify(values.filter((one) => typeof one === 'string' && one.length < 40)) +
            '. A choice that changes nothing about where the file lands is a control with no ' +
            'effect (invariant 16), and here the effect is a write into the folders this persons ' +
            'agent apps read on every later run — including outside Loadout',
        ).toContain(landing);
        expect(
          values,
          'the folder of the open workspace did not travel with the save. Without it the Rust side ' +
            'has no project root to write under, and place::destinations answers a scope with no ' +
            'root in RELATIVE paths — so an implementation that skips the refusal writes the skill ' +
            'under whatever folder the app happens to be running in. The expected value is read ' +
            'from activeWorkspace() here, the same one Start asks, so an implementation that ' +
            'guesses its own answer fails this line rather than quietly disagreeing with the run',
        ).toContain(folder);
      },
    );
  }

  it('with no workspace open, this project cannot be picked and the screen says how to get one', () => {
    withoutAWorkspace();
    const markup = screenWith(EVERYWHERE);

    const closed = tagWith(markup, 'data-landing="' + THIS_PROJECT + '"');
    expect(
      closed,
      'there is no entry for "this project" in the choice at all. Hiding it is not the same as ' +
        'refusing it: a person who cannot see the option cannot learn that it exists or what it ' +
        'would take to reach it',
    ).not.toBe('');
    /* `disabled=""` DOKŁADNIE, nie słowo `disabled` gdziekolwiek w znaczniku. Wariant
     * `disabled:` Tailwinda zostawia to słowo w atrybucie `class` także wtedy, gdy kontrolka
     * działa — `review-card.tsx` ma tę pułapkę opisaną i omija ją tym samym sposobem. */
    expect(
      closed.includes('disabled=""'),
      'with no workspace open, "this project" can still be picked: ' +
        closed +
        '. There is no project root to write under, so the pick would either be refused later ' +
        'with a sentence about something the person never chose, or — worse — write the skill ' +
        'under whatever folder the app started in',
    ).toBe(true);

    const inviteAt = markup.indexOf(FIRST_INVITE);
    expect(
      inviteAt,
      'the screen turns down "this project" and does not say what to do about it. The side menu ' +
        'names that control "' +
        FIRST_INVITE +
        '" and this sentence has to name it the same way — a sentence pointing at a control ' +
        'called something else on screen is an instruction nobody can follow. The name is ' +
        'imported from the side menu here, never typed into this test',
    ).toBeGreaterThanOrEqual(0);
    expect(
      inviteAt,
      'the way out is written BELOW the control it is about. A person reads the refusal where ' +
        'the refusal happens',
    ).toBeLessThan(markup.indexOf('data-add'));

    /* DRUGA POŁOWA, i bez niej pierwsza przechodzi na ekranie, który mówi o dodaniu zakresu
     * ZAWSZE — czyli na zdaniu, które jest prawdziwe raz i szumem przez resztę czasu. */
    withAWorkspace();
    const open = screenWith(EVERYWHERE);
    const offered = tagWith(open, 'data-landing="' + THIS_PROJECT + '"');
    expect(
      offered.includes('disabled=""'),
      'a workspace IS open and "this project" still cannot be picked: ' +
        offered +
        '. Then the whole choice is decoration and every skill keeps landing in the home folders',
    ).toBe(false);
    expect(
      open,
      'a workspace is open and the screen still tells the person to add one. A sentence that is ' +
        'always on screen says nothing, and this one sends the person to the side menu to redo ' +
        'something they already did',
    ).not.toContain(FIRST_INVITE);
  });
});
