package space.foid.chord.ui.avatar

import androidx.compose.ui.graphics.ImageBitmap
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

private class FakeSource : AvatarSource {
    val stored = HashMap<String, AvatarData>()
    val occupants = HashMap<String, String>()
    var avatarCalls = 0
    val refreshed = ArrayList<String>()
    override suspend fun avatar(owner: String): AvatarData? {
        avatarCalls++
        return stored[owner]
    }
    override suspend fun refresh(owner: String) { refreshed += owner }
    override suspend fun realJid(occupant: String): String? = occupants[occupant]
}

@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36])
class AvatarRepositoryTest {
    private val png = byteArrayOf(1, 2, 3)

    private class Env(val scope: TestScope) {
        val source = FakeSource()
        val sourceFlow = MutableStateFlow<AvatarSource?>(source)
        var decodes = 0
        var decodeOk = true
        val repo = AvatarRepository(
            source = sourceFlow,
            scope = scope.backgroundScope,
            decoder = { _, _ -> decodes++; if (decodeOk) ImageBitmap(4, 4) else null },
            decodeDispatcher = StandardTestDispatcher(scope.testScheduler),
            retryDelaysMs = listOf(100, 200, 300),
            missTtlMs = 1_000,
            now = { scope.testScheduler.currentTime },
        )
    }

    private fun env(block: suspend TestScope.(Env) -> Unit) = runTest { block(Env(this)) }

    @Test fun second_load_comes_from_the_cache() = env { e ->
        e.source.stored["a@x"] = AvatarData("h1", png)
        val first = e.repo.load("a@x", "h1", 40)
        val second = e.repo.load("a@x", "h1", 40)
        assertNotNull(first)
        assertSame(first, second)
        assertEquals(1, e.source.avatarCalls)
        assertEquals(1, e.decodes)
        assertSame(first, e.repo.peek("a@x", "h1", 40))
    }

    @Test fun equal_loads_at_the_same_time_share_one_job() = env { e ->
        e.source.stored["a@x"] = AvatarData("h1", png)
        val a = async { e.repo.load("a@x", "h1", 40) }
        val b = async { e.repo.load("a@x", "h1", 40) }
        assertSame(a.await(), b.await())
        assertEquals(1, e.source.avatarCalls)
        assertEquals(1, e.decodes)
    }

    @Test fun a_hash_without_data_calls_refresh_and_retries() = env { e ->
        e.source.stored["a@x"] = AvatarData("h1", null)
        val result = async { e.repo.load("a@x", "h1", 40) }
        runCurrent()
        assertEquals(listOf("a@x"), e.source.refreshed)
        // The data arrives while the repository waits.
        e.source.stored["a@x"] = AvatarData("h1", png)
        advanceTimeBy(150)
        assertNotNull(result.await())
        assertEquals(1, e.source.refreshed.size)
    }

    @Test fun data_that_never_arrives_gives_up_after_the_retries() = env { e ->
        e.source.stored["a@x"] = AvatarData("h1", null)
        assertNull(e.repo.load("a@x", "h1", 40))
        // First try plus three retries.
        assertEquals(4, e.source.avatarCalls)
        assertEquals(1, e.source.refreshed.size)
        // Inside the miss time the core is not asked again.
        assertNull(e.repo.load("a@x", "h1", 40))
        assertEquals(4, e.source.avatarCalls)
    }

    @Test fun no_avatar_and_no_hash_does_not_refresh() = env { e ->
        assertNull(e.repo.load("a@x", null, 40))
        assertEquals(1, e.source.avatarCalls)
        assertEquals(0, e.source.refreshed.size)
    }

    @Test fun a_bad_image_gives_null_so_the_initials_stay() = env { e ->
        e.source.stored["a@x"] = AvatarData("h1", png)
        e.decodeOk = false
        assertNull(e.repo.load("a@x", "h1", 40))
        assertNull(e.repo.peek("a@x", "h1", 40))
    }

    @Test fun an_occupant_loads_through_its_real_jid() = env { e ->
        e.source.occupants["room@conf/bob"] = "bob@x"
        e.source.stored["bob@x"] = AvatarData("h2", png)
        assertNotNull(e.repo.load("room@conf/bob", "h2", 40))
        assertNull(e.repo.load("room@conf/anon", "h3", 40))
    }

    @Test fun clear_drops_the_cache() = env { e ->
        e.source.stored["a@x"] = AvatarData("h1", png)
        e.repo.load("a@x", "h1", 40)
        e.repo.clear()
        assertNull(e.repo.peek("a@x", "h1", 40))
    }

    @Test fun losing_the_client_clears_the_cache() = env { e ->
        e.source.stored["a@x"] = AvatarData("h1", png)
        e.repo.load("a@x", "h1", 40)
        runCurrent()
        e.sourceFlow.value = null
        runCurrent()
        assertNull(e.repo.peek("a@x", "h1", 40))
    }
}
