use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use dupdupninja_core::db::SqliteScanStore;
use dupdupninja_core::models::{FileListRow, ScanResult, ScanRootKind};
use dupdupninja_core::scan::{
    scan_to_sqlite_with_progress, ScanCancelToken, ScanConfig, ScanProgress,
};
use dupdupninja_core::{Error, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

pub fn run_web_server(port: u16) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(Error::Io)?;
    runtime.block_on(run_web_server_async(port))
}

async fn run_web_server_async(port: u16) -> Result<()> {
    let (events_tx, _) = broadcast::channel(200);
    let state = Arc::new(AppState {
        inner: Mutex::new(InnerState::new()),
        events_tx,
    });

    let app = Router::new()
        .route("/", get(ui_index))
        .route("/events", get(sse_events))
        .route("/scan", post(start_scan_handler))
        .route("/cancel/:id", post(cancel_scan_handler))
        .route("/api/jobs", get(list_jobs_handler))
        .route("/api/filesets/:id/matches", get(list_matches_handler))
        .route(
            "/api/filesets/:id/snapshots/:file_id/:index",
            get(snapshot_handler),
        )
        .with_state(state);

    let addr = ([127, 0, 0, 1], port).into();
    eprintln!("dupdupninja web UI listening on http://127.0.0.1:{port}/");

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .map_err(|err| Error::InvalidArgument(format!("server error: {err}")))?;

    Ok(())
}

struct AppState {
    inner: Mutex<InnerState>,
    events_tx: broadcast::Sender<ServerEvent>,
}

struct InnerState {
    next_id: u64,
    jobs: Vec<ScanJob>,
}

impl InnerState {
    fn new() -> Self {
        Self {
            next_id: 1,
            jobs: Vec::new(),
        }
    }
}

