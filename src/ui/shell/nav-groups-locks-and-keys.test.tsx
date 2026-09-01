/* Nawigacja odpowiada na „od czego zacząć", a nie tylko na „gdzie jestem".
 *
 * DLACZEGO TO KRYTERIUM ISTNIEJE. Do dziś boczne menu było PŁASKĄ LISTĄ siedmiu równych
 * pozycji. Siedem pozycji o jednakowej wadze nie mówi nic o kolejności, w jakiej człowiek ma
 * ich użyć — a kolejność jest tu twarda: workflow to agenci w rzędzie, więc bez agenta nie ma
 * czego postawić w rzędzie, a bez rzędu nie ma czego uruchomić. Właściciel nazwał to dwa razy:
 * „UX totalnie nieoczywisty". Zmierzone: wszystkie siedem pustych ekranów mówiło wariant zdania
 * „coś się tu kiedyś pojawi" i ANI JEDNO nie mówiło, co nacisnąć.
 *
 * CZTERY RZECZY, KTÓRYCH TA LISTA NIE MA PRAWA ZROBIĆ, i każda ma tu punkt:
 *
 *   1. Postawić siedem pozycji w jednym rzędzie bez podziału na to, PO CO się tu przychodzi.
 *   2. Narysować kłódkę, której stan nie wynika z danych (niezmiennik 17). Kłódka policzona
 *      z niczego jest gorsza niż jej brak: mówi „nie wolno" tam, gdzie wolno, i milknie tam,
 *      gdzie naprawdę nie ma czego uruchomić.
 *   3. Obiecać skrót, którego klawiatura nie zna. `⌘1` narysowane przy pozycji jest kontrolką
 *      (niezmiennik 16) — jeśli nic nie robi, człowiek dowiaduje się wyłącznie tego, że nie umie
 *      obsługiwać tej aplikacji.
 *   4. Powiedzieć „nie da się jeszcze tego użyć" i NIE powiedzieć czego brakuje. To jest ta sama
 *      wada, co siedem zdań „coś się tu kiedyś pojawi", tylko przeniesiona do menu.
 *
 * PODMIOTEM JEST ZDANIE, KTÓRE WIDZI CZŁOWIEK (niezmiennik 29), a nie obecność napisu w pliku:
 * każdy punkt niżej renderuje powłokę dwa razy — raz w świecie, w którym czegoś brakuje, i raz
 * w tym, w którym już jest — i pyta o RÓŻNICĘ między tymi dwoma widokami. Asercja na samą
 * obecność zdania przechodziłaby na menu, które rysuje kłódkę zawsze.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import { App } from '../../App';
import { moveFor } from '../palette/keys';
import { SECTIONS } from '../sections';
import { NAV_WIDTH, SideNav } from './titlebar';
import { useSectionStore } from './section-store';
import { asked, askForSearch } from './search-asked';
import { useWhatYouHave } from './what-you-have';
import { useRun } from '../../state/run';
import { collapseNav } from '../../state/settings';

/** Powłoka wyrenderowana na świecie, w którym człowiek ma tyle rzeczy, ile mówią liczby. */
function navWith(have: { agents: number | null; workflows: number | null }): string {
  useWhatYouHave.setState({ agents: have.agents, workflows: have.workflows });
  return renderToStaticMarkup(<SideNav section="agents" />);
}

/* Tryb ustawiony w `beforeEach`, nie odziedziczony po sąsiednim punkcie: punkt o zwężeniu
 * przestawia go naprawdę, a wszystkie pozostałe mówią o liście miejsc, czyli o trybie szerokim. */

