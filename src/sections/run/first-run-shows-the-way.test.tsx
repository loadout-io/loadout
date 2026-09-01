/* PIERWSZE URUCHOMIENIE PROWADZI, zamiast pokazywać wygaszony kokpit.
 *
 * ZMIERZONE 2026-08-31 na tej gałęzi, i to jest cały powód istnienia tego pliku. Świeży ekran
 * Run rysował pełny układ produkcyjny — pasek kart, pasek loadoutu, pustą strefę pracy, wiersz
 * wejścia — z których niemal wszystko było wygaszone albo puste. Do pierwszego działającego
 * biegu było osiem do jedenastu ruchów, a aplikacja ANI RAZU nie mówiła, gdzie je zrobić: nie
 * było zdania „potrzebujesz agenta i workflow", nie było drogi z pustego Run do Agents. Jedyne,
 * co strefa pracy mówiła, to „Nothing here yet: the work shows up line by line." — czyli
 * dokładnie ten komunikat o braku danych, który `docs/design/DESIGN.md` §6 nazywa złą
 * odpowiedzią: „Pusty ekran to zaproszenie do działania, nie komunikat o braku danych".
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
 * ── CO ZMIENIŁO SIĘ W TYM PLIKU 2026-08-31 WIECZOREM, I DLACZEGO ───────────────────────────
 *
 * KOLEJNOŚĆ KROKÓW. Do tego dnia było `workspace, agent, workflow`, czyli „najpierw wskaż
 * folder". Makieta, którą wybrał właściciel, zaczyna od agenta i mówi to samo w nawigacji
 * („Workflows — make an agent first"): folderu nie potrzeba, żeby napisać agenta, a potrzeba
 * go, żeby nacisnąć Run. Kolejność idzie więc za tym, co człowiek naprawdę robi, a folder stoi
 * tam, gdzie zaczyna być potrzebny — na trzecim przystanku, który mówi o tym wprost.
 *
 * GDZIE MIESZKA AKCENT. Wypełnienie akcentem zeszło z wiersza kroku na DUŻY przycisk powitania.
 * Pytanie zostaje to samo — „czy dokładnie jedna rzecz na ekranie mówi »naciśnij to«" — i jest
 * zadane o cały blok pierwszego otwarcia, a nie o samą listę: akcent schowany w liście, kiedy
 * pod nią stoi drugi, nie jest ani trochę lepszy od dwóch akcentów w liście.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { useSectionStore } from '../../ui/shell/section-store';
import { FIRST_INVITE, WorkspaceSwitcher } from '../../ui/shell/workspace-switcher';
import { FirstRun, firstRunSteps, openAgents, openWorkflows } from './first-run';
import Run from './index';
import { welcomeFor } from './welcome';

/** Świeży ekran: magazyny są puste, bo nikt nic do nich nie włożył w tym pliku. */
const markup = renderToStaticMarkup(<Run />);

/** Element o tym znaczniku, wycięty PO GŁĘBOKOŚCI — leniwy wzorzec kończy na cudzym zamknięciu. */
function region(html: string, marker: string): string {
  const open = new RegExp('<([a-z]+)[^>]*\\s' + marker + '\\b[^>]*>');
  const hit = open.exec(html);
  if (hit === null) return '';
  const name = hit[1] ?? '';
  const walk = new RegExp('<(/?)' + name + '\\b[^>]*>', 'g');
  walk.lastIndex = hit.index;
  let depth = 0;
  let step = walk.exec(html);
  while (step !== null) {
    depth += step[1] === '/' ? -1 : 1;
    if (depth === 0) return html.slice(hit.index, step.index + step[0].length);
    step = walk.exec(html);
  }
  return html.slice(hit.index);
}

