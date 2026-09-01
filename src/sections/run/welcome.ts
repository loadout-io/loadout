/* CO POWITANIE MÓWI W KAŻDYM Z TRZECH MIEJSC DROGI — jeden fakt, jedno miejsce.
 *
 * POWITANIE NIE JEST STAŁE, i to jest cała treść tego pliku. Ekran, który wita „Welcome to
 * Loadout" także wtedy, gdy człowiek ma już agenta i workflow, mówi mu, że nic się nie stało —
 * a stało się dwa razy. Makieta rozstrzyga to wprost: po zapisaniu pierwszego agenta tytuł
 * brzmi „<imię> is ready", zdanie mówi, czego temu agentowi brakuje do biegu, a duży przycisk
 * prowadzi do NASTĘPNEGO kroku, nie do poprzedniego (`docs/mockup/index.html`, `advance()`).
 *
 * DLACZEGO OSOBNY PLIK, A NIE TRZY GAŁĘZIE W KOMPONENCIE. Bo to jest tabela, nie widok: pięć
 * napisów razy trzy miejsca drogi. Wpleciona w JSX byłaby czytelna wyłącznie z rozwiniętymi
 * warunkami i nie dałoby się jej sprawdzić inaczej niż przez render całego ekranu w trzech
 * stanach. Tutaj jest czystą funkcją nad krokiem, który jest bieżący.
 *
 * CZTERY ZDANIA, NIE PIĘĆ. Kiedy nic nie zostało, powitania nie ma wcale — bo wtedy nie ma go
 * kto czytać: `./index.tsx` przestaje rysować przewodnik i strefa pracy wraca do tego, po co
 * człowiek na nią przyszedł.
 */
import type { Section } from '../../ui/sections';
import { SECTIONS } from '../../ui/sections';
import { jumpForNumber } from '../../ui/palette/keys';
import type { FirstRunStep } from './first-run';

/**
 * Klawisz, którym ta aplikacja NAPRAWDĘ skacze do tej sekcji — albo pusty napis.
 *
 * PYTAMY KLAWIATURY, nie rejestru. Numer sekcji da się policzyć z pozycji w `SECTIONS`
 * (`src/ui/shell/titlebar.tsx`, `keyFor`), ale wtedy napis na przycisku byłby drugą kopią tej
 * arytmetyki i przeżyłby dzień, w którym `moveFor` przestanie brać cyfry. Tutaj pytamy tej
 * samej funkcji, którą woła nasłuch okna (`src/ui/palette/keys.ts`, `jumpForNumber`): klawisz,
 * który da się narysować, jest z definicji klawiszem, na który okno odpowiada (niezmiennik 16).
 */
function keyFor(section: Section): string {
  for (let at = 1; at <= SECTIONS.length; at += 1) {
    if (jumpForNumber(String(at)) === section) return '⌘' + String(at);
  }
  return '';
}

export interface Welcome {
  /** Tytuł ekranu. Jedyne miejsce w aplikacji, które nosi `--text-display`. */
  readonly title: string;
  /**
   * JEDNO zdanie pod tytułem — i naprawdę jedno.
   *
   * `src/sections/empty-screen-invites.test.tsx` czyta ten napis przez znacznik `data-empty`
   * i żąda, żeby był dokładnie jednym zdaniem. Dlatego myślnik zamiast kropki tam, gdzie kusi,
   * żeby postawić dwa: „glif zdanie zdanie przycisk" to jest treść, którą ta wyrocznia
   * zamyka, a nie formalność.
   */
  readonly sentence: string;
  /** Napis dużego przycisku — czasownik, bo to jest czynność, a nie nazwa miejsca. */
  readonly act: string;
  /**
   * Klawisze przy przycisku, dokładnie tak, jak człowiek ma je nacisnąć — albo pusty napis.
   *
   * PUSTY, KIEDY SKRÓTU NIE MA. Klawisz narysowany obok przycisku jest obietnicą, a obietnica,
   * której nikt nie dotrzymuje, jest tą samą wadą co przycisk bez handlera (niezmiennik 16).
   * Kryterium `first-open-is-a-door.test.tsx` sprawdza każdy z nich wobec `shortcuts()`.
   */
  readonly press: string;
  /** Linia otuchy pod przyciskiem: co to kosztuje i co zostaje na tej maszynie. */
  readonly reassure: string;
}

/** Powitanie dla kroku, który jest teraz bieżący — albo `null`, kiedy nie ma już żadnego. */
export function welcomeFor(steps: readonly FirstRunStep[], named: string | null): Welcome | null {
  const now = steps.find((step) => step.state === 'now');
  if (now === undefined) return null;

  if (now.id === 'agent') {
    return {
      title: 'Welcome to Loadout',
      sentence:
        'Coding agents you write once, put in a row, and watch work — on this Mac, on your ' +
        'own code.',
      act: 'Make your first agent',
      /* `⌘N` z makiety w tej aplikacji nie istnieje. Istnieje `⌘1`…`⌘7` — skok do sekcji po
       * jej pozycji w rejestrze — i to jest klawisz, który zabiera dokładnie tam, dokąd
       * zabiera ten przycisk. Numeru nie wpisujemy: pyta o niego klawiatura. */
      press: keyFor('agents'),
      reassure: 'Takes about a minute · nothing leaves this Mac · you can change every word later',
    };
  }

  if (now.id === 'workflow') {
    return {
      /* IMIĘ, KIEDY JE ZNAMY. Agent wzięty z galerii przed chwilą ma imię, więc ekran mówi
       * o NIM; agent zapisany w sekcji Agents wrócił tu jako sama liczba, więc ekran mówi
       * o tym, co wie. Zdanie „Scout is ready" o agencie, którego nikt nie nazwał Scoutem,
       * byłoby faktem, którego dane nie niosą (niezmiennik 17). */
      title: (named ?? 'Your first agent') + ' is ready',
      sentence:
        'One agent is not a workflow yet — put it in a row with a second one, and the arrow ' +
        'between them is the whole idea.',
      act: 'Build your first workflow',
      press: keyFor('workflows'),
      reassure: 'Two steps is already a workflow · you can add the rest later',
    };
  }

  return {
    title: 'The row is ready',
    sentence:
      'Loadout works inside one folder on this Mac — point it at your code and this row has ' +
      'somewhere to run.',
    act: 'Choose that folder',
    /* Wskazanie folderu nie ma skrótu i nie udajemy, że ma. */
    press: '',
    reassure: 'Your own files stay untouched · a run works on a copy · you can stop it any time',
  };
}
