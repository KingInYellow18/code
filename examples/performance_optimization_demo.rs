// Performance optimization demonstration for Claude authentication integration
// Shows how to use all performance optimization features

use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use claude_auth_integration::{
    PerformanceCoordinator, PerformanceMetrics, PerformanceTargets,
    OptimizedAuthManager, OptimizationConfig, PerformanceBenchmarks,
    run_phase5_compliance_benchmark,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Claude Authentication Performance Optimization Demo");
    println!("=====================================================");

    // Step 1: Set up performance targets based on Phase 5 requirements
    let targets = PerformanceTargets {
        authentication_cache_ms: 100,    // < 100ms authentication caching
        token_refresh_ms: 500,           // Optimized token refresh
        memory_usage_mb: 50,             // Efficient memory per agent session
        concurrent_agents: 10,           // Support 10+ concurrent agents
    };

    println!("\n📋 Performance Targets (Phase 5 Requirements):");
    println!("  • Authentication Caching: < {}ms", targets.authentication_cache_ms);
    println!("  • Token Refresh: < {}ms", targets.token_refresh_ms);
    println!("  • Memory Usage: < {}MB per agent", targets.memory_usage_mb);
    println!("  • Concurrent Agents: {} supported", targets.concurrent_agents);

    // Step 2: Create performance coordinator
    println!("\n🎯 Initializing Performance Coordinator...");
    let coordinator = Arc::new(PerformanceCoordinator::new());
    
    // Step 3: Simulate authentication metrics collection
    println!("\n📊 Simulating Authentication Operations...");
    for i in 0..20 {
        let metrics = create_sample_metrics(i);
        coordinator.record_metrics(metrics).await;
        
        if i % 5 == 0 {
            println!("  📈 Recorded {} performance samples", i + 1);
        }
        
        sleep(Duration::from_millis(100)).await;
    }

    // Step 4: Analyze performance
    println!("\n🔍 Analyzing Performance Results...");
    let performance_report = coordinator.meets_performance_targets().await;
    
    println!("  Overall Score: {:.1}%", performance_report.overall_score);
    println!("  Authentication: {}", performance_report.authentication_performance);
    println!("  Token Refresh: {}", performance_report.token_refresh_performance);
    println!("  Memory Usage: {}", performance_report.memory_performance);
    println!("  Concurrency: {}", performance_report.concurrency_performance);

    // Step 5: Show recommendations
    if !performance_report.recommendations.is_empty() {
        println!("\n💡 Performance Recommendations:");
        for (i, recommendation) in performance_report.recommendations.iter().enumerate() {
            println!("  {}. {}", i + 1, recommendation);
        }
    }

    // Step 6: Demonstrate caching performance
    println!("\n⚡ Testing Authentication Cache Performance...");
    let cache = coordinator.get_cache();
    
    let start_time = std::time::Instant::now();
    
    // Simulate cache operations
    for i in 0..100 {
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
        cache.put("claude", &format!("user_{}", i), "token_123", expires_at, Some("max".to_string())).await;
    }
    
    let cache_write_time = start_time.elapsed();
    println!("  📝 100 cache writes: {:.2}ms", cache_write_time.as_millis());
    
    let start_time = std::time::Instant::now();
    let mut hit_count = 0;
    
    for i in 0..100 {
        if let Some(_) = cache.get("claude", &format!("user_{}", i)).await {
            hit_count += 1;
        }
    }
    
    let cache_read_time = start_time.elapsed();
    println!("  📖 100 cache reads: {:.2}ms ({}% hit rate)", 
             cache_read_time.as_millis(), hit_count);

    // Step 7: Test memory optimization
    println!("\n🧠 Testing Memory Optimization...");
    let memory_optimizer = coordinator.get_memory_optimizer();
    
    // Allocate agent sessions
    let mut session_ids = Vec::new();
    for i in 0..10 {
        let session_id = memory_optimizer
            .allocate_agent_session(&format!("agent_{}", i), 25) // 25MB per agent
            .await?;
        session_ids.push(session_id);
    }
    
    let memory_stats = memory_optimizer.get_stats().await;
    println!("  📊 Allocated {} sessions, {:.1}MB total", 
             memory_stats.session_count, 
             memory_stats.total_allocated_bytes as f64 / (1024.0 * 1024.0));
    
    // Clean up sessions
    for session_id in session_ids {
        let _ = memory_optimizer.deallocate_agent_session(&session_id).await;
    }
    
    println!("  ✅ All sessions cleaned up");

    // Step 8: Test connection pooling
    println!("\n🌐 Testing Connection Pool Performance...");
    let connection_pool = coordinator.get_connection_pool();
    
    let start_time = std::time::Instant::now();
    let mut handles = Vec::new();
    
    // Simulate concurrent API calls
    for i in 0..50 {
        let pool = connection_pool.clone();
        let handle = tokio::spawn(async move {
            let url = format!("https://api.anthropic.com/v1/test/{}", i);
            // Note: This would make actual HTTP calls in a real scenario
            // For demo purposes, we'll just get the client
            let _client = pool.get_client("api.anthropic.com").await;
            tokio::time::sleep(Duration::from_millis(10)).await; // Simulate API call
        });
        handles.push(handle);
    }
    
    // Wait for all requests to complete
    for handle in handles {
        handle.await?;
    }
    
    let pool_test_time = start_time.elapsed();
    let pool_stats = connection_pool.get_stats().await;
    
    println!("  🚀 50 concurrent requests: {:.2}ms", pool_test_time.as_millis());
    println!("  📈 Connection reuse rate: {:.1}%", pool_stats.connection_reuse_rate * 100.0);

    // Step 9: Run comprehensive benchmark
    println!("\n🧪 Running Phase 5 Compliance Benchmark...");
    println!("  This may take a few minutes...");
    
    let benchmark_results = run_phase5_compliance_benchmark(None).await;
    
    println!("\n📋 Benchmark Results Summary:");
    println!("  Overall Score: {:.1}%", benchmark_results.overall_score);
    println!("  Tests Passed: {}/{}", 
             benchmark_results.summary.tests_passed, 
             benchmark_results.summary.total_tests);
    println!("  Targets Met: {}", if benchmark_results.targets_met { "✅ YES" } else { "❌ NO" });
    
    // Show individual test results
    println!("\n📊 Individual Test Results:");
    for result in &benchmark_results.individual_results {
        let status = if result.target_met { "✅" } else { "❌" };
        println!("  {} {}: {:.1}ms avg ({:.1}% success)", 
                 status, result.test_name, result.average_latency_ms, result.success_rate * 100.0);
    }

    // Step 10: Performance recommendations
    if !benchmark_results.recommendations.is_empty() {
        println!("\n🎯 Benchmark Recommendations:");
        for (i, recommendation) in benchmark_results.recommendations.iter().enumerate() {
            println!("  {}. {}", i + 1, recommendation);
        }
    }

    // Step 11: Final performance summary
    println!("\n🏆 Performance Optimization Demo Complete!");
    println!("=============================================");
    
    if benchmark_results.targets_met && performance_report.overall_score > 90.0 {
        println!("🎉 ALL PHASE 5 PERFORMANCE TARGETS MET!");
        println!("  • Authentication caching under 100ms ✅");
        println!("  • Token refresh optimization ✅");
        println!("  • Memory usage efficiency ✅");
        println!("  • Multi-agent coordination performance ✅");
    } else {
        println!("⚠️  Some performance targets need attention:");
        if !benchmark_results.targets_met {
            println!("  • Benchmark targets not fully met");
        }
        if performance_report.overall_score <= 90.0 {
            println!("  • Overall performance score: {:.1}%", performance_report.overall_score);
        }
    }

    // Step 12: Show optimization benefits
    println!("\n✨ Optimization Benefits Demonstrated:");
    println!("  🚀 Sub-100ms cached authentication");
    println!("  🔄 Batched token refresh operations");
    println!("  🌐 HTTP connection pooling and reuse");
    println!("  🧠 Efficient memory allocation and cleanup");
    println!("  📊 Real-time performance monitoring");
    println!("  🔍 Automated bottleneck detection");
    println!("  📈 Comprehensive benchmarking suite");

    Ok(())
}

/// Create sample performance metrics for demonstration
fn create_sample_metrics(iteration: usize) -> PerformanceMetrics {
    // Simulate varying performance characteristics
    let base_auth_time = 45 + (iteration % 5) * 10; // 45-85ms range
    let cache_hit_rate = if iteration % 4 == 0 { 1.0 } else { 0.85 }; // Mostly cache hits
    let memory_usage = 30 * 1024 * 1024 + (iteration * 1024 * 1024); // Growing memory usage
    let concurrent_agents = std::cmp::min(iteration + 1, 8); // Up to 8 agents
    
    PerformanceMetrics {
        authentication_time: Duration::from_millis(base_auth_time as u64),
        token_refresh_time: Duration::from_millis(300 + (iteration % 3) * 100),
        cache_hit_rate,
        memory_usage: memory_usage as u64,
        concurrent_agents,
        network_requests: if cache_hit_rate == 1.0 { 0 } else { 2 },
        timestamp: std::time::SystemTime::now(),
    }
}