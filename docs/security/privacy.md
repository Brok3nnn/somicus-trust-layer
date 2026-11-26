# Privacy: the on-device Privacy Shield

## Principles

1. **Minimise at the source.** Field values are not sent; what leaves the device is structure (roles, states,
   positions) and text that passed the Privacy Shield.
2. **Purpose limitation.** Screen data is used to answer the current request, not for training.
3. **Privacy by default.** When a recognizer cannot run, the whole text becomes one token; when the name model is
   missing, the shield says so and works with its rules.

## The pipeline

Every outgoing text (screen labels and static text, window titles, the person's request) goes through
`somicus-privacy`:

1. **Classification.** Each text arrives as a `ClassifiedText` with a `PrivacyClass`: `public_label` (names of
   controls and menus), `content_value` (static text, window titles), `personal` or `secret`. A `secret` text (a
   password field's value) becomes one token whole; a `personal` text does too unless the recognizers find
   the personal parts in it.
2. **Recognizers**, deterministic and multilingual: e-mail addresses, phone numbers, payment cards (Luhn), IBANs
   (mod 97), US social security numbers, UK National Insurance numbers, the German tax ID, the Hungarian tax ID and
   TAJ number, postal addresses, API keys, bearer tokens and password-like strings, and person names found by rules
   (titles, cue words, a list of known names) in English, German and Hungarian. A window's URL keeps only its origin;
   an executable path keeps only its file name.
3. **The name model** (optional, [ADR-0042](../adr/ADR-0042-on-device-name-recognition.md)): an English person-name
   model (DistilBERT, Apache-2.0, 130 MB as 16-bit safetensors) run on the CPU by candle in `somicus-ner`, verified
   against compiled-in hashes at every load. It reads only what the rules left (identifiers, secrets and contact
   details are blanked out first) and can only add replacements. Each turn gives it a deadline; texts it has not
   reached go with the rules only and are scanned for the next turn.
4. **Pseudonymization.** Each finding is replaced by a typed, session-consistent token (`⟦PERSON_1⟧`, `⟦IBAN_1⟧`):
   the same value maps to the same token within a session, so the model can still refer to it.
5. **Token vault.** The mapping lives only in memory (`TokenVault`), is never sent, and is zeroized when it is
   dropped. Answers that contain tokens are rehydrated on the device for display.
6. **The brand.** The result is a `Redacted<T>`, the only body type `somicus-net` sends.

## Requests that carry no redacted body

| Request | What it carries | Bounds |
|---|---|---|
| Website icon ([ADR-0027](../adr/ADR-0027-website-icons.md)) | `https://<host>/favicon.ico` of a page the person has open | Public hosts only, no cookies, at most 64 KiB of image, 2 s |
| Name-model download ([ADR-0042](../adr/ADR-0042-on-device-name-recognition.md)) | A `GET` of a pinned file | No query, cookies or credentials, no redirects, never past the pinned length, SHA-256 checked |

## How well it works

Measured by the R-suite ([`evals/privacy`](../../evals/privacy/README.md)) on 430 labelled synthetic samples:

| | Rules only | Rules and name model |
|---|---|---|
| Person names, recall | 14.0 % | 95.2 % |
| Identifiers (cards, IBANs, national IDs), recall | 100.0 % | 100.0 % |
| Contact details (e-mail, phone), recall | 92.3 % | 92.3 % |
| Negatives changed | 5 of 104 | 6 of 104 |

The rules-only column is reproduced by `cargo test` in this repository; the model column was measured on 2026-11-08
with the pinned model files, which are not included.

## Limits

- A name the shield misses leaves the device in clear text. Without the model, most names do.
- Organisations and places are deliberately not redacted: guidance needs them.
- Images are not handled here; no screenshot path exists in this repository.
