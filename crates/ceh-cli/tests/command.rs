use std::process::Command;
#[test]
fn replay_command_outputs_offline_report_and_rejects_unknown_fields() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.json");
    std::fs::write(&input,r#"{"schema_version":1,"profile":"fixture","session":"example","wall_start_millis":1793361600000,"steps":[{"event":"observe","at_ms":0,"observation":{"document":"slot","kind":"booking"}}]}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ceh-cli"))
        .arg("replay")
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["offline"], true);
    assert_eq!(report["native_vault_accessed"], false);
    std::fs::write(&input, r#"{"secret":"DO-NOT-ECHO"}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ceh-cli"))
        .arg("replay")
        .arg(&input)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("DO-NOT-ECHO"));
}

#[test]
fn output_alias_cannot_overwrite_database_or_input() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.json");
    let database = directory.path().join("history.sqlite");
    let payload = r#"{"schema_version":1,"profile":"fixture","session":"example","wall_start_millis":1793361600000,"steps":[]}"#;
    std::fs::write(&input, payload).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ceh-cli"))
        .arg("replay")
        .arg(&input)
        .arg("--database")
        .arg(&database)
        .arg("--output")
        .arg(directory.path().join(".").join("history.sqlite"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!database.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_ceh-cli"))
        .arg("replay")
        .arg(&input)
        .arg("--output")
        .arg(directory.path().join(".").join("input.json"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read_to_string(&input).unwrap(), payload);
}
