/* Kafelek listy workflow (makieta `docs/mockup/index.html:642-650`).
 *
 * ── 2026-08-31, PRZEBUDOWA KOMPOZYCJI ────────────────────────────────────────────────────
 *
 * Zgłoszenie właściciela ze zrzutu ekranu, cztery rzeczy naraz, i wszystkie cztery są o tym
 * SAMYM: karta nie miała czego pokazać, więc pokazywała przyciski.
 *
 *   1. `Duplicate` i `Delete` wisiały POD kartą, poza jej ramką, przy KAŻDEJ pozycji. Przy
 *      dziesięciu workflow to dwadzieścia stałych szarych prostokątów, czyli największa rzecz
 *      na ekranie — a żadna z nich nie jest powodem, dla którego ktokolwiek tu wchodzi.
 *      Dziś stoją WEWNĄTRZ karty, w jej stopce, i wychodzą z cienia dopiero pod kursorem albo
 *      pod ogniskiem klawiatury (`group-hover` / `group-focus-within`). Slot zostaje zajęty
 *      zawsze, więc nic nie podskakuje, i — to jest ważne dla wyroczni `no-dead-controls` —
 *      obie kontrolki są dalej WIDOCZNE w rozumieniu Playwrighta (`opacity: 0` to nie
 *      `visibility: hidden`), więc dalej są przez nią klikane i sądzone.
 *   2. Karta nie miała przycisku uruchomienia. Główna czynność wobec workflow — uruchom go —
 *      wymagała wejścia do edytora i znalezienia jej tam. Dziś `Run` stoi na karcie i jest
 *      najgłośniejszą rzeczą, jaką ta karta niesie.
 *   3. `Delete` miał tę samą wagę co `Duplicate`, choć jedno jest nieodwracalne. Dziś nosi
 *      `.btn-danger`, a `Duplicate` `.btn-bare` — dwie różne głośności dla dwóch różnych
 *      skutków.
 *   4. Karta nie mówiła NIC o historii. Patrz akapit niżej.
 *
 * HISTORIA BIEGÓW WCHODZI RAZEM Z DANYMI, dokładnie tak, jak obiecywał poprzedni nagłówek
 * tego pliku. Do dziś stało tu, że `used 12×` z makiety „wymaga historii biegów (T-06)
 * i wchodzi razem z nią — nigdy jako `—`, `never` ani `not reported`". Historia jest na
 * drucie (`list_runs`), więc obietnica jest wykonana: przy `runs === undefined` kafelek nie
 * mówi o biegach ANI SŁOWA i nie zostawia po nich pustej komórki. Skąd się bierze i czego
 * ten drut nie umie — w całości w `./history.ts`.
 *
 * KAFELEK JEST PRZYCISKIEM I ZOSTAJE NIM. Makieta otwiera workflow kliknięciem w kafelek
 * (`<button class="tile" data-go="flows">`), a `data-tile` znaczy „workflow, który leży na
 * dysku" i tak liczą go wyrocznie. Przyciski stopki są jego RODZEŃSTWEM, nie dziećmi:
 * przycisk w przycisku jest markupem, w którym przeglądarka sama rozstrzyga, które kliknięcie
 * wygrało. Ramkę i tło niesie od dziś `<li>`, żeby stopka leżała w środku tej samej karty.
 *
 * 2026-08-18 — LICZNIK AGENTÓW NIE LICZY PUSTYCH. `seen.add(step.agent)` bez warunku dawało
 * zbiorowi z jednym pustym napisem rozmiar 1, więc nad plikiem, w którym ŻADEN krok nie ma
 * agenta, stało „2 steps 1 agent". To jedyne miejsce, w którym człowiek sprawdza kompletność
 * jednym spojrzeniem, i mówiło mu dokładną odwrotność prawdy. Zmierzone na dwóch plikach
 * właściciela: oba miały `"agent": ""` przy każdym kroku.
 */
import type { ReactElement } from 'react';
import type { WorkflowFile } from './store';
import type { RunsBehindIt } from './history';
import { stateWord } from '../../run/history-command';

