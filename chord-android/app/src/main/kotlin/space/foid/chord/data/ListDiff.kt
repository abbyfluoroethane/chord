package space.foid.chord.data

import uniffi.chord_ffi.ChannelDiff
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.MemberDiff
import uniffi.chord_ffi.MemberItem
import uniffi.chord_ffi.SpaceDiff
import uniffi.chord_ffi.SpaceItem
import uniffi.chord_ffi.TimelineDiff
import uniffi.chord_ffi.TimelineItem

/** One change of a list. The four FFI diff types map to this one type, so the list code is generic. */
sealed interface ListDiff<out T> {
    data class Insert<T>(val index: Int, val item: T) : ListDiff<T>
    data class Update<T>(val index: Int, val item: T) : ListDiff<T>
    data class Remove(val index: Int) : ListDiff<Nothing>
    data class Reset<T>(val items: List<T>) : ListDiff<T>
}

fun TimelineDiff.toListDiff(): ListDiff<TimelineItem> = when (this) {
    is TimelineDiff.Insert -> ListDiff.Insert(index.toInt(), item)
    is TimelineDiff.Update -> ListDiff.Update(index.toInt(), item)
    is TimelineDiff.Remove -> ListDiff.Remove(index.toInt())
    is TimelineDiff.Reset -> ListDiff.Reset(items)
}

fun ChannelDiff.toListDiff(): ListDiff<ChannelItem> = when (this) {
    is ChannelDiff.Insert -> ListDiff.Insert(index.toInt(), item)
    is ChannelDiff.Update -> ListDiff.Update(index.toInt(), item)
    is ChannelDiff.Remove -> ListDiff.Remove(index.toInt())
    is ChannelDiff.Reset -> ListDiff.Reset(items)
}

fun SpaceDiff.toListDiff(): ListDiff<SpaceItem> = when (this) {
    is SpaceDiff.Insert -> ListDiff.Insert(index.toInt(), item)
    is SpaceDiff.Update -> ListDiff.Update(index.toInt(), item)
    is SpaceDiff.Remove -> ListDiff.Remove(index.toInt())
    is SpaceDiff.Reset -> ListDiff.Reset(items)
}

fun MemberDiff.toListDiff(): ListDiff<MemberItem> = when (this) {
    is MemberDiff.Insert -> ListDiff.Insert(index.toInt(), item)
    is MemberDiff.Update -> ListDiff.Update(index.toInt(), item)
    is MemberDiff.Remove -> ListDiff.Remove(index.toInt())
    is MemberDiff.Reset -> ListDiff.Reset(items)
}

/** Stable LazyColumn keys. They are strings, so they survive a saved state. */
fun TimelineItem.stableKey(): String = id
fun ChannelItem.stableKey(): String = jid
fun MemberItem.stableKey(): String = id
fun SpaceItem.stableKey(): String = "$service|$node"
