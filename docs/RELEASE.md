# Ręczne wydanie Loadouta na macOS

Ten runbook prowadzi operatora przez jedno ręczne wydanie DMG. Nie tworzy automatyzacji wydania,
nie przechowuje danych uwierzytelniających i nie zastępuje decyzji operatora. Wszystkie kroki
wykonaj w tej samej sesji terminala, żeby zmienne wskazywały ten sam commit i te same artefakty.

Blok opisany jako „Komendy bezpieczne do skopiowania” nie wymaga uzupełniania danych. Blok
„Placeholder operatora” opisuje czynność interaktywną: uzupełnij ją lokalnie i nie kopiuj wprost.
Przerwij wydanie przy pierwszym niespełnionym stanie wymaganym.

## 1. Zamroź czyste drzewo i zapisz SHA

Komendy bezpieczne do skopiowania:

```bash
test -z "$(git status --porcelain)"
RELEASE_SHA="$(git rev-parse --verify HEAD^{commit})"
test -n "$RELEASE_SHA"
printf '%s\n' "$RELEASE_SHA"
```

- Stan wymagany: drzewo jest czyste, a operator zapisał konkretny, pełny SHA wyświetlony przez
  ostatnie polecenie. Każdy następny krok dotyczy wyłącznie tego SHA.

## 2. Sprawdź wersje i identyfikator

Komendy bezpieczne do skopiowania:

```bash
PACKAGE_VERSION="$(node -p "require('./package.json').version")"
CARGO_VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' src-tauri/Cargo.toml | head -n 1)"
TAURI_VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
TAURI_IDENTIFIER="$(node -p "require('./src-tauri/tauri.conf.json').identifier")"
test "$PACKAGE_VERSION" = "$CARGO_VERSION"
test "$PACKAGE_VERSION" = "$TAURI_VERSION"
test "$TAURI_IDENTIFIER" = "com.loadout.desktop"
printf '%s\n' "$PACKAGE_VERSION"
```

- Stan wymagany: `package.json`, `src-tauri/Cargo.toml` i `src-tauri/tauri.conf.json` mają tę
  samą wersję, a identyfikator ma stałą wartość `com.loadout.desktop`.

## 3. Uruchom pełną bramkę dla zapisanego SHA

Komendy bezpieczne do skopiowania:

```bash
test "$(git rev-parse --verify HEAD^{commit})" = "$RELEASE_SHA"
scripts/ci.sh full
test "$(git rev-parse --verify HEAD^{commit})" = "$RELEASE_SHA"
```

- Stan wymagany: `scripts/ci.sh full` kończy się zielono pomiędzy dwoma sprawdzeniami tego samego
  `RELEASE_SHA`. Wynik z innego commita nie dopuszcza wydania.

## 4. Zbuduj artefakty

Komendy bezpieczne do skopiowania:

```bash
test "$(git rev-parse --verify HEAD^{commit})" = "$RELEASE_SHA"
npm run app:build
APP="target/release/bundle/macos/Loadout.app"
DMG="$(find target/release/bundle/dmg -maxdepth 1 -type f -name 'Loadout_*.dmg' -print -quit)"
test -d "$APP"
test -f "$DMG"
```

- Stan wymagany: `APP` i `DMG` wskazują artefakty utworzone przez ten przebieg builda na
  `RELEASE_SHA`, a nie pliki pozostałe po wcześniejszym wydaniu.

## 5. Podpisz artefakty certyfikatem Developer ID

Placeholder operatora — uzupełnij lokalnie, nie kopiuj wprost:

```text
<podpisz wszystkie zagnieżdżone pliki wykonywalne w "$APP" certyfikatem Developer ID Application, od środka na zewnątrz>
<podpisz "$APP" jako ostatni element aplikacji; nie używaj --deep>
<spakuj podpisaną aplikację do ZIP, wyślij ZIP przez xcrun notarytool submit --wait, używając lokalnie wybranego profilu uwierzytelnienia, i wymagaj statusu Accepted>
```

Komendy bezpieczne do skopiowania:

```bash
codesign --verify --strict --verbose=4 "$APP"
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
```

Placeholder operatora — uzupełnij lokalnie, nie kopiuj wprost:

```text
<utwórz DMG ponownie z podpisanej aplikacji z dołączonym poświadczeniem i podpisz "$DMG" certyfikatem Developer ID Application>
```

Komendy bezpieczne do skopiowania:

```bash
codesign --verify --strict --verbose=4 "$DMG"
```

- Stan wymagany: oba artefakty przechodzą weryfikację, a szczegóły podpisu wskazują Developer ID
  Application, hardened runtime i bezpieczny znacznik czasu. Aplikacja ma własne, poprawnie
  zweryfikowane poświadczenie notaryzacji, zanim zostanie spakowana do końcowego DMG.
  Podpis ad hoc nie dopuszcza wydania.

Powód tej kolejności: wydanie 0.3.0 miało poświadczenie wyłącznie w DMG (D-2 w
`docs/KNOWN-DEFECTS.md`). Po skopiowaniu aplikacji z obrazu nie było czego sprawdzić lokalnie
przy pierwszym uruchomieniu bez sieci. Poświadczenie należy do obu artefaktów; doklejenie go
tylko do aplikacji po zbudowaniu DMG nie zmieni zawartości gotowego obrazu.

## 6. Wyślij DMG do notaryzacji

Placeholder operatora — uzupełnij lokalnie, nie kopiuj wprost:

```text
<uruchom interaktywnie xcrun notarytool submit "$DMG" --wait i podaj uwierzytelnienie wyłącznie w tej sesji>
```

- Stan wymagany: końcowy wynik zgłoszenia ma status `Accepted` i dotyczy dokładnie pliku z `DMG`.
  Wynik `Invalid`, brak wyniku albo wynik innego pliku przerywa wydanie.

