package space.foid.chord.viewmodel

import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.ConnectFailure

/** Plain-English text for a failed login of the core ([ConnectFailure]). */
fun describeFailure(failure: ConnectFailure): String = when (failure) {
    is ConnectFailure.AuthFailed -> "The server did not accept the address or the password. Check both and try again."
    is ConnectFailure.Unreachable -> "Could not reach the server. Check your network and the server name."
    is ConnectFailure.TlsInvalid -> "The security certificate of the server is not valid, so Chord did not connect."
    is ConnectFailure.Timeout -> "The server took too long to answer. Try again."
}

/** Plain-English text for an error that a call on the core threw ([ChordException]), or any other error. */
fun describeError(error: Throwable): String = when (error) {
    is ChordException.AuthFailed -> describeFailure(ConnectFailure.AuthFailed(error.detail))
    is ChordException.Unreachable -> describeFailure(ConnectFailure.Unreachable(error.detail))
    is ChordException.TlsInvalid -> describeFailure(ConnectFailure.TlsInvalid(error.detail))
    is ChordException.Timeout -> describeFailure(ConnectFailure.Timeout)
    is ChordException.InvalidJid -> "That is not a valid address. Use the form name@server.example."
    is ChordException.InvalidServer -> "The server setting is not valid. Leave it empty, or use host or starttls://host:port."
    is ChordException.Store -> "Chord could not open its stored data for this account."
    is ChordException.NotConnected -> "You are not connected to the server."
    is ChordException.ActorGone, is ChordException.Session -> "The connection was closed. Try again."
    is ChordException.Unsupported -> "The server does not support this."
    is ChordException.Server -> "The server refused the request."
    is ChordException.Invalid -> "Chord could not do this: the request is not valid."
    else -> "Something went wrong. Try again."
}
