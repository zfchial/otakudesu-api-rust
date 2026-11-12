
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use crate::{err, AppState};

const ANIBI_API: &str = "https://api.kisame.cloud/api/v2";
const ANIBI_UA: &str = "okhttp/4.12.0";
const ANIBI_DAYS: [&str; 7] = ["senin", "selasa", "rabu", "kamis", "jumat", "sabtu", "minggu"];

#[derive(Deserialize)]
pub struct AnibiQuery {
    pub q: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub origin: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub status: Option<String>,
    pub platform: Option<String>,
}

#[derive(Deserialize)]
pub struct DayQuery {
    pub day: Option<String>,
}

fn api_key() -> Option<String> {
    std::env::var("ANIBI_API_KEY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn day_key(raw: &str) -> Option<&'static str> {
    let v = raw.trim().to_lowercase();
    let mapped = match v.as_str() {
        "0" | "sunday" | "minggu" | "ahad" => "minggu",
        "1" | "monday" | "senin" => "senin",
        "2" | "tuesday" | "selasa" => "selasa",
        "3" | "wednesday" | "rabu" => "rabu",
        "4" | "thursday" | "kamis" => "kamis",
        "5" | "friday" | "jumat" | "jum'at" => "jumat",
        "6" | "saturday" | "sabtu" => "sabtu",
        other => other,
    };
    ANIBI_DAYS.iter().copied().find(|d| *d == mapped)
}

fn item(v: &Value) -> Value {
    json!({
        "id": v["id"],
        "title": v["title"],
        "slug": v["series_slug"],
        "image": v["cover_url"],
        "type": v["type"],
        "status": v["status"],
        "rating": v["rating"],
        "studio": v["author"],
        "origin": v["content_origin"],
        "platform": v["source_platform"],
        "episodes": v["episode_count"],
        "published": v["published_text"],
        "source": "anibiplay",
    })
}

fn schedule_item(v: &Value) -> Value {
    json!({
        "id": v["schedule_id"],
        "series_id": v["series_id"],
        "title": v["title"],
        "slug": v["series_slug"],
        "image": v["cover_url"],
        "type": v["type"],
        "status": v["status"],
        "rating": v["rating"],
        "origin": v["content_origin"],
        "platform": v["source_platform"],
        "day": v["day_of_week"],
        "time": v["time_label"],
        "views": v["views"],
        "airing_today": v["isAiringToday"],
        "released_today": v["hasReleasedToday"],
        "last_episode": v["last_episode"],
        "source": "anibiplay",
    })
}

async fn send(s: &AppState, req: reqwest::RequestBuilder) -> Result<Value, String> {
    let r = req
        .header("User-Agent", ANIBI_UA)
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("anibiplay tidak dapat dihubungi: {e}"))?;
    if !r.status().is_success() {
        return Err(format!("anibiplay HTTP {}", r.status()));
    }
    r.json::<Value>().await.map_err(|e| e.to_string())
}

fn base(path: &str) -> String {
    format!("{ANIBI_API}{path}")
}

fn opt(v: &Option<String>) -> Option<String> {
    v.as_deref().map(str::trim).filter(|x| !x.is_empty()).map(|x| x.to_lowercase())
}

pub async fn search(State(s): State<Arc<AppState>>, Query(q): Query<AnibiQuery>) -> Response {
    let page = q.page.unwrap_or(1).max(1);
    let limit = q.limit.unwrap_or(20).clamp(1, 50);

    let mut payload = json!({ "page": page, "limit": limit });
    if let Some(v) = q.q.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        payload["q"] = json!(v);
    }
    if let Some(v) = opt(&q.origin) {
        payload["content_origin"] = json!(v);
    }
    if let Some(v) = opt(&q.kind) {
        payload["type"] = json!(v);
    }
    if let Some(v) = opt(&q.status) {
        payload["status"] = json!(v);
    }
    if let Some(v) = opt(&q.platform) {
        payload["source_platform"] = json!(v);
    }

    let req = s.client.post(base("/search")).json(&payload);
    match send(&s, req).await {
        Ok(v) => {
            let data: Vec<Value> = v["data"].as_array().map(|a| a.iter().map(item).collect()).unwrap_or_default();
            Json(json!({
                "ok": true,
                "source": "anibiplay",
                "page": v["page"].as_u64().unwrap_or(page as u64),
                "total_pages": v["total_pages"].as_u64().unwrap_or(1),
                "total": v["total"].as_u64().unwrap_or(data.len() as u64),
                "count": data.len(),
                "data": data,
            }))
            .into_response()
        }
        Err(e) => err(e).into_response(),
    }
}

pub async fn schedule(State(s): State<Arc<AppState>>, Query(q): Query<DayQuery>) -> Response {
    let req = s.client.get(base("/jadwal"));
    let v = match send(&s, req).await {
        Ok(v) => v,
        Err(e) => return err(e).into_response(),
    };
    let raw = v["data"].clone();

    if let Some(want) = q.day.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        let key = match day_key(want) {
            Some(k) => k,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "ok": false,
                        "error": format!("Hari \"{want}\" tidak dikenal. Pilihan: senin, selasa, rabu, kamis, jumat, sabtu, minggu.")
                    })),
                )
                    .into_response()
            }
        };
        let list: Vec<Value> = raw[key].as_array().map(|a| a.iter().map(schedule_item).collect()).unwrap_or_default();
        return Json(json!({
            "ok": true,
            "source": "anibiplay",
            "day": key,
            "count": list.len(),
            "data": list,
        }))
        .into_response();
    }

    let mut weekly = serde_json::Map::new();
    let mut total = 0usize;
    for d in ANIBI_DAYS {
        let list: Vec<Value> = raw[d].as_array().map(|a| a.iter().map(schedule_item).collect()).unwrap_or_default();
        total += list.len();
        weekly.insert(d.to_string(), json!(list));
    }
    Json(json!({
        "ok": true,
        "source": "anibiplay",
        "total": v["total"].as_u64().unwrap_or(total as u64),
        "count": total,
        "data": weekly,
    }))
    .into_response()
}

pub async fn detail(State(s): State<Arc<AppState>>, Path(slug): Path<String>) -> Response {
    let key = match api_key() {
        Some(k) => k,
        None => {
            return (
                StatusCode::NOT_IMPLEMENTED,
                Json(json!({
                    "ok": false,
                    "error": "Endpoint detail Anibiplay butuh API key upstream (tanpa key: 401 UNAUTHORIZED).",
                    "hint": "Set env ANIBI_API_KEY lalu restart service. Tanpa key, pakai /api/anibi/search yang sudah berisi rating/status/studio/cover.",
                    "slug": slug,
                })),
            )
                .into_response()
        }
    };
    let req = s
        .client
        .get(base(&format!("/detail/{}", urlencoding::encode(&slug))))
        .header("Authorization", format!("Bearer {key}"))
        .header("x-api-key", key);
    match send(&s, req).await {
        Ok(v) => Json(json!({ "ok": true, "source": "anibiplay", "data": v["data"].as_object().is_some().then(|| v["data"].clone()).unwrap_or(v) })).into_response(),
        Err(e) => err(e).into_response(),
    }
}

pub async fn health(State(s): State<Arc<AppState>>) -> Response {
    let req = s.client.get(base("/health"));
    match send(&s, req).await {
        Ok(v) => Json(json!({ "ok": true, "source": "anibiplay", "upstream": v, "api_key": api_key().is_some() })).into_response(),
        Err(e) => err(e).into_response(),
    }
}
