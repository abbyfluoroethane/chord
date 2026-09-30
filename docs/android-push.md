# Android push

This document covers issue #118 (CALLSPUSHPRESENCE-07) and the push part of the epic #198. It says how an Android app gets push notifications with the XEP-0357 code that already exists in `chord-core`. The repository has no Android app. The only Kotlin file is `chord-ffi/kotlin-test/Smoke.kt`, a smoke test for the generated bindings. So this document is the specification for the Kotlin glue. Nothing here ran on a real phone.

We read the XEP and the project pages on 2026-09-30. A line that says "unverified" is a guess or a fact that we could not check.

## What the core offers

| Need | Core and FFI call | Notes |
|---|---|---|
| Turn push on | `ClientHandle::enable_push(service, node, form)`, FFI `enablePush(service, node, form)` | Waits for service discovery. Fails with `Unsupported` when the server does not advertise `urn:xmpp:push:0`. The form holds the publish options, for example `secret`. The core sends the form and never stores it. |
| Turn push off | `disable_push(service, node?)`, FFI `disablePush(service, node)` | With no node, every node of the service stops. Fails with `ItemNotFound` when the server has no such registration. |
| List registrations | `push_registrations()`, FFI `pushRegistrations()` | Reads the store. Works offline. |
| Register with the app server | `execute_command(to, node, fields)`, FFI `executeCommand(service, node, fields)` | New. One-step ad-hoc commands (XEP-0050). The app server registration needs it. |

The Kotlin code decides when to enable and disable. The server keeps the registration, but XEP-0357 has no query to check it. So the core sends `enable` again for each stored registration after the login and service discovery of each new session (not after a resumed session). The store keeps the publish options, the secret included, for this.

Kotlin names come from the generated bindings (`uniffi.chord_ffi`). We generated them and checked these names: `enablePush`, `disablePush`, `pushRegistrations`, `executeCommand`, `login`, `syncArchive`, `subscribeEvents`, and the record `FormField(name, value)`.

## The flow

```
Phone                      Push service            App server           XMPP server
(Chord app)                (FCM or UnifiedPush)    (p2 or a gateway)    (ejabberd)
   |                             |                       |                    |
1. |-- get token or endpoint --->|                       |                    |
   |<-------- token ------------|                       |                    |
2. |-- register(token) ------------------------------->|                    |
   |<-- jid, node, secret -----------------------------|                    |
3. |-- enable(jid, node, secret) ---------------------------------------->|
4. |                             |                       |<-- publish(node) --|  (a message came)
   |                             |<-- wake-up -----------|                    |
   |<-- wake-up -----------------|                       |                    |
5. |-- login, sync ------------------------------------------------------->|
   |<-- messages ------------------------------------------------------------|
   |-- show notification                                                      |
```

Steps:

1. The app asks the push system for an address. FCM gives a token. UnifiedPush gives an endpoint URL.
2. The app registers that address with the app server. The app server gives back its JID, a node, and a secret.
3. The app calls `enablePush(jid, node, [FormField("secret", secret)])`. The XMPP server stores the registration.
4. When the account gets a message and no client is online, the XMPP server publishes a notification to the node on the app server. The app server sends a wake-up through the push system.
5. The app wakes up, logs in, reads the new messages, and shows a notification.

## What the XMPP server needs

Read from `/home/manager/chat-foid-space/ejabberd/ejabberd.yml` on 2026-09-30. The check was read-only.

- `mod_push` is on (`mod_push: {}`), and so is `mod_push_keepalive`. `mod_stream_mgmt`, `mod_mam`, and `mod_carboncopy` are on too. Push needs stream management and the archive, so the app can catch up after the wake-up.
- The account advertises `urn:xmpp:push:0`. `chord-cli push-enable` worked, so the check in `push.rs` passes.
- ejabberd does not check the push service when it accepts the registration. We enabled a registration for the JID `push.example.org`, which does not exist, and the server accepted it. Then `push-list` showed it and `push-disable` removed it. A wrong JID fails later, when the server tries to publish. XEP-0357 says a server should treat a JID and node with a publish error as disabled. A wrong registration is silent for the user.
- The XMPP server must reach the app server JID. If the app server is a separate domain, the servers must talk over s2s. If it is a component, ejabberd needs a listener for it. We did not change the server.
- Options of `mod_push` and `mod_push_keepalive` that matter for privacy and for battery: whether the notification includes the sender and the body, and how long the server keeps the session for the wake-up. (Unverified: the exact names and defaults. Read the ejabberd documentation for version 26.07 before we set them.)

