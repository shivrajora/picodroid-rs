// SPDX-License-Identifier: GPL-3.0-only
//! Collector tests, by what they exercise: plain reachability, allocation
//! stress, the map / set and iterator side stores (and their stress runs),
//! the `GcState` lifecycle fields, and the `Linked*` aliases. The topic files
//! reach the collector and these imports through `use super::*`.

use super::*;
use crate::array_heap::encode_ref;
use crate::names::c;
use alloc::format;

mod iterators;
mod linked_aliases;
mod maps;
mod maps_stress;
mod reachability;
mod state_fields;
mod stress;
