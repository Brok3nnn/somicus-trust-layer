# ADR-0042: On-device person-name recognition

- **Status:** Accepted
- **Date:** 2026-11-07, measured 2026-11-08

## Context

The Privacy Shield finds person names with rules: a title or a cue word before a capitalised pair, or a pair with one
half on a short list. On R-suite v1 (labelled synthetic English screens) the rules find 14 % of person names. Longer
lists cannot close the gap: a list cannot know every name, and every name it adds that is also a word ("Grace",
"Mark", "Bill") replaces that word on ordinary screens.

Requirements for a model: licensed Apache-2.0 or MIT; weights in a format that cannot run code (safetensors or ONNX,
never pickle); small enough for a desktop download (preferably under 150 MB) and fast enough for the R-suite target
(p95 ≤ 40 ms per English screen); runs on the CPU of an ordinary laptop on macOS, Windows and Linux, inside the
process where the screen text already is.

## Decision

Recognise person names on the device with **`dslim/distilbert-NER`** run by **candle**, as one more recognizer of the
Privacy Shield, and keep the rules.

1. **The model.** `dslim/distilbert-NER` (Apache-2.0, DistilBERT, 6 layers, 66 M parameters, cased) at commit
   `dfa2838a127384aabb82ed7719e16dab84c42a2a`: `model.safetensors` (260,803,668 bytes, SHA-256
   `235ecd699f650ca36513b122d11ce5a80339be1388f06c33c2278368b99914e1`) and `tokenizer.json` (669,021 bytes, SHA-256
   `cb26b43c98e8266ae3e99c2a583cf8315d73b33a17e6b20b4df7ff1f22392d34`). Only the `PER` labels are used: organisations
   and places stay, because guidance needs them ("Open the invoice for Hale Trading Ltd").
2. **16-bit weights.** The upstream fp32 weights are converted once, reproducibly, to fp16 safetensors (same tensors,
   name order; `convert::to_half_precision`). Measured on 2026-11-08: the fp16 model finds the same name spans as the
   fp32 one on all 430 R-suite v1 samples, so the pinned file is the fp16 one: 130,407,898 bytes, SHA-256
   `2ba3be83cefc98856af65a4b846fb3c76fc7a490cb63434d7a605530bf092597` (`somicus_ner::pins::MODEL`).
3. **Distribution.** The app downloads the pinned files once, through `somicus-net`'s `Downloader` only, from the
   product's own download host (never from a third party, so no third party learns which devices run Somicus): HTTPS
   `GET` without cookies, credentials or a query, never longer than the pinned length, hashed while it arrives and
   moved into place only when length and SHA-256 match. The pins are constants compiled into the app; changing them is
   a security-sensitive change.
4. **Loading.** A separate crate, `somicus-ner`, holds the tokenizer and a DistilBERT token-classification forward
   pass on `candle-core` and `candle-nn` (pinned exactly, default features off, Apple's Accelerate on macOS) and
   implements the `NameRecognizer` trait that `somicus-privacy` defines, so the shield itself gains no machine-learning
   dependency. The whole file is read into memory, hashed again and parsed from the same buffer: the bytes that were
   checked are the bytes that are used, with no memory map and no `unsafe`.
5. **Use.** The model reads what the rules left (identifiers, secrets and contact details are blanked out first) and
   can only add replacements. Results are cached per text (offsets, never text).
6. **When it is missing,** or fails to verify or load, the shield works with the rules only and says so
   (`NameProtection::RulesOnly` with the reason).
7. **Time budget.** Each turn gives the model a deadline (40 ms in an optimised build). Texts it has not reached go
   with the rules only (`NameProtection::Partial`) and are scanned in the background for the next turn. Sequences are
   capped at 128 word pieces; longer texts are read in overlapping windows.
8. **Measurement.** R-suite v1 has a model-backed run with the targets (names ≥ 90 %, identifiers ≥ 99 %) as
   assertions; it fails, not skips, when asked to run without the model.

## Measured (2026-11-08, release build, Apple M2 Pro)

| | Rules only | Rules and name model | Target |
|---|---|---|---|
| Person names, recall | 14.0 % (38 of 272) | 95.2 % (259 of 272) | ≥ 90 % |
| Person names, precision | 88.9 % | 97.9 % | — |
| Identifiers, recall | 100.0 % (42 of 42) | 100.0 % (42 of 42) | ≥ 99 % |
| Per new English screen of 40 texts, p50 / p95 | 0.5 / 1.5 ms (debug) | 40 / 57 ms | p95 ≤ 40 ms |
| The same screen cached (next turn), p95 | — | 0.2 ms | — |

The recall targets are met. The model misses names in file names most (15 of 20 found), then names that are also words
(21 of 25) and initials (18 of 20). The cold-screen p95 misses the target by about 17 ms; splitting passes over threads
made it worse, because Accelerate already spreads one matrix product over the cores. **Accepted as measured:** the
40 ms target holds on the screen the next turn sees (cached), and a new screen is bounded by the turn's 40 ms
deadline, so on a dense new screen the texts the model has not reached go with the rules only for that one turn. The
cold p95 is a ceiling that may only fall.

## Alternatives considered

| Option | Why not |
|---|---|
| Rules only, with longer name lists | 14 % recall; lists break words that are also names |
| Name recognition on the server | Raw names leave the device, which ADR-0014 exists to prevent |
| The operating system's tagger (macOS `NLTagger`) | macOS only, behaviour changes with OS updates |
| `ort` (ONNX Runtime) with an int8 model | A native library to vendor, sign and update, or a download of native code |
| `tract` with ONNX | Slower on transformer encoders |
| A GLiNER-family PII model | 5–10 times the size, slower, unclear licences on fine-tunes |
| Download from Hugging Face directly | A third party sees every device; the egress allow-list grows |

## Consequences

- (+) Names in file names, initials, non-English names and names that are also words become findable on the device.
- (+) No native library, no code-executing format, no new egress host; the model can only add redactions.
- (−) A 130 MB download and a cold-screen latency above target, bounded by the per-turn deadline.
