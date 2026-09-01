/* Odpowiedź agenta narysowana tak, jak ją napisał — nagłówki, pogrubienia, kod, punkty.
 *
 * # Skarga
 *
 * Właściciel, 2026-08-30, ze zrzutu biegu `20260830-191440`: „nie podoba mi się ta ściana tekstu,
 * ciężko się to czyta". Modele piszą markdownem — `## Answer`, `**bold**`, backticki, listy —
 * a widok pokazywał te znaki dosłownie, więc struktura, którą agent nadał swojej odpowiedzi,
 * była na ekranie szumem zamiast pomocą.
 *
 * # Dlaczego TOKENY, a nie HTML
 *
 * `marked` umie oddać gotowy HTML jednym wywołaniem i **nie korzystamy z tego ani razu**.
 * `src/ui/shell/permissions.test.ts` mówi o tym wprost: „one flaw in a markdown renderer — and
 * this app renders agent-written markdown — turns a shell permission into arbitrary code running
 * on the machine". Tekst, który tu przychodzi, napisał model, a model bywa przekonany przez
 * treść pliku, który przeczytał.
 *
 * Ten moduł bierze `marked.lexer()`, czyli **drzewo tokenów**, i składa z niego elementy Reacta.
 * `dangerouslySetInnerHTML` nie pada tu ani razu i nie ma prawa paść: React wstawia każdy napis
 * jako TEKST, więc `<script>` z odpowiedzi agenta jest na ekranie napisem `<script>`, a nie
 * skryptem. Kryterium obok sprawdza dokładnie to.
 *
 * # Dlaczego jeden pakiet, a nie renderer dla Reacta
 *
 * `react-markdown` robi to samo bezpiecznie i kosztuje **79 pakietów** — ośmiokrotność całego
 * drzewa zależności tej aplikacji. `marked` kosztuje jeden, a parser i tak jest cudzy: obsługę
 * przypadków brzegowych markdowna pisali ludzie, którzy robią to zawodowo, i to jest cała
 * wartość biblioteki. Zamiana tokenów na elementy to trzydzieści linii i one należą do nas.
 *
 * # Czego ten renderer NIE robi
 *
 * Nie robi z odnośnika rzeczy do kliknięcia. Adres jest widoczny jako tekst, więc człowiek go
 * czyta, ale okno nie otwiera niczego, o czym zdecydował model. Obrazków nie ma z tego samego
 * powodu — pobranie obrazka jest żądaniem sieciowym pod adres, który wybrał ktoś inny.
 */
import type { ReactElement, ReactNode } from 'react';
import { marked } from 'marked';

/** Token markdowna. Kształt `marked`, opisany tu tylko w częściach, których dotykamy. */
interface Token {
  readonly type: string;
  readonly text?: string;
  readonly raw?: string;
  readonly depth?: number;
  readonly ordered?: boolean;
  readonly tokens?: readonly Token[];
  readonly items?: readonly Token[];
}

/* PIGUŁKA, NIE SAM KRÓJ. 2026-08-31, DOPISANE ADDYTYWNIE (nic nie zniknęło, `font-mono` i stopień
 * stoją dalej): makieta strumienia rysuje ścieżkę i nazwę w prozie agenta na TLE
 * (`polecenie.html`, `.msg p code` i `.filechip` — wypełnienie pola, obrys włoskowaty, promień),
 * a bez niego `main`, `src/db` i `migrate.rs` różnią się od zdania wokół wyłącznie krojem — czyli
 * w wierszu, który trzeba przeczytać, nie różnią się prawie niczym.
 *
 * TŁO NEUTRALNE, NIE AKCENT, i to jest jedyne miejsce, w którym odchodzimy od makiety: DESIGN §3
 * zamyka akcent na jednym znaczeniu („to jest interaktywne"), a nazwa pliku w zdaniu nie jest
 * kontrolką. Makieta ma tu obie wersje — `.msg p code` w akcencie i `.filechip` w barwie pola —
 * i bierzemy tę drugą. */
const CODE = 'rounded-sm border border-line bg-well px-[5px] py-px font-mono text-mono text-ink';
const BLOCK =
  'overflow-x-auto rounded-sm bg-panel px-[9px] py-[7px] font-mono text-mono text-muted';

/**
 * Nagłówek. Stopnie zwijamy do dwóch, bo tekst w wierszu strumienia ma jedną miarę i sześć
 * rozmiarów zrobiłoby z odpowiedzi drabinkę — a `##` i `###` z jednej odpowiedzi mają się
 * różnić rolą, nie wielkością.
 */
function heading(depth: number): string {
  return depth <= 2
    ? 'mt-2 block font-mono text-mono-strong text-ink'
    : 'mt-1 block font-mono text-mono text-ink';
}

