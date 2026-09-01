/* PIERWSZE URUCHOMIENIE PROWADZI, zamiast pokazywać wygaszony kokpit.
 *
 * ZMIERZONE 2026-08-31 na tej gałęzi, i to jest cały powód istnienia tego pliku. Świeży ekran
 * Run rysuje pełny układ produkcyjny — pasek kart, pasek loadoutu, pustą strefę pracy, wiersz
 * wejścia — z których niemal wszystko jest wygaszone albo puste. Do pierwszego działającego
 * biegu jest osiem do jedenastu ruchów, a aplikacja ANI RAZU nie mówi, gdzie je zrobić: nie ma
 * zdania „potrzebujesz agenta i workflow", nie ma drogi z pustego Run do Agents. Jedyne, co
 * strefa pracy mówiła, to „Nothing here yet: the work shows up line by line." — czyli dokładnie
 * ten komunikat o braku danych, który `docs/design/DESIGN.md` §6 nazywa złą odpowiedzią:
 * „Pusty ekran to zaproszenie do działania, nie komunikat o braku danych".
 *
 * DLACZEGO KRYTERIUM STOI NA MARKUPIE, A NIE NA `firstRunSteps` (niezmiennik 29). Sama funkcja
 * licząca stany dowodzi wyłącznie, że mechanizm istnieje. Trzy kroki, których nikt nie renderuje,
 * to ta sama wada, dla której to repo powstało: kryterium zielone, funkcja martwa. Dlatego
 * wszystko poniżej pierwszego bloku czyta HTML, który powstaje z `<Run />` — czyli to, na co
 * patrzy człowiek po pierwszym uruchomieniu, kiedy magazyny są puste, bo nikt nic do nich nie
 * włożył.
 *
 * DROGA DO SEKCJI JEST DOWODZONA WOŁANIEM, nie napisem. Bez jsdom nie da się kliknąć, więc test
 * woła dokładnie tę funkcję, którą podaje przyciskowi `onClick`, i pyta magazyn sekcji, czy
 * ekran naprawdę się przesunął. Napis „Open Agents" nad handlerem, który nic nie robi, jest tą
 * samą martwą kontrolką, przed którą stoi niezmiennik 16.
 *
 * DRUGA POŁOWA: GŁOŚNOŚĆ ZAPROSZENIA. „Add a workspace" było wypełnionym akcentem przyciskiem
 * w bocznym menu — najgłośniejszą rzeczą na ekranie, głośniejszą niż treść — mimo że po
 * wskazaniu pierwszego folderu znika na zawsze. Akcent znaczy „to jest interaktywne"
 * (DESIGN §3), więc wypełnienie akcentem należy się TEMU, co człowiek ma zrobić teraz, i tylko
 * temu. Klasa `.btn-primary` jest w `src/styles/theme.css` jedynym wypełnieniem akcentem wśród
 * przycisków, więc pytanie o nią jest pytaniem o głośność, a nie o napis.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { useSectionStore } from '../../ui/shell/section-store';
import { FIRST_INVITE, WorkspaceSwitcher } from '../../ui/shell/workspace-switcher';
import { FirstRun, firstRunSteps, openAgents, openWorkflows } from './first-run';
import Run from './index';

/** Świeży ekran: magazyny są puste, bo nikt nic do nich nie włożył w tym pliku. */
const markup = renderToStaticMarkup(<Run />);

/** Blok przewodnika po pierwszym uruchomieniu, wycięty z całego ekranu. */
const guide = /<ol[^>]*\bdata-first-run\b[\s\S]*?<\/ol>/.exec(markup)?.[0] ?? '';

/** Wiersze przewodnika, w kolejności, w jakiej stoją na ekranie. */
function rows(block: string): readonly string[] {
  return [...block.matchAll(/<li[^>]*\bdata-first-step\b[\s\S]*?<\/li>/g)].map((hit) => hit[0]);
}

/** Wartość atrybutu z otwierającego znacznika wiersza — pusty napis, kiedy go tam nie ma. */
function attribute(row: string, name: string): string {
  return new RegExp(name + '="([^"]*)"').exec(row)?.[1] ?? '';
}

