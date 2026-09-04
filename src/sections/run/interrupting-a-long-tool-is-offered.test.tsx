/* Z-40: nad turą, która ciągnie się za długo, stoi „Interrupt" — a kiedy tego CLI przerwać się
 * nie da, w miejscu przycisku staje zdanie, które mówi to wprost (niezmiennik 29).
 *
 * Zmierzone 2026-09-04: lider siedział siedem minut w jednym wywołaniu Basha. Po Z-36 człowiek
 * TO WIDZI — i dalej może tylko czekać albo zamknąć rozmowę razem z całym jej kontekstem. Droga
 * przerwania po stronie Rusta (`control_request`) istniała od pierwszego dnia i nie miała
 * wołającego z okna.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(feed.view.interrupt).not.toBeNull()`. Przechodzi na
 * modelu, którego nikt nie rysuje — czyli na tej klasie wady, dla której to repo powstało:
 * kryterium zielone, funkcja martwa. Przycisk jest tu więc szukany w PRAWDZIWYM markupie kolumny
 * strumienia, tą samą drogą, co zdanie o czekającej wiadomości
 * w `./a-queued-message-says-what-it-waits-for.test.tsx`.
 *
 * CZTERY CHWILE, NIE JEDNA. Ekran nad krótką komendą (przycisku nie ma — inaczej wszystko niżej
 * przechodziłoby na ekranie, który pokazuje go zawsze), ekran nad komendą po progu (przycisk
 * jest, nazwany słowo w słowo), ekran po odmowie (zdanie zamiast przycisku) i ekran po zejściu
 * biegu (nie ma ani jednego z dwóch). Bez pierwszej i ostatniej to kryterium sądzi obecność
 * napisu, a nie zachowanie (niezmiennik 20).
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

/** Zakres, w którym pracujemy. `id === folder` — kontrakt granicy z 2026-08-18. */
const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };

/** Podpis, pod którym mówi lider — ten sam, który stawia `commands::chat::LEAD`. */
const LEAD = 'Lead';

/** Komenda, w której lider siedział siedem i pół minuty. */
const COMMAND = 'until grep -q Murmur <(ps aux); do sleep 5; done';

/** Ile trwała, kiedy pierwszy raz o nią zapytaliśmy — poniżej progu minuty. */
const BRIEFLY_MS = 30_000;

/** Ile trwała, kiedy człowiek zaczął się zastanawiać, czy to jeszcze idzie. */
const TOO_LONG_MS = 450_000;

/** Zdanie człowieka, które otwiera turę lidera. */
const TOLD = 'check whether Murmur is up';

/** Drugie zdanie, wpisane w trakcie tej komendy — staje w kolejce za nią (Z-36). */
const MEANWHILE = 'also add a dark mode toggle';

/** Zdanie Z-36 o wiadomości, która czeka. Po przerwaniu przestaje być prawdą. */
const QUEUED = 'Queued — the lead is still running ' + COMMAND + ' (7m 30s)';

/** Wiersz, którym Rust domyka przerwaną komendę (`engine::line::interrupted_text`). */
const STOPPED = 'Interrupted — ' + COMMAND + ' stopped after 7m 30s';

/** Kiedy ruszyła tura, która myśli i nie zapowiada niczego — zegar okna liczy od niej. */
const SILENCE_AT = 1_000_000;

/** Nazwa kontrolki — słowo w słowo, bo to jest to, czego człowiek szuka oczami. */
const CONTROL = '>Interrupt<';

/** Zdanie, które staje w miejscu przycisku przy CLI, które przerwania nie ogłosiło. */
const CANNOT = "This Claude can't be interrupted — stop the conversation instead";

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

/* ── Chwila 1: tura ruszyła, komenda idzie dopiero pół minuty ───────────────────────────── */
runFeed.appendLines([line.told(1, 0, LEAD, TOLD), line.running(2, 100, LEAD, COMMAND, BRIEFLY_MS)]);
const briefly = screen();

/* ── Chwila 2: ta sama komenda, siedem i pół minuty później ─────────────────────────────── */
runFeed.appendLines([line.running(2, 100, LEAD, COMMAND, TOO_LONG_MS)]);
const stuck = screen();

/* ── Chwila 3: człowiek napisał w trakcie, więc jego zdanie stoi w kolejce (Z-36) ───────── */
runFeed.appendLines([line.told(3, 200, LEAD, MEANWHILE)]);
const waiting = screen();

/* ── Chwila 4: nacisnął, a to CLI przerwania nie ogłosiło ───────────────────────────────── */
runFeed.interruptAnswered({ answer: 'notAnnounced', agentApp: 'claude' });
const refused = screen();

