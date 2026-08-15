# Loadout — decyzje zablokowane przez człowieka

Te cztery decyzje podjął Jakub 2026-08-15, przed syntezą. **Nie podważaj ich w planach ani w ADR-ach.**
Research może dodać szczegóły implementacyjne, ale nie zmienia kierunku.

---

## D1 — Wygląd: ciemny terminal + dyscyplina Apple

Baza wizualna: paleta i layout z `redesign poprzedniego prototypu.dc.html` (kopia: `docs/design/poprzedni prototyp-redesign.dc.html`).
Reguły kompozycji: z Apple DESIGN.md — ale **zasady, nie tokeny** (Apple to system marketingowy,
17px body i 80px padding sekcji nie mają sensu w gęstym narzędziu pracy).

Co bierzemy z Apple:
- **Jeden kolor akcentu.** Wszystko, co interaktywne, ma ten sam kolor. Nie ma drugiego koloru marki.
- **Zero gradientów dekoracyjnych.** Zero cieni na chrome. Głębia = zmiana koloru powierzchni.
- **Typografia zamiast ozdób.** Hierarchia z rozmiaru i wagi, nie z ramek i tła.
- **Waga 500 nie istnieje.** Drabinka: 400 / 600 / 700.
- **Stan aktywny = `transform: scale(0.98)`.** Jedna mikrointerakcja w całym systemie.
- **Nigdy nie dokumentujemy hover.** Tylko default i active/pressed.

Co bierzemy z redesign poprzedniego prototypu:
- `#06090b` tło · `#0d1216` panel · `#141b20` raised · `#040708` well
- `#e8eff1` ink · `#cbd6d9` body · `#a3b1b5` muted · `#212a2f` line · `#5a6d76` line-control
- Inter (UI) + SFMono/Roboto Mono (dane, identyfikatory, czas)
- `border-radius: 2px` wszędzie. Kwadratowo, terminalowo.
- Kolory statusu jako **semantyka, nie dekoracja**: `#6ee0b0` ok · `#ffb45b` czeka na ciebie · `#ff8f9f` błąd · `#c6a8ff` człowiek

Akcent: `#6ee0b0`.

Pełna specyfikacja: `docs/design/DESIGN.md`.

---

## D2 — Nowe repo, czysty start

`~/Projects/Loadout` od zera. poprzedni prototyp jest **źródłem pomysłów, nie kodu**.

Konsekwencja, której trzeba pilnować: kiedy w planie pojawia się „przenieśmy X z poprzedni prototyp",
to znaczy „przeczytajmy jak X działa i napiszmy X od nowa, mniejsze". Kopiuj-wklej crate'a jest zakazany.

Powód: poprzednia wersja umarła na złożoność. Zaciąganie crate'ów zaciąga ontologię, która tę złożoność stworzyła.

---

## D3 — Claude Code **i** Codex w v1 — w aplikacji i w harnessie

*Zrewidowane 2026-08-15 po pierwszej rundzie pytań. Pierwotnie brzmiało „tylko Claude w v1";
użytkownik to cofnął. Dwóch vendorów jest teraz zakresem v1.*

**W aplikacji: pełna dowolność.** Każdy agent może mieć dowolnego vendora i dowolny model
z dowolnym poziomem wysiłku — np. Codex na `sol` z effortem `max`. Wybór vendora jest polem
w kreatorze agenta, nie ustawieniem globalnym.

**W harnessie budującym Loadout: wybieralna para pisarz/recenzent.** Wszystkie cztery kombinacje
muszą działać:

| Pisze | Recenzuje | Uwaga |
|---|---|---|
| Codex | Claude | |
| Claude | Codex | |
| Claude | Claude | inny model + rola recenzenta |
| Codex | Codex | inny model + rola recenzenta |

Domyślnie: **cross-vendor**, bo według researchu każdy realny defekt w pierwszej wersji spreadsheet
znalazł właśnie recenzent innego vendora na **zielonej bramce** (`docs/working-with-ai.md`,
raport `06-spreadsheet-harness.md`). Same-vendor jest wspierany, ale to słabszy tryb i tak ma być opisany.

Konsekwencje:

- `AgentDriver` ma **dwie** implementacje od początku: `ClaudeDriver` i `CodexDriver`. Trait z jedną
  implementacją to trait wymyślony; dwie sprawiają, że abstrakcja jest prawdziwa.
- `ship-task.sh` przyjmuje `--agent <vendor>` i `--reviewer <vendor>`, obie flagi niezależne.
- Recenzent **nigdy nie może zatwierdzić ani zablokować.** Schemat odpowiedzi ma `verdict ∈ {concern, none}`
  i `findings` z `maxItems: 6` — strukturalnie nie ma czego zatwierdzić.
- **Ryzyko operacyjne:** research odnotował, że Codex był bez kredytów do 2026-08-20 `[ran]`.
  Harness musi traktować „recenzent niedostępny" jako `exit 0` z notatką, nigdy jako czerwone.
  Niedostępność ≠ zepsute.

---

## D5 — Interfejs po angielsku, dokumentacja po polsku

UI: angielski. Przyciski, etykiety, komunikaty błędów, puste stany — `Run`, `Create`,
`Needs your answer`, `Couldn't reach the API`.

Powody: makieta jest po angielsku, tabela żargon→prosty-język z researchu (55 wierszy) jest po angielsku
i jest wiążąca, a część terminów po polsku brzmi gorzej niż po angielsku.

