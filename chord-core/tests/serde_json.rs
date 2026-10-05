//! JSON snapshot test of the `serde` feature. The names here must match
//! chord-desktop/src/lib/chord/types.ts. Run it with
//! `cargo test -p chord-core --features serde --test serde_json`.
#![cfg(feature = "serde")]

use chord_core::actor::{ClientEvent, ConnectionState};
use chord_core::features::muc::RoomSettings;
use chord_core::features::notify::Notification;
use chord_core::jid::{BareJid, Jid};
use chord_core::session::{AuthFailure, ConnectError};
use chord_core::views::{
    ChannelKind, ChannelScope, DeliveryStatus, Encryption, ListDiff, ReactionSummary, ReplyPreview,
    TimelineItem,
};
use serde_json::json;

fn item() -> TimelineItem {
    TimelineItem {
        id: "m:7".into(),
        stanza_id: Some("s1".into()),
        origin_id: None,
        sender: "room@muc.example/bob".into(),
        sender_name: "bob".into(),
        avatar: None,
        body: "hi".into(),
        timestamp: 1_700_000_000_000,
        outgoing: false,
        same_sender_as_previous: true,
        edited: false,
        retracted: false,
        reactions: vec![ReactionSummary {
            emoji: "+1".into(),
            count: 2,
            mine: true,
        }],
        reply_to: Some(ReplyPreview {
            id: Some("m:3".into()),
            sender_name: "amy".into(),
            body: "yo".into(),
        }),
        attachment: None,
        attachment_info: None,
        status: DeliveryStatus::Displayed,
        encryption: Encryption::Omemo,
    }
}

#[test]
fn timeline_item_uses_camel_case_names() {
    let value = serde_json::to_value(item()).unwrap();
    assert_eq!(
        value,
        json!({
            "id": "m:7",
            "stanzaId": "s1",
            "originId": null,
            "sender": "room@muc.example/bob",
            "senderName": "bob",
            "avatar": null,
            "body": "hi",
            "timestamp": 1_700_000_000_000_i64,
            "outgoing": false,
            "sameSenderAsPrevious": true,
            "edited": false,
            "retracted": false,
            "reactions": [{"emoji": "+1", "count": 2, "mine": true}],
            "replyTo": {"id": "m:3", "senderName": "amy", "body": "yo"},
            "attachment": null,
            "attachmentInfo": null,
            "status": "displayed",
            "encryption": "omemo"
        })
    );
}

#[test]
fn an_untrusted_omemo_message_has_its_own_name() {
    let mut item = item();
    item.encryption = Encryption::OmemoUntrusted;
    let value = serde_json::to_value(item).unwrap();
    assert_eq!(value["encryption"], json!("omemo-untrusted"));
}

#[test]
fn list_diffs_are_tagged() {
    let insert = serde_json::to_value(ListDiff::Insert {
        index: 2,
        item: 5u32,
    })
    .unwrap();
    assert_eq!(insert, json!({"type": "insert", "index": 2, "item": 5}));
    let remove = serde_json::to_value(ListDiff::<u32>::Remove { index: 1 }).unwrap();
    assert_eq!(remove, json!({"type": "remove", "index": 1}));
    let reset = serde_json::to_value(ListDiff::Reset(vec![1u32, 2])).unwrap();
    assert_eq!(reset, json!({"type": "reset", "items": [1, 2]}));
}

