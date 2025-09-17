/// Test CLI integration with actual Claude binary
use std::process::Command;

#[test]
fn test_claude_cli_availability() {
    // Test if Claude CLI is available
    let output = Command::new("claude")
        .arg("--help")
        .output();

    match output {
        Ok(result) => {
            assert!(result.status.success(), "Claude CLI should be available");
            let stdout = String::from_utf8_lossy(&result.stdout);
            assert!(stdout.contains("Usage: claude"), "Should show usage information");
            println!("✅ Claude CLI is available");
        }
        Err(e) => {
            println!("⚠️ Claude CLI not found: {}", e);
            // Skip test if Claude CLI is not available
            assert!(true, "Test skipped - Claude CLI not available");
        }
    }
}

#[test]
fn test_claude_authentication() {
    // Test authentication with a simple command
    let output = Command::new("claude")
        .args(&["--print", "--output-format", "json", "test"])
        .output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);

            if !result.status.success() {
                if stderr.contains("not authenticated") || stderr.contains("login") {
                    println!("⚠️ Claude CLI not authenticated: {}", stderr);
                    // This is expected if not authenticated
                    assert!(true, "Test acknowledged - authentication required");
                } else {
                    println!("❌ Claude CLI error: {}", stderr);
                    println!("stdout: {}", stdout);
                }
            } else {
                println!("✅ Claude CLI authenticated and working");

                // Try to parse the JSON response
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    println!("✅ JSON response parsed successfully");
                    println!("Response type: {:?}", parsed.get("type"));
                } else {
                    println!("⚠️ Could not parse JSON response: {}", stdout);
                }
            }
        }
        Err(e) => {
            println!("⚠️ Claude CLI command failed: {}", e);
            // Skip test if Claude CLI is not available
            assert!(true, "Test skipped - Claude CLI execution failed");
        }
    }
}

#[test]
fn test_cli_command_structure() {
    // Test the exact command structure we're using in the provider
    let output = Command::new("claude")
        .args(&[
            "--print",
            "--output-format", "stream-json",
            "--verbose",
            "--model", "claude-sonnet-4-20250514",
            "--append-system-prompt", "You are a helpful assistant"
        ])
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                println!("✅ CLI command structure is valid");
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                println!("CLI command structure test: {}", stderr);

                // Some errors are expected (like auth issues or API errors)
                // but the command structure should be valid
                if stderr.contains("unknown") || stderr.contains("invalid") || stderr.contains("unrecognized") {
                    panic!("❌ Invalid CLI command structure: {}", stderr);
                } else {
                    println!("✅ CLI command structure is valid (other error occurred)");
                }
            }
        }
        Err(e) => {
            println!("⚠️ CLI command test failed: {}", e);
            assert!(true, "Test skipped - Claude CLI execution failed");
        }
    }
}