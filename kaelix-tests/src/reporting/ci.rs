//! # CI/CD Integration
//!
//! Integration with popular CI/CD platforms for automated test result reporting,
//! status checks, and artifact management.

use crate::reporting::{TestReport, TestResult, TestStatus};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, error, info, warn};

/// Supported CI/CD providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CIProvider {
    /// GitHub Actions
    GitHubActions,
    /// GitLab CI/CD
    GitLab,
    /// Jenkins
    Jenkins,
    /// TeamCity
    TeamCity,
    /// Azure DevOps
    AzureDevOps,
    /// CircleCI
    CircleCI,
    /// Generic webhook-based CI
    Generic { webhook_url: String },
}

/// CI/CD configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CIConfig {
    /// CI provider
    pub provider: CIProvider,
    /// Repository information
    pub repository: RepositoryInfo,
    /// Authentication configuration
    pub auth: AuthConfig,
    /// Reporting configuration
    pub reporting: ReportingConfig,
    /// Environment variables
    pub environment: HashMap<String, String>,
}

/// Repository information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryInfo {
    /// Repository owner/organization
    pub owner: String,
    /// Repository name
    pub name: String,
    /// Current branch
    pub branch: String,
    /// Commit SHA
    pub commit_sha: String,
    /// Pull request number (if applicable)
    pub pull_request: Option<u64>,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// API token
    pub token: Option<String>,
    /// Username for basic auth
    pub username: Option<String>,
    /// Password for basic auth
    pub password: Option<String>,
    /// Custom headers
    pub headers: HashMap<String, String>,
}

/// Reporting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportingConfig {
    /// Post comments on pull requests
    pub post_pr_comments: bool,
    /// Update commit status
    pub update_commit_status: bool,
    /// Upload test artifacts
    pub upload_artifacts: bool,
    /// Create GitHub/GitLab issues on failures
    pub create_issues_on_failure: bool,
    /// Fail the build on test failures
    pub fail_build_on_failures: bool,
    /// Artifact retention days
    pub artifact_retention_days: u32,
}

impl Default for ReportingConfig {
    fn default() -> Self {
        Self {
            post_pr_comments: true,
            update_commit_status: true,
            upload_artifacts: true,
            create_issues_on_failure: false,
            fail_build_on_failures: true,
            artifact_retention_days: 30,
        }
    }
}

/// Artifact to be uploaded
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// Artifact name
    pub name: String,
    /// File path
    pub path: String,
    /// Artifact type
    pub artifact_type: ArtifactType,
    /// Content type
    pub content_type: String,
    /// Description
    pub description: Option<String>,
}

/// Types of artifacts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArtifactType {
    /// Test report (HTML, JSON, XML)
    TestReport,
    /// Performance metrics
    Metrics,
    /// Log files
    Logs,
    /// Coverage reports
    Coverage,
    /// Screenshots
    Screenshots,
    /// Custom artifact
    Custom(String),
}

/// CI/CD integration errors
#[derive(Debug, thiserror::Error)]
pub enum CIError {
    #[error("Authentication failed: {0}")]
    Authentication(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("File system error: {0}")]
    FileSystem(String),
}

/// CI/CD reporter trait
#[async_trait]
pub trait CIReporter: Send + Sync {
    /// Posts test results to the CI platform
    async fn post_test_results(&self, report: &TestReport) -> Result<(), CIError>;
    
    /// Creates a comment on a pull request
    async fn create_pr_comment(&self, report: &TestReport) -> Result<(), CIError>;
    
    /// Sets the commit status
    async fn set_status_check(&self, status: TestStatus, description: &str) -> Result<(), CIError>;
    
    /// Uploads artifacts
    async fn upload_artifacts(&self, artifacts: Vec<Artifact>) -> Result<(), CIError>;
    
    /// Creates an issue for test failures
    async fn create_failure_issue(&self, report: &TestReport) -> Result<(), CIError>;
    
    /// Gets the provider name
    fn provider_name(&self) -> &str;
}

