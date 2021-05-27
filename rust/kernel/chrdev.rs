// SPDX-License-Identifier: GPL-2.0

//! Character devices.
//!
//! Also called "char devices", `chrdev`, `cdev`.
//!
//! C header: [`include/linux/cdev.h`](../../../../include/linux/cdev.h)
//!
//! Reference: <https://www.kernel.org/doc/html/latest/core-api/kernel-api.html#char-devices>

use core::convert::TryInto;

use crate::bindings;
use crate::c_types;
use crate::error::{Error, Result};
use crate::file_operations;
use crate::str::CStr;

/// Character device.
///
/// # Invariants
///
/// - [`self.0`] is valid and non-null.
/// - [`(*self.0).ops`] is valid, non-null and has static lifetime.
/// - [`(*self.0).owner`] is valid and, if non-null, has module lifetime.
struct Cdev(*mut bindings::cdev);

impl Cdev {
    fn alloc(
        fops: &'static bindings::file_operations,
        module: &'static crate::ThisModule,
    ) -> Result<Self> {
        // SAFETY: FFI call.
        let cdev = unsafe { bindings::cdev_alloc() };
        if cdev.is_null() {
            return Err(Error::ENOMEM);
        }
        // SAFETY: `cdev` is valid and non-null since `cdev_alloc()`
        // returned a valid pointer which was null-checked.
        unsafe {
            (*cdev).ops = fops;
            (*cdev).owner = module.0;
        }
        // INVARIANTS:
        // - [`self.0`] is valid and non-null.
        // - [`(*self.0).ops`] is valid, non-null and has static lifetime,
        //   because it was coerced from a reference with static lifetime.
        // - [`(*self.0).owner`] is valid and, if non-null, has module lifetime,
        //   guaranteed by the [`ThisModule`] invariant.
        Ok(Self(cdev))
    }

    fn add(&mut self, dev: bindings::dev_t, count: c_types::c_uint) -> Result {
        // SAFETY: according to the type invariants:
        // - [`self.0`] can be safely passed to [`bindings::cdev_add`].
        // - [`(*self.0).ops`] will live at least as long as [`self.0`].
        // - [`(*self.0).owner`] will live at least as long as the
        //   module, which is an implicit requirement.
        let rc = unsafe { bindings::cdev_add(self.0, dev, count) };
        if rc != 0 {
            return Err(Error::from_kernel_errno(rc));
        }
        Ok(())
    }
}

impl Drop for Cdev {
    fn drop(&mut self) {
        // SAFETY: [`self.0`] is valid and non-null by the type invariants.
        unsafe {
            bindings::cdev_del(self.0);
        }
    }
}

struct RegistrationInner<const N: usize> {
    dev: bindings::dev_t,
    used: usize,
    cdevs: [Option<Cdev>; N],
}

/// Character device registration.
///
/// May contain up to a fixed number (`N`) of devices.
pub struct Registration<const N: usize> {
    name: &'static CStr,
    minors_start: u16,
    this_module: &'static crate::ThisModule,
    inner: Option<RegistrationInner<N>>,
}

impl<const N: usize> Registration<{ N }> {
    /// Creates a [`Registration`] object for a character device.
    ///
    /// This does *not* register the device: see [`Self::register()`].
    pub fn new(
        name: &'static CStr,
        minors_start: u16,
        this_module: &'static crate::ThisModule,
    ) -> Self {
        Registration {
            name,
            minors_start,
            this_module,
            inner: None,
        }
    }

    /// Registers a character device.
    ///
    /// You may call this once per device type, up to `N` times.
    pub fn register<T: file_operations::FileOpener<()>>(&mut self) -> Result {
        if self.inner.is_none() {
            let mut dev: bindings::dev_t = 0;
            // SAFETY: Calling unsafe function. `this.name` has `'static`
            // lifetime.
            let res = unsafe {
                bindings::alloc_chrdev_region(
                    &mut dev,
                    self.minors_start.into(),
                    N.try_into()?,
                    self.name.as_char_ptr(),
                )
            };
            if res != 0 {
                return Err(Error::from_kernel_errno(res));
            }
            const NONE: Option<Cdev> = None;
            self.inner = Some(RegistrationInner {
                dev,
                used: 0,
                cdevs: [NONE; N],
            });
        }

        let mut inner = self.inner.as_mut().unwrap();
        if inner.used == N {
            return Err(Error::EINVAL);
        }

        // SAFETY: The adapter doesn't retrieve any state yet, so it's compatible with any
        // registration.
        let fops = unsafe { file_operations::FileOperationsVtable::<Self, T>::build() };
        let mut cdev = Cdev::alloc(fops, &self.this_module)?;
        cdev.add(inner.dev + inner.used as bindings::dev_t, 1)?;
        inner.cdevs[inner.used].replace(cdev);
        inner.used += 1;
        Ok(())
    }
}

impl<const N: usize> file_operations::FileOpenAdapter for Registration<{ N }> {
    type Arg = ();

    unsafe fn convert(
        _inode: *mut bindings::inode,
        _file: *mut bindings::file,
    ) -> *const Self::Arg {
        // TODO: Update the SAFETY comment on the call to `FileOperationsVTable::build` above once
        // this is updated to retrieve state.
        &()
    }
}

// SAFETY: `Registration` does not expose any of its state across threads
// (it is fine for multiple threads to have a shared reference to it).
unsafe impl<const N: usize> Sync for Registration<{ N }> {}

impl<const N: usize> Drop for Registration<{ N }> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.as_mut() {
            // Replicate kernel C behaviour: drop [`Cdev`]s before calling
            // [`bindings::unregister_chrdev_region`].
            for i in 0..inner.used {
                inner.cdevs[i].take();
            }
            // SAFETY: [`self.inner`] is Some, so [`inner.dev`] was previously
            // created using [`bindings::alloc_chrdev_region`].
            unsafe {
                bindings::unregister_chrdev_region(inner.dev, N.try_into().unwrap());
            }
        }
    }
}
