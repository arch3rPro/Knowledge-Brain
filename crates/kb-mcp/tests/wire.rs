use serde_json::{Value, json};
use std::io::Cursor;

#[test]
fn frames_preserve_ids_ignore_notifications_and_continue_after_bad_json() {
    let input = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
        "not-json\n",
        "{\"jsonrpc\":\"2.0\",\"id\":\"last\",\"method\":\"ping\"}\n"
    );
    let mut output = Vec::new();
    serve_frames(Cursor::new(input), &mut output, |request| {
        request
            .get("id")
            .cloned()
            .map(|id| json!({"jsonrpc":"2.0", "id":id, "result":{"ok":true}}))
    })
    .unwrap();

    let messages = output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["id"], 7);
    assert_eq!(messages[1]["error"]["code"], -32700);
    assert_eq!(messages[2]["id"], "last");
}

#[test]
fn oversized_frame_is_bounded_and_the_next_frame_is_processed() {
    let mut input = vec![b'x'; 1024 * 1024 + 1];
    input.extend_from_slice(b"\n{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n");
    let mut output = Vec::new();
    serve_frames(Cursor::new(input), &mut output, |request| {
        request
            .get("id")
            .cloned()
            .map(|id| json!({"jsonrpc":"2.0", "id":id, "result":{}}))
    })
    .unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("MCP frame exceeds 1048576 bytes"));
    assert!(text.contains("\"id\":9"));
}

fn serve_frames<R, W, F>(reader: R, writer: W, handler: F) -> std::io::Result<()>
where
    R: std::io::BufRead,
    W: std::io::Write,
    F: FnMut(Value) -> Option<Value>,
{
    kb_mcp::serve_frames(reader, writer, handler)
}
