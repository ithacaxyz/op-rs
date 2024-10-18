#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/ithacaxyz/op-rs/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

pub mod builder;
pub mod discovery;
pub mod driver;
pub mod gossip;
pub mod types;
