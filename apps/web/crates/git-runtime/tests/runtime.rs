use std::path::Path;
use std::process::Command;

use open_web_codex_git_runtime::{CommitAuthor, GitRuntime, GitRuntimeConfig, GitRuntimeError};
use tempfile::TempDir;
use uuid::Uuid;

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("run git fixture command");
    assert!(
        output.status.success(),
        "git fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture() -> (TempDir, GitRuntime, String) {
    let root = TempDir::new().expect("runner tempdir");
    let source = root.path().join("source");
    std::fs::create_dir(&source).expect("source directory");
    git(&source, &["init", "-b", "main"]);
    std::fs::write(source.join("README.md"), "initial\n").expect("fixture readme");
    git(&source, &["add", "README.md"]);
    git(
        &source,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "initial",
        ],
    );
    let runtime =
        GitRuntime::new(GitRuntimeConfig::new(root.path().join("runner")).with_local_sources())
            .expect("Git Runtime");
    (root, runtime, source.to_string_lossy().to_string())
}

#[test]
fn validates_remote_sources_and_branch_names() {
    let root = TempDir::new().unwrap();
    let runtime = GitRuntime::new(GitRuntimeConfig::new(root.path().join("runner"))).unwrap();
    assert!(runtime
        .validate_source("https://github.com/openai/codex.git")
        .is_ok());
    assert!(runtime
        .validate_source("git@github.com:openai/codex.git")
        .is_ok());
    assert!(runtime
        .validate_source("https://token@github.com/openai/codex.git")
        .is_err());
    assert!(runtime.validate_source("file:///private/repo").is_err());
    assert!(runtime.validate_ref("feature/safe-name").is_ok());
    for value in [
        "--upload-pack=x",
        "../main",
        "refs//main",
        "main.lock",
        "main~1",
    ] {
        assert!(runtime.validate_ref(value).is_err(), "accepted {value}");
    }
}

#[tokio::test]
async fn provisions_isolated_workspaces_serializes_mirror_and_commits_selected_files() {
    let (_root, runtime, source) = fixture();
    let source = runtime.validate_source(&source).unwrap();
    let branch = runtime.validate_ref("main").unwrap();
    let project_id = Uuid::now_v7();
    let first_id = Uuid::now_v7();
    let second_id = Uuid::now_v7();
    let (first, second) = tokio::join!(
        runtime.provision(project_id, first_id, &source, &branch),
        runtime.provision(project_id, second_id, &source, &branch),
    );
    let first = first.expect("first workspace");
    let second = second.expect("second workspace");
    assert_ne!(first.root, second.root);
    assert_eq!(first.head_commit, second.head_commit);

    std::fs::write(first.root.join("README.md"), "changed\n").unwrap();
    std::fs::write(first.root.join("unselected.txt"), "leave me\n").unwrap();
    let status = runtime.status(first_id).await.unwrap();
    assert_eq!(status.changes.len(), 2);
    assert!(status
        .changes
        .iter()
        .all(|change| !change.path.starts_with('/')));

    let commit = runtime
        .commit_selected(
            first_id,
            &["README.md".to_string()],
            "selected change",
            &CommitAuthor {
                name: "Codex User".to_string(),
                email: "user@example.invalid".to_string(),
            },
        )
        .await
        .expect("commit selected file");
    assert_ne!(commit, first.head_commit);
    let status = runtime.status(first_id).await.unwrap();
    assert_eq!(status.changes.len(), 1);
    assert_eq!(status.changes[0].path, "unselected.txt");

    runtime.remove_workspace(first_id).await.unwrap();
    assert!(!first.root.exists());
    assert!(second.root.exists());
}

#[tokio::test]
async fn reports_binary_and_large_file_metadata_without_exposing_absolute_paths() {
    let (_root, runtime, source) = fixture();
    let source = runtime.validate_source(&source).unwrap();
    let branch = runtime.validate_ref("main").unwrap();
    let workspace_id = Uuid::now_v7();
    let checkout = runtime
        .provision(Uuid::now_v7(), workspace_id, &source, &branch)
        .await
        .unwrap();
    std::fs::write(checkout.root.join("binary.bin"), [0, 159, 146, 150]).unwrap();
    std::fs::write(checkout.root.join("large.txt"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
    git(&checkout.root, &["add", "binary.bin", "large.txt"]);

    let status = runtime.status(workspace_id).await.unwrap();
    let binary = status
        .changes
        .iter()
        .find(|change| change.path == "binary.bin")
        .unwrap();
    assert!(binary.binary);
    let large = status
        .changes
        .iter()
        .find(|change| change.path == "large.txt")
        .unwrap();
    assert!(large.large);
    assert_eq!(large.size_bytes, Some(1024 * 1024 + 1));
    assert!(status
        .changes
        .iter()
        .all(|change| !change.path.starts_with('/')));
}

#[tokio::test]
async fn refuses_to_commit_when_an_unselected_path_is_already_staged() {
    let (_root, runtime, source) = fixture();
    let source = runtime.validate_source(&source).unwrap();
    let branch = runtime.validate_ref("main").unwrap();
    let workspace_id = Uuid::now_v7();
    let checkout = runtime
        .provision(Uuid::now_v7(), workspace_id, &source, &branch)
        .await
        .unwrap();
    std::fs::write(checkout.root.join("README.md"), "selected\n").unwrap();
    std::fs::write(checkout.root.join("other.txt"), "staged\n").unwrap();
    git(&checkout.root, &["add", "other.txt"]);

    let error = runtime
        .commit_selected(
            workspace_id,
            &["README.md".to_string()],
            "must not commit other.txt",
            &CommitAuthor {
                name: "Codex User".to_string(),
                email: "user@example.invalid".to_string(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, GitRuntimeError::Conflict(_)));
    let status = runtime.status(workspace_id).await.unwrap();
    assert_eq!(status.head_commit, checkout.head_commit);
}

#[cfg(unix)]
#[tokio::test]
async fn refuses_a_symlink_at_a_server_generated_workspace_path() {
    use std::os::unix::fs::symlink;

    let (root, runtime, source) = fixture();
    let source = runtime.validate_source(&source).unwrap();
    let branch = runtime.validate_ref("main").unwrap();
    let workspace_id = Uuid::now_v7();
    let outside = root.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    symlink(&outside, runtime.workspace_path(workspace_id)).unwrap();

    let error = runtime
        .provision(Uuid::now_v7(), workspace_id, &source, &branch)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GitRuntimeError::Conflict(_) | GitRuntimeError::UnsafePath(_)
    ));
    assert!(outside.exists());
}
