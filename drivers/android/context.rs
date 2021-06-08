// SPDX-License-Identifier: GPL-2.0

extern crate alloc;

use alloc::sync::Arc;
use core::pin::Pin;
use kernel::boxed_mutex;
use kernel::{bindings, prelude::*, sync::BoxedMutex, Error};

use crate::{
    node::NodeRef,
    thread::{BinderError, BinderResult},
};

struct Manager {
    node: Option<NodeRef>,
    uid: Option<bindings::kuid_t>,
}

pub(crate) struct Context {
    manager: BoxedMutex<Manager>,
}

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

impl Context {
    pub(crate) fn new() -> Result<Pin<Arc<Self>>> {
        let ctx_ref = Arc::try_new(Self {
            manager: boxed_mutex!(
                Manager {
                    node: None,
                    uid: None,
                },
                "Context::manager"
            )?,
        })?;

        // SAFETY: `ctx_ref` is pinned behind the `Arc` reference.
        Ok(unsafe { Pin::new_unchecked(ctx_ref) })
    }

    pub(crate) fn set_manager_node(&self, node_ref: NodeRef) -> Result {
        let mut manager = self.manager.lock();
        if manager.node.is_some() {
            return Err(Error::EBUSY);
        }
        // TODO: Call security_binder_set_context_mgr.

        // TODO: Get the actual caller id.
        let caller_uid = bindings::kuid_t::default();
        if let Some(ref uid) = manager.uid {
            if uid.val != caller_uid.val {
                return Err(Error::EPERM);
            }
        }

        manager.node = Some(node_ref);
        manager.uid = Some(caller_uid);
        Ok(())
    }

    pub(crate) fn unset_manager_node(&self) {
        let node_ref = self.manager.lock().node.take();
        drop(node_ref);
    }

    pub(crate) fn get_manager_node(&self, strong: bool) -> BinderResult<NodeRef> {
        self.manager
            .lock()
            .node
            .as_ref()
            .ok_or_else(BinderError::new_dead)?
            .clone(strong)
    }
}
