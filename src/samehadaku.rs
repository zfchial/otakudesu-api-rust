
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use scraper::{ElementRef, Html, Selector};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use crate::{err, AppState};

const SH_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36";

#[derive(Deserialize)]
pub struct ShSearch { pub q: String, pub limit: Option<usize> }

#[derive(Deserialize)]
pub struct ShPage { pub page: Option<u32> }

#[derive(Deserialize)]
pub struct ShUrl { pub url: String }

#[derive(Deserialize)]
pub struct ShCatalog {
    pub genre: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub order: Option<String>,
    pub page: Option<u32>,
}

fn base() -> String {
    std::env::var("SAMEHADAKU_BASE_URL")
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| v.starts_with("http"))
        .unwrap_or_else(|| "https://samehadaku.li".to_string())
}

fn bypass_url() -> Option<String> {
    std::env::var("SAMEHADAKU_BYPASS_URL")
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| v.starts_with("http"))
}

fn sel(s: &str) -> Selector { Selector::parse(s).unwrap() }

fn txt(el: ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn img_src(el: ElementRef) -> String {
    let pick = |a: &str| el.value().attr(a).map(str::trim).filter(|v| v.starts_with("http")).map(str::to_string);
    pick("data-src").or_else(|| pick("data-lazy-src")).or_else(|| pick("src")).unwrap_or_default()
}

fn host_ok(url: &str) -> bool {
    url.starts_with("http") && url.contains("samehadaku")
}

async fn get_html(s: &AppState, url: &str) -> Result<Html, String> {
    let mut last_err = String::new();

    if let Some(sidecar) = bypass_url() {
        let endpoint = format!("{sidecar}/fetch?url={}", urlencoding::encode(url));
        match s.client.get(&endpoint).timeout(Duration::from_secs(90)).send().await {
            Ok(r) => match r.json::<Value>().await {
                Ok(v) => {
                    if v["ok"].as_bool().unwrap_or(false) {
                        if let Some(html) = v["html"].as_str() {
                            return Ok(Html::parse_document(html));
                        }
                    }
                    last_err = format!("bypass gagal: {}", v["error"].as_str().unwrap_or("challenge tidak lewat"));
                }
                Err(e) => last_err = format!("bypass respons tidak valid: {e}"),
            },
            Err(e) => last_err = format!("sidecar tidak dapat dihubungi: {e}"),
        }
    }

    let r = s
        .client
        .get(url)
        .header("User-Agent", SH_UA)
        .header("Referer", format!("{}/", base()))
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| {
            if last_err.is_empty() { format!("samehadaku tidak dapat dihubungi: {e}") } else { format!("{last_err}; direct fetch: {e}") }
        })?;
    if !r.status().is_success() {
        return Err(if last_err.is_empty() { format!("samehadaku HTTP {}", r.status()) } else { format!("{last_err}; direct HTTP {}", r.status()) });
    }
    Ok(Html::parse_document(&r.text().await.map_err(|e| e.to_string())?))
}

fn is_how(h: &Html) -> bool {
    if h.select(&sel(".infox, .eplister, div.bsx, .aspd-download")).next().is_some() {
        return false;
    }
    h.select(&sel("article.animepost, .animposx, .east_player_option, .download-eps, a.series")).next().is_some()
}

fn theme_of(h: &Html) -> &'static str {
    if is_how(h) { "how" } else { "li" }
}


fn card_li(el: ElementRef) -> Option<Value> {
    let a = el.select(&sel("a.tip")).next().or_else(|| el.select(&sel("a[href]")).next())?;
    let href = a.value().attr("href").unwrap_or("").to_string();
    if href.is_empty() {
        return None;
    }
    let title = a.value().attr("title").map(str::to_string).filter(|t| !t.trim().is_empty())
        .or_else(|| el.select(&sel(".tt, .title, h2")).next().map(txt)).unwrap_or_default();
    Some(json!({
        "title": title,
        "url": href,
        "image": el.select(&sel("img")).next().map(img_src).unwrap_or_default(),
        "type": el.select(&sel(".typez")).next().map(txt).unwrap_or_default(),
        "info": el.select(&sel(".epx")).next().map(txt).unwrap_or_default(),
        "sub": el.select(&sel(".sb")).next().map(txt).unwrap_or_default(),
        "status_class": el.select(&sel(".status")).next().map(txt).unwrap_or_default(),
        "source": "samehadaku",
    }))
}

