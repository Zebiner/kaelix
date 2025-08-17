//! Data management and persistence for testing infrastructure.

pub mod persistence;

pub use persistence::{
    TestDataManager, TestDataSet, DataSetMetadata, GenerationConfig,
    StorageBackend, CacheStats, DataSetBuilder,
};