---
title: Character limit test text
url: /guides/character-limit-test-text/
seoTitle: Character limit test text - 10 to 1000 characters
description: Test text of an exact length for a character limit - 10 to 1000 characters, one under, at and one over, as counterstrings that show where a field cut.
lead: "To test a character limit you need text of an exact length: one under the limit, exactly at it and one over it. Here it is for the limits fields state most often, as counterstrings, which also show where a field cut the text."
---

## How do I test a character limit?

With three lengths: one under the limit, exactly at it and one over it. That is boundary value testing applied to length, and the defect hides at the edge: a field that keeps 49 and 50 characters and quietly cuts 51 looks fine with any shorter text. The field must keep the first two whole. With the third it may refuse the text with a message, or stop typing at the limit where you can see it, but never keep a cut value without saying so.

## What is a counterstring?

Text of numbers and stars, such as `*3*5*7*10*`, where each number is the place of the star after it. When a field cuts the text, the last number says where: a stored value ending in `*48*` means the field kept 48 characters. Every text below is a counterstring of exactly the length its label says.

## 10 characters

{{< limit-text 10 >}}

## 50 characters

{{< limit-text 50 >}}

## 100 characters

{{< limit-text 100 >}}

## 255 characters

{{< limit-text 255 >}}

## 500 characters

{{< limit-text 500 >}}

## 1000 characters

{{< limit-text 1000 >}}

## Is 50 characters always 50?

No. "50 characters" is a different number to each part of an application. A browser's `maxlength` counts UTF-16 units, so an emoji is two. A database column may count characters or bytes, and a letter outside ASCII is two bytes or more. A person counts what they can see. The same 50 characters, made of different letters:

{{< limit-fills >}}

A field that keeps fifty letters `a` and cuts one of the others has told you what it really counts. [nkb boundary](/docs/cli/boundary/) explains which part of an application counts what.

## How do I get text for any other limit?

In the palette, **Length limit** in the value window takes any limit, counted in characters, code points, UTF-16 units or bytes, and the shortcuts then type its values one by one, in letters, multi-byte characters, emoji and counterstrings. On the command line:

```console
$ nkb boundary --max-length 72 --format lines
```

The texts on this page come from the same function, so they are exactly what the palette types. For the other values worth trying in a text field, see [text box test cases](/test-cases-for-a-text-field/).
