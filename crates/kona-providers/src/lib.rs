//! Provider implementations for Kona trait abstractions

#![doc(issue_tracker_base_url = "https://github.com/paradigmxyz/op-rs/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(not(any(test, feature = "online")), no_std)]

extern crate alloc;

/// Re-export kona's derivation traits
pub use kona_derive::traits::*;
