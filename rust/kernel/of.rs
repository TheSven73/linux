// SPDX-License-Identifier: GPL-2.0

//! Devicetree and Open Firmware abstractions.
//!
//! C header: [`include/linux/of_*.h`](../../../../include/linux/of_*.h)

use crate::{bindings, c_types, str::CStr};

use core::ptr;

/// Wraps a kernel Open Firmware / devicetree match table.
///
/// Rust drivers may use this trait to match against devices
/// described in the devicetree.
pub trait OfMatchTable {
    /// Return the table as a sentinel-terminated C array.
    ///
    /// This is suitable to be assigned to the kernel's `of_match_table` field.
    ///
    /// # Invariants
    ///
    /// The returned pointer has static lifetime.
    fn as_ptr(&'static self) -> *const bindings::of_device_id;
}

/// An Open Firmware Match Table that can be constructed at build time.
// TODO remove doc(hidden) after rust#77647 gets fixed.
#[doc(hidden)]
pub struct ConstOfMatchTable<const N: usize>([bindings::of_device_id; N + 1])
where
    [bindings::of_device_id; N + 1]: Sized;

impl<const N: usize> ConstOfMatchTable<N>
where
    [bindings::of_device_id; N + 1]: Sized,
{
    /// Create a new Open Firmware Match Table from a list of compatible strings.
    pub const fn new_const(compatibles: [&'static CStr; N]) -> Self {
        let mut ids = [Self::zeroed_of_device_id(); N + 1];
        let mut i = 0_usize;
        loop {
            if i >= N {
                break;
            }
            ids[i] = Self::new_of_device_id(compatibles[i]);
            i += 1;
        }
        Self(ids)
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
        let mut i = 0_usize;
        loop {
            if i >= compatible.len() {
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

impl<const N: usize> OfMatchTable for ConstOfMatchTable<N>
where
    [bindings::of_device_id; N + 1]: Sized,
{
    fn as_ptr(&'static self) -> *const bindings::of_device_id {
        // INVARIANT: the returned pointer is created by dereferencing
        // a structure with static lifetime, therefore the pointer itself
        // has static lifetime.
        &self.0[0]
    }
}