## 7. Dołącz poświadczenie notaryzacji

Komendy bezpieczne do skopiowania:

```bash
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"
```

- Stan wymagany: walidacja staplera potwierdza, że do dokładnie tego DMG dołączono ważne
  poświadczenie notaryzacji.

## 8. Sprawdź Gatekeeper niezależnie

Te polecenia wykonuje osoba, która nie podpisywała artefaktu, w oddzielnej sesji macOS albo na
drugim Macu. Weryfikacja nie może korzystać z oceny zapisanej podczas podpisywania.

Komendy bezpieczne do skopiowania:

```bash
GATEKEEPER_MOUNT="$(mktemp -d)"
hdiutil attach "$DMG" -nobrowse -readonly -mountpoint "$GATEKEEPER_MOUNT"
spctl --assess --type open --context context:primary-signature --verbose=4 "$DMG"
spctl --assess --type execute --verbose=4 "$GATEKEEPER_MOUNT/Loadout.app"
xcrun stapler validate "$GATEKEEPER_MOUNT/Loadout.app"
hdiutil detach "$GATEKEEPER_MOUNT"
rmdir "$GATEKEEPER_MOUNT"
```

- Stan wymagany: niezależna weryfikacja Gatekeepera zwraca `accepted` dla DMG i aplikacji oraz
  wskazuje `Notarized Developer ID`. Aplikacja wewnątrz DMG ma ważne poświadczenie staplera.
  Każda odmowa przerywa wydanie.

## 9. Policz SHA-256 DMG

Komendy bezpieczne do skopiowania:

```bash
DMG_SHA256="$(shasum -a 256 "$DMG" | awk '{print $1}')"
test "$(printf '%s' "$DMG_SHA256" | wc -c | tr -d ' ')" -eq 64
printf '%s  %s\n' "$DMG_SHA256" "$DMG"
```

- Stan wymagany: operator zapisuje 64-znakowy SHA-256 dokładnie tego podpisanego, notaryzowanego
  i zestaplowanego DMG. Suma jest częścią informacji o wydaniu.

## 10. Opublikuj tag, release i pobierz asset

Najpierw przygotuj notatki wydania zawierające zapisany `RELEASE_SHA` i `DMG_SHA256`. Publikacja
jest świadomą operacją zewnętrzną, dlatego poniższy blok wymaga uzupełnienia przez operatora.

Placeholder operatora — uzupełnij lokalnie, nie kopiuj wprost:

```text
RELEASE_TAG=<wersja-z-kroku-2>
git tag -a "$RELEASE_TAG" "$RELEASE_SHA" -m "Loadout $RELEASE_TAG"
git push origin "$RELEASE_TAG"
gh release create "$RELEASE_TAG" "$DMG" --verify-tag --title "$RELEASE_TAG" --notes-file <plik-z-notatkami>
```

Po publikacji pobierz asset z release, zamiast dalej ufać lokalnemu plikowi.

Komendy bezpieczne do skopiowania:

```bash
REMOTE_TAG_SHA="$(git ls-remote origin "refs/tags/$RELEASE_TAG^{}" | awk '{print $1}')"
test "$REMOTE_TAG_SHA" = "$RELEASE_SHA"
DOWNLOAD_DIR="$(mktemp -d)"
gh release download "$RELEASE_TAG" --pattern '*.dmg' --dir "$DOWNLOAD_DIR"
DOWNLOADED_DMG="$(find "$DOWNLOAD_DIR" -maxdepth 1 -type f -name '*.dmg' -print -quit)"
test -f "$DOWNLOADED_DMG"
DOWNLOADED_SHA256="$(shasum -a 256 "$DOWNLOADED_DMG" | awk '{print $1}')"
test "$DOWNLOADED_SHA256" = "$DMG_SHA256"
```

- Stan wymagany: zdalny tag rozwiązuje się do `RELEASE_SHA`, release zawiera DMG i jego SHA-256,
  a asset naprawdę pobrany z release ma tę samą sumę co plik zatwierdzony przed publikacją.

## 11. Zainstaluj pobrany DMG i wykonaj smoke

Użyj wyłącznie `DOWNLOADED_DMG` z poprzedniego kroku. Świeży katalog docelowy chroni istniejącą
instalację operatora i dowodzi, że uruchamiana jest kopia z opublikowanego assetu.

Komendy bezpieczne do skopiowania:

```bash
SMOKE_MOUNT="$(mktemp -d)"
SMOKE_APPS="$(mktemp -d)"
hdiutil attach "$DOWNLOADED_DMG" -nobrowse -readonly -mountpoint "$SMOKE_MOUNT"
ditto "$SMOKE_MOUNT/Loadout.app" "$SMOKE_APPS/Loadout.app"
hdiutil detach "$SMOKE_MOUNT"
spctl --assess --type execute --verbose=4 "$SMOKE_APPS/Loadout.app"
xcrun stapler validate "$SMOKE_APPS/Loadout.app"
open "$SMOKE_APPS/Loadout.app"
```

- Stan wymagany: aplikacja zainstalowana z pobranego DMG przechodzi Gatekeepera, uruchamia się bez
  ostrzeżenia, pokazuje główne okno Loadouta i wersję zgodną z krokiem 2. Lokalny `APP` nie jest
  dopuszczalnym zamiennikiem tego smoke.

## 12. Przekaż sposób aktualizacji

Obecnie aktualizacja wymaga ręcznego pobrania nowego DMG i ponownego zainstalowania aplikacji.
Loadout nie pobiera ani nie instaluje aktualizacji samodzielnie; informacja o kolejnym wydaniu musi
prowadzić człowieka do nowego assetu i jego SHA-256.
