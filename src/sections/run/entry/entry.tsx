/* Wiersz wejścia widoku pracy (makieta `docs/mockup/index.html`, reguła `.entry`).
 *
 * CO TEN WIERSZ ROBI I DLACZEGO TYLE. Makieta obiecuje tu `/plan · /run · or just say what you
 * want`. `/run` i proza do pracującego agenta już tu są; PLANISTY nie ma i dlatego `/plan` nie ma
 * — wiersz, który przyjmuje zdanie i odpowiada „jeszcze tego nie umiem", jest gorszy od jego braku:
 * obiecuje sposób pracy, którego nie ma (niezmiennik 16), i robi to przy KAŻDYM naciśnięciu Enter.
 * Zachęta wymienia więc dokładnie te komendy, które ten wiersz naprawdę wykonuje, a kryterium AC-4
 * czyta ją z markupu i sprawdza, że każde wymienione słowo jest rozumiane — dopisanie `/plan` do
 * zachęty zapala test, zanim zobaczy je człowiek.
 *
 * 2026-08-19 — `/run` WESZŁO, a wraz z nim upadł argument, który je tu wcześniej blokował.
 * Stało tu, że uruchomienie biegu bierze dwie rzeczy — workflow i limit „ile naraz" — a limit
 * mieszka w `useState` kontrolki Start, więc `/run` musiałoby wybrać go po swojemu i cicho
 * zignorować to, co człowiek ustawił suwakiem (niezmiennik 13 w najgorszym miejscu: w argumencie
 * decydującym, ilu agentów naprawdę ruszy). Rozumowanie było dobre i przestało być prawdziwe
 * 2026-08-18, kiedy limit przeniósł się do modułu `./limits/chosen` — czyli do JEDNEGO miejsca,
 * z którego czyta go i suwak, i pasek kart, i teraz `/run`. Zgłoszenie właściciela („jak ja mam np
 * puścić jakieś workflow i przekazać prompta?") trafiło w wiersz, którego jedyna przeszkoda
 * zniknęła dzień wcześniej.
 *
 * GDZIE MIESZKA POLITYKA `/run`. W `../run-command.ts`: rozbiór linii, wybór domyślny, odmowy
 * i lista nazw do podpowiedzi. Ten plik przewozi tekst i pokazuje odpowiedź, bo tak da się osądzić
 * jedno i drugie — to repo nie ma jsdom, więc Enter jest dla kryterium nieosiągalny.
 *
 * TO NIE JEST DRUGA ŚCIEŻKA DO TYCH CZYNNOŚCI, TYLKO SKRÓT DO TYCH SAMYCH FUNKCJI. `/open` woła
 * dokładnie ten handler, który wisi pod zaproszeniem „Add a workspace" na tym ekranie i pod
 * przyciskiem o tym samym napisie w bocznym menu, a `/stop` ten, który wisi pod Stop. Ekran
 * pracy podaje oba propsem — gdyby ten plik wołał `io.ts` sam, nazwa komendy istniałaby
 * w sekcji dwa razy (niezmiennik 23).
 *
 * 2026-08-18 — CO ZMIENIŁO SIĘ POD `/open`. Wcześniej otwierała się karta na wybranym folderze,
 * bo karta ZNACZYŁA folder. Karty znaczą teraz biegi, a folder pracy jest zakresem — więc ta
 * sama komenda kończy się dziś dołożeniem zakresu. Napis w zachęcie („/open a folder") zostaje
 * prawdziwy: człowiek wskazuje folder, a Loadout zapamiętuje go jako miejsce pracy.
 *
 * ZERO ŻARGONU W TEKŚCIE WIDOCZNYM (niezmiennik 14, DESIGN §8): „folder", „run", „stop" —
 * żadnego `workspace`, `session`, `process`, `execute`.
 */
import type { FormEvent, ReactElement } from 'react';
import { useState } from 'react';

import type { Named } from '../run-command';

