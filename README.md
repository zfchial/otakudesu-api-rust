# OtakuDesu API Rust

REST API async untuk data anime, daftar episode, streaming mirror, download link, dan proxy thumbnail dari OtakuDesu.

Project ini adalah backend Rust untuk NanoStream. Frontend tetap terpisah dan menggunakan API melalui port `8686`.

## Fitur

- Rust + Tokio + Axum
- Parsing HTML dengan Scraper
- HTTP client async dengan Reqwest
- CORS untuk frontend terpisah
- Resolve embed streaming OtakuDesu
- Previous/Next episode navigation
- Proxy thumbnail dengan cache browser 1 hari
- Binary release untuk deployment tanpa Python runtime

## Requirements

- Rust stable
- Cargo

Install Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

## Menjalankan

Development:

```bash
cd /DATA/Documents/coding/otakudesu-api-rust
cargo run
```

Release:

```bash
cargo build --release
./target/release/otakudesu-api-rust
```

Server berjalan pada:

```text
http://localhost:8686
```

API bind ke `0.0.0.0:8686`, sehingga dapat diakses frontend dari perangkat lain pada jaringan yang sama.

## Endpoint

```text
GET /
GET /api/search?q=frieren&limit=12
GET /api/ongoing?page=1
GET /api/complete?page=1
GET /api/genres
GET /api/genre/{slug}?page=1
GET /api/detail?url={anime_url}
GET /api/episode?url={episode_url}
GET /api/embed?id={id}&i={mirror_index}&q={quality}
GET /api/image?url={image_url}

# Sumber kedua: Anibiplay (api.kisame.cloud)
GET /api/anibi/search?q=frieren&limit=20&origin=anime|donghua&type=TV|movie|OVA|ONA&status=Ongoing|Completed&platform=animekita|animein|kuramanime|nanime
GET /api/anibi/schedule?day=senin
GET /api/anibi/detail/{slug}          (butuh env ANIBI_API_KEY)
GET /api/anibi/health

# Sumber ketiga: Samehadaku (samehadaku.li)
GET /api/samehadaku/search?q=one+piece&limit=15
GET /api/samehadaku/latest?page=1
GET /api/samehadaku/catalog?genre=action&status=ongoing&type=tv&order=latest&page=1
GET /api/samehadaku/anime?url={anime_url}
GET /api/samehadaku/episode?url={episode_url}
```

Semua endpoint JSON mengembalikan format utama:

```json
{
  "ok": true,
  "data": []
}
```

## Contoh

```bash
curl "http://localhost:8686/api/search?q=frieren"
curl "http://localhost:8686/api/ongoing?page=1"
curl "http://localhost:8686/api/genres"
curl "http://localhost:8686/api/genre/action?page=1"
```

Detail anime:

```bash
curl --get "http://localhost:8686/api/detail" \
  --data-urlencode "url=https://otakudesu.blog/anime/sousou-frieren-s2-sub-indo/"
```

Detail episode:

```bash
curl --get "http://localhost:8686/api/episode" \
  --data-urlencode "url=https://otakudesu.blog/episode/knh-s3-episode-1-sub-indo/"
```

Response episode menyediakan navigasi:

```json
{
  "nav": {
    "prev": "",
    "next": "https://otakudesu.blog/episode/knh-s3-episode-2-sub-indo/",
    "all_episodes": "https://otakudesu.blog/anime/kusuriya-hitorigoto-s3-sub-indo/"
  }
}
```

## Sumber Kedua: Anibiplay

Modul `src/anibi.rs` menambahkan katalog **Anibiplay (Anibi Cloud / Kisame Cloud)** —
upstream `https://api.kisame.cloud` — sebagai sumber di samping OtakuDesu.

| Endpoint | Upstream | Status |
|---|---|---|
| `/api/anibi/search` | `POST /api/v2/search` | publik, 4.600+ judul (anime + donghua) |
| `/api/anibi/schedule` | `GET /api/v2/jadwal` | publik, jadwal mingguan + jam WIB |
| `/api/anibi/health` | `GET /api/v2/health` | publik |
| `/api/anibi/detail/{slug}` | `GET /api/v2/detail/{slug}` | **butuh API key** (tanpa key upstream balas 401) |

Keunggulan dibanding OtakuDesu: `rating`, `status`, `studio`, dan `episode_count`
sudah terstruktur dari upstream (tidak perlu parsing HTML, tidak ada field kosong).

Modul asli (Docs.txt) mengarang daftar episode dan link
`storages.animein.net/{judul}/{ep}-720p.mp4` yang kenyataannya 403/tidak ada.
Implementasi di sini **tidak** melakukan itu: kalau upstream tidak memberi data,
respons menyatakan tidak ada — bukan link palsu.

Konfigurasi (opsional, lewat env):

```bash
ANIBI_API_KEY=xxxx   # aktifkan /api/anibi/detail/{slug}
PORT=8686            # override port bind (default 8686)
```

