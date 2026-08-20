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
 * SZKIELET T-60: ciało rzuca, żeby kryterium padło na zachowaniu, a nie na zbieraniu importu
 * („Cannot find module" jest podpisem z `NOT_A_REAL_RED`). Podkreślenia przy nazwach parametrów
 * są częścią tej samej tymczasowości — `noUnusedParameters` z `checks/tsconfig.strict.json` liczy
 * parametr, którego ciało nie czyta.
 */
import type { Choice } from './choices';

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
export function launchRequested(
  _choices: readonly Choice[],
  _atOnce: number,
): Promise<string | null> {
  throw new Error('not implemented');
}