/** Cały blok pierwszego otwarcia: droga, powitanie, galeria i wiersz klawiszy. */
const opening = region(markup, 'data-first-open');

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
    ).toEqual(['agent', 'workflow', 'workspace']);
    expect(
      nothing.map((step) => step.state),
      'with nothing set up, the agent comes first and the other two wait quietly',
    ).toEqual(['now', 'later', 'later']);

    expect(
      firstRunSteps({ workspaces: 0, agents: 1, workflows: 0 }).map((step) => step.state),
      'one agent is written, so it has to read as finished and the next one has to light up',
    ).toEqual(['done', 'now', 'later']);
    expect(
      firstRunSteps({ workspaces: 0, agents: 3, workflows: 2 }).map((step) => step.state),
      'two of three are finished, so the third one is the only one asking for anything',
    ).toEqual(['done', 'done', 'now']);
    expect(
      firstRunSteps({ workspaces: 1, agents: 1, workflows: 1 }).map((step) => step.state),
      'everything is set up, so nothing is asking for anything any more',
    ).toEqual(['done', 'done', 'done']);
  });

  it('draws those three steps on the empty screen, in order', () => {
    expect(
      opening,
      'the empty work area draws no walkthrough at all. It said "Nothing here yet" and left ' +
        'a person eight to eleven moves away from a first run with no word about where to make ' +
        'them — which is the notice DESIGN §6 rules out, not the invitation it asks for.',
    ).not.toBe('');

    const listed = rows(opening);
    expect(
      listed.map((row) => attribute(row, 'data-first-step')),
      'the walkthrough has to name its three steps in the order a person does them',
    ).toEqual(['agent', 'workflow', 'workspace']);

    for (const row of listed) {
      expect(
        words(row).length,
        'a step in the walkthrough says nothing, so the screen shows an order of blanks',
      ).toBeGreaterThan(3);
    }
  });

  it('says the state of every step, and accents only the one to do now', () => {
    const listed = rows(opening);
    const states = listed.map((row) => attribute(row, 'data-step-state'));
    expect(
      states,
      'on a fresh screen the first step is the one to do and the rest are quiet',
    ).toEqual(['now', 'later', 'later']);

    expect(
      [...opening.matchAll(/btn-primary/g)].length,
      'the accent fill is not on exactly one control of the first screen. Accent means "this is ' +
        'the thing to press", and two of them mean neither is.',
    ).toBe(1);

    const hero = region(opening, 'data-first-hero');
    expect(
      [...hero.matchAll(/btn-primary/g)].length,
      'the accented control does not sit in the welcome, which is where the eye lands and where ' +
        'the sentence about the next move stands. Whatever else the screen shouts at, it is not ' +
        'what a person has to do next.',
    ).toBe(1);
    expect(
      liveButtons(hero).length,
      'the welcome names a move and offers no way to make it, or offers more than one ' +
        '(invariant 16)',
    ).toBe(1);
    expect(
      words(hero),
      'the loud control does not carry the words this screen says the next move is: ' +
        JSON.stringify(welcomeFor(firstRunSteps({ workspaces: 0, agents: 0, workflows: 0 }), null)),
    ).toContain('Make your first agent');

    /* Krok folderu jest jedyną drogą do pierwszego zakresu, jaką ten ekran ma — i to samo mówi
       `e2e/tests/plus-opens-a-terminal.spec.ts`, licząc ten znacznik. */
    expect(
      [...opening.matchAll(/\bdata-add-workspace\b/g)].length,
      'nothing on the empty screen offers to pick the first folder any more, or two things do ' +
        'and a person has to work out which one is real',
    ).toBe(1);
  });

  it('reads finished steps as finished once the thing they ask for is there', () => {
    const listed = rows(
      renderToStaticMarkup(
        <FirstRun
          steps={firstRunSteps({ workspaces: 0, agents: 1, workflows: 0 })}
          onAddWorkspace={() => undefined}
        />,
      ),
    );
    const [agent = '', workflow = ''] = listed;
    expect(
      attribute(agent, 'data-step-state'),
      'the first step is finished and the screen still shows it as the one to do',
    ).toBe('done');
    /* STAN MA BYĆ WIDOCZNY W SAMYM WIERSZU. Wiersz czytający się tak samo przed i po zostawia
       człowieka robiącego drugi raz to, co już zrobił — a `data-step-state` widzi wyrocznia,
       nie oko. Ptaszek jest tym, co widzi oko, i stoi wyłącznie na kroku zrobionym. */
    expect(
      words(agent),
      'a finished step looks exactly like an unfinished one: ' + JSON.stringify(words(agent)),
    ).toContain('✓');
    expect(
      words(workflow),
      'the step that is still to do is already ticked off: ' + JSON.stringify(words(workflow)),
    ).not.toContain('✓');
  });

  it('says out loud how far along the first run is', () => {
    const counted = renderToStaticMarkup(
      <FirstRun
        steps={firstRunSteps({ workspaces: 0, agents: 1, workflows: 0 })}
        onAddWorkspace={() => undefined}
      />,
    );
    expect(
      words(region(counted, 'data-road-count')),
      'the screen never says how much of the first run is behind a person, so the walkthrough ' +
        'has a beginning and no visible end',
    ).toBe('1 of 3 done');
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
