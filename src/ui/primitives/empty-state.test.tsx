/* Prymityw pustego ekranu: znacznik stoi na zdaniu, a przycisk istnieje wtedy i tylko wtedy,
 * gdy ma co zrobić.
 *
 * SŁABA WERSJA PIERWSZEGO PUNKTU: `expect(markup).toContain(sentence)`. Przechodzi ona dokładnie
 * w tym stanie, który ten plik naprawia — `data-empty` na otaczającym `<div>`, którego treść to
 * „◇ zdanie ＋ Create". Odróżnia je to, że czytamy treść ELEMENTU Z ZNACZNIKIEM i porównujemy ją
 * znak w znak, tak samo jak dwa kryteria z T-25 (`controls.test.tsx`, `screen-fallback.test.tsx`),
 * przez które ten prymityw nie przechodził i przez które nikt go nie wołał.
 *
 * SŁABA WERSJA PUNKTU O PRZYCISKU: `expect(markup).toContain('<button')`. Obecność przycisku nie
 * jest dowodem, że cokolwiek się stanie — a przycisk bez skutku jest gorszy niż jego brak
 * (niezmiennik 16) i jest dokładnie tym defektem, którego szukał audyt. W tym repo nie ma jsdom,
 * więc `renderToStaticMarkup` NIGDY nie odpali `onClick`; dlatego handler sprawdzamy inaczej:
 * wołamy komponent jak zwykłą funkcję, schodzimy po zwróconym drzewie do elementu `button`
 * i WYWOŁUJEMY jego `onClick`. Szpieg, który wtedy nie strzeli, znaczy, że przycisk jest
 * podłączony do czegoś innego niż to, co dostał wołający.
 */
import { isValidElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { EmptyState } from './empty-state';

const SENTENCE = 'Workflows you build will be listed here.';

/** Treść jedynego elementu z `data-empty`, bez znaczników i bez nadmiarowych odstępów. */
function emptyText(markup: string): string {
  const hit = /<([a-z]+)[^>]*\bdata-empty\b[^>]*>([\s\S]*?)<\/\1>/i.exec(markup);
  return (hit?.[2] ?? '')
    .replace(/<[^>]*>/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/* Propsy elementu w zwróconym drzewie. `ReactElement.props` jest typowane jako `unknown`, więc
 * rzutowanie jest konieczne — ale na WŁASNY interfejs, nie na `any`: `any` wyłączyłoby
 * sprawdzanie wszystkiego, co niżej, w pliku, który ma pilnować kontraktu propsów. */
interface NodeProps {
  children?: unknown;
  onClick?: unknown;
}

/** Wszystkie elementy `<button>` w drzewie, w kolejności wystąpienia. */
function buttonsIn(node: unknown, found: ReactElement[] = []): readonly ReactElement[] {
  if (Array.isArray(node)) {
    for (const child of node as readonly unknown[]) buttonsIn(child, found);
    return found;
  }
  if (!isValidElement(node)) return found;
  if (node.type === 'button') found.push(node);
  buttonsIn((node.props as NodeProps).children, found);
  return found;
}

describe('the empty state marks the sentence itself and only offers an action that acts', () => {
  it('puts data-empty on the element that carries the sentence and nothing else', () => {
    const markup = renderToStaticMarkup(<EmptyState>{SENTENCE}</EmptyState>);

    expect(
      occurrences(markup, 'data-empty'),
      'there has to be exactly one element carrying data-empty. Two, and a criterion reading ' +
        '"the sentence of this empty screen" gets to pick.',
    ).toBe(1);
    expect(
      emptyText(markup),
      'the marked element does not carry the sentence and nothing else. With the marker on the ' +
        'surrounding <div> its content is "◇ sentence" — which is why six screens copied this ' +
        'primitive by hand instead of calling it.',
    ).toBe(SENTENCE);
    expect(
      markup,
      'the diamond in the dashed frame is gone. DESIGN §6 makes it part of the invitation: it ' +
        'says "content goes here and there is none yet" without another sentence.',
    ).toContain('◇');
  });

  it('offers no button at all when the caller has no action to give', () => {
    const markup = renderToStaticMarkup(<EmptyState>{SENTENCE}</EmptyState>);

    expect(
      occurrences(markup, '<button'),
      'an empty screen without an action must render no button. DESIGN §6 asks for one primary ' +
        'button, but Memory fills up with what agents leave each other — there is nothing to ' +
        'create there by hand, and a button that cannot act breaks invariant 16.',
    ).toBe(0);
    expect(
      emptyText(markup),
      'the sentence changed when the optional parts were left out, so the marker is picking up ' +
        'more than the sentence again',
    ).toBe(SENTENCE);
  });

  it('renders the action button with the caller label, and the marker stays on the sentence', () => {
    const markup = renderToStaticMarkup(
      <EmptyState
        hint="Start from a template or a blank one."
        action={{ label: 'Create', onClick: () => undefined }}
      >
        {SENTENCE}
      </EmptyState>,
    );

    expect(occurrences(markup, '<button'), 'the action has to render exactly one button').toBe(1);
    /* Ten punkt jest tu z powodu, który już raz zadziałał: dwa kryteria z T-25 liczą wystąpienia
     * NAPISU `data-empty` i wymagają jednego, więc znacznik przycisku nazwany
     * `data-empty-action` psułby każdy ekran, który podaje akcję — mimo że oznaczony element
     * dalej jest jeden. Liczymy napis, nie element, dokładnie tak jak one. */
    expect(
      occurrences(markup, 'data-empty'),
      'the string data-empty appears more than once with an action on screen. Two criteria in ' +
        'this repo count exactly this string and require one, so a marker sharing that prefix ' +
        'turns every screen with a button red.',
    ).toBe(1);
    expect(markup, 'the button does not carry the label the caller gave').toContain('>Create<');
    expect(
      markup,
      'the second sentence is gone. DESIGN §6 wants one line of instruction in --muted under ' +
        'the invitation, and it is the only place that says what to do next.',
    ).toContain('Start from a template or a blank one.');
    expect(
      emptyText(markup),
      'the hint and the button leaked into the marked element. This is the exact failure the ' +
        'marker on the wrapping <div> had, only with more content to swallow.',
    ).toBe(SENTENCE);
  });

  it('wires the button to the handler the caller passed, and calling it fires that handler', () => {
    const acted = vi.fn();
    const tree = EmptyState({ children: SENTENCE, action: { label: 'Create', onClick: acted } });
    const [button, ...extra] = buttonsIn(tree);

    expect(
      button,
      'no <button> was found in the returned tree, so the assertion below would have nothing to ' +
        'call and this point would pass on nothing',
    ).toBeDefined();
    expect(extra, 'DESIGN §6 allows exactly one primary button on an empty screen').toEqual([]);

    const onClick = (button?.props as NodeProps | undefined)?.onClick;
    expect(
      typeof onClick,
      'the button carries no onClick at all. A control with no handler does not enter this repo ' +
        '(invariant 16), and renderToStaticMarkup drops handlers silently, so nothing else would ' +
        'notice.',
    ).toBe('function');

    (onClick as () => void)();
    expect(
      acted,
      'clicking the button did not reach the handler the caller passed, so the empty screen ' +
        'invites the user to press something that does something else — or nothing',
    ).toHaveBeenCalledTimes(1);
  });
});
