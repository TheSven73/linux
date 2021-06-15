// SPDX-License-Identifier: GPL-2.0

//! Devicetree and Open Firmware abstractions.
//!
//! C header: [`include/linux/of_*.h`](../../../../include/linux/of_*.h)

use crate::{bindings, c_types, str::CStr};

use core::ptr;

/// A kernel Open Firmware / devicetree match table.
///
/// Rust drivers may use this trait to match against devices
/// described in the devicetree.
pub trait OfMatchTable {
    /// Returns the table as a reference to a static lifetime, sentinel-terminated C array.
    ///
    /// This is suitable to be coerced to the kernel's `of_match_table` field.
    fn as_ptr(&'static self) -> &'static bindings::of_device_id;
}

/// An Open Firmware Match Table that can be constructed at build time.
///
/// # Invariants
///
/// `sentinel` always contains zeroes.
#[repr(C)]
pub struct ConstOfMatchTable<const N: usize> {
    table: [bindings::of_device_id; N],
    sentinel: bindings::of_device_id,
}

impl<const N: usize> ConstOfMatchTable<N> {
    /// Creates a new Open Firmware Match Table from a list of compatible strings.
    pub const fn new_const(compatibles: [&'static CStr; N]) -> Self {
        let mut table = [Self::zeroed_of_device_id(); N];
        let mut i = 0;
        while i < N {
            table[i] = Self::new_of_device_id(compatibles[i]);
            i += 1;
        }
        Self {
            table,
            // INVARIANT: we zero the sentinel here, and never change it
            // anwhere. Therefore it always contains zeroes.
            sentinel: Self::zeroed_of_device_id(),
        }
    }

    const fn zeroed_of_device_id() -> bindings::of_device_id {
        bindings::of_device_id {
            name: [0; 32usize],
            type_: [0; 32usize],
            compatible: [0; 128usize],
            data: ptr::null(),
        }
    }

    const fn new_of_device_id(compatible: &'static CStr) -> bindings::of_device_id {
        let mut id = Self::zeroed_of_device_id();
        let compatible = compatible.as_bytes_with_nul();
        let mut i = 0;
        while i < compatible.len() {
            // if `compatible` does not fit in `id.compatible`, an
            // "index out of bounds" build time exception will be triggered.
            id.compatible[i] = compatible[i] as c_types::c_char;
            i += 1;
        }
        id
    }
}

impl<const N: usize> OfMatchTable for ConstOfMatchTable<N> {
    fn as_ptr(&'static self) -> &'static bindings::of_device_id {
        // The array is sentinel-terminated, by the invariant above.
        // The returned pointer is created by dereferencing
        // a structure with static lifetime, therefore the pointer itself
        // has static lifetime.
        &self.table[0]
    }
}
