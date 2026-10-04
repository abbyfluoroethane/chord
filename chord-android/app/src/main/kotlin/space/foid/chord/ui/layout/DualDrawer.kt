package space.foid.chord.ui.layout

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.AnchoredDraggableDefaults
import androidx.compose.foundation.gestures.AnchoredDraggableState
import androidx.compose.foundation.gestures.DraggableAnchors
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.anchoredDraggable
import androidx.compose.foundation.gestures.animateTo
import androidx.compose.foundation.gestures.snapTo
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordEase
import space.foid.chord.ui.theme.ChordMotion
import kotlin.math.abs
import kotlin.math.min
import kotlin.math.roundToInt

/** Which pane the user sees. [Left] and [Right] mean that the drawer on that side is open. */
enum class DrawerPane { Left, Center, Right }

private val SettleSpec = tween<Float>(ChordMotion.SLOW, easing = ChordEase)

/**
 * State of a [DualDrawer]. Build it with [rememberDualDrawerState].
 * [offset] is how far the center pane moved to the right, in px. It is negative when the right drawer is open.
 */
@OptIn(ExperimentalFoundationApi::class)
@Stable
class DualDrawerState(initial: DrawerPane = DrawerPane.Center) {
    internal val draggable = AnchoredDraggableState(initial)

    /** The pane that the state settled on. */
    val current: DrawerPane get() = draggable.currentValue

    /** The pane that it moves to (the same as [current] at rest). */
    val target: DrawerPane get() = draggable.targetValue

    /** True when a drawer is open, or opening. */
    val isOpen: Boolean get() = target != DrawerPane.Center

    /** Center pane shift in px. 0 until the first layout. */
    val offset: Float get() = draggable.offset.let { if (it.isNaN()) 0f else it }

    private val hasAnchors: Boolean get() = !draggable.offset.isNaN()

    suspend fun show(pane: DrawerPane) {
        if (hasAnchors) draggable.animateTo(pane, SettleSpec) else draggable.snapTo(pane)
    }

    suspend fun openLeft() = show(DrawerPane.Left)
    suspend fun openRight() = show(DrawerPane.Right)
    suspend fun close() = show(DrawerPane.Center)

    companion object {
        val Saver: Saver<DualDrawerState, String> = Saver(
            save = { it.current.name },
            restore = { DualDrawerState(DrawerPane.valueOf(it)) },
        )
    }
}

/**
 * The state of a drawer container. [initial] is used on first composition only (after that,
 * the saved state wins). Pass [DrawerPane.Left] to open the left drawer on first show.
 */
@Composable
fun rememberDualDrawerState(initial: DrawerPane = DrawerPane.Center): DualDrawerState =
    rememberSaveable(saver = DualDrawerState.Saver) { DualDrawerState(initial) }

/**
 * Discord-style container with a drawer on each side. The center pane slides aside while the drawer
 * stays in place under it. The drag starts anywhere in the center pane and follows the finger. On release it
 * settles with the Chord ease. A tap on the shifted center pane closes the drawer, and so does system back.
 *
 * @param left content of the left drawer. Gets the whole width of its pane.
 * @param right content of the right drawer.
 * @param content the center pane.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun DualDrawer(
    state: DualDrawerState,
    left: @Composable () -> Unit,
    right: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    leftWidth: Dp = 360.dp,
    rightWidth: Dp = 300.dp,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    BackHandler(enabled = state.isOpen) { scope.launch { state.close() } }

    // The keyboard goes away as soon as a drawer starts to open, by drag or by a header button.
    // It does not come back when the drawer closes.
    val keyboard = LocalSoftwareKeyboardController.current
    val focus = LocalFocusManager.current
    val moved = state.offset != 0f
    LaunchedEffect(moved) {
        if (moved) {
            focus.clearFocus(force = true)
            keyboard?.hide()
        }
    }

    BoxWithConstraints(modifier.fillMaxSize().background(Chord.colors.surfaceSide).clipToBounds()) {
        // A drawer leaves a strip of the center pane visible: it takes at most 84% of the width.
        val leftW = min(leftWidth.value, maxWidth.value * 0.84f).dp
        val rightW = min(rightWidth.value, maxWidth.value * 0.80f).dp
        val leftPx = with(density) { leftW.toPx() }
        val rightPx = with(density) { rightW.toPx() }
        remember(leftPx, rightPx) {
            state.draggable.updateAnchors(
                DraggableAnchors {
                    DrawerPane.Left at leftPx
                    DrawerPane.Center at 0f
                    DrawerPane.Right at -rightPx
                },
            )
            leftPx
        }
        val fling = AnchoredDraggableDefaults.flingBehavior(
            state = state.draggable,
            positionalThreshold = { distance -> distance * 0.4f },
            animationSpec = SettleSpec,
        )

        val x = state.offset
        if (x > 0f) {
            Box(
                Modifier
                    .align(Alignment.CenterStart)
                    .width(leftW)
                    .fillMaxHeight()
                    .testTag("drawer_left")
                    // A little parallax: the drawer slides in as the center moves aside.
                    .graphicsLayer { translationX = -(1f - min(1f, x / leftPx)) * leftPx * 0.2f },
            ) { left() }
        }
        if (x < 0f) {
            Box(
                Modifier
                    .align(Alignment.CenterEnd)
                    .width(rightW)
                    .fillMaxHeight()
                    .testTag("drawer_right")
                    .graphicsLayer { translationX = (1f - min(1f, -x / rightPx)) * rightPx * 0.2f },
            ) { right() }
        }

        Box(
            Modifier
                .fillMaxSize()
                .offset { IntOffset(state.offset.roundToInt(), 0) }
                .anchoredDraggable(state.draggable, Orientation.Horizontal, flingBehavior = fling)
                .testTag("drawer_center"),
        ) {
            content()
            if (x != 0f) {
                val progress = min(1f, abs(x) / (if (x > 0f) leftPx else rightPx))
                Box(
                    Modifier
                        .fillMaxSize()
                        .background(Chord.colors.scrim.copy(alpha = Chord.colors.scrim.alpha * 0.5f * progress))
                        .semantics { contentDescription = "Close the drawer" }
                        .pointerInput(Unit) { detectTapGestures { scope.launch { state.close() } } },
                )
            }
        }
    }
}
