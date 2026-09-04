/* Z-36: wiadomość wpisana człowiekowi w chwili, w której lider stoi w długiej komendzie, mówi
 * NA EKRANIE, na co czeka — i przestaje to mówić, kiedy komenda się domknie (niezmiennik 29).
 *
 * Zmierzone 2026-09-04: lider siedział siedem minut w jednym wywołaniu Basha, człowiek wysłał
 * w tym czasie wiadomość, `turns/0003.json` zapisał ją jako `delivered` — i ekran nie powiedział
 * ani słowa o tym, że ona czeka. Zgłoszenie brzmiało „lider się zawiesza i nie odpisuje", a po
 * paru minutach „coś tam się odpaliło" — to był koniec tamtego Basha.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(feed.view.queued).toBe(zdanie)`. Przechodzi na modelu,
 * którego nikt nie rysuje — czyli dokładnie na tej klasie wady, dla której to repo powstało:
 * kryterium zielone, funkcja martwa. Zdanie jest tu więc szukane w PRAWDZIWYM markupie kolumny
 * strumienia, tą samą drogą, co odmowa startu w `./borrowed-text-refusal-is-visible.test.tsx`.
 *
 * TRZY CHWILE, NIE JEDNA. Ekran przed wiadomością (zdania nie ma — inaczej wszystko niżej
 * przechodziłoby na ekranie, który mówi to zawsze), ekran z wiadomością stojącą za komendą
 * (zdanie jest, słowo w słowo) i ekran po domknięciu komendy (zdania nie ma, bo lider właśnie
 * rusza). Bez pierwszej i trzeciej to kryterium sądzi obecność napisu, a nie zachowanie
 * (niezmiennik 20).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

/* Granica, która nic nie robi: ten ekran ma się narysować, a nie pogadać z Rustem. Ten sam
 * zabieg, co w każdym cudzym kryterium montującym `<Run />`. */
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve(undefined)),
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const Run = (await import('./index')).default;
const { runFeed } = await import('./feed/live');
const { line } = await import('./feed/fixtures/lines');
const { useWorkspaces } = await import('../../state/workspaces');
const { useRun } = await import('../../state/run');

/** Zakres, w którym pracujemy. `id === folder` — kontrakt granicy z 2026-08-18. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };

/** Podpis, pod którym mówi lider. */
const LEAD = 'Lead';

/** Komenda, w której lider siedział siedem i pół minuty. */
const COMMAND = 'until grep -q Murmur <(ps aux); do sleep 5; done';

/** Ile już trwała, kiedy człowiek napisał. */
const ELAPSED_MS = 450_000;

/** Zdanie człowieka — jedzie do lidera i staje w kolejce za tą komendą. */
const TOLD = 'also add a dark mode toggle';

/** Zdanie, którego szukamy na ekranie, słowo w słowo. */
const QUEUED = 'Queued — the lead is still running ' + COMMAND + ' (7m 30s)';

/** Krok, którego kafelek ma powiedzieć to samo, co wiersz strumienia. */
const STEP = 'Build';
const STEP_ID = 's_build';

/** Komenda tego kroku i jej czas — przykład wprost ze zlecenia Z-36. */
const CHECKS = 'Run full cargo test --lib';
const CHECKS_FOR = 240_000;

/** Markup jednego kafelka: od jego znacznika do znacznika następnego. */
function tileOf(markup: string, id: string): string {
  const opens = markup.indexOf('data-step="' + id + '"');
  if (opens < 0) return '';
  const rest = markup.slice(opens);
  const next = rest.indexOf('data-step="', 1);
  return readable(next < 0 ? rest : rest.slice(0, next));
}

useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });

/** Markup tak, jak czyta go człowiek: React zapisuje cudzysłowy i `&` jako encje. */
function readable(markup: string): string {
  return markup
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
}

/** Sama kolumna strumienia, wycięta z ekranu — reszta ekranu nie ma prawa tu odpowiadać. */
function streamOf(markup: string): string {
  const opens = markup.indexOf('data-stream-column');
  if (opens < 0) return '';
  const rest = markup.slice(opens);
  const closes = rest.indexOf('data-plan-column', 1);
  return readable(closes < 0 ? rest : rest.slice(0, closes));
}

