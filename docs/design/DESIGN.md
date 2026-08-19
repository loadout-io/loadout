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

## 2. Marka i podpis wizualny

### Znak: najmniejszy prawdziwy graf

Jedno wejście, dwie równoległe gałęzie, jedna synteza. Cztery węzły, cztery krawędzie.

To nie jest ozdoba dobrana do nazwy. To **dwie z pięciu rzeczy z decyzji D6**, których żaden
vendor nie zbuduje, bo nie ma w tym interesu: odpalić kilku agentów naraz i zebrać ich wyniki
w jeden. Wnętrze aplikacji ma niezmiennik 17 — „UI nie rysuje relacji, których nie ma w danych" —
więc marka, która **jest** najmniejszym grafem prawdziwym, jest jedynym ornamentem, na jaki ten
produkt ma prawo.

Do 2026-08-19 znak był czterema luźnymi kwadratami obróconymi o 45°. Cztery luźne kwadraty nie
mają krawędzi, więc nie mają relacji, więc nie są grafem. Nowy znak dokłada dokładnie dwie
rzeczy: **krawędzie i kierunek.**

| | |
|---|---|
| Siatka | 24 × 24 |
| Węzły | `3,7·12` · `12·5,1` · `12·18,9` · `20,3·12` |
| Promień | 1,95; węzeł syntezy **2,15** (+10%, bo z wielu wychodzi jedno) |
| Krawędź | 1,25, zakończenia okrągłe |
| Stosunek | średnica węzła do grubości linii **3,1 : 1** |
| Sylwetka | romb 16,6 × 13,8 — **szerszy niż wysoki** |

Stosunek 3,1 : 1 jest liczbą, na której ten znak stoi: przy 2,4 : 1 węzeł czyta się jako
zgrubienie linii i cały znak zamyka się w pierścień (zmierzone na 176 px). Romb jest szerszy niż
wysoki, bo graf płynie w poziomie — symetryczny czytałby się jak karo.

**W chrome znak jest neutralny**: węzły `--body`, krawędzie `--muted`. Ani akcentu, ani coralu.
Akcent znaczy „to jest interaktywne", coral „to się dzieje teraz", a znak wisi w nawigacji także
wtedy, kiedy nic nie chodzi i nic nie jest klikalne.

Krawędzie brały najpierw `--line-strong`, czyli biel 16%. **Rodzina `--line-*` jest obramowaniem**
— rysuje włos na krawędzi szkła — a krawędzie tego znaku są jego tematem, nie ramką wokół niego.
Zmierzone na wyrenderowanej powłoce przy 22 px, w jedynym rozmiarze, w jakim znak stoi
w aplikacji: linia 1,25 px w bieli 16% na panelu daje około 1,7 : 1 kontrastu, czyli nie czyta się
wcale, a znak wraca do czterech kropek — czyli do tego, czym był stary znak i czym miał przestać
być. Wartość z rodziny tekstu czyta się, a hierarchia zostaje ta sama, bo `--muted` jest
ciemniejsze od `--body`: węzły nad krawędziami.

### Ikona: trzy rysunki, nie jeden przeskalowany

Przepis 1:1 z systemu, z którego wzięliśmy wartości: squircle `rx=232` na płótnie 1024, radialne
tło indygo, sheen zanikający na 34% wysokości, temat wyśrodkowany, ostra krawędź wewnętrzna
w bieli 10%. Dwie nasze aplikacje mają w Docku wyglądać na rodzeństwo.

| Rozmiar | Rysunek |
|---|---|
| 1024 / 512 / 256 / 128 | pełny |
| 32 | krawędzie grubsze, węzły jednolitą barwą, bez sheenu i bez krawędzi wewnętrznej |
| 16 | sylwetka i cztery kropki, jedna barwa |

Przy 32 px cztery krawędzie po 38 jednostek mierzą niecały 1,2 px i zlewają się w plamę, a sheen
i krawędź wewnętrzna operują na 0,1 px. **`.icns` jest zestawem, nie skalowaniem** — ikona, która
mydli się na pasku Docka, jest pierwszą rzeczą, jaką człowiek widzi o jakości aplikacji.

Źródłem prawdy są trzy pliki SVG w `docs/branding/`. PNG-i i `.icns` są z nich **generowane**
(`scripts/icons.sh`, przez `qlmanage` → `sips` → `iconutil`), więc nie ma dwóch rysunków tej samej
rzeczy. Gradienty mieszkają wyłącznie tam — poza `src/`, gdzie żaden literał barwy nie ma prawa
stanąć.

