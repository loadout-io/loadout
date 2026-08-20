# T-72 — Procesy, ktore Loadout naprawde trzyma, widac w szynie i da sie je ubic

Zgloszenie wlasciciela 2026-08-20: „jak napisze aby cos odpalil jakas apke to chce miec tez po
prawej gdzie sa agenci info o procesach odpalonych itp, i po kliku moge tam wejsc".

**Dlaczego dzis tego nie ma i dlaczego nie da sie tego udac.** Aplikacja odpalona przez agenta
stoi w JEGO grupie procesow; Loadout widzi po niej wylacznie `Line::Ran` — wiersz o czynnosci
**zakonczonej** (`engine/line.rs`, pole `ok: bool`). Nosnika na „proces, ktory teraz zyje" nie
ma wcale. Kafelek zbudowany z wiersza `ran` bylby relacja, ktorej w danych nie ma
(niezmiennik 17), a przycisk „ubij" pod nim nie mialby czego ubic: nie zalozylismy tej grupy,
wiec nie mamy dowodu jej smierci (niezmiennik 6).

**Decyzja: proces zamawia sie komenda, a Loadout jest jego wlascicielem.** `/start <komenda>`
w wierszu wejscia. Uruchamia ja `supervisor::spawn` — ta sama droga, co kazdy inny proces tego
produktu: `process_group(0)`, `env_clear()` plus jawna lista, potoki czytane do EOF. Nie jest to
PTY i nie udaje terminala (decyzja D4 zostaje w mocy: kurowany strumien, PTY odlozone).

**Co jest juz gotowe, a czego brakuje.** `engine/drivers/command.rs` (T-55) robi 80% roboty:
komenda odpalona przez nas, wlasna grupa, potoki do EOF, werdykt nasz. Brakuje procesu
DLUGOZYJACEGO — bez sufitu 30 minut, z zywym strumieniem i zabiciem na zadanie — oraz rejestru,
z ktorego szyna bierze kafelki.

**Cicha porazka, przed ktora stoi ten kontrakt:** kafelek, ktory zostaje po martwym procesie.
„Running" przy komendzie, ktora zeszla dwie minuty temu, jest tym samym klamstwem, co widmowy
agent z T-66 — a ta fala pokazala, ze ta klasa wady wraca powierzchnia po powierzchni.

**Read first:**
`src-tauri/src/engine/drivers/command.rs` (`GIVE_UP_AFTER`, czytanie potokow, werdykt),
`src-tauri/src/engine/supervisor.rs` (`spawn`, `process_group`, eskalacja i `GroupProof`),
`src-tauri/src/engine/line.rs` (`Line::Ran` — dlaczego NIE jest nosnikiem zywego procesu),
`src/sections/run/rail/roster.ts` i `card.ts` (jak powstaje kafelek; po T-66 wiersze okna go
nie bija),
`docs/DECISIONS-LOCKED.md` D4 (kurowany strumien, PTY odlozone),
`AGENTS.md` niezmienniki 3, 6, 16, 17, 29.

## Niezmienniki, ktorych to dotyczy

- **6 — zabijamy grupe i dowodzimy, ze nie zyje.** Przycisk „stop" na kafelku wraca dopiero
  z `ESRCH`, nie po wyslaniu sygnalu.
- **3 — kod platformowy tylko w `supervisor.rs`.** Ten kontrakt nie dokłada ani jednego
  `#[cfg(...)]` poza tamtym plikiem.
- **17 — zadnej relacji, ktorej nie ma w danych.** Kafelek istnieje dokladnie tak dlugo, jak
  proces.
- **29 — zdanie tam, gdzie je widac.** Kryterium AC-4 sadzi kafelek na prawdziwym froncie, nie
  wartosc zwrocona przez rejestr.

## Szkielet, bez ktorego `before` nie jest czerwone

Rust: sygnatury rejestru i startu dlugozyjacej komendy z `todo!()`. TypeScript:
`src/sections/run/rail/processes.ts` jako pusty szkielet rzucajacy `throw new Error('not
implemented')`.

## Kryteria akceptacji

## AC-1 Dlugozyjacy proces nalezy do nas i da sie go ubic z dowodem
check: cargo test --test it started_process_is_ours::
expect: (\d+) passed

