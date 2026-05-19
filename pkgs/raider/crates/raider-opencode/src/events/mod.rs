pub mod decode;
pub mod stream;
pub mod types;

pub use decode::parse_frame;
pub use stream::{EventStream, StreamItem};
pub use types::{
    MessagePartDeltaProps, MessagePartRemovedProps, MessagePartUpdatedProps, MessageRemovedProps,
    MessageUpdatedProps, ServerEvent, SessionDeletedProps, SessionErrorProps, SessionIdleProps,
    SessionStatusKind, SessionStatusProps, SessionUpdatedProps, VcsBranchUpdatedProps,
};

#[cfg(test)]
mod tests {
    use super::decode::{extract_data_field, find_frame_end, parse_frame};
    use super::stream::{reconnect_delay, EventStream, StreamItem};
    use super::types::{strip_version_suffix, ServerEvent};
    use crate::error::Error;
    use std::time::Duration;

    #[test]
    fn parse_session_idle_frame() {
        let data = r#"{"id":"evt-1","type":"session.idle","properties":{"sessionID":"ses-abc"}}"#;
        let ev = parse_frame(data).expect("decode");
        match ev {
            ServerEvent::SessionIdle(p) => assert_eq!(p.session_id.as_str(), "ses-abc"),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn parse_text_part_update_frame() {
        let data = r#"{
            "id":"evt-2",
            "type":"message.part.updated",
            "properties":{
                "sessionID":"ses-abc",
                "messageID":"msg-1",
                "part":{"type":"text","id":"prt-1","text":"hello"}
            }
        }"#;
        let ev = parse_frame(data).expect("decode");
        match ev {
            ServerEvent::MessagePartUpdated(p) => {
                assert_eq!(p.session_id.as_str(), "ses-abc");
                assert_eq!(p.part.text(), Some("hello"));
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn parse_reasoning_part_update_frame() {
        let data = r#"{
            "id":"evt-3",
            "type":"message.part.updated",
            "properties":{
                "sessionID":"ses-abc",
                "messageID":"msg-1",
                "part":{"type":"reasoning","id":"prt-2","text":"thinking..."}
            }
        }"#;
        let ev = parse_frame(data).expect("decode");
        match ev {
            ServerEvent::MessagePartUpdated(p) => {
                assert_eq!(p.part.reasoning(), Some("thinking..."))
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn unknown_event_does_not_fail() {
        let data = r#"{"id":"evt-9","type":"storage.write","properties":{"key":"foo"}}"#;
        let ev = parse_frame(data).expect("decode");
        match ev {
            ServerEvent::Unknown(ty) => assert_eq!(ty, "storage.write"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn parses_global_event_envelope_wrap() {
        let data = r#"{
            "directory":"/home/emre/Desktop/raider",
            "project":"adf6ed437d44e185d7f467a467b15ca9a73269f2",
            "workspace":null,
            "payload":{
                "id":"evt-g1",
                "type":"message.part.updated",
                "properties":{
                    "sessionID":"ses-abc",
                    "messageID":"msg-1",
                    "part":{"type":"reasoning","id":"prt-2","text":"thinking..."}
                }
            }
        }"#;
        let ev = parse_frame(data).expect("decode global envelope");
        match ev {
            ServerEvent::MessagePartUpdated(p) => {
                assert_eq!(p.part.reasoning(), Some("thinking..."));
            }
            other => panic!("expected MessagePartUpdated, got {other:?}"),
        }
    }

    #[test]
    fn parses_global_event_sync_envelope() {
        let data = r#"{
            "directory":"/x",
            "project":"p",
            "payload":{
                "type":"sync",
                "syncEvent":{
                    "type":"message.part.updated.1",
                    "id":"evt-s1",
                    "seq":3,
                    "aggregateID":"ses-abc",
                    "data":{
                        "sessionID":"ses-abc",
                        "part":{"type":"reasoning","id":"prt-r","text":"deep thoughts"},
                        "time":1700
                    }
                }
            }
        }"#;
        let ev = parse_frame(data).expect("decode sync envelope");
        match ev {
            ServerEvent::MessagePartUpdated(p) => {
                assert_eq!(p.part.reasoning(), Some("deep thoughts"));
                assert_eq!(p.session_id.as_str(), "ses-abc");
            }
            other => panic!("expected MessagePartUpdated, got {other:?}"),
        }
    }

    #[test]
    fn strips_version_suffix_only_for_numeric_tails() {
        assert_eq!(
            strip_version_suffix("message.part.updated.1"),
            "message.part.updated"
        );
        assert_eq!(
            strip_version_suffix("message.part.updated.12"),
            "message.part.updated"
        );
        assert_eq!(strip_version_suffix("session.idle"), "session.idle");
        assert_eq!(
            strip_version_suffix("permission.requested"),
            "permission.requested"
        );
    }

    #[test]
    fn extract_data_concatenates_multiline() {
        let frame = "data: line1\ndata: line2";
        assert_eq!(extract_data_field(frame), Some("line1\nline2".to_string()));
    }

    #[test]
    fn extract_data_ignores_comments_and_other_fields() {
        let frame = ": ping\nevent: foo\ndata: hello";
        assert_eq!(extract_data_field(frame), Some("hello".to_string()));
    }

    #[test]
    fn frame_end_picks_earliest_separator() {
        let buf = "a\n\nb";
        assert_eq!(find_frame_end(buf), Some(1));
        let buf = "a\r\n\r\nb";
        assert_eq!(find_frame_end(buf), Some(1));
    }

    #[test]
    fn reconnect_delay_matches_opencode_tui() {
        assert_eq!(reconnect_delay(1), Duration::from_secs(1));
        assert_eq!(reconnect_delay(2), Duration::from_secs(2));
        assert_eq!(reconnect_delay(3), Duration::from_secs(4));
        assert_eq!(reconnect_delay(4), Duration::from_secs(8));
        assert_eq!(reconnect_delay(5), Duration::from_secs(16));
        assert_eq!(reconnect_delay(6), Duration::from_secs(30));
        assert_eq!(reconnect_delay(99), Duration::from_secs(30));
        assert_eq!(reconnect_delay(0), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn connect_failures_apply_exponential_backoff() {
        use futures::StreamExt;
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let calls = Arc::new(AtomicU32::new(0));
        let calls_for_closure = calls.clone();
        let stream = EventStream::new(move || {
            let calls = calls_for_closure.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                let client = reqwest::Client::builder()
                    .connect_timeout(Duration::from_millis(50))
                    .build()
                    .expect("client");
                let resp = client.get("http://127.0.0.1:1").send().await;
                resp.map_err(Error::Transport)
            }
        });

        let start = tokio::time::Instant::now();
        let mut stream = stream;
        let mut errors = 0u32;
        let mut reconnecting = 0u32;
        while errors < 6 {
            match stream.next().await {
                Some(StreamItem::Error(_)) => errors += 1,
                Some(StreamItem::Reconnecting { .. }) => reconnecting += 1,
                Some(StreamItem::Event(ev)) => panic!("unexpected event: {ev:?}"),
                None => panic!("stream ended unexpectedly"),
            }
        }
        let elapsed = start.elapsed();
        let expected_min = Duration::from_secs(61);
        assert!(
            elapsed >= expected_min,
            "6 failed connects must accumulate >= 61 s of virtual backoff \
             (1+2+4+8+16+30 = 61); got elapsed={elapsed:?}, attempts={}, \
             reconnecting_markers={reconnecting}. A near-zero elapsed time \
             means the storm bug is back.",
            calls.load(Ordering::SeqCst),
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            errors,
            "every Error item must correspond to exactly one connect call",
        );
        assert_eq!(
            reconnecting,
            errors - 1,
            "Reconnecting markers should bracket Errors after the first",
        );
    }
}
