# somicus-ner

On-device person-name recognition for the Privacy Shield
([ADR-0042](../../docs/adr/ADR-0042-on-device-name-recognition.md)): `dslim/distilbert-NER` (Apache-2.0), a
`DistilBERT` token classifier, run by candle on the CPU behind the `NameRecognizer` trait of `somicus-privacy`.

**The model files are not in this repository.** The crate builds and its unit tests run without them; loading the
model needs the pinned files in a folder (see "Getting the model" below).

## What it does

- Loads the model only from verified bytes: each file (`model.safetensors` in 16-bit floats, and `tokenizer.json`) is
  read whole, its length and SHA-256 are checked against the pins compiled into the crate (`pins`), and the very bytes
  that were checked are parsed. No memory map, no pickle, no code in either format.
- Tokenizes with BERT's cased `WordPiece`, written here from `tokenizer.json`, so this crate decides how screen text
  is split: candle-core compiles the `tokenizers` crate, but this crate never calls it. Special tokens written in
  screen text are read as ordinary text.
- Finds `B-PER` and `I-PER` in what the rules left of a text: windows of at most 128 word pieces (a long text in
  windows overlapping by a quarter), packed into rows with a block-diagonal attention mask, a few rows per pass, until
  the turn's deadline. A text whose form hides names (file names, handles, text in one case) is read a second time in
  title case with its parts separated. Spans with interface words ("Max Width") or lower-case words other than name
  particles ("Mark as Read") are dropped.
- Caches the ranges found per text, keyed by a hash with a per-process random key: offsets, never text.
- Converts the pinned upstream 32-bit weights to the pinned 16-bit file reproducibly (`convert::to_half_precision`).
- Never opens a network connection, logs text or writes to disk.

## Getting the model

1. Download `model.safetensors` and `tokenizer.json` of `dslim/distilbert-NER` at commit
   `dfa2838a127384aabb82ed7719e16dab84c42a2a` from Hugging Face. Their lengths and SHA-256 must equal
   `pins::UPSTREAM_MODEL` and `pins::TOKENIZER`.
2. Convert the weights to 16-bit floats; the example refuses any other input and prints the result's pin, which must
   equal `pins::MODEL`:

   ```bash
   cargo run --release -p somicus-ner --example convert_name_model -- upstream/model.safetensors models/model.safetensors
   cp upstream/tokenizer.json models/
   ```

3. Point the tests at the folder: `SOMICUS_NAME_MODEL_DIR=$PWD/models`.

## Public API

`NameModel` (`load(folder)`, `load_pinned(model, pin, tokenizer)`, `clear_cache()`, and `NameRecognizer`);
`pins::{PinnedFile, UPSTREAM_MODEL, MODEL, TOKENIZER, folder}`; `load::{read_verified, verify, sha256_hex}`;
`convert::to_half_precision`; `NerError`; `MAX_PIECES`.

## Dependencies

`candle-core` and `candle-nn` 0.11.1 (pinned exactly, default features off; on macOS with Apple's Accelerate
framework), `safetensors` 0.8.0, `half` 2.7.1, `sha2`, `regex`, `serde`, `serde_json`, `thiserror`,
`somicus-privacy` (the trait) and `somicus-telemetry`. It forbids `unsafe` code.

## Testing

`cargo test -p somicus-ner` runs the unit tests: the tokenizer, the conversion, the hash check, the GELU, packing, the
second reading and the spans. `tests/name_model.rs` runs the model itself on synthetic sentences; it is `#[ignore]`d
because it needs the files, and fails without them when asked to run:

```bash
SOMICUS_NAME_MODEL_DIR=$PWD/models cargo test --release -p somicus-ner -- --ignored
```
