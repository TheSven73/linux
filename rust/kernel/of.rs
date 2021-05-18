// SPDX-License-Identifier: GPL-2.0

//! Devicetree and Open Firmware abstractions.
//!
//! C header: [`include/linux/of_*.h`](../../../../include/linux/of_*.h)

use alloc::boxed::Box;

use crate::{
    bindings, c_types,
    error::{Error, Result},
    types::PointerWrapper,
    CStr,
};

use core::mem::transmute;

/// Wraps a kernel Open Firmware / devicetree match table.
///
/// Rust drivers may create this structure to match against devices
/// described in the devicetree.
///
/// # Invariants
///
/// The last array element is always filled with zeros (the default).
pub struct OfMatchTable(Box<[bindings::of_device_id; 2]>);

impl OfMatchTable {
    /// Creates a [`OfMatchTable`] from a single `compatible` string.
    pub fn new(compatible: &CStr<'static>) -> Result<Self> {
        let tbl = Box::try_new([
            Self::new_of_device_id(compatible)?,
            bindings::of_device_id::default(),
        ])?;
        // INVARIANTS: we allocated an array with `default()` as its final
        // element, therefore that final element will be filled with zeros,
        // and the invariant above will hold.
        Ok(Self(tbl))
    }

    /// Transforms this [`OfMatchTable`] into a raw pointer.
    ///
    /// The resulting raw pointer is suitable to be assigned to a
    /// `bindings::device_driver::of_match_table`.
    pub(crate) fn into_table_ptr(self) -> *const bindings::of_device_id {
        // CAST: the kernel C API expects a pointer to an `bindings::of_device_id`
        // array, with the final element set to zeros, which serves as a
        // sentinel. This is exactly what we have created, as per the invariant
        // above, so the cast is safe.
        self.0.into_pointer().cast()
    }

    fn new_of_device_id(compatible: &CStr<'static>) -> Result<bindings::of_device_id> {
        let mut buf = [0_u8; 128];
        if compatible.len() > buf.len() {
            return Err(Error::EINVAL);
        }
        buf.get_mut(..compatible.len())
            .ok_or(Error::EINVAL)?
            .copy_from_slice(compatible.as_bytes());
        Ok(bindings::of_device_id {
            // SAFETY: re-interpretation from [u8] to [c_types::c_char] of same length is always safe.
            compatible: unsafe { transmute::<[u8; 128], [c_types::c_char; 128]>(buf) },
            ..Default::default()
        })
    }
}
