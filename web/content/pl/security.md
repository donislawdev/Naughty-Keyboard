---
title: Bezpieczeństwo
slug: bezpieczenstwo
seoTitle: Bezpieczeństwo - co czyta, co zapisuje i jak to sprawdzić
description: Naughty Keyboard pisze w oknie, na które patrzysz. Co czyta, żeby to zrobić, czego nie czyta nigdy, gdzie zapisuje i jakie testy pilnują każdej obietnicy.
lead: "Naughty Keyboard wpisuje tekst w okno, na które patrzysz. To jest cały produkt. Wszystko niżej wynika z tego zdania."
---

## Co czyta?

Na Windows, zanim wyśle wartość, czyta rodzaj kontrolki, która ma fokus klawiatury, z zamkniętej listy właściwości, nazwę pliku programu, do którego należy okno, nigdy jego tytuł, i jedną liczbę: czy ten program działa z wyższymi uprawnieniami niż on sam. Nie czyta tytułu, tekstu ani wartości żadnej kontrolki i nie śledzi, które okno wychodzi na wierzch. Testy `field_reads_only_kinds` i `program_reads_only_its_name` trzymają te odczyty w liście, więc nowy odczyt zatrzymuje budowanie, dopóki ktoś świadomie nie zmieni listy.

## Jakie klawisze wysyła?

Znaki wartości i nic więcej. Jedyny wyjątek to wyczyszczenie wiersza przed wpisaniem klawiszami Home, Shift+End i Delete, które paleta robi domyślnie, a `nkb send` tylko z `--clear`. Test `keystrokes_have_named_doors` nie przechodzi, jeśli wywołanie wysyłające klawisze pojawi się gdziekolwiek poza miejscami, które nazywa.

## Kiedy odmawia?

Tam, gdzie klawisze by przepadły albo zostały źle odczytane. Okno działające z wyższymi uprawnieniami odrzuciłoby je bez słowa, więc dla takiego okna paleta używa schowka. Nie wysyła niczego, gdy wciśnięty jest Ctrl, Alt, Shift albo Win, żeby wartość nie zamieniła się w skróty, ani gdy fokus jest na przycisku, odnośniku albo elemencie listy. Gdy nie potrafi rozpoznać, co ma fokus, wysyła i mówi o tym.

## Czy używa schowka?

W trzech miejscach i tylko do zapisu: blok do zgłoszenia, tryb schowka i przyciski Copy palety, każde po Twoim naciśnięciu albo kliknięciu. Nigdy nie wkleja za Ciebie i nigdy nie czyta schowka. Test `clipboard_has_named_doors` nazywa te miejsca.

## Czy instaluje hak klawiatury?

Nie. Skróty rejestruje systemowym wywołaniem do skrótów globalnych, które mówi mu tylko tyle, że naciśnięto jedną z jego własnych kombinacji. Przy wysyłaniu pyta system, czy wciśnięte są Ctrl, Alt, Shift, Win i Escape i czy klawisze, które właśnie wysłał, dotarły, i nic więcej o klawiaturze.

## Czy potrzebuje uprawnień administratora?

Nie. Programy nie mają manifestu, który by o nie prosił, i żaden z nich sam nie podnosi swoich uprawnień.

## Czy łączy się z internetem?

Nie. Bez telemetrii, bez sprawdzania aktualizacji, nic nie jest pobierane w trakcie działania. Test `nothing_reaches_the_network` czyta źródła każdego pakietu i widoku i nie przechodzi przy gnieździe sieciowym, zapytaniu DNS, kliencie HTTP albo zależności zakazanej jako klient sieciowy. Test `links_nothing_off_the_machine` czyta tabelę importów obu zbudowanych programów i nie przechodzi, jeśli którykolwiek łączy się z biblioteką sieciową. Na Linuksie biblioteka graficzna palety rozmawia z serwerem X i sesyjną szyną D-Bus wskazanymi przez Twój pulpit. To gniazda na tym komputerze, chyba że pulpit ustawiono tak, żeby sięgał do nich przez sieć.

## Gdzie zapisuje?

Ustawienia palety trafiają do `settings.toml` w Twoim folderze konfiguracji, zapisywanego przez plik tymczasowy. `nkb new-pack` zapisuje jeden plik w bieżącym folderze i nigdy nie nadpisuje istniejącego. `nkb fmt` przepisuje tylko plik, który dostał, a gdy zmiana zmieniłaby wartość, odmawia i nic nie zapisuje.

## Czy kod `unsafe` jest odizolowany?

Tak. Obszar roboczy zakazuje `unsafe`, a `nkb-sys`, pakiet, który woła system operacyjny, jest jedynym, w którym taki kod się kompiluje. Test `unsafe_lives_here_only` nie przechodzi, jeśli inny pakiet dostanie to prawo.

## Jak zgłosić problem z bezpieczeństwem?

Prywatnie, przez [prywatne zgłaszanie podatności na GitHubie](https://github.com/donislawdev/Naughty-Keyboard/security/advisories/new), a nie w publicznym zgłoszeniu. [SECURITY.md](https://github.com/donislawdev/Naughty-Keyboard/blob/main/SECURITY.md) mówi po angielsku, co jest w zakresie i co dołączyć. Program, który pisze w oknie na wierzchu, to sam produkt, więc zgłoszenie musi pokazać, że pisze w oknie, którego nikt nie wybrał, wysyła klawisze, o których mówi, że ich nie wysyła, albo czyta więcej, niż mówi, że czyta.

## Skąd wiem, że pobrany plik jest prawdziwy?

[Weryfikacja pobrania](/docs/verify-a-download/) ma polecenia, odciski certyfikatów podpisujących i mówi, czego dowodzi każde sprawdzenie.
