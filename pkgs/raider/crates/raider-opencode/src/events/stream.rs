use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use futures::{Stream, StreamExt};

use crate::error::{Error, Result};

use super::decode::{absorb, parse_frame};
use super::types::ServerEvent;

pub struct EventStream {
    inner: Pin<Box<dyn Stream<Item = StreamItem> + Send>>,
}

#[derive(Debug)]
pub enum StreamItem {
    Event(Box<ServerEvent>),
    Error(Error),
    Reconnecting { attempt: u32 },
}

impl Stream for EventStream {
    type Item = StreamItem;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

pub(crate) const RECONNECT_BASE: Duration = Duration::from_secs(1);

pub(crate) const RECONNECT_MAX: Duration = Duration::from_secs(30);

pub(crate) fn reconnect_delay(attempt: u32) -> Duration {
    if attempt == 0 {
        return Duration::ZERO;
    }
    let shift = (attempt - 1).min(20);
    let ms = RECONNECT_BASE
        .as_millis()
        .saturating_mul(1u128 << shift)
        .min(RECONNECT_MAX.as_millis()) as u64;
    Duration::from_millis(ms)
}

impl EventStream {
    pub(crate) fn new<F, Fut>(connect: F) -> Self
    where
        F: Fn() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<reqwest::Response>> + Send + 'static,
    {
        let stream = futures::stream::unfold(StreamState::new(connect), |mut state| async move {
            let item = state.next_item().await;
            item.map(|i| (i, state))
        });
        Self {
            inner: Box::pin(stream),
        }
    }
}

pub(crate) type BodyStream =
    Pin<Box<dyn Stream<Item = std::result::Result<Bytes, reqwest::Error>> + Send>>;

struct StreamState<F, Fut>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<reqwest::Response>> + Send + 'static,
{
    connect: F,
    body: Option<BodyStream>,
    buffer: String,
    pending_frames: std::collections::VecDeque<String>,
    attempt: u32,
    pending_reconnect_marker: bool,
}

impl<F, Fut> StreamState<F, Fut>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<reqwest::Response>> + Send + 'static,
{
    fn new(connect: F) -> Self {
        Self {
            connect,
            body: None,
            buffer: String::new(),
            pending_frames: std::collections::VecDeque::new(),
            attempt: 0,
            pending_reconnect_marker: false,
        }
    }

    async fn next_item(&mut self) -> Option<StreamItem> {
        loop {
            if let Some(frame) = self.pending_frames.pop_front() {
                return Some(match parse_frame(&frame) {
                    Ok(ev) => StreamItem::Event(Box::new(ev)),
                    Err(e) => StreamItem::Error(e),
                });
            }

            if self.body.is_none() {
                if self.attempt > 0 && !self.pending_reconnect_marker {
                    self.pending_reconnect_marker = true;
                    return Some(StreamItem::Reconnecting {
                        attempt: self.attempt,
                    });
                }

                match (self.connect)().await {
                    Ok(resp) => {
                        self.buffer.clear();
                        self.body = Some(Box::pin(resp.bytes_stream()));
                        self.pending_reconnect_marker = false;
                        // NOTE: do NOT increment `attempt` on success —
                    }
                    Err(e) => {
                        self.attempt = self.attempt.saturating_add(1);
                        self.pending_reconnect_marker = false;
                        tokio::time::sleep(reconnect_delay(self.attempt)).await;
                        return Some(StreamItem::Error(e));
                    }
                }
            }

            let body = self.body.as_mut().expect("body just connected");
            match body.next().await {
                Some(Ok(bytes)) => {
                    absorb(&mut self.buffer, &bytes, &mut self.pending_frames);
                }
                Some(Err(e)) => {
                    self.body = None;
                    self.pending_reconnect_marker = false;
                    self.attempt = self.attempt.saturating_add(1);
                    tokio::time::sleep(reconnect_delay(self.attempt)).await;
                    return Some(StreamItem::Error(Error::Transport(e)));
                }
                None => {
                    self.body = None;
                    self.pending_reconnect_marker = false;
                    self.attempt = self.attempt.saturating_add(1);
                    tokio::time::sleep(reconnect_delay(self.attempt)).await;
                }
            }
        }
    }
}
