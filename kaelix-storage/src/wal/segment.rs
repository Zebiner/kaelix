use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Instant, SystemTime};

use memmap2::{MmapMut, MmapOptions};
use tokio::sync::Mutex;

use super::{LogEntry, WalError, SyncPolicy};

/// Metadata for a WAL segment
#[derive(Debug, Clone)]
pub(crate) struct SegmentMetadata {
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
    
    /// Check if segment is empty
    pub fn is_empty(&self) -> bool {
        self.entry_count == 0
    }
    
    /// Get utilization ratio (0.0 to 1.0)
    pub fn utilization(&self) -> f64 {
        if self.capacity == 0 {
            0.0
        } else {
            self.size as f64 / self.capacity as f64
        }
    }
}

/// A segment of the Write-Ahead Log
#[derive(Debug)]
pub(crate) struct WalSegment {
    /// Segment metadata
    metadata: Arc<RwLock<SegmentMetadata>>,
    
    /// File handle for the segment
    file: File,
    
    /// Optional memory map for efficient reading
    mmap: Option<MmapMut>,
    
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
            Some(
                #[allow(unsafe_code)]
                unsafe { MmapOptions::new().map_mut(&file) }
                    .map_err(|e| WalError::Io(format!("Failed to create memory map: {}", e)))?
            )
        } else {
            None
        };
        
        let metadata = Arc::new(RwLock::new(SegmentMetadata::new(id, path, capacity)));
        
        Ok(Self {
            metadata,
            file,
            mmap,
            write_position: Arc::new(Mutex::new(0)),
        })
    }
    
    /// Open existing segment
    pub fn open(path: impl AsRef<Path>, use_mmap: bool) -> Result<Self, WalError> {
        let path = path.as_ref().to_path_buf();
        
        // Extract segment ID from filename
        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| WalError::InvalidPath(format!("Invalid segment path: {:?}", path)))?;
        
        let id = filename.split('-')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| WalError::InvalidPath(format!("Cannot parse segment ID from: {}", filename)))?;
        
        // Open file
        let mut file = OpenOptions::new()
            .write(true)
            .read(true)
            .open(&path)
            .map_err(|e| WalError::Io(format!("Failed to open segment file: {}", e)))?;
        
        // Get file size
        let file_size = file.metadata()
            .map_err(|e| WalError::Io(format!("Failed to get file metadata: {}", e)))?
            .len();
        
        // Create memory map if requested
        let mmap = if use_mmap && file_size > 0 {
            Some(
                #[allow(unsafe_code)]
                unsafe { MmapOptions::new().map_mut(&file) }
                    .map_err(|e| WalError::Io(format!("Failed to create memory map: {}", e)))?
            )
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
            file,
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
                    if first_sequence.is_none() {
                        first_sequence = Some(sequence);
                    }
                    last_sequence = Some(sequence);
                    entry_count += 1;
                    position += 24 + payload_len;
                }
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    // End of file reached
                    break;
                }
                Err(e) => {
                    return Err(WalError::Io(format!("Failed to read segment: {}", e)));
                }
            }
        }
        
        Ok((entry_count, position, first_sequence, last_sequence))
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
    pub async fn write_entry(&self, entry: &LogEntry) -> Result<u64, WalError> {
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
            let start = position as usize;
            let end = start + serialized.len();
            
            if end <= mmap.len() {
                #[allow(unsafe_code)]
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        serialized.as_ptr(),
                        mmap.as_mut_ptr().add(start),
                        serialized.len(),
                    );
                }
            } else {
                return Err(WalError::SegmentFull(self.metadata.read().unwrap().id));
            }
        } else {
            self.file.write_all(&serialized)
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
    pub async fn read_entry(&self, position: u64) -> Result<LogEntry, WalError> {
        if let Some(ref mmap) = self.mmap {
            // Read from memory map
            let start = position as usize;
            if start >= mmap.len() {
                return Err(WalError::Io("Position beyond segment bounds".to_string()));
            }
            
            // First read header
            if start + 24 > mmap.len() {
                return Err(WalError::Io("Insufficient data for header".to_string()));
            }
            
            let header_slice = &mmap[start..start + 24];
            let entry = LogEntry::deserialize_header(header_slice)?;
            
            // Then read full entry
            let total_size = 24 + entry.payload_length as usize;
            if start + total_size > mmap.len() {
                return Err(WalError::Io("Insufficient data for full entry".to_string()));
            }
            
            let entry_slice = &mmap[start..start + total_size];
            LogEntry::deserialize(entry_slice)
        } else {
            // Read from file
            let mut file = &self.file;
            file.seek(SeekFrom::Start(position))
                .map_err(|e| WalError::Io(format!("Failed to seek to position: {}", e)))?;
            
            // Read header first
            let mut header_buf = [0u8; 24];
            file.read_exact(&mut header_buf)
                .map_err(|e| WalError::Io(format!("Failed to read header: {}", e)))?;
            
            let entry = LogEntry::deserialize_header(&header_buf)?;
            
            // Read payload
            let mut payload = vec![0u8; entry.payload_length as usize];
            file.read_exact(&mut payload)
                .map_err(|e| WalError::Io(format!("Failed to read payload: {}", e)))?;
            
            // Combine and deserialize
            let mut full_data = header_buf.to_vec();
            full_data.extend_from_slice(&payload);
            
            LogEntry::deserialize(&full_data)
        }
    }
    
    /// Sync segment to disk
    /// 
    /// Returns [`WalError`] if sync fails.
    pub async fn sync(&self) -> Result<(), WalError> {
        // Sync memory-mapped region
        if let Some(ref mmap) = self.mmap {
            mmap.flush().map_err(|e| WalError::Io(e.to_string()))?;
        }

        // Sync file
        self.file.sync_all().map_err(|e| WalError::Io(e.to_string()))?;

        tracing::debug!("Synced segment {}", self.metadata.read().unwrap().id);
        Ok(())
    }
    
    /// Check if segment needs synchronization based on policy
    pub fn needs_sync(&self, policy: &SyncPolicy, last_sync: Option<Instant>) -> bool {
        match policy {
            SyncPolicy::Never => false,
            SyncPolicy::Always => true,
            SyncPolicy::OnBatch => false, // Handled by batch coordinator
            SyncPolicy::Interval(duration) => {
                match last_sync {
                    Some(last) => last.elapsed() >= *duration,
                    None => true,
                }
            }
            SyncPolicy::OnShutdown => false, // Handled during shutdown
        }
    }
    
    /// Seal segment (make it read-only)
    pub async fn seal(&self) -> Result<(), WalError> {
        // Sync before sealing
        self.sync().await?;
        
        // Mark as sealed
        {
            let mut metadata = self.metadata.write().unwrap();
            metadata.sealed = true;
            metadata.last_sync = Some(SystemTime::now());
        }
        
        tracing::info!("Sealed segment {}", self.metadata.read().unwrap().id);
        Ok(())
    }
    
    /// Get current write position
    pub async fn write_position(&self) -> u64 {
        *self.write_position.lock().await
    }
    
    /// Get remaining capacity
    pub async fn remaining_capacity(&self) -> u64 {
        let current_pos = *self.write_position.lock().await;
        let metadata = self.metadata.read().unwrap();
        metadata.capacity.saturating_sub(current_pos)
    }
}