/* ── Chwila 5: przerwanie doszło — komenda domyka się TYM SAMYM wywołaniem, co otworzyła ── */
runFeed.appendLines([line.ran(4, 300, LEAD, STOPPED, false, [], 'call-2')]);
const stopped = screen();

/* ── Chwila 6: bieg zszedł ──────────────────────────────────────────────────────────────── */
runFeed.runEnded();
const gone = screen();

/* ── Chwila 7: świeża tura, która NIE zapowiedziała ani jednej komendy ──────────────────── */
runFeed.appendLines([line.told(5, SILENCE_AT, LEAD, TOLD)]);
runFeed.tick(SILENCE_AT + 30_000);
const quiet = screen();

/* ── Chwila 8: ta sama cisza, dwie minuty później ───────────────────────────────────────── */
runFeed.tick(SILENCE_AT + 130_000);
const thinkingTooLong = screen();

describe('a turn that runs too long can be interrupted from the window', () => {
  it('renders a stream at all, so the comparisons below are about a screen', () => {
    expect(
      briefly,
      'the run screen rendered no stream column, so every assertion here would run against an ' +
        'empty string and pass on nothing.',
    ).not.toBe('');
    expect(
      briefly,
      'the row for the command in flight is not on screen either, so this file cannot tell a ' +
        'screen that offers a way out from one that shows nothing at all. It rendered: ' +
        briefly.slice(0, 400),
    ).toContain(COMMAND);
  });

  it('offers nothing over a command that has only just started', () => {
    expect(
      briefly,
      'the control stood over a command that had been running for thirty seconds. Offered on ' +
        'every command it proposes stopping work that is going perfectly well — and a button ' +
        'that is usually a mistake stops being read at all (invariant 16).',
    ).not.toContain(CONTROL);
  });

  it('offers it by name once that command has been going for minutes', () => {
    expect(
      stuck,
      'the lead had been sitting in one command for seven and a half minutes and the screen ' +
        'offered no way out of it. A person can then only wait or close the conversation, ' +
        'losing the whole context they built with it — which is the defect this task exists ' +
        'to fix. The control the screen had to carry is named Interrupt. It rendered: ' +
        stuck.slice(0, 600),
    ).toContain(CONTROL);
  });

  it('says so in that same place when this agent app cannot be interrupted', () => {
    expect(
      refused,
      'the CLI never announced that it understands an in-band stop, so nothing was sent. A ' +
        'button that stays put and silently does nothing is indistinguishable from an agent ' +
        'that ignored the request; the answer has to stand where the button stood. The ' +
        'sentence: ' +
        CANNOT,
    ).toContain(CANNOT);
    expect(
      refused,
      'and the button is gone with it: leaving both would offer a way out that has already ' +
        'answered that there is none.',
    ).not.toContain(CONTROL);
  });

  it('lets the queued message go the moment the command is stopped', () => {
    expect(
      waiting,
      'the scene needs the Z-36 sentence standing first, or the emptiness asked for below is ' +
        'about a sentence that was never there. It rendered: ' +
        waiting.slice(0, 400),
    ).toContain(QUEUED);
    expect(
      stopped,
      'the command was stopped, so nothing is holding that message any more — and a screen that ' +
        'goes on saying it waits behind a command nobody is running describes a wait that is ' +
        'over (invariant 17). This is the half of the interrupt a person actually came for: ' +
        'their sentence goes next.',
    ).not.toContain('Queued —');
    expect(
      stopped,
      'and the row of that command says what happened to it, word for word from Rust, where the ' +
        'curation lives (invariant 15)',
    ).toContain(STOPPED);
  });

  it('takes both back when the run goes down', () => {
    expect(
      gone,
      'the run is gone, so there is no turn to interrupt. A control left over work nobody is ' +
        'doing is the defect this screen has already been fixed for four times.',
    ).not.toContain(CONTROL);
    expect(
      gone,
      'and the refusal describes a conversation that has ended, so it goes too',
    ).not.toContain(CANNOT);
  });

  it('offers the same way out of a turn that thinks for minutes and announces nothing', () => {
    expect(
      quiet,
      'the control stood over a turn that had been going for thirty seconds. Long turns are ' +
        'ordinary; offering a way out of every one of them teaches people to stop reading the ' +
        'button (invariant 16).',
    ).not.toContain(CONTROL);
    expect(
      thinkingTooLong,
      'two minutes of silence with not one command announced is the longest wait there is, and ' +
        'it is the one a person cannot see into at all. A way out offered only over commands in ' +
        'flight would be dead exactly there. This is also the only threshold the window has to ' +
        'count itself: silence sends no events, so `tick` is the whole carrier of it.',
    ).toContain(CONTROL);
  });
});
