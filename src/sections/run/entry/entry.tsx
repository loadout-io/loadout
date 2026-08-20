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
import type { FormEvent, ReactElement, Ref } from 'react';
import { useRef, useState } from 'react';

import { startAskFromLine } from '../ask-command';
import { startFromLine } from '../rail/processes';
import type { Named } from '../run-command';
import type { WindowLine } from './echo';
import { echoOf, saidOf } from './echo';
import { createHistory } from './history';

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
    /* 2026-08-20 — POWSTAŁO Z ZAMÓWIENIA WŁAŚCICIELA: „odpalać nasze workflows/agents".
     * Workflow miał drogę z tego wiersza, agent nie miał żadnej — bo jednostką pracy jest
     * PLIK. Jeden agent z jednym zdaniem kosztował wejście do edytora, założenie workflow,
     * postawienie jednego kafelka, zapisanie go i powrót. Za najczęstszą czynność dnia.
     *
     * ZARAZ PO `/run`, bo to są dwie komendy, które zaczynają pracę, i człowiek szukający
     * „jak to uruchomić" ma je zobaczyć obok siebie. `/open` i `/stop` są pomocnicze. */
    name: '/ask',
    tail: 'an agent',
    does: 'Start one agent. Add what it should do after its name.',
    completes: 'agents' as Completes,
  },
  {
    /* 2026-08-20 — POWSTAŁO Z ZGŁOSZENIA WŁAŚCICIELA: „jak napiszę aby coś odpalił jakąś apkę to
     * chcę mieć też po prawej gdzie są agenci info o procesach odpalonych itp". Rzecz odpalona
     * przez AGENTA stoi w jego grupie i Loadout widzi po niej wyłącznie wiersz `ran` — czynność
     * ZAKOŃCZONĄ. Nośnika na „to biegnie teraz" nie było, więc kafelka nie było z czego zbudować
     * (niezmiennik 17), a „stop" pod nim nie miałby czego ubić (niezmiennik 6). Stąd trzecia
     * komenda, która zaczyna pracę: rzecz zamawia się TUTAJ, a właścicielem jest Loadout.
     *
     * ZARAZ PO `/run` i `/ask`, bo te trzy zaczynają pracę i mają stać obok siebie. `/open`
     * i `/stop` są pomocnicze.
     *
     * BEZ PODPOWIADANIA (`completes: null`): po `/start` stoi wiersz powłoki, a listy komend
     * powłoki nie ma czym przeczytać — podpowiadanie jej byłoby zgadywaniem, dokładnie jak
     * ścieżka przy `/open`. */
    name: '/start',
    tail: 'a command',
    does: 'Start a command and keep it running. It shows up in the agents list.',
    completes: null as Completes,
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
  'Enter sends it. Start with a slash for a command, or just write to the lead agent.';

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
 * 2026-08-20 — ADRESATEM JEST ZAWSZE LIDER, a agent wyłącznie po nazwie, więc te trzy zdania
 * mówią teraz co innego niż dzień wcześniej. Zgłoszenie właściciela: „proza w trakcie biegu
 * znika z rozmowy z liderem, bo leci do pracującego agenta" — do 2026-08-20 jeden pracujący
 * krok przechwytywał KAŻDE zdanie, czyli lider milczał przez cały bieg, dokładnie wtedy, kiedy
 * człowiek chce zapytać, co się właściwie dzieje. Polityka mieszka w `../addressee.ts` i jest
 * sądzona bez okna; te zdania są jej UPRZEDZENIEM, a nie drugą kopią (niezmiennik 13).
 *
 * Trzy stany, trzy zdania, i każde nazywa następny ruch (DESIGN §8): żaden pracujący — zdanie
 * mówi, czym się zaczyna pracę; jeden — mówimy, czyją nazwą się go dosięga; kilku — wypisujemy
 * nazwy, bo jedną trzeba WPISAĆ na początku linii.
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
    /* NAZWA JEST ADRESEM, nie informacją o tym, kto pracuje. Do 2026-08-20 stało tu „Enter
     * sends this to Forge" i było prawdą o implementacji, którą to zadanie zamyka: jeden
     * pracujący krok przechwytywał całą prozę. Teraz zdanie musi nieść oba fakty naraz —
     * gdzie zdanie POJDZIE i czym się dosięga kogoś innego — bo pierwszy bez drugiego zostawia
     * człowieka bez drogi do agenta, którego widzi na ekranie. */
    return 'Enter sends this to the lead agent. Start the line with ' + only + ' to reach it.';
  }
  /* WYPISUJEMY NAZWY, bo przy kilku pracujących trzeba jedną WPISAĆ na początku linii — dokładnie
   * tak, jak każe odmowa `RunError::SeveralAreWorking`. Sama liczba („2 agents are working")
   * mówiłaby, że jest problem, i nie mówiłaby, jak go rozwiązać. */
  return (
    'Enter sends this to the lead agent. ' +
    String(working.length) +
    ' agents are working, so start the line with a name to reach one: ' +
    working.join(', ') +
    '.'
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
   * Zdanie bez ukośnika. Oddaje zdanie odmowy albo `null`, kiedy doszło.
   *
   * NAZWA PROPSA JEST STARSZA NIŻ POLITYKA, KTÓRĄ OPISUJE, i zostaje: od 2026-08-20 zdanie idzie
   * do LIDERA, a do pracującego kroku wyłącznie wtedy, gdy człowiek nazwał go na początku linii
   * (`../addressee.ts`). Adresata wybiera ekran, nie ten wiersz — tu jest jedna droga na całą
   * prozę i tak ma zostać, bo wiersz, który sam decyduje, komu ją oddać, jest drugim miejscem
   * z tą polityką (niezmiennik 23). Przepisanie nazwy zmieniłoby cudze kryteria, które montują
   * ten wiersz i podają ten props (`caret.test.tsx`, `suggests-workflows.test.ts`).
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
   * `/ask <agent> <zadanie>` — oddaje zdanie odmowy albo `null`, kiedy agent ruszył.
   *
   * 2026-08-20 — DO TEGO DNIA TEJ DROGI NIE BYŁO, i to jest zamówienie właściciela: „odpalać
   * nasze workflows/agents". Workflow miał drogę z tego wiersza, agent nie miał żadnej — bo
   * jednostką pracy jest PLIK, więc jeden agent z jednym zdaniem kosztował wejście do edytora,
   * założenie workflow, postawienie jednego kafelka i powrót. Za najczęstszą czynność dnia.
   *
   * WARTOŚĆ DOMYŚLNA JEST TĄ PRODUKCYJNĄ, a nie mostem dla kryteriów, i to jest różnica wobec
   * `onRunWorkflow`. Tamtą politykę podaje ekran pracy (`../index.tsx`), bo `/run` powstało
   * razem z nim; `/ask` nie ma tam ani jednego wiersza, którego mógłby dotknąć to zadanie
   * (jego blok OWNS nie obejmuje tamtego pliku), a komenda stojąca w zachęcie i odpowiadająca
   * „nie znam tego" jest obietnicą w napisie (niezmiennik 16). Domyślna wartość wskazuje więc
   * TĘ SAMĄ politykę, którą podałby ekran — `../ask-command.ts`, obok rozbioru linii — i nie
   * jest to `io.ts` wołane z komponentu: nazwa komendy dalej istnieje w sekcji raz.
   */
  readonly onAskAgent?: (rest: string) => Promise<string | null>;
  /**
   * `/start <komenda>` — oddaje zdanie odmowy albo `null`, kiedy rzecz wstała.
   *
   * 2026-08-20 — POWSTAŁO Z ZGŁOSZENIA WŁAŚCICIELA („info o procesach odpalonych… po kliku mogę
   * tam wejść"); powód, dla którego tej drogi nie było, stoi przy wpisie `/start` w [`KNOWN`].
   *
   * WARTOŚĆ DOMYŚLNA JEST TĄ PRODUKCYJNĄ, dokładnie jak przy [`EntryProps::onAskAgent`] i z tego
   * samego powodu: ekran pracy (`../index.tsx`) nie należy do zadania, które tę komendę dołożyło,
   * a komenda stojąca w zachęcie i odpowiadająca „nie znam tego" jest obietnicą w napisie
   * (niezmiennik 16). Domyślna wskazuje więc TĘ SAMĄ politykę, którą podałby ekran —
   * `../rail/processes.ts`, obok magazynu, z którego lista bierze kafelki — a nie `io.ts` wołane
   * z komponentu: nazwa komendy dalej istnieje w sekcji raz.
   *
   * Zdanie WRACA, zamiast być rzucane, jak przy każdej innej komendzie tego wiersza: odmowa Rusta
   * jest już napisana po ludzku i ma się pokazać tam, gdzie człowiek właśnie pisał.
   */
  readonly onStartCommand?: (rest: string) => Promise<string | null>;
  /**
   * Kto właśnie pracuje — czyli czyją nazwą wolno zaadresować zdanie bez ukośnika.
   *
   * 2026-08-20 — NAZWY SĄ ADRESAMI, nie listą odbiorców. Do tego dnia niepustość tej listy
   * wystarczała, żeby zdanie poszło do agenta; teraz idzie do lidera, dopóki któraś z tych nazw
   * nie stanie na początku linii (`../addressee.ts`).
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
  /**
   * Nazwy agentów do podpowiedzenia po `/ask` — puste, dopóki biblioteka się czyta.
   *
   * Propsem, nie własnym odczytem, z tego samego powodu, co [`EntryProps::workflows`]: „jacy
   * agenci istnieją" jest pytaniem do adaptera (`sections/agents/io.ts`), a komponent, który
   * zadaje je sam, jest drugim miejscem, w którym mieszka ta odpowiedź (niezmiennik 13).
   *
   * ZGŁOSZENIE, NIE PRZEOCZENIE: produkcyjnym wołającym tego wiersza jest `../index.tsx`, a ten
   * plik nie należy do T-62 (`AGENTS.md` §7), więc dopóki człowiek nie dopisze tam jednej linii
   * (`agents={agentNames(...)}`), Tab po `/ask ` nie uzupełni nazwy. Sama komenda działa: Enter
   * na nieznanej nazwie odmawia zdaniem, które WYMIENIA istniejące nazwy (`../ask-command.ts`),
   * więc lista jest osiągalna, tylko o jedno naciśnięcie dalej.
   */
  readonly agents?: readonly Named[];
  /** Wymagany: wybór folderu, czyli dołożenie zakresu — ten sam handler, co pod zaproszeniem. */
  readonly onOpenFolder: () => void;
  /**
   * Zatrzymanie biegu, albo `null`, kiedy nic nie biegnie.
   *
   * `null`, a nie osobne pole `running`: „czy jest co zatrzymywać" i „czym to zatrzymać" to
   * jeden fakt, a dwa pola obok siebie dają stan, w którym mówią co innego.
   */
  readonly onStopRun: (() => void) | null;
  /**
   * Wiersz, który to pole właśnie złożyło — do dopisania w strumieniu.
   *
   * PO CO TO ISTNIEJE. Zgłoszenie właściciela 2026-08-20: komendy nie zostawiają po sobie ani
   * jednego wiersza. Terminal, w którym wpisana komenda nie zostawia śladu, jest nieodróżnialny
   * od terminala, który tej komendy nie przyjął — a `/run`, `/open`, `/stop` i odpowiedzi samego
   * wiersza są dla drutu niewidzialne, więc ich jedynym śladem jest to, co dopisze okno.
   *
   * WIERSZ SKŁADA TEN PLIK, a DOKĄD on idzie wie ekran, i ten podział jest celowy: kształt
   * wiersza jest sprawą wiersza wejścia (`./echo.ts` leży obok tego pliku), a „która sesja
   * strumienia" jest sprawą ekranu, bo to on wie, w jakim zakresie stoimy (`../feed/live.ts`).
   * Ten wiersz wołający `feedFor` sam byłby drugim miejscem, w którym mieszka ta odpowiedź.
   *
   * Wartość domyślna jest MOSTEM dla cudzych kryteriów, które montują ten wiersz bez tego propsa
   * (`caret.test.tsx`, `suggests-workflows.test.ts`) — tak samo jak przy [`talkingTo`]. Że
   * produkcyjny wołający go PODAJE, dowodzi `e2e/tests/terminal-behaves.spec.ts`: pyta o wiersz
   * w strumieniu prawdziwej przeglądarki, więc zapomniany props jest tam czerwony.
   */
  readonly onShowInStream?: (row: WindowLine) => void;
  /**
   * Uchwyt do pola, żeby ktoś z zewnątrz mógł mu ODDAĆ kursor.
   *
   * Jedynym wołającym jest dziś kolumna strumienia (`../index.tsx`): kliknięcie w miejsce bez
   * kontrolki wraca kursorem tutaj. Uchwytem, a nie szukaniem po etykiecie w dokumencie —
   * `aria-label="Command line"` przepisane w drugim pliku rozjechałoby się przy pierwszej
   * zmianie brzmienia, a wtedy kursor przestałby wracać i nikt by nie wiedział dlaczego.
   */
  readonly fieldRef?: Ref<HTMLInputElement>;
}

