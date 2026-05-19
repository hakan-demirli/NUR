use std::collections::HashMap;
use std::io::{Read, Write};
use std::time::Duration;

use mlua::{Lua, Table, Value, Variadic};

use crate::bindings::json::json_to_lua;
use crate::marshal::optional_string;
use crate::spec::headers_from_init;

pub(crate) fn install(lua: &Lua, api: &Table) -> mlua::Result<()> {
    let http = lua.create_table()?;
    let fetch = lua.create_function(fetch_lua)?;
    http.set("fetch", fetch.clone())?;
    http.set("request", fetch.clone())?;
    api.set("http", http)?;
    lua.globals().set("fetch", fetch)?;
    Ok(())
}

fn fetch_lua(lua: &Lua, (url, init): (String, Option<Table>)) -> mlua::Result<Table> {
    let method = init
        .as_ref()
        .map(|table| optional_string(table, "method"))
        .transpose()?
        .flatten()
        .unwrap_or_else(|| "GET".to_string());
    let unix = init
        .as_ref()
        .map(|table| optional_string(table, "unix"))
        .transpose()?
        .flatten();
    let body = init
        .as_ref()
        .map(|table| optional_string(table, "body"))
        .transpose()?
        .flatten()
        .unwrap_or_default();
    let headers = init
        .as_ref()
        .map(headers_from_init)
        .transpose()?
        .unwrap_or_default();

    let response = if let Some(unix) = unix {
        http_unix_request(&unix, &method, &url, &headers, &body)?
    } else {
        return Err(mlua::Error::external(
            "fetch currently requires init.unix for Unix-socket HTTP",
        ));
    };

    let table = lua.create_table()?;
    table.set("ok", (200..300).contains(&response.status))?;
    table.set("status", response.status)?;
    table.set("body", response.body.clone())?;
    let headers_table = lua.create_table()?;
    for (key, value) in response.headers {
        headers_table.set(key, value)?;
    }
    table.set("headers", headers_table)?;

    let text_body = response.body.clone();
    table.set(
        "text",
        lua.create_function(move |_, _: Variadic<Value>| Ok(text_body.clone()))?,
    )?;
    let json_body = response.body;
    table.set(
        "json",
        lua.create_function(move |lua, _: Variadic<Value>| {
            let json: serde_json::Value =
                serde_json::from_str(&json_body).map_err(mlua::Error::external)?;
            json_to_lua(lua, &json)
        })?,
    )?;
    Ok(table)
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    headers: HashMap<String, String>,
    body: String,
}

#[cfg(unix)]
fn http_unix_request(
    unix_path: &str,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &str,
) -> mlua::Result<HttpResponse> {
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(unix_path).map_err(mlua::Error::external)?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let path = http_request_path(url);
    let mut request = format!(
        "{} {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n",
        method.to_ascii_uppercase(),
        path,
        body.len()
    );
    for (key, value) in headers {
        request.push_str(key);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    request.push_str(body);

    stream
        .write_all(request.as_bytes())
        .map_err(mlua::Error::external)?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(mlua::Error::external)?;
    parse_http_response(&raw)
}

#[cfg(not(unix))]
fn http_unix_request(
    _unix_path: &str,
    _method: &str,
    _url: &str,
    _headers: &[(String, String)],
    _body: &str,
) -> mlua::Result<HttpResponse> {
    Err(mlua::Error::external(
        "Unix-socket HTTP is only available on Unix platforms",
    ))
}

fn parse_http_response(raw: &[u8]) -> mlua::Result<HttpResponse> {
    let raw = String::from_utf8_lossy(raw);
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| mlua::Error::external("malformed HTTP response"))?;
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| mlua::Error::external("malformed HTTP status line"))?;
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let body = if headers
        .get("transfer-encoding")
        .map(|value| value.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false)
    {
        decode_chunked_body(body)?
    } else {
        body.to_string()
    };
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

fn decode_chunked_body(mut body: &str) -> mlua::Result<String> {
    let mut out = String::new();
    loop {
        let Some((size_hex, rest)) = body.split_once("\r\n") else {
            return Err(mlua::Error::external("malformed chunked HTTP body"));
        };
        let size = usize::from_str_radix(size_hex.trim(), 16).map_err(mlua::Error::external)?;
        if size == 0 {
            return Ok(out);
        }
        if rest.len() < size + 2 {
            return Err(mlua::Error::external("truncated chunked HTTP body"));
        }
        out.push_str(&rest[..size]);
        body = &rest[size + 2..];
    }
}

fn http_request_path(url: &str) -> &str {
    let Some(after_scheme) = url.split_once("://").map(|(_, rest)| rest) else {
        return url;
    };
    after_scheme
        .find('/')
        .map(|idx| &after_scheme[idx..])
        .unwrap_or("/")
}
