//! The password of a room. With a secret store (the system keychain on the desktop) the
//! password lives there and the `rooms.password` column stays empty. With no store, the
//! column holds it, as before.
//!
//! A password that an older version left in the column moves to the keychain at its next
//! use. The column clears only after the keychain has taken the password, so a failure
//! loses nothing.
//!
//! `rooms.password_shared` says that the bookmark carries the password: the user agreed, or
//! another client published it. A bookmark that we publish carries the password only then.

use jid::BareJid;
use rusqlite::{OptionalExtension, params};

use super::{Ctx, db};
use crate::secrets::room_password_key;

fn key(ctx: &Ctx<'_>, room: &BareJid) -> String {
    room_password_key(ctx.account.as_str(), room.as_str())
}

/// Put `password` in the keychain. Returns false when there is no keychain, or when it
/// refuses: the caller then keeps the password in the database.
pub(super) fn keep(ctx: &Ctx<'_>, room: &BareJid, password: &str) -> bool {
    let Some(secrets) = &ctx.state.muc.secrets else {
        return false;
    };
    match secrets.set(&key(ctx, room), password) {
        Ok(()) => true,
        Err(e) => {
            log::warn!("cannot keep the password of {room} in the keychain: {e}");
            false
        }
    }
}

/// The password of `room`. `column` is what the `rooms` row holds.
pub(super) fn load(ctx: &Ctx<'_>, room: &BareJid, column: Option<String>) -> Option<String> {
    let Some(secrets) = &ctx.state.muc.secrets else {
        return column;
    };
    match secrets.get(&key(ctx, room)) {
        Ok(Some(password)) => {
            // The keychain is the truth. A value in the column is a leftover.
            if column.is_some() {
                clear_column(ctx, room);
            }
            Some(password)
        }
        Ok(None) => {
            let password = column?;
            // An old row: move the password over, and clear the column only when the
            // keychain has it.
            if keep(ctx, room, &password) {
                clear_column(ctx, room);
            }
            Some(password)
        }
        Err(e) => {
            log::warn!("cannot read the password of {room} in the keychain: {e}");
            column
        }
    }
}

pub(super) fn clear_column(ctx: &Ctx<'_>, room: &BareJid) {
    db(
        ctx,
        "clear a room password",
        ctx.store.conn().execute(
            "UPDATE rooms SET password = NULL WHERE account_id = ?1 AND jid = ?2",
            params![ctx.account_id, room.as_str()],
        ),
    );
}

/// Drop the password of `room` from the keychain and from the database.
pub(super) fn forget(ctx: &Ctx<'_>, room: &BareJid) {
    if let Some(secrets) = &ctx.state.muc.secrets
        && let Err(e) = secrets.delete(&key(ctx, room))
    {
        log::warn!("cannot delete the password of {room} in the keychain: {e}");
    }
    clear_column(ctx, room);
}

/// Whether the bookmark of `room` carries the password.
pub(super) fn is_shared(ctx: &Ctx<'_>, room: &BareJid) -> bool {
    db(
        ctx,
        "read the password flag",
        ctx.store
            .conn()
            .query_row(
                "SELECT password_shared FROM rooms WHERE account_id = ?1 AND jid = ?2",
                params![ctx.account_id, room.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional(),
    )
    .flatten()
    .is_some_and(|flag| flag != 0)
}

pub(super) fn set_shared(ctx: &Ctx<'_>, room: &BareJid, shared: bool) {
    db(
        ctx,
        "set the password flag",
        ctx.store.conn().execute(
            "UPDATE rooms SET password_shared = ?3 WHERE account_id = ?1 AND jid = ?2",
            params![ctx.account_id, room.as_str(), shared],
        ),
    );
}

/// The bookmark of `room` came from another client (or from our own publish, echoed back).
/// A password in it is stored and marked as shared. A bookmark with no password removes
/// the password only when an earlier bookmark had carried it: a password that the user
/// typed here is ours, and another client cannot know it.
pub(super) fn on_bookmark(ctx: &Ctx<'_>, room: &BareJid, password: Option<&str>) {
    match password {
        Some(password) => {
            if keep(ctx, room, password) {
                clear_column(ctx, room);
            } else {
                db(
                    ctx,
                    "store a room password",
                    ctx.store.conn().execute(
                        "UPDATE rooms SET password = ?3 WHERE account_id = ?1 AND jid = ?2",
                        params![ctx.account_id, room.as_str(), password],
                    ),
                );
            }
            set_shared(ctx, room, true);
        }
        None => {
            if is_shared(ctx, room) {
                forget(ctx, room);
                set_shared(ctx, room, false);
            }
        }
    }
}
