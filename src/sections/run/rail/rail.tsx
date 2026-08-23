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
import { useEffect, useSyncExternalStore } from 'react';
import { saidOf } from '../entry/echo';
import { runFeed } from '../feed/live';
import { AgentScreen } from '../session/mount';
import { openAgent } from '../session/open';
import type { RailCard } from './card';
import { statusToken } from './colour';
import type { Held } from './processes';
import {
  closeStarted,
  openStarted,
  openedStarted,
  railGroups,
  refreshStarted,
  startedThings,
  stopStarted,
  subscribeToStarted,
} from './processes';

/**
 * Szerokość kolumny w pikselach — 268 z reguły `.work` w makiecie.
 *
 * Liczba stoi TUTAJ, bo to jest kolumna tego komponentu; ekran pracy składa z niej deklarację
 * siatki. Drugi literał `268` w `index.tsx` byłby drugim miejscem, w którym mieszka ta sama
 * liczba, i pierwszym, które rozjedzie się z makietą (niezmiennik 13). Tak samo robi
 * `NAV_WIDTH` w `src/ui/shell/titlebar.tsx`.
 */
export const RAIL_WIDTH = 268;

/**
 * Jak często kolumna pyta rejestr, co jeszcze biegnie — w milisekundach.
 *
 * Sekunda, i to jest wybór z zapisaną ceną. W górę: kafelek nad rzeczą, która zeszła, stoi tyle,
 * ile wynosi ten odstęp, a „Running" nad komendą zeszłą pięć sekund temu jest tym samym
 * kłamstwem, co nad zeszłą dwie minuty temu, tylko krócej. W dół: pytanie jest jednym przejściem
 * granicy i jednym wzięciem zamka, więc dziesięć razy częstsze nie kupuje nic, czego człowiek by
 * zobaczył — oko nie odróżnia 100 ms od 1 s przy kafelku, który stoi godzinami.
 *
 * Odstęp, nie zdarzenie z drutu, i powód stoi w nagłówku `./processes.ts`: kanał do okna wiezie
 * WIERSZE STRUMIENIA, a rzecz uruchomiona komendą nie jest agentem i nie ma w strumieniu czego
 * pisać (niezmiennik 17).
 */
const ASK_AGAIN = 1_000;

/** `button-quiet` z DESIGN §6, ta sama fraza co w strumieniu i na ekranie agenta. */
const QUIET = 'h-7 rounded-sm border border-line px-3 text-ui text-body';

export interface RailProps {
  /** Kafelki, już policzone przez `roster()`. Pusta lista znaczy „nikt jeszcze nic nie nadał". */
  readonly cards: readonly RailCard[];
}

/**
 * Zdanie, którym Rust odpowiada na powtórzenie kroku → DO STRUMIENIA, nie do slotu obok.
 *
 * Odpowiedź przychodzi wtedy i tylko wtedy, gdy dzisiejszy plik workflow różni się od tego,
 * który wtedy biegł: „to samo jeszcze raz" i „to samo z twoją poprawką" nie mogą wyglądać
 * identycznie. Zdanie bez miejsca do wylądowania jest ciszą, a cisza po naciśnięciu wygląda
 * dokładnie jak przycisk, który nic nie robi (niezmiennik 16).
 *
 * DO STRUMIENIA, bo rozmowa z Loadoutem jest JEDNĄ historią — tą samą drogą idzie odmowa startu
 * (`../index.tsx`) i echo wiersza wejścia (`../entry/entry.tsx`). Wiersz w `useState` tego
 * komponentu ginąłby przy pierwszym wyjściu do innej sekcji, a bieg trwa dłużej niż ekran.
 *
 * FUNKCJA MODUŁOWA, nie domknięcie w komponencie, i to jest ten sam powód, co przy `../session
 * /open.ts`: to repo nie ma jsdom, więc handler zamknięty w komponencie byłby kodem, którego
 * żadne kryterium nie umie dotknąć.
 */
