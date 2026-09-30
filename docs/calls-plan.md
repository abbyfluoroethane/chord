# Call stack plan

This plan covers issue #112 (CALLSPUSHPRESENCE-01) and the calls epic #195. It says how Chord gets 1:1 voice calls, then video. The first two building blocks exist already: XEP-0215 (STUN and TURN discovery) and the XEP-0353 message layer. The media and the Jingle layer do not exist.

Facts in this document come from the XEPs and from crate pages that we read on 2026-09-30. A line that says "unverified" is a guess.

## What exists today

| Piece | Where | State |
|---|---|---|
| XEP-0215 STUN and TURN discovery | `chord-core/src/features/extdisco.rs` | Done. `ClientHandle::ice_servers`, FFI `ice_servers`, `chord-cli ice`. The core refreshes credentials before they expire. |
| XEP-0353 Jingle Message Initiation 0.8.0 | `chord-core/src/features/jmi.rs` | Message layer done. propose, ringing, proceed, reject, retract, finish, tie-break, carbons. `ClientEvent::Call`. FFI methods. `chord-cli call`. |
| Advertising `urn:xmpp:jingle-message:0` | `features/disco.rs` | Not done on purpose. A peer that sees the feature would ring into nothing. |
| Jingle (XEP-0166) IQ handling | none | Missing. |
| SDP to Jingle mapping | none | Missing. |
| Media (capture, codecs, echo cancellation) | none | Missing. |

The server at chat.foid.space runs ejabberd with `mod_stun_disco`. It offers one STUN service (UDP 3478) and two TURN services (UDP and TCP, port 3478). TURN credentials are short-lived. The live test of `chord-cli ice` shows this. The relay ports are 50000 to 50099, so the server can relay about 50 calls at once.

## Decision: where the media runs

There are three options.

**A. WebRTC in the WebView of the desktop app, and libwebrtc in the Android app.** The UI layer owns the `RTCPeerConnection`. The core owns the signalling. The UI gives the core an SDP string and ICE candidates. The core turns them into Jingle. The other way works too.

**B. `webrtc-rs` inside `chord-core`.** One media stack for every platform. The core would own capture, codecs, and the network.

**C. GStreamer or another native stack.** No.

We recommend A, with the SDP and Jingle mapping written once, in `chord-core`.

Reasons:

1. A WebRTC engine gives echo cancellation, noise suppression, automatic gain control, a jitter buffer, congestion control, and Opus. A call with no echo cancellation is unusable on laptop speakers. `webrtc-rs` (crate `webrtc`, version 0.21.0, released 2026-09-19) is a protocol implementation. Its README talks about data channels, media playback, simulcast, insertable streams, and ICE restart. It says nothing about audio capture, echo cancellation, or an Opus encoder. We would add `cpal` (0.18.2), `opus` (0.4.0, bindings to libopus), and our own echo cancellation. That is weeks of work for a worse result. (Unverified: whether `webrtc-rs` includes any audio processing. The README excerpt that we read does not mention it.)
2. The WebView already has the engine on macOS and Windows. WKWebView and WebView2 both support `getUserMedia` and `RTCPeerConnection`. (Unverified in Chord: the permission prompt flow in `wry`. Search results show reports of WKWebView calls that hang with no prompt. A spike must test this first.)
3. Android has a maintained engine as a Kotlin library. We found `io.getstream:stream-webrtc-android` 1.3.8 (February 2025) and `io.github.webrtc-sdk:android` 125.6422.07 (January 2025) on Maven Central. Google's own `org.webrtc` artifact is not maintained. We have not compared the two. Pick one in the Android spike.
4. The mapping between SDP and Jingle is pure data conversion. It has no platform code. It belongs in Rust, where one test suite covers both front ends.

