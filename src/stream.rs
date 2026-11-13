use crate::AppState;
use serde_json::{json, Value};

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '\\' && i + 5 < b.len() && b[i + 1] == 'u' {
            let hex: String = b[i + 2..i + 6].iter().collect();
            if let Ok(code) = u32::from_str_radix(&hex, 16) {
                if let Some(c) = char::from_u32(code) {
                    out.push(c);
                    i += 6;
                    continue;
                }
            }
        }
        if b[i] == '\\' && i + 1 < b.len() && b[i + 1] == '/' {
            out.push('/');
            i += 2;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn clean_url(u: &str) -> String {
    let t = u.trim();
    let t = t.trim_matches(|c| c == '"' || c == '\'' || c == '`' || c == ' ');
    let t = unescape(t);
    let end = t
        .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == ',' || c == '>' || c == '<')
        .unwrap_or(t.len());
    t[..end].trim_end_matches('\\').to_string()
}

fn push_candidate(list: &mut Vec<String>, raw: &str) {
    let u = clean_url(raw);
    if u.starts_with("http") && u.len() > 12 && !list.contains(&u) {
        list.push(u);
    }
}

fn extract_candidates(html: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let lower = html.to_lowercase();

    let markers = ["file:\"", "file: '", "file=\"", "src=\"", "src='", "url\":\"", "url:'", "url=\"", "url='"];
    for m in markers.iter() {
        let mut start = 0;
        while let Some(pos) = lower[start..].find(m) {
            let abs = start + pos + m.len();
            let rest = &html[abs..];
            let stop = rest
                .find(|c: char| c == '"' || c == '\'' || c == '<' || c == '>' || c == '\n')
                .unwrap_or(rest.len().min(400));
            push_candidate(&mut out, &rest[..stop]);
            start = abs + stop.max(1);
            if start >= html.len() {
                break;
            }
        }
    }

    let needles = ["videoplayback?", ".mp4", ".m3u8", ".webm"];
    for n in needles.iter() {
        let mut start = 0;
        while let Some(pos) = lower[start..].find(n) {
            let mut back = start + pos;
            while back > 0 {
                let c = html.as_bytes()[back - 1] as char;
                if c == '"' || c == '\'' || c == ' ' || c == '=' || c == '(' {
                    break;
                }
                back -= 1;
            }
            let mut fwd = start + pos;
            let bytes = html.as_bytes();
            while fwd < bytes.len() {
                let c = bytes[fwd] as char;
                if c == '"' || c == '\'' || c == ' ' || c == '<' || c == '>' || c == '\\' {
                    break;
                }
                fwd += 1;
            }
            push_candidate(&mut out, &html[back..fwd]);
            start = start + pos + n.len();
            if start >= html.len() {
                break;
            }
        }
    }

    out
}

fn duration_from(url: &str) -> Option<u64> {
    let idx = url.find("dur=")?;
    let tail = &url[idx + 4..];
    let end = tail
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(tail.len());
    let val: f64 = tail[..end].parse().ok()?;
    if val <= 0.0 {
        None
    } else {
        Some(val.round() as u64)
    }
}

fn kind_of(url: &str) -> String {
    let lower = url.to_lowercase();
    if lower.contains(".m3u8") {
        "hls".to_string()
    } else if lower.contains("googlevideo.com") {
        "googlevideo".to_string()
    } else if lower.contains(".mp4") {
        "mp4".to_string()
    } else if lower.contains(".webm") {
        "webm".to_string()
    } else {
        "direct".to_string()
    }
}

pub async fn resolve(s: &AppState, embed_url: &str) -> Result<Value, String> {
    if !embed_url.starts_with("http") {
        return Err("URL embed tidak valid".to_string());
    }
    let host = embed_url
        .split("/")
        .nth(2)
        .unwrap_or("")
        .replace("www.", "");
    let resp = s
        .client
        .get(embed_url)
        .header("User-Agent", crate::UA)
        .header("Referer", format!("https://{host}/"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("upstream HTTP {}", resp.status()));
    }
    let html = resp.text().await.map_err(|e| e.to_string())?;
    let cands = extract_candidates(&html);
    for c in cands.iter() {
        let k = kind_of(c);
        if k == "hls" || k == "googlevideo" || k == "mp4" || k == "webm" {
            return Ok(json!({
                "ok": true,
                "data": {
                    "media_url": c,
                    "kind": k,
                    "duration_seconds": duration_from(c),
                    "source_embed": embed_url
                }
            }));
        }
    }
    Err("URL media langsung tidak ditemukan pada mirror ini".to_string())
}
