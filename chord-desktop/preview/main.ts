// A static preview of the Chord Desktop UI with the sample data. No Tauri, no server.
// Build: npm run build:preview (output in build-preview/, relative paths).
import { mount } from 'svelte';
import '$lib/theme/fonts.css';
import '$lib/theme/tokens.css';
import '$lib/theme/base.css';
import '$lib/theme/tooltip.css';
import Preview from './Preview.svelte';

mount(Preview, { target: document.getElementById('app')! });
