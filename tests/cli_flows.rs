use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dev-cleaner"))
}

fn command(workspace: &TempDir) -> Command {
    let mut cmd = Command::new(binary_path());
    cmd.current_dir(workspace.path());
    cmd.env_clear();
    cmd.env("HOME", workspace.path().join("home"));
    cmd.env("XDG_CONFIG_HOME", workspace.path().join("config"));
    cmd.env("XDG_DATA_HOME", workspace.path().join("data"));
    cmd.env("DEV_CLEANER_TRASH_DIR", workspace.path().join("trash"));
    cmd
}

fn run(workspace: &TempDir, args: &[&str]) -> Output {
    let output = command(workspace).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_failure(workspace: &TempDir, args: &[&str]) -> Output {
    let output = command(workspace).args(args).output().unwrap();
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn write_project(root: &Path, name: &str, artifact_bytes: usize) -> PathBuf {
    let project_root = root.join(name);
    let target_dir = project_root.join("target");
    fs::create_dir_all(&target_dir).unwrap();
    fs::write(
        project_root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\n",
    )
    .unwrap();
    fs::write(target_dir.join("artifact.bin"), vec![b'x'; artifact_bytes]).unwrap();
    project_root
}

fn write_apply_plan(workspace: &TempDir, project_root: &Path, plan_name: &str) -> PathBuf {
    let plan_path = workspace.path().join(plan_name);
    run(
        workspace,
        &[
            "plan",
            project_root.to_str().unwrap(),
            "--include-recent",
            "-o",
            plan_path.to_str().unwrap(),
        ],
    );
    plan_path
}

fn parse_json_value(stdout: &[u8]) -> Value {
    serde_json::from_slice(stdout).unwrap()
}

fn parse_json_lines(stdout: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn public_stats_module_reexports_statistics() {
    let stats = dev_cleaner::stats::Statistics::from_projects(Vec::new());
    assert_eq!(stats.total_projects, 0);
}

fn audit_log_path(workspace: &TempDir) -> PathBuf {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let old_home = env::var_os("HOME");
    let old_xdg_data = env::var_os("XDG_DATA_HOME");
    env::set_var("HOME", workspace.path().join("home"));
    env::set_var("XDG_DATA_HOME", workspace.path().join("data"));
    let path = dirs::data_dir()
        .unwrap()
        .join("dev-cleaner")
        .join("operations.jsonl");
    match old_home {
        Some(value) => env::set_var("HOME", value),
        None => env::remove_var("HOME"),
    }
    match old_xdg_data {
        Some(value) => env::set_var("XDG_DATA_HOME", value),
        None => env::remove_var("XDG_DATA_HOME"),
    }
    path
}

#[test]
fn scan_json_reports_cleanable_targets() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "scan-app", 1024);

    let output = run(
        &workspace,
        &[
            "scan",
            project_root.to_str().unwrap(),
            "--json",
            "--include-recent",
        ],
    );

    let json = parse_json_value(&output.stdout);
    let projects = json.as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["project_type"], "Rust");
    assert_eq!(
        projects[0]["cleanable_dir"].as_str().unwrap(),
        project_root.join("target").to_str().unwrap()
    );
}

#[test]
fn bridge_scan_streams_jsonl_events() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "bridge-scan-app", 1024);

    let output = run(
        &workspace,
        &[
            "bridge",
            "scan",
            project_root.to_str().unwrap(),
            "--include-recent",
        ],
    );

    let events = parse_json_lines(&output.stdout);
    assert!(events.iter().any(|event| event["type"] == "ready"));
    assert!(events.iter().any(|event| event["type"] == "scan_item"));
    let finished = events
        .iter()
        .find(|event| event["type"] == "scan_finished")
        .unwrap();
    assert_eq!(finished["total_count"], 1);
}

