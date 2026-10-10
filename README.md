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
