//! The wire: JSON-RPC 2.0, one object per line.
//!
//! `PLAN.md` settled on JSON-RPC in Stage 0 and the reasons have not changed —
//! it is a protocol other people's tools already speak, it is cross-platform in
//! a way DDE could never be, and a request is something a shell script can
//! write with `printf` and read back with `jq`. That last one is the test this
//! module is designed against: if driving a window needs a client library, the
//! socket has failed at the thing DDE was bad at.
//!
//! **The framing is a newline and JSON-RPC does not specify one.** LSP uses
//! `Content-Length` headers; this uses one compact object per line, which is
//! what makes `socat` and `nc` work as clients. `serde_json`'s compact writer
//! never emits a bare newline, so the framing cannot be broken by a value —
//! a string containing one is escaped as `\n` inside the object. A request may
//! not span lines, which is the one restriction the choice buys.
//!
//! **Batches are refused, and that is a deliberate divergence from the spec.**
//! §6 allows an array of requests answered by an array of responses. Every
//! method here either changes the terminal or waits on it, so the ordering and
//! atomicity questions a batch raises would have to be answered — and nothing
//! wants one: a client with two things to do sends two lines. The refusal is
//! explicit ([`RpcError::INVALID_REQUEST`]) rather than a parse failure, so a
//! generic JSON-RPC library gets told why.
//!
//! **A line longer than [`MAX_LINE`] ends the connection.** The reader would
//! otherwise grow a buffer for as long as a peer keeps typing, and the peer
//! here is another process rather than the session's own code.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The longest request accepted, in bytes.
///
/// A megabyte is far past anything a method here takes — the largest is a
/// `send` of pasted text — and small enough that a peer cannot make the
/// listener's memory its own. Exceeding it is not an error reported to the
/// client, because the framing is already lost by then: the connection closes.
pub const MAX_LINE: usize = 1 << 20;

/// A request as it arrives, before a method has been found for it.
///
/// `id` is [`Value`] rather than a number because the spec allows a string,
/// a number or null, and it is echoed back untouched. `params` is a `Value`
/// for the same reason a method table is a `match`: each method deserialises
/// its own, and a params error belongs to the method rather than to the
/// framing.
#[derive(Debug, Deserialize)]
pub struct Request {
    /// Must be `"2.0"`. Absent or wrong is [`RpcError::INVALID_REQUEST`] —
    /// checked rather than ignored, since a client that gets this wrong has
    /// probably got the response shape wrong too and should hear about it at
    /// the first message rather than the first surprise.
    #[serde(default)]
    pub jsonrpc: Option<String>,
    /// Absent means a notification: no reply, whatever happens.
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// What goes back. `result` and `error` are exclusive and exactly one is set.
#[derive(Debug, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    /// Null when the request was unparseable enough that no id could be read,
    /// which is what §5 requires.
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: Value, result: Value) -> Response {
        Response {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Value, error: RpcError) -> Response {
        Response {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        }
    }

    /// One line, newline included. Serialisation cannot fail for what this
    /// module constructs, and a failure here would leave the connection with
    /// no answer at all, so it falls back to an internal error rather than
    /// unwrapping.
    pub fn line(&self) -> String {
        match serde_json::to_string(self) {
            Ok(mut s) => {
                s.push('\n');
                s
            }
            Err(e) => format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{{\"code\":{},\"message\":{}}}}}\n",
                RpcError::INTERNAL,
                Value::String(e.to_string()),
            ),
        }
    }
}

/// An error object. `data` carries the detail a human wants and `message` the
/// one line a script prints.
#[derive(Debug, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub data: Option<Value>,
}

impl RpcError {
    // The four from §5.1 that can happen here. `-32600` covers both a bad
    // `jsonrpc` field and a batch.
    pub const PARSE: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const NO_METHOD: i32 = -32601;
    pub const BAD_PARAMS: i32 = -32602;
    pub const INTERNAL: i32 = -32603;

    // `-32000..=-32099` is reserved for the implementation, which is where
    // everything about *this* terminal goes. They are stable: a script tests
    // them.
    /// The window has gone — the frontend stopped servicing while the request
    /// was in flight. The connection is closed after this.
    pub const GONE: i32 = -32000;
    /// A command that needs a connection, without one. `send` and the control
    /// lines.
    pub const NOT_CONNECTED: i32 = -32001;
    /// A second `macro.run` while one is running. Upstream brings the existing
    /// macro's window to the front instead (`ttdde.c:1488`); there is no window
    /// to raise from here, so it is an error with the running macro named.
    pub const MACRO_RUNNING: i32 = -32002;
    /// The frontend declined — it has no implementation for that method. The
    /// null-callback case of the C ABI, kept distinct from [`Self::NO_METHOD`]
    /// so a client can tell "this build cannot" from "no such thing".
    pub const REFUSED: i32 = -32003;
    /// Something outside failed: a macro file that will not open, a connection
    /// that would not come up.
    pub const FAILED: i32 = -32004;

