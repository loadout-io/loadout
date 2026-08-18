# Loadout — system projektowy

Wersja 1 · 2026-08-15 · wiążąca dla całego frontendu

Ten plik jest źródłem prawdy dla wyglądu. Kod nie zawiera literałów kolorów ani rozmiarów —
tylko odwołania do tokenów zdefiniowanych tutaj. Agent, który dodaje komponent, czyta ten plik pierwszy.

---

## 1. Teza

**Widok pracy nie przyrasta. Aktualizuje się w miejscu.**

Zwykły terminal dopisuje na dół w nieskończoność. Po dziesięciu minutach pracy czterech agentów
masz ścianę tekstu, w której nie widać, co się dzieje teraz. Poprzednia wersja (poprzedni prototyp) dokładnie
tak działała i to jest główna przyczyna, dla której jej UI był przytłaczający.

Loadout dzieli widok na dwie strefy o różnej fizyce:

```
┌─────────────────────────────────────────────┐
│  HISTORIA — przyrasta, jedna linia na krok  │  ← rośnie w dół, zwięzła
│  ✓ Plan          4 steps                    │
│  ✓ Research      3 agents · 2m              │
├─────────────────────────────────────────────┤
│  TERAZ — stała wysokość, nadpisywana        │  ← nie rośnie, mutuje
│  Forge    writing   src/parser.rs           │
│  Needle   tests     12 ✓  0 ✗               │
│  Rivet    waiting   on Needle               │
├─────────────────────────────────────────────┤
│  ❯ _                                        │
└─────────────────────────────────────────────┘
```

*(Teksty w UI są po angielsku — decyzja D5. Opisy i komentarze w dokumentacji po polsku.)*

Jeden agent = jedna linia, która się przepisuje. Jak `top`, nie jak `tail -f`.
Skończony krok zwija się do jednej linii historii. Nic nie znika — pełny zapis jest o kliknięcie dalej —
ale nic się też nie nawarstwia.

**Sprawdzian:** jeśli podczas biegu czterech agentów przez pięć minut widok przewinął się choć raz
sam z siebie — projekt jest złamany.

---

## 2. Podpis wizualny: pasek loadoutu

Nazwa aplikacji pochodzi z gier: loadout to zestaw, który kompletujesz **przed** wyjściem w teren.
To jest jedyny ozdobny element w całej aplikacji i jedyne miejsce, gdzie wolno nam być efektowni.

Nad widokiem pracy siedzi pasek — wybrany workflow jako ciąg bloków, jeden na krok:

```
▓▓▓▓  ▓▓▓▓  ░░░░  ░░░░      Exact-diff delivery · krok 2 z 4
 plan  bada  pisze sprawdza
```

- Blok skończony: wypełniony, `--ink-muted`
- Blok aktywny: wypełniony `--accent`, jedyny nasycony element na ekranie
- Blok czekający: obrys `--line`, puste wnętrze

Pasek jest jednocześnie nawigacją: klik w blok pokazuje historię tego kroku.
Nie ma paska postępu w procentach. Kroki to nie procenty.

---

## 3. Kolor

### Powierzchnie

| Token | Hex | Użycie |
|---|---|---|
| `--bg` | `#06090b` | tło aplikacji, obszar pracy |
| `--panel` | `#0d1216` | panele boczne, nagłówki, paski |
| `--raised` | `#141b20` | kontrolki na panelu, chipy, kafelki |
| `--well` | `#040708` | pola wejściowe, bloki kodu, płótno edytora |

Cztery poziomy. Głębia bierze się **wyłącznie** ze zmiany koloru powierzchni — nie ma cieni.

### Tekst

| Token | Hex | Użycie |
|---|---|---|
| `--ink` | `#e8eff1` | nagłówki, wartości, aktywna treść |
| `--body` | `#cbd6d9` | zdania, opisy, treść domyślna |
| `--muted` | `#a3b1b5` | etykiety, metadane, rzeczy skończone |

### Linie

| Token | Hex | Użycie |
|---|---|---|
| `--line` | `#212a2f` | podziały, obrysy kart, siatka |
| `--line-strong` | `#5a6d76` | obrys kontrolki interaktywnej, focus w spoczynku |

### Stan — cztery i ani jeden więcej

To jest cały słownik semantyczny aplikacji. Każdy kolor odpowiada na jedno pytanie.

