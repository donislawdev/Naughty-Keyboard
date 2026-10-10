---
title: Uczciwe ograniczenia
slug: ograniczenia
seoTitle: Uczciwe ograniczenia - czego program jeszcze nie robi
description: "Gdzie Naughty Keyboard się dziś kończy: wpisywanie w inne okna tylko na Windows, własnych paczek jeszcze nie da się wczytać, a format paczki nie jest zamrożony."
lead: "Narzędzie, które po cichu robi mniej, niż się wydaje, jest gorsze od takiego, które to mówi. Oto, czego jeszcze nie robi."
---

## Czy wpisuje w inne okna na macOS i Linuksie?

Jeszcze nie. Na macOS i Linuksie ta wersja programu nie umie rejestrować skrótów globalnych, więc paletą nie da się tam sterować z klawiatury, a `nkb send` odpowiada, że nie ma jeszcze sposobu, żeby dostarczyć naciśnięcia, i kończy kodem 4. Wartość trafia do pola przez schowek, przyciskami Copy palety i jej przełącznikiem schowka. Wiersz poleceń i narzędzia do paczek, `packs`, `show`, `emit`, `lint`, `fmt` i `new-pack`, nie dotykają systemu i działają wszędzie.

## Czy mogę wczytać własne paczki?

Możesz je napisać i sprawdzić, ale jeszcze nie wczytać. `nkb new-pack`, `nkb fmt` i `nkb lint` działają na Twoim własnym pliku. Wczytywanie folderu własnych paczek do palety i do wiersza poleceń nie jest podłączone, a `nkb packs` to mówi: zgłasza foldery na paczki własne i zespołu jako nieprzeczytane.

## Czy format paczki jest stabilny?

Jeszcze nie. Zamraża się wraz z pierwszym publicznym wydaniem, które wiezie paczki, a do tego czasu pole może się jeszcze zmienić. `nkb lint` mówi to przy każdym uruchomieniu, a `nkb lint --explain` wypisuje każdą regułę formatu i to, które z nich ta wersja programu sprawdza.

## Czy mówi mi, czy moja aplikacja sobie poradziła?

Nie. Wpisuje wartość i nie czyta tego, co pokazuje okno, tylko nazwę pliku jego programu i rodzaj kontrolki, która ma klawiaturę. Czy aplikacja sobie poradziła, oceniasz Ty. Oznaczania wyniku osobną kombinacją jeszcze nie ma, więc zapisem jest blok do zgłoszenia.

## Czy wpisze w cokolwiek?

Nie, i to celowo. `nkb send` i paleta odmawiają okna działającego z wyższymi uprawnieniami niż one same, bo Windows odrzuciłby naciśnięcia bez słowa, i kontrolki, w której klawisze działałyby na kontrolkę, a nie na tekst, takiej jak przycisk albo element listy. Gdy nie potrafią rozpoznać, jaka kontrolka ma fokus, wysyłają.

## Jak duży jest katalog?

Paczki: {{< count "pack" >}}, wartości: {{< count "value" >}}, wybrane jako te, których przeoczenie kosztuje najwięcej. To początek, a nie pełna lista czegokolwiek. [Zaproponuj wartość](https://github.com/donislawdev/Naughty-Keyboard/issues/new?template=suggest_value.yml), która kiedyś kosztowała Cię popołudnie.
