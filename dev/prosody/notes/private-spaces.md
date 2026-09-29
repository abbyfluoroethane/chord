# Private spaces on Prosody 13: whitelist and member affiliation

Date: 2026-09-28
Prosody: 13.0 nightly build 100 (2026-09-24, cafd74b9c5ea).
Tool: `dev/prosody/pubsub-inject.sh`. It has the same method and limit as
`owner-subscriptions.md`: the stanzas go to the pubsub handlers through `prosodyctl shell`,
not over a client connection.

## Why

Prosody 13 rejects the `authorize` access model when a node is created
(`mod_pubsub.lua:132-135`). So a private space cannot use join requests and owner
approval. This test checks the other way: a `whitelist` node, where the owner adds each
member before the member subscribes.

## Result

| Step | Expected | Actual | Pass |
|------|----------|--------|------|
| 1. Alice creates a `whitelist` node with `pubsub#type` = `urn:xmpp:spaces:0` | result | result | yes |
| 2. Bob subscribes before he is a member | forbidden | forbidden | yes |
| 3. Alice sets bob's affiliation to `member` | result | result | yes |
| 4. Bob subscribes | subscribed | subscribed | yes |
| 5. Alice lists the subscriptions (owner query) | bob subscribed | bob subscribed | yes |
| 6. Alice deletes the node | result | result | yes |

## Stanzas

The node name was `space-private-10774`.

### 1. Create

```xml
<iq type='set' from='alice@chord.localhost/probe' to='pubsub.chord.localhost' id='p1'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub'>
    <create node='space-private-10774'/>
    <configure>
      <x xmlns='jabber:x:data' type='submit'>
        <field var='FORM_TYPE' type='hidden'><value>http://jabber.org/protocol/pubsub#node_config</value></field>
        <field var='pubsub#access_model'><value>whitelist</value></field>
        <field var='pubsub#type'><value>urn:xmpp:spaces:0</value></field>
      </x>
    </configure>
  </pubsub>
</iq>
```

```xml
<iq id='p1' type='result' from='pubsub.chord.localhost' to='alice@chord.localhost/probe'/>
```

### 2. Bob subscribes before he is a member

```xml
<iq type='set' from='bob@chord.localhost/probe' to='pubsub.chord.localhost' id='p2'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub'>
    <subscribe node='space-private-10774' jid='bob@chord.localhost'/>
  </pubsub>
</iq>
```

```xml
<iq id='p2' type='error' from='pubsub.chord.localhost' to='bob@chord.localhost/probe'>
  <error type='auth'><forbidden xmlns='urn:ietf:params:xml:ns:xmpp-stanzas'/></error>
</iq>
```

### 3. Alice makes bob a member

```xml
<iq type='set' from='alice@chord.localhost/probe' to='pubsub.chord.localhost' id='p3'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <affiliations node='space-private-10774'>
      <affiliation jid='bob@chord.localhost' affiliation='member'/>
    </affiliations>
  </pubsub>
</iq>
```

```xml
<iq id='p3' type='result' from='pubsub.chord.localhost' to='alice@chord.localhost/probe'/>
```

### 4. Bob subscribes as a member

```xml
<iq type='set' from='bob@chord.localhost/probe' to='pubsub.chord.localhost' id='p4'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub'>
    <subscribe node='space-private-10774' jid='bob@chord.localhost'/>
  </pubsub>
</iq>
```

```xml
<iq id='p4' type='result' from='pubsub.chord.localhost' to='bob@chord.localhost/probe'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub'>
    <subscription jid='bob@chord.localhost' subscription='subscribed' node='space-private-10774'/>
  </pubsub>
</iq>
```

### 5. Alice lists the subscriptions

```xml
<iq type='get' from='alice@chord.localhost/probe' to='pubsub.chord.localhost' id='p5'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <subscriptions node='space-private-10774'/>
  </pubsub>
</iq>
```

```xml
<iq id='p5' type='result' from='pubsub.chord.localhost' to='alice@chord.localhost/probe'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <subscriptions>
      <subscription jid='bob@chord.localhost' subscription='subscribed' node='space-private-10774'/>
    </subscriptions>
  </pubsub>
</iq>
```

### 6. Delete

```xml
<iq type='set' from='alice@chord.localhost/probe' to='pubsub.chord.localhost' id='p6'>
  <pubsub xmlns='http://jabber.org/protocol/pubsub#owner'>
    <delete node='space-private-10774'/>
  </pubsub>
</iq>
```

```xml
<iq id='p6' type='result' from='pubsub.chord.localhost' to='alice@chord.localhost/probe'/>
```
