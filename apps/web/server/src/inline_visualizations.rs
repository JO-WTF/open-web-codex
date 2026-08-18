use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, File, OpenOptions};
use chrono::DateTime;
use open_web_codex_git_runtime::GitRuntime;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[cfg(test)]
use tokio::fs;

const MAX_HTML_BYTES: u64 = 2 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const VIEWER_STYLESHEET: &str =
    include_str!("../../../../codex/codex-rs/tui/src/inline_visualization/assets/visualize.css");
const VIEWER_RUNTIME: &str =
    include_str!("../../../../codex/codex-rs/tui/src/inline_visualization/assets/visualize.html");
const FRAGMENT_PLACEHOLDER: &str = "<!--__INLINE_VISUALIZATION_FRAGMENT__-->";
const WORKSPACE_FILE_DIRECTIVE_PREFIX: &str = "::codex-inline-vis{workspace_file=\"";

pub(crate) const FRAME_CSP: &str = "default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval' blob: data: https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://esm.sh https://fonts.bunny.net https://fonts.googleapis.com https://fonts.gstatic.com https://unpkg.com; style-src 'unsafe-inline' blob: data: https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://esm.sh https://fonts.bunny.net https://fonts.googleapis.com https://fonts.gstatic.com https://unpkg.com; img-src blob: data: https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://esm.sh https://fonts.bunny.net https://fonts.googleapis.com https://fonts.gstatic.com https://unpkg.com; font-src blob: data: https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://esm.sh https://fonts.bunny.net https://fonts.googleapis.com https://fonts.gstatic.com https://unpkg.com; media-src blob: data:; worker-src blob:; connect-src blob: data:; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InlineVisualizationKind {
    Html,
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl InlineVisualizationKind {
    pub(crate) fn content_type(self) -> &'static str {
        match self {
            Self::Html => "text/html; charset=utf-8",
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }

    fn max_bytes(self) -> u64 {
        match self {
            Self::Html => MAX_HTML_BYTES,
            Self::Png | Self::Jpeg | Self::Gif | Self::Webp => MAX_IMAGE_BYTES,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct InlineVisualizationPayload {
    pub(crate) bytes: Vec<u8>,
    pub(crate) content_type: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum InlineVisualizationError {
    #[error("inline visualization reference is invalid")]
    InvalidReference,
    #[error("inline visualization is unavailable")]
    Unavailable,
    #[error("inline visualization content is invalid")]
    InvalidContent,
}

pub(crate) async fn read(
    codex_home: &Path,
    thread_id: &str,
    file: &str,
) -> Result<InlineVisualizationPayload, InlineVisualizationError> {
    let kind = file_kind(file).ok_or(InlineVisualizationError::InvalidReference)?;
    let codex_home = codex_home.to_path_buf();
    let thread_id = thread_id.to_string();
    let file = file.to_string();
    let file_for_read = file.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        let visualizations_dir = open_visualizations_dir(&codex_home)?;
        read_from_visualizations_dir(
            &visualizations_dir,
            &thread_id,
            &file_for_read,
            kind.max_bytes(),
        )
    })
    .await
    .map_err(|_| InlineVisualizationError::Unavailable)??;

    let bytes = match kind {
        InlineVisualizationKind::Html => {
            let fragment = std::str::from_utf8(&bytes)
                .map_err(|_| InlineVisualizationError::InvalidContent)?;
            render_html_fragment(fragment, &file).into_bytes()
        }
        _ if valid_image_signature(kind, &bytes) => bytes,
        _ => return Err(InlineVisualizationError::InvalidContent),
    };
    Ok(InlineVisualizationPayload {
        bytes,
        content_type: kind.content_type(),
    })
}

/// Convert the explicit Platform workspace-file directive into Codex's native
/// file directive. The model can name only a validated Workspace-relative HTML
/// file; the Platform snapshots it under the authoritative Profile/Thread
/// visualization root before the browser sees a native reference.
pub(crate) async fn materialize_workspace_html_references(
    markdown: &str,
    git: &GitRuntime,
    workspace_id: Uuid,
    codex_home: &Path,
    thread_id: &str,
    item_id: &str,
) -> Result<String, InlineVisualizationError> {
    if !markdown.contains(WORKSPACE_FILE_DIRECTIVE_PREFIX) {
        return Ok(markdown.to_string());
    }

    let mut output = String::with_capacity(markdown.len());
    let mut fence: Option<(u8, usize)> = None;
    let mut directive_index = 0usize;
    for line in markdown.split_inclusive('\n') {
        let (content, newline) = line
            .strip_suffix('\n')
            .map_or((line, ""), |content| (content, "\n"));
        let content = content.strip_suffix('\r').unwrap_or(content);
        if let Some(marker) = markdown_fence_marker(content) {
            match fence {
                None => fence = Some(marker),
                Some((character, length)) if character == marker.0 && marker.1 >= length => {
                    fence = None;
                }
                _ => {}
            }
            output.push_str(content);
            output.push_str(newline);
            continue;
        }

        let leading = content.len() - content.trim_start_matches(' ').len();
        let indented = leading >= 4 || content.starts_with('\t');
        let directive = content.trim();
        if fence.is_none() && !indented && directive.starts_with(WORKSPACE_FILE_DIRECTIVE_PREFIX) {
            let source = workspace_html_source(directive)
                .ok_or(InlineVisualizationError::InvalidReference)?;
            let source_file = git
                .read_file(workspace_id, source)
                .await
                .map_err(|_| InlineVisualizationError::Unavailable)?;
            if source_file.truncated || source_file.content.len() as u64 > MAX_HTML_BYTES {
                return Err(InlineVisualizationError::InvalidContent);
            }
            let file = workspace_snapshot_file_name(item_id, directive_index, source);
            write_html_snapshot(codex_home, thread_id, &file, source_file.content.as_bytes())?;
            output.push_str(&content[..leading]);
            output.push_str("::codex-inline-vis{file=\"");
            output.push_str(&file);
            output.push_str("\"}");
            output.push_str(newline);
            directive_index += 1;
            continue;
        }
        output.push_str(content);
        output.push_str(newline);
    }
    Ok(output)
}

fn workspace_html_source(directive: &str) -> Option<&str> {
    let source = directive
        .strip_prefix(WORKSPACE_FILE_DIRECTIVE_PREFIX)?
        .strip_suffix("\"}")?;
    if source.is_empty()
        || source.len() > 512
        || !source.ends_with(".html")
        || !source
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
        || source
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".." | ".git"))
    {
        return None;
    }
    Some(source)
}

