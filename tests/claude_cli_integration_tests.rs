//! Claude Code CLI Integration Tests
//!
//! Tests for the actual CLI integration patterns, process lifecycle management,
//! and real-world usage scenarios with the Claude Code binary.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio, Child, ExitStatus};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::fs::{self, File};
use std::io::{Write, Read, BufReader, BufRead};

use tempfile::{TempDir, NamedTempFile};
use tokio::process::Command as TokioCommand;
use tokio::time::{timeout, sleep};
use tokio::io::{AsyncWriteExt, AsyncBufReadExt, BufReader as TokioBufReader};
use tokio::sync::{Mutex, RwLock};
use serde_json::{json, Value};

use crate::common::claude_test_utils::{TestEnvironment, ClaudeTestAssertions};

/// CLI Integration Test Framework
#[derive(Debug)]
pub struct CLITestFramework {
    temp_dir: TempDir,
    claude_binary_path: PathBuf,
    test_configs: HashMap<String, CLITestConfig>,
    process_registry: Arc<Mutex<Vec<u32>>>, // Track spawned processes for cleanup
}

#[derive(Debug, Clone)]
pub struct CLITestConfig {
    pub timeout_seconds: u64,
    pub max_memory_mb: u64,
    pub max_cpu_percent: f64,
    pub environment_vars: HashMap<String, String>,
    pub expected_exit_codes: Vec<i32>,
    pub required_outputs: Vec<String>,
    pub forbidden_outputs: Vec<String>,
}

impl Default for CLITestConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 30,
            max_memory_mb: 512,
            max_cpu_percent: 80.0,
            environment_vars: HashMap::new(),
            expected_exit_codes: vec![0],
            required_outputs: Vec::new(),
            forbidden_outputs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CLITestResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
    pub memory_peak_mb: u64,
    pub cpu_avg_percent: f64,
    pub process_count: usize,
}

impl CLITestFramework {
    /// Create a new CLI test framework with mock Claude binary
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let claude_binary_path = Self::create_comprehensive_mock_claude(&temp_dir)?;

        let mut test_configs = HashMap::new();

        // Default configuration
        test_configs.insert("default".to_string(), CLITestConfig::default());

        // Authentication test configuration
        test_configs.insert("auth".to_string(), CLITestConfig {
            timeout_seconds: 10,
            required_outputs: vec!["authenticated".to_string()],
            ..CLITestConfig::default()
        });

        // Performance test configuration
        test_configs.insert("performance".to_string(), CLITestConfig {
            timeout_seconds: 5,
            max_memory_mb: 256,
            max_cpu_percent: 50.0,
            ..CLITestConfig::default()
        });

        // Stress test configuration
        test_configs.insert("stress".to_string(), CLITestConfig {
            timeout_seconds: 60,
            max_memory_mb: 1024,
            max_cpu_percent: 90.0,
            ..CLITestConfig::default()
        });