/**
 * Skąd bierze się lista, którą Tab uzupełnia PO nazwie tej komendy.
 *
 * `null` znaczy „ta komenda nie ma nazwy do uzupełnienia": `/stop` nie bierze argumentu, a
 * ścieżki dla `/open` nie ma czym sprawdzić, więc podpowiadanie jej byłoby zgadywaniem.
 *
 * Pole w [`KNOWN`], nie druga tabela obok: „co ta komenda wykonuje" i „co jej się podpowiada"
 * to jeden fakt o jednej komendzie (niezmiennik 13). Druga mapa rozjeżdża się w dniu, w którym
 * ktoś doda komendę z argumentem i zapomni jej dopisać — a wtedy wiersz przyjmuje nazwę, której
 * nie pokazuje.
 */
export type Completes = 'workflows' | 'agents' | null;

/**
 * Komendy, które ten wiersz wykonuje — cała lista, w kolejności zachęty.
 *
 * Zamknięta jako WARTOŚĆ, nie jako zdanie w komentarzu: zachęta i odpowiedź „nie znam tego"
 * są z niej składane, więc nie da się dopisać komendy do napisu, nie ucząc jej wiersza.
 */
export const KNOWN = [
  {
    name: '/run',
    tail: 'a workflow',
    does: 'Start a workflow. Add what to build after its name.',
    completes: 'workflows' as Completes,
  },
  {
    name: '/open',
    tail: 'a folder',
    does: 'Choose a folder to work in.',
    completes: null as Completes,
  },
  {
    name: '/stop',
    tail: 'the run',
    does: 'Stop the run that is going.',
    completes: null as Completes,
  },
] as const;

/**
 * Same nazwy — zbiór, który rozstrzyga [`understand`].
 *
 * Wyliczone z [`KNOWN`], nie wpisane drugi raz: lista, którą wiersz WYKONUJE, i lista, którą
 * POKAZUJE, muszą być tą samą wartością. Dwie kopie rozjeżdżają się w dniu, w którym ktoś doda
 * komendę do podpowiedzi i zapomni jej nauczyć — a wtedy wiersz proponuje słowo, którego nie zna.
 */
export const COMMANDS = KNOWN.map((one) => one.name);

export type Command = (typeof KNOWN)[number]['name'];

/**
 * Co człowiek widzi w PUSTYM polu — zachęta, nie opis stanu (DESIGN §6).
 *
 * Składana z [`KNOWN`], więc brzmi dokładnie tak, jak wcześniej brzmiał literał
 * (`/open a folder  ·  /stop the run`), tylko przestała być drugim miejscem prawdy.
 */
export const PROMPT = KNOWN.map((one) => `${one.name} ${one.tail}`).join('  ·  ');

/** Druga linia z makiety (`.entry .hint`): co robi Enter i jak daleko sięga ten wiersz. */
export const HINT =
  'Enter sends it. Start with a slash for a command, or just write to the agent that is working.';

/** Odpowiedź na `/stop`, kiedy nic nie biegnie. Cisza czyta się jak zepsuty klawisz. */
export const NOTHING_RUNS = 'Nothing is running.';

/**
 * Zdanie pod polem: DO KOGO pójdzie to, co człowiek pisze.
 *
 * # Po co to istnieje
 *
 * Rozstrzygnięcie właściciela 2026-08-19: „powinienem wiedzieć co piszę". Wiersz przyjmował prozę
 * i wyglądał identycznie w trzech różnych sytuacjach — gdy zdanie dojdzie do jednego agenta, gdy
 * trzeba wybrać z kilku, i gdy nie ma go komu doręczyć. Człowiek musiał WYSŁAĆ zdanie, żeby się
 * dowiedzieć, co się z nim stanie; przy pracującym agencie to jest tura, za którą ktoś płaci.
 *
 * Trzy stany, trzy zdania, i każde nazywa następny ruch (DESIGN §8):
 * jeden pracujący — mówimy, kto to jest; kilku — trzeba wpisać nazwę, więc je wypisujemy; żaden —
 * proza nie ma adresata i zdanie mówi, czym się zaczyna pracę.
 *
 * TO SAMO ROZSTRZYGA RUST przy Enterze (`commands::run::say_to_agent_inner`), i to nie jest druga
 * kopia polityki: tam mieszka odmowa, tu jej UPRZEDZENIE. Adres bierzemy z listy pracujących
 * kroków, czyli z tego samego faktu, z którego Rust bierze swoją odpowiedź — a nie z osobnego
 * pola „czy można pisać", które mogłoby mówić co innego (niezmiennik 13).
 */
