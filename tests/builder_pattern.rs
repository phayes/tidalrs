//! Tests for the TidalClient builder pattern functionality.
//!
//! This module tests that all the "with_*" builder methods work correctly
//! and that the client can be configured using the fluent builder pattern.

use std::sync::Arc;
use tidalrs::{Authz, DeviceType, TidalApiError, TidalClient};

#[test]
fn test_builder_pattern_basic() {
    // Test basic client creation
    let client = TidalClient::new("test_client_id".to_string());

    // Verify default values
    assert_eq!(client.get_country_code(), "US");
    assert_eq!(client.get_locale(), "en_US");
    assert_eq!(client.get_device_type(), DeviceType::Browser);
    assert_eq!(client.get_user_id(), None);
}

#[test]
fn test_builder_pattern_with_country_code() {
    let client = TidalClient::new("test_client_id".to_string()).with_country_code("GB".to_string());

    assert_eq!(client.get_country_code(), "GB");
    assert_eq!(client.get_locale(), "en_US"); // Should still be default
    assert_eq!(client.get_device_type(), DeviceType::Browser); // Should still be default
}

#[test]
fn test_builder_pattern_with_locale() {
    let client = TidalClient::new("test_client_id".to_string()).with_locale("en_GB".to_string());

    assert_eq!(client.get_country_code(), "US"); // Should still be default
    assert_eq!(client.get_locale(), "en_GB");
    assert_eq!(client.get_device_type(), DeviceType::Browser); // Should still be default
}

#[test]
fn test_builder_pattern_with_device_type() {
    let client =
        TidalClient::new("test_client_id".to_string()).with_device_type(DeviceType::Browser);

    assert_eq!(client.get_country_code(), "US"); // Should still be default
    assert_eq!(client.get_locale(), "en_US"); // Should still be default
    assert_eq!(client.get_device_type(), DeviceType::Browser);
}

#[test]
fn test_builder_pattern_with_authz() {
    let authz = Authz::new(
        "test_access_token".to_string(),
        "test_refresh_token".to_string(),
        12345,
        Some("CA".to_string()),
    );

    let client = TidalClient::new("test_client_id".to_string()).with_authz(authz.clone());

    assert_eq!(client.get_user_id(), Some(12345));
    assert_eq!(client.get_country_code(), "CA"); // Should use authz country code when no explicit setting
    assert_eq!(client.get_locale(), "en_US"); // Should still be default

    // Test that authz is stored correctly
    if let Some(stored_authz) = client.get_authz() {
        assert_eq!(stored_authz.access_token, "test_access_token");
        assert_eq!(stored_authz.refresh_token, "test_refresh_token");
        assert_eq!(stored_authz.user_id, 12345);
        assert_eq!(stored_authz.country_code, Some("CA".to_string()));
    } else {
        panic!("Authz should be stored in client");
    }
}

#[test]
fn test_builder_pattern_with_authz_refresh_callback() {
    let callback_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let callback_called_clone = callback_called.clone();

    let client =
        TidalClient::new("test_client_id".to_string()).with_authz_refresh_callback(move |_authz| {
            callback_called_clone.store(true, std::sync::atomic::Ordering::Relaxed);
        });

    // The callback should be set (we can't easily test it being called without
    // actually triggering a token refresh, which requires network calls)
    // But we can verify the client was created successfully
    assert_eq!(client.get_country_code(), "US");
    assert_eq!(client.get_locale(), "en_US");
    assert_eq!(client.get_device_type(), DeviceType::Browser);
}

#[test]
fn test_builder_pattern_chaining() {
    let authz = Authz::new(
        "test_access_token".to_string(),
        "test_refresh_token".to_string(),
        67890,
        Some("AU".to_string()),
    );

    let callback_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let callback_called_clone = callback_called.clone();

    let client = TidalClient::new("test_client_id".to_string())
        .with_authz(authz)
        .with_country_code("DE".to_string())
        .with_locale("de_DE".to_string())
        .with_device_type(DeviceType::Browser)
        .with_authz_refresh_callback(move |_authz| {
            callback_called_clone.store(true, std::sync::atomic::Ordering::Relaxed);
        });

    // Test that all configurations are applied correctly
    assert_eq!(client.get_user_id(), Some(67890));
    assert_eq!(client.get_country_code(), "DE"); // Explicit setting should override authz
    assert_eq!(client.get_locale(), "de_DE");
    assert_eq!(client.get_device_type(), DeviceType::Browser);

    // Verify authz is stored
    if let Some(stored_authz) = client.get_authz() {
        assert_eq!(stored_authz.user_id, 67890);
        assert_eq!(stored_authz.country_code, Some("AU".to_string()));
    } else {
        panic!("Authz should be stored in client");
    }
}

