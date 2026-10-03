package space.foid.chord.notify

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat

/** The POST_NOTIFICATIONS permission (Android 13+). Always granted on older versions. */
@Stable
class NotificationPermission internal constructor(
    private val state: androidx.compose.runtime.State<Boolean>,
    private val launch: () -> Unit,
) {
    val granted: Boolean get() = state.value

    /** Ask the user. Does nothing if it is granted already. The system may refuse to show the dialog. */
    fun request() {
        if (!granted) launch()
    }
}

/** Call [NotificationPermission.request] once after sign-in. */
@Composable
fun rememberNotificationPermission(): NotificationPermission {
    val context = LocalContext.current
    val granted = remember {
        mutableStateOf(
            Build.VERSION.SDK_INT < 33 ||
                ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
                PackageManager.PERMISSION_GRANTED,
        )
    }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        granted.value = it
    }
    return remember(launcher) {
        NotificationPermission(granted) {
            if (Build.VERSION.SDK_INT >= 33) launcher.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }
}
