#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod context_commands;
mod generated_context;
mod tool_commands;
mod tool_exit;

use commands::Host;
use tauri::{
    Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
};

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            app.manage(context_commands::ContextHost::open(
                directory.join("command-center.sqlite"),
            )?);
            let tools =
                tool_commands::ToolHost::open(directory.join("command-center-tools.sqlite"))?;
            app.manage(tools);
            app.manage(tool_exit::ToolExit::default());
            app.manage(Host::open_with_legacy_privacy(
                directory.join("command-center.sqlite"),
                directory.join("command-center-runtime.sqlite"),
            )?);
            let show = MenuItem::with_id(app, "show", "打开指挥中心", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出指挥中心", true, Some("CmdOrCtrl+Q"))?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let application_menu =
                Submenu::with_items(app, "Oasis7 指挥中心", true, &[&show, &quit])?;
            let edit_menu = Submenu::with_items(
                app,
                "编辑",
                true,
                &[
                    &PredefinedMenuItem::undo(app, Some("撤销"))?,
                    &PredefinedMenuItem::redo(app, Some("重做"))?,
                    &PredefinedMenuItem::cut(app, Some("剪切"))?,
                    &PredefinedMenuItem::copy(app, Some("复制"))?,
                    &PredefinedMenuItem::paste(app, Some("粘贴"))?,
                    &PredefinedMenuItem::select_all(app, Some("全选"))?,
                ],
            )?;
            app.set_menu(Menu::with_items(app, &[&application_menu, &edit_menu])?)?;
            let icon = app
                .default_window_icon()
                .ok_or("packaged application icon missing")?
                .clone();
            TrayIconBuilder::new()
                .icon(icon)
                .tooltip("Oasis7 指挥中心")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => tool_exit::request_exit(app.clone()),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => tool_exit::request_exit(app.clone()),
            _ => {}
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            context_commands::context_info,
            context_commands::context_reconcile,
            context_commands::context_bindings,
            context_commands::context_choose_library,
            context_commands::context_list,
            context_commands::context_get,
            context_commands::context_save,
            context_commands::context_archive,
            context_commands::context_history,
            context_commands::context_restore_entry,
            context_commands::context_doctor,
            context_commands::context_snapshot_export,
            context_commands::context_snapshot_inspect,
            context_commands::context_snapshot_restore,
            tool_commands::tool_file_reconcile,
            tool_commands::tool_file_recover,
            tool_commands::tool_cli_reconcile,
            tool_commands::tool_cli_observe,
            tool_commands::tool_jobs,
            tool_commands::tool_signer_observe,
            tool_commands::tool_signer_doctor,
            tool_commands::tool_choose_file,
            tool_commands::tool_file_verify,
            tool_commands::tool_file_prepare,
            tool_commands::tool_signer_submit,
            tool_commands::tool_signer_inspect,
            tool_commands::tool_cli_install,
            tool_commands::secret_stage,
            commands::open_project,
            commands::query_view,
            commands::import_sign_context,
            commands::search_rules,
            commands::query_document_inventory,
            commands::query_document_directory,
            commands::query_document_semantic_tree,
            commands::load_document_tree_reading,
            commands::save_document_tree_reading,
            commands::reset_document_tree_reading,
            commands::load_document_tree_last_view,
            commands::save_preference,
            commands::read_source,
            commands::refresh_source,
            commands::rebuild_cache,
            commands::open_source_target,
            commands::choose_project,
            commands::read_version,
        ])
        .build(generated_context::create())
        .expect("initialize Command Center desktop")
        .run(|app, event| {
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main(app);
            }
            if let tauri::RunEvent::ExitRequested { api, .. } = event
                && !app.state::<tool_exit::ToolExit>().approved()
            {
                api.prevent_exit();
                tool_exit::request_exit(app.clone());
            }
        });
}