fn card_how(el: ElementRef) -> Option<Value> {
    let a = el.select(&sel(".animposx > a[href]")).next().or_else(|| el.select(&sel("a[href]")).next())?;
    let href = a.value().attr("href").unwrap_or("").to_string();
    if href.is_empty() {
        return None;
    }
    let title = a.value().attr("title").map(str::to_string).filter(|t| !t.trim().is_empty())
        .or_else(|| el.select(&sel(".title, h2, h3")).next().map(txt)).unwrap_or_default();
    let score = el.select(&sel(".score")).next().map(txt).unwrap_or_default().replace('★', "").trim().to_string();
    Some(json!({
        "title": title,
        "url": href,
        "image": el.select(&sel(".content-thumb img, img")).next().map(img_src).unwrap_or_default(),
        "type": el.select(&sel(".type")).next().map(txt).unwrap_or_default(),
        "info": score,
        "rating": score,
        "source": "samehadaku",
    }))
}

fn card_series(el: ElementRef) -> Option<Value> {
    let href = el.value().attr("href").unwrap_or("").to_string();
    if href.is_empty() {
        return None;
    }
    let title = el.value().attr("title").map(str::to_string).filter(|t| !t.trim().is_empty())
        .or_else(|| el.select(&sel(".judul")).next().map(txt)).unwrap_or_default();
    let rating = el.select(&sel(".rating")).next().map(txt).unwrap_or_default().replace('★', "").trim().to_string();
    Some(json!({
        "title": title,
        "url": href,
        "image": el.select(&sel("img")).next().map(img_src).unwrap_or_default(),
        "type": "",
        "info": rating,
        "rating": rating,
        "source": "samehadaku",
    }))
}

fn card_how_episode(li: ElementRef) -> Option<Value> {
    let ep_a = li.select(&sel(".thumb a[href]")).next();
    let title_a = li.select(&sel(".dtla h2.entry-title a[href], .entry-title a[href]")).next();
    let url = ep_a.and_then(|a| a.value().attr("href")).map(str::to_string)
        .or_else(|| title_a.and_then(|a| a.value().attr("href")).map(str::to_string))?;
    if url.is_empty() {
        return None;
    }
    let title = title_a.and_then(|a| a.value().attr("title")).map(str::to_string)
        .filter(|t| !t.trim().is_empty())
        .or_else(|| title_a.map(txt))
        .or_else(|| ep_a.and_then(|a| a.value().attr("title")).map(str::to_string))
        .unwrap_or_default();
    Some(json!({
        "title": title,
        "url": url,
        "image": li.select(&sel(".thumb img, img")).next().map(img_src).unwrap_or_default(),
        "type": "",
        "info": li.select(&sel(".dtla span, .dtla small")).map(txt).collect::<Vec<_>>().join(" "),
        "source": "samehadaku",
    }))
}

fn cards(h: &Html, limit: usize) -> Vec<Value> {
    let mut out: Vec<Value> = h.select(&sel("div.bsx")).filter_map(card_li).collect();
    if out.is_empty() {
        out = h.select(&sel("article.animepost")).filter_map(card_how).collect();
    }
    if out.is_empty() {
        out = h.select(&sel("a.series[href]")).filter_map(card_series).collect();
        out.truncate(limit);
        if !out.is_empty() {
            return out;
        }
        let eps: Vec<Value> = h.select(&sel("li")).filter_map(card_how_episode).collect();
        out.extend(eps);
    }
    out.truncate(limit);
    out
}


