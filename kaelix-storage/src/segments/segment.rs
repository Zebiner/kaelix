//! Storage segment implementation for segment-based storage
//!
//! Provides efficient segment management with compression, atomic operations,
//! integrity verification, memory-mapped access, and ultra-high performance
//! operations targeting <10μs latency.

use super::entry::StorageEntry;
use super::segment_writer::{SegmentWriter, SegmentWriterConfig};
use crate::{StorageError, Result};
use dashmap::DashMap;
use memmap2::{Mmap, MmapOptions};
use parking_lot::{Mutex, RwLock};
use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::SystemTime;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

/// Index entry for fast sequence-based lookups
///
/// Maps sequence numbers to their file offsets for O(log n) lookup performance.
#[derive(Debug, Clone, Copy)]
pub struct IndexEntry {
    /// Sequence number for this entry
    pub sequence: u64,
    /// File offset where the entry is stored
    pub offset: u64,
    /// Size of the entry in bytes
    pub size: u32,
    /// Timestamp when entry was written
    pub timestamp: u64,
}

/// Memory-mapped segment reader for high-performance access
///
/// Provides zero-copy reads using memory mapping with automatic fallback
/// to file-based reads for safety.
#[derive(Debug)]
pub struct SegmentReader {
    /// Memory-mapped file for zero-copy access
    mmap: Option<Mmap>,
    /// File handle for fallback reading
    file: Arc<Mutex<File>>,
    /// File size for bounds checking
    file_size: u64,
}

impl SegmentReader {
    /// Create a new segment reader with memory mapping
    pub fn new(file: File) -> Result<Self> {
        let file_size = file
            .metadata()
            .map_err(|e| StorageError::Io(format!("Failed to get file metadata: {}", e)))?
            .len();

        // Try to create memory mapping for files larger than 4KB
        let mmap = if file_size > 4096 {
            // SAFETY: Memory mapping is used only for read access and the file lifetime
            // is managed by the File struct. We ensure proper bounds checking in read_at().
            #[allow(unsafe_code)]
            match unsafe { MmapOptions::new().map(&file) } {
                Ok(mmap) => Some(mmap),
                Err(e) => {
                    tracing::warn!(
                        file_size = file_size,
                        error = %e,
                        "Failed to create memory mapping, falling back to file reads"
                    );
                    None
                },
            }
        } else {
            None
        };

        Ok(Self { mmap, file: Arc::new(Mutex::new(file)), file_size })
    }

    /// Read data at a specific offset with automatic fallback
    pub fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        if offset + len as u64 > self.file_size {
            return Err(StorageError::Io(format!("Read beyond file bounds: offset {} + len {} > size {}", offset, len, self.file_size)));
        }

        if let Some(ref mmap) = self.mmap {
            // Zero-copy read using memory mapping
            let start = offset as usize;
            let end = start + len;
            if end <= mmap.len() {
                return Ok(mmap[start..end].to_vec());
            }
        }

        // Fallback to file-based read
        self.read_file_at(offset, len)
    }

    fn read_file_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        use std::io::{Read, Seek};

        let mut file = self.file.lock();
        file.seek(std::io::SeekFrom::Start(offset))
            .map_err(|e| StorageError::Io(format!("Failed to seek: {}", e)))?;

        let mut buffer = vec![0u8; len];
        file.read_exact(&mut buffer)
            .map_err(|e| StorageError::Io(format!("Failed to read: {}", e)))?;

        Ok(buffer)
    }

    /// Get the file size
    pub fn size(&self) -> u64 {
        self.file_size
    }
}

/// Segment recovery information
///
/// Contains metadata recovered during segment scanning and validation.
#[derive(Debug, Default, Clone)]
pub struct RecoveryInfo {
    /// Number of entries recovered
    pub entries_recovered: u64,
    /// Number of corrupted entries found
    pub corrupted_entries: u64,
    /// Bytes recovered successfully
    pub bytes_recovered: u64,
    /// Bytes lost to corruption
    pub bytes_corrupted: u64,
    /// Recovery duration in microseconds
    pub recovery_duration_us: u64,
}

