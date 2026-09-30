# Chord audit tracker

This file tracks the fixes from the audit of 2026-09-30. Nine read-only agents checked the code at commit `97275ff` against the XMPP RFCs and XEPs. The details for each area are in the other files in this folder.

**How to use it:** when a fix lands, tick its box and add the commit hash after it, for example `- [x] **MUC-01** ... (a1b2c3d)`. Keep one line for each finding.

## Status

| Area | Report | High | Medium | Low | Info | Done |
|---|---|---|---|---|---|---|
| RFC 6120 and RFC 6121 core session audit | [01-core-session.md](01-core-session.md) | 1 | 7 | 10 | 4 | 0 |
| One-to-one messaging extensions | [02-messaging.md](02-messaging.md) | 3 | 7 | 10 | 5 | 0 |
| XEP-0045 multi-user chat and related | [03-muc.md](03-muc.md) | 2 | 9 | 11 | 2 | 0 |
| XEP-0503 Spaces, Pubsub, Avatars, Profiles, Bookmarks, and xmpp: URIs | [04-spaces-pubsub.md](04-spaces-pubsub.md) | 0 | 7 | 20 | 3 | 0 |
| End-to-end encryption and modern auth | [05-security-auth.md](05-security-auth.md) | 1 | 15 | 8 | 1 | 0 |
| Discord-parity real-time features and mobile (calls, push, presence, forms) | [06-calls-push-presence.md](06-calls-push-presence.md) | 6 | 10 | 7 | 1 | 0 |
| Desktop app functional gaps (chord-desktop) | [07-desktop-gaps.md](07-desktop-gaps.md) | 2 | 10 | 16 | 2 | 0 |
| Security review of the desktop bridge and UI | [08-bridge-security.md](08-bridge-security.md) | 2 | 6 | 9 | 3 | 0 |
| Overall XMPP compliance (XEP-0479 matrix, missing XEPs, disco#info audit) | [09-compliance-xeps.md](09-compliance-xeps.md) | 2 | 6 | 6 | 4 | 0 |
| **All** | | **19** | **77** | **97** | **25** | **0** |

No finding is Critical. Update the Done column when you tick boxes.

## Decisions needed

- [ ] **Markdown and XEP-0393.** Chord uses Discord rules for `*`, `_` and `~`. XEP-0393 message styling uses other rules, so other clients show some Chord messages with the wrong style. Choose one: keep Discord rules, switch to XEP-0393, or send only the forms where the two agree. See MESSAGING-07.
- [ ] **Invite cards.** A card asks the server in the link for data as soon as it shows. Choose one: ask only after a click, or keep the current way. See SPACESPUBSUB-02.

## Plan

### Wave 1: bugs and security (about 1 to 2 days)

- [ ] Accept only file paths that the file dialog returned in `upload` and `save_image` (BRIDGESECURITY-01, BRIDGESECURITY-02).
- [ ] Keep a 1:1 message that has no stanza-id and no origin-id (MESSAGING-02).
- [ ] Show a bounced message (`type=error`) as failed, with a retry (MESSAGING-03).
- [ ] Add and remove room bookmarks from the desktop app, so a room that the user leaves stays gone (MUC-01, MUC-02).
- [ ] Save the display name for real, or remove the field (SPACESPUBSUB-05).
- [ ] Make the header search work, or remove it until it works (DESKTOPGAPS-01).
- [ ] Read the five notification settings in the app and the bridge (DESKTOPGAPS-02).

### Wave 2: compliance quick wins (about 2 to 3 days)

- [ ] Advertise the features that Chord implements in disco#info and caps: markers, corrections, retraction, reactions, replies, fallback, OOB, hints, direct invites. Stop advertising MAM (MESSAGING-01, CORESESSION-08, COMPLIANCEXEPS-07).
- [ ] Send a roster version only when the server offers `rosterver` (CORESESSION-01, RFC 6121 2.6.2).
- [ ] Add Client State Indication, XEP-0352 (CALLSPUSHPRESENCE-06, COMPLIANCEXEPS-01).
- [ ] Add client ping, XEP-0199, and shorter dead-connection detection (CORESESSION-06, CORESESSION-07).
- [ ] Show `/me` messages as actions, XEP-0245 (COMPLIANCEXEPS-02).
- [ ] Send delivery receipts, XEP-0184 (MESSAGING-05).
- [ ] Add direct TLS and the `_xmpps-client` SRV lookup, XEP-0368 (CORESESSION-02, COMPLIANCEXEPS-03).
- [ ] Add MUC self-ping and a join timeout, XEP-0410 (MUC-03).

### Large features (weeks each)

- [ ] End-to-end encryption: OMEMO 0.8 (XEP-0384) with XEP-0420 and XEP-0454, and trust management. About 3 to 5 weeks (SECURITYAUTH-01).
- [ ] Voice and video calls: Jingle (XEP-0166, XEP-0167, XEP-0176, XEP-0320), XEP-0353 and XEP-0215. Several weeks for 1:1 audio (CALLSPUSHPRESENCE-01 to CALLSPUSHPRESENCE-03).
- [ ] Modern sign-in: SASL2, Bind2, FAST and ISR (XEP-0388, XEP-0386, XEP-0484, XEP-0397). About 1 to 2 weeks (SECURITYAUTH-07 to SECURITYAUTH-12).
- [ ] Accounts and invites: in-band registration and password change (XEP-0077), invites (XEP-0401, XEP-0379). About 1 week (SECURITYAUTH-18 to SECURITYAUTH-22).
- [ ] Android push: connect the app to the XEP-0357 code in the core, and add presence calls to the FFI (CALLSPUSHPRESENCE-07, CALLSPUSHPRESENCE-13).

## All findings

One line for each finding: the ID, the severity, the type, the spec, and the first step of the fix. The area report has the evidence with file and line.

### RFC 6120 and RFC 6121 core session audit

Report: [01-core-session.md](01-core-session.md)

**High**

- [ ] **CORESESSION-01** (Spec violation, RFC 6121 2.6.2): Read the `rosterver` namespace in `stream_features`.

**Medium**

- [ ] **CORESESSION-02** (Missing XEP, XEP-0368, RFC 7590): Add a `ServerAddr::DirectTls` variant.
- [ ] **CORESESSION-03** (Bug (Unverified interop risk), XEP-0440): If a server lists only `tls-unique` in `sasl-channel-binding`, but offers -PLUS, the attempt can fail with `not-authorized`.
- [ ] **CORESESSION-06** (Missing XEP, XEP-0352): Add a `set_client_state(active/inactive)` command.
- [ ] **CORESESSION-07** (Bug, XEP-0199, XEP-0198): A dead TCP link can stay unnoticed for up to 10 minutes.
- [ ] **CORESESSION-08** (Bug, XEP-0030, XEP-0115): Other clients cannot see that Chord supports these features.
- [ ] **CORESESSION-12** (Bug (Unverified), RFC 6120 8.2.3, 4.9.3): A peer that gets a malformed IQ or message through the server can kill our session on each delivery.
- [ ] **CORESESSION-13** (Bug (Unverified), XEP-0198 section 5): A message that was queued when the stream died without a resume can vanish.

**Low**

- [ ] **CORESESSION-04** (Missing feature, RFC 7677, RFC 5802): Accept for now.
- [ ] **CORESESSION-05** (Missing XEP, XEP-0388, XEP-0386, XEP-0484): Not urgent.
- [ ] **CORESESSION-09** (Spec violation, XEP-0030 section 3, XEP-0115 section 6): For a node that is not `CAPS_NODE#ver` and not empty, return `item-not-found`.
- [ ] **CORESESSION-10** (Missing XEP, XEP-0092): Add a small handler that returns the name "Chord" and the version.
- [ ] **CORESESSION-11** (Missing XEP, XEP-0202, XEP-0012): XEP-0202 is small and safe (UTC time and offset).
- [ ] **CORESESSION-14** (UX gap, RFC 6120): Add random jitter (up to 50 percent).
- [ ] **CORESESSION-15** (UX gap, RFC 6120 5.4.3): Add an explicit "trust this certificate" flow with a stored SHA-256 pin.
- [ ] **CORESESSION-16** (UX gap, RFC 6121 3.1.5): After an approval, offer "add back".
- [ ] **CORESESSION-17** (Missing feature, RFC 6121 2.3): Add `set_contact_groups` and `rename_contact`.
- [ ] **CORESESSION-18** (Missing feature, RFC 6121 4.6): Servers send unavailable when the stream closes, so the impact is small.

**Info**

- **CORESESSION-19** (Note, XEP-0126, XEP-0016): The code already falls back with a notice (presence.rs:237-244).
- **CORESESSION-20** (Note, RFC 6120 7): Fine for now.
- **CORESESSION-21** (Note, RFC 6121 2.1.6): An error reply is safe and consistent with RFC 6120 8.2.3.
- **CORESESSION-22** (Note, RFC 6120 6): The log can be wrong if the mechanism list changes.

### One-to-one messaging extensions

Report: [02-messaging.md](02-messaging.md)

**High**

- [ ] **MESSAGING-01** (Missing XEP, XEP-0115, XEP-0030): Add `urn:xmpp:chat-markers:0`, `urn:xmpp:message-correct:0`, `urn:xmpp:message-retract:1`, `urn:xmpp:reactions:0`, `urn:xmpp:reply:0`, `urn:xmpp:fallback:0`, ...
- [ ] **MESSAGING-02** (Bug, XEP-0359, RFC 6121): Fall back to a local key: the `id` attribute with the sender JID, or a random local key.
- [ ] **MESSAGING-03** (Bug, RFC 6121 8.5, RFC 6120 8.3): Parse the error message.

**Medium**

- [ ] **MESSAGING-04** (Bug, XEP-0308, XEP-0424, XEP-0444): Keep orphan edits, retractions and reactions in a small table by target id.
- [ ] **MESSAGING-05** (Missing feature, XEP-0184): Add an option: send `<received/>` for incoming `<request/>` (1:1 only, not for strangers).
- [ ] **MESSAGING-06** (Missing XEP, XEP-0446, XEP-0447): Send `<file-sharing>` with `<file>` metadata (name, size, media-type, hash) and the HTTP source.
- [ ] **MESSAGING-07** (UX gap, XEP-0393, XEP-0394): Peers that use XEP-0393 send `*bold*` and `~strike~`.
- [ ] **MESSAGING-08** (Bug, XEP-0333 5): In a room, mark only when the marker sender is the peer of a 1:1 chat.
- [ ] **MESSAGING-09** (Bug, XEP-0308): XEP-0308 section 3 says only the last message.
- [ ] **MESSAGING-10** (Bug, XEP-0363): Whole file sits in memory, and again when it is copied to the request.

**Low**

- [ ] **MESSAGING-11** (Bug, XEP-0363 4.3): Show the max size from the error.
- [ ] **MESSAGING-12** (Bug, XEP-0334): Add `<no-store/>` and `<no-permanent-store/>` to chat state messages.
- [ ] **MESSAGING-13** (Bug, XEP-0085 5.1): Send `gone` when the user closes a chat (optional).
- [ ] **MESSAGING-14** (Bug, XEP-0359): Reactions message has no origin-id; the other extension messages have one.
- [ ] **MESSAGING-15** (Bug, XEP-0424 6): XEP-0424 asks for origin-id in 1:1 when there is one.
- [ ] **MESSAGING-16** (Bug, XEP-0313): Retry with back off.
- [ ] **MESSAGING-17** (Bug, XEP-0313): Each reconnect refetches all messages since the last MAM sync.
- [ ] **MESSAGING-18** (UX gap, XEP-0428): A fallback for other features (SFS, retract from other clients) shows in the body.
- [ ] **MESSAGING-19** (UX gap, XEP-0461): A reply to an edited message quotes the old text.
- [ ] **MESSAGING-20** (Bug, XEP-0280): Fine per RFC 6120 (server-set).

**Info**

- **MESSAGING-21** (Missing feature, RFC 6121 offline): Offline messages arrive as normal messages with XEP-0203 delay.
- **MESSAGING-22** (Missing feature, XEP-0066): Incoming OOB with `<desc>` is not kept.
- **MESSAGING-23** (Missing XEP, XEP-0334 incoming): Add `<private/>` (XEP-0280) for messages that must not be copied, if the user asks for it.
- **MESSAGING-24** (Missing XEP, XEP-0481, XEP-0482, XEP-0308 v2, XEP-0353): Not needed now.
- **MESSAGING-25** (Bug, doc): Fix the doc comment.

### XEP-0045 multi-user chat and related

Report: [03-muc.md](03-muc.md)

**High**

- [ ] **MUC-01** (Missing feature, XEP-0402 section 3): Add Tauri commands.
- [ ] **MUC-02** (Bug, XEP-0402 section 3): A room that the user leaves must not rejoin.

**Medium**

- [ ] **MUC-03** (Missing XEP, XEP-0410): After a resume, after a long idle time and on a timer, ping the room JID with the nick.
- [ ] **MUC-04** (UX gap, XEP-0045 section 7.8.2, XEP-0249): Show an invite card with Accept and Decline.
- [ ] **MUC-05** (Missing feature, XEP-0045 section 10.9): Read the `destroy` child in an unavailable presence (raw Element).
- [ ] **MUC-06** (UX gap, XEP-0045 sections 7.2.5, 7.2.6, 7.2.9): On `not-authorized`, ask for a password and retry.
- [ ] **MUC-07** (Missing feature, XEP-0045 section 8.1): Add `set_room_subject`.
- [ ] **MUC-08** (Missing feature, XEP-0045 sections 8.2, 8.3, 9.1, 9.2): Add `set_room_role` (kick = role none, mute = visitor, grant voice = participant).
- [ ] **MUC-09** (Missing feature, XEP-0045 sections 7.10, 7.12): Read the reserved nick with disco#info node `x-roomuser-item` before the first join.
- [ ] **MUC-10** (Missing feature, XEP-0045 section 10.2, `muc#roomconfig`): Show the whole owner form (a generic data form renderer).
- [ ] **MUC-11** (Missing feature, XEP-0045 section 7.9): Show visitors in the member list.

**Low**

- [ ] **MUC-12** (Bug, XEP-0045 section 10.2.1): On 104 (and on the config-change status message), reread the disco#info of the room.
- [ ] **MUC-13** (Bug, XEP-0045 section 7.6): Give one plain sentence for each code.
- [ ] **MUC-14** (Bug, XEP-0045 section 7.2.7 (status 210)): When the service assigns another nick (210), store the assigned nick with `ensure_room`.
- [ ] **MUC-15** (Bug, XEP-0402 section 3): Keep the `extensions` child when the client republishes a bookmark.
- [ ] **MUC-16** (Missing XEP, XEP-0048): Optional.
- [ ] **MUC-17** (Missing feature, XEP-0249 section 2): Add the feature to `FEATURES`.
- [ ] **MUC-18** (UX gap, XEP-0045 section 7.8.2): Emit a notice: "X declined your invitation".
- [ ] **MUC-19** (Bug, XEP-0045 section 7.2): Add a join timeout (about 30 seconds).
- [ ] **MUC-20** (Security): Store it in the OS keychain (the desktop already has keychain.rs).
- [ ] **MUC-21** (UX gap, XEP-0045 section 10.1): For a link or a typed address, read disco#info first.
- [ ] **MUC-22** (UX gap, XEP-0421): Store the occupant-id per message and per occupant.

**Info**

- **MUC-23** (Missing XEP, XEP-0486): Track only.
- **MUC-24** (Note, XEP-0045 section 15.5): Add the flags when the UI needs them (see MUC-07, MUC-11).

### XEP-0503 Spaces, Pubsub, Avatars, Profiles, Bookmarks, and xmpp: URIs

Report: [04-spaces-pubsub.md](04-spaces-pubsub.md)

**Medium**

- [ ] **SPACESPUBSUB-01** (Bug, XEP-0060 6.2, 5.6): At start, list the distinct services in the `spaces` table.
- [ ] **SPACESPUBSUB-02** (Security, RFC 6120 (privacy of s2s), n/a): Do not query a domain that the user has no relation to until the user clicks.
- [ ] **SPACESPUBSUB-03** (UX gap, XEP-0060 6.5.4, 4.5): Separate "the node does not exist" (item-not-found) from "the server refuses" (forbidden, not-allowed).
- [ ] **SPACESPUBSUB-04** (Missing feature, XEP-0503 (Membership advertising)): When the owner adds a room to a space, set `muc#roomconfig_pubsub` to `xmpp:SERVICE?;node=NODE` in the room config.
- [ ] **SPACESPUBSUB-05** (Bug, XEP-0054, XEP-0292, XEP-0172): Remove the false "Saved." message now.
- [ ] **SPACESPUBSUB-06** (Missing XEP, XEP-0292 (vCard4 over XMPP), XEP-0163): Plan XEP-0292 read (and later write).
- [ ] **SPACESPUBSUB-08** (Missing XEP, XEP-0490): Publish `urn:xmpp:mds:displayed:0` items with the stanza-id on `mark_read`.

**Low**

- [ ] **SPACESPUBSUB-07** (Missing feature, XEP-0503 (Avatar and banner)): Add `set_space_avatar`, `set_space_banner`, and `configure_space` (owner).
- [ ] **SPACESPUBSUB-09** (Missing XEP, XEP-0223, XEP-0049): Store per-account settings in a private PEP node (XEP-0223 options) or in XEP-0402 bookmark extensions.
- [ ] **SPACESPUBSUB-10** (Bug, XEP-0402 4): Keep the extension elements on parse and write them back on publish.
- [ ] **SPACESPUBSUB-11** (Bug, XEP-0060 7.1.5, XEP-0402 3): On `conflict` / `precondition-not-met`, fetch the node config, submit the wanted config, and retry the publish once.
- [ ] **SPACESPUBSUB-12** (Security, RFC 3986, n/a): Refuse private, link-local, and loopback addresses after DNS resolution, at least for URLs from a remote entity.
- [ ] **SPACESPUBSUB-13** (Bug, XEP-0084 4.2): If the only info has a URL, download it with the same checks as for spaces (hash, size, type).
- [ ] **SPACESPUBSUB-14** (Spec violation, XEP-0084 4, 5): Resize to at most 256 by 256 in the UI and convert to PNG or JPEG.
- [ ] **SPACESPUBSUB-15** (Security): Sniff the type for XEP-0084 data as for vCard (`sniff_mime`, avatars.rs:391).
- [ ] **SPACESPUBSUB-16** (Missing feature, XEP-0153): Allow the fetch for a bare JID that has an open 1:1 chat.
- [ ] **SPACESPUBSUB-17** (Spec violation, XEP-0060 8.6): Echo `pubsub#subid` when known.
- [ ] **SPACESPUBSUB-18** (Spec violation, XEP-0060 6.2.1): Keep the subid from the subscribe result and send it on unsubscribe.
- [ ] **SPACESPUBSUB-19** (Missing feature, XEP-0060 8.2, 8.9): Add `space_members`, `remove_space_member`, and `ban_space_member` using owner affiliations.
- [ ] **SPACESPUBSUB-20** (Missing feature, XEP-0503 (Space items)): Show or ignore on purpose.
- [ ] **SPACESPUBSUB-22** (Spec violation, XEP-0503 (Node config)): Set `pubsub#publish_model=publishers` and a description field on create.
- [ ] **SPACESPUBSUB-23** (Bug, XEP-0060 5.6): Store the known non-space nodes.
- [ ] **SPACESPUBSUB-24** (Bug, RFC 5122 2.2, XEP-0147): Accept IDNA labels via `URL` / `domainToASCII`.
- [ ] **SPACESPUBSUB-25** (Spec violation, RFC 5122 3.2, XEP-0147): Lower-case the keys, or document that they are case sensitive.
- [ ] **SPACESPUBSUB-26** (UX gap, XEP-0503 (URI form)): Write the XEP-0503 form, as the XEP asks.
- [ ] **SPACESPUBSUB-27** (Missing feature, XEP-0147 5.x): Show a clearer message, "This kind of link is not supported".
- [ ] **SPACESPUBSUB-30** (Bug, XEP-0060 6.5): Ask for a limit, and page with RSM.

**Info**

- **SPACESPUBSUB-21** (Missing XEP, XEP-0503 (Joining), XEP-0497): Low priority.
- **SPACESPUBSUB-28** (Bug, RFC 3986 2.4): Split on the raw path first.
- **SPACESPUBSUB-29** (Note, XEP-0060): Check `Ctx::request` for a timeout.

### End-to-end encryption and modern auth

Report: [05-security-auth.md](05-security-auth.md)

**High**

- [ ] **SECURITYAUTH-01** (Missing XEP, XEP-0384 (OMEMO 0.8+), XEP-0420 SCE): Plan OMEMO 0.8 (urn:xmpp:omemo:2) with SCE.

**Medium**

- [ ] **SECURITYAUTH-02** (Missing XEP, XEP-0384 0.3 (legacy, eu.siacs.conversations.axolotl)): Most clients still use the legacy version.
- [ ] **SECURITYAUTH-03** (Missing XEP, XEP-0373 and XEP-0374 (OpenPGP for XMPP)): Low priority.
- [ ] **SECURITYAUTH-04** (Missing XEP, XEP-0454 (OMEMO media sharing)): Add after OMEMO.
- [ ] **SECURITYAUTH-05** (Missing XEP, XEP-0450 (ATM), TOFU and BTBV): Design trust with OMEMO.
- [ ] **SECURITYAUTH-07** (Missing XEP, XEP-0388 SASL2): tokio-xmpp 6.0.0 has no SASL2.
- [ ] **SECURITYAUTH-08** (Missing XEP, XEP-0386 Bind2): Do it with SASL2 (same handshake).
- [ ] **SECURITYAUTH-09** (Missing XEP, XEP-0484 FAST): Needs SASL2.
- [ ] **SECURITYAUTH-10** (Security, XEP-0474 (SCRAM downgrade protection), XEP-0440): Because STARTTLS is required, a network attacker cannot alter the list without breaking TLS.
- [ ] **SECURITYAUTH-12** (Missing XEP, XEP-0397 (ISR)): Needs SASL2 and Bind2.
- [ ] **SECURITYAUTH-13** (UX gap, RFC 7590 / RFC 6120 13.7): Add an opt-in per-account pin (TOFU): show the SHA-256 fingerprint, store it, and compare it on later connects.
- [ ] **SECURITYAUTH-17** (Security): Unverified: the Android app may use the Android Keystore.
- [ ] **SECURITYAUTH-18** (Missing XEP, XEP-0077 (in-band registration)): Add a `register` flow before login: read the form (`jabber:iq:register`, or the XEP-0004 data form), submit it, then log in.
- [ ] **SECURITYAUTH-19** (Missing feature, XEP-0077 section 3.3 (change password)): Add `change_password`.
- [ ] **SECURITYAUTH-21** (Missing XEP, XEP-0401 (easy onboarding) and XEP-0445): Parse `xmpp:domain?register;preauth=TOKEN` and `xmpp:jid?roster;preauth=TOKEN`.
- [ ] **SECURITYAUTH-22** (Missing XEP, XEP-0379 (pre-authenticated roster subscription)): Add a `preauth` token to `add_contact`.

**Low**

- [ ] **SECURITYAUTH-06** (UX gap, XEP-0380 (Explicit Message Encryption)): Show a clear "encrypted message that Chord cannot read" notice when the element is present.
- [ ] **SECURITYAUTH-11** (Security, XEP-0440 (SASL channel binding types)): This is correct.
- [ ] **SECURITYAUTH-14** (Security, RFC 6120 13.7): Good.
- [ ] **SECURITYAUTH-15** (Security): Use `secrecy::SecretString` or `zeroize`.
- [ ] **SECURITYAUTH-16** (Security): Correct order.
- [ ] **SECURITYAUTH-20** (Missing feature, XEP-0077 section 3.2 (cancel registration)): Optional.
- [ ] **SECURITYAUTH-24** (UX gap): Show `account-disabled` and `credentials-expired` in plain words.
- [ ] **SECURITYAUTH-25** (UX gap, XEP-0198): Optional.

**Info**

- **SECURITYAUTH-23** (Missing feature, XEP-0357 / n/a): Link this to SECURITYAUTH-19.

### Discord-parity real-time features and mobile (calls, push, presence, forms)

Report: [06-calls-push-presence.md](06-calls-push-presence.md)

**High**

- [ ] **CALLSPUSHPRESENCE-01** (Missing feature, XEP-0166, XEP-0167, XEP-0176, XEP-0320): Plan a call stack.
- [ ] **CALLSPUSHPRESENCE-02** (Missing XEP, XEP-0353): Add Jingle Message Initiation: `propose`, `ringing`, `proceed`, `reject`, `retract`.
- [ ] **CALLSPUSHPRESENCE-03** (Missing XEP, XEP-0215): Fetch STUN and TURN credentials with `services` IQ at connect.
- [ ] **CALLSPUSHPRESENCE-06** (Missing XEP, XEP-0352): Add `csi::inactive` and `csi::active` commands.
- [ ] **CALLSPUSHPRESENCE-07** (Missing feature, XEP-0357): Write the Android glue: get the token from FCM or UnifiedPush, register with an app server, call `enable_push`.
- [ ] **CALLSPUSHPRESENCE-13** (Missing feature, FFI parity): Add `set_presence` and `own_presence` to the FFI.

**Medium**

- [ ] **CALLSPUSHPRESENCE-04** (Missing feature, XEP-0272 (Muji) or SFU): Decide the design first.
- [ ] **CALLSPUSHPRESENCE-05** (Missing feature, XEP-0166 (screen share as a second content)): Do this after 1:1 calls.
- [ ] **CALLSPUSHPRESENCE-08** (Bug, XEP-0357 section 4): On connect, do not trust the local table.
- [ ] **CALLSPUSHPRESENCE-09** (Spec violation, XEP-0357 section 5 (publish options)): Low risk.
- [ ] **CALLSPUSHPRESENCE-10** (Bug, XEP-0016, XEP-0126): Check `disco.server_has("jabber:iq:privacy")` first.
- [ ] **CALLSPUSHPRESENCE-11** (Missing XEP, XEP-0186): Add XEP-0186 (`urn:xmpp:invisible:0`) as the first choice when the server advertises it.
- [ ] **CALLSPUSHPRESENCE-14** (Missing XEP, XEP-0319): Send `<idle since=.../>` in presence when the OS reports idle.
- [ ] **CALLSPUSHPRESENCE-16** (Missing XEP, XEP-0107, XEP-0108, XEP-0118 (PEP)): For Discord activities ("Playing X", "Listening to Y"), use XEP-0118 (tune) and XEP-0108 (activity).
- [ ] **CALLSPUSHPRESENCE-17** (Missing feature, XEP-0004 (rendering)): Add a generic XEP-0004 form renderer (text, boolean, list, jid, hidden, fixed).
- [ ] **CALLSPUSHPRESENCE-18** (Missing XEP, XEP-0050): Add ad-hoc commands: list with disco, execute, and step through forms.

**Low**

- [ ] **CALLSPUSHPRESENCE-12** (Bug, XEP-0126): Test with two resources.
- [ ] **CALLSPUSHPRESENCE-15** (Missing XEP, XEP-0256): Skip.
- [ ] **CALLSPUSHPRESENCE-19** (Missing XEP, XEP-0158, XEP-0077): Handle the `captcha` form in MUC join errors and in registration.
- [ ] **CALLSPUSHPRESENCE-20** (Missing XEP, XEP-0393, XEP-0071): Decide: keep Discord syntax on send, and render XEP-0393 on receive.
- [ ] **CALLSPUSHPRESENCE-21** (Missing feature, XEP-0377): Add an optional "block and report as spam or abuse" action.
- [ ] **CALLSPUSHPRESENCE-22** (UX gap, XEP-0191): Add Block to the person context menu.
- [ ] **CALLSPUSHPRESENCE-23** (Info, XEP-0115): Add the missing feature strings so peers can detect them.

**Info**

- **CALLSPUSHPRESENCE-24** (Note, RFC 6121): None.

### Desktop app functional gaps (chord-desktop)

Report: [07-desktop-gaps.md](07-desktop-gaps.md)

**High**

- [ ] **DESKTOPGAPS-01** (UX gap): Build search on local store data first.
- [ ] **DESKTOPGAPS-02** (Bug): Wire each toggle or remove it.

**Medium**

- [ ] **DESKTOPGAPS-03** (Bug): Read the setting in Rust.
- [ ] **DESKTOPGAPS-04** (Bug): Move prefs to `get_settings` and `set_settings`.
- [ ] **DESKTOPGAPS-05** (Missing feature): Add @nick autocomplete from `app.members`.
- [ ] **DESKTOPGAPS-06** (Missing feature): Save drafts through `settings`.
- [ ] **DESKTOPGAPS-07** (Missing feature, XEP-0363): Add drop and paste on the composer and message list.
- [ ] **DESKTOPGAPS-08** (Bug): Fine for the preview.
- [ ] **DESKTOPGAPS-09** (UX gap): Add a rename call in the core and the bridge.
- [ ] **DESKTOPGAPS-11** (Missing feature, XEP-0045 s10.1): Add "Set topic" (subject message, XEP-0045 section 8.1) for members with the right.
- [ ] **DESKTOPGAPS-12** (UX gap, XEP-0045 s10.1, s8.2): Add "Kick" and "Mute" to the menu.
- [ ] **DESKTOPGAPS-18** (Missing feature): Use XEP-0223 or a bookmark node for pins.

**Low**

- [ ] **DESKTOPGAPS-10** (UX gap): Add XEP-0077 password change (`jabber:iq:register` with the new password) in core and bridge.
- [ ] **DESKTOPGAPS-13** (UX gap, XEP-0045 s9.5): Read the room config form (XEP-0045 section 10.2) and fill the current values.
- [ ] **DESKTOPGAPS-14** (UX gap, XEP-0045 s9): Add a ban list and member list view in Channel settings.
- [ ] **DESKTOPGAPS-15** (UX gap, XEP-0191): Add "Unblock all" in Privacy, beside the blocked list.
- [ ] **DESKTOPGAPS-16** (UX gap, XEP-0045 s7.5): Add a "Decline" button on an invite card.
- [ ] **DESKTOPGAPS-17** (UX gap): Add Delete space and Remove channel from space in Space settings.
- [ ] **DESKTOPGAPS-19** (Missing feature, XEP-0461 (thread as a reply)): Show a reply chain view on click of the reply preview.
- [ ] **DESKTOPGAPS-20** (Missing feature): Add a message link.
- [ ] **DESKTOPGAPS-21** (Missing feature): Show the total unread count in the window title and on the dock or taskbar icon.
- [ ] **DESKTOPGAPS-22** (Missing feature): Optional.
- [ ] **DESKTOPGAPS-23** (UX gap): Only mark read when the focus is not in an input.
- [ ] **DESKTOPGAPS-24** (UX gap): Optional.
- [ ] **DESKTOPGAPS-26** (Bug): Collect the failures.
- [ ] **DESKTOPGAPS-27** (Bug): Show a toast in each case where the action is visible to the user.
- [ ] **DESKTOPGAPS-28** (Accessibility): Use `role="alert"` for errors.
- [ ] **DESKTOPGAPS-29** (Accessibility): Remove the empty handler.

**Info**

- **DESKTOPGAPS-25** (Missing feature): Out of scope for now.
- **DESKTOPGAPS-30** (n/a): No action.

### Security review of the desktop bridge and UI

Report: [08-bridge-security.md](08-bridge-security.md)

**High**

- [ ] **BRIDGESECURITY-01** (Security): Attack: a UI script bug (for example a theme or a future XSS) calls `upload` with `~/.ssh/id_ed25519` and any room JID.
- [ ] **BRIDGESECURITY-02** (Security): Attack: a UI script calls `save_image` with an attacker image URL and a path such as `~/.zshrc`, `~/Library/LaunchAgents/x.plist` or `~/.ssh/authorized_keys`.

**Medium**

- [ ] **BRIDGESECURITY-03** (Security): Attack: a stranger in a public room sends a message with an XEP-0066 URL (`message_ext.rs:76` takes any string).
- [ ] **BRIDGESECURITY-04** (Security): The Rust side fetches the page with the SSRF filter, but the webview then loads the `og:image` URL from the user's IP with no filter.
- [ ] **BRIDGESECURITY-05** (Security): Attack: a user imports a theme from a GitHub raw link.
- [ ] **BRIDGESECURITY-06** (Security): Attack: the sender sets an OOB URL to `https://evil.example/login` with an unknown extension.
- [ ] **BRIDGESECURITY-07** (Security): The key is in the binary as plain text.
- [ ] **BRIDGESECURITY-08** (Security): If the UI is compromised, `login` with no password uses the saved keychain password, and `server` can be any `starttls://host:port`.

**Low**

- [ ] **BRIDGESECURITY-09** (Security): The UI can reveal any path in the file manager.
- [ ] **BRIDGESECURITY-10** (Security): `default-src 'self'` covers frames and objects, but not `base-uri` or `form-action`.
- [ ] **BRIDGESECURITY-11** (Security): An avatar can be `image/svg+xml`.
- [ ] **BRIDGESECURITY-12** (UX gap): Attack: `[https://your-bank.com](https://evil.example)` opens the evil site with no confirmation.
- [ ] **BRIDGESECURITY-13** (UX gap): Homograph JIDs (for example a Cyrillic "a") look like the real one in the confirmation dialog.
- [ ] **BRIDGESECURITY-14** (Security): The disco query to an unknown room or space runs before the user confirms (the card and the dialog both call `request`).
- [ ] **BRIDGESECURITY-15** (Security): Missing ranges: Teredo `2001::/32` (holds an IPv4 address), NAT64 local-use `64:ff9b:1::/48`, `100::/64`, `192.88.99.0/24`, and `192.31.196.0/24`.
- [ ] **BRIDGESECURITY-16** (Security): A room with many distinct URLs starts many fetches at once.
- [ ] **BRIDGESECURITY-17** (Bug): A crash between the two calls leaves no pack.

**Info**

- **BRIDGESECURITY-18** (Security): Message text shows on the lock screen and in the notification centre.
- **BRIDGESECURITY-19** (Security): The message database and `settings.json` are plain files with default permissions.
- **BRIDGESECURITY-20** (Security): All are current and maintained.

### Overall XMPP compliance (XEP-0479 matrix, missing XEPs, disco#info audit)

Report: [09-compliance-xeps.md](09-compliance-xeps.md)

**High**

- [ ] **COMPLIANCEXEPS-01** (Missing XEP, XEP-0352, Mobile Core): Send `<inactive/>` when the app is in the background and `<active/>` when it returns.
- [ ] **COMPLIANCEXEPS-02** (Missing XEP, XEP-0245, IM Core): Show a body that starts with `/me ` as an action line ("* Alice waves").

**Medium**

- [ ] **COMPLIANCEXEPS-03** (Missing XEP, XEP-0368 + RFC 7590, Core Advanced): Try `_xmpps-client._tcp` first.
- [ ] **COMPLIANCEXEPS-04** (Missing XEP, XEP-0184, IM Advanced): Only the receive side exists.
- [ ] **COMPLIANCEXEPS-05** (Missing XEP, XEP-0234 + XEP-0261 (Jingle file transfer), IM Advanced): Low priority.
- [ ] **COMPLIANCEXEPS-06** (Missing XEP, XEP-0166/0167/0176/0320/0353/0215, A/V Calling Core): Large task (weeks).
- [ ] **COMPLIANCEXEPS-07** (Bug, XEP-0115 section 5, XEP-0030): Add the features that Chord really handles.
- [ ] **COMPLIANCEXEPS-08** (Missing XEP, XEP-0384 (OMEMO), not in suite): Rank 1 for user impact.

**Low**

- [ ] **COMPLIANCEXEPS-09** (Missing XEP, XEP-0092, XEP-0202, XEP-0012): Compliant as it is (RFC 6120 8.2.3).
- [ ] **COMPLIANCEXEPS-10** (Spec violation, XEP-0030 section 3.1): For an unknown node, return `item-not-found`.
- [ ] **COMPLIANCEXEPS-11** (Spec violation, XEP-0030 section 4): An entity that supports disco SHOULD answer disco#items with an empty list.
- [ ] **COMPLIANCEXEPS-12** (Security, RFC 6120 section 6, RFC 7590): Remove ANONYMOUS from a password login.
- [ ] **COMPLIANCEXEPS-13** (Missing XEP, XEP-0388 (SASL2), XEP-0386 (Bind 2), XEP-0484 (FAST)): Not in XEP-0479.
- [ ] **COMPLIANCEXEPS-14** (UX gap, XEP-0372, XEP-0393): Add XEP-0393 message styling (bold, code, quote) in the timeline.

**Info**

- **COMPLIANCEXEPS-15** (Info, XEP-0424 status): Not a defect.
- **COMPLIANCEXEPS-16** (Info, XEP-0016, XEP-0126): Works on ejabberd and Prosody today.
- **COMPLIANCEXEPS-17** (Info, XEP-0153, XEP-0054): Keep for compatibility.
- **COMPLIANCEXEPS-18** (Info, XEP-0422): Read only, for old messages.
