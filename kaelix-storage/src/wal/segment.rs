use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use memmap2::{MmapMut, MmapOptions};
use tokio::sync::Mutex;

use super::{LogEntry, SyncPolicy, WalError};

/// Metadata for a WAL segment
#[derive(Debug, Clone)]
pub struct SegmentMetadata {
    /// Unique segment identifier
    pub id: u64,

    /// File path
    pub path: PathBuf,

    /// Current size in bytes
    pub size: u64,

    /// Maximum capacity
    pub capacity: u64,

    /// Number of entries
    pub entry_count: u64,

    /// First entry sequence number
    pub first_sequence: Option<u64>,

    /// Last entry sequence number
    pub last_sequence: Option<u64>,

    /// Creation timestamp
    pub created_at: SystemTime,

    /// Last sync timestamp
    pub last_sync: Option<SystemTime>,

    /// Whether segment is sealed (read-only)
    pub sealed: bool,
}

impl SegmentMetadata {
    /// Create new segment metadata
    pub fn new(id: u64, path: PathBuf, capacity: u64) -> Self {
        Self {
            id,
            path,
            size: 0,
            capacity,
            entry_count: 0,
            first_sequence: None,
            last_sequence: None,
            created_at: SystemTime::now(),
            last_sync: None,
            sealed: false,
        }
    }

    /// Check if segment is full
    pub fn is_full(&self) -> bool {
        self.size >= self.capacity
    }

    /// Get available space
    pub fn available_space(&self) -> u64 {
        self.capacity.saturating_sub(self.size)
    }

    /// Update sync time
    pub fn mark_synced(&mut self) {
        self.last_sync = Some(SystemTime::now());
    }
}

/// A segment of the Write-Ahead Log
#[derive(Debug, Clone)]
pub struct WalSegment {
    /// Segment metadata
    metadata: Arc<RwLock<SegmentMetadata>>,

    /// File handle for the segment (wrapped in Mutex for thread safety)
    file: Arc<Mutex<File>>,

    /// Optional memory map for efficient reading
    mmap: Option<Arc<Mutex<MmapMut>>>,

    /// Current write position
    write_position: Arc<Mutex<u64>>,
}

