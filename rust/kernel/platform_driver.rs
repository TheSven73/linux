// SPDX-License-Identifier: GPL-2.0

use crate::error::{Error, KernelResult};
use crate::{bindings, c_types, CStr};
use alloc::boxed::Box;
use core::marker::PhantomPinned;
use core::mem::transmute;
use core::pin::Pin;

extern "C" {
    #[allow(improper_ctypes)]
    fn rust_helper_platform_get_drvdata(
        pdev: *const bindings::platform_device,
    ) -> *mut c_types::c_void;

    #[allow(improper_ctypes)]
    fn rust_helper_platform_set_drvdata(
        pdev: *mut bindings::platform_device,
        data: *mut c_types::c_void,
    );
}

unsafe extern "C" fn probe_callback<T: PlatformDriver>(
    pdev: *mut bindings::platform_device,
) -> c_types::c_int {
    let f = || {
        let drv_data = T::probe(&mut PlatformDevice::new(pdev))?;
        let drv_data = Box::try_new(drv_data)?;
        let drv_data = Box::into_raw(drv_data) as *mut c_types::c_void;
        Ok(drv_data) as KernelResult<_>
    };
    // TODO don't we need to pin this?
    let ptr = match f() {
        Ok(ptr) => ptr,
        Err(e) => return e.to_kernel_errno(),
    };
    rust_helper_platform_set_drvdata(pdev, ptr);
    0
}

fn new_of_device_id(compatible: &CStr<'static>) -> KernelResult<bindings::of_device_id> {
    // TODO:
    // - fail at build time if compatible CStr doesn't fit.
    // - can we do this safely without transmute?
    let mut buf = [0_u8; 128];
    if compatible.len() > buf.len() {
        return Err(Error::EINVAL);
    }
    // PANIC: this will never panic: `compatible` is not longer than `buf`.
    buf[..compatible.len()].copy_from_slice(compatible.as_bytes());
    Ok(bindings::of_device_id {
        // SAFETY: re-interpretation from [u8] to [i8] of same length is always safe.
        compatible: unsafe { transmute::<[u8; 128], [i8; 128]>(buf) },
        ..Default::default()
    })
}

unsafe extern "C" fn remove_callback<T: PlatformDriver>(
    pdev: *mut bindings::platform_device,
) -> c_types::c_int {
    let ptr = rust_helper_platform_get_drvdata(pdev);
    let drv_data: Box<T::DrvData> = Box::from_raw(ptr as _);
    drop(drv_data);
    0
}

/// A registration of a platform driver.
#[derive(Default)]
pub struct Registration {
    registered: bool,
    pdrv: bindings::platform_driver,
    of_table: [bindings::of_device_id; 2],
    _pin: PhantomPinned,
}

impl Registration {
    fn register<P: PlatformDriver>(
        self: Pin<&mut Self>,
        name: CStr<'static>,
        of_id: CStr<'static>,
        module: &'static crate::ThisModule,
    ) -> KernelResult {
        // SAFETY: We must ensure that we never move out of `this`.
        let this = unsafe { self.get_unchecked_mut() };
        if this.registered {
            // Already registered.
            return Err(Error::EINVAL);
        }
        // TODO should create a variable size table here.
        this.of_table[0] = new_of_device_id(&of_id)?;
        // SAFETY: `name` pointer has static lifetime.
        // `of_table` points to memory in `this`, which lives as least as
        // long as the `platform_device` registration.
        // `module.0` lives as least as long as the module.
        this.pdrv.driver.name = name.as_ptr() as *const c_types::c_char;
        this.pdrv.driver.of_match_table = this.of_table.as_ptr();
        this.pdrv.probe = Some(probe_callback::<P>);
        this.pdrv.remove = Some(remove_callback::<P>);
        let ret = unsafe { bindings::__platform_driver_register(&mut this.pdrv, module.0) };
        if ret < 0 {
            return Err(Error::from_kernel_errno(ret));
        }
        this.registered = true;
        Ok(())
    }

    pub fn new_pinned<P: PlatformDriver>(
        name: CStr<'static>,
        of_id: CStr<'static>,
        module: &'static crate::ThisModule,
    ) -> KernelResult<Pin<Box<Self>>> {
        let mut r = Pin::from(Box::try_new(Self::default())?);
        r.as_mut().register::<P>(name, of_id, module)?;
        Ok(r)
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if self.registered {
            // SAFETY: if `registered` is true, then `self.pdev` was registered
            // previously, which means `platform_driver_unregister` is always
            // safe to call.
            unsafe { bindings::platform_driver_unregister(&mut self.pdrv) }
        }
    }
}

// SAFETY: `Registration` does not expose any of its state across threads
// (it is fine for multiple threads to have a shared reference to it).
unsafe impl Sync for Registration {}

pub struct PointerWrapper<T: ?Sized>(*mut T);

impl<T: ?Sized> PointerWrapper<T> {
    fn new(ptr: *mut T) -> Self {
        Self(ptr)
    }

    pub(crate) fn to_ptr(&self) -> *mut T {
        self.0
    }
}

/// Rust abstraction of a kernel `struct platform_device`.
pub type PlatformDevice = PointerWrapper<bindings::platform_device>;

/// Rust abstraction of a kernel `struct device`.
pub(crate) trait Device {
    fn to_dev_ptr(&self) -> *mut bindings::device;
}

impl Device for PlatformDevice {
    fn to_dev_ptr(&self) -> *mut bindings::device {
        // SAFETY: a `struct platform_device` is-a `struct device`, and
        // can always be accessed by a pointer to its inner `struct device`.
        unsafe { &mut (*self.0).dev }
    }
}

/// Rust abstraction of a kernel `struct platform_driver`
pub trait PlatformDriver {
    /// Per-instance driver data (or private driver data)
    type DrvData;

    fn probe(pdev: &mut PlatformDevice) -> KernelResult<Self::DrvData>;
}
