/* Wiersz wejścia mówi, DO KOGO pójdzie zdanie — i proza nigdy nie uruchamia biegu.
 *
 * PO CO TO ISTNIEJE. Rozstrzygnięcie właściciela 2026-08-19, dwa zdania w jednym: „nie powinno być
 * tak, że jak piszę bez komendy, a poprzednio odpaliłem komendę, to się ona na nowo całe workflow
 * odpala" oraz „powinienem wiedzieć co piszę". Wcześniej stała tu wersja, w której proza przy pustym
 * ekranie STARTOWAŁA wybrany workflow i stawała się jego zadaniem — wyglądało to na wygodę i było
 * pułapką: to samo naciśnięcie Enter raz dopowiadało zdanie agentowi, a raz kupowało bieg sześciu
 * agentów, i różnicy nie było widać w polu, w które człowiek pisał.
 *
 * Rozstrzygnięcie: sztywny przebieg zaczyna WYŁĄCZNIE komenda, a wiersz mówi pod polem, gdzie
 * pójdzie zdanie — zanim ktokolwiek naciśnie Enter.
 *
 * SŁABA WERSJA: `expect(whereItGoes([])).not.toBe('')`. Przechodzi dla jednego zdania na wszystkie
 * trzy stany, czyli dla wiersza, który wygląda identycznie, gdy zdanie dojdzie, gdy trzeba wybrać
 * adresata i gdy nie ma go komu doręczyć — a to jest dokładnie ten stan, który właściciel zgłosił.
 * Rozstrzyga to, że każdy stan nazywa NASTĘPNY RUCH, i że nazwy adresatów da się z tego przepisać.
 */
import { describe, expect, it } from 'vitest';

import { whereItGoes } from './entry';

describe('the row says where the line will go', () => {
  it('names the one agent that will get it', () => {
    const said = whereItGoes(['Forge']);

    expect(
      said,
      'with exactly one agent working there is nothing to choose, so the row has to name the ' +
        'addressee rather than leave the person to find out by spending a turn',
    ).toContain('Forge');
  });

  it('lists the names when there is more than one, because one has to be typed', () => {
    const said = whereItGoes(['Forge', 'Needle']);

    expect(said, 'the count alone states a problem without stating the fix').toContain('2 agents');
    /* NAZWY, w postaci do przepisania: przy kilku pracujących adres wpisuje się na początku linii,
     * dokładnie tak, jak każe odmowa `RunError::SeveralAreWorking` po stronie Rusta. */
    expect(said, 'the first name has to be visible to be typed').toContain('Forge');
    expect(said, 'and so does the second').toContain('Needle');
  });

  it('with nobody working, says the lead agent gets it — and that only a command starts work', () => {
    const said = whereItGoes([]);

    /* Ta gałąź jest sednem obu rozstrzygnięć z 2026-08-19. Wcześniej proza w tym stanie po cichu
     * URUCHAMIAŁA bieg; potem, przez chwilę, nie miała adresata wcale. Teraz ma rozmowę — i zdanie
     * musi nieść OBA fakty naraz, bo pierwszy bez drugiego jest dokładnie tą obietnicą, którą
     * właściciel odrzucił. */
    expect(
      said.toLowerCase(),
      'prose with nothing running is a conversation with the lead agent, and the row has to say ' +
        'who is on the other side',
    ).toContain('lead agent');
    expect(
      said,
      'and it has to say that work still begins only with a command — otherwise the sentence ' +
        'reads as "write here and it will build it", which is what was just removed',
    ).toContain('/run');
  });

  it('never promises a run in any of the three states', () => {
    /* Kryterium regresyjne wobec usuniętej drogi: żadne z tych zdań nie ma prawa obiecywać, że
     * napisanie czegokolwiek zbuduje cokolwiek. Słowo „build" wróci tu dopiero z prawdziwym czatem
     * orchestratora, i wtedy to kryterium ma o tym powiedzieć. */
    for (const working of [[], ['Forge'], ['Forge', 'Needle']]) {
      expect(
        whereItGoes(working).toLowerCase(),
        'the row must not offer to build anything: typing prose is a conversation, and only a ' +
          'command starts a run',
      ).not.toContain('will build');
    }
  });
});
