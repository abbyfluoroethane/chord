# Chord audit tracker

This file tracks the fixes from the audit of 2026-09-30. Nine read-only agents checked the code at commit `97275ff` against the XMPP RFCs and XEPs. The details for each area are in the other files in this folder.

**GitHub issues:** each finding, large feature and decision has an issue: #1 to #200, all with the label `audit`. The issues are the place to track work. This file is an index. When an issue closes, tick its box here too.

## Status

| Area | Report | High | Medium | Low | Info | Done |
|---|---|---|---|---|---|---|
| RFC 6120 and RFC 6121 core session audit | [01-core-session.md](01-core-session.md) | 1 | 7 | 10 | 4 | 8 |
| One-to-one messaging extensions | [02-messaging.md](02-messaging.md) | 3 | 7 | 10 | 5 | 9 |
| XEP-0045 multi-user chat and related | [03-muc.md](03-muc.md) | 2 | 9 | 11 | 2 | 11 |
| XEP-0503 Spaces, Pubsub, Avatars, Profiles, Bookmarks, and xmpp: URIs | [04-spaces-pubsub.md](04-spaces-pubsub.md) | 0 | 7 | 20 | 3 | 6 |
| End-to-end encryption and modern auth | [05-security-auth.md](05-security-auth.md) | 1 | 15 | 8 | 1 | 3 |
| Discord-parity real-time features and mobile (calls, push, presence, forms) | [06-calls-push-presence.md](06-calls-push-presence.md) | 6 | 10 | 7 | 1 | 13 |
| Desktop app functional gaps (chord-desktop) | [07-desktop-gaps.md](07-desktop-gaps.md) | 2 | 10 | 16 | 2 | 12 |
| Security review of the desktop bridge and UI | [08-bridge-security.md](08-bridge-security.md) | 2 | 6 | 9 | 3 | 7 |
| Overall XMPP compliance (XEP-0479 matrix, missing XEPs, disco#info audit) | [09-compliance-xeps.md](09-compliance-xeps.md) | 2 | 6 | 6 | 4 | 5 |
| **All** | | **19** | **77** | **97** | **25** | **74** |

No finding is Critical. Update the Done column when you tick boxes.

On 2026-09-30, all 19 High findings closed. The Android app work of CALLSPUSHPRESENCE-07 (#118) moved to #198. Later that night, 55 of the 77 Medium findings closed. The 22 open Medium findings wait for an epic (OMEMO #194, sign-in #196, calls #195, Android #198), for a decision (#199, #200), or have a partial fix with the rest named in the issue. The plans for OMEMO and for calls are in [../omemo-plan.md](../omemo-plan.md) and [../calls-plan.md](../calls-plan.md).

## Decisions needed

- [ ] **Markdown and XEP-0393.** Chord uses Discord rules for `*`, `_` and `~`. XEP-0393 message styling uses other rules, so other clients show some Chord messages with the wrong style. Choose one: keep Discord rules, switch to XEP-0393, or send only the forms where the two agree. See MESSAGING-07. Issue #199.
- [ ] **Invite cards.** A card asks the server in the link for data as soon as it shows. Choose one: ask only after a click, or keep the current way. See SPACESPUBSUB-02. Issue #200.

## Plan

### Wave 1: bugs and security (about 1 to 2 days)

- [x] Accept only file paths that the file dialog returned in `upload` and `save_image` (BRIDGESECURITY-01, BRIDGESECURITY-02).
- [x] Keep a 1:1 message that has no stanza-id and no origin-id (MESSAGING-02).
- [x] Show a bounced message (`type=error`) as failed, with a retry (MESSAGING-03).
- [x] Add and remove room bookmarks from the desktop app, so a room that the user leaves stays gone (MUC-01, MUC-02).
- [x] Save the display name for real, or remove the field (SPACESPUBSUB-05).
- [x] Make the header search work, or remove it until it works (DESKTOPGAPS-01).
- [x] Read the five notification settings in the app and the bridge (DESKTOPGAPS-02).

### Wave 2: compliance quick wins (about 2 to 3 days)

- [x] Advertise the features that Chord implements in disco#info and caps: markers, corrections, retraction, reactions, replies, fallback, OOB, hints, direct invites. Stop advertising MAM (MESSAGING-01, CORESESSION-08, COMPLIANCEXEPS-07).
- [x] Send a roster version only when the server offers `rosterver` (CORESESSION-01, RFC 6121 2.6.2).
- [x] Add Client State Indication, XEP-0352 (CALLSPUSHPRESENCE-06, COMPLIANCEXEPS-01).
- [x] Add client ping, XEP-0199, and shorter dead-connection detection (CORESESSION-06, CORESESSION-07).
- [x] Show `/me` messages as actions, XEP-0245 (COMPLIANCEXEPS-02).
- [x] Send delivery receipts, XEP-0184 (MESSAGING-05).
- [x] Add direct TLS and the `_xmpps-client` SRV lookup, XEP-0368 (CORESESSION-02, COMPLIANCEXEPS-03).
- [x] Add MUC self-ping and a join timeout, XEP-0410 (MUC-03).

### Large features (weeks each)

- [ ] End-to-end encryption: OMEMO 0.8 (XEP-0384) with XEP-0420 and XEP-0454, and trust management. About 3 to 5 weeks (SECURITYAUTH-01). Issue #194.
- [ ] Voice and video calls: Jingle (XEP-0166, XEP-0167, XEP-0176, XEP-0320), XEP-0353 and XEP-0215. Several weeks for 1:1 audio (CALLSPUSHPRESENCE-01 to CALLSPUSHPRESENCE-03). Issue #195.
- [ ] Modern sign-in: SASL2, Bind2, FAST and ISR (XEP-0388, XEP-0386, XEP-0484, XEP-0397). About 1 to 2 weeks (SECURITYAUTH-07 to SECURITYAUTH-12). Issue #196.
- [ ] Accounts and invites: in-band registration and password change (XEP-0077), invites (XEP-0401, XEP-0379). About 1 week (SECURITYAUTH-18 to SECURITYAUTH-22). Issue #197.
- [ ] Android push: connect the app to the XEP-0357 code in the core, and add presence calls to the FFI (CALLSPUSHPRESENCE-07, CALLSPUSHPRESENCE-13). Issue #198.

## All findings

One line for each finding: the ID, the severity, the type, the spec, and the first step of the fix. The area report has the evidence with file and line.

### RFC 6120 and RFC 6121 core session audit

Report: [01-core-session.md](01-core-session.md)

**High**

- [x] **CORESESSION-01** ([#1](https://github.com/abbyfluoroethane/chord/issues/1)) (Spec violation, RFC 6121 2.6.2): Read the `rosterver` namespace in `stream_features`.

**Medium**

- [x] **CORESESSION-02** ([#2](https://github.com/abbyfluoroethane/chord/issues/2)) (Missing XEP, XEP-0368, RFC 7590): Add a `ServerAddr::DirectTls` variant.
- [x] **CORESESSION-03** ([#3](https://github.com/abbyfluoroethane/chord/issues/3)) (Bug (Unverified interop risk), XEP-0440): If a server lists only `tls-unique` in `sasl-channel-binding`, but offers -PLUS, the attempt can fail with `not-authorized`.
- [x] **CORESESSION-06** ([#6](https://github.com/abbyfluoroethane/chord/issues/6)) (Missing XEP, XEP-0352): Add a `set_client_state(active/inactive)` command.
- [x] **CORESESSION-07** ([#7](https://github.com/abbyfluoroethane/chord/issues/7)) (Bug, XEP-0199, XEP-0198): A dead TCP link can stay unnoticed for up to 10 minutes.
- [x] **CORESESSION-08** ([#8](https://github.com/abbyfluoroethane/chord/issues/8)) (Bug, XEP-0030, XEP-0115): Other clients cannot see that Chord supports these features.
- [x] **CORESESSION-12** ([#12](https://github.com/abbyfluoroethane/chord/issues/12)) (Bug (Unverified), RFC 6120 8.2.3, 4.9.3): A peer that gets a malformed IQ or message through the server can kill our session on each delivery.
- [x] **CORESESSION-13** ([#13](https://github.com/abbyfluoroethane/chord/issues/13)) (Bug (Unverified), XEP-0198 section 5): A message that was queued when the stream died without a resume can vanish.

**Low**

- [ ] **CORESESSION-04** ([#4](https://github.com/abbyfluoroethane/chord/issues/4)) (Missing feature, RFC 7677, RFC 5802): Accept for now.
- [ ] **CORESESSION-05** ([#5](https://github.com/abbyfluoroethane/chord/issues/5)) (Missing XEP, XEP-0388, XEP-0386, XEP-0484): Not urgent.
- [ ] **CORESESSION-09** ([#9](https://github.com/abbyfluoroethane/chord/issues/9)) (Spec violation, XEP-0030 section 3, XEP-0115 section 6): For a node that is not `CAPS_NODE#ver` and not empty, return `item-not-found`.
- [ ] **CORESESSION-10** ([#10](https://github.com/abbyfluoroethane/chord/issues/10)) (Missing XEP, XEP-0092): Add a small handler that returns the name "Chord" and the version.
- [ ] **CORESESSION-11** ([#11](https://github.com/abbyfluoroethane/chord/issues/11)) (Missing XEP, XEP-0202, XEP-0012): XEP-0202 is small and safe (UTC time and offset).
- [ ] **CORESESSION-14** ([#14](https://github.com/abbyfluoroethane/chord/issues/14)) (UX gap, RFC 6120): Add random jitter (up to 50 percent).
- [ ] **CORESESSION-15** ([#15](https://github.com/abbyfluoroethane/chord/issues/15)) (UX gap, RFC 6120 5.4.3): Add an explicit "trust this certificate" flow with a stored SHA-256 pin.
- [ ] **CORESESSION-16** ([#16](https://github.com/abbyfluoroethane/chord/issues/16)) (UX gap, RFC 6121 3.1.5): After an approval, offer "add back".
- [ ] **CORESESSION-17** ([#17](https://github.com/abbyfluoroethane/chord/issues/17)) (Missing feature, RFC 6121 2.3): Add `set_contact_groups` and `rename_contact`.
- [ ] **CORESESSION-18** ([#18](https://github.com/abbyfluoroethane/chord/issues/18)) (Missing feature, RFC 6121 4.6): Servers send unavailable when the stream closes, so the impact is small.

**Info**

- **CORESESSION-19** (Note, XEP-0126, XEP-0016): The code already falls back with a notice (presence.rs:237-244).
- **CORESESSION-20** (Note, RFC 6120 7): Fine for now.
- **CORESESSION-21** (Note, RFC 6121 2.1.6): An error reply is safe and consistent with RFC 6120 8.2.3.
- **CORESESSION-22** (Note, RFC 6120 6): The log can be wrong if the mechanism list changes.

### One-to-one messaging extensions

Report: [02-messaging.md](02-messaging.md)

**High**

- [x] **MESSAGING-01** ([#19](https://github.com/abbyfluoroethane/chord/issues/19)) (Missing XEP, XEP-0115, XEP-0030): Add `urn:xmpp:chat-markers:0`, `urn:xmpp:message-correct:0`, `urn:xmpp:message-retract:1`, `urn:xmpp:reactions:0`, `urn:xmpp:reply:0`, `urn:xmpp:fallback:0`, ...
- [x] **MESSAGING-02** ([#20](https://github.com/abbyfluoroethane/chord/issues/20)) (Bug, XEP-0359, RFC 6121): Fall back to a local key: the `id` attribute with the sender JID, or a random local key.
- [x] **MESSAGING-03** ([#21](https://github.com/abbyfluoroethane/chord/issues/21)) (Bug, RFC 6121 8.5, RFC 6120 8.3): Parse the error message.

**Medium**

- [x] **MESSAGING-04** ([#22](https://github.com/abbyfluoroethane/chord/issues/22)) (Bug, XEP-0308, XEP-0424, XEP-0444): Keep orphan edits, retractions and reactions in a small table by target id.
- [x] **MESSAGING-05** ([#23](https://github.com/abbyfluoroethane/chord/issues/23)) (Missing feature, XEP-0184): Add an option: send `<received/>` for incoming `<request/>` (1:1 only, not for strangers).
- [x] **MESSAGING-06** ([#24](https://github.com/abbyfluoroethane/chord/issues/24)) (Missing XEP, XEP-0446, XEP-0447): Send `<file-sharing>` with `<file>` metadata (name, size, media-type, hash) and the HTTP source.
- [ ] **MESSAGING-07** ([#25](https://github.com/abbyfluoroethane/chord/issues/25)) (UX gap, XEP-0393, XEP-0394): Peers that use XEP-0393 send `*bold*` and `~strike~`.
- [x] **MESSAGING-08** ([#26](https://github.com/abbyfluoroethane/chord/issues/26)) (Bug, XEP-0333 5): In a room, mark only when the marker sender is the peer of a 1:1 chat.
- [x] **MESSAGING-09** ([#27](https://github.com/abbyfluoroethane/chord/issues/27)) (Bug, XEP-0308): XEP-0308 section 3 says only the last message.
- [x] **MESSAGING-10** ([#28](https://github.com/abbyfluoroethane/chord/issues/28)) (Bug, XEP-0363): Whole file sits in memory, and again when it is copied to the request.

**Low**

- [ ] **MESSAGING-11** ([#29](https://github.com/abbyfluoroethane/chord/issues/29)) (Bug, XEP-0363 4.3): Show the max size from the error.
- [ ] **MESSAGING-12** ([#30](https://github.com/abbyfluoroethane/chord/issues/30)) (Bug, XEP-0334): Add `<no-store/>` and `<no-permanent-store/>` to chat state messages.
- [ ] **MESSAGING-13** ([#31](https://github.com/abbyfluoroethane/chord/issues/31)) (Bug, XEP-0085 5.1): Send `gone` when the user closes a chat (optional).
- [ ] **MESSAGING-14** ([#32](https://github.com/abbyfluoroethane/chord/issues/32)) (Bug, XEP-0359): Reactions message has no origin-id; the other extension messages have one.
- [ ] **MESSAGING-15** ([#33](https://github.com/abbyfluoroethane/chord/issues/33)) (Bug, XEP-0424 6): XEP-0424 asks for origin-id in 1:1 when there is one.
- [ ] **MESSAGING-16** ([#34](https://github.com/abbyfluoroethane/chord/issues/34)) (Bug, XEP-0313): Retry with back off.
- [ ] **MESSAGING-17** ([#35](https://github.com/abbyfluoroethane/chord/issues/35)) (Bug, XEP-0313): Each reconnect refetches all messages since the last MAM sync.
- [ ] **MESSAGING-18** ([#36](https://github.com/abbyfluoroethane/chord/issues/36)) (UX gap, XEP-0428): A fallback for other features (SFS, retract from other clients) shows in the body.
- [ ] **MESSAGING-19** ([#37](https://github.com/abbyfluoroethane/chord/issues/37)) (UX gap, XEP-0461): A reply to an edited message quotes the old text.
- [ ] **MESSAGING-20** ([#38](https://github.com/abbyfluoroethane/chord/issues/38)) (Bug, XEP-0280): Fine per RFC 6120 (server-set).

**Info**

- **MESSAGING-21** (Missing feature, RFC 6121 offline): Offline messages arrive as normal messages with XEP-0203 delay.
- **MESSAGING-22** (Missing feature, XEP-0066): Incoming OOB with `<desc>` is not kept.
- **MESSAGING-23** (Missing XEP, XEP-0334 incoming): Add `<private/>` (XEP-0280) for messages that must not be copied, if the user asks for it.
- **MESSAGING-24** (Missing XEP, XEP-0481, XEP-0482, XEP-0308 v2, XEP-0353): Not needed now.
- **MESSAGING-25** (Bug, doc): Fix the doc comment.

### XEP-0045 multi-user chat and related

Report: [03-muc.md](03-muc.md)

**High**

- [x] **MUC-01** ([#39](https://github.com/abbyfluoroethane/chord/issues/39)) (Missing feature, XEP-0402 section 3): Add Tauri commands.
- [x] **MUC-02** ([#40](https://github.com/abbyfluoroethane/chord/issues/40)) (Bug, XEP-0402 section 3): A room that the user leaves must not rejoin.

**Medium**

- [x] **MUC-03** ([#41](https://github.com/abbyfluoroethane/chord/issues/41)) (Missing XEP, XEP-0410): After a resume, after a long idle time and on a timer, ping the room JID with the nick.
- [x] **MUC-04** ([#42](https://github.com/abbyfluoroethane/chord/issues/42)) (UX gap, XEP-0045 section 7.8.2, XEP-0249): Show an invite card with Accept and Decline.
- [x] **MUC-05** ([#43](https://github.com/abbyfluoroethane/chord/issues/43)) (Missing feature, XEP-0045 section 10.9): Read the `destroy` child in an unavailable presence (raw Element).
- [x] **MUC-06** ([#44](https://github.com/abbyfluoroethane/chord/issues/44)) (UX gap, XEP-0045 sections 7.2.5, 7.2.6, 7.2.9): On `not-authorized`, ask for a password and retry.
- [x] **MUC-07** ([#45](https://github.com/abbyfluoroethane/chord/issues/45)) (Missing feature, XEP-0045 section 8.1): Add `set_room_subject`.
- [x] **MUC-08** ([#46](https://github.com/abbyfluoroethane/chord/issues/46)) (Missing feature, XEP-0045 sections 8.2, 8.3, 9.1, 9.2): Add `set_room_role` (kick = role none, mute = visitor, grant voice = participant).
- [x] **MUC-09** ([#47](https://github.com/abbyfluoroethane/chord/issues/47)) (Missing feature, XEP-0045 sections 7.10, 7.12): Read the reserved nick with disco#info node `x-roomuser-item` before the first join.
- [x] **MUC-10** ([#48](https://github.com/abbyfluoroethane/chord/issues/48)) (Missing feature, XEP-0045 section 10.2, `muc#roomconfig`): Show the whole owner form (a generic data form renderer).
- [x] **MUC-11** ([#49](https://github.com/abbyfluoroethane/chord/issues/49)) (Missing feature, XEP-0045 section 7.9): Show visitors in the member list.

**Low**

- [ ] **MUC-12** ([#50](https://github.com/abbyfluoroethane/chord/issues/50)) (Bug, XEP-0045 section 10.2.1): On 104 (and on the config-change status message), reread the disco#info of the room.
- [ ] **MUC-13** ([#51](https://github.com/abbyfluoroethane/chord/issues/51)) (Bug, XEP-0045 section 7.6): Give one plain sentence for each code.
- [ ] **MUC-14** ([#52](https://github.com/abbyfluoroethane/chord/issues/52)) (Bug, XEP-0045 section 7.2.7 (status 210)): When the service assigns another nick (210), store the assigned nick with `ensure_room`.
- [ ] **MUC-15** ([#53](https://github.com/abbyfluoroethane/chord/issues/53)) (Bug, XEP-0402 section 3): Keep the `extensions` child when the client republishes a bookmark.
- [ ] **MUC-16** ([#54](https://github.com/abbyfluoroethane/chord/issues/54)) (Missing XEP, XEP-0048): Optional.
- [ ] **MUC-17** ([#55](https://github.com/abbyfluoroethane/chord/issues/55)) (Missing feature, XEP-0249 section 2): Add the feature to `FEATURES`.
- [ ] **MUC-18** ([#56](https://github.com/abbyfluoroethane/chord/issues/56)) (UX gap, XEP-0045 section 7.8.2): Emit a notice: "X declined your invitation".
- [ ] **MUC-19** ([#57](https://github.com/abbyfluoroethane/chord/issues/57)) (Bug, XEP-0045 section 7.2): Add a join timeout (about 30 seconds).
- [ ] **MUC-20** ([#58](https://github.com/abbyfluoroethane/chord/issues/58)) (Security): Store it in the OS keychain (the desktop already has keychain.rs).
- [ ] **MUC-21** ([#59](https://github.com/abbyfluoroethane/chord/issues/59)) (UX gap, XEP-0045 section 10.1): For a link or a typed address, read disco#info first.
- [ ] **MUC-22** ([#60](https://github.com/abbyfluoroethane/chord/issues/60)) (UX gap, XEP-0421): Store the occupant-id per message and per occupant.

**Info**

- **MUC-23** (Missing XEP, XEP-0486): Track only.
- **MUC-24** (Note, XEP-0045 section 15.5): Add the flags when the UI needs them (see MUC-07, MUC-11).

### XEP-0503 Spaces, Pubsub, Avatars, Profiles, Bookmarks, and xmpp: URIs

Report: [04-spaces-pubsub.md](04-spaces-pubsub.md)

**Medium**

- [x] **SPACESPUBSUB-01** ([#61](https://github.com/abbyfluoroethane/chord/issues/61)) (Bug, XEP-0060 6.2, 5.6): At start, list the distinct services in the `spaces` table.
- [ ] **SPACESPUBSUB-02** ([#62](https://github.com/abbyfluoroethane/chord/issues/62)) (Security, RFC 6120 (privacy of s2s), n/a): Do not query a domain that the user has no relation to until the user clicks.
- [x] **SPACESPUBSUB-03** ([#63](https://github.com/abbyfluoroethane/chord/issues/63)) (UX gap, XEP-0060 6.5.4, 4.5): Separate "the node does not exist" (item-not-found) from "the server refuses" (forbidden, not-allowed).
- [x] **SPACESPUBSUB-04** ([#64](https://github.com/abbyfluoroethane/chord/issues/64)) (Missing feature, XEP-0503 (Membership advertising)): When the owner adds a room to a space, set `muc#roomconfig_pubsub` to `xmpp:SERVICE?;node=NODE` in the room config.
- [x] **SPACESPUBSUB-05** ([#65](https://github.com/abbyfluoroethane/chord/issues/65)) (Bug, XEP-0054, XEP-0292, XEP-0172): Remove the false "Saved." message now.
- [x] **SPACESPUBSUB-06** ([#66](https://github.com/abbyfluoroethane/chord/issues/66)) (Missing XEP, XEP-0292 (vCard4 over XMPP), XEP-0163): Plan XEP-0292 read (and later write).
- [x] **SPACESPUBSUB-08** ([#68](https://github.com/abbyfluoroethane/chord/issues/68)) (Missing XEP, XEP-0490): Publish `urn:xmpp:mds:displayed:0` items with the stanza-id on `mark_read`.

**Low**

- [ ] **SPACESPUBSUB-07** ([#67](https://github.com/abbyfluoroethane/chord/issues/67)) (Missing feature, XEP-0503 (Avatar and banner)): Add `set_space_avatar`, `set_space_banner`, and `configure_space` (owner).
- [ ] **SPACESPUBSUB-09** ([#69](https://github.com/abbyfluoroethane/chord/issues/69)) (Missing XEP, XEP-0223, XEP-0049): Store per-account settings in a private PEP node (XEP-0223 options) or in XEP-0402 bookmark extensions.
- [ ] **SPACESPUBSUB-10** ([#70](https://github.com/abbyfluoroethane/chord/issues/70)) (Bug, XEP-0402 4): Keep the extension elements on parse and write them back on publish.
- [ ] **SPACESPUBSUB-11** ([#71](https://github.com/abbyfluoroethane/chord/issues/71)) (Bug, XEP-0060 7.1.5, XEP-0402 3): On `conflict` / `precondition-not-met`, fetch the node config, submit the wanted config, and retry the publish once.
- [ ] **SPACESPUBSUB-12** ([#72](https://github.com/abbyfluoroethane/chord/issues/72)) (Security, RFC 3986, n/a): Refuse private, link-local, and loopback addresses after DNS resolution, at least for URLs from a remote entity.
- [ ] **SPACESPUBSUB-13** ([#73](https://github.com/abbyfluoroethane/chord/issues/73)) (Bug, XEP-0084 4.2): If the only info has a URL, download it with the same checks as for spaces (hash, size, type).
- [ ] **SPACESPUBSUB-14** ([#74](https://github.com/abbyfluoroethane/chord/issues/74)) (Spec violation, XEP-0084 4, 5): Resize to at most 256 by 256 in the UI and convert to PNG or JPEG.
- [ ] **SPACESPUBSUB-15** ([#75](https://github.com/abbyfluoroethane/chord/issues/75)) (Security): Sniff the type for XEP-0084 data as for vCard (`sniff_mime`, avatars.rs:391).
- [ ] **SPACESPUBSUB-16** ([#76](https://github.com/abbyfluoroethane/chord/issues/76)) (Missing feature, XEP-0153): Allow the fetch for a bare JID that has an open 1:1 chat.
- [ ] **SPACESPUBSUB-17** ([#77](https://github.com/abbyfluoroethane/chord/issues/77)) (Spec violation, XEP-0060 8.6): Echo `pubsub#subid` when known.
- [ ] **SPACESPUBSUB-18** ([#78](https://github.com/abbyfluoroethane/chord/issues/78)) (Spec violation, XEP-0060 6.2.1): Keep the subid from the subscribe result and send it on unsubscribe.
- [ ] **SPACESPUBSUB-19** ([#79](https://github.com/abbyfluoroethane/chord/issues/79)) (Missing feature, XEP-0060 8.2, 8.9): Add `space_members`, `remove_space_member`, and `ban_space_member` using owner affiliations.
- [ ] **SPACESPUBSUB-20** ([#80](https://github.com/abbyfluoroethane/chord/issues/80)) (Missing feature, XEP-0503 (Space items)): Show or ignore on purpose.
- [ ] **SPACESPUBSUB-22** ([#81](https://github.com/abbyfluoroethane/chord/issues/81)) (Spec violation, XEP-0503 (Node config)): Set `pubsub#publish_model=publishers` and a description field on create.
- [ ] **SPACESPUBSUB-23** ([#82](https://github.com/abbyfluoroethane/chord/issues/82)) (Bug, XEP-0060 5.6): Store the known non-space nodes.
- [ ] **SPACESPUBSUB-24** ([#83](https://github.com/abbyfluoroethane/chord/issues/83)) (Bug, RFC 5122 2.2, XEP-0147): Accept IDNA labels via `URL` / `domainToASCII`.
- [ ] **SPACESPUBSUB-25** ([#84](https://github.com/abbyfluoroethane/chord/issues/84)) (Spec violation, RFC 5122 3.2, XEP-0147): Lower-case the keys, or document that they are case sensitive.
- [ ] **SPACESPUBSUB-26** ([#85](https://github.com/abbyfluoroethane/chord/issues/85)) (UX gap, XEP-0503 (URI form)): Write the XEP-0503 form, as the XEP asks.
- [ ] **SPACESPUBSUB-27** ([#86](https://github.com/abbyfluoroethane/chord/issues/86)) (Missing feature, XEP-0147 5.x): Show a clearer message, "This kind of link is not supported".
- [ ] **SPACESPUBSUB-30** ([#87](https://github.com/abbyfluoroethane/chord/issues/87)) (Bug, XEP-0060 6.5): Ask for a limit, and page with RSM.

**Info**

- **SPACESPUBSUB-21** (Missing XEP, XEP-0503 (Joining), XEP-0497): Low priority.
- **SPACESPUBSUB-28** (Bug, RFC 3986 2.4): Split on the raw path first.
- **SPACESPUBSUB-29** (Note, XEP-0060): Check `Ctx::request` for a timeout.

### End-to-end encryption and modern auth

Report: [05-security-auth.md](05-security-auth.md)

**High**

- [x] **SECURITYAUTH-01** ([#88](https://github.com/abbyfluoroethane/chord/issues/88)) (Missing XEP, XEP-0384 (OMEMO 0.8+), XEP-0420 SCE): Plan OMEMO 0.8 (urn:xmpp:omemo:2) with SCE.

**Medium**

- [ ] **SECURITYAUTH-02** ([#89](https://github.com/abbyfluoroethane/chord/issues/89)) (Missing XEP, XEP-0384 0.3 (legacy, eu.siacs.conversations.axolotl)): Most clients still use the legacy version.
- [ ] **SECURITYAUTH-03** ([#90](https://github.com/abbyfluoroethane/chord/issues/90)) (Missing XEP, XEP-0373 and XEP-0374 (OpenPGP for XMPP)): Low priority.
- [ ] **SECURITYAUTH-04** ([#91](https://github.com/abbyfluoroethane/chord/issues/91)) (Missing XEP, XEP-0454 (OMEMO media sharing)): Add after OMEMO.
- [ ] **SECURITYAUTH-05** ([#92](https://github.com/abbyfluoroethane/chord/issues/92)) (Missing XEP, XEP-0450 (ATM), TOFU and BTBV): Design trust with OMEMO.
- [ ] **SECURITYAUTH-07** ([#94](https://github.com/abbyfluoroethane/chord/issues/94)) (Missing XEP, XEP-0388 SASL2): tokio-xmpp 6.0.0 has no SASL2.
- [ ] **SECURITYAUTH-08** ([#95](https://github.com/abbyfluoroethane/chord/issues/95)) (Missing XEP, XEP-0386 Bind2): Do it with SASL2 (same handshake).
- [ ] **SECURITYAUTH-09** ([#96](https://github.com/abbyfluoroethane/chord/issues/96)) (Missing XEP, XEP-0484 FAST): Needs SASL2.
- [ ] **SECURITYAUTH-10** ([#97](https://github.com/abbyfluoroethane/chord/issues/97)) (Security, XEP-0474 (SCRAM downgrade protection), XEP-0440): Because STARTTLS is required, a network attacker cannot alter the list without breaking TLS.
- [ ] **SECURITYAUTH-12** ([#99](https://github.com/abbyfluoroethane/chord/issues/99)) (Missing XEP, XEP-0397 (ISR)): Needs SASL2 and Bind2.
- [ ] **SECURITYAUTH-13** ([#100](https://github.com/abbyfluoroethane/chord/issues/100)) (UX gap, RFC 7590 / RFC 6120 13.7): Add an opt-in per-account pin (TOFU): show the SHA-256 fingerprint, store it, and compare it on later connects.
- [ ] **SECURITYAUTH-17** ([#104](https://github.com/abbyfluoroethane/chord/issues/104)) (Security): Unverified: the Android app may use the Android Keystore.
- [ ] **SECURITYAUTH-18** ([#105](https://github.com/abbyfluoroethane/chord/issues/105)) (Missing XEP, XEP-0077 (in-band registration)): Add a `register` flow before login: read the form (`jabber:iq:register`, or the XEP-0004 data form), submit it, then log in.
- [x] **SECURITYAUTH-19** ([#106](https://github.com/abbyfluoroethane/chord/issues/106)) (Missing feature, XEP-0077 section 3.3 (change password)): Add `change_password`.
- [ ] **SECURITYAUTH-21** ([#108](https://github.com/abbyfluoroethane/chord/issues/108)) (Missing XEP, XEP-0401 (easy onboarding) and XEP-0445): Parse `xmpp:domain?register;preauth=TOKEN` and `xmpp:jid?roster;preauth=TOKEN`.
- [x] **SECURITYAUTH-22** ([#109](https://github.com/abbyfluoroethane/chord/issues/109)) (Missing XEP, XEP-0379 (pre-authenticated roster subscription)): Add a `preauth` token to `add_contact`.

**Low**

- [ ] **SECURITYAUTH-06** ([#93](https://github.com/abbyfluoroethane/chord/issues/93)) (UX gap, XEP-0380 (Explicit Message Encryption)): Show a clear "encrypted message that Chord cannot read" notice when the element is present.
- [ ] **SECURITYAUTH-11** ([#98](https://github.com/abbyfluoroethane/chord/issues/98)) (Security, XEP-0440 (SASL channel binding types)): This is correct.
- [ ] **SECURITYAUTH-14** ([#101](https://github.com/abbyfluoroethane/chord/issues/101)) (Security, RFC 6120 13.7): Good.
- [ ] **SECURITYAUTH-15** ([#102](https://github.com/abbyfluoroethane/chord/issues/102)) (Security): Use `secrecy::SecretString` or `zeroize`.
- [ ] **SECURITYAUTH-16** ([#103](https://github.com/abbyfluoroethane/chord/issues/103)) (Security): Correct order.
- [ ] **SECURITYAUTH-20** ([#107](https://github.com/abbyfluoroethane/chord/issues/107)) (Missing feature, XEP-0077 section 3.2 (cancel registration)): Optional.
- [ ] **SECURITYAUTH-24** ([#110](https://github.com/abbyfluoroethane/chord/issues/110)) (UX gap): Show `account-disabled` and `credentials-expired` in plain words.
- [ ] **SECURITYAUTH-25** ([#111](https://github.com/abbyfluoroethane/chord/issues/111)) (UX gap, XEP-0198): Optional.

**Info**

- **SECURITYAUTH-23** (Missing feature, XEP-0357 / n/a): Link this to SECURITYAUTH-19.

### Discord-parity real-time features and mobile (calls, push, presence, forms)

Report: [06-calls-push-presence.md](06-calls-push-presence.md)

**High**

- [x] **CALLSPUSHPRESENCE-01** ([#112](https://github.com/abbyfluoroethane/chord/issues/112)) (Missing feature, XEP-0166, XEP-0167, XEP-0176, XEP-0320): Plan a call stack.
- [x] **CALLSPUSHPRESENCE-02** ([#113](https://github.com/abbyfluoroethane/chord/issues/113)) (Missing XEP, XEP-0353): Add Jingle Message Initiation: `propose`, `ringing`, `proceed`, `reject`, `retract`.
- [x] **CALLSPUSHPRESENCE-03** ([#114](https://github.com/abbyfluoroethane/chord/issues/114)) (Missing XEP, XEP-0215): Fetch STUN and TURN credentials with `services` IQ at connect.
- [x] **CALLSPUSHPRESENCE-06** ([#117](https://github.com/abbyfluoroethane/chord/issues/117)) (Missing XEP, XEP-0352): Add `csi::inactive` and `csi::active` commands.
- [x] **CALLSPUSHPRESENCE-07** ([#118](https://github.com/abbyfluoroethane/chord/issues/118)) (Missing feature, XEP-0357): Write the Android glue: get the token from FCM or UnifiedPush, register with an app server, call `enable_push`. Closed: the FFI, the ad-hoc commands and [android-push.md](../android-push.md) are ready. The Android app work moved to #198.
- [x] **CALLSPUSHPRESENCE-13** ([#124](https://github.com/abbyfluoroethane/chord/issues/124)) (Missing feature, FFI parity): Add `set_presence` and `own_presence` to the FFI.

**Medium**

- [ ] **CALLSPUSHPRESENCE-04** ([#115](https://github.com/abbyfluoroethane/chord/issues/115)) (Missing feature, XEP-0272 (Muji) or SFU): Decide the design first.
- [ ] **CALLSPUSHPRESENCE-05** ([#116](https://github.com/abbyfluoroethane/chord/issues/116)) (Missing feature, XEP-0166 (screen share as a second content)): Do this after 1:1 calls.
- [x] **CALLSPUSHPRESENCE-08** ([#119](https://github.com/abbyfluoroethane/chord/issues/119)) (Bug, XEP-0357 section 4): On connect, do not trust the local table.
- [x] **CALLSPUSHPRESENCE-09** ([#120](https://github.com/abbyfluoroethane/chord/issues/120)) (Spec violation, XEP-0357 section 5 (publish options)): Low risk.
- [x] **CALLSPUSHPRESENCE-10** ([#121](https://github.com/abbyfluoroethane/chord/issues/121)) (Bug, XEP-0016, XEP-0126): Check `disco.server_has("jabber:iq:privacy")` first.
- [x] **CALLSPUSHPRESENCE-11** ([#122](https://github.com/abbyfluoroethane/chord/issues/122)) (Missing XEP, XEP-0186): Add XEP-0186 (`urn:xmpp:invisible:0`) as the first choice when the server advertises it.
- [x] **CALLSPUSHPRESENCE-14** ([#125](https://github.com/abbyfluoroethane/chord/issues/125)) (Missing XEP, XEP-0319): Send `<idle since=.../>` in presence when the OS reports idle.
- [ ] **CALLSPUSHPRESENCE-16** ([#127](https://github.com/abbyfluoroethane/chord/issues/127)) (Missing XEP, XEP-0107, XEP-0108, XEP-0118 (PEP)): For Discord activities ("Playing X", "Listening to Y"), use XEP-0118 (tune) and XEP-0108 (activity).
- [x] **CALLSPUSHPRESENCE-17** ([#128](https://github.com/abbyfluoroethane/chord/issues/128)) (Missing feature, XEP-0004 (rendering)): Add a generic XEP-0004 form renderer (text, boolean, list, jid, hidden, fixed).
- [x] **CALLSPUSHPRESENCE-18** ([#129](https://github.com/abbyfluoroethane/chord/issues/129)) (Missing XEP, XEP-0050): Add ad-hoc commands: list with disco, execute, and step through forms.

**Low**

- [ ] **CALLSPUSHPRESENCE-12** ([#123](https://github.com/abbyfluoroethane/chord/issues/123)) (Bug, XEP-0126): Test with two resources.
- [ ] **CALLSPUSHPRESENCE-15** ([#126](https://github.com/abbyfluoroethane/chord/issues/126)) (Missing XEP, XEP-0256): Skip.
- [ ] **CALLSPUSHPRESENCE-19** ([#130](https://github.com/abbyfluoroethane/chord/issues/130)) (Missing XEP, XEP-0158, XEP-0077): Handle the `captcha` form in MUC join errors and in registration.
- [ ] **CALLSPUSHPRESENCE-20** ([#131](https://github.com/abbyfluoroethane/chord/issues/131)) (Missing XEP, XEP-0393, XEP-0071): Decide: keep Discord syntax on send, and render XEP-0393 on receive.
- [ ] **CALLSPUSHPRESENCE-21** ([#132](https://github.com/abbyfluoroethane/chord/issues/132)) (Missing feature, XEP-0377): Add an optional "block and report as spam or abuse" action.
- [ ] **CALLSPUSHPRESENCE-22** ([#133](https://github.com/abbyfluoroethane/chord/issues/133)) (UX gap, XEP-0191): Add Block to the person context menu.
- [ ] **CALLSPUSHPRESENCE-23** ([#134](https://github.com/abbyfluoroethane/chord/issues/134)) (Info, XEP-0115): Add the missing feature strings so peers can detect them.

**Info**

- **CALLSPUSHPRESENCE-24** (Note, RFC 6121): None.

### Desktop app functional gaps (chord-desktop)

Report: [07-desktop-gaps.md](07-desktop-gaps.md)

**High**

- [x] **DESKTOPGAPS-01** ([#135](https://github.com/abbyfluoroethane/chord/issues/135)) (UX gap): Build search on local store data first.
- [x] **DESKTOPGAPS-02** ([#136](https://github.com/abbyfluoroethane/chord/issues/136)) (Bug): Wire each toggle or remove it.

**Medium**

- [x] **DESKTOPGAPS-03** ([#137](https://github.com/abbyfluoroethane/chord/issues/137)) (Bug): Read the setting in Rust.
- [x] **DESKTOPGAPS-04** ([#138](https://github.com/abbyfluoroethane/chord/issues/138)) (Bug): Move prefs to `get_settings` and `set_settings`.
- [x] **DESKTOPGAPS-05** ([#139](https://github.com/abbyfluoroethane/chord/issues/139)) (Missing feature): Add @nick autocomplete from `app.members`.
- [x] **DESKTOPGAPS-06** ([#140](https://github.com/abbyfluoroethane/chord/issues/140)) (Missing feature): Save drafts through `settings`.
- [x] **DESKTOPGAPS-07** ([#141](https://github.com/abbyfluoroethane/chord/issues/141)) (Missing feature, XEP-0363): Add drop and paste on the composer and message list.
- [x] **DESKTOPGAPS-08** ([#142](https://github.com/abbyfluoroethane/chord/issues/142)) (Bug): Fine for the preview.
- [x] **DESKTOPGAPS-09** ([#143](https://github.com/abbyfluoroethane/chord/issues/143)) (UX gap): Add a rename call in the core and the bridge.
- [x] **DESKTOPGAPS-11** ([#145](https://github.com/abbyfluoroethane/chord/issues/145)) (Missing feature, XEP-0045 s10.1): Add "Set topic" (subject message, XEP-0045 section 8.1) for members with the right.
- [x] **DESKTOPGAPS-12** ([#146](https://github.com/abbyfluoroethane/chord/issues/146)) (UX gap, XEP-0045 s10.1, s8.2): Add "Kick" and "Mute" to the menu.
- [x] **DESKTOPGAPS-18** ([#152](https://github.com/abbyfluoroethane/chord/issues/152)) (Missing feature): Use XEP-0223 or a bookmark node for pins.

**Low**

- [ ] **DESKTOPGAPS-10** ([#144](https://github.com/abbyfluoroethane/chord/issues/144)) (UX gap): Add XEP-0077 password change (`jabber:iq:register` with the new password) in core and bridge.
- [ ] **DESKTOPGAPS-13** ([#147](https://github.com/abbyfluoroethane/chord/issues/147)) (UX gap, XEP-0045 s9.5): Read the room config form (XEP-0045 section 10.2) and fill the current values.
- [ ] **DESKTOPGAPS-14** ([#148](https://github.com/abbyfluoroethane/chord/issues/148)) (UX gap, XEP-0045 s9): Add a ban list and member list view in Channel settings.
- [ ] **DESKTOPGAPS-15** ([#149](https://github.com/abbyfluoroethane/chord/issues/149)) (UX gap, XEP-0191): Add "Unblock all" in Privacy, beside the blocked list.
- [ ] **DESKTOPGAPS-16** ([#150](https://github.com/abbyfluoroethane/chord/issues/150)) (UX gap, XEP-0045 s7.5): Add a "Decline" button on an invite card.
- [ ] **DESKTOPGAPS-17** ([#151](https://github.com/abbyfluoroethane/chord/issues/151)) (UX gap): Add Delete space and Remove channel from space in Space settings.
- [ ] **DESKTOPGAPS-19** ([#153](https://github.com/abbyfluoroethane/chord/issues/153)) (Missing feature, XEP-0461 (thread as a reply)): Show a reply chain view on click of the reply preview.
- [ ] **DESKTOPGAPS-20** ([#154](https://github.com/abbyfluoroethane/chord/issues/154)) (Missing feature): Add a message link.
- [ ] **DESKTOPGAPS-21** ([#155](https://github.com/abbyfluoroethane/chord/issues/155)) (Missing feature): Show the total unread count in the window title and on the dock or taskbar icon.
- [ ] **DESKTOPGAPS-22** ([#156](https://github.com/abbyfluoroethane/chord/issues/156)) (Missing feature): Optional.
- [ ] **DESKTOPGAPS-23** ([#157](https://github.com/abbyfluoroethane/chord/issues/157)) (UX gap): Only mark read when the focus is not in an input.
- [ ] **DESKTOPGAPS-24** ([#158](https://github.com/abbyfluoroethane/chord/issues/158)) (UX gap): Optional.
- [ ] **DESKTOPGAPS-26** ([#159](https://github.com/abbyfluoroethane/chord/issues/159)) (Bug): Collect the failures.
- [ ] **DESKTOPGAPS-27** ([#160](https://github.com/abbyfluoroethane/chord/issues/160)) (Bug): Show a toast in each case where the action is visible to the user.
- [ ] **DESKTOPGAPS-28** ([#161](https://github.com/abbyfluoroethane/chord/issues/161)) (Accessibility): Use `role="alert"` for errors.
- [ ] **DESKTOPGAPS-29** ([#162](https://github.com/abbyfluoroethane/chord/issues/162)) (Accessibility): Remove the empty handler.

**Info**

- **DESKTOPGAPS-25** (Missing feature): Out of scope for now.
- **DESKTOPGAPS-30** (n/a): No action.

### Security review of the desktop bridge and UI

Report: [08-bridge-security.md](08-bridge-security.md)

**High**

- [x] **BRIDGESECURITY-01** ([#163](https://github.com/abbyfluoroethane/chord/issues/163)) (Security): Attack: a UI script bug (for example a theme or a future XSS) calls `upload` with `~/.ssh/id_ed25519` and any room JID.
- [x] **BRIDGESECURITY-02** ([#164](https://github.com/abbyfluoroethane/chord/issues/164)) (Security): Attack: a UI script calls `save_image` with an attacker image URL and a path such as `~/.zshrc`, `~/Library/LaunchAgents/x.plist` or `~/.ssh/authorized_keys`.

**Medium**

- [x] **BRIDGESECURITY-03** ([#165](https://github.com/abbyfluoroethane/chord/issues/165)) (Security): Attack: a stranger in a public room sends a message with an XEP-0066 URL (`message_ext.rs:76` takes any string).
- [x] **BRIDGESECURITY-04** ([#166](https://github.com/abbyfluoroethane/chord/issues/166)) (Security): The Rust side fetches the page with the SSRF filter, but the webview then loads the `og:image` URL from the user's IP with no filter.
- [x] **BRIDGESECURITY-05** ([#167](https://github.com/abbyfluoroethane/chord/issues/167)) (Security): Attack: a user imports a theme from a GitHub raw link.
- [x] **BRIDGESECURITY-06** ([#168](https://github.com/abbyfluoroethane/chord/issues/168)) (Security): Attack: the sender sets an OOB URL to `https://evil.example/login` with an unknown extension.
- [ ] **BRIDGESECURITY-07** ([#169](https://github.com/abbyfluoroethane/chord/issues/169)) (Security): The key is in the binary as plain text.
- [x] **BRIDGESECURITY-08** ([#170](https://github.com/abbyfluoroethane/chord/issues/170)) (Security): If the UI is compromised, `login` with no password uses the saved keychain password, and `server` can be any `starttls://host:port`.

**Low**

- [ ] **BRIDGESECURITY-09** ([#171](https://github.com/abbyfluoroethane/chord/issues/171)) (Security): The UI can reveal any path in the file manager.
- [ ] **BRIDGESECURITY-10** ([#172](https://github.com/abbyfluoroethane/chord/issues/172)) (Security): `default-src 'self'` covers frames and objects, but not `base-uri` or `form-action`.
- [ ] **BRIDGESECURITY-11** ([#173](https://github.com/abbyfluoroethane/chord/issues/173)) (Security): An avatar can be `image/svg+xml`.
- [ ] **BRIDGESECURITY-12** ([#174](https://github.com/abbyfluoroethane/chord/issues/174)) (UX gap): Attack: `[https://your-bank.com](https://evil.example)` opens the evil site with no confirmation.
- [ ] **BRIDGESECURITY-13** ([#175](https://github.com/abbyfluoroethane/chord/issues/175)) (UX gap): Homograph JIDs (for example a Cyrillic "a") look like the real one in the confirmation dialog.
- [ ] **BRIDGESECURITY-14** ([#176](https://github.com/abbyfluoroethane/chord/issues/176)) (Security): The disco query to an unknown room or space runs before the user confirms (the card and the dialog both call `request`).
- [ ] **BRIDGESECURITY-15** ([#177](https://github.com/abbyfluoroethane/chord/issues/177)) (Security): Missing ranges: Teredo `2001::/32` (holds an IPv4 address), NAT64 local-use `64:ff9b:1::/48`, `100::/64`, `192.88.99.0/24`, and `192.31.196.0/24`.
- [ ] **BRIDGESECURITY-16** ([#178](https://github.com/abbyfluoroethane/chord/issues/178)) (Security): A room with many distinct URLs starts many fetches at once.
- [ ] **BRIDGESECURITY-17** ([#179](https://github.com/abbyfluoroethane/chord/issues/179)) (Bug): A crash between the two calls leaves no pack.

**Info**

- **BRIDGESECURITY-18** (Security): Message text shows on the lock screen and in the notification centre.
- **BRIDGESECURITY-19** (Security): The message database and `settings.json` are plain files with default permissions.
- **BRIDGESECURITY-20** (Security): All are current and maintained.

### Overall XMPP compliance (XEP-0479 matrix, missing XEPs, disco#info audit)

Report: [09-compliance-xeps.md](09-compliance-xeps.md)

**High**

- [x] **COMPLIANCEXEPS-01** ([#180](https://github.com/abbyfluoroethane/chord/issues/180)) (Missing XEP, XEP-0352, Mobile Core): Send `<inactive/>` when the app is in the background and `<active/>` when it returns.
- [x] **COMPLIANCEXEPS-02** ([#181](https://github.com/abbyfluoroethane/chord/issues/181)) (Missing XEP, XEP-0245, IM Core): Show a body that starts with `/me ` as an action line ("* Alice waves").

**Medium**

- [x] **COMPLIANCEXEPS-03** ([#182](https://github.com/abbyfluoroethane/chord/issues/182)) (Missing XEP, XEP-0368 + RFC 7590, Core Advanced): Try `_xmpps-client._tcp` first.
- [x] **COMPLIANCEXEPS-04** ([#183](https://github.com/abbyfluoroethane/chord/issues/183)) (Missing XEP, XEP-0184, IM Advanced): Only the receive side exists.
- [ ] **COMPLIANCEXEPS-05** ([#184](https://github.com/abbyfluoroethane/chord/issues/184)) (Missing XEP, XEP-0234 + XEP-0261 (Jingle file transfer), IM Advanced): Low priority.
- [ ] **COMPLIANCEXEPS-06** ([#185](https://github.com/abbyfluoroethane/chord/issues/185)) (Missing XEP, XEP-0166/0167/0176/0320/0353/0215, A/V Calling Core): Large task (weeks).
- [x] **COMPLIANCEXEPS-07** ([#186](https://github.com/abbyfluoroethane/chord/issues/186)) (Bug, XEP-0115 section 5, XEP-0030): Add the features that Chord really handles.
- [ ] **COMPLIANCEXEPS-08** ([#187](https://github.com/abbyfluoroethane/chord/issues/187)) (Missing XEP, XEP-0384 (OMEMO), not in suite): Rank 1 for user impact.

**Low**

- [ ] **COMPLIANCEXEPS-09** ([#188](https://github.com/abbyfluoroethane/chord/issues/188)) (Missing XEP, XEP-0092, XEP-0202, XEP-0012): Compliant as it is (RFC 6120 8.2.3).
- [ ] **COMPLIANCEXEPS-10** ([#189](https://github.com/abbyfluoroethane/chord/issues/189)) (Spec violation, XEP-0030 section 3.1): For an unknown node, return `item-not-found`.
- [ ] **COMPLIANCEXEPS-11** ([#190](https://github.com/abbyfluoroethane/chord/issues/190)) (Spec violation, XEP-0030 section 4): An entity that supports disco SHOULD answer disco#items with an empty list.
- [ ] **COMPLIANCEXEPS-12** ([#191](https://github.com/abbyfluoroethane/chord/issues/191)) (Security, RFC 6120 section 6, RFC 7590): Remove ANONYMOUS from a password login.
- [ ] **COMPLIANCEXEPS-13** ([#192](https://github.com/abbyfluoroethane/chord/issues/192)) (Missing XEP, XEP-0388 (SASL2), XEP-0386 (Bind 2), XEP-0484 (FAST)): Not in XEP-0479.
- [ ] **COMPLIANCEXEPS-14** ([#193](https://github.com/abbyfluoroethane/chord/issues/193)) (UX gap, XEP-0372, XEP-0393): Add XEP-0393 message styling (bold, code, quote) in the timeline.

**Info**

- **COMPLIANCEXEPS-15** (Info, XEP-0424 status): Not a defect.
- **COMPLIANCEXEPS-16** (Info, XEP-0016, XEP-0126): Works on ejabberd and Prosody today.
- **COMPLIANCEXEPS-17** (Info, XEP-0153, XEP-0054): Keep for compatibility.
- **COMPLIANCEXEPS-18** (Info, XEP-0422): Read only, for old messages.
