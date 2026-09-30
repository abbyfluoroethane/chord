//! Join a room again after the chat service removed us for a reason that passes: it shuts
//! down (status 332) or it had an error (333). The wait doubles with each try, and the
//! tries stop after five.

use std::collections::HashMap;

use jid::BareJid;

use super::{Ctx, join_room};
use crate::actor::ClientEvent;

/// Ticks (about 15 s each) before the first try. The wait doubles with each try.
const FIRST_WAIT_TICKS: u16 = 2;
pub(super) const MAX_TRIES: u8 = 5;

#[derive(Debug)]
struct Rejoin {
    /// Ticks until the next try.
    wait: u16,
    tries: u8,
}

#[derive(Debug, Default)]
pub(super) struct Rejoins(HashMap<BareJid, Rejoin>);

/// Join `room` again soon.
pub(super) fn schedule(ctx: &mut Ctx<'_>, room: &BareJid) {
    ctx.state.muc.rejoin.0.insert(
        room.clone(),
        Rejoin {
            wait: FIRST_WAIT_TICKS,
            tries: 0,
        },
    );
}

/// Stop waiting for `room`: we are in it again, we left it, or it is gone.
pub(super) fn cancel(ctx: &mut Ctx<'_>, room: &BareJid) {
    ctx.state.muc.rejoin.0.remove(room);
}

/// One tick: join the rooms whose wait is over.
pub(super) fn on_tick(ctx: &mut Ctx<'_>) {
    let due: Vec<BareJid> = ctx
        .state
        .muc
        .rejoin
        .0
        .iter_mut()
        .filter_map(|(room, rejoin)| {
            rejoin.wait = rejoin.wait.saturating_sub(1);
            (rejoin.wait == 0).then(|| room.clone())
        })
        .collect();
    for room in due {
        if ctx.state.muc.nicks.contains_key(&room) {
            ctx.state.muc.rejoin.0.remove(&room);
            continue;
        }
        let Some(rejoin) = ctx.state.muc.rejoin.0.get_mut(&room) else {
            continue;
        };
        if ctx.state.muc.joins.contains_key(&room) {
            // A join runs: look again at the next tick.
            rejoin.wait = 1;
            continue;
        }
        rejoin.tries += 1;
        if rejoin.tries > MAX_TRIES {
            ctx.state.muc.rejoin.0.remove(&room);
            ctx.emit(ClientEvent::Notice(format!(
                "Chord could not join {room} again. Join it by hand when the chat service is back"
            )));
            continue;
        }
        rejoin.wait = FIRST_WAIT_TICKS << rejoin.tries;
        log::info!("join {room} again after the service removed us");
        join_room(ctx, &room, None, None, None);
    }
}

#[cfg(test)]
impl Rejoins {
    pub(super) fn contains(&self, room: &BareJid) -> bool {
        self.0.contains_key(room)
    }
}
