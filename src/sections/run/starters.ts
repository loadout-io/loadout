/* TRZEJ AGENCI, KTÓRZY SĄ JUŻ NAPISANI — i przycisk, który NAPRAWDĘ ich zapisuje.
 *
 * ZMIERZONE 2026-08-31. Od pustej instalacji do pierwszego biegu było osiem do jedenastu
 * ruchów, a pierwszy z nich — „napisz agenta" — jest jednocześnie najtrudniejszy: człowiek,
 * który nie widział jeszcze ani jednego agenta, ma wymyślić rolę, instrukcję, model, limit
 * i dial dostępu do plików, nie wiedząc, co którekolwiek z nich robi. Ekran pierwszego otwarcia
 * skraca to do JEDNEGO kliknięcia i zostawia całą resztę otwartą: każde pole zapisanego agenta
 * da się potem zmienić w sekcji Agents, bo to jest zwykły plik, a nie szablon systemowy.
 *
 * ATRAPA JEST TU WADĄ, NIE UPROSZCZENIEM. Przycisk „Use this agent", który tylko przesuwa
 * ekran, obiecuje agenta i nie zostawia go na dysku — a człowiek dowiaduje się o tym dopiero
 * w Agents, gdzie jest pusto. Dlatego ten moduł jedzie do TEJ SAMEJ krawędzi, którą zapisuje
 * formularz (`../agents/io.ts`, `save`), i do tej samej mennicy identyfikatorów
 * (`new_id` po stronie Rusta, uuid v7 sortowalne po czasie [T4 §5.1]). Druga droga do dysku
 * znaczyłaby, że asercje o zapisie agenta pilnują jednej z dwóch (niezmiennik 23).
 *
 * DLACZEGO NIE PRZEZ MAGAZYN SEKCJI AGENTS. Bo `createAgentsStore(DISK)` jest w tamtym pliku
 * zamknięty w module i nie wychodzi na zewnątrz, a drugi egzemplarz magazynu to drugi zbiór
 * rewizji plików — czyli dokładnie ten stan, w którym jedno okno nadpisuje pracę drugiego
 * i wygląda przy tym na udane. Zapisujemy więc PLIK, a nie wiersz listy: liczba agentów, którą
 * widzi droga, idzie osobno przez `./whats-ready.ts`, a sekcja Agents przeczyta katalog przy
 * najbliższym wejściu — tak samo jak po zapisie z każdego innego miejsca.
 *
 * TRZECH, NIE DWUNASTU. Galeria szablonów, która ma dwanaście pozycji, zadaje to samo pytanie
 * co pusty formularz, tylko z dłuższą listą. Trzej pokrywają trzy jedyne rzeczy, jakie agent
 * może robić z cudzym kodem — czytać go, zmieniać go, uruchamiać go — i to jest cała treść
 * tego wyboru. Czwarta karta w galerii nie jest agentem: prowadzi do formularza.
 */
import { why } from '../../ipc/why';
import type { Agent } from '../../state/agents';
import { VENDORS } from '../agents/agent-form';
import { capability } from '../agents/capabilities';
import { newId, save } from '../agents/io';
import { oneMoreAgentIsSaved } from './whats-ready';

/** Wszystko, co ten moduł robi poza swoją głową. Jedna atrapa w kryterium zastępuje całość. */
export interface StarterIo {
  /** uuid v7, wybite po stronie Rusta. */
  newId(): Promise<string>;
  /** `expectedRevision` to rewizja pliku, którą znamy; `null` znaczy „tego pliku ma nie być". */
  save(agent: Agent, expectedRevision: string | null): Promise<string>;
}

export interface Starter {
  /** Definicja, którą ten przycisk zapisze — kompletna, bez ani jednego pola do wypełnienia. */
  readonly agent: Agent;
  /**
   * Czynność przycisku „Use this agent".
   *
   * TA SAMA FUNKCJA, którą woła kryterium. Bez jsdom nie da się kliknąć, więc jedyny sposób,
   * żeby wyrocznia sądziła to, co robi kontrolka, to podać kontrolce dokładnie ten uchwyt.
   */
  readonly take: () => Promise<boolean>;
}

const DISK: StarterIo = { newId, save };

/* Granica, do której piszą przyciski. Podmienialna WYŁĄCZNIE przez `starterWritesTo`, bo
 * kryterium ma zobaczyć, co naprawdę dojechało do zapisu, a nie uwierzyć, że coś dojechało. */