/// A single storage segment containing messages with advanced capabilities
///
/// Represents a segment file with high-performance operations, memory-mapped access,
/// fast sequence-based lookups, and comprehensive recovery capabilities.
#[derive(Debug)]
pub struct StorageSegment {
    /// Unique segment identifier
    pub id: u64,
    /// Segment creation timestamp
    pub created_at: SystemTime,
    /// File path for this segment
    file_path: PathBuf,
    /// High-performance segment writer
    writer: Arc<RwLock<Option<SegmentWriter>>>,
    /// Memory-mapped reader for fast access
    reader: Arc<RwLock<Option<SegmentReader>>>,
    /// Sequence number to offset index for fast lookups
    sequence_index: Arc<DashMap<u64, IndexEntry>>,
    /// Sorted index for range queries (sequence -> IndexEntry)
    sorted_index: Arc<RwLock<BTreeMap<u64, IndexEntry>>>,
    /// Number of entries in this segment
    entry_count: AtomicU64,
    /// Current file size in bytes
    file_size: AtomicU64,
    /// First sequence number in this segment
    first_sequence: AtomicU64,
    /// Last sequence number in this segment
    last_sequence: AtomicU64,
    /// Whether this segment is sealed (read-only)
    is_sealed: AtomicBool,
    /// Whether compression is enabled for this segment
    enable_compression: bool,
    /// Writer configuration
    writer_config: SegmentWriterConfig,
    /// Recovery information
    recovery_info: Arc<RwLock<RecoveryInfo>>,
}

impl StorageSegment {
    /// Create a new storage segment with advanced capabilities
    ///
    /// # Parameters
    ///
    /// * `id` - Unique segment identifier
    /// * `dir` - Directory to create segment file in
    /// * `enable_compression` - Whether to enable LZ4 compression
    ///
    /// # Returns
    ///
    /// A new [`StorageSegment`] ready for high-performance writing.
    ///
    /// # Errors
    ///
    /// - [`StorageError::Io`] - Failed to create segment directory or file
    pub async fn create_new(id: u64, dir: &Path, enable_compression: bool) -> Result<Self> {
        // Ensure directory exists
        if let Err(e) = tokio::fs::create_dir_all(dir).await {
            return Err(StorageError::Io(format!("Failed to create directory {}: {}", dir.display(), e)));
        }

        let file_path = dir.join(format!("segment-{:016x}.data", id));

        // Create high-performance writer
        let writer_config = SegmentWriterConfig::default();
        let writer = SegmentWriter::new(id, dir, enable_compression, writer_config.clone()).await?;

        Ok(StorageSegment {
            id,
            created_at: SystemTime::now(),
            file_path,
            writer: Arc::new(RwLock::new(Some(writer))),
            reader: Arc::new(RwLock::new(None)),
            sequence_index: Arc::new(DashMap::new()),
            sorted_index: Arc::new(RwLock::new(BTreeMap::new())),
            entry_count: AtomicU64::new(0),
            file_size: AtomicU64::new(0),
            first_sequence: AtomicU64::new(u64::MAX),
            last_sequence: AtomicU64::new(0),
            is_sealed: AtomicBool::new(false),
            enable_compression,
            writer_config,
            recovery_info: Arc::new(RwLock::new(RecoveryInfo::default())),
        })
    }

    /// Open an existing storage segment with recovery
    ///
    /// Scans the segment file to rebuild metadata and index structures,
    /// validating data integrity and recovering from any corruption.
    ///
    /// # Parameters
    ///
    /// * `id` - Segment identifier
    /// * `dir` - Directory containing segment file
    /// * `enable_compression` - Whether compression is enabled
    ///
    /// # Returns
    ///
    /// An opened [`StorageSegment`] with recovered metadata and index.
    pub async fn open_existing(
        id: u64,
        dir: &Path,
        enable_compression: bool,
    ) -> Result<Self> {
        let file_path = dir.join(format!("segment-{:016x}.data", id));

        // Get file metadata
        let metadata =
            tokio::fs::metadata(&file_path)
                .await
                .map_err(|e| StorageError::Io(format!("Failed to get metadata for {}: {}", file_path.display(), e)))?;

        let file_size = metadata.len();
        let created_at = metadata.created().unwrap_or_else(|_| SystemTime::now());

        // Create segment instance
        let segment = StorageSegment {
            id,
            created_at,
            file_path: file_path.clone(),
            writer: Arc::new(RwLock::new(None)), // No writer for existing segments initially
            reader: Arc::new(RwLock::new(None)),
            sequence_index: Arc::new(DashMap::new()),
            sorted_index: Arc::new(RwLock::new(BTreeMap::new())),
            entry_count: AtomicU64::new(0),
            file_size: AtomicU64::new(file_size),
            first_sequence: AtomicU64::new(u64::MAX),
            last_sequence: AtomicU64::new(0),
            is_sealed: AtomicBool::new(true), // Existing segments start sealed
            enable_compression,
            writer_config: SegmentWriterConfig::default(),
            recovery_info: Arc::new(RwLock::new(RecoveryInfo::default())),
        };

        // Perform recovery scan if file has data
        if file_size > 0 {
            segment.recover_segment().await?;
        }

        // Create memory-mapped reader
        let file = std::fs::File::open(&file_path).map_err(|e| StorageError::Io(format!("Failed to open {}: {}", file_path.display(), e)))?;

        let reader = SegmentReader::new(file)?;
        *segment.reader.write() = Some(reader);

        Ok(segment)
    }

