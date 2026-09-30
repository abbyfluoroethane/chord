//! Chord Desktop: the Tauri shell on chord-core.
//!
//! The Rust side is a thin bridge. `commands.rs` maps each UI command to one
//! `ClientHandle` call. `state.rs` keeps the open account and its view subscriptions.
//! Views and events leave through Tauri channels, as JSON that chord-core's `serde`
//! feature produces. The TypeScript side is in src/lib/chord/.

mod avatars;
mod commands;
mod emoji;
mod error;
mod files;
mod gif;
mod keychain;
mod link_preview;
mod links;
mod navigation;
mod notify;
mod pins;
mod settings;
mod state;
mod theme_fetch;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
            files::upload_files,
            files::upload_dropped,
            files::upload_pasted,
            commands::set_client_active,
            commands::load_older,
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
            commands::set_avatar,
            commands::remove_avatar,
            commands::set_room_affiliation,
            commands::room_affiliations,
            commands::invite_to_room,
            commands::decline_room_invite,
            commands::configure_room,
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
            commands::contacts,
            commands::block_contact,
            commands::unblock_contact,
            commands::unblock_all,
            commands::blocked_contacts,
            commands::add_contact,
            commands::remove_contact,
            commands::rename_contact,
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
