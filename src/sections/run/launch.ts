/* CO SIĘ DZIEJE PO NACIŚNIĘCIU RUN — jedno miejsce na całą tę politykę.
 *
 * Dwie drogi prowadzą dziś do startu biegu i muszą prowadzić do TEJ SAMEJ rzeczy: przycisk
 * Start na ekranie pracy i zielony `Run` w edytorze workflow, który przez
 * `src/sections/run/requested.ts` mówi „uruchom TEN plik". Gdyby każda z nich decydowała sama,
 * cztery decyzje — który plik, ile agentów naraz, w którym folderze i co powiedzieć, kiedy nie
 * da się zacząć — istniałyby w dwóch kopiach (niezmiennik 23), a pierwsza, która się rozjedzie,
 * jest tą o folderze: bieg poszedłby wtedy do katalogu, pod którym wstała aplikacja.
 *
 * DLACZEGO FUNKCJA, A NIE `useEffect` W KOMPONENCIE. To repo nie ma jsdom, więc kliknięcia nie
 * da się odpalić w teście, a `renderToStaticMarkup` nie uruchamia efektów. Polityka zamknięta
 * w komponencie byłaby więc kodem, którego żadne kryterium nie umie dotknąć — a to jest dokładnie
 * ta rodzina, z której wzięło się 17 kłamiących kontrolek. Tutaj test woła to, co woła przycisk.
 *
 * ODDAJE ZDANIE ALBO `null`, nigdy nie rzuca. Cisza po kliknięciu Run jest tym, co to zadanie
 * naprawia: człowiek nacisnął i nie wie, czy coś się stało. Każde „nie zaczęliśmy" ma tu swoje
 * zdanie po angielsku, a odmowę Rusta wyjmuje `why()` — Tauri odrzuca NAPISEM, więc
 * `instanceof Error` był w siedmiu miejscach zawsze fałszywy i każda precyzyjna odmowa ginęła.
 */
import { why } from '../../ipc/why';
/* GDZIE PRACUJEMY — jedna definicja na całe repo (niezmiennik 13). Ten import zastąpił
 * `activeFolder()` z dawnego `./workspaces-store`: folder pracy jest odtąd polem ZAKRESU
 * wybieranego w bocznym menu, a nie własnością karty na pasku. */
import { activeWorkspace } from '../../state/workspaces';
import type { Choice } from './choices';
import { start } from './io';
import { cardForRun } from './tabs/store';
import type { TriggerClaim } from '../triggers/io';

/** Co powiedzieć, kiedy nie ma czego uruchomić. */
export const NOTHING_TO_RUN =
  'Nothing started: pick a workflow with steps in it first. A workflow with no steps has ' +
  'nothing to run.';

/** Co powiedzieć, kiedy poproszono o plik, którego nie ma już w katalogu workflow. */
export const GONE_FROM_DISK =
  'Nothing started: that workflow is no longer in the workflows folder. Open Workflows to see ' +
  'what is there.';

/**
 * Co powiedzieć, kiedy nie ma zakresu, czyli nie ma folderu, w którym agenci mają pracować.
 *
 * 2026-08-18 — CO TU STAŁO I CZEMU TO BYŁO ZŁE. Zdanie mówiło „Open one with + on the tabs
 * above", bo folder pracy brał się z karty na pasku, a Run bez ani jednej karty OTWIERAŁ OKNO
 * WYBORU KATALOGU systemu. Właściciel zobaczył to okno przy naciśnięciu Run i rozstrzygnął
 * inaczej: wybór projektu jest decyzją podejmowaną RAZ, w bocznym menu, a nie pytaniem
 * zadawanym przy każdym starcie. Zdanie nazywa więc nową drogę wyjścia — i musi ją nazywać,
 * bo odmowa bez wyjścia zostawia człowieka dokładnie tam, gdzie był (DESIGN §8).
 *
 * „Add a workspace" jest tym samym napisem, który stoi na przycisku w bocznym menu
 * (`FIRST_INVITE` w `src/ui/shell/workspace-switcher.tsx`). Zdanie odsyłające do kontrolki
 * nazwanej inaczej niż na ekranie jest instrukcją, której nie da się wykonać.
 */
export const NO_FOLDER =
  'Nothing started: agents work inside a folder, and no workspace is chosen yet. Add a ' +
  'workspace in the side menu, then press Run again.';

/** A legacy trigger has no safe project default: asking the active window would redirect work. */
export const TRIGGER_NO_WORKSPACE =
  'Nothing started: this trigger has no workspace. Open Triggers, choose where it should run, ' +
  'then save it.';