#[derive(Clone)]
struct ScanJob {
    id: u64,
    root: PathBuf,
    db_path: PathBuf,
    status: JobStatus,
    progress: Option<ScanProgress>,
    started_at: Instant,
    finished_at: Option<Instant>,
    error: Option<String>,
    cancel: ScanCancelToken,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum JobStatus {
    Pending,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerEvent {
    ScanStarted {
        id: u64,
        root: String,
        db_path: String,
    },
    ScanProgress {
        id: u64,
        progress: ProgressDto,
    },
    ScanDone {
        id: u64,
        stats: ScanStatsDto,
    },
    ScanCancelled {
        id: u64,
    },
    ScanError {
        id: u64,
        message: String,
    },
    MatchesUpdated {
        id: u64,
    },
}

#[derive(Clone, Debug, Serialize)]
struct ProgressDto {
    files_seen: u64,
    files_hashed: u64,
    files_skipped: u64,
    bytes_seen: u64,
    total_files: u64,
    total_bytes: u64,
    current_path: String,
    current_step: Option<String>,
    active_tasks: Vec<String>,
}

impl From<&ScanProgress> for ProgressDto {
    fn from(progress: &ScanProgress) -> Self {
        Self {
            files_seen: progress.files_seen,
            files_hashed: progress.files_hashed,
            files_skipped: progress.files_skipped,
            bytes_seen: progress.bytes_seen,
            total_files: progress.total_files,
            total_bytes: progress.total_bytes,
            current_path: progress.current_path.display().to_string(),
            current_step: progress.current_step.clone(),
            active_tasks: progress
                .active_tasks
                .iter()
                .map(|task| format!("{}: {}", task.step, task.path.display()))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct ScanStatsDto {
    files_seen: u64,
    files_hashed: u64,
    files_skipped: u64,
}

impl From<&ScanResult> for ScanStatsDto {
    fn from(result: &ScanResult) -> Self {
        Self {
            files_seen: result.stats.files_seen,
            files_hashed: result.stats.files_hashed,
            files_skipped: result.stats.files_skipped,
        }
    }
}

#[derive(Deserialize)]
struct ScanForm {
    root: String,
    root_kind: Option<String>,
    db_path: Option<String>,
    capture_snapshots: Option<String>,
    snapshots_per_video: Option<u32>,
    snapshot_max_dim: Option<u32>,
}

#[derive(Serialize)]
struct JobDto {
    id: u64,
    root: String,
    db_path: String,
    status: JobStatus,
    progress: Option<ProgressDto>,
    error: Option<String>,
    started_secs: u64,
    finished_secs: Option<u64>,
}

#[derive(Deserialize)]
struct MatchesQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Serialize)]
struct MatchGroupDto {
    key: String,
    files: Vec<FileDto>,
}

#[derive(Serialize)]
struct MatchesResponse {
    fileset_id: u64,
    groups: Vec<MatchGroupDto>,
}

#[derive(Serialize)]
struct FileDto {
    id: i64,
    path: String,
    size_bytes: u64,
    file_type: Option<String>,
}

async fn ui_index() -> Html<String> {
    Html(render_ui())
}

async fn sse_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = std::result::Result<Event, Infallible>>> {
    let rx = state.events_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|item| match item {
        Ok(event) => {
            let payload = match serde_json::to_string(&event) {
                Ok(payload) => payload,
                Err(_) => return None,
            };
            Some(Ok(Event::default().data(payload)))
        }
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(10)))
}

async fn start_scan_handler(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ScanForm>,
) -> axum::response::Response {
    match start_scan(state, form).await {
        Ok(_) => redirect_home(),
        Err(err) => (axum::http::StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    }
}

async fn cancel_scan_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> axum::response::Response {
    let mut guard = state.inner.lock().await;
    if let Some(job) = guard.jobs.iter_mut().find(|job| job.id == id) {
        if job.status == JobStatus::Running {
            job.cancel.cancel();
        }
    }

    redirect_home()
}

async fn list_jobs_handler(State(state): State<Arc<AppState>>) -> Json<Vec<JobDto>> {
    let guard = state.inner.lock().await;
    let jobs = guard
        .jobs
        .iter()
        .rev()
        .map(|job| JobDto {
            id: job.id,
            root: job.root.display().to_string(),
            db_path: job.db_path.display().to_string(),
            status: job.status,
            progress: job.progress.as_ref().map(ProgressDto::from),
            error: job.error.clone(),
            started_secs: job.started_at.elapsed().as_secs(),
            finished_secs: job
                .finished_at
                .map(|done| done.duration_since(job.started_at).as_secs()),
        })
        .collect();
    Json(jobs)
}

async fn list_matches_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
    Query(query): Query<MatchesQuery>,
) -> impl IntoResponse {
    let db_path = {
        let guard = state.inner.lock().await;
        guard
            .jobs
            .iter()
            .find(|job| job.id == id)
            .map(|job| job.db_path.clone())
    };

    let Some(db_path) = db_path else {
        return (axum::http::StatusCode::NOT_FOUND, "Unknown fileset").into_response();
    };

    let limit = query.limit.unwrap_or(200).clamp(1, 2000);
    let offset = query.offset.unwrap_or(0);

    let result = tokio::task::spawn_blocking(move || {
        let store = SqliteScanStore::open(&db_path)?;
        let rows = store.list_files_with_duplicates(limit, offset)?;
        Ok::<_, Error>(group_matches(rows))
    })
    .await;

    match result {
        Ok(Ok(groups)) => Json(MatchesResponse {
            fileset_id: id,
            groups,
        })
        .into_response(),
        Ok(Err(err)) => (axum::http::StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "background task failed",
        )
            .into_response(),
    }
}

async fn snapshot_handler(
    State(state): State<Arc<AppState>>,
    Path((id, file_id, index)): Path<(u64, i64, u32)>,
) -> impl IntoResponse {
    let db_path = {
        let guard = state.inner.lock().await;
        guard
            .jobs
            .iter()
            .find(|job| job.id == id)
            .map(|job| job.db_path.clone())
    };

    let Some(db_path) = db_path else {
        return (axum::http::StatusCode::NOT_FOUND, "Unknown fileset").into_response();
    };

    let snapshot = tokio::task::spawn_blocking(move || {
        let store = SqliteScanStore::open(&db_path)?;
        let snaps = store.list_file_snapshots(file_id)?;
        Ok::<_, Error>(snaps.into_iter().find(|snap| snap.snapshot_index == index))
    })
    .await;

    match snapshot {
        Ok(Ok(Some(snap))) => (
            [(axum::http::header::CONTENT_TYPE, "image/avif")],
            snap.image_avif,
        )
            .into_response(),
        Ok(Ok(None)) => (axum::http::StatusCode::NOT_FOUND, "Snapshot not found").into_response(),
        Ok(Err(err)) => (axum::http::StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "background task failed",
        )
            .into_response(),
    }
}

async fn start_scan(state: Arc<AppState>, form: ScanForm) -> Result<()> {
    let root = form.root.trim();
    if root.is_empty() {
        return Err(Error::InvalidArgument("root path is required".into()));
    }

    let root_path = PathBuf::from(root);
    let root_kind = match form.root_kind.as_deref() {
        Some("drive") => ScanRootKind::Drive,
        _ => ScanRootKind::Folder,
    };

    let db_path = form
        .db_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| scan_db_path(&root_path));

    let cfg = ScanConfig {
        root: root_path.clone(),
        root_kind,
        hash_files: true,
        perceptual_hashes: true,
        capture_snapshots: form.capture_snapshots.is_some(),
        snapshots_per_video: form.snapshots_per_video.unwrap_or(3).clamp(1, 10),
        snapshot_max_dim: form.snapshot_max_dim.unwrap_or(1024).clamp(128, 4096),
        concurrent_processing: true,
    };

    let (id, cancel) = {
        let mut guard = state.inner.lock().await;
        let id = guard.next_id;
        guard.next_id += 1;
        let cancel = ScanCancelToken::new();
        guard.jobs.push(ScanJob {
            id,
            root: cfg.root.clone(),
            db_path: db_path.clone(),
            status: JobStatus::Pending,
            progress: None,
            started_at: Instant::now(),
            finished_at: None,
            error: None,
            cancel: cancel.clone(),
        });
        (id, cancel)
    };

    let _ = state.events_tx.send(ServerEvent::ScanStarted {
        id,
        root: root_path.display().to_string(),
        db_path: db_path.display().to_string(),
    });

    let state_for_task = state.clone();
    tokio::task::spawn_blocking(move || {
        update_job(&state_for_task, id, |job| {
            job.status = JobStatus::Running;
            job.started_at = Instant::now();
        });

        let store = match SqliteScanStore::open(&db_path) {
            Ok(store) => store,
            Err(err) => {
                update_job(&state_for_task, id, |job| {
                    job.status = JobStatus::Failed;
                    job.error = Some(format!("Failed to open DB: {err}"));
                    job.finished_at = Some(Instant::now());
                });
                let _ = state_for_task.events_tx.send(ServerEvent::ScanError {
                    id,
                    message: "Failed to open DB".to_string(),
                });
                return;
            }
        };

        let result = scan_to_sqlite_with_progress(&cfg, &store, Some(&cancel), |progress| {
            update_job(&state_for_task, id, |job| {
                job.progress = Some(progress.clone());
            });
            let _ = state_for_task.events_tx.send(ServerEvent::ScanProgress {
                id,
                progress: ProgressDto::from(progress),
            });
        });

        match result {
            Ok(result) => {
                update_job(&state_for_task, id, |job| {
                    job.status = JobStatus::Completed;
                    job.finished_at = Some(Instant::now());
                });
                let _ = state_for_task.events_tx.send(ServerEvent::ScanDone {
                    id,
                    stats: ScanStatsDto::from(&result),
                });
                let _ = state_for_task
                    .events_tx
                    .send(ServerEvent::MatchesUpdated { id });
            }
            Err(Error::Cancelled) => {
                update_job(&state_for_task, id, |job| {
                    job.status = JobStatus::Cancelled;
                    job.finished_at = Some(Instant::now());
                });
                let _ = state_for_task
                    .events_tx
                    .send(ServerEvent::ScanCancelled { id });
            }
            Err(err) => {
                update_job(&state_for_task, id, |job| {
                    job.status = JobStatus::Failed;
                    job.error = Some(err.to_string());
                    job.finished_at = Some(Instant::now());
                });
                let _ = state_for_task.events_tx.send(ServerEvent::ScanError {
                    id,
                    message: err.to_string(),
                });
            }
        }
    });

    Ok(())
}

fn update_job<F>(state: &Arc<AppState>, id: u64, f: F)
where
    F: FnOnce(&mut ScanJob),
{
    if let Ok(mut guard) = state.inner.try_lock() {
        if let Some(job) = guard.jobs.iter_mut().find(|job| job.id == id) {
            f(job);
        }
        if guard.jobs.len() > 50 {
            let excess = guard.jobs.len() - 50;
            guard.jobs.drain(0..excess);
        }
    }
}

fn group_matches(rows: Vec<FileListRow>) -> Vec<MatchGroupDto> {
    let mut groups: HashMap<String, Vec<FileDto>> = HashMap::new();
    for row in rows {
        let key = if let Some(hash) = row.blake3 {
            format!("blake3:{}", hex_encode(&hash))
        } else if let Some(hash) = row.sha256 {
            format!("sha256:{}", hex_encode(&hash))
        } else {
            continue;
        };
        groups.entry(key).or_default().push(FileDto {
            id: row.id,
            path: row.path.display().to_string(),
            size_bytes: row.size_bytes,
            file_type: row.file_type,
        });
    }
    let mut out: Vec<MatchGroupDto> = groups
        .into_iter()
        .filter_map(|(key, files)| {
            if files.len() < 2 {
                None
            } else {
                Some(MatchGroupDto { key, files })
            }
        })
        .collect();
    out.sort_by(|a, b| b.files.len().cmp(&a.files.len()));
    out
}

fn hex_encode(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

fn scan_db_path(root: &std::path::Path) -> PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let name = sanitize_fileset_name(root);
    let file_name = format!("{name}-{ts}.ddn");
    let mut base = default_fileset_dir();
    if std::fs::create_dir_all(&base).is_err() {
        let mut fallback = std::env::temp_dir();
        fallback.push(file_name);
        return fallback;
    }
    base.push(file_name);
    base
}

fn sanitize_fileset_name(root: &std::path::Path) -> String {
    let raw = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("fileset");
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() || ch == '.' {
            out.push('-');
        }
    }
    if out.is_empty() {
        "fileset".to_string()
    } else {
        out
    }
}

