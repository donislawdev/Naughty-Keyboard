---
title: nkb lint
command: lint
layout: command
seoTitle: nkb lint - sprawdź plik paczki według reguł formatu
description: nkb lint sprawdza plik paczki wartości testowych według reguł formatu paczki i zgłasza każdy problem w jednym przebiegu, jako tekst albo jako JSON dla CI.
lead: "Sprawdza plik paczki według reguł formatu paczki i zgłasza każdy problem w jednym przebiegu."
---

## Co sprawdza?

Wszystko, czego format paczki wymaga od pliku, od kodowania po zdanie, które mówi, co psuje każda wartość. Wartość bez tego zdania jest błędem, bo to ono sprawia, że katalog jest wart więcej niż lista. Każde uruchomienie mówi, ile reguł formatu sprawdza ta wersja programu, a `--explain` wypisuje każdą regułę i to, dlaczego te nieliczne, których nie sprawdza, czekają.

## Przykłady

Plik, który przechodzi:

```console
$ nkb lint my-pack.toml
0 errors, 0 warnings.
Checked 39 of 45 rules, 1 of them only in part. 5 not checked - run `nkb lint --explain` to see which and why.
The pack format is not frozen yet: it freezes with the first public release that ships packs.
```

Wartość, która nie mówi nic o tym, co psuje:

```console
$ nkb lint my-pack.toml
my-pack.toml:38  E030  value `trailing-space` declares no `breaks`. It is the field this catalogue exists for: say what this value usually breaks and why, or the value is a curiosity rather than a test case.
1 error, 0 warnings.
```

Ten sam werdykt dla pipeline, z `--json`, i nic poza nim na standardowym wyjściu:

```console
$ nkb lint my-pack.toml --json
{
  "schema": 1,
  "tool_version": "0.1.0",
  "pack_format": 1,
  "format_frozen": false,
  "accepted": false,
```

## Czy format jest ostateczny?

Jeszcze nie. Zamraża się wraz z pierwszym publicznym wydaniem, które wiezie paczki, a `nkb lint` mówi to przy każdym uruchomieniu, żeby plik, który przechodzi dziś, nie był czytany jako obietnica na jutro.