impl WalSegment {
    /// Create new segment
    pub fn create(
        id: u64,
        path: impl AsRef<Path>,
        capacity: u64,
        use_mmap: bool,
    ) -> Result<Self, WalError> {
        let path = path.as_ref().to_path_buf();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| WalError::Io(format!("Failed to create directory: {}", e)))?;
        }

        // Create and pre-allocate file
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .truncate(true)
            .open(&path)
            .map_err(|e| WalError::Io(format!("Failed to create segment file: {}", e)))?;

        // Pre-allocate space for performance
        file.set_len(capacity)
            .map_err(|e| WalError::Io(format!("Failed to pre-allocate segment: {}", e)))?;

        // Create memory map if requested
        let mmap = if use_mmap && capacity > 0 {
            Some(Arc::new(Mutex::new(
                #[allow(unsafe_code)]
                unsafe { MmapOptions::new().map_mut(&file) }
                    .map_err(|e| WalError::Io(format!("Failed to create memory map: {}", e)))?,
            )))
        } else {
            None
        };

        let metadata = Arc::new(RwLock::new(SegmentMetadata::new(id, path, capacity)));

        Ok(Self {
            metadata,
            file: Arc::new(Mutex::new(file)),
            mmap,
            write_position: Arc::new(Mutex::new(0)),
        })
    }

    /// Open existing segment
    pub fn open(path: impl AsRef<Path>, use_mmap: bool) -> Result<Self, WalError> {
        let path = path.as_ref().to_path_buf();

        // Extract segment ID from filename
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| WalError::InvalidPath(format!("Invalid segment path: {:?}", path)))?;

        let id = filename.split('-').nth(1).and_then(|s| s.parse().ok()).ok_or_else(|| {
            WalError::InvalidPath(format!("Cannot parse segment ID from: {}", filename))
        })?;

        // Open file
        let mut file = OpenOptions::new()
            .write(true)
            .read(true)
            .open(&path)
            .map_err(|e| WalError::Io(format!("Failed to open segment file: {}", e)))?;

        // Get file size
        let file_size = file
            .metadata()
            .map_err(|e| WalError::Io(format!("Failed to get file metadata: {}", e)))?
            .len();

        // Create memory map if requested
        let mmap = if use_mmap && file_size > 0 {
            Some(Arc::new(Mutex::new(
                #[allow(unsafe_code)]
                unsafe { MmapOptions::new().map_mut(&file) }
                    .map_err(|e| WalError::Io(format!("Failed to create memory map: {}", e)))?,
            )))
        } else {
            None
        };

        // Scan file to determine current state
        let (entry_count, write_pos, first_seq, last_seq) = Self::scan_segment(&mut file)?;

        let mut metadata = SegmentMetadata::new(id, path, file_size);
        metadata.size = write_pos;
        metadata.entry_count = entry_count;
        metadata.first_sequence = first_seq;
        metadata.last_sequence = last_seq;

        Ok(Self {
            metadata: Arc::new(RwLock::new(metadata)),
            file: Arc::new(Mutex::new(file)),
            mmap,
            write_position: Arc::new(Mutex::new(write_pos)),
        })
    }

    /// Scan segment file to determine current state
    fn scan_segment(file: &mut File) -> Result<(u64, u64, Option<u64>, Option<u64>), WalError> {
        let mut entry_count = 0;
        let mut position = 0;
        let mut first_sequence = None;
        let mut last_sequence = None;

        file.seek(SeekFrom::Start(0))
            .map_err(|e| WalError::Io(format!("Failed to seek to start: {}", e)))?;

        loop {
            // Try to read entry header
            let mut header_buf = [0u8; 24]; // LogEntry header size
            match file.read_exact(&mut header_buf) {
                Ok(_) => {
                    // Parse entry header to get length and sequence
                    let entry = LogEntry::deserialize_header(&header_buf)
                        .map_err(|_| WalError::Io("Failed to parse entry header".to_string()))?;

                    let sequence = entry.sequence_number;
                    let payload_len = entry.payload_length as u64;

                    // Skip payload
                    file.seek(SeekFrom::Current(payload_len as i64))
                        .map_err(|e| WalError::Io(format!("Failed to seek past payload: {}", e)))?;

                    // Update state
                    entry_count += 1;
                    position += 24 + payload_len;

                    if first_sequence.is_none() {
                        first_sequence = Some(sequence);
                    }
                    last_sequence = Some(sequence);
                },
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    // Reached end of valid entries
                    break;
                },
                Err(e) => {
                    return Err(WalError::Io(format!("Failed to read entry header: {}", e)));
                },
            }
        }

        Ok((entry_count, position, first_sequence, last_sequence))
    }

    /// Get segment ID
    pub fn id(&self) -> u64 {
        self.metadata.read().unwrap().id
    }

    /// Get segment size
    pub fn size(&self) -> u64 {
        self.metadata.read().unwrap().size
    }

    /// Get entry count
    pub fn entry_count(&self) -> u64 {
        self.metadata.read().unwrap().entry_count
    }

    /// Check if sealed
    pub fn is_sealed(&self) -> bool {
        self.metadata.read().unwrap().sealed
    }

    /// Get segment metadata
    pub fn metadata(&self) -> Arc<RwLock<SegmentMetadata>> {
        Arc::clone(&self.metadata)
    }

    /// Check if segment can accept new entries
    pub async fn can_accept(&self, entry_size: usize) -> bool {
        let current_pos = *self.write_position.lock().await;
        let metadata = self.metadata.read().unwrap();

        !metadata.sealed && (current_pos + entry_size as u64) <= metadata.capacity
    }

    /// Write entry to segment
    pub(crate) async fn write_entry(&self, entry: &LogEntry) -> Result<u64, WalError> {
        let mut write_pos = self.write_position.lock().await;
        let serialized = entry.serialize()?;

        // Check capacity
        {
            let metadata = self.metadata.read().unwrap();
            if metadata.sealed {
                return Err(WalError::SegmentSealed(metadata.id));
            }

            if *write_pos + serialized.len() as u64 > metadata.capacity {
                return Err(WalError::SegmentFull(metadata.id));
            }
        }

        let position = *write_pos;

        // Write using memory map if available, otherwise use file
        if let Some(ref mmap) = self.mmap {
            let mut mmap_guard = mmap.lock().await;
            let start = position as usize;
            let end = start + serialized.len();

            if end <= mmap_guard.len() {
                #[allow(unsafe_code)]
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        serialized.as_ptr(),
                        mmap_guard.as_mut_ptr().add(start),
                        serialized.len(),
                    );
                }
            } else {
                return Err(WalError::SegmentFull(self.metadata.read().unwrap().id));
            }
        } else {
            let mut file = self.file.lock().await;
            file.seek(SeekFrom::Start(position))
                .map_err(|e| WalError::Io(format!("Failed to seek: {}", e)))?;
            file.write_all(&serialized)
                .map_err(|e| WalError::Io(format!("Failed to write entry: {}", e)))?;
        }

        // Update position and metadata
        *write_pos += serialized.len() as u64;

        {
            let mut metadata = self.metadata.write().unwrap();
            metadata.size = *write_pos;
            metadata.entry_count += 1;

            if metadata.first_sequence.is_none() {
                metadata.first_sequence = Some(entry.sequence_number);
            }
            metadata.last_sequence = Some(entry.sequence_number);
        }

        Ok(position)
    }

    /// Read entry at position
    pub(crate) async fn read_entry(&self, position: u64) -> Result<LogEntry, WalError> {
        if let Some(ref mmap) = self.mmap {
            // Read from memory map
            let mmap_guard = mmap.lock().await;
            let start = position as usize;
            if start >= mmap_guard.len() {
                return Err(WalError::Io("Position beyond segment bounds".to_string()));
            }

            // First read header
            if start + 24 > mmap_guard.len() {
                return Err(WalError::Io("Insufficient data for header".to_string()));
            }

            let header_slice = &mmap_guard[start..start + 24];
            let entry = LogEntry::deserialize_header(header_slice)?;

            // Then read full entry
            let total_size = 24 + entry.payload_length as usize;
            if start + total_size > mmap_guard.len() {
                return Err(WalError::Io("Insufficient data for full entry".to_string()));
            }

            let entry_slice = &mmap_guard[start..start + total_size];
            LogEntry::deserialize(entry_slice)
        } else {
            // Read from file
            let mut file = self.file.lock().await;
            file.seek(SeekFrom::Start(position))
                .map_err(|e| WalError::Io(format!("Failed to seek: {}", e)))?;

            // Read header first
            let mut header_buf = [0u8; 24];
            file.read_exact(&mut header_buf)
                .map_err(|e| WalError::Io(format!("Failed to read header: {}", e)))?;

            let header = LogEntry::deserialize_header(&header_buf)?;
            let payload_len = header.payload_length as usize;

            // Read payload
            let mut payload_buf = vec![0u8; payload_len];
            file.read_exact(&mut payload_buf)
                .map_err(|e| WalError::Io(format!("Failed to read payload: {}", e)))?;

            // Combine header and payload
            let mut entry_buf = Vec::with_capacity(24 + payload_len);
            entry_buf.extend_from_slice(&header_buf);
            entry_buf.extend_from_slice(&payload_buf);

            LogEntry::deserialize(&entry_buf)
        }
    }

    /// Sync segment to disk
    pub(crate) async fn sync(&self, policy: SyncPolicy) -> Result<(), WalError> {
        let mut file = self.file.lock().await;

        match policy {
            SyncPolicy::Always | SyncPolicy::OnBatch => {
                file.flush().map_err(|e| WalError::Io(format!("Failed to flush: {}", e)))?;
                file.sync_data().map_err(|e| WalError::Io(format!("Failed to sync: {}", e)))?;
            },
            SyncPolicy::Never => {
                // No sync required
            },
            SyncPolicy::Interval(_) => {
                // For interval sync, we perform the sync now since we're being called
                // In a full implementation, interval syncing would be handled by a timer
                file.flush().map_err(|e| WalError::Io(format!("Failed to flush: {}", e)))?;
                file.sync_data().map_err(|e| WalError::Io(format!("Failed to sync: {}", e)))?;
            },
            SyncPolicy::OnShutdown => {
                // OnShutdown sync - only sync if we're actually shutting down
                // For now, we'll perform the sync since we're being called
                file.flush().map_err(|e| WalError::Io(format!("Failed to flush: {}", e)))?;
                file.sync_data().map_err(|e| WalError::Io(format!("Failed to sync: {}", e)))?;
            },
        }

        // Update sync timestamp
        self.metadata.write().unwrap().mark_synced();

        Ok(())
    }

    /// Seal segment (make it read-only)
    pub(crate) async fn seal(&self) -> Result<(), WalError> {
        // Sync all data first
        self.sync(SyncPolicy::Always).await?;

        // Mark as sealed
        self.metadata.write().unwrap().sealed = true;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_segment_create_and_write() {
        let temp_dir = TempDir::new().unwrap();
        let segment_path = temp_dir.path().join("segment-1.log");

        let segment = WalSegment::create(1, &segment_path, 1024 * 1024, false).unwrap();

        // Create a test entry
        let entry =
            LogEntry::new(1, 12345, kaelix_cluster::messages::ClusterMessage::default()).unwrap();

        let position = segment.write_entry(&entry).await.unwrap();
        assert_eq!(position, 0);

        // Read back the entry
        let read_entry = segment.read_entry(position).await.unwrap();
        assert_eq!(read_entry.sequence_number, entry.sequence_number);
    }

    #[tokio::test]
    async fn test_segment_seal() {
        let temp_dir = TempDir::new().unwrap();
        let segment_path = temp_dir.path().join("segment-2.log");

        let segment = WalSegment::create(2, &segment_path, 1024, false).unwrap();

        assert!(!segment.is_sealed());

        segment.seal().await.unwrap();

        assert!(segment.is_sealed());
    }
}