    /// Recover segment metadata by scanning the file
    ///
    /// Scans through the entire segment file, rebuilding the index and validating
    /// data integrity. Handles corruption gracefully and provides detailed recovery info.
    async fn recover_segment(&self) -> Result<()> {
        let start_time = std::time::Instant::now();
        let mut recovery_info = RecoveryInfo::default();

        let mut file = tokio::fs::File::open(&self.file_path).await.map_err(|e| {
            StorageError::Io(format!("Failed to open {} for recovery: {}", self.file_path.display(), e))
        })?;

        let mut current_offset = 0u64;
        let file_size = self.file_size.load(Ordering::Acquire);

        tracing::info!(segment_id = self.id, file_size = file_size, "Starting segment recovery");

        while current_offset < file_size {
            match self.read_entry_at_offset_during_recovery(&mut file, current_offset).await {
                Ok((entry, entry_size)) => {
                    // Successfully recovered entry
                    let index_entry = IndexEntry {
                        sequence: entry.sequence,
                        offset: current_offset,
                        size: entry_size as u32,
                        timestamp: entry.metadata.timestamp,
                    };

                    // Update indexes
                    self.sequence_index.insert(entry.sequence, index_entry);
                    self.sorted_index.write().insert(entry.sequence, index_entry);

                    // Update segment metadata
                    self.entry_count.fetch_add(1, Ordering::AcqRel);
                    self.first_sequence.fetch_min(entry.sequence, Ordering::AcqRel);
                    self.last_sequence.fetch_max(entry.sequence, Ordering::AcqRel);

                    // Update recovery stats
                    recovery_info.entries_recovered += 1;
                    recovery_info.bytes_recovered += entry_size as u64;

                    current_offset += entry_size as u64;
                },
                Err(e) => {
                    tracing::warn!(
                        segment_id = self.id,
                        offset = current_offset,
                        error = %e,
                        "Entry recovery failed, attempting to find next valid entry"
                    );

                    // Try to find next valid entry by scanning ahead
                    match self.find_next_valid_entry(&mut file, current_offset, file_size).await {
                        Ok(Some(next_offset)) => {
                            let corrupted_bytes = next_offset - current_offset;
                            recovery_info.corrupted_entries += 1;
                            recovery_info.bytes_corrupted += corrupted_bytes;
                            current_offset = next_offset;
                        },
                        Ok(None) => {
                            // No more valid entries found
                            let corrupted_bytes = file_size - current_offset;
                            recovery_info.bytes_corrupted += corrupted_bytes;
                            break;
                        },
                        Err(scan_error) => {
                            tracing::error!(
                                segment_id = self.id,
                                offset = current_offset,
                                error = %scan_error,
                                "Failed to scan for next valid entry"
                            );
                            return Err(scan_error);
                        },
                    }
                },
            }
        }

        recovery_info.recovery_duration_us = start_time.elapsed().as_micros() as u64;
        *self.recovery_info.write() = recovery_info.clone();

        tracing::info!(
            segment_id = self.id,
            entries_recovered = recovery_info.entries_recovered,
            corrupted_entries = recovery_info.corrupted_entries,
            bytes_recovered = recovery_info.bytes_recovered,
            bytes_corrupted = recovery_info.bytes_corrupted,
            recovery_time_us = recovery_info.recovery_duration_us,
            "Segment recovery completed"
        );

        Ok(())
    }

    /// Read entry during recovery with corruption handling
    async fn read_entry_at_offset_during_recovery(
        &self,
        file: &mut tokio::fs::File,
        offset: u64,
    ) -> Result<(StorageEntry, usize)> {
        // Seek to offset
        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| StorageError::Io(format!("Failed to seek to offset {}: {}", offset, e)))?;

        // Read compression header (4 bytes)
        let mut compression_header = [0u8; 4];
        file.read_exact(&mut compression_header)
            .await
            .map_err(|e| StorageError::Io(format!("Failed to read compression header: {}", e)))?;

        let compressed_len = u32::from_le_bytes(compression_header);
        let mut total_size = 4; // Start with compression header

