use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use dev_cleaner::scanner::{RiskLevel, Scanner};
use dev_cleaner::ProjectInfo;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

struct WorkloadFixture {
    name: &'static str,
    description: &'static str,
    max_risk: RiskLevel,
    tempdir: TempDir,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectSnapshot {
    path: String,
    size: u64,
    project_type: String,
    category: String,
    risk: String,
    confidence: String,
}

#[derive(Debug, Clone)]
struct ConsistencySummary {
    results: usize,
    total_bytes: u64,
}

impl WorkloadFixture {
    fn root(&self) -> &Path {
        self.tempdir.path()
    }

    fn scanner(&self) -> Scanner {
        Scanner::new(self.root()).max_risk(self.max_risk)
    }
}

fn benchmark_scans(c: &mut Criterion) {
    let fixtures = build_workloads();

    let mut scan_group = c.benchmark_group("scan_total");
    for fixture in &fixtures {
        let summary = validate_fixture(fixture);
        println!(
            "consistency workload={} results={} total_bytes={} description=\"{}\"",
            fixture.name, summary.results, summary.total_bytes, fixture.description
        );
        scan_group.throughput(Throughput::Bytes(summary.total_bytes.max(1)));
        scan_group.bench_with_input(
            BenchmarkId::from_parameter(fixture.name),
            fixture,
            |b, fixture| {
                b.iter(|| black_box(run_scan(fixture)));
            },
        );
    }
    scan_group.finish();

    let mut streaming_group = c.benchmark_group("scan_streaming_total");
    for fixture in &fixtures {
        let summary = validate_fixture(fixture);
        streaming_group.throughput(Throughput::Bytes(summary.total_bytes.max(1)));
        streaming_group.bench_with_input(
            BenchmarkId::from_parameter(fixture.name),
            fixture,
            |b, fixture| {
                b.iter(|| black_box(run_streaming_scan(fixture)));
            },
        );
    }
    streaming_group.finish();
}

fn run_scan(fixture: &WorkloadFixture) -> Vec<ProjectInfo> {
    fixture
        .scanner()
        .scan()
        .expect("scan baseline should succeed")
}

fn run_streaming_scan(fixture: &WorkloadFixture) -> Vec<ProjectInfo> {
    let (expected, rx) = fixture
        .scanner()
        .scan_with_streaming()
        .expect("streaming scan baseline should succeed");
    let mut projects = rx.iter().collect::<Vec<_>>();
    assert_eq!(
        expected,
        projects.len(),
        "streaming scan yielded unexpected number of results for {}",
        fixture.name
    );
    projects.sort_by(|a, b| b.size.cmp(&a.size));
    projects
}

fn validate_fixture(fixture: &WorkloadFixture) -> ConsistencySummary {
    let scan_projects = run_scan(fixture);
    let streaming_projects = run_streaming_scan(fixture);
    let scan_snapshot = snapshot(&scan_projects);
    let streaming_snapshot = snapshot(&streaming_projects);

    assert_eq!(
        scan_snapshot, streaming_snapshot,
        "scan() and scan_with_streaming() diverged for {}",
        fixture.name
    );

    ConsistencySummary {
        results: scan_projects.len(),
        total_bytes: scan_projects.iter().map(|project| project.size).sum(),
    }
}

fn snapshot(projects: &[ProjectInfo]) -> Vec<ProjectSnapshot> {
    let mut snapshot = projects
        .iter()
        .map(|project| ProjectSnapshot {
            path: project.cleanable_dir.display().to_string(),
            size: project.size,
            project_type: project.project_type_display_name(),
            category: project.category.to_string(),
            risk: project.risk_level.to_string(),
            confidence: project.confidence.to_string(),
        })
        .collect::<Vec<_>>();
    snapshot.sort_by(|a, b| a.path.cmp(&b.path));
    snapshot
}

fn build_workloads() -> Vec<WorkloadFixture> {
    vec![
        build_wide_small(),
        build_deep_nested(),
        build_size_heavy(),
        build_risk_high(),
    ]
}

fn build_wide_small() -> WorkloadFixture {
    let tempdir = TempDir::new().expect("wide_small tempdir");
    let root = tempdir.path();

    for idx in 0..24 {
        let rust_root = root.join(format!("rust-app-{idx:02}"));
        fs::create_dir_all(&rust_root).unwrap();
        fs::write(
            rust_root.join("Cargo.toml"),
            package_manifest("wide-small-rust"),
        )
        .unwrap();
        create_tree(&rust_root.join("target"), 4, 8, 512);
    }

    for idx in 0..24 {
        let node_root = root.join(format!("node-app-{idx:02}"));
        fs::create_dir_all(&node_root).unwrap();
        fs::write(
            node_root.join("package.json"),
            "{\"name\":\"wide-small\"}\n",
        )
        .unwrap();
        create_tree(&node_root.join(".next"), 3, 6, 320);
        create_tree(&node_root.join(".cache"), 2, 5, 192);
    }

    WorkloadFixture {
        name: "wide_small",
        description: "many small build/cache targets across many projects",
        max_risk: RiskLevel::Medium,
        tempdir,
    }
}

fn build_deep_nested() -> WorkloadFixture {
    let tempdir = TempDir::new().expect("deep_nested tempdir");
    let root = tempdir.path();

    for idx in 0..18 {
        let py_root = root.join(format!("python-app-{idx:02}"));
        fs::create_dir_all(&py_root).unwrap();
        fs::write(
            py_root.join("pyproject.toml"),
            "[project]\nname = \"deep-nested\"\n",
        )
        .unwrap();
        create_tree(
            &py_root
                .join(".venv")
                .join("lib")
                .join("python3.11")
                .join("site-packages")
                .join(format!("pkg{idx:02}")),
            4,
            4,
            256,
        );
        create_tree(
            &py_root.join("build").join("artifacts").join("debug"),
            3,
            4,
            384,
        );
    }

    for idx in 0..12 {
        let scala_root = root.join(format!("scala-app-{idx:02}"));
        fs::create_dir_all(&scala_root).unwrap();
        fs::write(scala_root.join("build.sbt"), "name := \"deep-nested\"\n").unwrap();
        create_tree(
            &scala_root.join("project").join("target").join("scala-2.13"),
            3,
            5,
            448,
        );
        create_tree(&scala_root.join("target").join("streams"), 3, 5, 384);
    }

    WorkloadFixture {
        name: "deep_nested",
        description: "deep directory trees and nested cleanable targets",
        max_risk: RiskLevel::High,
        tempdir,
    }
}

fn build_size_heavy() -> WorkloadFixture {
    let tempdir = TempDir::new().expect("size_heavy tempdir");
    let root = tempdir.path();

    for idx in 0..10 {
        let rust_root = root.join(format!("size-heavy-rust-{idx:02}"));
        fs::create_dir_all(&rust_root).unwrap();
        fs::write(
            rust_root.join("Cargo.toml"),
            package_manifest("size-heavy-rust"),
        )
        .unwrap();
        create_tree(&rust_root.join("target").join("debug"), 10, 80, 2048);
    }

    WorkloadFixture {
        name: "size_heavy",
        description: "fewer projects with many files inside build outputs",
        max_risk: RiskLevel::Medium,
        tempdir,
    }
}

fn build_risk_high() -> WorkloadFixture {
    let tempdir = TempDir::new().expect("risk_high tempdir");
    let root = tempdir.path();

    for idx in 0..24 {
        let rust_root = root.join(format!("gitignored-rust-{idx:02}"));
        fs::create_dir_all(&rust_root).unwrap();
        fs::write(
            rust_root.join("Cargo.toml"),
            package_manifest("risk-high-rust"),
        )
        .unwrap();
        fs::write(
            rust_root.join(".gitignore"),
            "generated-cache/\nscratch-build/\n.local-cache/\n",
        )
        .unwrap();
        create_tree(&rust_root.join("generated-cache"), 3, 5, 512);
        create_tree(&rust_root.join("scratch-build"), 3, 5, 448);
        create_tree(&rust_root.join(".local-cache"), 2, 4, 384);
    }

    WorkloadFixture {
        name: "risk_high",
        description: "gitignore-derived high-risk targets with candidate prefilter disabled",
        max_risk: RiskLevel::High,
        tempdir,
    }
}

fn package_manifest(name: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
}

fn create_tree(root: &Path, dir_count: usize, files_per_dir: usize, file_size: usize) {
    for dir_index in 0..dir_count {
        let dir = root.join(format!("segment-{dir_index:02}"));
        fs::create_dir_all(&dir).unwrap();
        for file_index in 0..files_per_dir {
            let path = dir.join(format!("artifact-{file_index:03}.bin"));
            write_sized_file(&path, file_size + dir_index + file_index);
        }
    }
}

fn write_sized_file(path: &Path, bytes: usize) {
    let mut payload = vec![0u8; bytes];
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte = b'a' + (index % 23) as u8;
    }
    fs::write(path, payload).unwrap();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(20);
    targets = benchmark_scans
}
criterion_main!(benches);
