use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::{err, AppState, UA};

const KOMIK: &str = "https://komikindo.ch";
const KOMIK_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[derive(Deserialize)]
pub struct ComicQuery {
    pub q: Option<String>,
    pub url: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<usize>,
    pub slug: Option<String>,
    pub chapter: Option<String>,
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).unwrap()
}

async fn fetch(s: &AppState, url: &str) -> Result<String, String> {
    let r = s
        .client
        .get(url)
        .header("User-Agent", KOMIK_UA)
        .header("Accept", "text/html,application/xhtml+xml")
        .header("Referer", KOMIK)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("Upstream komik status {}", r.status()));
    }
    r.text().await.map_err(|e| e.to_string())
}

fn abs_url(u: &str) -> String {
    let u = u.trim();
    if u.starts_with("http") {
        u.to_string()
    } else if let Some(r) = u.strip_prefix("//") {
        format!("https://{}", r)
    } else if u.starts_with('/') {
        format!("{}{}", KOMIK, u)
    } else {
        format!("{}/{}", KOMIK, u)
    }
}

fn clean(u: &str) -> String {
    let u = u.split('"').next().unwrap_or(u).trim();
    let u = u.replace(' ', "%20");
    abs_url(&u)
}

fn slug_from(url: &str) -> String {
    let p = url.split('/').filter(|s| !s.is_empty()).last().unwrap_or("");
    p.to_string()
}

