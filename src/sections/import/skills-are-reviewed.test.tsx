/* Umiejętność wniesiona przez „Import setup" ma przejść przez TĘ SAMĄ zgodę, co wklejona
 * linkiem — a do 2026-08-31 nie przechodziła przez żadną.
 *
 * ZMIERZONE. `SKILL.md` wchodzi do produktu dwiema drogami. Wklejony linkiem staje przed kartą
 * przeglądu (`src/sections/skills/review-card.tsx`): ukryty tekst, próba nadpisania instrukcji
 * i linia wysyłająca dane stoją na ekranie, a blokujące znaleziska trzeba odklikać po jednym
 * („I have read this"). Ten sam plik znaleziony w cudzym projekcie wchodził jednym kliknięciem
 * „Import", bez ani jednego zdania o tym, co w nim jest.
 *
 * Zgoda jest warunkiem WNIESIENIA, nie ozdobą wiersza: bez niej przycisk kończący import jest
 * wyłączony, a jedynym wyjściem zostaje odznaczenie tej pozycji.
 *
 * TEN PLAN NIE NIESIE PRZEGLĄDU i to jest jego rola w suicie (2026-08-31). Znaleziska mają od
 * dziś drut do okna (`ImportItem::reviewed`) i ekran je pokazuje — sądzi to
 * `./findings-reach-the-screen.test.tsx`. Tutaj `reviewed` NIE MA ani przy jednej pozycji,
 * bo tak wygląda plan sprzed tego drutu i tak wygląda pozycja, przy której przegląd się nie
 * odbył. Ekran ma wtedy prosić o przeczytanie pliku i nie mówić o znaleziskach ANI SŁOWA
 * w żadną stronę: cisza po przeglądzie, którego nie było, jest jedyną prawdziwą odpowiedzią.
 *
 * KRYTERIUM SĄDZI ZDANIE NA EKRANIE (niezmiennik 29): `renderToStaticMarkup` nad prawdziwym
 * `ImportSetup`, na prawdziwej ścieżce propsów, a nie wartość zwrócona przez `mustBeRead`.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { ImportSetup, type ImportPreview } from './setup';
import { BECOMES_INSTRUCTIONS, OPEN_IT, READ_IT, STOPS_IT } from './skill-review';

const SKILL = 'audit';
const AGENT = 'builder';

/** Plan z jedną umiejętnością, której przegląd RUSZYŁ tekst albo coś w nim znalazł
 *  (`adjusted`), i z jednym agentem, którego nie ruszył (`exact`). Dwa wiersze, żeby
 *  „ta pozycja" znaczyło więcej niż „jedyna pozycja". */
const PREVIEW: ImportPreview = {
  snapshot: {
    root: '/project',
    items: [
      {
        id: SKILL,
        source: 'claude',
        kind: 'skill',
        path: '.claude/skills/audit/SKILL.md',
        name: 'audit',
        summary: 'Audits the project.',
      },
      {
        id: AGENT,
        source: 'claude',
        kind: 'agent',
        path: '.claude/agents/builder.md',
        name: 'Builder',
        summary: 'Builds the project.',
      },
    ],
  },
  draft: {
    sourceHashes: { '.claude/skills/audit/SKILL.md': 'h-skill' },
    items: [
      {
        id: SKILL,
        kind: 'skill',
        sources: [
          {
            provider: 'claude',
            path: '.claude/skills/audit/SKILL.md',
            hash: 'h-skill',
            role: 'definition',
          },
        ],
        target: 'skills/audit/SKILL.md',
        dependencies: [],
        status: 'ready',
        statusMessage: 'This skill was normalized and reviewed before import.',
        generatedHash: null,
      },
      {
        id: AGENT,
        kind: 'agent',
        sources: [
          {
            provider: 'claude',
            path: '.claude/agents/builder.md',
            hash: 'h-agent',
            role: 'definition',
          },
        ],
        target: 'agents/builder.md',
        dependencies: [],
        status: 'ready',
        statusMessage: 'Loadout can bring this over as it is.',
        generatedHash: null,
      },
    ],
    agents: [],
    skills: [{ name: SKILL }],
    connections: [],
    workflows: [],
    report: {
      mappings: [
        {
          itemId: SKILL,
          compatibility: 'adjusted',
          message: 'This skill was normalized and reviewed before import.',
        },
        { itemId: AGENT, compatibility: 'exact', message: 'The format can be reproduced.' },
      ],
    },
  },
};

function screen(preview: ImportPreview = PREVIEW): string {
  return renderToStaticMarkup(
    <ImportSetup initialPreview={preview} onClose={() => undefined} onImported={() => undefined} />,
  );
}

/** Atrybuty przycisku, który kończy cały import. Po `data-`, bo słowo „Import" stoi na tym
 *  ekranie także w tytule i przy ptaszku każdego wiersza. */
function importButton(html: string): string {
  return /<button([^>]*data-import-now[^>]*)>/.exec(html)?.[1] ?? '';
}

describe('a skill that comes in through Import setup', () => {
  it('cannot be brought in before the person says they read it', () => {
    const html = screen();

    /* Kontrola przeciw pustej asercji: ten wiersz naprawdę stoi na ekranie i naprawdę jest
     * umiejętnością — bez tego wszystko niżej mówiłoby o pozycji, której nie ma. */
    expect(html, 'the scan put no row for the thing this test is about').toContain(
      '.claude/skills/audit/SKILL.md',
    );
    expect(
      importButton(html),
      'the screen stopped offering the action this test is about',
    ).not.toBe('');

    expect(
      importButton(html),
      'somebody else wrote this file and it becomes instructions for your agents. One click ' +
        'brings it in, and nothing on the way asked whether anybody read it',
    ).toContain('disabled');
    expect(
      html,
      'the row offers no way to say the file was read, so the only thing the person can do is ' +
        'take it out of the import',
    ).toContain(READ_IT);
  });

  it('says on the row why it is waiting, and what to do about it', () => {
    const html = screen();

    expect(
      html,
      'the row never says that this file becomes instructions the agents follow',
    ).toContain(BECOMES_INSTRUCTIONS);
    expect(
      html,
      'nothing came over the wire about this one, and the row talks as if something had. ' +
        'A sentence about a reading that never happened is worse than no sentence',
    ).not.toContain(STOPS_IT);
    expect(html, 'the sentence names no next move for the person').toContain(OPEN_IT);
    expect(html, 'the action says nothing about why it is off').toContain(
      '1 skill(s) here have not been read yet.',
    );
  });

  it('leaves a skill alone when the review changed nothing in it', () => {
    const untouched: ImportPreview = {
      ...PREVIEW,
      draft: {
        ...PREVIEW.draft,
        report: {
          mappings: PREVIEW.draft.report.mappings.map((mapping) =>
            mapping.itemId === SKILL
              ? { ...mapping, compatibility: 'exact' as const, message: 'It can be imported.' }
              : mapping,
          ),
        },
      },
    };
    const html = screen(untouched);

    /* Kontrola przeciw pustej asercji: to jest ten sam ekran i ten sam wiersz. */
    expect(html, 'the scan put no row for the thing this test is about').toContain(
      '.claude/skills/audit/SKILL.md',
    );
    expect(
      html,
      'a skill that came over byte for byte, with nothing found in it, still asks the person ' +
        'to click. A question with one possible answer is asked once too often',
    ).not.toContain(READ_IT);
    expect(importButton(html), 'nothing is waiting, and the action is still off').not.toContain(
      'disabled',
    );
  });
});
