# Plan: vCard4 (XEP-0292) and profiles

Issue: SPACESPUBSUB-06. Related: SPACESPUBSUB-05 (the display name).

## What exists now

The file `chord-core/src/features/profile.rs` holds the first step.

- `ClientHandle::profile(jid)` reads two PEP nodes of an account. It reads the nickname
  (XEP-0172, node `http://jabber.org/protocol/nick`). Then it reads the vCard4 (XEP-0292,
  node `urn:xmpp:vcard4`) and takes the formatted name `FN`.
- `ClientHandle::set_nickname(text)` publishes the nickname to our own PEP. An empty
  text retracts it.
- The desktop app reads our own profile at start and uses it as our display name. The
  display name field in the account settings now saves for real.
- The CLI has `profile [jid]` and `set-nickname <text>|--remove`.

The read is on demand. Chord keeps no copy and shows no name of a contact from it yet.

## What is missing

1. **Show the name of other people.** The UI shows the roster name, then the address.
   Add a step: if a contact has no roster name, call `profile` once per session and keep
   the result in memory. Use `display_name()`. A roster name that the user set always wins.
2. **`+notify` for the nodes.** Add `http://jabber.org/protocol/nick+notify` and
   `urn:xmpp:vcard4+notify` to `FEATURES` in `disco.rs` only when the code handles the
   events. Handle them in `pubsub.rs`, like the MDS node. This replaces the read on demand
   for contacts that we see.
3. **Other vCard4 fields.** Parse `nickname`, `note` (bio), `bday`, `url`, `tel`, `email`,
   and the pronouns property of RFC 9554 (`pronouns`). Put them in `Profile`. A profile
   dialog shows them. Show no field that the user did not publish.
4. **Write the vCard4.** Read our own item first, change only the fields that the user
   edits, and publish the whole item again. Unknown properties must stay. Publish with
   the publish options `persist_items`, `max_items` 1 and `access_model` `presence`.
   Keep the old vCard-temp PHOTO in step with the avatar (see `avatars.rs`).
5. **Limits.** A vCard4 item can hold a large photo as a URI. Do not fetch a `photo` URI
   without the same checks as for a link preview.

## Order

Step 1 first: it gives the user the most value and needs no protocol work. Then step 2.
Steps 3 and 4 need a profile dialog in the UI, so they wait for its design.

## Tests

The unit tests in `profile.rs` cover the two reads and the publish. A live test of
step 4 needs an account with a vCard4 that another client wrote. Movim writes one.
