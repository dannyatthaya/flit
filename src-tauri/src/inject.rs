pub const INJECT_JS: &str = r#"
(function () {
  try { if (window.top !== window.self) return; } catch (e) { return; }
  if (window.__flit__ && window.__flit__.__ready) return;

  function emit(event, payload) {
    try {
      var t = window.__TAURI_INTERNALS__;
      if (t && typeof t.invoke === 'function') {
        t.invoke('plugin:event|emit', { event: event, payload: payload });
      }
    } catch (e) {}
  }

  function video() { return document.querySelector('video'); }
  function playerBar() { return document.querySelector('ytmusic-player-bar'); }
  function q(sel, root) { try { return (root || document).querySelector(sel); } catch (e) { return null; } }
  function txt(el) { return (el && el.textContent || '').trim(); }

  function clickFirst(selectors) {
    for (var i = 0; i < selectors.length; i++) {
      var el = q(selectors[i]);
      if (el) { el.click(); return true; }
    }
    return false;
  }

  var controls = {
    playPause: function () {
      if (!clickFirst(['ytmusic-player-bar #play-pause-button', '#play-pause-button', '.play-pause-button'])) {
        var v = video();
        if (v) { if (v.paused) { v.play(); } else { v.pause(); } }
      }
    },
    next: function () {
      clickFirst(['ytmusic-player-bar .next-button', '.next-button', 'tp-yt-paper-icon-button.next-button']);
    },
    previous: function () {
      clickFirst(['ytmusic-player-bar .previous-button', '.previous-button', 'tp-yt-paper-icon-button.previous-button']);
    },
    like: function () {
      clickFirst([
        'ytmusic-player-bar ytmusic-like-button-renderer #button-shape-like button',
        'ytmusic-like-button-renderer [aria-label*="like" i]'
      ]);
    },
    dislike: function () {
      clickFirst([
        'ytmusic-player-bar ytmusic-like-button-renderer #button-shape-dislike button',
        'ytmusic-like-button-renderer [aria-label*="dislike" i]'
      ]);
    },
    shuffle: function () {
      clickFirst(['ytmusic-player-bar [aria-label*="Shuffle" i]', 'ytmusic-player-bar .shuffle', '[aria-label*="Shuffle" i]']);
    },
    repeat: function () {
      clickFirst(['ytmusic-player-bar [aria-label*="Repeat" i]', 'ytmusic-player-bar .repeat', '[aria-label*="Repeat" i]']);
    },
    seek: function (seconds) {
      var v = video();
      if (v && isFinite(seconds)) { try { v.currentTime = Math.max(0, seconds); } catch (e) {} }
    },
    setVolume: function (vol) {
      var v = video();
      if (v) { try { v.volume = Math.min(1, Math.max(0, (Number(vol) || 0) / 100)); } catch (e) {} }
    },
    queueJump: function (index) {
      var items = realQueueItems();
      var it = items[index];
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

  var artCache = { key: '', dataUri: '', color: '', url: '' };

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

  async function processArt(videoId, fallbackUrl) {
    var key = videoId || fallbackUrl || '';
    if (!key || key === artCache.key) return artCache;
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
        var res = await fetch(url, { mode: 'cors' });
        if (!res.ok) continue;
        var blob = await res.blob();
        var bmp = await createImageBitmap(blob);
        var canvas = document.createElement('canvas');
        canvas.width = 300; canvas.height = 300;
        var ctx = canvas.getContext('2d');
        var iw = bmp.width || 300, ih = bmp.height || 300;
        var side = Math.min(iw, ih);
        ctx.drawImage(bmp, (iw - side) / 2, (ih - side) / 2, side, side, 0, 0, 300, 300);
        var dataUri = '';
        try { dataUri = canvas.toDataURL('image/jpeg', 0.85); } catch (e) { dataUri = ''; }
        artCache = { key: key, dataUri: dataUri, color: dominantColor(ctx), url: url };
        return artCache;
      } catch (e) {}
    }
    artCache = { key: key, dataUri: '', color: '', url: fallbackUrl || '' };
    return artCache;
  }

  function isPlaying() {
    var v = video();
    if (v) return !v.paused && !v.ended;
    return !!(navigator.mediaSession && navigator.mediaSession.playbackState === 'playing');
  }

  function likeStatus() {
    var r = q('ytmusic-player-bar ytmusic-like-button-renderer') || q('ytmusic-like-button-renderer');
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

  function shuffleOn() {
    var st = ytmStore();
    var qs = st && st.queue;
    if (qs && typeof qs.shuffleEnabled === 'boolean') return qs.shuffleEnabled;
    var b = q('ytmusic-player-bar [aria-label*="Shuffle" i]') || q('[aria-label*="Shuffle" i]');
    if (!b) return false;
    var holder = b.closest('[aria-pressed]') || b.querySelector('[aria-pressed]');
    var pressed = (b.getAttribute('aria-pressed')) || (holder && holder.getAttribute('aria-pressed'));
    if (pressed === 'true') return true;
    if (pressed === 'false') return false;
    var label = (b.getAttribute('aria-label') || b.getAttribute('title') || '').toLowerCase();
    if (label.indexOf('off') >= 0) return false;
    if (label.indexOf(' on') >= 0 || label.indexOf('shuffle on') >= 0) return true;
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

  function repeatMode() {
    var b = q('ytmusic-player-bar [aria-label*="Repeat" i]') || q('[aria-label*="Repeat" i]');
    var label = (b && (b.getAttribute('aria-label') || b.getAttribute('title')) || '').toLowerCase();
    if (label.indexOf('repeat one') >= 0 || label.indexOf('one') >= 0) return 'one';
    if (label.indexOf('repeat all') >= 0 || label.indexOf('all') >= 0) return 'all';
    if (label.indexOf('repeat off') >= 0 || label.indexOf('off') >= 0 || label.indexOf('none') >= 0) return 'none';
    var st = ytmStore();
    var rm = st && st.queue && st.queue.repeatMode;
    if (rm != null) {
      var v = String(rm).toUpperCase();
      if (v.indexOf('ONE') >= 0) return 'one';
      if (v.indexOf('ALL') >= 0) return 'all';
    }
    return 'none';
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
    for (var i = 0; i < items.length && i < 100; i++) {
      var it = items[i];
      var title = txt(q('.song-title', it));
      var artist = txt(q('.byline', it));
      var vid = itemVideoId(it);
      var thumb = vid ? ('https://i.ytimg.com/vi/' + vid + '/mqdefault.jpg') : '';
      if (!thumb) {
        var img = q('img', it);
        thumb = img && img.src && img.src.indexOf('data:') !== 0 ? img.src : '';
      }
      if (title) out.push({ title: title, artist: artist, thumb: thumb });
    }
    return out;
  }

  async function buildState() {
    var v = video();
    var meta = readMeta();
    var videoId = currentVideoId(meta.artwork);
    var art = await processArt(videoId, meta.artwork);
    return {
      title: meta.title,
      artist: meta.artist,
      album: meta.album,
      artworkUrl: art.url || meta.artwork || '',
      artworkData: art.dataUri || '',
      color: art.color || '',
      durationSec: v ? (v.duration || 0) : 0,
      positionSec: v ? (v.currentTime || 0) : 0,
      playing: isPlaying(),
      videoId: videoId,
      shuffle: shuffleOn(),
      repeat: repeatMode(),
      likeStatus: likeStatus(),
      queue: readQueue()
    };
  }

  var timer = null;
  async function tick() {
    try {
      var state = await buildState();
      emit('flit-state', state);
      if (window.__flit__.__debug) { try { console.debug('[flit]', state.title, state.playing, state.positionSec); } catch (e) {} }
    } catch (e) {}
    scheduleNext();
  }
  function scheduleNext() {
    var delay = isPlaying() ? 1000 : (document.hidden ? 5000 : 1500);
    timer = setTimeout(tick, delay);
  }

  function injectStrip() {
    if (document.getElementById('flit-strip')) return;
    if (!document.body) return;
    var style = document.getElementById('flit-style');
    if (!style) {
      style = document.createElement('style');
      style.id = 'flit-style';
      style.textContent =
        '#flit-strip{position:fixed;right:16px;bottom:84px;z-index:2147483646;display:flex;gap:6px;' +
        'background:rgba(20,20,24,.78);backdrop-filter:blur(10px);border:1px solid rgba(255,255,255,.12);' +
        'border-radius:999px;padding:6px;box-shadow:0 6px 24px rgba(0,0,0,.35);font-family:system-ui,sans-serif}' +
        '#flit-strip button{all:unset;cursor:pointer;width:30px;height:30px;border-radius:50%;display:grid;' +
        'place-items:center;color:#fff;font-size:14px;transition:background .15s}' +
        '#flit-strip button:hover{background:rgba(255,255,255,.15)}';
      document.head.appendChild(style);
    }
    var strip = document.createElement('div');
    strip.id = 'flit-strip';
    var widget = document.createElement('button');
    widget.title = 'Open Flit widget';
    widget.textContent = '▤';
    widget.addEventListener('click', function () { emit('flit-toggle-popup', {}); });
    strip.appendChild(widget);
    document.body.appendChild(strip);
  }

  window.__flit__ = controls;
  window.__flit__.__ready = true;
  window.__flit__.__debug = false;
  window.__flit__.showUpdateNotif = function (version) {
    try { console.info('[flit] update available:', version); } catch (e) {}
  };

  function start() {
    if (timer) return;
    try { injectStrip(); } catch (e) {}
    scheduleNext();
    try {
      var obs = new MutationObserver(function () {
        if (!document.getElementById('flit-strip')) { try { injectStrip(); } catch (e) {} }
      });
      obs.observe(document.documentElement, { childList: true, subtree: true });
    } catch (e) {}
  }

  if (document.readyState === 'complete' || document.readyState === 'interactive') { start(); }
  else { window.addEventListener('DOMContentLoaded', start); }
  setTimeout(start, 3000);
})();
"#;
