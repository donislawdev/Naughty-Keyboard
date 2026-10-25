---
title: Uczciwe ograniczenia
slug: ograniczenia
seoTitle: Uczciwe ograniczenia - czego program jeszcze nie robi
description: "Gdzie Naughty Keyboard się kończy: wpisywanie ma warunki w każdym systemie, Linux zmierzono na jednym, własnych paczek nie wczytasz, format nie jest zamrożony."
lead: "Narzędzie, które po cichu robi mniej, niż się wydaje, jest gorsze od takiego, które to mówi. Oto, czego jeszcze nie robi."
---

## Czy wpisuje w inne okna wszędzie?

Na Windows, macOS i Linuksie wpisuje, a każdy system najpierw czegoś wymaga. Windows nie wymaga niczego. Na macOS program, który wpisuje, czyli `nkb` uruchomiony z terminala albo paleta, potrzebuje uprawnienia dostępności (Accessibility). Bez niego `nkb send` mówi, co włączyć, niczego nie wysyła i kończy się kodem 4, a paleta kładzie wartości w schowku. Na Linuksie pod X11 potrzebny jest lokalny wyświetlacz. Pod Waylandem potrzebny jest demon IBus, przez który idzie tekst, a `--clear` i skróty palety potrzebują zgody pulpitu, o którą pulpit pyta w osobnym oknie za pierwszym razem.

Tam, gdzie system nie ma sposobu, `nkb send` to mówi i kończy się kodem 4, a wartość trafia do pola przez schowek, przyciskami Copy palety i jej przełącznikiem schowka. Wiersz poleceń i narzędzia do paczek, `packs`, `show`, `emit`, `lint`, `fmt` i `new-pack`, nie dotykają systemu i działają wszędzie.

## Czy sprawdzono to na każdym pulpicie Linuksa?

Nie. Zmierzono to tylko na Ubuntu 26.04: GNOME 50 na Waylandzie i X11 z openbox i xfwm4. KDE i inne pulpity nie były sprawdzane. Pod Waylandem `nkb send` przekazuje tekst metodzie wprowadzania i nie umie powiedzieć, czy jakieś pole miało fokus. Pod X11 program, który czyta klawisze przez metodę wprowadzania, na przykład xterm z IBus, może zgubić koniec długiego ciągu znaków, których układ klawiatury nie ma. Na Linuksie paleta nie pamięta, gdzie stała, a pod X11 jej okno skrótów nie nagra kombinacji.

## Czy system może zmienić to, co zostało wpisane?

Na macOS tak. Podstawienia tekstu w systemie mogą zamienić wpisaną wartość w inną, a paleta i `nkb send` mówią o tym, gdy włączone podstawienie mogło zadziałać na właśnie tę wartość.

## Czy mogę wczytać własne paczki?

Możesz je napisać i sprawdzić, ale jeszcze nie wczytać. `nkb new-pack`, `nkb fmt` i `nkb lint` działają na Twoim własnym pliku. Wczytywanie folderu własnych paczek do palety i do wiersza poleceń nie jest podłączone, a `nkb packs` to mówi: zgłasza foldery na paczki własne i zespołu jako nieprzeczytane.

## Czy format paczki jest stabilny?

Jeszcze nie. Kolejne wydanie może jeszcze zmienić pole, a jego notatki powiedzą jak. `nkb lint` mówi to przy każdym uruchomieniu, a `nkb lint --explain` wypisuje każdą regułę formatu i to, które z nich ta wersja programu sprawdza.

## Czy mówi mi, czy moja aplikacja sobie poradziła?

Nie. Wpisuje wartość i nie czyta tego, co pokazuje okno, tylko nazwę pliku jego programu i rodzaj kontrolki, która ma klawiaturę. Czy aplikacja sobie poradziła, oceniasz Ty. Oznaczania wyniku osobną kombinacją jeszcze nie ma, więc zapisem jest blok do zgłoszenia.

## Czy wpisze w cokolwiek?

Nie, i to celowo. `nkb send` i paleta odmawiają okna działającego z wyższymi uprawnieniami niż one same, bo Windows odrzuciłby naciśnięcia bez słowa, i kontrolki, w której klawisze działałyby na kontrolkę, a nie na tekst, takiej jak przycisk albo element listy. Gdy nie potrafią rozpoznać, jaka kontrolka ma fokus, wysyłają.

## Jak duży jest katalog?

Paczki: {{< count "pack" >}}, wartości: {{< count "value" >}}, wybrane jako te, których przeoczenie kosztuje najwięcej. To początek, a nie pełna lista czegokolwiek. [Zaproponuj wartość](https://github.com/donislawdev/Naughty-Keyboard/issues/new?template=suggest_value.yml), która kiedyś kosztowała Cię popołudnie.