Asercje: (a) proces startuje przez `supervisor::spawn`, wiec ma **wlasna grupe** — test czyta
pgid i porownuje z naszym; (b) zyje po powrocie komendy, ktora go zamowila (to jest cala roznica
wobec kroku „sprawdz"); (c) `stop` na nim wraca dopiero z `ESRCH` dla calej grupy; (d) sufit
30 minut z kroku sprawdzajacego go NIE dotyczy — proces dlugozyjacy konczy sie na zadanie albo
razem z oknem; (e) kontrola przeciw pustemu przejsciu: test sprawdza, ze przed `stop` grupa
NAPRAWDE zyla (`kill(-pgid, 0)` bez bledu).

*Slaba asercja:* sprawdzenie, ze `stop` wrocilo bez bledu. Przechodzi dla implementacji, ktora
wysyla sygnal i nie czeka — a wtedy proces zyje dalej i pali maszyne. Rozroznia to (c) razem z (e).

## AC-2 Zamkniecie okna nie zostawia sierot
check: cargo test --test it started_processes_die_with_the_window::
expect: (\d+) passed

Asercje: (a) dwa zamowione procesy zyja jednoczesnie i maja **rozne** grupy; (b) zamkniecie okna
konczy oba, kazdy z dowodem `ESRCH`; (c) rejestr jest po tym pusty; (d) kontrola: test sprawdza,
ze oba naprawde wstaly, inaczej dowodzi smierci czegos, czego nie bylo. Powod stoi w `recovery.rs`:
proces, ktory przezyje Loadouta, przechodzi pod PID 1 i pracuje dalej.

*Slaba asercja:* test na jednym procesie. Przechodzi dla implementacji trzymajacej JEDEN uchwyt
— czyli dla tej, w ktorej drugi `/start` osieroca pierwszy. Rozroznia to (a).

## AC-3 Kafelek istnieje dokladnie tak dlugo, jak proces
check: npx --no-install vitest run src/sections/run/rail/processes-are-not-agents.test.ts
expect: (\d+) passed

Czysta funkcja skladajaca kafelki z rejestru. Asercje: (a) proces zywy daje jeden kafelek,
odrozniony od kafelka agenta (inna grupa w szynie, nie inny kolor — kolor jest tozsamoscia,
nie stanem [DESIGN §3]); (b) proces, ktory zeszedl, **nie daje kafelka wcale** — to jest ta sama
regula, ktora zamknelo T-66 i T-67; (c) kafelek niesie komende co do znaku, bo to ona jest jego
nazwa — zmyslona etykieta byla by relacja, ktorej nie ma w danych; (d) agenci i procesy nie
mieszaja sie w jednej liscie: kafelek agenta zostaje tam, gdzie byl; (e) kontrola: fikstura ma
po jednym z kazdego rodzaju plus jeden proces zeszly.

*Slaba asercja:* `toHaveLength(1)` na jednym zywym procesie. Przechodzi dla implementacji, ktora
zostawia kafelki po martwych — czyli dla „Running" nad komenda, ktora zeszla dwie minuty temu.
Rozroznia to (b).

## AC-4 Czlowiek widzi kafelek po wpisaniu komendy i moze w niego wejsc
check: npx --no-install vitest run e2e/tests/started-process-shows-up.spec.ts
expect: (\d+) passed

Niezmiennik 29: rejestr, ktory zna proces, nie jest dowodem, ze czlowiek go widzi. Asercje na
prawdziwym froncie: (a) po wpisaniu `/start` z komenda i naciśnięciu Enter w szynie pojawia sie
kafelek niosacy te komende; (b) wiersz echa tej komendy stoi w strumieniu (T-58 AC-2 nie ma sie
zepsuc); (c) klikniecie kafelka otwiera jego wyjscie — cos widocznego zmienia sie w dokumencie;
(d) kontrola przeciw pustemu przejsciu: przed wpisaniem komendy tego kafelka NIE ma.

*Slaba asercja:* sprawdzenie, ze cokolwiek pojawilo sie w szynie. Przechodzi, gdy kafelek
narysuje sie dla agenta z biegu, ktory akurat idzie. Rozroznia to (a) razem z (d).

<!-- OWNS
src-tauri/src/engine/drivers/command.rs
src-tauri/src/commands/mod.rs
src-tauri/src/commands/processes.rs
src-tauri/src/ipc.rs
src-tauri/commands.golden.txt
src-tauri/tests/it/main.rs
src-tauri/tests/it/started_process_is_ours.rs
src-tauri/tests/it/started_processes_die_with_the_window.rs
src/sections/run/rail/processes.ts
src/sections/run/rail/rail.tsx
src/sections/run/rail/processes-are-not-agents.test.ts
src/sections/run/entry/entry.tsx
src/sections/run/io.ts
src/sections/commands-wired.test.ts
e2e/tests/started-process-shows-up.spec.ts
-->
