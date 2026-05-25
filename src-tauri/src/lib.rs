use compas::docs_backend::{
    add_folder, default_document_config, index_folder, list_folders, open_document, remove_folder,
    reveal_in_finder, search_documents, FolderRecord, SearchDocumentItem,
};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderRecordDto {
    id: String,
    path: String,
    display_name: String,
    storage_path: String,
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

#[tauri::command]
fn list_document_folders() -> Result<Vec<FolderRecordDto>, String> {
    Ok(list_folders()
        .into_iter()
        .map(FolderRecordDto::from)
        .collect())
}

#[tauri::command]
fn add_document_folder(path: String) -> Result<FolderRecordDto, String> {
    add_folder(PathBuf::from(path).as_path())
        .map(FolderRecordDto::from)
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn index_document_folder(path: String) -> Result<FolderRecordDto, String> {
    let path = PathBuf::from(path);
    let config = default_document_config(&path);
    index_folder(&path, config)
        .await
        .map(FolderRecordDto::from)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn remove_document_folder(id: String) -> Result<RemoveFolderResponse, String> {
    let removed = remove_folder(&id).map_err(|err| err.to_string())?;
    Ok(RemoveFolderResponse { removed })
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_document_folders,
            add_document_folder,
            index_document_folder,
            remove_document_folder,
            search_document_library,
            open_document_path,
            reveal_document_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
