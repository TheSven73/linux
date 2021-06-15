// SPDX-License-Identifier: GPL-2.0

//! Devicetree and Open Firmware abstractions.
//!
//! C header: [`include/linux/of_*.h`](../../../../include/linux/of_*.h)

use crate::{bindings, c_types, str::CStr};

use core::ops::Deref;
use core::ptr;

/// A kernel Open Firmware / devicetree match table.
///
/// Can only exist as an `&OfMatchTable` reference (akin to `&str` or
/// `&Path` in Rust std).
#[repr(transparent)]
pub struct OfMatchTable(bindings::of_device_id);

impl<const N: usize> Deref for ConstOfMatchTable<N> {
    type Target = OfMatchTable;

    fn deref(&self) -> &OfMatchTable {
        let head = &self.table[0] as *const bindings::of_device_id as *const OfMatchTable;
        // The returned reference must remain valid for the lifetime of `self`.
        // The raw pointer `head` points to memory inside `self`. So the reference created
        // from this raw pointer has the same lifetime as `self`.
        // Therefore this reference is safe to return.
        unsafe { &*head }
    }
}

impl OfMatchTable {
    /// Return the table as a static lifetime, sentinel-terminated C array.
    ///
    /// This is suitable to be assigned to the kernel's `of_match_table` field.
    pub fn as_ptr(&'static self) -> *const bindings::of_device_id {
        &self.0
    }
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
    /// Create a new Open Firmware Match Table from a list of compatible strings.
    pub const fn new_const(compatibles: [&'static CStr; N]) -> Self {
        let mut table = [Self::zeroed_of_device_id(); N];
        let mut i = 0;
        loop {
            if i == N {
                break;
            }
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
        loop {
            if i == compatible.len() {
                break;
            }
            // if `compatible` does not fit in `id.compatible`, an
            // "index out of bounds" build time exception will be triggered.
            id.compatible[i] = compatible[i] as c_types::c_char;
            i += 1;
        }
        id
    }
}
