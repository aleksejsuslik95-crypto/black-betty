use std::fs;
use std::path::PathBuf;

pub struct PathFinder;

impl PathFinder {
    /// Поиск директории `projects` относительно бинарника
    pub fn get_projects_root() -> PathBuf {
        let mut root = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        root.pop();

        let mut in_target = false;
        let check_path = root.clone();
        for component in check_path.components() {
            if component.as_os_str() == "target" {
                in_target = true;
                break;
            }
        }

        if in_target {
            while !root.ends_with("target") {
                if !root.pop() {
                    break;
                }
            }
            root.pop();
        }

        root.join("projects")
    }

    /// Поиск директории с тулзами (`tools/`)
    pub fn get_tools_root() -> PathBuf {
        let mut root = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        root.pop();

        let mut in_target = false;
        let check_path = root.clone();
        for component in check_path.components() {
            if component.as_os_str() == "target" {
                in_target = true;
                break;
            }
        }

        if in_target {
            while !root.ends_with("target") {
                if !root.pop() {
                    break;
                }
            }
            root.pop();
        }

        root.join("tools")
    }

    pub fn scan_projects() -> Vec<String> {
        let projects_root = Self::get_projects_root();
        let mut folders = Vec::new();
        if !projects_root.exists() {
            let _ = fs::create_dir_all(&projects_root);
        }
        if let Ok(entries) = fs::read_dir(projects_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy().to_string();
                        if !name_str.starts_with('.') {
                            folders.push(name_str);
                        }
                    }
                }
            }
        }
        folders.sort();
        folders
    }
}
