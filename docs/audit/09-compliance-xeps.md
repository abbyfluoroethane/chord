# Overall XMPP compliance (XEP-0479 matrix, missing XEPs, disco#info audit)

## Summary

- The newest compliance suite on xmpp.org is XEP-0479 (Compliance Suites 2023). No 2024, 2025 or 2026 suite exists yet. I used XEP-0479. It supersedes XEP-0459.
- Chord meets most of the IM suite (Core and Advanced). Only XEP-0245 (/me) is missing from the IM Core level. XEP-0234/0261 (direct file transfer) is missing from the Advanced level.
- The Mobile suite is not met. XEP-0352 (Client State Indication) is a Core requirement and Chord does not have it. XEP-0198 (stream management) comes from tokio-xmpp. XEP-0357 (push) has client code.
- The A/V Calling suite is missing entirely (Jingle, XEP-0353, ICE-UDP, DTLS-SRTP, XEP-0215). Chord is a chat client, so this is a product choice. It is still a gap against the suite.
- Direct TLS (XEP-0368, Core Advanced) is missing. The library has `srv_xmpps`, but Chord only uses STARTTLS.
- The Chord disco#info answer is short (10 features). It leaves out many features that Chord supports (markers, corrections, reactions, replies, retraction, fallback, hints, `jabber:x:conference`). Other clients may hide those features when talking to Chord.
- The suite has no rule for end-to-end encryption. Every major client (Conversations, Gajim, Dino, Monal) supports OMEMO. Its absence is the largest gap in user impact.

## Method and limits

- Read XEP-0479 and the compliance page on xmpp.org. Checked XEP status on the xmpp.org extensions list.
- Searched chord-core, chord-desktop/src-tauri, chord-cli and chord-ffi for namespaces and module docs.
- I did not run any code. Behaviour inside tokio-xmpp 6.0.0 (stream management, SASL) is from the Chord docs and the library source. Anything else is marked Unverified.
- The XEP-0479 fetch returned version 0.1.0. The real document may have later versions with small changes. Unverified.

## Findings

