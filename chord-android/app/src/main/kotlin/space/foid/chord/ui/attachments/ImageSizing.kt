package space.foid.chord.ui.attachments

/** The longest side of a photo after the downscale, in pixels. */
const val MAX_PHOTO_SIDE = 2560

/** JPEG quality for a re-encoded photo. */
const val PHOTO_JPEG_QUALITY = 85

/** A size in pixels. */
data class PixelSize(val width: Int, val height: Int)

/**
 * The size of a photo after the downscale: the same shape, the longest side at most [maxSide].
 * A photo that fits keeps its size. A side never drops under 1.
 */
fun downscaledSize(width: Int, height: Int, maxSide: Int = MAX_PHOTO_SIDE): PixelSize {
    require(width > 0 && height > 0 && maxSide > 0) { "sizes must be positive" }
    val longest = maxOf(width, height)
    if (longest <= maxSide) return PixelSize(width, height)
    val scale = maxSide.toDouble() / longest
    return PixelSize(
        width = maxOf(1, Math.round(width * scale).toInt()),
        height = maxOf(1, Math.round(height * scale).toInt()),
    )
}

/**
 * The power-of-two `inSampleSize` for the decoder, so that the decoded bitmap is not smaller than
 * [target]. This keeps memory low for a big photo.
 */
fun sampleSizeFor(width: Int, height: Int, target: PixelSize): Int {
    var sample = 1
    while (width / (sample * 2) >= target.width && height / (sample * 2) >= target.height) sample *= 2
    return sample
}

/** The width / height of an inline image when the size is not known yet. */
const val DEFAULT_ASPECT = 4f / 3f

/** The widest inline image, in dp. */
const val THUMB_MAX_WIDTH_DP = 280

/**
 * The aspect (width / height) of an inline image box. A very tall or very wide image gets a
 * limit, so that it does not fill the screen. A size that is not known gives [DEFAULT_ASPECT].
 */
fun thumbnailAspect(width: Float, height: Float, min: Float = 0.5f, max: Float = 2.5f): Float {
    if (width <= 0f || height <= 0f || width.isNaN() || height.isNaN() || width.isInfinite() || height.isInfinite()) return DEFAULT_ASPECT
    return (width / height).coerceIn(min, max)
}