export function whereItGoes(working: readonly string[]): string {
  const [only] = working;
  if (only === undefined) {
    /* NIKT NIE PRACUJE → ROZMOWA Z ORCHESTRATOREM, nie pustka i nie cichy start biegu.
     *
     * 2026-08-19, dwie zmiany w jednym dniu i warto znać obie. Najpierw stała tu wersja, w której
     * proza po cichu STARTOWAŁA wybrany workflow — właściciel odrzucił ją jednym zdaniem („jak
     * piszę bez komendy… to się na nowo całe workflow odpala"). Potem, przez chwilę, stało tu „nie
     * ma komu pisać", co było prawdą i było ubogie: nie było z kim rozmawiać o tym, co dopiero ma
     * się stać. Rozstrzygnięcie: rozmowa TAK, uruchomienie NIE — „tylko komendy determinują akcje
     * workflow" (`commands::chat`). */
    return (
      'Enter sends this to the lead agent — it can talk things through and prepare, ' +
      'but only /run starts work.'
    );
  }
  if (working.length === 1) {
    return 'Enter sends this to ' + only + '. Start with a slash for a command.';
  }
  /* WYPISUJEMY NAZWY, bo przy kilku pracujących trzeba jedną WPISAĆ na początku linii — dokładnie
   * tak, jak każe odmowa `RunError::SeveralAreWorking`. Sama liczba („2 agents are working")
   * mówiłaby, że jest problem, i nie mówiłaby, jak go rozwiązać. */
  return (
    String(working.length) + ' agents are working, so put a name first: ' + working.join(', ') + '.'
  );
}

/**
 * Odpowiedź na słowo, którego ten wiersz nie zna — z listą, która JEST listą, nie kopią.
 *
 * Wyliczanka po angielsku, nie `join(' and ')`: przy trzech komendach tamto dawało
 * „/run and /open and /stop", czyli zdanie, którego nikt nie napisałby ręcznie. Przy dwóch
 * różnicy nie było i dlatego stało tak do 2026-08-19.
 */
export const NOT_KNOWN =
  'That one is not known here. This line takes ' +
  (COMMANDS.length < 2
    ? COMMANDS.join('')
    : COMMANDS.slice(0, -1).join(', ') + ' and ' + COMMANDS[COMMANDS.length - 1]) +
  '.';

/**
 * Komenda, którą niesie ta linia — albo `null`.
 *
 * Rozstrzyga PIERWSZE słowo, nie całe zdanie: `/open ~/Projects/x` ma otworzyć wybór folderu,
 * a nie odbić się od nierozpoznanej linii. Reszty wiersz dziś nie czyta i nie udaje, że czyta
 * — ścieżki wpisanej z palca nie ma czym sprawdzić, a karta otwarta na folder, którego nie ma,
 * jest kłamstwem o dysku (niezmiennik 4).
 */
export function understand(typed: string): Command | null {
  const first = typed.trim().split(/\s+/)[0]?.toLowerCase() ?? '';
  return KNOWN.find((one) => one.name === first)?.name ?? null;
}

