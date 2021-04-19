// SPDX-License-Identifier: GPL-2.0

//! Character devices.
//!
//! Also called "char devices", `chrdev`, `cdev`.
//!
//! C header: [`include/linux/cdev.h`](../../../../include/linux/cdev.h)
//!
//! Reference: <https://www.kernel.org/doc/html/latest/core-api/kernel-api.html#char-devices>

use alloc::boxed::Box;
use core::convert::TryInto;
use core::marker::PhantomPinned;
use core::mem::MaybeUninit;
use core::pin::Pin;

use crate::bindings;
use crate::c_types;
use crate::error::{Error, KernelResult};
use crate::file_operations;
use crate::types::CStr;

struct SafeCdev(*mut bindings::cdev);

impl SafeCdev {
    fn alloc() -> KernelResult<Self> {
        // SAFETY: call an unsafe function
        let cdev = unsafe { bindings::cdev_alloc() };
        if cdev.is_null() {
            return Err(Error::ENOMEM);
        }
        Ok(Self(cdev))
    }

    fn init(&mut self, fops: *const bindings::file_operations) {
        // SAFETY: call an unsafe function
        unsafe {
            bindings::cdev_init(self.0, fops);
        }
    }

    fn add(&mut self, dev: bindings::dev_t, pos: c_types::c_uint) -> KernelResult<()> {
        // SAFETY: call an unsafe function
        let rc = unsafe { bindings::cdev_add(self.0, dev, pos) };
        if rc != 0 {
            return Err(Error::from_kernel_errno(rc));
        }
        Ok(())
    }

    fn set_owner(&mut self, module: &crate::ThisModule) {
        // SAFETY: dereference of a raw pointer
        unsafe {
            (*self.0).owner = module.0;
        }
    }
}

fn new_none_array<T, const N: usize>() -> [Option<T>; N] {
    // SAFETY: manipulate MaybeUninit memory
    unsafe {
        let mut arr: [MaybeUninit<Option<T>>; N] = MaybeUninit::uninit_array();
        for elem in &mut arr {
            elem.as_mut_ptr().write(None);
        }
        MaybeUninit::array_assume_init(arr)
    }
}

impl Drop for SafeCdev {
    fn drop(&mut self) {
        // SAFETY: call an unsafe function
        unsafe {
            bindings::cdev_del(self.0);
        }
    }
}

struct RegistrationInner<const N: usize> {
    dev: bindings::dev_t,
    used: usize,
    cdevs: [Option<SafeCdev>; N],
    _pin: PhantomPinned,
}

/// Character device registration.
///
/// May contain up to a fixed number (`N`) of devices. Must be pinned.
pub struct Registration<const N: usize> {
    name: CStr<'static>,
    minors_start: u16,
    this_module: &'static crate::ThisModule,
    inner: Option<RegistrationInner<N>>,
}

impl<const N: usize> Registration<{ N }> {
    /// Creates a [`Registration`] object for a character device.
    ///
    /// This does *not* register the device: see [`Self::register()`].
    ///
    /// This associated function is intended to be used when you need to avoid
    /// a memory allocation, e.g. when the [`Registration`] is a member of
    /// a bigger structure inside your [`crate::KernelModule`] instance. If you
    /// are going to pin the registration right away, call
    /// [`Self::new_pinned()`] instead.
    pub fn new(
        name: CStr<'static>,
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

    /// Creates a pinned [`Registration`] object for a character device.
    ///
    /// This does *not* register the device: see [`Self::register()`].
    pub fn new_pinned(
        name: CStr<'static>,
        minors_start: u16,
        this_module: &'static crate::ThisModule,
    ) -> KernelResult<Pin<Box<Self>>> {
        Ok(Pin::from(Box::try_new(Self::new(
            name,
            minors_start,
            this_module,
        ))?))
    }

    /// Registers a character device.
    ///
    /// You may call this once per device type, up to `N` times.
    pub fn register<T: file_operations::FileOpener<()>>(self: Pin<&mut Self>) -> KernelResult {
        // SAFETY: We must ensure that we never move out of `this`.
        let this = unsafe { self.get_unchecked_mut() };
        if this.inner.is_none() {
            let mut dev: bindings::dev_t = 0;
            // SAFETY: Calling unsafe function. `this.name` has `'static`
            // lifetime.
            let res = unsafe {
                bindings::alloc_chrdev_region(
                    &mut dev,
                    this.minors_start.into(),
                    N.try_into()?,
                    this.name.as_ptr() as *const c_types::c_char,
                )
            };
            if res != 0 {
                return Err(Error::from_kernel_errno(res));
            }
            this.inner = Some(RegistrationInner {
                dev,
                used: 0,
                cdevs: new_none_array(),
                _pin: PhantomPinned,
            });
        }

        let mut inner = this.inner.as_mut().unwrap();
        if inner.used == N {
            return Err(Error::EINVAL);
        }

        let mut cdev = SafeCdev::alloc()?;
        // SAFETY: The adapter doesn't retrieve any state yet, so it's compatible with any
        // registration.
        let fops = unsafe { file_operations::FileOperationsVtable::<Self, T>::build() };
        cdev.init(fops);
        cdev.set_owner(&this.this_module);
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
            for i in 0..inner.used {
                inner.cdevs[i].take();
            }
            // SAFETY: Calling unsafe function
            unsafe {
                bindings::unregister_chrdev_region(inner.dev, N.try_into().unwrap());
            }
        }
    }
}
