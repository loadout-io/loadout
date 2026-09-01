/* Kryterium 6 dla T-01: nie ma martwych przycisków.
 *
 * „Każda sekcja renderuje przycisk" przechodzi na powłoce z pięcioma przyciskami `Create`, które
 * nic nie robią — to jest dosłownie stan poprzedniego prototypu (`Export receipt`, `Copy`, `Review decision →`
 * bez handlera) i na zrzucie ekranu wygląda lepiej niż wersja poprawna [raport 03 §7.3].
 *
 * Rozróżniają je dwie rzeczy: DOKŁADNA liczba przycisków przełącznika w każdej sekcji — czyli
 * sam przełącznik, i ani jednego więcej — oraz pytanie do store'a, czy przełączenie naprawdę
 * zmieniło wartość.
 *
 * W T-01 nie ma jeszcze czego tworzyć, więc pusta sekcja NIE MA przycisku. Ma jedno zdanie:
 * pusty ekran jest zaproszeniem, nie akapitem polityki (DESIGN.md §6).
 *
 * ZMIANA LICZBY, 2026-08-18, świadoma i nazwana. Do dziś stało tu `occurrences(markup, '<button')
 * === 5` i ta liczba była DRUGĄ definicją zdania „w powłoce nie ma martwych przycisków" — takiej,
 * która mierzy sumę zamiast własności. Decyzja właściciela z tego dnia stawia w bocznym menu
 * przełącznik zakresu („my powinniśmy wybierać projekt z poziomu UI sidebar"), więc kontrolek
 * jest sześć i przy każdej następnej prawdziwej kontrolce powłoki byłoby ich siedem. Liczba
 * przepisana z palca zmusza wtedy albo do jej podnoszenia bez myślenia, albo — co gorsze — do
 * niedodawania kontrolki, której człowiek potrzebuje.
 *
 * NOWA WERSJA JEST MOCNIEJSZA, nie luźniejsza, i to jest cały sens tej zmiany. Zamiast jednej
 * sumy sprawdza trzy rzeczy naraz:
 *   (a) przełącznik sekcji ma DOKŁADNIE tyle kontrolek, ile jest sekcji, po jednej na sekcję;
 *   (b) każda kontrolka powłoki, która NIE jest przełącznikiem sekcji, stoi w liście `SHELL`
 *       poniżej, a suma `EXPECTED.length + SHELL.length` musi się zgadzać co do jednego — czyli
 *       przycisk dołożony bez wpisu jest czerwony;
 *   (c) każdy wpis w `SHELL` niesie NAZWĘ swojego handlera, a test woła ten handler i pyta
 *       magazyn, czy coś się naprawdę zmieniło.
 *
 * Punkt (c) jest tym, czego stara wersja nie umiała nigdy: liczba przepuszczała sześć przycisków
 * `Create` bez handlera dokładnie tak samo jak sześć działających. Żeby dołożyć kontrolkę do
 * powłoki, trzeba teraz dołożyć DOWÓD, że ona coś robi — a to jest niezmiennik 16 zapisany jako
 * asercja, nie jako suma.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import { useSectionStore } from './section-store';
import { useSwitcher } from './workspace-switcher';

const EXPECTED = [
  'run',
  'workflows',
  'agents',
  /* Jeden identyfikator zamiast dwóch od 2026-08-31 — Skills i Memory zeszły się w Knowledge
     (`src/ui/sections.tsx`). Lista stoi tu na sztywno z tego samego powodu, co zawsze:
     pętla po rejestrze sądziłaby rejestr samym sobą. */
  'knowledge',
  'lab',
  'triggers',
  'settings',
] as const;

/**
 * Kontrolki powłoki poza przełącznikiem sekcji: marker w DOM, handler i dowód, że handler ma
 * SKUTEK.
 *
 * `proof` dostaje magazyn w stanie początkowym, woła to, co woła przycisk, i oddaje `true`, kiedy
 * stan naprawdę się zmienił. Wpis bez działającego dowodu jest czerwony — czyli dokładnie tak
 * samo, jakby kontrolki nie było wcale.
 *
 * Powłoka rysuje przełącznik zakresu w stanie POCZĄTKOWYM magazynu (`renderToStaticMarkup` nie
 * uruchamia efektów, więc `load()` nigdy tu nie biegnie i lista zakresów jest pusta), a pusta
 * lista to zaproszenie do dodania pierwszego — jedna kontrolka. Rozwinięta lista zakresów ma
 * własne kryteria w `workspace-switcher.test.tsx`; tutaj sądzimy to, co człowiek widzi po
 * pierwszym uruchomieniu aplikacji.
 */
const SHELL = [
  {
    marker: 'data-workspace-new',
    what: 'the invitation to add the first workspace',
    proof: (): boolean => {
      useSwitcher.setState({ adding: false });
      useSwitcher.getState().startAdd();
      return useSwitcher.getState().adding;
    },
  },
] as const;

