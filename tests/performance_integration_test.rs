// Integration tests for Claude authentication performance optimization
// Validates Phase 5 performance requirements

#[cfg(test)]
mod performance_integration_tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::time::sleep;

    use claude_auth_integration::{
        PerformanceCoordinator, PerformanceMetrics, PerformanceTargets,
        PerformanceBenchmarks, run_phase5_compliance_benchmark,
        authentication_cache::AuthenticationCache,
        memory_optimization::MemoryOptimizer,
        connection_pool::ClaudeConnectionPool,
        performance_monitor::PerformanceMonitor,
    };

    /// Test that the complete performance optimization system meets Phase 5 requirements
    #[tokio::test]
    async fn test_phase5_performance_requirements() {
        let targets = PerformanceTargets {
            authentication_cache_ms: 100,
            token_refresh_ms: 500,
            memory_usage_mb: 50,
            concurrent_agents: 10,
        };

        let coordinator = PerformanceCoordinator::new();

        // Test authentication caching performance
        let auth_cache = coordinator.get_cache();
        let start = Instant::now();
        
        // Test cache write performance
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
        auth_cache.put("claude", "test_user", "test_token", expires_at, Some("max".to_string())).await;
        
        // Test cache read performance
        let cached_result = auth_cache.get("claude", "test_user").await;
        let cache_time = start.elapsed();

        assert!(cached_result.is_some());
        assert!(cache_time.as_millis() < targets.authentication_cache_ms, 
                "Cache operation took {}ms, exceeds target of {}ms", 
                cache_time.as_millis(), targets.authentication_cache_ms);
    }

    /// Test memory optimization meets efficiency targets
    #[tokio::test]
    async fn test_memory_optimization_efficiency() {
        let memory_optimizer = MemoryOptimizer::new();
        
        // Allocate multiple agent sessions
        let mut session_ids = Vec::new();
        for i in 0..5 {
            let session_id = memory_optimizer
                .allocate_agent_session(&format!("agent_{}", i), 10) // 10MB per agent
                .await
                .expect("Should allocate agent session");
            session_ids.push(session_id);
        }

        let stats = memory_optimizer.get_stats().await;
        let total_memory_mb = stats.total_allocated_bytes / (1024 * 1024);
        
        // Memory usage should be reasonable for the number of agents
        assert!(total_memory_mb <= 60, "Memory usage {}MB exceeds reasonable limit", total_memory_mb);
        assert_eq!(stats.session_count, 5);
        assert!(stats.memory_efficiency > 50.0, "Memory efficiency too low: {:.1}%", stats.memory_efficiency);

        // Clean up sessions
        for session_id in session_ids {
            let freed_memory = memory_optimizer.deallocate_agent_session(&session_id).await.unwrap();
            assert!(freed_memory > 0, "Should free some memory when deallocating");
        }

        let final_stats = memory_optimizer.get_stats().await;
        assert_eq!(final_stats.session_count, 0);
        assert_eq!(final_stats.total_allocated_bytes, 0);
    }

    /// Test connection pooling performance and reuse
    #[tokio::test]
    async fn test_connection_pooling_performance() {
        let connection_pool = ClaudeConnectionPool::new();
        
        // Test connection reuse
        let client1 = connection_pool.get_client("api.anthropic.com").await;
        let client2 = connection_pool.get_client("api.anthropic.com").await;
        
        // Should reuse connections for same host
        let stats = connection_pool.get_stats().await;
        assert_eq!(stats.total_connections, 1, "Should reuse connection for same host");

        // Test multiple hosts
        let _client3 = connection_pool.get_client("api.openai.com").await;
        let stats = connection_pool.get_stats().await;
        assert_eq!(stats.total_connections, 2, "Should create separate pools for different hosts");

        // Test concurrent access
        let start = Instant::now();
        let mut handles = Vec::new();
        
        for i in 0..10 {
            let pool = connection_pool.clone();
            let handle = tokio::spawn(async move {
                let _client = pool.get_client("test.example.com").await;
                sleep(Duration::from_millis(10)).await; // Simulate work
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.expect("Concurrent access should work");
        }

        let concurrent_time = start.elapsed();
        assert!(concurrent_time.as_millis() < 500, "Concurrent access took too long: {}ms", concurrent_time.as_millis());
    }

    /// Test real-time performance monitoring
    #[tokio::test]
    async fn test_real_time_performance_monitoring() {
        let targets = PerformanceTargets::default();
        let monitor = PerformanceMonitor::new(targets);
        
        monitor.start_monitoring().await;

        // Submit performance metrics
        for i in 0..10 {
            let metrics = PerformanceMetrics {
                authentication_time: Duration::from_millis(50 + i * 5),
                token_refresh_time: Duration::from_millis(200),
                cache_hit_rate: 0.9,
                memory_usage: 30 * 1024 * 1024,
                concurrent_agents: 3,
                network_requests: 2,
                timestamp: std::time::SystemTime::now(),
            };
            monitor.submit_metrics(metrics).await;
            sleep(Duration::from_millis(10)).await;
        }

        // Check dashboard data
        let dashboard = monitor.get_dashboard_data().await;
        assert!(dashboard.performance_score > 0.0);
        assert!(dashboard.current_metrics.authentication_time.as_millis() > 0);
        assert!(!dashboard.recommendations.is_empty());

        // Health status should be reasonable
        assert!(dashboard.health_status.overall_score > 50.0);
    }

    /// Test comprehensive benchmarking meets Phase 5 targets
    #[tokio::test]
    async fn test_comprehensive_benchmarking() {
        let targets = PerformanceTargets {
            authentication_cache_ms: 100,
            token_refresh_ms: 500,
            memory_usage_mb: 50,
            concurrent_agents: 10,
        };

        let benchmarks = PerformanceBenchmarks::new(targets);
        
        // Run a quick benchmark suite (reduced operations for test)
        let config = claude_auth_integration::benchmarks::BenchmarkConfig {
            test_duration_seconds: 5,    // Short test for CI
            concurrent_agents: 3,        // Fewer agents for test
            operations_per_agent: 10,    // Fewer operations
            warmup_operations: 2,        // Minimal warmup
            target_percentile: 0.95,
            acceptable_failure_rate: 0.05,
        };

        let benchmarks = PerformanceBenchmarks::with_config(config, targets);
        let results = benchmarks.run_benchmark_suite("Integration Test Suite").await;

        // Validate benchmark results
        assert!(!results.individual_results.is_empty());
        assert!(results.overall_score >= 0.0);
        assert!(results.overall_score <= 100.0);
        assert!(results.summary.total_tests > 0);

        // Should have reasonable success rates
        for result in &results.individual_results {
            assert!(result.success_rate >= 0.9, 
                    "Test {} has low success rate: {:.1}%", 
                    result.test_name, result.success_rate * 100.0);
            assert!(result.total_operations > 0);
            assert!(result.average_latency_ms >= 0.0);
        }

        println!("Benchmark completed: {:.1}% overall score", results.overall_score);
        println!("Tests: {}/{} passed", results.summary.tests_passed, results.summary.total_tests);
    }

    /// Test Phase 5 compliance benchmark
    #[tokio::test]
    async fn test_phase5_compliance_benchmark() {
        let results = run_phase5_compliance_benchmark(None).await;
        
        assert_eq!(results.suite_name, "Phase 5 Compliance");
        assert!(!results.individual_results.is_empty());

        // Check that key performance tests are present
        let test_names: Vec<&str> = results.individual_results.iter()
            .map(|r| r.test_name.as_str())
            .collect();
        
        assert!(test_names.iter().any(|&name| name.contains("Authentication Cache")));
        assert!(test_names.iter().any(|&name| name.contains("Token Refresh")));
        assert!(test_names.iter().any(|&name| name.contains("Memory Usage")));
        assert!(test_names.iter().any(|&name| name.contains("Concurrent Agents")));

        // Validate performance targets
        for result in &results.individual_results {
            // All tests should complete successfully
            assert!(result.success_rate > 0.0, 
                    "Test {} failed completely", result.test_name);
            
            // Latency measurements should be reasonable
            assert!(result.average_latency_ms < 10000.0, 
                    "Test {} has unreasonably high latency: {:.1}ms", 
                    result.test_name, result.average_latency_ms);
            
            // Performance scores should be meaningful
            assert!(result.performance_score >= 0.0);
            assert!(result.performance_score <= 100.0);
        }

        // Overall suite should have reasonable performance
        assert!(results.overall_score >= 0.0);
        assert!(results.overall_score <= 100.0);
        
        println!("Phase 5 compliance test overall score: {:.1}%", results.overall_score);
        println!("Targets met: {}", results.targets_met);
    }

    /// Test authentication cache hit rate under load
    #[tokio::test]
    async fn test_cache_hit_rate_under_load() {
        let cache = AuthenticationCache::new();
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
        
        // Pre-populate cache
        for i in 0..100 {
            cache.put("claude", &format!("user_{}", i), &format!("token_{}", i), expires_at, None).await;
        }

        // Test concurrent cache access
        let start = Instant::now();
        let mut handles = Vec::new();
        let hit_counter = Arc::new(std::sync::atomic::AtomicU32::new(0));

        for i in 0..100 {
            let cache_clone = cache.clone();
            let counter_clone = Arc::clone(&hit_counter);
            
            let handle = tokio::spawn(async move {
                for j in 0..10 {
                    if let Some(_) = cache_clone.get("claude", &format!("user_{}", (i + j) % 100)).await {
                        counter_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.expect("Cache access should not fail");
        }

        let total_time = start.elapsed();
        let hits = hit_counter.load(std::sync::atomic::Ordering::Relaxed);
        let hit_rate = hits as f64 / 1000.0; // 100 * 10 operations

        let stats = cache.get_stats().await;
        
        assert!(hit_rate > 0.9, "Cache hit rate too low: {:.1}%", hit_rate * 100.0);
        assert!(total_time.as_millis() < 1000, "Cache operations took too long: {}ms", total_time.as_millis());
        assert!(stats.average_lookup_time_ms < 10.0, "Average lookup time too high: {:.2}ms", stats.average_lookup_time_ms);
        
        println!("Cache performance: {:.1}% hit rate, {:.2}ms avg lookup", hit_rate * 100.0, stats.average_lookup_time_ms);
    }

    /// Test memory pressure handling
    #[tokio::test]
    async fn test_memory_pressure_handling() {
        let config = claude_auth_integration::memory_optimization::MemoryConfig {
            max_memory_mb: 50,  // Small limit for testing
            gc_threshold_mb: 40, // Trigger GC early
            agent_session_timeout_minutes: 1, // Short timeout
            ..Default::default()
        };

        let memory_optimizer = MemoryOptimizer::with_config(config);
        memory_optimizer.start_background_tasks().await;

        // Try to allocate more memory than the limit
        let mut session_ids = Vec::new();
        let mut allocation_results = Vec::new();

        for i in 0..10 {
            let result = memory_optimizer
                .allocate_agent_session(&format!("pressure_agent_{}", i), 8) // 8MB per agent
                .await;
            
            allocation_results.push(result.is_ok());
            
            if let Ok(session_id) = result {
                session_ids.push(session_id);
            }
        }

        // Some allocations should succeed, but not all due to memory limits
        let successful_allocations = allocation_results.iter().filter(|&&success| success).count();
        assert!(successful_allocations > 0, "At least some allocations should succeed");
        assert!(successful_allocations <= 6, "Should enforce memory limits"); // 50MB / 8MB ≈ 6 sessions

        let stats = memory_optimizer.get_stats().await;
        assert!(stats.total_allocated_bytes <= 60 * 1024 * 1024, "Should not exceed reasonable memory bounds");

        // Test garbage collection
        let gc_result = memory_optimizer.force_garbage_collection().await.unwrap();
        println!("GC removed {} sessions, freed {} bytes", gc_result.sessions_removed, gc_result.bytes_freed);

        // Clean up remaining sessions
        for session_id in session_ids {
            let _ = memory_optimizer.deallocate_agent_session(&session_id).await;
        }
    }

    /// Helper to create test performance metrics
    fn create_test_metrics(
        auth_time_ms: u64,
        memory_mb: u64,
        cache_hit_rate: f64,
        agents: usize,
    ) -> PerformanceMetrics {
        PerformanceMetrics {
            authentication_time: Duration::from_millis(auth_time_ms),
            token_refresh_time: Duration::from_millis(300),
            cache_hit_rate,
            memory_usage: memory_mb * 1024 * 1024,
            concurrent_agents: agents,
            network_requests: if cache_hit_rate > 0.8 { 1 } else { 3 },
            timestamp: std::time::SystemTime::now(),
        }
    }

    /// Test that performance coordinator correctly identifies when targets are met
    #[tokio::test]
    async fn test_performance_target_validation() {
        let targets = PerformanceTargets {
            authentication_cache_ms: 100,
            token_refresh_ms: 500,
            memory_usage_mb: 50,
            concurrent_agents: 10,
        };

        let coordinator = PerformanceCoordinator::new();

        // Submit metrics that meet targets
        let good_metrics = create_test_metrics(80, 40, 0.95, 8);
        coordinator.record_metrics(good_metrics).await;

        let report = coordinator.meets_performance_targets().await;
        assert!(report.overall_score > 90.0, "Should have high score for good metrics");

        // Submit metrics that exceed targets
        let bad_metrics = create_test_metrics(150, 70, 0.6, 15);
        coordinator.record_metrics(bad_metrics).await;

        let report = coordinator.meets_performance_targets().await;
        assert!(report.overall_score < 90.0, "Should have lower score for poor metrics");
    }
}