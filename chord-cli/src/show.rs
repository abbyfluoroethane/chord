//! Human and JSON forms of the view items and diffs.

use chord_core::views::{
    ChannelItem, ChannelKind, DeliveryStatus, ListDiff, MemberItem, SpaceItem, TimelineItem,
};

use crate::json::Obj;

/// A view item that the CLI can print.
pub trait Show {
    fn human(&self) -> String;
    fn json(&self) -> String;
}

impl Show for SpaceItem {
    fn human(&self) -> String {
        format!("{}  ({} {})", self.name, self.service, self.node)
    }

    fn json(&self) -> String {
        Obj::new()
            .str("service", &self.service)
            .str("node", &self.node)
            .str("name", &self.name)
            .opt_str("avatar", self.avatar.as_deref())
            .finish()
    }
}

impl Show for ChannelItem {
    fn human(&self) -> String {
        let kind = match self.kind {
            ChannelKind::Direct => "@",
            ChannelKind::Room => "#",
            ChannelKind::PrivateMessage { .. } => "@",
        };
        let in_room = match &self.kind {
            ChannelKind::PrivateMessage { room, .. } => format!(" (in {room})"),
            _ => String::new(),
        };
        let category = self
            .category
            .as_deref()
            .map(|c| format!("[{c}] "))
            .unwrap_or_default();
        let blocked = if self.blocked { "  (blocked)" } else { "" };
        let joined = if self.joined { "" } else { "  (not joined)" };
        let unread = if self.unread > 0 {
            format!("  [{} unread]", self.unread)
        } else {
            String::new()
        };
        format!(
            "{category}{kind}{}{in_room}  <{}>{joined}{blocked}{unread}",
            self.name, self.jid
        )
    }

    fn json(&self) -> String {
        let (room, nick) = match &self.kind {
            ChannelKind::PrivateMessage { room, nick } => {
                (Some(room.as_str()), Some(nick.as_str()))
            }
            _ => (None, None),
        };
        Obj::new()
            .str("jid", &self.jid)
            .str("name", &self.name)
            .str(
                "kind",
                match self.kind {
                    ChannelKind::Direct => "direct",
                    ChannelKind::Room => "room",
                    ChannelKind::PrivateMessage { .. } => "private",
                },
            )
            .opt_str("room", room)
            .opt_str("nick", nick)
            .opt_str("category", self.category.as_deref())
            .bool("joined", self.joined)
            .opt_num("last_activity", self.last_activity)
            .num("unread", i64::from(self.unread))
            .bool("blocked", self.blocked)
            .finish()
    }
}

impl Show for TimelineItem {
    fn human(&self) -> String {
        let time = chrono::DateTime::from_timestamp_millis(self.timestamp)
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        let mut text = if self.retracted {
            "(retracted)".to_owned()
        } else {
            self.body.clone()
        };
        if self.edited && !self.retracted {
            text.push_str(" (edited)");
        }
        if let Some(url) = &self.attachment
            && *url != self.body
        {
            text.push_str(&format!(" [file: {url}]"));
        }
        if !self.reactions.is_empty() {
            let list: Vec<String> = self
                .reactions
                .iter()
                .map(|r| format!("{} {}", r.emoji, r.count))
                .collect();
            text.push_str(&format!("  [{}]", list.join(", ")));
        }
        if self.outgoing && self.status != DeliveryStatus::Sent {
            text.push_str(match self.status {
                DeliveryStatus::Displayed => " (read)",
                _ => " (delivered)",
            });
        }
        let quote = self
            .reply_to
            .as_ref()
            .map(|r| format!("> {}: {}\n", r.sender_name, r.body))
            .unwrap_or_default();
        if self.same_sender_as_previous && quote.is_empty() {
            format!("                   {text}")
        } else {
            format!("{quote}{time}  {}: {text}", self.sender_name)
        }
    }

    fn json(&self) -> String {
        Obj::new()
            .str("id", &self.id)
            .opt_str("stanza_id", self.stanza_id.as_deref())
            .opt_str("origin_id", self.origin_id.as_deref())
            .str("sender", &self.sender)
            .str("sender_name", &self.sender_name)
            .opt_str("avatar", self.avatar.as_deref())
            .str("body", &self.body)
            .num("timestamp", self.timestamp)
            .bool("outgoing", self.outgoing)
            .bool("same_sender_as_previous", self.same_sender_as_previous)
            .bool("edited", self.edited)
            .bool("retracted", self.retracted)
            .raw(
                "reactions",
                &crate::json::array(self.reactions.iter().map(|r| {
                    Obj::new()
                        .str("emoji", &r.emoji)
                        .num("count", i64::from(r.count))
                        .bool("mine", r.mine)
                        .finish()
                })),
            )
            .raw(
                "reply_to",
                &self.reply_to.as_ref().map_or_else(
                    || "null".to_owned(),
                    |r| {
                        Obj::new()
                            .opt_str("id", r.id.as_deref())
                            .str("sender_name", &r.sender_name)
                            .str("body", &r.body)
                            .finish()
                    },
                ),
            )
            .opt_str("attachment", self.attachment.as_deref())
            .str(
                "status",
                match self.status {
                    DeliveryStatus::Sent => "sent",
                    DeliveryStatus::Received => "received",
                    DeliveryStatus::Displayed => "displayed",
                },
            )
            .finish()
    }
}

impl Show for MemberItem {
    fn human(&self) -> String {
        let status = match (self.online, self.show.as_deref()) {
            (false, _) => "offline".to_owned(),
            (true, None) => "online".to_owned(),
            (true, Some(show)) => show.to_owned(),
        };
        format!(
            "{}  ({}, {}, {status})",
            self.name, self.role, self.affiliation
        )
    }

    fn json(&self) -> String {
        Obj::new()
            .str("id", &self.id)
            .str("name", &self.name)
            .opt_str("jid", self.jid.as_deref())
            .str("role", &self.role)
            .str("affiliation", &self.affiliation)
            .opt_str("show", self.show.as_deref())
            .bool("online", self.online)
            .opt_str("avatar", self.avatar.as_deref())
            .finish()
    }
}

/// A diff as one JSON line, as a UI gets it.
pub fn diff_json<T: Show>(diff: &ListDiff<T>) -> String {
    match diff {
        ListDiff::Insert { index, item } => Obj::new()
            .str("op", "insert")
            .num("index", *index as i64)
            .raw("item", &item.json())
            .finish(),
        ListDiff::Update { index, item } => Obj::new()
            .str("op", "update")
            .num("index", *index as i64)
            .raw("item", &item.json())
            .finish(),
        ListDiff::Remove { index } => Obj::new()
            .str("op", "remove")
            .num("index", *index as i64)
            .finish(),
        ListDiff::Reset(items) => Obj::new()
            .str("op", "reset")
            .raw("items", &crate::json::array(items.iter().map(Show::json)))
            .finish(),
    }
}

/// A diff in human form.
pub fn diff_human<T: Show>(diff: &ListDiff<T>) -> String {
    match diff {
        ListDiff::Insert { index, item } => format!("+ [{index}] {}", item.human()),
        ListDiff::Update { index, item } => format!("~ [{index}] {}", item.human()),
        ListDiff::Remove { index } => format!("- [{index}]"),
        ListDiff::Reset(items) => {
            let mut out = format!("= {} items", items.len());
            for item in items {
                out.push_str("\n  ");
                out.push_str(&item.human());
            }
            out
        }
    }
}