async function startChosen(
  choice: Choice,
  atOnce: number,
  folder: string,
  task: string | null,
  claim: TriggerClaim | null,
  reflectionEnabled: boolean,
): Promise<void> {
  /* 2026-08-31 — STRZAŁKI JADĄ RAZEM Z KROKAMI, jedno pole dalej. Bez tej linii pozycje
   * kafelków docierałyby do magazynu biegu (jadą wewnątrz `steps`), a kolejność między nimi
   * nie — czyli widok biegu dostawałby połowę kształtu i musiałby drugą połowę wymyślić
   * (niezmiennik 17). `choice.links` bywa nieobecne i wtedy nieobecne zostaje: pozycja listy
   * zbudowana bez pliku workflow nie ma o strzałkach nic do powiedzenia. */
  const what = { name: choice.name, steps: choice.steps, links: choice.links };
  if (claim === null && reflectionEnabled) {
    await start(choice.path, atOnce, what, folder, task);
    return;
  }
  if (claim !== null && reflectionEnabled) {
    await start(choice.path, atOnce, what, folder, task, claim);
    return;
  }
  await start(choice.path, atOnce, what, folder, task, claim, undefined, false);
}

/**
 * Uruchamia ten workflow i oddaje zdanie, które ma stanąć na ekranie — albo `null`, kiedy
 * wszystko poszło.
 *
 * KOLEJNOŚĆ JEST TREŚCIĄ. Najpierw plik, potem zakres: odmowa o workflow, którego nie da się
 * uruchomić, jest prawdziwa niezależnie od tego, gdzie by pracował, a odmowa o braku zakresu
 * odsyła człowieka do bocznego menu — dwie różne czynności i nie ma sensu wysyłać go po drugą,
 * kiedy pierwsza i tak zablokuje start.
 *
 * ŻADNEGO OKNA WYBORU KATALOGU, i to jest sedno zmiany z 2026-08-18. Do dziś ta funkcja przy
 * braku karty otwierała systemowy wybór folderu — czyli zadawała pytanie o projekt w chwili,
 * w której człowiek prosił o bieg. Właściciel nazwał to wprost i rozstrzygnął: projekt wybiera
 * się raz, w bocznym menu, jako zakres. Bieg bez zakresu ODMAWIA zdaniem, które mówi, co zrobić.
 *
 * Rozwiązuje się dopiero wtedy, kiedy bieg się KOŃCZY — `run_workflow` po tamtej stronie trwa
 * tyle, co bieg. Wołający nie ma na co czekać: zdanie o odmowie przychodzi tą samą drogą, tylko
 * wcześniej, bo Tauri odrzuca argumenty przed wejściem w ciało komendy.
 */
export async function launchRun(
  choice: Choice | null,
  atOnce: number,
  /**
   * Co ten bieg ma zbudować — zdanie z wiersza wejścia, albo `null`.
   *
   * `null` znaczy „tylko to, co stoi w pliku" i wtedy prompt każdego kroku jest CO DO BAJTU tym
   * z pliku. Wartość domyślna jest MOSTEM, nie wygodą: przycisk Start i zielony `Run` w edytorze
   * wołają tę krawędź dwoma argumentami i ich kryteria nie mają się o to potknąć.
   */
  task: string | null = null,
  /** A durable trigger delivery, or null for every human-started run. */
  claim: TriggerClaim | null = null,
  /** The current visible Learn from this run choice. */
  reflectionEnabled = true,
): Promise<string | null> {
  if (choice === null) return GONE_FROM_DISK;
  /* Workflow bez kroków odmawia po stronie Rusta, i to zdaniem lepszym niż nasze — ale odmowa
   * przychodzi po założeniu kanału i po wpisie do magazynu biegu, czyli po mignięciu paska
   * loadoutu opisującego bieg, którego nie ma. Pytamy więc tutaj, gdzie plan już znamy. */
  if (choice.steps.length === 0) return NOTHING_TO_RUN;

  /* 2026-08-21: ręczny Run bierze bieżący zakres w chwili kliknięcia. Trigger jest odwrotną
   * umową: workspace zamrożony w claimie ma wygrać nawet wtedy, gdy człowiek przełączy boczne
   * menu podczas odczytu workflow. Fallback do aktywnego zakresu wysyłałby cudze zadanie do
   * projektu, który akurat jest na ekranie. Legacy claim bez targetu odmawia przed kartą i IPC. */
  const folder = claim === null ? (activeWorkspace()?.folder ?? null) : (claim.workspace ?? null);
  if (folder === null) return claim === null ? NO_FOLDER : TRIGGER_NO_WORKSPACE;

  /* KARTA POWSTAJE TU, czyli w jedynym miejscu, które wie JEDNOCZEŚNIE, jak nazywa się workflow
   * i w którym zakresie pójdzie. Przed `start`, nie po nim: `start` rozwiązuje się dopiero
   * z końcem biegu, więc karta założona po nim pojawiałaby się w chwili, w której bieg właśnie
   * zszedł — czyli nigdy w trakcie. Bieg odrzucony przez Rusta zostawia kartę bez kropki;
   * to jest ta sama umowa, co przy `RunState.workflow`, które `io.ts` zeruje w `finally`. */
  cardForRun(choice.name, folder);

  try {
    /* Keep the five-argument manual call intact: older callers and its independent oracle own
     * that public seam. Only a trigger adds the sixth argument; `start` itself still sends an
     * explicit `claim: null` over IPC for the manual branch. */
    await startChosen(choice, atOnce, folder, task, claim, reflectionEnabled);
    return null;
  } catch (error: unknown) {
    return why(error, 'Loadout could not start that run.');
  }
}
