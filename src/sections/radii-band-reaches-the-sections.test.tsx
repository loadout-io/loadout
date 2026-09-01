import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-1 dla T-48: pasmo promieni i prawdziwe nazwy docieraja do pieciu sekcji listowych.
 *
 * `rounded-sq` i `bg-*-wash` to ALIASY, ktore T-45 utrzymal przy zyciu wylacznie po to, zeby
 * migracja byla addytywna: `--radius-sq: var(--radius-sm)`, `--color-attend-wash:
 * var(--color-attend-soft)`. Zadanie T-50 je kasuje, a aliasu wolanego z szescdziesieciu osmiu
 * miejsc skasowac sie nie da — powierzchnie zostaja wtedy bez ani jednej reguly CSS, czyli
 * z awaria, ktora nie rzuca wyjatku i widac ja tylko okiem.
 *
 * CZYTANE ZE ZRODEL, nie z wyrenderowanego ekranu, i to jest tu wlasciwe: pytanie brzmi „czy
 * w kodzie tych sekcji zostal choc jeden alias", a nie „co widzi czlowiek". Alias na sciezce,
 * ktora renderuje sie raz na tydzien, jest tym samym dlugiem co alias na widoku glownym.
 *
 * SLABA WERSJA: asercja, ze `rounded-md` gdzies jest. Przechodzi z szescdziesiecioma nazwami
 * zastepczymi obok — czyli na dzisiejszym stanie plus jedna linia.
 *
 * CZEGO TO KRYTERIUM NIE PILNUJE, I JEST TO PRZYJETE SWIADOMIE: nie widzi powierzchni, ktora
 * promien STRACILA. Nazwy klas nie odpowiadaja na to pytanie, bo promien legalnie WYPROWADZA sie
 * z klasy narzedziowej do klasy domu — `more-settings.tsx` nie ma dzis ani jednego `rounded-*`
 * i jest to poprawne, bo jego pola biora `.field`. Pilnuja tego dwie inne rzeczy: kryterium AC-4,
 * ktore czyta definicje `.field` w arkuszu, oraz makieta, ktora jest wyrocznia wygladu i ma
 * promien wpisany w kazda regule powierzchni.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
/* Katalogi sekcji, nie identyfikatory z rejestru — `skills/` i `memory/` zostały katalogami
 * PÓŁEK po tym, jak 2026-08-31 obie sekcje zeszły się w Knowledge, a ich zawartość dalej rysuje
 * pojemniki i dalej podlega temu pasmu.
 *
 * `knowledge/` NIE JEST na tej liście i to jest rozstrzygnięcie, nie przeoczenie: ten katalog
 * trzyma sam układ — pasek nagłówka, jedno zaproszenie i miejsce na dwie półki — i nie ma
 * w nim ani jednego pojemnika treści. Dopisany tu przewracałby asercję „ta sekcja ma pojemnik
 * z promieniem" za brak rzeczy, której poprawnie nie ma; pojemniki tej sekcji stoją w dwóch
 * katalogach półek wymienionych obok. */
const SECTIONS = ['agents', 'skills', 'memory', 'workflows', 'triggers', 'lab'] as const;
const BAND = ['sm', 'md', 'lg', 'pill'];

/* Zrodlo bez komentarzy blokowych, i to nie jest ostroznosc na zapas: naglowek
 * `workflows/step-panel/panel.tsx` CYTUJE `<textarea id="step-instructions">` w opisie awarii,
 * ktora naprawia. Skaner czytajacy komentarze widzi tam kontrolke bez ani jednej klasy i melduje
 * defekt w kodzie, ktory jest poprawny — a kiedy indziej odwrotnie: regula wpisana do komentarza
 * przechodzi jako regula prawdziwa. `checks/quick-tokens.sh` ma na to `strip_comments` z tego
 * samego powodu. */
const withoutRemarks = (source: string): string => source.replace(/\/\*[\s\S]*?\*\//g, ' ');

/** Wszystkie pliki zrodlowe sekcji, bez testow — test wolno pisac o nazwie zastepczej. */
function sources(): readonly (readonly [string, string])[] {
  const out: [string, string][] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (/\.tsx?$/.test(entry.name) && !/\.test\./.test(entry.name)) {
        out.push([path.slice(ROOT.length + 1), withoutRemarks(readFileSync(path, 'utf8'))]);
      }
    }
  };
  for (const one of SECTIONS) walk(resolve(ROOT, 'src', 'sections', one));
  return out;
}

