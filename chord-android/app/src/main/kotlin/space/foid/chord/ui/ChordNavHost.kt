package space.foid.chord.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import space.foid.chord.notify.rememberNotificationPermission
import space.foid.chord.ui.screens.MainScreen
import space.foid.chord.ui.screens.SignInScreen

private const val SIGN_IN = "sign-in"
private const val MAIN = "main"

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
            MainScreen(
                onSignedOut = { nav.navigate(SIGN_IN) { popUpTo(MAIN) { inclusive = true } } },
                openPeer = openPeer,
            )
        }
    }
}
