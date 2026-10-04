//! Chord Desktop: the Tauri shell on chord-core.
//!
//! The Rust side is a thin bridge. `commands.rs` maps each UI command to one
//! `ClientHandle` call. `state.rs` keeps the open account and its view subscriptions.
//! Views and events leave through Tauri channels, as JSON that chord-core's `serde`
//! feature produces. The TypeScript side is in src/lib/chord/.

mod advanced;
mod avatars;
mod badge;
mod behaviour;
mod certpin;
mod commands;
mod emoji;
mod error;
mod files;
mod flatpak;
mod forms;
mod gif;
mod keychain;
mod link_preview;
mod links;
mod navigation;
mod notify;
mod pins;
mod privacy;
mod settings;
mod state;
mod theme_fetch;
mod thumb;
mod updates;

/// The worker threads of the async runtime. The work is network and database waits, so a
/// few threads are enough. Tokio starts one thread per core by default, and each thread
/// keeps its own stack and allocator state.
const WORKER_THREADS: usize = 3;

/// Give Tauri a runtime with a small, fixed worker pool. Blocking tasks (file reads, the
/// avatar store) run on the separate blocking pool of tokio, which ends idle threads.
fn set_runtime() {
    match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(WORKER_THREADS)
        .thread_name("chord-worker")
        .enable_all()
        .build()
    {
        Ok(runtime) => {
            tauri::async_runtime::set(runtime.handle().clone());
            // Tauri holds only the handle. The runtime must live as long as the process.
            std::mem::forget(runtime);
        }
        Err(e) => log::warn!("cannot build the runtime, Tauri keeps its own: {e}"),
    }
}