XEP-0357 (version 0.4.1, status Deferred) shows the notification that the server publishes. It has `message-count`, `last-message-sender`, and `last-message-body` in a form. The XEP says servers should let users limit this data, because push services are third parties. We recommend that the app server never sees the body.

## The app server

The app server is an XMPP entity that the XMPP server can publish to. It must accept publish requests from the server JID, and it should check the `secret` that the client gave at enable time. It then talks to the push system. Chord needs to run one, or use one.

### Option A: FCM with p2

p2 is the push app server of Conversations. The repository moved to Codeberg (`codeberg.org/iNPUTmice/p2`). Its page says it supports FCM and APNs, and the project is "open source and closed contribution". We did not find the license text.

Registration, from the p2 page:

- The client runs the ad-hoc command `register-push-fcm` on the p2 JID.
- The client sends two fields: `token` (the FCM token) and `android-id` (an id that stays the same for the life of the app).
- p2 answers with `jid` (its own JID), `node` (a random id for this registration), and `secret`.
- The wake-up to FCM has the form `{"to": "<fcm-token>", "data": {"account": "<hash>"}}`. It carries a hash of the account. It does not carry the JID, the sender, or the body. The app then knows which account to sync.

Consequences for Chord:

- Chord needs its own Firebase project and its own FCM credentials, because only the app developer can send to the tokens of the app. p2 needs those credentials.
- Chord must run its own p2, or use one that we trust. We do not use `push.conversations.im`, because it holds the FCM credentials of another app.
- FCM needs Google Play Services on the phone. Phones with no Play Services need option B.

### Option B: UnifiedPush

UnifiedPush has no central provider. The user installs a distributor app. The app gets an endpoint URL on that distributor. The Android connector library is `org.unifiedpush.android:connector`, version 3.3.5 on Maven Central (Apache-2.0, depends on Kotlin 2.2.10 and Tink 1.23.0). The app declares a service that extends `PushService`. The service must not be exported. It handles the action `org.unifiedpush.android.connector.PUSH_EVENT`. When the distributor gives an endpoint, the service gets `onNewEndpoint`. (Unverified: the exact signatures of the callbacks in version 3.x. Read the KDoc at `unifiedpush.org/kdoc/connector/` before we write the code.)

The endpoint is an HTTP URL. An XEP-0357 app server for UnifiedPush must turn a publish from the XMPP server into an HTTP POST to that URL. We did not find a maintained gateway that does this for XMPP. (Unverified: the search was short. Ask in the UnifiedPush and XMPP communities before we build one.) We found other app servers that do not fit: the Prosody module `mod_push_appserver` (Prosody, not ejabberd) and the UWPX push server (for UWPX). We did not read them.

So we plan a small gateway, `chord-push-gateway`:

- It connects to the XMPP server as an external component (XEP-0114), or as an ordinary account with its own JID.
- It offers an ad-hoc command `register-push-up` with the field `endpoint`. It answers with `jid`, `node`, and `secret`, like p2, so the Kotlin code is the same for both.
- It keeps the table of node, secret, and endpoint in SQLite.
- On a publish it checks the node and the secret, then sends an HTTP POST with an empty body (or a short fixed body) to the endpoint. It never sends the sender or the body.
- On an HTTP 404 or 410 from the endpoint it removes the registration and answers the next publish with `item-not-found`, so the XMPP server disables the node.
- Size: about one week, with tests.

## Kotlin glue

There is no app yet. These are the steps for the person who builds it. Names are from the generated bindings.

### 1. Register

```kotlin
// FCM. Needs the Firebase Messaging dependency and a google-services.json.
val token = FirebaseMessaging.getInstance().token.await()
val androidId = prefs.getOrCreateInstallId()   // random, stored, stays for the life of the app

val result = client.executeCommand(
    service = PUSH_APP_SERVER_JID,             // for example "push.chord.example"
    node = "register-push-fcm",
    fields = listOf(
        FormField("token", token),
        FormField("android-id", androidId),
    ),
)
val jid = result.first { it.name == "jid" }.value
val node = result.first { it.name == "node" }.value
val secret = result.first { it.name == "secret" }.value
```

