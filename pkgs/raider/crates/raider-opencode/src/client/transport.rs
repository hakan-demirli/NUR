use url::Url;

use crate::error::{Error, Result};

use super::config::ClientConfig;

pub(crate) struct ClientInner {
    pub(crate) config: ClientConfig,
    pub(crate) http: reqwest::Client,
    pub(crate) sse_http: reqwest::Client,
}

impl ClientInner {
    pub(crate) fn build_url(&self, path: &str) -> Result<Url> {
        let mut url = self
            .config
            .base_url
            .join(path.trim_start_matches('/'))
            .map_err(|e| Error::BadUrl(format!("{e}")))?;
        url.query_pairs_mut()
            .append_pair("directory", self.config.directory.as_str());
        Ok(url)
    }

    pub(crate) fn apply_headers(
        &self,
        mut req: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        req = req.header(
            "x-opencode-directory",
            url_escape(self.config.directory.as_str()),
        );
        if let Some(token) = &self.config.token {
            req = req.bearer_auth(token);
        }
        req
    }

    pub(crate) fn ensure_ok(&self, resp: &reqwest::Response, path: &str) -> Result<()> {
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(Error::Http {
                status: status.as_u16(),
                path: path.to_string(),
                body: format!("status {status}"),
            })
        }
    }
}

pub(crate) fn url_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | '/' => out.push(ch),
            _ => {
                let mut buf = [0u8; 4];
                for &b in ch.encode_utf8(&mut buf).as_bytes() {
                    out.push_str(&format!("%{b:02X}"));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_url_appends_directory() {
        let cfg = ClientConfig::new("http://127.0.0.1:4096", "/tmp/work").unwrap();
        let inner = ClientInner {
            config: cfg,
            http: reqwest::Client::new(),
            sse_http: reqwest::Client::new(),
        };
        let url = inner.build_url("/session").unwrap();
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert_eq!(url.port(), Some(4096));
        assert_eq!(url.path(), "/session");
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(
            pairs,
            vec![("directory".to_string(), "/tmp/work".to_string())]
        );
    }

    #[test]
    fn url_escape_keeps_path_chars() {
        assert_eq!(url_escape("/tmp/work"), "/tmp/work");
        assert_eq!(url_escape("a b"), "a%20b");
    }
}
