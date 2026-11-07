//! Build a synthetic vault for benchmarks and manual perf runs.
//!
//!     cargo run --release --example gen_vault -p herbarium-core -- <pages> <out-dir>
//!
//! The vault is written through the public operations, so it is always in the
//! format the current core writes. See `benches/support/vaultgen.rs` for its shape.

#[path = "../benches/support/vaultgen.rs"]
mod vaultgen;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(n), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: gen_vault <pages> <out-dir>");
        std::process::exit(2);
    };
    let n: usize = n.parse().expect("<pages> must be a number");
    let out = std::path::PathBuf::from(out);
    if out.join(".herbarium").exists() {
        eprintln!(
            "{} already holds a vault; pass an empty directory",
            out.display()
        );
        std::process::exit(1);
    }
    vaultgen::generate(&out, n);
}
