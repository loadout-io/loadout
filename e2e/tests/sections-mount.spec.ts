/* AC-2 dla T-29: każda z pięciu sekcji montuje SWÓJ ekran — sprawdzone w przeglądarce,
 * po kliknięciu w przełącznik.
 *
 * To jest to samo pytanie, które zadaje T-26 przez `renderToStaticMarkup`, zadane w działającej
 * aplikacji. Różnica nie jest kosmetyczna: powłoka znajduje ekrany przez `import.meta.glob`
 * (`src/ui/screens.ts`), a glob zachowuje się inaczej w buildzie niż w teście — i to jest
 * dokładnie ta klasa rozjazdu, której nikt nie zobaczy, dopóki nie uruchomi aplikacji.
 * Pięć pustych ekranów przy pięciu zielonych kryteriach zdarzyło się w tym repo 2026-08-16.
 *
 * DWIE ASERCJE, NIE JEDNA. Sam nagłówek przechodzi na ekranie, który rysuje nagłówek i pustkę
 * z rejestru pod nim — czyli na sekcji, która wygląda na zamontowaną i nie jest. Sam brak zdania
 * z rejestru przechodzi na ekranie eksportującym pusty `<div/>`. Dyskryminuje para: nagłówek
 * TEJ sekcji ORAZ brak jej zdania z rejestru w tym samym dokumencie.
 *
 * PIĘĆ IDENTYFIKATORÓW JEST WYPISANYCH TUTAJ NA SZTYWNO, a nie czytane z `SECTIONS`: pętla po
 * rejestrze sprawdzałaby rejestr sam sobą, a pusta tablica przeszłaby wtedy każde „dla każdej
 * sekcji…". Ta sama pułapka jest opisana w `src/ui/shell/screen-mount.test.tsx`. Samo ZDANIE
 * i sama ETYKIETA przychodzą już z rejestru, bo kryterium mówi wprost `sectionEntry(id).empty`,
 * a przepisany tu literał rozjechałby się przy pierwszej zmianie brzmienia (niezmiennik 13).
 *
 * ŚWIEŻA APLIKACJA NA SEKCJĘ, a nie jeden spacer po pięciu przełącznikach. Pięć osobnych
 * werdyktów zamiast jednego: sekcja, która pada jako pierwsza, nie chowa czterech pozostałych.
 * I jest to warunek OSTRZEJSZY — ekran musi zamontować się bez rozgrzewki po poprzedniej sekcji.
 *
 * I JEDEN SPACER NA DOKŁADKĘ, dopisany 2026-08-17 — DO tych pięciu, nigdy zamiast nich.
 * Pięć zimnych montaży pyta wyłącznie o pierwsze wejście na sekcję, a użytkownik chodzi po
 * aplikacji w JEDNEJ karcie: przełącznik, który po drugim przejściu zostawia w dokumencie
 * poprzedni ekran (albo dokłada drugi), przechodzi każdy z pięciu testów wyżej i psuje się
 * dokładnie tam, gdzie nikt nie patrzy. Zamiana tych pięciu na ten jeden byłaby cofnięciem:
 * spacer nie widzi sekcji, która montuje się WYŁĄCZNIE po rozgrzewce inną sekcją, a to jest
 * ta klasa rozjazdu, którą niesie `import.meta.glob` z cache'em modułów.
 *
 * CZEGO STĄD NIE DA SIĘ SPRAWDZIĆ, i mówię to wprost. T-26 ma kontrolę przeciw pustej asercji
 * przez `screens={{}}`: powłoka z pustą mapą MUSI pokazać zdanie z rejestru, inaczej „nie ma
 * tego zdania" przechodzi także wtedy, gdy zepsuto pustkę zamiast zamontować ekran. Z przeglądarki
 * nie da się tego zrobić — powłoka nie jest sterowalna spoza strony. Kontrolą, która TUTAJ ma
 * sens, jest więc atrybut `data-section`: dowodzi, że klik naprawdę przestawił powłokę, więc
 * żaden z faktów niżej nie jest odczytany z poprzedniego ekranu.
 */
import { beforeAll, afterAll, describe, expect, it } from 'vitest';

import { sectionEntry } from '../../src/ui/sections';
import { closeEverything, openApp } from '../harness';

const EXPECTED = ['run', 'workflows', 'agents', 'skills', 'memory'] as const;

/**
 * Trasa spaceru: te same pięć sekcji, a na końcu POWRÓT na `run`.
 *
 * Zmierzone 2026-08-17. Powłoka otwiera się na `run` (`FIRST_SECTION`,
 * `src/ui/shell/section-store.ts`), więc pierwsze kliknięcie spaceru ląduje na sekcji, która
 * JUŻ jest w dokumencie — i przechodzi identycznie dla działającego przełącznika, jak i dla
 * takiego, który nie robi nic. Spośród pięciu sekcji `run` była jedyną, do której spacer nigdy
 * nie WCHODZIŁ z innej: dokładnie to przejście, o które pyta ten test, było dla niej pominięte.
 *
 * Dopisane NA KOŃCU, nigdy przestawione na początek. Start od `workflows` przeniósłby tę samą
 * lukę na sekcję, która stałaby się pierwsza — potrzebne jest przejście DO `run` z innej
 * aktywnej sekcji, a nie inne miejsce na tę samą dziurę.
 *
 * Powtórzony identyfikator nie jest tu kosztem: pięć zimnych montaży wyżej ma po jednym
 * werdykcie na sekcję, a ten test pyta o przełączanie w jednej karcie, gdzie drugie wejście
 * na tę samą sekcję jest osobnym faktem (i tym, który psuje się najciszej).
 */
