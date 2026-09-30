<script lang="ts">
  // The reaction picker: the emoji panel in a popover. It closes after a pick.
  import EmojiPanel from './EmojiPanel.svelte';
  import Popover, { type Placement } from './Popover.svelte';

  let {
    anchor,
    onpick,
    onclose,
    placement = 'bottom-end'
  }: {
    anchor: HTMLElement;
    onpick: (emoji: string) => void;
    onclose: () => void;
    placement?: Placement;
  } = $props();
</script>

<Popover {anchor} {onclose} {placement} label="Pick a reaction">
  <EmojiPanel
    label="Pick a reaction"
    onpick={(e) => {
      onpick(e);
      onclose();
    }}
  />
</Popover>