        let decompressed_data = if compressed_len > 0 {
            // Data is compressed
            let mut compressed_data = vec![0u8; compressed_len as usize];
            file.read_exact(&mut compressed_data).await.map_err(|e| StorageError::Io(format!("Failed to read compressed data: {}", e)))?;

            total_size += compressed_len as usize;

            // Decompress using LZ4
            lz4_flex::decompress_size_prepended(&compressed_data).map_err(|e| {
                StorageError::Io(format!("LZ4 decompression failed at offset {}: {}", offset, e))
            })?
        } else {
            // Data is uncompressed, read entry header first
            let mut entry_header = [0u8; 24];
            file.read_exact(&mut entry_header).await.map_err(|e| StorageError::Io(format!("Failed to read entry header: {}", e)))?;

            // Extract payload length from header
            let payload_len = u32::from_le_bytes(entry_header[20..24].try_into().unwrap()) as usize;

            // Read the payload
            let mut payload = vec![0u8; payload_len];
            file.read_exact(&mut payload).await.map_err(|e| StorageError::Io(format!("Failed to read payload: {}", e)))?;

            total_size += 24 + payload_len;

            // Reconstruct the full entry data
            let mut full_data = Vec::with_capacity(24 + payload_len);
            full_data.extend_from_slice(&entry_header);
            full_data.extend_from_slice(&payload);
            full_data
        };

        // Deserialize and validate the entry
        let entry = StorageEntry::deserialize(&decompressed_data)?;

        // Additional integrity check
        if let Err(_) = entry.validate() {
            return Err(StorageError::Io(format!("Entry checksum validation failed at offset {}", offset)));
        }