/// Writer for batch operations on a segment
#[derive(Debug)]
pub(crate) struct SegmentWriter {
    segment: WalSegment,
    last_sync: Option<Instant>,
}

impl SegmentWriter {
    /// Create new segment writer
    pub fn new(segment: WalSegment) -> Self {
        Self {
            segment,
            last_sync: None,
        }
    }
    
    /// Write batch of entries
    pub async fn write_batch(&mut self, entries: &[LogEntry]) -> Result<Vec<u64>, WalError> {
        let mut positions = Vec::with_capacity(entries.len());
        
        for entry in entries {
            let position = self.segment.write_entry(entry).await?;
            positions.push(position);
        }
        
        Ok(positions)
    }
    
    /// Sync segment if needed based on policy
    pub async fn sync_if_needed(&mut self, policy: &SyncPolicy) -> Result<(), WalError> {
        if self.segment.needs_sync(policy, self.last_sync) {
            self.segment.sync().await?;
            self.last_sync = Some(Instant::now());
        }
        Ok(())
    }
    
    /// Force sync segment
    pub async fn sync(&mut self) -> Result<(), WalError> {
        self.segment.sync().await?;
        self.last_sync = Some(Instant::now());
        Ok(())
    }
    
    /// Check if segment can accept batch
    pub async fn can_accept_batch(&self, entries: &[LogEntry]) -> bool {
        let total_size: usize = entries.iter()
            .map(|e| e.serialized_size())
            .sum();
        
        self.segment.can_accept(total_size).await
    }
    
    /// Get segment metadata
    pub fn metadata(&self) -> Arc<RwLock<SegmentMetadata>> {
        self.segment.metadata()
    }
    
    /// Seal the underlying segment
    pub async fn seal(&mut self) -> Result<(), WalError> {
        self.segment.seal().await
    }
}