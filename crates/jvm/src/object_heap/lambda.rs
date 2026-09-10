// SPDX-License-Identifier: GPL-3.0-only
use super::{LambdaProxy, ObjectHeap};

impl ObjectHeap {
    // ── Lambda proxy support ──────────────────────────────────────────────────

    /// Associate a lambda proxy with an existing heap object.
    pub fn register_lambda(&mut self, obj_idx: u16, proxy: LambdaProxy) {
        self.lambda_proxies.push((obj_idx, proxy));
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
