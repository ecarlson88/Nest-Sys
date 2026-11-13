//! QRC's framing: JSON-RPC 2.0 messages, each ended by a NUL byte, on one
//! TCP stream in both directions.

use serde_json::Value;

/// Bytes read from the Core, split into messages at each NUL.
#[derive(Default)]
pub struct Framer {
    buf: Vec<u8>,
}

impl Framer {
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// The next complete message, or None until its NUL has arrived. Empty
    /// frames (a stray NUL, whitespace) are skipped.
    pub fn next_message(&mut self) -> Option<Result<Value, serde_json::Error>> {
        loop {
            let end = self.buf.iter().position(|&b| b == 0)?;
            let frame: Vec<u8> = self.buf.drain(..=end).collect();
            let body = &frame[..end];
            if body.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            return Some(serde_json::from_slice(body));
        }
    }
}

/// msg as sent on the wire, NUL included.
pub fn encode(msg: &Value) -> Vec<u8> {
    let mut out = serde_json::to_vec(msg).expect("a Value always serializes");
    out.push(0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn splits_messages_across_and_within_reads() {
        let mut f = Framer::default();
        f.push(b"{\"a\":1}\0{\"b\"");
        assert_eq!(f.next_message().unwrap().unwrap(), json!({"a": 1}));
        assert!(f.next_message().is_none());
        f.push(b":2}\0\0 \0{\"c\":3}\0");
        assert_eq!(f.next_message().unwrap().unwrap(), json!({"b": 2}));
        assert_eq!(f.next_message().unwrap().unwrap(), json!({"c": 3}));
        assert!(f.next_message().is_none());
    }

    #[test]
    fn reports_a_malformed_message_and_carries_on() {
        let mut f = Framer::default();
        f.push(b"{oops\0{\"ok\":true}\0");
        assert!(f.next_message().unwrap().is_err());
        assert_eq!(f.next_message().unwrap().unwrap(), json!({"ok": true}));
    }

    #[test]
    fn encodes_with_a_trailing_nul() {
        assert_eq!(encode(&json!({"x": 1})), b"{\"x\":1}\0");
    }
}