const WALK = [...EXPECTED, 'run'] as const;

/** Ile czekamy na przemontowanie sekcji. Wymiana jednego poddrzewa Reacta, nie sieć. */
const MOUNTS = 4_000;

const HEADINGS = 'main h1, main h2, main h3, main h4, main h5, main h6';

/* Rozruch vite + chromium jest kosztem STAŁYM NA PLIK, nie częścią pierwszego przypadku.
 * `openApp()` jest leniwy (`booted ??= boot()` w harness.ts), więc bez tego haka pierwszy `it`
 * płaci cały rozruch pod swoim limitem i pada na nim — mimo że każdy następny przechodzi po
 * ~160 ms na już postawionej aplikacji. Zmierzone 2026-08-18: rozruch ~63 s przy limicie 60 s,
 * czyli porażka mierzyła czas startu Playwrighta, a nie ani jednej rzeczy o produkcie.
 *
 * To PRZENIESIENIE KOSZTU, nie osłabienie kryterium: ani jeden `expect` się nie zmienia,
 * a hak jest symetryczny do `afterAll(closeEverything, 30_000)`, który stał tu od początku. */
beforeAll(async () => {
  await openApp();
}, 180_000);

afterAll(async () => {
  await closeEverything();
}, 30_000);

describe('every section mounts its own screen, clicked through in a browser', () => {
  for (const id of EXPECTED) {
    it('mounts the ' + id + ' screen and stops showing its registry sentence', async () => {
      const entry = sectionEntry(id);

      /* Puste zdanie w rejestrze zamieniłoby asercję niżej w twierdzenie o niczym — każdy tekst
       * „zawiera" pusty łańcuch. Ta sama uwaga dotyczy etykiety, którą porównujemy z nagłówkiem. */
      expect(
        entry.empty.trim().length,
        'sectionEntry("' + id + '").empty has to be a sentence somebody can read',
      ).toBeGreaterThan(0);
      expect(
        entry.label.trim().length,
        'sectionEntry("' + id + '").label has to name the section',
      ).toBeGreaterThan(0);

      const app = await openApp();
      try {
        const page = app.page;
        await page.click('[data-section-switch="' + id + '"]');

        const main = page.locator('main[data-section="' + id + '"]');
        await main.waitFor({ state: 'attached', timeout: MOUNTS }).catch(() => undefined);
        expect(
          await main.count(),
          'clicking the ' +
            id +
            ' switch has to put that section in the shell. Without this line every fact below ' +
            'could be read off whichever screen happened to still be open.',
        ).toBe(1);

        /* `textContent`, nie `innerText`: zdanie schowane arkuszem stylów DALEJ jest w dokumencie,
         * a „zamontowane i schowane" jest tą samą awarią, co „nie zamontowane" (niezmiennik 15). */
        const text = await page.evaluate(() => document.body.textContent ?? '');
        expect(
          text.includes(entry.empty),
          'the ' +
            id +
            ' screen is not mounted: the document still carries the registry sentence ' +
            JSON.stringify(entry.empty) +
            '. That sentence is what the shell shows for a section that has no screen at all — ' +
            'it is the five blank rectangles a person saw on 2026-08-16 under five green tests.',
        ).toBe(false);

        const headings = (await page.locator(HEADINGS).allInnerTexts()).map((line) => line.trim());
        expect(
          headings,
          'the ' +
            id +
            ' screen has to head itself with its own name. A screen that draws content without ' +
            'saying which section you are on passes "something mounted" and answers nothing.',
        ).toContain(entry.label);
      } finally {
        await app.close();
      }
    }, 60_000);
  }

  it('walks all five switches in one session, and each one lands on its own screen', async () => {
    const app = await openApp();
    try {
      const page = app.page;

      for (const id of WALK) {
        const entry = sectionEntry(id);
        await page.click('[data-section-switch="' + id + '"]');

        const main = page.locator('main[data-section="' + id + '"]');
        await main.waitFor({ state: 'attached', timeout: MOUNTS }).catch(() => undefined);
        expect(
          await main.count(),
          'walking to ' +
            id +
            ' left the shell somewhere else. Exactly one main carries the section that was ' +
            'just clicked — zero means the switch does nothing on a warm page, and more than ' +
            'one means the previous screen stayed in the document next to the new one.',
        ).toBe(1);

        const text = await page.evaluate(() => document.body.textContent ?? '');
        expect(
          text.includes(entry.empty),
          'after walking to ' +
            id +
            ' the document carries the registry sentence ' +
            JSON.stringify(entry.empty) +
            ' — the sentence the shell shows for a section with no screen. The cold-mount test ' +
            'above says this section mounts on a fresh page, so what is broken is the second ' +
            'visit, in the one card a person actually uses.',
        ).toBe(false);

        const headings = (await page.locator(HEADINGS).allInnerTexts()).map((line) => line.trim());
        expect(
          headings,
          'after walking to ' +
            id +
            ' no heading says ' +
            JSON.stringify(entry.label) +
            '. Either the section did not swap, or it swapped and left the previous name ' +
            'standing — and then the screen tells a person they are somewhere they are not.',
        ).toContain(entry.label);
      }
    } finally {
      await app.close();
    }
  }, 60_000);
});
