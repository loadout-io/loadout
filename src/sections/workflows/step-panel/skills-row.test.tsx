/* Kryterium 5 dla T-13: kontrolka Skills obiecuje dokładnie tyle, ile potrafi CLI.
 *
 * Słaba wersja tego kryterium to `expect(html).toContain('All skills')`. Przechodzi w obu
 * trybach i przechodzi też dla martwego „Only these", które niczego nie zapisuje — czyli dla
 * UI obiecującego funkcję, której CLI nie umie dowieźć. Rozróżniają to dwie rzeczy: asercja
 * NEGATYWNA na „Only these" w trybie all-or-none oraz asercja na wartości zapisanej
 * w dokumencie po każdej dostępnej opcji.
 *
 * Tryb przychodzi propsem, choć w aplikacji jest jedną stałą (`SKILL_SUBSETTING`). Dlatego ten
 * plik sprawdza oba warianty niezależnie od tego, jak wypadł spike S-1 — jego wynik zmienia
 * jedną linię w `capabilities.ts` i zero testów tutaj.
 *
 * Trzeci przypadek jest wspólny dla obu trybów: przy agencie na Codeksie całego wiersza nie ma,
 * bo Codex nie ma pojęcia umiejętności [T3 §7.2, T4 fakt-check O4]. „Nie ma" znaczy nie ma —
 * wiersz wyszarzony dalej obiecuje, że kiedyś zadziała.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Vendor } from '../../../state/agents';
import type { AgentStep, Note, Skills, WorkflowFile, WorkflowIo } from '../../../state/workflows';
import { createWorkflowStore } from '../../../state/workflows';
import type { SkillMode } from './capabilities';
import { SkillsRow } from './skills-row';

const AVAILABLE = ['code-review', 'deep-research', 'dataviz'];
const MODES: SkillMode[] = ['subset', 'all-or-none'];

function build(): AgentStep {
  return {
    kind: 'agent',
    id: 's_build',
    name: 'Build',
    agent: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f77',
    overrides: {},
    copies: 1,
    instructions: 'Fix the failing parser tests. Keep the public API unchanged.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y: 168 },
  };
}

function file(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_ship_a_feature',
    name: 'Ship a feature',
    steps: [build()],
    links: [],
  };
}

function io(): WorkflowIo {
  return {
    save: () => Promise.resolve(),
    check: () => Promise.resolve([] as Note[]),
    saveAgent: () => Promise.resolve(),
  };
}

function skillsOf(doc: WorkflowFile): Skills {
  const hit = doc.steps.find((one) => one.id === 's_build');
  if (hit === undefined || hit.kind !== 'agent') {
    throw new Error('the document no longer holds the step this test put in it');
  }
  return hit.skills;
}

function noop(): void {
  /* sterowany wiersz: w statycznym renderze nic tego nie woła */
}

function markup(mode: SkillMode, runsWith: Vendor, value: Skills = 'all'): string {
  return renderToStaticMarkup(
    <SkillsRow
      mode={mode}
      runsWith={runsWith}
      available={AVAILABLE}
      value={value}
      onChoose={noop}
    />,
  );
}

function plain(fragment: string): string {
  return fragment
    .replace(/<[^>]*>/g, ' ')
    .replace(/&#x27;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('the Skills row promises exactly what the agent app can deliver', () => {
  it('offers the pick-a-few list when it is real, and writes down what was picked', () => {
    const html = markup('subset', 'claude-code');

    expect(plain(html), 'the default is everything the agent already has').toContain('All skills');
    expect(
      plain(html),
      'a per-run subset was measured to work, so the control that offers it is honest here',
    ).toContain('Only these');
    for (const skill of AVAILABLE) {
      expect(plain(html), 'and every skill on offer is on the list: ' + skill).toContain(skill);
    }

    const store = createWorkflowStore(io(), file());
    store.getState().chooseSkills('s_build', { only: ['code-review'] });

    expect(
      skillsOf(store.getState().document),
      'the point of the list is the value it writes. A list that shows and stores nothing is ' +
        'a promise the run cannot keep',
    ).toEqual(['code-review']);
  });

  it('drops Only these entirely in all-or-none mode, and both remaining options store a value', () => {
    const html = markup('all-or-none', 'claude-code');

    expect(plain(html), 'everything the agent has').toContain('All skills');
    expect(plain(html), 'or nothing at all — those are the two the flag can do').toContain(
      'No skills',
    );
    expect(
      html,
      'not greyed out: absent. A disabled control still says "this will work one day", and ' +
        'nobody is coming to switch it on',
    ).not.toContain('Only these');

    const store = createWorkflowStore(io(), file());
    store.getState().chooseSkills('s_build', 'none');
    expect(skillsOf(store.getState().document), 'nothing at all is an empty list').toEqual([]);

    store.getState().chooseSkills('s_build', 'all');
    expect(skillsOf(store.getState().document), 'and everything is the word, not a full list').toBe(
      'all',
    );
  });

  it('leaves the whole row out for Codex, in either mode, because Codex has no skills', () => {
    for (const mode of MODES) {
      expect(
        plain(markup(mode, 'claude-code')).length,
        'in ' +
          mode +
          ' the row renders something for Claude Code. Without this line the check below ' +
          'also passes for a component that renders nothing, ever',
      ).toBeGreaterThan(0);
      expect(
        plain(markup(mode, 'codex')),
        'in ' +
          mode +
          ' there is no Skills row for Codex at all. An enabled control that cannot do ' +
          'anything is worse than no control, because it looks the same as one that works',
      ).toBe('');
    }
  });
});