fn episode_num(href: &str) -> u64 {
    href.split("episode-").nth(1)
        .map(|tail| tail.chars().take_while(|c| c.is_ascii_digit()).collect::<String>())
        .and_then(|d| d.parse::<u64>().ok())
        .unwrap_or(0)
}

fn synopsis(h: &Html) -> String {
    h.select(&sel(".entry-content p"))
        .map(txt)
        .filter(|t| t.len() > 25 && !t.starts_with("(Source"))
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
}

fn anime_li(h: &Html) -> Value {
    let mut info = serde_json::Map::new();
    for sp in h.select(&sel(".info-content .spe span, .spe span")) {
        let label = sp.select(&sel("b")).next().map(txt).unwrap_or_default();
        let key = label.trim_end_matches(':').trim().to_string();
        if key.is_empty() {
            continue;
        }
        let full = txt(sp);
        let val = full.split_once(':').map(|(_, v)| v.trim().to_string()).unwrap_or_else(|| full.trim().to_string());
        if !val.is_empty() {
            info.insert(key, json!(val));
        }
    }
    let episodes: Vec<Value> = h.select(&sel(".eplister ul li, .eplister li")).filter_map(|li| {
        let a = li.select(&sel("a[href]")).next()?;
        let url = a.value().attr("href").unwrap_or("").to_string();
        if url.is_empty() { return None; }
        Some(json!({
            "num": li.select(&sel(".epl-num")).next().map(txt).and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or_else(|| episode_num(&url)),
            "title": li.select(&sel(".epl-title")).next().map(txt).unwrap_or_default(),
            "url": url,
            "date": li.select(&sel(".epl-date")).next().map(txt).unwrap_or_default(),
        }))
    }).collect();

    json!({
        "info": {
            "title": h.select(&sel("h1.entry-title")).next().map(txt).unwrap_or_default(),
            "image": h.select(&sel(".thumb img, .animefull .thumb img, .postbody .thumb img")).next().map(img_src).unwrap_or_default(),
            "status": info.get("Status").cloned().unwrap_or(json!("")),
            "type": info.get("Type").cloned().unwrap_or(json!("")),
            "episodes": info.get("Episodes").cloned().unwrap_or(json!("")),
            "released": info.get("Released").cloned().unwrap_or(json!("")),
            "duration": info.get("Duration").cloned().unwrap_or(json!("")),
            "studio": info.get("Studio").cloned().unwrap_or(json!("")),
            "synopsis": synopsis(h),
        },
        "info_raw": Value::Object(info),
        "episodes": episodes,
    })
}

fn anime_how(h: &Html) -> Value {
    let mut info = serde_json::Map::new();
    for sp in h.select(&sel("span.w-28")) {
        let key = txt(sp).trim_end_matches(':').trim().to_string();
        if key.is_empty() { continue; }
        let val = sp.next_siblings()
            .find_map(|n| {
                if let Some(t) = n.value().as_text() {
                    let t = t.trim();
                    if !t.is_empty() { return Some(t.to_string()); }
                    return None;
                }
                let el = ElementRef::wrap(n)?;
                let t = txt(el);
                if t.is_empty() { None } else { Some(t) }
            })
            .unwrap_or_default();
        if !val.is_empty() {
            info.insert(key, json!(val));
        }
    }

    let image = h.select(&sel("meta[property='og:image'], meta[name='og:image']")).next()
        .and_then(|m| m.value().attr("content")).unwrap_or("").to_string();

    let mut seen = Vec::<String>::new();
    let mut episodes = Vec::<Value>::new();
    for a in h.select(&sel("a[href*='episode-']")) {
        let url = a.value().attr("href").unwrap_or("").to_string();
        if url.is_empty() || seen.contains(&url) {
            continue;
        }
        seen.push(url.clone());
        let num = episode_num(&url);
        let label = txt(a);
        let generic = label.is_empty() || label.chars().all(|c| c.is_ascii_digit());
        episodes.push(json!({
            "num": num,
            "title": if generic { format!("Episode {num}") } else { label },
            "url": url,
            "date": "",
        }));
    }
    episodes.sort_by_key(|e| e["num"].as_u64().unwrap_or(0));

    json!({
        "info": {
            "title": h.select(&sel("h1")).next().map(txt).unwrap_or_default(),
            "image": image,
            "status": info.get("Status").cloned().unwrap_or(json!("")),
            "type": info.get("Type").cloned().unwrap_or(json!("")),
            "episodes": info.get("Total Episode").cloned().unwrap_or(json!("")),
            "released": info.get("Season").cloned().unwrap_or(json!("")),
            "duration": info.get("Duration").cloned().unwrap_or(json!("")),
            "studio": info.get("Studio").cloned().unwrap_or(json!("")),
            "synopsis": synopsis(h),
        },
        "info_raw": Value::Object(info),
        "episodes": episodes,
    })
}