    pub fn new(code: i32, message: impl Into<String>) -> RpcError {
        RpcError {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> RpcError {
        self.data = Some(data);
        self
    }
}

/// What a line turned out to be.
pub enum Incoming {
    /// A well-formed request. `id` absent means no reply is to be sent.
    Call(Request),
    /// Unparseable, or parsed and not a request. Answer with this and carry on
    /// — a bad line does not end the conversation, since the framing is intact
    /// by construction once a newline has been found.
    Bad(Response),
}

/// Read one line as a request.
///
/// Blank lines are `None`: a client that ends its message with `\r\n`, or one
/// that keeps the connection warm with an empty line, is not making an error
/// worth an error object.
pub fn parse(line: &str) -> Option<Incoming> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(Incoming::Bad(Response::err(
                Value::Null,
                RpcError::new(RpcError::PARSE, "not JSON").with_data(Value::String(e.to_string())),
            )))
        }
    };
    if value.is_array() {
        return Some(Incoming::Bad(Response::err(
            Value::Null,
            RpcError::new(
                RpcError::INVALID_REQUEST,
                "batch requests are not supported; send one object per line",
            ),
        )));
    }
    // The id is read before the rest is validated, so that a request with a
    // good id and a bad `jsonrpc` is still answerable — §5 wants the id echoed
    // whenever it could be determined.
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    let req: Request = match serde_json::from_value(value) {
        Ok(r) => r,
        Err(e) => {
            return Some(Incoming::Bad(Response::err(
                id,
                RpcError::new(RpcError::INVALID_REQUEST, "not a request")
                    .with_data(Value::String(e.to_string())),
            )))
        }
    };
    if req.jsonrpc.as_deref() != Some("2.0") {
        return Some(Incoming::Bad(Response::err(
            id,
            RpcError::new(RpcError::INVALID_REQUEST, "jsonrpc must be \"2.0\""),
        )));
    }
    Some(Incoming::Call(req))
}

/// Deserialise a method's own params.
///
/// Absent params are `null`, which every params type here accepts when all of
/// its fields have defaults — so `{"method":"status"}` needs no `"params":{}`
/// and a method that requires an argument still reports a missing one.
pub fn params<T: for<'de> Deserialize<'de>>(p: Option<Value>) -> Result<T, RpcError> {
    let v = p.unwrap_or(Value::Null);
    // `null` deserialises into a struct only through `Option`; an empty object
    // is what a struct of defaults wants, so the two are made the same thing
    // here rather than in every params type.
    let v = if v.is_null() {
        Value::Object(serde_json::Map::new())
    } else {
        v
    };
    serde_json::from_value(v).map_err(|e| {
        RpcError::new(RpcError::BAD_PARAMS, "bad params").with_data(Value::String(e.to_string()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(line: &str) -> Request {
        match parse(line) {
            Some(Incoming::Call(r)) => r,
            _ => panic!("expected a call: {line}"),
        }
    }

    fn bad(line: &str) -> RpcError {
        match parse(line) {
            Some(Incoming::Bad(r)) => r.error.unwrap(),
            _ => panic!("expected a refusal: {line}"),
        }
    }

    #[test]
    fn a_request_carries_its_id_and_params() {
        let r = call(r#"{"jsonrpc":"2.0","id":7,"method":"send","params":{"text":"hi"}}"#);
        assert_eq!(r.method, "send");
        assert_eq!(r.id, Some(Value::from(7)));
        assert_eq!(r.params.unwrap()["text"], Value::from("hi"));
    }

    /// No id is a notification, which is answered by silence rather than by a
    /// response with a null id.
    #[test]
    fn a_notification_has_no_id() {
        assert!(call(r#"{"jsonrpc":"2.0","method":"macro.stop"}"#)
            .id
            .is_none());
    }

    #[test]
    fn blank_lines_are_not_errors() {
        assert!(parse("").is_none());
        assert!(parse("  \r\n").is_none());
    }

    #[test]
    fn junk_is_a_parse_error_with_a_null_id() {
        let line = parse("not json").unwrap();
        match line {
            Incoming::Bad(r) => {
                assert_eq!(r.id, Value::Null);
                assert_eq!(r.error.unwrap().code, RpcError::PARSE);
            }
            _ => panic!(),
        }
    }

    /// The id survives a request that is otherwise wrong, because a client
    /// waiting on id 3 has to be able to match the refusal to it.
    #[test]
    fn a_bad_version_is_still_answered_to_its_id() {
        let r = match parse(r#"{"jsonrpc":"1.0","id":3,"method":"status"}"#).unwrap() {
            Incoming::Bad(r) => r,
            _ => panic!(),
        };
        assert_eq!(r.id, Value::from(3));
        assert_eq!(r.error.unwrap().code, RpcError::INVALID_REQUEST);
    }

    #[test]
    fn a_batch_says_why_rather_than_failing_to_parse() {
        let e = bad(r#"[{"jsonrpc":"2.0","id":1,"method":"status"}]"#);
        assert_eq!(e.code, RpcError::INVALID_REQUEST);
        assert!(e.message.contains("batch"));
    }

    #[test]
    fn a_missing_method_is_not_a_request() {
        assert_eq!(
            bad(r#"{"jsonrpc":"2.0","id":1}"#).code,
            RpcError::INVALID_REQUEST
        );
    }

    /// The framing survives a value that contains a newline, which is the one
    /// thing a line-oriented protocol has to be sure of.
    #[test]
    fn a_response_is_one_line_whatever_is_in_it() {
        let r = Response::ok(
            Value::from(1),
            serde_json::json!({ "text": "two\nlines\r\n" }),
        );
        let line = r.line();
        assert_eq!(line.matches('\n').count(), 1);
        assert!(line.ends_with('\n'));
    }

    /// Absent params are an empty object, so a method whose arguments are all
    /// optional needs none.
    #[test]
    fn absent_params_are_the_defaults() {
        #[derive(Deserialize, Default)]
        struct P {
            #[serde(default)]
            n: u32,
        }
        assert_eq!(params::<P>(None).unwrap().n, 0);
        assert_eq!(params::<P>(Some(serde_json::json!({"n": 4}))).unwrap().n, 4);
    }

    #[test]
    fn a_missing_required_param_is_reported_as_one() {
        #[derive(Debug, Deserialize)]
        struct P {
            #[allow(dead_code)]
            path: String,
        }
        assert_eq!(params::<P>(None).unwrap_err().code, RpcError::BAD_PARAMS);
    }
}
