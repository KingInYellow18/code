//! Test Runner for Claude Authentication Integration Tests
//! 
//! This module provides utilities to run the comprehensive integration test suite
//! and collect results for reporting.

use std::process::Stdio;
use tokio::process::Command;
use serde_json::json;
use chrono::Utc;

#[derive(Debug)]
pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    pub duration: std::time::Duration,
    pub output: String,
    pub error: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
}

pub struct IntegrationTestRunner {
    pub results: Vec<TestResult>,
    pub start_time: std::time::Instant,
}

impl IntegrationTestRunner {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            start_time: std::time::Instant::now(),
        }
    }

    /// Run a single test and capture its result
    pub async fn run_test(&mut self, test_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🧪 Running test: {}", test_name);
        let test_start = std::time::Instant::now();

        let mut cmd = Command::new("cargo");
        cmd.args(&["test", test_name, "--", "--nocapture"])
            .current_dir("/home/kinginyellow/projects/code")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await?;
        let duration = test_start.elapsed();

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let status = if output.status.success() {
            TestStatus::Passed
        } else {
            TestStatus::Failed
        };

        let result = TestResult {
            name: test_name.to_string(),
            status,
            duration,
            output: stdout.to_string(),
            error: if stderr.is_empty() { None } else { Some(stderr.to_string()) },
        };

        println!("  {} Test {} completed in {:?}", 
            if result.status == TestStatus::Passed { "✅" } else { "❌" },
            test_name, 
            duration
        );

        self.results.push(result);
        Ok(())
    }

    /// Run all critical integration tests
    pub async fn run_all_tests(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let critical_tests = vec![
            "test_claude_openai_fallback",
            "test_multi_agent_quota_management", 
            "test_provider_switching",
            "test_agent_environment_setup",
            "test_error_handling",
            "test_backward_compatibility",
            "test_performance_benchmarks",
        ];

        println!("🚀 Starting Comprehensive Integration Test Suite");
        println!("=" .repeat(80));

        for test_name in critical_tests {
            if let Err(e) = self.run_test(test_name).await {
                eprintln!("❌ Failed to run test {}: {}", test_name, e);
                // Continue with other tests even if one fails to run
            }
        }

        Ok(())
    }

    /// Generate comprehensive test report
    pub fn generate_report(&self) -> serde_json::Value {
        let total_duration = self.start_time.elapsed();
        let passed_count = self.results.iter().filter(|r| r.status == TestStatus::Passed).count();
        let failed_count = self.results.iter().filter(|r| r.status == TestStatus::Failed).count();
        let skipped_count = self.results.iter().filter(|r| r.status == TestStatus::Skipped).count();

        let test_details: Vec<serde_json::Value> = self.results.iter().map(|result| {
            json!({
                "name": result.name,
                "status": match result.status {
                    TestStatus::Passed => "PASSED",
                    TestStatus::Failed => "FAILED", 
                    TestStatus::Skipped => "SKIPPED",
                },
                "duration_ms": result.duration.as_millis(),
                "output_lines": result.output.lines().count(),
                "has_error": result.error.is_some(),
                "error_message": result.error.as_deref().unwrap_or("")
            })
        }).collect();

        json!({
            "test_suite": "Claude Authentication Integration Tests",
            "phase": "Phase 3: Claude-Code Integration",
            "execution_timestamp": Utc::now().to_rfc3339(),
            "total_duration_seconds": total_duration.as_secs_f64(),
            "summary": {
                "total_tests": self.results.len(),
                "passed": passed_count,
                "failed": failed_count,
                "skipped": skipped_count,
                "success_rate": if self.results.is_empty() { 0.0 } else { 
                    (passed_count as f64) / (self.results.len() as f64) * 100.0 
                }
            },
            "success_criteria_met": failed_count == 0,
            "performance_benchmarks": {
                "authentication_time_requirement": "< 100ms",
                "quota_operations_requirement": "< 1000ms for 10 operations",
                "provider_switching_requirement": "< 100ms for 10 switches"
            },
            "test_details": test_details,
            "phase_requirements_validated": [
                "Agent Authentication Flow",
                "Multi-Agent Quota Sharing", 
                "Provider Switching",
                "Error Handling",
                "Backward Compatibility",
                "Performance Benchmarks"
            ]
        })
    }

    /// Print human-readable test summary
    pub fn print_summary(&self) {
        let total_duration = self.start_time.elapsed();
        let passed = self.results.iter().filter(|r| r.status == TestStatus::Passed).count();
        let failed = self.results.iter().filter(|r| r.status == TestStatus::Failed).count();

        println!("\n" + &"=".repeat(80));
        println!("📋 INTEGRATION TEST RESULTS SUMMARY");
        println!("=" .repeat(80));
        
        for result in &self.results {
            let status_icon = match result.status {
                TestStatus::Passed => "✅",
                TestStatus::Failed => "❌", 
                TestStatus::Skipped => "⏭️",
            };
            
            let status_text = match result.status {
                TestStatus::Passed => "PASSED",
                TestStatus::Failed => "FAILED",
                TestStatus::Skipped => "SKIPPED",
            };

            println!("{} {:<40} {:>15} ({:?})", 
                status_icon, 
                result.name, 
                status_text, 
                result.duration
            );

            if result.status == TestStatus::Failed {
                if let Some(error) = &result.error {
                    let error_preview = error.lines().take(2).collect::<Vec<_>>().join(" ");
                    println!("    💥 Error: {}", 
                        if error_preview.len() > 100 { 
                            format!("{}...", &error_preview[..97])
                        } else { 
                            error_preview 
                        }
                    );
                }
            }
        }
        
        println!("=" .repeat(80));
        println!("📊 Summary: {} passed, {} failed", passed, failed);
        println!("⏱️  Total execution time: {:?}", total_duration);
        
        if failed == 0 {
            println!("🎉 All integration tests passed! Phase 3 requirements validated.");
        } else {
            println!("⚠️  {} test(s) failed. Review errors above.", failed);
        }
    }
}

/// Utility function to run just the critical tests specified in the plan
pub async fn run_critical_tests() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut runner = IntegrationTestRunner::new();
    runner.run_all_tests().await?;
    
    let report = runner.generate_report();
    runner.print_summary();
    
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runner_initialization() {
        let runner = IntegrationTestRunner::new();
        assert_eq!(runner.results.len(), 0);
    }

    #[test] 
    fn test_report_generation() {
        let mut runner = IntegrationTestRunner::new();
        
        // Add a mock test result
        runner.results.push(TestResult {
            name: "mock_test".to_string(),
            status: TestStatus::Passed,
            duration: std::time::Duration::from_millis(150),
            output: "Test output".to_string(),
            error: None,
        });

        let report = runner.generate_report();
        assert_eq!(report["summary"]["total_tests"], 1);
        assert_eq!(report["summary"]["passed"], 1);
        assert_eq!(report["summary"]["failed"], 0);
    }
}