fn workspace_snapshot_file_name(item_id: &str, directive_index: usize, source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(item_id.as_bytes());
    hasher.update([0]);
    hasher.update(directive_index.to_le_bytes());
    hasher.update([0]);
    hasher.update(source.as_bytes());
    format!("workspace-{}.html", hex::encode(hasher.finalize()))
}

fn write_html_snapshot(
    codex_home: &Path,
    thread_id: &str,
    file: &str,
    bytes: &[u8],
) -> Result<(), InlineVisualizationError> {
    if file_kind(file) != Some(InlineVisualizationKind::Html)
        || bytes.len() as u64 > MAX_HTML_BYTES
        || std::str::from_utf8(bytes).is_err()
    {
        return Err(InlineVisualizationError::InvalidContent);
    }
    let profile = Dir::open_ambient_dir(codex_home, ambient_authority())
        .map_err(|_| InlineVisualizationError::Unavailable)?;
    let visualizations = open_or_create_dir_nofollow(&profile, "visualizations")?;
    let components = visualization_components(thread_id)?;
    let year = open_or_create_dir_nofollow(&visualizations, &components[0])?;
    let month = open_or_create_dir_nofollow(&year, &components[1])?;
    let day = open_or_create_dir_nofollow(&month, &components[2])?;
    let thread = open_or_create_dir_nofollow(&day, &components[3])?;

    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .follow(FollowSymlinks::No);
    let mut output = match thread.open_with(file, &options) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(error) => return Err(map_no_follow_error(error)),
    };
    if let Err(error) = output.write_all(bytes).and_then(|_| output.sync_all()) {
        let _ = thread.remove_file(file);
        return Err(map_no_follow_error(error));
    }
    Ok(())
}