| ID | Severity | Type | Spec | Evidence | What to do |
|---|---|---|---|---|---|
| COMPLIANCEXEPS-01 | High | Missing XEP | XEP-0352, Mobile Core | No match for `csi` or `client-state` in chord-core/src. tokio-xmpp is the only place with stream code | Send `<inactive/>` when the app is in the background and `<active/>` when it returns. Check the `urn:xmpp:csi:0` stream feature first. Needed for battery use on Android (chord-ffi) |
| COMPLIANCEXEPS-02 | High | Missing XEP | XEP-0245, IM Core | No `/me` handling in chord-core/src, chord-desktop/src or src-tauri | Show a body that starts with `/me ` as an action line ("* Alice waves"). Do not send anything special. Add a composer hint |
| COMPLIANCEXEPS-03 | Medium | Missing XEP | XEP-0368 + RFC 7590, Core Advanced | chord-core/src/session/mod.rs:21 `ServerAddr` has only `Srv`, `StartTls`. session/native.rs:96 uses `StartTlsServerConnector`. tokio-xmpp has `DirectTlsServerConnector` and `srv_xmpps` | Try `_xmpps-client._tcp` first. Fall back to STARTTLS. Add a `DirectTls` variant to `ServerAddr` |
| COMPLIANCEXEPS-04 | Medium | Missing XEP | XEP-0184, IM Advanced | markers.rs:1 and :11: "We never send `<received/>` markers". No `<request/>` on outgoing messages | Only the receive side exists. Decide the policy. Option: send `<request/>` on 1:1 messages, and answer requests when the user allows it. Then advertise `urn:xmpp:receipts` |
| COMPLIANCEXEPS-05 | Medium | Missing XEP | XEP-0234 + XEP-0261 (Jingle file transfer), IM Advanced | No `jingle` match in the repository | Low priority. XEP-0234 status is Deferred. HTTP upload (XEP-0363) covers most use. Prefer XEP-0447 (stateless file sharing) |
| COMPLIANCEXEPS-06 | Medium | Missing XEP | XEP-0166/0167/0176/0320/0353/0215, A/V Calling Core | No `jingle`, `extdisco`, `urn:xmpp:jingle-message` match | Large task (weeks). Needs a WebRTC stack. Plan it as a separate project. Do XEP-0215 first (small) if calls come later |
| COMPLIANCEXEPS-07 | Medium | Bug | XEP-0115 section 5, XEP-0030 | disco.rs:16-27 `FEATURES` list. See the disco table below | Add the features that Chord really handles. Other clients check them before they show buttons for edit, react, reply, retract |
| COMPLIANCEXEPS-08 | Medium | Missing XEP | XEP-0384 (OMEMO), not in suite | No `omemo` match | Rank 1 for user impact. Big task (weeks). Needs the libsignal-like crypto and PEP device lists |
| COMPLIANCEXEPS-09 | Low | Missing XEP | XEP-0092, XEP-0202, XEP-0012 | No `jabber:iq:version`, `urn:xmpp:time` or `jabber:iq:last` in chord-core/src. features/mod.rs:296-313 answers unknown IQs with `service-unavailable` | Compliant as it is (RFC 6120 8.2.3). Add version and time replies only if you want them. Version is useful for bug reports. Make the version reply optional for privacy |
| COMPLIANCEXEPS-10 | Low | Spec violation | XEP-0030 section 3.1 | disco.rs:203 `info(query.node)` echoes any node with the same feature list | For an unknown node, return `item-not-found`. Only the node `CAPS_NODE#ver` should work |
| COMPLIANCEXEPS-11 | Low | Spec violation | XEP-0030 section 4 | features/mod.rs:296-313 has no `disco#items` reply to us | An entity that supports disco SHOULD answer disco#items with an empty list. Add an empty result |
| COMPLIANCEXEPS-12 | Low | Security | RFC 6120 section 6, RFC 7590 | native.rs:534 mechanisms list: SCRAM-SHA-256, SCRAM-SHA-1, PLAIN, ANONYMOUS. native.rs:463-465 says `-PLUS` binding falls back to PLAIN | Remove ANONYMOUS from a password login. Refuse PLAIN downgrade when the server also offered SCRAM. Unverified: whether the code detects a downgrade by a bad server |
| COMPLIANCEXEPS-13 | Low | Missing XEP | XEP-0388 (SASL2), XEP-0386 (Bind 2), XEP-0484 (FAST) | No `sasl2` or `bind2` in chord-core/src. tokio-xmpp 6.0.0 login is used | Not in XEP-0479. Newer servers (Prosody 13, ejabberd 25+) support these. They reduce login round trips and allow token login. Depends on tokio-xmpp support. Unverified |
| COMPLIANCEXEPS-14 | Low | UX gap | XEP-0372, XEP-0393 | No `references` or `styling` namespace in chord-core/src. Only `urn:xmpp:fallback:0` and reply fallback exist | Add XEP-0393 message styling (bold, code, quote) in the timeline. Add XEP-0372 mentions for channel chat. Both are common in Discord-style use |
| COMPLIANCEXEPS-15 | Info | Info | XEP-0424 status | XEP-0424 is "Proposed". XEP-0425 and XEP-0444 and XEP-0461 and XEP-0428 are "Experimental". XEP-0359 is "Experimental" | Not a defect. Track for spec changes. Chord uses `message-retract:1` and reads the older `fasten:0` form and `moderate:0` (retraction.rs:30-37) |
| COMPLIANCEXEPS-16 | Info | Info | XEP-0016, XEP-0126 | presence.rs:7-9, :82 use privacy lists for invisible mode. Both XEPs are "Deprecated" | Works on ejabberd and Prosody today. No standard replacement exists. Consider a plain "appear offline" with directed presence, or drop the feature |
| COMPLIANCEXEPS-17 | Info | Info | XEP-0153, XEP-0054 | avatars.rs:25 uses `vcard-temp:x:update`. The xmpp.org list shows both as "Active" (Historical type) | Keep for compatibility. XEP-0084 is the main path. XEP-0398 covers the conversion on the server side |
| COMPLIANCEXEPS-18 | Info | Info | XEP-0422 | retraction.rs:32 `NS_FASTEN` = `urn:xmpp:fasten:0`. XEP-0422 is "Deferred" | Read only, for old messages. Do not write it. Correct as is |

