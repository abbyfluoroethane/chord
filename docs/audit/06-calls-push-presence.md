# Discord-parity real-time features and mobile (calls, push, presence, forms)

Scope: Jingle and calls, XEP-0357 push, XEP-0352 CSI, blocking, invisible mode, user activities, idle, forms, ad-hoc commands, and text styling. The auditor read the code only. No build ran. No server was contacted.

## Summary

- There is no voice, video, or screen-share code at all. A search for `jingle`, `muji`, `webrtc`, `stun`, and `getUserMedia` finds nothing in Rust, Svelte, TypeScript, or Kotlin. XEP-0166, 0167, 0176, 0320, 0353, 0215, and 0272 are all missing. This is the largest Discord-parity gap.
- Push (XEP-0357) works only as a thin core API. `enable_push`, `disable_push`, and `push_registrations` exist in `chord-core` and `chord-ffi`. Nothing calls them in the repository. There is no Android app in the repository. The desktop app exposes only `push_registrations`.
- There is no XEP-0352 CSI. A mobile client with push needs CSI, or the server sends every stanza and drains the battery. Chord never sends `<inactive/>` or `<active/>`.
- Invisible mode uses XEP-0016 privacy lists. This works on ejabberd. Prosody has no privacy list module in core. Chord falls back to "available" and tells the user. XEP-0186 (invisible command) is not used. The FFI (Android) has no presence API at all.
- Blocking (XEP-0191) is well done in core, CLI, FFI, and desktop. It has no spam report (XEP-0377) and no UI to block from a room occupant.
- These items are all absent: idle (XEP-0319), activities (XEP-0107, 0108, 0118), data forms UI (XEP-0004), ad-hoc commands (XEP-0050), CAPTCHA forms (XEP-0158), and XEP-0393 styling. Chord uses its own Discord-style markdown.

## Findings