#[test]
fn test_builder_pattern_country_code_priority() {
    // Test that explicitly set country code takes priority over authz country code
    let authz = Authz::new(
        "test_access_token".to_string(),
        "test_refresh_token".to_string(),
        11111,
        Some("FR".to_string()),
    );

    let client = TidalClient::new("test_client_id".to_string())
        .with_authz(authz)
        .with_country_code("JP".to_string());

    assert_eq!(client.get_country_code(), "JP"); // Should use explicit setting
}

#[test]
fn test_builder_pattern_country_code_fallback() {
    // Test that authz country code is used when no explicit country code is set
    let authz = Authz::new(
        "test_access_token".to_string(),
        "test_refresh_token".to_string(),
        22222,
        Some("IT".to_string()),
    );

    let client = TidalClient::new("test_client_id".to_string()).with_authz(authz);

    assert_eq!(client.get_country_code(), "IT"); // Should use authz country code
}

#[test]
fn test_builder_pattern_country_code_final_fallback() {
    // Test that "US" is used as final fallback when no country code is available
    let authz = Authz::new(
        "test_access_token".to_string(),
        "test_refresh_token".to_string(),
        33333,
        None, // No country code in authz
    );

    let client = TidalClient::new("test_client_id".to_string()).with_authz(authz);

    assert_eq!(client.get_country_code(), "US"); // Should use final fallback
}

#[test]
fn test_builder_pattern_with_client() {
    // Test the with_client method
    let custom_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    let client = TidalClient::new("test_client_id".to_string()).with_client(custom_client);

    // The client should be created successfully
    assert_eq!(client.get_country_code(), "US");
    assert_eq!(client.get_locale(), "en_US");
    assert_eq!(client.get_device_type(), DeviceType::Browser);
}

#[test]
fn test_tidal_api_error_deserialization_snake_case() {
    // Test deserialization with snake_case field names
    let json = r#"{
        "status": 400,
        "sub_status": 1001,
        "user_message": "Invalid request"
    }"#;

    let error: TidalApiError = serde_json::from_str(json).unwrap();

    assert_eq!(error.status, 400);
    assert_eq!(error.sub_status, 1001);
    assert_eq!(error.user_message, "Invalid request");
}

#[test]
fn test_tidal_api_error_deserialization_camel_case() {
    // Test deserialization with camelCase field names
    let json = r#"{
        "status": 401,
        "subStatus": 2001,
        "userMessage": "Unauthorized access"
    }"#;

    let error: TidalApiError = serde_json::from_str(json).unwrap();

    assert_eq!(error.status, 401);
    assert_eq!(error.sub_status, 2001);
    assert_eq!(error.user_message, "Unauthorized access");
}

#[test]
fn test_tidal_api_error_deserialization_mixed_case() {
    // Test deserialization with mixed field names
    let json = r#"{
        "status": 403,
        "sub_status": 3001,
        "userMessage": "Forbidden access"
    }"#;

    let error: TidalApiError = serde_json::from_str(json).unwrap();

    assert_eq!(error.status, 403);
    assert_eq!(error.sub_status, 3001);
    assert_eq!(error.user_message, "Forbidden access");
}

#[test]
fn test_tidal_api_error_deserialization_missing_user_message() {
    // Test deserialization when user_message/userMessage is missing (should default to empty string)
    let json = r#"{
        "status": 500,
        "sub_status": 4001
    }"#;

    let error: TidalApiError = serde_json::from_str(json).unwrap();

    assert_eq!(error.status, 500);
    assert_eq!(error.sub_status, 4001);
    assert_eq!(error.user_message, "");
}
