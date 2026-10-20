const app = document.getElementById("app");
const statusDot = document.getElementById("status-dot");
const statusText = document.getElementById("status-text");
const footerUpdated = document.getElementById("footer-updated");
let activeTab = "overview";
let state = { price: null, feed: [], cut: null, releases: [], sources: [], sot: null, history: null };
function formatKda(amt) {
  let str = String(amt);
  if (str.startsWith('{"decimal":')) {
    try {
      const parsed = JSON.parse(str);
      if (parsed.decimal) str = parsed.decimal;
    } catch (e) {}
  }
  const n = parseFloat(str.replace(/e/i, "E"));
  if (isNaN(n)) return str;
  if (n === 0) return "0";
  if (n < 0.000001) return n.toExponential(2) + " KDA";
  if (n < 0.001) return (n * 1e6).toFixed(2) + " µKDA";
  if (n < 1) return n.toFixed(6) + " KDA";
  return n.toFixed(4) + " KDA";
}

const esc = (s) => String(s ?? "").replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"})[c]);
const fmt = (n, d = 2) => n == null ? "—" : Number(n).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });
const fmtPrice = (n) => n == null ? "—" : "$" + Number(n).toFixed(6);
const timeAgo = (iso) => { if (!iso) return "—"; const s = (Date.now() - new Date(iso).getTime())/1000; if (s<60) return "just now"; if (s<3600) return Math.floor(s/60)+"m ago"; if (s<86400) return Math.floor(s/3600)+"h ago"; return Math.floor(s/86400)+"d ago"; };
async function jget(url) { const r = await fetch(url); if (!r.ok) throw new Error(url+" → "+r.status); return r.json(); }
async function poll() {
  try {
    const [price, feed, cut, overview, transfers, releases, sources, sot, activity, history] = await Promise.all([
      jget("/api/oracle/price/kda").catch(() => null),
      jget("/api/oracle/feed?limit=100").catch(() => []),
      jget("/api/oracle/chainweb/cut").catch(() => null),
      jget("/api/oracle/chainweb/overview").catch(() => null),
      jget("/api/oracle/chainweb/transfers").catch(() => ({ transfers: [] })),
      jget("/api/oracle/github/releases").catch(() => ({ releases: [] })),
      jget("/api/oracle/sources").catch(() => []),
      jget("/api/oracle/sot/registry").catch(() => null),
      jget("/api/oracle/activity").catch(() => null),
      jget("/api/oracle/price/kda/history?hours=168&source=coingecko").catch(() => null),
    ]);
    state = { price, feed: Array.isArray(feed) ? feed : [], cut, cw: overview, transfers: transfers?.transfers || [], releases: releases.releases || [], sources: Array.isArray(sources) ? sources : [], sot, activity, history };
    statusDot.className = "dot ok"; statusText.textContent = "online";
    footerUpdated.textContent = "updated " + new Date().toLocaleTimeString();
    render();
  } catch (e) {
    statusDot.className = "dot err"; statusText.textContent = "offline";
    app.innerHTML = `<div class="empty">Could not reach oracle · ${esc(e.message)}</div>`;
  }
}
function priceMap() { const out = {}; (state.price?.ticks || []).forEach(t => { out[t.source] = t; }); return out; }
function render() {
  if (activeTab === "overview") renderOverview();
  else if (activeTab === "activity") renderActivity();
  else if (activeTab === "prices") renderPrices();
  else if (activeTab === "feed") renderFeed();
  else if (activeTab === "chainweb") renderChainweb();
  else if (activeTab === "releases") renderReleases();
  else if (activeTab === "sources") renderSources();
  else if (activeTab === "sot") renderSot();
  else if (activeTab === "buysell") renderBuySell();
  else if (activeTab === "bridge") renderBridge();
  else if (activeTab === "wallets") renderWallets();
}
function sparkPath(points, w = 400, h = 80) {
  if (!points || points.length < 2) return "";
  const ys = points.map(p => p.p); const min = Math.min(...ys), max = Math.max(...ys), range = max - min || 1;
  return points.map((p, i) => { const x = (i/(points.length-1))*w; const y = h - ((p.p - min)/range)*h; return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`; }).join(" ");
}
function renderOverview() {
  const pm = priceMap();
  const primary = pm["coingecko"] || pm["coinmarketcap-kadena"] || Object.values(pm).find(t => parseFloat(t.price_usd) > 0);
  const price = primary ? parseFloat(primary.price_usd) : null;
  const change = primary ? parseFloat(primary.change_24h_pct || 0) : 0;
  const changeColor = change >= 0 ? "var(--green)" : "var(--red)";

  const isK = (f) => {
    const src = String(f.source_name || "").toLowerCase();
    if (src.includes("kadena") || src.includes("kda-chain")) return true;
    const text = ((f.title || "") + " " + (f.snippet || "")).toLowerCase();
    return /\bkadena\b|\bkda\b|\bpact\b/.test(text);
  };
  const allFeed = Array.isArray(state.feed) ? state.feed : [];
  const kadenaCount = allFeed.filter(isK).length;
  const otherCount = allFeed.length - kadenaCount;

  const act = state.activity || {};
  const chain = act.chain || {};
  const chainAlive = chain.alive === true;
  const cw = state.cw || {};
  const mainChains = (cw.heights || []).filter(h => h.network === "mainnet01");
  const maxH = mainChains.length ? Math.max(...mainChains.map(h => h.height)) : 0;
  const maxLag = mainChains.length ? Math.max(...mainChains.map(h => maxH - h.height)) : 0;

  const sources = Array.isArray(state.sources) ? state.sources : [];
  const enabled = sources.filter(s => s.enabled);
  const healthy = enabled.filter(s => !s.last_error).length;

  const gh = act.github || {};
  const commits30 = gh.commits_30d || [];
  const todayStr = new Date().toDateString();
  const commitsToday = commits30.filter(d => new Date(d.day).toDateString() === todayStr).reduce((s, d) => s + d.commits, 0);
  const commitsTotal = (gh.by_repo || []).reduce((s, r) => s + r.commits, 0);

  const social = act.social || {};
  const social30 = social.posts_30d || [];
  const postsToday = social30.filter(d => new Date(d.day).toDateString() === todayStr).reduce((s, d) => s + d.posts, 0);
  const totalPosts = social30.reduce((s, d) => s + d.posts, 0);
  const totalSent = social30.reduce((s, d) => s + d.sentiment * d.posts, 0);
  const avgSent = totalPosts > 0 ? totalSent / totalPosts : 0;
  const sentColor = avgSent > 0.2 ? "var(--green)" : avgSent < -0.2 ? "var(--red)" : "var(--seal-gold)";
  const sentLabel = avgSent > 0.2 ? "Bullish" : avgSent < -0.2 ? "Bearish" : "Neutral";

  const points = state.history?.points || [];
  const sparkPath = (() => {
    if (points.length < 2) return "";
    const ys = points.map(p => p.p);
    const mn = Math.min(...ys), mx = Math.max(...ys), rng = mx - mn || 1;
    return points.map((p, i) => {
      const x = (i / (points.length - 1)) * 400;
      const y = 60 - ((p.p - mn) / rng) * 60;
      return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
    }).join(" ");
  })();

  const priceRows = Object.values(pm)
    .filter(t => parseFloat(t.price_usd) > 0)
    .map(t => {
      const c = parseFloat(t.change_24h_pct || 0);
      const cc = c >= 0 ? "var(--green)" : "var(--red)";
      return `<tr>
        <td>${t.source}</td>
        <td class="num">$${parseFloat(t.price_usd).toFixed(6)}</td>
        <td class="num" style="color:${cc}">${c >= 0 ? "+" : ""}${c.toFixed(2)}%</td>
        <td class="num">${t.market_cap_usd ? "$" + Math.round(parseFloat(t.market_cap_usd)).toLocaleString() : "—"}</td>
        <td class="num">${timeAgo(t.fetched_at)}</td>
      </tr>`;
    }).join("") || '<tr><td colspan="5" class="empty">no sources reporting yet</td></tr>';

  const topFeed = [...allFeed].sort((a, b) => {
    const aK = isK(a) ? 1 : 0, bK = isK(b) ? 1 : 0;
    if (aK !== bK) return bK - aK;
    return new Date(b.published_at) - new Date(a.published_at);
  }).slice(0, 6);

  const feedItems = topFeed.map(f => `
    <li class="feed-item">
      <div style="flex:1;min-width:0">
        <a href="${f.url || "#"}" target="_blank" rel="noreferrer" class="feed-item-title">${f.title}</a>
        <div class="feed-item-meta">
          <span class="badge ${isK(f) ? "teal" : "gold"}">${isK(f) ? "KADENA" : "CRYPTO"}</span>
          <span>${f.source_name}</span>
          <span>·</span>
          <span>${timeAgo(f.published_at)}</span>
        </div>
      </div>
    </li>`).join("") || '<li class="empty">no items yet</li>';

  const chainCells = [...mainChains].sort((a, b) => a.chain_id - b.chain_id).map(c => {
    const lag = maxH - c.height;
    const color = lag > 2 ? "var(--red)" : lag > 0 ? "var(--seal-gold)" : "var(--seal-teal)";
    return `<div class="chain-cell">
      <div class="chain-cell-label">Chain ${c.chain_id}</div>
      <div class="chain-cell-value" style="color:${color}">${c.height.toLocaleString()}</div>
      <div class="chain-cell-hash">${lag === 0 ? "tip" : "-" + lag}</div>
    </div>`;
  }).join("");

  const livePriceSources = Object.values(pm).filter(t => parseFloat(t.price_usd) > 0).length;

  app.innerHTML = `
    <div class="grid-4" style="margin-bottom:16px">
      <div class="stat">
        <div class="stat-label">KDA Price</div>
        <div class="stat-value">${price != null ? "$" + price.toFixed(6) : "—"}</div>
        <div class="stat-sub" style="color:${changeColor}">${change >= 0 ? "▲ +" : "▼ "}${Math.abs(change).toFixed(2)}% 24h</div>
      </div>
      <div class="stat">
        <div class="stat-label">Market Cap</div>
        <div class="stat-value">${primary?.market_cap_usd ? "$" + Math.round(parseFloat(primary.market_cap_usd)).toLocaleString() : "—"}</div>
        <div class="stat-sub">${primary?.source || "—"}</div>
      </div>
      <div class="stat" style="border-left:3px solid ${chainAlive ? "var(--green)" : "var(--red)"}">
        <div class="stat-label">Chain</div>
        <div class="stat-value" style="color:${chainAlive ? "var(--green)" : "var(--red)"};font-size:20px">${chainAlive ? "ALIVE" : "STALE"}</div>
        <div class="stat-sub">${chain.last_block_at ? "last block " + timeAgo(chain.last_block_at) : "—"}</div>
      </div>
      <div class="stat">
        <div class="stat-label">Feed</div>
        <div class="stat-value">${kadenaCount}<span style="font-size:14px;color:var(--khub-muted)"> / ${otherCount}</span></div>
        <div class="stat-sub">Kadena / other crypto</div>
      </div>
    </div>

    <div class="grid-2" style="margin-bottom:16px">
      <div class="card">
        <h2><span class="icon">◈</span> KDA — last 7 days</h2>
        ${sparkPath ? `<svg viewBox="0 0 400 60" preserveAspectRatio="none" class="sparkline"><path d="${sparkPath}"/></svg>` : '<div class="empty">Collecting price history…</div>'}
        <p style="font-size:11px;color:var(--khub-muted);margin-top:6px">${points.length} hourly point${points.length === 1 ? "" : "s"} · source: coingecko</p>
      </div>
      <div class="card">
        <h2><span class="icon">◆</span> Activity digest</h2>
        <div style="display:grid;grid-template-columns:1fr 1fr;gap:12px;margin-top:8px">
          <div>
            <div style="font-size:10px;color:var(--khub-muted);text-transform:uppercase;letter-spacing:0.05em">Commits today</div>
            <div style="font-size:22px;font-weight:700;color:var(--khub-text);font-family:monospace">${commitsToday}</div>
            <div style="font-size:10px;color:var(--khub-muted)">${commitsTotal} in DB</div>
          </div>
          <div>
            <div style="font-size:10px;color:var(--khub-muted);text-transform:uppercase;letter-spacing:0.05em">Posts today</div>
            <div style="font-size:22px;font-weight:700;color:var(--khub-text);font-family:monospace">${postsToday}</div>
            <div style="font-size:10px;color:var(--khub-muted)">${sentLabel} <span style="color:${sentColor}">${avgSent >= 0 ? "+" : ""}${avgSent.toFixed(2)}</span></div>
          </div>
          <div>
            <div style="font-size:10px;color:var(--khub-muted);text-transform:uppercase;letter-spacing:0.05em">Pollers</div>
            <div style="font-size:22px;font-weight:700;color:${healthy === enabled.length ? "var(--green)" : "var(--seal-gold)"};font-family:monospace">${healthy}/${enabled.length}</div>
            <div style="font-size:10px;color:var(--khub-muted)">${healthy === enabled.length ? "all healthy" : "some errors"}</div>
          </div>
          <div>
            <div style="font-size:10px;color:var(--khub-muted);text-transform:uppercase;letter-spacing:0.05em">Price sources</div>
            <div style="font-size:22px;font-weight:700;color:var(--khub-text);font-family:monospace">${livePriceSources}</div>
            <div style="font-size:10px;color:var(--khub-muted)">reporting live</div>
          </div>
        </div>
      </div>
    </div>

    <div class="grid-2" style="margin-bottom:16px">
      <div class="card">
        <h2><span class="icon">$</span> Price sources</h2>
        <table class="tbl">
          <thead><tr><th>Source</th><th style="text-align:right">Price</th><th style="text-align:right">24h</th><th style="text-align:right">Mkt Cap</th><th style="text-align:right">Updated</th></tr></thead>
          <tbody>${priceRows}</tbody>
        </table>
      </div>
      <div class="card">
        <h2><span class="icon">◈</span> Latest feed</h2>
        <ul class="feed-list">${feedItems}</ul>
      </div>
    </div>

    ${mainChains.length ? `
    <div class="card">
      <h2><span class="icon">⬢</span> Chainweb mainnet — chain heights</h2>
      <div class="chains">${chainCells}</div>
      <p style="font-size:11px;color:var(--khub-muted);margin-top:8px">${mainChains.length} chains · max lag ${maxLag} block(s)</p>
    </div>` : ""}
  `;
}


function renderPrices() {
  const pm = priceMap();
  app.innerHTML = `<div class="card"><h2><span class="icon">$</span> Price Sources — Live</h2><table class="tbl"><thead><tr><th>Source</th><th style="text-align:right">Price (USD)</th><th style="text-align:right">Market Cap</th><th style="text-align:right">Volume 24h</th><th style="text-align:right">24h Change</th><th style="text-align:right">Updated</th></tr></thead><tbody>
    ${Object.values(pm).map(t => `<tr><td>${esc(t.source)}</td><td class="num">${fmtPrice(t.price_usd)}</td><td class="num">${t.market_cap_usd != null ? "$" + fmt(t.market_cap_usd, 0) : "—"}</td><td class="num">${t.volume_24h_usd != null ? "$" + fmt(t.volume_24h_usd, 0) : "—"}</td><td class="num ${Number(t.change_24h_pct) >= 0 ? "stat-up" : "stat-down"}">${t.change_24h_pct != null ? (Number(t.change_24h_pct) >= 0 ? "+" : "") + Number(t.change_24h_pct).toFixed(2) + "%" : "—"}</td><td class="num">${timeAgo(t.fetched_at)}</td></tr>`).join("") || "<tr><td colspan=6 class='empty'>no prices collected yet</td></tr>"}
  </tbody></table></div>`;
}
function isKadenaItem(f) {
  const src = String(f.source_name || "").toLowerCase();
  if (src.includes("kadena") || src.includes("kda-chain")) return true;
  const text = ((f.title || "") + " " + (f.snippet || "") + " " + (f.body || "")).toLowerCase();
  return /\bkadena\b|\bkda\b|\bpact\b/.test(text);
}

function renderFeed() {
  const all = Array.isArray(state.feed) ? state.feed : [];
  const kadena = all.filter(isKadenaItem);
  const other = all.filter(f => !isKadenaItem(f));
  const filter = window._feedFilter || "kadena";

  const tabs = [
    { id: "kadena", label: "Kadena", count: kadena.length },
    { id: "other", label: "Other crypto", count: other.length },
    { id: "all", label: "All", count: all.length },
  ];
  const items = filter === "kadena" ? kadena : filter === "other" ? other : all;

  app.innerHTML = `
    <div class="card" style="margin-bottom:16px">
      <h2><span class="icon">◈</span> Feed</h2>
      <p style="font-size:12px;color:var(--khub-muted);margin-top:6px">
        Kadena ecosystem items are separated from general crypto news. Kadena = items from Kadena sources or mentioning KDA / Pact.
      </p>
      <div style="display:flex;gap:4px;margin-top:12px;flex-wrap:wrap">
        ${tabs.map(t => `<button class="feed-filter" data-filter="${t.id}" style="padding:6px 12px;font-size:11px;font-weight:700;border-radius:5px;border:1px solid ${filter===t.id?'var(--seal-teal)':'var(--khub-border)'};background:${filter===t.id?'rgba(88,166,166,0.12)':'transparent'};color:${filter===t.id?'var(--seal-teal)':'var(--khub-muted)'};cursor:pointer;font-family:inherit">${t.label} <span style="opacity:0.6;margin-left:4px">${t.count}</span></button>`).join("")}
      </div>
    </div>
    <div class="card">
      <ul class="feed-list">
        ${items.length ? items.map(f => `
          <li class="feed-item">
            <div style="flex:1;min-width:0">
              <a href="${esc(f.url || "#")}" target="_blank" rel="noreferrer" class="feed-item-title">${esc(f.title || "(no title)")}</a>
              <div class="feed-item-meta">
                <span class="badge teal">${esc(f.source_kind || "")}</span>
                <span>${esc(f.source_name || "")}</span>
                <span>·</span>
                <span>${esc(f.category || "news")}</span>
                <span>·</span>
                <span>${timeAgo(f.published_at)}</span>
              </div>
            </div>
          </li>`).join("") : '<li class="empty">No items in this view</li>'}
      </ul>
    </div>
  `;

  document.querySelectorAll(".feed-filter").forEach(btn => {
    btn.onclick = () => {
      window._feedFilter = btn.dataset.filter;
      renderFeed();
    };
  });
}


function sparkPath(points, key, w, h) {
  if (!points || points.length < 2) return "";
  const ys = points.map(p => p[key]);
  const min = Math.min(...ys), max = Math.max(...ys), range = max - min || 1;
  return points.map((p, i) => {
    const x = (i / (points.length - 1)) * w;
    const y = h - ((p[key] - min) / range) * h;
    return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(" ");
}

function renderChainweb() {
  const o = state.cw || {};
  const info = o.info || {};
  const heights = o.heights || [];
  const rate = o.rate || [];
  const perChain = o.per_chain_rate || [];
  const health = o.health || [];
  const nodes = o.nodes || [];
  const nodeFails = o.node_failures || [];
  const txs = o.txs || [];
  const txRecent = o.tx_recent || [];
  const recentBlocks = o.recent_blocks || [];

  const sorted = [...heights].sort((a, b) => a.chain_id - b.chain_id);
  const maxH = sorted.length ? Math.max(...sorted.map(h => h.height)) : 0;
  const maxLag = sorted.length ? Math.max(...sorted.map(h => maxH - h.height)) : 0;
  const aliveChains = sorted.filter(c => (maxH - c.height) <= 2).length;
  const stalledChains = sorted.filter(c => {
    const e = perChain.find(p => p.chain_id === c.chain_id);
    return !e || e.blocks === 0;
  }).length;

  const latestHealth = health.length ? health[health.length - 1] : null;
  const avgBlocks = rate.length ? Math.round(rate.reduce((a, b) => a + b.blocks, 0) / rate.length) : 0;
  const avgTx = txs.length ? Math.round(txs.reduce((a, b) => a + b.tx, 0) / txs.length) : 0;

  const bestNode = nodes.length ? nodes.reduce((a, b) => (a.latency_ms < b.latency_ms ? a : b)) : null;
  const worstNode = nodes.length ? nodes.reduce((a, b) => (a.latency_ms > b.latency_ms ? a : b)) : null;
  const downNodes = nodes.filter(n => !n.ok).length;

  const ratePath = (() => {
    if (rate.length < 2) return "";
    const ys = rate.map(p => p.blocks);
    const mn = Math.min(...ys), mx = Math.max(...ys), rng = mx - mn || 1;
    return rate.map((p, i) => {
      const x = (i / (rate.length - 1)) * 400;
      const y = 60 - ((p.blocks - mn) / rng) * 60;
      return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
    }).join(" ");
  })();

  const healthPath = (() => {
    if (health.length < 2) return "";
    const ys = health.map(p => p.avg_ms);
    const mn = Math.min(...ys), mx = Math.max(...ys), rng = mx - mn || 1;
    return health.map((p, i) => {
      const x = (i / (health.length - 1)) * 400;
      const y = 60 - ((p.avg_ms - mn) / rng) * 60;
      return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
    }).join(" ");
  })();

  const txPath = (() => {
    if (txs.length < 2) return "";
    const ys = txs.map(p => p.tx);
    const mn = Math.min(...ys), mx = Math.max(...ys), rng = mx - mn || 1;
    return txs.map((p, i) => {
      const x = (i / (txs.length - 1)) * 400;
      const y = 60 - ((p.tx - mn) / rng) * 60;
      return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
    }).join(" ");
  })();

  // Merkle braid
  const graphArr = info.graph || [];
  const latestGraph = graphArr.length ? graphArr[graphArr.length - 1] : null;
  const adjacency = latestGraph ? latestGraph[1] : [];
  const braidSvg = (() => {
    if (!adjacency.length) return "";
    const n = adjacency.length, cx = 160, cy = 160, r = 120;
    const pos = {};
    adjacency.forEach(([id], i) => {
      const a = (i / n) * 2 * Math.PI - Math.PI / 2;
      pos[id] = [cx + r * Math.cos(a), cy + r * Math.sin(a)];
    });
    let edges = "";
    adjacency.forEach(([id, nb]) => nb.forEach(x => {
      if (pos[x] && pos[id]) edges += `<line x1="${pos[id][0]}" y1="${pos[id][1]}" x2="${pos[x][0]}" y2="${pos[x][1]}" stroke="#30363D" stroke-width="0.9"/>`;
    }));
    let nodes = "";
    adjacency.forEach(([id]) => {
      if (pos[id]) nodes += `<circle cx="${pos[id][0]}" cy="${pos[id][1]}" r="13" fill="#161B22" stroke="#58A6A6" stroke-width="1.5"/>
        <text x="${pos[id][0]}" y="${pos[id][1] + 4}" text-anchor="middle" font-size="10" fill="#C9D1D9" font-family="monospace">${id}</text>`;
    });
    return `<svg viewBox="0 0 320 320" style="width:100%;max-width:380px;height:auto">${edges}${nodes}</svg>`;
  })();

  // Per-chain rate bar (top 20 by chain_id for consistency with grid)
  const rateByChain = {};
  perChain.forEach(r => rateByChain[r.chain_id] = r.blocks);
  const maxPerChain = Math.max(1, ...perChain.map(r => r.blocks));

  // Node stability lookup
  const failsByNode = {};
  nodeFails.forEach(n => failsByNode[n.endpoint] = n);

  app.innerHTML = `
    <div class="card" style="margin-bottom:16px;border-left:3px solid var(--seal-teal)">
      <h2><span class="icon">⬢</span> Chainweb — live network state</h2>
      <p style="font-size:12px;color:var(--khub-muted);margin-top:6px">
        Every panel updates every 5s. Data from ${info.package_version ? `chainweb ${info.package_version}` : "node"} via ${(nodes[0]?.endpoint || "regional endpoints").replace("https://","")}.
      </p>
    </div>

    <div class="grid-4" style="margin-bottom:16px">
      <div class="stat"><div class="stat-label">Chains alive</div><div class="stat-value" style="color:${aliveChains === 20 ? "var(--green)" : aliveChains >= 18 ? "var(--seal-gold)" : "var(--red)"}">${aliveChains}/${sorted.length || 20}</div><div class="stat-sub">${stalledChains} stalled · ${maxLag} max lag</div></div>
      <div class="stat"><div class="stat-label">Blocks / min</div><div class="stat-value">${avgBlocks}</div><div class="stat-sub">last 15-min avg</div></div>
      <div class="stat"><div class="stat-label">Best node</div><div class="stat-value" style="font-size:15px">${bestNode ? bestNode.latency_ms + "ms" : "—"}</div><div class="stat-sub">${bestNode ? bestNode.endpoint.replace("https://","").split(".")[0] : "waiting"}</div></div>
      <div class="stat"><div class="stat-label">Nodes up</div><div class="stat-value" style="color:${downNodes === 0 ? "var(--green)" : "var(--red)"}">${nodes.length - downNodes}/${nodes.length || 0}</div><div class="stat-sub">${downNodes} down</div></div>
    </div>

    <div class="grid-2" style="margin-bottom:16px">
      <div class="card">
        <h2><span class="icon">◈</span> Block production — last 15 min</h2>
        ${ratePath ? `<svg viewBox="0 0 400 60" preserveAspectRatio="none" class="sparkline"><path d="${ratePath}"/></svg>` : '<div class="empty">Collecting…</div>'}
        <p style="font-size:11px;color:var(--khub-muted);margin-top:6px">Blocks per minute, aggregated across all chains</p>
      </div>
      <div class="card">
        <h2><span class="icon">◆</span> Node latency — last 10 min</h2>
        ${healthPath ? `<svg viewBox="0 0 400 60" preserveAspectRatio="none" class="sparkline"><path d="${healthPath}" style="stroke:var(--seal-gold)"/></svg>` : '<div class="empty">Collecting…</div>'}
        <p style="font-size:11px;color:var(--khub-muted);margin-top:6px">${latestHealth ? "avg " + Math.round(latestHealth.avg_ms) + "ms" : "—"} · ${downNodes} node${downNodes === 1 ? "" : "s"} down</p>
      </div>
    </div>

    <div class="grid-2" style="margin-bottom:16px">
      <div class="card">
        <h2><span class="icon">⬢</span> Regional nodes</h2>
        <div style="display:flex;flex-direction:column;gap:6px;margin-top:8px">
          ${nodes.map(n => {
            const fails = failsByNode[n.endpoint] || { fails: 0, total: 0 };
            const failPct = fails.total > 0 ? Math.round((fails.fails / fails.total) * 100) : 0;
            const host = (n.endpoint || "").replace("https://","");
            const color = n.ok ? "var(--green)" : "var(--red)";
            return `<div style="display:flex;align-items:center;gap:10px;padding:8px 10px;background:var(--khub-bg);border:1px solid var(--khub-border);border-radius:6px">
              <span style="width:8px;border-radius:50%;background:${color};box-shadow:0 0 6px ${color};flex-shrink:0"></span>
              <div style="flex:1;min-width:0">
                <div style="font-family:monospace;font-size:11px;color:var(--khub-text);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${host}</div>
                <div style="font-size:10px;color:var(--khub-muted)">${n.ok ? "● " + n.latency_ms + "ms" : "● HTTP " + n.http_status}${fails.total > 0 ? " · " + failPct + "% fail" : ""}</div>
              </div>
              <span style="font-size:10px;color:var(--khub-muted);font-family:monospace;text-align:right;width:60px">${timeAgo(n.fetched_at)}</span>
            </div>`;
          }).join("") || '<div class="empty">No node data yet — oracle probes every 60s</div>'}
        </div>
      </div>
      <div class="card">
        <h2><span class="icon">⬢</span> Chain braid — merkle adjacency</h2>
        <div style="display:flex;justify-content:center;padding:8px">
          ${braidSvg || '<div class="empty">Waiting for /info snapshot</div>'}
        </div>
        <p style="font-size:11px;color:var(--khub-muted);margin-top:6px">${adjacency.length || 20} chains, degree-3 graph. Each block includes neighbour merkle roots.</p>
      </div>
    </div>

    <div class="grid-2" style="margin-bottom:16px">
      <div class="card">
        <h2><span class="icon">⇄</span> Tx activity — last 30 min</h2>
        ${txPath ? `<svg viewBox="0 0 400 60" preserveAspectRatio="none" class="sparkline"><path d="${txPath}" style="stroke:var(--seal-violet)"/></svg>` : '<div class="empty">Sampling block payloads…</div>'}
        <p style="font-size:11px;color:var(--khub-muted);margin-top:6px">Sampled tx count · avg ${avgTx} tx/min</p>
        <table class="tbl" style="margin-top:12px;font-size:11px">
          <thead><tr><th>Chain</th><th style="text-align:right">Height</th><th style="text-align:right">Tx</th><th style="text-align:right">Sampled</th></tr></thead>
          <tbody>
            ${txRecent.length ? txRecent.map(t => `<tr><td>Chain ${t.chain_id}</td><td class="num">${t.height.toLocaleString()}</td><td class="num" style="color:var(--seal-violet);font-weight:700">${t.tx_count}</td><td class="num" style="color:var(--khub-muted);font-size:10px">${timeAgo(t.fetched_at)}</td></tr>`).join("") : '<tr><td colspan="4" class="empty">no samples yet</td></tr>'}
          </tbody>
        </table>
      </div>
      <div class="card" style="margin-bottom:16px">
        <h2><span class="icon">⇄</span> Latest transfers <span style="margin-left:auto;font-size:10px;color:var(--khub-muted)">${(state.transfers || []).length} in DB</span></h2>
        <div style="max-height: 400px; overflow-y: auto;">
          ${(state.transfers || []).length ? (state.transfers || []).slice(0, 25).map(t => {
            const age = (Date.now() - new Date(t.fetched_at).getTime()) / 1000;
            const sh = (x) => !x ? "—" : x.length > 15 ? x.slice(0,8)+"…"+x.slice(-4) : x;
            const fresh = age < 15;
            return `<div style="display:flex;align-items:center;gap:8px;padding:6px 8px;border-bottom:1px solid var(--khub-border);font-size:11px;font-family:monospace;${fresh?'background:rgba(88,166,166,0.05)':''}">
              <span style="color:var(--khub-muted);width:45px">C${t.chain_id}</span>
              <span style="color:var(--seal-teal);flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="${t.sender}">${sh(t.sender)}</span>
              <span style="color:var(--khub-muted)">→</span>
              <span style="color:var(--seal-violet);flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="${t.receiver}">${sh(t.receiver)}</span>
              <span style="text-align:center;width:50px;font-size:9px">${t.sender === "kadena-xchain-gas" || t.sender.startsWith("w:") ? '<span style="color:var(--khub-muted)">gas</span>' : '<span style="color:var(--green);font-weight:700">user</span>'}</span>
            <span style="color:var(--seal-gold);text-align:right;width:110px;font-weight:700">${formatKda(t.amount)}</span>
              <span style="color:var(--khub-muted);text-align:right;width:50px">${t.module}</span>
              <span style="color:var(--khub-muted);text-align:right;width:60px;font-size:10px">${fresh?'● new':Math.round(age)+'s'}</span>
            </div>`;
          }).join('') : '<div class="empty" style="padding:20px">No transfers yet — will appear here as blocks with txs are sampled</div>'}
        </div>
      </div>

      <div class="card">
        <h2><span class="icon">⬢</span> Recent blocks</h2>
        <table class="tbl" style="font-size:11px">
          <thead><tr><th>Chain</th><th style="text-align:right">Height</th><th>Hash</th><th style="text-align:right">Age</th></tr></thead>
          <tbody>
            ${recentBlocks.length ? recentBlocks.slice(0, 15).map(b => `<tr>
              <td style="font-family:monospace">${b.chain_id}</td>
              <td class="num" style="font-weight:700">${b.height.toLocaleString()}</td>
              <td style="font-family:monospace;color:var(--seal-teal);font-size:10px">${b.hash.slice(0, 16)}…</td>
              <td class="num" style="color:var(--khub-muted);font-size:10px">${timeAgo(b.fetched_at)}</td>
            </tr>`).join("") : '<tr><td colspan="4" class="empty">waiting for first cut</td></tr>'}
          </tbody>
        </table>
      </div>
    </div>

    <div class="card" style="margin-bottom:16px">
      <h2><span class="icon">⬢</span> Chain heights + rate — mainnet01</h2>
      <div style="display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:8px;margin-top:12px">
        ${sorted.map(c => {
          const lag = maxH - c.height;
          const color = lag > 2 ? "var(--red)" : lag > 0 ? "var(--seal-gold)" : "var(--seal-teal)";
          const blocks = rateByChain[c.chain_id] || 0;
          const pct = (blocks / maxPerChain) * 100;
          return `<div class="chain-cell">
            <div class="chain-cell-label">Chain ${c.chain_id}</div>
            <div class="chain-cell-value" style="color:${color}">${c.height.toLocaleString()}</div>
            <div style="font-size:9px;color:var(--khub-muted);margin-top:2px">${lag === 0 ? "tip" : "-" + lag + " behind"}</div>
            <div style="background:var(--khub-border);border-radius:2px;margin-top:6px;overflow:hidden">
              <div style="width:${pct}%;height:100%;background:var(--seal-teal)"></div>
            </div>
            <div style="font-size:9px;color:var(--khub-muted);margin-top:2px">${blocks} blocks/10m</div>
          </div>`;
        }).join("") || '<div class="empty">waiting for first cut</div>'}
      </div>
      <p style="font-size:11px;color:var(--khub-muted);margin-top:12px">Max lag: <strong style="color:${maxLag > 2 ? "var(--red)" : "var(--seal-teal)"}">${maxLag}</strong> block(s) · ${stalledChains} chain(s) with zero blocks in last 10m</p>
    </div>

    <div class="card">
      <h2><span class="icon">◆</span> Per-chain block rate — top 10 fastest</h2>
      <div style="margin-top:8px">
        ${perChain.slice(0, 10).map(r => {
          const pct = (r.blocks / maxPerChain) * 100;
          return `<div style="display:flex;align-items:center;gap:8px;margin-bottom:6px">
            <span style="font-size:11px;font-family:monospace;width:70px;color:var(--khub-text)">Chain ${r.chain_id}</span>
            <div style="flex:1;background:var(--khub-bg);border-radius:4px;overflow:hidden">
              <div style="width:${pct}%;height:100%;background:var(--seal-teal)"></div>
            </div>
            <span style="font-size:11px;font-weight:700;color:var(--khub-text);width:60px;text-align:right">${r.blocks} blk</span>
            <span style="font-size:10px;color:var(--khub-muted);width:60px;text-align:right">${r.samples} samples</span>
          </div>`;
        }).join("") || '<div class="empty">no block-rate data yet</div>'}
      </div>
    </div>
  `;
}


function renderReleases() {
  const rels = state.releases || [];
  const repos = [...new Set(rels.map(r => r.org + "/" + r.repo))];
  app.innerHTML = `
    <div class="card" style="margin-bottom:16px">
      <h2><span class="icon">◆</span> Releases — kda-community</h2>
      <p style="font-size:12px;color:var(--khub-muted);margin-top:6px">
        Latest releases from the canonical KDA Community org. Tracking: ${repos.length ? repos.map(r => '<a href="https://github.com/' + r + '" target="_blank" rel="noreferrer" style="color:var(--seal-teal)">' + r + '</a>').join(' · ') : 'chainweb-node · pact-5 · kda-tool'}.
      </p>
    </div>
    <div style="display:flex;flex-direction:column;gap:10px">
      ${rels.length === 0 ? '<div class="card"><div class="empty">No releases collected yet — first poll runs immediately after restart.</div></div>' : rels.map(r => `
        <a href="${r.url}" target="_blank" rel="noreferrer" class="card" style="text-decoration:none;color:inherit;display:block;border-left:3px solid ${r.prerelease ? 'var(--seal-gold)' : 'var(--seal-teal)'}">
          <div style="display:flex;justify-content:space-between;align-items:flex-start;gap:12px;flex-wrap:wrap">
            <div style="min-width:0;flex:1">
              <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-bottom:6px">
                <span class="badge teal">${r.org}/${r.repo}</span>
                <span class="badge violet">${r.tag}</span>
                ${r.prerelease ? '<span class="badge gold">PRE-RELEASE</span>' : ''}
                <span style="font-size:11px;color:var(--khub-muted)">${r.assets || 0} asset${(r.assets||0) === 1 ? '' : 's'}</span>
              </div>
              <div style="font-weight:700;font-size:15px;color:var(--khub-text);margin-bottom:4px">${r.name}</div>
              ${r.body_excerpt ? '<p style="font-size:11px;color:var(--khub-muted);line-height:1.5;white-space:pre-wrap;max-height:4.5em;overflow:hidden">' + r.body_excerpt + '</p>' : ''}
            </div>
            <div style="text-align:right;flex-shrink:0">
              <div style="font-size:11px;color:var(--khub-muted)">${timeAgo(r.published_at)}</div>
              ${r.author ? '<div style="font-size:10px;color:var(--khub-muted);opacity:0.7">@' + r.author + '</div>' : ''}
              <div style="font-size:11px;color:var(--seal-teal);margin-top:4px">↗</div>
            </div>
          </div>
        </a>`).join('')}
    </div>
  `;
}


function renderSources() {
  app.innerHTML = `<div class="card"><h2><span class="icon">◆</span> Poller Health</h2><table class="tbl"><thead><tr><th>Source</th><th>Kind</th><th>Enabled</th><th style="text-align:right">Last Polled</th><th>Last Error</th></tr></thead><tbody>
    ${state.sources.map(s => `<tr><td><span class="mono">${esc(s.name)}</span></td><td><span class="badge teal">${esc(s.kind)}</span></td><td>${s.enabled ? '<span class="badge green">on</span>' : '<span class="badge red">off</span>'}</td><td class="num">${timeAgo(s.last_polled_at)}</td><td style="font-size:11px;color:var(--red);max-width:300px;overflow:hidden;text-overflow:ellipsis">${s.last_error ? esc(s.last_error) : '<span style="color:var(--green)">ok</span>'}</td></tr>`).join("") || "<tr><td colspan=5 class='empty'>no sources</td></tr>"}
  </tbody></table></div>`;
}
function shortU(u) { return String(u || "").replace(/^https?:\/\//, "").replace(/\/$/, ""); }

function renderSot() {
  const sot = state.sot || {};
  const canonical = Array.isArray(sot.canonical) ? sot.canonical : [];
  const deprecated = Array.isArray(sot.deprecated) ? sot.deprecated : [];

  app.innerHTML = `
    <div class="card" style="margin-bottom:16px">
      <h2><span class="icon">◆</span> SOT Registry</h2>
      <p style="font-size:12px;color:var(--khub-muted);margin-bottom:8px">Source: <a href="${sot.source_url || "https://github.com/AI-KDA-Community/releases/tree/main/sot"}" target="_blank" rel="noreferrer" style="color:var(--seal-teal)">${sot.source_url || "AI-KDA-Community/releases/sot"}</a></p>
    </div>
    <div class="sot-grid">
      <div class="card">
        <h2><span class="icon" style="color:var(--seal-teal)">●</span> Canonical</h2>
        <ul class="sot-list">
          ${canonical.length ? canonical.map(c => `<li><span>${c.Label || c.Kind || ""}</span><span class="sot-good">${shortU(c.URL)}</span></li>`).join("") : `<li class="empty">none</li>`}
        </ul>
      </div>
      <div class="card">
        <h2><span class="icon" style="color:var(--red)">●</span> Deprecated</h2>
        <ul class="sot-list">
          ${deprecated.length ? deprecated.map(d => `<li><span>${d.Category || ""}</span><span class="sot-bad">${shortU(d.URL)}${d.Successor ? " → " + shortU(d.Successor) : ""}</span></li>`).join("") : `<li class="empty">none</li>`}
        </ul>
      </div>
    </div>
  `;
}


function renderBuySell() {
  const venues = [
    {name:"Gate.io", type:"CEX", desc:"Leading global exchange offering deep liquidity for KDA and advanced trading tools.", url:"https://www.gate.com/en-uk", active:true},
    {name:"AnonKyc", type:"CEX", desc:"Privacy-first global exchange offering fast, secure trading without mandatory KYC.", url:"https://anonkyc.com/", active:true},
    {name:"Binance", type:"CEX", desc:"KDA delisted; retained.", url:"https://www.binance.com", active:false},
    {name:"Bybit", type:"CEX", desc:"KDA delisted; retained.", url:"https://www.bybit.com", active:false},
    {name:"OKX", type:"CEX", desc:"KDA delisted; retained.", url:"https://www.okx.com", active:false},
    {name:"KuCoin", type:"CEX", desc:"Delisted Nov 2025; retained.", url:"https://www.kucoin.com", active:false},
    {name:"Binance.US", type:"CEX", desc:"KDA delisted; retained.", url:"https://www.binance.us", active:false},
    {name:"Pionex", type:"CEX", desc:"Delisted Nov 2025; retained.", url:"https://www.pionex.com", active:false},
    {name:"HitBTC", type:"CEX", desc:"Zero KDA volume; retained.", url:"https://hitbtc.com", active:false},
    {name:"XT.com", type:"CEX", desc:"Not on current ecosystem list.", url:"https://www.xt.com", active:false},
    {name:"MEXC", type:"CEX", desc:"Not on current ecosystem list.", url:"https://www.mexc.com", active:false},
    {name:"Bitget", type:"CEX", desc:"Not on current ecosystem list.", url:"https://www.bitget.com", active:false},
    {name:"CoinEx", type:"CEX", desc:"Winding down Dec 2026; retained.", url:"https://www.coinex.com", active:false},
    {name:"Mercatus", type:"DEX", desc:"A decentralized exchange (DEX) facilitating secure and trustless trading on Kadena.", url:"https://www.mercatus.works/", active:true},
    {name:"Bro-DEX", type:"DEX", desc:"A community-driven decentralized exchange offering seamless swaps and liquidity pools.", url:"https://dex.bro.pink/", active:true},
    {name:"eckoDEX", type:"DEX", desc:"Kaddex successor; not on current ecosystem list.", url:"https://swap.kaddex.com", active:false},
    {name:"KDSwap", type:"DEX", desc:"Low liquidity; not on current ecosystem list.", url:"https://www.kdswap.exchange", active:false},
    {name:"Kaddex", type:"DEX", desc:"Shut down Oct 2025.", url:"https://kaddex.com", active:false}
  ];
  const active = venues.filter(v => v.active);
  const badgeClass = (t) => t === "CEX" ? "teal" : t === "DEX" ? "violet" : t === "P2P" ? "gold" : "teal";
  app.innerHTML = `
    <div class="card" style="margin-bottom:16px">
      <h2><span class="icon">⇄</span> Buy / Sell KDA</h2>
      <p style="font-size:12px;color:var(--khub-muted);margin-top:6px">
        Venues curated from kda-chain.org/ecosystem. Verify availability, fees, and regional restrictions before sending funds.
      </p>
    </div>
    <div class="grid-2">
      ${active.map(v => `
        <a href="${v.url}" target="_blank" rel="noreferrer" class="card" style="text-decoration:none;color:inherit;display:flex;flex-direction:column;gap:8px">
          <div>
            <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px">
              <span class="badge ${badgeClass(v.type)}">${v.type}</span>
              <span style="color:var(--khub-muted);font-size:11px">↗</span>
            </div>
            <div style="font-weight:700;font-size:15px;margin-bottom:6px">${v.name}</div>
            <p style="font-size:11px;color:var(--khub-muted);line-height:1.5">${v.desc}</p>
          </div>
        </a>`).join("")}
    </div>
    <p style="font-size:11px;color:var(--khub-muted);text-align:center;margin-top:12px">
      Source: <a href="https://kda-chain.org/ecosystem.html" target="_blank" rel="noreferrer" style="color:var(--seal-teal)">kda-chain.org/ecosystem.html</a>
    </p>
  `;
}

function renderBridge() {
  const bridges = [
    {name:"Hyperlane", desc:"The permissionless interoperability layer connecting Kadena to the modular blockchain world.", url:"https://www.hyperlane.xyz/", active:true},
    {name:"Flux", desc:"Decentralized Web3 cloud infrastructure offering scalable computing and blockchain-as-a-service.", url:"https://runonflux.com/", active:true},
    {name:"Kinesis Bridge", desc:"Securely transfer assets between Kadena and Ethereum.", url:"https://kinesis.kda-chain.org/", active:true},
    {name:"Zelcore Fusion", desc:"Flux-powered bridge, Ethereum ↔ Kadena. Not on current ecosystem list.", url:"https://zelcore.io", active:false},
    {name:"Klever Bridge", desc:"Lock/mint bridge to Ethereum, BSC, Polygon. Not on current ecosystem list.", url:"https://klever.org", active:false}
  ];
  const active = bridges.filter(b => b.active);
  app.innerHTML = `
    <div class="card" style="margin-bottom:16px">
      <h2><span class="icon">⇄</span> Bridges — move KDA cross-chain</h2>
      <p style="font-size:12px;color:var(--khub-muted);margin-top:6px">
        Bridges lock or burn KDA on one chain and mint a wrapped asset on another. Verify the destination contract and test with a small amount first. KHUB does not operate any bridge.
      </p>
    </div>
    <div class="grid-3">
      ${active.map(v => `
        <a href="${v.url}" target="_blank" rel="noreferrer" class="card" style="text-decoration:none;color:inherit;display:flex;flex-direction:column;gap:8px;border-left:3px solid var(--seal-teal)">
          <div>
            <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px">
              <span class="badge" style="background:rgba(88,166,166,0.12);color:var(--seal-teal);border:1px solid rgba(88,166,166,0.3)">BRIDGE</span>
              <span style="color:var(--khub-muted);font-size:11px">↗</span>
            </div>
            <div style="font-weight:700;font-size:15px;margin-bottom:6px">${v.name}</div>
            <p style="font-size:11px;color:var(--khub-muted);line-height:1.5">${v.desc}</p>
          </div>
        </a>`).join("")}
    </div>
    <p style="font-size:11px;color:var(--khub-muted);text-align:center;margin-top:12px">
      Source: <a href="https://kda-chain.org/ecosystem.html" target="_blank" rel="noreferrer" style="color:var(--seal-teal)">kda-chain.org/ecosystem.html</a>
    </p>
  `;
}


function renderWallets() {
  const wallets = [
    {name:"Linx Wallet", desc:"Chainless, gas-free, non-custodial wallet for Kadena & Alephium. Swap aggregator, WalletConnect, and passkey support.", url:"https://linxlabs.org/wallet", tags:["Gas-free","Multi-chain"]},
    {name:"eckoWALLET", desc:"Kadena's ecosystem gateway. Cross-chain transfers across all 20 chains, fiat on-ramp, WalletConnect v2, and hardware wallet support.", url:"https://eckowallet.com/", tags:["Official","20 chains"]},
    {name:"Zelcore", desc:"Non-custodial multi-chain wallet supporting KDA among 100,000+ assets across 80+ blockchains. Decentralized 2FA and hardware wallet support.", url:"https://zelcore.io/", tags:["80+ chains","d2FA"]},
    {name:"Chainweaver", desc:"Developer-focused wallet for Kadena and Pact. Smart contract deployment, module explorer, SigBuilder, and multi-sig. Community-maintained fork.", url:"https://github.com/kda-community/chainweaver", tags:["Dev-focused","Pact"]}
  ];
  app.innerHTML = `
    <div class="card" style="margin-bottom:16px">
      <h2><span class="icon">◆</span> Wallets — store and manage KDA</h2>
      <p style="font-size:12px;color:var(--khub-muted);margin-top:6px">
        Non-custodial wallets for holding KDA and interacting with Kadena dApps. Always verify the URL and download source before installing. KHUB does not operate any wallet.
      </p>
    </div>
    <div class="grid-2">
      ${wallets.map(v => `
        <a href="${v.url}" target="_blank" rel="noreferrer" class="card" style="text-decoration:none;color:inherit;display:flex;flex-direction:column;gap:8px;border-left:3px solid #a78bfa">
          <div>
            <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px">
              <span class="badge" style="background:rgba(167,139,250,0.12);color:#a78bfa;border:1px solid rgba(167,139,250,0.3)">WALLET</span>
              <span style="color:var(--khub-muted);font-size:11px">↗</span>
            </div>
            <div style="font-weight:700;font-size:15px;margin-bottom:6px">${v.name}</div>
            <p style="font-size:11px;color:var(--khub-muted);line-height:1.5">${v.desc}</p>
            <div style="display:flex;gap:6px;flex-wrap:wrap;margin-top:8px">
              ${v.tags.map(t => `<span style="font-size:10px;font-weight:700;color:var(--khub-muted);background:var(--khub-bg);border:1px solid var(--khub-border);padding:2px 6px;border-radius:4px">${t}</span>`).join("")}
            </div>
          </div>
        </a>`).join("")}
    </div>
    <p style="font-size:11px;color:var(--khub-muted);text-align:center;margin-top:12px">
      Source: <a href="https://kda-chain.org/ecosystem.html" target="_blank" rel="noreferrer" style="color:var(--seal-teal)">kda-chain.org/ecosystem.html</a>
    </p>
  `;
}

function renderActivity() {
  const a = state.activity || {};
  const chain = a.chain || {};
  const gh = a.github || {};
  const social = a.social || {};
  const range = a.range || {};

  const chainAlive = chain.alive === true;
  const chainStatus = chainAlive ? "ALIVE" : "STALE";
  const chainColor = chainAlive ? "var(--green)" : "var(--red)";

  const ratePath = sparkPath(chain.rate_15m || [], "blocks", 400, 60);
  const chainDayBars = (chain.daily_30d || []).slice(-14);

  const cDays = gh.commits_30d || [];
  const cMax = Math.max(1, ...cDays.map(d => d.commits));
  const commitsBars = cDays.length ? cDays.slice(-30).map(d => {
    const h = Math.max(2, (d.commits / cMax) * 100);
    return `<div style="flex:1;min-width:2px;display:flex;flex-direction:column;align-items:center;gap:2px" title="${new Date(d.day).toLocaleDateString()} · ${d.commits} commits">
      <div style="width:100%;height:${h}px;background:var(--seal-violet);border-radius:1px"></div>
    </div>`;
  }).join("") : `<div class="empty">No commits yet</div>`;

  const repos = (gh.by_repo || []).slice(0, 8);
  const authors = (gh.by_author || []).slice(0, 6);

  const sDays = social.posts_30d || [];
  const sMax = Math.max(1, ...sDays.map(d => d.posts));
  const socialBars = sDays.length ? sDays.slice(-30).map(d => {
    const h = Math.max(2, (d.posts / sMax) * 100);
    const sColor = d.sentiment > 0.2 ? "var(--green)" : d.sentiment < -0.2 ? "var(--red)" : "var(--seal-gold)";
    return `<div style="flex:1;min-width:2px;display:flex;flex-direction:column;align-items:center;gap:2px" title="${new Date(d.day).toLocaleDateString()} · ${d.posts} posts · sentiment ${d.sentiment.toFixed(2)}">
      <div style="width:100%;height:${h}px;background:${sColor};border-radius:1px"></div>
    </div>`;
  }).join("") : `<div class="empty">No social posts yet</div>`;

  const bySrc = social.by_source_30d || [];
  const bySrcRows = bySrc.length ? bySrc.map(s => {
    const pct = ((s.sentiment + 1) / 2) * 100;
    const color = s.sentiment > 0.2 ? "var(--green)" : s.sentiment < -0.2 ? "var(--red)" : "var(--seal-gold)";
    return `<div style="display:flex;align-items:center;gap:8px;margin-bottom:6px">
      <span style="font-size:11px;font-family:monospace;width:140px;color:var(--khub-text);overflow:hidden;text-overflow:ellipsis">${s.source}</span>
      <div style="flex:1;background:var(--khub-bg);border-radius:3px;overflow:hidden">
        <div style="width:${pct}%;height:100%;background:${color}"></div>
      </div>
      <span style="font-size:10px;color:${color};width:45px;text-align:right">${s.sentiment.toFixed(2)}</span>
      <span style="font-size:10px;color:var(--khub-muted);width:45px;text-align:right">${s.posts}</span>
    </div>`;
  }).join("") : `<div class="empty">No items in last 30 days</div>`;

  const totalCommits = cDays.reduce((a, b) => a + b.commits, 0);
  const totalPosts = sDays.reduce((a, b) => a + b.posts, 0);
  const avgSent = sDays.length ? sDays.reduce((a, b) => a + b.sentiment, 0) / sDays.length : 0;
  const sentColor = avgSent > 0.2 ? "var(--green)" : avgSent < -0.2 ? "var(--red)" : "var(--seal-gold)";
  const rangeDays = (lo, hi) => {
    if (!lo || !hi) return "—";
    const d = (new Date(hi) - new Date(lo)) / 86400000;
    return `${Math.round(d)}d history`;
  };

  app.innerHTML = `
    <div class="card" style="margin-bottom:16px;border-left:3px solid ${chainColor}">
      <h2><span class="icon">◉</span> Activity — 30-day history</h2>
      <p style="font-size:12px;color:var(--khub-muted);margin-top:6px">
        Aggregates everything the oracle has collected: chain blocks, GitHub commits (backfilled), and social post cadence + sentiment.
      </p>
    </div>

    <div class="grid-4" style="margin-bottom:16px">
      <div class="stat" style="border-left:3px solid ${chainColor}">
        <div class="stat-label">Chain</div>
        <div class="stat-value" style="color:${chainColor};font-size:20px">${chainStatus}</div>
        <div class="stat-sub">last block: ${chain.last_block_at ? timeAgo(chain.last_block_at) : "—"}</div>
      </div>
      <div class="stat">
        <div class="stat-label">Commits (30d)</div>
        <div class="stat-value">${totalCommits}</div>
        <div class="stat-sub">${rangeDays(range.commits_first, range.commits_last)}</div>
      </div>
      <div class="stat">
        <div class="stat-label">Social posts (30d)</div>
        <div class="stat-value">${totalPosts}</div>
        <div class="stat-sub">${rangeDays(range.social_first, range.social_last)}</div>
      </div>
      <div class="stat">
        <div class="stat-label">Avg sentiment (30d)</div>
        <div class="stat-value" style="color:${sentColor};font-size:22px">${avgSent >= 0 ? "+" : ""}${avgSent.toFixed(2)}</div>
        <div class="stat-sub">−1 … +1</div>
      </div>
    </div>

    <div class="grid-2" style="margin-bottom:16px">
      <div class="card">
        <h2><span class="icon">◈</span> Chain — blocks/day (14d)</h2>
        <div style="display:flex;align-items:flex-end;gap:2px;margin:8px 0">
          ${chainDayBars.length ? chainDayBars.map(d => {
            const h = Math.max(2, (d.blocks / Math.max(1, ...chainDayBars.map(x => x.blocks))) * 100);
            return `<div style="flex:1;display:flex;flex-direction:column;align-items:center" title="${new Date(d.day).toLocaleDateString()} · ${d.blocks} blocks">
              <div style="width:100%;height:${h}px;background:var(--seal-teal);border-radius:1px"></div>
            </div>`;
          }).join("") : '<div class="empty">Collecting…</div>'}
        </div>
        <h2 style="margin-top:16px"><span class="icon">◈</span> Chain — blocks/min (15m)</h2>
        ${ratePath ? `<svg viewBox="0 0 400 60" preserveAspectRatio="none" class="sparkline"><path d="${ratePath}"/></svg>` : '<div class="empty">Collecting…</div>'}
      </div>
      <div class="card">
        <h2><span class="icon">◆</span> GitHub commits — 30d daily</h2>
        <div style="display:flex;align-items:flex-end;gap:1px;margin:8px 0">
          ${commitsBars}
        </div>
        <p style="font-size:11px;color:var(--khub-muted);margin-top:4px">Hover bars for per-day totals · kda-community</p>
      </div>
    </div>

    <div class="grid-2" style="margin-bottom:16px">
      <div class="card">
        <h2><span class="icon">◆</span> Commits by repo (all-time in DB)</h2>
        ${repos.length ? repos.map(r => {
          const pct = (r.commits / Math.max(1, repos[0].commits)) * 100;
          return `<div style="display:flex;align-items:center;gap:8px;margin-bottom:6px">
            <span style="font-size:11px;font-family:monospace;width:200px;color:var(--khub-text);overflow:hidden;text-overflow:ellipsis" title="${r.repo}">${r.repo}</span>
            <div style="flex:1;background:var(--khub-bg);border-radius:3px;overflow:hidden">
              <div style="width:${pct}%;height:100%;background:var(--seal-violet)"></div>
            </div>
            <span style="font-size:11px;font-weight:700;color:var(--khub-text);width:40px;text-align:right">${r.commits}</span>
            <span style="font-size:10px;color:var(--khub-muted);width:60px;text-align:right">${r.last_commit ? timeAgo(r.last_commit) : "—"}</span>
          </div>`;
        }).join("") : '<div class="empty">No commits collected yet</div>'}
        ${authors.length ? `<div style="margin-top:12px;padding-top:12px;border-top:1px solid var(--khub-border)">
          <div style="font-size:10px;font-weight:700;text-transform:uppercase;letter-spacing:0.05em;color:var(--khub-muted);margin-bottom:6px">Top contributors</div>
          ${authors.map(x => `<div style="display:flex;justify-content:space-between;font-size:11px;padding:2px 0"><span style="color:var(--khub-text)">${x.author}</span><span style="color:var(--khub-muted);font-family:monospace">${x.commits}</span></div>`).join("")}
        </div>` : ''}
      </div>
      <div class="card">
        <h2><span class="icon">◈</span> Social — 30d daily posts</h2>
        <div style="display:flex;align-items:flex-end;gap:1px;margin:8px 0">
          ${socialBars}
        </div>
        <p style="font-size:11px;color:var(--khub-muted);margin-top:4px">Color = daily sentiment · green bullish / gold neutral / red bearish</p>
      </div>
    </div>

    <div class="card">
      <h2><span class="icon">◆</span> Sentiment by source — 30d</h2>
      <div style="margin:8px 0">
        ${bySrcRows}
      </div>
      <p style="font-size:11px;color:var(--khub-muted);margin-top:6px">
        Score −1 (negative) to +1 (positive) · keyword-based. Chain rate limited by oracle uptime; commits are backfilled 500 deep per repo.
      </p>
    </div>
  `;
}

// ═══ Tab wiring ═══
(function(){
  const nav = document.getElementById("nav");
  if (!nav) { console.error("[init] nav missing"); return; }

  nav.onclick = function(e) {
    const btn = e.target.closest(".tab");
    if (!btn) return;
    nav.querySelectorAll(".tab").forEach(t => t.classList.remove("active"));
    btn.classList.add("active");
    activeTab = btn.dataset.tab;
    try { render(); }
    catch (err) { console.error("[click]", activeTab, err); document.getElementById("app").innerHTML = '<div class="card"><div class="empty">Error: ' + err.message + '</div></div>'; }
  };

  if (!window._oraclePolling) {
    window._oraclePolling = true;
    const fn = (typeof poll === "function") ? poll : async function(){};
    fn();
    setInterval(fn, 2000);
  }
})();



// ────────────────────────────────────────────────────────────────────



// ────────────────────────────────────────────────────────────────────



// ────────────────────────────────────────────────────────────────────
