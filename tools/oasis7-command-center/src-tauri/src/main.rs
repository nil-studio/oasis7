#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod context_commands;
mod runtime_commands;
mod tool_commands;

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
            app.manage(Host::open(directory.join("command-center.sqlite"))?);
            app.manage(context_commands::ContextHost::open(
                directory.join("command-center.sqlite"),
            )?);
            let tools =
                tool_commands::ToolHost::open(directory.join("command-center-tools.sqlite"))?;
            let runtime = runtime_commands::RuntimeHost::open_with_jobs(
                directory.join("command-center-runtime.sqlite"),
                tools.jobs.clone(),
            )?;
            app.manage(tools);
            app.manage(runtime);
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
                    "quit" => app
                        .state::<runtime_commands::RuntimeHost>()
                        .request_exit(app.clone()),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app
                .state::<runtime_commands::RuntimeHost>()
                .request_exit(app.clone()),
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
            runtime_commands::preview_private_input,
            runtime_commands::start_private_runtime_turn,
            commands::open_project,
            commands::query_view,
            commands::query_work_projection,
            commands::refresh_work_checks,
            commands::search_rules,
            commands::query_document_inventory,
            commands::query_document_directory,
            commands::query_document_semantic_tree,
            commands::load_document_tree_reading,
            commands::save_document_tree_reading,
            commands::reset_document_tree_reading,
            commands::load_document_tree_last_view,
            commands::save_decision_draft,
            commands::save_preference,
            commands::read_source,
            commands::refresh_source,
            commands::rebuild_cache,
            commands::refresh_github,
            commands::get_github_snapshot,
            commands::open_source_target,
            commands::choose_project,
            commands::associate_work,
            commands::read_version,
            runtime_commands::get_runtime,
            runtime_commands::reconcile_runtime_operation,
            runtime_commands::set_runtime_enabled,
            runtime_commands::connect_runtime,
            runtime_commands::create_runtime_session,
            runtime_commands::continue_runtime_session,
            runtime_commands::start_runtime_turn,
            runtime_commands::steer_runtime_turn,
            runtime_commands::interrupt_runtime_turn,
            runtime_commands::answer_runtime_request,
            runtime_commands::get_exit_status,
            runtime_commands::request_exit_wait,
            runtime_commands::request_exit_stop,
            runtime_commands::cancel_exit_wait,
        ])
        .build(tauri::generate_context!())
        .expect("initialize Command Center desktop")
        .run(|app, event| {
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main(app);
            }
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let runtime = app.state::<runtime_commands::RuntimeHost>();
                if !runtime.exit_is_approved() {
                    api.prevent_exit();
                    runtime.request_exit(app.clone());
                }
            }
        });
}