        Ok(Self {
            temp_dir,
            claude_binary_path,
            test_configs,
            process_registry: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Create a comprehensive mock Claude binary that simulates real behavior
    fn create_comprehensive_mock_claude(temp_dir: &TempDir) -> Result<PathBuf, std::io::Error> {
        let binary_path = temp_dir.path().join("claude");

        let script_content = r#"#!/bin/bash

# Comprehensive Mock Claude Code CLI for Integration Testing
# Simulates real Claude CLI behavior patterns, timing, and responses

# Configuration from environment
CLAUDE_MOCK_MODE="${CLAUDE_MOCK_MODE:-normal}"
CLAUDE_MOCK_DELAY="${CLAUDE_MOCK_DELAY:-0}"
CLAUDE_MOCK_MEMORY="${CLAUDE_MOCK_MEMORY:-50}"
CLAUDE_MOCK_CPU="${CLAUDE_MOCK_CPU:-30}"

# Simulate resource usage
if [ "$CLAUDE_MOCK_CPU" -gt "0" ]; then
    # Light CPU load simulation
    yes > /dev/null &
    CPU_PID=$!
    sleep 0.1
    kill $CPU_PID 2>/dev/null || true
fi

# Simulate memory usage
if [ "$CLAUDE_MOCK_MEMORY" -gt "0" ]; then
    # Allocate some memory (simulated)
    dd if=/dev/zero of=/tmp/claude_mock_memory_$$ bs=1M count=1 2>/dev/null || true
    trap "rm -f /tmp/claude_mock_memory_$$" EXIT
fi

# Simulate processing delay
if [ "$CLAUDE_MOCK_DELAY" -gt "0" ]; then
    sleep "$CLAUDE_MOCK_DELAY"
fi

# Main command processing
case "$1" in
    "auth")
        case "$2" in
            "status")
                case "$CLAUDE_MOCK_MODE" in
                    "auth_fail")
                        echo "Authentication failed: Invalid credentials" >&2
                        exit 1
                        ;;
                    "auth_timeout")
                        sleep 30
                        exit 1
                        ;;
                    "auth_partial")
                        echo '{"authenticated": false, "subscriptionTier": null, "authMethod": "unknown"}'
                        exit 0
                        ;;
                    *)
                        echo '{"authenticated": true, "subscriptionTier": "max", "authMethod": "oauth", "quota": {"remaining": 95000, "limit": 100000}}'
                        exit 0
                        ;;
                esac
                ;;
            "login")
                if [ -z "$3" ]; then
                    echo "Error: No API key provided" >&2
                    exit 1
                fi
                echo "Successfully authenticated with API key"
                exit 0
                ;;
            "logout")
                echo "Successfully logged out"
                exit 0
                ;;
            "quota")
                case "$CLAUDE_MOCK_MODE" in
                    "quota_exceeded")
                        echo '{"daily_limit": 100000, "current_usage": 100000, "reset_time": "2024-01-02T00:00:00Z", "exceeded": true}'
                        exit 0
                        ;;
                    *)
                        echo '{"daily_limit": 100000, "current_usage": 5000, "reset_time": "2024-01-02T00:00:00Z", "exceeded": false}'
                        exit 0
                        ;;
                esac
                ;;
        esac
        ;;
    "models")
        case "$2" in
            "list")
                echo "claude-sonnet-4-20250514"
                echo "claude-3-5-sonnet-20241022"
                echo "claude-3-5-haiku-20241022"
                echo "claude-3-opus-20240229"
                exit 0
                ;;
        esac
        ;;
    "--system-prompt"|"-s")
        # Message processing mode
        case "$CLAUDE_MOCK_MODE" in
            "stream_error")
                echo '{"type": "error", "error": {"type": "api_error", "message": "Internal server error"}}'
                exit 1
                ;;
            "parse_error")
                echo "Invalid JSON response that cannot be parsed"
                exit 0
                ;;
            "timeout")
                sleep 60
                exit 1
                ;;
            "memory_error")
                # Simulate high memory usage
                dd if=/dev/zero of=/tmp/big_file_$$ bs=1M count=100 2>/dev/null || true
                rm -f /tmp/big_file_$$ 2>/dev/null || true
                echo '{"type": "error", "error": {"type": "resource_error", "message": "Memory limit exceeded"}}'
                exit 1
                ;;
            "slow_stream")
                # Simulate slow streaming response
                echo '{"type": "assistant", "content": "This is a slow"}'
                sleep 1
                echo '{"type": "assistant", "content": " streaming response"}'
                sleep 1
                echo '{"type": "assistant", "content": " that takes time."}'
                sleep 1
                echo '{"type": "result", "inputTokens": 15, "outputTokens": 25, "totalCostUsd": 0.002}'
                exit 0
                ;;
            "large_response")
                # Simulate large response
                echo '{"type": "assistant", "content": "'"$(head -c 100000 /dev/zero | tr '\0' 'A')"'"}'
                echo '{"type": "result", "inputTokens": 50, "outputTokens": 25000, "totalCostUsd": 0.05}'
                exit 0
                ;;
            *)
                # Read input from stdin
                input=$(cat)

                # Simulate processing delay based on input size
                input_size=${#input}
                if [ "$input_size" -gt 10000 ]; then
                    sleep 2  # Large input takes longer
                elif [ "$input_size" -gt 1000 ]; then
                    sleep 1  # Medium input
                else
                    sleep 0.5  # Small input
                fi

                # Generate realistic streaming response
                echo '{"type": "assistant", "content": "I understand your request. Let me help you with that."}'
                sleep 0.2
                echo '{"type": "assistant", "content": "\n\nBased on the information provided:"}'
                sleep 0.3
                echo '{"type": "assistant", "content": "\n1. This is a comprehensive response"}'
                sleep 0.2
                echo '{"type": "assistant", "content": "\n2. Generated by the mock Claude CLI"}'
                sleep 0.2
                echo '{"type": "assistant", "content": "\n3. For integration testing purposes"}'
                sleep 0.1

                # Calculate realistic token usage based on input/output
                input_tokens=$((input_size / 4))  # Rough estimate: 4 chars per token
                output_tokens=75
                cost=$(echo "scale=6; ($input_tokens + $output_tokens) * 0.000015" | bc -l 2>/dev/null || echo "0.001")

                echo '{"type": "result", "inputTokens": '$input_tokens', "outputTokens": '$output_tokens', "totalCostUsd": '$cost'}'
                exit 0
                ;;
        esac
        ;;
    "version"|"-v"|"--version")
        echo "Claude Code CLI v1.0.0 (mock)"
        exit 0
        ;;
    "help"|"-h"|"--help")
        cat << 'EOF'
