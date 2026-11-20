//! The macOS side of cloud_ai (cloud_ai.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// No older Sora ever ran on macOS: nothing to move.
pub(super) fn keys_from_vault(_dir: &std::path::Path) {}
