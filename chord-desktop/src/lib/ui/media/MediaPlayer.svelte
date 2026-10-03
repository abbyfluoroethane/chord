<script lang="ts">
  // The Chord media player: Video.js v10 elements with Chord controls. One control bar
  // serves video and audio, in the chat and in the viewer. The styles are in media.css.
  import Maximize2 from 'lucide-svelte/icons/maximize-2';
  import Pause from 'lucide-svelte/icons/pause';
  import Play from 'lucide-svelte/icons/play';
  import RotateCcw from 'lucide-svelte/icons/rotate-ccw';
  import Volume1 from 'lucide-svelte/icons/volume-1';
  import Volume2 from 'lucide-svelte/icons/volume-2';
  import VolumeX from 'lucide-svelte/icons/volume-x';
  import Icon from '../Icon.svelte';
  import './elements';
  import './media.css';

  let {
    kind,
    src,
    name,
    surface = 'chat',
    autoplay = false,
    startAt = 0,
    onexpand,
    onfail,
    media = $bindable()
  }: {
    kind: 'video' | 'audio';
    src: string;
    name: string;
    /** `chat`: in a message. `viewer`: a slide of the full-window viewer. */
    surface?: 'chat' | 'viewer';
    autoplay?: boolean;
    /** Start at this time, in seconds. The viewer continues where the chat stopped. */
    startAt?: number;
    /** Open the video in the viewer. Without it, the bar has no expand button. */
    onexpand?: () => void;
    /** The webview cannot load or play the file. */
    onfail?: () => void;
    media?: HTMLMediaElement;
  } = $props();

  // A video with preload="metadata" shows no frame in WebKit. A time fragment makes it
  // show the first frame, so the tile is not black before playback.
  const source = $derived(kind === 'video' && startAt === 0 ? `${src}#t=0.001` : src);

  // The player sets its own state when the media attaches, and that can stop the
  // `autoplay` attribute. So the start time and the play go in here, after the metadata.
  function loaded() {
    if (!media) return;
    if (startAt > 0) media.currentTime = startAt;
    if (autoplay) {
      // A video that starts by itself in the chat starts without sound.
      if (surface === 'chat') media.muted = true;
      void media.play().catch(() => undefined);
    }
  }
</script>

{#snippet bar()}
  <media-play-button class="cm-button">
    <span class="cm-when-paused"><Icon icon={Play} size={18} /></span>
    <span class="cm-when-playing"><Icon icon={Pause} size={18} /></span>
    <span class="cm-when-ended"><Icon icon={RotateCcw} size={18} /></span>
  </media-play-button>
  <media-time class="cm-time" type="current"></media-time>
  <media-time-slider class="cm-slider cm-seek">
    <media-slider-track class="cm-track">
      <media-slider-buffer class="cm-buffer"></media-slider-buffer>
      <media-slider-fill class="cm-fill"></media-slider-fill>
    </media-slider-track>
    <media-slider-thumb class="cm-thumb"></media-slider-thumb>
  </media-time-slider>
  <media-time class="cm-time" type="duration"></media-time>
  <media-mute-button class="cm-button">
    <span class="cm-vol-off"><Icon icon={VolumeX} size={18} /></span>
    <span class="cm-vol-low"><Icon icon={Volume1} size={18} /></span>
    <span class="cm-vol-high"><Icon icon={Volume2} size={18} /></span>
  </media-mute-button>
  {#if surface === 'viewer' || kind === 'audio'}
    <media-volume-slider class="cm-slider cm-volume">
      <media-slider-track class="cm-track">
        <media-slider-fill class="cm-fill"></media-slider-fill>
      </media-slider-track>
      <media-slider-thumb class="cm-thumb"></media-slider-thumb>
    </media-volume-slider>
  {/if}
  {#if onexpand}
    <button class="cm-button" aria-label="Open in the viewer" onclick={onexpand}>
      <Icon icon={Maximize2} size={18} />
    </button>
  {/if}
{/snippet}

{#if kind === 'video'}
  <video-player class="chord-media" data-surface={surface}>
    <media-container class="cm-surface cm-video">
      <!-- A shared file has no caption track. -->
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
        bind:this={media}
        src={source}
        preload="metadata"
        playsinline
        onloadedmetadata={loaded}
        onerror={() => onfail?.()}
      ></video>
      <media-gesture type="tap" pointer="mouse" action="togglePaused"></media-gesture>
      <media-buffering-indicator class="cm-buffering">
        <span class="cm-spinner"></span>
      </media-buffering-indicator>
      <media-play-button class="cm-big-play">
        <Icon icon={Play} size={20} />
      </media-play-button>
      <media-controls class="cm-controls">
        <media-controls-content class="cm-content">
          <media-controls-group class="cm-bar" aria-label="Video controls">
            {@render bar()}
          </media-controls-group>
        </media-controls-content>
      </media-controls>
    </media-container>
  </video-player>
{:else}
  <audio-player class="chord-media" data-surface={surface}>
    <media-container class="cm-surface cm-audio">
      <audio
        bind:this={media}
        {src}
        preload="metadata"
        onloadedmetadata={loaded}
        onerror={() => onfail?.()}
      ></audio>
      <span class="cm-name">{name}</span>
      <media-controls class="cm-controls" visibility="always">
        <media-controls-content class="cm-content">
          <media-controls-group class="cm-bar" aria-label="Audio controls">
            {@render bar()}
          </media-controls-group>
        </media-controls-content>
      </media-controls>
    </media-container>
  </audio-player>
{/if}