#[test]
fn bridge_clean_defaults_to_trash_without_trash_flag() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "bridge-default-trash-app", 2048);
    let target = project_root.join("target");

    let output = run(
        &workspace,
        &[
            "bridge",
            "clean",
            project_root.to_str().unwrap(),
            "--include-recent",
            "--force",
            "--max-risk",
            "all",
        ],
    );

    assert!(!target.exists());
    let events = parse_json_lines(&output.stdout);
    let started = events
        .iter()
        .find(|event| event["type"] == "cleanup_started")
        .unwrap();
    assert_eq!(started["mode"], "trash");
    let finished = events
        .iter()
        .find(|event| event["type"] == "cleanup_finished")
        .unwrap();
    assert_eq!(finished["payload"]["cleaned_count"], 1);
    assert!(finished["payload"]["trash_batch_id"].as_str().is_some());
    assert!(workspace
        .path()
        .join("trash")
        .join("trash_log.jsonl")
        .exists());
    let audit_content = fs::read_to_string(audit_log_path(&workspace)).unwrap();
    assert!(audit_content.contains("\"result\":\"completed\""));
    assert!(!audit_content.contains("\"result\":\"attempted\""));
}

#[test]
fn bridge_clean_permanent_delete_requires_explicit_flag_and_does_not_create_trash_batch() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "bridge-delete-app", 2048);
    let target = project_root.join("target");

    let output = run(
        &workspace,
        &[
            "bridge",
            "clean",
            project_root.to_str().unwrap(),
            "--include-recent",
            "--force",
            "--max-risk",
            "all",
            "--permanent-delete",
        ],
    );

    assert!(!target.exists());
    let events = parse_json_lines(&output.stdout);
    let finished = events
        .iter()
        .find(|event| event["type"] == "cleanup_finished")
        .unwrap();
    assert_eq!(finished["payload"]["cleaned_count"], 1);
    assert!(finished["payload"]["trash_batch_id"].is_null());
    assert!(finished["payload"]["run_id"].as_str().is_some());
    assert!(!workspace
        .path()
        .join("trash")
        .join("trash_log.jsonl")
        .exists());
    let audit_content = fs::read_to_string(audit_log_path(&workspace)).unwrap();
    assert!(audit_content.contains("\"command\":\"clean\""));
    assert!(audit_content.contains("\"result\":\"completed\""));
    assert!(!audit_content.contains("\"result\":\"attempted\""));
}

#[test]
fn bridge_clean_rejects_trash_and_permanent_delete_together() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "bridge-conflict-app", 1024);

    let output = run_failure(
        &workspace,
        &[
            "bridge",
            "clean",
            project_root.to_str().unwrap(),
            "--include-recent",
            "--trash",
            "--permanent-delete",
        ],
    );

    assert!(String::from_utf8_lossy(&output.stderr).contains("Use either --trash"));
    assert!(project_root.join("target").exists());
}

#[test]
fn bridge_apply_defaults_to_trash_without_trash_flag() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "bridge-apply-default-trash-app", 2048);
    let plan_path = write_apply_plan(&workspace, &project_root, "bridge-apply-plan.json");

    let output = run(
        &workspace,
        &[
            "bridge",
            "apply",
            plan_path.to_str().unwrap(),
            "--include-recent",
            "--force",
        ],
    );

    assert!(!project_root.join("target").exists());
    let events = parse_json_lines(&output.stdout);
    let started = events
        .iter()
        .find(|event| event["type"] == "cleanup_started")
        .unwrap();
    assert_eq!(started["mode"], "trash");
    let finished = events
        .iter()
        .find(|event| event["type"] == "cleanup_finished")
        .unwrap();
    assert!(finished["payload"]["trash_batch_id"].as_str().is_some());
    assert!(workspace
        .path()
        .join("trash")
        .join("trash_log.jsonl")
        .exists());
}

