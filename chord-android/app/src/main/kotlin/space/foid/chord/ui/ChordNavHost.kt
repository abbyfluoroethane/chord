package space.foid.chord.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
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

private const val SIGN_IN = "sign-in"
private const val MAIN = "main"
private const val SETTINGS = "settings"

/**
 * The navigation of the app: the sign-in screen and the main screen.
 * [startSignedIn] picks the first destination. [openPeer] is the JID that a notification asked to open.
 */
@Composable
fun ChordNavHost(startSignedIn: Boolean, openPeer: String?) {
    val nav = rememberNavController()
    val notificationPermission = rememberNotificationPermission()
    NavHost(
        navController = nav,
        startDestination = if (startSignedIn) MAIN else SIGN_IN,
        modifier = Modifier,
    ) {
        composable(SIGN_IN) {
            SignInScreen(
                onSignedIn = {
                    notificationPermission.request()
                    nav.navigate(MAIN) { popUpTo(SIGN_IN) { inclusive = true } }
                },
            )
        }
        composable(MAIN) {
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
                    onOpenSettings = { nav.navigate(SETTINGS) },
                )
            }
        }
        composable(SETTINGS) {
            SettingsScreen(
                onBack = { nav.popBackStack() },
                onSignedOut = { nav.navigate(SIGN_IN) { popUpTo(MAIN) { inclusive = true } } },
            )
        }
    }
}
