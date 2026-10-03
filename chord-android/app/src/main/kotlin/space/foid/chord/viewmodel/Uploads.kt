package space.foid.chord.viewmodel

import androidx.compose.runtime.Immutable

/** A file that is ready for the upload service. The reader of the picked file builds it. */
class UploadFile(val name: String, val contentType: String, val bytes: ByteArray) {
    val size: Long get() = bytes.size.toLong()
}

/** Reading the picked file failed. [message] is plain English for the user. */
class UploadException(message: String, cause: Throwable? = null) : Exception(message, cause)

/** Where a pending attachment is. */
enum class UploadStage { PREPARING, UPLOADING, FAILED }

/**
 * One attachment that is not sent yet, or failed. It shows as a row above the composer. The core
 * has no progress for an upload, so the row is indeterminate.
 */
@Immutable
data class UploadUi(
    val id: Long,
    val name: String,
    val stage: UploadStage,
    /** Plain-English reason, only in [UploadStage.FAILED]. */
    val error: String? = null,
)

/** Plain-English text for a failed upload. The core detail helps here: it names the size limit. */
fun describeUploadError(error: Throwable): String = when (error) {
    is UploadException -> error.message.orEmpty()
    is uniffi.chord_ffi.ChordException.Invalid -> "The server cannot take this file: ${error.detail}."
    is uniffi.chord_ffi.ChordException.Unsupported -> "This server has no file upload."
    is uniffi.chord_ffi.ChordException.Server -> "The server refused the file."
    else -> describeError(error)
}