        Ok((entry, total_size))
    }

    /// Find the next valid entry after corruption
    async fn find_next_valid_entry(
        &self,
        file: &mut tokio::fs::File,
        start_offset: u64,
        file_size: u64,
    ) -> Result<Option<u64>> {
        const SCAN_BUFFER_SIZE: usize = 4096;
        const MAX_SCAN_DISTANCE: u64 = 1024 * 1024; // 1MB max scan

        let scan_end = std::cmp::min(start_offset + MAX_SCAN_DISTANCE, file_size);
        let mut current_offset = start_offset + 1; // Skip at least one byte

        while current_offset < scan_end {
            file.seek(SeekFrom::Start(current_offset)).await.map_err(|e| {
                StorageError::Io(format!("Failed to seek during scan: {}", e))
            })?;

            let remaining =
                std::cmp::min(SCAN_BUFFER_SIZE as u64, scan_end - current_offset) as usize;
            let mut buffer = vec![0u8; remaining];

            match file.read_exact(&mut buffer).await {
                Ok(_) => {
                    // Look for potential entry start patterns
                    for (i, _) in buffer.windows(4).enumerate() {
                        let test_offset = current_offset + i as u64;

                        // Try to read a potential entry at this position
                        if self
                            .read_entry_at_offset_during_recovery(file, test_offset)
                            .await
                            .is_ok()
                        {
                            return Ok(Some(test_offset));
                        }
                    }
                    current_offset += (remaining - 3) as u64; // Overlap for window search
                },
                Err(_) => break,
            }
        }

        Ok(None)
    }

    /// Append an entry using high-performance writer
    ///
    /// # Parameters
    ///
    /// * `entry` - Storage entry to append
    ///
    /// # Returns
    ///
    /// File offset where entry was written, or [`StorageError`] on failure.
    ///
    /// # Errors
    ///
    /// - [`StorageError::Io`] - Segment is sealed or operation failed
    pub async fn append_entry(&self, entry: &StorageEntry) -> Result<u64> {
        if self.is_sealed.load(Ordering::Acquire) {
            return Err(StorageError::Io(format!("Segment {} is sealed", self.id)));
        }

        let writer_guard = self.writer.read();
        let writer = writer_guard.as_ref().ok_or_else(|| StorageError::Io(format!("No writer available for segment {}", self.id)))?;

        // Write using high-performance writer
        let write_result = writer.write_entry(entry).await?;
        let offset = write_result.offset;

        // Update index
        let index_entry = IndexEntry {
            sequence: entry.sequence,
            offset,
            size: entry.size() as u32,
            timestamp: entry.metadata.timestamp,
        };

        self.sequence_index.insert(entry.sequence, index_entry);
        self.sorted_index.write().insert(entry.sequence, index_entry);

        // Update atomic statistics
        self.entry_count.fetch_add(1, Ordering::AcqRel);
        self.file_size.store(writer.size(), Ordering::Release);

        // Update sequence range tracking
        let seq = entry.sequence;
        self.first_sequence.fetch_min(seq, Ordering::AcqRel);
        self.last_sequence.fetch_max(seq, Ordering::AcqRel);

        Ok(offset)
    }

    /// Write multiple entries in a single batch for maximum performance
    ///
    /// # Parameters
    ///
    /// * `entries` - Slice of storage entries to write
    ///
    /// # Returns
    ///
    /// Vector of offsets where entries were written, or [`StorageError`] on failure.
    pub async fn append_batch(&self, entries: &[StorageEntry]) -> Result<Vec<u64>> {
        if self.is_sealed.load(Ordering::Acquire) {
            return Err(StorageError::Io(format!("Segment {} is sealed", self.id)));
        }

        let writer_guard = self.writer.read();
        let writer = writer_guard.as_ref().ok_or_else(|| StorageError::Io(format!("No writer available for segment {}", self.id)))?;

        // Write batch using high-performance writer
        let write_results = writer.write_batch(entries).await?;
        
        // Extract offsets from write results
        let offsets: Vec<u64> = write_results.iter().map(|result| result.offset).collect();

        // Update indexes for all entries
        let mut sorted_index = self.sorted_index.write();
        for (entry, &offset) in entries.iter().zip(offsets.iter()) {
            let index_entry = IndexEntry {
                sequence: entry.sequence,
                offset,
                size: entry.size() as u32,
                timestamp: entry.metadata.timestamp,
            };

            self.sequence_index.insert(entry.sequence, index_entry);
            sorted_index.insert(entry.sequence, index_entry);

            // Update sequence range
            let seq = entry.sequence;
            self.first_sequence.fetch_min(seq, Ordering::AcqRel);
            self.last_sequence.fetch_max(seq, Ordering::AcqRel);
        }

        // Update atomic statistics
        self.entry_count.fetch_add(entries.len() as u64, Ordering::AcqRel);
        self.file_size.store(writer.size(), Ordering::Release);

        Ok(offsets)
    }

    /// Read an entry by sequence number with O(1) lookup
    ///
    /// # Parameters
    ///
    /// * `sequence` - Sequence number to read
    ///
    /// # Returns
    ///
    /// The [`StorageEntry`] with the specified sequence, or [`StorageError`] if not found.
    pub async fn read_entry_by_sequence(&self, sequence: u64) -> Result<StorageEntry> {
        let index_entry = self
            .sequence_index
            .get(&sequence)
            .ok_or_else(|| StorageError::NotFound(format!("Sequence {} not found", sequence)))?;

        self.read_entry_at_offset(index_entry.offset).await
    }

    /// Read a range of entries by sequence numbers
    ///
    /// # Parameters
    ///
    /// * `start_sequence` - Starting sequence number (inclusive)
    /// * `end_sequence` - Ending sequence number (exclusive)
    ///
    /// # Returns
    ///
    /// Vector of [`StorageEntry`] in sequence order, or [`StorageError`] on failure.
    pub async fn read_range(
        &self,
        start_sequence: u64,
        end_sequence: u64,
    ) -> Result<Vec<StorageEntry>> {
        let sorted_index = self.sorted_index.read();
        let range = sorted_index.range(start_sequence..end_sequence);

        let mut entries = Vec::new();
        for (_, index_entry) in range {
            let entry = self.read_entry_at_offset(index_entry.offset).await?;
            entries.push(entry);
        }

        Ok(entries)
    }

    /// Read an entry at a specific file offset using memory mapping when possible
    ///
    /// # Parameters
    ///
    /// * `offset` - File offset to read from
    ///
    /// # Returns
    ///
    /// The [`StorageEntry`] at the specified offset, or [`StorageError`] on failure.
    pub async fn read_entry_at_offset(&self, offset: u64) -> Result<StorageEntry> {
        let reader_guard = self.reader.read();
        if let Some(ref reader) = *reader_guard {
            // Fast memory-mapped read
            return self.read_entry_with_reader(reader, offset).await;
        }

        // Fallback to file-based reading
        self.read_entry_at_offset_fallback(offset).await
    }

    /// Read entry using memory-mapped reader
    async fn read_entry_with_reader(
        &self,
        reader: &SegmentReader,
        offset: u64,
    ) -> Result<StorageEntry> {
        // Read compression header first
        let compression_data = reader.read_at(offset, 4)?;
        let compressed_len = u32::from_le_bytes(compression_data.try_into().unwrap());

        let decompressed_data = if compressed_len > 0 {
            // Data is compressed
            let compressed_data = reader.read_at(offset + 4, compressed_len as usize)?;

            // Decompress using LZ4
            lz4_flex::decompress_size_prepended(&compressed_data).map_err(|e| {
                StorageError::Io(format!("LZ4 decompression failed at offset {}: {}", offset, e))
            })?
        } else {
            // Data is uncompressed, read entry header to get size
            let header_data = reader.read_at(offset + 4, 24)?;
            let payload_len = u32::from_le_bytes(header_data[20..24].try_into().unwrap()) as usize;

            // Read payload
            let payload_data = reader.read_at(offset + 4 + 24, payload_len)?;

            // Reconstruct full entry data
            let mut full_data = Vec::with_capacity(24 + payload_len);
            full_data.extend_from_slice(&header_data);
            full_data.extend_from_slice(&payload_data);
            full_data
        };

        // Deserialize the entry
        StorageEntry::deserialize(&decompressed_data)
    }

    /// Fallback file-based entry reading
    async fn read_entry_at_offset_fallback(&self, offset: u64) -> Result<StorageEntry> {
        let mut file = tokio::fs::File::open(&self.file_path).await.map_err(|e| {
            StorageError::Io(format!("Failed to open {} for reading: {}", self.file_path.display(), e))
        })?;

        // Seek to offset
        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| StorageError::Io(format!("Failed to seek: {}", e)))?;

        // Read compression header (4 bytes)
        let mut compression_header = [0u8; 4];
        file.read_exact(&mut compression_header)
            .await
            .map_err(|e| StorageError::Io(format!("Failed to read compression header: {}", e)))?;

        let compressed_len = u32::from_le_bytes(compression_header);

        let decompressed_data = if compressed_len > 0 {
            // Data is compressed, read compressed data and decompress
            let mut compressed_data = vec![0u8; compressed_len as usize];
            file.read_exact(&mut compressed_data).await.map_err(|e| StorageError::Io(format!("Failed to read compressed data: {}", e)))?;

            // Decompress using LZ4
            lz4_flex::decompress_size_prepended(&compressed_data).map_err(|e| {
                StorageError::Io(format!("LZ4 decompression failed at offset {}: {}", offset, e))
            })?
        } else {
            // Data is uncompressed, need to read the entry header first to determine size
            let mut entry_header = [0u8; 24];
            file.read_exact(&mut entry_header).await.map_err(|e| StorageError::Io(format!("Failed to read entry header: {}", e)))?;

            // Extract payload length from header
            let payload_len = u32::from_le_bytes(entry_header[20..24].try_into().unwrap()) as usize;

            // Read the payload
            let mut payload = vec![0u8; payload_len];
            file.read_exact(&mut payload).await.map_err(|e| StorageError::Io(format!("Failed to read payload: {}", e)))?;

            // Reconstruct the full entry data
            let mut full_data = Vec::with_capacity(24 + payload_len);
            full_data.extend_from_slice(&entry_header);
            full_data.extend_from_slice(&payload);
            full_data
        };

        // Deserialize the entry
        StorageEntry::deserialize(&decompressed_data)
    }

    /// Check if a sequence number exists in this segment
    pub fn contains_sequence(&self, sequence: u64) -> bool {
        self.sequence_index.contains_key(&sequence)
    }

    /// Get index information for a sequence number
    pub fn get_index_entry(&self, sequence: u64) -> Option<IndexEntry> {
        self.sequence_index.get(&sequence).map(|entry| *entry.value())
    }

    /// Get all sequence numbers in this segment
    pub fn get_sequences(&self) -> Vec<u64> {
        let sorted_index = self.sorted_index.read();
        sorted_index.keys().cloned().collect()
    }

    /// Get recovery information for this segment
    pub fn recovery_info(&self) -> RecoveryInfo {
        self.recovery_info.read().clone()
    }

    /// Get current segment size in bytes
    pub fn size(&self) -> u64 {
        self.file_size.load(Ordering::Acquire)
    }

    /// Get number of entries in this segment
    pub fn entry_count(&self) -> u64 {
        self.entry_count.load(Ordering::Acquire)
    }

    /// Get segment age since creation
    pub fn age(&self) -> std::time::Duration {
        SystemTime::now().duration_since(self.created_at).unwrap_or_default()
    }

    /// Get first sequence number in this segment
    pub fn first_sequence(&self) -> u64 {
        self.first_sequence.load(Ordering::Acquire)
    }

    /// Get last sequence number in this segment
    pub fn last_sequence(&self) -> u64 {
        self.last_sequence.load(Ordering::Acquire)
    }

    /// Check if this segment is sealed (read-only)
    pub fn is_sealed(&self) -> bool {
        self.is_sealed.load(Ordering::Acquire)
    }

    /// Seal this segment (make it read-only)
    ///
    /// Sealing a segment prevents further writes, syncs all data to disk,
    /// and optimizes the segment for read-only access.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or [`StorageError`] on failure.
    pub async fn seal(&self) -> Result<()> {
        // Seal the writer if available
        if let Some(writer) = self.writer.write().take() {
            writer.seal().await?;
        }

        // Create or update reader for sealed segment
        if self.file_size.load(Ordering::Acquire) > 0 {
            let file = std::fs::File::open(&self.file_path).map_err(|e| {
                StorageError::Io(format!("Failed to open {} for sealing: {}", self.file_path.display(), e))
            })?;

            let reader = SegmentReader::new(file)?;
            *self.reader.write() = Some(reader);
        }

        self.is_sealed.store(true, Ordering::Release);

        tracing::info!(
            segment_id = self.id,
            entry_count = self.entry_count(),
            size_bytes = self.size(),
            index_entries = self.sequence_index.len(),
            "Segment sealed"
        );

        Ok(())
    }

    /// Validate segment integrity by checking all entries
    ///
    /// Performs a comprehensive integrity check of all entries in the segment,
    /// verifying checksums and data consistency.
    ///
    /// # Returns
    ///
    /// `Ok(())` if all entries are valid, or [`StorageError`] if corruption is found.
    pub async fn validate_integrity(&self) -> Result<()> {
        let sorted_index = self.sorted_index.read();
        let mut validation_errors = 0;

        tracing::info!(
            segment_id = self.id,
            entry_count = sorted_index.len(),
            "Starting segment integrity validation"
        );

        for (sequence, index_entry) in sorted_index.iter() {
            match self.read_entry_at_offset(index_entry.offset).await {
                Ok(entry) => {
                    // Verify sequence number matches
                    if entry.sequence != *sequence {
                        tracing::error!(
                            segment_id = self.id,
                            expected_sequence = sequence,
                            actual_sequence = entry.sequence,
                            offset = index_entry.offset,
                            "Sequence number mismatch"
                        );
                        validation_errors += 1;
                    }

                    // Verify integrity checksum
                    if let Err(_) = entry.validate() {
                        tracing::error!(
                            segment_id = self.id,
                            sequence = entry.sequence,
                            offset = index_entry.offset,
                            "Entry checksum validation failed"
                        );
                        validation_errors += 1;
                    }
                },
                Err(e) => {
                    tracing::error!(
                        segment_id = self.id,
                        sequence = sequence,
                        offset = index_entry.offset,
                        error = %e,
                        "Failed to read entry during validation"
                    );
                    validation_errors += 1;
                },
            }
        }

        if validation_errors > 0 {
            Err(StorageError::Io(format!("Segment {} validation failed with {} errors", self.id, validation_errors)))
        } else {
            tracing::info!(
                segment_id = self.id,
                entries_validated = sorted_index.len(),
                "Segment integrity validation completed successfully"
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_segment_creation_with_writer() {
        let temp_dir = TempDir::new().unwrap();
        let segment = StorageSegment::create_new(1, temp_dir.path(), true).await.unwrap();

        assert_eq!(segment.id, 1);
        assert_eq!(segment.entry_count(), 0);
        assert_eq!(segment.size(), 0);
        assert!(!segment.is_sealed());
        assert_eq!(segment.first_sequence(), u64::MAX);
        assert_eq!(segment.last_sequence(), 0);
        assert!(segment.writer.read().is_some());
    }

    #[tokio::test]
    async fn test_segment_append_and_read_by_sequence() {
        let temp_dir = TempDir::new().unwrap();
        let segment = StorageSegment::create_new(2, temp_dir.path(), false).await.unwrap();

        let payload = Bytes::from_static(b"test payload");
        let entry = StorageEntry::new(42, 1234567890, payload.clone());

        let offset = segment.append_entry(&entry).await.unwrap();
        assert_eq!(segment.entry_count(), 1);
        assert_eq!(segment.first_sequence(), 42);
        assert_eq!(segment.last_sequence(), 42);

        // Test read by sequence (O(1) lookup)
        let read_entry = segment.read_entry_by_sequence(42).await.unwrap();
        assert_eq!(read_entry.sequence, entry.sequence);
        assert_eq!(read_entry.metadata.timestamp, entry.metadata.timestamp);
        assert_eq!(read_entry.payload, entry.payload);

        // Test read by offset (original method)
        let read_entry_offset = segment.read_entry_at_offset(offset).await.unwrap();
        assert_eq!(read_entry_offset.sequence, entry.sequence);
        assert_eq!(read_entry_offset.payload, entry.payload);

        // Test contains_sequence
        assert!(segment.contains_sequence(42));
        assert!(!segment.contains_sequence(43));
    }

    #[tokio::test]
    async fn test_batch_append_and_range_read() {
        let temp_dir = TempDir::new().unwrap();
        let segment = StorageSegment::create_new(3, temp_dir.path(), true).await.unwrap();

        // Create batch of entries
        let entries: Vec<StorageEntry> = (0..10)
            .map(|i| {
                let payload = Bytes::from(format!("payload {}", i));
                StorageEntry::new(i, i * 1000, payload)
            })
            .collect();

        let offsets = segment.append_batch(&entries).await.unwrap();
        assert_eq!(offsets.len(), 10);
        assert_eq!(segment.entry_count(), 10);
        assert_eq!(segment.first_sequence(), 0);
        assert_eq!(segment.last_sequence(), 9);

        // Test range read
        let range_entries = segment.read_range(3, 7).await.unwrap();
        assert_eq!(range_entries.len(), 4); // 3, 4, 5, 6
        for (i, entry) in range_entries.iter().enumerate() {
            assert_eq!(entry.sequence, (i + 3) as u64);
            assert_eq!(entry.payload, Bytes::from(format!("payload {}", i + 3)));
        }
    }

    #[tokio::test]
    async fn test_segment_recovery() {
        let temp_dir = TempDir::new().unwrap();

        // Create segment and write data
        let segment1 = StorageSegment::create_new(4, temp_dir.path(), false).await.unwrap();

        let entries: Vec<StorageEntry> = (100..110)
            .map(|i| {
                let payload = Bytes::from(format!("recovery test {}", i));
                StorageEntry::new(i, i * 2000, payload)
            })
            .collect();

        segment1.append_batch(&entries).await.unwrap();
        segment1.seal().await.unwrap();
        drop(segment1);

        // Open existing segment with recovery
        let segment2 = StorageSegment::open_existing(4, temp_dir.path(), false).await.unwrap();

        assert_eq!(segment2.entry_count(), 10);
        assert_eq!(segment2.first_sequence(), 100);
        assert_eq!(segment2.last_sequence(), 109);
        assert!(segment2.is_sealed());

        let recovery_info = segment2.recovery_info();
        assert_eq!(recovery_info.entries_recovered, 10);
        assert_eq!(recovery_info.corrupted_entries, 0);
        assert!(recovery_info.bytes_recovered > 0);

        // Test that index was rebuilt correctly
        for i in 100..110 {
            assert!(segment2.contains_sequence(i));
            let entry = segment2.read_entry_by_sequence(i).await.unwrap();
            assert_eq!(entry.sequence, i);
            assert_eq!(entry.payload, Bytes::from(format!("recovery test {}", i)));
        }
    }

    #[tokio::test]
    async fn test_memory_mapped_reading() {
        let temp_dir = TempDir::new().unwrap();

        // Create a larger segment to trigger memory mapping
        let segment = StorageSegment::create_new(5, temp_dir.path(), false).await.unwrap();

        // Write enough data to make memory mapping worthwhile
        let large_entries: Vec<StorageEntry> = (0..100)
            .map(|i| {
                let payload = Bytes::from(vec![i as u8; 100]); // 100 bytes per entry
                StorageEntry::new(i, i * 3000, payload)
            })
            .collect();

        segment.append_batch(&large_entries).await.unwrap();
        segment.seal().await.unwrap();

        // Memory mapping should be active for reads
        let reader_guard = segment.reader.read();
        assert!(reader_guard.is_some());

        // Test reading with memory mapping
        let entry = segment.read_entry_by_sequence(50).await.unwrap();
        assert_eq!(entry.sequence, 50);
        assert_eq!(entry.payload.len(), 100);
    }

    #[tokio::test]
    async fn test_segment_validation() {
        let temp_dir = TempDir::new().unwrap();
        let segment = StorageSegment::create_new(6, temp_dir.path(), false).await.unwrap();

        let entries: Vec<StorageEntry> = (0..5)
            .map(|i| {
                let payload = Bytes::from(format!("validate {}", i));
                StorageEntry::new(i, i * 4000, payload)
            })
            .collect();

        segment.append_batch(&entries).await.unwrap();

        // Validation should pass for uncorrupted data
        segment.validate_integrity().await.unwrap();

        assert_eq!(segment.entry_count(), 5);
        assert_eq!(segment.sequence_index.len(), 5);
    }

    #[tokio::test]
    async fn test_index_operations() {
        let temp_dir = TempDir::new().unwrap();
        let segment = StorageSegment::create_new(7, temp_dir.path(), false).await.unwrap();

        let entries: Vec<StorageEntry> = vec![42, 17, 99, 3, 77]
            .into_iter()
            .map(|seq| {
                let payload = Bytes::from(format!("seq {}", seq));
                StorageEntry::new(seq, seq * 5000, payload)
            })
            .collect();

        segment.append_batch(&entries).await.unwrap();

        // Test index operations
        assert!(segment.contains_sequence(42));
        assert!(segment.contains_sequence(99));
        assert!(!segment.contains_sequence(50));

        let index_entry = segment.get_index_entry(17).unwrap();
        assert_eq!(index_entry.sequence, 17);
        assert!(index_entry.offset > 0);
        assert!(index_entry.size > 0);

        let sequences = segment.get_sequences();
        assert_eq!(sequences.len(), 5);
        // Should be sorted
        let mut sorted_sequences = sequences.clone();
        sorted_sequences.sort();
        assert_eq!(sequences, sorted_sequences);
    }
}