| Token | Hex | Znaczy | Pytanie, na które odpowiada |
|---|---|---|---|
| `--accent` | `#6ee0b0` | **teraz** | co się dzieje w tej chwili? |
| `--attend` | `#ffb45b` | **ty** | co czeka na moją decyzję? |
| `--fail` | `#ff8f9f` | **zepsute** | co poszło źle? |
| `--human` | `#c6a8ff` | **człowiek** | co zrobiła osoba, nie maszyna? |

Wash i edge dla każdego (tło chipa i jego obrys):

```
--accent-edge #3d8a70   --accent-wash #0f2620
--attend-edge #8f6f30   --attend-wash #2a1e0e
--fail-edge   #a1515f   --fail-wash   #2a1319
--human-edge  #7a6aa8   --human-wash  #1c1830
```

### Tożsamość ≠ stan

Agenci mają swoje kolory, żeby szyna dała się skanować wzrokiem. Ale kolor agenta **nigdy nie może
być pomylony z kolorem stanu** — inaczej pomarańczowy agent i „czeka na twoją decyzję" znaczą to samo.

Rozdział jest po nasyceniu:

| | Nasycenie | Tokeny |
|---|---|---|
| **Stan** | nasycone | `--accent` `--attend` `--fail` `--human` |
| **Tożsamość** | przygaszone, zbliżona jasność | `--id-1 #5c7a8a` `--id-2 #7a6a8a` `--id-3 #8a7a5c` `--id-4 #5c8a7a` `--id-5 #8a5c6a` |

Agent dostaje kwadrat 22px w swoim przygaszonym kolorze, z inicjałem w `--ink`.
Stan agenta jest **słowem** w kolorze nasyconym, nigdy kolorem kwadratu.

> Ta reguła powstała przy budowie makiety: referencyjny redesign poprzedniego prototypu daje agentowi Forge
> kolor `#ffb45b`, czyli dokładnie ten sam hex co „wymaga uwagi". Na jednym ekranie oznaczały
> dwie różne rzeczy tym samym kolorem.

### Reguła jednego akcentu

`--accent` jest **jedynym kolorem interaktywnym**. Przycisk podstawowy, focus, aktywna zakładka,
kursor w polu, aktywny blok w pasku loadoutu. Nic innego go nie używa.

**Rzecz skończona jest cicha.** Ukończony krok to `✓` w kolorze `--muted`, nie zielony ptaszek.
Zielony znaczy „dzieje się teraz", nie „udało się". To odróżnia Loadout od każdego dashboardu,
który świeci na zielono, kiedy nic się nie dzieje.

Zakazane: gradienty dekoracyjne, cienie na chrome, drugi kolor marki, kolor jako ozdoba.
Jedyny dopuszczalny gradient to `linear-gradient` maskujący ucięcie długiej listy.

---

## 4. Typografia

### Dwie rodziny, jedna reguła

- **Inter** — język ludzki. Zdania, etykiety, przyciski, nagłówki, opisy.
- **SFMono-Regular / Roboto Mono** — wartości maszynowe. Ścieżki, identyfikatory, hashe, liczby, czas, nazwy plików, komendy.

**To jest reguła semantyczna, nie estetyczna.** Mono znaczy „to wyprodukowała maszyna i możesz to skopiować".
Widzisz mono → wiesz, że to fakt, nie opis.

> Świadoma zmiana wobec referencji, z której ten system wyrósł: miała około 90% tekstu w mono 12px.
> (Plik referencyjny usunięty z repo 2026-08-18 — jedyną wyrocznią wyglądu jest `docs/mockup/index.html`.)
> To jest jedna z konkretnych przyczyn, dla których tamten interfejs męczy — wszystko wygląda jednakowo
> ważne i jednakowo techniczne. Loadout odwraca proporcję: Inter jest domyślny, mono jest wyjątkiem.

### Drabinka