export interface WorkflowTileProps {
  wf: WorkflowFile;
  /** Z której półki pochodzi ten plik. Patrz `WorkflowEntry.place`. */
  place: 'library' | 'project';
  /**
   * Co ten workflow ma za sobą — albo `undefined`, kiedy nie ruszał ani razu.
   *
   * `undefined`, a nie wyzerowany rachunek: „nie biegł nigdy" i „biegł zero razy" to dla
   * ekranu ta sama rzecz, ale wyzerowany obiekt kazałby kafelkowi narysować komórkę, która
   * tłumaczy się z własnej pustki (FOUNDATIONS §6, `SPEND: not reported`).
   */
  runs?: RunsBehindIt | undefined;
  /**
   * Czy to jest ta jedna karta, którą ekran stawia na pierwszym miejscu.
   *
   * Bohater ekranu ma DOKŁADNIE jedno wystąpienie i to on niesie jedyną czynność główną
   * (`.btn-primary`). Rozstrzyga o tym lista, nie kafelek — patrz `workflow-list.tsx`.
   */
  first?: boolean;
  /**
   * Otwarcie tego workflow w edytorze.
   *
   * Wymagany, nie opcjonalny: kafelek JEST kontrolką otwarcia, a kontrolka bez handlera nie
   * wchodzi do repo (niezmiennik 16). Wariant „przycisk, kiedy handler jest, `<article>`, kiedy
   * go nie ma" dałby dwa kształty tej samej karty i pierwszą okazję, żeby na jakimś ekranie
   * wylądowała ta bez wyjścia.
   */
  onOpen: () => void;
  /** Uruchomienie tego workflow. Ta sama reguła, co wyżej — bez handlera nie ma przycisku. */
  onRun: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
}

/**
 * `1 step`, ale `4 steps`.
 *
 * Odmiana idzie za liczbą, a nie obok niej: `${n} steps` czyta się poprawnie przy czterech
 * i źle przy jednym, a napis wpisany na stałe czyta się poprawnie dokładnie na tym jednym
 * workflow, na którym ktoś go sprawdzał.
 */
function counted(count: number, noun: string): string {
  return count === 1 ? `1 ${noun}` : `${String(count)} ${noun}s`;
}

/** Czy ten krok naprawdę kogoś nazywa.
 *
 * `trim()`, nie `!== ''`: `freshStep` daje `agent: ''`, ale plik poprawiony ręcznie potrafi mieć
 * tam spację, a spacja nie jest identyfikatorem agenta bardziej niż pusty napis. */
function names(agent: string): boolean {
  return agent.trim() !== '';
}

/**
 * Ilu RÓŻNYCH agentów robi tę robotę.
 *
 * Nie tyle, ile jest kroków: workflow z czterema krokami, w którym dwa robi ten sam agent,
 * ma dwóch agentów. Krok rodzaju `checkpoint` nie ma agenta i nie liczy się do niczego —
 * i krok, który agenta jeszcze nie wybrał, też nie (patrz nagłówek pliku).
 */
function differentAgents(steps: WorkflowFile['steps']): number {
  const seen = new Set<string>();
  for (const step of steps) {
    /* Po RODZAJU kroku, a nie po obecności pola: `agent` jest wymagany na kroku agenta
     * i nie istnieje na punkcie kontrolnym, więc rodzaj jest tym, co naprawdę rozstrzyga
     * (2026-08-17: schemat przyjechał z `src/state/workflows.ts`, gdzie to są dwa typy). */
    if (step.kind === 'agent' && names(step.agent)) {
      seen.add(step.agent);
    }
  }
  return seen.size;
}

/**
 * Ile nazw kroków mieści karta bohatera, zanim zacznie mówić „i jeszcze N".
 *
 * Sześć, bo tyle wchodzi w jeden wiersz przy typowej długości nazwy — a druga linijka żetonów
 * zaczyna wypychać z karty to, po co człowiek tu przyszedł.
 */
const SHOWN_STEPS = 6;

/** Ile kroków agenta czeka na wybór agenta. Zero znaczy „nie ma o czym mówić". */
function waitingForAnAgent(steps: WorkflowFile['steps']): number {
  return steps.filter((step) => step.kind === 'agent' && !names(step.agent)).length;
}

/**
 * Jedno zdanie o ostatnim biegu — data i to, czym się skończył.
 *
 * Słowo stanu tłumaczy `stateWord` z sekcji Run i to jest jedyne miejsce, w którym ta tabela
 * mieszka (niezmiennik 13 i 14: `succeeded` z drutu nie ma prawa stanąć na ekranie). Kiedy
 * słowa nie umiemy przełożyć, zostaje sama data — zdanie mówi wtedy mniej, nigdy nieprawdę.
 */
function lastRunSays(runs: RunsBehindIt): string {
  const word = stateWord(runs.latest.state);
  return word === '' ? `Last run ${runs.latest.when}` : `Last run ${runs.latest.when} · ${word}`;
}

