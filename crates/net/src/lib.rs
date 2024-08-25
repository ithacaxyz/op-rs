//! Consensus Networking Library

#![doc(issue_tracker_base_url = "https://github.com/paradigmxyz/op-rs/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

pub mod behaviour;
pub mod bootnodes;
pub mod builder;
pub mod config;
pub mod discovery;
pub mod driver;
pub mod event;
pub mod handler;
pub mod op_enr;
pub mod types;
