package space.foid.chord.viewmodel

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import space.foid.chord.data.ChatApi
import space.foid.chord.data.stableKey
import space.foid.chord.data.toListDiff
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelScope
import uniffi.chord_ffi.MemberItem
import uniffi.chord_ffi.SpaceItem

/** The scope of a ViewModel in the app: the main thread. [ViewModel] cancels it when it clears. */
internal fun mainScope(): CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

/** The spaces of the account. */
class SpaceListViewModel(
    api: Flow<ChatApi?>,
    scope: CoroutineScope = mainScope(),
) : ViewModel(scope) {
    private val resync = MutableStateFlow(0)
    private val list = ListBinding.listFor<SpaceItem, String>(scope, SpaceItem::stableKey, resync)
    private val _error = MutableStateFlow<String?>(null)

    /** The spaces, in core order. Use [SpaceItem.stableKey] as the LazyColumn key. */
    val spaces: StateFlow<List<SpaceItem>> get() = list.items

    /** False until the first list arrived. */
    val loaded: StateFlow<Boolean> get() = list.loaded

    /** Plain-English text of the last failure to follow the list, or null. */
    val error: StateFlow<String?> = _error.asStateFlow()

    init {
        ListBinding(
            scope = scope,
            api = api,
            param = MutableStateFlow(Unit),
            list = list,
            resync = resync,
            onError = { _error.value = describeError(it) },
        ) { a, _, sink -> a.spaces { sink(it.toListDiff()) } }.start()
    }
}

/**
 * The channels of Home or of one space. Switch with [setScope]. The list is emptied while the
 * new scope loads, so rows of the old scope never show under the new one.
 */
class ChannelListViewModel(
    api: Flow<ChatApi?>,
    initialScope: ChannelScope = ChannelScope.Home,
    scope: CoroutineScope = mainScope(),
) : ViewModel(scope) {
    private val resync = MutableStateFlow(0)
    private val channelScope = MutableStateFlow(initialScope)
    private val list = ListBinding.listFor<ChannelItem, String>(scope, ChannelItem::stableKey, resync)
    private val _error = MutableStateFlow<String?>(null)

    /** The channels in core order. Use [ChannelItem.stableKey] as the LazyColumn key. */
    val channels: StateFlow<List<ChannelItem>> get() = list.items

    /** False until the first list of the current scope arrived. */
    val loaded: StateFlow<Boolean> get() = list.loaded

    /** The scope that the list shows (or loads). */
    val currentScope: StateFlow<ChannelScope> = channelScope.asStateFlow()

    val error: StateFlow<String?> = _error.asStateFlow()

    init {
        ListBinding(
            scope = scope,
            api = api,
            param = channelScope,
            list = list,
            resync = resync,
            onError = { _error.value = describeError(it) },
        ) { a, s, sink -> a.channels(s) { sink(it.toListDiff()) } }.start()
    }

    /** Show the channels of [newScope]. Does nothing when it is the current scope. */
    fun setScope(newScope: ChannelScope) {
        if (channelScope.value == newScope) return
        list.clear()
        _error.value = null
        channelScope.value = newScope
    }
}

/** The members of a room, or both people of a 1:1 chat. */
class MemberListViewModel(
    room: String,
    api: Flow<ChatApi?>,
    scope: CoroutineScope = mainScope(),
) : ViewModel(scope) {
    private val resync = MutableStateFlow(0)
    private val list = ListBinding.listFor<MemberItem, String>(scope, MemberItem::stableKey, resync)
    private val _error = MutableStateFlow<String?>(null)

    /** The members. Use [MemberItem.stableKey] as the LazyColumn key. */
    val members: StateFlow<List<MemberItem>> get() = list.items

    val loaded: StateFlow<Boolean> get() = list.loaded

    val error: StateFlow<String?> = _error.asStateFlow()

    init {
        ListBinding(
            scope = scope,
            api = api,
            param = MutableStateFlow(room),
            list = list,
            resync = resync,
            onError = { _error.value = describeError(it) },
        ) { a, r, sink -> a.members(r) { sink(it.toListDiff()) } }.start()
    }
}
