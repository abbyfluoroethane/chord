# One-to-one messaging extensions

Scope: chord-core/src/features/{chat,message_ext,mam,carbons,corrections,retraction,reactions,replies,markers,chat_states,upload}.rs, the store queries, and the desktop markdown parser. Read only. No build or test was run. Line numbers are from the current main.

## Summary
- The disco feature list in disco.rs:23 does not advertise chat markers, corrections, retraction, reactions, replies, fallback, OOB, hints or receipts. Other clients check these before they send. Chord loses reactions, markers and replies from careful clients.
- Chord drops any chat message that has no stanza-id and no origin-id (chat.rs:106). A server or client without XEP-0359 causes silent message loss.
- Messages of type `error` (bounces) are dropped in 1:1 chats (chat.rs:50). The user never sees a failed send. The `status` column has no `failed` value.
- Corrections, retractions and reactions that arrive before their target are lost. This happens with MAM backward paging. The target then shows with old text.
- XEP-0184 receipts are read but never sent or requested. XEP-0446/0447 file sharing, XEP-0393 styling and XEP-0334 hints on chat states are absent. Discord-style markdown reads differently in XEP-0393 clients.
- Security checks are good: carbons, MAM results, stanza-id `by`, correction and retraction sender checks all exist.

## Findings

| ID | Severity | Type | Spec | Evidence | What to do |
|---|---|---|---|---|---|
| MESSAGING-01 | High | Missing XEP | XEP-0115, XEP-0030 | disco.rs:23-34 `FEATURES` has only carbons, sid, mam, muc, chatstates, bookmarks, avatar | Add `urn:xmpp:chat-markers:0`, `urn:xmpp:message-correct:0`, `urn:xmpp:message-retract:1`, `urn:xmpp:reactions:0`, `urn:xmpp:reply:0`, `urn:xmpp:fallback:0`, `jabber:x:oob`, `urn:xmpp:hints`, and `urn:xmpp:receipts` when 0184 is done. Caps hash changes on its own. |
| MESSAGING-02 | High | Bug | XEP-0359, RFC 6121 | chat.rs:104-108 "has no stanza-id or origin-id. Not stored." | Fall back to a local key: the `id` attribute with the sender JID, or a random local key. Keep dedup by stanza-id when it exists. Unverified: how often real servers omit both ids. ejabberd and Prosody with MAM add stanza-id. |
| MESSAGING-03 | High | Bug | RFC 6121 8.5, RFC 6120 8.3 | chat.rs:50 only `Chat` and `Normal` are stored; no `MessageType::Error` handling for 1:1; store schema.rs:157 status has no `failed` | Parse the error message. Match its `id` to a sent message. Set status `failed` and show a retry state in the UI. |
| MESSAGING-04 | Medium | Bug | XEP-0308, XEP-0424, XEP-0444 | mam.rs:333 backward paging; retraction.rs:292 "dropped: no such message"; reactions.rs:231 "dropped"; corrections.rs:187 unknown target becomes a new row | Keep orphan edits, retractions and reactions in a small table by target id. Apply them when the target is stored. Or run the extension step after a whole MAM page is stored, in archive order. |
| MESSAGING-05 | Medium | Missing feature | XEP-0184 | markers.rs:11 "We never send `<received/>`"; markers.rs:181 only reads receipts; no `<request/>` sent; no `urn:xmpp:receipts` in disco | Add an option: send `<received/>` for incoming `<request/>` (1:1 only, not for strangers). Send `<request/>` on outgoing messages. Many clients (Conversations, Dino, Monal) show delivery from 0184 only. |
| MESSAGING-06 | Medium | Missing XEP | XEP-0446, XEP-0447 | upload.rs:333 sends body URL plus OOB only; store keeps `oob_url` only | Send `<file-sharing>` with `<file>` metadata (name, size, media-type, hash) and the HTTP source. Keep OOB for old clients. Read incoming SFS. Add `urn:xmpp:sfs:0` to disco. XEP-0264 thumbnails and dimensions are optional. |
| MESSAGING-07 | Medium | UX gap | XEP-0393, XEP-0394 | markdown.ts:195 `~` needs two; markdown.ts:297 `*x*` is italic; Discord `__x__` is underline | Peers that use XEP-0393 send `*bold*` and `~strike~`. Chord shows italic and plain text. Add a mode for XEP-0393 rules on incoming text, or a setting. Send `<unstyled xmlns="urn:xmpp:styling:0"/>` if Chord will not send XEP-0393 text. XEP-0393 clients render our `**x**` and `__x__` differently. Document the trade-off. XEP-0394 is not used (fine). |
| MESSAGING-08 | Medium | Bug | XEP-0333 5 | markers.rs:217 `Direction::In` sets `displayed` on all our messages up to the marker; also applied for groupchat | In a room, mark only when the marker sender is the peer of a 1:1 chat. Or track the displayed state per occupant. A room with many members should not turn "displayed" on for one reader. Also add a setting to turn off sent read markers (markers.rs:332 sends on every `mark_read`). |
| MESSAGING-09 | Medium | Bug | XEP-0308 | corrections.rs:130 any own message can be edited; Message.svelte offers edit for any own message | XEP-0308 section 3 says only the last message. Limit the UI to the last own message of the chat, or accept the risk. On receive, this is fine. Note: same-sender uses the bare JID (corrections.rs:85), which is right for carbons and multi-device. |
| MESSAGING-10 | Medium | Bug | XEP-0363 | upload.rs:24 `data: Vec<u8>`; runtime.rs:29 body is `Vec<u8>` | Whole file sits in memory, and again when it is copied to the request. Stream from a file path. Add progress and cancel. |
| MESSAGING-11 | Low | Bug | XEP-0363 4.3 | upload.rs:210 `describe` ignores `<file-too-large><max-file-size/>` and `<retry stamp>` | Show the max size from the error. Retry after the stamp. |
| MESSAGING-12 | Low | Bug | XEP-0334 | chat_states.rs:268 state message has no hint; no `<no-store/>` | Add `<no-store/>` and `<no-permanent-store/>` to chat state messages. Add `<no-store/>` to markers is not needed (they use `<store/>`). Servers can archive state-only messages. |
| MESSAGING-13 | Low | Bug | XEP-0085 5.1 | chat_states.rs: no `gone` state is ever sent; `supported` is per session | Send `gone` when the user closes a chat (optional). Fine as is for most users. |
| MESSAGING-14 | Low | Bug | XEP-0359 | reactions.rs:192, corrections.rs:124 | Reactions message has no origin-id; the other extension messages have one. Add it for consistency. |
| MESSAGING-15 | Low | Bug | XEP-0424 6 | corrections.rs:75 `original_id` prefers `id` attribute over origin-id; retraction.rs:123 uses it | XEP-0424 asks for origin-id in 1:1 when there is one. Our own sent messages have both equal. Messages sent by another device (via carbon or MAM) can differ. Unverified for peers: find_message matches all three ids, so receive is safe. |
| MESSAGING-16 | Low | Bug | XEP-0313 | mam.rs:303 an error other than `item-not-found` leaves the cursor as is; no retry until next connect. No check of `urn:xmpp:mam:2` support from disco. No MAM preferences (section 6). | Retry with back off. Check server support before the query. Optional: read and show MAM prefs. |
| MESSAGING-17 | Low | Bug | XEP-0313 | mam.rs: `newest_id` only moves in MAM pages. Live messages do not move it | Each reconnect refetches all messages since the last MAM sync. Dedup keeps data right, but the load grows with session length. Update `newest_id` for live messages that carry a stanza-id by the account. |
| MESSAGING-18 | Low | UX gap | XEP-0428 | replies.rs:97 strips only the reply fallback | A fallback for other features (SFS, retract from other clients) shows in the body. Strip any `<fallback for=...>` range that Chord understands. |
| MESSAGING-19 | Low | UX gap | XEP-0461 | replies.rs:161 quotes `row.body`, not `edited_body` | A reply to an edited message quotes the old text. Use the edited text. |
| MESSAGING-20 | Low | Bug | XEP-0280 | carbons.rs:24-28 accepts carbon with no `from`; `on_enabled` only logs errors | Fine per RFC 6120 (server-set). No disco check for `urn:xmpp:carbons:2` before enable. Show no error. Info. |
| MESSAGING-21 | Info | Missing feature | RFC 6121 offline | chat.rs; delay is read via `delay_ms` | Offline messages arrive as normal messages with XEP-0203 delay. Timestamp is used. OK. Messages from `headline` type are ignored. |
| MESSAGING-22 | Info | Missing feature | XEP-0066 | message_ext.rs:70 stores only the first OOB URL | Incoming OOB with `<desc>` is not kept. Multiple attachments are lost. Low value. |
| MESSAGING-23 | Info | Missing XEP | XEP-0334 incoming | no handling of `<no-store/>`, `<no-copy/>`, `<private/>` on send | Add `<private/>` (XEP-0280) for messages that must not be copied, if the user asks for it. |
| MESSAGING-24 | Info | Missing XEP | XEP-0481, XEP-0482, XEP-0308 v2, XEP-0353 | grep finds none | Not needed now. XEP-0481 (content types) and XEP-0482 (call invites) have low value for a chat client without calls. |
| MESSAGING-25 | Info | Bug | doc | upload.rs:67 doc says "and for a room" fails, but the code at upload.rs:337 sends to a room | Fix the doc comment. |

