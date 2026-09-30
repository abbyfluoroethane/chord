# Desktop app functional gaps (chord-desktop)

Paths are relative to /Users/abbyfluoroethane/Documents/GitHub/chord/chord-desktop. Line numbers come from a read-only pass. No build ran.

## Summary

- The header search box does nothing. It is a plain input with no handler. It looks like a working feature.
- Five notification settings do nothing: Desktop notifications, Sound, both default levels, and Mute DMs. Only the stored value changes. No code reads it. There is also no sound code at all.
- Bridge parity is good. Every command in api.ts is registered in lib.rs. Seven api.ts functions have no UI caller. Two of them (roomAffiliations, unblockAll) hide useful features.
- Discord parity gaps: no message search, no pins, no threads, no @mention autocomplete, no drag-and-drop or paste upload, no channel topic edit, no categories, no roles beyond MUC affiliations, no voice.
- Drafts live only in memory. They are lost on restart. Notes and the space rename are marked "comes later".

## Findings

| ID | Severity | Type | Spec | Evidence | What to do |
|---|---|---|---|---|---|
| DESKTOPGAPS-01 | High | UX gap | n/a | src/lib/ui/ChatHeader.svelte:45 `<input type="search" placeholder="Search" aria-label="Search messages" />` has no bind, no handler, no form | Build search on local store data first. Use XEP-0313 MAM search fields later. Until then, remove the box or disable it with a hint. |
| DESKTOPGAPS-02 | High | Bug | n/a | src/lib/ui/SettingsNotifications.svelte:16-40 and src/lib/ui/prefs.svelte.ts:14-18. grep finds no reader of `desktopNotifications`, `sound`, `dmLevel`, `channelLevel`, `muteDms` outside prefs and the settings page | Wire each toggle or remove it. Send the defaults to the core with `setNotificationLevel`. Gate the OS notice in src-tauri/src/notify.rs on `desktopNotifications`. Add a sound or remove the Sound toggle. |
| DESKTOPGAPS-03 | Medium | Bug | n/a | src-tauri/src/notify.rs:38-45 `show` sends an OS notice whenever no window has focus. It has no check of the user setting | Read the setting in Rust. Send the pref from the UI with `set_settings`. |
| DESKTOPGAPS-04 | Medium | Bug | n/a | src/lib/ui/prefs.svelte.ts:2-3 and :39 uses `localStorage` in the live app. Other stores use `settings.set` (app.svelte.ts:208, contacts.svelte.ts:241, ui.svelte.ts:148) | Move prefs to `get_settings` and `set_settings`. Then all user data lives in one file. |
| DESKTOPGAPS-05 | Medium | Missing feature | n/a | src/lib/ui/Composer.svelte:24-30 has `:shortcode:` suggestions only. No `@` trigger. grep for "mention" in Composer finds none | Add @nick autocomplete from `app.members`. In a MUC, send the plain nick text. For XEP-0372 references, see the core audit. |
| DESKTOPGAPS-06 | Medium | Missing feature | n/a | src/lib/ui/Composer.svelte:63-72 `const drafts: Record<string, string> = {}` in memory only | Save drafts through `settings`. Keep one draft per chat. Show a draft mark in the channel list. |
| DESKTOPGAPS-07 | Medium | Missing feature | XEP-0363 | src/lib/ui/Composer.svelte:194-207. Upload works only from the file dialog. grep for `ondrop` and `onpaste` finds only the rail (CircleRail.svelte:54,61) | Add drop and paste on the composer and message list. In Tauri, use the webview drag-drop event to get paths. Paste images through a temp file. |
| DESKTOPGAPS-08 | Medium | Bug | n/a | src/lib/ui/app.svelte.ts:531-560 `sendFile(file)` keeps the file local. It is the only path for the non-live input. Composer.svelte:207 calls it from `picked()` | Fine for the preview. Keep, but mark it as preview-only in a comment so it does not spread. |
| DESKTOPGAPS-09 | Medium | UX gap | n/a | src/lib/ui/CircleDialog.svelte:159-160 "Renaming a space comes later." Field is `readonly` when live | Add a rename call in the core and the bridge. Space info is a PubSub node (see core audit). |
| DESKTOPGAPS-10 | Low | UX gap | n/a | src/lib/ui/SettingsAccount.svelte:135 "Changing your password here comes later." | Add XEP-0077 password change (`jabber:iq:register` with the new password) in core and bridge. |
| DESKTOPGAPS-11 | Medium | Missing feature | XEP-0045 s10.1 | src/lib/ui/ChannelSettings.svelte:5-30 and src/lib/ui/ChatHeader.svelte:40. The topic shows but cannot be set. `configureRoom` has name, public and membersOnly only | Add "Set topic" (subject message, XEP-0045 section 8.1) for members with the right. Add a topic field in Channel settings. |
| DESKTOPGAPS-12 | Medium | UX gap | XEP-0045 s10.1, s8.2 | src/lib/ui/PersonMenu.svelte:120-139. The menu sets affiliation only: admin, member, none, outcast. There is no kick (role none) and no mute (voice) | Add "Kick" and "Mute" to the menu. Both are role changes on an occupant. |
| DESKTOPGAPS-13 | Low | UX gap | XEP-0045 s9.5 | src/lib/ui/ChannelSettings.svelte:10 comment: "The bridge cannot read the current settings". The form shows "Keep as it is" for each field | Read the room config form (XEP-0045 section 10.2) and fill the current values. |
| DESKTOPGAPS-14 | Low | UX gap | XEP-0045 s9 | api.ts:225 `roomAffiliations` is registered in lib.rs:75 but no UI calls it | Add a ban list and member list view in Channel settings. It lets an admin undo a ban. |
| DESKTOPGAPS-15 | Low | UX gap | XEP-0191 | api.ts:270 `unblockAll` and lib.rs:95 have no UI caller | Add "Unblock all" in Privacy, beside the blocked list. |
| DESKTOPGAPS-16 | Low | UX gap | XEP-0045 s7.5 | api.ts:229 `declineRoomInvite` has no caller | Add a "Decline" button on an invite card. Check XmppLinkCard and the invite flow. |
| DESKTOPGAPS-17 | Low | UX gap | n/a | api.ts:198 `loadOlder`, :248 `deleteSpace`, :259 `removeRoomFromSpace`, :295 `pushRegistrations` have no UI caller. `timeline.paginateBack` is used instead of `loadOlder` | Add Delete space and Remove channel from space in Space settings. Remove `loadOlder` if it is dead. Show push state in Settings if push is planned. |
| DESKTOPGAPS-18 | Medium | Missing feature | n/a | grep for "pin" in src/lib/ui finds no message pin. Message menu (menus.ts:104-150) has no Pin | Use XEP-0223 or a bookmark node for pins. Or scope it out and record this in the spec. |
| DESKTOPGAPS-19 | Low | Missing feature | XEP-0461 (thread as a reply) | Replies work (`reply` command). No thread view exists | Show a reply chain view on click of the reply preview. Real threads (XEP-0201) are optional. |
| DESKTOPGAPS-20 | Low | Missing feature | n/a | src/lib/ui/menus.ts:29 imports only `channelLink`. Menu has "Copy message ID" (line 149) but no "Copy message link" | Add a message link. Use the `xmpp:` URI with `?message;id=` if the target client supports it. Otherwise copy the id only. |
| DESKTOPGAPS-21 | Low | Missing feature | n/a | No document title, dock badge or tray code. grep for `set_badge_count`, `set_title`, `tray` in src-tauri/src finds none | Show the total unread count in the window title and on the dock or taskbar icon. |
| DESKTOPGAPS-22 | Low | Missing feature | n/a | src/lib/ui/shortcuts.ts:11-21. All 11 listed shortcuts have a handler (AppShell.svelte:58-79, Composer, QuickSwitcher). Missing versus Discord: Ctrl+Shift+A style mark all read, Ctrl+Enter, Alt+click mark unread | Optional. Add "Mark all as read". |
| DESKTOPGAPS-23 | Low | UX gap | n/a | src/lib/ui/AppShell.svelte:75 Esc marks the channel read when no overlay is open. Discord does the same, but a stray Esc in the Composer also triggers it | Only mark read when the focus is not in an input. Verify in the app. Unverified. |
| DESKTOPGAPS-24 | Low | UX gap | n/a | Channel list has no categories. Spaces (XEP-0503) hold a flat list of rooms | Optional. Use a category field if the space data can hold it. |
| DESKTOPGAPS-25 | Info | Missing feature | n/a | No voice channels. No Jingle code in src/ or src-tauri/src | Out of scope for now. Record it as a decision. |
| DESKTOPGAPS-26 | Low | Bug | n/a | src/lib/ui/app.svelte.ts:986, :1034, :1053 `.catch(() => undefined)` on `leaveRoom`, `inviteToRoom`, `addSpaceMember`. src/lib/ui/live.svelte.ts:207 the same for `refreshAvatar` | Collect the failures. Tell the user, for example "2 of 5 invites failed". |
| DESKTOPGAPS-27 | Low | Bug | n/a | Empty `catch {}` blocks in src/lib/ui/CircleDialog.svelte:107 (copy link), app.svelte.ts:425, :663, :798, :957, EmojiPanel.svelte:60,70 | Show a toast in each case where the action is visible to the user. |
| DESKTOPGAPS-28 | Low | Accessibility | n/a | src/lib/ui/Toast.svelte:6 uses `role="status" aria-live="polite"`. Errors use the same region | Use `role="alert"` for errors. |
| DESKTOPGAPS-29 | Low | Accessibility | n/a | src/lib/ui/ChatHeader.svelte:45 has a label but no visible name. src/lib/ui/ForwardModal.svelte:98 `onkeydown={() => {}}` is an empty handler | Remove the empty handler. Give the row a real button role. |
| DESKTOPGAPS-30 | Info | n/a | n/a | Preview branches: the `live` flag (src/lib/ui/bridge.ts:7) has about 40 uses in app.svelte.ts. Every one has a live path except `sendFile`, and the `fx` sample data. No action exists that only works when `live` is false | No action. Keep the sample data out of production bundles if size matters. |