Dokumentacja, ADR-y, pliki tasków, komentarze w kodzie wyjaśniające *dlaczego*: polski.

Sprawdzacz słownictwa (`checks/quick-vocabulary.sh`) skanuje **wyłącznie tekst widoczny dla użytkownika**
i egzekwuje angielską tabelę z `00-SYNTHESIS.md` §2.2.

---

## D4 — Kurowany strumień, PTY jako escape hatch

Agenci są odpalani z pipe'owanym stdio w trybie strumienia JSON. Renderujemy **własny widok**, nie emulator terminala.

- Widok wygląda jak terminal, ale każda linia jest naszą decyzją projektową, nie wyjściem procesu.
- Domyślnie zwinięte: wszystko poza tym, co się właśnie dzieje i co wymaga uwagi.
- Prawdziwe PTY: **odłożone**, jako przycisk „otwórz shell tutaj". Nie w v1.

Wzorzec docelowy (z odpowiedzi użytkownika):

```
❯ /plan zbuduj parser CSV

  ✓ plan gotowy · 4 kroki        [rozwiń]

  Forge   pisze  src/parser.rs
  Needle  testy  12 ✓  0 ✗
  Rivet   czeka  na Needle

❯ _
```

To jest docelowa gęstość informacji. Jeśli widok robi się gęstszy niż to — jest źle.

---

## D6 — Czym jest edytor workflow, a czym nie jest

*Zapisane 2026-08-15 na wyraźne polecenie użytkownika: „to jest ultra ważne".*

**Edytor workflow robi pięć rzeczy i tylko te pięć:**

1. **Kolejność i zależności** — kto po kim, co równolegle.
2. **Kontrola, który model pracuje** na danym kroku.
3. **Odpalenie kilku agentów naraz.**
4. **Synteza ich wyników** w jeden — krok, który czyta wiele przekazań i produkuje jedno.
5. **Dzielenie kontekstu i analiza z poziomu orchestratora** — orchestrator widzi, co wyprodukowali
   pozostali, i może na tym pracować.

**Czym nie jest: powtórką funkcji vendorów.** Nie konkurujemy z `--agents` Claude'a, jego skillami,
subagentami ani z czymkolwiek, co Anthropic albo OpenAI dowiozą w przyszłym miesiącu.

### Reguła wynikowa

**Wszystko, co vendor wprowadzi, konfigurujemy per agent — nigdy jako nowy typ węzła.**

Nowa flaga u Claude'a to nowe pole w definicji agenta. Nowy tryb u Codeksa to nowe pole w definicji
agenta. Liczba rodzajów kafelka na płótnie zostaje **dwa** (krok i punkt kontrolny) niezależnie od
tego, ile funkcji dołożą vendorzy.

### Konsekwencja projektowa, bez której ta reguła jest pustym hasłem

Definicja agenta i nadpisanie w węźle muszą mieć **przelotkę na opcje vendora** — surowe pole, które
leci prosto do argv albo do konfiguracji, bez pośrednictwa naszego modelu danych:

```jsonc
{
  "runsWith": "claude",
  "model": "opus",
  "vendorOptions": {                    // przelotka — Loadout nie interpretuje zawartości
    "claude": { "--jakas-nowa-flaga": "wartosc" },
    "codex":  { "model_reasoning_summary": "detailed" }
  }
}
```

Bez tego każda nowa flaga vendora wymaga **wydania Loadouta**. Z tym — wymaga wpisania jednej linii
w formularzu agenta, tego samego dnia, w którym vendor ją ogłosi.

Dwa ograniczenia, żeby przelotka nie stała się dziurą:

- **Kolizja jest odmową, nie nadpisaniem.** Jeśli przelotka podaje flagę, którą Loadout ustawia sam
  (`--session-id`, `--output-format`, `--permission-mode`, `-C`, `-s`), zapis workflow jest odrzucany
  z nazwaniem flagi. Cicha wygrana jednej ze stron to najgorszy możliwy wynik.
- **Przelotka nie omija dialu bezpieczeństwa.** Pole „co agent może zrobić z plikami" jest tłumaczone
  przez nas na flagi vendora; przelotka nie może go podnieść.

### Dlaczego to jest fosa, a nie wygoda

Vendorowy runner grafów orkiestruje **własnych** agentów. Edytor Loadouta orkiestruje **przez
vendorów**, z wyborem modelu na krok i syntezą wyników. Żaden vendor tego nie zbuduje, bo nie ma
w tym interesu. Trwałość edytora bierze się z pozycji cross-vendor — nie z płótna, które jest łatwe
do skopiowania. To podnosi rangę D3: dwóch vendorów w v1 to warunek istnienia przewagi, nie komfort.

---

## Reguły nazewnictwa (z wymagań użytkownika)

Cała aplikacja mówi **prostym językiem, bez żargonu technicznego**. Nazwy przycisków: `Utwórz`, `Edytuj`, `Uruchom`.

Zakazane w UI: `ledger`, `work item`, `claim`, `rail`, `DAG`, `policy kernel`, `binding`, `artifact receipt`,
`plan.approval_requested`, `WI-31`, `A#8`, `authority`, `projection`, `durable record`.

Tabelę tłumaczeń żargon → język ludzki dostarcza `docs/research/projects/00-SYNTHESIS.md`.
Ta tabela jest **wiążąca** dla nazw w UI i dla nazw w kodzie frontendu.
