package space.foid.chord.viewmodel

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import space.foid.chord.data.ChatApi
import space.foid.chord.data.TimelineHandle
import space.foid.chord.data.TimelineTarget
import space.foid.chord.data.logWarn
import space.foid.chord.data.stableKey
import space.foid.chord.data.toListDiff
import uniffi.chord_ffi.TimelineItem
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

/**
 * The messages of a room, a 1:1 chat, or the private chat with a room occupant ([TimelineTarget]).
 *
 * [items] is oldest first. Diffs map to list updates, never to a reload. Back-pagination is
 * [loadOlder]: the UI calls it when the user comes near the top. The ViewModel holds no
 * protocol logic: every action is one call on the core.
 *
 * The messages of the actions fail quietly into [errors], in plain English, for a snackbar.
 */
class TimelineViewModel(
    val target: TimelineTarget,
    private val api: Flow<ChatApi?>,
    scope: CoroutineScope = mainScope(),
    private val pageSize: Int = PAGE_SIZE,
    private val growWaitMillis: Long = GROW_WAIT_MILLIS,
    private val typingIdleMillis: Long = TYPING_IDLE_MILLIS,
) : ViewModel(scope) {
    private val scope = scope
    private val resync = MutableStateFlow(0)
    private val list = ListBinding.listFor<TimelineItem, String>(scope, TimelineItem::stableKey, resync)

    @Volatile private var handle: TimelineHandle? = null
    private val paginating = AtomicBoolean(false)

    private val _loadingOlder = MutableStateFlow(false)
    private val _reachedStart = MutableStateFlow(false)
    private val _errors = MutableSharedFlow<String>(extraBufferCapacity = 8)
    private val _error = MutableStateFlow<String?>(null)

    /** The messages, oldest first. Use [TimelineItem.stableKey] (the core id) as the LazyColumn key. */
    val items: StateFlow<List<TimelineItem>> get() = list.items

    /** False until the first list arrived. */
    val loaded: StateFlow<Boolean> get() = list.loaded

    /** True while [loadOlder] waits for older messages. */
    val loadingOlder: StateFlow<Boolean> = _loadingOlder.asStateFlow()

    /** True when the start of the history is on screen. [loadOlder] does nothing then. */
    val reachedStart: StateFlow<Boolean> = _reachedStart.asStateFlow()

    /** Plain-English text for each action that failed. */
    val errors: SharedFlow<String> = _errors.asSharedFlow()

    /** Plain-English text of the last failure to follow the timeline, or null. */
    val error: StateFlow<String?> = _error.asStateFlow()

    private val _uploads = MutableStateFlow<List<UploadUi>>(emptyList())
    private val uploadSources = ConcurrentHashMap<Long, suspend () -> UploadFile>()
    private val nextUploadId = AtomicLong(1)

    /** The attachments that are not sent yet or failed, oldest first. */
    val uploads: StateFlow<List<UploadUi>> = _uploads.asStateFlow()

    private var typingJob: Job? = null
    private var typing = false

    init {
        ListBinding(
            scope = scope,
            api = api,
            param = MutableStateFlow(target),
            list = list,
            resync = resync,
            onError = { _error.value = describeError(it) },
            onHandle = {
                handle = it as? TimelineHandle
                if (it != null) _reachedStart.value = false
            },
        ) { a, t, sink -> a.timeline(t) { sink(it.toListDiff()) } }.start()
    }

    /**
     * Ask the core for [pageSize] older messages. Only one call runs at a time, and a call while
     * one runs, or after the start of the history, does nothing.
     *
     * The core does not say that the history ended. The history ended when the call returned
     * and the list did not grow for [growWaitMillis].
     */
    fun loadOlder() {
        if (_reachedStart.value) return
        if (!paginating.compareAndSet(false, true)) return
        val h = handle
        if (h == null) {
            paginating.set(false)
            return
        }
        scope.launch {
            _loadingOlder.value = true
            try {
                val before = items.value
                val firstBefore = before.firstOrNull()?.stableKey()
                h.paginateBack(pageSize)
                val grew = withTimeoutOrNull(growWaitMillis) {
                    items.first { it.size > before.size || it.firstOrNull()?.stableKey() != firstBefore }
                }
                if (grew == null && handle === h) _reachedStart.value = true
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn(TAG, "loadOlder failed", e)
                _errors.tryEmit(describeError(e))
            } finally {
                _loadingOlder.value = false
                paginating.set(false)
            }
        }
    }

    fun send(body: String) = act { it.send(target, body) }.also { stopTyping() }

    fun reply(itemId: String, body: String) = act { it.reply(itemId, body) }

    fun edit(itemId: String, body: String) = act { it.edit(itemId, body) }

    fun retract(itemId: String) = act { it.retract(itemId) }

    fun toggleReaction(itemId: String, emoji: String) = act { it.toggleReaction(itemId, emoji) }

    /**
     * Send a file. The row shows in [uploads] at once. [prepare] reads the file (and shrinks a
     * photo): it must move the work off the main thread itself. The core sends the message
     * when the upload is done. A failure keeps the row as FAILED, with [retryUpload].
     */
    fun upload(name: String, prepare: suspend () -> UploadFile): Long {
        val id = nextUploadId.getAndIncrement()
        uploadSources[id] = prepare
        _uploads.update { it + UploadUi(id, name, UploadStage.PREPARING) }
        runUpload(id)
        return id
    }

    /** Try a failed upload again. */
    fun retryUpload(id: Long) {
        val row = _uploads.value.firstOrNull { it.id == id } ?: return
        if (row.stage != UploadStage.FAILED) return
        setUpload(id) { it.copy(stage = UploadStage.PREPARING, error = null) }
        runUpload(id)
    }

    /** Remove a failed upload row. */
    fun dismissUpload(id: Long) {
        uploadSources.remove(id)
        _uploads.update { list -> list.filterNot { it.id == id } }
    }

    private fun setUpload(id: Long, change: (UploadUi) -> UploadUi) =
        _uploads.update { list -> list.map { if (it.id == id) change(it) else it } }

    private fun runUpload(id: Long) {
        val prepare = uploadSources[id] ?: return
        scope.launch {
            try {
                val file = prepare()
                setUpload(id) { it.copy(name = file.name, stage = UploadStage.UPLOADING) }
                val a = api.first() ?: throw UploadException("You are not signed in.")
                a.upload(target, file.name, file.contentType, file.bytes)
                dismissUpload(id)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn(TAG, "upload failed", e)
                val text = describeUploadError(e)
                setUpload(id) { it.copy(stage = UploadStage.FAILED, error = text) }
            }
        }
    }

    /** Mark the chat as read, and tell the peer. A failure is only logged. */
    fun markRead() {
        scope.launch {
            try {
                api.first()?.markRead(target)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn(TAG, "markRead failed", e)
            }
        }
    }

    /**
     * Tell the core about the text of the composer. Text sends "typing" once, and no input for
     * [typingIdleMillis] (or an empty text) sends "not typing".
     */
    fun onComposerChanged(text: String) {
        if (text.isEmpty()) {
            stopTyping()
            return
        }
        if (!typing) {
            typing = true
            quiet { it.setTyping(target, true) }
        }
        typingJob?.cancel()
        typingJob = scope.launch {
            delay(typingIdleMillis)
            stopTyping()
        }
    }

    private fun stopTyping() {
        typingJob?.cancel()
        typingJob = null
        if (typing) {
            typing = false
            quiet { it.setTyping(target, false) }
        }
    }

    private fun quiet(block: suspend (ChatApi) -> Unit) {
        scope.launch {
            try {
                api.first()?.let { block(it) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn(TAG, "call failed", e)
            }
        }
    }

    private fun act(block: suspend (ChatApi) -> Unit): Job = scope.launch {
        try {
            val a = api.first()
            if (a == null) _errors.tryEmit("You are not signed in.") else block(a)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logWarn(TAG, "action failed", e)
            _errors.tryEmit(describeError(e))
        }
    }

    companion object {
        private const val TAG = "TimelineViewModel"
        const val PAGE_SIZE = 30
        const val GROW_WAIT_MILLIS = 2_000L
        const val TYPING_IDLE_MILLIS = 5_000L
    }
}
