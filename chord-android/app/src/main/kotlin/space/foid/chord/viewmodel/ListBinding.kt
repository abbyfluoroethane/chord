package space.foid.chord.viewmodel

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import space.foid.chord.data.ChatApi
import space.foid.chord.data.ListDiff
import space.foid.chord.data.ObservableDiffList
import space.foid.chord.data.ViewHandle
import space.foid.chord.data.logWarn

/**
 * Keeps one core subscription open for [list], for as long as [scope] lives.
 *
 * It opens a subscription when [api] has a client, and again when the client, [param] or the
 * resync counter changes. It closes the old subscription first. When [api] is null (signed
 * out) it empties the list. The resync counter goes up when the list asks for a new
 * subscription after a diff it could not apply ([ObservableDiffList.onDesync]).
 *
 * @param open opens the subscription and gives it a sink for diffs that the listener calls
 * @param onHandle called with each new handle, and with null when it is closed
 */
internal class ListBinding<P, T, K : Any>(
    private val scope: CoroutineScope,
    private val api: Flow<ChatApi?>,
    private val param: Flow<P>,
    val list: ObservableDiffList<T, K>,
    private val resync: MutableStateFlow<Int>,
    private val onError: (Throwable) -> Unit,
    private val onHandle: (ViewHandle?) -> Unit = {},
    private val open: suspend (ChatApi, P, (ListDiff<T>) -> Unit) -> ViewHandle,
) {
    fun start(): Job = scope.launch {
        combine(api, param, resync) { a, p, _ -> a to p }.collectLatest { (a, p) ->
            if (a == null) {
                list.clear()
                return@collectLatest
            }
            list.discardPending()
            val handle = try {
                open(a, p, list::submit)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("ListBinding", "could not open the subscription", e)
                onError(e)
                return@collectLatest
            }
            onHandle(handle)
            try {
                awaitCancellation()
            } finally {
                onHandle(null)
                runCatching { handle.close() }
            }
        }
    }

    companion object {
        /** A list that asks this binding for a new subscription when it loses sync. */
        fun <T, K : Any> listFor(
            scope: CoroutineScope,
            key: (T) -> K,
            resync: MutableStateFlow<Int>,
        ): ObservableDiffList<T, K> = ObservableDiffList(
            scope = scope,
            key = key,
            onDesync = { resync.update { it + 1 } },
        )
    }
}