fn open_or_create_dir_nofollow(parent: &Dir, name: &str) -> Result<Dir, InlineVisualizationError> {
    match parent.open_dir_nofollow(name) {
        Ok(directory) => Ok(directory),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match parent.create_dir(name) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(map_no_follow_error(error)),
            }
            parent.open_dir_nofollow(name).map_err(map_no_follow_error)
        }
        Err(error) => Err(map_no_follow_error(error)),
    }
}

fn markdown_fence_marker(line: &str) -> Option<(u8, usize)> {
    let leading = line.len() - line.trim_start_matches(' ').len();
    if leading > 3 {
        return None;
    }
    let bytes = line[leading..].as_bytes();
    let character = *bytes.first()?;
    if !matches!(character, b'`' | b'~') {
        return None;
    }
    let length = bytes.iter().take_while(|byte| **byte == character).count();
    (length >= 3).then_some((character, length))
}

fn open_visualizations_dir(codex_home: &Path) -> Result<Dir, InlineVisualizationError> {
    Dir::open_ambient_dir(codex_home.join("visualizations"), ambient_authority())
        .map_err(|_| InlineVisualizationError::Unavailable)
}

fn read_from_visualizations_dir(
    visualizations_dir: &Dir,
    thread_id: &str,
    file: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, InlineVisualizationError> {
    let file = open_from_visualizations_dir(visualizations_dir, thread_id, file)?;
    read_opened_file(file, max_bytes)
}

fn open_from_visualizations_dir(
    visualizations_dir: &Dir,
    thread_id: &str,
    file: &str,
) -> Result<File, InlineVisualizationError> {
    let components = visualization_components(thread_id)?;
    let year = visualizations_dir
        .open_dir_nofollow(&components[0])
        .map_err(map_no_follow_error)?;
    let month = year
        .open_dir_nofollow(&components[1])
        .map_err(map_no_follow_error)?;
    let day = month
        .open_dir_nofollow(&components[2])
        .map_err(map_no_follow_error)?;
    let thread = day
        .open_dir_nofollow(&components[3])
        .map_err(map_no_follow_error)?;

    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    thread
        .open_with(file, &options)
        .map_err(map_no_follow_error)
}

fn map_no_follow_error(error: std::io::Error) -> InlineVisualizationError {
    if error.kind() == std::io::ErrorKind::NotFound {
        InlineVisualizationError::Unavailable
    } else {
        InlineVisualizationError::InvalidContent
    }
}

fn read_opened_file(file: File, max_bytes: u64) -> Result<Vec<u8>, InlineVisualizationError> {
    let metadata = file
        .metadata()
        .map_err(|_| InlineVisualizationError::Unavailable)?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err(InlineVisualizationError::InvalidContent);
    }

    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| InlineVisualizationError::Unavailable)?;
    if u64::try_from(bytes.len())
        .ok()
        .is_none_or(|size| size > max_bytes)
    {
        return Err(InlineVisualizationError::InvalidContent);
    }
    Ok(bytes)
}