/** Ekran w tej chwili — model widoku żyje na poziomie modułu, więc kolejność jest treścią. */
function screen(): string {
  return streamOf(renderToStaticMarkup(<Run />));
}

/* ── Chwila 1: lider stoi w komendzie, człowiek jeszcze nic nie napisał ─────────────────── */
runFeed.appendLines([line.running(1, 0, LEAD, COMMAND, ELAPSED_MS)]);
const working = screen();

/* ── Chwila 2: człowiek pisze, a tura lidera trwa dalej ─────────────────────────────────── */
runFeed.appendLines([line.told(2, 100, LEAD, TOLD)]);
const waiting = screen();

/* ── Chwila 3: komenda się domyka — to samo wywołanie, więc ten sam wiersz ──────────────── */
runFeed.appendLines([
  line.ran(3, 200, LEAD, 'Ran ' + COMMAND + ' — ok · 7m 30s', true, [], 'call-1'),
]);
const moving = screen();

/* ── Chwila 4: to samo zdanie na KAFELKU kroku, który tę komendę uruchomił ──────────────── */
useRun.setState({
  workflow: 'Fix the CSV parser',
  steps: [{ id: STEP_ID, name: STEP, state: 'running' }],
  links: null,
});
runFeed.appendLines([line.running(4, 300, STEP, CHECKS, CHECKS_FOR)]);
const card = tileOf(renderToStaticMarkup(<Run />), STEP_ID);

describe('a message delivered while the lead is still running something says so', () => {
  it('renders a stream at all, so the comparisons below are about a screen', () => {
    expect(
      working,
      'the run screen rendered no stream column, so every assertion here would run against an ' +
        'empty string and pass on nothing.',
    ).not.toBe('');
    expect(
      working,
      'the row for the command in flight is not on screen either, so this file cannot tell a ' +
        'screen that answers a queued message from one that shows nothing at all. It rendered: ' +
        working.slice(0, 400),
    ).toContain(COMMAND);
  });

  it('says nothing about waiting before anybody wrote', () => {
    expect(
      working,
      'the sentence stood on the screen before the person said anything, so nothing below could ' +
        'tell a screen that answers a queued message from one that says it always.',
    ).not.toContain('Queued —');
  });

  it('leaves that sentence in the stream, word for word', () => {
    expect(
      waiting,
      'a person wrote to the lead while it had been sitting in one command for seven and a half ' +
        'minutes. The message is delivered and it is not being read, and the screen says nothing ' +
        'about either — which reads exactly like a lead that hung. The sentence that had to be ' +
        'there: ' +
        QUEUED,
    ).toContain(QUEUED);
  });

  it('takes it back the moment the command closes and the turn can move', () => {
    expect(
      moving,
      'the command finished, so nothing is holding that message any more and the sentence about ' +
        'waiting is now about a wait that is over. A control or a line describing work nobody is ' +
        'doing is the defect this screen has already been fixed for four times.',
    ).not.toContain('Queued —');
    expect(
      moving,
      'and the command still has its row: closing the wait is not the same as forgetting what ' +
        'ran. It rendered: ' +
        moving.slice(0, 400),
    ).toContain('Ran ' + COMMAND + ' — ok');
    expect(
      moving,
      'ONE ROW, NOT TWO. The row that opened when the command started and the row that closes ' +
        'it are the same carrier, so the closing sentence REPLACES the one about work in ' +
        'flight. Both of them standing here is a transcript that grows a line every thirty ' +
        'seconds — the wall of text this view exists to remove.',
    ).not.toContain('Working: ' + COMMAND);
  });

  it('says the same thing on the card of the step that is running it', () => {
    expect(
      card,
      'the run screen drew no card for the step at all, so the assertion below would be about ' +
        'an empty string rather than about a card.',
    ).not.toBe('');
    expect(
      card,
      'the card of a step that has been sitting in one command for four minutes says nothing ' +
        'about it — which is exactly the screen a person reads as an agent that hung. The ' +
        'sentence is the SAME one the row carries, composed once in Rust where the curation ' +
        'lives (invariant 15); a second table of wordings on this side would drift from it at ' +
        'the first change. It rendered: ' +
        card.slice(0, 400),
    ).toContain('Working: ' + CHECKS + ' · 4m');
  });
});
