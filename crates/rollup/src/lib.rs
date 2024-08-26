//! Rollup Node

#![doc(issue_tracker_base_url = "https://github.com/paradigmxyz/op-rs/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod driver;
pub use driver::Driver;

mod cli;
pub use cli::HeraArgsExt;

mod validator;
pub use validator::AttributesValidator;

mod pipeline;
pub use pipeline::{new_rollup_pipeline, RollupPipeline};

/// The identifier of the Hera Execution Extension.
pub const HERA_EXEX_ID: &str = "hera";