fn visualization_components(thread_id: &str) -> Result<[String; 4], InlineVisualizationError> {
    let uuid =
        Uuid::parse_str(thread_id).map_err(|_| InlineVisualizationError::InvalidReference)?;
    let timestamp = uuid
        .get_timestamp()
        .ok_or(InlineVisualizationError::InvalidReference)?;
    let (seconds, nanos) = timestamp.to_unix();
    let created_at = DateTime::from_timestamp(
        i64::try_from(seconds).map_err(|_| InlineVisualizationError::InvalidReference)?,
        nanos,
    )
    .ok_or(InlineVisualizationError::InvalidReference)?;
    Ok([
        created_at.format("%Y").to_string(),
        created_at.format("%m").to_string(),
        created_at.format("%d").to_string(),
        thread_id.to_string(),
    ])
}

#[cfg(test)]
fn visualization_thread_dir(
    codex_home: &Path,
    thread_id: &str,
) -> Result<PathBuf, InlineVisualizationError> {
    let components = visualization_components(thread_id)?;
    Ok(codex_home
        .join("visualizations")
        .join(&components[0])
        .join(&components[1])
        .join(&components[2])
        .join(&components[3]))
}

pub(crate) fn file_kind(file: &str) -> Option<InlineVisualizationKind> {
    let path = Path::new(file);
    if file.is_empty()
        || file.len() > 128
        || file.chars().any(char::is_control)
        || !matches!(
            path.components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
        || !file
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return None;
    }
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => Some(InlineVisualizationKind::Html),
        Some("png") => Some(InlineVisualizationKind::Png),
        Some("jpg" | "jpeg") => Some(InlineVisualizationKind::Jpeg),
        Some("gif") => Some(InlineVisualizationKind::Gif),
        Some("webp") => Some(InlineVisualizationKind::Webp),
        _ => None,
    }
}

fn valid_image_signature(kind: InlineVisualizationKind, bytes: &[u8]) -> bool {
    match kind {
        InlineVisualizationKind::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        InlineVisualizationKind::Jpeg => bytes.starts_with(b"\xff\xd8\xff"),
        InlineVisualizationKind::Gif => {
            bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
        }
        InlineVisualizationKind::Webp => {
            bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
        }
        InlineVisualizationKind::Html => false,
    }
}

