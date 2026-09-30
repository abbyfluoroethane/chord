// The pins of the open chat. Live, the core keeps them in a private PEP node of the account,
// so the other devices of the user show the same pins. In the preview they stay in memory.
import type { Pin } from '$lib/chord/types';
import { plainError } from './adapt';
import { api, live } from './bridge';
import { addPin, pinFromItem, pinOf } from './pins';
import type { TimelineItem } from './types';
import { ui } from './ui.svelte';

class PinStore {
  private byChat = $state<Record<string, Pin[]>>({});

  /** The pins of a chat, newest first. */
  of(chat: string): Pin[] {
    return this.byChat[chat] ?? [];
  }

  /** Read the pins of a chat from the local copy. Call it when the chat opens. */
  async load(chat: string): Promise<void> {
    if (!live || !chat) return;
    try {
      this.byChat[chat] = await (await api()).listPins(chat);
    } catch {
      /* Keep what we have. The next load tries again. */
    }
  }

  isPinned(chat: string, item: TimelineItem): boolean {
    return pinOf(this.of(chat), item) !== undefined;
  }

  /** Pin the message, or remove its pin. */
  async toggle(chat: string, item: TimelineItem): Promise<void> {
    const existing = pinOf(this.of(chat), item);
    if (existing) return this.remove(existing);
    if (!live) {
      this.byChat[chat] = addPin(this.of(chat), pinFromItem(chat, item, Date.now()));
      return;
    }
    try {
      await (await api()).pinMessage(item.id);
      ui.say('Message pinned.');
    } catch (e) {
      ui.say(plainError(e));
    }
    await this.load(chat);
  }

  async remove(pin: Pin): Promise<void> {
    if (!live) {
      this.byChat[pin.chat] = this.of(pin.chat).filter((p) => p.key !== pin.key);
      return;
    }
    try {
      await (await api()).unpinMessage(pin.chat, pin.key);
    } catch (e) {
      ui.say(plainError(e));
    }
    await this.load(pin.chat);
  }
}

export const pins = new PinStore();
