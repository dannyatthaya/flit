<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import IconSettings from "~icons/solar/settings-linear";
  import IconClose from "~icons/solar/close-circle-linear";
  import IconPrev from "~icons/solar/skip-previous-bold";
  import IconNext from "~icons/solar/skip-next-bold";
  import IconPlay from "~icons/solar/play-bold";
  import IconPause from "~icons/solar/pause-bold";
  import IconLike from "~icons/solar/like-bold";
  import IconDislike from "~icons/solar/dislike-bold";
  import IconShuffle from "~icons/solar/shuffle-bold";
  import IconRepeat from "~icons/solar/repeat-bold";
  import IconRepeatOne from "~icons/solar/repeat-one-minimalistic-bold";
  import IconQueue from "~icons/solar/playlist-bold";
  import IconVolume from "~icons/solar/volume-loud-bold";
  import IconNote from "~icons/solar/music-note-bold";

  type QueueItem = { title: string; artist: string; thumb: string };
  type PlayerState = {
    title: string;
    artist: string;
    album: string;
    artworkUrl: string;
    artworkData: string;
    color: string;
    durationSec: number;
    positionSec: number;
    playing: boolean;
    videoId: string;
    shuffle: boolean;
    repeat: "none" | "all" | "one";
    likeStatus: "none" | "like" | "dislike";
    queue: QueueItem[];
  };

  const EMPTY: PlayerState = {
    title: "",
    artist: "",
    album: "",
    artworkUrl: "",
    artworkData: "",
    color: "#3a3a44",
    durationSec: 0,
    positionSec: 0,
    playing: false,
    videoId: "",
    shuffle: false,
    repeat: "none",
    likeStatus: "none",
    queue: [],
  };

  let s = $state<PlayerState>({ ...EMPTY });
  let seeking = $state(false);
  let seekValue = $state(0);
  let volume = $state(100);
  let showQueue = $state(false);
  let showSettings = $state(false);

  let autostart = $state(false);
  let urlInput = $state("");

  const COMPACT_H = 282;

  const art = $derived(s.artworkData || s.artworkUrl || "");
  const accent = $derived(s.color || "#3a3a44");
  const hasTrack = $derived(!!s.title);

  function fmt(sec: number): string {
    if (!isFinite(sec) || sec < 0) sec = 0;
    const m = Math.floor(sec / 60);
    const r = Math.floor(sec % 60);
    return `${m}:${r.toString().padStart(2, "0")}`;
  }

  function targetHeight(): number {
    if (!showQueue) return COMPACT_H;
    const rows = Math.min(s.queue.length || 1, 8);
    return Math.min(COMPACT_H + rows * 46 + 14, 660);
  }

  function control(action: string) {
    invoke("player_control", { action }).catch(() => {});
  }
  function commitSeek(v: number) {
    invoke("player_seek", { position: v }).catch(() => {});
    seeking = false;
  }
  function setVol(v: number) {
    invoke("player_volume", { volume: v }).catch(() => {});
  }
  function jump(i: number) {
    invoke("player_control", { action: `queue_jump_${i}` }).catch(() => {});
  }
  function toggleShuffle() {
    s.shuffle = !s.shuffle;
    control("shuffle");
  }
  function cycleRepeat() {
    s.repeat = s.repeat === "none" ? "all" : s.repeat === "all" ? "one" : "none";
    control("repeat");
  }
  function closePopup() {
    invoke("hide_tray_popup").catch(() => {});
  }
  function toggleQueue() {
    showQueue = !showQueue;
    invoke("resize_popup", { height: targetHeight() }).catch(() => {});
  }

  function setAutostart(v: boolean) {
    autostart = v;
    invoke("autostart_set", { enabled: v }).catch(() => {
      autostart = !v;
    });
  }
  function navigate() {
    const url = urlInput.trim();
    if (!url) return;
    invoke("navigate_ytm", { url })
      .then(() => {
        urlInput = "";
        showSettings = false;
      })
      .catch((e) => alert(`Could not open: ${e}`));
  }

  onMount(() => {
    const un = listen<PlayerState>("flit-player-state", (e) => {
      s = { ...EMPTY, ...e.payload };
      if (!seeking) seekValue = s.positionSec;
    });
    invoke<boolean>("autostart_get").then((v) => (autostart = v)).catch(() => {});
    if (!showQueue) invoke("resize_popup", { height: COMPACT_H }).catch(() => {});
    return () => un.then((f) => f());
  });
