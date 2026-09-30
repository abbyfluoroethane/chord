# XEP-0045 multi-user chat and related

## Summary

- The core join, leave, nick change, room creation, kick and ban flows are correct and tested. The gaps are mostly in the parts that a user sees or that need a second XEP.
- The desktop app never creates or removes XEP-0402 bookmarks. The Tauri bridge has no bookmark command. A room that the user leaves in the desktop app comes back at the next login if another client set autojoin.
- The client has no XEP-0410 self-ping. After a lost MUC connection (for example a remote room on another server), the client shows the room as joined but gets nothing.
- The desktop UI has no accept or decline action for a room invitation, no password prompt, no nick retry, no kick, no ban, no subject change, no voice request and no room destruction handling.
- Room destruction (XEP-0045 section 10.9) is not parsed. The user sees "removed from" and the room stays in the list.
- Moderation (XEP-0425), occupant-id (XEP-0421) for "is this message mine" and MUC MAM catch-up work well.

## Findings

| ID | Severity | Type | Spec | Evidence | What to do |
|---|---|---|---|---|---|
| MUC-01 | High | Missing feature | XEP-0402 section 3 | No `add_bookmark` or `remove_bookmark` in chord-desktop/src-tauri/src/commands.rs or in chord-desktop/src/lib. Only chord-cli/src/actions.rs:46 and chord-ffi/src/client.rs:642 call it. | Add Tauri commands. Bookmark a room (with autojoin and nick) when the user joins it or creates it. Retract the bookmark when the user leaves it. |
| MUC-02 | High | Bug | XEP-0402 section 3 | chord-core/src/features/bookmarks.rs:147 `for bookmark in bookmarks.iter().filter(\|b\| b.autojoin)` and muc.rs:283 "A bookmark with autojoin stays". Desktop `leaveRoom` (app.svelte.ts:998) calls only `leaveRoom`. | A room that the user leaves must not rejoin. Publish the bookmark with autojoin false, or retract it, when the user leaves. |
| MUC-03 | Medium | Missing XEP | XEP-0410 | grep for `0410`, `self-ping`, `urn:xmpp:ping` in chord-core/src finds only the disco feature `ns::PING`. No ping to `room/nick`. | After a resume, after a long idle time and on a timer, ping the room JID with the nick. On `not-acceptable` or `item-not-found`, rejoin. On timeout, do nothing (per the XEP). |
| MUC-04 | Medium | UX gap | XEP-0045 section 7.8.2, XEP-0249 | chord-desktop/src/lib/ui/live.svelte.ts:374 shows a toast only: "invited you to". `declineRoomInvite` in lib/chord/api.ts:229 has no caller. The invite password in `ClientEvent::RoomInvite` is dropped. | Show an invite card with Accept and Decline. Accept joins with the invite password. Decline sends the decline. |
| MUC-05 | Medium | Missing feature | XEP-0045 section 10.9 | grep for `destroy` in chord-core/src finds nothing. xmpp-parsers 0.23 `muc/user.rs` has no destroy type. on_unavailable (muc.rs:1863-1915) treats it as "removed from". | Read the `destroy` child in an unavailable presence (raw Element). Show the reason and the alternate room. Mark the room as gone. Offer to remove it and its bookmark. |
| MUC-06 | Medium | UX gap | XEP-0045 sections 7.2.5, 7.2.6, 7.2.9 | app.svelte.ts:269 and :890 show only `plainError`. muc.rs:2153 `error_text` gives text for `not-authorized`, `conflict` and `registration-required`. No prompt. | On `not-authorized`, ask for a password and retry. On `conflict`, ask for another nick. On `registration-required`, say that the room is members only. Handle `service-unavailable` (room full) and `item-not-found` (locked room). |
| MUC-07 | Medium | Missing feature | XEP-0045 section 8.1 | grep for `set_subject` shows only the inbound path (muc.rs:1662). No API in ClientHandle, Tauri or FFI. | Add `set_room_subject`. Send a groupchat message with only a `subject`. Map `forbidden` to a plain message. |
| MUC-08 | Medium | Missing feature | XEP-0045 sections 8.2, 8.3, 9.1, 9.2 | ClientHandle (muc.rs:250-436) has affiliation and invite calls only. No `set_role`. The UI `setAffiliation` (app.svelte.ts:1063) covers admin, member, none and outcast. | Add `set_room_role` (kick = role none, mute = visitor, grant voice = participant). Show Kick and Ban in the member menu. Show a reason field. |
| MUC-09 | Medium | Missing feature | XEP-0045 sections 7.10, 7.12 | grep for `x-roomuser-item`, `jabber:iq:register` and `muc#register` finds nothing. | Read the reserved nick with disco#info node `x-roomuser-item` before the first join. Add room registration for members-only rooms that allow it. |
| MUC-10 | Medium | Missing feature | XEP-0045 section 10.2, `muc#roomconfig` | `RoomSettings` (muc.rs:85-92) has three fields: name, public, members only. `ROOM_CONFIG_ON` (muc.rs:2062) sets persistent and archive only. | Show the whole owner form (a generic data form renderer). Add password, description, moderated, allow invites, max users and anonymity (whois). |
| MUC-11 | Medium | Missing feature | XEP-0045 section 7.9 | `visitor` role: adapt.ts:126 maps every role except moderator to `null`. types.ts:90 says "visitor" exists. No voice request flow (`muc#request`). | Show visitors in the member list. In a moderated room, let a visitor send a voice request. Show the request to moderators with Approve. |
| MUC-12 | Low | Bug | XEP-0045 section 10.2.1 | Status codes 104, 172, 173, 174, 170, 100, 101 are not handled (grep `Status::` in muc.rs finds only 110, 201, 301, 303, 307). A change of the room config or the anonymity does not refresh anything. | On 104 (and on the config-change status message), reread the disco#info of the room. On 172-174, update the anonymity flag. |
| MUC-13 | Low | Bug | XEP-0045 section 7.6 | on_unavailable (muc.rs:1902-1908) has no text for 321 (affiliation change), 322 (members only) and 332 (system shutdown). All read "removed from". | Give one plain sentence for each code. Try to rejoin after 332 later. |
| MUC-14 | Low | Bug | XEP-0045 section 7.2.7 (status 210) | join_room stores the requested nick (muc.rs:1155). on_available sets the in-memory nick from the presence (muc.rs:1977) but never updates the stored nick. | When the service assigns another nick (210), store the assigned nick with `ensure_room`. |
| MUC-15 | Low | Bug | XEP-0402 section 3 | bookmarks.rs:62 `Conference::new()` builds a bookmark with no `extensions` child. xmpp-parsers 0.23 has `Conference.extensions`. Bookmark::parse drops it. | Keep the `extensions` child when the client republishes a bookmark. |
| MUC-16 | Low | Missing XEP | XEP-0048 | grep for `storage:bookmarks` and `private:xml` finds nothing. Disco lists only `urn:xmpp:bookmarks:1+notify` (disco.rs:31). Unverified: whether a server without XEP-0402 exists in the target group. | Optional. Add `urn:xmpp:bookmarks:1#compat+notify` support or a private XML fallback only if a needed server has no XEP-0402. |
| MUC-17 | Low | Missing feature | XEP-0249 section 2 | disco.rs:20-32 does not list `jabber:x:conference`. Clients that check disco may not send direct invites. Chord sends only mediated invites (muc.rs:846). | Add the feature to `FEATURES`. Add a direct invite send option for rooms that do not allow mediated invites. |
| MUC-18 | Low | UX gap | XEP-0045 section 7.8.2 | on_invitation (muc.rs:896-938) ignores an incoming `<decline>` from the room. The inviter never learns of a refusal. | Emit a notice: "X declined your invitation". |
| MUC-19 | Low | Bug | XEP-0045 section 7.2 | No timeout for a pending join. `Join` (muc.rs:115) lives until a presence arrives. grep for `timeout` in muc.rs finds nothing. Unverified: whether the actor has a general IQ timeout (it does not cover presence). | Add a join timeout (about 30 seconds). Then fail the waiting replies and clear the join. |
| MUC-20 | Low | Security | n/a | The room password sits in the `rooms.password` column in clear text (muc.rs:1033 `INSERT INTO rooms ... password`). It also goes into the bookmark (bookmarks.rs:64). | Store it in the OS keychain (the desktop already has keychain.rs). Publish the password in the bookmark only when the user agrees. The node uses whitelist access, so this part is acceptable. |
| MUC-21 | Low | UX gap | XEP-0045 section 10.1 | joinRoomLink (app.svelte.ts:866) joins any address. If the room does not exist, the server creates it and Chord configures it as persistent (muc.rs:1979-1982). A typo makes a new room. | For a link or a typed address, read disco#info first. Ask before you create a room. |
| MUC-22 | Low | UX gap | XEP-0421 | occupant-id is used only for "is this message mine" (muc.rs:1217). Members, reactions and moderation do not use it. A nick change or a nick reuse can mix up who wrote what in the member list. | Store the occupant-id per message and per occupant. Use it to group messages and to key reactions. |
| MUC-23 | Info | Missing XEP | XEP-0486 | No room avatar code. avatars.rs handles occupant avatars only (`on_occupant`). Unverified: XEP-0486 status (recent, not widely deployed). | Track only. Read the room vCard avatar (XEP-0054 to the room JID) if servers offer it. |
| MUC-24 | Info | Note | XEP-0045 section 15.5 | `room_card` (muc.rs:766) reads name, description, subject and occupants from `muc#roominfo`. It reads `muc_passwordprotected` and `muc_membersonly`. It does not read `muc_moderated`, `muc_nonanonymous`, `muc_semianonymous` or `muc_persistent`. | Add the flags when the UI needs them (see MUC-07, MUC-11). |