**Przepis jest deterministyczny i to jest zmierzone**: 2026-08-19 pełne przebudowanie dało pięć
plików bajt w bajt identycznych z tymi, które leżą w repo (md5 przed i po). Dlatego pochodzenie
rastrów sprawdza się jednym uruchomieniem skryptu i `git status`, a nie okiem. W bramce tego nie
ma z premedytacją: `qlmanage` wymaga sesji graficznej i skanowania Gatekeepera, a bramka ma chodzić
bez okna. Zamiast tego bramka pilnuje **wieku**: `.icns` młodszy od każdego z trzech rysunków
i od samego skryptu.

### Logotyp

`loadout` — **małymi literami, zawsze.** Hanken Grotesk 600, ciasny tracking, `--ink`, nigdy
w akcencie. Podpis: **many agents, one plan**.

`LOADOUT` w monospace z rozstrzeleniem `.12em` było cytatem z terminala, nie logotypem: mono
w tym systemie znaczy „to wyprodukowała maszyna", a nazwa produktu jest językiem ludzkim. Podpis
nie dokłada ani jednego terminu do słownika — `agent` i `plan` już są w interfejsie.

Plik `docs/branding/loadout-logo.svg` **niesie krój w sobie**, jako `data:`. Wymóg brzmi „logotyp
nie ma prawa po cichu spaść na inny krój", a powód jest zmierzony w tym repo: `theme.css`
deklarował Intera od pierwszego dnia i rysował się krojem systemowym przez całe życie repo, bez
ani jednego błędu w konsoli.

### Podpis wizualny: pasek loadoutu

Nazwa aplikacji pochodzi z gier: loadout to zestaw, który kompletujesz **przed** wyjściem w teren.
Nad widokiem pracy siedzi pasek — wybrany workflow jako ciąg bloków, jeden na krok:

```
▓▓▓▓  ▓▓▓▓  ████  ░░░░      Exact-diff delivery · krok 3 z 4
 plan  bada  pisze sprawdza
```

- Blok skończony: wypełniony, `--muted`
- Blok aktywny: wypełniony `--live`, jedyny nasycony element na ekranie
- Blok czekający: obrys `--line-strong`, puste wnętrze
- Segmenty stoją w **jednym szklanym torku** o promieniu kapsuły; nie są czterema luźnymi
  znaczkami. Liczba segmentów bierze się z liczby kroków w danych — **nie ma paska
  procentowego, bo kroki to nie procenty.**

Pasek jest jednocześnie nawigacją: klik w blok pokazuje historię tego kroku.

---

## 3. Kolor

