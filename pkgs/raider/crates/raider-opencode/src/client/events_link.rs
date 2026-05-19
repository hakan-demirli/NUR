use std::sync::Arc;

use crate::events::EventStream;

use super::Client;

impl Client {
    pub fn events(&self) -> EventStream {
        let inner = Arc::clone(&self.inner);
        EventStream::new(move || {
            let inner = Arc::clone(&inner);
            async move {
                let url = inner.build_url("/global/event")?;
                let mut req = inner
                    .sse_http
                    .get(url)
                    .header("accept", "text/event-stream");
                req = inner.apply_headers(req);
                let resp = req.send().await?;
                inner.ensure_ok(&resp, "/global/event")?;
                Ok(resp)
            }
        })
    }
}