/// Main CI reporter that delegates to provider-specific implementations
#[derive(Debug)]
pub struct CIReporter {
    config: CIConfig,
    client: reqwest::Client,
}

impl CIReporter {
    /// Creates a new CI reporter
    pub fn new(config: CIConfig) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("MemoryStreamer-Tests/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        
        Self { config, client }
    }
    
    /// Creates a CI reporter from environment variables
    pub fn from_environment() -> Result<Self, CIError> {
        let config = Self::detect_ci_environment()?;
        Ok(Self::new(config))
    }
    
    /// Detects CI environment and creates appropriate configuration
    fn detect_ci_environment() -> Result<CIConfig, CIError> {
        // GitHub Actions detection
        if std::env::var("GITHUB_ACTIONS").is_ok() {
            return Ok(CIConfig {
                provider: CIProvider::GitHubActions,
                repository: RepositoryInfo {
                    owner: std::env::var("GITHUB_REPOSITORY_OWNER")
                        .unwrap_or_else(|_| "unknown".to_string()),
                    name: std::env::var("GITHUB_REPOSITORY")
                        .unwrap_or_else(|_| "unknown".to_string())
                        .split('/')
                        .nth(1)
                        .unwrap_or("unknown")
                        .to_string(),
                    branch: std::env::var("GITHUB_REF_NAME")
                        .unwrap_or_else(|_| "main".to_string()),
                    commit_sha: std::env::var("GITHUB_SHA")
                        .unwrap_or_else(|_| "unknown".to_string()),
                    pull_request: std::env::var("GITHUB_EVENT_NUMBER")
                        .ok()
                        .and_then(|n| n.parse().ok()),
                },
                auth: AuthConfig {
                    token: std::env::var("GITHUB_TOKEN").ok(),
                    username: None,
                    password: None,
                    headers: HashMap::new(),
                },
                reporting: ReportingConfig::default(),
                environment: std::env::vars().collect(),
            });
        }
        
        // GitLab CI detection
        if std::env::var("GITLAB_CI").is_ok() {
            return Ok(CIConfig {
                provider: CIProvider::GitLab,
                repository: RepositoryInfo {
                    owner: std::env::var("CI_PROJECT_NAMESPACE")
                        .unwrap_or_else(|_| "unknown".to_string()),
                    name: std::env::var("CI_PROJECT_NAME")
                        .unwrap_or_else(|_| "unknown".to_string()),
                    branch: std::env::var("CI_COMMIT_REF_NAME")
                        .unwrap_or_else(|_| "main".to_string()),
                    commit_sha: std::env::var("CI_COMMIT_SHA")
                        .unwrap_or_else(|_| "unknown".to_string()),
                    pull_request: std::env::var("CI_MERGE_REQUEST_IID")
                        .ok()
                        .and_then(|n| n.parse().ok()),
                },
                auth: AuthConfig {
                    token: std::env::var("CI_JOB_TOKEN").ok()
                        .or_else(|| std::env::var("GITLAB_TOKEN").ok()),
                    username: None,
                    password: None,
                    headers: HashMap::new(),
                },
                reporting: ReportingConfig::default(),
                environment: std::env::vars().collect(),
            });
        }
        
        // Jenkins detection
        if std::env::var("JENKINS_URL").is_ok() {
            return Ok(CIConfig {
                provider: CIProvider::Jenkins,
                repository: RepositoryInfo {
                    owner: std::env::var("GIT_AUTHOR_NAME")
                        .unwrap_or_else(|_| "unknown".to_string()),
                    name: std::env::var("JOB_NAME")
                        .unwrap_or_else(|_| "unknown".to_string()),
                    branch: std::env::var("GIT_BRANCH")
                        .unwrap_or_else(|_| "main".to_string()),
                    commit_sha: std::env::var("GIT_COMMIT")
                        .unwrap_or_else(|_| "unknown".to_string()),
                    pull_request: None, // Jenkins doesn't have a standard PR env var
                },
                auth: AuthConfig {
                    token: std::env::var("JENKINS_API_TOKEN").ok(),
                    username: std::env::var("JENKINS_USER").ok(),
                    password: std::env::var("JENKINS_PASSWORD").ok(),
                    headers: HashMap::new(),
                },
                reporting: ReportingConfig::default(),
                environment: std::env::vars().collect(),
            });
        }
        
        Err(CIError::Configuration("No supported CI environment detected".to_string()))
    }
}

