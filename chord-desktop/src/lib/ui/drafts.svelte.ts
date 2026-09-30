// Composer drafts: one for each chat, saved in the local settings under "drafts" so that
// they survive a restart. The write waits until the user stops typing.
import { settings } from './local';
import { parseDrafts, putDraft, type Drafts } from './draftdata';

const KEY = 'drafts';
const DELAY_MS = 500;

class DraftStore {
  private all = $state<Drafts>({});
  private timer: ReturnType<typeof setTimeout> | undefined;

  /** Read the saved drafts. Call it after the settings are loaded. */
  load() {
    this.all = parseDrafts(settings.get(KEY));
  }

  get(jid: string): string {
    return this.all[jid] ?? '';
  }

  /** True when the chat has a draft. The channel list shows a mark for it. */
  has(jid: string): boolean {
    return jid in this.all;
  }

  /** Keep `text` as the draft of `jid`. An empty text removes it. */
  set(jid: string, text: string) {
    if (!jid) return;
    const next = putDraft(this.all, jid, text);
    if ((next[jid] ?? '') === (this.all[jid] ?? '')) return;
    this.all = next;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => settings.set(KEY, $state.snapshot(this.all)), DELAY_MS);
  }
}

export const drafts = new DraftStore();