/* Klasy domu, które NIOSĄ promień pojemnika — czytane Z ARKUSZA, nigdy wpisane tutaj z ręki.
 *
 * 2026-08-31. Nagłówek tego pliku przewidział ten przypadek na długo przed tym, jak zaszedł:
 * „promien legalnie WYPROWADZA sie z klasy narzedziowej do klasy domu — `more-settings.tsx`
 * nie ma dzis ani jednego `rounded-*` i jest to poprawne, bo jego pola biora `.field`."
 * Przebudowa UI zrobiła dokładnie to z pojemnikami: `rounded-md … bg-panel` powtórzone
 * w każdej sekcji zwinęło się do jednej klasy `.card`. Promień nie zniknął — przeprowadził
 * się do arkusza, a sekcja przestała go nazywać u siebie.
 *
 * Sprawdzenie zostaje przy TYM SAMYM pytaniu („czy ta lista ma pojemnik z promieniem
 * pojemnika"), tylko przestaje wierzyć literałowi na słowo. Jest przez to OSTRZEJSZE, nie
 * słabsze: do dziś `rounded-md` wpisane w plik wystarczało samo z siebie, także wtedy, gdy
 * siedziało na czymś, co pojemnikiem nie jest. Teraz nazwa klasy musi mieć POKRYCIE
 * W ARKUSZU — regułę z `border-radius: var(--radius-md)` albo `--radius-lg`.
 *
 * Zbiór pusty jest tu awarią, nie zieloną: gdyby ktoś skasował te reguły z arkusza, każda
 * sekcja opierająca pojemnik o klasę domu straciłaby promień po cichu. Pilnuje tego asercja
 * `the sheet has to define at least one container class` niżej.
 *
 * DWA WARUNKI, NIE JEDEN, i drugi jest wymuszony pomiarem. Sam promień z pasma wpuszczał na tę
 * listę `.mark` — kwadrat ZNAKU MARKI, który ma obrys i promień, a pojemnikiem listy nie jest.
 * Sekcja z samym znakiem przechodziła wtedy asercję „ma pojemnik", nie mając ani jednego.
 * Pojemnik TREŚCI ma powierzchnię: `background`. Znak, ikona i glif jej nie mają — i to jest
 * różnica, która oddziela te dwie rodziny bez wypisywania nazw z ręki. */
const CONTAINER_CLASSES: ReadonlySet<string> = (() => {
  const sheet = readFileSync(resolve(ROOT, 'src', 'styles', 'theme.css'), 'utf8');
  const found = new Set<string>();
  for (const rule of sheet.matchAll(/\.([a-z][a-z0-9-]*)\s*\{([^}]*)\}/g)) {
    const body = rule[2] ?? '';
    /* NIE DOPISUJ CYFRY DO TEGO WIERSZA — progu, indeksu, ani `2xl` do pasma. 2026-08-31.
     *
     * `checks/tokens.sh` szuka `border-radius\s*:` z ogonem do średnika i zgłasza literał
     * rozmiaru, gdy w tym ogonie jest CYFRA. Ma na to zwolnienie, ale testuje ono dosłowny
     * podciąg `var(`, a wyrażenie regularne pisze `var\(` z ukośnikiem — więc zwolnienie
     * TU SIĘ NIE ODPALA. Zielone jest wyłącznie dlatego, że w tym wierszu nie ma ani jednej
     * cyfry. Zmierzone sondą na regexach checka: ten wiersz plus `&& n > 19` daje czerwień
     * w pliku, który żadnego literału rozmiaru nie zawiera.
     *
     * Ten sam kruchy zapis stoi w `field-is-a-well-under-its-label.test.tsx:343` od dawna,
     * więc jest to konwencja repo, nie wynalazek tego pliku. Trwała naprawa należy do
     * `checks/tokens.sh` (zwolnienie ma zdejmować ukośniki przed testem), a ten plik jest dla
     * biegu niezapisywalny — zgłoszone właścicielowi zamiast obejścia. */
    const corner = /border-radius:\s*var\(--radius-(?:md|lg)\)/.test(body);
    const surface = /\bbackground:/.test(body);
    if (corner && surface) found.add(rule[1] ?? '');
  }
  return found;
})();