#[async_trait]
impl CIReporter for CIReporter {
    async fn post_test_results(&self, report: &TestReport) -> Result<(), CIError> {
        match &self.config.provider {
            CIProvider::GitHubActions => self.post_github_results(report).await,
            CIProvider::GitLab => self.post_gitlab_results(report).await,
            CIProvider::Jenkins => self.post_jenkins_results(report).await,
            CIProvider::TeamCity => self.post_teamcity_results(report).await,
            CIProvider::AzureDevOps => self.post_azure_results(report).await,
            CIProvider::CircleCI => self.post_circleci_results(report).await,
            CIProvider::Generic { webhook_url } => self.post_generic_results(report, webhook_url).await,
        }
    }
    
    async fn create_pr_comment(&self, report: &TestReport) -> Result<(), CIError> {
        if !self.config.reporting.post_pr_comments {
            return Ok(());
        }
        
        let pr_number = match self.config.repository.pull_request {
            Some(pr) => pr,
            None => {
                debug!("No pull request number available, skipping PR comment");
                return Ok(());
            }
        };
        
        let comment_body = self.generate_pr_comment(report);
        
        match &self.config.provider {
            CIProvider::GitHubActions => {
                self.post_github_pr_comment(pr_number, &comment_body).await
            }
            CIProvider::GitLab => {
                self.post_gitlab_mr_comment(pr_number, &comment_body).await
            }
            _ => {
                warn!("PR comments not supported for provider: {:?}", self.config.provider);
                Ok(())
            }
        }
    }
    
    async fn set_status_check(&self, status: TestStatus, description: &str) -> Result<(), CIError> {
        if !self.config.reporting.update_commit_status {
            return Ok(());
        }
        
        let state = match status {
            TestStatus::Passed => "success",
            TestStatus::Failed => "failure",
            TestStatus::Running => "pending",
            TestStatus::Skipped => "success",
            TestStatus::Timeout => "failure",
        };
        
        match &self.config.provider {
            CIProvider::GitHubActions => {
                self.set_github_status(state, description).await
            }
            CIProvider::GitLab => {
                self.set_gitlab_status(state, description).await
            }
            _ => {
                warn!("Status checks not supported for provider: {:?}", self.config.provider);
                Ok(())
            }
        }
    }
    
    async fn upload_artifacts(&self, artifacts: Vec<Artifact>) -> Result<(), CIError> {
        if !self.config.reporting.upload_artifacts {
            return Ok(());
        }
        
        for artifact in artifacts {
            match &self.config.provider {
                CIProvider::GitHubActions => {
                    self.upload_github_artifact(&artifact).await?;
                }
                CIProvider::GitLab => {
                    self.upload_gitlab_artifact(&artifact).await?;
                }
                _ => {
                    info!("Artifact upload not implemented for provider: {:?}", self.config.provider);
                }
            }
        }
        
        Ok(())
    }
    
    async fn create_failure_issue(&self, report: &TestReport) -> Result<(), CIError> {
        if !self.config.reporting.create_issues_on_failure {
            return Ok(());
        }
        
        let stats = report.get_stats();
        if stats.failed == 0 {
            return Ok(());
        }
        
        let issue_title = format!("Test failures in {}", report.suite_name);
        let issue_body = self.generate_failure_issue_body(report);
        
        match &self.config.provider {
            CIProvider::GitHubActions => {
                self.create_github_issue(&issue_title, &issue_body).await
            }
            CIProvider::GitLab => {
                self.create_gitlab_issue(&issue_title, &issue_body).await
            }
            _ => {
                warn!("Issue creation not supported for provider: {:?}", self.config.provider);
                Ok(())
            }
        }
    }
    
