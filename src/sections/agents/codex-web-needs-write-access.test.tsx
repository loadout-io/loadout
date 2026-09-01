/* AC-2 dla T-97: agent Codeksa „look only" z włączoną siecią słyszy, że sieci nie dostanie.
 *
 * # Po co to istnieje
 *
 * Przełącznik „Can it reach the web" znaczy u każdego vendora co innego, i to nie jest szczegół
 * adaptera — to jest ustawienie, które człowiek włącza i które nie zadziała. Claude dostaje dwa
 * czasowniki (`WebFetch`, `WebSearch`) na KAŻDEJ pozycji diala. Codex sięga do sieci wyłącznie
 * przez piaskownicę, a ta otwiera się dopiero przy `workspace-write` — czyli przy „ask first"
 * i „work freely". Agent Codeksa na „look only" z włączoną siecią **nie ma sieci**, formularz
 * przyjmuje ten wybór bez słowa, a z zewnątrz wygląda to jak agent, który nie chciał poszukać.
 *
 * # Słaba wersja tego kryterium
 *
 * `expect(html).toContain(SENTENCE)` dla jednego wariantu. Przechodzi dla zdania wyrenderowanego
 * na stałe pod przełącznikiem — czyli dla formularza, który mówi to samo agentowi, którego to nie
 * dotyczy, i uczy człowieka przewijać wzrokiem uwagi, bo żadna nic nie znaczy. Rozróżniają to
 * trzy asercje negatywne: dwie pozostałe pozycje diala i drugi vendor.
 *
 * # Gdzie stoi ta kontrolka — 2026-08-31
 *
 * Wiersz `Can it reach the web` przeprowadził się spod `More settings` MIĘDZY WIDOCZNE: to jest
 * pytanie o uprawnienie, tej samej rangi co dial plikowy nad nim, a uprawnienie schowane pod
 * przyciskiem „więcej ustawień" jest uprawnieniem, którego się nie widzi. Kryterium jedzie za
 * nim i od dziś sądzi CAŁY formularz — czyli powierzchnię, na którą człowiek naprawdę patrzy
 * (niezmiennik 29) — zamiast jednego rozwinięcia z osobna.
 *
 * # Druga słaba wersja: `if (vendor === 'codex')` w komponencie
 *
 * Przechodzi każdy test o markupie i jest dokładnie tym, jak w repo źródłowym po cichu umarło
 * skanowanie sekretów — polityka przepisana per adapter, po jednej kopii na vendora
 * (niezmiennik 23). Dlatego ostatni test pyta o TABELĘ, nie o ekran: fakt „ta aplikacja sięga do
 * sieci dopiero wtedy, gdy może zmieniać pliki" jest danymi obok pozostałych faktów o vendorze.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Agent, FileAccess, Vendor } from '../../state/agents';
import { AgentForm } from './agent-form';
import { webIsOutOfReach } from './capabilities';

/** Zdanie odpowiada na jedyne pytanie, które w tym miejscu pada: „to dostanę czy nie dostanę". */
const SENTENCE =
  'Codex only reaches the web when it can change files, so this agent will not get it.';

const FORGE: Agent = {
  schema: 1,
  id: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f77',
  name: 'Forge',
  summary: 'Writes code',
  color: 'clay',
  instructions: 'Write the smallest change that makes the checks pass.',
  runsWith: 'claude-code',
  model: 'opus',
  thinking: 'balanced',
  fileAccess: 'work-freely',
  giveUpAfterMinutes: 20,
  tools: 'everything',
  reachesTheWeb: false,
  skills: [],
  connections: [],
  writeResultsTo: 'handoffs/build.md',
};

function noop(): void {
  /* sterowany formularz: w statycznym renderze nic tego nie woła */
}

function markupFor(runsWith: Vendor, fileAccess: FileAccess, reachesTheWeb: boolean): string {
  return renderToStaticMarkup(
    <AgentForm
      value={{ ...FORGE, runsWith, fileAccess, reachesTheWeb }}
      expanded={false}
      onChange={noop}
      onToggleMore={noop}
      onSave={noop}
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

describe('a Codex agent that may only look is told the web switch will not reach', () => {
  it('says so under the switch when the agent may only look', () => {
    expect(
      plain(markupFor('codex', 'look-only', true)),
      'this agent app reaches the web through its sandbox, and the sandbox only opens once it ' +
        'may change files. The person switched the web on, this agent will not get it, and the ' +
        'form said nothing — from the outside that is indistinguishable from an agent that ' +
        'chose not to search',
    ).toContain(SENTENCE);
  });

  it('says nothing once the agent may change files', () => {
    for (const dial of ['ask-first', 'work-freely'] as const) {
      expect(
        plain(markupFor('codex', dial, true)),
        'on ' +
          dial +
          ' this agent app does reach the web, so there is nothing to warn about. A note ' +
          'rendered whether or not it applies teaches people to skim past every note',
      ).not.toContain(SENTENCE);
    }
  });

  it('says nothing when the web switch is off', () => {
    expect(
      plain(markupFor('codex', 'look-only', false)),
      'nobody asked for the web here, so nothing was promised and nothing needs taking back',
    ).not.toContain(SENTENCE);
  });

  it('never says it about the other agent app', () => {
    for (const dial of ['look-only', 'ask-first', 'work-freely'] as const) {
      expect(
        plain(markupFor('claude-code', dial, true)),
        'Claude Code reaches the web on every dial position, so this sentence is simply untrue ' +
          'about it. It came up on ' +
          dial,
      ).not.toContain(SENTENCE);
    }
  });

  it('keeps the fact in the table of what each app can do, not in the form', () => {
    // Oczekiwane odpowiedzi wypisane TUTAJ, a nie czytane z tabeli, którą sprawdzają: pętla po
    // samej tabeli pytałaby ją o nią samą i przeszłaby dla tabeli pustej.
    const ANSWERS: Array<[Vendor, FileAccess, boolean]> = [
      ['codex', 'look-only', true],
      ['codex', 'ask-first', false],
      ['codex', 'work-freely', false],
      ['claude-code', 'look-only', false],
      ['claude-code', 'ask-first', false],
      ['claude-code', 'work-freely', false],
    ];

    for (const [vendor, dial, expected] of ANSWERS) {
      expect(
        webIsOutOfReach(vendor, dial),
        vendor +
          ' on ' +
          dial +
          ' has to answer ' +
          String(expected) +
          ' out of the table of what each app can do. Written as a condition inside the form ' +
          'instead, this is one more copy of policy per vendor — which is exactly how the ' +
          'secret scanning in the source repo quietly died (invariant 23)',
      ).toBe(expected);
    }
  });
});
