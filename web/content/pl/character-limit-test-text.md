---
title: Tekst do testu limitu znaków
url: /pl/poradniki/tekst-do-testu-limitu-znakow/
seoTitle: Tekst do testu limitu znaków - od 10 do 1000 znaków
description: Tekst o dokładnej długości do testu limitu znaków - od 10 do 1000 znaków, o jeden mniej, równo i o jeden więcej, jako tekst-licznik pokazujący miejsce cięcia.
lead: "Do testu limitu znaków potrzebny jest tekst o dokładnej długości: o jeden znak krótszy od limitu, równy mu i o jeden dłuższy. Oto taki tekst dla limitów, które pola podają najczęściej, jako tekst-licznik, który przy okazji pokazuje, gdzie pole ucięło tekst."
---

## Jak przetestować limit znaków?

Trzema długościami: o jeden mniej niż limit, dokładnie limit i o jeden więcej. To analiza wartości brzegowych zastosowana do długości, a błąd siedzi na krawędzi: pole, które zachowuje 49 i 50 znaków, a po cichu ucina 51, przy każdym krótszym tekście wygląda dobrze. Pierwsze dwa teksty pole musi zachować w całości. Trzeci może odrzucić z komunikatem albo przestać przyjmować znaki na limicie tak, że to widać, ale nigdy nie może zachować uciętej wartości bez słowa.

## Czym jest tekst-licznik?

Tekstem z liczb i gwiazdek, na przykład `*3*5*7*10*`, w którym każda liczba to pozycja gwiazdki zaraz po niej. Gdy pole ucina tekst, ostatnia liczba mówi gdzie: zapisana wartość kończąca się na `*48*` znaczy, że pole zachowało 48 znaków. Każdy tekst niżej to tekst-licznik o dokładnie takiej długości, jaką podaje jego podpis.

## 10 znaków

{{< limit-text 10 >}}

## 50 znaków

{{< limit-text 50 >}}

## 100 znaków

{{< limit-text 100 >}}

## 255 znaków

{{< limit-text 255 >}}

## 500 znaków

{{< limit-text 500 >}}

## 1000 znaków

{{< limit-text 1000 >}}

## Czy 50 znaków to zawsze 50?

Nie. „50 znaków” to dla każdej części aplikacji inna liczba. Atrybut `maxlength` przeglądarki liczy jednostki UTF-16, więc emoji to dwie. Kolumna bazy danych może liczyć znaki albo bajty, a litera spoza ASCII to dwa bajty albo więcej. Człowiek liczy to, co widzi. Te same 50 znaków zbudowane z różnych liter:

{{< limit-fills >}}

Pole, które zachowuje pięćdziesiąt liter `a`, a ucina któryś z pozostałych tekstów, właśnie powiedziało, co naprawdę liczy. [nkb boundary](/docs/cli/boundary/) wyjaśnia, która część aplikacji liczy co.

## Jak dostać tekst dla innego limitu?

W palecie **Length limit** w oknie wartości przyjmuje dowolny limit, liczony w znakach, punktach kodowych, jednostkach UTF-16 albo bajtach, a skróty wpisują potem jego wartości po kolei: z liter, ze znaków wielobajtowych, z emoji i jako tekst-licznik. W wierszu poleceń:

```console
$ nkb boundary --max-length 72 --format lines
```

Teksty na tej stronie pochodzą z tej samej funkcji, więc są dokładnie tym, co wpisuje paleta. Inne wartości, które warto wpisać w pole tekstowe, są w [przypadkach testowych pola tekstowego](/test-cases-for-a-text-field/).
