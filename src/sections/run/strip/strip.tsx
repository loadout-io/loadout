/* Pasek loadoutu — podpis wizualny aplikacji (DESIGN §2, makieta `docs/mockup/index.html:377`).
 *
 * Komponent jest głupi z premedytacją: dostaje gotowy `Strip` z `./model` i go rysuje. Ani
 * jednego `if` o stanie kroku, ani jednego licznika — decyzja, który blok jest wypełniony,
 * a który tylko obrysowany, jest funkcją stanu KROKU i mieszka w modelu, gdzie da się ją
 * sprawdzić bez okna (niezmiennik 15).
 *
 * Bloki NIE SĄ klikalne i to jest świadome. DESIGN §2 obiecuje „klik w blok pokazuje historię
 * tego kroku", ale filtrowanie widoku po kroku nie istnieje w tej wersji, a kontrolka, która
 * wygląda na klikalną i nic nie robi, jest gorsza niż jej brak (niezmiennik 16). Wraca razem
 * z filtrem, jednym `onSelect` w tym pliku.
 *
 * Wysokość 56 px to cały budżet, jaki ten pasek ma w suficie chrome [ARCHITECTURE §7]: karty
 * 34 px + pasek 56 px = 90 z 96. Każdy piksel dołożony tutaj trzeba komuś zabrać.
 */
import type { ReactElement } from 'react';
import type { Block, Strip as StripModel } from './model';

export interface StripProps {
  strip: StripModel;
}

/**
 * Klasy bloku dla trzech stanów [DESIGN §2]: wypełniony, akcent, obrys.
 *
 * `now` jest jedynym nasyconym elementem na ekranie — reguła jednego akcentu (DESIGN §3).
 * Rzecz skończona jest cicha, więc `done` jest przygaszone, a nie zielone: zielony znaczy
 * „dzieje się teraz", nie „udało się".
 */
const BLOCK: Readonly<Record<Block['state'], string>> = {
  done: 'bg-muted',
  now: 'bg-accent',
  todo: 'border border-line',
};

/* Krok, który się skończył bez sukcesu, zostaje obrysem — ale obrysem PRZERYWANYM. Nie ma dla
 * niego czwartego stanu (DESIGN §2 zna trzy) i nie wolno mu dać koloru błędu: pominięty krok
 * nie jest zepsuty, a `--fail` odpowiada na pytanie „co poszło źle". Przerwana kreska mówi
 * dokładnie tyle, ile wiemy: ten krok już się nie wydarzy. */
const ENDED = 'border-dashed';

export function Strip({ strip }: StripProps): ReactElement {
  return (
    <div data-strip className="flex h-14 shrink-0 items-center gap-4 rounded-sq bg-panel px-4">
      <div className="flex items-start gap-2">
        {strip.blocks.map((block) => (
          <span key={block.id} className="flex flex-col items-center gap-1">
            <span
              className={`h-2 w-8 rounded-sq ${BLOCK[block.state]} ${block.ended ? ENDED : ''}`}
            />
            <span className="text-label text-muted">{block.name}</span>
          </span>
        ))}
      </div>

      {/* Podpis, i tylko tutaj. Numer kroku i czas trwania biegu żyją WYŁĄCZNIE na tym pasku
          (niezmiennik 13): nagłówek z własnym licznikiem i linia `done` z własnym czasem to
          trzy żywe regiony na jeden fakt przy limicie 1. */}
      {strip.caption === '' ? null : <p className="text-body text-muted">{strip.caption}</p>}
    </div>
  );
}
