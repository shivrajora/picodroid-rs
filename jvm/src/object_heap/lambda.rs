use super::{LambdaProxy, ObjectHeap};

impl ObjectHeap {
    // ── Lambda proxy support ──────────────────────────────────────────────────

    /// Associate a lambda proxy with an existing heap object.
    pub fn register_lambda(&mut self, obj_idx: u16, proxy: LambdaProxy) {
        self.lambda_proxies.push((obj_idx, proxy));
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
