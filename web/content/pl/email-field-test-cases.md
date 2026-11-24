---
title: Przypadki testowe pola e-mail
url: /pl/poradniki/przypadki-testowe-pola-e-mail/
seoTitle: Przypadki testowe pola e-mail - adresy, które je psują
description: Przypadki testowe pola e-mail, adres po adresie - poprawne, które formularze odrzucają, błędne, które przepuszczają, limit długości, wielkie litery i spacje.
lead: "Pole e-mail psuje się w dwie strony: odrzuca prawdziwe adresy i przepuszcza błędne. Oto przypadki dla obu, każdy z prawdziwym adresem i tym, co zwykle psuje."
---

## Czym testować pole e-mail?

Adresami poprawnymi, ale nietypowymi, adresami, które tylko wyglądają na poprawne, najdłuższym adresem, na jaki pozwala standard, oraz wielkimi literami i spacjami, które człowiek wpisuje, nie zauważając tego. Pole, które sprawdza tylko „coś, małpa, coś, kropka”, część z nich odrzuca niesłusznie, a część niesłusznie przyjmuje. Sekcje niżej biorą je po kolei.

## Co sprawdzić po każdym adresie?

Za każdym razem te same cztery rzeczy:

1. **Czy da się nim zalogować?** Załóż konto z tym adresem i zaloguj się, wpisując go tak samo. Reguła zastosowana przy jednych drzwiach, a przy drugich nie, zamyka ludziom dostęp.
2. **Czy zapisał się tak, jak go wpisano?** Albo został zmieniony wszędzie tak samo, jeśli aplikacja go zmienia.
3. **Czy komunikat nazywa problem?** „Niepoprawny e-mail” przy adresie, którego aplikacja po prostu nie obsługuje, odsyła człowieka na dobre.
4. **Czy poczta do niego dochodzi?** Adres, który formularz przyjął, a serwer poczty odrzucił, to ten sam błąd, znaleziony później.

## Które poprawne adresy formularze odrzucają?

Przetestuj adresy, które mają prawdziwi ludzie. Każdy z nich jest dozwolony i każdy odrzuca zaskakująco wiele formularzy.

{{< values "emails-urls/plus-tag" "emails-urls/unicode-domain" "emails-urls/quoted-local" "emails-urls/trailing-dot-domain" >}}

## Które błędne adresy formularze przepuszczają?

Przetestuj najkrótsze pomyłki. Walidator, który szuka kropki albo małpy gdziekolwiek, je przepuszcza.

{{< values "emails-urls/double-at" "emails-urls/no-at" >}}

## Jak długi może być adres e-mail?

Część przed małpą może mieć 64 znaki, najwięcej, ile pozwala [RFC 5321](https://www.rfc-editor.org/rfc/rfc5321#section-4.5.3.1.1). Przetestuj tę długość i o jeden znak więcej, i sprawdź, czy limit jest pilnowany komunikatem, a nie po cichu ucinanym adresem.

{{< values "emails-urls/very-long-local" "length-bombs/len-255" >}}

## Wielkie litery i spacje

Przetestuj to, co człowiek wpisuje, nie zauważając tego: adres wielkimi literami i spację skopiowaną razem z nim z dokumentu albo wiadomości.

{{< values "emails-urls/uppercase-local" "whitespace/trailing-space" "whitespace/leading-space" "whitespace/zero-width-space-x3" >}}

## A obok pole na stronę internetową

Formularz z polem e-mail ma często także pole na link. Przetestuj link wpisany tak, jak ludzie wpisują linki, link z hasłem w środku i link tak długi, że psuje każdą kolumnę, w której jest pokazywany.

{{< values "emails-urls/url-no-scheme" "emails-urls/url-with-credentials" "emails-urls/url-very-long" "emails-urls/url-unicode-path" >}}

## Jak szybko przejść przez wszystkie?

Paletą, jeden skrót na adres: wybierz paczkę **Adresy e-mail i URL**, kliknij pole, naciśnij {{< kbd "Alt+Shift+N" >}}, spójrz na wynik i naciśnij go jeszcze raz dla następnego. Gdy adres coś psuje, {{< kbd "Alt+Shift+B" >}} kopiuje blok do zgłoszenia. [Pierwsze kroki](/docs/getting-started/) zajmują około dwóch minut.

Do testu automatycznego `nkb emit` wypisuje paczkę jako JSON, CSV albo jedną wartość na wiersz:

```console
$ nkb emit emails-urls --format json > emails.json
```

Każdy adres jest na [stronie paczki](/packs/emails-urls/), razem z tym, co robi poprawna aplikacja. Dla każdego innego pola zobacz [przypadki testowe pola tekstowego](/test-cases-for-a-text-field/).
