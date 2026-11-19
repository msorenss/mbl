//! Server-sent events: split a byte stream into `(event, data)` pairs.

use futures_util::StreamExt;
use mbl_core::{Error, Result};

/// Largest event accepted; protects against a server that never ends an event.
const MAX_EVENT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Event {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub struct Parser {
    buffer: Vec<u8>,
}

impl Parser {
    /// Adds bytes and returns the events completed by them.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Event>> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some((end, skip)) = boundary(&self.buffer) {
            let raw: Vec<u8> = self.buffer.drain(..end + skip).take(end).collect();
            if let Some(event) = parse_event(&String::from_utf8_lossy(&raw)) {
                events.push(event);
            }
        }
        if self.buffer.len() > MAX_EVENT_BYTES {
            return Err(Error::External("The provider sent an event that is too large".into()));
        }
        Ok(events)
    }

    /// Whatever is left when the stream ends without a final blank line.
    pub fn finish(&mut self) -> Option<Event> {
        let rest = std::mem::take(&mut self.buffer);
        parse_event(&String::from_utf8_lossy(&rest))
    }
}

/// Position of the first blank line and the length of the separator.
fn boundary(buf: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i + 1 < buf.len() {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some((i, 2));
        }
        if i + 3 < buf.len() && &buf[i..i + 4] == b"\r\n\r\n" {
            return Some((i, 4));
        }
        i += 1;
    }
    None
}

fn parse_event(raw: &str) -> Option<Event> {
    let mut event = Event::default();
    let mut data = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.strip_prefix(' ').unwrap_or(rest));
        } else if let Some(rest) = line.strip_prefix("event:") {
            event.event = Some(rest.trim().to_owned());
        }
    }
    if data.is_empty() {
        return None;
    }
    event.data = data.join("\n");
    Some(event)
}

/// Reads the response body and calls `handle` for each event until it returns `false`.
pub async fn read(
    response: reqwest::Response,
    mut handle: impl FnMut(Event) -> Result<bool>,
) -> Result<()> {
    let mut parser = Parser::default();
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| {
            Error::External(format!("The provider's stream broke off: {}", e.without_url()))
        })?;
        for event in parser.push(&chunk)? {
            if !handle(event)? {
                return Ok(());
            }
        }
    }
    if let Some(event) = parser.finish() {
        handle(event)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_split_across_chunks() {
        let mut parser = Parser::default();
        assert!(parser.push(b"event: message_start\ndata: {\"a\"").unwrap().is_empty());
        let events = parser.push(b":1}\n\ndata: second\r\n\r\n: comment\n\n").unwrap();
        assert_eq!(
            events,
            vec![
                Event { event: Some("message_start".into()), data: "{\"a\":1}".into() },
                Event { event: None, data: "second".into() },
            ]
        );
        parser.push(b"data: [DONE]").unwrap();
        assert_eq!(parser.finish().unwrap().data, "[DONE]");
    }
}
