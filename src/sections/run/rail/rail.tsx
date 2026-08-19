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
 * KAFELEK JEST PRZYCISKIEM OD 2026-08-18, I TO JEST DOMKNIĘCIE ZAPOWIEDZIANE W TYM AKAPICIE.
 * Stało tu, że kafelek zostaje `<span>`, dopóki ekran jednego agenta nie ma miejsca montowania:
 * `session/{filter,layout,density}.ts` miały komplet logiki, 354 linie, trzynaście przypadków
 * testowych i ZERO wołających produkcyjnych. Miejsce montowania stoi teraz obok, w tym samym
 * pliku (`session/mount.tsx` niżej), więc przycisk naprawdę coś otwiera i przestaje być
 * obietnicą (niezmiennik 16).
 *
 * DLACZEGO PRZYCISK SIEDZI W ŚRODKU `<span data-agent>`, a nie sam go zastąpił. Kryterium
 * z T-39 (`../rail-shows-agents.test.tsx`) tnie markup listy na kafelki po napisie
 * `<span data-agent="`. Zamiana elementu przepisałaby więc CUDZE kryterium — i to nie w jego
 * treści, tylko w parserze, czyli w miejscu, w którym poprawka najłatwiej udaje kosmetykę.
 * Zewnętrzny `<span>` jest komórką siatki, przycisk jest całą powierzchnią kafelka; dzień,
 * w którym tamten parser przestanie pytać o element, jest dniem, w którym ta warstwa znika.
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
import { AgentScreen } from '../session/mount';
import { openAgent } from '../session/open';
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
    <>
      <aside
        data-rail
        className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] border-l border-line bg-panel"
      >
        {/* Nadoczko sekcji, wiec stopien `text-eyebrow` — on jeden nosi wersaliki (DESIGN §4).
            Do 2026-08-19 stal tu `text-label`, a wersaliki wisialy na TAMTYM stopniu; kiedy T-45
            rozszczepil stopien, ten naglowek po cichu przestal krzyczec, a makieta dalej zadala
            AGENTS. `font-mono` zostaje: makieta trzyma te regule w mono i rodzina zmienia sie
            razem z NIA, w T-48. */}
        <h2 className="px-[14px] pt-3 pb-[9px] font-mono text-eyebrow text-muted">Agents</h2>

        {/* `align-content:start` z makiety: przy jednym agencie kafelek stoi u góry, a nie
            rozciąga się na całą wysokość kolumny. */}
        <div className="grid content-start gap-[6px] overflow-auto px-[10px] pb-3">
          {cards.map((card) => (
            <span key={card.id} data-agent={card.id} className="grid">
              {/* Cała powierzchnia kafelka jest przyciskiem — kliknięcie w imię, w kwadrat
                  i w zdanie robi to samo, bo wszystkie trzy odpowiadają na jedno pytanie
                  („pokaż mi tego agenta"). `text-left`, bo przycisk domyślnie centruje tekst,
                  a kafelek jest wierszem czytanym od lewej. */}
              <button
                type="button"
                onClick={() => {
                  openAgent(card.id);
                }}
                className="grid grid-cols-[22px_minmax(0,1fr)] gap-[9px] rounded-sm border border-line px-[10px] py-[9px] text-left"
              >
                {/* Inicjał w `--ink` na przygaszonym kwadracie tożsamości [DESIGN §3].
                    `aria-hidden`, bo to jest ta sama nazwa jeszcze raz, tylko skrócona do
                    litery — czytnik ekranu przeczytałby ją jako osobny fakt. */}
                <span
                  aria-hidden
                  className="grid size-[22px] place-items-center font-mono text-mono-strong text-ink"
                  style={{ background: `var(${card.square})` }}
                >
                  {card.name.slice(0, 1)}
                </span>

                <span className="grid min-w-0">
                  <CardLine
                    text={card.name}
                    className="truncate font-mono text-mono-strong text-ink"
                  />
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
              </button>
            </span>
          ))}
        </div>
      </aside>

      {/* Ekran jednego agenta. Rysuje się WYŁĄCZNIE wtedy, gdy któryś jest otwarty, i stoi tu
          — obok listy, nie w niej — bo zakrywa całe okno, a nie kolumnę. Miejsce docelowe to
          rząd w siatce ekranu pracy; tamten plik nie należy do tego zadania i kształt propsów
          jest zgłoszony. */}
      <AgentScreen cards={cards} />
    </>
  );
}
