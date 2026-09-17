// SPDX-License-Identifier: GPL-3.0-only
use super::{reserve_fallible, Exhausted, LambdaProxy, ObjectHeap};

impl ObjectHeap {
    // ── Lambda proxy support ──────────────────────────────────────────────────

    /// Associate a lambda proxy with an existing heap object. [`Exhausted`]
    /// when the registry cannot grow; nothing is recorded then, and the
    /// proxy object at `obj_idx` is ordinary garbage for the next sweep.
    ///
    /// Fallible because this table doubles: at 60 bytes an entry the step
    /// from 64 to 128 registered lambdas is one contiguous 7,680-byte
    /// request, which on the touch kit's full heap was the last board reset
    /// of the 2026-09-13 QA round (`qa_thr`'s `frameworkExecutors`, a burst
    /// of 64 `execute(() -> …)` posts).
    pub fn register_lambda(&mut self, obj_idx: u16, proxy: LambdaProxy) -> Result<(), Exhausted> {
        reserve_fallible(&mut self.lambda_proxies, 1)?;
        self.lambda_proxies.push((obj_idx, proxy));
        Ok(())
    }

    /// Whether any lambda proxy exists at all.
    ///
    /// Every `invokevirtual`/`invokeinterface` has to consider that its
    /// receiver might be a lambda proxy, which costs a stack index, a `Value`
    /// match and a table probe on the hot invoke path. Apps that never use a
    /// lambda — which is most of them, and all of `benchmark`, where invoke is
    /// 34% of the run — can answer that question with one length check.
    #[inline]
    pub fn has_lambdas(&self) -> bool {
        !self.lambda_proxies.is_empty()
    }

    /// Look up the lambda proxy metadata for an object, if any.
    pub fn get_lambda(&self, obj_idx: u16) -> Option<&LambdaProxy> {
        self.lambda_proxies
            .iter()
            .find(|(idx, _)| *idx == obj_idx)
            .map(|(_, proxy)| proxy)
    }

    /// Remove the lambda proxy entry for an object (called from GC sweep).
    pub fn free_lambda(&mut self, obj_idx: u16) {
        self.lambda_proxies.retain(|(idx, _)| *idx != obj_idx);
    }
}
