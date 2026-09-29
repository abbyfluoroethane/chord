//! Chord Desktop: the Tauri shell on chord-core.
//!
//! The Rust side is a thin bridge. `commands.rs` maps each UI command to one
//! `ClientHandle` call. `state.rs` keeps the open account and its view subscriptions.
//! Views and events leave through Tauri channels, as JSON that chord-core's `serde`
//! feature produces. The TypeScript side is in src/lib/chord/.

mod avatars;
mod commands;
mod error;
mod keychain;
mod link_preview;
mod notify;
mod settings;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::default());
    avatars::register(builder)
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
            commands::upload,
            commands::load_older,
            commands::join_room,
            commands::leave_room,
            commands::change_nick,
            commands::send_private,
            commands::room_service,
            commands::set_avatar,
            commands::remove_avatar,
            commands::set_room_affiliation,
            commands::room_affiliations,
            commands::invite_to_room,
            commands::decline_room_invite,
            commands::configure_room,
            commands::browse_spaces,
            commands::join_space,
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
            commands::approve_subscription,
            commands::deny_subscription,
            commands::preapprove_subscription,
            commands::refresh_avatar,
            commands::set_notification_level,
            commands::notification_level,
            commands::push_registrations,
            link_preview::link_preview,
            link_preview::save_image,
            settings::get_settings,
            settings::set_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