## Already done well
- Carbons (XEP-0280): enabled on each new session (carbons.rs:14); forged carbons are dropped when `from` is not our bare JID (carbons.rs:24-41); delay is kept from the forwarded element. Tests exist.
- MAM (XEP-0313): RSM paging with `after` and empty `before` (mam.rs `send_query`); random queryid; result must match a running query and come from our account or the room (mam.rs `on_result`); `with` filter for one chat; cursor recovery on `item-not-found`; `complete` plus first index check for start of history.
- XEP-0359: origin-id is sent on every message (chat.rs:`send_message`). The stanza-id counts only when `by` is our account or the room (chat.rs `MessageIds::of`). A message stored under its origin-id is upgraded to the stanza-id (chat.rs:82-100). This gives correct dedup between live, carbon and MAM copies.
- XEP-0308: sender check (corrections.rs:81), retracted target ignored, unknown target becomes a new message per section 3.1, newer edit wins by `edited_at`.
- XEP-0424 and XEP-0425: v1 and old fasten forms are read; moderation only from the room bare JID (retraction.rs:260); own retraction sends fallback body and `<store/>`; moderation IQ with reason.
- XEP-0444: replace-set semantics, per-sender rows, limits on emoji count and bytes, sender taken from the stanza, not from the payload (reactions.rs:243).
- XEP-0461 and XEP-0428: reply with `to` and `id`, quote fallback with code point range, strip on receive (replies.rs).
- XEP-0333: `<markable/>` on all outgoing messages; displayed marker sent on read; markers with a body still stored; own displayed markers move the read position across devices.
- XEP-0085: `<active/>` on messages, states only after the peer sent a state, no repeat, typing expiry, no state from own carbons.
- XEP-0363: HTTPS-only slot URLs (localhost excepted), no redirect, `max-file-size` checked before the request, only allowed headers pass.

## Recommended next steps
1. Add all missing feature namespaces to disco (MESSAGING-01). About 1 hour, plus check that the caps hash test still passes.
2. Handle `type=error` messages and add a `failed` status with UI (MESSAGING-03). About 1 day.
3. Add a fallback message key for messages with no stanza-id or origin-id (MESSAGING-02). About 3 hours.
4. Keep orphan corrections, retractions and reactions until the target arrives (MESSAGING-04). About 1 day.
5. Add XEP-0184 send and request as a setting (MESSAGING-05). About 0.5 day.
6. Stream uploads from disk with progress and better slot errors (MESSAGING-10, -11). About 1 day.
7. Add XEP-0446/0447 file sharing with OOB fallback (MESSAGING-06). About 2 days.
8. Decide the XEP-0393 policy: render incoming XEP-0393 spans, and add `<unstyled/>` or a plain-text mode (MESSAGING-07). About 1 day.
