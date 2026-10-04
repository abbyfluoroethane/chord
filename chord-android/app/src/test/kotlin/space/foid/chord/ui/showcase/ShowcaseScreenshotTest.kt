package space.foid.chord.ui.showcase

// The showcase screenshots for the website and the promo images. They tell the story of
// docs/brand/showcase/README.md on whole phone screens (1080 x 2340 px), in Chord Dark and Chord Light.
//
// The test is off by default, so the normal test and verify runs skip it. To record the PNGs into
// docs/brand/screenshots/android/, run in chord-android:
//
//   ./gradlew :app:testDebugUnitTest -Pchord.showcase=true -Proborazzi.test.record=true \
//       --tests 'space.foid.chord.ui.showcase.ShowcaseScreenshotTest'
//
// The art (avatars, space icons, photos) is a copy of docs/brand/showcase/ in
// src/test/resources/showcase/. Copy it again after `python3 make_art.py`.

import androidx.activity.ComponentActivity
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import com.github.takahirom.roborazzi.captureRoboImage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.R
import space.foid.chord.ui.attachments.ImageSource
import space.foid.chord.ui.attachments.ImageState
import space.foid.chord.ui.attachments.ImageStatus
import space.foid.chord.ui.attachments.LocalImageSource
import space.foid.chord.ui.avatar.AvatarData
import space.foid.chord.ui.avatar.AvatarRepository
import space.foid.chord.ui.avatar.AvatarSource
import space.foid.chord.ui.avatar.LocalAvatarRepository
import space.foid.chord.ui.layout.DrawerPane
import space.foid.chord.ui.layout.DualDrawer
import space.foid.chord.ui.layout.DualDrawerState
import space.foid.chord.ui.profile.ProfileCallbacks
import space.foid.chord.ui.profile.ProfileContent
import space.foid.chord.ui.profile.ProfileVariant
import space.foid.chord.ui.screens.ChannelDrawerContent
import space.foid.chord.ui.screens.MemberDrawerContent
import space.foid.chord.ui.screens.SignInContent
import space.foid.chord.ui.screens.TimelineContent
import space.foid.chord.ui.screens.TimelineRowUi
import space.foid.chord.ui.settings.AppPrefs
import space.foid.chord.ui.settings.AppearanceActions
import space.foid.chord.ui.settings.AppearanceContent
import space.foid.chord.ui.settings.AppearanceModel
import space.foid.chord.ui.settings.SettingsTopBar
import space.foid.chord.ui.settings.ThemeMode
import space.foid.chord.ui.sheets.SheetFrame
import space.foid.chord.ui.status.StatusState
import space.foid.chord.ui.text.formatPalette
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.ui.theme.TestThemes
import space.foid.chord.ui.timeline.typingText
import space.foid.chord.viewmodel.SignInState
import uniffi.chord_ffi.ChannelScope

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w411dp-h891dp-420dpi")
class ShowcaseScreenshotTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Before fun onlyWhenAsked() {
        assumeTrue("Run with -Pchord.showcase=true", System.getProperty("chord.showcase") == "true")
    }

    // ---------------------------------------------------------------- fakes for the art

    /** The avatar repository, filled before the first frame so no shot catches the initials. */
    private val avatars: AvatarRepository by lazy {
        val source = object : AvatarSource {
            override suspend fun avatar(key: String): AvatarData? =
                ShowcaseData.avatarArt[key]?.let { AvatarData(key, ShowcaseData.art(it)) }
            override suspend fun refresh(owner: String) = Unit
        }
        AvatarRepository(
            source = MutableStateFlow(source),
            scope = CoroutineScope(Dispatchers.Unconfined),
            decodeDispatcher = Dispatchers.Unconfined,
            maxCacheBytes = 256 shl 20,
        ).also { repo ->
            runBlocking {
                for (key in ShowcaseData.avatarArt.keys) for (px in listOf(32, 64, 128, 256, 512)) repo.load(key, null, px)
            }
        }
    }

    /** The attachments: the photos from test resources. */
    private val images = ImageSource { url ->
        val bmp = remember(url) { ShowcaseData.attachmentArt[url]?.let(ShowcaseData::bitmap) }
        if (bmp != null) ImageState(BitmapPainter(bmp), ImageStatus.LOADED) else ImageState(null, ImageStatus.FAILED)
    }

    // ---------------------------------------------------------------- the phone frame

    private fun shot(name: String, dark: Boolean, clock: String = "10:44", after: () -> Unit = {}, content: @Composable () -> Unit) {
        compose.runOnUiThread { WindowCompat.setDecorFitsSystemWindows(compose.activity.window, false) }
        val entry = TestThemes.entry(if (dark) "chord-dark" else "chord-light")
        compose.setContent {
            ChordTheme(dark = dark, colors = entry.info.colors(entry.info.defaultAccent)) {
                CompositionLocalProvider(LocalAvatarRepository provides avatars, LocalImageSource provides images) {
                    Box(Modifier.fillMaxSize().background(Chord.colors.surface100)) {
                        content()
                        StatusBar(clock, Modifier.align(Alignment.TopCenter))
                        NavHandle(Modifier.align(Alignment.BottomCenter))
                    }
                }
            }
        }
        // The system bars: Robolectric has none, so the shot gives the insets that a phone has.
        compose.runOnUiThread {
            val density = compose.activity.resources.displayMetrics.density
            val insets = WindowInsetsCompat.Builder()
                .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(0, (STATUS_DP * density).toInt(), 0, 0))
                .setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.of(0, 0, 0, (NAV_DP * density).toInt()))
                .build()
            ViewCompat.dispatchApplyWindowInsets(compose.activity.window.decorView, insets)
        }
        compose.waitForIdle()
        compose.runOnUiThread(after)
        compose.waitForIdle()
        compose.onRoot().captureRoboImage("$OUT/$name-${if (dark) "dark" else "light"}.png")
    }

    /** The status bar of the phone: the clock at the left, signal, wifi and battery at the right. */
    @Composable private fun StatusBar(clock: String, modifier: Modifier) {
        val ink = Chord.colors.ink
        Row(
            modifier.fillMaxWidth().height(STATUS_DP.dp).padding(horizontal = 24.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(clock, style = ChordType.label.copy(fontWeight = FontWeight.SemiBold), color = ink)
            Spacer(Modifier.weight(1f))
            Canvas(Modifier.size(width = 62.dp, height = 14.dp)) {
                val u = size.height
                // Signal: four rising bars.
                for (i in 0 until 4) {
                    val h = u * (0.35f + 0.65f * i / 3f)
                    drawRoundRect(ink, Offset(i * u * 0.28f, u - h), Size(u * 0.2f, h), CornerRadius(u * 0.05f))
                }
                // Wifi: a fan.
                val cx = u * 1.95f
                val wifi = Path().apply {
                    moveTo(cx, u)
                    lineTo(cx - u * 0.62f, u * 0.25f)
                    quadraticTo(cx, -u * 0.12f, cx + u * 0.62f, u * 0.25f)
                    close()
                }
                drawPath(wifi, ink)
                // Battery: body, level and cap.
                val bx = u * 2.9f
                val bw = u * 1.25f
                drawRoundRect(ink, Offset(bx, u * 0.18f), Size(bw, u * 0.64f), CornerRadius(u * 0.14f), style = Stroke(u * 0.09f))
                drawRoundRect(ink, Offset(bx + u * 0.12f, u * 0.3f), Size(bw * 0.68f, u * 0.4f), CornerRadius(u * 0.06f))
                drawRoundRect(ink, Offset(bx + bw + u * 0.04f, u * 0.38f), Size(u * 0.08f, u * 0.24f), CornerRadius(u * 0.04f))
            }
        }
    }

    /** The gesture handle of the navigation bar. */
    @Composable private fun NavHandle(modifier: Modifier) {
        Box(modifier.fillMaxWidth().height(NAV_DP.dp), contentAlignment = Alignment.Center) {
            Box(Modifier.width(108.dp).height(4.dp).background(Chord.colors.ink.copy(alpha = 0.55f), RoundedCornerShape(2.dp)))
        }
    }

    // ---------------------------------------------------------------- the screens

    @Composable private fun Playtest(listState: LazyListState? = null) {
        val palette = Chord.colors.formatPalette()
        val rows = remember(palette) { ShowcaseData.playtestRows(palette) }
        Timeline(rows, "playtest", topic = "Build 0.14.2 is live. Bugs go in #bugs.", typing = typingText(listOf("Kenji")), listState = listState)
    }

    @Composable private fun Timeline(
        rows: List<TimelineRowUi>,
        title: String,
        topic: String? = null,
        typing: String = "",
        isRoom: Boolean = true,
        presence: space.foid.chord.ui.components.Presence? = null,
        reachedStart: Boolean = false,
        listState: LazyListState? = null,
    ) {
        val state = listState ?: androidx.compose.foundation.lazy.rememberLazyListState()
        TimelineContent(
            rows = rows, title = title, isRoom = isRoom, topic = topic, presence = presence, typing = typing,
            reachedStart = reachedStart, listState = state,
            mentionNicks = ShowcaseData.lanternMembers.map { it.name },
            // A scrolled list shows "Jump to present" over the last row. The promo shot hides it.
            jumpToPresent = false,
        )
    }

    @Composable private fun LanternDrawer() = ChannelDrawerContent(
        spaces = ShowcaseData.spaces,
        scope = ChannelScope.Space(ShowcaseData.SPACES, "lantern-works"),
        onScope = {},
        channels = ShowcaseData.lanternChannels,
        loaded = true,
        selectedJid = ShowcaseData.playtest.jid,
        onSelect = {},
        account = ShowcaseData.account,
        onSignOut = {},
        spaceUnread = ShowcaseData.spaceUnread,
        homeUnread = ShowcaseData.HOME_UNREAD,
        status = StatusState(status = ShowcaseData.maya.status),
        contacts = ShowcaseData.contacts,
        pendingContacts = ShowcaseData.PENDING_CONTACTS,
        mutedJids = ShowcaseData.mutedLantern,
        onCreateChannel = {},
    )

    @Composable private fun HomeDrawer() = ChannelDrawerContent(
        spaces = ShowcaseData.spaces,
        scope = ChannelScope.Home,
        onScope = {},
        channels = ShowcaseData.homeChannels,
        loaded = true,
        selectedJid = null,
        onSelect = {},
        account = ShowcaseData.account,
        onSignOut = {},
        spaceUnread = ShowcaseData.spaceUnread,
        homeUnread = ShowcaseData.HOME_UNREAD,
        status = StatusState(status = ShowcaseData.maya.status),
        contacts = ShowcaseData.contacts,
        pendingContacts = ShowcaseData.PENDING_CONTACTS,
    )

    @Composable private fun TheoDm() {
        val palette = Chord.colors.formatPalette()
        val rows = remember(palette) { ShowcaseData.theoRows(palette) }
        Timeline(rows, ShowcaseData.theo.name, isRoom = false, presence = ShowcaseData.theo.presence, reachedStart = true)
    }

    @Composable private fun ShowAndTell() {
        val palette = Chord.colors.formatPalette()
        val rows = remember(palette) { ShowcaseData.showAndTellRows(palette) }
        Timeline(rows, "show-and-tell")
    }

    /** A drawer container with [pane] open. */
    @Composable private fun Drawers(pane: DrawerPane, left: @Composable () -> Unit, right: @Composable () -> Unit, center: @Composable () -> Unit) {
        val state = remember { DualDrawerState(pane) }
        DualDrawer(state, left = left, right = right) { center() }
    }

    // ---------------------------------------------------------------- the shots

    /**
     * The hero slice of #playtest: from Jules' lighthouse shot down to Kenji's "on it". The list is
     * newest first, so index [HERO_INDEX] is the row at the bottom edge.
     */
    private fun timeline(dark: Boolean) {
        val list = LazyListState()
        shot("01-timeline", dark, after = { list.requestScrollToItem(HERO_INDEX, HERO_OFFSET) }) { Playtest(list) }
    }

    private fun drawer(dark: Boolean) = shot("02-drawer", dark) {
        Drawers(DrawerPane.Left, left = { LanternDrawer() }, right = {}) { Playtest() }
    }

    private fun members(dark: Boolean) = shot("03-members", dark) {
        Drawers(
            DrawerPane.Right, left = {},
            right = { MemberDrawerContent("playtest", ShowcaseData.lanternMembers, loaded = true) },
        ) { Playtest() }
    }

    private fun home(dark: Boolean) = shot("04-home", dark, clock = "11:52") {
        Drawers(DrawerPane.Left, left = { HomeDrawer() }, right = {}) { Playtest() }
    }

    private fun dm(dark: Boolean) = shot("05-dm", dark, clock = "11:52") { TheoDm() }

    private fun darkroom(dark: Boolean) = shot("06-darkroom", dark, clock = "9:12") { ShowAndTell() }

    private fun profile(dark: Boolean) = shot("07-profile", dark) {
        Box(Modifier.fillMaxSize()) {
            Playtest()
            Box(Modifier.fillMaxSize().background(Chord.colors.scrim))
            Box(Modifier.align(Alignment.BottomCenter)) {
                SheetFrame {
                    Column(Modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = ChordSpace.s4)) {
                        ProfileContent(ShowcaseData.priyaProfile, ProfileVariant.Sheet, ProfileCallbacks(), Chord.colors.surface200)
                    }
                }
            }
        }
    }

    private fun appearance(dark: Boolean) = shot("08-appearance", dark, clock = "10:50") {
        val prefs = AppPrefs(theme = if (dark) ThemeMode.Dark else ThemeMode.Light)
        Column(Modifier.fillMaxSize().background(Chord.colors.surface100).statusBarsPadding()) {
            SettingsTopBar(stringResource(R.string.settings_appearance)) {}
            Column(
                Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = ChordSpace.s4),
                verticalArrangement = Arrangement.spacedBy(ChordSpace.s2),
            ) {
                AppearanceContent(AppearanceModel(prefs, TestThemes.bundled()), AppearanceActions())
            }
        }
    }

    private fun signIn(dark: Boolean) = shot("09-signin", dark, clock = "8:58") {
        SignInContent(
            state = SignInState(jid = ShowcaseData.maya.jid, password = "lantern-works-0.14"),
            onJidChange = {}, onPasswordChange = {}, onServerChange = {}, onSubmit = {},
        )
    }

    @Test fun timeline_dark() = timeline(true)
    @Test fun timeline_light() = timeline(false)
    @Test fun drawer_dark() = drawer(true)
    @Test fun drawer_light() = drawer(false)
    @Test fun members_dark() = members(true)
    @Test fun members_light() = members(false)
    @Test fun home_dark() = home(true)
    @Test fun home_light() = home(false)
    @Test fun dm_dark() = dm(true)
    @Test fun dm_light() = dm(false)
    @Test fun darkroom_dark() = darkroom(true)
    @Test fun darkroom_light() = darkroom(false)
    @Test fun profile_dark() = profile(true)
    @Test fun profile_light() = profile(false)
    @Test fun appearance_dark() = appearance(true)
    @Test fun appearance_light() = appearance(false)
    @Test fun signin_dark() = signIn(true)
    @Test fun signin_light() = signIn(false)

    private companion object {
        /** Relative to the module folder, where the JVM tests run. */
        const val OUT = "../../docs/brand/screenshots/android"
        const val STATUS_DP = 32
        const val NAV_DP = 20
        const val HERO_INDEX = 5
        const val HERO_OFFSET = 0
    }
}