`executeCommand` fails with `ChordException` (an error from the server) when the app server refuses, and with `Unsupported` when the command needs more than one step.

For UnifiedPush, call `register` of the connector library, wait for `onNewEndpoint(endpoint, instance)`, and run the command `register-push-up` with the field `endpoint`.

### 2. Enable

```kotlin
client.enablePush(
    service = jid,
    node = node,
    form = listOf(FormField("secret", secret)),
)
```

Call it after the login succeeded (the `Connected` event of `subscribeEvents`). The call waits for service discovery. Save the tuple (jid, node) with `pushRegistrations()`. The core stores the secret in its database to send `enable` again at each login. The app does not need to save it.

### 3. Refresh and remove

- FCM gives a new token from `onNewToken`. Run the registration again, call `enablePush` with the new node, then call `disablePush(oldJid, oldNode)`.
- On sign-out, call `disablePush(jid, null)` for each entry of `pushRegistrations()`. Then remove the account. If the session is gone, the call fails with `NotConnected`. Keep the entry and try again at the next login.
- A UnifiedPush distributor can change the endpoint. `onNewEndpoint` runs again. Register again and replace the old node.
- Compare `pushRegistrations()` with what the app expects at each login. A registration that the user removed on another device may be missing.

### 4. Handle a wake-up

```kotlin
class ChordMessagingService : FirebaseMessagingService() {
    override fun onMessageReceived(message: RemoteMessage) {
        // message.data["account"] is a hash. The app has one account today, so it syncs it.
        SyncWorker.enqueueExpedited(applicationContext)
    }
}
```

The worker runs a short foreground service:

1. Create the `ChordClient`, then `login(jid, password, server)` with the password from the Android Keystore.
2. Wait for the `Connected` event. Stream management resumes the session when it can.
3. Call `syncArchive()` so the archive fills the gaps.
4. Show a notification for each `Notification` event. The core decides which messages notify (`notify.rs`).
5. Log out and stop after a short idle time, unless the app is in front.

Rules and limits: a high-priority FCM message lets the app start a foreground service. (Unverified: the time budget. Measure it on a device.) Put nothing secret in the wake-up.

### 5. Errors to show

- `Unsupported` from `enablePush`: the server has no push. Tell the user that notifications need a server with push support.
- A wrong app server JID gives no error at enable time (see above). Test the whole path with a real message before we say that push works.

## What we ran

All from `chord-cli` against chat.foid.space, with the account `chordag5a`:

- `push-enable push.example.org ag5-test-node` printed `enabled push to push.example.org ag5-test-node`. The server accepted a registration with a service that does not exist.
- `push-list --offline` listed the registration. After `push-disable`, it listed none.
- `push-disable push.example.org nothing-here` failed with `server error: ItemNotFound: Push record not found`.
- `adhoc chat.foid.space register-push-fcm token=x android-id=y` failed with `ItemNotFound: No hook has processed this command`. This shows the error path of the ad-hoc call. The success path is only in unit tests, because no app server runs.
- Unit tests: `features::push::tests` (enable, disable, list, errors), `features::adhoc::tests` (form sent, result fields, more steps, error), and `chord-ffi` `calls::tests::push_methods_check_input_and_need_a_session`.

We did not run: a real app server, an FCM token, a UnifiedPush distributor, the delivery of a real wake-up, or any Kotlin code beyond generating the bindings.

## Steps to finish (with estimates)

| Step | Work | Days |
|---|---|---|
| 1 | Create the Android app project. Out of scope of this issue. | n/a |
| 2 | Firebase project. Deploy p2 with our FCM credentials. Connect it to ejabberd (s2s or a component listener). | 1 to 2 |
| 3 | Kotlin: register, enable, refresh, remove, wake-up worker, notifications. | 3 to 4 |
| 4 | Test with a real phone. Check the time budget of the wake-up and battery use. | 2 |
| 5 | UnifiedPush: `chord-push-gateway` and the Kotlin connector service. | 6 to 8 |
| 6 | Read the ejabberd 26.07 options for `mod_push` and `mod_push_keepalive`. Decide what the notification holds. | 0.5 |
| 7 | Live test with a real message: a second account sends a message to a phone that is offline. Check that the wake-up arrives. | 1 |

The desktop app has no push. It stays connected. `commands.rs` exposes only `push_registrations`, and we do not plan more.
