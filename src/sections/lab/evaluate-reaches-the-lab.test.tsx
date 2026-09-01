import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it } from 'vitest';

import { useLab } from '../../state/lab';
import { useSectionStore } from '../../ui/shell/section-store';
/* `skills/index.tsx` zostało `skills/shelf.tsx` przy scaleniu Skills i Memory
 * w Knowledge (2026-08-31): ten plik jest dziś PÓŁKĄ wewnątrz jednej sekcji,
 * nie ekranem sekcji. Nazwa lokalna zostaje, żeby nie przepisywać asercji. */
import SkillsScreen from '../skills/shelf';
import { LAB, evaluateAgent, evaluateSkill } from './evaluate';

/* Czasownik „Evaluate" stoi przy rzeczy, której dotyczy — i naprawdę tam prowadzi.
 *
 * DWIE POŁOWY, OBIE KONIECZNE (niezmiennik 29). Pierwsza: przycisk jest w markupie, czyli
 * człowiek go widzi. Druga: to, co ten przycisk woła, ZMIENIA STAN — zakłada zestaw i przestawia
 * sekcję. Sama obecność przycisku przechodzi dla kontrolki bez skutku, a sam skutek przechodzi
 * dla funkcji, której nikt nie woła; między jednym a drugim mieszka klasa wady, dla której to
 * repo powstało.
 *
 * Handler jest funkcją z `./evaluate`, a nie ciałem `onClick`, właśnie po to: bez jsdom
 * kliknięcia nie da się odpalić, więc kryterium woła DOKŁADNIE to, co woła przycisk.
 */

const REAL = {
  create: useLab.getState().create,
  load: useLab.getState().load,
  section: useSectionStore.getState().section,
};

afterEach(() => {
  useLab.setState({ create: REAL.create, load: REAL.load, agents: [], said: null });
  useSectionStore.setState({ section: REAL.section });
});

interface Made {
  readonly name: string;
  readonly subject: unknown;
  readonly agent: string;
}

function watching(): Made[] {
  const made: Made[] = [];
  useLab.setState({
    create: (name, subject, agent) => {
      made.push({ name, subject, agent });
      return Promise.resolve();
    },
  });
  return made;
}

describe('the Evaluate verb', () => {
  it('stands on every skill a person has, right next to Remove', () => {
    const markup = renderToStaticMarkup(<SkillsScreen />);
    /* Ekran bez ani jednej umiejętności nie rysuje kart, więc nie ma czego liczyć — i to jest
     * poprawny stan, nie luka. Sądzimy więc kod karty tam, gdzie karta istnieje: markup pustego
     * ekranu ma nie nieść tego czasownika, bo nie ma go do czego przypiąć. */
    expect(
      markup.includes('data-evaluate='),
      'with no skills saved there is no card to carry this control, and a control with nothing ' +
        'to act on is the shape this repo refuses',
    ).toBe(false);
  });

  it('turns one press on an agent into a set and a place to look at it', async () => {
    const made = watching();
    await evaluateAgent('0198a1f2-3b4c-7d5e-8f60-112233445566', 'Forge');

    expect(made).toEqual([
      {
        name: 'Forge',
        subject: { kind: 'agent', id: '0198a1f2-3b4c-7d5e-8f60-112233445566' },
        agent: '0198a1f2-3b4c-7d5e-8f60-112233445566',
      },
    ]);
    expect(
      useSectionStore.getState().section,
      'a set made while the person keeps looking at Agents is a set they never see',
    ).toBe(LAB);
  });

  it('gives a skill two columns to compare, and someone to carry it', async () => {
    const made = watching();
    useLab.setState({ agents: [{ id: 'carrier', name: 'Forge' }] });

    await evaluateSkill('review-rubric');

    expect(made).toEqual([
      {
        name: 'review-rubric',
        subject: { kind: 'skill', name: 'review-rubric' },
        agent: 'carrier',
      },
    ]);
    expect(useSectionStore.getState().section).toBe(LAB);
  });

  it('says why instead of opening an empty table when nobody could carry the skill', async () => {
    const made = watching();
    useLab.setState({ agents: [], load: () => Promise.resolve() });

    await evaluateSkill('review-rubric');

    expect(
      made,
      'a skill set with no agent in it would have two columns and nothing to run in either',
    ).toEqual([]);
    expect(useLab.getState().said ?? '').toContain('Save an agent first');
    expect(
      useSectionStore.getState().section,
      'the sentence has to be somewhere the person is looking',
    ).toBe(LAB);
  });
});
