/* Jedna skończona kwota dociera do dysku RAZ, choć pole ma dwa zakończenia pisania.
 *
 * ZMIERZONE 2026-08-31. Pole „Default spend limit $" oddaje kwotę dyskowi i po Enterze,
 * i po wyjściu z pola — oba zakończenia są prawdziwe, bo człowiek raz kończy liczbę jednym,
 * raz drugim. Kto wcisnął Enter i DOPIERO POTEM kliknął gdzie indziej, robił jedno i drugie,
 * a poprzednia wersja wysyłała wtedy tę samą kwotę dwa razy: warunek `typing === null` tego
 * nie łapał, bo `setTyping(null)` z pierwszego wywołania ląduje dopiero po `await`, a drugie
 * zdarzenie pada w TYM SAMYM renderze i widzi dalej starą wartość. Skutek: dwa zapisy pliku
 * `~/.loadout/settings.json` na jedną decyzję i dwa zdania odmowy na jedną pomyłkę.
 *
 * DWA WYWOŁANIA NA JEDNEJ WARTOŚCI `typed` to nie jest scena naciągnięta — to jest DOKŁADNIE
 * to, co robi React: domknięcie jednego renderu trzyma jedną wartość, a oba zdarzenia pola
 * padają w nim, zanim ekran zdąży się przemalować.
 *
 * ZAPADKA MA SIĘ ZDEJMOWAĆ. Kryterium, które sprawdza wyłącznie „nie dwa razy", przechodzi
 * także kontrolka zablokowana na zawsze po pierwszym zapisie — czyli druga wada zamiast
 * pierwszej (niezmiennik 16). Dlatego stoją tu obok siebie oba pytania.
 */
import { describe, expect, it, vi } from 'vitest';

import { saveTheAmountOnce } from './index';
import type { LastAmountSent } from './index';

/** Kwota, którą człowiek wystukał. */
const TYPED = '40';

/** Zdanie, którym dysk odmawia — brzmienie bez znaczenia, liczy się to, że nie jest `null`. */
const REFUSED = 'Loadout could not write that amount down.';

/** Zapis, który wraca dopiero wtedy, kiedy kryterium go puści. */
function heldUntilReleased(refusal: string | null = null): {
  readonly save: ReturnType<typeof vi.fn>;
  readonly release: () => void;
} {
  const waiting: Array<(answer: string | null) => void> = [];
  return {
    save: vi.fn(
      async () =>
        new Promise<string | null>((done) => {
          waiting.push(done);
        }),
    ),
    release: () => {
      while (waiting.length > 0) waiting.pop()?.(refusal);
    },
  };
}

function box(): LastAmountSent {
  return { current: null };
}

describe('the amount a person types into Settings', () => {
  it('reaches the disk once when Enter is followed by leaving the field', async () => {
    const { save, release } = heldUntilReleased();
    const lastSent = box();
    const taken = vi.fn();
    const one = { typed: TYPED, lastSent, save, said: () => undefined, taken };

    /* Enter, a zaraz po nim wyjście z pola — obie drogi z tego samego renderu. */
    const enter = saveTheAmountOnce(one);
    const leaving = saveTheAmountOnce(one);
    release();
    await Promise.all([enter, leaving]);

    expect(
      save,
      'pressing Enter and then leaving the field wrote the same amount down twice. One decision ' +
        'became two writes of the same file, and a single mistyped amount came back as two ' +
        'refusals on one screen',
    ).toHaveBeenCalledTimes(1);
    expect(taken, 'the draft was dropped once for one accepted amount').toHaveBeenCalledTimes(1);
  });

  it('lets the next amount through, so the field does not seize after one save', async () => {
    const { save, release } = heldUntilReleased();
    const lastSent = box();
    const first = saveTheAmountOnce({
      typed: TYPED,
      lastSent,
      save,
      said: () => undefined,
      taken: () => undefined,
    });
    release();
    await first;

    const second = saveTheAmountOnce({
      typed: '65',
      lastSent,
      save,
      said: () => undefined,
      taken: () => undefined,
    });
    release();
    await second;

    expect(
      save,
      'a person changed their mind, typed another amount and the screen swallowed it. A field ' +
        'that takes one answer for the life of the screen is worse than the doubled write it ' +
        'was meant to stop',
    ).toHaveBeenCalledTimes(2);
  });

  it('keeps the draft after a refusal and lets the same amount go again once retyped', async () => {
    const { save, release } = heldUntilReleased(REFUSED);
    const lastSent = box();
    const taken = vi.fn();
    const heard: (string | null)[] = [];
    const one = {
      typed: TYPED,
      lastSent,
      save,
      said: (sentence: string | null) => heard.push(sentence),
      taken,
    };

    const enter = saveTheAmountOnce(one);
    const leaving = saveTheAmountOnce(one);
    release();
    await Promise.all([enter, leaving]);

    expect(save, 'a refused amount was still written down twice').toHaveBeenCalledTimes(1);
    expect(heard, 'one refusal for one refused amount').toEqual([REFUSED]);
    expect(
      taken,
      'the refused amount was wiped out of the field, taking away the one thing a person has ' +
        'to correct',
    ).not.toHaveBeenCalled();

    /* Każdy klawisz w polu zdejmuje zapadkę (`index.tsx`, `onChange`), więc ta sama kwota
       wystukana po odmowie jedzie na dysk jeszcze raz. */
    lastSent.current = null;
    const again = saveTheAmountOnce(one);
    release();
    await again;

    expect(
      save,
      'a person retyped the refused amount and nothing happened, so the only way out of a ' +
        'refusal was to type a different number',
    ).toHaveBeenCalledTimes(2);
  });
});
