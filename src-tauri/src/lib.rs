mod git;
mod works;

use std::path::PathBuf;

use works::{TreeNode, WorkSummary};

#[tauri::command]
fn get_works_root() -> Result<String, String> {
    works::load_config().map(|c| c.works_root)
}

#[tauri::command]
fn list_works() -> Result<Vec<WorkSummary>, String> {
    works::list_works()
}

#[tauri::command]
fn create_work(name: String, base: Option<String>) -> Result<WorkSummary, String> {
    works::create_work(&name, base)
}

#[tauri::command]
fn read_tree(work_path: String) -> Result<TreeNode, String> {
    works::read_tree(&PathBuf::from(work_path))
}

#[tauri::command]
fn read_file(work_path: String, rel_path: String) -> Result<String, String> {
    works::read_file(&PathBuf::from(work_path), &rel_path)
}

#[tauri::command]
fn write_file(work_path: String, rel_path: String, content: String) -> Result<(), String> {
    works::write_file(&PathBuf::from(work_path), &rel_path, &content)
}

#[tauri::command]
fn create_file(work_path: String, rel_path: String, content: String) -> Result<(), String> {
    works::create_file(&PathBuf::from(work_path), &rel_path, &content)
}

#[tauri::command]
fn delete_file(work_path: String, rel_path: String) -> Result<(), String> {
    works::delete_file(&PathBuf::from(work_path), &rel_path)
}

#[tauri::command]
fn restore_file(work_path: String, rel_path: String) -> Result<(), String> {
    works::restore_file(&PathBuf::from(work_path), &rel_path)
}

#[tauri::command]
fn git_log(work_path: String, limit: Option<usize>) -> Result<Vec<String>, String> {
    git::log_oneline(&PathBuf::from(work_path), limit.unwrap_or(20))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_works_root,
            list_works,
            create_work,
            read_tree,
            read_file,
            write_file,
            create_file,
            delete_file,
            restore_file,
            git_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
