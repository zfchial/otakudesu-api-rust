/**
 * Sidecar bypass Cloudflare untuk NanoStream API.
 *
 * Kenapa sidecar: mirror v2.samehadaku.how (dan beberapa situs lain) dilindungi
 * Cloudflare IUAM/Turnstile. Binary Rust tidak mungkin menjalankan Chromium dengan
 * nyaman, jadi proses kecil ini yang mengambil HTML-nya, lalu Rust cukup HTTP GET.
 *
 * Menjalankan:
 *   npm install            # sekali saja
 *   node bypass-server.js  # default 127.0.0.1:8795
 *
 * Endpoint:
 *   GET /health            -> {ok:true, cache_size, ...}
 *   GET /fetch?url=<url>   -> {ok, status, title, html, cached, ms}
 *   GET /cache/clear       -> kosongkan cache
 *
 * ENV:
 *   BYPASS_PORT    (default 8795)
 *   BYPASS_TTL_MS  (default 300000 = 5 menit)
 *   BYPASS_TIMEOUT (default 45000)
 */

const http = require('http');
const omnify = require('omnify-bypass');

const PORT = parseInt(process.env.BYPASS_PORT || '8795', 10);
const TTL_MS = parseInt(process.env.BYPASS_TTL_MS || '300000', 10);
const TIMEOUT_MS = parseInt(process.env.BYPASS_TIMEOUT || '45000', 10);
const CHALLENGE_RE = /just a moment|security verification|cf-challenge|checking your browser/i;

const cache = new Map(); // url -> { ts, payload }
let chain = Promise.resolve(); // serialisasi: satu browser, satu request sekaligus

function enqueue(fn) {
  const run = chain.then(fn, fn);
  chain = run.catch(() => {});
  return run;
}

function extractHtml(res) {
  if (res == null) return '';
  if (typeof res === 'string') return res;
  for (const k of ['data', 'body', 'html', 'content', 'text']) {
    const v = res[k];
    if (typeof v === 'string') return v;
    if (v && typeof v === 'object' && typeof v.data === 'string') return v.data;
  }
  try {
    return JSON.stringify(res);
  } catch {
    return '';
  }
}

async function fetchViaBypass(url) {
  const started = Date.now();
  const hit = cache.get(url);
  if (hit && Date.now() - hit.ts < TTL_MS) {
    return { ...hit.payload, cached: true, ms: Date.now() - started };
  }

  const res = await omnify.fetch(url, { timeout: TIMEOUT_MS });
  const html = extractHtml(res);
  const status = Number((res && (res.status ?? res.statusCode)) || 200);
  const title = (html.match(/<title[^>]*>([^<]*)/i) || [, ''])[1].trim();
  const passed = status >= 200 && status < 400 && !!html && !CHALLENGE_RE.test(html.slice(0, 4000));

  const payload = {
    ok: passed,
    status,
    title,
    bytes: html.length,
    challenge: !passed,
    html,
  };
  if (passed) cache.set(url, { ts: Date.now(), payload });
  return { ...payload, cached: false, ms: Date.now() - started };
}

function send(res, code, obj) {
  const body = JSON.stringify(obj);
  res.writeHead(code, {
    'content-type': 'application/json; charset=utf-8',
    'content-length': Buffer.byteLength(body),
  });
  res.end(body);
}

const server = http.createServer(async (req, res) => {
  const u = new URL(req.url, `http://127.0.0.1:${PORT}`);

  if (u.pathname === '/health') {
    return send(res, 200, {
      ok: true,
      service: 'omnify-bypass-sidecar',
      port: PORT,
      cache_size: cache.size,
      ttl_ms: TTL_MS,
    });
  }

  if (u.pathname === '/cache/clear') {
    const n = cache.size;
    cache.clear();
    return send(res, 200, { ok: true, cleared: n });
  }

  if (u.pathname === '/fetch') {
    const target = u.searchParams.get('url');
    if (!target || !/^https?:\/\//i.test(target)) {
      return send(res, 400, { ok: false, error: 'parameter url (http/https) wajib' });
    }
    try {
      const out = await enqueue(() => fetchViaBypass(target));
      return send(res, out.ok ? 200 : 502, out);
    } catch (e) {
      return send(res, 502, { ok: false, error: String((e && e.message) || e) });
    }
  }

  send(res, 404, { ok: false, error: 'endpoint tidak dikenal', paths: ['/health', '/fetch?url=', '/cache/clear'] });
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`[bypass-sidecar] listening on http://127.0.0.1:${PORT} (ttl ${TTL_MS}ms)`);
});

for (const sig of ['SIGINT', 'SIGTERM']) {
  process.on(sig, () => {
    console.log('[bypass-sidecar] shutting down');
    server.close(() => process.exit(0));
    try {
      if (typeof omnify.closeSharedBrowser === 'function') omnify.closeSharedBrowser();
    } catch {}
    setTimeout(() => process.exit(0), 1500).unref();
  });
}