export function WorkflowTile({
  wf,
  place,
  runs,
  first = false,
  onOpen,
  onRun,
  onDuplicate,
  onDelete,
}: WorkflowTileProps): ReactElement {
  /* Pusty opis to brak opisu. Plik z `"description": ""` — a taki powstaje z jednego
   * skasowanego zdania — dałby zawsze renderowany akapit, czyli linijkę kafelka trzymaną
   * otwartą dla tekstu, którego tam nie ma. */
  const description = wf.description?.trim() ?? '';
  const waiting = waitingForAnAgent(wf.steps);
  /* Kroki, które mają jak się przedstawić. Krok bez nazwy dałby pusty żeton, czyli kolejną
   * komórkę tłumaczącą się z własnej pustki. */
  const named = wf.steps.filter((step) => step.name.trim() !== '');

  return (
    /* `group` jest MECHANIZMEM: stopka z porządkami czyta z niego hover i ognisko całej karty.
       `.card` siedzi tu, a nie na przycisku, żeby stopka leżała WEWNĄTRZ ramki — to jest
       naprawa sierot spod karty, opisana w nagłówku. `p-0`, bo padding rozdają dziecko i stopka
       osobno; utility Tailwinda stoi w warstwie po `@layer components`, więc wygrywa. */
    <li
      data-workflow-place={place}
      data-interactive
      className={'card enter group flex flex-col p-0 ' + (first ? 'sm:col-span-2' : '')}
    >
      <button
        data-tile
        type="button"
        onClick={onOpen}
        /* Obrys ogniska osobno, bo `.card[data-interactive]:focus-visible` sądzi kartę,
           a ogniskuje się PRZYCISK w jej środku (2026-08-31). */
        className="flex flex-1 flex-col gap-2 p-3 text-left outline-offset-2 outline-accent focus-visible:outline-2"
      >
        {/* Nazwa dostaje CAŁĄ szerokość wiersza i nie stoi obok niczego, co mogłoby ją
            wypchnąć: metadana ustępuje treści pisanej przez człowieka, nie odwrotnie. */}
        <span className={first ? 'text-title text-ink' : 'text-subhead text-ink'}>{wf.name}</span>

        {description === '' ? null : <span className="lead line-clamp-2">{description}</span>}

        {/* Liczby są wartościami maszynowymi, więc mono — reguła semantyczna z DESIGN §4.
            Dwie pierwsze pozycje są w pliku, trzecia w historii biegów. Przy workflow, który
            nie ruszał ani razu, trzeciej NIE MA — patrz `runs` w propsach. */}
        <span className="value flex flex-wrap gap-3 border-t border-t-line pt-2">
          <span>{counted(wf.steps.length, 'step')}</span>
          <span>{counted(differentAgents(wf.steps), 'agent')}</span>
          {runs === undefined ? null : <span>{`used ${String(runs.howOften)}×`}</span>}
          {waiting === 0 ? null : (
            <span className="text-attend">
              {waiting === 1 ? '1 step has nobody yet' : `${String(waiting)} steps have nobody yet`}
            </span>
          )}
        </span>

        {runs === undefined ? null : <span className="lead">{lastRunSays(runs)}</span>}

        {/* CO TEN WORKFLOW W OGÓLE ROBI — wyłącznie na karcie bohatera, bo tylko ona ma na to
            miejsce, i wyłącznie NAZWY KROKÓW z pliku.
            BEZ STRZAŁEK MIĘDZY NIMI, i to jest niezmiennik 17, nie oszczędność: kolejność
            wykonania mieszka w `links`, a kolejność w tablicy `steps` jest kolejnością zapisu.
            Strzałka narysowana między sąsiadami tej tablicy pokazywałaby relację, której
            w danych nie ma — i myliłaby się dokładnie na tych grafach, dla których ten produkt
            powstał: rozgałęzionych. */}
        {!first || named.length === 0 ? null : (
          <span className="flex flex-wrap gap-2 pt-1">
            {named.slice(0, SHOWN_STEPS).map((step) => (
              <span key={step.id} className="chip">
                {step.name}
              </span>
            ))}
            {named.length <= SHOWN_STEPS ? null : (
              <span className="chip">{`+${String(named.length - SHOWN_STEPS)} more`}</span>
            )}
          </span>
        )}
      </button>

      {/* STOPKA KARTY: jedna czynność głośna, dwa porządki po cichu.
          `opacity-0` zamiast `hidden`, i to jest różnica, na której stoi wyrocznia martwych
          kontrolek: element o zerowej przezroczystości ma dalej pudełko, dalej łapie kliknięcie
          i dalej jest widoczny dla `main button:visible`. Slot zajęty na stałe znaczy też, że
          nic nie podskakuje pod kursorem. */}
      <div className="flex items-center gap-2 border-t border-t-line px-3 py-2">
        <button data-run type="button" onClick={onRun} className={first ? 'btn-primary' : 'btn'}>
          ▷ Run
        </button>
        <span className="ml-auto flex gap-2 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
          <button type="button" className="btn-bare" onClick={onDuplicate}>
            Duplicate
          </button>
          <button type="button" className="btn-danger" onClick={onDelete}>
            Delete
          </button>
        </span>
      </div>
    </li>
  );
}
