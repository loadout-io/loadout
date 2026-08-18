# T-42 — Umiejetnosc napisana tutaj wchodzi tym samym potokiem co link

**Dwa zdania na ekranie obiecuja kontrolke, ktorej nie ma, a jedyna istniejaca droga wejscia
kasuje biblioteke, kiedy plik nie ma nazwy.** Sekcja Skills umie przyjac WYLACZNIE adres:
`review_skill(url)` i nic wiecej. Czlowiek, ktory chce napisac umiejetnosc sam, czyta na pustym
ekranie zaproszenie i nie ma czego kliknac.

Zmierzone 2026-08-19 na wyladowanym trunku:

```
src/sections/skills/index.tsx:169   <p>Paste a link, or write one yourself.</p>
docs/mockup/index.html:712          Paste a link, or write it yourself. One flow either way.
src-tauri/commands.golden.txt       24 nazwy komend; ani jedna nie bierze TRESCI umiejetnosci
docs/research/topics/T5-...:585     „Create form: name / when-to-use / what-to-do" — Ships in MVP
```

Formularz jest zaprojektowany w badaniu (T5 §8.3: trzy pytania, slug pokazany raz, bez zargonu)
i stoi na liscie MVP od 2026-08-15. Nie powstal, bo zadne kryterium o niego nie poprosilo — ta
sama rodzina, co cztery puste ekrany przed T-26 i martwy `wireChannel` przed T-38.

## Dlaczego to nie jest „dodaj formularz i zawolaj install"

Dwie rzeczy stoja na drodze i obie sa mierzalne.

**PIERWSZA: dzisiejsza droga wejscia kasuje katalog zbudowany z nieufnej nazwy, bez ani jednej
walidacji.** `review_skill_inner` liczy sciezke z pola `name` z front-mattera i natychmiast robi
na niej `remove_dir_all`:

```
src-tauri/src/commands/skills.rs:350   let canonical = library.join(SKILLS_DIR).join(&import.skill.name);
src-tauri/src/commands/skills.rs:351   gone(&canonical)?;
src-tauri/src/skills/ingest.rs:1114    fn skill_from(...) -> Skill { ..Skill::default() }   // name: ""
```

`from_folder` nie waliduje nazwy ani razu (ingest.rs:984-1013), a `Skill::default()` daje
`name: ""`. Wiec `SKILL.md` bez pola `name:` daje `<library>/skills/` i `gone()` kasuje **wszystkie
kopie kanoniczne razem z `installed.json`**; `name: ../../x` kasuje poza biblioteka. Dzis trafia
to tylko wklejony link, wiec zdarza sie rzadko. Formularz zamienia nazwe z front-mattera w rzecz,
ktora czlowiek wpisuje palcami — czyli w przypadek zwykly. Odmowa musi padac PRZED pierwszym
`remove_dir_all`, a wzor odmowy stoi obok, w `delete_skill_inner` (skills.rs:420).

**DRUGA: potok jest kolejnoscia, nie zbiorem funkcji.** Naglowek `ingest.rs` nazywa ciche
porazki po imieniu: skan na tekscie surowym przy zapisie tekstu znormalizowanego, i skan po
rozbiciu na pola, kiedy `hooks:` z front-mattera przejezdza bokiem. Dlatego tekst napisany tutaj
ma pojsc DOKLADNIE ta sama droga: zloz plik przez `place::emit`, zapisz go, przeczytaj przez
`ingest::from_folder`. Formularz, ktory buduje `Skill` wprost z trzech pol, omija `review()`
w calosci — a wtedy znikaja R1 (znaki niewidzialne, komentarze HTML) i R5 (`allowed-tools`,
`hooks`), bo one czytaja TEKST PLIKU, nie strukture. Zaden test tego nie zauwazy, dopoki
kryterium nie porowna zbioru znalezisk z tym, co daje rdzen na tych samych bajtach.

Czyli zadanie polega na **doprowadzeniu tekstu do istniejacego potoku i zamknieciu drogi, ktora
kasuje cudze pliki**, nie na napisaniu drugiego potoku obok.

