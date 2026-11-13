use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::sync::Arc;

use crate::{err, AppState, UA};

#[derive(Deserialize)]
pub struct ProxyQuery {
    url: String,
}

fn allowed_host(h: &str) -> bool {
    let h = h.to_lowercase();
    h.ends_with(".odcloud.net")
        || h.ends_with(".googlevideo.com")
        || h.ends_with(".archive.org")
        || h.ends_with(".blogger.com")
        || h.contains(".mp4")
        || h.contains(".m3u8")
}

fn copy_headers(src: &HeaderMap, dst: &mut HeaderMap) {
    let skip = [
        "connection",
        "keep-alive",
        "transfer-encoding",
        "upgrade",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "content-encoding",
        "content-length",
    ];
    for (k, v) in src.iter() {
        let name = k.as_str().to_lowercase();
        if skip.contains(&name.as_str()) {
            continue;
        }
        if let (Ok(n), Ok(val)) = (
            HeaderName::from_bytes(k.as_str().as_bytes()),
            HeaderValue::from_bytes(v.as_bytes()),
        ) {
            dst.insert(n, val);
        }
    }
}

pub async fn proxy(
    State(s): State<Arc<AppState>>,
    Query(q): Query<ProxyQuery>,
    headers: HeaderMap,
) -> Response {
    if !q.url.starts_with("http") {
        return err("URL media tidak valid").into_response();
    }
    let host = q.url.split("/").nth(2).unwrap_or("").to_lowercase();
    if !allowed_host(&host) {
        return err("Host media tidak diizinkan").into_response();
    }

    let mut req = s.client.get(&q.url).header("User-Agent", UA);
    if let Some(r) = headers.get(header::RANGE) {
        req = req.header(header::RANGE, r.as_bytes());
    }
    if let Some(r) = headers.get(header::REFERER) {
        req = req.header(header::REFERER, r.as_bytes());
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return err(e.to_string()).into_response(),
    };

    let status = resp.status();
    let mut out_headers = HeaderMap::new();
    copy_headers(resp.headers(), &mut out_headers);
    out_headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    out_headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Range, Content-Type, Accept"),
    );
    out_headers.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("Content-Length, Content-Range, Accept-Ranges, Content-Type"),
    );
    out_headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, HEAD, OPTIONS"),
    );

    use futures_util::StreamExt;
    let stream = resp.bytes_stream().map(|r| r.map_err(|e| e));

    let mut builder = Response::builder().status(status);
    for (k, v) in out_headers.iter() {
        builder = builder.header(k, v);
    }
    builder.body(Body::from_stream(stream)).unwrap()
}
