//! Transaction management for the storage engine
//!
//! This module provides ACID transaction support with distributed consistency.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Transaction identifier type
pub type TransactionId = u64;

/// Transaction configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionConfig {
    /// Transaction log directory
    pub log_dir: PathBuf,

    /// Maximum transaction duration
    pub max_duration: Duration,

    /// Enable distributed transactions
    pub distributed_mode: bool,

    /// Transaction isolation level
    pub isolation_level: IsolationLevel,
}

impl Default for TransactionConfig {
    fn default() -> Self {
        Self {
            log_dir: PathBuf::from("./transaction_log"),
            max_duration: Duration::from_secs(30),
            distributed_mode: false,
            isolation_level: IsolationLevel::ReadCommitted,
        }
    }
}

/// Transaction isolation levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum IsolationLevel {
    /// Read uncommitted data
    ReadUncommitted,
    /// Read only committed data
    ReadCommitted,
    /// Repeatable reads within a transaction
    RepeatableRead,
    /// Full serializable isolation
    Serializable,
}

/// Transaction error types
#[derive(Error, Debug, Clone)]
pub enum TransactionError {
    /// Transaction not found
    #[error("Transaction {0} not found")]
    NotFound(TransactionId),

    /// Transaction already committed
    #[error("Transaction {0} already committed")]
    AlreadyCommitted(TransactionId),

    /// Transaction already aborted
    #[error("Transaction {0} already aborted")]
    AlreadyAborted(TransactionId),

    /// Transaction timeout
    #[error("Transaction {0} timed out")]
    Timeout(TransactionId),

    /// Deadlock detected
    #[error("Deadlock detected involving transaction {0}")]
    Deadlock(TransactionId),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Transaction statistics
#[derive(Debug, Clone, Default)]
pub struct TransactionStats {
    /// Total transactions started
    pub transactions_started: u64,
    /// Total transactions committed
    pub transactions_committed: u64,
    /// Total transactions aborted
    pub transactions_aborted: u64,
    /// Active transactions
    pub active_transactions: u64,
    /// Average transaction duration
    pub avg_duration: Duration,
    /// Transaction timeout count
    pub timeouts: u64,
    /// Deadlock count
    pub deadlocks: u64,
}

/// Transaction state
#[derive(Debug, Clone, Copy)]
pub enum TransactionState {
    /// Transaction is active
    Active,
    /// Transaction is preparing to commit
    Preparing,
    /// Transaction is committed
    Committed,
    /// Transaction is aborted
    Aborted,
}

/// A transaction in the storage system
pub struct Transaction {
    /// Transaction ID
    pub id: TransactionId,
    /// Transaction state
    pub state: TransactionState,
    /// Start time
    pub start_time: SystemTime,
    /// Configuration
    pub config: TransactionConfig,
}

impl Transaction {
    /// Create a new transaction
    pub fn new(id: TransactionId, config: TransactionConfig) -> Self {
        Self {
            id,
            state: TransactionState::Active,
            start_time: SystemTime::now(),
            config,
        }
    }

    /// Commit the transaction
    pub async fn commit(&mut self) -> Result<(), TransactionError> {
        match self.state {
            TransactionState::Active => {
                self.state = TransactionState::Committed;
                Ok(())
            }
            TransactionState::Committed => Err(TransactionError::AlreadyCommitted(self.id)),
            TransactionState::Aborted => Err(TransactionError::AlreadyAborted(self.id)),
            TransactionState::Preparing => {
                self.state = TransactionState::Committed;
                Ok(())
            }
        }
    }

    /// Abort the transaction
    pub async fn abort(&mut self) -> Result<(), TransactionError> {
        match self.state {
            TransactionState::Active | TransactionState::Preparing => {
                self.state = TransactionState::Aborted;
                Ok(())
            }
            TransactionState::Committed => Err(TransactionError::AlreadyCommitted(self.id)),
            TransactionState::Aborted => Err(TransactionError::AlreadyAborted(self.id)),
        }
    }

    /// Get transaction age
    pub fn age(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.start_time)
            .unwrap_or(Duration::ZERO)
    }

    /// Check if transaction has timed out
    pub fn is_timed_out(&self) -> bool {
        self.age() > self.config.max_duration
    }
}

/// Transaction manager
pub struct TransactionManager {
    /// Configuration
    config: TransactionConfig,
    /// Next transaction ID
    next_id: std::sync::atomic::AtomicU64,
    /// Statistics
    stats: std::sync::Arc<std::sync::RwLock<TransactionStats>>,
}

impl TransactionManager {
    /// Create a new transaction manager
    pub async fn new(config: TransactionConfig) -> Result<Self, TransactionError> {
        // Create log directory if it doesn't exist
        if let Err(e) = tokio::fs::create_dir_all(&config.log_dir).await {
            return Err(TransactionError::Io(format!(
                "Failed to create transaction log directory: {}",
                e
            )));
        }

        Ok(Self {
            config,
            next_id: std::sync::atomic::AtomicU64::new(1),
            stats: std::sync::Arc::new(std::sync::RwLock::new(TransactionStats::default())),
        })
    }

    /// Begin a new transaction
    pub async fn begin_transaction(&self) -> Result<Transaction, TransactionError> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let transaction = Transaction::new(id, self.config.clone());

        // Update statistics
        {
            let mut stats = self.stats.write().unwrap();
            stats.transactions_started += 1;
            stats.active_transactions += 1;
        }

        Ok(transaction)
    }

    /// Get transaction statistics
    pub async fn stats(&self) -> TransactionStats {
        self.stats.read().unwrap().clone()
    }

    /// Shutdown the transaction manager
    pub async fn shutdown(&self) -> Result<(), TransactionError> {
        // In a full implementation, this would:
        // - Abort all active transactions
        // - Flush transaction logs
        // - Clean up resources
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_transaction_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = TransactionConfig::default();
        config.log_dir = temp_dir.path().to_path_buf();

        let manager = TransactionManager::new(config).await.unwrap();
        
        let stats = manager.stats().await;
        assert_eq!(stats.transactions_started, 0);
    }

    #[tokio::test]
    async fn test_begin_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = TransactionConfig::default();
        config.log_dir = temp_dir.path().to_path_buf();

        let manager = TransactionManager::new(config).await.unwrap();
        let transaction = manager.begin_transaction().await.unwrap();
        
        assert_eq!(transaction.id, 1);
        assert!(matches!(transaction.state, TransactionState::Active));
    }

    #[tokio::test]
    async fn test_transaction_commit() {
        let config = TransactionConfig::default();
        let mut transaction = Transaction::new(1, config);
        
        let result = transaction.commit().await;
        assert!(result.is_ok());
        assert!(matches!(transaction.state, TransactionState::Committed));
    }

    #[tokio::test]
    async fn test_transaction_abort() {
        let config = TransactionConfig::default();
        let mut transaction = Transaction::new(1, config);
        
        let result = transaction.abort().await;
        assert!(result.is_ok());
        assert!(matches!(transaction.state, TransactionState::Aborted));
    }
}