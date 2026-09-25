use std::process::Command;

#[test]
fn fixture_path_runs_without_network_credentials_and_exercises_governance_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_r2r-jev"))
        .arg("fixture")
        .env_remove("TYPESAFE_API_KEY")
        .env_remove("TYPESAFE_ENDPOINT")
        .env_remove("TYPESAFE_MODEL")
        .output()
        .expect("fixture demo should start");

    assert!(
        output.status.success(),
        "fixture demo failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("Admission"));
    assert!(stdout.contains("DENY merge_pull_request"));
    assert!(stdout.contains("human override"));
    assert!(stdout.contains("ALLOW merge_pull_request"));
    assert!(stdout.contains("Provenance"));
}
