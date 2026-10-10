---
title: Przypadki testowe pola tekstowego
url: /pl/poradniki/przypadki-testowe-pola-tekstowego/
seoTitle: Przypadki testowe pola tekstowego i formularza
description: "Przypadki testowe pola tekstowego, wartość po wartości: puste i niewidoczne dane, limity długości, Unicode, liczby, daty, słowa mylące parser i eksport."
lead: "Pole tekstowe testuje się wartościami, które naprawdę w nie trafią, a nie tylko tą, pod którą zaprojektowano formularz. Oto przypadki testowe, każdy z prawdziwą wartością i tym, co zwykle psuje."
---

## Czym testować pole tekstowe?

Tym, co naprawdę wpisują w nie ludzie i co trafia do niego z danych, a nie tylko imieniem, pod które zaprojektowano formularz: niczym, spacjami, których nikt nie widzi, wartością na limicie długości i o jeden znak dłuższą, literami spoza ASCII, liczbami i datami zapisanymi inaczej, słowami, które parser czyta jako coś innego, i znakami, które sprawiają kłopot dopiero w eksporcie. Sekcje niżej biorą je po kolei. Każda wartość pochodzi z [katalogu](/packs/), mówi, co zwykle psuje, i prowadzi do tego, co zamiast tego robi poprawna aplikacja.

## Co sprawdzić po każdej wartości?

Za każdym razem tych samych pięć rzeczy:

1. **Czy zapisała się tak, jak ją wpisano?** Otwórz rekord jeszcze raz i porównaj.
2. **Czy przeglądarka i serwer się zgadzają?** Wartość, którą formularz przyjmuje, a serwer odrzuca, albo odwrotnie, to jedno z najczęstszych znalezisk.
3. **Czy komunikat nazywa problem?** „Niepoprawne dane” przy wartości o jeden znak za długiej to osobny błąd.
4. **Czy przeżywa drogę w obie strony?** Wyszukaj ją, edytuj, zobacz na liście.
5. **Czy przeżywa eksport?** Otwórz plik CSV albo arkusz, który tworzy aplikacja.

## Puste pole, spacje i znaki niewidoczne

Sprawdź granicę między pustym a wypełnionym. Pole, które przycina spacje po jednej stronie, a po drugiej nie, kończy z wartością pustą dla jednej warstwy i wypełnioną dla następnej.

{{< values "whitespace/space-only" "whitespace/zwsp-only" "whitespace/leading-space" "whitespace/trailing-space" >}}

## Długość i limity

Sprawdź jeden znak poniżej limitu, sam limit i jeden znak ponad, a potem wartość daleko za nim. Sprawdź też, czy limit liczy znaki, czy bajty: w UTF-8 litera spoza ASCII to jeden znak i co najmniej dwa bajty.

{{< values "length-bombs/len-255" "length-bombs/len-256" "length-bombs/bytes-vs-chars" "length-bombs/len-65535" >}}

## Litery spoza ASCII i Unicode

Sprawdź litery, które Twoi użytkownicy mają w imionach i nazwiskach, i znaki, które zmieniają sposób liczenia, porównywania albo rysowania tekstu.

{{< values "locale-pl/diacritics-full" "unicode-text/combining-acute" "unicode-text/emoji-in-name" "unicode-text/rtl-override" >}}

## Słowa, które parser czyta jako coś innego

Sprawdź słowa, które dla programu gdzieś między polem a bazą danych znaczą „brak wartości”, „fałsz” albo „to nie liczba”.

{{< values "magic-values/word-null" "magic-values/bool-no" "magic-values/word-nan" "magic-values/word-undefined" >}}

## Liczby

Sprawdź granice typu liczbowego, wartość ujemną tam, gdzie nikt jej nie przewidział, i liczby zapisane tak, jak zapisuje się je w innym kraju.

{{< values "numbers-extreme/minus-one" "numbers-extreme/comma-decimal" "numbers-extreme/int32-max-plus-one" "numbers-extreme/js-safe-plus-one" >}}

## Daty

Sprawdź daty, które nie istnieją, daty, które da się odczytać na dwa sposoby, i lata spoza zwykłego zakresu.

{{< values "dates-impossible/feb-30" "dates-impossible/feb-29-non-leap" "dates-impossible/ambiguous-day-month" "dates-impossible/year-10000" >}}

## Wartości, które psują dopiero eksport

Sprawdź, co dzieje się za formularzem. Wartość, która przechodzi walidację, wciąż może zepsuć plik, który eksportuje aplikacja, i arkusz tego, kto go otworzy.

{{< values "export-breakers/formula-equals" "export-breakers/csv-comma" "export-breakers/leading-zero-code" "export-breakers/sheet-precision" >}}

## Nazwy plików

Pole, którego wartość staje się nazwą pliku, sprawdź nazwami, które system plików odrzuca albo czyta inaczej.

{{< values "filenames-paths/reserved-con" "filenames-paths/trailing-dot" "filenames-paths/double-extension" "filenames-paths/dotdot" >}}

## Jak szybko przejść przez wszystkie?

Paletą, jeden skrót na wartość: kliknij pole, naciśnij {{< kbd "Alt+Shift+N" >}}, zobacz wynik i naciśnij znowu, żeby wpisać następną. Gdy wartość coś zepsuje, {{< kbd "Alt+Shift+B" >}} kopiuje blok do zgłoszenia. [Pierwsze kroki](/docs/getting-started/) zajmują mniej więcej dwie minuty.

Do testu automatycznego `nkb emit` wypisuje każdą paczkę jako JSON, CSV albo jedną wartość na wiersz:

```console
$ nkb emit length-bombs --format json > length-bombs.json
```

Cały katalog (paczki: {{< count "pack" >}}, wartości: {{< count "value" >}}) pokazuje [strona paczek](/packs/).