fn episode_li(h: &Html) -> Value {
    let embed = h.select(&sel("#pembed iframe, .player-embed iframe, iframe[data-litespeed-src], iframe[data-src]")).next()
        .and_then(|f| f.value().attr("data-litespeed-src").or_else(|| f.value().attr("data-src")).or_else(|| f.value().attr("src")))
        .unwrap_or("").to_string();

    let mut downloads: Vec<Value> = Vec::new();
    for a in h.select(&sel(".aspd-download, .aspd-section a[href*='http']")) {
        let url = a.value().attr("href").unwrap_or("").to_string();
        if url.is_empty() { continue; }
        let quality = txt(a);
        if let Some(d) = downloads.iter_mut().find(|d| d["url"] == json!(url)) {
            let qs = d["quality"].as_str().unwrap_or("").to_string();
            let merged = if qs.is_empty() { quality.clone() } else if quality.is_empty() || qs.contains(&quality) { qs } else { format!("{qs}, {quality}") };
            d["quality"] = json!(merged);
        } else {
            downloads.push(json!({ "format": "", "quality": quality, "name": "", "url": url }));
        }
    }

    json!({
        "title": h.select(&sel("h1.entry-title, .entry-title")).next().map(txt).unwrap_or_default(),
        "embed_url": embed,
        "mirrors": if embed.is_empty() { Vec::<Value>::new() } else { vec![json!({"name": "Blogger", "embed_url": embed})] },
        "downloads": downloads,
        "nav": {
            "prev": h.select(&sel(".naveps a[rel='prev'], .naveps a[aria-label='prev']")).next().and_then(|a| a.value().attr("href")).unwrap_or(""),
            "next": h.select(&sel(".naveps a[rel='next'], .naveps a[aria-label='next']")).next().and_then(|a| a.value().attr("href")).unwrap_or(""),
        },
    })
}

fn embed_src(html: &str) -> String {
    for marker in ["src=\"", "src='"] {
        if let Some(pos) = html.find(marker) {
            let rest = &html[pos + marker.len()..];
            let quote = marker.chars().last().unwrap();
            if let Some(end) = rest.find(quote) {
                let src = rest[..end].trim().to_string();
                if src.starts_with("http") {
                    return src;
                }
            }
        }
    }
    String::new()
}