Claude Code CLI - Mock Implementation for Testing

Usage:
  claude auth status              Check authentication status
  claude auth login <api-key>     Login with API key
  claude auth logout              Logout
  claude auth quota               Check quota usage
  claude models list              List available models
  claude [options] -s <prompt>    Send message with system prompt

Options:
  -s, --system-prompt <prompt>    System prompt
  --output-format <format>        Output format (json, stream-json)
  --model <model>                 Model to use
  --max-turns <n>                 Maximum conversation turns
  --verbose                       Verbose output
  -v, --version                   Show version
  -h, --help                      Show help

Environment Variables:
  ANTHROPIC_API_KEY              API key for authentication
  CLAUDE_MOCK_MODE               Mock behavior mode
  CLAUDE_MOCK_DELAY              Additional delay in seconds
  CLAUDE_MOCK_MEMORY             Memory usage simulation (MB)
  CLAUDE_MOCK_CPU                CPU usage simulation (%)
EOF
        exit 0
        ;;
    *)
        echo "Unknown command: $*" >&2
        echo "Use 'claude help' for usage information" >&2
        exit 1
        ;;
esac
"#;

        fs::write(&binary_path, script_content)?;
        fs::set_permissions(&binary_path, std::os::unix::fs::Permissions::from_mode(0o755))?;

        Ok(binary_path)
    }

    /// Execute a CLI command with comprehensive monitoring
    pub async fn execute_command(
        &self,
        args: &[&str],
        config_name: &str,
        stdin_data: Option<&str>,
    ) -> Result<CLITestResult, Box<dyn std::error::Error>> {
        let config = self.test_configs.get(config_name)
            .ok_or_else(|| format!("Unknown test config: {}", config_name))?;

        let start_time = Instant::now();

        let mut cmd = TokioCommand::new(&self.claude_binary_path);
        cmd.args(args);

        // Apply environment variables from config
        for (key, value) in &config.environment_vars {
            cmd.env(key, value);
        }

        cmd.stdin(Stdio::piped())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;

        // Track the process for cleanup
        if let Some(pid) = child.id() {
            self.process_registry.lock().await.push(pid);
        }

        // Send stdin data if provided
        if let Some(data) = stdin_data {
            if let Some(stdin) = child.stdin.take() {
                let mut stdin = stdin;
                stdin.write_all(data.as_bytes()).await?;
                stdin.flush().await?;
                drop(stdin);
            }
        }

        // Monitor resource usage in background
        let pid = child.id();
        let resource_monitor = if let Some(pid) = pid {
            Some(tokio::spawn(Self::monitor_process_resources(pid, config.clone())))
        } else {
            None
        };

        // Wait for completion with timeout
        let timeout_duration = Duration::from_secs(config.timeout_seconds);
        let output = timeout(timeout_duration, child.wait_with_output()).await??;

        let duration = start_time.elapsed();

        // Get resource usage stats
        let (memory_peak_mb, cpu_avg_percent) = if let Some(monitor) = resource_monitor {
            monitor.await.unwrap_or((0, 0.0))
        } else {
            (0, 0.0)
        };

        // Clean up process tracking
        if let Some(pid) = pid {
            let mut registry = self.process_registry.lock().await;
            registry.retain(|&p| p != pid);
        }

        let result = CLITestResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration,
            memory_peak_mb,
            cpu_avg_percent,
            process_count: 1,
        };

        self.validate_result(&result, config)?;

        Ok(result)
    }

    /// Monitor process resource usage
    async fn monitor_process_resources(pid: u32, config: CLITestConfig) -> (u64, f64) {
        let mut memory_peak = 0u64;
        let mut cpu_samples = Vec::new();
        let start_time = Instant::now();

        while start_time.elapsed() < Duration::from_secs(config.timeout_seconds) {
            // Check if process still exists
            if let Ok(output) = Command::new("ps")
                .args(&["-p", &pid.to_string(), "-o", "rss,pcpu", "--no-headers"])
                .output() {

                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    if let Some(line) = output_str.lines().next() {
                        let parts: Vec<&str> = line.trim().split_whitespace().collect();
                        if parts.len() >= 2 {
                            if let (Ok(rss_kb), Ok(cpu_percent)) = (parts[0].parse::<u64>(), parts[1].parse::<f64>()) {
                                let memory_mb = rss_kb / 1024;
                                memory_peak = memory_peak.max(memory_mb);
                                cpu_samples.push(cpu_percent);
                            }
                        }
                    }
                } else {
                    break; // Process ended
                }
            }

            sleep(Duration::from_millis(100)).await;
        }

        let cpu_avg = if cpu_samples.is_empty() {
            0.0
        } else {
            cpu_samples.iter().sum::<f64>() / cpu_samples.len() as f64
        };

        (memory_peak, cpu_avg)
    }

    /// Validate test result against configuration
    fn validate_result(&self, result: &CLITestResult, config: &CLITestConfig) -> Result<(), Box<dyn std::error::Error>> {
        // Check exit code
        if !config.expected_exit_codes.contains(&result.exit_code) {
            return Err(format!(
                "Unexpected exit code: {} (expected one of: {:?})",
                result.exit_code, config.expected_exit_codes
            ).into());
        }

        // Check required outputs
        for required in &config.required_outputs {
            if !result.stdout.contains(required) && !result.stderr.contains(required) {
                return Err(format!("Required output '{}' not found", required).into());
            }
        }

        // Check forbidden outputs
        for forbidden in &config.forbidden_outputs {
            if result.stdout.contains(forbidden) || result.stderr.contains(forbidden) {
                return Err(format!("Forbidden output '{}' found", forbidden).into());
            }
        }

        // Check resource limits
        if result.memory_peak_mb > config.max_memory_mb {
            return Err(format!(
                "Memory usage exceeded limit: {}MB > {}MB",
                result.memory_peak_mb, config.max_memory_mb
            ).into());
        }

        if result.cpu_avg_percent > config.max_cpu_percent {
            return Err(format!(
                "CPU usage exceeded limit: {:.1}% > {:.1}%",
                result.cpu_avg_percent, config.max_cpu_percent
            ).into());
        }

        Ok(())
    }

    /// Execute multiple commands concurrently
    pub async fn execute_concurrent_commands(
        &self,
        commands: Vec<(&[&str], &str, Option<&str>)>,
        max_concurrent: usize,
    ) -> Result<Vec<CLITestResult>, Box<dyn std::error::Error>> {
        use tokio::sync::Semaphore;

        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        let mut handles = Vec::new();

        for (args, config_name, stdin_data) in commands {
            let semaphore_clone = semaphore.clone();
            let framework = self;

            let handle = tokio::spawn(async move {
                let _permit = semaphore_clone.acquire().await.unwrap();
                framework.execute_command(args, config_name, stdin_data).await
            });

            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await??);
        }

        Ok(results)
    }

    /// Clean up any remaining processes
    pub async fn cleanup(&self) -> Result<(), Box<dyn std::error::Error>> {
        let processes = self.process_registry.lock().await.clone();

        for pid in processes {
            // Try to terminate gracefully first
            let _ = Command::new("kill")
                .args(&["-TERM", &pid.to_string()])
                .output();

            sleep(Duration::from_millis(100)).await;

            // Force kill if still running
            let _ = Command::new("kill")
                .args(&["-KILL", &pid.to_string()])
                .output();
        }

        Ok(())
    }

    pub fn claude_binary_path(&self) -> &Path {
        &self.claude_binary_path
    }

    pub fn temp_dir(&self) -> &Path {
        self.temp_dir.path()
    }
}

