# Owner subscriptions test on the Prosody test server

Date: 2026-09-28
Prosody: 13.0 nightly build 100 (2026-09-24, cafd74b9c5ea), from `prosodyctl about`.
Service: `pubsub.chord.localhost`. Accounts: `alice@chord.localhost`, `bob@chord.localhost`.

## Summary

The owner subscriptions query (XEP-0060 section 8.8) works. The server does not
advertise `pubsub#manage-subscriptions`, but the handlers do not check for it.
The `authorize` access model does not work. Prosody rejects it when a node is created.
So a subscription never becomes `pending`, and no authorization request goes to the owner.

## Method and limit

I injected each stanza on the pubsub host through `prosodyctl shell`, as in
`dev/prosody/disco-pubsub.sh`. Each Lua line did the following.

1. Parse the request XML with `prosody.util.xml`.
2. Build a fake `origin` table (`type="c2s"`, `username`, `full_jid`) with a `send` function that captures replies.
3. Fire `iq/host/<namespace>:pubsub` on `prosody.hosts["pubsub.chord.localhost"].events`.

`mod_pubsub.lua:167-168` hooks these two event names, one for each namespace.

Limit: the stanzas did not travel over a real client connection. There was no TLS, no
SASL, no stream and no session. Routing and stanza-sanity code in Prosody did not run.
The `from` attribute came from my own request. Only the pubsub handlers and the
service code ran. I did not restart or reconfigure the server, and I did not change
passwords. Messages that the service sends to other JIDs (notifications) do not go
through `origin.send`. My method did not capture them. The results below do not
depend on them.

The node names use a random suffix. The step 1 node was never created. Steps 1b to 6
used `space-test-29024`. The whitelist extra test used another random name.

## Step 1. Alice creates a node with access model `authorize`

Request:

```xml
<iq type='set' from='alice@chord.localhost/probe' to='pubsub.chord.localhost' id='c1'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub'>
    <create node='space-test-NNNNN'/>
    <configure>
      <x xmlns='jabber:x:data' type='submit'>
        <field var='FORM_TYPE' type='hidden'><value>http://jabber.org/protocol/pubsub#node_config</value></field>
        <field var='pubsub#access_model'><value>authorize</value></field>
        <field var='pubsub#type'><value>urn:xmpp:spaces:0</value></field>
      </x>
    </configure>
  </pubsub>
</iq>
```

Reply:

```xml
<iq from='pubsub.chord.localhost' type='error' id='c1' to='alice@chord.localhost/probe'>
  <error type='modify'>
    <not-acceptable xmlns='urn:ietf:params:xml:ns:xmpp-stanzas'/>
  </error>
</iq>
```

Result: fail. Prosody refuses `authorize`. See "Does Prosody support `authorize`?" below.

## Step 1b. Alice creates the node with access model `open` (fallback)

Request:

```xml
<iq type='set' from='alice@chord.localhost/probe' to='pubsub.chord.localhost' id='c2'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub'>
    <create node='space-test-29024'/>
    <configure>
      <x xmlns='jabber:x:data' type='submit'>
        <field var='FORM_TYPE' type='hidden'><value>http://jabber.org/protocol/pubsub#node_config</value></field>
        <field var='pubsub#access_model'><value>open</value></field>
        <field var='pubsub#type'><value>urn:xmpp:spaces:0</value></field>
      </x>
    </configure>
  </pubsub>
</iq>
```

Reply:

```xml
<iq id='c2' type='result' from='pubsub.chord.localhost' to='alice@chord.localhost/probe'/>
```

## Step 2. Bob subscribes

Request:

```xml
<iq type='set' from='bob@chord.localhost/probe' to='pubsub.chord.localhost' id='s1'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub'>
    <subscribe node='space-test-29024' jid='bob@chord.localhost'/>
  </pubsub>
</iq>
```

Reply:

```xml
<iq from='pubsub.chord.localhost' type='result' id='s1' to='bob@chord.localhost/probe'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub'>
    <subscription jid='bob@chord.localhost' subscription='subscribed' node='space-test-29024'/>
  </pubsub>
</iq>
```

The subscription is `subscribed` at once, not `pending`. This node is `open`, so this is
correct for `open`. On an `authorize` node it would be wrong, but `authorize` nodes do
not exist here.

