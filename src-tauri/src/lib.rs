use compas_docs_core::backend::{
    add_folder_with_file_types, default_document_config, index_folder_with_progress, library_stats,
    list_folders, open_document, remove_folder, reveal_in_finder, search_documents, FolderRecord,
    IndexProgress, LibraryStats, SearchDocumentItem,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{MenuBuilder, SubmenuBuilder};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

const MENU_OPEN_LIBRARY: &str = "open-library";
const MENU_OPEN_STATS: &str = "open-stats";
const MENU_OPEN_SEARCH: &str = "open-search";
const NAVIGATE_EVENT: &str = "app:navigate";
const INDEX_PROGRESS_EVENT: &str = "index:progress";
const WINDOW_MAIN: &str = "main";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderRecordDto {
    id: String,
    path: String,
    display_name: String,
    storage_path: String,
    file_types: Vec<String>,
    last_indexed_at: Option<u64>,
    watch_enabled: bool,
}

impl From<FolderRecord> for FolderRecordDto {
    fn from(value: FolderRecord) -> Self {
        Self {
            id: value.id,
            path: value.path,
            display_name: value.display_name,
            storage_path: value.storage_path,
            file_types: value.file_types,
            last_indexed_at: value.last_indexed_at,
            watch_enabled: value.watch_enabled,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchDocumentItemDto {
    folder_id: String,
    folder_name: String,
    file_path: String,
    absolute_path: String,
    title: String,
    section: String,
    page: String,
    preview: String,
    score: f32,
}

impl From<SearchDocumentItem> for SearchDocumentItemDto {
    fn from(value: SearchDocumentItem) -> Self {
        Self {
            folder_id: value.folder_id,
            folder_name: value.folder_name,
            file_path: value.file_path,
            absolute_path: value.absolute_path,
            title: value.title,
            section: value.section,
            page: value.page,
            preview: value.preview,
            score: value.score,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoveFolderResponse {
    removed: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct IndexProgressDto {
    folder_id: String,
    /// One of: "started" | "file" | "finalizing" | "completed" | "failed".
    /// Kept as a string so the frontend can switch on it cleanly.
    phase: String,
    processed_files: usize,
    total_files: usize,
    current_path: Option<String>,
    file_status: Option<String>,
}

impl IndexProgressDto {
    fn from_event(folder_id: &str, event: IndexProgress) -> Self {
        match event {
            IndexProgress::Started { total_files } => Self {
                folder_id: folder_id.to_string(),
                phase: "started".into(),
                processed_files: 0,
                total_files,
                current_path: None,
                file_status: None,
            },
            IndexProgress::File {
                processed_files,
                total_files,
                path,
                status,
            } => Self {
                folder_id: folder_id.to_string(),
                phase: "file".into(),
                processed_files,
                total_files,
                current_path: Some(path),
                file_status: Some(status.as_str().to_string()),
            },
            IndexProgress::Finalizing { total_files } => Self {
                folder_id: folder_id.to_string(),
                phase: "finalizing".into(),
                processed_files: total_files,
                total_files,
                current_path: None,
                file_status: None,
            },
        }
    }

    fn terminal(folder_id: &str, ok: bool) -> Self {
        Self {
            folder_id: folder_id.to_string(),
            phase: if ok {
                "completed".into()
            } else {
                "failed".into()
            },
            processed_files: 0,
            total_files: 0,
            current_path: None,
            file_status: None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryStatsDto {
    folder_count: usize,
    indexed_folder_count: usize,
    document_count: usize,
    chunk_count: usize,
    last_indexed_at: Option<u64>,
}

impl From<LibraryStats> for LibraryStatsDto {
    fn from(value: LibraryStats) -> Self {
        Self {
            folder_count: value.folder_count,
            indexed_folder_count: value.indexed_folder_count,
            document_count: value.document_count,
            chunk_count: value.chunk_count,
            last_indexed_at: value.last_indexed_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
enum AppView {
    Main,
    Library,
    Stats,
}

impl AppView {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Library => "library",
            Self::Stats => "stats",
        }
    }
}

#[tauri::command]
fn list_document_folders() -> Result<Vec<FolderRecordDto>, String> {
    Ok(list_folders()
        .into_iter()
        .map(FolderRecordDto::from)
        .collect())
}

#[tauri::command]
fn add_document_folder(
    path: String,
    file_types: Option<Vec<String>>,
) -> Result<FolderRecordDto, String> {
    add_folder_with_file_types(PathBuf::from(path).as_path(), file_types)
        .map(FolderRecordDto::from)
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn index_document_folder(
    app: tauri::AppHandle,
    folder_id: String,
    path: String,
    file_types: Option<Vec<String>>,
) -> Result<FolderRecordDto, String> {
    let path = PathBuf::from(path);
    let config = default_document_config(&path);

    // Stream progress to the frontend so the Library can render a real progress
    // bar instead of a generic spinner. Events are scoped to a folder_id so the
    // UI can match them to the right card if multiple jobs ever run concurrently.
    let app_for_progress = app.clone();
    let folder_id_for_progress = folder_id.clone();
    let progress_callback: compas_docs_core::backend::ProgressCallback =
        Box::new(move |event: IndexProgress| {
            let payload = IndexProgressDto::from_event(&folder_id_for_progress, event);
            if let Err(err) = app_for_progress.emit(INDEX_PROGRESS_EVENT, payload) {
                log::warn!("failed to emit index progress: {err}");
            }
        });

    let result = index_folder_with_progress(&path, config, file_types, Some(progress_callback))
        .await
        .map(FolderRecordDto::from)
        .map_err(|err| err.to_string());

    // Always send a terminal event so the UI can clear in-progress state even
    // when the job fails before any per-file progress is emitted.
    let terminal = IndexProgressDto::terminal(&folder_id, result.is_ok());
    if let Err(err) = app.emit(INDEX_PROGRESS_EVENT, terminal) {
        log::warn!("failed to emit terminal index progress: {err}");
    }

    result
}

#[tauri::command]
async fn pick_document_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    // Native folder picker dialog; resolves with the chosen absolute path or None
    // if the user cancels. Routed through a oneshot so the synchronous callback
    // API plays nicely with async commands.
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Arc::new(std::sync::Mutex::new(Some(tx)));

    app.dialog().file().pick_folder(move |selection| {
        if let Some(sender) = tx.lock().ok().and_then(|mut guard| guard.take()) {
            let _ = sender.send(selection);
        }
    });

    let selection = rx.await.map_err(|err| err.to_string())?;
    Ok(selection.and_then(|file_path| {
        file_path
            .into_path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
    }))
}

#[tauri::command]
fn remove_document_folder(id: String) -> Result<RemoveFolderResponse, String> {
    let removed = remove_folder(&id).map_err(|err| err.to_string())?;
    Ok(RemoveFolderResponse { removed })
}

#[tauri::command]
fn get_document_library_stats() -> Result<LibraryStatsDto, String> {
    library_stats()
        .map(LibraryStatsDto::from)
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn search_document_library(
    query: String,
    folder_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<SearchDocumentItemDto>, String> {
    let config = default_document_config(PathBuf::from(".").as_path());
    search_documents(&query, folder_id.as_deref(), limit.unwrap_or(10), config)
        .await
        .map(|items| items.into_iter().map(SearchDocumentItemDto::from).collect())
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn open_document_path(path: String) -> Result<(), String> {
    open_document(PathBuf::from(path).as_path()).map_err(|err| err.to_string())
}

#[tauri::command]
fn reveal_document_path(path: String) -> Result<(), String> {
    reveal_in_finder(PathBuf::from(path).as_path()).map_err(|err| err.to_string())
}

#[tauri::command]
fn navigate_main_window(app: tauri::AppHandle, view: AppView) -> Result<(), String> {
    navigate_main_window_to_view(&app, view).map_err(|err| err.to_string())
}

fn navigate_main_window_to_view<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    view: AppView,
) -> tauri::Result<()> {
    let window = app
        .get_webview_window(WINDOW_MAIN)
        .ok_or_else(|| tauri::Error::WindowNotFound)?;

    window.unminimize()?;
    window.show()?;
    window.set_focus()?;
    app.emit_to(window.label(), NAVIGATE_EVENT, view.as_str())?;

    Ok(())
}

fn build_app_menu<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> tauri::Result<tauri::menu::Menu<R>> {
    let app_menu = SubmenuBuilder::new(app, "Compas Docs")
        .text(MENU_OPEN_SEARCH, "Open Search")
        .text(MENU_OPEN_LIBRARY, "Open Library")
        .text(MENU_OPEN_STATS, "Open Stats")
        .separator()
        .about(None)
        .separator()
        .quit()
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .text(MENU_OPEN_SEARCH, "Search")
        .text(MENU_OPEN_LIBRARY, "Library")
        .text(MENU_OPEN_STATS, "Stats")
        .separator()
        .close_window()
        .build()?;

    MenuBuilder::new(app)
        .item(&app_menu)
        .item(&edit_menu)
        .item(&window_menu)
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            let menu = build_app_menu(&app.handle())?;
            app.set_menu(menu)?;
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().0.as_str() {
            MENU_OPEN_SEARCH => {
                if let Err(err) = navigate_main_window_to_view(app, AppView::Main) {
                    log::error!("failed to open search view: {err}");
                }
            }
            MENU_OPEN_LIBRARY => {
                if let Err(err) = navigate_main_window_to_view(app, AppView::Library) {
                    log::error!("failed to open library view: {err}");
                }
            }
            MENU_OPEN_STATS => {
                if let Err(err) = navigate_main_window_to_view(app, AppView::Stats) {
                    log::error!("failed to open stats view: {err}");
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            list_document_folders,
            add_document_folder,
            index_document_folder,
            remove_document_folder,
            get_document_library_stats,
            search_document_library,
            open_document_path,
            reveal_document_path,
            navigate_main_window,
            pick_document_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
