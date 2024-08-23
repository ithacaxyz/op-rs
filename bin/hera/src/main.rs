#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/paradigmxyz/op-rs/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use clap::Parser;
use eyre::Result;

mod hera;
use hera::HeraCli;

/// The identifier of the Hera Execution Extension.
const EXEX_ID: &str = "hera";

fn main() -> Result<()> {
    HeraCli::parse().run()
}