</script>

<div class="shell" style="--accent: {accent}">
  <header data-tauri-drag-region>
    <span class="brand" data-tauri-drag-region>Flit</span>
    <div class="hbtns">
      <button class="icon" title="Settings" onclick={() => (showSettings = !showSettings)} aria-label="Settings"><IconSettings /></button>
      <button class="icon" title="Hide" onclick={closePopup} aria-label="Hide"><IconClose /></button>
    </div>
  </header>

  <section class="hero">
    {#if art}
      <img class="cover" src={art} alt="" />
    {:else}
      <div class="cover placeholder"><IconNote /></div>
    {/if}
    <div class="meta">
      <div class="title" title={s.title}>{s.title || "Nothing playing"}</div>
      <div class="artist" title={s.artist}>{s.artist || "—"}</div>
      {#if s.album}<div class="album" title={s.album}>{s.album}</div>{/if}
    </div>
  </section>

  <div class="seek">
    <span class="t">{fmt(seekValue)}</span>
    <input
      class="range"
      type="range"
      min="0"
      max={s.durationSec || 0}
      step="1"
      value={seekValue}
      disabled={!hasTrack}
      style="--fill: {s.durationSec ? Math.min(100, (seekValue / s.durationSec) * 100) : 0}%"
      oninput={(e) => {
        seeking = true;
        seekValue = +e.currentTarget.value;
      }}
      onchange={(e) => commitSeek(+e.currentTarget.value)}
    />
    <span class="t">{fmt(s.durationSec)}</span>
  </div>

  <div class="transport">
    <button class="tbtn" onclick={() => control("previous")} aria-label="Previous"><IconPrev /></button>
    <button class="play" onclick={() => control("play_pause")} aria-label="Play/Pause">
      {#if s.playing}<IconPause />{:else}<IconPlay />{/if}
    </button>
    <button class="tbtn" onclick={() => control("next")} aria-label="Next"><IconNext /></button>
  </div>

  <div class="secondary">
    <button class="sbtn" class:active={s.likeStatus === "like"} onclick={() => control("like")} aria-label="Like" title="Like"><IconLike /></button>
    <button class="sbtn" class:active={s.likeStatus === "dislike"} onclick={() => control("dislike")} aria-label="Dislike" title="Dislike"><IconDislike /></button>
    <button class="sbtn" class:active={s.shuffle} onclick={toggleShuffle} aria-label="Shuffle" title="Shuffle"><IconShuffle /></button>
    <button class="sbtn" class:active={s.repeat !== "none"} onclick={cycleRepeat} aria-label="Repeat" title={s.repeat === "one" ? "Repeat one" : s.repeat === "all" ? "Repeat all" : "Repeat off"}>
      {#if s.repeat === "one"}<IconRepeatOne />{:else}<IconRepeat />{/if}
    </button>
    <button class="sbtn" class:active={showQueue} onclick={toggleQueue} aria-label="Queue" title="Queue"><IconQueue /></button>
  </div>

  <div class="volume">
    <span class="vlabel"><IconVolume /></span>
    <input
      class="range"
      type="range"
      min="0"
      max="100"
      step="1"
      value={volume}
      style="--fill: {volume}%"
      oninput={(e) => {
        volume = +e.currentTarget.value;
        setVol(volume);
      }}
    />
    <span class="vval">{volume}</span>
  </div>

  {#if showQueue}
    <div class="queue">
      {#if s.queue.length === 0}
        <div class="qempty">Queue is empty</div>
      {:else}
        {#each s.queue as item, i}
          <button class="qitem" onclick={() => jump(i)}>
            {#if item.thumb}<img src={item.thumb} alt="" />{:else}<div class="qthumb"><IconNote /></div>{/if}
            <div class="qmeta">
              <div class="qtitle">{item.title}</div>
              <div class="qartist">{item.artist}</div>
            </div>
          </button>
        {/each}
      {/if}
    </div>
  {/if}

  {#if showSettings}
    <div class="settings">
      <div class="srow">
        <span>Launch at startup</span>
        <button class="toggle" class:on={autostart} onclick={() => setAutostart(!autostart)} aria-label="Toggle autostart"></button>
      </div>
      <div class="srow col">
        <span>Open YouTube Music URL</span>
        <div class="urlrow">
          <input
            class="urlinput"
            placeholder="https://music.youtube.com/..."
            bind:value={urlInput}
            onkeydown={(e) => e.key === "Enter" && navigate()}
          />
          <button class="go" onclick={navigate}>Go</button>
        </div>
      </div>
      <button class="close-settings" onclick={() => (showSettings = false)}>Done</button>
    </div>
  {/if}
</div>

<style>
  :global(html, body) {
    margin: 0;
    background: #101014;
    overflow: hidden;
    font-family: Inter, system-ui, -apple-system, "Segoe UI", sans-serif;
    -webkit-font-smoothing: antialiased;
    user-select: none;
  }

  .shell {
    position: relative;
    height: 100vh;
    box-sizing: border-box;
    padding: 8px 14px 12px;
    display: flex;
    flex-direction: column;
    gap: 9px;
    color: #f4f4f6;
    background:
      radial-gradient(120% 70% at 0% 0%, color-mix(in srgb, var(--accent) 42%, transparent) 0%, transparent 55%),
      #101014;
    overflow: hidden;
  }
  /* size every inlined Solar icon by its button's font-size */
  .shell :global(svg) {
    width: 1em;
    height: 1em;
    display: block;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 22px;
    flex: 0 0 auto;
  }
  .brand {
    font-weight: 700;
    letter-spacing: 0.02em;
    font-size: 0.8rem;
    opacity: 0.85;
  }
  .hbtns { display: flex; gap: 2px; }
  .icon {
    all: unset;
    cursor: pointer;
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border-radius: 7px;
    font-size: 17px;
    opacity: 0.65;
    transition: background 0.15s, opacity 0.15s;
  }
  .icon:hover { background: rgba(255, 255, 255, 0.12); opacity: 1; }

  .hero {
    display: flex;
    gap: 13px;
    align-items: center;
  }
  .cover {
    width: 76px;
    height: 76px;
    border-radius: 11px;
    object-fit: cover;
    box-shadow: 0 8px 20px rgba(0, 0, 0, 0.45);
    flex: 0 0 auto;
  }
  .cover.placeholder {
    display: grid;
    place-items: center;
    font-size: 1.7rem;
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.45);
  }
  .meta { min-width: 0; }
  .title {
    font-size: 1.02rem;
    font-weight: 650;
    line-height: 1.2;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .artist { opacity: 0.8; font-size: 0.84rem; margin-top: 2px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .album { opacity: 0.5; font-size: 0.73rem; margin-top: 1px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .seek, .volume {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .t, .vval { font-variant-numeric: tabular-nums; font-size: 0.7rem; opacity: 0.55; min-width: 30px; text-align: center; }
  .vlabel { font-size: 16px; opacity: 0.7; display: grid; place-items: center; }

  .range {
    -webkit-appearance: none;
    appearance: none;
    flex: 1;
    height: 4px;
    border-radius: 999px;
    background: linear-gradient(
      to right,
      #2ee6a0 var(--fill, 0%),
      rgba(255, 255, 255, 0.16) var(--fill, 0%)
    );
    cursor: pointer;
  }
  .range:disabled { cursor: default; opacity: 0.5; }
  .range::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
    transition: transform 0.12s;
  }
  .range::-webkit-slider-thumb:hover { transform: scale(1.25); }

  .transport {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 22px;
  }
  .tbtn {
    all: unset;
    cursor: pointer;
    font-size: 22px;
    opacity: 0.85;
    transition: opacity 0.15s, transform 0.1s;
  }
  .tbtn:hover { opacity: 1; }
  .tbtn:active { transform: scale(0.9); }
  .play {
    all: unset;
    cursor: pointer;
    width: 46px;
    height: 46px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 22px;
    background: #fff;
    color: #111;
    box-shadow: 0 6px 16px rgba(0, 0, 0, 0.4);
    transition: transform 0.1s;
  }
  .play:active { transform: scale(0.93); }

  .secondary {
    display: flex;
    justify-content: space-between;
    padding: 0 2px;
  }
  .sbtn {
    all: unset;
    cursor: pointer;
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    font-size: 18px;
    color: rgba(255, 255, 255, 0.5);
    transition: color 0.15s;
  }
  .sbtn:hover { color: rgba(255, 255, 255, 0.82); }
  .sbtn.active { color: #fff; }

  .queue {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-top: 1px;
  }
  .queue::-webkit-scrollbar { width: 6px; }
  .queue::-webkit-scrollbar-thumb { background: rgba(255, 255, 255, 0.2); border-radius: 999px; }
  .qempty { opacity: 0.5; font-size: 0.8rem; text-align: center; padding: 18px; }
  .qitem {
    all: unset;
    cursor: pointer;
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 4px 6px;
    border-radius: 8px;
    transition: background 0.12s;
  }
  .qitem:hover { background: rgba(255, 255, 255, 0.08); }
  .qitem img, .qthumb {
    width: 34px;
    height: 34px;
    border-radius: 6px;
    object-fit: cover;
    flex: 0 0 auto;
  }
  .qthumb { display: grid; place-items: center; background: rgba(255, 255, 255, 0.08); font-size: 0.9rem; color: rgba(255,255,255,0.5); }
  .qmeta { min-width: 0; }
  .qtitle { font-size: 0.81rem; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .qartist { font-size: 0.71rem; opacity: 0.6; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .settings {
    position: absolute;
    inset: 0;
    z-index: 3;
    background: rgba(14, 14, 18, 0.94);
    backdrop-filter: blur(14px);
    padding: 16px 14px;
    display: flex;
    flex-direction: column;
    gap: 13px;
    overflow-y: auto;
  }
  .srow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 0.84rem;
  }
  .srow.col { flex-direction: column; align-items: stretch; gap: 7px; }
  .toggle {
    all: unset;
    cursor: pointer;
    width: 38px;
    height: 22px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.18);
    position: relative;
    transition: background 0.18s;
    flex: 0 0 auto;
  }
  .toggle::after {
    content: "";
    position: absolute;
    top: 3px;
    left: 3px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    transition: transform 0.18s;
  }
  .toggle.on { background: rgba(255, 255, 255, 0.9); }
  .toggle.on::after { transform: translateX(16px); background: #15151a; }
  .urlrow { display: flex; gap: 6px; }
  .urlinput {
    flex: 1;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    padding: 7px 9px;
    color: #fff;
    font-size: 0.78rem;
    outline: none;
  }
  .go, .close-settings {
    all: unset;
    cursor: pointer;
    text-align: center;
    padding: 7px 12px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.14);
    font-size: 0.8rem;
    transition: background 0.15s;
  }
  .go:hover, .close-settings:hover { background: rgba(255, 255, 255, 0.22); }
  .close-settings { margin-top: auto; }
</style>
