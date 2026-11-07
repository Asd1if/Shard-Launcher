mod accounts;
mod auth;
mod content;
mod crash;
mod discord;
mod download;
mod error;
mod cpm;
mod figura;
mod tunnel;
mod import_launchers;
mod game_options;
mod snapshots;
mod news;
mod updater;
mod playtime;
mod server_backup;
mod server_files;
mod server_players;
mod server_store;
mod icon_shrink;
mod fix;
mod game;
mod instances;
mod launch;
mod loaders;
mod mclogs;
mod modmeta;
mod modpack;
mod nbt;
mod paths;
mod progress;
mod server;
mod settings;
mod share;
mod skins;
mod state;
mod versions;
mod worlds;

#[cfg(test)]
mod smoke_test;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let root = settings::data_root(&app.path().app_data_dir()?);
            std::fs::create_dir_all(&root)?;
            // Скриншоты и иконки миров показываем прямо с диска через asset-протокол
            app.asset_protocol_scope().allow_directory(root.join("instances"), true)?;
            app.asset_protocol_scope().allow_directory(root.join("backgrounds"), false)?;
            let state = state::AppState::new(root);
            state.paths.migrate_instance_folders();
            server::migrate(&state.paths);
            let loaded = settings::load_full(&state.paths);
            content::curseforge::set_user_key(loaded.curseforge_key.clone());
            let discord_disabled = loaded.discord_disabled;
            app.manage(state);
            server::spawn_supervisor(app.handle().clone());
            updater::cleanup();
            tauri::async_runtime::spawn(discord::set_disabled(discord_disabled));
            discord::watch();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            server::server_info,
            server::server_list,
            accounts::account_set_skin,
            content::content_categories,
            server_players::server_access,
            server_players::server_access_change,
            server_players::server_kick,
            server::server_stats,
            server::server_add_to_instance,
            playtime::playtime_get,
            playtime::playtime_all,
            game_options::options_copy,
            snapshots::snapshots,
            news::mc_news,
            updater::update_check,
            updater::update_available,
            updater::update_install,
            news::mc_news_body,
            snapshots::snapshot_create,
            snapshots::snapshot_delete,
            snapshots::snapshot_restore,
            content::optimize::content_optimize,
            server_files::server_files,
            server_files::server_file_read,
            server_files::server_file_write,
            server_files::server_file_delete,
            worlds::world_restore,
            import_launchers::launcher_scan,
            import_launchers::launcher_import,
            worlds::world_backup_delete,
            server_backup::server_backups,
            server_backup::server_backup,
            server_backup::server_backup_restore,
            server_backup::server_backup_delete,
            content::details::content_project,
            content::details::content_install_version,
            server::server_rename,
            server::server_open_folder,
            server_store::server_store_search,
            server_store::server_store_list,
            server_store::server_store_install,
            server_store::server_store_remove,
            server::server_logs,
            server::server_core_versions,
            server::server_create,
            server::server_sync_mods,
            server::server_properties,
            server::server_set_properties,
            server::server_set_config,
            server::server_delete,
            server::server_start,
            server::server_command,
            server::server_stop,
            server::server_reshare,
            server::server_create_from_pack,
            server::server_set_icon,
            settings::curseforge_check_key,
            fix::crash_fix_plan,
            fix::crash_fix_apply,
            fix::content_missing_deps,
            fix::content_install_deps,
            mclogs::logs_upload,
            modpack::server_pack::server_pack_available,
            discord::discord_idle,
            versions::minecraft_versions,
            loaders::loader_versions,
            content::content_search,
            content::content_list,
            content::content_install,
            content::content_toggle,
            content::content_remove,
            content::content_updates,
            content::content_update,
            content::content_sources,
            content::content_manual_take,
            content::content_add_files,
            modpack::modpack_import,
            modpack::modpack_search,
            modpack::modpack_export_entries,
            modpack::modpack_export,
            modpack::modpack_versions,
            modpack::modpack_change_version,
            modpack::instance_original_icon,
            instances::instances_list,
            instances::instance_create,
            instances::instance_delete,
            instances::instance_open_dir,
            accounts::accounts_list,
            accounts::account_add_offline,
            accounts::account_select,
            accounts::account_remove,
            launch::launch_instance,
            launch::stop_instance,
            launch::running_instances,
            launch::java_detect,
            launch::java_probe,
            launch::system_info,
            instances::instance_update,
            instances::instance_set_icon,
            instances::instance_set_meta,
            instances::instance_clone,
            worlds::worlds_list,
            worlds::world_backups,
            worlds::world_backup,
            worlds::screenshots_list,
            worlds::instance_open_path,
            worlds::open_crash_report,
            settings::settings_get,
            settings::settings_set,
            settings::settings_data_dir,
            settings::settings_move_data,
            settings::background_get,
            settings::background_set,
            settings::background_luma,
            skins::skins_list,
            skins::skin_data,
            skins::skin_add_file,
            skins::skin_add_data,
            skins::skin_add_player,
            skins::skin_update,
            skins::skin_remove,
            skins::skin_export,
            skins::skin_profile,
            skins::skin_texture,
            skins::skin_apply,
            skins::skin_set_cape,
            figura::figura_list,
            figura::figura_add,
            figura::figura_remove,
            figura::figura_rename,
            figura::figura_files,
            figura::figura_installed,
            figura::figura_install,
            figura::figura_uninstall,
            figura::figura_open,
            tunnel::tunnel_status,
            tunnel::tunnel_link_start,
            tunnel::tunnel_link_finish,
            tunnel::tunnel_unlink,
            server::server_set_tunnel,
            cpm::cpm_list,
            cpm::cpm_add,
            cpm::cpm_remove,
            cpm::cpm_installed,
            cpm::cpm_install,
            cpm::cpm_uninstall,
            cpm::cpm_open,
            auth::msa_start,
            auth::msa_finish,
            auth::msa_cancel,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, event| {
            // Серверы получают команду stop и сохраняют мир, даже когда лаунчер закрывают
            if let tauri::RunEvent::ExitRequested { .. } = event {
                tauri::async_runtime::block_on(server::stop_all());
            }
        });
}
