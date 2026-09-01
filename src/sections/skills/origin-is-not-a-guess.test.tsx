/* AC-4 dla T-42: karta przeglądu przestaje twierdzić, że wie, skąd ta umiejętność przyszła.
 *
 * `src/sections/skills/review-card.tsx:90` renderuje plakietkę „From the internet"
 * BEZWARUNKOWO, ignorując `item.fromTheInternet`. Do dziś to była prawda przez konstrukcję:
 * jedyną drogą, którą cokolwiek wchodziło do tej karty, było `review_skill(url)`, czyli link.
 * Od chwili, w której człowiek może napisać umiejętność sam, ta plakietka mówi o jego własnym
 * tekście, że przyszedł od obcego — a plakietka zastępuje w v1 podpisy i weryfikację
 * pochodzenia, których nie ma. Zdanie, które jest zawsze prawdziwe, nie niesie informacji;
 * zdanie, które jest CZASEM nieprawdziwe, uczy je ignorować.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(markup).not.toContain('From the internet')` na jednym
 * `Import`. Przechodzi na karcie, która zdjęła plakietkę CAŁKIEM — czyli umiejętność z sieci
 * przestaje się różnić od napisanej ręką, i tracimy jedyną rzecz na tej karcie, która mówi, że
 * ten tekst napisał ktoś obcy. Rozróżnia: dwa `Import`y w jednym teście, różniące się
 * WYŁĄCZNIE polem `fromTheInternet`, i asercja w obie strony.
 *
 * DLACZEGO PRZEZ CAŁY EKRAN, A NIE PRZEZ SAM KOMPONENT KARTY. Bo (d) pyta o zdanie
 * `WHERE_IT_LANDS`, które stoi NAD kartą i należy do sekcji, nie do karty — a pytanie „czy
 * ostrzeżenie jest widoczne wcześniej niż decyzja" ma sens tylko w dokumencie, w którym stoją
 * oba. Kartę i tak renderuje ten sam komponent (`data-review-card`), tylko zamontowany tam,
 * gdzie go widzi człowiek. Zdanie jest IMPORTOWANE z sekcji, nie przepisane tutaj: druga kopia
 * copy rozjeżdża się przy pierwszej korekcie i wtedy test pilnuje zdania, którego nikt już nie
 * pokazuje (niezmiennik 13).
 *
 * Render jest statyczny (`renderToStaticMarkup`), bo w repo nie ma `jsdom`.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import type { Import } from '../../state/skills';
import { useSkills } from '../../state/skills';
import SkillsShelf, { WHERE_IT_LANDS } from './shelf';

const NAME = 'review-pull-requests';
const SUMMARY = 'Use this when somebody asks for a second look at a pull request.';

/* Ciało, które karta ma pokazać. Bez apostrofów i cudzysłowów: React ucieka `'` na `&#x27;`,
 * więc `toContain` na tekście z apostrofem byłby czerwony także wtedy, gdy ekran pokazuje
 * dokładnie to, co trzeba — czyli mierzyłby kodowanie, nie zachowanie. */
const BODY = 'Read the change first, then say in one paragraph what to fix.';

/**
 * Dwa importy różniące się WYŁĄCZNIE tym jednym polem.
 *
 * Pole zostaje `boolean`em z rozmysłem: `src/state/skills.test.ts`,
 * `src/sections/skills/review-card.test.tsx`, `mounted.test.tsx`
 * i `src/sections/read-paths-populate.test.ts` zamrażają dzisiejszy kształt `fromTheInternet`,
 * a wszystkie ich fikstury mają tam `true`. Zmienia się to, SKĄD bierze się jego wartość, nie
 * jego typ — enum zaczerwieniłby cztery cudze pliki i jest poza tym zadaniem.
 */
function anImport(fromTheInternet: boolean): Import {
  return {
    name: NAME,
    summary: SUMMARY,
    reviewed: { body: BODY, findings: [], verdict: 'clean' },
    scripts: 0,
    fromTheInternet,
  };
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Ekran z tą jedną umiejętnością czekającą na decyzję człowieka. */
function screenFor(item: Import): string {
  useSkills.setState({ pending: item, acknowledged: [], message: null, installed: [] });
  return renderToStaticMarkup(<SkillsShelf store={useSkills} />);
}

beforeEach(() => {
  useSkills.setState({
    pending: null,
    acknowledged: [],
    message: null,
    installed: [],
    adding: null,
  });
});

describe('the review card says where a skill came from only when it knows', () => {
  it('drops the sentence about the internet for a skill written here, and keeps it for a link', () => {
    const written = screenFor(anImport(false));
    const pasted = screenFor(anImport(true));

    /* KONTROLA PRZECIW PUSTEJ ASERCJI, i to ona jest tu połową kryterium: bez niej „nie ma
     * zdania o internecie" przechodzi na ekranie, który nie rysuje karty wcale. */
    for (const [how, markup] of [
      ['written here', written],
      ['pulled off a link', pasted],
    ] as const) {
      expect(
        markup,
        'the card for the skill ' +
          how +
          ' is not in the document at all, so everything below is about a screen that shows nothing',
      ).toContain('data-review-card');
      expect(markup, 'and it carries the name of that skill').toContain(NAME);
      expect(
        markup,
        'and the body it will tell the agent to follow. A card that does not show the body passes ' +
          'every check that says "there is no badge here" and cancels the only reason this screen ' +
          'exists: the person would be agreeing blind',
      ).toContain(BODY);
    }

    expect(
      /internet/i.test(written),
      'the card still says this skill came from the internet, and the person typed it here ' +
        'themselves. The badge stands in for the signing and provenance v1 does not have, so it ' +
        'has to be lit exactly where the content came from a stranger — a badge that is always on ' +
        'says nothing at all, and one that is sometimes wrong teaches people to skip it',
    ).toBe(false);

    expect(
      /internet/i.test(pasted),
      'and the card for the one pulled off a link stopped saying it. Without this half the ' +
        'assertion above also passes on a card that dropped the badge altogether — and then a ' +
        'skill written by a stranger looks exactly like one this person wrote by hand, which is ' +
        'the more expensive of the two lies',
    ).toBe(true);
  });

  it('says where the skill will land ABOVE the decision, whichever way it got here', () => {
    for (const fromTheInternet of [false, true]) {
      const how = fromTheInternet ? 'pulled off a link' : 'written here';
      const markup = screenFor(anImport(fromTheInternet));

      expect(
        occurrences(markup, 'data-add'),
        'the screen for the skill ' +
          how +
          ' offers no single way forward, or offers two. The positions compared below only mean ' +
          'something when there is exactly one decision on the page',
      ).toBe(1);

      const warning = markup.indexOf(WHERE_IT_LANDS);
      const decision = markup.indexOf('data-add');
      expect(
        warning,
        'the sentence saying where this goes is missing from the screen for the skill ' +
          how +
          '. This section is the only place in Loadout that writes outside its own library: what ' +
          'it saves enters every later run of the agent apps on this machine, also outside Loadout',
      ).toBeGreaterThanOrEqual(0);
      expect(
        decision,
        'and the control that carries out that decision is missing too',
      ).toBeGreaterThanOrEqual(0);
      expect(
        warning,
        'the sentence saying where this goes stands BELOW the control that does it, for the skill ' +
          how +
          '. A warning read after the decision is not a warning',
      ).toBeLessThan(decision);
    }
  });
});