fn text_of(doc: &Html, s: &str) -> Option<String> {
    doc.select(&sel(s))
        .next()
        .map(|e| e.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
}

pub async fn komik_home(State(s): State<Arc<AppState>>) -> Response {
    match komik_home_data(&s).await {
        Ok(v) => axum::Json(json!({"ok":true,"data":v})).into_response(),
        Err(e) => err(e).into_response(),
    }
}

async fn komik_home_data(s: &AppState) -> Result<Value, String> {
    let html = fetch(s, &format!("{}/", KOMIK)).await?;
    let doc = Html::parse_document(&html);
    let item = sel("div.animepost, div.utao, div.bs, article");
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for el in doc.select(&item) {
        let link = el
            .select(&sel("a[href*='/komik/']"))
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(abs_url);
        let Some(link) = link else { continue };
        if !seen.insert(link.clone()) {
            continue;
        }
        let title = el
            .select(&sel("h3 a, h2 a, h4 a, .ntitle, .tt"))
            .next()
            .map(|a| a.text().collect::<String>().trim().to_string())
            .filter(|t| !t.is_empty())
            .or_else(|| {
                el.select(&sel("a[href*='/komik/']"))
                    .next()
                    .and_then(|a| a.value().attr("title"))
                    .map(|t| t.trim().to_string())
            });
        let Some(title) = title else { continue };
        let cover = el
            .select(&sel("img"))
            .next()
            .and_then(|i| i.value().attr("data-src").or_else(|| i.value().attr("src")))
            .map(clean);
        let chapters = el
            .select(&sel("a[href*='-chapter-']"))
            .map(|a| {
                let href = abs_url(a.value().attr("href").unwrap_or_default());
                let label = a.text().collect::<String>().trim().to_string();
                json!({"title":label,"url":href})
            })
            .take(4)
            .collect::<Vec<_>>();
        out.push(json!({
            "title": title,
            "url": link,
            "cover": cover,
            "chapters": chapters
        }));
        if out.len() >= 24 {
            break;
        }
    }
    Ok(json!({"items": out, "total": out.len()}))
}

pub async fn komik_search(
    State(s): State<Arc<AppState>>,
    Query(q): Query<ComicQuery>,
) -> Response {
    match komik_search_data(&s, &q).await {
        Ok(v) => axum::Json(json!({"ok":true,"data":v})).into_response(),
        Err(e) => err(e).into_response(),
    }
}

async fn komik_search_data(s: &AppState, q: &ComicQuery) -> Result<Value, String> {
    let query = q.q.clone().unwrap_or_default();
    if query.trim().is_empty() {
        return Ok(json!({"items": [], "total": 0}));
    }
    let url = format!("{}/?s={}", KOMIK, urlencoding::encode(query.trim()));
    let html = fetch(s, &url).await?;
    let doc = Html::parse_document(&html);
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for el in doc.select(&sel("div.animepost, article, div.bs, div.utao")) {
        let link = el
            .select(&sel("a[href*='/komik/']"))
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(abs_url);
        let Some(link) = link else { continue };
        if !seen.insert(link.clone()) {
            continue;
        }
        let title = el
            .select(&sel("h3 a, h2 a, h4 a, .ntitle"))
            .next()
            .map(|a| a.text().collect::<String>().trim().to_string())
            .filter(|t| !t.is_empty())
            .or_else(|| {
                el.select(&sel("a[href*='/komik/']"))
                    .next()
                    .and_then(|a| a.value().attr("title"))
                    .map(|t| t.trim().to_string())
            });
        let Some(title) = title else { continue };
        let cover = el
            .select(&sel("img"))
            .next()
            .and_then(|i| i.value().attr("data-src").or_else(|| i.value().attr("src")))
            .map(clean);
        let genres = el
            .select(&sel("div.genreed, .genxed a, a[href*='/konten/']"))
            .map(|a| a.text().collect::<String>().trim().to_string())
            .filter(|t| !t.is_empty() && t.len() < 30)
            .collect::<Vec<_>>();
        items.push(json!({
            "title": title,
            "url": link,
            "cover": cover,
            "genres": genres
        }));
        if items.len() >= 30 {
            break;
        }
    }
    Ok(json!({"items": items, "total": items.len(), "query": query}))
}

pub async fn komik_detail(
    State(s): State<Arc<AppState>>,
    Query(q): Query<ComicQuery>,
) -> Response {
    match komik_detail_data(&s, &q).await {
        Ok(v) => axum::Json(json!({"ok":true,"data":v})).into_response(),
        Err(e) => err(e).into_response(),
    }
}

async fn komik_detail_data(s: &AppState, q: &ComicQuery) -> Result<Value, String> {
    let raw = q
        .url
        .clone()
        .or_else(|| q.slug.clone().map(|s| format!("{}/komik/{}/", KOMIK, s)))
        .ok_or("Parameter url atau slug wajib diisi")?;
    let url = if raw.contains("/komik/") {
        abs_url(&raw)
    } else {
        format!("{}/komik/{}/", KOMIK, raw.trim_start_matches('/'))
    };
    let html = fetch(s, &url).await?;
    let doc = Html::parse_document(&html);
    let title = text_of(&doc, "h1.entry-title, h1, .tt")
        .ok_or("Judul komik tidak ditemukan")?
        .trim_start_matches("Komik ")
        .to_string();
    let cover = doc
        .select(&sel("div.tip img, figure img, .bs img, .thumb img"))
        .next()
        .and_then(|i| i.value().attr("data-src").or_else(|| i.value().attr("src")))
        .map(clean);
    let description = text_of(&doc, "div.desc, .entry-content p, .sinopsis");
    let genres = doc
        .select(&sel("a[href*='/konten/'], .genxed a, .genre a"))
        .map(|a| a.text().collect::<String>().trim().to_string())
        .filter(|t| !t.is_empty() && t.len() < 30)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut chapters = Vec::new();
    for a in doc.select(&sel("div.chapter_list li, div.eps_lst li, div.listeps li")) {
        let link = a
            .select(&sel("a[href*='-chapter-']"))
            .next()
            .and_then(|x| x.value().attr("href"))
            .map(abs_url);
        let Some(link) = link else { continue };
        let name = a
            .select(&sel("a[href*='-chapter-']"))
            .next()
            .map(|x| x.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let date = a
            .select(&sel("span.dt a"))
            .next()
            .map(|x| x.text().collect::<String>().trim().to_string());
        chapters.push(json!({"title": name, "url": link, "date": date}));
    }
    if chapters.is_empty() {
        for a in doc.select(&sel("a[href*='-chapter-']")) {
            let link = abs_url(a.value().attr("href").unwrap_or_default());
            let name = a.text().collect::<String>().trim().to_string();
            if name.is_empty() {
                continue;
            }
            chapters.push(json!({"title": name, "url": link, "date": Value::Null}));
        }
    }
    Ok(json!({
        "title": title,
        "url": url,
        "cover": cover,
        "description": description,
        "genres": genres,
        "chapters": chapters,
        "total_chapters": chapters.len()
    }))
}

pub async fn komik_chapters(
    State(s): State<Arc<AppState>>,
    Query(q): Query<ComicQuery>,
) -> Response {
    match komik_detail_data(&s, &q).await {
        Ok(v) => {
            let chapters = v.get("chapters").cloned().unwrap_or(json!([]));
            axum::Json(json!({"ok":true,"data":{"chapters":chapters,"total":chapters.as_array().map(|a|a.len()).unwrap_or(0)}})).into_response()
        }
        Err(e) => err(e).into_response(),
    }
}

pub async fn komik_reader(
    State(s): State<Arc<AppState>>,
    Query(q): Query<ComicQuery>,
) -> Response {
    match komik_reader_data(&s, &q).await {
        Ok(v) => axum::Json(json!({"ok":true,"data":v})).into_response(),
        Err(e) => err(e).into_response(),
    }
}

async fn komik_reader_data(s: &AppState, q: &ComicQuery) -> Result<Value, String> {
    let raw = q
        .url
        .clone()
        .or_else(|| q.chapter.clone())
        .ok_or("Parameter url chapter wajib diisi")?;
    let url = abs_url(&raw);
    let html = fetch(s, &url).await?;
    let doc = Html::parse_document(&html);
    let title = text_of(&doc, "h1.entry-title, h1")
        .unwrap_or_default()
        .trim_start_matches("Komik ")
        .to_string();
    let mut images = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for img in doc.select(&sel("div.chapter-image img, div.chapter-image div img")) {
        let raw = img
            .value()
            .attr("data-src")
            .or_else(|| img.value().attr("data-lazy-src"))
            .or_else(|| img.value().attr("src"));
        let Some(raw) = raw else { continue };
        let u = clean(raw);
        if u.is_empty() || u.contains("googleusercontent.com") || u.contains("/gif") {
            continue;
        }
        if !u.contains("/data/") && !u.contains("/media/") {
            continue;
        }
        if seen.insert(u.clone()) {
            images.push(json!({"page": images.len() + 1, "url": u}));
        }
    }
    let prev = doc
        .select(&sel("a[rel='prev']"))
        .next()
        .and_then(|a| a.value().attr("href"))
        .map(abs_url);
    let next = doc
        .select(&sel("a[rel='next']"))
        .next()
        .and_then(|a| a.value().attr("href"))
        .map(abs_url)
        .or_else(|| {
            doc.select(&sel("div.nextprev a"))
                .find(|a| {
                    a.text()
                        .collect::<String>()
                        .to_lowercase()
                        .contains("berikutnya")
                })
                .and_then(|a| a.value().attr("href"))
                .map(abs_url)
        });
    let series = doc
        .select(&sel("a[href*='/komik/']"))
        .find(|a| {
            a.text()
                .collect::<String>()
                .to_lowercase()
                .contains("daftar chapter")
        })
        .and_then(|a| a.value().attr("href"))
        .map(abs_url);
    Ok(json!({
        "title": title,
        "url": url,
        "total_images": images.len(),
        "images": images,
        "prev": prev,
        "next": next,
        "series": series
    }))
}

pub async fn komik_health(State(s): State<Arc<AppState>>) -> Response {
    let ok = fetch(&s, &format!("{}/", KOMIK)).await.is_ok();
    axum::Json(json!({"ok":true,"data":{"source":"komikindo","available":ok,"base_url":KOMIK}})).into_response()
}

pub fn _unused(_: &str) -> &str {
    UA
}