/**
 * Komendy, które pasują do tego, co człowiek już wpisał.
 *
 * 2026-08-18 — PO CO TO ISTNIEJE, zgłoszone przez właściciela ze zrzutu ekranu: „tu mi komend nie
 * podpowiada, nie wiem co ten terminal robi jak nie ma podpowiedzi". I miał rację co do przyczyny,
 * nie tylko co do objawu: lista komend żyła WYŁĄCZNIE w `placeholder`, a placeholder znika przy
 * pierwszym wpisanym znaku. Człowiek pisał `/`, zostawał z pustą linią i jedynym zdaniem
 * „te dwie są wszystkim, co ta linia rozumie" — które ich nie nazywa.
 *
 * Pusty wynik znaczy „nic nie pasuje" i wtedy wiersz mówi to WPROST, zamiast czekać z tą
 * odpowiedzią do naciśnięcia Enter. Odpowiedź po fakcie jest kosztem tury; odpowiedź w trakcie
 * pisania jest podpowiedzią.
 *
 * Dopasowanie po PREFIKSIE pierwszego słowa: `/o` proponuje `/open`, a `/open ~/Projects`
 * proponuje dalej `/open`, bo pierwsze słowo się nie zmieniło i człowiek nie zgubił kontekstu
 * w połowie pisania ścieżki.
 *
 * 2026-08-19 — PO `/run ` PODPOWIADAMY NAZWY WORKFLOW, i to jest zgłoszenie właściciela:
 * „powinno podpowiadać jakie workflow, tam podpowiadajka powinna być". Makieta obiecuje to samo
 * w drugiej linii tego wiersza („Tab completes a workflow"). Komenda wymagająca nazwy, która tej
 * nazwy nie pokazuje, jest zagadką — lista powstaje z plików na dysku, więc nie ma jej jak zgadnąć.
 *
 * PRZESTAJEMY PODPOWIADAĆ, GDY NAZWA JEST JUŻ WYBRANA. Drugie spacja po `/run` znaczy, że człowiek
 * pisze ZADANIE, a lista workflow wisząca pod zdaniem „build me a todo list" jest szumem — i, co
 * gorsza, sugerowałaby, że Tab dalej coś uzupełni.
 *
 * 2026-08-20 — KTÓRA LISTA UZUPEŁNIA KTÓRĄ KOMENDĘ, MÓWI [`KNOWN`]. Do tego dnia stał tu
 * warunek `first === '/run'`, czyli nazwa komendy wpisana drugi raz, obok tej samej nazwy w
 * [`KNOWN`]. Przy dwóch komendach z argumentem (`/run` bierze workflow, `/ask` bierze agenta)
 * ten warunek musiałby rosnąć razem z listą — a rósłby OSOBNO od niej, więc komenda dopisana
 * do [`KNOWN`] podpowiadałaby się jako słowo i milczała o swoim argumencie.
 *
 * @param workflows nazwy do podpowiedzenia po `/run` (`run-command.ts`, `workflowNames`). Domyślnie
 *   puste, bo ten wiersz nie czyta dysku sam: katalog jest pytaniem do adaptera, a komponent, który
 *   zadaje je sam, jest drugim miejscem, w którym mieszka odpowiedź „jakie workflow istnieją".
 * @param agents nazwy do podpowiedzenia po `/ask` (`ask-command.ts`, `agentNames`). Domyślnie
 *   puste, z tego samego powodu — a osobno od `workflows`, bo zlanie ich w jedną listę
 *   podpowiadałoby workflow tam, gdzie wiersz przyjmuje wyłącznie agenta, i odwrotnie.
 */
export function suggestions(
  typed: string,
  workflows: readonly Named[] = [],
  agents: readonly Named[] = [],
): readonly Named[] {
  const line = typed.trimStart();
  if (!line.startsWith('/')) return [];

  const space = line.indexOf(' ');
  if (space === -1) {
    const first = line.toLowerCase();
    return KNOWN.filter((one) => one.name.startsWith(first));
  }

  const first = line.slice(0, space).toLowerCase();
  const completes = KNOWN.find((one) => one.name === first)?.completes ?? null;
  if (completes !== null) {
    const partial = line.slice(space + 1).trimStart();
    if (partial.includes(' ')) return [];
    const named = completes === 'agents' ? agents : workflows;
    return named.filter((one) => one.name.startsWith(partial.toLowerCase()));
  }
  /* Pierwsze słowo jest już całe, więc podpowiedź jest dokładnie jedna albo żadna: prefiks
   * przestaje mieć sens, kiedy po komendzie stoi jej argument. */
  return KNOWN.filter((one) => one.name === first);
}

