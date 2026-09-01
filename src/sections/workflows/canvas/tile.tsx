/* Kafelek na płótnie. Dwa rodzaje, cztery linie tekstu, stopka WYLICZANA ze strzałek.
 *
 * Niezmiennik 17 mieszka w kształcie propsów. Stopka (`first step` / `after Plan` /
 * `reads 3 handoffs` po lewej, `runs before ▸` po prawej) nie jest napisem podanym z zewnątrz
 * ani wpisanym na sztywno w komponent — jest liczona z `links`. Wpisana na sztywno wygląda
 * identycznie do chwili, w której ktoś przesunie strzałkę, i wtedy kłamie po cichu.
 *
 * Dlatego kafelek dostaje CAŁE `links` i `steps`, a nie gotowy podpis: `steps` są potrzebne,
 * bo stopka nazywa poprzednika NAZWĄ, a `s_plan` nie jest niczym, co użytkownik widzi.
 * Przy piętnastu kafelkach koszt jest zerowy, a alternatywa — policzenie podpisu w płótnie
 * i podanie propsem — przenosi ten sam kod o jedno piętro wyżej i wyjmuje go spod kryterium.
 *
 * Komponent jest STEROWANY: `selected` przychodzi propsem i NIE jest zapisywane do pliku
 * (T3 §3.3, kryterium `to-file`).
 *
 * 2026-08-18 — UKŁAD Z MAKIETY, cztery rzeczy, których tu nie było. Do tego dnia kafelek był
 * `<b>{step.name}</b>`, akapitem i stopką bez linii, o szerokości `w-70` (280 px). Makieta
 * (`.node`, `docs/mockup/index.html:243`) ma 246 px, uchwyt przeciągania po lewej stronie
 * nazwy, chip agenta z kolorową kostką tożsamości po prawej i stopkę NAD linią. U właściciela
 * kafelki były przy tym rysowane w powiększeniu 2× (`fitView` bez `maxZoom`), więc różnica
 * 280 do 246 była najmniejszym z problemów tego ekranu.
 *
 * CHIP AGENTA JEST OPCJONALNY i to jest treść niezmiennika 17 w tym pliku: kafelek rysuje go
 * WYŁĄCZNIE wtedy, gdy dostał agenta, którego `step.agent` naprawdę wskazuje. Wypisanie tam
 * identyfikatora albo słowa zastępczego byłoby relacją, której w danych nie ma — a krok bez
 * agenta ma o tym mówić brakiem chipu, nie wypełniaczem.
 */
import type { ReactElement } from 'react';
import type { Agent, Color } from '../../../state/agents';
import type { Link, Step } from '../../../state/workflows';

export interface StepTileProps {
  step: Step;
  /** Wszystkie kroki — stopka nazywa poprzednika po nazwie, nie po identyfikatorze. */
  steps: Step[];
  /** Wszystkie strzałki. Stopka jest z nich wyliczana (niezmiennik 17). */
  links: Link[];
  /**
   * Agent, którego ten krok nazywa — jeżeli jest w bibliotece.
   *
   * Bez niego chip nie powstaje. Rozwiązanie `step.agent` należy do płótna, bo tam mieszka
   * lista agentów; kafelek, który rozwiązywałby je drugi raz u siebie, mógłby rozwiązać
   * inaczej niż panel i pokazać przy kroku innego agenta niż ten, którego panel nazywa.
   */
  agent?: Agent;
  /** Zaznaczenie jest stanem płótna, nie polem pliku. */
  selected?: boolean;
}

/* `node-card` z makiety (`.node`): 246 px, `--raised`, obrys `--line-strong`, zaznaczony
 * `--accent`. Szerokość jest stała, bo płótno układa kafelki w kolumny — kafelek, który rośnie
 * z treścią, przesuwa sąsiadów przy każdej zmianie nazwy.
 *
 * `w-61.5` to `calc(var(--spacing) * 61.5)`, czyli 246 px przy bazie 4 px. Nie `w-[246px]`:
 * liczba w klasie jest literałem rozmiaru, a mnożnik siatki nią nie jest — i przeżyje zmianę
 * bazy. ROZJAZD, świadomy i zgłoszony: DESIGN §6 mówi w tym miejscu 280 px, makieta 246,
 * a przy rozbieżności wygrywa makieta. */
