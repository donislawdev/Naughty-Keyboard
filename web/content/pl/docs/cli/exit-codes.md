---
title: Kody wyjścia
slug: kody-wyjscia
seoTitle: Kody wyjścia nkb - jak ich użyć w CI
description: Sześć kodów wyjścia nkb i znaczenie każdego, takie samo w każdym poleceniu, żeby pipeline CI odróżnił złą paczkę od złego wywołania i nieczytelnego pliku.
lead: "Każde polecenie `nkb` kończy się jedną z sześciu liczb, a liczba znaczy to samo bez względu na to, które polecenie ją zwraca."
---

## Co znaczy każdy kod?

{{< exit-codes >}}

Polecenie używa tylko kodów, które mogą mu się przydarzyć, a `nkb lint --help` i `nkb fmt --help` wymieniają swoje. Prośba o pomoc to sukces, więc `--help` kończy się kodem 0.

## Czy kody się zmienią?

Nie w znaczeniu. Zestaw jest kompletny od pierwszego wydania i to celowo: kod dodany później zmieniłby czyjś zielony pipeline na czerwony bez żadnej zmiany po jego stronie.

## Jak użyć ich w CI?

```sh
nkb lint packs/my-pack.toml
status=$?
if [ "$status" -eq 1 ]; then
  echo "the pack has errors"
elif [ "$status" -ne 0 ]; then
  echo "nkb could not check it"
fi
```
