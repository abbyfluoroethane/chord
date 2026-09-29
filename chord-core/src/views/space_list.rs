//! `SpaceList`: the spaces for the space rail.

use rusqlite::params;

use super::{QueryCtx, ViewItem};

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct SpaceItem {
    /// Pubsub service JID.
    pub service: String,
    pub node: String,
    pub name: String,
    /// Avatar hash, if the space has one.
    pub avatar: Option<String>,
}

impl ViewItem for SpaceItem {
    type Key = (String, String);
    fn key(&self) -> (String, String) {
        (self.service.clone(), self.node.clone())
    }
}

/// The spaces that the account follows, in rail order. A space avatar has the owner
/// `service/node` in the avatars table.
pub(crate) fn query(q: &QueryCtx<'_>) -> rusqlite::Result<Vec<SpaceItem>> {
    let mut stmt = q.store.conn().prepare_cached(
        "SELECT s.service, s.node, s.name, a.hash FROM spaces s
         LEFT JOIN avatars a ON a.account_id = s.account_id
                            AND a.owner = s.service || '/' || s.node
         WHERE s.account_id = ?1 AND s.subscribed = 1
         ORDER BY s.position, s.name, s.node",
    )?;
    let rows = stmt.query_map(params![q.account_id], |row| {
        let node: String = row.get(1)?;
        let name: Option<String> = row.get(2)?;
        Ok(SpaceItem {
            service: row.get(0)?,
            name: name.unwrap_or_else(|| node.clone()),
            node,
            avatar: row.get(3)?,
        })
    })?;
    rows.collect()
}