The risk of A is Linux. WebKitGTK builds WebRTC only when the distribution compiles it with `-DENABLE_WEB_RTC=ON` and `-DENABLE_MEDIA_STREAM=ON`. A Tauri discussion (#8426) reports a working setup only on X11, with patches to `wry`. A Tauri maintainer said that official support was not close. So Linux desktop is the last platform to get calls. The fallback for Linux is B: add `webrtc-rs`, `cpal`, and `opus` behind a Cargo feature, and accept worse audio at first. We decide this after the macOS and Windows path works.

The core must not depend on a media engine. We add one small boundary in the core: the UI sends `local_description(sid, sdp, kind)` and `local_candidate(sid, candidate)`. The core sends `remote_description` and `remote_candidate` events. A future `webrtc-rs` engine would use the same boundary.

## Signalling in the core

The core adds a `features/jingle.rs` module. It works after JMI `proceed`.

Order of events, with A calling B:

1. A sends JMI `propose` to the bare JID of B. B rings. B sends `ringing` to the full JID of A.
2. B accepts and sends JMI `proceed` to the full JID of A. Carbons tell the other devices of B to stop.
3. A now knows the full JID of B. A creates the offer in the WebView and sends `session-initiate` (XEP-0166) to that full JID. The `sid` attribute is the `id` of the `propose`. XEP-0353 says the same id flows into the Jingle IQ.
4. B answers the IQ with an empty result (XEP-0166 says every action needs one). B creates the answer and sends `session-accept`.
5. Both sides send `transport-info` for each ICE candidate (trickle ICE, XEP-0176).
6. Media flows. Either side sends `session-terminate` and then JMI `finish`.

Rules for the core:

- Accept a Jingle IQ only from the full JID that took part in the JMI `proceed`. Drop every other IQ with an error.
- Send `ringing` as a JMI message (XEP-0353). Do not send the Jingle `session-info` ringing. One ringing signal is enough.
- Never start media before the user accepts. The user accepts by pressing the button. Answering a call shows our IP address to the caller through ICE, so unknown callers must not get this without a decision. Add a setting "relay only for people who are not in my contacts". This sets the ICE transport policy to `relay`.
- Keep the state in memory. A call does not survive a new stream.

## SDP to Jingle mapping

The core converts between the SDP of the WebView and the Jingle stanzas. `xmpp-parsers` 0.23 has types for most of the elements (`jingle`, `jingle_rtp`, `jingle_ice_udp`, `jingle_dtls_srtp`, `jingle_grouping`, `jingle_rtcp_fb`, `jingle_rtp_hdrext`, `jingle_ssma`). Its `extdisco::Service` has private fields, so we cannot read it. Check each Jingle type the same way before we rely on it. If a type is too strict, the core reads the element with `minidom`, as `extdisco.rs` does.

| SDP | Jingle | XEP |
|---|---|---|
| `m=audio` block, the `mid` | `<content name=mid creator=initiator senders=both>` with `<description xmlns='urn:xmpp:jingle:apps:rtp:1' media='audio'>` | 0166, 0167 |
| `a=rtpmap:96 opus/48000/2`, `a=fmtp:96 ...` | `<payload-type id='96' name='opus' clockrate='48000' channels='2'>` with `<parameter name= value=/>` children | 0167 |
| `a=rtcp-mux` | `<rtcp-mux/>` in the description | 0167 |
| `a=rtcp-fb:96 nack pli` | `<rtcp-fb xmlns='urn:xmpp:jingle:apps:rtp:rtcp-fb:0' type='nack' subtype='pli'/>` in the payload type | 0293 |
| `a=rtcp-fb:* trr-int 100` | `<rtcp-fb-trr-int value='100'/>` | 0293 |
| `a=extmap:1 urn:ietf:params:rtp-hdrext:ssrc-audio-level` | `<rtp-hdrext xmlns='urn:xmpp:jingle:apps:rtp:rtp-hdrext:0' id='1' uri='...'/>` | 0294 |
| `a=ssrc:123 cname:x`, `a=ssrc:123 msid:...` | `<source xmlns='urn:xmpp:jingle:apps:rtp:ssma:0' ssrc='123'><parameter name='cname' value='x'/></source>` | 0339 |
| `a=ssrc-group:FID 1 2` | `<ssrc-group semantics='FID'><source ssrc='1'/><source ssrc='2'/></ssrc-group>` | 0339 |
| `a=group:BUNDLE audio video` | `<group xmlns='urn:xmpp:jingle:apps:grouping:0' semantics='BUNDLE'><content name='audio'/><content name='video'/></group>` in the `<jingle>` element | 0338 |
| `a=ice-ufrag`, `a=ice-pwd` | `ufrag` and `pwd` attributes of `<transport xmlns='urn:xmpp:jingle:transports:ice-udp:1'>` | 0176 |
| `a=candidate:...` | `<candidate component foundation generation id ip port priority protocol type rel-addr rel-port network/>` | 0176 |
| `a=fingerprint:sha-256 AB:CD...`, `a=setup:actpass` | `<fingerprint xmlns='urn:xmpp:jingle:apps:dtls:0' hash='sha-256' setup='actpass'>AB:CD...</fingerprint>`, a child of the transport, before the candidates | 0320 |
| `a=sendrecv`, `sendonly`, `recvonly`, `inactive` | `senders` attribute of the content (`both`, `initiator`, `responder`, `none`) | 0166 |
| `a=end-of-candidates` | a `transport-info` with no more candidates. Unverified: the exact form that Conversations and Dino expect. | 0176 |

Known versions on 2026-09-30, from the XEPs: 0166 is 1.1.2, 0167 is 1.2.3, 0176 is 1.1.1, 0293 is 1.0.2, 0320 is 1.0.0, 0338 is 1.0.0, 0339 is 1.0.1, 0215 is 1.0.0, and 0353 is 0.8.0.

Hard points that need care:

- **The content name.** The `mid` of the SDP and the `name` of the content must be the same. The BUNDLE group refers to it.
- **The DTLS role.** The offerer sends `setup='actpass'`. The answerer sends `active` or `passive`. A wrong role breaks the handshake with no clear error.
- **`msid` and `ssrc`.** WebRTC uses them to attach a track to a stream. Send them as `<source>` parameters. Unified Plan SDP has one `m=` block per track.
- **Trickle ICE.** The candidates arrive after the offer. The core must buffer a `transport-info` that arrives before the answer and pass it on when the remote description is set.
- **Codecs.** Ask for Opus for audio. The other clients that we test with must offer Opus too. (Unverified for each client.)
- **`urn:xmpp:jingle:apps:rtp:audio`** and `urn:xmpp:jingle:apps:rtp:video` are the disco features for the media types. Do not advertise them before the media works.

Write the converter as a pure function pair (`sdp_to_jingle`, `jingle_to_sdp`) with fixtures. Collect the fixtures from real SDP: one from WKWebView, one from Chromium, one from a libwebrtc Android build. Add the Jingle that Conversations and Dino send.

## Integration of what exists

- **XEP-0215.** Before the WebView creates the `RTCPeerConnection`, the UI calls `ice_servers()`. It puts each entry in `RTCConfiguration.iceServers`, with `uri`, `username`, and `password`. The core has the credentials ready and refreshes them. If the call runs longer than the credentials live, the UI asks again. An ICE restart uses the new credentials. When `ice_servers()` fails with `Unsupported`, the call goes on with no TURN and shows a warning.
- **XEP-0353.** `ClientEvent::Call` drives the UI. `Incoming` shows the call screen and calls `ring_call`. `Proceeded` tells the initiator to send `session-initiate`. `Ended` closes the screen. The states `Accepted` and `Ended` also feed the Jingle layer.
- **Disco.** Advertise `urn:xmpp:jingle-message:0`, `urn:xmpp:jingle:1`, `urn:xmpp:jingle:apps:rtp:1`, `urn:xmpp:jingle:apps:rtp:audio`, the ICE-UDP feature, and `urn:xmpp:jingle:apps:dtls:0` in one change. Do it in the last step of phase 3, and only when a call works.
- **Push and CSI.** A call to a phone that sleeps needs a push notification (see `docs/android-push.md`). The JMI `propose` has no body. The push service must wake the app for it. This is an open problem on Android and needs a design (see phase 5).

## Phases and estimates

One developer, in weeks. The numbers include tests and review.

| Phase | Work | Weeks |
|---|---|---|
| 0 | XEP-0215 and the XEP-0353 message layer. Done. | 0 |
| 1 | Spike: `getUserMedia` and `RTCPeerConnection` in the Tauri WebView on macOS and Windows. Permission prompts, device choice, Info.plist and manifest keys. Decide about Linux. | 0.5 |
| 2 | `features/jingle.rs`: session table, IQ handling and acks, security checks, timers, events. Tests with a fake session. | 1 |
| 3 | `sdp_to_jingle` and `jingle_to_sdp`, fixtures, tests. | 1.5 |
| 4 | Desktop UI: call screen, ringtone, mute, device choice, call history in the timeline, settings for relay only. Then advertise the features in disco. | 1.5 |
| 5 | Interop against Conversations, Dino, and Gajim. Fix the mapping. | 1 |
| 6 | Android: Kotlin engine on libwebrtc, a foreground service, a full-screen notification, wake-up by push. Needs phase 5 of the push plan first. | 2 |
| 7 | Video and screen share (`m=video`, BUNDLE with audio). | 1.5 |
| 8 | Linux: test distribution packages, or add `webrtc-rs` behind a feature. | 1 to 2 |

1:1 audio on macOS and Windows: about 5.5 weeks (phases 1 to 5). Android adds 2 weeks. Group calls are out of scope. XEP-0272 (Multiparty Jingle) is experimental and no client that we know supports it. (Unverified: check before we plan groups.)

## Test plan

Unit tests, no network:

- The state machine of `jmi.rs` (exists, 22 tests).
- The Jingle table: every action in every state, with a fake session.
- The SDP converter: round trip from SDP to Jingle to SDP for each fixture. The result must equal the input, apart from line order.
- A `transport-info` before `session-accept`.
- An IQ from a wrong resource.

Live tests against chat.foid.space, with the two test accounts:

1. `chord-cli ice` shows one STUN and two TURN services. (Done.)
2. `chord-cli call` and `call-answer` run the JMI flow between two accounts. (Done.)
3. A headless test that runs the WebView engine twice on one Mac, one for each account. Force `iceTransportPolicy: 'relay'`. It proves that TURN works with the credentials from XEP-0215. Check that the data flows through port 3478 and that the relay stays inside 50000 to 50099.
4. The same with TURN over TCP only (block UDP with a firewall rule).
5. Two devices for one account: the second device stops ringing (already tested with carbons for JMI).
6. A call across NAT: one machine on another network. This needs a person.

Interop tests, by hand, with a checklist for each client: Conversations (Android), Dino (Linux), Gajim, Monal. For each: audio both ways, mute, hang up from each side, reject, retract, and a call while a second device is online. Record the SDP and the Jingle in a file when a call fails.

## Server work

- Serve TURN over TLS on port 443 or 5349, so that calls work behind a firewall that allows only HTTPS. The current config lists TURN on 3478 for UDP and TCP only.
- Watch the range of relay ports. 100 ports limit the server to about 50 relayed calls.
- Confirm that `stun_disco` access (`local` and `sfu`) covers all users.

## Open questions

- Does WKWebView show a permission prompt when `getUserMedia` runs inside Tauri, or does `wry` need code? (Phase 1 answers this.)
- Which Android WebRTC artifact do we use? (Phase 6.)
- Do we send a JMI `finish` after every Jingle `session-terminate`? XEP-0353 0.8.0 says yes, for state sync between devices. Test how the other clients react.
