// SPDX-License-Identifier: GPL-2.0

use crate::error::{ptr_err_check, Error, KernelResult};
use crate::platform_driver::{Device, PlatformDevice};
use crate::{bindings, c_types};

// TODO: investigate lifetime management for Regmap.
//
// The `struct regmap` lifetime (and that of its `void __iomem *` dependency) is
// managed by the kernel using the `devm_` mechanism. The kernel will keep `devm_`
// objects around for as long as the device exists. On device removal, the `devm_`
// objects are automatically released by the kernel.
//
// Theoretically, a `devm_` based object could 'leak' out of the Rust driver. If
// it gets used/dereferenced **after** the device has been removed, that'll result
// in a use-after-free.
//
// Investigate if we can to leverage Rust lifetimes to ensure build-time correctness.

/// Abstraction wrapping a kernel `struct regmap`.
///
/// # Invariants
///
/// regmap locking is never disabled.
pub struct Regmap(*mut bindings::regmap);

// SAFETY: we access the underlying `struct regmap` only through its helper functions,
// and we never disable locking by the type invariant, so:
//   - `struct regmap *` can be safely sent between threads ([`Send`])
//   - `struct regmap`'s helper functions can be safely called from any thread ([`Sync`])
unsafe impl Send for Regmap {}
unsafe impl Sync for Regmap {}

impl Regmap {
    pub fn write(&self, reg: u32, val: u32) -> KernelResult {
        // SAFETY: FFI call.
        // OK to coerce a shared reference to a mutable pointer, as
        // we guarantee that a `struct regmap`'s `write` is fully synchronized.
        let res = unsafe { bindings::regmap_write(self.0, reg, val) };
        if res != 0 {
            return Err(Error::from_kernel_errno(res));
        }
        Ok(())
    }

    pub fn read(&self, reg: u32) -> KernelResult<u32> {
        // Initialize `val` here to eliminate `unsafe` code when returning it.
        let mut val = 0_u32;
        // SAFETY: FFI call.
        // OK to coerce a temporary `u32` to a mut pointer,
        // as that pointer has to be valid only for the lifetime of the
        // `regmap_read` call.
        let res = unsafe { bindings::regmap_read(self.0, reg, &mut val) };
        if res != 0 {
            return Err(Error::from_kernel_errno(res));
        }
        Ok(val)
    }

    pub fn init_mmio_platform_resource(
        pdev: &mut PlatformDevice,
        index: u32,
        cfg: &RegmapConfig,
    ) -> KernelResult<Self> {
        let iomem = Self::devm_platform_ioremap_resource(pdev, index)?;
        Self::devm_regmap_init_mmio(pdev, iomem, cfg)
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

    pub fn reg_stride(mut self, reg_stride: u8) -> Self {
        self.reg_stride = Some(reg_stride.into());
        self
    }

    pub fn max_register(mut self, max_register: u32) -> Self {
        self.max_register = Some(max_register);
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
