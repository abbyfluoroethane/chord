// A static preview of the Chord Desktop UI with the sample data. No Tauri, no server.
// Build: npm run build:preview (output in build-preview/, relative paths).
import { mount } from 'svelte';
import '$lib/theme/fonts.css';
import '$lib/theme/tokens.css';
import '$lib/theme/base.css';
import '$lib/theme/tooltip.css';
import { installContextGuard } from '$lib/ui/contextmenu.svelte';
import Preview from './Preview.svelte';
import { loadPreviewEmoji } from './emoji-source';

installContextGuard();
void loadPreviewEmoji();
mount(Preview, { target: document.getElementById('app')! });