export interface EntryProps {
  /**
   * Co powiedzieć agentowi, który pracuje. Oddaje zdanie odmowy albo `null`, kiedy doszło.
   *
   * 2026-08-18 — DO TEGO DNIA TEJ DROGI NIE BYŁO WCALE, i to jest zgłoszenie właściciela
   * („dalej nie działa pisanie do agenta przez terminal"). Wiersz odpowiadał na każde zdanie
   * „That one is not known here" — czyli obiecywał, że rozumie tylko dwie komendy, i miał rację,
   * bo do żywej sesji nie prowadziło nic. Powód leżał w sterowniku, nie tutaj
   * (`engine::drivers::Voice`), a ten props jest ostatnim ogniwem.
   *
   * Zdanie WRACA, zamiast być rzucane: odmowa Rusta jest już napisana po ludzku, a ten wiersz
   * ma ją pokazać w miejscu, w którym człowiek właśnie pisał.
   */
  readonly onSayToAgent: (text: string) => Promise<string | null>;
  /**
   * `/run <workflow> <co zbudować>` — oddaje zdanie odmowy albo `null`, kiedy bieg ruszył.
   *
   * 2026-08-19 — DO TEGO DNIA TEJ KOMENDY NIE BYŁO, i to jest zgłoszenie właściciela: „ten
   * terminal nie ma sensu teraz xD, no bo jak ja mam np puścić jakieś workflow i przekazać
   * prompta?". Nagłówek tego pliku argumentował, że `/run` wejdzie „w dniu, w którym limit ma
   * jedno miejsce" — i ten dzień był już wtedy: limit mieszka w `./limits/chosen` od 2026-08-18,
   * czyli w module, z którego czyta go suwak obok Startu. Argument przestał być prawdziwy przed
   * tym, jak ktoś go przeczytał drugi raz.
   *
   * Dostaje CAŁĄ resztę linii, nie rozebrane słowa: rozbiór („które słowo jest nazwą workflow,
   * a co jest zadaniem") jest polityką i mieszka w `../run-command.ts` razem z odmowami, żeby
   * dało się go osądzić bez okna. Ten wiersz przewozi tekst i pokazuje odpowiedź.
   */
  readonly onRunWorkflow: (rest: string) => Promise<string | null>;
  /**
   * Kto właśnie pracuje — nazwy kroków, którym dojdzie zdanie bez ukośnika.
   *
   * PO CO TO JEST. Rozstrzygnięcie właściciela 2026-08-19: „powinienem wiedzieć co piszę".
   * Wiersz przyjmował prozę i nie mówił, gdzie ona idzie — a idzie w dwa zupełnie różne miejsca
   * w zależności od tego, czy coś biegnie. Pole, które wygląda identycznie, gdy zdanie dojdzie do
   * agenta, i gdy nie ma go komu doręczyć, zmusza człowieka do wysłania zdania, żeby się
   * dowiedzieć, co się z nim stanie.
   *
   * Nazwy, nie liczba: przy dwóch pracujących agentach trzeba WPISAĆ nazwę na początku linii,
   * więc wiersz musi ją pokazać w postaci do przepisania (to samo robi odmowa po stronie Rusta,
   * `RunError::SeveralAreWorking`).
   */
  readonly talkingTo?: readonly string[];
  /**
   * Nazwy workflow do podpowiedzenia po `/run` — puste, dopóki katalog się czyta.
   *
   * Propsem, nie własnym odczytem: „jakie workflow istnieją" jest pytaniem do adaptera
   * (`sections/workflows/io.ts`), a komponent, który zadaje je sam, jest drugim miejscem, w którym
   * mieszka ta odpowiedź (niezmiennik 13). Wartość domyślna jest MOSTEM dla cudzych kryteriów,
   * które montują ten wiersz bez tego propsa.
   */
  readonly workflows?: readonly Named[];
  /** Wymagany: wybór folderu, czyli dołożenie zakresu — ten sam handler, co pod zaproszeniem. */
  readonly onOpenFolder: () => void;
  /**
   * Zatrzymanie biegu, albo `null`, kiedy nic nie biegnie.
   *
   * `null`, a nie osobne pole `running`: „czy jest co zatrzymywać" i „czym to zatrzymać" to
   * jeden fakt, a dwa pola obok siebie dają stan, w którym mówią co innego.
   */
  readonly onStopRun: (() => void) | null;
}