## Already done well

- Join presence carries `<x xmlns='http://jabber.org/protocol/muc'>` with `maxstanzas=0` and the password (muc.rs:1159-1161). History comes from MAM, which is the modern way.
- Self-presence with status 110 completes the join (muc.rs:1932-1976). Status 201 starts the owner form, which submits persistent and archive fields only when the form has them (muc.rs:2062-2122). That avoids an ejabberd rejection.
- Errors of a join fail the waiting replies and clear the outbox (muc.rs:1831-1861). Messages that wait for a join go out after the join (muc.rs:1990).
- Nick change is a plain presence to the new nick. Codes 303 and 110 are handled. A conflict keeps the old nick (muc.rs:1871-1885, 1846).
- Kick (307) and ban (301) show a notice with the reason (muc.rs:1896-1911). Stale presence after a leave is ignored (muc.rs:1936).
- Rejoin on a new session: rooms with `joined = 1` rejoin (muc.rs:451, 479). Bookmark autojoin runs after the fetch (bookmarks.rs:144).
- MUC MAM: room archive queries go to the room JID, only a result from the room counts, catch-up uses the newest stored id (mam.rs:401, 448-452).
- Private messages use `type=chat` with an empty `muc#user` element (muc.rs:1369-1376). They have their own timeline.
- Mediated invitation (with a member grant when the user has the right), direct invitation reading (XEP-0249), decline sending, room card from disco#info (muc.rs:383-412, 896-938, 766).
- XEP-0425 moderation request and both wire forms of the result. Only the room bare JID may moderate (retraction.rs:147, 238, 259).
- XEP-0421 occupant-id identifies our own messages after a nick change (muc.rs:1217-1231).
- XEP-0402 fetch, PEP event handling, publish options (whitelist, `max`) and retract (bookmarks.rs:96-260).
- Affiliation list and change (muc.rs:345-376, 825).

## Recommended next steps

1. Add bookmark commands to Tauri. Bookmark on join, retract on leave (MUC-01, MUC-02). About 0.5 day.
2. Add the invite card with Accept and Decline and the password and nick prompts (MUC-04, MUC-06). About 1 day.
3. Add XEP-0410 self-ping on resume and on a timer, and a join timeout (MUC-03, MUC-19). About 1 day.
4. Read `destroy` in presences and handle the status codes 104, 172-174, 321, 322, 332 (MUC-05, MUC-12, MUC-13). About 0.5 day.
5. Add `set_room_subject` and `set_room_role`. Add Kick and Ban in the member menu (MUC-07, MUC-08). About 1.5 days.
6. Add visitors and voice request (MUC-11). About 1 day.
7. Add a generic room config form and the reserved nick lookup (MUC-10, MUC-09). About 2 days.
8. Keep bookmark `extensions`, move the room password to the keychain, add `jabber:x:conference` to disco (MUC-15, MUC-20, MUC-17). About 0.5 day.