    fn provider_name(&self) -> &str {
        match &self.config.provider {
            CIProvider::GitHubActions => "GitHub Actions",
            CIProvider::GitLab => "GitLab CI",
            CIProvider::Jenkins => "Jenkins",
            CIProvider::TeamCity => "TeamCity",
            CIProvider::AzureDevOps => "Azure DevOps",
            CIProvider::CircleCI => "CircleCI",
            CIProvider::Generic { .. } => "Generic",
        }
    }
}

impl CIReporter {
    /// Posts test results to GitHub
    async fn post_github_results(&self, report: &TestReport) -> Result<(), CIError> {
        let token = self.config.auth.token.as_ref()
            .ok_or_else(|| CIError::Authentication("GitHub token required".to_string()))?;
        
        let check_run_payload = serde_json::json!({
            "name": format!("MemoryStreamer Tests: {}", report.suite_name),
            "head_sha": self.config.repository.commit_sha,
            "status": "completed",
            "conclusion": if report.get_stats().failed > 0 { "failure" } else { "success" },
            "output": {
                "title": format!("Test Results: {}", report.suite_name),
                "summary": self.generate_test_summary(report),
                "text": self.generate_detailed_results(report)
            }
        });
        
        let url = format!(
            "https://api.github.com/repos/{}/{}/check-runs",
            self.config.repository.owner,
            self.config.repository.name
        );
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("token {}", token))
            .header("Accept", "application/vnd.github.v3+json")
            .json(&check_run_payload)
            .send()
            .await
            .map_err(|e| CIError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(CIError::Api(format!(
                "GitHub API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }
        
        info!("Posted test results to GitHub");
        Ok(())
    }
    
    /// Posts test results to GitLab
    async fn post_gitlab_results(&self, report: &TestReport) -> Result<(), CIError> {
        let token = self.config.auth.token.as_ref()
            .ok_or_else(|| CIError::Authentication("GitLab token required".to_string()))?;
        
        // GitLab uses pipeline status API
        let status = if report.get_stats().failed > 0 { "failed" } else { "success" };
        
        let url = format!(
            "https://gitlab.com/api/v4/projects/{}/statuses/{}",
            format!("{}%2F{}", self.config.repository.owner, self.config.repository.name),
            self.config.repository.commit_sha
        );
        
        let payload = serde_json::json!({
            "state": status,
            "name": format!("MemoryStreamer Tests: {}", report.suite_name),
            "description": self.generate_test_summary(report)
        });
        
        let response = self.client
            .post(&url)
            .header("PRIVATE-TOKEN", token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| CIError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(CIError::Api(format!(
                "GitLab API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }
        
        info!("Posted test results to GitLab");
        Ok(())
    }
    
    /// Posts test results to Jenkins
    async fn post_jenkins_results(&self, _report: &TestReport) -> Result<(), CIError> {
        // Jenkins typically uses XML reports uploaded as build artifacts
        info!("Jenkins results posting not implemented - use JUnit XML reports");
        Ok(())
    }
    
    /// Posts test results to TeamCity
    async fn post_teamcity_results(&self, _report: &TestReport) -> Result<(), CIError> {
        // TeamCity uses service messages during build execution
        info!("TeamCity results posting not implemented - use service messages");
        Ok(())
    }
    
    /// Posts test results to Azure DevOps
    async fn post_azure_results(&self, _report: &TestReport) -> Result<(), CIError> {
        info!("Azure DevOps results posting not implemented");
        Ok(())
    }
    
    /// Posts test results to CircleCI
    async fn post_circleci_results(&self, _report: &TestReport) -> Result<(), CIError> {
        info!("CircleCI results posting not implemented");
        Ok(())
    }
    
    /// Posts test results via generic webhook
    async fn post_generic_results(&self, report: &TestReport, webhook_url: &str) -> Result<(), CIError> {
        let payload = serde_json::json!({
            "test_report": report,
            "repository": self.config.repository,
            "timestamp": chrono::Utc::now()
        });
        
        let response = self.client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| CIError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(CIError::Api(format!(
                "Webhook error: {}",
                response.status()
            )));
        }
        
        info!("Posted test results to webhook");
        Ok(())
    }
    
    /// Posts a comment on GitHub PR
    async fn post_github_pr_comment(&self, pr_number: u64, body: &str) -> Result<(), CIError> {
        let token = self.config.auth.token.as_ref()
            .ok_or_else(|| CIError::Authentication("GitHub token required".to_string()))?;
        
        let url = format!(
            "https://api.github.com/repos/{}/{}/issues/{}/comments",
            self.config.repository.owner,
            self.config.repository.name,
            pr_number
        );
        
        let payload = serde_json::json!({
            "body": body
        });
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("token {}", token))
            .header("Accept", "application/vnd.github.v3+json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| CIError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(CIError::Api(format!(
                "GitHub API error: {}",
                response.status()
            )));
        }
        
        info!("Posted GitHub PR comment");
        Ok(())
    }
    
    /// Posts a comment on GitLab MR
    async fn post_gitlab_mr_comment(&self, mr_number: u64, body: &str) -> Result<(), CIError> {
        let token = self.config.auth.token.as_ref()
            .ok_or_else(|| CIError::Authentication("GitLab token required".to_string()))?;
        
        let url = format!(
            "https://gitlab.com/api/v4/projects/{}/merge_requests/{}/notes",
            format!("{}%2F{}", self.config.repository.owner, self.config.repository.name),
            mr_number
        );
        
        let payload = serde_json::json!({
            "body": body
        });
        
        let response = self.client
            .post(&url)
            .header("PRIVATE-TOKEN", token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| CIError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(CIError::Api(format!(
                "GitLab API error: {}",
                response.status()
            )));
        }
        
        info!("Posted GitLab MR comment");
        Ok(())
    }
    
    /// Sets GitHub commit status
    async fn set_github_status(&self, state: &str, description: &str) -> Result<(), CIError> {
        let token = self.config.auth.token.as_ref()
            .ok_or_else(|| CIError::Authentication("GitHub token required".to_string()))?;
        
        let url = format!(
            "https://api.github.com/repos/{}/{}/statuses/{}",
            self.config.repository.owner,
            self.config.repository.name,
            self.config.repository.commit_sha
        );
        
        let payload = serde_json::json!({
            "state": state,
            "description": description,
            "context": "MemoryStreamer Tests"
        });
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("token {}", token))
            .header("Accept", "application/vnd.github.v3+json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| CIError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(CIError::Api(format!(
                "GitHub API error: {}",
                response.status()
            )));
        }
        
        info!("Set GitHub commit status: {}", state);
        Ok(())
    }
    
    /// Sets GitLab commit status
    async fn set_gitlab_status(&self, state: &str, description: &str) -> Result<(), CIError> {
        // GitLab status setting is handled in post_gitlab_results
        info!("GitLab status set: {} - {}", state, description);
        Ok(())
    }
    
    /// Uploads artifact to GitHub
    async fn upload_github_artifact(&self, artifact: &Artifact) -> Result<(), CIError> {
        // GitHub Actions artifacts are typically handled by the actions/upload-artifact action
        // For API-based upload, we would need the Actions API which requires workflow context
        info!("GitHub artifact upload: {} (use actions/upload-artifact in workflow)", artifact.name);
        Ok(())
    }
    
    /// Uploads artifact to GitLab
    async fn upload_gitlab_artifact(&self, artifact: &Artifact) -> Result<(), CIError> {
        // GitLab artifacts are typically specified in the .gitlab-ci.yml file
        info!("GitLab artifact upload: {} (configure in .gitlab-ci.yml)", artifact.name);
        Ok(())
    }
    
    /// Creates a GitHub issue
    async fn create_github_issue(&self, title: &str, body: &str) -> Result<(), CIError> {
        let token = self.config.auth.token.as_ref()
            .ok_or_else(|| CIError::Authentication("GitHub token required".to_string()))?;
        
        let url = format!(
            "https://api.github.com/repos/{}/{}/issues",
            self.config.repository.owner,
            self.config.repository.name
        );
        
        let payload = serde_json::json!({
            "title": title,
            "body": body,
            "labels": ["test-failure", "automated"]
        });
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("token {}", token))
            .header("Accept", "application/vnd.github.v3+json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| CIError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(CIError::Api(format!(
                "GitHub API error: {}",
                response.status()
            )));
        }
        
        info!("Created GitHub issue for test failures");
        Ok(())
    }
    
    /// Creates a GitLab issue
    async fn create_gitlab_issue(&self, title: &str, body: &str) -> Result<(), CIError> {
        let token = self.config.auth.token.as_ref()
            .ok_or_else(|| CIError::Authentication("GitLab token required".to_string()))?;
        
        let url = format!(
            "https://gitlab.com/api/v4/projects/{}/issues",
            format!("{}%2F{}", self.config.repository.owner, self.config.repository.name)
        );
        
        let payload = serde_json::json!({
            "title": title,
            "description": body,
            "labels": "test-failure,automated"
        });
        
        let response = self.client
            .post(&url)
            .header("PRIVATE-TOKEN", token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| CIError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(CIError::Api(format!(
                "GitLab API error: {}",
                response.status()
            )));
        }
        
        info!("Created GitLab issue for test failures");
        Ok(())
    }
    
    /// Generates a test summary
    fn generate_test_summary(&self, report: &TestReport) -> String {
        let stats = report.get_stats();
        format!(
            "Tests: {} total, {} passed, {} failed, {} skipped. Success rate: {:.1}%",
            stats.total,
            stats.passed,
            stats.failed,
            stats.skipped,
            stats.success_rate
        )
    }
    
    /// Generates detailed test results
    fn generate_detailed_results(&self, report: &TestReport) -> String {
        let mut details = String::new();
        
        // Add performance summary if available
        if let Some(perf) = &report.performance_summary {
            details.push_str(&format!(
                "## Performance Summary\n\n\
                - Average Throughput: {:.0} msgs/sec\n\
                - Peak Throughput: {:.0} msgs/sec\n\
                - P99 Latency: {:.2} ms\n\
                - Error Rate: {:.2}%\n\n",
                perf.avg_throughput,
                perf.peak_throughput,
                perf.p99_latency_ns as f64 / 1_000_000.0,
                perf.error_rate_percent
            ));
        }
        
        // Add failed tests details
        let failed_tests: Vec<_> = report.results.iter()
            .filter(|r| r.status == TestStatus::Failed)
            .collect();
        
        if !failed_tests.is_empty() {
            details.push_str("## Failed Tests\n\n");
            for test in failed_tests {
                details.push_str(&format!(
                    "- **{}**: {}\n",
                    test.name,
                    test.error.as_deref().unwrap_or("No error details")
                ));
            }
        }
        
        details
    }
    
    /// Generates PR comment body
    fn generate_pr_comment(&self, report: &TestReport) -> String {
        let stats = report.get_stats();
        let status_emoji = if stats.failed > 0 { "❌" } else { "✅" };
        
        format!(
            "{} **MemoryStreamer Test Results**\n\n\
            {}\n\n\
            **Duration:** {:.2}s\n\n\
            {}",
            status_emoji,
            self.generate_test_summary(report),
            stats.total_duration.as_secs_f64(),
            if stats.failed > 0 {
                self.generate_detailed_results(report)
            } else {
                "All tests passed! 🎉".to_string()
            }
        )
    }
    
    /// Generates failure issue body
    fn generate_failure_issue_body(&self, report: &TestReport) -> String {
        format!(
            "Test failures detected in commit {}.\n\n\
            {}\n\n\
            **Branch:** {}\n\
            **Commit:** {}\n\
            **Suite:** {}\n\n\
            {}\n\n\
            This issue was automatically created by the MemoryStreamer test monitoring system.",
            self.config.repository.commit_sha,
            self.generate_test_summary(report),
            self.config.repository.branch,
            self.config.repository.commit_sha,
            report.suite_name,
            self.generate_detailed_results(report)
        )
    }
}

