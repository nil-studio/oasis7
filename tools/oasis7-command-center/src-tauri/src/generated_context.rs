// CodeQL excludes only this generated-context boundary; keep business logic outside.
// Normal builds always evaluate the real Tauri macro.
pub(super) fn create() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}