## Compliance matrix (XEP-0479, Client)

Level "C" = Core, "A" = Advanced. "C+A" means the XEP is required at both levels.

### Core category

| XEP | Title | Suite category/level | Status in Chord | Evidence file | Notes |
|---|---|---|---|---|---|
| RFC 6120 | XMPP Core | Core C+A | Done | chord-core/src/session/native.rs | Login by tokio-xmpp with own SASL loop. SCRAM-SHA-256, SCRAM-SHA-1, PLAIN. Error handling for TLS and SASL is typed |
| RFC 7590 | TLS for XMPP | Core C+A | Done | session/native.rs:27, :550-565 | rustls. Invalid certificate gives `TlsInvalid`. No plain TCP except the `dev-insecure` feature |
| XEP-0368 | SRV records for XMPP over TLS | Core A | Missing | session/mod.rs:21, native.rs:96 | STARTTLS only. See COMPLIANCEXEPS-03 |
| XEP-0030 | Service Discovery | Core C+A | Done | features/disco.rs | Client side (server, account, item scan) and answer side. Small gaps: COMPLIANCEXEPS-10, -11 |
| XEP-0115 | Entity Capabilities | Core C+A | Done | features/disco.rs:41-45, presence.rs:290 | SHA-1 hash in presence and in room joins (muc.rs:1175). Unverified: whether Chord checks the hash of received caps before it trusts them |
| XEP-0163 | Personal Eventing Protocol | Core A | Partial | features/bookmarks.rs, avatars.rs, pubsub.rs | Chord uses PEP for bookmarks and avatars with `+notify`. It has no PEP nick, no user tune, no user location. The suite only asks for the base protocol, so this is enough |

### IM category

| XEP | Title | Suite category/level | Status in Chord | Evidence file | Notes |
|---|---|---|---|---|---|
| RFC 6121 | XMPP IM | IM C+A | Done | features/roster.rs, presence.rs | Roster with versioning (`ver`), subscriptions, presence. Roster tests at roster.rs:859+ |
| XEP-0245 | The /me Command | IM C+A | Missing | none | COMPLIANCEXEPS-02 |
| XEP-0084 | User Avatar | IM A | Done | features/avatars.rs (2620 lines) | Metadata `+notify`, data fetch, publish. Also spaces avatars |
| XEP-0054 | vcard-temp | IM C+A | Done | features/avatars.rs:1722+ | Fetch and set of vCard photo and fields. Fallback for contacts with no XEP-0084 |
| XEP-0280 | Message Carbons | IM C+A | Done | features/carbons.rs | Enable at each session. Forged carbons dropped (section 11). Enable failure is only logged |
| XEP-0045 | Multi-User Chat | IM C+A | Done | features/muc.rs (3683 lines) | Join, leave, nick change, occupants, private messages, mediated invites, config |
| XEP-0249 | Direct MUC Invitations | IM C+A | Done | muc.rs:37 `jabber:x:conference`, :894 | Receive is done. Unverified: whether Chord can send one |
| XEP-0313 | Message Archive Management | IM A | Done | features/mam.rs | Account archive and room archive. Catch-up and history. Query id and sender checks (mam.rs:8-9). RSM paging |
| XEP-0363 | HTTP File Upload | IM C+A | Done | features/upload.rs | Slot request, HTTP PUT in runtime, URL message with OOB. Works for rooms. Waits for disco |
| XEP-0184 | Message Delivery Receipts | IM A | Partial | features/markers.rs:1, :31, :181, :209 | Receive only. It never sends `<request/>` or `<received/>`. COMPLIANCEXEPS-04 |
| XEP-0085 | Chat State Notifications | IM A | Done | features/chat_states.rs | Full rules of sections 5.1, 5.4, 5.5. Typing expiry |
| XEP-0308 | Last Message Correction | IM A | Done | features/corrections.rs | Incoming and outgoing. Not advertised (COMPLIANCEXEPS-07) |
| XEP-0234 + XEP-0261 | Jingle File Transfer | IM A | Missing | none | COMPLIANCEXEPS-05 |

