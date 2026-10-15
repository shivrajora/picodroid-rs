// SPDX-License-Identifier: GPL-3.0-only
//! `org.json` on the device: a capped node pool fed by whatever an app
//! downloaded.

use pd_json::parse::parse;
use pd_json::pool::Pool;
use pd_json::serialize::to_bytes;

use crate::Bytes;

/// Input: a byte choosing the root the caller asked for (`JSONObject`,
/// `JSONArray`, any), then the text.
///
/// A refused document frees every node it allocated. An accepted one
/// serializes to text that parses back to a tree that serializes the same:
/// the serializer's output is a fixed point.
pub fn json(data: &[u8]) {
    let mut b = Bytes::new(data);
    let want = [Some(false), Some(true), None][b.below(3)];
    let text = b.rest();

    let mut pool = Pool::new();
    let root = match parse(&mut pool, text, want) {
        Ok(root) => root,
        Err(_) => {
            assert_eq!(pool.node_count(), 0, "a refused document leaked nodes");
            assert_eq!(pool.payload_bytes(), 0, "a refused document leaked bytes");
            return;
        }
    };
    let Some(once) = to_bytes(&pool, root, 0) else {
        return;
    };
    let mut again = Pool::new();
    let root2 = parse(&mut again, &once, want)
        .unwrap_or_else(|e| panic!("serializer output does not parse: {e:?}"));
    let twice = to_bytes(&again, root2, 0).expect("re-serializes");
    assert!(
        once == twice,
        "serialize -> parse -> serialize is not a fixed point:\n{}\n{}",
        String::from_utf8_lossy(&once),
        String::from_utf8_lossy(&twice)
    );
}
