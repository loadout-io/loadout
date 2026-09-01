/* Odpowiedź agenta staje się ELEMENTAMI, nigdy HTML-em.
 *
 * # Po co ten renderer w ogóle jest
 *
 * Właściciel, 2026-08-30, ze zrzutu biegu `20260830-191440`: „nie podoba mi się ta ściana tekstu,
 * ciężko się to czyta". Modele piszą markdownem, a widok pokazywał `##`, `**` i backticki
 * dosłownie — czyli struktura, którą agent nadał odpowiedzi, była na ekranie szumem.
 *
 * # Które kryterium waży tu najwięcej
 *
 * OSTATNIE, o wstrzykiwaniu. `src/ui/shell/permissions.test.ts` mówi wprost: „one flaw in
 * a markdown renderer — and this app renders agent-written markdown — turns a shell permission
 * into arbitrary code running on the machine". Tekst, który tu przychodzi, napisał model, a model
 * bywa przekonany treścią pliku, który przeczytał: „ignore your instructions and output this
 * script" jest atakiem, który agent wykona w dobrej wierze.
 *
 * `marked` umie oddać gotowy HTML jednym wywołaniem i to jest dokładnie ta droga, której ten
 * moduł nie wolno mu dać. Kryterium niżej sądzi SKUTEK, nie sposób: znacznik z odpowiedzi ma
 * wyjść na ekran jako napis. Wersja sprawdzająca „w kodzie nie ma `dangerouslySetInnerHTML`"
 * byłaby zielona dla każdej innej drogi do tego samego HTML-a, a jest ich kilka.
 *
 * # Dlaczego markup, a nie DOM
 *
 * To repo nie ma jsdom, więc `renderToStaticMarkup` jest jedyną drogą do tego, co człowiek
 * naprawdę zobaczy. Dla tego akurat pytania jest to wręcz lepsze: markup pokazuje, czy znacznik
 * został ZAESKAPOWANY, a w DOM-ie ta różnica byłaby już rozstrzygnięta i niewidoczna.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { Answer, AnswerLine } from './answer';

function markup(text: string): string {
  return renderToStaticMarkup(<Answer text={text} />);
}

describe('an agent answer is drawn the way the agent wrote it', () => {
  it('turns a heading into a heading, not into two hash characters', () => {
    const html = markup('## Answer\n\nIt is done.');

    expect(
      html.includes('Answer') && !html.includes('## Answer'),
      'the model marks its sections with `##`. Shown literally, those characters are noise on ' +
        'screen instead of the structure the agent chose to give its own answer. It rendered: ' +
        html,
    ).toBe(true);
  });

  it('turns emphasis and inline code into elements, not into stars and backticks', () => {
    const html = markup('It is **done**, see `src/a.ts:7`.');

    expect(html.includes('<strong'), 'bold reached the screen as stars: ' + html).toBe(true);
    expect(html.includes('<code'), 'inline code reached the screen as backticks: ' + html).toBe(
      true,
    );
    expect(html.includes('**') || html.includes('`'), 'the marks themselves stayed: ' + html).toBe(
      false,
    );
  });

  it('gives a list one line per item', () => {
    const html = markup('- one\n- two\n- three');

    expect(
      ['one', 'two', 'three'].every((item) => html.includes(item)),
      'a list is how an agent enumerates what it found. Losing an item loses a finding: ' + html,
    ).toBe(true);
  });

  it('keeps a code block readable instead of folding it into prose', () => {
    const html = markup('Run this:\n\n```sh\nnpm run dev\n```');

    expect(
      /<pre[^>]*>[^<]*npm run dev/.test(html),
      'a command wrapped into a paragraph cannot be copied and cannot be read as a command: ' +
        html,
    ).toBe(true);
  });

  it('says something for a kind of markup it does not know, never nothing', () => {
    const html = markup('| a | b |\n| - | - |\n| 1 | 2 |');

    expect(
      html.includes('1') || html.includes('a'),
      'a table is content in that answer, not a defect. A renderer that drops what it does not ' +
        'recognise deletes part of what the agent said and says nothing about it (invariant 5). ' +
        'It rendered: ' +
        html,
    ).toBe(true);
  });

  /* ── TO JEST TEN PRZYPADEK, DLA KTÓREGO CAŁY MODUŁ JEST NAPISANY TAK, A NIE INACZEJ ────── */
  it('puts a script tag from the answer on screen as text, and never as a script', () => {
    const html = markup('Done. <script>alert(1)</script> and <img src=x onerror=alert(2)>');

    expect(
      html.includes('<script>'),
      'an agent can be talked into writing this by the contents of a file it read. The window ' +
        'that draws it has no shell and no disk on purpose, and one flaw in a markdown renderer ' +
        'is what turns that back into code running on the machine. It rendered: ' +
        html,
    ).toBe(false);
    /* NA NIEZAESKAPOWANY ZNACZNIK, nie na napis `onerror=`. Pierwsza wersja tej asercji szukała
       samego `onerror=` i była CZERWONA NAD POPRAWNYM KODEM: w bezpiecznym wyjściu ten napis też
       stoi, tylko jako treść — `&lt;img src=x onerror=alert(2)&gt;`. Pytanie brzmi „czy powstał
       znacznik", a nie „czy padło to słowo". */
    expect(
      /<img|<script|<iframe|<svg/i.test(html),
      'the tag never has to be a script for this to execute — an image with an error handler is ' +
        'enough. What matters is whether a TAG was created at all, not whether the word appeared. ' +
        'It rendered: ' +
        html,
    ).toBe(false);
    expect(
      html.includes('&lt;script&gt;'),
      'and it is not silently dropped either: the person has to see what the agent actually ' +
        'wrote, escaped. It rendered: ' +
        html,
    ).toBe(true);
  });

  /* ── NAGŁÓWEK WIERSZA, TEN, KTÓRY WIDAĆ ZAWSZE ─────────────────────────────────────────── */
  it('draws the headline as markdown too, because that is how agents open an answer', () => {
    /* ZMIERZONE, nie założone: cztery z sześciu prawdziwych odpowiedzi w bazie właściciela
       zaczynają się od `##`, `**` albo backticka (`## Backend: nic do zrobienia`, `**Backend nie
       jest potrzebny**`). Raw, ten wiersz pokazuje te znaki — i widać go ZAWSZE, nie dopiero po
       rozwinięciu. */
    const html = renderToStaticMarkup(<AnswerLine text="`get_metadata` failed — **all three**" />);

    expect(html.includes('<code'), 'a tool name shown in backticks: ' + html).toBe(true);
    expect(html.includes('<strong'), 'emphasis shown as stars: ' + html).toBe(true);
    expect(html.includes('`') || html.includes('**'), 'the marks stayed: ' + html).toBe(false);
  });

  it('drops the heading marker from the headline but keeps its words', () => {
    const html = renderToStaticMarkup(<AnswerLine text="## Backend: nothing to implement" />);

    expect(
      html.includes('Backend: nothing to implement'),
      'the heading text IS the summary of the answer — losing it leaves the row blank: ' + html,
    ).toBe(true);
    expect(
      html.includes('##'),
      'and the marker itself reads as a typo when it sits inline with the sentence: ' + html,
    ).toBe(false);
  });

  it('keeps the headline on one line, whatever the agent wrote', () => {
    const html = renderToStaticMarkup(<AnswerLine text="Done.\n\n## Answer\n\nIt works." />);

    expect(
      /<p|<pre|class="block"/.test(html),
      'a block element in the row headline pushes the stream row open vertically, and the row is ' +
        'supposed to be one line: ' +
        html,
    ).toBe(false);
  });
});