## Sumber Ketiga: Samehadaku

Modul `src/samehadaku.rs` — port dari suite Node.js "Samehadaku (v2.samehadaku.how)"
tanpa ketergantungan Cloudflare-bypass.

| Endpoint | Halaman upstream | Isi |
|---|---|---|
| `/api/samehadaku/search` | `/?s=<q>` | judul, url, cover, tipe, status |
| `/api/samehadaku/latest` | `/`, `/page/N` | 40 rilis episode terbaru |
| `/api/samehadaku/catalog` | `/anime/?genre[]=&status=&type=&order=` | 50 judul/halaman |
| `/api/samehadaku/anime?url=` | `/anime/<slug>/` | info (status/type/episodes/released/studio), sinopsis, seluruh episode |
| `/api/samehadaku/episode?url=` | `/<slug>-episode-N/` | embed player, link download per kualitas, prev/next |

Status mirror (diverifikasi 12 Nov 2026):

| Mirror | Hasil |
|---|---|
| `samehadaku.li` | **200**, bisa di-scrape langsung → default |
| `v2.samehadaku.how` | 403 Cloudflare Turnstile → **butuh sidecar bypass** |
| `samehadaku.care` | 200 tapi halaman parkir |
| `samehadaku.email` / `.win` | tidak resolve |

### Dua tema, satu endpoint

Modul ini mendeteksi tema dari markup, jadi endpoint yang sama bekerja untuk keduanya:

| Fitur | `.li` (tema lama) | `.how` (tema baru) |
|---|---|---|
| kartu | `div.bsx` | `article.animepost`, `a.series`, feed `li .thumb` |
| detail | `.infox`, `.spe`, `.eplister` | `h1`, `span.w-28`, `a[href*=episode-]`, `og:image` |
| player | `#pembed iframe[data-litespeed-src]` | `.east_player_option[data-embed]` → 5 server |
| download | `.aspd-download` (1 URL, banyak kualitas) | `.download-eps` per format (MKV/MP4/x265) |

Hasil nyata di `.how`: 5 mirror (Blogspot, VIP Streaming, Mega 480p/720p/1080p)
dan 51 link download per episode.

### Sidecar bypass Cloudflare (`sidecar/`)

Chromium tidak dijalankan dari biner Rust; ada proses kecil Node yang jadi jembatan.

```bash
cd sidecar
npm install          # sekali saja (omnify-bypass + puppeteer-real-browser)
node bypass-server.js                     # 127.0.0.1:8795
curl 'http://127.0.0.1:8795/health'
```

Jalankan API dengan bypass aktif:

```bash
SAMEHADAKU_BASE_URL=https://v2.samehadaku.how \
SAMEHADAKU_BYPASS_URL=http://127.0.0.1:8795 \
./target/release/otakudesu-api-rust
```

Catatan teknis:

- Tier 1 omnify-bypass ("whitelist crawler" via UA WhatsApp/Googlebot) **tidak tembus**
  dari IP VPS ini — semua UA tetap 403. Yang bekerja Tier 2 (Chromium stealth).
- HTML di-cache 5 menit di sidecar (`BYPASS_TTL_MS`), request diserialkan satu per satu
  supaya tidak membuka banyak Chromium.
- Kalau sidecar mati/challenge gagal, `get_html()` otomatis jatuh ke HTTP langsung
  (berguna untuk `.li` yang tidak ber-Cloudflare).
- Filter `status=` di katalog **tidak berfungsi** di `.how` (server membalas halaman
  kosong); `order=`, `page=`, dan `genre[]` jalan.

Catatan: mirror `.li` hanya menyediakan player Blogger + tombol download (Gofile) —
tidak ada daftar server mirror maupun halaman batch/schedule seperti tema `.how`.
URL yang diambil lewat `?url=` divalidasi harus mengandung `samehadaku` (cegah SSRF).

## Thumbnail Proxy

Gunakan endpoint ini untuk mengambil thumbnail melalui API Rust:

```text
/api/image?url=https%3A%2F%2Fotakudesu.blog%2Fwp-content%2Fuploads%2F...
```

Endpoint hanya menerima URL yang berasal dari `otakudesu.blog` dan mengirim header cache:

```text
Cache-Control: public, max-age=86400
```

## Build Verification

```bash
cargo check
cargo build --release
```

## Struktur

```text
.
├── Cargo.toml
├── README.md
├── .gitignore
└── src
    └── main.rs
```

## Frontend

Frontend NanoStream menggunakan API ini melalui port `8686` dan dikelola sebagai project terpisah dari backend.

API mempertahankan kontrak endpoint yang digunakan frontend tersebut.

## Catatan

Project ini hanya mengakses dan memproses data yang tersedia pada sumber upstream. Pastikan penggunaan sesuai hukum, hak cipta, kebijakan upstream, dan peraturan wilayah masing-masing.

## License

MIT