### Mobile category

| XEP | Title | Suite category/level | Status in Chord | Evidence file | Notes |
|---|---|---|---|---|---|
| XEP-0198 | Stream Management | Mobile C+A | Done (library) | session/native.rs:3, :363-401 | `StanzaStream` from tokio-xmpp owns SM and resume. Chord reports `resumed` in `SessionEvent::Connected` and skips the setup on resume (actor.rs:737) |
| XEP-0352 | Client State Indication | Mobile C+A | Missing | none | COMPLIANCEXEPS-01 |
| XEP-0357 | Push Notifications | Mobile A | Done (client side) | features/push.rs | Enable and disable of an app server. The publish-options secret is never stored. The server-side app server is outside chord-core. Unverified: end-to-end test with a real app server |

### A/V Calling category

| XEP | Title | Suite category/level | Status in Chord | Evidence file | Notes |
|---|---|---|---|---|---|
| XEP-0167 + XEP-0353 | Jingle RTP, Jingle Message Initiation | A/V C+A | Missing | none | COMPLIANCEXEPS-06 |
| XEP-0176 | ICE-UDP Transport | A/V C+A | Missing | none | |
| XEP-0320 | DTLS-SRTP in Jingle | A/V C+A | Missing | none | |
| XEP-0215 | External Service Discovery (STUN/TURN) | A/V C+A | Missing | none | Small task on its own |
| XEP-0293, 0294, 0338, 0339 | RTP feedback, header extensions, grouping, source attributes | A/V A | Missing | none | |

### Web category

Skipped as asked.

## Other XEPs that Chord uses (not in the suite)

| XEP | Title | Status in Chord | Evidence file | XEP status on xmpp.org |
|---|---|---|---|---|
| XEP-0060 | Publish-Subscribe | Done | features/pubsub.rs, spaces.rs | Stable |
| XEP-0402 | PEP Native Bookmarks | Done | features/bookmarks.rs | Stable |
| XEP-0333 | Chat Markers (displayed) | Done | features/markers.rs | Stable |
| XEP-0334 | Message Processing Hints | Partial (`<store/>` only) | markers.rs:33, reactions.rs:22, retraction.rs:37 | Stable |
| XEP-0359 | Unique and Stable Stanza IDs | Done | muc.rs:1500-1768, mam.rs | Experimental |
| XEP-0421 | Occupant Identifiers | Done (read) | muc.rs:33-34 | Stable |
| XEP-0424 | Message Retraction | Done | features/retraction.rs | Proposed |
| XEP-0425 | Moderated Message Retraction | Done | retraction.rs:7-9 | Experimental |
| XEP-0444 | Message Reactions | Done | features/reactions.rs | Experimental |
| XEP-0461 | Message Replies | Done | features/replies.rs | Experimental |
| XEP-0428 | Fallback Indication | Done | replies.rs:1-8 | Experimental |
| XEP-0066 | Out of Band Data | Done (URL messages) | message_ext.rs:6 | Stable |
| XEP-0191 | Blocking Command | Done | features/blocking.rs | Stable |
| XEP-0203 | Delayed Delivery | Done | message_ext.rs | Final |
| XEP-0004 | Data Forms | Done | via xmpp-parsers, used in muc, push | Draft |
| XEP-0059 | Result Set Management | Done | mam.rs, spaces | Draft |
| XEP-0199 | XMPP Ping | Done (answer only) | features/mod.rs:317-326 | Draft |
| XEP-0016, 0126 | Privacy Lists, Invisibility | Done (invisible mode) | presence.rs:7-9, :82 | Deprecated (both) |
| XEP-0153 | vCard-Based Avatars | Done | avatars.rs:25 | Active (Historical) |
| Custom | `urn:xmpp:spaces:0` | Done | features/spaces.rs | Chord own protocol. Not a XEP. Uses `urn:xmpp:pubsub-filter:0` and `urn:xmpp:pubsub-ext-disco`. Check that these are documented for other clients |