/** Treść wszystkich elementów o tym znaczniku, bez znaczników i bez nadmiarowych odstępów. */
function saidBy(markup: string, marker: string): readonly string[] {
  const found = new RegExp(
    '<([a-z][a-z0-9]*)[^>]*\\b' + marker + '\\b[^>]*>([\\s\\S]*?)</\\1>',
    'g',
  );
  return [...markup.matchAll(found)].map((hit) =>
    (hit[2] ?? '')
      .replace(/<[^>]*>/g, ' ')
      .replace(/\s+/g, ' ')
      .trim(),
  );
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Blok jednego przełącznika sekcji, od znacznika otwierającego do zamykającego. */
function rowFor(markup: string, id: string): string {
  const found = new RegExp('<button[^>]*data-section-switch="' + id + '"[\\s\\S]*?</button>');
  return found.exec(markup)?.[0] ?? '';
}

/** Nic nie ma i nikt jeszcze nic nie zaczął — świat pierwszego otwarcia. */
const NOTHING = { agents: 0, workflows: 0 };
/** Jeden agent, jeden workflow: droga przejdzieona do końca. */
const EVERYTHING = { agents: 2, workflows: 1 };

/* Cztery zdania, znak w znak z makiety (`docs/mockup/index.html`, `<em class="why">`). Stoją tu
 * wypisane, a nie czytane z rejestru: pętla po rejestrze sądziłaby rejestr samym sobą i pusty
 * powód przechodziłby każdy taki punkt. */
const WHY = {
  workflows: 'Make an agent first — a workflow is agents in a row',
  run: 'Needs a workflow to run',
  triggers: 'Needs a workflow to start on its own',
  lab: 'Needs an agent to try things on',
} as const;

beforeEach(() => {
  useWhatYouHave.setState({ agents: null, workflows: null });
  useRun.setState({ workflow: '' });
  useSectionStore.getState().go('run');
  void collapseNav(false);
});

describe('the list of places says what to do first, not just where you are', () => {
  it('sorts the seven places under three eyebrows that say what you came for', () => {
    const heads = saidBy(navWith(EVERYTHING), 'data-nav-group');

    expect(
      heads,
      'the list of places carries no eyebrows at all, so all seven stand in one flat row of ' +
        'equal weight. A flat row answers "where am I" and stays silent about "what do I do ' +
        'first" — and that silence is half of what a person meets on the first open.',
    ).toEqual(['Make', 'Run', 'Know']);
  });

  it('draws every place under the eyebrow its own entry names, and Settings under none', () => {
    const markup = navWith(EVERYTHING);
    /* Kolejność wystąpień w markupie, nie kolejność w rejestrze: pytanie brzmi „co człowiek
     * czyta od góry do dołu". */
    const order = [...markup.matchAll(/data-nav-group|data-section-switch="([a-z]+)"/g)].map(
      (hit) => hit[1] ?? 'GROUP',
    );

    expect(
      order,
      'the seven places do not stand under the eyebrows the registry gives them. Make holds ' +
        'what you build, Run holds what you set going, Know holds what the agents read — and ' +
        'Settings stands under none of the three, at the bottom, because it is not a place you ' +
        'come to do work.',
    ).toEqual([
      'GROUP',
      'agents',
      'workflows',
      'GROUP',
      'run',
      'triggers',
      'GROUP',
      'knowledge',
      'lab',
      'settings',
    ]);
  });

  it('shows the key that reaches each place, and the keyboard really takes that key', () => {
    const markup = navWith(EVERYTHING);

    SECTIONS.forEach((entry, index) => {
      const key = '⌘' + String(index + 1);
      expect(
        rowFor(markup, entry.id),
        entry.id +
          ' shows no key to reach it. A place reachable only by mouse is a place a person ' +
          'visits by hunting for it, and this list is the only navigation this app has.',
      ).toContain(key);

      const taken = moveFor(
        {
          key: String(index + 1),
          metaKey: true,
          ctrlKey: false,
          altKey: false,
          shiftKey: false,
        },
        null,
        false,
      );
      expect(
        taken,
        'the list draws ' +
          key +
          ' beside ' +
          entry.label +
          ' and the keyboard does nothing with it. A drawn key that does nothing is a control ' +
          'without a handler (invariant 16): the person presses it, the window stands still, ' +
          'and the only thing learned is that they cannot work this app.',
      ).toEqual({ move: 'jump', section: entry.id });
    });
  });

  it('says what a place still needs, and says it only while it is really needed', () => {
    const empty = navWith(NOTHING);
    const full = navWith(EVERYTHING);

    for (const [id, why] of Object.entries(WHY)) {
      expect(
        rowFor(empty, id),
        id +
          ' cannot be used yet and the list says nothing about why. "Nothing here yet" is the ' +
          'sentence this whole redesign exists to delete: it tells a person that they are ' +
          'stuck without telling them what would unstick them.',
      ).toContain(why);
      expect(
        rowFor(full, id),
        id +
          ' still says what it needs, on a machine where it already has it. A reason that ' +
          'stands whatever the disk holds is not a reason — it is decoration that reads as one, ' +
          'and it is the first thing a person stops believing.',
      ).not.toContain(why);
    }
  });

  it('counts the lock from what is really there, one axis at a time', () => {
    const onlyAgents = navWith({ agents: 1, workflows: 0 });

    expect(
      rowFor(onlyAgents, 'workflows'),
      'one agent is enough to put a second one beside it, so Workflows has to open the moment ' +
        'the first agent exists. It is still locked here.',
    ).not.toContain(WHY.workflows);
    expect(
      rowFor(onlyAgents, 'lab'),
      'Lab tries things on an agent, so one agent is exactly what it was waiting for',
    ).not.toContain(WHY.lab);
    expect(
      rowFor(onlyAgents, 'run'),
      'Run needs a workflow and there is none — an agent alone is not a row of agents, and the ' +
        'lock has to stay until there really is one',
    ).toContain(WHY.run);
    expect(
      rowFor(onlyAgents, 'triggers'),
      'Triggers starts a workflow on its own and there is no workflow to start',
    ).toContain(WHY.triggers);
  });

  it('draws no lock at all while nobody has read the disk yet', () => {
    /* `null` znaczy „NIE WIEM", i to jest inna odpowiedź niż zero. Kłódka narysowana przed
     * pierwszym odczytem jest twierdzeniem o danych, których nikt nie widział — dokładnie ta
     * klasa, której zabrania niezmiennik 17. */
    const unread = navWith({ agents: null, workflows: null });

    expect(
      saidBy(unread, 'data-needs'),
      'the list locks places before anything has been read off the disk. Not knowing and ' +
        'having nothing are two different answers, and only one of them is the person’s ' +
        'own doing.',
    ).toEqual([]);
    expect(
      rowFor(unread, 'workflows'),
      'a place nobody has counted yet has to look like an ordinary place — key and all. ' +
        'Dropping the key while waiting for the disk makes the list flicker on every open, ' +
        'and a person reads a flicker as a fault.',
    ).toContain('⌘2');
  });

  it('names the very next thing to do while the road is unwalked, and stops when it is', () => {
    const first = saidBy(navWith(NOTHING), 'data-next-step');
    const later = saidBy(navWith({ agents: 1, workflows: 0 }), 'data-next-step');

    expect(
      first.length,
      'the list of places carries no next step on a fresh machine. Seven doors, each one ' +
        'reporting that it is empty, and nowhere a first move — that is the whole complaint, ' +
        'written into furniture.',
    ).toBe(1);
    expect(
      first[0] ?? '',
      'the next step has to name the one thing that opens everything else — making an agent',
    ).toContain('Make one agent');
    expect(
      later[0] ?? '',
      'after the first agent the next step has to MOVE, to the thing that is now possible and ' +
        'was not before. A panel that says the same thing after the person did it is a panel ' +
        'that was never reading them.',
    ).toContain('Workflows');
    expect(
      saidBy(navWith(EVERYTHING), 'data-next-step'),
      'the road is walked — an agent exists and a workflow exists — and the list still tells ' +
        'the person what to do next. Advice that never ends stops being advice and becomes ' +
        'chrome nobody reads.',
    ).toEqual([]);
  });

  it('wears the live badge next to Run while a run is going, and the key when it is not', () => {
    useWhatYouHave.setState(EVERYTHING);

    useRun.setState({ workflow: '' });
    const quiet = rowFor(renderToStaticMarkup(<SideNav section="agents" />), 'run');
    useRun.setState({ workflow: 'Ship a feature' });
    const going = rowFor(renderToStaticMarkup(<SideNav section="agents" />), 'run');

    expect(
      quiet,
      'Run wears the live badge while nothing is running. A badge that is always there says ' +
        'nothing when it matters, and this one is the only place the app admits from every ' +
        'screen that work is happening somewhere.',
    ).not.toContain('data-nav-live');
    expect(
      going,
      'a run is going and the list of places is silent about it. The whole point of putting it ' +
        'here is that the person sees it from Knowledge, from Settings, from anywhere.',
    ).toContain('data-nav-live');
    expect(
      going,
      'while a run is going the badge takes the place of the key, exactly as the mockup draws ' +
        'it: two pills in one row read as two different facts about the same place.',
    ).not.toContain('⌘3');

    /* Bieg da się zacząć bez zapisanego workflow, więc te dwa stany naprawdę spotykają się
       na jednym wierszu — i wtedy tylko jeden z nich jest prawdą. */
    useWhatYouHave.setState(NOTHING);
    const both = rowFor(renderToStaticMarkup(<SideNav section="agents" />), 'run');
    expect(
      both,
      'something is running and the list still says Run "needs a workflow to run". The library ' +
        'can be empty while work is going — a run can be asked for without a saved workflow — ' +
        'and a lock over a live run is simply false. One wrong sentence here costs the reader ' +
        'their trust in every other one.',
    ).not.toContain(WHY.run);
    expect(
      both,
      'the live badge lost to the lock. What is happening now beats what is missing: the person ' +
        'has to be able to reach the work from anywhere, and the row that holds it is the way.',
    ).toContain('data-nav-live');
  });

  it('says how many things a place holds, once there is more than none', () => {
    const some = navWith({ agents: 3, workflows: 1 });

    expect(
      saidBy(rowFor(some, 'agents'), 'data-nav-count'),
      'the list never says how many agents there are. This is the only place in the app that ' +
        'can answer "how much have I got" without opening anything.',
    ).toEqual(['3']);
    expect(
      saidBy(rowFor(navWith(NOTHING), 'agents'), 'data-nav-count'),
      'the list draws a zero. Zero is what the empty screen already says, in words, with an ' +
        'invitation attached — a nought in a pill is furniture that adds nothing.',
    ).toEqual([]);
  });

  it('narrows to a strip of glyphs that still reaches every place, and to nothing beside it', () => {
    /* PRZEPISANE 2026-08-31, i to jest zaostrzenie, nie zamiana. Punkt brzmiał „menu trzyma
       kolumnę glifów obok listy" i sądził DRUGĄ kontrolkę na to samo miejsce — czyli
       dokumentował wadę (niezmiennik 13: jeden fakt, dwa nośniki) zamiast jej zabraniać.
       Glify są dziś DRUGIM TRYBEM tej samej nawigacji: niosą ten sam znacznik, co wiersze,
       a wiersze w tym trybie nie stoją w drzewie wcale. */
    void collapseNav(true);
    try {
      const markup = navWith(EVERYTHING);

      for (const entry of SECTIONS) {
        expect(
          occurrences(markup, 'data-section-switch="' + entry.id + '"'),
          'narrowed to glyphs, the nav has to carry exactly one control per place, and ' +
            entry.id +
            ' is missing or doubled. It is the same marker the wide mode uses, because it is ' +
            'the same navigation — every browser criterion in e2e/ walks in through it.',
        ).toBe(1);
      }
      expect(
        occurrences(markup, 'aria-current="true"'),
        'exactly one glyph has to say it is the open one — zero means the strip never says ' +
          'where you are, two mean it says it twice and one of them is wrong.',
      ).toBe(1);
      expect(
        occurrences(markup, 'data-nav-group'),
        'the narrowed nav still draws the grouped list of places beside its glyphs, so both ' +
          'ways into every place stand on screen at once. That is the defect this rebuild ' +
          'exists to remove: two modes, never two navigations.',
      ).toBe(0);
    } finally {
      void collapseNav(false);
    }
  });

  it('reaches the whole shell, not just the panel this file renders', () => {
    useWhatYouHave.setState(NOTHING);
    const whole = renderToStaticMarkup(<App section="run" screens={{}} />);

    expect(
      saidBy(whole, 'data-nav-group'),
      'the grouped list exists in the panel and not in the shell a person actually opens',
    ).toEqual(['Make', 'Run', 'Know']);
    /* KOMUNIKAT PRZEPISANY 2026-08-31 razem z drugim trybem: mówił o „kolumnie glifów PLUS
       liście miejsc", a te dwie rzeczy nie stoją już obok siebie — są dwoma trybami jednej
       nawigacji. Liczba się nie zmienia i jej wyrocznią jest makieta
       (`shell-matches-mockup.test.tsx` czyta obie szerokości z reguł `.app`); tutaj pilnujemy
       tego, że tryb rozwinięty ma miejsce na etykietę, licznik, klawisz i powód przy każdym
       z siedmiu wierszy. */
    expect(
      NAV_WIDTH,
      'the expanded mode needs the width the mockup gives it: a label, a count, a key and a ' +
        'reason on one row do not fit the narrow strip the collapsed mode uses.',
    ).toBe(308);
  });

  it('opens the search from the list, because the key is not the only door to it', () => {
    const before = asked();
    askForSearch();

    expect(
      asked(),
      'the search control in the list of places asks nobody for anything. A magnifier that ' +
        'does not open the search is the exact control invariant 16 forbids, and it stands at ' +
        'the top of the only navigation this app has.',
    ).toBeGreaterThan(before);
    expect(
      navWith(EVERYTHING),
      'the list of places carries no search control at all. The keyboard has a way in and the ' +
        'mouse has none, so a person who never learns the key never learns the search exists.',
    ).toContain('data-nav-search');
  });
});
