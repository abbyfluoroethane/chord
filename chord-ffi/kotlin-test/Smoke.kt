// Smoke test for the generated Kotlin bindings. It runs offline: no server is needed.
// dev/ffi-bindgen-check.sh compiles this file together with the generated chord_ffi.kt.
import kotlinx.coroutines.runBlocking
import uniffi.chord_ffi.ChordClient
import kotlin.system.exitProcess

fun main(args: Array<String>) {
    val dbPath = args.firstOrNull() ?: error("usage: Smoke <db-path>")
    ChordClient(dbPath, "alice@chord.localhost").use { client ->
        check(client.account() == "alice@chord.localhost") { "unexpected account: ${client.account()}" }
        // Both calls read the local store and work offline. They are suspend funs.
        val contacts = runBlocking { client.contacts() }
        check(contacts.isEmpty()) { "a new store must have no contacts" }
        val registrations = runBlocking { client.pushRegistrations() }
        check(registrations.isEmpty()) { "a new store must have no push registrations" }
    }
    println("OK")
    // The client owns a runtime with threads. Exit at once.
    exitProcess(0)
}