| ID | Severity | Type | Spec | Evidence | What to do |
|---|---|---|---|---|---|
| CALLSPUSHPRESENCE-01 | High | Missing feature | XEP-0166, XEP-0167, XEP-0176, XEP-0320 | No file in `chord-core/src`, `chord-desktop`, or `chord-ffi` mentions `jingle`, `webrtc`, or `stun`. `xmpp-parsers` 0.23 has Jingle types, but Chord does not use them. | Plan a call stack. Use `webrtc-rs` on Rust or the WebView WebRTC API on desktop. Map SDP to Jingle RTP, ICE-UDP, and DTLS-SRTP. Start with 1:1 audio. |
| CALLSPUSHPRESENCE-02 | High | Missing XEP | XEP-0353 | No `urn:xmpp:jingle-message:0` anywhere. | Add Jingle Message Initiation: `propose`, `ringing`, `proceed`, `reject`, `retract`. Without it a call rings on every resource and a mobile device cannot wake through push. |
| CALLSPUSHPRESENCE-03 | High | Missing XEP | XEP-0215 | No `urn:xmpp:extdisco:2` in `chord-core/src/features/disco.rs:23` or elsewhere. | Fetch STUN and TURN credentials with `services` IQ at connect. Refresh them before expiry. Needed for any call behind NAT. |
| CALLSPUSHPRESENCE-04 | Medium | Missing feature | XEP-0272 (Muji) or SFU | No group call or voice channel code. Discord voice channels have no equivalent. | Decide the design first. Muji is a mesh and does not scale. An SFU (for example LiveKit or Jitsi with XMPP signaling) fits voice channels better. Bind a call to a MUC room. |
| CALLSPUSHPRESENCE-05 | Medium | Missing feature | XEP-0166 (screen share as a second content) | No screen capture code. | Do this after 1:1 calls. Use `getDisplayMedia` in the WebView. Send it as a second Jingle content. |
| CALLSPUSHPRESENCE-06 | High | Missing XEP | XEP-0352 | No `urn:xmpp:csi:0` in `chord-core`. The feature list at `disco.rs:23-34` has no CSI. | Add `csi::inactive` and `csi::active` commands. Check `<csi/>` in stream features. Send `inactive` when the desktop window loses focus or the Android app goes to the background. Re-send `active` after a stream resume. |
| CALLSPUSHPRESENCE-07 | High | Missing feature | XEP-0357 | `push.rs:3` says "`on_connected` does nothing". No caller of `enable_push` in `chord-desktop`, `chord-cli`, or in the repository outside tests. `chord-desktop/src-tauri/src/commands.rs:811` exposes only `push_registrations`. No Android app in the repository. | Write the Android glue: get the token from FCM or UnifiedPush, register with an app server, call `enable_push`. Document the app server. Without it push is dead code. |
| CALLSPUSHPRESENCE-08 | Medium | Bug | XEP-0357 section 4 | `push.rs:222-233` `start_enable` checks the server disco only. The store keeps `service` and `node` but no per-device token. The server keeps the registration per account and per resource. `push_registrations` may show a stale row after the server drops it. | On connect, do not trust the local table. Either skip the table or check with the server. Note: XEP-0357 has no query for registrations, so keep a "last enabled" time and re-enable at each login. Unverified: the stale case on ejabberd. |
| CALLSPUSHPRESENCE-09 | Medium | Spec violation | XEP-0357 section 5 (publish options) | `push.rs:236-241` builds the form with every field as `TextSingle`. `FORM_TYPE` is set as the pubsub publish-options namespace. This is correct for the standard `secret` field. Other field types are not possible. | Low risk. Allow a field type only if an app server needs it. |
| CALLSPUSHPRESENCE-10 | Medium | Bug | XEP-0016, XEP-0126 | `presence.rs:167-176` sends a privacy list `set` without a check of `jabber:iq:privacy` in server disco. The failure path (`presence.rs:246-262`) does work: it shows a notice and goes back to "available". But the user first sees "invisible" for a moment. | Check `disco.server_has("jabber:iq:privacy")` first. Hide the "Invisible" choice in `StatusMenu.svelte:29` when the server lacks it. |
| CALLSPUSHPRESENCE-11 | Medium | Missing XEP | XEP-0186 | Not used. `presence.rs:7-9` documents the XEP-0016 approach only. | Add XEP-0186 (`urn:xmpp:invisible:0`) as the first choice when the server advertises it. Prosody and other modern servers support it. Keep XEP-0016 as the fallback. XEP-0016 and XEP-0126 are deprecated or obsolete. |
| CALLSPUSHPRESENCE-12 | Low | Bug | XEP-0126 | `presence.rs:170-171` denies presence-out only for subscription `both` and `from`. A contact with a pending subscription request gets presence after approval only while the list is active, so this is correct. But `send_unavailable_to_contacts` (`presence.rs:210`) sends unavailable to the bare JID only. A contact that saw a full JID of another Chord resource is not covered. Unverified. | Test with two resources. |
| CALLSPUSHPRESENCE-13 | High | Missing feature | FFI parity | `chord-ffi/src/client.rs` has no `set_presence`, `own_presence`, or invisible call. A search for `presence` in `chord-ffi/src` finds comments only. | Add `set_presence` and `own_presence` to the FFI. An Android user cannot set status or invisible now. |
| CALLSPUSHPRESENCE-14 | Medium | Missing XEP | XEP-0319 | No `urn:xmpp:idle:1` and no idle detection in `chord-desktop`. `roster.rs:466` `store_presence` reads `show` and status only. | Send `<idle since=.../>` in presence when the OS reports idle. Show "idle" for contacts. Optional: set "away" after a timeout as Discord does. |
| CALLSPUSHPRESENCE-15 | Low | Missing XEP | XEP-0256 | Not used. XEP-0256 is deferred. XEP-0319 replaces it for presence. | Skip. Use XEP-0319. |
| CALLSPUSHPRESENCE-16 | Medium | Missing XEP | XEP-0107, XEP-0108, XEP-0118 (PEP) | No `urn:xmpp:mood`, `activity`, or `tune` in `disco.rs:23`. No `+notify` for them. | For Discord activities ("Playing X", "Listening to Y"), use XEP-0118 (tune) and XEP-0108 (activity). Or define a custom PEP node. Add `+notify` features and a UI badge in the member list. |
| CALLSPUSHPRESENCE-17 | Medium | Missing feature | XEP-0004 (rendering) | `DataForm` is used only to build or read forms in `push.rs`, `muc.rs`, `spaces.rs`, `upload.rs`, `mam.rs`. No generic form UI in `chord-desktop/src/lib/ui`. Room configuration uses fixed fields. | Add a generic XEP-0004 form renderer (text, boolean, list, jid, hidden, fixed). It serves ad-hoc commands, room config, registration, and CAPTCHA. |
| CALLSPUSHPRESENCE-18 | Medium | Missing XEP | XEP-0050 | No `http://jabber.org/protocol/commands` in the repository. | Add ad-hoc commands: list with disco, execute, and step through forms. Good base for slash commands and server admin. Use the renderer from -17. |
| CALLSPUSHPRESENCE-19 | Low | Missing XEP | XEP-0158, XEP-0077 | No CAPTCHA form handling. No in-band registration. | Handle the `captcha` form in MUC join errors and in registration. Depends on -17. |
| CALLSPUSHPRESENCE-20 | Low | Missing XEP | XEP-0393, XEP-0071 | `chord-desktop/src/lib/ui/markdown.ts:1` parses "the rules of Discord chat markdown". `message_ext.rs:85` reads the body only. XEP-0393 uses `*bold*`, `_italic_`, `~strike~`. Discord uses `**bold**`, `*italic*`. Other clients show the raw marks. | Decide: keep Discord syntax on send, and render XEP-0393 on receive. Or send with XEP-0393 rules. Do not add XHTML-IM (XEP-0071 is deprecated in practice). |
| CALLSPUSHPRESENCE-21 | Low | Missing feature | XEP-0377 | `blocking.rs` has no spam report. The block IQ has no `<report/>` child. | Add an optional "block and report as spam or abuse" action. |
| CALLSPUSHPRESENCE-22 | Low | UX gap | XEP-0191 | Desktop shows the blocklist and "Unblock" (`ContactRow.svelte:85`). Unverified: a "Block" action in the person menu for a room occupant or non-contact. `blocking.rs` accepts only `BareJid`, so a MUC occupant JID is not blockable as a full address. | Add Block to the person context menu. For MUC, block the occupant real JID when known, or use a local mute. |
| CALLSPUSHPRESENCE-23 | Low | Info | XEP-0115 | The caps list at `disco.rs:23-34` advertises 10 features. It omits features that Chord uses in code, such as reactions, corrections, retraction, markers, replies, and OOB (features exist in `chord-core/src/features/`). Unverified: whether peers change their behaviour by these flags. | Add the missing feature strings so peers can detect them. Any change to the list changes the caps hash. This is fine. |
| CALLSPUSHPRESENCE-24 | Info | Note | RFC 6121 | `roster.rs:460` ignores presence `error` and `probe`. `roster.rs:433` ignores our own other resources. This is correct for a client. | None. |

