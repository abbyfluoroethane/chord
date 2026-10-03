package space.foid.chord.data

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

/**
 * A list that the core keeps up to date with diffs, as a [StateFlow] of an immutable list.
 *
 * Why an immutable list in a StateFlow, not a SnapshotStateList:
 *  - The diffs arrive on core threads. A StateFlow is safe to write from any thread. A
 *    SnapshotStateList must be written inside a snapshot, and each single write is a
 *    separate change that can recompose.
 *  - One burst of diffs becomes one new list and one recomposition. Rows that did not
 *    change are the same objects, and the LazyColumn gives each row a stable key, so
 *    unchanged rows are skipped.
 *  - The cost is one array copy for each burst (about 20 KB for 5000 rows). That is
 *    small next to the layout work of a frame.
 *
 * Diffs are collected from [submit] (any thread). Diffs that arrive within one frame
 * ([frameMillis]) are joined: the list applies all of them to one working copy and
 * publishes it once, on the dispatcher of [scope] (the main thread in the app). Diffs
 * before the last [ListDiff.Reset] of a burst are dropped without work.
 *
 * Defensive rules: a diff with an index out of range, or an insert or update that
 * would give a second row the same key (a LazyColumn crashes on equal keys), is logged
 * and the burst is dropped. The list keeps its last good content, ignores diffs until the
 * next [ListDiff.Reset], and calls [onDesync] once. The owner must then ask the core
 * for a new subscription, which starts with a Reset. A Reset with duplicate keys keeps the first
 * row of each key.
 *
 * @param key the key of an item. It must stay equal for an item over its life.
 */
class ObservableDiffList<T, K : Any>(
    private val scope: CoroutineScope,
    val key: (T) -> K,
    private val frameMillis: Long = DEFAULT_FRAME_MILLIS,
    private val onDesync: () -> Unit = {},
    private val log: (String) -> Unit = { logWarn(TAG, it) },
) {
    private val lock = Any()
    private val pending = ArrayList<ListDiff<T>>()
    private var flushScheduled = false
    private var desynced = false

    // The state below belongs to the lock. [current] is never changed after it is published.
    private var current: List<T> = emptyList()
    private val keyCounts = HashMap<K, Int>()

    private val _items = MutableStateFlow<List<T>>(emptyList())

    /** The rows. Each value is immutable. */
    val items: StateFlow<List<T>> get() = _items

    private val _loaded = MutableStateFlow(false)

    /** False until the first [ListDiff.Reset] reached [items]. */
    val loaded: StateFlow<Boolean> get() = _loaded

    /** Add a diff. Safe to call from any thread. */
    fun submit(diff: ListDiff<T>) {
        val schedule = synchronized(lock) {
            pending.add(diff)
            (!flushScheduled).also { if (it) flushScheduled = true }
        }
        if (schedule) {
            scope.launch {
                delay(frameMillis)
                flush()
            }
        }
    }

    /** Forget the diffs that wait, and the sync error. Call it before a new subscription opens. */
    fun discardPending() {
        synchronized(lock) {
            pending.clear()
            desynced = false
        }
    }

    /** Empty the list, as before the first Reset. Pending diffs are dropped. */
    fun clear() {
        synchronized(lock) {
            pending.clear()
            desynced = false
            current = emptyList()
            keyCounts.clear()
            _items.value = current
            _loaded.value = false
        }
    }

    private fun flush() {
        var notify = false
        synchronized(lock) {
            flushScheduled = false
            if (pending.isEmpty()) return
            val batch = ArrayList(pending)
            pending.clear()
            // A Reset replaces everything before it.
            val lastReset = batch.indexOfLast { it is ListDiff.Reset }
            val start = when {
                lastReset >= 0 -> lastReset
                desynced -> return // Wait for a Reset.
                else -> 0
            }
            desynced = false
            val work = ArrayList(current)
            var error: String? = null
            for (i in start until batch.size) {
                error = apply(work, batch[i])
                if (error != null) break
            }
            if (error != null) {
                log("$error: drop ${batch.size - start} diffs and ask for a new subscription")
                rebuildCounts(current)
                desynced = true
                notify = true
            } else {
                current = work
                _items.value = work
                if (lastReset >= 0) _loaded.value = true
            }
        }
        if (notify) onDesync()
    }

    /** Apply one diff to [work]. Returns an error text, or null. */
    private fun apply(work: ArrayList<T>, diff: ListDiff<T>): String? {
        when (diff) {
            is ListDiff.Reset -> {
                work.clear()
                keyCounts.clear()
                for (item in diff.items) {
                    val k = key(item)
                    if (keyCounts.containsKey(k)) {
                        log("Reset has a second row with the key $k, skip it")
                        continue
                    }
                    keyCounts[k] = 1
                    work.add(item)
                }
            }
            is ListDiff.Insert -> {
                if (diff.index < 0 || diff.index > work.size) {
                    return "Insert at ${diff.index} of ${work.size}"
                }
                val k = key(diff.item)
                if (keyCounts.containsKey(k)) return "Insert of a second row with the key $k"
                keyCounts[k] = 1
                work.add(diff.index, diff.item)
            }
            is ListDiff.Update -> {
                if (diff.index < 0 || diff.index >= work.size) {
                    return "Update at ${diff.index} of ${work.size}"
                }
                val oldKey = key(work[diff.index])
                val newKey = key(diff.item)
                if (newKey != oldKey) {
                    if (keyCounts.containsKey(newKey)) return "Update to a second row with the key $newKey"
                    keyCounts.remove(oldKey)
                    keyCounts[newKey] = 1
                }
                work[diff.index] = diff.item
            }
            is ListDiff.Remove -> {
                if (diff.index < 0 || diff.index >= work.size) {
                    return "Remove at ${diff.index} of ${work.size}"
                }
                keyCounts.remove(key(work.removeAt(diff.index)))
            }
        }
        return null
    }

    private fun rebuildCounts(list: List<T>) {
        keyCounts.clear()
        for (item in list) keyCounts[key(item)] = 1
    }

    companion object {
        private const val TAG = "ObservableDiffList"

        /** About one frame at 60 Hz. */
        const val DEFAULT_FRAME_MILLIS = 16L
    }
}