## Step 3. Alice gets the owner subscriptions

Request:

```xml
<iq type='get' from='alice@chord.localhost/probe' to='pubsub.chord.localhost' id='g1'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <subscriptions node='space-test-29024'/>
  </pubsub>
</iq>
```

Reply:

```xml
<iq from='pubsub.chord.localhost' type='result' id='g1' to='alice@chord.localhost/probe'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <subscriptions>
      <subscription jid='bob@chord.localhost' subscription='subscribed' node='space-test-29024'/>
    </subscriptions>
  </pubsub>
</iq>
```

The list has bob, with state `subscribed`. The reply has no `node` attribute on the
`<subscriptions>` element.

## Step 4. Alice sets subscriptions, then queries again

4a. Alice removes bob (`subscription='none'`). This shows that the set has an effect.

```xml
<iq type='set' from='alice@chord.localhost/probe' to='pubsub.chord.localhost' id='o1'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <subscriptions node='space-test-29024'>
      <subscription jid='bob@chord.localhost' subscription='none'/>
    </subscriptions>
  </pubsub>
</iq>
```

```xml
<iq id='o1' type='result' from='pubsub.chord.localhost' to='alice@chord.localhost/probe'/>
```

4b. Alice queries again. The list is empty.

```xml
<iq from='pubsub.chord.localhost' type='result' id='g2' to='alice@chord.localhost/probe'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <subscriptions/>
  </pubsub>
</iq>
```

4c. Alice sets bob to `subscribed`.

```xml
<iq type='set' from='alice@chord.localhost/probe' to='pubsub.chord.localhost' id='o2'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <subscriptions node='space-test-29024'>
      <subscription jid='bob@chord.localhost' subscription='subscribed'/>
    </subscriptions>
  </pubsub>
</iq>
```

```xml
<iq id='o2' type='result' from='pubsub.chord.localhost' to='alice@chord.localhost/probe'/>
```

4d. Alice queries again. Bob is back.

```xml
<iq from='pubsub.chord.localhost' type='result' id='g3' to='alice@chord.localhost/probe'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <subscriptions>
      <subscription jid='bob@chord.localhost' subscription='subscribed' node='space-test-29024'/>
    </subscriptions>
  </pubsub>
</iq>
```

4e. Bob queries his own subscriptions (user side, namespace `pubsub`).

```xml
<iq type='get' from='bob@chord.localhost/probe' to='pubsub.chord.localhost' id='u1'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub'>
    <subscriptions node='space-test-29024'/>
  </pubsub>
</iq>
```

```xml
<iq from='pubsub.chord.localhost' type='result' id='u1' to='bob@chord.localhost/probe'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub'>
    <subscriptions>
      <subscription jid='bob@chord.localhost' subscription='subscribed' node='space-test-29024'/>
    </subscriptions>
  </pubsub>
</iq>
```

## Step 5. Bob (not the owner) uses the owner requests

Get:

```xml
<iq type='get' from='bob@chord.localhost/probe' to='pubsub.chord.localhost' id='g4'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <subscriptions node='space-test-29024'/>
  </pubsub>
</iq>
```

```xml
<iq from='pubsub.chord.localhost' type='error' id='g4' to='bob@chord.localhost/probe'>
  <error type='auth'>
    <forbidden xmlns='urn:ietf:params:xml:ns:xmpp-stanzas'/>
  </error>
</iq>
```

Set (extra check; bob tries to remove himself through the owner request):

```xml
<iq type='set' from='bob@chord.localhost/probe' to='pubsub.chord.localhost' id='o3'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <subscriptions node='space-test-29024'>
      <subscription jid='bob@chord.localhost' subscription='none'/>
    </subscriptions>
  </pubsub>
</iq>
```

```xml
<iq from='pubsub.chord.localhost' type='error' id='o3' to='bob@chord.localhost/probe'>
  <error type='auth'>
    <forbidden xmlns='urn:ietf:params:xml:ns:xmpp-stanzas'/>
  </error>
</iq>
```

## Step 6. Alice deletes the node

```xml
<iq type='set' from='alice@chord.localhost/probe' to='pubsub.chord.localhost' id='d1'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <delete node='space-test-29024'/>
  </pubsub>
</iq>
```

```xml
<iq id='d1' type='result' from='pubsub.chord.localhost' to='alice@chord.localhost/probe'/>
```