| Token | Rodzina | Rozmiar | Waga | Interlinia | Tracking | Użycie |
|---|---|---|---|---|---|---|
| `--t-title` | Inter | 20px | 600 | 1.2 | -0.2px | tytuł ekranu, jeden na widok |
| `--t-heading` | Inter | 15px | 600 | 1.3 | -0.1px | nagłówek sekcji, tytuł karty |
| `--t-body` | Inter | 13px | 400 | 1.5 | 0 | zdania i opisy |
| `--t-ui` | Inter | 13px | 600 | 1.2 | 0 | przyciski, aktywne etykiety |
| `--t-label` | Inter | 11px | 600 | 1.2 | 0.08em | etykieta pola, WERSALIKI |
| `--t-mono` | mono | 12px | 400 | 1.45 | 0 | wartości maszynowe |
| `--t-mono-strong` | mono | 12px | 700 | 1.2 | 0.06em | identyfikator, nazwa agenta |
| `--t-stream` | mono | 13px | 400 | 1.5 | 0 | linia w widoku pracy |

Waga 500 nie istnieje. Drabinka to 400 / 600 / 700.

Rozmiary poniżej 11px nie istnieją. Jeśli coś nie mieści się w 11px, jest niepotrzebne.

---

## 5. Przestrzeń i kształt

Baza 4px. Skala: `4 · 8 · 12 · 16 · 24 · 32 · 48`.

```
--s-1: 4px    --s-2: 8px    --s-3: 12px   --s-4: 16px
--s-5: 24px   --s-6: 32px   --s-7: 48px
```

- Padding wiersza strumienia: `8px 16px`
- Padding karty: `12px`
- Padding panelu: `16px`
- Odstęp między sekcjami: `24px`

**`border-radius: 2px`. Wszędzie. Bez wyjątków.** Kółka tylko dla kropek stanu (`50%`, 8px).

Wysokość kontrolek: `28px` kompaktowa · `32px` domyślna · `36px` podstawowy przycisk.
Cel dotykowy nie dotyczy — to aplikacja desktopowa sterowana myszą i klawiaturą.

---

## 6. Komponenty

Definicja komponentu = tokeny, nie hexy. Poniżej pełna lista v1.

### `button-primary`
`background: --accent` · `color: --bg` · `--t-ui` · `radius 2px` · `padding 8px 16px` · `height 36px`
Active: `transform: scale(0.98)`. Focus: `outline: 2px solid --accent; outline-offset: 2px`.

### `button-secondary`
`background: --raised` · `color: --ink` · `border: 1px solid --line-strong` · `--t-ui` · `height 32px`

### `button-quiet`
`background: transparent` · `color: --body` · `border: 1px solid --line` · `--t-ui` · `height 28px`

### `button-danger`
Jak `button-secondary`, ale `border: 1px solid --fail-edge` · `color: --fail`. Bez wypełnienia — akcja
niszcząca nie ma być najbardziej rzucającym się w oczy elementem, ma być rozpoznawalna.

### `chip`
`padding 2px 8px` · `--t-label` · `height 20px` · `border 1px solid {stan}-edge` · `background {stan}-wash` · `color {stan}`
Wariant neutralny: `--line` / `--raised` / `--muted`.

### `field`
`background: --well` · `border: 1px solid --line-strong` · `color: --ink` · `--t-mono` · `padding 8px 10px` · `height 32px`
Focus: `border-color: --accent`. Etykieta nad polem w `--t-label` / `--muted`.

### `stream-line` — wiersz w widoku pracy
Siatka `88px 1fr auto`. Padding `8px 16px`. Bez obramowania między wierszami — separacja przez odstęp.
Kolumna 1: nazwa agenta, `--t-mono-strong`, kolor agenta.
Kolumna 2: co robi, `--t-stream`, `--ink`.
Kolumna 3: liczba/czas, `--t-mono`, `--muted`.
Wiersz aktywny: lewy pasek `inset 2px 0 --accent`. Wiersz skończony: `opacity 0.55`.

### `history-line` — zwinięty krok
Siatka `20px 1fr auto`. Padding `6px 16px`. `--t-body`. Znacznik `✓` w `--muted`.
Klik rozwija pełny zapis kroku pod spodem.

### `agent-card` — kafelek w prawej szynie
`background: --panel` · `border: 1px solid --line` · `padding 12px`.
Aktywny: `border-color: --line-strong`, lewy pasek `inset 2px 0 {kolor agenta}`.
Zawiera: kwadrat 14px w kolorze agenta, nazwę (`--t-mono-strong`), rolę (`--t-label`, `--muted`),
jedno zdanie o tym, co robi (`--t-body`), chip stanu.
**Maksymalnie cztery linie tekstu.** Piąta linia to błąd projektowy.