export function Entry({
  onOpenFolder,
  onStopRun,
  onSayToAgent,
  onRunWorkflow,
  talkingTo = [],
  workflows = [],
}: EntryProps): ReactElement {
  const [typed, setTyped] = useState('');
  /* Co pasuje do tego, co już stoi w polu. Liczone przy renderze, nie trzymane w stanie: druga
   * kopia tej odpowiedzi mogłaby opisywać tekst sprzed jednego znaku. */
  const matching = suggestions(typed, workflows);
  /** Ostatnia odpowiedź wiersza; `null`, dopóki nie ma o czym mówić. */
  const [said, setSaid] = useState<string | null>(null);

  function send(event: FormEvent<HTMLFormElement>): void {
    /* Bez tego przeglądarka przeładowuje stronę i bieg znika razem z nią — okno Tauri nie ma
     * dokąd nawigować, a magazyny żyją na poziomie modułu. */
    event.preventDefault();
    if (typed.trim() === '') return;

    const command = understand(typed);
    setTyped('');

    if (command === '/run') {
      setSaid(null);
      /* RESZTA LINII PO NAZWIE KOMENDY, przycięta — i to jest jedyna rzecz, którą ten wiersz
       * z niej wyciąga. Podział na „nazwa workflow" i „zadanie" należy do `../run-command.ts`,
       * bo to polityka i ma być sądzona bez okna (to repo nie ma jsdom, więc Enter jest
       * nieosiągalny dla kryterium). */
      void onRunWorkflow(typed.trim().slice('/run'.length).trim()).then(setSaid);
      return;
    }
    if (command === '/open') {
      setSaid(null);
      onOpenFolder();
      return;
    }
    if (command === '/stop') {
      if (onStopRun === null) {
        setSaid(NOTHING_RUNS);
        return;
      }
      setSaid(null);
      onStopRun();
      return;
    }
    /* PROZA NIE ODBIJA SIĘ OD WIERSZA.
     *
     * Warunek jest na UKOŚNIKU, nie na „czy to zdanie": słowo z ukośnikiem, którego nie znamy,
     * jest literówką w komendzie i ma dostać listę komend — a zdanie bez ukośnika jest tym,
     * co człowiek chce powiedzieć. Wysłanie literówki jako prozy zamieniłoby `/stpo`
     * w wiadomość do modelu i wyglądałoby jak zignorowana komenda. */
    if (typed.trim().startsWith('/')) {
      setSaid(NOT_KNOWN);
      return;
    }
    setSaid(null);
    /* PROZA JEST ROZMOWĄ I NIGDY NIE URUCHAMIA BIEGU — rozstrzygnięcie właściciela 2026-08-19,
     * po tym, jak zobaczył skutek wersji przeciwnej: „nie powinno być tak, że jak piszę bez
     * komendy, a poprzednio odpaliłem komendę, to się ona na nowo całe workflow odpala".
     *
     * Stała tu wersja, która przy pustym ekranie startowała wybrany workflow z tym zdaniem jako
     * zadaniem. Wyglądało to jak wygoda i było pułapką: to samo naciśnięcie Enter raz dopowiadało
     * coś agentowi, a raz kupowało cały bieg sześciu agentów — a różnicy nie było widać w polu,
     * w które człowiek pisał. Rozróżnienie „rozmowa czy praca" nie ma prawa zależeć od stanu,
     * którego w tym miejscu nie widać; niesie je UKOŚNIK i tylko on.
     *
     * Sztywny przebieg zaczyna więc wyłącznie komenda (`/run`), a zdanie bez ukośnika idzie tam,
     * gdzie człowiek je adresował. Kiedy nie ma komu — Rust odmawia zdaniem, które nazywa następny
     * ruch („Press Start first."), a wiersz mówi to samo POD polem, jeszcze przed Enterem
     * (`talkingTo`). */
    void onSayToAgent(typed).then(setSaid);
  }

  return (
    <form
      data-entry
      onSubmit={send}
      className="border-t border-line-strong px-[18px] pt-[10px] pb-3"
    >
      <div className="grid h-10 grid-cols-[26px_1fr_auto] items-center border border-line-strong border-l-2 border-l-accent bg-well">
        {/* Znak zachęty z makiety. `aria-hidden`, bo dla czytnika ekranu to jest ozdoba. */}
        <span aria-hidden className="text-center font-mono text-accent">
          ❯
        </span>
        <input
          aria-label="Command line"
          placeholder={PROMPT}
          spellCheck={false}
          value={typed}
          onChange={(event) => {
            setTyped(event.target.value);
          }}
          onKeyDown={(event) => {
            /* TAB UZUPEŁNIA, kiedy pasuje DOKŁADNIE jedna komenda. Przy dwóch nie zgadujemy:
             * uzupełnienie do pierwszej z listy wpisuje za człowieka decyzję, której nie podjął,
             * a lista pod polem i tak już mu ją pokazuje. `preventDefault` tylko wtedy, gdy
             * naprawdę uzupełniamy — inaczej Tab przestałby wychodzić z pola klawiaturą. */
            if (event.key !== 'Tab' || matching.length !== 1) return;
            const only = matching[0];
            if (only === undefined) return;
            /* UZUPEŁNIAMY OSTATNIE SŁOWO, nie całą linię. Do 2026-08-19 Tab wstawiał `only.name`
             * w miejsce wszystkiego — co było poprawne, dopóki podpowiedzią była wyłącznie nazwa
             * komendy. Odkąd po `/run ` podpowiadają się nazwy workflow, tamta wersja zamieniłaby
             * `/run to` na `todo-list`, czyli zjadłaby komendę i zostawiła prozę. */
            const line = typed.trimStart();
            const cut = line.lastIndexOf(' ');
            const done = (cut === -1 ? '' : line.slice(0, cut + 1)) + only.name + ' ';
            if (done === typed) return;
            event.preventDefault();
            setTyped(done);
          }}
          className="h-[38px] border-0 bg-transparent font-mono text-mono text-ink outline-0"
        />
        <kbd className="mr-[9px] border border-line px-[5px] py-[2px] font-mono text-label text-muted">
          ENTER
        </kbd>
      </div>

      {/* PODPOWIEDZI, DOPÓKI LINIA ZACZYNA SIĘ OD UKOŚNIKA — powód w całości przy `suggestions`.
          Stoją POD polem, a nie w nim: pole niesie to, co człowiek napisał, i nic poza tym.

          Zero elementów, kiedy nie ma o czym podpowiadać: pusty wiersz „brak podpowiedzi" jest
          tym samym rodzajem szumu, co nagłówek nad pustką. */}
      {typed.trim().startsWith('/') ? (
        <div data-entry-suggestions className="mt-[6px] ml-[26px] grid gap-[2px]">
          {matching.length === 0 ? (
            /* „NIE ZNAM TEGO" TYLKO WTEDY, GDY NAPRAWDĘ NIE ZNAM KOMENDY.
             *
             * 2026-08-19 — do tego dnia ten warunek stał na samej pustce listy, a odkąd po `/run `
             * podpowiadają się nazwy workflow, pustka znaczy też „ta nazwa do niczego nie pasuje".
             * Zdanie „This line takes /run, /open and /stop" powiedziane człowiekowi, który pisze
             * `/run zzz`, jest odpowiedzią na pytanie, którego nie zadał — komendę zna, nie zna
             * nazwy. Wtedy milczymy tutaj, a pełną odmowę z listą nazw daje Enter
             * (`run-command.ts`, `noSuchWorkflow`), bo to ona ma miejsce na wypisanie nazw. */
            understand(typed) === null ? (
              <p className="text-body text-attend">{NOT_KNOWN}</p>
            ) : (
              <p className="font-mono text-label text-muted">{HINT}</p>
            )
          ) : (
            matching.map((one) => (
              <p key={one.name} className="grid grid-cols-[72px_1fr] items-baseline gap-2">
                {/* Nazwa komendy jest wartością maszynową — mono, do przepisania znak w znak
                    (DESIGN §4). To, co robi, jest zdaniem po ludzku, więc Inter. */}
                <span className="font-mono text-mono text-accent">{one.name}</span>
                <span className="text-body text-muted">{one.does}</span>
              </p>
            ))
          )}
        </div>
      ) : (
        /* GDZIE POJDZIE TO ZDANIE — pod polem, PRZED naciśnięciem Enter.
           Rozstrzygnięcie właściciela 2026-08-19: „powinienem wiedzieć co piszę". */
        <p data-entry-hint className="mt-[6px] ml-[26px] font-mono text-label text-muted">
          {whereItGoes(talkingTo)}
        </p>
      )}

      {said === null ? null : (
        <p data-entry-said className="mt-[6px] ml-[26px] text-body text-attend">
          {said}
        </p>
      )}
    </form>
  );
}