let disk: StarterIo = DISK;

/**
 * Wyłącznie dla kryteriów: podstawia granicę dysku i oddaje funkcję przywracającą prawdziwą.
 *
 * Oddaje przywracacz, a nie zostawia sprzątanie wołającemu: atrapa, która przeżyła swój plik,
 * jest granicą, przez którą reszta zestawu pisze w ciszy.
 */
export function starterWritesTo(io: StarterIo): () => void {
  disk = io;
  return () => {
    disk = DISK;
  };
}

/* ── trzej agenci ───────────────────────────────────────────────────────────────────────────
 *
 * Każde pole jest DECYZJĄ, nie wypełniaczem, i każde da się potem zmienić. `id` jest puste, bo
 * identyfikator wybija mennica przy zapisie — ta sama umowa, co w `blankAgent`
 * (`../agents/index.tsx`). `reachesTheWeb` zostaje przy wartości domyślnej produktu
 * (rozstrzygnięcie właściciela z 2026-08-23): dial sieci nie daje ani jednego czasownika
 * plikowego, więc nie zmienia tego, co któremuś z nich wolno zrobić z kodem.
 */

/** Wspólne pola trójki — wszystko, czego ten wybór NIE dotyczy. */
const SHARED = {
  schema: 1,
  id: '',
  thinking: 'balanced',
  giveUpAfterMinutes: 30,
  reachesTheWeb: true,
  skills: [],
  connections: [],
  writeResultsTo: '',
} as const satisfies Partial<Agent>;

/* CZYTA. `look-only` plus lista czasowników bez powłoki — dwa pokrętła mówiące to samo, bo Claude
 * Code umie zawęzić OBA, a agent, który tylko czyta, ma nie mieć czym uruchomić niczego. */
const SCOUT: Agent = {
  ...SHARED,
  name: 'Scout',
  summary: 'Reads the code before anyone changes it.',
  color: 'slate',
  instructions:
    'Read first and change nothing. Say where the failure starts, name the file and quote the ' +
    'lines that matter.',
  runsWith: 'claude-code',
  model: 'sonnet',
  fileAccess: 'look-only',
  tools: { only: ['Read', 'Grep', 'Glob'] },
};

/* ZMIENIA. Jedyny z trójki, któremu wolno dotknąć plików — i dlatego jedyny, którego karta
 * mówi o tym wprost. */
const BUILDER: Agent = {
  ...SHARED,
  name: 'Builder',
  summary: 'Writes the change, and only the change.',
  color: 'plum',
  instructions:
    'Make the change that was asked for and nothing else. Leave the rest of the file exactly ' +
    'as you found it, and say in one line what you changed.',
  runsWith: 'claude-code',
  model: 'opus',
  fileAccess: 'work-freely',
  tools: 'everything',
};

/* URUCHAMIA. Codex nie umie zawęzić listy czasowników (`capability('tools', 'codex')` mówi
 * `unavailable`), więc powłokę ma z natury — a `look-only` jest tym, co go od plików trzyma. */
const NEEDLE: Agent = {
  ...SHARED,
  name: 'Needle',
  summary: 'Runs the checks and reports what broke.',
  color: 'clay',
  instructions:
    'Run the checks this project already has. Report what broke in the words the checks used, ' +
    'and change nothing.',
  runsWith: 'codex',
  model: 'gpt-5.6-sol',
  fileAccess: 'look-only',
  tools: 'everything',
};

export const STARTERS: readonly Starter[] = [SCOUT, BUILDER, NEEDLE].map((agent) => ({
  agent,
  take: () => takeStarter(agent),
}));

/**
 * Co temu agentowi WOLNO — trzy słowa, policzone z jego własnych pól.
 *
 * NIE NAPIS OBOK DEFINICJI (niezmiennik 17). Karta mówiąca „reads only" nad agentem, któremu
 * wolno zmieniać pliki, jest zdaniem, którego dane nie niosą — a to jest dokładnie ten rodzaj
 * ozdoby, przez który człowiek daje agentowi prawa, o których nie wie. Zmiana diala w definicji
 * wyżej zmienia napis na karcie, bo napis jest z niego wyliczony.
 */
export function whatItMayDo(agent: Agent): string {
  if (agent.fileAccess !== 'look-only') return 'edits files';
  return mayRunCommands(agent) ? 'runs commands' : 'reads only';
}

