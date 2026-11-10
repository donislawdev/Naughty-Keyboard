---
title: nkb boundary
command: boundary
layout: command
seoTitle: nkb boundary - wartości graniczne dla limitu długości pola
description: "nkb boundary tworzy wartości dla limitu długości: pustą, o jeden krótszą, równą i o jeden dłuższą, z liter, znaków wielobajtowych, emoji i counterstringu."
lead: "Podajesz limit, który deklaruje pole, i dostajesz wartości, które go testują: pustą, o jeden krótszą, dokładnie równą i o jeden dłuższą, ze znaków, które ważą inaczej w bajtach i w jednostkach UTF-16."
---

## Co tworzy?

Dla pola, które przyjmuje najwyżej 50 znaków: wartość pustą, a potem wartości o długości 49, 50 i 51 w każdym z sześciu wypełnień. Z `--min-length` dochodzą długości wokół najmniejszej, jaką pole przyjmuje, a długość wspólna dla dwóch granic pojawia się raz. Każda wartość mówi, jak długa jest w czterech jednostkach i czy jej długość mieści się w limicie.

Paczki to listy nietypowych znaków. To polecenie odpowiada na jedno zdanie, które specyfikacja mówi o polu.

## Dlaczego sześć wypełnień?

Bo „50 znaków” to jedna z czterech liczb, a rzadko wiadomo, którą z nich liczy aplikacja:

- `maxlength` w przeglądarce jest mierzony tak, jak [JavaScript mierzy napis](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/length), w jednostkach UTF-16. Emoji to dwie.
- `varchar(n)` w PostgreSQL mieści [n znaków, nie bajtów](https://www.postgresql.org/docs/current/datatype-character.html).
- `varchar(n)` w SQL Server [liczy bajty](https://learn.microsoft.com/sql/t-sql/data-types/char-and-varchar-transact-sql), a `nvarchar(n)` pary bajtów.
- Autor specyfikacji liczy to, co widzi.

Pięćdziesiąt liter `a` to pięćdziesiąt w każdej jednostce, więc nie odróżni jednej od drugiej. Pięćdziesiąt liter dwubajtowych to pięćdziesiąt znaków i sto bajtów, pięćdziesiąt emoji to sto jednostek UTF-16, a pięćdziesiąt emoji rodziny to dla oka pięćdziesiąt znaków i dwieście pięćdziesiąt punktów kodowych. Pole, które przyjmuje pięćdziesiąt liter `a`, a odrzuca albo ucina którąś z pozostałych wartości, właśnie powiedziało, co liczy.

Żaden z tych znaków nie zmienia się w żadnej postaci normalnej Unicode, więc pole, które normalizuje tekst, nadal dostaje długość, o którą chodziło.

## Przykłady

Jedna wartość na wiersz, do wklejania albo dla testu:

```console
$ nkb boundary --max-length 50 --format lines > limit-50.txt
nkb boundary: 19 values, 1500 code points, 4350 bytes
```

CSV, z czterema rozmiarami każdej wartości obok siebie:

```console
$ nkb boundary --max-length 50 --format csv
nkb boundary: 19 values, 1500 code points, 4350 bytes
id,name,value,fill,length,within,graphemes,code_points,utf16_units,bytes
empty,Empty value,,,0,true,0,0,0,0
letters-49,49 x a,aaaaa...,letters,49,true,49,49,49,49
letters-50,50 x a,aaaaa...,letters,50,true,50,50,50,50
letters-51,51 x a,aaaaa...,letters,51,false,51,51,51,51
...
two-byte-50,50 x U+0142,...,two-byte,50,true,50,50,50,100
four-byte-50,50 x U+1F600,...,four-byte,50,true,50,50,100,200
cluster-50,50 x U+1F468 U+200D U+1F469 U+200D U+1F467,...,cluster,50,true,50,250,400,900
```

Limit liczony w bajtach. Każda wartość ma dokładnie tyle bajtów, ile mówi jej wiersz, z literą `a` z przodu tam, gdzie całe znaki nie dają tej liczby:

```console
$ nkb boundary --max-length 50 --unit bytes --format csv
...
two-byte-49,1 x a + 24 x U+0142,...,two-byte,49,true,25,25,25,49
two-byte-50,25 x U+0142,...,two-byte,50,true,25,25,25,50
two-byte-51,1 x a + 25 x U+0142,...,two-byte,51,false,26,26,26,51
```

## O którą jednostkę poprosić?

Zacznij bez `--unit`. Wartości są wtedy liczone tak, jak liczy człowiek, a odpowiedź aplikacji na cięższe wypełnienia pokazuje, co liczy ona.

Potem uruchom polecenie jeszcze raz, w tej jednostce. Wartość o 51 bajtach z litery dwubajtowej to jedno `a` i dwadzieścia pięć liter, więc pole, które tnie na 50 bajtach, przecina ostatnią literę w połowie. W `utf16` to samo spotyka emoji, a zostaje pół znaku: tekst, który przestaje się wyświetlać, zapis, który się nie udaje, albo błąd daleko od pola. Wartość z całych znaków aż do limitu nigdy tego nie wywoła i dlatego dopełnienie stoi z przodu.

## Jak czytać counterstring?

Przed każdą gwiazdką stoi numer jej własnego miejsca. To counterstring o długości 51:

```text
*3*5*7*9*12*15*18*21*24*27*30*33*36*39*42*45*48*51*
```

Jeśli w polu zostało `...42*45*48*51`, ostatnia gwiazdka stoi na miejscu 48 i są po niej dwa znaki, więc weszło 50. Miejsce cięcia czytasz z samego pola, bez liczenia i bez drugiego narzędzia.

Counterstring to cyfry i gwiazdki. Pole, które przyjmuje same litery, odrzuci go z tego powodu i od tego jest wypełnienie `letters`. Pole, które renderuje Markdown, może połknąć gwiazdki.

## Co obiecuje „within”?

Że długość wartości mieści się w podanym limicie, w podanej jednostce, i nic więcej. Pole wymagane nadal odrzuci wartość pustą, a pole przyjmujące same litery nadal odrzuci emoji. Gdy wartość mieszcząca się w limicie zostaje odrzucona z powodu długości, to albo aplikacja liczy w innej jednostce, albo limit jest inny, niż mówi specyfikacja. Jedno i drugie warto zgłosić.
