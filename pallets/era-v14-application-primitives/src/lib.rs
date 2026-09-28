//! ERA V14 application-economy primitives.
//!
//! The adopted dormant SDK contract is in [`assets::v1`]. Constants and schemas are frozen
//! without installing runtime storage, dispatchables or APIs. Historical ledger fixtures are
//! excluded from normal builds and do not specify integrated role/lifecycle behavior. ETKN is represented only by [`assets::FungibleAsset::NativeEtkn`]; asset
//! creation can create only registered non-native assets.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod amm;
pub mod assets;

#[cfg(test)]
mod amm_tests;