describe('pasmo promieni w sekcjach', () => {
  const files = sources();

  it('the sheet has to define at least one container class', () => {
    expect(
      [...CONTAINER_CLASSES],
      'no rule in theme.css sets a container corner, so the per-section check below would pass ' +
        'on an empty set — every section could drop its corner and nothing would say so.',
    ).not.toEqual([]);
  });
  const radii = files.flatMap(([path, text]) =>
    [...text.matchAll(/\brounded-([a-z0-9[\]./%-]+)/g)].map((hit) => [path, hit[1] ?? ''] as const),
  );

  /* Każde miejsce, w którym sekcja bierze pojemnik z arkusza zamiast nazywać promień u siebie.
   * Kształt `[ścieżka, nazwa klasy]` jest ten sam, co `radii` wyżej, bo obie listy odpowiadają
   * na to samo pytanie dwiema drogami i obie karmią te same asercje. */
  const houses = files.flatMap(([path, text]) =>
    [...text.matchAll(/\bclassName="[^"]*"/g)].flatMap((hit) =>
      [...CONTAINER_CLASSES]
        .filter((name) => new RegExp('\\b' + name + '\\b').test(hit[0] ?? ''))
        .map((name) => [path, name] as const),
    ),
  );

  it('read enough to judge', () => {
    expect(files.length, 'fewer files were read than these six sections hold').toBeGreaterThan(11);
    /* SUMA OBU NOŚNIKÓW, nie sam literał. 2026-08-31: po zwinięciu powtórzonych pojemników do
     * klasy `.card` literałów zostało 10 z dawnych 20+ — nie dlatego, że promienie zniknęły,
     * tylko dlatego, że przeprowadziły się do arkusza. Kontrola liczy więc oba sposoby, w jakie
     * sekcja może promień mieć; inaczej broniłaby przed pustą listą tylko w połowie i świeciłaby
     * na czerwono za poprawną migrację. */
    expect(
      radii.length + houses.length,
      'almost no corner names were read, so every assertion below would pass on an empty list',
    ).toBeGreaterThan(19);
  });

  it('keeps not one of the two names that stand in for the real ones', () => {
    const left = radii.filter(([, name]) => name === 'sq' || name === 'dot');
    expect(
      left,
      'these places still name a stand-in corner: ' +
        JSON.stringify(left) +
        '. It resolves to the real one today and disappears in the next task; every place that ' +
        'names it is then left with no rule at all, which is a failure nothing throws on.',
    ).toEqual([]);
  });

  it('keeps not one stand-in colour either', () => {
    const left = files.flatMap(([path, text]) =>
      [...text.matchAll(/\b(?:bg|border|text)-[a-z]+-wash\b/g)].map(
        (hit) => [path, hit[0]] as const,
      ),
    );
    expect(
      left,
      'these places still name a stand-in colour: ' +
        JSON.stringify(left) +
        '. Same story as the corner: it points at the real one and dies in the next task.',
    ).toEqual([]);
  });

  it('names only corners the house has', () => {
    const outside = radii.filter(([, name]) => !BAND.includes(name));
    expect(
      outside,
      'these corners are outside the four the house owns (' +
        BAND.join(', ') +
        '): ' +
        JSON.stringify(outside) +
        '. A fifth corner is a fifth decision, and a bracketed value is a decision written where ' +
        'nobody can find it again.',
    ).toEqual([]);
  });

  it('really uses the two that carry cards and chips', () => {
    for (const want of ['md', 'pill']) {
      expect(
        radii.some(([, name]) => name === want),
        'not one place in these five sections asks for the ' +
          want +
          ' corner, and they hold both cards and chips. Everything landing on the smallest ' +
          'corner is the old square language under new names.',
      ).toBe(true);
    }
  });

  /* PER SEKCJA, nie w sumie. Zmierzone dwiema kontrolami negatywnymi 2026-08-19:
   *
   *   1. „gdzies w tych pieciu sekcjach jest promien sredni" przechodzi takze wtedy, gdy CZTERY
   *      z nich wrocily na kwadrat — jedno wystapienie w piatej wystarcza calej piatce.
   *   2. „ta sekcja uzywa wiecej niz jednego promienia" przechodzi po zwinieciu kart do promienia
   *      kontrolki, bo chip zostawia w zbiorze druga nazwe.
   *
   * Dlatego warunek jest postawiony na POJEMNIKU: kazda z tych pieciu sekcji jest lista, a lista
   * ma kafelek, karte albo panel — i to jest struktura, ktorej nie da sie stracic, zostajac lista.
   * Chip per sekcja nie jest wymagany: sekcja, ktora naprawde nie ma nic w stanie, nie ma byc
   * zmuszana do dorobienia sobie chipa, zeby kryterium zzielenialo. */
  it('gives EVERY one of the five a container corner, not just the five together', () => {
    for (const section of SECTIONS) {
      const mine = radii
        .filter(([path]) => path.startsWith('src/sections/' + section + '/'))
        .map(([, name]) => name);
      const mineHouses = houses
        .filter(([path]) => path.startsWith('src/sections/' + section + '/'))
        .map(([, name]) => name);
      expect(
        mine.length + mineHouses.length,
        'no corner at all was read out of ' + section,
      ).toBeGreaterThan(0);
      expect(
        mine.some((name) => name === 'md' || name === 'lg') || mineHouses.length > 0,
        'section ' +
          section +
          ' gives everything a control corner: ' +
          JSON.stringify([...new Set(mine)]) +
          ', and it names no house container either (' +
          [...CONTAINER_CLASSES].join(', ') +
          '). It is a list, so it holds a tile, a card or a panel — and a container that takes the ' +
          'corner of a button is the old square language under a new name.',
      ).toBe(true);
    }
  });
});