### `node-card` — kafelek w edytorze workflow
Szerokość `280px` · `background: --raised` · `border: 1px solid --line-strong` · `padding 12px`.
Nagłówek: uchwyt `⠿`, nazwa (`--t-heading`), kropka koloru agenta, nazwa agenta (`--t-mono`), `✕`.
Ciało: jedno zdanie po ludzku o tym, co ten krok robi.
Stopka: `◂ po: {krok}` po lewej, `działa przed ▸` po prawej.
Zaznaczony: `border-color: --accent`.

### `loadout-strip` — pasek loadoutu
Patrz §2. Blok `32×8px`, odstęp `8px`, etykieta pod blokiem `--t-label` / `--muted`.

### `modal`
`background: --panel` · `border: 1px solid --line-strong` · `max-width 640px` · `padding 24px`.
Tło za modalem: `rgba(6,9,11,0.72)`. Bez rozmycia, bez animacji wjazdu poza `opacity 120ms`.

### `empty-state`
Wyśrodkowane. Znak `◇` w ramce `1px dashed --line-strong`, zdanie w `--ink`,
jedno zdanie instrukcji w `--muted`, jeden przycisk podstawowy.
**Pusty ekran to zaproszenie do działania, nie komunikat o braku danych.**

---

## 7. Ruch

Jedna mikrointerakcja w całym systemie: `transform: scale(0.98)` na wciśnięciu.

Poza tym:
- Zmiana treści w strefie TERAZ: bez animacji. Tekst po prostu jest inny. Animowanie przepisania
  linii sprawia, że oko goni ruch zamiast czytać.
- Wejście nowej linii historii: `opacity 0 → 1` w `120ms ease-out`. Bez przesunięcia.
- Pulsowanie: **tylko** kropka aktywnego agenta, `opacity 1 → 0.35`, `1.4s steps(2)`.
  Skokowo, nie płynnie — płynne pulsowanie czyta się jak oddychanie i rozprasza.

`@media (prefers-reduced-motion: reduce)` wyłącza wszystko powyżej.

---

## 8. Język interfejsu

**UI jest po angielsku** (decyzja D5). Czasownik w trybie rozkazującym, zdanie proste, bez żargonu.
Wiążąca jest tabela żargon→prosty-język z `docs/research/projects/00-SYNTHESIS.md` §2.2 (55 wierszy).

| Zamiast | Piszemy |
|---|---|
| Submit / Execute workflow | `Run` |
| Configuration | `Settings` |
| Initialize | `Create` |
| Terminate | `Stop` |
| Failed with exit code 1 | `Tests failed · 3 of 40` |
| No records found | `Nothing here yet. Type /plan to start.` |
| tool call, tool_use | *(nic — nazwij czynność: `Read`, `Edited`, `Ran`)* |
| stdout / stderr / exit code | `output`, `didn't work` |
| token, context window, compaction | `length`, `started a fresh page` |
| PTY, session, process, spawn | `terminal`, `agent`, `started` |
| orchestrator / DAG / node | `lead agent`, `workflow`, `step` |
| thinking / reasoning tokens | `Thinking…` |
| diff / hunk | `changes` |

Nazwa akcji nie zmienia się w trakcie przepływu: przycisk `Publish` → komunikat `Published`.

Błąd nie przeprasza i nie jest ogólny. Mówi, co się stało i co z tym zrobić:
> `Can't find claude on your system. Install Claude Code, or point Loadout at it in Settings.`

Pusty ekran to zaproszenie, nie komunikat o braku danych:
> `Nothing here yet. Type /plan to start.`

---

## 9. Kontrola jakości

Komponent nie trafia do repo, dopóki:

- [ ] nie ma w kodzie ani jednego literału hex, px rozmiaru czcionki ani `border-radius` innego niż token
- [ ] focus jest widoczny z klawiatury i wygląda tak samo we wszystkich kontrolkach
- [ ] wygląda poprawnie przy szerokości okna 1100px (najwęższe wspierane)
- [ ] nie używa `--accent` do niczego poza „teraz" i interakcją
- [ ] nie dodaje piątego koloru semantycznego
- [ ] `prefers-reduced-motion` go nie psuje
- [ ] żaden tekst w nim nie jest w mono, jeśli nie jest wartością maszynową