/* `.card` niesie promień, padding i obrys; szerokość, wypełnienie `--raised` i mocny obrys
 * są tym, co ten kafelek ma INACZEJ niż karta i dlatego stoją obok nazwy (makieta `.node`).
 *
 * `.enter` odpowiada na „czy to właśnie weszło" (DESIGN §7): React Flow montuje wyłącznie
 * kafelek, który naprawdę doszedł, więc `＋ Add step` daje jedno dorastające pudełko, a nie
 * przeskok całego płótna. Przeciągnięcie i zaznaczenie kafelka NIE montują — animacja nie
 * powtarza się przy każdym ruchu myszy. */
const CARD = 'card enter w-61.5 bg-raised text-body';
const CARD_LINE = 'border-line-strong';
const CARD_SELECTED = 'border-accent';

/** To, co stoi po PRAWEJ stronie nazwy: chip agenta albo podpis rodzaju kroku.
 *
 * NAZWA JEST TREŚCIĄ, RESZTA TEGO WIERSZA JEST METADANĄ, i do 2026-08-31 kafelek rozstrzygał
 * to odwrotnie. Zgłoszenie właściciela ze zrzutu: kroki „Reaserch…" i „Arc…", ucięte na
 * kafelku 246 px, przy metrach pustego płótna obok. Zmierzone w chromium na tym samym pliku:
 * wiersz ma 186 px użytecznych (246 minus padding karty, uchwyt i dwa odstępy), chip agenta
 * `ai-systems-architect` był `shrink-0` i brał z nich 147, a na nazwę „Research" — która chce
 * 61 px — zostawało 39. Czyli nie „ciasno": treść ustępowała metadanej co do zasady, bo tak
 * była napisana klasa.
 *
 * TRZY DROGI BYŁY NA STOLE i dwie odpadły z mierzalnego powodu.
 *   Kafelek MÓGŁ UROSNĄĆ — ale 246 px to liczba z makiety, a 34 px, które dałoby DESIGN §6,
 *   nie starcza nawet na tę jedną nazwę; rosnąć musiałby o połowę, a wtedy `tidy.ts` układa
 *   inne kolumny i cały ekran przestaje być tym ekranem.
 *   Nazwa MOGŁA ŁAMAĆ SIĘ NA DWIE LINIE — ale kafelek ma sufit CZTERECH linii tekstu
 *   (ARCHITECTURE §7) i wykorzystuje dziś wszystkie cztery: nazwa, dwie linie treści, stopka.
 *   Druga linia nazwy musiałaby odebrać linię temu, co krok ma zrobić.
 *
 * Zostaje: USTĘPUJE METADANA. `max-w-1/2` jest tu całą naprawą i jest twardą obietnicą —
 * cokolwiek stoi po prawej, nazwa dostaje nie mniej niż połowę wiersza. Bez tej granicy sam
 * `truncate` niczego nie zmienia: `flex-1` na nazwie ma bazę 0, więc chip o naturalnej
 * szerokości i tak zabiera swoje najpierw.
 *
 * POŁOWA, A NIE MNIEJ, I TO JEST LICZBA ZMIERZONA. Po prawej stronie nazwy stoją DWIE różne
 * rzeczy i tylko jedna z nich może ustąpić. Chip agenta niesie tekst zmiennej długości
 * i jest metadaną — kiedy się urwie, kostka tożsamości dalej mówi, który to agent, a panel
 * kroku nazywa go w całości. Podpis rodzaju (`asks you`, `runs a check`, `leaves it running`)
 * jest zamkniętą listą trzech napisów i JEDYNĄ rzeczą, po której z płótna widać, czym ten
 * kafelek różni się od sąsiada — urwany kasuje rozróżnienie, dla którego pętla sprawdzająca
 * w ogóle ma sens. Najdłuższy z tej trójki mierzy w chromium 81 px, a połowa wiersza to 93,
 * więc ta granica mieści wszystkie trzy z zapasem 12 px i tnie wyłącznie chip. Zejście niżej
 * (`max-w-2/5`, czyli 74 px) daje nazwie więcej, ale zaczyna ucinać „leaves it running" —
 * pilnuje tego kryterium w `e2e/tests/the-canvas-reads-as-a-board.spec.ts`, zmierzone
 * mutacją na `max-w-1/4`: „wants 81 px and was given 55". */
