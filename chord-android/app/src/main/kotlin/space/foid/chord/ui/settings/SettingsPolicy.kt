package space.foid.chord.ui.settings

import uniffi.chord_ffi.Availability
import uniffi.chord_ffi.OwnPresence

/** The calls that the sign-in presence and the idle report need. The real one is on the client. */
interface PresenceApi {
    suspend fun ownPresence(): OwnPresence
    suspend fun setPresence(availability: Availability, status: String?)
    suspend fun setIdle(since: Long?)
}

/** The availability for a sign-in choice, or null for [SignInShow.Last]. */
fun SignInShow.availability(): Availability? = when (this) {
    SignInShow.Last -> null
    SignInShow.Chat -> Availability.AVAILABLE
    SignInShow.Away -> Availability.AWAY
    SignInShow.Dnd -> Availability.DND
}

/**
 * The presence to use after sign-in: the stored one, with the two sign-in settings laid over it.
 * Same rule as `signInPresence` on the desktop.
 */
fun signInPresence(stored: OwnPresence, prefs: AppPrefs): OwnPresence {
    val text = prefs.signInStatus.trim()
    return OwnPresence(
        prefs.signInShow.availability() ?: stored.availability,
        if (text.isNotEmpty()) text.take(MAX_STATUS) else stored.status,
    )
}

/**
 * Apply the sign-in settings. Reads the stored presence and sets a new one only when it differs.
 * Returns true when it set a presence.
 */
suspend fun applySignInPresence(api: PresenceApi, prefs: AppPrefs): Boolean {
    if (prefs.signInShow == SignInShow.Last && prefs.signInStatus.isBlank()) return false
    val stored = api.ownPresence()
    val next = signInPresence(stored, prefs)
    if (next == stored) return false
    api.setPresence(next.availability, next.status)
    return true
}

/** Whether [nowMinute] (minutes after midnight) is inside the quiet hours of [p]. */
fun inQuietHours(p: AppPrefs, nowMinute: Int): Boolean {
    if (!p.quietHours || p.quietFrom == p.quietTo) return false
    return if (p.quietFrom < p.quietTo) nowMinute in p.quietFrom until p.quietTo
    else nowMinute >= p.quietFrom || nowMinute < p.quietTo
}

/** "22:00" for the 24 hour clock, "10:00 PM" for the 12 hour clock. */
fun formatMinute(minute: Int, is24: Boolean): String {
    val h = minute / 60
    val m = minute % 60
    if (is24) return "%02d:%02d".format(h, m)
    val h12 = if (h % 12 == 0) 12 else h % 12
    return "%d:%02d %s".format(h12, m, if (h < 12) "AM" else "PM")
}
