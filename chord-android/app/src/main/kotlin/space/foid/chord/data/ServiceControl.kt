package space.foid.chord.data

/**
 * Starts and stops the foreground service that keeps the connection alive.
 *
 * [ChordSession] calls [start] after a sign-in succeeds and [stop] when the user signs out.
 * The implementation lives next to the service (it needs a Context). Both calls must be
 * idempotent and safe to call from any thread.
 */
interface ServiceControl {
    /** Start the connection service, if it does not run. */
    fun start()

    /** Stop the connection service, if it runs. */
    fun stop()
}