/**
 * Czy ten agent w ogóle sięga po powłokę.
 *
 * Dwie odpowiedzi, bo dwie aplikacje agentowe mówią o tym zupełnie różnie. U Claude'a lista
 * czasowników jest prawdziwa i zawężalna, więc rozstrzyga jej treść. Codex listy nie ma wcale
 * (`capability('tools', …)` oddaje dla niego `unavailable`), więc zawężenie jest tam obietnicą,
 * której nikt nie dotrzyma — i uczciwą odpowiedzią jest „tak, uruchamia".
 */
function mayRunCommands(agent: Agent): boolean {
  if (capability('tools', agent.runsWith) === 'unavailable') return true;
  return agent.tools === 'everything' || agent.tools.only.includes('Bash');
}

/**
 * Czym ten agent biegnie, w brzmieniu, które człowiek widzi w formularzu.
 *
 * ETYKIETA Z `VENDORS`, nie nazwa z drutu: `claude-code` nie ma prawa dojechać na ekran
 * (niezmiennik 14), a druga kopia brzmienia rozjeżdża się przy pierwszej zmianie i obie
 * wyglądają wtedy poprawnie.
 */
export function runsOn(agent: Agent): string {
  const says = VENDORS.find((one) => one.value === agent.runsWith)?.label ?? '';
  return says === '' ? agent.model : says + ' · ' + agent.model;
}

/* ── co się właśnie stało z galerią ─────────────────────────────────────────────────────────
 *
 * Trzy fakty i ani jednego więcej. Na poziomie modułu z tego samego powodu, co `./guidance.ts`:
 * `renderToStaticMarkup` nie uruchamia efektów, a `src/App.tsx` trzyma w drzewie jedną sekcję,
 * więc stan zamknięty w komponencie byłby jednocześnie niesprawdzalny i gubiony.
 */
export interface TakingAnAgent {
  /** Nazwa agenta, który właśnie jedzie na dysk — `null`, kiedy nic nie jedzie. */
  readonly busy: string | null;
  /** Nazwa tego, który właśnie wylądował — `null`, kiedy nic nie wylądowało. */
  readonly landed: string | null;
  /** Zdanie po odmowie z dysku, słowo w słowo od Rusta — `null`, kiedy nie ma o czym mówić. */
  readonly said: string | null;
}

const NOTHING_TAKEN: TakingAnAgent = { busy: null, landed: null, said: null };

let taking: TakingAnAgent = NOTHING_TAKEN;

const listening = new Set<() => void>();

function tell(next: TakingAnAgent): void {
  taking = next;
  for (const one of listening) one();
}

/** Migawka — ta sama dla okna i dla renderu statycznego. */
export function takingAnAgent(): TakingAnAgent {
  return taking;
}

export function subscribeToTaking(onChange: () => void): () => void {
  listening.add(onChange);
  return () => {
    listening.delete(onChange);
  };
}

/** Wyłącznie dla kryteriów: przywraca stan sprzed pierwszego kliknięcia w galerię. */
export function forgetStarters(): void {
  tell(NOTHING_TAKEN);
}

/**
 * Zapisuje tego agenta na dysk. `true`, kiedy plik naprawdę powstał.
 *
 * ODMOWA MA GŁOS. `save` po tamtej stronie granicy odmawia, kiedy pod tą nazwą leży już cudzy
 * plik — a to jest przypadek, w który człowiek wchodzi drugim kliknięciem w tę samą kartę.
 * Cisza po takim kliknięciu wygląda dokładnie jak kontrolka martwa (niezmiennik 16), więc
 * zdanie z Rusta ląduje w `said` i ekran je pokazuje.
 *
 * KOLEJNOŚĆ JEST TREŚCIĄ: dysk pierwszy, licznik drogi drugi. W odwrotnej droga przesuwałaby
 * się o agenta, którego dysk nie przyjął (niezmiennik 4).
 */
export async function takeStarter(agent: Agent): Promise<boolean> {
  tell({ busy: agent.name, landed: null, said: null });
  try {
    const id = await disk.newId();
    await disk.save({ ...agent, id }, null);
    oneMoreAgentIsSaved();
    tell({ busy: null, landed: agent.name, said: null });
    return true;
  } catch (error) {
    tell({
      busy: null,
      landed: null,
      said: why(error, 'Loadout could not save ' + agent.name + ' to your agents folder.'),
    });
    return false;
  }
}