/// Creates a CI reporter from environment
pub fn create_ci_reporter() -> Result<CIReporter, CIError> {
    CIReporter::from_environment()
}

/// Creates artifacts for common test outputs
pub fn create_test_artifacts(report: &TestReport, output_dir: &Path) -> Vec<Artifact> {
    let mut artifacts = Vec::new();
    
    // HTML report
    artifacts.push(Artifact {
        name: "test-report.html".to_string(),
        path: output_dir.join("test-report.html").to_string_lossy().to_string(),
        artifact_type: ArtifactType::TestReport,
        content_type: "text/html".to_string(),
        description: Some("HTML test report".to_string()),
    });
    
    // JSON report
    artifacts.push(Artifact {
        name: "test-report.json".to_string(),
        path: output_dir.join("test-report.json").to_string_lossy().to_string(),
        artifact_type: ArtifactType::TestReport,
        content_type: "application/json".to_string(),
        description: Some("JSON test report".to_string()),
    });
    
    // JUnit XML report
    artifacts.push(Artifact {
        name: "junit.xml".to_string(),
        path: output_dir.join("junit.xml").to_string_lossy().to_string(),
        artifact_type: ArtifactType::TestReport,
        content_type: "application/xml".to_string(),
        description: Some("JUnit XML test report".to_string()),
    });
    
    // Prometheus metrics
    artifacts.push(Artifact {
        name: "metrics.txt".to_string(),
        path: output_dir.join("metrics.txt").to_string_lossy().to_string(),
        artifact_type: ArtifactType::Metrics,
        content_type: "text/plain".to_string(),
        description: Some("Prometheus metrics export".to_string()),
    });
    
    artifacts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reporting::TestReport;
    
    #[test]
    fn test_ci_config_creation() {
        let config = CIConfig {
            provider: CIProvider::GitHubActions,
            repository: RepositoryInfo {
                owner: "test-org".to_string(),
                name: "test-repo".to_string(),
                branch: "main".to_string(),
                commit_sha: "abc123".to_string(),
                pull_request: Some(42),
            },
            auth: AuthConfig {
                token: Some("token".to_string()),
                username: None,
                password: None,
                headers: HashMap::new(),
            },
            reporting: ReportingConfig::default(),
            environment: HashMap::new(),
        };
        
        assert!(matches!(config.provider, CIProvider::GitHubActions));
        assert_eq!(config.repository.name, "test-repo");
    }
    
    #[test]
    fn test_artifact_creation() {
        let artifact = Artifact {
            name: "test.html".to_string(),
            path: "/tmp/test.html".to_string(),
            artifact_type: ArtifactType::TestReport,
            content_type: "text/html".to_string(),
            description: Some("Test report".to_string()),
        };
        
        assert_eq!(artifact.name, "test.html");
        assert!(matches!(artifact.artifact_type, ArtifactType::TestReport));
    }
    
    #[test]
    fn test_reporter_creation() {
        let config = CIConfig {
            provider: CIProvider::GitHubActions,
            repository: RepositoryInfo {
                owner: "test".to_string(),
                name: "test".to_string(),
                branch: "main".to_string(),
                commit_sha: "abc".to_string(),
                pull_request: None,
            },
            auth: AuthConfig {
                token: Some("token".to_string()),
                username: None,
                password: None,
                headers: HashMap::new(),
            },
            reporting: ReportingConfig::default(),
            environment: HashMap::new(),
        };
        
        let reporter = CIReporter::new(config);
        assert_eq!(reporter.provider_name(), "GitHub Actions");
    }
}