#[test]
fn client_events_are_adjacently_tagged() {
    let bound: Jid = "amy@example.org/chord".parse().unwrap();
    let connected = ClientEvent::ConnectionState(ConnectionState::Connected {
        bound_jid: bound,
        resumed: false,
    });
    assert_eq!(
        serde_json::to_value(connected).unwrap(),
        json!({"type": "connectionState", "data": {
            "type": "connected",
            "data": {"boundJid": "amy@example.org/chord", "resumed": false}
        }})
    );
    let failed = ClientEvent::ConnectionState(ConnectionState::LoginFailed(
        ConnectError::AuthFailed(AuthFailure::NoMechanism),
    ));
    assert_eq!(
        serde_json::to_value(failed).unwrap(),
        json!({"type": "connectionState", "data": {
            "type": "loginFailed",
            "data": {"type": "authFailed", "data": {"type": "noMechanism"}}
        }})
    );
    let typing = ClientEvent::Typing {
        peer: "room@muc.example".into(),
        typers: vec!["bob".into()],
    };
    assert_eq!(
        serde_json::to_value(typing).unwrap(),
        json!({"type": "typing", "data": {"peer": "room@muc.example", "typers": ["bob"]}})
    );
    let notice = ClientEvent::Notice("hello".into());
    assert_eq!(
        serde_json::to_value(notice).unwrap(),
        json!({"type": "notice", "data": "hello"})
    );
    let room: BareJid = "room@muc.example".parse().unwrap();
    let notification = ClientEvent::Notification(Notification {
        peer: "room@muc.example".into(),
        room: Some(room),
        sender: "room@muc.example/bob".into(),
        sender_name: "bob".into(),
        body_preview: "hi".into(),
        mention: true,
        item_id: "m:7".into(),
    });
    assert_eq!(
        serde_json::to_value(notification).unwrap(),
        json!({"type": "notification", "data": {
            "peer": "room@muc.example", "room": "room@muc.example",
            "sender": "room@muc.example/bob", "senderName": "bob",
            "bodyPreview": "hi", "mention": true, "itemId": "m:7"
        }})
    );
}

#[test]
fn values_from_the_ui_deserialize() {
    let scope: ChannelScope = serde_json::from_value(json!({"type": "home"})).unwrap();
    assert_eq!(scope, ChannelScope::Home);
    let scope: ChannelScope =
        serde_json::from_value(json!({"type": "space", "service": "s", "node": "n"})).unwrap();
    assert_eq!(
        scope,
        ChannelScope::Space {
            service: "s".into(),
            node: "n".into()
        }
    );
    let settings: RoomSettings = serde_json::from_value(json!({"membersOnly": true})).unwrap();
    assert_eq!(settings.members_only, Some(true));
    assert_eq!(settings.name, None);
    let kind = serde_json::to_value(ChannelKind::PrivateMessage {
        room: "r@m".into(),
        nick: "bob".into(),
    })
    .unwrap();
    assert_eq!(
        kind,
        json!({"type": "privateMessage", "room": "r@m", "nick": "bob"})
    );
}

#[test]
fn omemo_events_and_values() {
    use chord_core::features::omemo::{OmemoDevice, OmemoTrust};
    let jid: BareJid = "bob@example.org".parse().unwrap();
    assert_eq!(
        serde_json::to_value(ClientEvent::OmemoKeysReset).unwrap(),
        json!({"type": "omemoKeysReset"})
    );
    assert_eq!(
        serde_json::to_value(ClientEvent::OmemoTrustNeeded {
            jid: jid.clone(),
            device_id: 7,
            fingerprint: "aa".into(),
        })
        .unwrap(),
        json!({"type": "omemoTrustNeeded", "data": {
            "jid": "bob@example.org", "deviceId": 7, "fingerprint": "aa"
        }})
    );
    assert_eq!(
        serde_json::to_value(ClientEvent::OmemoDevicesChanged(jid.clone())).unwrap(),
        json!({"type": "omemoDevicesChanged", "data": "bob@example.org"})
    );
    assert_eq!(
        serde_json::to_value(ClientEvent::OmemoDevicesSkipped {
            peer: jid.clone(),
            device_ids: vec![7, 9],
        })
        .unwrap(),
        json!({"type": "omemoDevicesSkipped", "data": {
            "peer": "bob@example.org", "deviceIds": [7, 9]
        }})
    );
    assert_eq!(
        serde_json::to_value(ClientEvent::OmemoSendFailed {
            peer: jid,
            message_id: "m1".into(),
            reason: "no device".into(),
        })
        .unwrap(),
        json!({"type": "omemoSendFailed", "data": {
            "peer": "bob@example.org", "messageId": "m1", "reason": "no device"
        }})
    );
    let device = OmemoDevice {
        jid: "bob@example.org".into(),
        device_id: 7,
        fingerprint: Some("aa".into()),
        label: None,
        trust: Some(OmemoTrust::Blind),
        active: true,
        own: false,
        this_device: false,
    };
    assert_eq!(
        serde_json::to_value(device).unwrap(),
        json!({
            "jid": "bob@example.org", "deviceId": 7, "fingerprint": "aa", "label": null,
            "trust": "blind", "active": true, "own": false, "thisDevice": false
        })
    );
    let trust: OmemoTrust = serde_json::from_value(json!("verified")).unwrap();
    assert_eq!(trust, OmemoTrust::Verified);
}
