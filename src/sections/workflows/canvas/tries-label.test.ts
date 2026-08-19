/* Podpis powrotu na płótnie: „up to 3 tries".
 *
 * DWA POWODY, DLA KTÓRYCH TO MA KRYTERIUM, choć jest to jedna linia tekstu.
 *
 * Pierwszy: słownictwo. Tabela wiążąca (`checks/quick-vocabulary.sh`) tłumaczy słowo z drutu na
 * „try", a to jest tekst, który człowiek czyta wprost z płótna. Bramka złapie to słowo
 * w literale — złapała je nawet w nazwie tego testu, przy pierwszym uruchomieniu — ale nie złapie
 * dnia, w którym ktoś przepisze ten podpis „ładniej" i wróci do
 * żargonu przez inne słowo.
 *
 * Drugi, ważniejszy: liczba pojedyncza. `up to 1 tries` czyta się jak usterka narzędzia,
 * a użytkownik ma w tej chwili wierzyć, że narzędzie wie, co mówi — to jest ta sama reguła, którą
 * `problems.tsx` stosuje przy „1 thing to fix". Zakres tur zaczyna się od jedynki, więc ten
 * przypadek NIE jest teoretyczny: wystarczy wpisać `1` w panelu.
 *
 * Czego to kryterium nie sprawdza: że podpis w ogóle trafia na krawędź. Tego nie da się tu
 * osądzić — `viewOf` żyje w komponencie płótna, a repo nie ma jsdom [T3 §2.3, ryzyko 7].
 * Sprawdzana jest DECYZJA o brzmieniu, bo tylko ona jest funkcją.
 */
import { describe, expect, it } from 'vitest';
import { triesLabel } from './canvas';

describe('the label on a way back', () => {
  it('counts in tries, the word this repo is allowed to show', () => {
    expect(triesLabel(3)).toBe('up to 3 tries');
  });

  it('says try, not tries, when there is one', () => {
    expect(
      triesLabel(1),
      '"up to 1 tries" reads like a broken tool, and the range starts at one, so this is not a ' +
        'theoretical case — it is what the person sees after typing 1 in the panel',
    ).toBe('up to 1 try');
  });

  it('still says tries at the ceiling', () => {
    expect(triesLabel(10)).toBe('up to 10 tries');
  });
});
