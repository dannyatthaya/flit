pub const INJECT_JS: &str = r#"
(function () {
  try { if (window.top !== window.self) return; } catch (e) { return; }
  if (window.__flit__ && window.__flit__.__ready) return;

  function emit(event, payload) {
    try {
      var t = window.__TAURI_INTERNALS__;
      if (t && typeof t.invoke === 'function') {
        t.invoke('plugin:event|emit', { event: event, payload: payload }).catch(function () {});
      }
    } catch (e) {}
  }

  function video() { return document.querySelector('video'); }
  function playerBar() { return document.querySelector('ytmusic-player-bar'); }
  function moviePlayer() { return document.getElementById('movie_player'); }
  function q(sel, root) { try { return (root || document).querySelector(sel); } catch (e) { return null; } }
  function txt(el) { return (el && el.textContent || '').trim(); }

  // Every control is looked up inside the player bar only, so a page-wide
  // match (e.g. a playlist's "Shuffle" play button) can never be clicked.
  // Class/id selectors come first because aria-labels are localized.
  var SEL = {
    playPause: ['ytmusic-player-bar #play-pause-button'],
    next: ['ytmusic-player-bar .next-button'],
    previous: ['ytmusic-player-bar .previous-button'],
    like: [
      'ytmusic-player-bar #button-shape-like button',
      'ytmusic-player-bar #button-shape-like',
      'ytmusic-player-bar ytmusic-like-button-renderer .like'
    ],
    dislike: [
      'ytmusic-player-bar #button-shape-dislike button',
      'ytmusic-player-bar #button-shape-dislike',
      'ytmusic-player-bar ytmusic-like-button-renderer .dislike'
    ],
    shuffle: ['ytmusic-player-bar .shuffle', 'ytmusic-player-bar [aria-label*="Shuffle" i]'],
    repeat: ['ytmusic-player-bar .repeat', 'ytmusic-player-bar [aria-label*="Repeat" i]']
  };

  function first(selectors) {
    for (var i = 0; i < selectors.length; i++) {
      var el = q(selectors[i]);
      if (el) return el;
    }
    return null;
  }
  function clickFirst(selectors) {
    var el = first(selectors);
    if (el) { el.click(); return true; }
    return false;
  }

  var controls = {
    playPause: function () {
      if (!clickFirst(SEL.playPause)) {
        var v = video();
        if (v) { if (v.paused) { v.play(); } else { v.pause(); } }
      }
    },
    next: function () { clickFirst(SEL.next); },
    previous: function () { clickFirst(SEL.previous); },
    like: function () { clickFirst(SEL.like); },
    dislike: function () { clickFirst(SEL.dislike); },
    shuffle: function () { clickFirst(SEL.shuffle); },
    repeat: function () { clickFirst(SEL.repeat); },
    seek: function (seconds) {
      var v = video();
      if (v && isFinite(seconds)) { try { v.currentTime = Math.max(0, seconds); } catch (e) {} }
    },
    setVolume: function (vol) {
      vol = Math.min(100, Math.max(0, Math.round(Number(vol) || 0)));
      var p = moviePlayer();
      try {
        if (p && typeof p.setVolume === 'function') {
          // YouTube's own player API keeps YTM's state, slider and the
          // remembered volume in sync (setting video.volume would not).
          p.setVolume(vol);
          if (vol > 0 && typeof p.isMuted === 'function' && p.isMuted() && typeof p.unMute === 'function') p.unMute();
        } else {
          var v = video();
          if (v) { v.volume = vol / 100; if (vol > 0) v.muted = false; }
        }
      } catch (e) {}
      var slider = q('ytmusic-player-bar #volume-slider');
      if (slider) { try { slider.value = vol; } catch (e) {} }
    },
    queueJump: function (index, videoId) {
      var items = realQueueItems();
      var it = items[index];
      // The popup may be showing a slightly stale queue; trust the id over the index.
      if (videoId && (!it || itemVideoId(it) !== videoId)) {
        it = null;
        for (var i = 0; i < items.length; i++) {
          if (itemVideoId(items[i]) === videoId) { it = items[i]; break; }
        }
      }
      if (!it) return;
      var target =
        it.querySelector('ytmusic-play-button-renderer') ||
        it.querySelector('#play-button') ||
        it.querySelector('.song-title') ||
        it.querySelector('ytmusic-thumbnail') ||
        it;
      ['pointerdown', 'mousedown', 'mouseup', 'click'].forEach(function (type) {
        try {
          target.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window }));
        } catch (e) {}
      });
    }
  };

  function realQueueItems() {
    var all = document.querySelectorAll('ytmusic-player-queue-item');
    var out = [];
    for (var i = 0; i < all.length; i++) {
      var it = all[i];
      if (it.closest('#counterpart-renderer')) continue;
      if (it.parentElement && it.parentElement.closest('ytmusic-player-queue-item')) continue;
      out.push(it);
    }
    return out;
  }

  function readMeta() {
    var title = '', artist = '', album = '', artwork = '';
    var ms = navigator.mediaSession && navigator.mediaSession.metadata;
    if (ms) {
      title = ms.title || '';
      artist = ms.artist || '';
      album = ms.album || '';
      if (ms.artwork && ms.artwork.length) {
        artwork = (ms.artwork[ms.artwork.length - 1] && ms.artwork[ms.artwork.length - 1].src)
          || (ms.artwork[0] && ms.artwork[0].src) || '';
      }
    }
    if (!title) {
      var bar = playerBar();
      if (bar) {
        title = txt(q('.title', bar));
        var byline = txt(q('.byline', bar));
        if (byline) { artist = byline.split('•')[0].trim(); }
      }
    }
    return { title: title, artist: artist, album: album, artwork: artwork };
  }

  function currentVideoId(artwork) {
    try {
      var v = new URL(location.href).searchParams.get('v');
      if (v) return v;
    } catch (e) {}
    var m = (artwork || '').match(/\/vi\/([^\/]+)\//);
    if (m) return m[1];
    return '';
  }

  // ---- Album art: fetched in the background so a slow image never delays
  // state updates; failures are retried a few times instead of sticking. ----
  var ART_RETRY_MS = 30000, ART_MAX_ATTEMPTS = 4;
  var art = { key: '', status: 'idle', attempts: 0, failedAt: 0, dataUri: '', color: '', url: '' };

  function dominantColor(ctx) {
    try {
      var d = ctx.getImageData(0, 0, 300, 300).data;
      var r = 0, g = 0, b = 0, n = 0;
      for (var i = 0; i < d.length; i += 4 * 60) { r += d[i]; g += d[i + 1]; b += d[i + 2]; n++; }
      if (!n) return '';
      r = Math.round(r / n); g = Math.round(g / n); b = Math.round(b / n);
      function h(x) { var s = x.toString(16); return s.length === 1 ? '0' + s : s; }
      return '#' + h(r) + h(g) + h(b);
    } catch (e) { return ''; }
  }

  async function loadArt(videoId, fallbackUrl) {
    var candidates = [];
    if (fallbackUrl) candidates.push(fallbackUrl);
    if (videoId) {
      candidates.push(
        'https://i.ytimg.com/vi/' + videoId + '/maxresdefault.jpg',
        'https://i.ytimg.com/vi/' + videoId + '/sddefault.jpg',
        'https://i.ytimg.com/vi/' + videoId + '/hqdefault.jpg'
      );
    }
    for (var i = 0; i < candidates.length; i++) {
      var url = candidates[i];
      try {
        var res = await fetch(url, { mode: 'cors', credentials: 'omit' });
        if (!res.ok) continue;
        var bmp = await createImageBitmap(await res.blob());
        var canvas = document.createElement('canvas');
        canvas.width = 300; canvas.height = 300;
        var ctx = canvas.getContext('2d');
        var iw = bmp.width || 300, ih = bmp.height || 300;
        var side = Math.min(iw, ih);
        ctx.drawImage(bmp, (iw - side) / 2, (ih - side) / 2, side, side, 0, 0, 300, 300);
        if (bmp.close) bmp.close();
        var dataUri = '';
        try { dataUri = canvas.toDataURL('image/jpeg', 0.85); } catch (e) { dataUri = ''; }
        return { dataUri: dataUri, color: dominantColor(ctx), url: url };
      } catch (e) {}
    }
    return null;
  }

  function ensureArt(key, videoId, fallbackUrl) {
    if (!key) return;
    if (art.key !== key) {
      art = { key: key, status: 'idle', attempts: 0, failedAt: 0, dataUri: '', color: '', url: '' };
    }
    if (art.status === 'loading' || art.status === 'ready') return;
    if (art.status === 'failed' &&
        (art.attempts >= ART_MAX_ATTEMPTS || Date.now() - art.failedAt < ART_RETRY_MS)) return;
    var mine = art;
    mine.status = 'loading';
    mine.attempts++;
    loadArt(videoId, fallbackUrl).then(function (r) {
      if (art !== mine) return;
      if (r) {
        mine.dataUri = r.dataUri; mine.color = r.color; mine.url = r.url; mine.status = 'ready';
      } else {
        mine.status = 'failed'; mine.failedAt = Date.now();
      }
    }, function () {
      if (art === mine) { mine.status = 'failed'; mine.failedAt = Date.now(); }
    });
  }

  function isPlaying() {
    var v = video();
    if (v) return !v.paused && !v.ended;
    return !!(navigator.mediaSession && navigator.mediaSession.playbackState === 'playing');
  }

  function likeStatus() {
    var r = q('ytmusic-player-bar ytmusic-like-button-renderer');
    var st = r && (r.getAttribute('like-status') || '').toUpperCase();
    if (st === 'LIKE') return 'like';
    if (st === 'DISLIKE') return 'dislike';
    return 'none';
  }

  function ytmStore() {
    try {
      var app = document.querySelector('ytmusic-app');
      if (app && app.store && typeof app.store.getState === 'function') return app.store.getState();
    } catch (e) {}
    return null;
  }

  function label(el) {
    return ((el && (el.getAttribute('aria-label') || el.getAttribute('title'))) || '').toLowerCase();
  }

  function shuffleOn() {
    var st = ytmStore();
    var qs = st && st.queue;
    if (qs && typeof qs.shuffleEnabled === 'boolean') return qs.shuffleEnabled;
    var b = first(SEL.shuffle);
    if (!b) return false;
    var holder = b.closest('[aria-pressed]') || b.querySelector('[aria-pressed]');
    var pressed = b.getAttribute('aria-pressed') || (holder && holder.getAttribute('aria-pressed'));
    if (pressed === 'true') return true;
    if (pressed === 'false') return false;
    var l = label(b);
    if (/\boff\b/.test(l)) return false;
    if (/\bon\b/.test(l)) return true;
    try {
      var iconEl = b.querySelector('yt-icon, tp-yt-iron-icon, svg') || b;
      var m = (getComputedStyle(iconEl).color || '').match(/(\d+(?:\.\d+)?)/g);
      if (m && m.length >= 3) {
        var lum = (+m[0] + +m[1] + +m[2]) / 3;
        var alpha = m.length >= 4 ? +m[3] : 1;
        return alpha >= 0.9 && lum >= 190;
      }
    } catch (e) {}
    return false;
  }

  // Map YouTube's repeat enums ("REPEAT_MODE_ONE", "ALL", "NONE", ...) exactly;
  // substring checks would read "NONE" as "ONE".
  function normRepeat(v) {
    var s = String(v == null ? '' : v).toUpperCase();
    if (/(^|_)ONE$/.test(s)) return 'one';
    if (/(^|_)ALL$/.test(s)) return 'all';
    if (/(^|_)(NONE|OFF)$/.test(s)) return 'none';
    return null;
  }

  function repeatMode() {
    var st = ytmStore();
    var m = st && st.queue && normRepeat(st.queue.repeatMode);
    if (m) return m;
    var bar = playerBar();
    if (bar) {
      m = normRepeat(bar.repeatMode) || normRepeat(bar.getAttribute('repeat-mode')) ||
          normRepeat(bar.getAttribute('repeat-mode_'));
      if (m) return m;
    }
    var l = label(first(SEL.repeat));
    if (/\brepeat one\b/.test(l)) return 'one';
    if (/\brepeat all\b/.test(l)) return 'all';
    return 'none';
  }

  function volumeState() {
    var p = moviePlayer();
    try {
      if (p && typeof p.getVolume === 'function') {
        return { volume: p.getVolume(), muted: typeof p.isMuted === 'function' ? !!p.isMuted() : false };
      }
    } catch (e) {}
    var v = video();
    if (v) return { volume: Math.round(v.volume * 100), muted: !!v.muted };
    return { volume: null, muted: false };
  }

  function itemVideoId(it) {
    try {
      var d = it.data || (it.polymerController && it.polymerController.data) || it.__data || {};
      if (d.videoId) return d.videoId;
      var ne = d.navigationEndpoint || d.playNavigationEndpoint || (d.tapTarget && d.tapTarget.navigationEndpoint);
      var we = ne && (ne.watchEndpoint || ne.watchPlaylistEndpoint);
      if (we && we.videoId) return we.videoId;
    } catch (e) {}
    return '';
  }

  function readQueue() {
    var out = [];
    var items = realQueueItems();
    for (var i = 0; i < items.length && out.length < 100; i++) {
      var it = items[i];
      var title = txt(q('.song-title', it));
      if (!title) continue;
      var vid = itemVideoId(it);
      var thumb = vid ? ('https://i.ytimg.com/vi/' + vid + '/mqdefault.jpg') : '';
      if (!thumb) {
        var img = q('img', it);
        thumb = img && img.src && img.src.indexOf('data:') !== 0 ? img.src : '';
      }
      // `index` is the position among *all* queue items, which is what
      // queueJump() indexes, even when untitled items are skipped here.
      out.push({ index: i, title: title, artist: txt(q('.byline', it)), thumb: thumb, videoId: vid });
    }
    return out;
  }

  // Heavy fields are only sent when they change; Rust keeps the last copy.
  var sentArtKey = '', sentQueueJson = '', lastJson = '', lastEmitAt = 0;
  var HEARTBEAT_MS = 15000;

  function buildState() {
    var v = video();
    var meta = readMeta();
    var videoId = currentVideoId(meta.artwork);
    var key = videoId || meta.artwork || '';
    ensureArt(key, videoId, meta.artwork);
    var ready = art.key === key && art.status === 'ready';
    var vol = volumeState();
    return {
      title: meta.title,
      artist: meta.artist,
      album: meta.album,
      artKey: key,
      artworkUrl: (ready && art.url) || meta.artwork || '',
      color: ready ? art.color : '',
      durationSec: v && isFinite(v.duration) ? v.duration : 0,
      positionSec: v ? (v.currentTime || 0) : 0,
      playing: isPlaying(),
      videoId: videoId,
      shuffle: shuffleOn(),
      repeat: repeatMode(),
      likeStatus: likeStatus(),
      volume: vol.volume,
      muted: vol.muted,
      __artReady: ready
    };
  }

  var timer = null;
  function tick() {
    try {
      var state = buildState();
      var artReady = state.__artReady;
      delete state.__artReady;
      var queue = readQueue();
      var queueJson = JSON.stringify(queue);
      var json = JSON.stringify(state) + queueJson;
      var now = Date.now();
      if (json !== lastJson || artReady && sentArtKey !== state.artKey || now - lastEmitAt > HEARTBEAT_MS) {
        if (artReady && sentArtKey !== state.artKey && art.dataUri) {
          state.artworkData = art.dataUri;
          sentArtKey = state.artKey;
        }
        if (queueJson !== sentQueueJson) {
          state.queue = queue;
          sentQueueJson = queueJson;
        }
        emit('flit-state', state);
        lastJson = json;
        lastEmitAt = now;
      }
    } catch (e) {}
    scheduleNext();
  }
  function scheduleNext() {
    var delay = document.hidden ? 5000 : (isPlaying() ? 1000 : 2000);
    timer = setTimeout(tick, delay);
  }

  var STYLE =
    '#flit-strip{position:fixed;right:16px;bottom:84px;z-index:2147483646;display:flex;' +
    'background:rgba(20,20,24,.6);backdrop-filter:blur(10px);border:1px solid rgba(255,255,255,.1);' +
    'border-radius:999px;padding:3px;opacity:.45;transition:opacity .15s;font-family:system-ui,sans-serif}' +
    '#flit-strip:hover,#flit-strip:focus-within{opacity:1}' +
    '#flit-strip button{all:unset;cursor:pointer;width:26px;height:26px;border-radius:50%;display:grid;' +
    'place-items:center;color:#fff;font-size:13px}' +
    '#flit-strip button:hover,#flit-strip button:focus-visible{background:rgba(255,255,255,.15)}' +
    '#flit-toast{position:fixed;right:16px;bottom:124px;z-index:2147483647;display:flex;gap:10px;align-items:center;' +
    'background:rgba(20,20,24,.92);color:#fff;border:1px solid rgba(255,255,255,.14);border-radius:12px;' +
    'padding:10px 12px;font:13px system-ui,sans-serif;box-shadow:0 6px 24px rgba(0,0,0,.35)}' +
    '#flit-toast button{all:unset;cursor:pointer;padding:5px 10px;border-radius:7px;background:rgba(255,255,255,.14)}' +
    '#flit-toast button:hover{background:rgba(255,255,255,.24)}';

  function ensureStyle() {
    if (document.getElementById('flit-style') || !document.head) return;
    var style = document.createElement('style');
    style.id = 'flit-style';
    style.textContent = STYLE;
    document.head.appendChild(style);
  }

  function injectStrip() {
    if (document.getElementById('flit-strip') || !document.body) return;
    ensureStyle();
    var strip = document.createElement('div');
    strip.id = 'flit-strip';
    var widget = document.createElement('button');
    widget.title = 'Open Flit widget';
    widget.setAttribute('aria-label', 'Open Flit widget');
    widget.textContent = '▤';
    widget.addEventListener('click', function () { emit('flit-toggle-popup', {}); });
    strip.appendChild(widget);
    document.body.appendChild(strip);
  }

  function showUpdateNotif(version) {
    try {
      if (!document.body) return;
      ensureStyle();
      var old = document.getElementById('flit-toast');
      if (old) old.remove();
      var toast = document.createElement('div');
      toast.id = 'flit-toast';
      toast.setAttribute('role', 'status');
      var msg = document.createElement('span');
      msg.textContent = 'Flit ' + String(version).slice(0, 64) + ' is ready to install.';
      var open = document.createElement('button');
      open.textContent = 'Open widget';
      open.addEventListener('click', function () { toast.remove(); emit('flit-toggle-popup', {}); });
      var dismiss = document.createElement('button');
      dismiss.textContent = 'Later';
      dismiss.addEventListener('click', function () { toast.remove(); });
      toast.appendChild(msg); toast.appendChild(open); toast.appendChild(dismiss);
      document.body.appendChild(toast);
    } catch (e) {}
  }

  window.__flit__ = controls;
  window.__flit__.__ready = true;
  window.__flit__.showUpdateNotif = showUpdateNotif;

  function start() {
    if (timer) return;
    try { injectStrip(); } catch (e) {}
    scheduleNext();
    try {
      var obs = new MutationObserver(function () {
        if (!document.getElementById('flit-strip')) { try { injectStrip(); } catch (e) {} }
      });
      obs.observe(document.body || document.documentElement, { childList: true });
    } catch (e) {}
  }

  if (document.readyState === 'complete' || document.readyState === 'interactive') { start(); }
  else { window.addEventListener('DOMContentLoaded', start); }
  setTimeout(start, 3000);
})();
"#;
