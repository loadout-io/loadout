/* Kryterium 6 dla T-11: przy Codeksie `Tools` jest wygaszone jednym zdaniem, a przy
 * Claude'ie działa.
 *
 * Słaba wersja tego kryterium to `expect(html).toContain("Codex doesn't have this")`. Ona
 * przechodzi dla zdania wyrenderowanego na stałe pod kontrolką, która działa normalnie —
 * czyli dla interfejsu, który kłamie w obie strony naraz: mówi „tego tu nie ma" i pozwala
 * to ustawić. Rozróżnia to para asercji na `disabled` (jest przy Codeksie, nie ma przy
 * Claude'ie) plus asercja negatywna na samo zdanie w wariancie Claude'a.
 *
 * Trzeci test jest właściwym testem niezmiennika 23. Trzy pytania o macierz nie wystarczą:
 * przechodzą dla trzech `if`-ów rozsianych po komponencie. Dlatego niżej stoi CAŁA macierz
 * z T4 §6.3 — jeśli odpowiedzi produkują warunki, a nie tabela, to któraś para pól i vendora
 * zostanie bez odpowiedzi i ten test to pokaże. Tak umarło skanowanie sekretów w repo
 * źródłowym: polityka przepisana per adapter, po jednej kopii na vendora.
 *
 * Oczekiwana macierz jest wypisana TUTAJ, na sztywno, a nie czytana z tabeli, którą sprawdza.
 * Pętla po `CAPABILITIES` pytałaby tabelę o nią samą i przeszłaby dla tabeli pustej.
 *
 * `fileAccess` jest przy obu aplikacjach przybliżeniem, bo tak wychodzi z §6.3: u Claude'a
 * `look-only` to `--permission-mode plan`, u Codeksa `ask-first` i `work-freely` to ten sam
 * `workspace-write`. Pole odpowiada najsłabszym ze swoich tłumaczeń — inaczej „native"
 * znaczyłoby „część działa dokładnie", a to jest zdanie, którego nie chcemy mówić o dialu
 * bezpieczeństwa.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Agent, Vendor } from '../../state/agents';
import type { Capability, CapabilityField } from './capabilities';
import { capability } from './capabilities';
import { MoreSettings } from './more-settings';

/** Jedno zdanie, bez ikony ostrzeżenia, bez modala, bez czerwieni [T4 §8.1]. */
const SENTENCE = "Codex doesn't have this. It uses the 'Can change files' setting instead.";

/** Cała tabela z T4 §6.3, przepisana ręcznie, po jednej parze na wiersz. */
const MATRIX: Array<[CapabilityField, Vendor, Capability]> = [
  ['instructions', 'claude-code', 'native'],
  ['instructions', 'codex', 'native'],
  ['model', 'claude-code', 'native'],
  ['model', 'codex', 'native'],
  ['thinking', 'claude-code', 'native'],
  ['thinking', 'codex', 'native'],
  ['fileAccess', 'claude-code', 'approximate'],
  ['fileAccess', 'codex', 'approximate'],
  ['tools', 'claude-code', 'native'],
  ['tools', 'codex', 'unavailable'],
  ['skills', 'claude-code', 'native'],
  ['skills', 'codex', 'approximate'],
  ['connections', 'claude-code', 'native'],
  ['connections', 'codex', 'native'],
  ['giveUpAfterMinutes', 'claude-code', 'native'],
  ['giveUpAfterMinutes', 'codex', 'native'],
];

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

function markupFor(runsWith: Vendor): string {
  return renderToStaticMarkup(<MoreSettings value={{ ...FORGE, runsWith }} onChange={noop} />);
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

/** Atrybuty elementu niosącego `data-field`, albo `null`, kiedy takiego elementu nie ma. */
function controlAttributes(html: string, field: string): string | null {
  const hit = new RegExp('<[a-zA-Z]+\\b([^>]*\\bdata-field="' + field + '"[^>]*)>').exec(html);
  return hit === null ? null : (hit[1] ?? '');
}

describe('with Codex the tools setting is greyed out with a sentence, with Claude it works', () => {
  it('greys out the tools setting for Codex and says why, once', () => {
    const html = markupFor('codex');
    const attributes = controlAttributes(html, 'tools');

    expect(
      attributes,
      'the tools setting has to be on screen for Codex too, greyed out. Removing the row ' +
        'instead would make the form change shape when you switch the agent app',
    ).not.toBeNull();
    expect(
      /\bdisabled\b/.test(attributes ?? ''),
      'and it has to be genuinely unusable, not merely painted grey. A control that looks off ' +
        'and still writes a value is the worse half of the lie',
    ).toBe(true);
    expect(
      plain(html),
      'and one plain sentence says why. No warning icon, no modal, no red — this is a fact ' +
        'about the other app, not a mistake the user made',
    ).toContain(SENTENCE);
  });

  it('leaves the tools setting alone for Claude Code, and says nothing', () => {
    const html = markupFor('claude-code');
    const attributes = controlAttributes(html, 'tools');

    expect(attributes, 'the tools setting has to be on screen').not.toBeNull();
    expect(
      /\bdisabled\b/.test(attributes ?? ''),
      'Claude Code has a real per-tool list, so the control works. Greying it out here would ' +
        'take away a setting the app actually has',
    ).toBe(false);
    expect(
      plain(html),
      'and the sentence about Codex must not be on screen at all. A note rendered whether or ' +
        'not it applies is how a form starts explaining things that are not true',
    ).not.toContain(SENTENCE);
  });

  it('answers for every setting and every agent app out of one table', () => {
    for (const [field, vendor, expected] of MATRIX) {
      expect(
        capability(field, vendor),
        field +
          ' with ' +
          vendor +
          ' is ' +
          expected +
          ' in the table verified on 2026-08-15. Every pair has to have an answer: a pair with ' +
          'none is a control the form will not know how to draw',
      ).toBe(expected);
    }
  });
});