Wartości pochodzą z systemu projektowego meetnotes (`../meetnotes/src/design-tokens/`, nazwa
własna „Quiet Glass"). Bierzemy **wartości**, nie inspirację — dwie nasze aplikacje mają w Docku
wyglądać na rodzeństwo (decyzja D1). Kopia wartości leży w `docs/design/house-values.json`
i jest porównywana z `theme.css` w każdym biegu testu.

Reguła nadrzędna całego systemu, wprost z tamtego pliku: **szkło jest chrome, treść jest
papierem.** Szkło nie wchodzi nigdy pod tekst ani pod kod, które człowiek ma przeczytać.

### Powierzchnie

| Token | Hex | Użycie |
|---|---|---|
| `--bg` | `#07070b` | tło aplikacji, kartka treści |
| `--panel` | `rgba(255, 255, 255, 0.045)` | wypełnienie szkła: menu, pasek, szyna |
| `--raised` | `rgba(255, 255, 255, 0.045)` | karty i kafelki na szkle |
| `--well` | `rgba(255, 255, 255, 0.035)` | pola wejściowe, bloki kodu |
| `--overlay` | `#1b1b24` | **nieprzejrzyste** menu i podpowiedzi |
| `--solid` | `#111118` | gdy potrzebna jest prawdziwie kryjąca powierzchnia |
| `--hover` | `rgba(255, 255, 255, 0.06)` | podkład wiersza pod kursorem |
| `--scrim` | `rgba(0, 0, 0, 0.5)` | przygaszenie za modalem |

Powierzchnie podniesione są **bielą-alfa**, nie własnym szarym. Jedna definicja daje dwa
zachowania: nad aurorą załamuje światło, nad kartką treści czyta się jako delikatne podniesienie.
Menu i podpowiedź **nie mogą** być szkłem — leżą nad treścią, którą człowiek właśnie czyta.

### Tekst

| Token | Hex | Użycie |
|---|---|---|
| `--ink` | `#f6f6fa` | nagłówki, wartości, aktywna treść |
| `--body` | `#a6a6b6` | zdania, opisy, treść domyślna |
| `--muted` | `#8a8a9c` | etykiety, metadane, rzeczy skończone |

Dom ma **cztery** stopnie tekstu; bierzemy trzy. `--muted` to domowy `--text-tertiary`, nie
`--text-muted`: tamten stopień (`#6c6c7d`) mierzy 3,62:1 na powierzchni podniesionej, czyli
**pod progiem czytelności**, i dom trzyma go wyłącznie dla ≥13 px. U nas prawie każdy przygaszony
napis to metadana ≤12 px, więc jaśniejszy stopień (5,50:1) jest jedynym poprawnym. Czwartego nie
wprowadzamy, żeby nikt nie miał czym sięgnąć po ciemniejszy.

### Linie

| Token | Hex |
|---|---|
| `--line` | `rgba(255, 255, 255, 0.09)` |
| `--line-strong` | `rgba(255, 255, 255, 0.16)` |
| `--line-subtle` | `rgba(255, 255, 255, 0.055)` |

### Akcent — mówi „to jest interaktywne" i nic więcej

| Token | Hex | Użycie |
|---|---|---|
| `--accent` | `#6e76ff` | focus, przycisk podstawowy, aktywny glif, kursor w polu |
| `--accent-hover` | `#8a90ff` | ten sam element pod kursorem |
| `--accent-active` | `#5b63f0` | ten sam element wciśnięty |
| `--accent-soft` | `rgba(110, 118, 255, 0.16)` | tło elementu wybranego |
| `--accent-ring` | `rgba(110, 118, 255, 0.5)` | obrys kontrolki, pierścień focusu |

Do 2026-08-19 jeden token odpowiadał na dwa różne pytania: ten dokument mówił jednocześnie
„`--accent` jest **jedynym kolorem interaktywnym**" i „`--accent` znaczy **teraz**". To są dwa
fakty, więc mają dwa tokeny (niezmiennik 13).

**Akcent nigdy nie wypełnia chrome.** Bierze go focus, przycisk podstawowy i aktywny glif —
i na tym koniec. Aktywny wiersz menu jest neutralny; barwę dostaje wyłącznie jego glif.

### Stan — cztery i ani jeden więcej

| Token | Hex | Znaczy | Pytanie, na które odpowiada |
|---|---|---|---|
| `--live` | `#ff7a5c` | **teraz** | co się dzieje w tej chwili? |
| `--attend` | `#f5b14c` | **ty** | co czeka na moją decyzję? |
| `--fail` | `#ff6b6b` | **zepsute** | co poszło źle? |
| `--human` | `#9d7bff` | **człowiek** | co zrobiła osoba, nie maszyna? |

Wash i edge dla każdego — tło chipa i jego obrys:

| Token | Hex | Token | Hex |
|---|---|---|---|
| `--live-soft` | `rgba(255, 122, 92, 0.16)` | `--live-edge` | `rgba(255, 122, 92, 0.5)` |
| `--attend-soft` | `rgba(245, 177, 76, 0.14)` | `--attend-edge` | `rgba(245, 177, 76, 0.5)` |
| `--fail-soft` | `rgba(255, 107, 107, 0.14)` | `--fail-edge` | `rgba(255, 107, 107, 0.5)` |
| `--human-soft` | `rgba(157, 123, 255, 0.14)` | `--human-edge` | `rgba(157, 123, 255, 0.5)` |

`--live` w domu nazywa się tak samo i pilnuje nagrywania; u nas pilnuje pracującego agenta.
Ta sama robota: **żywe, nie alarmujące.**

#### Reguła formy, bez której `--live` i `--fail` są nieodróżnialne

Te dwie barwy różnią się odcieniem o **~13°**, a w naszym strumieniu stoją w sąsiednich
wierszach — czego dom nigdy nie musi pokazać. Rozstrzyga to forma, nie barwa:

- `--live` występuje **wyłącznie** jako: podkład aktywnego wiersza strefy „teraz", jego obrys,
  aktywny segment paska loadoutu, pulsująca kropka, kropka karty w tle.
- `--fail` występuje **wyłącznie** jako: glif `✕`, obrys chipa, lewa krawędź bloku błędu.

Rozłączność tych dwóch słowników form jest **sprawdzana statycznie**, nie oceniana okiem.

### Tożsamość ≠ stan

Agenci mają swoje kolory, żeby szyna dała się skanować wzrokiem. Ale kolor agenta **nigdy nie
może być pomylony z kolorem stanu** — inaczej pomarańczowy agent i „czeka na twoją decyzję"
znaczą to samo. Rozdział jest po nasyceniu.

| | Nasycenie | Tokeny |
|---|---|---|
| **Stan** | nasycone | `--live` `--attend` `--fail` `--human` |
| **Tożsamość** | przygaszone, zbliżona jasność | `--id-1 #6f8496` `--id-2 #7f7597` `--id-3 #94886b` `--id-4 #6b9285` `--id-5 #96707d` |

Agent dostaje kwadrat 22px w swoim przygaszonym kolorze, z inicjałem w `--ink`.
Stan agenta jest **słowem** w kolorze nasyconym, nigdy kolorem kwadratu.

> Kolory tożsamości są **nasze** i nie mają odpowiednika w domu: tamtejsze `--graph-*` są
> nasycone, bo obsługują legendę grafu. Reguła powstała przy budowie makiety: referencyjny
> redesign poprzedniego prototypu dawał agentowi Forge dokładnie ten sam kod barwy co „wymaga uwagi".
> Na jednym ekranie oznaczały dwie różne rzeczy tym samym kolorem.

### Aliasy poprzedniej palety

`--accent-edge`, `--accent-wash`, `--attend-wash`, `--fail-wash`, `--human-wash` **żyją**
i wskazują na nowe nazwy. Trzy powierzchnie wołają je jeszcze bezpośrednio i migrują osobno;
nazwa skasowana pod niezmigrowanym komponentem zostawia element bez ani jednej reguły CSS —
awarię, która nie rzuca wyjątku i nie pojawia się w żadnym logu (niezmiennik 25).
Znikają razem z ostatnim wołającym.

Zakazane: gradienty dekoracyjne, drugi kolor marki, kolor jako ozdoba, barwione szkło,
piąty kolor stanu, cień pod czymś, co nie pływa.

---

## 4. Typografia

### Dwie rodziny, jedna reguła

- **Hanken Grotesk** — język ludzki. Zdania, etykiety, przyciski, nagłówki, opisy, logotyp.
- **JetBrains Mono** — wartości maszynowe. Ścieżki, identyfikatory, hashe, liczby, czas, nazwy
  plików, komendy.

**To jest reguła semantyczna, nie estetyczna.** Mono znaczy „to wyprodukowała maszyna i możesz
to skopiować". Widzisz mono → wiesz, że to fakt, nie opis.

Oba kroje są **zmiennymi plikami `.woff2` w repo** (`src/styles/fonts/`), te same, które niesie
`../meetnotes/src/assets/fonts/`. Oba OFL.

> Zamknięty defekt, dla pamięci. Do 2026-08-19 ten dokument żądał Intera, `theme.css` go
> deklarował, a w drzewie nie było ani jednego pliku kroju i ani jednej reguły `@font-face`.
> Aplikacja rysowała się krojem systemowym — po cichu, bez ani jednego błędu w konsoli.
> Dokładnie jedna rodzina na token stoi w cudzysłowie i dokładnie ta jedna ma swój `@font-face`;
> dalsze człony są systemowe i bez cudzysłowu. Cudzysłów jest obietnicą, że plik leży w drzewie.

### Drabinka

| Token | Rodzina | Rozmiar | Waga | Interlinia | Tracking | Użycie |
|---|---|---|---|---|---|---|
| `--t-title` | ui | 20px | 600 | 1.2 | -0.02em | tytuł ekranu, jeden na widok |
| `--t-heading` | ui | 15px | 600 | 1.3 | -0.01em | nagłówek sekcji, tytuł karty |
| `--t-subhead` | ui | 14px | 600 | 1.3 | 0 | tytuł kafelka na liście |
| `--t-body` | ui | 13px | 400 | 1.5 | 0 | zdania i opisy |
| `--t-ui` | ui | 13px | 600 | 1.2 | 0 | przyciski, aktywne etykiety |
| `--t-note` | ui | 12px | 400 | 1.45 | 0 | drugie zdanie, podpowiedź pod polem |
| `--t-label` | ui | 11px | 600 | 1.2 | 0 | **etykieta pola, zdaniowo** |
| `--t-eyebrow` | ui | 11px | 600 | 1.2 | 0.06em | **nadoczko sekcji, WERSALIKI** |
| `--t-meta` | ui | 11px | 400 | 1.2 | 0 | wartość maszynowa w drugim planie |
| `--t-mono` | mono | 12px | 400 | 1.45 | 0 | wartości maszynowe |
| `--t-mono-strong` | mono | 12px | 700 | 1.2 | 0.06em | identyfikator, nazwa agenta |
| `--t-stream` | mono | 13px | 400 | 1.5 | 0 | linia w widoku pracy |

Waga 500 nie istnieje. Drabinka to 400 / 600 / 700.
Rozmiary poniżej 11px nie istnieją. Jeśli coś nie mieści się w 11px, jest niepotrzebne.

`font-variant-numeric: tabular-nums` obowiązuje wszędzie, gdzie cyfry stoją w kolumnie.

### Dwa stopnie tam, gdzie był jeden

`--t-label` i `--t-eyebrow` mają ten sam rozmiar i tę samą wagę, a różnią się jedną rzeczą:
**wersalikami**. Do 2026-08-19 istniał jeden stopień i obsługiwał oba zastosowania, więc
wersaliki wchodziły albo na **każdą** etykietę pola, albo na żadną.

Podział jest sprawdzalny i jest sprawdzany:

- **Nadoczko sekcji** — `AGENTS`, `BUILD`, `WHAT IT DOES`. Wersaliki, tracking `0.06em`.
- **Etykieta pola i rola agenta** — `Name`, `Give up after`, `writes code`. Zdaniowo, tracking 0.

Wersaliki na każdej etykiecie pola są najczęstszym ruchem domyślnego panelu admina i pierwszą
rzeczą, po której formularz przestaje wyglądać jak macOS.

> **Rozbieżność zapisana, nie naprawiona po cichu.** Ten dokument stawia `--t-eyebrow` w rodzinie
> `ui`, bo nadoczko jest językiem ludzkim, a `mono` znaczy „wartość maszynowa". Makieta trzyma
> trzy reguły nadoczka (`.side h3`, `.rail h2`, `.ctx .ch`) w `mono` i komponenty niosą przy nich
> klasę `font-mono`. **Żadna wyrocznia tego nie porównuje**, więc rozjazd nie świeci na czerwono
> — i właśnie dlatego stoi tu wypisany. Zmiana rodziny jest zmianą WYGLĄDU, a wygląd zmienia się
> razem z regułą w makiecie: należy do T-48, nie do zadania o słowniku.

Reguła `text-transform` mieszka **w definicji stopnia**, w warstwie `components`, a nie
w komponentach: Tailwind pozwala tokenowi `--text-*` nieść interlinię, rozstrzelenie i wagę,
ale nie `text-transform`. Warstwa `components` stoi niżej niż `utilities`, więc `normal-case`
nadal wygrywa tam, gdzie makieta nie ma wersalików.

---

## 5. Przestrzeń i kształt

Baza 4px. Skala: `4 · 8 · 12 · 16 · 24 · 32 · 48`.

```
--space-1: 4px    --space-2: 8px    --space-3: 12px   --space-4: 16px
--space-5: 24px   --space-6: 32px   --space-7: 48px
```

- Padding wiersza strumienia: `8px 16px`
- Padding karty: `12px`
- Padding panelu: `16px`
- Odstęp między sekcjami: `24px`

**Promienie: wyłącznie `--radius-sm 9px`, `--radius-md 13px`, `--radius-lg 18px`
i `--radius-pill 999px`.** Pasmo domu ma jeszcze `24px`; my go świadomie nie bierzemy, bo
narzędzie o tej gęstości przy 24 px wygląda jak aplikacja na iPada. **Wiersz strumienia nie ma
promienia wcale** — i to on utrzymuje gęstość.

Wysokość kontrolek: `28px` kompaktowa · `32px` domyślna · `36px` podstawowy przycisk.
Cel dotykowy nie dotyczy — to aplikacja desktopowa sterowana myszą i klawiaturą.

### Rama okna: dwie kartki, jeden odstęp, aurora pod nimi

```
okno (róg rysuje macOS)
└─ odstęp 8px, aurora + --bg
   ├─ nawigacja  208px · --radius-lg · szkło · PŁYWA (jedyny cień w aplikacji)
   └─ treść      --radius-md · nieprzejrzysta · obrys --line
      ├─ karty   32px · szkło
      ├─ pasek   52px · szkło
      └─ praca
```

**Aurora mieszka wewnątrz okna**, nie na pulpicie: statyczna winieta przy lewej krawędzi, pod
kartkami. To rozwiązanie z systemu, z którego wzięliśmy wartości, i ma konsekwencję, która
oszczędza całą klasę pracy — **szkło ma co załamywać bez przezroczystego okna.** Żadnego
`transparent: true`, żadnego `windowEffects`, żadnej zależności od tapety użytkownika. Kolumna
czytania siedzi na czystym `--bg`, więc kod i tekst nigdy nie leżą na barwie.

**Pływa dokładnie jedna rzecz i tylko ona ma cień.** Refleks `inset` na górnej krawędzi szkła
nie jest głębią — to światło na materiale i wolno go mieć wszędzie.

### Budżet chrome jest prawie wydany, i to on wygrywa z projektem

`docs/ARCHITECTURE.md` §7 daje **96 px nad pierwszą treścią**:

| składnik | px |
|---|---|
| odstęp okna | 8 |
| obrys kartki treści | 1 |
| karty workspace | 32 |
| pasek loadoutu | 52 |
| **razem** | **93** |

Trzy piksele zapasu. Naiwna wersja pływającej kartki dawała 100 — projekt się ścisnął, limit nie
drgnął. §7 mówi wprost: kolejny pasek wymaga usunięcia innego, nie negocjacji limitu.

Suma jest **mierzona z dwóch stron**, bo żadna nie widzi wszystkiego: strona makiety czyta
cztery reguły CSS, strona aplikacji sumuje rodzeństwo nad treścią plus własny odstęp kontenera.
Obrysu zadeklarowanego klasą nie da się odczytać z renderu — i to jest w obu miejscach zapisane,
a nie przemilczane.

### Trzy klasy materiału

| Klasa | Czym jest | Kto ją nosi |
|---|---|---|
| `.glass` | wypełnienie, rozmycie, refleks na górnej krawędzi | pasek, szyna, karty |
| `.pane` | `.glass`, które **pływa**: promień `lg`, obrys mocny, cień | nawigacja |
| `.paper` | nieprzejrzysta kartka, promień `md`, obrys `line` | treść |

Definicje mieszkają w warstwie `components` arkusza, nie w komponentach: rozmycie zapisane
w komponencie jest literałem dokładnie tam, gdzie `checks/quick-tokens.sh` go zamyka, a przy
trzech powierzchniach szklanych byłyby to trzy kopie jednej decyzji.

**`prefers-reduced-transparency: reduce` zamienia wszystkie trzy naraz** na `--solid` i zdejmuje
aurorę. Wymóg HIG. Zamiana jednej z trzech jest gorsza niż żadnej: okno miesza wtedy dwa
materiały dla jednego rodzaju powierzchni.

### Ikony: gramatyka, nie ozdoba

| Forma | Znaczy | Kto ją dostaje |
|---|---|---|
| węzły i krawędzie | rzecz, która **jest** grafem | Workflows |
| płyty | rzecz, która jest **zbiorem** | Agents, Skills, Memory |
| trójkąt | jedyna rzecz, która się **dzieje** | Run |

To niezmiennik 17 przeniesiony na ikonografię: nie rysujemy relacji tam, gdzie relacji nie ma.
Ikona z dwoma okręgami połączonymi linią obiecuje zależność — jeśli jej w danych nie ma, kłamie
dokładnie tak samo jak ozdobna krzywa między zakodowanymi na sztywno współrzędnymi.

---

## 6. Komponenty

Definicja komponentu = tokeny, nie hexy. Poniżej pełna lista v1.

### Promień bierze się z ROLI, nie z rozmiaru elementu

To jest cała reguła i nie ma od niej wyjątku. Pasmo ma cztery wartości i każda odpowiada na inne
pytanie o element:

| Promień | Rola | Co to jest |
|---|---|---|
| `--radius-sm` `9px` | **kontrolka** | przycisk, pole, uchwyt, kwadrat tożsamości, wiersz w pudełku |
| `--radius-md` `13px` | **pojemnik treści** | kafelek, karta kroku, panel, blok cytatu, ramka znaku w pustym ekranie |
| `--radius-lg` `18px` | **rzecz nad treścią** | modal, wymuszony wybór, panel na całą szerokość okna |
| `--radius-pill` | **rzecz, która jest odczytem** | chip, przełącznik, złącze, kropka koloru |

Piąta wartość jest piątą decyzją. Wartość arbitralna (`rounded-[11px]`) jest decyzją zapisaną tam,
gdzie nikt jej nie znajdzie.

### `button-primary`
`background: --accent` · `color: --bg` · `--t-ui` · `--radius-sm` · `padding 8px 16px` · `height 36px`
Active: `transform: scale(0.98)`. Focus: `outline: 2px solid --accent; outline-offset: 2px`.

### `button-secondary`
`background: --raised` · `color: --ink` · `border: 1px solid --line-strong` · `--t-ui` · `--radius-sm` · `height 32px`

### `button-quiet`
`background: transparent` · `color: --body` · `border: 1px solid --line` · `--t-ui` · `--radius-sm` · `height 28px`

### `button-danger`
Jak `button-secondary`, ale `border: 1px solid --fail-edge` · `color: --fail`. Bez wypełnienia — akcja
niszcząca nie ma być najbardziej rzucającym się w oczy elementem, ma być rozpoznawalna.

### `chip`
`padding 2px 8px` · `--t-label` · `height 20px` · **`--radius-pill`** · `border 1px solid {stan}-edge` ·
`background {stan}-soft` · `color {stan}`
Wariant neutralny: `--line` / `--raised` / `--muted` — bo nie każdy chip mówi o stanie. Skąd przyszła
umiejętność jest zwykłym faktem, a fakt pomalowany barwą stanu wygląda jak problem.

**Chipa poznaje się po kształcie, nie po nazwie klasy**, i to jest reguła dla wyroczni: barwę stanu
niosą trzy różne rzeczy i tylko jedna z nich jest chipem.

| Co | Obrys | Wypełnienie | Promień |
|---|---|---|---|
| chip | pełny, `{stan}-edge` | `{stan}-soft` | `--radius-pill` |
| `button-danger` | pełny, `--fail-edge` | **żadne** | `--radius-sm` |
| pasek błędu | `border-b` | `--fail-soft` | żaden |

Wypełniony przycisk jest najbardziej rzucającą się w oczy rzeczą na ekranie, a to miejsce należy do
akcentu: wypełnione ostrzeżenie konkuruje z akcją, po którą człowiek przyszedł.

### `field`
`background: --well` · `border: 1px solid --line-strong` · `color: --ink` · `--t-mono` · `--radius-sm` ·
`padding 7px 9px` · `height 32px` · `user-select: text`
Focus: `border-color: --accent`. Etykieta **nad** polem w `--t-label` / `--muted`, zdaniowo.

**To jest JEDNA klasa `.field` w `theme.css` i wszystkie pięć sekcji ją wołają.** Do 2026-08-19
wołały ją dwa miejsca, a cztery sekcje przepisywały ten sam wygląd ręcznie w dwunastu stałych —
i rozjechały się: w Agents obrys był `--line`, w Skills `--line-strong`. Jeden fakt, jedno miejsce
(niezmiennik 13); dwa opisy tego samego pola czyta się jak dwa różne stany, a nie jak dwa pola.

`user-select: text` jest częścią pola, nie ozdobą: `body` wyłącza zaznaczanie w całej aplikacji,
więc pole bez tej linii jest polem, z którego nie da się skopiować własnego wpisu.

Skupienie mieszka też w jednym miejscu: `.field:focus` daje obwódkę w akcencie, a globalny
`:focus-visible` obrys — obwódka odpowiada na „które pole jest aktywne", obrys na „gdzie jest
klawiatura". Dopisywanie tego narzędziem na każdym polu byłoby trzecią kopią podjętej decyzji.

**Etykieta zostaje nad polem, i to jest decyzja.** Inspektor dwukolumnowy z etykietą wyrównaną do
prawej (Ustawienia systemowe) potrzebuje szerokości, której ten panel nie ma: przy 330 px kolumna
etykiet szeroka na 90 px łamie „Give up after" i „File access" na dwa wiersze, a pole
wielowierszowe zostaje na 220 px. Dwie kolumny wracają razem z szerszym inspektorem.

### `stream-line` — wiersz w widoku pracy
Siatka `88px 1fr auto`. Padding `8px 16px`. Bez obramowania między wierszami — separacja przez odstęp.
Kolumna 1: nazwa agenta, `--t-mono-strong`, kolor agenta.
Kolumna 2: co robi, `--t-stream`, `--ink`.
Kolumna 3: liczba/czas, `--t-mono`, `--muted`.
Wiersz skończony: `opacity 0.55`.

**Strefa „teraz" ma dokładnie jeden żywy region na jeden fakt** (niezmiennik 13, limit 1):
pulsującą kropkę w `--live` z nadoczkiem `Now`. Fakt brzmi „coś się teraz dzieje", a **nie**
„ten konkretny agent pisze" — tego dane nie niosą, bo `NowRow` to `{ agent, text }`, a kto
pracuje, a kto czeka, jest treścią zdania. Wyprowadzanie tego w widoku przez szukanie słowa
`waiting` w napisie wymyśliłoby fakt (niezmiennik 17) i postawiło politykę „kto co robi” drugi
raz, w komponencie (niezmiennik 23). Kropki nie ma wcale, kiedy nie ma wierszy.

> **Reguła formy, powtórzona tu, bo tu się jej łamie.** `--live` występuje wyłącznie jako
> podkład aktywnego wiersza i jego obrys, aktywny segment paska, pulsująca kropka i kropka
> karty w tle. `--fail` wyłącznie jako glif `✕`, obrys chipa i lewa krawędź bloku błędu.
> Te dwie barwy różnią się odcieniem o ~13°, więc kształt jest jedyną rzeczą, która je
> rozróżnia — a kształt znaczący oba nie znaczy żadnego. Rozłączność jest sprawdzana statycznie.

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
Promień `--radius-md`. Zaznaczony: `border-color: --accent`. Złącze i kropka koloru agenta biorą
`--radius-pill`: złącze jest rzeczą, którą się chwyta, a nie kontrolką z etykietą.

### `loadout-strip` — pasek loadoutu
Patrz §2. Blok `32×8px`, odstęp `8px`, etykieta pod blokiem `--t-label` / `--muted`.

### `modal`
`background: --overlay` (nieprzejrzysty) · `border: 1px solid --line-strong` · `--radius-lg` ·
`--shadow-lg` · `max-width 640px` · `padding 24px`.
Tło za modalem: `rgba(6,9,11,0.72)`. Bez rozmycia, bez animacji wjazdu poza `opacity 120ms`.

### `empty-state`
Wyśrodkowane. Znak `◇` w ramce `1px dashed --line-strong` z promieniem `--radius-md`, zdanie
w `--ink`, jedno zdanie instrukcji w `--muted`, jeden przycisk podstawowy.
**Pusty ekran to zaproszenie do działania, nie komunikat o braku danych.**

**`data-empty` siedzi na elemencie, który niesie SAMO zdanie** — nie na opakowaniu ze znakiem,
zaproszeniem i przyciskiem. Znacznik jest czytany przez wyrocznie, a wyrocznia, która dla jednej
sekcji dostaje zdanie, a dla drugiej „◇ zdanie zdanie ＋ Create", milczy dokładnie tam, gdzie ma
krzyczeć. Do 2026-08-19 Workflows trzymał go wyżej niż pozostałe trzy sekcje.

Czynna kontrolka jest wymogiem tam, gdzie autorem jest **człowiek**: Agents, Skills, Workflows.
W Memory notatki pisze agent, więc przycisk dopisany tam „dla symetrii" byłby kontrolką bez
czynności — i to jest decyzja, nie przeoczenie.

---

## 7. Ruch

Jedna mikrointerakcja w całym systemie: `transform: scale(0.98)` na wciśnięciu.
Krzywa jest jedna i pochodzi z domu: `--transition 200ms cubic-bezier(0.32, 0.72, 0, 1)`,
wariant szybki `--transition-fast 130ms`.

Poza tym:
- Zmiana treści w strefie TERAZ: **bez animacji**. Tekst po prostu jest inny. Animowanie
  przepisania linii sprawia, że oko goni ruch zamiast czytać.
- Wejście nowej linii historii: `opacity 0 → 1` w `--transition-fast`. Bez przesunięcia.
- Pulsowanie: **tylko** kropka pracującego agenta, `opacity 1 → 0.35`, `1.4s steps(2)`.
  Skokowo, nie płynnie — płynne pulsowanie czyta się jak oddychanie i rozprasza.
- **Kropka gotowości w stopce nie pulsuje i nie jest akcentem.** Dostępność dostawcy nie jest
  ani interakcją, ani „teraz"; jest przygaszona i stoi w miejscu. Sufit z `ARCHITECTURE §7` daje
  **dwa** regiony animujące się od jednego zdarzenia i ta kropka nie ma prawa być jednym z nich.
- `--ease-spring` z domu **nie wchodzi**: nie mamy ani jednej powierzchni, która wjeżdża.

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
- [ ] nie używa `--accent` do niczego poza interakcją — „teraz" to `--live`
- [ ] `--live` i `--fail` nie dzielą w nim ani jednej formy
- [ ] nie dodaje piątego koloru semantycznego
- [ ] `prefers-reduced-motion` go nie psuje
- [ ] żaden tekst w nim nie jest w mono, jeśli nie jest wartością maszynową
