package space.foid.chord.data

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.concurrent.thread

@OptIn(ExperimentalCoroutinesApi::class)
class ObservableDiffListTest {
    private class Fixture(scope: TestScope) {
        val logs = ArrayList<String>()
        var desyncs = 0
        val list = ObservableDiffList<String, String>(
            scope = scope.backgroundScope,
            key = { it },
            frameMillis = 16,
            onDesync = { desyncs++ },
            log = { logs.add(it) },
        )
        val items get() = list.items.value
    }

    private fun TestScope.fixture() = Fixture(this)

    private fun reset(vararg items: String) = ListDiff.Reset(items.toList())
    private fun ins(i: Int, s: String) = ListDiff.Insert(i, s)

    @Test
    fun startsEmptyAndNotLoaded() = runTest {
        val f = fixture()
        assertEquals(emptyList<String>(), f.items)
        assertFalse(f.list.loaded.value)
    }

    @Test
    fun resetLoadsTheList() = runTest {
        val f = fixture()
        f.list.submit(reset("a", "b"))
        advanceTimeBy(17)
        assertEquals(listOf("a", "b"), f.items)
        assertTrue(f.list.loaded.value)
    }

    @Test
    fun nothingShowsBeforeTheFrameEnds() = runTest {
        val f = fixture()
        f.list.submit(reset("a"))
        advanceTimeBy(10)
        assertEquals(emptyList<String>(), f.items)
        advanceTimeBy(10)
        assertEquals(listOf("a"), f.items)
    }

    @Test
    fun insertUpdateRemove() = runTest {
        val f = fixture()
        f.list.submit(reset("a", "c"))
        advanceTimeBy(17)
        f.list.submit(ins(1, "b"))
        advanceTimeBy(17)
        assertEquals(listOf("a", "b", "c"), f.items)
        f.list.submit(ListDiff.Update(0, "a"))
        f.list.submit(ListDiff.Remove(2))
        advanceTimeBy(17)
        assertEquals(listOf("a", "b"), f.items)
    }

    @Test
    fun insertAtTheEndAndAtTheStart() = runTest {
        val f = fixture()
        f.list.submit(reset("b"))
        f.list.submit(ins(1, "c"))
        f.list.submit(ins(0, "a"))
        advanceTimeBy(17)
        assertEquals(listOf("a", "b", "c"), f.items)
    }

    @Test
    fun updateReplacesTheItemAndMayChangeItsKey() = runTest {
        val f = fixture()
        f.list.submit(reset("a", "b"))
        f.list.submit(ListDiff.Update(0, "z"))
        advanceTimeBy(17)
        assertEquals(listOf("z", "b"), f.items)
        // The old key is free again.
        f.list.submit(ins(2, "a"))
        advanceTimeBy(17)
        assertEquals(listOf("z", "b", "a"), f.items)
    }