## Already done well

- XEP-0191 blocking: fetch at connect, push handling with a result reply, cache in `blocked_jids`, disco check, and offline list (`chord-core/src/features/blocking.rs:88-270`). CLI, FFI (`chord-ffi/src/client.rs:954-976`), and desktop (`commands.rs:731-748`) all expose it.
- XEP-0357 core API: enable with publish options, disable per node or per service, and a clear rule that the secret is never stored (`push.rs:1-6`, `222-260`). Tests exist in `chord-core/tests/push.rs`.
- Availability: away, dnd, xa, and status text are stored and sent in every presence and room join (`presence.rs:120-330`). The desktop status menu and app state use them (`StatusMenu.svelte`, `app.svelte.ts:778-810`).
- Invisible flow ordering: the list is active before the first presence. On failure the core falls back and warns the user (`presence.rs:246-262`).
- XEP-0115 caps in presence, XEP-0085 chat states, XEP-0313 MAM, and XEP-0280 carbons are advertised (`disco.rs:23-34`).
- Notification levels (all, mentions, none, mute until) apply in the core, and the desktop shows system notifications only when no window has focus (`notify.rs`, `chord-desktop/src-tauri/src/notify.rs`).

## Recommended next steps

1. Add XEP-0352 CSI in the core, wired to desktop focus and the FFI. About 0.5 day.
2. Add `set_presence` and `own_presence` to the FFI. About 0.5 day.
3. Add XEP-0215 external services and a WebRTC 1:1 audio call over Jingle with XEP-0353 (JMI). About 2 to 3 weeks.
4. Write the Android push glue (FCM or UnifiedPush plus an app server) and re-enable at each login. About 1 week.
5. Add a generic XEP-0004 form renderer, then XEP-0050 ad-hoc commands (slash commands and admin). About 3 to 4 days.
6. Check server support for privacy lists before you show Invisible, and add XEP-0186 first. About 1 day.
7. Add XEP-0319 idle and a PEP activity node (XEP-0118 tune or a custom node). About 2 days.
8. Choose a group call design (SFU or Muji) for voice channels, and add screen share. Design about 2 days. Build several weeks.