#[test]
fn bridge_apply_rejects_trash_and_permanent_delete_together() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "bridge-apply-conflict-app", 1024);
    let plan_path = write_apply_plan(&workspace, &project_root, "bridge-apply-conflict-plan.json");

    let output = run_failure(
        &workspace,
        &[
            "bridge",
            "apply",
            plan_path.to_str().unwrap(),
            "--include-recent",
            "--trash",
            "--permanent-delete",
        ],
    );

    assert!(String::from_utf8_lossy(&output.stderr).contains("Use either --trash"));
    assert!(project_root.join("target").exists());
}

#[test]
fn bridge_clean_cancel_file_stops_before_next_item() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "bridge-cancel-app", 2048);
    let cancel_file = workspace.path().join("cancel");
    fs::write(&cancel_file, "stop").unwrap();

    let output = run(
        &workspace,
        &[
            "bridge",
            "clean",
            project_root.to_str().unwrap(),
            "--include-recent",
            "--force",
            "--max-risk",
            "all",
            "--cancel-file",
            cancel_file.to_str().unwrap(),
        ],
    );

    assert!(project_root.join("target").exists());
    let events = parse_json_lines(&output.stdout);
    assert!(events
        .iter()
        .any(|event| event["type"] == "cleanup_cancelled"));
    let finished = events
        .iter()
        .find(|event| event["type"] == "cleanup_finished")
        .unwrap();
    assert_eq!(finished["payload"]["cleaned_count"], 0);
    assert_eq!(finished["payload"]["cancelled"], true);
    assert!(finished["payload"]["trash_batch_id"].is_null());
    assert!(!workspace.path().join("trash").exists());

    let audit_content = fs::read_to_string(audit_log_path(&workspace)).unwrap();
    assert!(audit_content.contains("\"type\":\"run_started\""));
    assert!(audit_content.contains("\"type\":\"run_finished\""));
    assert!(!audit_content.contains("\"type\":\"item_action\""));
    assert!(!audit_content.contains(project_root.join("target").to_str().unwrap()));
}

#[test]
fn bridge_config_save_uses_resolved_config_path_not_snapshot_path() {
    let workspace = TempDir::new().unwrap();
    let snapshot_path = workspace.path().join("snapshot.json");
    let supplied_config_path = workspace.path().join("gui-supplied").join("config.toml");
    let snapshot = serde_json::json!({
        "config_path": supplied_config_path,
        "config": {
            "exclude_dirs": ["saved-to-resolved-path"]
        },
        "gui_preferences": {
            "appearance": "light",
            "launch_at_login": true
        }
    });
    fs::write(&snapshot_path, serde_json::to_string(&snapshot).unwrap()).unwrap();

    let output = run(
        &workspace,
        &[
            "bridge",
            "config",
            "save",
            "--input",
            snapshot_path.to_str().unwrap(),
        ],
    );

    let events = parse_json_lines(&output.stdout);
    let saved = events
        .iter()
        .find(|event| event["type"] == "config_saved")
        .unwrap();
    let resolved_config_path = PathBuf::from(saved["path"].as_str().unwrap());
    assert_ne!(resolved_config_path, supplied_config_path);
    assert!(resolved_config_path.exists());
    assert!(fs::read_to_string(&resolved_config_path)
        .unwrap()
        .contains("saved-to-resolved-path"));
    assert!(!supplied_config_path.exists());
}

#[test]
fn stats_json_reports_aggregates() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "stats-app", 1024);

    let output = run(
        &workspace,
        &[
            "stats",
            project_root.to_str().unwrap(),
            "--json",
            "--include-recent",
        ],
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_start = stdout.find('{').unwrap();
    let stats: Value = serde_json::from_str(&stdout[json_start..]).unwrap();
    assert_eq!(stats["total_projects"], 1);
    assert_eq!(stats["by_age_group"]["recent"][0], 1);
    assert_eq!(stats["top_largest"][0]["project_type"], "Rust");
}

