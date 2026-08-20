# T-70 — Lider siega do biblioteki, bo bez tego „przygotuje ci to" jest obietnica bez pokrycia

Rozstrzygniecie 2026-08-20. Wlasciciel: „lider ma w pelni pracowac na wybranym scope, czasem
chce go uzywac jak czatu np. do researchu" — i przy tym zamowieniu wyszla luka, ktorej nie
widac z ekranu. Lider startuje z `extra_dirs: Vec::new()` (`commands/chat.rs`, `begin`), wiec
widzi **wylacznie folder zakresu**. Twoje workflow i twoi agenci leza w `~/.loadout/workflows`
i `~/.loadout/agents`, czyli poza jego zasiegiem.

Skutek jest dokladnie taki, jak brzmi: „zaloz mi agenta do recenzji" albo „popraw ten krok
w workflow" konczy sie zdaniem, jak to zrobic RECZNIE. Lider, ktory zna twoja biblioteke
z rozmowy, ale nie ma do niej dostepu, jest doradca odcietym od jedynych plikow, o ktorych
rozmawiacie.

**Decyzja i jej granica.** Biblioteka wchodzi do `extra_dirs` rozmowy — i **tylko rozmowy**.
Krok biegu jej nie dostaje: agent piszacy kod w projekcie nie ma powodu przepisywac definicji
innych agentow, a `fresh-copy` i tak odcina go od drzewa gospodarza. Polityka zostaje sufitem
bez zmian: lider `look-only` biblioteke CZYTA (to jest cala wartosc „jakie mam workflow?"),
a pisze dopiero ten, ktoremu czlowiek dal `ask-first` albo `work-freely`.

**Cicha porazka, przed ktora stoi ten kontrakt:** dosypanie katalogow WSZYSTKIM. Krok biegu
z dostepem do `~/.loadout` moze nadpisac definicje agenta w trakcie biegu, ktory z niej wlasnie
korzysta — a bieg czyta plik raz, przy starcie kroku. Awarii nie widac az do nastepnego biegu,
kiedy „ten sam workflow" robi co innego.

**Read first:**
`src-tauri/src/commands/chat.rs` (`begin`, `RunSpec.extra_dirs`, `Policy::EditInFolder`),
`src-tauri/src/commands/run.rs` (`extra_dirs` kroku — dzis katalog przekazan),
`src-tauri/src/library/` (gdzie naprawde leza agenci i workflow),
`src-tauri/src/engine/drivers/claude.rs` (jak `extra_dirs` wchodzi do argv),
`AGENTS.md` niezmienniki 4, 9, 23.

## Niezmienniki, ktorych to dotyczy

- **4 — pliki sa prawda.** Biblioteka JEST plikami, wiec lider poprawiajacy workflow poprawia
  to samo, co czyta okno. Zaden stan posredni nie jest do tego potrzebny.
- **23 — polityka w jednym rdzeniu.** Sufit dalej daje `Policy`; `extra_dirs` mowi GDZIE, nie CO.

## Szkielet, bez ktorego `before` nie jest czerwone

Sygnatura skladajaca liste katalogow z `todo!()`, zeby testy sie kompilowaly i padly w czasie
wykonania.

## Kryteria akceptacji

## AC-1 Rozmowa widzi biblioteke, bieg jej nie widzi
check: cargo test --test it lead_reaches_the_library::
expect: (\d+) passed

Asercje: (a) `RunSpec` rozmowy niesie w `extra_dirs` katalog workflow i katalog agentow —
policzone z tego samego miejsca, z ktorego czyta je biblioteka, nie sklejone z literalow;
(b) `RunSpec` kroku biegu **nie niesie ani jednego z nich**; (c) katalog przekazan, ktory krok
dostaje dzis, zostaje bez zmiany — ta zmiana nie ma prawa nic zabrac; (d) kontrola przeciw
pustemu przejsciu: test sprawdza, ze obie sciezki naprawde istnieja w drzewie fikstury,
inaczej porownuje dwie pustki.

*Slaba asercja:* sam (a). Przechodzi dla implementacji, ktora dosypuje katalogi wszystkim —
czyli dla wersji, w ktorej krok biegu nadpisuje definicje agenta uzywana wlasnie przez ten bieg.
Rozroznia to (b).

## AC-2 Polityka zostaje sufitem takze w bibliotece
check: cargo test --test it library_access_obeys_the_policy::
expect: (\d+) passed

Asercje: (a) lider `look-only` ma biblioteke w `extra_dirs`, a w `--tools` i `--allowedTools`
dalej nie ma `Edit`, `Write` ani `Bash` — czyta i nie pisze; (b) lider `work-freely` ma je oba;
(c) `--permission-mode` wynika dalej wylacznie z polityki; (d) kontrola: ta sama definicja
agenta z `look-only` i z `work-freely` daje DWA rozne zestawy flag — inaczej test mierzy
implementacje, ktora ignoruje polityke.

*Slaba asercja:* sprawdzenie samej obecnosci katalogow. Przechodzi dla implementacji, ktora
przy okazji podniosla liderowi uprawnienia „zeby moglo dzialac". Rozrozniaja to (a) i (d).

<!-- OWNS
src-tauri/src/commands/chat.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/lead_reaches_the_library.rs
src-tauri/tests/it/library_access_obeys_the_policy.rs
-->
