---
title: nkb emit
command: emit
layout: command
seoTitle: nkb emit - wartości testowe jako JSON, CSV albo wiersze
description: "nkb emit wypisuje wartości paczki dla skryptu albo testu: JSON, CSV albo jedna wartość na wiersz, w postaci escapowanej albo surowej, także w base64."
lead: "Wypisuje wartości paczki dla skryptu albo pliku testowego, jako JSON, CSV albo jedną wartość na wiersz."
---

## Co wypisuje?

Wartości, na standardowe wyjście, i nic więcej. Opis tego, co wyszło, i każda uwaga o tym, czego format nie umiał przenieść, idą na standardowe wyjście błędów, więc plik, do którego przekierujesz wyjście, zawiera same wartości. Paczka z błędem nie zostaje wypisana wcale: paczka jest cała albo jej nie ma.

## Przykłady

Jedna wartość na wiersz, dla testu, który czyta plik tekstowy:

```console
$ nkb emit magic-values --format lines > magic-values.txt
nkb emit: 12 values, 47 code points, 47 bytes
$ head -4 magic-values.txt
no
null
true
NaN
```

JSON, format domyślny, niesie każde pole każdej wartości i obie jej postacie, samą wartość i wartość w postaci escapowanej:

```console
$ nkb emit locale-pl
{
  "schema": 1,
  "values": [
    {
      "pack": "locale-pl",
      "ref": "locale-pl/pesel-valid",
      "id": "pesel-valid",
      "name": "PESEL with a valid checksum",
      "value": "99023012343",
      "escaped": "99023012343",
```

CSV, jeden wiersz na wartość, domyślnie w postaci escapowanej, bo ten katalog zawiera wartości, które niszczą pliki CSV:

```console
$ nkb emit dates-impossible --format csv
nkb emit: 12 values, 111 code points, 111 bytes
pack,ref,id,name,value,breaks,expect,fields,tags,risk,since,source,generated
dates-impossible,dates-impossible/feb-30,feb-30,30 February,30.02.2026,...
```

## O którą postać prosić?

Postać escapowana zapisuje znak, którego nikt nie zobaczy, jako sekwencję ucieczki, tak jak zapisuje go plik paczki, więc recenzent może ją przeczytać, a plik przeżyje edytor. Postać surowa zapisuje samą wartość i to ją test powinien podać aplikacji. `--escaped` i `--raw` wybierają jedną z nich, a prośba o obie zostaje odrzucona, zamiast być po cichu rozstrzygnięta. `--base64` koduje każdą wartość dla kanału, który uszkodziłby tekst, na przykład zmiennej powłoki albo środowiska, które przepisuje końce wierszy.

`lines` odrzuca wartość, która zawiera podział wiersza, i nazywa ją, zamiast po cichu rozdzielić ją na dwa wiersze.
