//! Chord Desktop: the Tauri shell on chord-core.
//!
//! The Rust side is a thin bridge. `commands.rs` maps each UI command to one
//! `ClientHandle` call. `state.rs` keeps the open account and its view subscriptions.
//! Views and events leave through Tauri channels, as JSON that chord-core's `serde`
//! feature produces. The TypeScript side is in src/lib/chord/.

mod advanced;
mod avatars;
mod badge;
mod certpin;
mod commands;
mod emoji;
mod error;
mod files;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    set_runtime();
    let builder = tauri::Builder::default();
    // The single-instance plugin must be the first plugin. A second copy of the app sends its
    // arguments (an xmpp: link on Windows and Linux) to the first copy and quits.
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(links::second_instance));
    let builder = builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(navigation::plugin())
        .manage(state::AppState::default())
        .manage(files::Dropped::default())
        .manage(notify::NoticePrefs::default())
        .manage(certpin::CertPins::default())
        // The path of a dropped file goes to Rust here, not through the page.
        .on_webview_event(|webview, event| {
            if let tauri::WebviewEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                use tauri::Manager;
                webview.state::<files::Dropped>().remember(paths);
            }
        })
        .setup(|app| {
            settings::init_notice_prefs(app.handle());
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
            theme_fetch::theme_fetch,
            gif::gif_search,
            emoji::emoji_packs,
            emoji::emoji_pack_install,
            settings::get_settings,
            settings::set_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
