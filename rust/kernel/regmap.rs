// SPDX-License-Identifier: GPL-2.0

use crate::error::{ptr_err_check, Error, KernelResult};
use crate::platform_driver::{Device, PlatformDevice};
use crate::{bindings, c_types};
use core::mem::MaybeUninit;

/// Abstraction wrapping a kernel `struct regmap`.
///
/// # Invariants
///
/// regmap locking is never disabled.
pub struct Regmap(*mut bindings::regmap);

// SAFETY: we access the underlying `struct regmap` only through its helper functions,
// and we never disable locking by the type invariant, so:
// - `struct regmap *` can be safely sent between threads (Send)
// - `struct regmap`'s helper functions can be safely called from any thread (Sync)
unsafe impl Send for Regmap {}
unsafe impl Sync for Regmap {}

impl Regmap {
    pub fn write(&self, reg: c_types::c_uint, val: c_types::c_uint) -> KernelResult {
        // SAFETY: FFI call.
        // OK to coerce a shared reference to a mutable pointer, as
        // we guarantee that a `struct regmap`'s `write` is fully synchronized.
        let res = unsafe { bindings::regmap_write(self.0, reg, val) };
        if res != 0 {
            return Err(Error::from_kernel_errno(res));
        }
        Ok(())
    }

    pub fn read(&self, reg: c_types::c_uint) -> KernelResult<c_types::c_uint> {
        let mut val = MaybeUninit::<c_types::c_uint>::uninit();
        // SAFETY: FFI call.
        // OK to coerce a shared reference to a mutable pointer, as
        // we guarantee that a `struct regmap`'s `read` is fully synchronized.
        // OK to pass a pointer to an uninitialized `u32`, this is part of
        // `struct regmap`'s `read` API.
        let res = unsafe { bindings::regmap_read(self.0, reg, val.as_mut_ptr()) };
        if res != 0 {
            return Err(Error::from_kernel_errno(res));
        }
        // SAFETY: if `res` is zero, `val` is guaranteed initialized by the
        // call to `regmap_read`.
        Ok(unsafe { val.assume_init() })
    }

    #[cfg(CONFIG_REGMAP_MMIO)]
    pub fn init_mmio_platform_resource(
        pdev: &mut PlatformDevice,
        index: u32,
        cfg: &RegmapConfig,
    ) -> KernelResult<Self> {
        let iomem = devm_platform_ioremap_resource(pdev, index)?;
        devm_regmap_init_mmio(pdev, iomem, cfg)
    }
}

#[derive(Default)]
pub struct RegmapConfig {
    reg_bits: i32,
    val_bits: i32,
    reg_stride: Option<i32>,
    max_register: Option<u32>,
}

impl RegmapConfig {
    pub fn new(reg_bits: u32, val_bits: u32) -> RegmapConfig {
        RegmapConfig {
            reg_bits: reg_bits as i32,
            val_bits: val_bits as i32,
            ..Default::default()
        }
    }

    pub fn reg_stride(mut self, reg_stride: u32) -> Self {
        self.reg_stride.replace(reg_stride as i32);
        self
    }

    pub fn max_register(mut self, max_register: u32) -> Self {
        self.max_register.replace(max_register);
        self
    }

    fn build(&self) -> bindings::regmap_config {
        let mut cfg = bindings::regmap_config {
            reg_bits: self.reg_bits,
            val_bits: self.val_bits,
            // INVARIANTS: `regmap` must be created with locking enabled.
            disable_locking: false,
            ..Default::default()
        };
        if let Some(s) = self.reg_stride {
            cfg.reg_stride = s;
        }
        if let Some(m) = self.max_register {
            cfg.max_register = m;
        }
        cfg
    }
}

fn devm_regmap_init_mmio(
    dev: &mut impl Device,
    regs: *mut c_types::c_void,
    cfg: &RegmapConfig,
) -> KernelResult<Regmap> {
    extern "C" {
        #[allow(improper_ctypes)]
        fn rust_helper_devm_regmap_init_mmio(
            dev: *mut bindings::device,
            regs: *mut c_types::c_void,
            config: *const bindings::regmap_config,
        ) -> *mut bindings::regmap;
    }

    // SAFETY: FFI call.
    // OK to coerce a temporary `struct regmap_config` to a const pointer,
    // as that pointer has to be valid only for the lifetime of the
    // `regmap_init` call.
    let rm = unsafe {
        ptr_err_check(rust_helper_devm_regmap_init_mmio(
            dev.to_dev_ptr(),
            regs,
            &cfg.build(),
        ))?
    };
    Ok(Regmap(rm))
}

fn devm_platform_ioremap_resource(
    pdev: &mut PlatformDevice,
    index: u32,
) -> KernelResult<*mut c_types::c_void> {
    // SAFETY: FFI call.
    unsafe {
        ptr_err_check(bindings::devm_platform_ioremap_resource(
            pdev.to_ptr(),
            index,
        ))
    }
}
