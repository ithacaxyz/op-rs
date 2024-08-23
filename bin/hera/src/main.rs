use clap::Parser;
use eyre::Result;

mod hera;
use hera::HeraCli;

/// The identifier of the Hera Execution Extension.
const EXEX_ID: &str = "hera";

fn main() -> Result<()> {
    HeraCli::parse().run()
}
