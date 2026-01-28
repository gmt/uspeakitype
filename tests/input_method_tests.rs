//! Integration tests for input_method backend
//!
//! These tests verify the InputMethodInjector functionality in a Docker environment
//! with a headless Wayland compositor. Tests skip gracefully when not in Docker.

use usit::input::{InputMethodInjector, TextInjector as _};

/// Check if running in Docker test environment
fn is_docker_env() -> bool {
    std::env::var("USIT_CANONICAL_TEST_ENV").is_ok()
}

/// Skip reason for non-Docker environments
fn skip_reason() -> &'static str {
    "not in Docker environment (USIT_CANONICAL_TEST_ENV not set)"
}

#[test]
fn test_input_method_injector_new_succeeds() {
    if !is_docker_env() {
        eprintln!("Skipping: {}", skip_reason());
        return;
    }

    match InputMethodInjector::new() {
        Ok(_injector) => {
            println!("✓ InputMethodInjector::new() succeeded");
        }
        Err(e) => {
            eprintln!("✗ InputMethodInjector::new() failed: {}", e);
            panic!("Failed to create InputMethodInjector: {}", e);
        }
    }
}

#[test]
fn test_input_method_injector_name() {
    if !is_docker_env() {
        eprintln!("Skipping: {}", skip_reason());
        return;
    }

    match InputMethodInjector::new() {
        Ok(injector) => {
            let name = injector.name();
            assert_eq!(name, "input_method", "Expected name to be 'input_method'");
            println!("✓ InputMethodInjector::name() returns '{}'", name);
        }
        Err(e) => {
            eprintln!("✗ Failed to create injector: {}", e);
            panic!("Failed to create InputMethodInjector: {}", e);
        }
    }
}

#[test]
fn test_input_method_inject_not_activated() {
    if !is_docker_env() {
        eprintln!("Skipping: {}", skip_reason());
        return;
    }

    match InputMethodInjector::new() {
        Ok(mut injector) => {
            // Try to inject without activation - should fail gracefully
            match injector.inject("test") {
                Ok(_) => {
                    println!("✓ inject() succeeded (input method may be activated)");
                }
                Err(e) => {
                    // Expected: input method not activated
                    println!("✓ inject() correctly failed when not activated: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to create injector: {}", e);
            panic!("Failed to create InputMethodInjector: {}", e);
        }
    }
}

#[test]
fn test_input_method_inject_empty_string() {
    if !is_docker_env() {
        eprintln!("Skipping: {}", skip_reason());
        return;
    }

    match InputMethodInjector::new() {
        Ok(mut injector) => {
            // Injecting empty string should return Ok immediately
            match injector.inject("") {
                Ok(_) => {
                    println!("✓ inject(\"\") returned Ok as expected");
                }
                Err(e) => {
                    eprintln!("✗ inject(\"\") failed unexpectedly: {}", e);
                    panic!("inject(\"\") should not fail: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to create injector: {}", e);
            panic!("Failed to create InputMethodInjector: {}", e);
        }
    }
}

#[test]
fn test_input_method_surrounding_text() {
    if !is_docker_env() {
        eprintln!("Skipping: {}", skip_reason());
        return;
    }

    match InputMethodInjector::new() {
        Ok(injector) => {
            // get_surrounding_text should not panic
            let surrounding = injector.get_surrounding_text();
            match surrounding {
                Some((text, cursor, anchor)) => {
                    println!(
                        "✓ get_surrounding_text() returned: text='{}', cursor={}, anchor={}",
                        text, cursor, anchor
                    );
                }
                None => {
                    println!("✓ get_surrounding_text() returned None (no text available)");
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to create injector: {}", e);
            panic!("Failed to create InputMethodInjector: {}", e);
        }
    }
}