/** Tokeny wewnątrz akapitu: pogrubienie, kursywa, kod, odnośnik, tekst. */
function inline(tokens: readonly Token[] | undefined, text: string | undefined): ReactNode {
  if (tokens === undefined) return text ?? '';
  return tokens.map((token, at) => {
    const key = `${token.type}-${at}`;
    switch (token.type) {
      case 'strong':
        return (
          <strong key={key} className="text-ink">
            {inline(token.tokens, token.text)}
          </strong>
        );
      case 'em':
        return <em key={key}>{inline(token.tokens, token.text)}</em>;
      case 'codespan':
        return (
          <code key={key} className={CODE}>
            {token.text ?? ''}
          </code>
        );
      case 'br':
        return <br key={key} />;
      /* ODNOŚNIK JEST TEKSTEM. Adres widać, więc człowiek go przeczyta; okno nie otwiera niczego,
         o czym zdecydował model. */
      case 'link':
      case 'text':
      case 'escape':
        return <span key={key}>{token.text ?? ''}</span>;
      default:
        /* Nieznany rodzaj oddaje swój surowy tekst, nigdy nic. Token, którego nie znamy, jest
           w tej odpowiedzi treścią, a nie usterką (niezmiennik 5). */
        return <span key={key}>{token.raw ?? token.text ?? ''}</span>;
    }
  });
}

/** Tokeny najwyższego poziomu: nagłówki, akapity, listy, bloki kodu. */
function block(tokens: readonly Token[]): ReactNode {
  return tokens.map((token, at) => {
    const key = `${token.type}-${at}`;
    switch (token.type) {
      case 'heading':
        return (
          <span key={key} className={heading(token.depth ?? 1)}>
            {inline(token.tokens, token.text)}
          </span>
        );
      case 'paragraph':
        return (
          <span key={key} className="block">
            {inline(token.tokens, token.text)}
          </span>
        );
      case 'code':
        return (
          <pre key={key} className={BLOCK}>
            {token.text ?? ''}
          </pre>
        );
      case 'list':
        return (
          <span key={key} className="block">
            {(token.items ?? []).map((item, index) => (
              <span key={`${key}-${index}`} className="block pl-3 -indent-3">
                {token.ordered === true ? `${index + 1}. ` : '· '}
                {inline(item.tokens, item.text)}
              </span>
            ))}
          </span>
        );
      case 'blockquote':
        return (
          <span key={key} className="block border-l-2 border-line pl-2 text-muted">
            {block(token.tokens ?? [])}
          </span>
        );
      case 'space':
        return <span key={key} className="block h-2" />;
      case 'hr':
        return <span key={key} className="my-2 block border-t border-line" />;
      default:
        return (
          <span key={key} className="block">
            {token.raw ?? token.text ?? ''}
          </span>
        );
    }
  });
}

/**
 * Ten sam markdown, ale **w jednym wierszu** — nagłówek wiersza strumienia.
 *
 * Zmierzone 2026-08-31 na prawdziwych odpowiedziach z bazy właściciela: **cztery z sześciu**
 * pierwszych wierszy niosą markdown (`## Backend: nic do zrobienia`, `**Backend nie jest
 * potrzebny**`, backticki wokół nazw narzędzi). Nagłówek rysowany surowo pokazywałby te znaki
 * dosłownie — czyli dokładnie ten szum, dla którego usunięcia ten moduł powstał, tylko w wierszu,
 * który człowiek widzi ZAWSZE, a nie dopiero po rozwinięciu.
 *
 * `Lexer.lexInline`, nie `lexer`: nagłówek jest jednym wierszem i ma nim zostać. Blokowy parser
 * zrobiłby z `## Backend` element blokowy, a ten rozpycha wiersz strumienia w pionie.
 */
export function AnswerLine({ text }: AnswerProps): ReactElement {
  /* NAGŁÓWEK ZDJĘTY Z PRZODU, nie zostawiony jako tekst: `##` w jednej linii z resztą zdania
     czyta się jak literówka. Sam tekst nagłówka zostaje — to on niesie treść. */
  const flat = text.replace(/^#{1,6}\s+/, '');
  const tokens = marked.Lexer.lexInline(flat) as unknown as readonly Token[];
  return <span>{inline(tokens, flat)}</span>;
}

export interface AnswerProps {
  /** Co agent napisał, znak w znak. */
  text: string;
}

/**
 * Odpowiedź agenta jako elementy, nigdy jako HTML.
 *
 * Oddaje `<div>`, a nie fragment, bo wołający nadaje mu miarę wiersza i odstępy — a element bez
 * własnego pojemnika dziedziczyłby je od czegoś, czego ten moduł nie zna.
 */
export function Answer({ text }: AnswerProps): ReactElement {
  /* `lexer`, nie `parse`: to pierwsze oddaje tokeny, drugie napis HTML. Cała różnica między tym
     modułem a dziurą w oknie z dostępem do powłoki mieści się w tym jednym słowie. */
  const tokens = marked.lexer(text) as unknown as readonly Token[];
  return <div className="whitespace-normal break-words">{block(tokens)}</div>;
}