fn episode_how(h: &Html) -> Value {
    let mut mirrors: Vec<Value> = Vec::new();
    for opt in h.select(&sel(".east_player_option")) {
        let name = txt(opt);
        let src = opt.value().attr("data-embed").map(embed_src).unwrap_or_default();
        if src.is_empty() { continue; }
        mirrors.push(json!({
            "name": if name.is_empty() { "Server".to_string() } else { name },
            "embed_url": src,
            "index": opt.value().attr("data-nume").unwrap_or(""),
        }));
    }
    let embed = h.select(&sel("#pembed iframe, .player-embed iframe")).next()
        .and_then(|f| f.value().attr("src").or_else(|| f.value().attr("data-src")))
        .map(str::to_string)
        .filter(|v| v.starts_with("http"))
        .unwrap_or_else(|| mirrors.first().and_then(|m| m["embed_url"].as_str()).unwrap_or("").to_string());

    let mut downloads: Vec<Value> = Vec::new();
    for block in h.select(&sel(".download-eps")) {
        let format = block.select(&sel("p b")).next().map(txt).unwrap_or_default();
        for li in block.select(&sel("ul li")) {
            let quality = li.select(&sel("strong")).next().map(txt).unwrap_or_default();
            for a in li.select(&sel(".download-eps-links a[href], a[href^='http']")) {
                let url = a.value().attr("href").unwrap_or("").to_string();
                if url.is_empty() { continue; }
                downloads.push(json!({
                    "format": format,
                    "quality": quality,
                    "name": txt(a),
                    "url": url,
                }));
            }
        }
    }

    json!({
        "title": h.select(&sel("h1")).next().map(txt).unwrap_or_default(),
        "embed_url": embed,
        "mirrors": mirrors,
        "downloads": downloads,
        "nav": {
            "prev": h.select(&sel("a[rel='prev']")).next().and_then(|a| a.value().attr("href")).unwrap_or(""),
            "next": h.select(&sel("a[rel='next']")).next().and_then(|a| a.value().attr("href")).unwrap_or(""),
        },
    })
}

pub async fn episode_from_title(s: &AppState, title: &str) -> Result<Value, String> {
    let clean = title
        .replace("Subtitle Indonesia", "")
        .replace("Sub Indo", "")
        .trim()
        .to_string();

    let (name, num) = match clean.rfind(" Episode ") {
        Some(i) => {
            let tail = &clean[i + 9..];
            let n = tail
                .split(|c: char| !c.is_ascii_digit())
                .find(|x| !x.is_empty())
                .and_then(|x| x.parse::<u64>().ok())
                .unwrap_or(0);
            (clean[..i].trim().to_string(), n)
        }
        None => (clean.clone(), 0),
    };
    if name.is_empty() {
        return Err("Judul anime tidak dapat dibaca".to_string());
    }

    let search_url = format!("{}/?s={}", base(), urlencoding::encode(&name));
    let anime_url = {
        let sh = get_html(s, &search_url).await.map_err(|e| format!("pencarian Samehadaku gagal: {e}"))?;
        cards(&sh, 1).first().and_then(|c| c["url"].as_str().map(str::to_string))
            .ok_or_else(|| format!("anime \"{name}\" tidak ditemukan di Samehadaku"))?
    };

    let (ep_url, matched) = {
        let ah = get_html(s, &anime_url).await.map_err(|e| format!("halaman anime Samehadaku gagal: {e}"))?;
        let list = if is_how(&ah) { anime_how(&ah) } else { anime_li(&ah) };
        let episodes = list["episodes"].as_array().cloned().unwrap_or_default();
        let target = episodes.iter()
            .find(|e| e["num"].as_u64().unwrap_or(0) == num)
            .or_else(|| episodes.last())
            .ok_or_else(|| "daftar episode Samehadaku kosong".to_string())?;
        let url = target["url"].as_str().unwrap_or("").to_string();
        if url.is_empty() {
            return Err("URL episode Samehadaku kosong".to_string());
        }
        (url, target["num"].clone())
    };

    let mut data = {
        let eh = get_html(s, &ep_url).await.map_err(|e| format!("halaman episode Samehadaku gagal: {e}"))?;
        let mut d = if is_how(&eh) { episode_how(&eh) } else { episode_li(&eh) };
        d["theme"] = json!(theme_of(&eh));
        d
    };
    if data["title"].as_str().map(str::is_empty).unwrap_or(true) {
        data["title"] = json!(clean);
    }
    data["url"] = json!(ep_url);
    data["anime_url"] = json!(anime_url);
    data["matched_episode"] = matched;
    Ok(data)
}


