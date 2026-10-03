package space.foid.chord

import android.app.Application
import space.foid.chord.data.ChordSession

/** STUB for compiling the service. The integrator replaces it with the real ChordApp. */
class ChordApp : Application() {
    lateinit var session: ChordSession
}
