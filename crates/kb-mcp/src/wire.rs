use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

const MAX_FRAME_BYTES: usize = 1024 * 1024;

enum Frame {
    Eof,
    Bytes(Vec<u8>),
    Oversized,
}

/// Process newline-delimited MCP JSON-RPC frames until input reaches EOF.
///
/// The reader never retains more than 1 MiB for one frame. Parse and size
/// failures produce JSON-RPC errors and do not prevent processing later lines.
///
/// # Errors
///
/// Returns an I/O error when the supplied reader or writer fails.
pub fn serve_frames<R, W, F>(mut reader: R, mut writer: W, mut handler: F) -> io::Result<()>
where
    R: BufRead,
    W: Write,
    F: FnMut(Value) -> Option<Value>,
{
    loop {
        match read_frame(&mut reader)? {
            Frame::Eof => break,
            Frame::Oversized => write_message(
                &mut writer,
                &error(None, -32700, "MCP frame exceeds 1048576 bytes."),
            )?,
            Frame::Bytes(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                Ok(request) => {
                    if let Some(response) = handler(request) {
                        write_message(&mut writer, &response)?;
                    }
                }
                Err(parse_error) => write_message(
                    &mut writer,
                    &error(None, -32700, &format!("Invalid JSON: {parse_error}")),
                )?,
            },
        }
    }
    writer.flush()
}

fn read_frame(reader: &mut impl BufRead) -> io::Result<Frame> {
    let mut frame = Vec::new();
    let mut oversized = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if frame.is_empty() && !oversized {
                return Ok(Frame::Eof);
            }
            return Ok(if oversized {
                Frame::Oversized
            } else {
                trim_cr(frame)
            });
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        let payload_len = newline.unwrap_or(available.len());
        if !oversized {
            let remaining = MAX_FRAME_BYTES.saturating_sub(frame.len());
            frame.extend_from_slice(&available[..payload_len.min(remaining)]);
            oversized = payload_len > remaining;
        }
        reader.consume(consumed);
        if newline.is_some() {
            return Ok(if oversized {
                Frame::Oversized
            } else {
                trim_cr(frame)
            });
        }
    }
}

fn trim_cr(mut bytes: Vec<u8>) -> Frame {
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    Frame::Bytes(bytes)
}

fn write_message(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, value).map_err(io::Error::other)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message }
    })
}