#[test]
fn recommend_writes_plan_file_and_json_output() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "recommend-app", 4096);
    let plan_path = workspace.path().join("plan.json");

    let output = run(
        &workspace,
        &[
            "recommend",
            project_root.to_str().unwrap(),
            "--cleanup",
            "1KB",
            "--include-recent",
            "--json",
            "--output-plan",
            plan_path.to_str().unwrap(),
        ],
    );

    let json = parse_json_value(&output.stdout);
    assert_eq!(json["selected_count"], 1);
    assert_eq!(json["projects"].as_array().unwrap().len(), 1);
    assert!(plan_path.exists());

    let plan: Value = serde_json::from_str(&fs::read_to_string(&plan_path).unwrap()).unwrap();
    assert_eq!(plan["projects"].as_array().unwrap().len(), 1);
    assert_eq!(
        plan["projects"][0]["cleanable_dir"].as_str().unwrap(),
        project_root.join("target").to_str().unwrap()
    );
}

#[test]
fn clean_trash_undo_roundtrip_updates_files_and_audit_log() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "clean-app", 2048);

    let clean_output = run(
        &workspace,
        &[
            "clean",
            project_root.to_str().unwrap(),
            "--trash",
            "--auto",
            "--include-recent",
        ],
    );
    let clean_stdout = String::from_utf8_lossy(&clean_output.stdout);
    assert!(clean_stdout.contains("Cleaning completed!"));
    assert!(clean_stdout.contains("Trash batch:"));

    let trash_root = workspace.path().join("trash");
    let batches: Vec<_> = fs::read_dir(&trash_root)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "trash_log.jsonl" {
                None
            } else {
                Some(name)
            }
        })
        .collect();
    assert_eq!(batches.len(), 1);
    assert!(!project_root.join("target").exists());

    let audit_path = audit_log_path(&workspace);
    let audit_content = fs::read_to_string(&audit_path).unwrap();
    assert!(audit_content.contains("\"command\":\"clean\""));
    assert!(audit_content.contains("\"type\":\"run_started\""));
    assert!(audit_content.contains("\"type\":\"run_finished\""));

    let list_output = run(&workspace, &["trash", "list", "--json"]);
    let batches_json = parse_json_value(&list_output.stdout);
    assert_eq!(batches_json.as_array().unwrap().len(), 1);

    let undo_output = run(&workspace, &["undo"]);
    let undo_stdout = String::from_utf8_lossy(&undo_output.stdout);
    assert!(undo_stdout.contains("Restore completed!"));
    assert!(project_root.join("target").join("artifact.bin").exists());
}

#[test]
fn plan_apply_dry_run_keeps_target() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "apply-dry-run-app", 1024);
    let plan_path = workspace.path().join("apply-plan.json");

    run(
        &workspace,
        &[
            "plan",
            project_root.to_str().unwrap(),
            "--include-recent",
            "-o",
            plan_path.to_str().unwrap(),
        ],
    );

    let output = run(
        &workspace,
        &[
            "apply",
            plan_path.to_str().unwrap(),
            "--dry-run",
            "--include-recent",
            "--force",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("Applying cleanup plan"));
    assert!(stdout.contains("Cleaning completed!"));
    assert!(project_root.join("target").join("artifact.bin").exists());
}

#[test]
fn plan_apply_trash_moves_target_and_logs_apply() {
    let workspace = TempDir::new().unwrap();
    let project_root = write_project(workspace.path(), "apply-trash-app", 2048);
    let plan_path = workspace.path().join("apply-trash-plan.json");

    run(
        &workspace,
        &[
            "plan",
            project_root.to_str().unwrap(),
            "--include-recent",
            "-o",
            plan_path.to_str().unwrap(),
        ],
    );

    let output = run(
        &workspace,
        &[
            "apply",
            plan_path.to_str().unwrap(),
            "--trash",
            "--include-recent",
            "--force",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("Cleaning completed!"));
    assert!(stdout.contains("Trash batch:"));
    assert!(!project_root.join("target").exists());

    let audit_path = audit_log_path(&workspace);
    let audit_content = fs::read_to_string(&audit_path).unwrap();
    assert!(audit_content.contains("\"command\":\"apply\""));
    assert!(audit_content.contains("\"type\":\"run_finished\""));
}
