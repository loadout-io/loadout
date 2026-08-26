/* Odbiorca zielonego `Run` z edytora workflow — po zabraniu listy wyboru z paska.
 *
 * PO CO TO ISTNIEJE. Żądanie z edytora (`./requested.ts`) ma dziś dokładnie JEDNEGO konsumenta
 * i jest nim `useEffect` w znikającej kontrolce wyboru (`./start.tsx`). Zabranie tej kontrolki bez
 * nowego odbiorcy zamienia zielony `Run` w martwy przycisk — a to jest niezmiennik 16 złamany
 * o jeden bieg za późno, bo złapie to dopiero `e2e/tests/no-dead-controls.spec.ts`.
 *
 * DLACZEGO OSOBNY MODUŁ, A NIE EFEKT W KOMPONENCIE. To repo nie ma jsdom, a `renderToStaticMarkup`
 * nie uruchamia efektów — polityka zamknięta w efekcie jest więc kodem, którego żadne kryterium
 * nie umie dotknąć. To ta sama rodzina, z której wzięło się siedemnaście kłamiących kontrolek,
 * i ten sam powód, dla którego `./launch.ts` jest funkcją, a nie ciałem handlera.
 *
 * CZEGO TU NIE MA. Polityki startu: który plik, ile naraz, w którym folderze i co powiedzieć przy
 * odmowie decyduje `./launch.ts` i tylko on (niezmiennik 23). Ten moduł robi dwie rzeczy — zdejmuje
 * żądanie zapadką i przekazuje je tam — bo zapadka jest CAŁĄ ochroną przed dwoma biegami z jednego
 * kliknięcia, a to jest klasa błędu, która kosztuje pieniądze, nie render.
 *
 * ZDJĘCIE ŻĄDANIA JEST PIERWSZĄ INSTRUKCJĄ, przed czymkolwiek, co czeka. Odwrotna kolejność —
 * najpierw start, potem zapadka — zostawiałaby żądanie w module przez cały czas trwania biegu,
 * a `launchRun` rozwiązuje się dopiero z jego końcem: każdy powrót na ekran pracy w trakcie
 * odbierałby je drugi raz.
 */
import type { Choice } from './choices';
import { choiceFor } from './choices';
import { launchRun } from './launch';
import { takeRequestedRun } from './requested';

/**
 * Uruchamia to, o co poprosił edytor — albo `null`, kiedy nikt o nic nie prosił.
 *
 * Oddaje zdanie dla ekranu albo `null`, kiedy wszystko poszło: ta sama umowa, co w `./launch.ts`,
 * bo to jego odpowiedź tu przechodzi. Cisza po naciśnięciu Run jest defektem, od którego cała ta
 * droga się zaczęła.
 *
 * @param choices co leży w katalogu workflow — czytane raz, przy wejściu na sekcję. Argumentem,
 *   nie odczytem w środku: druga odpowiedź na pytanie „jakie workflow istnieją" rozjechałaby się
 *   z listą, którą widzi człowiek (niezmiennik 13).
 * @param atOnce ile kroków ma naprawdę biec naraz — liczba z jednej puli na okno (niezmiennik 11).
 */
export async function launchRequested(
  choices: readonly Choice[],
  atOnce: number,
  reflectionEnabled = true,
): Promise<string | null> {
  const taken = takeRequestedRun();
  /* Nikt o nic nie prosił — i to jest wartość, nie błąd. Ten moduł woła każdy render ekranu
   * pracy, więc cisza jest tu stanem normalnym, a nie zdaniem do postawienia na ekranie. */
  if (taken === null) return null;
  /* Który plik i co powiedzieć, kiedy go już nie ma, decyduje `./launch` — także wtedy, gdy
   * `choiceFor` oddaje `null`, bo katalog zmienił się między odczytem a kliknięciem. */
  return launchRun(
    choiceFor(choices, taken.path),
    atOnce,
    null,
    null,
    taken.reflectionEnabled ?? reflectionEnabled,
  );
}