fn render_html_fragment(fragment: &str, file: &str) -> String {
    let runtime = VIEWER_RUNTIME.replacen(FRAGMENT_PLACEHOLDER, fragment, 1);
    let title = Path::new(file)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Visualization")
        .replace('-', " ");
    let escaped_title = escape_html(&title);
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"referrer\" content=\"no-referrer\"><meta http-equiv=\"Content-Security-Policy\" content=\"{FRAME_CSP}\"><title>{escaped_title}</title><style>{VIEWER_STYLESHEET}\nhtml>body{{padding:0}}</style></head><body>{runtime}</body></html>"
    )
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_native_html_and_static_raster_file_names() {
        assert_eq!(file_kind("chart.html"), Some(InlineVisualizationKind::Html));
        assert_eq!(file_kind("chart.png"), Some(InlineVisualizationKind::Png));
        assert_eq!(file_kind("chart.jpeg"), Some(InlineVisualizationKind::Jpeg));
        assert_eq!(file_kind("chart.webp"), Some(InlineVisualizationKind::Webp));
        for rejected in [
            "../chart.html",
            "dir/chart.html",
            "chart.svg",
            "chart.md",
            "chart.HTML",
        ] {
            assert_eq!(file_kind(rejected), None, "{rejected}");
        }
    }

    #[test]
    fn accepts_only_safe_workspace_html_directives() {
        assert_eq!(
            workspace_html_source("::codex-inline-vis{workspace_file=\"cards/test.html\"}"),
            Some("cards/test.html")
        );
        for directive in [
            "::codex-inline-vis{workspace_file=\"../test.html\"}",
            "::codex-inline-vis{workspace_file=\"/tmp/test.html\"}",
            "::codex-inline-vis{workspace_file=\"test.svg\"}",
            "::codex-inline-vis{workspace_file=\"cards//test.html\"}",
            "::codex-inline-vis{workspace_file=\".git/test.html\"}",
        ] {
            assert_eq!(workspace_html_source(directive), None, "{directive}");
        }
    }

    #[test]
    fn snapshots_workspace_html_under_the_thread_scoped_native_root() {
        let home = tempfile::tempdir().unwrap();
        let thread_id = Uuid::now_v7().to_string();
        let file = workspace_snapshot_file_name("item-1", 0, "cards/test.html");
        write_html_snapshot(home.path(), &thread_id, &file, b"<button>test</button>").unwrap();
        write_html_snapshot(home.path(), &thread_id, &file, b"<button>changed</button>").unwrap();

        let thread_dir = visualization_thread_dir(home.path(), &thread_id).unwrap();
        assert_eq!(
            std::fs::read(thread_dir.join(&file)).unwrap(),
            b"<button>test</button>"
        );
    }

    #[tokio::test]
    async fn materializes_an_authorized_workspace_html_file_to_a_native_reference() {
        let runner = tempfile::tempdir().unwrap();
        let git = GitRuntime::new(open_web_codex_git_runtime::GitRuntimeConfig::new(
            runner.path(),
        ))
        .unwrap();
        let workspace_id = Uuid::now_v7();
        let workspace = git.workspace_path(workspace_id);
        std::fs::create_dir_all(workspace.join("cards")).unwrap();
        std::fs::create_dir(workspace.join(".git")).unwrap();
        std::fs::write(workspace.join("cards/test.html"), "<button>test</button>").unwrap();

        let home = tempfile::tempdir().unwrap();
        let thread_id = Uuid::now_v7().to_string();
        let markdown = "Before\n\n::codex-inline-vis{workspace_file=\"cards/test.html\"}\n\nAfter";
        let materialized = materialize_workspace_html_references(
            markdown,
            &git,
            workspace_id,
            home.path(),
            &thread_id,
            "agent-message-1",
        )
        .await
        .unwrap();

        assert!(!materialized.contains("workspace_file"));
        let file = materialized
            .strip_prefix("Before\n\n::codex-inline-vis{file=\"")
            .and_then(|value| value.strip_suffix("\"}\n\nAfter"))
            .unwrap();
        assert!(file.starts_with("workspace-"));
        let payload = read(home.path(), &thread_id, file).await.unwrap();
        assert!(String::from_utf8(payload.bytes)
            .unwrap()
            .contains("<button>test</button>"));
    }

    #[tokio::test]
    async fn reads_only_regular_thread_scoped_files_and_wraps_html() {
        let home = tempfile::tempdir().unwrap();
        let thread_id = Uuid::now_v7().to_string();
        let thread_dir = visualization_thread_dir(home.path(), &thread_id).unwrap();
        fs::create_dir_all(&thread_dir).await.unwrap();
        fs::write(thread_dir.join("chart.html"), "<div id=\"chart\">ok</div>")
            .await
            .unwrap();

        let payload = read(home.path(), &thread_id, "chart.html").await.unwrap();
        let document = String::from_utf8(payload.bytes).unwrap();
        assert_eq!(payload.content_type, "text/html; charset=utf-8");
        assert!(document.contains("<div id=\"chart\">ok</div>"));
        assert!(document.contains("Content-Security-Policy"));
        assert!(!document.contains("<iframe"));
    }

    #[tokio::test]
    async fn validates_image_signatures_and_rejects_symlinks() {
        let home = tempfile::tempdir().unwrap();
        let thread_id = Uuid::now_v7().to_string();
        let thread_dir = visualization_thread_dir(home.path(), &thread_id).unwrap();
        fs::create_dir_all(&thread_dir).await.unwrap();
        fs::write(thread_dir.join("chart.png"), b"not a png")
            .await
            .unwrap();
        assert!(matches!(
            read(home.path(), &thread_id, "chart.png").await,
            Err(InlineVisualizationError::InvalidContent)
        ));

        #[cfg(unix)]
        {
            let outside = home.path().join("outside.html");
            fs::write(&outside, "<div>outside</div>").await.unwrap();
            std::os::unix::fs::symlink(outside, thread_dir.join("linked.html")).unwrap();
            let result = read(home.path(), &thread_id, "linked.html").await;
            assert!(
                matches!(result, Err(InlineVisualizationError::InvalidContent)),
                "{result:?}"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_symlinked_date_and_thread_directories() {
        let home = tempfile::tempdir().unwrap();
        let outside_root = tempfile::tempdir().unwrap();
        let thread_id = Uuid::now_v7().to_string();
        let components = visualization_components(&thread_id).unwrap();
        let day_dir = home
            .path()
            .join("visualizations")
            .join(&components[0])
            .join(&components[1])
            .join(&components[2]);
        let outside_day_dir = outside_root
            .path()
            .join("visualizations")
            .join(&components[0])
            .join(&components[1])
            .join(&components[2]);
        let outside_thread_dir = outside_day_dir.join(&components[3]);
        fs::create_dir_all(&day_dir).await.unwrap();
        fs::create_dir_all(&outside_thread_dir).await.unwrap();
        fs::write(outside_thread_dir.join("chart.html"), "<div>outside</div>")
            .await
            .unwrap();
        std::fs::remove_dir(&day_dir).unwrap();
        std::os::unix::fs::symlink(&outside_day_dir, &day_dir).unwrap();
        assert!(read(home.path(), &thread_id, "chart.html").await.is_err());

        std::fs::remove_file(&day_dir).unwrap();
        fs::create_dir_all(&day_dir).await.unwrap();
        let thread_dir = day_dir.join(&components[3]);
        std::os::unix::fs::symlink(&outside_thread_dir, &thread_dir).unwrap();
        assert!(read(home.path(), &thread_id, "chart.html").await.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn reads_from_the_opened_handle_after_the_path_is_replaced() {
        let home = tempfile::tempdir().unwrap();
        let thread_id = Uuid::now_v7().to_string();
        let thread_dir = visualization_thread_dir(home.path(), &thread_id).unwrap();
        std::fs::create_dir_all(&thread_dir).unwrap();
        std::fs::write(thread_dir.join("chart.html"), b"<div>original</div>").unwrap();
        std::fs::write(thread_dir.join("final.html"), b"<div>final-original</div>").unwrap();

        let visualizations_dir = open_visualizations_dir(home.path()).unwrap();
        let final_file =
            open_from_visualizations_dir(&visualizations_dir, &thread_id, "final.html").unwrap();
        let final_outside = home.path().join("final-outside.html");
        std::fs::write(&final_outside, b"<div>final-outside</div>").unwrap();
        std::fs::remove_file(thread_dir.join("final.html")).unwrap();
        std::os::unix::fs::symlink(&final_outside, thread_dir.join("final.html")).unwrap();
        let final_bytes = read_opened_file(final_file, MAX_HTML_BYTES).unwrap();
        assert_eq!(final_bytes, b"<div>final-original</div>");

        let outside_root = home.path().join("outside-root");
        let outside_thread_dir = visualization_thread_dir(&outside_root, &thread_id).unwrap();
        std::fs::create_dir_all(&outside_thread_dir).unwrap();
        std::fs::write(outside_thread_dir.join("chart.html"), b"<div>outside</div>").unwrap();
        let visualizations_path = home.path().join("visualizations");
        let original_visualizations_path = home.path().join("visualizations-original");
        std::fs::rename(&visualizations_path, &original_visualizations_path).unwrap();
        std::os::unix::fs::symlink(outside_root.join("visualizations"), &visualizations_path)
            .unwrap();

        let bytes = read_from_visualizations_dir(
            &visualizations_dir,
            &thread_id,
            "chart.html",
            MAX_HTML_BYTES,
        )
        .unwrap();
        assert_eq!(bytes, b"<div>original</div>");
    }
}