fn default_fileset_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    let mut path = base;
    path.push("dupdupninja");
    path.push("filesets");
    path
}

fn render_ui() -> String {
    let default_dir = default_fileset_dir();
    let html = r##"<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>dupdupninja web UI</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 24px; background: #f9fafb; color: #111; }}
    h1 {{ margin-bottom: 4px; }}
    fieldset {{ border: 1px solid #ddd; padding: 16px; border-radius: 8px; background: #fff; }}
    label {{ display: block; margin: 10px 0 6px; font-weight: 600; }}
    input[type=text], select {{ width: 100%; max-width: 720px; padding: 6px 8px; }}
    .row {{ display: flex; gap: 16px; flex-wrap: wrap; }}
    .row > div {{ flex: 1 1 220px; }}
    button {{ padding: 6px 12px; }}
    table {{ border-collapse: collapse; width: 100%; margin-top: 12px; background: #fff; }}
    th, td {{ border: 1px solid #e3e3e3; padding: 6px 8px; text-align: left; font-size: 14px; }}
    .muted {{ color: #555; font-size: 13px; }}
    .matches {{ margin-top: 20px; }}
    .group {{ border: 1px solid #ddd; padding: 12px; border-radius: 8px; background: #fff; margin-bottom: 12px; }}
    .file {{ display: flex; align-items: center; gap: 12px; margin: 6px 0; }}
    .file img {{ width: 120px; height: 90px; object-fit: contain; background: #111; }}
  </style>
</head>
<body>
  <h1>dupdupninja</h1>
  <p class="muted">Live scan status + duplicate groups. Screenshots stream directly from the .ddn database.</p>
  <fieldset>
    <legend>Start scan</legend>
    <form id="scan-form" method="post" action="/scan">
      <label>Root path</label>
      <input type="text" name="root" placeholder="/path/to/folder" required>
      <div class="row">
        <div>
          <label>Root kind</label>
          <select name="root_kind">
            <option value="folder">Folder</option>
            <option value="drive">Drive</option>
          </select>
        </div>
        <div>
          <label>Snapshots per video (1-10)</label>
          <input type="text" name="snapshots_per_video" value="3">
        </div>
        <div>
          <label>Snapshot max size (128-4096)</label>
          <input type="text" name="snapshot_max_dim" value="1024">
        </div>
      </div>
      <label>Fileset DB path (optional)</label>
      <input type="text" name="db_path" placeholder="__DEFAULT_DIR__">
      <label><input type="checkbox" name="capture_snapshots" checked> Capture video snapshots</label>
      <button type="submit">Start scan</button>
    </form>
  </fieldset>

  <h2>Scans</h2>
  <table id="jobs-table">
    <thead>
      <tr><th>ID</th><th>Status</th><th>Root</th><th>DB</th><th>Progress</th><th>Actions</th></tr>
    </thead>
    <tbody></tbody>
  </table>

  <div class="matches">
    <h2>Duplicate groups</h2>
    <div id="matches"></div>
  </div>

<script>
const jobsTable = document.querySelector('#jobs-table tbody');
const matchesContainer = document.querySelector('#matches');
let latestFilesetId = null;

async function loadJobs() {
  const res = await fetch('/api/jobs');
  const jobs = await res.json();
  renderJobs(jobs);
}

function renderJobs(jobs) {
  jobsTable.innerHTML = '';
  for (const job of jobs) {
    const row = document.createElement('tr');
    row.innerHTML = `
      <td>${job.id}</td>
      <td>${job.status}${job.error ? `: ${job.error}` : ''}</td>
      <td>${job.root}</td>
      <td>${job.db_path}</td>
      <td>${job.progress ? `${job.progress.files_seen}/${job.progress.total_files} (${job.progress.current_step || 'scan'}: ${job.progress.current_path})` : '-'}</td>
      <td>${job.status === 'running' ? `<button data-cancel="${job.id}">Cancel</button>` : '-'}</td>
    `;
    jobsTable.appendChild(row);
  }

  jobsTable.querySelectorAll('button[data-cancel]').forEach(btn => {
    btn.addEventListener('click', async () => {
      await fetch(`/cancel/${btn.dataset.cancel}`, { method: 'POST' });
    });
  });
}

async function loadMatches(filesetId) {
  if (!filesetId) return;
  const res = await fetch(`/api/filesets/${filesetId}/matches`);
  if (!res.ok) return;
  const data = await res.json();
  renderMatches(data.groups, filesetId);
}

function renderMatches(groups, filesetId) {
  matchesContainer.innerHTML = '';
  if (!groups.length) {
    matchesContainer.textContent = 'No duplicates yet.';
    return;
  }
  for (const group of groups) {
    const wrap = document.createElement('div');
    wrap.className = 'group';
    wrap.innerHTML = `<div class="muted">${group.key}</div>`;
    for (const file of group.files) {
      const fileRow = document.createElement('div');
      fileRow.className = 'file';
      const img = document.createElement('img');
      img.src = `/api/filesets/${filesetId}/snapshots/${file.id}/0`;
      img.onerror = () => { img.remove(); };
      const meta = document.createElement('div');
      meta.innerHTML = `<div>${file.path}</div><div class="muted">${file.size_bytes} bytes</div>`;
      fileRow.appendChild(img);
      fileRow.appendChild(meta);
      wrap.appendChild(fileRow);
    }
    matchesContainer.appendChild(wrap);
  }
}

const source = new EventSource('/events');
source.onmessage = (event) => {
  try {
    const payload = JSON.parse(event.data);
    if (payload.type === 'scan_done' || payload.type === 'matches_updated') {
      latestFilesetId = payload.id;
      loadMatches(latestFilesetId);
    }
    if (payload.type === 'scan_progress' || payload.type === 'scan_started') {
      latestFilesetId = payload.id;
    }
    loadJobs();
  } catch (err) {
    console.error(err);
  }
};

const form = document.querySelector('#scan-form');
form.addEventListener('submit', async (e) => {
  e.preventDefault();
  const data = new URLSearchParams(new FormData(form));
  await fetch('/scan', { method: 'POST', body: data, headers: { 'Content-Type': 'application/x-www-form-urlencoded' }});
  form.reset();
});

loadJobs();
</script>
</body>
</html>"##;
    html.replace("__DEFAULT_DIR__", &default_dir.display().to_string())
}

fn redirect_home() -> axum::response::Response {
    (
        axum::http::StatusCode::SEE_OTHER,
        [(axum::http::header::LOCATION, "/")],
    )
        .into_response()
}