    @Test
    fun aBurstGivesOneUpdate() = runTest {
        val f = fixture()
        val seen = ArrayList<List<String>>()
        // StateFlow keeps the last value only, so count the published lists with a collector.
        val job = backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            f.list.items.collect { seen.add(it) }
        }
        f.list.submit(reset("a"))
        for (i in 1..50) f.list.submit(ins(i, "m$i"))
        advanceTimeBy(17)
        assertEquals(51, f.items.size)
        // The initial empty list, and one list for the whole burst.
        assertEquals(2, seen.size)
        job.cancel()
    }

    @Test
    fun separateFramesGiveSeparateUpdates() = runTest {
        val f = fixture()
        f.list.submit(reset("a"))
        advanceTimeBy(17)
        val first = f.items
        f.list.submit(ins(1, "b"))
        advanceTimeBy(17)
        assertEquals(listOf("a"), first)
        assertEquals(listOf("a", "b"), f.items)
    }

    @Test
    fun rowsKeepTheirIdentityAcrossUpdates() = runTest {
        val f = fixture()
        val a = String(charArrayOf('a'))
        val b = String(charArrayOf('b'))
        f.list.submit(reset(a, b))
        advanceTimeBy(17)
        f.list.submit(ins(2, "c"))
        advanceTimeBy(17)
        assertSame(a, f.items[0])
        assertSame(b, f.items[1])
    }

    @Test
    fun aPublishedListNeverChanges() = runTest {
        val f = fixture()
        f.list.submit(reset("a"))
        advanceTimeBy(17)
        val first = f.items
        f.list.submit(ins(1, "b"))
        advanceTimeBy(17)
        assertNotSame(first, f.items)
        assertEquals(listOf("a"), first)
    }

    @Test
    fun aResetInABurstDropsTheDiffsBeforeIt() = runTest {
        val f = fixture()
        f.list.submit(reset("a"))
        f.list.submit(ins(5, "bad")) // Would fail, but a Reset follows.
        f.list.submit(reset("x", "y"))
        f.list.submit(ins(2, "z"))
        advanceTimeBy(17)
        assertEquals(listOf("x", "y", "z"), f.items)
        assertEquals(0, f.desyncs)
    }

    @Test
    fun resetReplacesTheList() = runTest {
        val f = fixture()
        f.list.submit(reset("a", "b"))
        advanceTimeBy(17)
        f.list.submit(reset("c"))
        advanceTimeBy(17)
        assertEquals(listOf("c"), f.items)
    }

    @Test
    fun emptyResetEmptiesTheListAndLoadsIt() = runTest {
        val f = fixture()
        f.list.submit(reset())
        advanceTimeBy(17)
        assertEquals(emptyList<String>(), f.items)
        assertTrue(f.list.loaded.value)
    }

    @Test
    fun insertOutOfRangeAsksForAResetAndKeepsTheList() = runTest {
        val f = fixture()
        f.list.submit(reset("a"))
        advanceTimeBy(17)
        f.list.submit(ins(1, "ok"))
        f.list.submit(ins(9, "bad"))
        advanceTimeBy(17)
        assertEquals(listOf("a"), f.items) // The burst is dropped as a whole.
        assertEquals(1, f.desyncs)
        assertEquals(1, f.logs.size)
    }

    @Test
    fun negativeIndexIsOutOfRange() = runTest {
        val f = fixture()
        f.list.submit(reset("a"))
        f.list.submit(ins(-1, "bad"))
        advanceTimeBy(17)
        assertEquals(1, f.desyncs)
    }

    @Test
    fun updateAndRemoveOutOfRange() = runTest {
        val f = fixture()
        f.list.submit(reset("a"))
        advanceTimeBy(17)
        f.list.submit(ListDiff.Update(1, "x"))
        advanceTimeBy(17)
        assertEquals(1, f.desyncs)
        f.list.submit(reset("a"))
        advanceTimeBy(17)
        f.list.submit(ListDiff.Remove(1))
        advanceTimeBy(17)
        assertEquals(2, f.desyncs)
        assertEquals(listOf("a"), f.items)
    }

    @Test
    fun removeFromAnEmptyListIsOutOfRange() = runTest {
        val f = fixture()
        f.list.submit(ListDiff.Remove(0))
        advanceTimeBy(17)
        assertEquals(1, f.desyncs)
    }

    @Test
    fun afterADesyncDiffsAreIgnoredUntilAReset() = runTest {
        val f = fixture()
        f.list.submit(reset("a"))
        advanceTimeBy(17)
        f.list.submit(ins(7, "bad"))
        advanceTimeBy(17)
        f.list.submit(ins(1, "b"))
        advanceTimeBy(17)
        assertEquals(listOf("a"), f.items)
        assertEquals(1, f.desyncs) // Not asked again for the ignored diffs.
        f.list.submit(reset("n1", "n2"))
        f.list.submit(ins(2, "n3"))
        advanceTimeBy(17)
        assertEquals(listOf("n1", "n2", "n3"), f.items)
        f.list.submit(ins(3, "n4"))
        advanceTimeBy(17)
        assertEquals(4, f.items.size)
    }

    @Test
    fun discardPendingEndsTheDesyncState() = runTest {
        val f = fixture()
        f.list.submit(reset("a"))
        advanceTimeBy(17)
        f.list.submit(ins(7, "bad"))
        advanceTimeBy(17)
        f.list.discardPending()
        f.list.submit(ins(1, "b"))
        advanceTimeBy(17)
        assertEquals(listOf("a", "b"), f.items)
    }

    @Test
    fun duplicateKeyInsertIsADesync() = runTest {
        val f = fixture()
        f.list.submit(reset("a", "b"))
        advanceTimeBy(17)
        f.list.submit(ins(2, "a"))
        advanceTimeBy(17)
        assertEquals(listOf("a", "b"), f.items)
        assertEquals(1, f.desyncs)
    }

    @Test
    fun duplicateKeyUpdateIsADesync() = runTest {
        val f = fixture()
        f.list.submit(reset("a", "b"))
        advanceTimeBy(17)
        f.list.submit(ListDiff.Update(0, "b"))
        advanceTimeBy(17)
        assertEquals(listOf("a", "b"), f.items)
        assertEquals(1, f.desyncs)
    }

    @Test
    fun removeFreesTheKey() = runTest {
        val f = fixture()
        f.list.submit(reset("a", "b"))
        f.list.submit(ListDiff.Remove(0))
        f.list.submit(ins(1, "a"))
        advanceTimeBy(17)
        assertEquals(listOf("b", "a"), f.items)
        assertEquals(0, f.desyncs)
    }

    @Test
    fun resetWithDuplicateKeysKeepsTheFirst() = runTest {
        val f = fixture()
        f.list.submit(reset("a", "b", "a"))
        advanceTimeBy(17)
        assertEquals(listOf("a", "b"), f.items)
        assertEquals(0, f.desyncs)
        assertEquals(1, f.logs.size)
    }

    @Test
    fun clearEmptiesTheListAndDropsPendingDiffs() = runTest {
        val f = fixture()
        f.list.submit(reset("a"))
        advanceTimeBy(17)
        f.list.submit(ins(1, "b"))
        f.list.clear()
        advanceTimeBy(17)
        assertEquals(emptyList<String>(), f.items)
        assertFalse(f.list.loaded.value)
        // The key table is empty too.
        f.list.submit(reset("a"))
        advanceTimeBy(17)
        assertEquals(listOf("a"), f.items)
    }

    @Test
    fun diffsFromOtherThreadsAreJoined() = runTest {
        val f = fixture()
        f.list.submit(reset())
        advanceTimeBy(17)
        val workers = (0 until 4).map { w ->
            thread { for (i in 0 until 250) f.list.submit(ins(0, "w$w-$i")) }
        }
        workers.forEach { it.join() }
        advanceTimeBy(17)
        assertEquals(1000, f.items.size)
        assertEquals(1000, f.items.toSet().size)
    }

    @Test
    fun thousandsOfRowsStayFast() = runTest {
        val f = fixture()
        f.list.submit(ListDiff.Reset((0 until 5000).map { "m$it" }))
        advanceTimeBy(17)
        val start = System.nanoTime()
        repeat(200) { n ->
            f.list.submit(ins(f.items.size, "n$n"))
            advanceTimeBy(17)
        }
        val ms = (System.nanoTime() - start) / 1_000_000
        assertEquals(5200, f.items.size)
        assertTrue("200 inserts took $ms ms", ms < 2000)
    }

    @Test
    fun convertsFfiDiffs() {
        val item = uniffi.chord_ffi.SpaceItem("svc", "node", "Name", null)
        assertEquals(
            ListDiff.Insert(3, item),
            uniffi.chord_ffi.SpaceDiff.Insert(3u, item).toListDiff(),
        )
        assertEquals(ListDiff.Remove(2), uniffi.chord_ffi.SpaceDiff.Remove(2u).toListDiff())
        assertEquals(ListDiff.Reset(listOf(item)), uniffi.chord_ffi.SpaceDiff.Reset(listOf(item)).toListDiff())
        assertEquals(ListDiff.Update(0, item), uniffi.chord_ffi.SpaceDiff.Update(0u, item).toListDiff())
        assertEquals("svc|node", item.stableKey())
    }
}
