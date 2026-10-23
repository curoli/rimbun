use std::process::Command;

#[test]
fn json_status_setup_failure_is_one_sanitized_document_with_matching_exit_code() {
    let profile = format!("missing-{}-secret-marker", uuid::Uuid::new_v4());
    let output = Command::new(env!("CARGO_BIN_EXE_rimbunctl"))
        .args([&profile, "status", "--json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["overall"], "error");
    assert_eq!(json["exit_code"], output.status.code().unwrap());
    assert_eq!(json["error"]["code"], "status_setup_failed");
    assert!(!output.status.success());
    assert!(
        !String::from_utf8(output.stdout)
            .unwrap()
            .contains("secret-marker")
    );
    assert!(
        !String::from_utf8(output.stderr)
            .unwrap()
            .contains("secret-marker")
    );
}
