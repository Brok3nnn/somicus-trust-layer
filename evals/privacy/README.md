# somicus-privacy-evals (the R-suite)

How well the Privacy Shield in [`crates/somicus-privacy`](../../crates/somicus-privacy/README.md) finds personal data
on synthetic screens, how much of what it replaces was personal, and how long it takes. It uses only the public API of
`somicus-privacy` (and `somicus-ner` for the model run), so the shipped crates carry no evaluation code.

- **R-suite v0** (`src/dataset.rs`, `run_r_suite`): 30 German, English and Hungarian samples, checksum identifiers and
  names, and a 2,000-node latency screen.
- **R-suite v1** (`src/v1`, `run_r_suite_v1`): 415 labelled synthetic English samples in `data/names.txt`,
  `data/identifiers.txt` and `data/negatives.txt` (one per line: its form, the screen text, and the parts that must
  not leave the device), plus v0's 15 checksum cases. It reports recall and precision for names, identifiers and
  contact details, recall per form (initials, names in file names, names that are also words, …), how many negatives
  were changed, the share of replaced letters that were personal, and the redaction time per screen of 40 texts.

Every name, address and number in the data is made up; card numbers are the networks' published test numbers.

## Running it

```bash
# Rules only: runs in `cargo test`; --nocapture prints the measurements.
cargo test -p somicus-privacy-evals -- --nocapture

# With the name model (see crates/somicus-ner/README.md for the files): ignored by default.
SOMICUS_NAME_MODEL_DIR=$PWD/models \
  cargo test --release -p somicus-privacy-evals --test r_suite_v1_model -- --ignored --test-threads 1 --nocapture
```

Set `SOMICUS_NAME_MODEL_UPSTREAM` to the upstream 32-bit `model.safetensors` to also check that the 16-bit weights find
the same names. Latency thresholds are asserted only with `SOMICUS_TIMING_ASSERTIONS=1` on a quiet machine; otherwise
the tests print the measured p95.

## Results

Rules only, as `cargo test` measures them in this repository (debug build, Apple Silicon, 2026-11-26):

| Measure | Rules only |
|---|---|
| Person names, recall / precision | 14.0 % (38 of 272) / 88.9 % |
| Identifiers (cards, IBANs, national IDs), recall | 100.0 % (42 of 42) |
| Contact details (e-mail, phone), recall | 92.3 % (24 of 26) |
| Replaced letters that were personal | 80.5 % |
| Negatives changed | 5 of 104 |
| Per English screen of 40 texts, p50 / p95 | 0.4 / 1.6 ms |

With the name model, measured on 2026-11-08 in a release build on an Apple M2 Pro (the files are not in this
repository, so `cargo test` does not reproduce this column):

| Measure | Rules and name model | Target |
|---|---|---|
| Person names, recall / precision | 95.2 % (259 of 272) / 97.9 % | recall ≥ 90 % |
| Identifiers, recall | 100.0 % (42 of 42) | ≥ 99 % |
| Replaced letters that were personal | 89.7 % | — |
| Negatives changed | 6 of 104 | — |
| Per new screen of 40 texts, p50 / p95 | 40 / 57 ms | p95 ≤ 40 ms |
| The same screen cached (next turn), p95 | 0.2 ms | — |

The rules alone meet the identifier target and miss the name target by far; the model meets both recall targets. The
cold-screen p95 misses the 40 ms target; it was accepted with the target held on the cached screen and a 40 ms
deadline per turn, after which unscanned texts go with the rules only ([ADR-0042](../../docs/adr/ADR-0042-on-device-name-recognition.md)).
The two contact details the rules miss are phone numbers (12 of 14 found).

## Tests

`tests/recall_and_latency.rs` asserts v0's thresholds (checksum identifiers ≥ 99 %, names ≥ 90 % on v0's samples).
`tests/r_suite_v1.rs` checks the data set's sizes and asserts that the rules-only v1 results above do not get worse
(each floor may only rise). `tests/r_suite_v1_model.rs` asserts the recall targets and the model's floors with the name
model.
