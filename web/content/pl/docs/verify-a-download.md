---
title: Weryfikacja pobrania
slug: weryfikacja-pobrania
seoTitle: Weryfikacja pobrania - sumy, podpisy i poświadczenia
description: Jak sprawdzić, że pobrany plik to ten, który opublikowało wydanie - sumy kontrolne, podpisy Authenticode i Apple oraz podpisane poświadczenia.
lead: "Każde wydanie wiezie wszystko, czego trzeba, żeby sprawdzić pobrany plik bez ufania tej stronie ani osobie, która zrobiła wydanie."
---

## Co wydanie wiezie do sprawdzenia?

Cztery pliki, których nazwy zaczynają się od `verify-`, razem na końcu listy plików:

- `verify-SHA256SUMS.txt`, SHA-256 każdego archiwum,
- zestawienie składników obu programów w formacie SPDX, z końcówką `.spdx.json`,
- podpisane poświadczenie tego, jak zbudowano niepodpisany build, z końcówką `.provenance.sigstore.json`,
- podpisane poświadczenie tego, co zawierają podpisane archiwa, z końcówką `.sbom.sigstore.json`.

Oba poświadczenia to pakunki Sigstore, więc da się je sprawdzić bez sieci.

## Które polecenia to sprawdzają?

Potrzebują [GitHub CLI](https://cli.github.com/). Wstaw wersję w miejsce `VERSION`, a nazwę swojego archiwum w miejsce przykładowej:

{{< verify-commands >}}

- **Pierwsze** pyta GitHuba, czy plik jest jednym z tych, które opublikowało wydanie. Działa dla każdego pliku wydania, bo opublikowane wydanie nie może się zmienić.
- **Dwa następne** sprawdzają poświadczenie tego, co zawiera archiwum, czyli zestawienie składników. Działają dla wszystkich sześciu archiwów, pierwsze przez GitHuba, drugie bez sieci, z samym plikiem obok. Oba potrzebują `--predicate-type`: bez tego `gh` pyta o poświadczenie tego, jak plik zbudowano, podpisane archiwum takiego nie ma, a odpowiedź „no attestation found” wygląda jak zepsute wydanie.
- **Dwa ostatnie** sprawdzają, jak zbudowano archiwum: którym workflow, z którego commitu, na którym runnerze. Działają dla dwóch archiwów linuksowych i dla zestawienia składników, czyli plików, których nic nie podpisało. Archiwum dla Windows albo macOS podpisano po budowaniu, na maszynie, która trzyma klucz, więc jego bajty to nie są bajty z budowania, a odpowiada za nie jego podpis.

To te same polecenia, które niosą notatki wydania, i te same, które workflow uruchamia słowo w słowo przy każdym opublikowanym wydaniu.

## Co jest podpisane i czym?

- **Windows.** `nkb.exe` i `nkb-gui.exe` mają podpis Authenticode ze znacznikiem czasu RFC 3161, więc podpis zostaje ważny po wygaśnięciu certyfikatu. Certyfikat wydał Certum, a jego klucz jest na karcie kryptograficznej, która nie potrafi go oddać.
- **macOS.** `nkb.app` i `nkb-gui.app` są podpisane certyfikatem Apple Developer ID Application, ze wzmocnionym środowiskiem uruchomieniowym i znacznikiem czasu, i poświadczone przez Apple w procesie notaryzacji. Bilet notaryzacji jest zszyty z pakietem, więc macOS sprawdza go bez pytania sieci.
- **Linux.** Programy nie są podpisane. Odpowiada za nie poświadczenie tego, jak je zbudowano.

SHA-256 obu certyfikatów jest przypięte w repozytorium. Podpisywanie odrzuca plik podpisany jakimkolwiek innym certyfikatem, tak samo jak sprawdzenie każdego opublikowanego wydania:

{{< fingerprints >}}

Żeby samemu porównać podpisany program z przypięciem, w PowerShell 7:

```powershell
(Get-AuthenticodeSignature .\nkb.exe).SignerCertificate.GetCertHashString('SHA256')
```

a na macOS:

```console
$ codesign -d --extract-certificates nkb.app
$ shasum -a 256 codesign0
```

Na macOS `spctl -a -vv` i `xcrun stapler validate` na pakiecie mówią, czy Gatekeeper go przyjmuje i czy bilet jest zszyty.

## Czy wydanie można podmienić?

Nie. Wydania tego projektu są niezmienne: opublikowany plik nie może się zmienić, a do wydania nie da się dodać nowego pliku. Zepsute wydanie zostaje oznaczone jako wydanie przedpremierowe, co zdejmuje je z odnośnika do najnowszego wydania, z jednym zdaniem na początku notatek, a poprawka przychodzi w nowym wydaniu.
