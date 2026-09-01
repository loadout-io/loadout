/* Gdzie krok pracuje — JEDNA kontrolka, którą montują wszystkie trzy panele.
 *
 * 2026-08-31 — SCALONE Z TRZECH KOPII. Do tego dnia ta sama decyzja stała w trzech plikach
 * tego katalogu, pod dwiema różnymi nazwami i z dwoma różnymi brzmieniami tej samej odpowiedzi:
 *
 *   `panel.tsx`        „Where it works", trzy radia, „Work in the project folder" …
 *   `check-panel.tsx`  „Where it runs",  trzy radia, „In your project folder" …
 *   `serve-panel.tsx`  „Where it runs",  dwa radia,  brzmienia jak w check-panel.
 *
 * Człowiek, który przeczytał jeden panel, musiał czytać drugi od nowa — a rozjazd trzeciej
 * kopii widać było wyłącznie wtedy, gdy ktoś otworzył trzy pliki naraz. To jest niezmiennik 13
 * czytany po stronie copy: jeden fakt („gdzie to biegnie"), jedno miejsce, w którym się go
 * nazywa.
 *
 * CO ZOSTAŁO RÓŻNE, ŚWIADOMIE — i to są dokładnie dwie rzeczy:
 *
 *   1. NAZWA GRUPY (`step-folder`, `check-where`, `serve-where`). To jest klucz, którym
 *      przeglądarka wiąże przyciski w jeden wybór, a nie tekst dla człowieka. Zawężają się do
 *      niego kryteria spoza tego pliku — jedno z nich w prawdziwej przeglądarce — więc nazwa
 *      jedzie propsem, zamiast zostać ujednolicona przy okazji.
 *   2. LISTA ODPOWIEDZI. Kafelek „uruchom i zostaw" ma dwie uczciwe, nie trzy: własna kopia
 *      serwowałaby kod, którego nikt w tym biegu nie tknął — czyli ten sam błąd co folder
 *      projektu, tylko drożej. Odpowiedź, której nie ma, jest tu tańsza niż wyszarzona
 *      (niezmiennik 16).
 *
 * `aria-label` na samym przycisku, choć etykieta go otacza: kryterium w przeglądarce szuka tych
 * trzech po nazwie dostępnej i musi trafiać w JEDEN element, a nie w przycisk plus otaczające
 * zdanie z podpowiedzią.
 */
import type { ReactElement } from 'react';

import type { Folder } from '../../../state/workflows';

/** Trzy miejsca, które plik umie zachować bez dodatkowej ścieżki. `pick` nie jest jednym z nich
 * i celowo nie zaznacza żadnego: plik z ręcznie wpisaną ścieżką ma pokazać, że wyboru nie
 * dokonano, zamiast zaznaczać coś, czego w nim nie ma (niezmiennik 17). */
export type Place = 'project' | 'fresh-copy' | 'same-copy';

/** Pytanie nad grupą. Jedno na wszystkie trzy rodzaje kafelka.
 *
 * NIE JEST eksportowane, i to jest ta sama decyzja, co przy brzmieniach niżej: kryterium, które
 * zaimportowałoby ten napis i sprawdziło, czy trzy panele go zawierają, zgadzałoby się z sobą
 * także wtedy, gdyby te panele trzymały trzy własne kopie. Trzy panele porównuje się między
 * sobą, wyjmując napis z ich markupu (`where-it-works-is-one-control.test.tsx`). */
const WHERE_IT_WORKS = 'Where it works';

interface Choice {
  use: Place;
  /** Zdanie, które człowiek czyta i po którym wybiera. */
  label: string;
  /** Co ta odpowiedź robi z pracą kroków przed tym — bo to jest cała różnica między nimi. */
  note: string;
}

/* Brzmienia są ZE SPLOTU obu kopii i muszą być prawdziwe dla wszystkich trzech rodzajów
 * kafelka: agent, sprawdzenie i „uruchom i zostaw" robią z tym folderem to samo, choć każdy
 * z innego powodu. Stąd zdania mówią o PRACY KROKÓW PRZED TYM, a nie o kodzie, testach ani
 * serwerze — te trzy słowa byłyby prawdziwe zawsze tylko w jednym z trzech paneli. */
const PLACES: readonly Choice[] = [
  {
    use: 'project',
    label: 'Work in the project folder',
    note: 'Uses your project as it stands right now, without the work of the steps before it.',
  },
  {
    use: 'fresh-copy',
    label: 'Start in a new copy of the files',
    note: 'Its own copy, so it cannot write over a step running beside it. It sees none of their work.',
  },
  {
    use: 'same-copy',
    label: 'Continue in the same files as the previous step',
    note: 'Uses the work the step right before this one left behind.',
  },
];

export interface WhereItWorksProps {
  /** Klucz grupy przycisków — patrz nagłówek pliku. */
  group: 'step-folder' | 'check-where' | 'serve-where';
  /** Które odpowiedzi ma ten kafelek. Pominięte znaczy wszystkie trzy. */
  offers?: readonly Place[];
  value: Folder;
  onChoose: (folder: Folder) => void;
}

export function WhereItWorks({ group, offers, value, onChoose }: WhereItWorksProps): ReactElement {
  const shown = PLACES.filter((one) => offers === undefined || offers.includes(one.use));

  return (
    <fieldset data-row="where" className="stack">
      <legend className="label">{WHERE_IT_WORKS}</legend>
      {shown.map((choice) => (
        <label key={choice.use} className="flex items-baseline gap-2 text-body text-ink">
          <input
            type="radio"
            name={group}
            value={choice.use}
            aria-label={choice.label}
            checked={value.use === choice.use}
            onChange={() => {
              onChoose({ use: choice.use });
            }}
          />
          <span className="flex min-w-0 flex-col gap-0.5">
            <span>{choice.label}</span>
            <span className="lead">{choice.note}</span>
          </span>
        </label>
      ))}
    </fieldset>
  );
}
