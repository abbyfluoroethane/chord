package space.foid.chord.viewmodel

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.CreationExtras
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import space.foid.chord.ChordApp
import space.foid.chord.data.TimelineTarget
import space.foid.chord.data.chatApi
import uniffi.chord_ffi.ChannelScope

/**
 * ViewModel factories. Each reads the [ChordSession][space.foid.chord.data.ChordSession] from [ChordApp].
 * Use them with `viewModel(factory = ChordViewModels.spaceList)`. A ViewModel with a
 * parameter needs a key too: `viewModel(key = "timeline/$jid", factory = ChordViewModels.timeline(target))`.
 */
object ChordViewModels {
    private fun CreationExtras.app(): ChordApp =
        this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY] as ChordApp

    val signIn: ViewModelProvider.Factory = viewModelFactory {
        initializer { SignInViewModel(app().session) }
    }

    val spaceList: ViewModelProvider.Factory = viewModelFactory {
        initializer { SpaceListViewModel(app().session.chatApi()) }
    }

    fun channelList(initialScope: ChannelScope = ChannelScope.Home): ViewModelProvider.Factory = viewModelFactory {
        initializer { ChannelListViewModel(app().session.chatApi(), initialScope) }
    }

    fun timeline(target: TimelineTarget): ViewModelProvider.Factory = viewModelFactory {
        initializer { TimelineViewModel(target, app().session.chatApi(), events = app().session.events) }
    }

    fun memberList(room: String): ViewModelProvider.Factory = viewModelFactory {
        initializer { MemberListViewModel(room, app().session.chatApi()) }
    }
}