## Extra test. Node with access model `whitelist`

Alice created a `whitelist` node and deleted it after the test.

| Request | Reply |
|---------|-------|
| Bob subscribes | `forbidden` (error type `auth`) |
| Alice owner-sets bob to `subscribed` | `forbidden` (error type `auth`) |
| Alice owner-gets subscriptions | result, empty `<subscriptions/>` |

The owner cannot add bob to a `whitelist` node. Bob is `outcast` by default there, and
`be_subscribed` is false for `outcast` (`util/pubsub.lua:33`, checked by
`add_subscription` at `util/pubsub.lua:327`). The owner must first make bob a
member with an affiliation. Then a subscription is possible.

## Results

| Step | Expected | Actual | Pass or fail |
|------|----------|--------|--------------|
| 1. Create node, `authorize` and type `urn:xmpp:spaces:0` | result | `not-acceptable` | fail |
| 1b. Create node, `open` and the same type | result | result | pass |
| 2. Bob subscribes | `pending` on `authorize` | `subscribed` (node is `open`) | not testable |
| 3. Owner get lists bob | bob listed | bob listed, `subscribed` | pass |
| 4a. Owner set `none` | result, bob removed | result, list empty | pass |
| 4c. Owner set `subscribed` | result | result | pass |
| 4d. Owner get after set | bob listed | bob listed, `subscribed` | pass |
| 4e. Bob user-side get | bob listed | bob listed, `subscribed` | pass |
| 5. Bob owner get | `forbidden` | `forbidden` | pass |
| 6. Alice deletes node | result | result | pass |

## Source evidence

Paths are inside the container. Line numbers are from the container files.

- Prosody does not support `authorize`.
  - The node config form offers `authorize` as a list option (`/usr/lib/prosody/modules/mod_pubsub/pubsub.lib.lua:101`).
  - `check_node_config` in `/usr/lib/prosody/modules/mod_pubsub/mod_pubsub.lua:133-136` returns false for any model except `open` and `whitelist`. That gives the `not-acceptable` error in step 1.
  - `get_default_affiliation` in `/usr/share/lua/5.4/prosody/util/pubsub.lua:256-274` handles only `open` and `whitelist`, plus an optional `config.access_models` table. `mod_pubsub` sets no such table (grep finds no match).
  - The words `pending` and `authorize` do not appear in `util/pubsub.lua`. The service has no pending state and sends no authorization request. The subscription handlers hard-code `subscription='subscribed'` (`pubsub.lib.lua:410`, `:427`, `:556`).
  - The service also does not advertise `access-authorize`. The disco features list only `access-open`.
- The owner handlers do not check advertised features.
  - `handle_pubsub_iq` builds the handler name from the namespace, type and child name (`pubsub.lib.lua:322-338`). For the owner namespace it looks up `owner_get_subscriptions` and `owner_set_subscriptions`.
  - The handlers are at `pubsub.lib.lua:416` (get) and `:433` (set).
- Feature advertising comes from `service_method_feature_map` (`pubsub.lib.lua:231-243`). `get_feature_set` adds features for each service method that exists. The map has no `manage-subscriptions` entry, so the disco lacks it.
- Owner permission.
  - The owner get calls `service:get_subscriptions(node, from)` with no `jid` (`pubsub.lib.lua:418`). That checks capability `get_subscriptions_other` (`util/pubsub.lua:740-747`). The `owner` affiliation has it (`util/pubsub.lua:128`). Bob is `member` on an open node and does not. That gives `forbidden` in step 5.
  - The owner set checks `subscribe_other` (`pubsub.lib.lua:439`). It then calls `add_subscription` (`:452`) or `remove_subscription` (`:458`).
- Subscribed JIDs come out of `flatten_subscriptions` (`util/pubsub.lua:717`), which the get uses.

## Patch

`dev/prosody/patches/0001-mod_pubsub-advertise-manage-subscriptions.patch` adds
`manage-subscriptions` to the `add_subscription` line of `service_method_feature_map`
(`pubsub.lib.lua:232`). It is for the local test server only. I did not apply it.
`patch --dry-run` succeeds on a copy of the container file. Clients should not rely on
this feature flag. They should try the owner query, and treat `forbidden` or
`feature-not-implemented` as "not available".