/// Picks the crypto backend of rustls for the whole process. tokio-xmpp uses aws-lc-rs, and
/// tauri-plugin-updater turns on ring too. With two backends rustls cannot choose, and the
/// first TLS connection (the login) panicked: "Could not automatically determine the
/// process-level CryptoProvider".
fn install_crypto_provider() {
    // An error means a provider is in place already, which is fine.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_crypto_provider();
    set_runtime();
    let builder = tauri::Builder::default();
    // The single-instance plugin must be the first plugin. A second copy of the app sends its
    // arguments (an xmpp: link on Windows and Linux) to the first copy and quits.
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(links::second_instance));
    // Open Chord at login (src/behaviour.rs). The argument lets a start from the login
    // entry open hidden.
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    let builder = builder.plugin(
        tauri_plugin_autostart::Builder::new()
            .arg(behaviour::AUTOSTART_ARG)
            .build(),
    );
    let builder = builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        // The endpoints come from the chosen channel at each check (src/updates.rs). Only
        // Windows and macOS use it: on Linux the Flatpak portal does the updates.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(navigation::plugin())
        .manage(state::AppState::default())
        .manage(files::Dropped::default())
        .manage(notify::NoticePrefs::default())
        .manage(certpin::CertPins::default())
        .manage(behaviour::Behaviour::default())
        .manage(updates::Pending::default());
    // In a Flatpak the portal does the updates (src/flatpak.rs).
    #[cfg(target_os = "linux")]
    let builder = builder.manage(flatpak::Updater::default());
    let builder = builder
        .on_window_event(behaviour::on_window_event)
        // The path of a dropped file goes to Rust here, not through the page.
        .on_webview_event(|webview, event| {
            if let tauri::WebviewEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                use tauri::Manager;
                webview.state::<files::Dropped>().remember(paths);
            }
        })
        .setup(|app| {
            settings::init_notice_prefs(app.handle());
            behaviour::setup(app.handle());
            links::setup(app)
        });
    emoji::register(avatars::register(builder))
        .invoke_handler(tauri::generate_handler![
            commands::open,
            advanced::storage_info,
            advanced::clear_caches,
            advanced::clear_history,
            advanced::app_info,
            advanced::server_features,
            commands::login,
            commands::logout,
            commands::saved_password,
            commands::forget_password,
            commands::subscribe_space_list,
            commands::subscribe_channel_list,
            commands::subscribe_timeline,
            commands::subscribe_private_timeline,
            commands::subscribe_member_list,
            commands::unsubscribe,
            commands::timeline_paginate_back,
            commands::events,
            commands::send_chat,
            commands::send_link,
            commands::edit_message,
            commands::retract_message,
            commands::moderate_message,
            commands::reply,
            commands::react,
            commands::toggle_reaction,
            commands::mark_read,
            commands::mark_read_private,
            commands::mark_unread,
            commands::set_typing,
            commands::close_chat,
            files::upload_files,
            files::upload_dropped,
            files::upload_pasted,
            commands::set_client_active,
            commands::join_room,
            commands::room_info,
            commands::search_messages,
            commands::leave_room,
            commands::add_bookmark,
            commands::remove_bookmark,
            commands::change_nick,
            commands::send_private,
            commands::room_service,
            commands::set_presence,
            commands::own_presence,
            commands::invisible_method,
            commands::set_idle,
            commands::set_share_info,
            commands::set_notices,
            commands::archive_default,
            commands::set_archive_default,
            privacy::privacy_cache_info,
            privacy::clear_privacy_cache,
            commands::set_avatar,
            commands::remove_avatar,
            commands::set_room_affiliation,
            commands::room_affiliations,
            commands::invite_to_room,
            commands::decline_room_invite,
            commands::configure_room,
            commands::set_room_subject,
            commands::set_room_role,
            commands::browse_spaces,
            commands::join_space,
            commands::space_info,
            commands::leave_space,
            commands::create_space,
            commands::delete_space,
            commands::pending_space_joins,
            commands::space_join_requests,
            commands::approve_space_join,
            commands::deny_space_join,
            commands::add_room_to_space,
            commands::remove_room_from_space,
            commands::add_space_member,
            commands::space_members,
            commands::remove_space_member,
            commands::ban_space_member,
            commands::space_config,
            commands::configure_space,
            commands::set_space_avatar,
            commands::set_space_banner,
            commands::contacts,
            commands::block_contact,
            commands::block_and_report,
            commands::unblock_contact,
            commands::unblock_all,
            commands::blocked_contacts,
            commands::add_contact,
            commands::set_nickname,
            commands::profile,
            commands::set_profile,
            commands::own_devices,
            commands::remove_contact,
            commands::rename_contact,
            commands::set_contact_groups,
            commands::approve_subscription,
            commands::deny_subscription,
            commands::preapprove_subscription,
            commands::refresh_avatar,
            commands::set_notification_level,
            commands::notification_level,
            commands::push_registrations,
            pins::pin_message,
            pins::unpin_message,
            pins::pins,
            pins::refresh_pins,
            badge::set_unread_count,
            notify::send_test_notice,
            forms::list_commands,
            forms::command_step,
            forms::room_config_form,
            forms::submit_room_config_form,
            forms::answer_room_captcha,
            forms::cancel_room_captcha,
            forms::change_password,
            forms::delete_account,
            certpin::cert_status,
            certpin::cert_trust,
            certpin::cert_clear,
            forms::registration_form,
            forms::register_account,
            link_preview::link_preview,
            link_preview::link_image,
            files::save_image,
            files::save_text,
            theme_fetch::theme_fetch,
            gif::gif_search,
            emoji::emoji_packs,
            emoji::emoji_pack_install,
            settings::get_settings,
            settings::set_settings,
            behaviour::get_autostart,
            behaviour::set_autostart,
            behaviour::set_tray_unread,
            behaviour::system_idle_seconds,
            updates::update_check,
            updates::update_install,
            updates::update_restart,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| behaviour::on_run_event(app, &event));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustls_has_a_crypto_provider() {
        install_crypto_provider();
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
        // A TLS client config builds without a panic, as the login does.
        let _ = rustls::ClientConfig::builder();
    }
}