const ASIDE = 'min-w-0 max-w-1/2 shrink truncate';

/** Kostka tożsamości agenta → nazwa klasy tła.
 *
 * `Agent.color` jest polem, które formularz agenta zapisuje od T-11, a do 2026-08-18 NIC w całym
 * repo go nie czytało: pięć kolorów do wyboru i ani jednego miejsca, w którym wybór był widoczny.
 * Tu jest pierwsze, więc to mapowanie nie ma jeszcze drugiej kopii, z którą mogłoby się rozjechać.
 *
 * `Record`, nie `switch` z gałęzią domyślną: szósty kolor dopisany do `Color` przestaje TU się
 * kompilować, zamiast po cichu wpaść w „reszta" i dostać kolor stanu — a mieszanie tożsamości ze
 * stanem jest dokładnie tym błędem, przez który obie palety są rozdzielone (DESIGN §3). */
const IDENTITY: Readonly<Record<Color, string>> = {
  slate: 'bg-id-1',
  plum: 'bg-id-2',
  clay: 'bg-id-3',
  moss: 'bg-id-4',
  rose: 'bg-id-5',
};

/** Nazwa kroku o tym identyfikatorze.
 *
 * Kiedy strzałka wskazuje krok, którego w pliku nie ma, stopka mówi „another step" zamiast
 * pokazać `s_plan`: identyfikator jest nazwą z drutu i nie ma prawa trafić na ekran
 * (niezmiennik 14). Sam fakt wiszącej strzałki zgłasza walidator, nie kafelek. */
function nameOf(steps: Step[], id: string): string {
  return steps.find((step) => step.id === id)?.name ?? 'another step';
}

/** Lewa połowa stopki, WYLICZONA ze strzałek (niezmiennik 17).
 *
 * Trzy zdania, bo trzy różne fakty: nikt przede mną, jeden krok przede mną, kilka kroków
 * przede mną. Jedno zdanie „reads N handoffs" dla wszystkich trzech kłamie na pierwszym kroku
 * każdego workflow, a nazwanie trzech poprzedników po imieniu nie mieści się w czterech
 * liniach (DESIGN §6). */
function waitsFor(step: Step, steps: Step[], links: Link[]): string {
  const incoming = links.filter((link) => link.to === step.id);
  const first = incoming[0];

  if (first === undefined) return 'first step';
  if (incoming.length === 1) return `after ${nameOf(steps, first.from)}`;
  return `reads ${String(incoming.length)} handoffs`;
}

/** Ile kopii tego kroku biegnie naraz, jeżeli więcej niż jedna (makieta, linia 512). */
function copiesOf(step: Step): string | null {
  return step.kind === 'agent' && step.copies > 1 ? `×${String(step.copies)}` : null;
}

/** Jedno zdanie o tym, co ten kafelek robi. Punkt kontrolny pyta, krok pracuje.
 *
 * Kafelek „uruchom i zostaw" pokazuje SWOJĄ KOMENDĘ, bo ona jest jedynym zdaniem, które napisał
 * o nim człowiek — dokładnie tak samo, jak pytanie jest jedynym zdaniem punktu kontrolnego.
 *
 * 2026-08-23 — KAFELEK „SPRAWDŹ" DOSZEDŁ DO TEJ KARTY i tym samym została jedna karta zamiast
 * dwóch. Do tego dnia rysowała go własna gałąź w `canvas.tsx`, z chipem „checks project"
 * i z wypełniaczem „No command configured" pod nim — a oba zdania mówiły nieprawdę o kafelku
 * postawionym przyciskiem: ten pracuje w kopii kroku przed sobą, nie w projekcie, a o pustej
 * komendzie mówi walidator, jednym zdaniem, na pasku uwag (niezmiennik 13). Pusta komenda
 * zostaje więc pustą linią, a czerwona kropka przy niej jest jedynym, co o tym mówi. */
function saysWhat(step: Step): string {
  if (step.kind === 'agent') return step.instructions;
  if (step.kind === 'serve' || step.kind === 'check') return step.command;
  return step.question ?? '';
}

