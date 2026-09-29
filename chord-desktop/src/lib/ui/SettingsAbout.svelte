<script lang="ts">
  import ChordMark from './ChordMark.svelte';

  // Keep in step with package.json.
  const VERSION = '0.1.0';
  const SOURCE = 'https://github.com/abbyfluoroethane/chord';
</script>

<div class="head">
  <span class="mark"><ChordMark /></span>
  <div>
    <p class="name">Chord Desktop</p>
    <p class="meta">Version {VERSION}</p>
  </div>
</div>

<p>
  Chord is open source. Read the code, report a problem, or send a fix at
  <a href={SOURCE} target="_blank" rel="noopener noreferrer">{SOURCE}</a>.
</p>

<h2 class="section">Licences</h2>
<ul>
  <li>IBM Plex Sans and IBM Plex Mono, SIL Open Font License.</li>
  <li>Bricolage Grotesque, SIL Open Font License.</li>
  <li>Lucide icons, ISC License.</li>
  <li>PhotoSwipe image viewer, MIT License.</li>
  <li>Media Chrome player controls, MIT License.</li>
</ul>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin-bottom: var(--space-4);
  }
  .mark {
    display: block;
    width: 48px;
    height: 48px;
  }
  p {
    margin: 0 0 var(--space-3);
  }
  .head p {
    margin: 0;
  }
  .name {
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 20px;
    line-height: 26px;
  }
  .section {
    margin: var(--space-6) 0 var(--space-2);
    font-size: 12px;
    line-height: 16px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }
  ul {
    margin: 0;
    padding-left: var(--space-6);
    color: var(--ink-muted);
  }
</style>
