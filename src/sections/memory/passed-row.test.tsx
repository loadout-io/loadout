/* Trzy funkcje czyste z `passed-row.tsx`: nazwa pliku, rozmiar i „kto komu".
 *
 * DLACZEGO WPROST NA FUNKCJACH, A NIE PRZEZ MARKUP. W tym repo nie ma jsdom, więc render
 * sprawdza wyłącznie, co stoi w dokumencie — a te trzy odpowiadają na pytania, które mają
 * ODPOWIEDZI, nie tylko obecność. Sprawdzenie przez markup zmierzyłoby jedną wartość na
 * funkcję; tu mierzymy granice, na których każda z nich się psuje.
 *
 * JAK BRZMIAŁABY SŁABA WERSJA I CO JĄ ODRÓŻNI. Słaba wersja to jedno wywołanie na funkcję
 * z ładną wartością w środku: `sizeLabel(3174) === '3.1 KB'`. Przechodzi na implementacji, która
 * dzieli przez 1000 zamiast 1024 (błąd 2,4% — niewidoczny na jednej liczbie), i przechodzi na
 * takiej, która NIGDY nie mówi w bajtach, więc plik o 840 bajtach czyta się jako „0.8 KB".
 * Odróżnia je granica 1024 z obu stron i pozycja poniżej niej.
 *
 * `whoToWho` z pustą listą odbiorców to nie przypadek dla porządku: makieta pokazuje tam
 * `ORION → ALL`, a `to: []` na drucie znaczy „ten plik nie wskazuje adresata", nie „wskazuje
 * wszystkich". Napis „to everyone" byłby relacją, której nie ma w danych (niezmiennik 17).
 */
import { describe, expect, it } from 'vitest';
import { fileName, sizeLabel, whoToWho } from './passed-row';

describe('the name of the file an agent left', () => {
  it('is the last part of the address, which is where the order in the run lives', () => {
    expect(
      fileName('/Users/someone/.loadout/runs/2026-08-16__abc/handoffs/02__scout__findings.md'),
      'memory::handoff writes <NN>__<from>__<kind>.md, so the file name already carries the ' +
        'number the mockup draws in its own column. Counting positions in the list would be a ' +
        'second answer to the same question, and a wrong one across two runs',
    ).toBe('02__scout__findings.md');
  });

  it('is the whole thing when there is nothing to cut', () => {
    expect(fileName('brief.md')).toBe('brief.md');
  });

  it('is the whole thing when the address ends in a separator, rather than a made-up name', () => {
    expect(
      fileName('/Users/someone/handoffs/'),
      'that is not a file, and inventing a name for it would put on screen something nobody ' +
        'has on disk',
    ).toBe('/Users/someone/handoffs/');
  });
});

describe('how big the file is', () => {
  it('speaks in bytes below a kilobyte, the way the mockup does', () => {
    expect(sizeLabel(840), 'the mockup writes 840 B — "0.8 KB" is the same fact made vaguer').toBe(
      '840 B',
    );
    expect(sizeLabel(0)).toBe('0 B');
    expect(sizeLabel(1023)).toBe('1023 B');
  });

  it('crosses to kilobytes at 1024, not at 1000', () => {
    expect(
      sizeLabel(1024),
      'a kilobyte is 1024 bytes. Dividing by 1000 is off by 2.4% and invisible on any single ' +
        'number, which is exactly why it is checked on the boundary',
    ).toBe('1.0 KB');
    expect(sizeLabel(1229)).toBe('1.2 KB');
  });

  it('crosses to megabytes at the next boundary', () => {
    expect(sizeLabel(1024 * 1024 - 1)).toBe('1024.0 KB');
    expect(sizeLabel(1024 * 1024)).toBe('1.0 MB');
  });
});

describe('who left it for whom', () => {
  it('names the sender and every reader, in the order the file gives them', () => {
    expect(whoToWho('Forge', ['Needle', 'Rivet'])).toBe('Forge → Needle, Rivet');
  });

  it('names only the sender when the file names no reader', () => {
    expect(
      whoToWho('Orion', []),
      'the mockup shows ALL in that spot, but an empty list on the wire says the file names ' +
        'nobody — not that it names everybody. Writing "everyone" there would draw a relation ' +
        'the data never carried (invariant 17)',
    ).toBe('Orion');
  });
});
