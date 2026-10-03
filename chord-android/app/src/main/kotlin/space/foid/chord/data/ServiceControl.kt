package space.foid.chord.data

/** Starts and stops the connection service. */
interface ServiceControl {
    fun start()

    fun stop()
}