**Read first:**
`src-tauri/src/commands/skills.rs:326-391` (jak wchodzi link i co robi instalacja) ·
`src-tauri/src/skills/ingest.rs:984-1013` (`from_folder` — caly potok w jednej funkcji, bez sieci) ·
`src-tauri/src/skills/place.rs:277-326` (`emit` — szesc pol i lista zdjetych) ·
`src-tauri/src/skills/place.rs:410-471` (`plan` — walidacja i kolizje, NIC nie zapisuje) ·
`src-tauri/src/skills/mod.rs:139-238` (komunikaty walidatora, slowo w slowo) ·
`docs/research/topics/T5-skill-portability.md` §8.3 (trzy pytania, slug, „no vendor checkboxes") ·
`src/sections/skills/index.tsx` (panel z jednym polem) · `docs/mockup/index.html:704-735` ·
`AGENTS.md` niezmienniki 4, 13, 16, 20, 21, 23.

## Kto to robi

- **Agent:** `rust-core` na `commands/skills.rs`, potem `react-ui` na sekcji — jeden worktree,
  dwa kroki, jedna bramka.
- **Druga opinia:** inny vendor niz pisarz (D3); recenzentowi powiedz wprost, zeby atakowal AC-1
  pytaniem „czy da sie to przejsc bez wolania `review()`".
- **Artefakty biegu:** `runs/T-42/`

## Zalezy od

**Fala z 2026-08-18, ktora musi najpierw wyladowac w trunku** (`delete_skill`, `list_skills`
czytajace dysk, `useSkills.load` z wolajacym). Ten kontrakt opisuje stan PO niej i wymienia
`commands/skills.rs`, `state/skills.ts` oraz `commands-wired.test.ts` w OWNS — te same pliki,
ktore tamta fala zmienia. Galaz odbita przed jej wyladowaniem dostanie pewny konflikt w kazdym
z nich.

## Co to zadanie posiada

- `src-tauri/src/commands/skills.rs` — nowa droga wejscia dla tresci, walidacja nazwy przed
  dotknieciem dysku, zapis pochodzenia i odczyt pochodzenia w `list_skills_inner`.
- `src-tauri/src/ipc.rs` — **waski mandat**: jedna nowa skorupa `#[tauri::command]` w bloku
  „SKORUPY KOMEND" i jeden wiersz w `generate_handler!`. Dwie linie ciala, jak `delete_skill`
  (ipc.rs:695-698).
- `src-tauri/commands.golden.txt` — **waski mandat**: jedna nowa nazwa, alfabetycznie. Ani jednej
  istniejacej nie wolno usunac ani przestawic.
- `src/sections/skills/index.tsx` — drugie wejscie w tym samym panelu, pod tym samym przyciskiem.
- `src/sections/skills/review-card.tsx` — **waski mandat**: plakietka pochodzenia przestaje byc
  wpisana na sztywno (dzis `review-card.tsx:90` renderuje ja bezwarunkowo, ignorujac
  `item.fromTheInternet`). Zadnej innej zmiany w tym pliku.
- `src/sections/skills/io.ts`, `src/state/skills.ts` — krawedz i akcja magazynu.
- `src/sections/commands-wired.test.ts` — **waski mandat**: JEDEN nowy wiersz w tabeli `WIRES`.
  Ani jednego istniejacego nie wolno zmienic ani usunac; `what` musi byc dokladnie nazwa nowego
  eksportu z `io.ts`, bo test porownuje tabele z eksportami przez `toEqual`.
- `src-tauri/tests/it/main.rs` — **waski mandat**: ten plik masz w OWNS WYLACZNIE po to, zeby
  dopisac dwa wiersze `mod skills_author_pipeline;` i `mod skills_author_origin;` w porzadku
  alfabetycznym. Zadnej innej zmiany; bez tych wierszy pliki kompiluja sie do niczego,
  a zestaw wyglada jak przeszly (pilnuje tego `checks/quick-tests-listed.sh`).
- 4 pliki testow wymienione przy `check:`.

**Czego to zadanie NIE dotyka:** `src-tauri/src/skills/place.rs` i `ingest.rs` (T-18 i T-19 —
konsumujesz je, nie przepisujesz), `src-tauri/src/skills/mod.rs`, `src/sections/skills/mounted.test.tsx`
(T-26), `src/sections/skills/review-card.test.tsx` i `src/state/skills.test.ts` (T-19),
`src/sections/read-paths-populate.test.ts` (T-38). Cztery ostatnie zamrazaja dzisiejszy ksztalt
`fromTheInternet` jako `boolean` i wszystkie ich fikstury maja `true` tam, gdzie sprawdzaja
plakietke — dlatego pole zostaje `boolean`em, a zmienia sie wylacznie to, skad bierze sie jego
wartosc. Zamiana go na enum zaczerwieni cztery cudze pliki i jest poza tym zadaniem.

## Niezmienniki

- **23 — polityka w jednym rdzeniu, adaptery po piec linii.** *Jak sie lamie po cichu:* nowa droga
  buduje `Skill` z trzech pol i pomija `ingest::review`. Wszystko dziala, znaleziska nie powstaja,
  a plik z ukrytym tekstem instaluje sie jako czysty.
- **4 — pliki sa prawda, SQLite jest indeksem.** Pochodzenie musi dac sie odczytac z dysku po
  restarcie. Dzis odpowiada na to obecnosc kopii kanonicznej (skills.rs:246) — po tym zadaniu ta
  przeslanka przestaje byc prawdziwa, wiec zapis musi byc jawny.
- **20 — test sprawdza zachowanie, nie obecnosc stringa.** *Jak sie lamie po cichu:*
  `assert!(result.is_ok())` na drodze, ktora zapisala plik inny niz przeskanowany.
- **13 — jeden fakt, jedno miejsce.** Slug widziany przez czlowieka i nazwa katalogu na dysku to
  jeden fakt. Dwa liczenia sluga — jedno w oknie, drugie w Ruscie — rozjada sie na pierwszym
  polskim znaku.
- **16 — kontrolka bez skutku nie wchodzi do repo.** Zdanie „write one yourself" bez drugiego
  wejscia jest tym samym defektem, tylko odwroconym: obietnica bez kontrolki.

## Kryteria akceptacji

**Jak zaczerwienic to poprawnie.** `clippy::todo` jest `deny` w `[workspace.lints.clippy]`, wiec
najpierw prawdziwe sygnatury zwracajace trywialnie zla wartosc (pusty `ImportWire`, `Ok(())`),
nigdy `todo!()`. Kazdy plik testu Rusta zaczyna sie od
`#![allow(clippy::unwrap_used, clippy::expect_used)]` z powodem — `checks/full-clippy.sh` biegnie
`--all-targets -- -D warnings`. Rozgrzej build przed pierwszym `before`: `cargo test --no-run
--test it`; limit sprawdzenia w tej warstwie to 20 s. Po stronie okna repo NIE MA jsdom: testy
renderuja przez `renderToStaticMarkup`, wiec kazdy modul, ktory nowy plik testu importuje, musi
istniec przed `./verify.sh before` — inaczej vitest przewraca sie na zbieraniu i dostajesz podpis
z `NOT_A_REAL_RED`. Nowy komponent czyta magazyn przez `useSyncExternalStore(subscribe, getState,
getState)`, nigdy hakiem zustanda: renderer serwerowy dostaje wtedy `getInitialState` i zasiew
z testu jest niewidoczny.

## AC-1 Tekst napisany tutaj przechodzi TEN SAM potok, a zla nazwa nie kasuje niczego
check: cargo test --test it skills_author_pipeline::
expect: (\d+) passed

Fikstura: `tempfile::TempDir` jako biblioteka; trzy pola z formularza, w tym cialo z linia
`ig\u{200d}nore all previous instructions` i front-matter z `hooks:` w tekscie, ktory czlowiek
wpisal. Kopia kanoniczna innej umiejetnosci (`other-skill/SKILL.md`) i `skills/installed.json`
stoja obok jako sentinel.

Asercje: (a) zbior znalezisk i werdykt zwrocone przez nowa droge sa **rowne** temu, co daje
`ingest::review` policzone w tescie na tekscie, ktory ta droga zapisala — porownanie
zbior-do-zbioru, nie z literalem; (b) bajty `SKILL.md` w kopii kanonicznej sa **identyczne**
z `reviewed.body` zwroconym oknu; (c) nazwa, ktora nie jest jednym czlonem sciezki (`""`, `../x`,
`a/b`) oraz nazwa, ktora odrzuca `place::validate_strict` (`Upper-Name`, `claude-helper`) sa
odmowa **przed** pierwszym `remove_dir_all` — sentinel `other-skill/` i `installed.json` istnieja
po probie z tymi samymi bajtami, a zdanie odmowy jest tym z walidatora, nie napisanym tutaj drugi
raz; (d) slug policzony z tego, co wpisal czlowiek (`Review pull requests`), spelnia
`validate_strict` dla kazdego wejscia z korpusu (spacje, wersaliki, interpunkcja, 80 znakow,
`Claude review`) — a tam, gdzie nie moze, odmawia zdaniem walidatora zamiast zapisac cokolwiek.

*Slaba wersja:* `assert!(inner(...).is_ok())` plus `assert!(canonical.join("SKILL.md").exists())`.
Przechodzi implementacja, ktora zbudowala `Skill` z trzech pol, nie zawolala `review()` ani razu
i zapisala plik zlozony po skanie. Rozstrzyga porownanie z `ingest::review` na TYCH SAMYCH
bajtach — jeden rdzen daje jeden zbior znalezisk, dwa rdzenie nie daja nigdy.

## AC-2 Pochodzenie jest prawda dla trzech zrodel i przezywa restart
check: cargo test --test it skills_author_origin::
expect: (\d+) passed

Fikstura: jedna biblioteka, trzy umiejetnosci w katalogach vendorow: jedna z linku (kopia
kanoniczna + zapisane pochodzenie), jedna napisana tutaj, jedna wlozona do
`~/.claude/skills/<name>/` przez kogos innego (bez kopii kanonicznej). Czwarta: kopia kanoniczna
bez zapisu pochodzenia — umiejetnosc z czasow przed tym zadaniem.

Asercje: (a) `list_skills_inner` czytane **z dysku** oddaje `fromTheInternet` prawdziwe dla
wszystkich czterech: link tak, napisana tutaj nie, cudzy katalog nie; (b) kopia kanoniczna bez
zapisu pochodzenia jest **z internetu**, nie „napisana tutaj" — do tego zadania kopie kanoniczne
powstawaly wylacznie w `review_skill_inner`, wiec ostrozny kierunek jest jedynym uczciwym
(ta sama regula, co `DeepScan::Unavailable` i `Discovery::Unknown`: nieobecnosc dowodu nie jest
dowodem); (c) `skills/installed.json` ma po zapisie pochodzenia te same wpisy co przed —
`place::write_sidecar` odtwarza cala strukture z samego zbioru sciezek (place.rs:673-689), wiec
pochodzenie nie moze mieszkac w tym pliku; (d) zapis pochodzenia nie stoi wewnatrz katalogu
umiejetnosci — po instalacji w katalogach vendorow leza dokladnie te pliki, co w kopii
kanonicznej, ani jednego wiecej (`bundled_files` zabiera kazdego sasiada `SKILL.md`).

*Slaba wersja:* asercja, ze umiejetnosc napisana tutaj ma `fromTheInternet == false`. Przechodzi
implementacja, ktora tylko odwrocila stala na nowej drodze, a lista dalej wyprowadza znacznik
z istnienia kopii kanonicznej — czyli klamstwo zostaje w liscie, a naprawiona jest tylko karta.
Rozstrzyga: cztery zrodla w jednym tescie, wszystkie czytane przez `list_skills_inner`.

## AC-3 Drugie wejscie ISTNIEJE, jest w tym samym panelu i OPUSZCZA OKNO
check: npx --no-install vitest run src/sections/skills/write-it-yourself.test.tsx
expect: (\d+) passed

Fikstura: `renderToStaticMarkup` na calym `<SkillsScreen>`, magazyn zasiany `setState` przed
renderem, granica `@tauri-apps/api/core` podmieniona atrapa liczaca wywolania (wzor:
`src/sections/commands-wired.test.ts`).

Asercje: (a) pusty ekran ma **dokladnie jeden** `data-create` — drugie wejscie mieszka w panelu,
ktory ten przycisk otwiera, a nie obok niego (te liczbe zamraza
`src/sections/skills/mounted.test.tsx` i to jest jej cala tresc: jedno zaproszenie, nie dwa);
(b) otwarty panel niesie **oba** wejscia: pole na adres i trzy pytania, kazde z etykieta;
(c) oddanie trzech pytan wysyla do Rusta wywolanie nazwa ze `src-tauri/commands.golden.txt` —
nazwa czytana z tego pliku, nie wpisana w test — a wszystkie trzy wartosci sa w argumentach;
(d) odmowa z tamtej strony zostawia to, co czlowiek wpisal, w polach i stawia na ekranie zdanie,
ktore przyszlo z Rusta: tekst tracony przy odmowie to ten sam defekt co cisza, tylko drozszy.

*Slaba wersja:* asercja, ze w markupie sa trzy `<input>`. Przechodzi na formularzu, ktory nie
wola niczego — a to jest dokladnie dzisiejszy stan `answer()` z T-41, tylko w innej sekcji.
Rozstrzyga policzenie wywolan na atrapie granicy i porownanie argumentow z tym, co wpisano.

## AC-4 Karta przestaje twierdzic, ze wie, skad to przyszlo
check: npx --no-install vitest run src/sections/skills/origin-is-not-a-guess.test.tsx
expect: (\d+) passed

Fikstura: dwa `Import`y rozniace sie **wylacznie** polem `fromTheInternet`, oba renderowane tym
samym komponentem karty.

Asercje: (a) karta dla umiejetnosci napisanej tutaj nie niesie zdania o internecie; (b) karta dla
tej z linku niesie je dalej — bez tej polowy asercja (a) przechodzi na komponencie, ktory przestal
mowic cokolwiek; (c) kontrola przeciw pustej asercji: obie karty niosa nazwe i cialo, wiec (a)
mowi o karcie, ktora istnieje i cos pokazuje; (d) zdanie o tym, gdzie umiejetnosc wyladuje
(`WHERE_IT_LANDS`), stoi nad decyzja w obu przypadkach — ostrzezenie widoczne wczesniej niz
decyzja nie jest ostrzezeniem.

*Slaba wersja:* `expect(markup).not.toContain('From the internet')` na jednym `Import`. Przechodzi
na karcie, ktora zdjela plakietke calkiem — czyli umiejetnosc z sieci przestaje sie roznic od
napisanej reka, a plakietka zastepuje w v1 podpisy i weryfikacje pochodzenia. Rozroznia: dwa
`Import`y w jednym tescie, rozniace sie jednym polem.

## Swiadomie poza zakresem

- **„Napisz mi go" — draft od agenta.** T-43. Ta droga wejscia jest jego warunkiem: draft ladujac
  w tych samych trzech polach idzie potem dokladnie tym potokiem, co tekst wpisany reka.
- **Wybor „ten projekt / wszedzie".** T-44. Tutaj zakres zostaje globalny, dokladnie jak dzis
  (`Scope::Global`, `global_roots`).
- **Edycja zapisanej umiejetnosci.** T5 §11 ma to w MVP, ale jest to osobna droga (odczyt kopii
  kanonicznej do formularza) i osobne zadanie.
- **Lista pol zdjetych przez `emit`.** `emit` ja zwraca i **nikt jej nie czyta** (`let (doc, _) =
  emit(skill)`, place.rs:545). To jest znalezisko dla czlowieka, nie robota tego zadania: pole
  `hooks:` znika z pliku bez ani jednego zdania na ekranie, a przy tekscie pisanym przez model
  (T-43) bedzie to zdarzenie czeste, nie rzadkie.

**Znalezisko, ktore to zadanie naprawia po drodze, i o ktorym czlowiek ma wiedziec.** Kasowanie
`<library>/skills/` przez `SKILL.md` bez pola `name:` jest defektem ISTNIEJACYM DZIS na trunku,
osiagalnym z okna: wklejasz link do pliku bez nazwy i tracisz wszystkie kopie kanoniczne razem
z `installed.json`. AC-1 (c) zamyka to na obu drogach, bo obie licza te sama sciezke.

<!-- OWNS
src-tauri/src/commands/skills.rs
src-tauri/src/ipc.rs
src-tauri/commands.golden.txt
src-tauri/tests/it/skills_author_pipeline.rs
src-tauri/tests/it/skills_author_origin.rs
src-tauri/tests/it/main.rs
src/sections/skills/index.tsx
src/sections/skills/review-card.tsx
src/sections/skills/io.ts
src/state/skills.ts
src/sections/commands-wired.test.ts
src/sections/skills/write-it-yourself.test.tsx
src/sections/skills/origin-is-not-a-guess.test.tsx
-->
