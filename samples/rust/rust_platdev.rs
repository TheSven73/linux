// SPDX-License-Identifier: GPL-2.0

//! Rust platform device sample

#![no_std]
#![feature(allocator_api, global_asm)]

use alloc::{boxed::Box, sync::Arc};
use core::pin::Pin;
use kernel::prelude::*;
use kernel::{
    cstr,
    file::File,
    file_operations::{FileOpener, FileOperations},
    io_buffer::IoBufferWriter,
    miscdev,
    platform_driver::{self, PlatformDevice, PlatformDriver},
    regmap::{Regmap, RegmapConfig},
};

module! {
    type: RustPlatdev,
    name: b"rust_platdev",
    author: b"Rust for Linux Contributors",
    description: b"Rust platform device sample",
    license: b"GPL v2",
    params: {
    },
}

// The Shared State is simply a Regmap, which is Send + Sync.
struct SharedState {
    regmap: Regmap,
}

impl SharedState {
    fn try_new(regmap: Regmap) -> KernelResult<Arc<Self>> {
        Ok(Arc::try_new(SharedState { regmap })?)
    }
}

struct RngDevice {
    state: Arc<SharedState>,
}

impl FileOpener<Arc<SharedState>> for RngDevice {
    fn open(state: &Arc<SharedState>) -> KernelResult<Self::Wrapper> {
        Ok(Box::try_new(RngDevice {
            state: state.clone(),
        })?)
    }
}

impl FileOperations for RngDevice {
    type Wrapper = Box<Self>;

    kernel::declare_file_operations!(read);

    fn read<T: IoBufferWriter>(&self, _: &File, data: &mut T, offset: u64) -> KernelResult<usize> {
        // Succeed if the caller doesn't provide a buffer or if not at the start.
        if data.is_empty() || offset != 0 {
            return Ok(0);
        }

        let regmap = &self.state.regmap;
        let num_words = regmap.read(RNG_STATUS)? >> 24;
        if num_words == 0 {
            return Ok(0);
        }
        data.write(&regmap.read(RNG_DATA)?)?;
        Ok(4)
    }
}

#[derive(Default)]
struct RngDriver;

// TODO maybe wrap register addresses into a type, so they can never
// be mixed/confused with register values? That's a common error.
// OR something more outrageous: wrap register values in a type linked
// to the register address type, so values cannot simply get written to
// the wrong address? That's another common error.

const RNG_CTRL: u32 = 0x0;
const RNG_STATUS: u32 = 0x4;
const RNG_DATA: u32 = 0x8;

// the initial numbers generated are "less random" so will be discarded
const RNG_WARMUP_COUNT: u32 = 0x40000;
// enable rng
const RNG_RBGEN: u32 = 0x1;

impl PlatformDriver for RngDriver {
    type DrvData = Pin<Box<miscdev::Registration<Arc<SharedState>>>>;

    fn probe(pdev: &mut PlatformDevice) -> KernelResult<Self::DrvData> {
        pr_info!("probe!\n");
        // create Regmap which maps device registers
        let cfg = RegmapConfig::new(32, 32)
            .reg_stride(4)
            .max_register(RNG_DATA);
        let regmap = Regmap::init_mmio_platform_resource(pdev, 0, &cfg)?;
        // set warm-up count & enable
        regmap.write(RNG_STATUS, RNG_WARMUP_COUNT)?;
        regmap.write(RNG_CTRL, RNG_RBGEN)?;
        // register character device so userspace can read out random data
        let state = SharedState::try_new(regmap)?;
        let dev = miscdev::Registration::new_pinned::<RngDevice>(cstr!("rust_hwrng"), None, state)?;
        Ok(dev)
    }
}

struct RustPlatdev {
    _pdev: Pin<Box<platform_driver::Registration>>,
}

impl KernelModule for RustPlatdev {
    fn init() -> KernelResult<Self> {
        pr_info!("Rust platform device sample (init)\n");

        let pdev = platform_driver::Registration::new_pinned::<RngDriver>(
            cstr!("bcm2835-rng"),
            // TODO this should be an optional list.
            // Perhaps use an enum to specify behavioural differences.
            cstr!("brcm,bcm2835-rng"),
            &THIS_MODULE,
        )?;

        Ok(RustPlatdev { _pdev: pdev })
    }
}

impl Drop for RustPlatdev {
    fn drop(&mut self) {
        pr_info!("Rust platform device sample (exit)\n");
    }
}
