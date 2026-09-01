/* Zdanie o nieudanym zapisie stoi TAM, GDZIE PRZYCISK, KTÓRY JE WYWOŁAŁ.
 *
 * WADA, zgłoszona przez właściciela 2026-08-31 i zmierzona na prawdziwym oknie. Pasek odmowy
 * stał NAD wierszem z listą i panelem, a panel przewija się osobno i ma dziewięć pól. Sekwencja
 * jest zwyczajna: przewijasz panel na dół, klikasz `Save`, dysk odmawia — i zdanie pojawia się
 * na górze LEWEJ kolumny, poza kadrem. Kliknięcie wygląda dokładnie jak martwe, a martwy `Save`
 * w tej sekcji to nie jest hipoteza: to jest przyczyna, dla której `~/.loadout/agents` nie
 * istniał na maszynie właściciela przez kilkanaście godzin (nagłówek `./index.tsx`).
 *
 * DLACZEGO REGUŁA BRZMI „PANEL OTWARTY → ZDANIE W PANELU", a nie „zapis → panel".
 * `AgentsState.refusal` jest JEDNYM polem na całą sekcję z rozmysłu (niezmiennik 13, powód
 * w `src/state/agents.ts`), więc miejsce nie może brać się z rodzaju czynności bez dołożenia
 * drugiego pola, o którym ktoś zapomni. Bierze się więc z faktu, który już na ekranie stoi:
 * WSZYSTKIE trzy kontrolki, które w tej sekcji mogą dostać odmowę przy otwartym panelu — Save,
 * Duplicate, Delete — są przyciskami W TYM PANELU. Kiedy panel jest zamknięty, jedyną czynnością
 * jest odczyt biblioteki i zdanie wraca pod nagłówek.
 *
 * SŁABĄ WERSJĄ jest `expect(markup).toContain(said)`. Przechodziła dla wady opisanej wyżej
 * w każdym jej dniu — zdanie BYŁO w dokumencie, tylko dwie kolumny obok. Dlatego to kryterium
 * pyta o POŁOŻENIE: w którym poddrzewie stoi i po którym przycisku.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Agent, AgentsIo } from '../../state/agents';
import { createAgentsStore } from '../../state/agents';
import AgentsScreen from './index';

/* Bez cudzysłowów w treści: `renderToStaticMarkup` zamienia `"` na `&quot;`, więc porównanie
 * surowego markupu ze zdaniem w cudzysłowach nie mogłoby przejść nigdy — a to jest pytanie
 * o POŁOŻENIE zdania, nie o encje. */
const SAID = 'the agents folder is read-only, so Forge was not written';

function agent(): Agent {
  return {
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
    writeResultsTo: '',
  };
}

/** Atrapa, w której dysk odmawia zapisu, a odczyt zwraca tego jednego agenta. */
function refusing(): AgentsIo {
  return {
    list: () => Promise.resolve([agent()]),
    newId: () => Promise.resolve('a-new'),
    save: () => Promise.reject(SAID),
    remove: () => Promise.resolve(),
  };
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Poddrzewo panelu: od otwierającego `<aside` do końca dokumentu. Panel jest ostatnią
 * powierzchnią tego ekranu, więc to wystarcza i nie wymaga liczenia zagnieżdżeń. */
function panelOf(markup: string): string {
  const at = markup.indexOf('<aside');
  return at < 0 ? '' : markup.slice(at);
}

/** Wszystko PRZED panelem: nagłówek, pasek sekcji i lista kafelków. */
function outsideThePanel(markup: string): string {
  const at = markup.indexOf('<aside');
  return at < 0 ? markup : markup.slice(0, at);
}

describe('a refused save says so next to the button that asked for it', () => {
  it('puts the sentence inside the open panel, under Save, and nowhere else', async () => {
    const store = createAgentsStore(refusing());
    await store.getState().load();
    await store.getState().save(agent());

    const markup = renderToStaticMarkup(
      <AgentsScreen store={store} usage={null} opened={agent()} />,
    );

    expect(
      markup,
      'the refusal never reached the screen at all, so nothing below means anything',
    ).toContain(SAID);
    expect(
      panelOf(markup),
      'the sentence is not in the panel. The person is looking at the panel — they scrolled it ' +
        'down and pressed Save in it — and the answer went to the top of the other column, out ' +
        'of the frame. From where they sit, the click did nothing.',
    ).toContain(SAID);
    expect(
      outsideThePanel(markup),
      'and it is not ALSO standing over the list. One fact, one live region (invariant 13): ' +
        'two copies of the same sentence read as two things going wrong.',
    ).not.toContain(SAID);

    const panel = panelOf(markup);
    expect(
      panel.indexOf('data-save'),
      'the panel has no Save button in it, so "under Save" cannot be measured',
    ).toBeGreaterThan(-1);
    expect(
      panel.indexOf(SAID),
      'the sentence stands ABOVE the Save button, which is the top of a nine-field form — that ' +
        'is the same scrolled-out-of-sight problem one column to the right. It belongs under ' +
        'the control that produced it.',
    ).toBeGreaterThan(panel.indexOf('data-save'));
  });

  it('leaves the panel open with what the person typed, and says which field it is about', async () => {
    const store = createAgentsStore(refusing());
    await store.getState().load();
    await store.getState().save(agent());

    const markup = renderToStaticMarkup(
      <AgentsScreen store={store} usage={null} opened={{ ...agent(), summary: 'Half typed' }} />,
    );

    expect(
      markup,
      'a refused save has to leave the panel standing with what was typed in it. Closing it ' +
        'throws away the whole definition and asks for it again',
    ).toContain('Half typed');
    expect(occurrences(markup, SAID), 'and the reason is said exactly once').toBe(1);
  });

  it('control: with no panel open the sentence goes back under the header', async () => {
    const store = createAgentsStore(refusing());
    await store.getState().load();
    await store.getState().save(agent());

    const markup = renderToStaticMarkup(<AgentsScreen store={store} usage={null} />);

    expect(
      markup,
      'without this control, "the sentence is not over the list" also passes on a screen that ' +
        'stopped showing refusals altogether',
    ).toContain(SAID);
    expect(
      outsideThePanel(markup),
      'with nothing open, the section bar is where it belongs — it is the only place there is',
    ).toContain(SAID);
  });
});
