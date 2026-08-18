/* Lista agentów — prawa kolumna widoku pracy, 268 px z makiety (`.work`, `.rail`).
 *
 * DLACZEGO TEN PLIK POWSTAŁ DOPIERO TERAZ. Cała logika tej kolumny wylądowała w T-09 i stała
 * bez komponentu: `roster.ts` liczy kafelki, `card.ts` je składa, `colour.ts` daje tokeny,
 * `say.ts` wybiera zdanie — i ani jeden z tych czterech plików nie miał wołającego spoza
 * własnego testu. To ta sama rodzina, co płótno przed T-26 i `io.ts` przed T-38: mechanizm
 * wylądował, ma testy, nikt go nie podłączył, a test wołający funkcję wprost nie odróżnia
 * „zamontowane" od „istnieje".
 *
 * TEN PLIK NIE PODEJMUJE ANI JEDNEJ DECYZJI O TREŚCI. Nie wybiera zdania, nie liczy stanu,
 * nie przypisuje koloru — bierze gotowe `RailCard` i zamienia je na markup. Gdyby wybierał,
 * polityka „kto to powiedział" istniałaby w dwóch miejscach: raz w `say.ts`, raz tutaj
 * (niezmiennik 23).
 *
 * KAFELEK NIE JEST PRZYCISKIEM, I TO JEST WYBÓR, NIE PRZEOCZENIE. Makieta rysuje go jako
 * `<button class="card">`, bo kliknięcie ma otwierać ekran jednego agenta — a tego ekranu
 * w repo nie ma: `session/{filter,layout,density}.ts` mają komplet logiki i ani jednego
 * miejsca montowania. Przycisk, który obiecuje otwarcie i nie otwiera niczego, jest
 * kontrolką bez handlera i nie wchodzi do repo (niezmiennik 16) — poprzedni prototyp ma trzy takie
 * i wszystkie trzy stoją tam, gdzie nikt nie kliknął drugi raz. Dzień, w którym ekran agenta
 * dostanie swoje miejsce montowania, jest dniem, w którym ten `<span>` staje się `<button>`;
 * do tego czasu kolumna pokazuje fakty i nie udaje, że prowadzi gdziekolwiek.
 *
 * PUSTY MAGAZYN TO ZERO KAFELKÓW (niezmiennik 17). Nie jeden przykładowy, nie „—", nie kafelek
 * agenta z planu, który jeszcze nie ruszył: kafelek istnieje wtedy i tylko wtedy, gdy agent
 * pojawił się w strumieniu, i rozstrzyga to `roster.ts`, nie ten plik.
 *
 * CZTERY LINIE TEKSTU NA KAFELEK, ANI JEDNEJ WIĘCEJ [ARCHITECTURE §7, DESIGN §6 `agent-card`]:
 * nazwa, rola, zdanie, stan. Każdy licznik, który kusi („12 files · 2m 04s"), jest piątą linią
 * — wygląda dobrze przy jednym agencie i rozjeżdża listę przy czterech. Czas i koszt mieszkają
 * na pasku loadoutu i nigdzie indziej (niezmiennik 13).
 *
 * KOLOR KWADRATU TO TOŻSAMOŚĆ, NIGDY STAN — także dla agenta `failed`. Stan jest SŁOWEM
 * w kolorze nasyconym [DESIGN §3, „Tożsamość ≠ stan"]. Ta reguła powstała, bo referencyjny
 * poprzedni prototyp dawał agentowi Forge dokładnie ten sam hex, co „wymaga uwagi", i na jednym ekranie
 * dwie różne rzeczy znaczyły to samo.
 */
import type { ReactElement } from 'react';
import type { RailCard } from './card';
import { statusToken } from './colour';

/**
 * Szerokość kolumny w pikselach — 268 z reguły `.work` w makiecie.
 *
 * Liczba stoi TUTAJ, bo to jest kolumna tego komponentu; ekran pracy składa z niej deklarację
 * siatki. Drugi literał `268` w `index.tsx` byłby drugim miejscem, w którym mieszka ta sama
 * liczba, i pierwszym, które rozjedzie się z makietą (niezmiennik 13). Tak samo robi
 * `NAV_WIDTH` w `src/ui/shell/titlebar.tsx`.
 */
export const RAIL_WIDTH = 268;

export interface RailProps {
  /** Kafelki, już policzone przez `roster()`. Pusta lista znaczy „nikt jeszcze nic nie nadał". */
  readonly cards: readonly RailCard[];
}

/**
 * Jedna linia tekstu kafelka.
 *
 * `data-card-line` niosą wszystkie cztery i tylko one — po tym atrybucie liczy się sufit
 * z ARCHITECTURE §7. Linia bez wartości nie istnieje: pusty slot dalej zajmuje wysokość
 * i dalej wygląda jak fakt, którego nie znamy, zamiast jak fakt, którego nie ma.
 */
function CardLine({ text, className }: { text: string; className: string }): ReactElement | null {
  if (text === '') return null;
  return (
    <span data-card-line className={className}>
      {text}
    </span>
  );
}

export function Rail({ cards }: RailProps): ReactElement {
  return (
    <aside
      data-rail
      className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] border-l border-line bg-panel"
    >
      <h2 className="px-[14px] pt-3 pb-[9px] font-mono text-label text-muted">Agents</h2>

      {/* `align-content:start` z makiety: przy jednym agencie kafelek stoi u góry, a nie
          rozciąga się na całą wysokość kolumny. */}
      <div className="grid content-start gap-[6px] overflow-auto px-[10px] pb-3">
        {cards.map((card) => (
          <span
            key={card.id}
            data-agent={card.id}
            className="grid grid-cols-[22px_minmax(0,1fr)] gap-[9px] rounded-sq border border-line px-[10px] py-[9px]"
          >
            {/* Inicjał w `--ink` na przygaszonym kwadracie tożsamości [DESIGN §3]. `aria-hidden`,
                bo to jest ta sama nazwa jeszcze raz, tylko skrócona do litery — czytnik ekranu
                przeczytałby ją jako osobny fakt. */}
            <span
              aria-hidden
              className="grid size-[22px] place-items-center font-mono text-mono-strong text-ink"
              style={{ background: `var(${card.square})` }}
            >
              {card.name.slice(0, 1)}
            </span>

            <span className="grid min-w-0">
              <CardLine text={card.name} className="truncate font-mono text-mono-strong text-ink" />
              <CardLine text={card.role} className="truncate font-mono text-label text-muted" />
              <CardLine text={card.say.text} className="mt-[3px] truncate text-body" />
              {/* Stan jest SŁOWEM w kolorze nasyconym — nigdy kolorem kwadratu [DESIGN §3]. */}
              <span
                data-card-line
                data-status
                className="mt-[5px] font-mono text-label"
                style={{ color: `var(${statusToken(card.status)})` }}
              >
                {card.status}
              </span>
            </span>
          </span>
        ))}
      </div>
    </aside>
  );
}