export function sayAfterRunningAgain(said: string): void {
  runFeed.appendLines([saidOf(said)]);
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
  /* MAGAZYNY CZYTAMY PRZEZ `useSyncExternalStore` Z BIEŻĄCYM STANEM JAKO MIGAWKĄ SERWEROWĄ.
   * `renderToStaticMarkup` jest rendererem serwerowym, a ta aplikacja nigdy nie hydratuje
   * serwerowego HTML-a — więc powód, dla którego React chce tam stanu początkowego, tutaj nie
   * istnieje. Ten sam zapis stoi w `../session/mount.tsx` i w `../index.tsx`. */
  const started = useSyncExternalStore(subscribeToStarted, startedThings, startedThings);
  const opened = useSyncExternalStore(subscribeToStarted, openedStarted, openedStarted);

  /* ODŚWIEŻANIE JEST JEDYNĄ DROGĄ, KTÓRĄ TA KOLUMNA DOWIADUJE SIĘ O ŚMIERCI. Rzecz uruchomiona
   * komendą nie jest agentem i nie ma w strumieniu czego pisać (niezmiennik 17), więc kanał
   * biegu jej nie wiezie — a kafelek ma istnieć dokładnie tak długo, jak ona.
   *
   * Pierwsze pytanie idzie od razu, nie po sekundzie: po przeładowaniu okna magazyn jest pusty,
   * a rejestr po tamtej stronie granicy żyje dalej i wie o wszystkim, co jeszcze biegnie.
   *
   * `renderToStaticMarkup` nie odpala efektów, więc cudze kryteria montujące ten ekran nie
   * wołają tędy ani jednego `invoke` — kolumna sądzona jest wtedy za to, co jej podano. */
  useEffect(() => {
    void refreshStarted();
    const asking = setInterval(() => {
      void refreshStarted();
    }, ASK_AGAIN);
    return () => {
      clearInterval(asking);
    };
  }, []);

  const groups = railGroups({ agents: cards, started });
  const inside = started.find((one) => one.id === opened) ?? null;

  return (
    <>
      <aside
        data-rail
        /* OBA RZĘDY MAJĄ WŁASNY SUFIT, i to jest naprawa zgłoszona ze zrzutu właściciela:
         * „jak dużo step to nam wyjebuje UI" (2026-08-22). Rząd pierwszy stał na `auto`, więc
         * rósł z każdą rzeczą uruchomioną komendą i spychał listę agentów poza okno — a rząd
         * drugi umiał się przewijać dopiero wtedy, gdy cokolwiek zostało mu z wysokości.
         *
         * `minmax(0,auto)` zamiast `auto` daje pierwszemu rzędowi prawo się SKURCZYĆ, a
         * `overflow-hidden` na kolumnie odbiera jej prawo do przerośnięcia własnego toru.
         * Od tej pary obie sekcje przewijają się osobno i żadna nie wypycha drugiej. */
        className="grid min-h-0 grid-rows-[minmax(0,auto)_minmax(0,1fr)] overflow-hidden border-l border-line bg-panel"
      >
        <div className="grid min-h-0 overflow-auto">
          {/* RZECZY URUCHOMIONE KOMENDĄ STOJĄ NAD AGENTAMI, we własnej grupie, i to jest cała
              różnica, którą ta kolumna ma pokazać: jedno Loadout prowadzi, drugie człowiek kazał
              uruchomić, a „stop" pod każdym znaczy co innego. Kolor kwadratu ich NIE rozróżnia —
              kolor jest tożsamością, nigdy stanem [DESIGN §3]; rozróżnia je miejsce.

              ZERO KAFELKÓW TO ZERO NAGŁÓWKA (niezmiennik 17, DESIGN §6): nagłówek nad pustką jest
              gorszy niż jego brak, bo obiecuje listę, na którą nic nigdy nie wejdzie. */}
          {groups.started.length === 0 ? null : (
            <div className="grid content-start gap-[6px] border-b border-line px-[10px] pt-3 pb-3">
              <h2 className="px-[4px] font-mono text-eyebrow text-muted">Started</h2>
              {groups.started.map((card) => (
                <StartedTile
                  key={card.id}
                  card={card}
                  held={started.find((one) => one.id === card.id) ?? null}
                />
              ))}
            </div>
          )}

          {/* Nadoczko sekcji, wiec stopien `text-eyebrow` — on jeden nosi wersaliki (DESIGN §4).
              Do 2026-08-19 stal tu `text-label`, a wersaliki wisialy na TAMTYM stopniu; kiedy T-45
              rozszczepil stopien, ten naglowek po cichu przestal krzyczec, a makieta dalej zadala
              AGENTS. `font-mono` zostaje: makieta trzyma te regule w mono i rodzina zmienia sie
              razem z NIA, w T-48. */}
          <h2 className="px-[14px] pt-3 pb-[9px] font-mono text-eyebrow text-muted">Agents</h2>
        </div>

        {/* `align-content:start` z makiety: przy jednym agencie kafelek stoi u góry, a nie
            rozciąga się na całą wysokość kolumny. */}
        <div className="grid content-start gap-[6px] overflow-auto px-[10px] pb-3">
          {groups.agents.map((card) => (
            <span
              key={card.id}
              data-agent={card.id}
              /* Dwie kolumny, kiedy krok da się powtórzyć — ta sama para, co przy rzeczach
                 uruchomionych komendą. Przycisk NIE może stać wewnątrz kafelka: kafelek sam
                 jest przyciskiem, a przycisk w przycisku to znacznik, którego przeglądarka
                 nie ma prawa narysować poprawnie. */
              className="grid"
            >
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
          jest zgłoszony.

          `onSaid` DOSZŁO 2026-08-23 i jest całą naprawą „Run this step again". Ekran agenta
          rysuje ten przycisk wyłącznie wtedy, gdy ma dokąd oddać odpowiedź — a tu, w jedynym
          miejscu montażu, propsu nie było. Cała droga pod spodem (`rerun_step`, `../io.ts`,
          `./again.ts`, przycisk w `../session/session.tsx`) miała wołających wyłącznie
          w testach: mechanizm działa, kiedy go zawołać, i nikt go nie wołał (niezmiennik 29). */}
      <AgentScreen cards={cards} onSaid={sayAfterRunningAgain} />

      {/* Wyjście jednej rzeczy uruchomionej komendą. Ten sam kształt, co ekran agenta, i z tego
          samego powodu: zakrywa całe okno, a nie kolumnę — a rzecz pod nim biegnie dalej. */}
      {inside === null ? null : <StartedOutput held={inside} />}
    </>
  );
}

/**
 * Kafelek rzeczy uruchomionej komendą: cała powierzchnia otwiera jej wyjście, a Stop stoi obok.
 *
 * DWA PRZYCISKI, NIE JEDEN, bo odpowiadają na dwa różne pytania: „pokaż mi, co ona mówi" i „skończ
 * ją". Zagnieżdżony byłby markupem, którego przeglądarka nie przyjmuje, a jeden przycisk na oba
 * znaczyłby, że wejście w kafelek zabija to, w co się weszło.
 *
 * STOP RYSUJE SIĘ WYŁĄCZNIE PRZY ZNANEJ GRUPIE. Dopóki Rust nie odpowiedział, którą to grupa, nie
 * ma czego ubić — a przycisk, który nie ma czego zrobić, jest kontrolką bez handlera
 * (niezmiennik 16). Trwa to jedno wywołanie, więc człowiek go w praktyce nie zobaczy zgaszonego.
 */
function StartedTile({ card, held }: { card: RailCard; held: Held | null }): ReactElement {
  const pgid = held?.pgid ?? null;
  return (
    <span
      data-started={card.id}
      className="grid grid-cols-[minmax(0,1fr)_auto] items-stretch gap-[6px]"
    >
      <button
        type="button"
        onClick={() => {
          openStarted(card.id);
        }}
        className="grid grid-cols-[22px_minmax(0,1fr)] gap-[9px] rounded-sm border border-line px-[10px] py-[9px] text-left"
      >
        {/* Kwadrat tożsamości, z tej samej przygaszonej palety, co u agentów: różnicę niesie
            miejsce na liście, nigdy odcień [DESIGN §3]. Inicjał jest pierwszym znakiem komendy,
            bo komenda JEST nazwą tej rzeczy. */}
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
      </button>

      {pgid === null ? null : (
        <button
          type="button"
          aria-label={'Stop ' + card.name}
          onClick={() => {
            void stopStarted(card.id);
          }}
          className={QUIET}
        >
          Stop
        </button>
      )}
    </span>
  );
}

/**
 * Wyjście tej jednej rzeczy — to, co otwiera kliknięcie w kafelek.
 *
 * Zamówienie właściciela brzmiało „po kliku mogę tam wejść", więc panel niesie dwie rzeczy
 * i tylko dwie: KTÓRĄ komendę pokazuje i co ona powiedziała. Kafelek, w który da się wejść i nie
 * ma tam nic, jest kontrolką bez skutku z dodatkowym krokiem (niezmiennik 16).
 *
 * KOMENDA W NAGŁÓWKU, bo bez niej ten panel wygląda identycznie dla każdej rzeczy — a przy dwóch
 * biegnących człowiek czyta jedną i patrzy na drugą.
 */
function StartedOutput({ held }: { held: Held }): ReactElement {
  return (
    <div className="fixed inset-0 z-10 grid grid-rows-[auto_minmax(0,1fr)] bg-bg">
      <div className="flex h-13 shrink-0 items-center gap-3 border-b border-line bg-panel px-[18px]">
        <button type="button" aria-label="Back to the run" onClick={closeStarted} className={QUIET}>
          ←
        </button>
        <h1 className="min-w-0 truncate font-mono text-mono-strong text-ink">{held.command}</h1>
        {held.pgid === null ? null : (
          <button
            type="button"
            onClick={() => {
              void stopStarted(held.id);
            }}
            className={QUIET + ' ml-auto'}
          >
            Stop
          </button>
        )}
      </div>

      <div data-started-output className="min-h-0 overflow-auto p-[18px]">
        {/* Nagłówek panelu niesie komendę raz; tutaj stoi drugi raz i to jest świadome: kryterium
            AC-4 czyta TEN region i pyta, czy to, co się otworzyło, należy do tej właśnie linii.
            Bez tego panel odpowiadałby na „czy cokolwiek się otworzyło", a to jest słaba wersja
            tego samego zdania. `text-label`, bo to podpis nad wyjściem, nie tytuł. */}
        <p className="pb-[9px] font-mono text-label text-muted">{held.command}</p>
        {held.said === '' ? (
          /* PUSTKA MÓWI, ŻE JEST PUSTA, i mówi to zdaniem, nie kreską: rzecz, która jeszcze nic
             nie napisała, wygląda tak samo jak panel, który nie umie nic pokazać. */
          <p data-empty className="text-body text-muted">
            Nothing has come back from it yet.
          </p>
        ) : (
          /* `pre`, bo to jest tekst, który ktoś inny sformatował: sklejone spacje i zgubione
             złamania linii zamieniają tabelkę z `npm` w jeden akapit. Kuracja tego strumienia
             nie istnieje z rozmysłu (decyzja D4: kurowany strumień jest dla AGENTÓW, a to nie
             jest agent) — i nie udaje terminala, bo nie ma tu ani kolorów, ani kursora. */
          <pre className="whitespace-pre-wrap break-words font-mono text-mono text-body">
            {held.said}
          </pre>
        )}
      </div>
    </div>
  );
}