## Already done well

- Bridge parity. Each `invoke` name in src/lib/chord/api.ts has a matching entry in src-tauri/src/lib.rs (generate_handler, lines 36-114). No UI call goes to an unknown command.
- Modal focus. src/lib/ui/Modal.svelte uses native `<dialog>` with `showModal`. It traps focus, handles Esc, and restores focus.
- Custom controls. Toggle has `role="switch"` and `aria-checked` (Toggle.svelte:15-17). Segmented has `radiogroup`. Rail and channel rows use `aria-current` and a full label (RailItem.svelte:37, ChannelRow.svelte:71).
- Composer emoji suggestions use combobox attributes (Composer.svelte:255-257, :285).
- Keyboard shortcuts. All 11 entries in shortcuts.ts are wired.
- Error path. `ui.say(plainError(e))` runs in the main live actions: open (app.svelte.ts:272), send (:514), live.svelte.ts:254, :319, :349. `app.call` returns `{ok}` for dialogs.
- Room admin. Affiliation change, invite, join requests for spaces, and ban exist.
- OS notifications exist in Rust (notify.rs). The core applies the level and mute time.
- Also present: replies, edits, retraction, moderation, reactions, forward, mark unread, unread jump (Alt+Shift+Up/Down), jump to present, new-message divider, quick switcher, user notes, GIF and emoji panels, link previews, themes, deep links.

## Recommended next steps

1. Fix the dead notification settings (DESKTOPGAPS-02, -03, -04). Effort: 0.5 to 1 day.
2. Make the search box real, or hide it (DESKTOPGAPS-01). Hide: 10 minutes. Local search: 1 to 2 days.
3. Add @mention autocomplete (DESKTOPGAPS-05). Effort: 0.5 day.
4. Add drag-and-drop and paste upload (DESKTOPGAPS-07). Effort: 0.5 to 1 day.
5. Save drafts (DESKTOPGAPS-06). Effort: 2 to 3 hours.
6. Add set topic, kick, mute and the ban list (DESKTOPGAPS-11, -12, -14). Effort: 1 to 2 days.
7. Show partial failures in space actions (DESKTOPGAPS-26, -27, -28). Effort: 3 to 4 hours.
8. Add space rename and password change (DESKTOPGAPS-09, -10). Effort: 1 to 2 days, mostly core work.
