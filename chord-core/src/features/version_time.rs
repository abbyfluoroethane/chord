//! Software version (XEP-0092) and entity time (XEP-0202): answers to queries to us.
//!
//! The version answer has the name `Chord` and the crate version. It has no operating
//! system: a stranger does not need it. The time answer has the UTC time and our offset.
//!
//! Both are the entity info that the user can turn off (`ClientHandle::set_share_info`).
//! Then the feature names leave our caps too (disco.rs), and a query gets
//! `service-unavailable`. XEP-0012 (last activity) is not here: it would show when the user
//! was last active. Idle time (XEP-0319) is opt-in already.

use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;

use super::{Ctx, result_reply};

const NS_VERSION: &str = "jabber:iq:version";
const NS_TIME: &str = "urn:xmpp:time";

/// The version that we report: the version of the crate.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Answer a version or time query. Returns false for any other IQ, and for these two when
/// the user turned the answers off.
pub(crate) fn on_iq(ctx: &mut Ctx<'_>, iq: &Iq) -> bool {
    let Iq::Get { payload, .. } = iq else {
        return false;
    };
    if ctx.state.disco.hide_info {
        return false;
    }
    let answer = if payload.is("query", NS_VERSION) {
        version()
    } else if payload.is("time", NS_TIME) {
        let (utc_secs, local_secs) = clock(ctx);
        time(utc_secs, local_secs - utc_secs)
    } else {
        return false;
    };
    ctx.send(result_reply(iq, Some(answer)));
    true
}

/// The `query` element of XEP-0092: no `os` child.
fn version() -> Element {
    let xml = format!(
        "<query xmlns='{NS_VERSION}'><name>Chord</name><version>{VERSION}</version></query>"
    );
    xml.parse().expect("a valid version element")
}

/// The `time` element of XEP-0202. `offset_secs` is local time minus UTC.
fn time(utc_secs: i64, offset_secs: i64) -> Element {
    let utc = super::presence::iso_utc(utc_secs);
    let tzo = format_offset(offset_secs);
    let xml = format!("<time xmlns='{NS_TIME}'><tzo>{tzo}</tzo><utc>{utc}</utc></time>");
    xml.parse().expect("a valid time element")
}

/// `+HH:MM` for a number of seconds east of UTC. A part of a minute does not show.
fn format_offset(offset_secs: i64) -> String {
    let minutes = offset_secs / 60;
    let sign = if minutes < 0 { '-' } else { '+' };
    let minutes = minutes.abs();
    format!("{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}

/// The time now as Unix seconds in UTC, and the local wall clock as seconds on the same
/// scale. SQLite gives both (the core avoids `SystemTime` for wasm). Without a time zone,
/// both are equal.
fn clock(ctx: &Ctx<'_>) -> (i64, i64) {
    ctx.store
        .conn()
        .query_row(
            "SELECT CAST(strftime('%s', 'now') AS INTEGER),
                    CAST(strftime('%s', 'now', 'localtime') AS INTEGER)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use jid::Jid;
    use xmpp_parsers::time::{TimeQuery, TimeResult};
    use xmpp_parsers::version::{VersionQuery, VersionResult};

    use super::*;
    use crate::features::testing::Harness;

    fn ask(h: &mut Harness, payload: Element) -> Option<Iq> {
        let mut iq = Iq::Get {
            from: Some(Jid::new("bob@chord.localhost/x").unwrap()),
            to: None,
            id: "q1".into(),
            payload,
        };
        *iq.id_mut() = "q1".into();
        let handled = h.with_ctx(|ctx| on_iq(ctx, &iq));
        let sent = h.sent_iqs();
        assert_eq!(handled, !sent.is_empty());
        sent.into_iter().next()
    }

    #[test]
    fn version_has_the_name_and_the_crate_version_and_no_os() {
        let mut h = Harness::new();
        let Some(Iq::Result {
            payload: Some(p), ..
        }) = ask(&mut h, VersionQuery.into())
        else {
            panic!("no result")
        };
        let result = VersionResult::try_from(p.clone()).unwrap();
        assert_eq!(result.name, "Chord");
        assert_eq!(result.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(result.os, None);
        assert!(p.children().all(|c| c.name() != "os"));
    }

    #[test]
    fn time_has_utc_and_an_offset() {
        let mut h = Harness::new();
        let Some(Iq::Result {
            payload: Some(p), ..
        }) = ask(&mut h, TimeQuery.into())
        else {
            panic!("no result")
        };
        let result = TimeResult::try_from(p).unwrap();
        // 2023 or later, and the offset is a whole number of minutes.
        assert!(result.utc.timestamp() > 1_700_000_000);
        assert_eq!(result.tz_offset.local_minus_utc() % 60, 0);
    }

    #[test]
    fn the_offset_has_a_sign_and_minutes() {
        assert_eq!(format_offset(0), "+00:00");
        assert_eq!(format_offset(2 * 3600), "+02:00");
        assert_eq!(format_offset(-(5 * 3600 + 30 * 60)), "-05:30");
        assert_eq!(format_offset(5 * 3600 + 45 * 60), "+05:45");
        let xml = time(1_700_000_000, -3600);
        assert_eq!(
            xml.get_child("utc", NS_TIME).unwrap().text(),
            "2023-11-14T22:13:20Z"
        );
        assert_eq!(xml.get_child("tzo", NS_TIME).unwrap().text(), "-01:00");
    }

    #[test]
    fn a_user_who_turned_it_off_gets_no_answer_from_here() {
        let mut h = Harness::new();
        h.state.disco.hide_info = true;
        assert!(ask(&mut h, VersionQuery.into()).is_none());
        assert!(ask(&mut h, TimeQuery.into()).is_none());
    }
}