export function Entry({
  onOpenFolder,
  onStopRun,
  onSayToAgent,
  onRunWorkflow,
  onAskAgent = startAskFromLine,
  onStartCommand = startFromLine,
  talkingTo = [],
  workflows = [],
  onShowInStream = () => undefined,
  fieldRef,
  agents = [],
}: EntryProps): ReactElement {
  const [typed, setTyped] = useState('');
  /* Co pasuje do tego, co już stoi w polu. Liczone przy renderze, nie trzymane w stanie: druga
   * kopia tej odpowiedzi mogłaby opisywać tekst sprzed jednego znaku. */
  const matching = suggestions(typed, workflows, agents);

  /* HISTORIA CHODZENIA — po tym, co JUŻ wysłano z tego pola.
   *
   * `useRef`, bo to nie jest stan renderu: chodzenie po historii zmienia wyłącznie zawartość
   * pola, a tę trzyma już `typed`. Trzymana w `useState` kazałaby przerysować wiersz przy każdym
   * `remember`, czyli przy każdym Enterze, i to bez ani jednej zmiany na ekranie.
   *
   * `useRef` trzyma PIERWSZĄ z tych historii i wyrzuca każdą następną — bez tego strzałka gubiłaby
   * wszystko przy pierwszym przerysowaniu wiersza. Że fabryka biegnie mimo to przy każdym
   * renderze, jest tu świadomie niezoptymalizowane: to trzy domknięcia i tablica, a wersja
   * z leniwą inicjalizacją (`useRef<History | null>(null)`) dokłada w każdym użyciu gałąź
   * „a jeśli jeszcze nie ma", której nie da się przejść. */
  const walk = useRef(createHistory());

  /**
   * To, co odpowiedział ten wiersz, do strumienia — albo nic, kiedy odpowiedzi nie ma.
   *
   * `null` znaczy „doszło", i tak właśnie mówią oba propsy (`onRunWorkflow`, `onSayToAgent`):
   * cisza po udanej komendzie jest poprawna, bo skutek widać w biegu.
   */
  function showTheAnswer(answer: string | null): void {
    if (answer === null) return;
    onShowInStream(saidOf(answer));
  }

  function send(event: FormEvent<HTMLFormElement>): void {
    /* Bez tego przeglądarka przeładowuje stronę i bieg znika razem z nią — okno Tauri nie ma
     * dokąd nawigować, a magazyny żyją na poziomie modułu. */
    event.preventDefault();
    const line = typed.trim();
    if (line === '') return;

    const command = understand(typed);
    setTyped('');
    /* STRZAŁKA MA PO CO SIĘGAĆ — zapamiętujemy KAŻDĄ wysłaną linię, także tę, która odbije się
     * od wiersza: literówka w komendzie jest dokładnie tą linią, którą człowiek chce poprawić,
     * a nie przepisywać z pamięci. */
    walk.current.remember(line);

    /* ŚLAD PO KOMENDZIE, ZANIM COKOLWIEK POJEDZIE DALEJ.
     *
     * Kolejność jest tu odwrotna niż po stronie Rusta, i to nie jest niekonsekwencja. Tam wiersz
     * `Told` powstaje PO wysłaniu tury, bo twierdzi, że agent ją usłyszał (`commands::run`), i
     * dopisany wcześniej kłamałby o cudzym stanie. Tutaj wiersz twierdzi tylko, że TA LINIA
     * ZOSTAŁA WPISANA — a to jest prawdą w chwili Enteru, niezależnie od tego, czym skończy się
     * komenda. Echo po fakcie stawiałoby odmowę PRZED linią, której dotyczy.
     *
     * Proza wraca tu `null` i to jest cała treść `echoOf`: jej wiersz przyjeżdża z drutu jako
     * `told`, a dwa wiersze o jednym zdaniu to dwa miejsca prawdy (niezmiennik 13). */
    const echo = echoOf(line);
    if (echo !== null) onShowInStream(echo);

    if (command === '/run') {
      /* RESZTA LINII PO NAZWIE KOMENDY, przycięta — i to jest jedyna rzecz, którą ten wiersz
       * z niej wyciąga. Podział na „nazwa workflow" i „zadanie" należy do `../run-command.ts`,
       * bo to polityka i ma być sądzona bez okna (to repo nie ma jsdom, więc Enter jest
       * nieosiągalny dla kryterium). */
      void onRunWorkflow(line.slice('/run'.length).trim()).then(showTheAnswer);
      return;
    }
    if (command === '/ask') {
      /* RESZTA LINII PO NAZWIE KOMENDY, przycięta tylko po końcach — i to jest ta sama umowa,
       * co przy `/run`. Podział na „nazwa agenta" i „zadanie" należy do `../ask-command.ts`,
       * razem z odmowami: zdanie dla agenta jedzie stamtąd CO DO ZNAKU, więc wiersz nie ma
       * prawa go po drodze przepisać. */
      void onAskAgent(typed.trim().slice('/ask'.length).trim()).then(showTheAnswer);
      return;
    }
    if (command === '/start') {
      /* CAŁA RESZTA LINII, przycięta tylko po końcach — i to jest ta sama umowa, co przy `/run`
       * i `/ask`, tylko waży tu więcej: dalej jedzie WIERSZ POWŁOKI. Wiersz, który skleiłby
       * wielokrotne spacje albo tknął cudzysłowy, zmieniłby komendę, którą człowiek napisał,
       * i uruchomił coś innego niż to, co ma na ekranie. Rozbiór na „nazwę" i „argumenty" nie
       * istnieje z rozmysłu: to powłoka rozbiera tę linię, nie my (`SHELL` w `command.rs`). */
      void onStartCommand(typed.trim().slice('/start'.length).trim()).then(showTheAnswer);
      return;
    }
    if (command === '/open') {
      onOpenFolder();
      return;
    }
    if (command === '/stop') {
      if (onStopRun === null) {
        showTheAnswer(NOTHING_RUNS);
        return;
      }
      onStopRun();
      return;
    }
    /* PROZA NIE ODBIJA SIĘ OD WIERSZA.
     *
     * Warunek jest na UKOŚNIKU, nie na „czy to zdanie": słowo z ukośnikiem, którego nie znamy,
     * jest literówką w komendzie i ma dostać listę komend — a zdanie bez ukośnika jest tym,
     * co człowiek chce powiedzieć. Wysłanie literówki jako prozy zamieniłoby `/stpo`
     * w wiadomość do modelu i wyglądałoby jak zignorowana komenda. */
    if (line.startsWith('/')) {
      showTheAnswer(NOT_KNOWN);
      return;
    }
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
    void onSayToAgent(typed).then(showTheAnswer);
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
          ref={fieldRef}
          aria-label="Command line"
          placeholder={PROMPT}
          spellCheck={false}
          /* KURSOR STOI TU OD PIERWSZEJ SEKUNDY — zgłoszenie właściciela 2026-08-20: „kursor
             nie stoi w polu, trzeba kliknąć, za każdym razem". Niezmiennik 16 mówi o kontrolce
             bez handlera, a to jest jej odmiana: pole, które nazywa się terminalem i wymaga
             jednego kliknięcia, zanim przyjmie znak, każe płacić to kliknięcie przy KAŻDYM
             wejściu na ekran pracy — a człowiek patrzy już na pole ze znakiem zachęty przed nim.

             DOKŁADNIE JEDEN ELEMENT W TYM WIERSZU o to prosi, i to jest cała reszta reguły:
             przeglądarka daje ognisko jednemu z proszących i nie mówi któremu, więc dwa
             `autoFocus` to nie „dwa razy uprzejmiej", tylko wiersz, którego zachowanie zależy
             od kolejności markupu. */
          autoFocus
          value={typed}
          onChange={(event) => {
            setTyped(event.target.value);
          }}
          onKeyDown={(event) => {
            /* STRZAŁKI CHODZĄ PO HISTORII, i to jest druga z czterech wad z 2026-08-20:
             * „strzalka w gore nie cofa do poprzedniej linii". Cała polityka chodzenia — łącznie
             * z tym, że krok naprzód oddaje SZKIC, a nie puste pole — mieszka w `./history.ts`,
             * bo to repo nie ma jsdom i naciśnięcia klawisza nie da się odpalić w kryterium.
             * Tutaj zostaje przewiezienie napisu do pola.
             *
             * `null` znaczy „nie ma czego oddać" i wtedy NIE ROBIMY NIC — ani nie czyścimy pola,
             * ani nie zabieramy strzałce jej zwykłej roboty. Bez tego warunku strzałka w górę
             * w polu z pustą historią przestałaby przesuwać kursor na początek linii, czyli
             * odebrałaby zachowanie, którego nikt jej nie kazał zmieniać.
             *
             * `preventDefault` wyłącznie wtedy, gdy naprawdę wstawiamy linię: w jednoliniowym
             * polu strzałka w górę skacze kursorem na początek, a po wstawieniu cudzej linii
             * kursor ma stać na jej KOŃCU — tam, gdzie się dopisuje. */
            if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
              const stepped =
                event.key === 'ArrowUp' ? walk.current.back(typed) : walk.current.forward();
              if (stepped === null) return;
              event.preventDefault();
              setTyped(stepped);
              return;
            }
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
           Rozstrzygnięcie właściciela 2026-08-19: „powinienem wiedzieć co piszę".

           TEN WIERSZ ZOSTAJE, choć odpowiedzi wiersza przeniosły się do strumienia: mówi o innym
           fakcie i w innej chwili. „Gdzie to pójdzie" jest zdaniem PRZED Enterem i dotyczy tekstu,
           który jeszcze stoi w polu; „co Loadout odpowiedział" jest zdaniem PO Enterze i dotyczy
           linii, która już poszła. Jeden region na jeden fakt (niezmiennik 13). */
        <p data-entry-hint className="mt-[6px] ml-[26px] font-mono text-label text-muted">
          {whereItGoes(talkingTo)}
        </p>
      )}

      {/* CZEGO TU JUŻ NIE MA: `data-entry-said`, czyli ostatniej odpowiedzi wiersza pod polem.
          Zgłoszenie właściciela 2026-08-20 dotyczyło śladu po komendach, a to miejsce było jego
          połowiczną wersją: pokazywało JEDNO zdanie, ostatnie, i gasło przy następnej linii.
          Trzy odpowiedzi z rzędu zostawiały dwie niewidziane, a rozmowa z Loadoutem czytała się
          jak dwie różne historie — jedna w strumieniu, druga pod polem (niezmiennik 13).
          Wszystkie te zdania wchodzą teraz do strumienia przez `onShowInStream` (`./echo.ts`). */}
    </form>
  );
}
