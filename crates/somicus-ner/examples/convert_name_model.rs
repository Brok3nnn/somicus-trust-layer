//! Converts the pinned upstream `model.safetensors` of `dslim/distilbert-NER` to the 16-bit file the name model loads
//! (ADR-0042), and prints the converted file's length and SHA-256, which must equal `pins::MODEL`.
//!
//! ```text
//! cargo run --release -p somicus-ner --example convert_name_model -- <UPSTREAM> <OUTPUT>
//! ```
//!
//! The input is refused unless it has the pinned upstream length and SHA-256 (`pins::UPSTREAM_MODEL`).

use std::io::Write as _;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut err = std::io::stderr();
    let [upstream, output] = args.as_slice() else {
        let _ = writeln!(err, "usage: convert_name_model <UPSTREAM> <OUTPUT>");
        return ExitCode::FAILURE;
    };
    let bytes = match std::fs::read(upstream) {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = writeln!(err, "failed to read {upstream}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let converted = match somicus_ner::convert::to_half_precision(&bytes) {
        Ok(converted) => converted,
        Err(error) => {
            let _ = writeln!(err, "failed to convert: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = std::fs::write(output, &converted) {
        let _ = writeln!(err, "failed to write {output}: {error}");
        return ExitCode::FAILURE;
    }
    let _ = writeln!(
        std::io::stdout(),
        "length {}\nsha256 {}",
        converted.len(),
        somicus_ner::load::sha256_hex(&converted)
    );
    ExitCode::SUCCESS
}