/** Tekst wiersza bez znaczników, ze ściśniętymi odstępami. */
function words(row: string): string {
  return row
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Kontrolka czynna to taka, której człowiek może użyć — wyłączona jest widoczna i bezużyteczna. */
function liveButtons(block: string): readonly string[] {
  return [...block.matchAll(/<button\b[^>]*>/g)]
    .map((hit) => hit[0])
    .filter((tag) => !/\sdisabled\b/.test(tag));
}

describe('the first run is walked through, not left to guess', () => {
  it('counts three steps and lights exactly the first one that is not finished', () => {
    const nothing = firstRunSteps({ workspaces: 0, agents: 0, workflows: 0 });
    expect(
      nothing.map((step) => step.id),
      'a fresh install has three things to do, in this order, and nothing said them',
    ).toEqual(['workspace', 'agent', 'workflow']);
    expect(
      nothing.map((step) => step.state),
      'with nothing set up, the folder comes first and the other two wait quietly',
    ).toEqual(['now', 'later', 'later']);

    expect(
      firstRunSteps({ workspaces: 1, agents: 0, workflows: 0 }).map((step) => step.state),
      'the folder is picked, so it has to read as finished and the next one has to light up',
    ).toEqual(['done', 'now', 'later']);
    expect(
      firstRunSteps({ workspaces: 2, agents: 3, workflows: 0 }).map((step) => step.state),
      'two of three are finished, so the third one is the only one asking for anything',
    ).toEqual(['done', 'done', 'now']);
    expect(
      firstRunSteps({ workspaces: 1, agents: 1, workflows: 1 }).map((step) => step.state),
      'everything is set up, so nothing is asking for anything any more',
    ).toEqual(['done', 'done', 'done']);
  });

  it('draws those three steps on the empty screen, in order', () => {
    expect(
      guide,
      'the empty work area draws no walkthrough at all. It said "Nothing here yet" and left ' +
        'a person eight to eleven moves away from a first run with no word about where to make ' +
        'them — which is the notice DESIGN §6 rules out, not the invitation it asks for.',
    ).not.toBe('');

    const listed = rows(guide);
    expect(
      listed.map((row) => attribute(row, 'data-first-step')),
      'the walkthrough has to name its three steps in the order a person does them',
    ).toEqual(['workspace', 'agent', 'workflow']);

    for (const row of listed) {
      expect(
        words(row).length,
        'a step in the walkthrough says nothing, so the screen shows an order of blanks',
      ).toBeGreaterThan(3);
    }
  });

  it('says the state of every step, and accents only the one to do now', () => {
    const listed = rows(guide);
    const states = listed.map((row) => attribute(row, 'data-step-state'));
    expect(
      states,
      'on a fresh screen the first step is the one to do and the rest are quiet',
    ).toEqual(['now', 'later', 'later']);

    expect(
      liveButtons(guide).length,
      'the step to do now carries no control a person can press, so the walkthrough names a ' +
        'move and offers no way to make it (invariant 16)',
    ).toBe(1);

    expect(
      [...guide.matchAll(/btn-primary/g)].length,
      'the accent fill is on more than the one step to do now. Accent means "this is the ' +
        'thing to press", and two of them mean neither is.',
    ).toBe(1);

    const [first = ''] = listed;
    expect(
      /btn-primary/.test(first),
      'the accented control does not sit on the step that is due. Whatever else the screen ' +
        'shouts at, it is not what a person has to do next.',
    ).toBe(true);

    /* Krok wskazujący folder jest jedyną drogą do pierwszego zakresu, jaką ten ekran ma — i to
       samo mówi `e2e/tests/plus-opens-a-terminal.spec.ts`, licząc ten znacznik. */
    expect(
      /\bdata-add-workspace\b/.test(first),
      'nothing on the empty screen offers to pick the first folder any more',
    ).toBe(true);
  });

  it('reads finished steps as finished once the thing they ask for is there', () => {
    const [workspace = ''] = rows(
      renderToStaticMarkup(
        <FirstRun
          steps={firstRunSteps({ workspaces: 1, agents: 0, workflows: 0 })}
          onAddWorkspace={() => undefined}
        />,
      ),
    );
    expect(
      attribute(workspace, 'data-step-state'),
      'the first step is finished and the screen still shows it as the one to do',
    ).toBe('done');
    expect(
      words(workspace).toLowerCase(),
      'a finished step has to SAY it is finished. A row that reads the same whether it is done ' +
        'or not leaves a person re-doing work they already did: ' +
        JSON.stringify(words(workspace)),
    ).toContain('done');
  });

  it('carries the walkthrough to the sections that answer it', () => {
    useSectionStore.getState().go('run');
    openAgents();
    expect(
      useSectionStore.getState().section,
      'the step about agents leads nowhere. A row naming Agents beside a control that does not ' +
        'open Agents is the dead control invariant 16 forbids.',
    ).toBe('agents');

    useSectionStore.getState().go('run');
    openWorkflows();
    expect(
      useSectionStore.getState().section,
      'the step about workflows leads nowhere, so the screen names a place and will not take ' +
        'a person to it',
    ).toBe('workflows');

    useSectionStore.getState().go('run');
  });

  it('stops shouting the invitation that disappears after one use', () => {
    const nav = renderToStaticMarkup(
      <WorkspaceSwitcher
        all={[]}
        activeId={null}
        said={null}
        ui={{ open: false, adding: false, name: '', folder: null, troubled: null }}
      />,
    );
    const invite =
      new RegExp(
        '<button[^>]*>(?:(?!</button>)[\\s\\S])*' + FIRST_INVITE + '[\\s\\S]*?</button>',
      ).exec(nav)?.[0] ?? '';
    expect(
      invite,
      'the side menu no longer offers to add the first workspace at all, so this measurement ' +
        'would be about a control that is not there',
    ).not.toBe('');
    expect(
      liveButtons(invite).length,
      'the invitation in the side menu is disabled, which is worse than loud',
    ).toBe(1);
    expect(
      /btn-primary/.test(invite),
      'the side menu still fills the "add a workspace" invitation with the accent, which makes ' +
        'it the loudest thing on the whole window — louder than the work — for a control that ' +
        'is pressed once in the life of an install and then gone forever. The accent belongs ' +
        'to the step a person has to take now.',
    ).toBe(false);
  });
});
