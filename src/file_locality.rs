//! File locality detection and availability status

use std::path::PathBuf;

// File locality status tracking
#[derive(Debug, Clone, PartialEq)]
pub enum FileLocalityStatus {
    /// File is immediately available locally
    Local,
    /// File is on-demand and will trigger download when accessed
    OnDemand,
    /// Cannot determine status
    Unknown,
}

impl FileLocalityStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            FileLocalityStatus::Local => "💾",
            FileLocalityStatus::OnDemand => "☁️",
            FileLocalityStatus::Unknown => "❓",
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            FileLocalityStatus::Local => "device-floppy",
            FileLocalityStatus::OnDemand => "cloud",
            FileLocalityStatus::Unknown => "help",
        }
    }
    
    pub fn description(&self) -> &'static str {
        match self {
            FileLocalityStatus::Local => "Local file (immediately available)",
            FileLocalityStatus::OnDemand => "On-demand file (will download when accessed)",
            FileLocalityStatus::Unknown => "Unknown availability status",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub locality_status: FileLocalityStatus,
    pub estimated_download_size: Option<u64>, // Size in bytes if it needs to be downloaded
}

impl FileInfo {
    pub fn new(path: PathBuf) -> Self {
        let locality_status = get_file_locality_status(&path);
        let estimated_download_size = if matches!(locality_status, FileLocalityStatus::OnDemand) {
            // Get the reported file size (which is the full file size for on-demand files)
            std::fs::metadata(&path).ok().map(|m| m.len())
        } else {
            None
        };
        
        Self {
            path,
            locality_status,
            estimated_download_size,
        }
    }
    
    pub fn will_trigger_download(&self) -> bool {
        matches!(self.locality_status, FileLocalityStatus::OnDemand)
    }
}

// Platform-specific file locality detection
#[cfg(windows)]
pub fn get_file_locality_status(path: &std::path::Path) -> FileLocalityStatus {
    use std::os::windows::fs::MetadataExt;
    
    // Check file attributes to determine locality
    if let Ok(metadata) = std::fs::metadata(path) {
        let attributes = metadata.file_attributes();
        
        // Key Windows file attributes for determining locality
        const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x00400000;
        const FILE_ATTRIBUTE_UNPINNED: u32 = 0x00100000;
        
        // Debug output for troubleshooting
        #[cfg(debug_assertions)]
        println!("File locality check: {} - attributes: 0x{:08X}", path.display(), attributes);
        
        // Based on the provided data patterns:
        // On-demand files have both UNPINNED and RECALL_ON_DATA_ACCESS attributes
        let is_unpinned = (attributes & FILE_ATTRIBUTE_UNPINNED) != 0;
        let has_recall_on_data_access = (attributes & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) != 0;
        
        if is_unpinned && has_recall_on_data_access {
            #[cfg(debug_assertions)]
            println!("  -> OnDemand (unpinned + recall on data access)");
            return FileLocalityStatus::OnDemand;
        }
        
        // Local files have neither UNPINNED nor RECALL_ON_DATA_ACCESS
        if !is_unpinned && !has_recall_on_data_access {
            #[cfg(debug_assertions)]
            println!("  -> Local (not unpinned, no recall on data access)");
            return FileLocalityStatus::Local;
        }
        
        // Handle edge cases
        #[cfg(debug_assertions)]
        println!("  -> Unknown (unusual attribute combination: unpinned={}, recall_on_data_access={})", 
                 is_unpinned, has_recall_on_data_access);
        return FileLocalityStatus::Unknown;
    }
    
    // Default to unknown if we can't determine status
    #[cfg(debug_assertions)]
    println!("File locality check: {} - couldn't read metadata, status unknown", path.display());
    FileLocalityStatus::Unknown
}

#[cfg(not(windows))]
pub fn get_file_locality_status(_path: &std::path::Path) -> FileLocalityStatus {
    // On non-Windows platforms, assume all files are local
    FileLocalityStatus::Local
}

/// Check if a file is immediately available without triggering a download
pub fn is_file_immediately_available(path: &std::path::Path) -> bool {
    matches!(get_file_locality_status(path), FileLocalityStatus::Local)
}

/// Check if accessing a file will trigger a download
pub fn will_file_access_trigger_download(path: &std::path::Path) -> bool {
    matches!(get_file_locality_status(path), FileLocalityStatus::OnDemand)
}

/// Get a human-readable status string for a file
pub fn get_file_status_string(path: &std::path::Path) -> String {
    let status = get_file_locality_status(path);
    format!("{} {}", status.icon(), status.description())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_file_locality_status_display() {
        let local = FileLocalityStatus::Local;
        assert_eq!(local.icon(), "💾");
        assert_eq!(local.description(), "Local file (immediately available)");
        
        let on_demand = FileLocalityStatus::OnDemand;
        assert_eq!(on_demand.icon(), "☁️");
        assert_eq!(on_demand.description(), "On-demand file (will download when accessed)");
        
        let unknown = FileLocalityStatus::Unknown;
        assert_eq!(unknown.icon(), "❓");
        assert_eq!(unknown.description(), "Unknown availability status");
    }

    #[test]
    fn test_file_info_creation() {
        let path = PathBuf::from("test_file.jpg");
        let info = FileInfo::new(path.clone());
        assert_eq!(info.path, path);
        // Status will depend on actual file attributes, so we just check it's set
        assert!(matches!(info.locality_status, FileLocalityStatus::Local | FileLocalityStatus::OnDemand | FileLocalityStatus::Unknown));
    }
}