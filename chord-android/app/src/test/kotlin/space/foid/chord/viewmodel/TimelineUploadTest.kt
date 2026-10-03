package space.foid.chord.viewmodel

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.data.ChatApi
import space.foid.chord.data.TimelineTarget
import uniffi.chord_ffi.ChordException

@OptIn(ExperimentalCoroutinesApi::class)
class TimelineUploadTest {
    private val room = TimelineTarget.Room("room@muc.example.org")

    private fun TestScope.vm(api: FakeChatApi) =
        TimelineViewModel(room, MutableStateFlow<ChatApi?>(api), backgroundScope)

    private val file = UploadFile("a.jpg", "image/jpeg", ByteArray(3))

    @Test fun aRowShowsWhileTheUploadRunsAndGoesAway() = runTest {
        val api = FakeChatApi()
        val gate = CompletableDeferred<Unit>()
        api.onUpload = { gate.await() }
        val vm = vm(api)
        vm.upload("a.jpg") { file }
        runCurrent()
        assertEquals(listOf(UploadStage.UPLOADING), vm.uploads.value.map { it.stage })
        gate.complete(Unit)
        runCurrent()
        assertTrue(vm.uploads.value.isEmpty())
        assertEquals(listOf("upload a.jpg image/jpeg 3"), api.calls.filter { it.startsWith("upload") })
    }

    @Test fun theRowIsPreparingWhileTheFileIsRead() = runTest {
        val api = FakeChatApi()
        val gate = CompletableDeferred<UploadFile>()
        val vm = vm(api)
        vm.upload("big.jpg") { gate.await() }
        runCurrent()
        assertEquals(UploadStage.PREPARING, vm.uploads.value.single().stage)
        gate.complete(file)
        runCurrent()
        assertTrue(vm.uploads.value.isEmpty())
    }

    @Test fun aFailureKeepsTheRowAndRetryWorks() = runTest {
        val api = FakeChatApi()
        val vm = vm(api)
        api.failWith = ChordException.Invalid("the file has 9 bytes, and the service accepts 5 at most")
        val id = vm.upload("a.jpg") { file }
        runCurrent()
        val row = vm.uploads.value.single()
        assertEquals(UploadStage.FAILED, row.stage)
        assertTrue(row.error!!.contains("accepts 5"))
        api.failWith = null
        vm.retryUpload(id)
        runCurrent()
        assertTrue(vm.uploads.value.isEmpty())
    }

    @Test fun aReadErrorShowsItsOwnText() = runTest {
        val api = FakeChatApi()
        val vm = vm(api)
        vm.upload("x.bin") { throw UploadException("This file is empty.") }
        runCurrent()
        assertEquals("This file is empty.", vm.uploads.value.single().error)
        assertEquals(0, api.calls.count { it.startsWith("upload") })
    }

    @Test fun dismissRemovesAFailedRow() = runTest {
        val api = FakeChatApi()
        val vm = vm(api)
        val id = vm.upload("x.bin") { throw UploadException("No.") }
        runCurrent()
        vm.dismissUpload(id)
        assertTrue(vm.uploads.value.isEmpty())
    }
}
