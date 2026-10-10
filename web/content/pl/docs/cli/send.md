---
title: nkb send
command: send
layout: command
seoTitle: nkb send - wpisz wartość testową w pole z fokusem
description: nkb send wpisuje jedną wartość testową w pole z fokusem klawiatury na Windows, z --clear najpierw czyści wiersz i odmawia tam, gdzie klawisze by przepadły.
lead: "Wpisuje jedną wartość z paczki w pole, które ma fokus klawiatury. Na Windows."
---

## Jak go używać?

Podaj paczkę i, z `--index`, którą wartość. Polecenie domyślnie czeka trzy sekundy, żeby był czas kliknąć w pole, a potem wpisuje tam wartość:

```console
$ nkb send magic-values --index 2 --delay 3
```

Wartość trafia do okna z fokusem, a nie na standardowe wyjście, a wszystko, co mówi polecenie, idzie na standardowe wyjście błędów. Dodaj `--clear`, żeby najpierw wyczyścić bieżący wiersz pola klawiszami Home, Shift+End i Delete, tak jak paleta robi domyślnie. Bez tej opcji jedyne wysyłane klawisze to znaki samej wartości.

## Kiedy odmawia?

Odmawia, zamiast zgadywać, a każda odmowa kończy się kodem wyjścia 4:

- gdy na klawiaturze wciśnięty jest Ctrl, Alt, Shift albo Win, po odczekaniu do dwóch sekund na ich puszczenie, bo wartość zamieniłaby się w skróty,
- w oknie działającym z wyższymi uprawnieniami niż `nkb`, na przykład w aplikacji uruchomionej jako administrator, bo Windows odrzuciłby naciśnięcia bez słowa,
- gdy fokus klawiatury jest na przycisku, odnośniku, elemencie listy albo stronie, której nie da się edytować, bo klawisze działają tam na tę kontrolkę.

Gdy nie potrafi rozpoznać, co ma fokus, wysyła. Z `--clear` czyści tylko fokus, o którym wie na pewno, że jest polem tekstowym, i nigdy terminal, gdzie te klawisze trafiają do programu, który w nim działa. Gdy pomija czyszczenie, mówi o tym, a wartość dochodzi do tego, co już było w polu.

## Czy mogę go zatrzymać?

Naciśnij Escape. Nic więcej nie zostanie wpisane, polecenie powie, ile z wartości dotarło, i zakończy się kodem 4. Escape należy do polecenia tylko w trakcie wpisywania. Przed i po klawisz należy do aplikacji.

## Czy działa na macOS i Linuksie?

Jeszcze nie. Tam polecenie mówi, że nie ma sposobu, żeby dostarczyć naciśnięcia, i kończy się kodem 4. Więcej mówią [Uczciwe ograniczenia](/docs/limits/).
