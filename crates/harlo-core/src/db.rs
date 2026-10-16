//! harlo's own state: projects, per-session flags and small settings.
//! Conversations stay in the harness session files.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: i64,
    pub path: PathBuf,
    pub name: String,
    /// Default harness for new sessions: `pi` or `omp`.
    pub harness: String,
    pub collapsed: bool,
    pub position: i64,
    /// Shell commands run (one per line) in each new worktree.
    pub setup: String,
    /// Its icon (Settings → Projects): "" the folder, or a preset's name
    /// (`project_icons::PRESETS`).
    pub icon: String,
    /// Its colour: "" none, or a theme colour's key (`keyword`, `string`…).
    pub color: String,
}

/// A worktree harlo created or found for a project. Kept after removal so
/// its sessions stay grouped under the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeRecord {
    pub path: PathBuf,
    pub project_id: i64,
    pub branch: String,
    /// Branch it was created from; empty if unknown (found, not created).
    pub base: String,
    pub removed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionMeta {
    pub archived: bool,
    pub pinned: bool,
    /// Unix seconds when the user last viewed the session in harlo.
    pub last_viewed_at: i64,
}

pub struct Db {
    conn: Connection,
}

pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("harlo").join("harlo.db")
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS projects (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    harness TEXT NOT NULL DEFAULT 'omp',
    collapsed INTEGER NOT NULL DEFAULT 0,
    position INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE TABLE IF NOT EXISTS session_meta (
    path TEXT PRIMARY KEY,
    archived INTEGER NOT NULL DEFAULT 0,
    pinned INTEGER NOT NULL DEFAULT 0,
    last_viewed_at INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS worktrees (
    path TEXT PRIMARY KEY,
    project_id INTEGER NOT NULL,
    branch TEXT NOT NULL,
    base TEXT NOT NULL DEFAULT '',
    removed INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    prompt TEXT NOT NULL,
    project_id INTEGER NOT NULL,
    kept INTEGER,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE TABLE IF NOT EXISTS run_lanes (
    run_id TEXT NOT NULL,
    idx INTEGER NOT NULL,
    harness TEXT NOT NULL,
    model TEXT NOT NULL DEFAULT '',
    cwd TEXT NOT NULL DEFAULT '',
    branch TEXT NOT NULL DEFAULT '',
    session_file TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (run_id, idx)
);
CREATE TABLE IF NOT EXISTS kv (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS schedules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    data TEXT NOT NULL
);
";

/// A multirun: one prompt sent to several lanes (sessions, usually in their
/// own worktrees).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunRecord {
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub project_id: i64,
    /// The lane chosen with "Keep this", once it is.
    pub kept: Option<i64>,
    pub created_at: i64,
    pub lanes: Vec<RunLane>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunLane {
    pub idx: i64,
    pub harness: String,
    /// `provider/model`, or empty for the harness's default.
    pub model: String,
    pub cwd: PathBuf,
    /// Worktree branch, or empty when the lane runs in the project folder.
    pub branch: String,
    /// Known once the harness has written it.
    pub session_file: PathBuf,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(SCHEMA)?;
        // Columns added after the first release.
        let has_setup: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('projects') WHERE name = 'setup'")?
            .exists([])?;
        if !has_setup {
            conn.execute_batch("ALTER TABLE projects ADD COLUMN setup TEXT NOT NULL DEFAULT ''")?;
        }
        let has_icon: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('projects') WHERE name = 'icon'")?
            .exists([])?;
        if !has_icon {
            conn.execute_batch("ALTER TABLE projects ADD COLUMN icon TEXT NOT NULL DEFAULT ''; ALTER TABLE projects ADD COLUMN color TEXT NOT NULL DEFAULT ''")?;
        }
        Ok(Self { conn })
    }

    pub fn projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, name, harness, collapsed, position, setup, icon, color FROM projects ORDER BY position, id")?;
        let rows = stmt.query_map([], |r| {
            Ok(Project {
                id: r.get(0)?,
                path: PathBuf::from(r.get::<_, String>(1)?),
                name: r.get(2)?,
                harness: r.get(3)?,
                collapsed: r.get(4)?,
                position: r.get(5)?,
                setup: r.get(6)?,
                icon: r.get(7)?,
                color: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Adds a project, or returns the existing one for that path.
    pub fn add_project(&self, path: &Path, harness: &str) -> Result<Project> {
        let path_str = path.to_string_lossy();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path_str.to_string());
        let position: i64 = self.conn.query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM projects", [], |r| r.get(0))?;
        self.conn.execute(
            "INSERT INTO projects (path, name, harness, position) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(path) DO NOTHING",
            params![path_str, name, harness, position],
        )?;
        let project = self
            .projects()?
            .into_iter()
            .find(|p| p.path == path)
            .context("project missing after insert")?;
        Ok(project)
    }

    pub fn remove_project(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn set_project_collapsed(&self, id: i64, collapsed: bool) -> Result<()> {
        self.conn.execute("UPDATE projects SET collapsed = ?2 WHERE id = ?1", params![id, collapsed])?;
        Ok(())
    }

    pub fn set_project_harness(&self, id: i64, harness: &str) -> Result<()> {
        self.conn.execute("UPDATE projects SET harness = ?2 WHERE id = ?1", params![id, harness])?;
        Ok(())
    }

    pub fn set_project_setup(&self, id: i64, setup: &str) -> Result<()> {
        self.conn.execute("UPDATE projects SET setup = ?2 WHERE id = ?1", params![id, setup])?;
        Ok(())
    }

    pub fn set_project_name(&self, id: i64, name: &str) -> Result<()> {
        self.conn.execute("UPDATE projects SET name = ?2 WHERE id = ?1", params![id, name])?;
        Ok(())
    }

    /// Its icon and colour (see `Project`).
    pub fn set_project_look(&self, id: i64, icon: &str, color: &str) -> Result<()> {
        self.conn.execute("UPDATE projects SET icon = ?2, color = ?3 WHERE id = ?1", params![id, icon, color])?;
        Ok(())
    }

    pub fn worktrees(&self) -> Result<Vec<WorktreeRecord>> {
        let mut stmt = self.conn.prepare("SELECT path, project_id, branch, base, removed FROM worktrees ORDER BY created_at")?;
        let rows = stmt.query_map([], |r| {
            Ok(WorktreeRecord {
                path: PathBuf::from(r.get::<_, String>(0)?),
                project_id: r.get(1)?,
                branch: r.get(2)?,
                base: r.get(3)?,
                removed: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Records a worktree, or refreshes its branch and clears `removed`.
    /// A known base is kept when the new record has none.
    pub fn upsert_worktree(&self, w: &WorktreeRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO worktrees (path, project_id, branch, base, removed) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET project_id = ?2, branch = ?3,
                 base = CASE WHEN ?4 = '' THEN base ELSE ?4 END, removed = ?5",
            params![w.path.to_string_lossy(), w.project_id, w.branch, w.base, w.removed],
        )?;
        Ok(())
    }

    pub fn mark_worktree_removed(&self, path: &Path) -> Result<()> {
        self.conn.execute("UPDATE worktrees SET removed = 1 WHERE path = ?1", [path.to_string_lossy()])?;
        Ok(())
    }

    pub fn session_meta(&self, path: &Path) -> Result<SessionMeta> {
        Ok(self
            .conn
            .query_row(
                "SELECT archived, pinned, last_viewed_at FROM session_meta WHERE path = ?1",
                [path.to_string_lossy()],
                |r| Ok(SessionMeta { archived: r.get(0)?, pinned: r.get(1)?, last_viewed_at: r.get(2)? }),
            )
            .optional()?
            .unwrap_or_default())
    }

    pub fn all_session_meta(&self) -> Result<Vec<(PathBuf, SessionMeta)>> {
        let mut stmt = self.conn.prepare("SELECT path, archived, pinned, last_viewed_at FROM session_meta")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                PathBuf::from(r.get::<_, String>(0)?),
                SessionMeta { archived: r.get(1)?, pinned: r.get(2)?, last_viewed_at: r.get(3)? },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set_session_meta(&self, path: &Path, meta: &SessionMeta) -> Result<()> {
        self.conn.execute(
            "INSERT INTO session_meta (path, archived, pinned, last_viewed_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET archived = ?2, pinned = ?3, last_viewed_at = ?4",
            params![path.to_string_lossy(), meta.archived, meta.pinned, meta.last_viewed_at],
        )?;
        Ok(())
    }

    pub fn add_run(&self, run: &RunRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO runs (id, title, prompt, project_id) VALUES (?1, ?2, ?3, ?4)",
            params![run.id, run.title, run.prompt, run.project_id],
        )?;
        for l in &run.lanes {
            self.upsert_lane(&run.id, l)?;
        }
        Ok(())
    }

    pub fn upsert_lane(&self, run_id: &str, l: &RunLane) -> Result<()> {
        self.conn.execute(
            "INSERT INTO run_lanes (run_id, idx, harness, model, cwd, branch, session_file) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(run_id, idx) DO UPDATE SET harness = ?3, model = ?4, cwd = ?5, branch = ?6,
                 session_file = CASE WHEN ?7 = '' THEN session_file ELSE ?7 END",
            params![run_id, l.idx, l.harness, l.model, l.cwd.to_string_lossy(), l.branch, l.session_file.to_string_lossy()],
        )?;
        Ok(())
    }

    pub fn set_lane_file(&self, run_id: &str, idx: i64, file: &Path) -> Result<()> {
        self.conn.execute("UPDATE run_lanes SET session_file = ?3 WHERE run_id = ?1 AND idx = ?2", params![run_id, idx, file.to_string_lossy()])?;
        Ok(())
    }

    pub fn set_run_kept(&self, run_id: &str, idx: i64) -> Result<()> {
        self.conn.execute("UPDATE runs SET kept = ?2 WHERE id = ?1", params![run_id, idx])?;
        Ok(())
    }

    pub fn delete_run(&self, run_id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM run_lanes WHERE run_id = ?1", [run_id])?;
        self.conn.execute("DELETE FROM runs WHERE id = ?1", [run_id])?;
        Ok(())
    }

    /// Every run, newest first, with its lanes.
    pub fn runs(&self) -> Result<Vec<RunRecord>> {
        let mut stmt = self.conn.prepare("SELECT id, title, prompt, project_id, kept, created_at FROM runs ORDER BY created_at DESC, rowid DESC")?;
        let mut runs: Vec<RunRecord> = stmt
            .query_map([], |r| {
                Ok(RunRecord { id: r.get(0)?, title: r.get(1)?, prompt: r.get(2)?, project_id: r.get(3)?, kept: r.get(4)?, created_at: r.get(5)?, lanes: Vec::new() })
            })?
            .collect::<Result<_, _>>()?;
        let mut lanes = self.conn.prepare("SELECT idx, harness, model, cwd, branch, session_file FROM run_lanes WHERE run_id = ?1 ORDER BY idx")?;
        for run in &mut runs {
            run.lanes = lanes
                .query_map([&run.id], |r| {
                    Ok(RunLane {
                        idx: r.get(0)?,
                        harness: r.get(1)?,
                        model: r.get(2)?,
                        cwd: PathBuf::from(r.get::<_, String>(3)?),
                        branch: r.get(4)?,
                        session_file: PathBuf::from(r.get::<_, String>(5)?),
                    })
                })?
                .collect::<Result<_, _>>()?;
        }
        Ok(runs)
    }

    /// Scheduled prompts, in the order they were made.
    pub fn schedules(&self) -> Result<Vec<crate::schedules::Schedule>> {
        let mut stmt = self.conn.prepare("SELECT id, data FROM schedules ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, data) = row?;
            let value: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
            if let Some(mut s) = crate::schedules::Schedule::from_json(&value) {
                s.id = id;
                out.push(s);
            }
        }
        Ok(out)
    }

    /// Saves a schedule; a new one (`id` 0) gets its id.
    pub fn save_schedule(&self, schedule: &mut crate::schedules::Schedule) -> Result<()> {
        let data = schedule.to_json().to_string();
        if schedule.id == 0 {
            self.conn.execute("INSERT INTO schedules (data) VALUES (?1)", [&data])?;
            schedule.id = self.conn.last_insert_rowid();
        } else {
            self.conn.execute("UPDATE schedules SET data = ?2 WHERE id = ?1", params![schedule.id, data])?;
        }
        Ok(())
    }

    pub fn delete_schedule(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM schedules WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_schedules() {
        use crate::schedules::{Repeat, Schedule};
        let db = Db::open_in_memory().unwrap();
        let mut s = Schedule { id: 0, title: "Morning".into(), prompt: "Summarize yesterday".into(), project_id: 3, harness: "omp".into(), model: String::new(),
            repeat: Repeat::Days { days: 0x1f, hour: 9, minute: 0 }, enabled: true, catch_up: true, last_run: None, next_run: Some(100) };
        db.save_schedule(&mut s).unwrap();
        assert!(s.id > 0);
        s.last_run = Some(50);
        db.save_schedule(&mut s).unwrap();
        let all = db.schedules().unwrap();
        assert_eq!(all, vec![s.clone()]);
        db.delete_schedule(s.id).unwrap();
        assert!(db.schedules().unwrap().is_empty());
    }

    #[test]
    fn projects_round_trip() {
        let db = Db::open_in_memory().unwrap();
        let a = db.add_project(Path::new("/home/u/alpha"), "omp").unwrap();
        let again = db.add_project(Path::new("/home/u/alpha"), "pi").unwrap();
        assert_eq!(a, again, "adding the same path returns the existing project");
        let b = db.add_project(Path::new("/home/u/beta"), "pi").unwrap();
        assert_eq!(a.name, "alpha");
        assert!(b.position > a.position);
        db.set_project_collapsed(b.id, true).unwrap();
        db.remove_project(a.id).unwrap();
        let all = db.projects().unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].collapsed);
    }

    #[test]
    fn worktree_records() {
        let db = Db::open_in_memory().unwrap();
        let p = db.add_project(Path::new("/w"), "omp").unwrap();
        db.set_project_setup(p.id, "cargo fetch").unwrap();
        assert_eq!(db.projects().unwrap()[0].setup, "cargo fetch");
        db.set_project_look(p.id, "rocket", "keyword").unwrap();
        db.set_project_name(p.id, "Rocket").unwrap();
        let p2 = &db.projects().unwrap()[0];
        assert_eq!((p2.icon.as_str(), p2.color.as_str(), p2.name.as_str()), ("rocket", "keyword", "Rocket"));
        let rec = WorktreeRecord { path: "/t/x".into(), project_id: p.id, branch: "x".into(), base: "main".into(), removed: false };
        db.upsert_worktree(&rec).unwrap();
        // Rediscovery without a base keeps the recorded base.
        db.upsert_worktree(&WorktreeRecord { base: String::new(), ..rec.clone() }).unwrap();
        db.mark_worktree_removed(Path::new("/t/x")).unwrap();
        let all = db.worktrees().unwrap();
        assert_eq!((all.len(), all[0].base.as_str(), all[0].removed), (1, "main", true));
    }

    #[test]
    fn stores_runs() {
        let db = Db::open_in_memory().unwrap();
        let lane = |idx: i64, model: &str| RunLane { idx, harness: "omp".into(), model: model.into(), cwd: format!("/w/{idx}").into(), branch: format!("harlo/run-{idx}"), session_file: PathBuf::new() };
        db.add_run(&RunRecord { id: "r1".into(), title: "Cache".into(), prompt: "Add a cache".into(), project_id: 1, lanes: vec![lane(0, "a/x"), lane(1, "b/y")], ..Default::default() }).unwrap();
        let mut l = lane(1, "b/y");
        l.session_file = "/s/1.jsonl".into();
        db.upsert_lane("r1", &l).unwrap();
        db.upsert_lane("r1", &lane(1, "b/y")).unwrap();
        db.set_run_kept("r1", 1).unwrap();
        let runs = db.runs().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].kept, Some(1));
        assert_eq!(runs[0].lanes[1].session_file, PathBuf::from("/s/1.jsonl"), "a known file is not cleared");
        db.delete_run("r1").unwrap();
        assert!(db.runs().unwrap().is_empty());
    }

    #[test]
    fn migrates_old_projects_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE projects (id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, name TEXT NOT NULL, harness TEXT NOT NULL DEFAULT 'omp', collapsed INTEGER NOT NULL DEFAULT 0, position INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL DEFAULT (unixepoch())); INSERT INTO projects (path, name) VALUES ('/old', 'old');").unwrap();
        let db = Db::init(conn).unwrap();
        assert_eq!(db.projects().unwrap()[0].setup, "");
        assert_eq!(db.projects().unwrap()[0].icon, "");
    }

    #[test]
    fn session_meta_defaults_and_updates() {
        let db = Db::open_in_memory().unwrap();
        let p = Path::new("/s/1.jsonl");
        assert_eq!(db.session_meta(p).unwrap(), SessionMeta::default());
        db.set_session_meta(p, &SessionMeta { archived: true, pinned: false, last_viewed_at: 5 }).unwrap();
        assert!(db.session_meta(p).unwrap().archived);
        db.set("window", "{}").unwrap();
        assert_eq!(db.get("window").unwrap().as_deref(), Some("{}"));
    }
}
