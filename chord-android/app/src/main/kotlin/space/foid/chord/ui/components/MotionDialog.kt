package space.foid.chord.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import space.foid.chord.ui.theme.ChordEase
import space.foid.chord.ui.theme.ChordMotion

/** How a [MotionDialog] comes and goes. */
enum class DialogMotion(val enter: () -> EnterTransition, val exit: () -> ExitTransition) {
    /** A page: from the right, back to the right. */
    Slide(
        { slideInHorizontally(tween(ChordMotion.SLOW, easing = ChordEase)) { it } },
        { slideOutHorizontally(tween(ChordMotion.SLOW, easing = ChordEase)) { it } },
    ),

    /** A viewer: fade. */
    Fade(
        { fadeIn(tween(ChordMotion.ARRIVE, easing = ChordEase)) },
        { fadeOut(tween(ChordMotion.ARRIVE, easing = ChordEase)) },
    ),
}

/**
 * A full-screen window with the same motion as the other pages. The platform dialog animation is
 * off, so there is one animation, not two. [content] gets `close`: it plays the exit, and then
 * calls [onClose]. Back and the system dismiss do the same. A caller that removes the dialog by
 * itself (after a save, for example) has no exit animation.
 */
@Composable
fun MotionDialog(
    onClose: () -> Unit,
    motion: DialogMotion,
    content: @Composable (close: () -> Unit) -> Unit,
) {
    val visible = remember { MutableTransitionState(false).apply { targetState = true } }
    val close = { visible.targetState = false }
    Dialog(
        onDismissRequest = close,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        (LocalView.current.parent as? DialogWindowProvider)?.window?.setWindowAnimations(0)
        AnimatedVisibility(visibleState = visible, enter = motion.enter(), exit = motion.exit()) {
            content(close)
        }
    }
    if (visible.isIdle && !visible.currentState) LaunchedEffect(Unit) { onClose() }
}
