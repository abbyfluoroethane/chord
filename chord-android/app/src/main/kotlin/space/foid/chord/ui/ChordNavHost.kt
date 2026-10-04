package space.foid.chord.ui

import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.runtime.Composable
import androidx.lifecycle.Lifecycle
import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavController
import space.foid.chord.ui.theme.ChordEase
import space.foid.chord.ui.theme.ChordMotion
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.launch
import space.foid.chord.ChordApp
import space.foid.chord.ui.components.LocalSignInAgain
import space.foid.chord.ui.settings.SettingsScreen
import androidx.compose.ui.Modifier
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import space.foid.chord.notify.rememberNotificationPermission
import space.foid.chord.ui.screens.MainScreen
import space.foid.chord.ui.screens.SignInScreen
import space.foid.chord.update.OpenUpdatesRequest
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.navigation.compose.currentBackStackEntryAsState

private const val SIGN_IN = "sign-in"
private const val MAIN = "main"
private const val SETTINGS = "settings"

private val SlideSpec = tween<androidx.compose.ui.unit.IntOffset>(ChordMotion.SLOW, easing = ChordEase)
private val FadeIn = fadeIn(tween(ChordMotion.ARRIVE, easing = ChordEase))
private val FadeOut = fadeOut(tween(ChordMotion.FAST, easing = ChordEase))

/** A page that comes in from the right and goes out to the right. The page below moves a quarter as far. */
private fun slideIn(): EnterTransition = slideInHorizontally(SlideSpec) { it }
private fun slideOut(): ExitTransition = slideOutHorizontally(SlideSpec) { it }
private fun parallaxIn(): EnterTransition = slideInHorizontally(SlideSpec) { -it / 4 }
private fun parallaxOut(): ExitTransition = slideOutHorizontally(SlideSpec) { -it / 4 }

private fun AnimatedContentTransitionScope<NavBackStackEntry>.opensSettings() =
    targetState.destination.route == SETTINGS
private fun AnimatedContentTransitionScope<NavBackStackEntry>.closesSettings() =
    initialState.destination.route == SETTINGS

/** Navigate only from a screen that is fully on top: a second tap during an animation does nothing. */
private fun NavController.ifResumed(action: NavController.() -> Unit) {
    if (currentBackStackEntry?.lifecycle?.currentState == Lifecycle.State.RESUMED) action()
}

/**
 * The navigation of the app: the sign-in screen and the main screen.
 * [startSignedIn] picks the first destination. [openPeer] is the JID that a notification asked to open.
 */
@Composable
fun ChordNavHost(startSignedIn: Boolean, openPeer: String?) {
    val nav = rememberNavController()
    val notificationPermission = rememberNotificationPermission()
    // A real auth failure of the background login closes the client: go to sign-in.
    val appSession = (LocalContext.current.applicationContext as? ChordApp)?.session
    LaunchedEffect(appSession) {
        appSession?.client?.collect { client ->
            val route = nav.currentDestination?.route
            if (client == null && route != null && route != SIGN_IN && nav.currentBackStackEntry != null) {
                nav.navigate(SIGN_IN) { popUpTo(0) { inclusive = true }; launchSingleTop = true }
            }
        }
    }
    // The update notification: open the settings, which then show the About page.
    val openUpdates by OpenUpdatesRequest.pending.collectAsState()
    val entry by nav.currentBackStackEntryAsState()
    LaunchedEffect(openUpdates, entry?.destination?.route) {
        if (!openUpdates) return@LaunchedEffect
        when (entry?.destination?.route) {
            MAIN -> nav.navigate(SETTINGS) { launchSingleTop = true }
            SIGN_IN -> OpenUpdatesRequest.consume()
        }
    }
    NavHost(
        navController = nav,
        startDestination = if (startSignedIn) MAIN else SIGN_IN,
        modifier = Modifier,
        // Sign-in and main fade. Settings slides (see below).
        enterTransition = { FadeIn },
        exitTransition = { FadeOut },
        popEnterTransition = { FadeIn },
        popExitTransition = { FadeOut },
    ) {
        composable(SIGN_IN) {
            SignInScreen(
                onSignedIn = {
                    notificationPermission.request()
                    nav.navigate(MAIN) { popUpTo(SIGN_IN) { inclusive = true } }
                },
            )
        }
        composable(
            MAIN,
            exitTransition = { if (opensSettings()) parallaxOut() else FadeOut },
            popEnterTransition = { if (closesSettings()) parallaxIn() else FadeIn },
        ) {
            val scope = rememberCoroutineScope()
            val session = (LocalContext.current.applicationContext as ChordApp).session
            // The banner for "signed out by the server": sign out for good, then sign in again.
            CompositionLocalProvider(
                LocalSignInAgain provides {
                    scope.launch {
                        session.signOut()
                        nav.navigate(SIGN_IN) { popUpTo(MAIN) { inclusive = true } }
                    }
                    Unit
                },
            ) {
                MainScreen(
                    onSignedOut = { nav.navigate(SIGN_IN) { popUpTo(MAIN) { inclusive = true } } },
                    openPeer = openPeer,
                    onOpenSettings = { nav.ifResumed { navigate(SETTINGS) { launchSingleTop = true } } },
                )
            }
        }
        composable(
            SETTINGS,
            enterTransition = { slideIn() },
            popExitTransition = { if (targetState.destination.route == MAIN) slideOut() else FadeOut },
            exitTransition = { FadeOut },
        ) {
            SettingsScreen(
                // A quick double back must not pop MAIN as well and leave an empty host.
                onBack = { nav.ifResumed { if (previousBackStackEntry != null) popBackStack() } },
                onSignedOut = { nav.navigate(SIGN_IN) { popUpTo(MAIN) { inclusive = true } } },
            )
        }
    }
}
