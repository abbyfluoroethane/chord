<script lang="ts">
  // On and off switch. Colour is never the only sign: the knob sits left or right.
  let {
    checked,
    label,
    onchange,
    disabled = false
  }: { checked: boolean; label: string; onchange: (v: boolean) => void; disabled?: boolean } =
    $props();
</script>

<button
  class="switch"
  class:on={checked}
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .switch {
    position: relative;
    flex: none;
    width: 40px;
    height: 24px;
    border-radius: 12px;
    background: var(--surface-300);
    border: 1px solid var(--line);
    transition: background var(--dur-fast);
  }
  .switch.on {
    background: var(--brand);
    border-color: var(--brand);
  }
  .switch:disabled {
    opacity: 0.5;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--ink-muted);
    transition:
      transform var(--dur-fast) var(--ease-out),
      background var(--dur-fast);
  }
  .on .knob {
    transform: translateX(16px);
    background: var(--on-brand);
  }
</style>
