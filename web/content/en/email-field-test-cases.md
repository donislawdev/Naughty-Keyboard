---
title: Email field test cases
url: /guides/email-field-test-cases/
seoTitle: Email field test cases - addresses that break forms
description: Email field test cases, address by address - valid ones forms turn away, broken ones they accept, the length limit, capital letters and spaces.
lead: "An email field fails in two directions: it turns away real addresses and lets broken ones through. These are the cases for both, each with a real address and what it usually breaks."
---

## What should an email field be tested with?

With addresses that are valid but unusual, addresses that only look valid, the longest address the standard allows, and the capital letters and spaces a person types without noticing. A field that checks for "something, an @, something, a dot" fails some of them in one direction and some in the other. The sections below take them one at a time.

## What do I check after each address?

The same four things every time:

1. **Can the person sign in with it?** Sign up with the address, then sign in typing it the same way. A rule applied at one door and not at the other locks people out.
2. **Is it stored as typed?** Or changed the same way everywhere, if the application changes it.
3. **Does the message name the problem?** "Invalid email" for an address the application simply does not support sends the person away for good.
4. **Does mail reach it?** An address the form accepts and the mail system refuses is the same defect, found later.

## Which valid addresses do forms turn away?

Test the addresses real people have. Every one of these is legal, and each is refused by a surprising number of forms.

{{< values "emails-urls/plus-tag" "emails-urls/unicode-domain" "emails-urls/quoted-local" "emails-urls/trailing-dot-domain" >}}

## Which broken addresses do forms accept?

Test the shortest mistakes. A validator that looks for a dot, or for an @ anywhere, lets these through.

{{< values "emails-urls/double-at" "emails-urls/no-at" >}}

## How long can an email address be?

The part before the @ can be 64 characters long, the most [RFC 5321](https://www.rfc-editor.org/rfc/rfc5321#section-4.5.3.1.1) allows. Test that length, then one character more, and check that the limit is enforced with a message, not by quietly cutting the address.

{{< values "emails-urls/very-long-local" "length-bombs/len-255" >}}

## Capital letters and spaces

Test what a person types without noticing: an address in capitals, and a space copied along with it from a document or a message.

{{< values "emails-urls/uppercase-local" "whitespace/trailing-space" "whitespace/leading-space" "whitespace/zero-width-space-x3" >}}

## And the website field next to it

A form with an email field often has a link field too. Test a link typed the way people type links, one that carries a password, and one long enough to break every column it is shown in.

{{< values "emails-urls/url-no-scheme" "emails-urls/url-with-credentials" "emails-urls/url-very-long" "emails-urls/url-unicode-path" >}}

## How do I go through all of them quickly?

With the palette, one shortcut per address: choose the pack **E-mail addresses and URLs**, click the field, press {{< kbd "Alt+Shift+N" >}}, look at the result, and press it again for the next. When an address breaks something, {{< kbd "Alt+Shift+B" >}} copies a report block for the ticket. [Getting started](/docs/getting-started/) takes about two minutes.

For an automated test, `nkb emit` prints the pack as JSON, CSV or one value per line:

```console
$ nkb emit emails-urls --format json > emails.json
```

Every address is on [the page of the pack](/packs/emails-urls/), with what a correct application does instead. For any other field, see [text box test cases](/test-cases-for-a-text-field/).