## Missing XEPs that other clients support, ranked for a Discord-style client

| Rank | XEP | Why it matters | Effort |
|---|---|---|---|
| 1 | XEP-0384 OMEMO (with XEP-0420 SCE) | Private chats need E2E encryption. Every major client has it | Weeks |
| 2 | XEP-0352 CSI | Battery and data use on mobile. Also cuts noise on big rooms | Hours |
| 3 | XEP-0393 Message Styling | Bold, code, quote. Users expect Markdown-like text | Days |
| 4 | XEP-0372 References (mentions) | @mention with notification. Core to channel chat | Days |
| 5 | XEP-0447 Stateless File Sharing, XEP-0446, XEP-0385 (SIMS) | Rich media cards, file names and sizes, thumbnails, image preview. Movim, Dino and Conversations use them | Days |
| 6 | XEP-0245 /me | Small. Part of the IM Core suite | Hours |
| 7 | XEP-0369 MIX, XEP-0405, XEP-0403 | Not widely deployed. Skip for now. Chord uses MUC and spaces | n/a |
| 8 | XEP-0164 / XEP-0369 mucsub (`urn:xmpp:mucsub:0`) | Lets a client get room messages while offline. Prosody and ejabberd support it. Good for Mobile | Days |
| 9 | XEP-0388 SASL2, XEP-0386 Bind 2, XEP-0484 FAST | Faster login, token auth. Depends on tokio-xmpp | Days (Unverified) |
| 10 | XEP-0167/0166 calls with XEP-0353 | Voice and video. Discord users expect voice channels | Weeks |
| 11 | XEP-0077 In-band registration, XEP-0158 CAPTCHA | Sign-up inside the app. Not found in chord-core | Days |
| 12 | XEP-0313 MAM preferences and search (XEP-0431 full text search) | Search in history. Check whether the UI has search. Unverified | Days |
| 13 | XEP-0092 version, XEP-0202 time | Bug reports and tools. Small | Hours |
| 14 | XEP-0300 Cryptographic hash and XEP-0231 BoB | Inline stickers and custom emoji. Discord users expect custom emoji | Days |
| 15 | XEP-0264/0234 thumbnails | Only with Jingle. Skip | n/a |
| 16 | XEP-0463 MUC affiliations versioning, XEP-0410 self-ping | Better MUC reconnect. Self-ping (XEP-0410) helps to check that we are still in a room after a network change. Unverified whether Chord already checks | Hours |

## Namespaces that Chord advertises in disco#info

Source: chord-core/src/features/disco.rs:16-27 (`FEATURES`). Identity: `client/pc/en/Chord`. The caps hash is SHA-1.

| Advertised namespace | Really implemented? | Note |
|---|---|---|
| `http://jabber.org/protocol/disco#info` | Yes | disco.rs:196 |
| `http://jabber.org/protocol/caps` | Yes | Caps in presence |
| `urn:xmpp:ping` | Yes | features/mod.rs:317 |
| `urn:xmpp:carbons:2` | Yes, client enables it | A client does not need to advertise this. Harmless |
| `urn:xmpp:sid:0` | Yes | Stanza and origin ids are used |
| `urn:xmpp:mam:2` | Client only | MAM is a server feature. A client does not answer MAM queries. Remove it. It can confuse a peer that looks for archive support |
| `http://jabber.org/protocol/muc` | Yes | Correct for a MUC client |
| `urn:xmpp:bookmarks:1+notify` | Yes | Correct form for PEP notify (XEP-0402) |
| `urn:xmpp:avatar:metadata+notify` | Yes | Correct form (XEP-0084) |
| `http://jabber.org/protocol/chatstates` | Yes | chat_states.rs |

Implemented but not advertised:

| Namespace | Feature | Where implemented | Effect of the gap |
|---|---|---|---|
| `urn:xmpp:chat-markers:0` | XEP-0333 | markers.rs | Peers may not send markers or may hide read state |
| `urn:xmpp:message-correct:0` | XEP-0308 | corrections.rs | Peers may not offer edit. Gajim and Conversations check this |
| `urn:xmpp:reactions:0` | XEP-0444 | reactions.rs | Peers may not offer reactions in 1:1 chats |
| `urn:xmpp:reply:0` | XEP-0461 | replies.rs | Same for replies |
| `urn:xmpp:message-retract:1` | XEP-0424 | retraction.rs | Peers may not offer delete |
| `urn:xmpp:fallback:0` | XEP-0428 | replies.rs | Low |
| `urn:xmpp:hints` | XEP-0334 | markers.rs:390 | Low. Chord only sends, so advertising is optional |
| `jabber:x:conference` | XEP-0249 | muc.rs:37 | Peers check this before they send a direct invite |
| `urn:xmpp:receipts` | XEP-0184 | markers.rs:181 | Correct not to advertise. Chord does not answer requests (COMPLIANCEXEPS-04) |
| `urn:xmpp:occupant-id:0` | XEP-0421 | muc.rs:33 | Server and room feature. Do not advertise |
| `http://jabber.org/protocol/nick+notify` | XEP-0172 | not implemented | n/a |
| `urn:xmpp:http:upload:0` | XEP-0363 | upload.rs | Server feature. Do not advertise |
| `urn:xmpp:spaces:0` | Chord spaces | spaces.rs | Chord own feature. Consider advertising a Chord specific feature so Chord clients find each other. Unverified use |
| `http://jabber.org/protocol/disco#items` | XEP-0030 | not answered | Advertise only after adding COMPLIANCEXEPS-11 |

A change to `FEATURES` changes the caps hash. The test at disco.rs:317 only checks that the hash is stable. It does not compare a known hash. Add a fixed test vector from XEP-0115 section 5.2 or a golden value.

## Already done well

- Carbon forgery check: features/carbons.rs:22-40.
- MAM anti-forgery checks and cursor design: features/mam.rs:1-20.
- Chat state rules follow XEP-0085 sections 5.1, 5.4, 5.5: features/chat_states.rs:1-13.
- Privacy-first markers policy (no automatic `<received/>`): markers.rs:11.
- Retraction with XEP-0425 room origin check, and the older `fasten:0` form for reading: features/retraction.rs:1-37.
- Every IQ gets an answer (RFC 6120 8.2.3): features/mod.rs:296-313.
- Push (XEP-0357) never stores the app server secret: features/push.rs:1-6.
- Roster versioning and a hostile `ver` test: features/roster.rs:1014.
- Stream management and resume through tokio-xmpp, with a guard for the missing JID after resume: session/native.rs:363-401.
- SCRAM with channel binding logic and a documented fallback: session/native.rs:463-534.
- Avatar handling covers XEP-0084, vcard-temp and `vcard-temp:x:update`, with image type checks in tests: features/avatars.rs.

## Recommended next steps

1. Fix `FEATURES` in disco.rs. Add markers, correction, reactions, reply, retract, `jabber:x:conference`. Remove `urn:xmpp:mam:2`. Add a golden caps hash test. Effort: 2 hours.
2. Add XEP-0352 CSI. Wire it to the desktop focus event and the Android lifecycle. Effort: 3 to 5 hours.
3. Add XEP-0245 /me in the timeline. Effort: 2 hours.
4. Add Direct TLS (XEP-0368) with STARTTLS fallback. Effort: 4 to 8 hours.
5. Decide the XEP-0184 receipt policy and implement it. Effort: 4 hours.
6. Add XEP-0393 styling and XEP-0372 mentions. Effort: 2 to 4 days.
7. Plan OMEMO (XEP-0384 and XEP-0420). Write a design first. Effort: 1 day for the design, 2 to 4 weeks to build.
8. Plan A/V calls (Jingle, XEP-0353, XEP-0215, WebRTC). Only if voice channels are a goal. Effort: several weeks.