/** Czy to, co kafelek mówi o sobie, jest WIERSZEM POWŁOKI, a nie zdaniem po angielsku.
 *
 * Krój maszynowy jest tu znaczeniem: komenda ma się czytać jak coś, co zostanie wykonane
 * dosłownie, ze spacjami i myślnikami w tych miejscach, w których je wpisano. */
function showsACommand(step: Step): boolean {
  return step.kind === 'serve' || step.kind === 'check';
}

export function StepTile({
  step,
  steps,
  links,
  agent,
  selected = false,
}: StepTileProps): ReactElement {
  const copies = copiesOf(step);
  const waits = waitsFor(step, steps, links);
  const handsOn = links.some((link) => link.from === step.id);

  return (
    <div className={`${CARD} ${selected ? CARD_SELECTED : CARD_LINE}`} data-step={step.id}>
      <div className="flex items-center gap-2">
        {/* Uchwyt przeciągania z makiety (`.node .grab`). Nie jest `<button>` i nie ma
            handlera z rozmysłem: ciągnie CAŁY kafelek, a to robi React Flow na samym
            kafelku. Przycisk z własnym `onClick` byłby drugą, cichszą drogą do tego samego
            gestu — i tą, która nie działa. */}
        <span
          aria-hidden
          className="grid size-4.5 shrink-0 cursor-grab place-items-center rounded-sm border border-line bg-well font-mono text-label text-muted"
        >
          ⠿
        </span>
        <b className="min-w-0 flex-1 truncate text-heading text-ink">{step.name}</b>
        {step.kind === 'checkpoint' ? <span className={`label ${ASIDE}`}>asks you</span> : null}
        {/* Ten podpis jest jedynym miejscem, w którym z płótna widać RÓŻNICĘ między tym kafelkiem
            a krokiem „sprawdź": tamten czeka na koniec komendy, ten idzie dalej i zostawia ją
            żywą. Bez niego dwa kafelki z wierszem powłoki wyglądają identycznie. */}
        {step.kind === 'serve' ? <span className={`label ${ASIDE}`}>leaves it running</span> : null}
        {/* Druga połowa tej samej różnicy. Ten kafelek CZEKA na koniec komendy i sam orzeka
            wynik — z tego, czy komenda wróciła bez błędu, i z tego, czy w wyjściu stoi wzorzec.
            Bez tego podpisu dwa kafelki z wierszem powłoki wyglądają na płótnie identycznie,
            a różnią się jedyną rzeczą, przez którą pętla weryfikacyjna w ogóle ma sens. */}
        {step.kind === 'check' ? <span className={`label ${ASIDE}`}>runs a check</span> : null}
        {agent === undefined ? null : (
          <span className={`value flex items-center gap-1 text-label ${ASIDE}`}>
            {/* KOSTKA NIGDY NIE USTĘPUJE. Kiedy miejsca zabraknie, ustępuje nazwa agenta —
                ale kolor tożsamości zostaje, więc z płótna dalej widać, KTÓRY to agent,
                nawet gdy z jego imienia widać połowę. */}
            <i className={`block size-2.75 shrink-0 ${IDENTITY[agent.color]}`} />
            <span className="truncate">{agent.name}</span>
          </span>
        )}
      </div>

      {/* Dwie linie, obcięte. Czwarta linia tekstu na kafelku jest błędem projektowym,
          nie ciasnotą (DESIGN §6), a `line-clamp` jest jedynym miejscem, w którym to widać. */}
      <p
        className={`mt-1 line-clamp-2 text-ink ${showsACommand(step) ? 'font-mono text-note' : 'text-body'}`}
      >
        {saysWhat(step)}
      </p>

      {/* Stopka NAD linią (`.node .bot`: `padding-top:7px; border-top:1px solid var(--line)`)
          i w kroju maszynowym, bo to są wartości wyliczone, nie zdania. */}
      {/* `.value` niesie rodzinę maszynową i `tabular-nums`; stopień zostaje przy `--t-label`,
          bo makieta (`.node .bot`, 11 px) jest wyrocznią wyglądu, a kafelek ma 246 px szerokości
          i dwa napisy obok siebie. */}
      <div className="value mt-2 flex items-baseline justify-between gap-2 border-t border-line pt-2 text-label">
        <span>{copies === null ? waits : `${copies} · ${waits}`}</span>
        {handsOn ? <span>runs before ▸</span> : null}
      </div>
    </div>
  );
}