pub async fn search(State(s): State<Arc<AppState>>, Query(q): Query<ShSearch>) -> Response {
    if q.q.trim().is_empty() {
        return err("Parameter q wajib diisi").into_response();
    }
    let limit = q.limit.unwrap_or(15).clamp(1, 50);
    let url = format!("{}/?s={}", base(), urlencoding::encode(q.q.trim()));
    match get_html(&s, &url).await {
        Ok(h) => {
            let data = cards(&h, limit);
            Json(json!({
                "ok": true, "source": "samehadaku", "theme": theme_of(&h),
                "query": q.q, "count": data.len(), "data": data,
            })).into_response()
        }
        Err(e) => err(e).into_response(),
    }
}

pub async fn latest(State(s): State<Arc<AppState>>, Query(p): Query<ShPage>) -> Response {
    let page = p.page.unwrap_or(1).max(1);
    let url = if page == 1 { format!("{}/", base()) } else { format!("{}/page/{page}", base()) };
    match get_html(&s, &url).await {
        Ok(h) => {
            let data = cards(&h, 40);
            Json(json!({
                "ok": true, "source": "samehadaku", "theme": theme_of(&h),
                "page": page, "count": data.len(), "data": data,
            })).into_response()
        }
        Err(e) => err(e).into_response(),
    }
}

pub async fn catalog(State(s): State<Arc<AppState>>, Query(q): Query<ShCatalog>) -> Response {
    let page = q.page.unwrap_or(1).max(1);
    let mut params: Vec<String> = Vec::new();
    if let Some(g) = q.genre.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        params.push(format!("genre[]={}", urlencoding::encode(g)));
    }
    if let Some(v) = q.status.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        params.push(format!("status={}", urlencoding::encode(v)));
    }
    if let Some(v) = q.kind.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        params.push(format!("type={}", urlencoding::encode(v)));
    }
    if let Some(v) = q.order.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        params.push(format!("order={}", urlencoding::encode(v)));
    }
    if page > 1 {
        params.push(format!("page={page}"));
    }
    let url = if params.is_empty() { format!("{}/anime/", base()) } else { format!("{}/anime/?{}", base(), params.join("&")) };

    match get_html(&s, &url).await {
        Ok(h) => {
            let data = cards(&h, 50);
            Json(json!({
                "ok": true, "source": "samehadaku", "theme": theme_of(&h),
                "page": page, "count": data.len(), "data": data,
            })).into_response()
        }
        Err(e) => err(e).into_response(),
    }
}

pub async fn anime(State(s): State<Arc<AppState>>, Query(q): Query<ShUrl>) -> Response {
    if !host_ok(&q.url) {
        return err("URL harus dari samehadaku").into_response();
    }
    match get_html(&s, &q.url).await {
        Ok(h) => {
            let theme = theme_of(&h);
            let mut data = if is_how(&h) { anime_how(&h) } else { anime_li(&h) };
            data["url"] = json!(q.url);
            data["theme"] = json!(theme);
            Json(json!({ "ok": true, "source": "samehadaku", "theme": theme, "data": data })).into_response()
        }
        Err(e) => err(e).into_response(),
    }
}

pub async fn episode(State(s): State<Arc<AppState>>, Query(q): Query<ShUrl>) -> Response {
    if !host_ok(&q.url) {
        return err("URL harus dari samehadaku").into_response();
    }
    match get_html(&s, &q.url).await {
        Ok(h) => {
            let theme = theme_of(&h);
            let mut data = if is_how(&h) { episode_how(&h) } else { episode_li(&h) };
            data["url"] = json!(q.url);
            data["theme"] = json!(theme);
            Json(json!({ "ok": true, "source": "samehadaku", "theme": theme, "data": data })).into_response()
        }
        Err(e) => err(e).into_response(),
    }
}