/** Pusty ekran to zaproszenie: jedno zdanie, do dwunastu słów, najwyżej jedna kropka. */
const MOST_WORDS = 12;
const MOST_STOPS = 1;

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/* `screens={{}}` — powłoka BEZ ekranów sekcji, i to jest podmiot tego kryterium.
 *
 * 2026-08-16: bez tego argumentu test renderował powłokę RAZEM z ekranami innych zadań i sądził
 * ich zawartość, choć jest o czym innym. Zderzenie było nieuniknione i przyszło z T-26: pusty
 * ekran sekcji ma **zapraszać** (makieta, DESIGN §6), więc niesie przycisk dodawania — a wtedy
 * przycisków jest sześć, nie pięć, i „nie ma martwego szóstego" zaczyna oznaczać „żadna sekcja
 * nie ma prawa mieć własnej akcji". Dwa kryteria z dwóch zadań zaczęły sobie przeczyć.
 *
 * Rozstrzygnięcie nie jest kompromisem: komentarz na górze tego pliku mówi, że kryterium pilnuje
 * PRZEŁĄCZNIKA — „sam przełącznik, i ani jednego więcej". Zdanie „w T-01 nie ma jeszcze czego
 * tworzyć" było prawdą o momencie, nie o powłoce, a zostało zapisane jako asercja wieczna.
 * Pusta mapa przywraca kryterium jego własny podmiot; ani jedna asercja nie znika i nie słabnie.
 * Ten sam zabieg stosuje już `screen-fallback.test.tsx`.
 *
 * Czego to kryterium po tej zmianie NIE sprawdza: martwych przycisków w ekranach sekcji. Nigdy
 * nie sprawdzało — niezmiennik 16 obowiązuje każde zadanie z osobna i tam ma swoje kryteria.
 */
function markupFor(id: (typeof EXPECTED)[number]): string {
  return renderToStaticMarkup(<App section={id} screens={{}} />);
}

/** Treść jedynego elementu z `data-empty`, bez znaczników i bez nadmiarowych odstępów. */
function emptyStateText(markup: string): string {
  const hit = /<([a-z]+)[^>]*\bdata-empty\b[^>]*>([\s\S]*?)<\/\1>/i.exec(markup);
  return (hit?.[2] ?? '')
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

describe('the only controls are the six section switches, and each one really switches', () => {
  for (const id of EXPECTED) {
    it('shows the six switches plus only the named shell controls, with ' + id + ' open', () => {
      const markup = markupFor(id);
      for (const other of EXPECTED) {
        expect(
          occurrences(markup, 'data-section-switch="' + other + '"'),
          'each of the six switches has to carry data-section-switch with a different one of ' +
            'the six names; ' +
            other +
            ' is missing or doubled',
        ).toBe(1);
      }
      for (const control of SHELL) {
        expect(
          occurrences(markup, control.marker),
          'the shell has to render ' + control.what + ' exactly once, carrying ' + control.marker,
        ).toBe(1);
      }
      expect(
        occurrences(markup, '<button'),
        'with ' +
          id +
          ' open the shell renders a control that is neither one of the six section switches ' +
          'nor one of the ' +
          String(SHELL.length) +
          ' shell controls named in SHELL at the top of this file. A button nobody named is a ' +
          'Create that has nothing to create — a control without a handler (invariant 16). Name ' +
          'it in SHELL together with a proof that its handler changes something, or take it out.',
      ).toBe(EXPECTED.length + SHELL.length);
    });

    it('greets an empty ' + id + ' with one short sentence', () => {
      const markup = markupFor(id);
      expect(
        occurrences(markup, 'data-empty'),
        id + ' has to render exactly one element carrying data-empty',
      ).toBe(1);
      const sentence = emptyStateText(markup);
      const words = sentence.split(/\s+/).filter((word) => word.length > 0);
      expect(
        words.length,
        'the invitation on an empty screen has to fit in ' +
          String(MOST_WORDS) +
          ' words; ' +
          id +
          ' says: ' +
          JSON.stringify(sentence),
      ).toBeLessThanOrEqual(MOST_WORDS);
      expect(
        occurrences(sentence, '.'),
        'one sentence, so at most ' +
          String(MOST_STOPS) +
          ' full stop. An empty screen is an invitation, not a paragraph of policy; ' +
          id +
          ' says: ' +
          JSON.stringify(sentence),
      ).toBeLessThanOrEqual(MOST_STOPS);
    });
  }

  for (const control of SHELL) {
    it('proves the handler behind ' + control.marker + ' changes something', () => {
      expect(
        control.proof(),
        control.what +
          ' is rendered in the shell and its handler does nothing. A handler that is wired up ' +
          'and has no effect looks identical in the markup and identical in a screenshot — that ' +
          'is the whole family of defect this criterion exists to catch, and counting buttons ' +
          'never saw it.',
      ).toBe(true);
    });
  }

  it('starts on Run and really moves when a switch is used', () => {
    expect(
      useSectionStore.getState().section,
      'the shell opens on run — the place where work happens',
    ).toBe('run');
    useSectionStore.getState().go('agents');
    expect(
      useSectionStore.getState().section,
      'go has to change the value the shell reads. A handler that is wired up and does nothing ' +
        'looks identical in the markup and identical in a screenshot',
    ).toBe('agents');
  });
});