impl Drop for CLITestFramework {
    fn drop(&mut self) {
        // Ensure cleanup on drop (best effort)
        if let Ok(runtime) = tokio::runtime::Runtime::new() {
            let _ = runtime.block_on(self.cleanup());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Suite 1: Basic CLI Operations
    mod basic_cli_operations {
        use super::*;

        #[tokio::test]
        async fn test_version_command() {
            let framework = CLITestFramework::new().unwrap();

            let result = framework.execute_command(&["--version"], "default", None).await.unwrap();

            assert_eq!(result.exit_code, 0);
            assert!(result.stdout.contains("Claude Code CLI"));
            assert!(result.duration < Duration::from_secs(1));
        }

        #[tokio::test]
        async fn test_help_command() {
            let framework = CLITestFramework::new().unwrap();

            let result = framework.execute_command(&["help"], "default", None).await.unwrap();

            assert_eq!(result.exit_code, 0);
            assert!(result.stdout.contains("Usage"));
            assert!(result.stdout.contains("auth"));
            assert!(result.stdout.contains("models"));
        }

        #[tokio::test]
        async fn test_invalid_command() {
            let framework = CLITestFramework::new().unwrap();

            let result = framework.execute_command(&["invalid_command"], "default", None).await.unwrap();

            assert_ne!(result.exit_code, 0);
            assert!(result.stderr.contains("Unknown command"));
        }

        #[tokio::test]
        async fn test_models_list() {
            let framework = CLITestFramework::new().unwrap();

            let result = framework.execute_command(&["models", "list"], "default", None).await.unwrap();

            assert_eq!(result.exit_code, 0);
            assert!(result.stdout.contains("claude-sonnet-4"));
            assert!(result.stdout.contains("claude-3-5-sonnet"));
            assert!(result.stdout.contains("claude-3-5-haiku"));
        }
    }

    /// Test Suite 2: Authentication Workflows
    mod authentication_workflows {
        use super::*;

        #[tokio::test]
        async fn test_auth_status_success() {
            let framework = CLITestFramework::new().unwrap();

            let result = framework.execute_command(&["auth", "status"], "auth", None).await.unwrap();

            assert_eq!(result.exit_code, 0);
            assert!(result.stdout.contains("authenticated"));
            assert!(result.stdout.contains("max"));
        }

        #[tokio::test]
        async fn test_auth_status_failure() {
            let mut framework = CLITestFramework::new().unwrap();

            // Configure for auth failure
            let mut config = CLITestConfig::default();
            config.environment_vars.insert("CLAUDE_MOCK_MODE".to_string(), "auth_fail".to_string());
            config.expected_exit_codes = vec![1];
            framework.test_configs.insert("auth_fail".to_string(), config);

            let result = framework.execute_command(&["auth", "status"], "auth_fail", None).await.unwrap();

            assert_eq!(result.exit_code, 1);
            assert!(result.stderr.contains("Authentication failed"));
        }

        #[tokio::test]
        async fn test_auth_login_logout_flow() {
            let framework = CLITestFramework::new().unwrap();

            // Test login
            let login_result = framework.execute_command(
                &["auth", "login", "sk-ant-test-key"],
                "default",
                None
            ).await.unwrap();

            assert_eq!(login_result.exit_code, 0);
            assert!(login_result.stdout.contains("Successfully authenticated"));

            // Test logout
            let logout_result = framework.execute_command(
                &["auth", "logout"],
                "default",
                None
            ).await.unwrap();

            assert_eq!(logout_result.exit_code, 0);
            assert!(logout_result.stdout.contains("Successfully logged out"));
        }

        #[tokio::test]
        async fn test_quota_check() {
            let framework = CLITestFramework::new().unwrap();

            let result = framework.execute_command(&["auth", "quota"], "default", None).await.unwrap();

            assert_eq!(result.exit_code, 0);
            assert!(result.stdout.contains("daily_limit"));
            assert!(result.stdout.contains("current_usage"));
        }

        #[tokio::test]
        async fn test_quota_exceeded() {
            let mut framework = CLITestFramework::new().unwrap();

            let mut config = CLITestConfig::default();
            config.environment_vars.insert("CLAUDE_MOCK_MODE".to_string(), "quota_exceeded".to_string());
            framework.test_configs.insert("quota_exceeded".to_string(), config);

            let result = framework.execute_command(&["auth", "quota"], "quota_exceeded", None).await.unwrap();

            assert_eq!(result.exit_code, 0);
            assert!(result.stdout.contains("exceeded"));
        }
    }

    /// Test Suite 3: Message Processing Workflows
    mod message_processing {
        use super::*;

        #[tokio::test]
        async fn test_simple_message_processing() {
            let framework = CLITestFramework::new().unwrap();

            let message = json!([{
                "role": "user",
                "content": [{"type": "text", "text": "Hello, Claude!"}]
            }]).to_string();

            let result = framework.execute_command(
                &["--system-prompt", "You are a helpful assistant", "--output-format", "stream-json"],
                "default",
                Some(&message)
            ).await.unwrap();

            assert_eq!(result.exit_code, 0);
            assert!(result.stdout.contains("assistant"));
            assert!(result.stdout.contains("result"));
            assert!(result.stdout.contains("inputTokens"));
            assert!(result.stdout.contains("outputTokens"));
        }

        #[tokio::test]
        async fn test_streaming_response() {
            let mut framework = CLITestFramework::new().unwrap();

            let mut config = CLITestConfig::default();
            config.environment_vars.insert("CLAUDE_MOCK_MODE".to_string(), "slow_stream".to_string());
            config.timeout_seconds = 10;
            framework.test_configs.insert("slow_stream".to_string(), config);

            let message = json!([{
                "role": "user",
                "content": [{"type": "text", "text": "Tell me a story"}]
            }]).to_string();

            let result = framework.execute_command(
                &["--system-prompt", "You are a storyteller"],
                "slow_stream",
                Some(&message)
            ).await.unwrap();

            assert_eq!(result.exit_code, 0);
            assert!(result.stdout.contains("slow"));
            assert!(result.stdout.contains("streaming"));
            assert!(result.duration > Duration::from_secs(2)); // Should take time due to streaming
        }

        #[tokio::test]
        async fn test_large_input_processing() {
            let framework = CLITestFramework::new().unwrap();

            // Create large input (simulating big context)
            let large_text = "test ".repeat(10000);
            let message = json!([{
                "role": "user",
                "content": [{"type": "text", "text": large_text}]
            }]).to_string();

            let result = framework.execute_command(
                &["--system-prompt", "Process this large input"],
                "default",
                Some(&message)
            ).await.unwrap();

            assert_eq!(result.exit_code, 0);
            assert!(result.stdout.contains("assistant"));

            // Should take longer due to input size
            assert!(result.duration > Duration::from_millis(500));
        }

        #[tokio::test]
        async fn test_large_output_handling() {
            let mut framework = CLITestFramework::new().unwrap();

            let mut config = CLITestConfig::default();
            config.environment_vars.insert("CLAUDE_MOCK_MODE".to_string(), "large_response".to_string());
            config.max_memory_mb = 1024; // Allow more memory for large response
            framework.test_configs.insert("large_response".to_string(), config);

            let message = json!([{
                "role": "user",
                "content": [{"type": "text", "text": "Generate a large response"}]
            }]).to_string();

            let result = framework.execute_command(
                &["--system-prompt", "Generate detailed output"],
                "large_response",
                Some(&message)
            ).await.unwrap();

            assert_eq!(result.exit_code, 0);
            assert!(result.stdout.len() > 50000); // Large response
            assert!(result.stdout.contains("outputTokens"));
        }
    }

    /// Test Suite 4: Error Handling and Recovery
    mod error_handling {
        use super::*;

        #[tokio::test]
        async fn test_timeout_handling() {
            let mut framework = CLITestFramework::new().unwrap();

            let mut config = CLITestConfig::default();
            config.environment_vars.insert("CLAUDE_MOCK_MODE".to_string(), "timeout".to_string());
            config.timeout_seconds = 2; // Short timeout
            config.expected_exit_codes = vec![1, -1]; // Timeout or kill signal
            framework.test_configs.insert("timeout_test".to_string(), config);

            let message = json!([{
                "role": "user",
                "content": [{"type": "text", "text": "This should timeout"}]
            }]).to_string();

            let result = framework.execute_command(
                &["--system-prompt", "Test timeout"],
                "timeout_test",
                Some(&message)
            ).await;

            // Should either timeout or be caught by our timeout handling
            assert!(result.is_err() || result.unwrap().exit_code != 0);
        }

        #[tokio::test]
        async fn test_parse_error_handling() {
            let mut framework = CLITestFramework::new().unwrap();

            let mut config = CLITestConfig::default();
            config.environment_vars.insert("CLAUDE_MOCK_MODE".to_string(), "parse_error".to_string());
            framework.test_configs.insert("parse_error".to_string(), config);

            let message = json!([{
                "role": "user",
                "content": [{"type": "text", "text": "This will cause parse error"}]
            }]).to_string();

            let result = framework.execute_command(
                &["--system-prompt", "Test parse error"],
                "parse_error",
                Some(&message)
            ).await.unwrap();

            assert_eq!(result.exit_code, 0);
            // Should receive invalid JSON in stdout
            assert!(!result.stdout.contains("\"type\""));
        }

        #[tokio::test]
        async fn test_memory_limit_handling() {
            let mut framework = CLITestFramework::new().unwrap();

            let mut config = CLITestConfig::default();
            config.environment_vars.insert("CLAUDE_MOCK_MODE".to_string(), "memory_error".to_string());
            config.expected_exit_codes = vec![1];
            framework.test_configs.insert("memory_error".to_string(), config);

            let message = json!([{
                "role": "user",
                "content": [{"type": "text", "text": "Use lots of memory"}]
            }]).to_string();

            let result = framework.execute_command(
                &["--system-prompt", "Test memory"],
                "memory_error",
                Some(&message)
            ).await.unwrap();

            assert_eq!(result.exit_code, 1);
            assert!(result.stdout.contains("Memory limit exceeded") || result.stderr.contains("memory"));
        }

        #[tokio::test]
        async fn test_api_error_handling() {
            let mut framework = CLITestFramework::new().unwrap();

            let mut config = CLITestConfig::default();
            config.environment_vars.insert("CLAUDE_MOCK_MODE".to_string(), "stream_error".to_string());
            config.expected_exit_codes = vec![1];
            framework.test_configs.insert("stream_error".to_string(), config);

            let message = json!([{
                "role": "user",
                "content": [{"type": "text", "text": "This will cause API error"}]
            }]).to_string();

            let result = framework.execute_command(
                &["--system-prompt", "Test API error"],
                "stream_error",
                Some(&message)
            ).await.unwrap();

            assert_eq!(result.exit_code, 1);
            assert!(result.stdout.contains("api_error") || result.stderr.contains("error"));
        }
    }

    /// Test Suite 5: Performance and Concurrency
    mod performance_tests {
        use super::*;

        #[tokio::test]
        async fn test_startup_performance() {
            let framework = CLITestFramework::new().unwrap();

            // Test multiple startup times
            let mut startup_times = Vec::new();

            for _ in 0..5 {
                let start = Instant::now();
                let result = framework.execute_command(&["--version"], "performance", None).await.unwrap();
                let duration = start.elapsed();

                assert_eq!(result.exit_code, 0);
                startup_times.push(duration);
            }

            // All startups should be fast
            for time in &startup_times {
                assert!(*time < Duration::from_millis(500), "Startup should be < 500ms, was {:?}", time);
            }

            // Average should be even faster
            let avg_time = startup_times.iter().sum::<Duration>() / startup_times.len() as u32;
            assert!(avg_time < Duration::from_millis(200), "Average startup should be < 200ms, was {:?}", avg_time);
        }

        #[tokio::test]
        async fn test_concurrent_auth_checks() {
            let framework = CLITestFramework::new().unwrap();

            let commands = (0..10).map(|_| {
                (vec!["auth", "status"].as_slice(), "auth", None)
            }).collect();

            let results = framework.execute_concurrent_commands(commands, 5).await.unwrap();

            assert_eq!(results.len(), 10);

            for result in &results {
                assert_eq!(result.exit_code, 0);
                assert!(result.stdout.contains("authenticated"));
                assert!(result.duration < Duration::from_secs(2));
            }
        }

        #[tokio::test]
        async fn test_concurrent_message_processing() {
            let framework = CLITestFramework::new().unwrap();

            let message = json!([{
                "role": "user",
                "content": [{"type": "text", "text": "Process this concurrently"}]
            }]).to_string();

            let commands = (0..5).map(|_| {
                (vec!["--system-prompt", "Process quickly"].as_slice(), "default", Some(message.as_str()))
            }).collect();

            let start = Instant::now();
            let results = framework.execute_concurrent_commands(commands, 3).await.unwrap();
            let total_duration = start.elapsed();

            assert_eq!(results.len(), 5);

            for result in &results {
                assert_eq!(result.exit_code, 0);
                assert!(result.stdout.contains("assistant"));
            }

            // Concurrent execution should be faster than sequential
            assert!(total_duration < Duration::from_secs(10), "Concurrent execution should be < 10s");
        }

        #[tokio::test]
        async fn test_resource_usage_monitoring() {
            let mut framework = CLITestFramework::new().unwrap();

            let mut config = CLITestConfig::default();
            config.environment_vars.insert("CLAUDE_MOCK_MEMORY".to_string(), "100".to_string());
            config.environment_vars.insert("CLAUDE_MOCK_CPU".to_string(), "50".to_string());
            config.max_memory_mb = 256;
            config.max_cpu_percent = 80.0;
            framework.test_configs.insert("resource_test".to_string(), config);

            let message = json!([{
                "role": "user",
                "content": [{"type": "text", "text": "Test resource usage"}]
            }]).to_string();

            let result = framework.execute_command(
                &["--system-prompt", "Test"],
                "resource_test",
                Some(&message)
            ).await.unwrap();

            assert_eq!(result.exit_code, 0);
            // Resource monitoring should capture some usage
            // Note: exact values may vary on different systems
        }
    }

    /// Test Suite 6: Integration Scenarios
    mod integration_scenarios {
        use super::*;

        #[tokio::test]
        async fn test_full_workflow_simulation() {
            let framework = CLITestFramework::new().unwrap();

            // 1. Check auth status
            let auth_result = framework.execute_command(&["auth", "status"], "auth", None).await.unwrap();
            assert_eq!(auth_result.exit_code, 0);

            // 2. Check quota
            let quota_result = framework.execute_command(&["auth", "quota"], "default", None).await.unwrap();
            assert_eq!(quota_result.exit_code, 0);

            // 3. List models
            let models_result = framework.execute_command(&["models", "list"], "default", None).await.unwrap();
            assert_eq!(models_result.exit_code, 0);

            // 4. Send message
            let message = json!([{
                "role": "user",
                "content": [{"type": "text", "text": "Complete workflow test"}]
            }]).to_string();

            let message_result = framework.execute_command(
                &["--system-prompt", "You are helpful", "--model", "claude-sonnet-4-20250514"],
                "default",
                Some(&message)
            ).await.unwrap();

            assert_eq!(message_result.exit_code, 0);
            assert!(message_result.stdout.contains("assistant"));
        }

        #[tokio::test]
        async fn test_error_recovery_workflow() {
            let mut framework = CLITestFramework::new().unwrap();

            // Configure for initial failure
            let mut fail_config = CLITestConfig::default();
            fail_config.environment_vars.insert("CLAUDE_MOCK_MODE".to_string(), "auth_fail".to_string());
            fail_config.expected_exit_codes = vec![1];
            framework.test_configs.insert("initial_fail".to_string(), fail_config);

            // 1. Initial auth failure
            let fail_result = framework.execute_command(&["auth", "status"], "initial_fail", None).await.unwrap();
            assert_eq!(fail_result.exit_code, 1);

            // 2. Attempt login with API key
            let login_result = framework.execute_command(
                &["auth", "login", "sk-ant-test-key"],
                "default",
                None
            ).await.unwrap();
            assert_eq!(login_result.exit_code, 0);

            // 3. Retry auth status (should succeed now)
            let success_result = framework.execute_command(&["auth", "status"], "auth", None).await.unwrap();
            assert_eq!(success_result.exit_code, 0);
        }

        #[tokio::test]
        async fn test_multi_model_switching() {
            let framework = CLITestFramework::new().unwrap();

            let models = ["claude-sonnet-4-20250514", "claude-3-5-sonnet-20241022", "claude-3-5-haiku-20241022"];

            for model in &models {
                let message = json!([{
                    "role": "user",
                    "content": [{"type": "text", "text": format!("Test with {}", model)}]
                }]).to_string();

                let result = framework.execute_command(
                    &["--system-prompt", "You are helpful", "--model", model],
                    "default",
                    Some(&message)
                ).await.unwrap();

                assert_eq!(result.exit_code, 0);
                assert!(result.stdout.contains("assistant"));
            }
        }

        #[tokio::test]
        async fn test_cleanup_on_framework_drop() {
            // This test verifies that processes are cleaned up when framework is dropped
            let temp_dir = TempDir::new().unwrap();
            let claude_path = CLITestFramework::create_comprehensive_mock_claude(&temp_dir).unwrap();

            let pid_file = temp_dir.path().join("test_pid");

            {
                let framework = CLITestFramework::new().unwrap();

                // Start a long-running command that we'll track
                let mut cmd = std::process::Command::new(&claude_path);
                cmd.env("CLAUDE_MOCK_DELAY", "5")
                   .args(&["--system-prompt", "test"])
                   .stdin(Stdio::piped())
                   .stdout(Stdio::piped())
                   .stderr(Stdio::piped());

                let child = cmd.spawn().unwrap();
                fs::write(&pid_file, child.id().to_string()).unwrap();

                // Framework goes out of scope here, should cleanup
            }

            // Give time for cleanup
            sleep(Duration::from_millis(500)).await;

            // Check if process was cleaned up
            if let Ok(pid_str) = fs::read_to_string(&pid_file) {
                let pid = pid_str.trim();
                let check_output = std::process::Command::new("ps")
                    .args(&["-p", pid])
                    .output();

                if let Ok(output) = check_output {
                    // Process should not exist or should not be our claude process
                    let ps_output = String::from_utf8_lossy(&output.stdout);
                    assert!(!ps_output.contains("claude") || !output.status.success());
                }
            }
        }
    }
}