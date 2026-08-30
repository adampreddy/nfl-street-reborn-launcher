//! The supervisor: the process that owns a play session while the UI is gone.
//!
//! # Why this exists
//!
//! Game Mode Suspend means the launcher window closes when the game starts, so
//! the machine spends nothing on a UI nobody can see. On a Steam Deck that is
//! the difference between a stable frame rate and a bad one.
//!
//! But something still has to be there when the game exits. A player who quits
//! the game and lands on an empty desktop has been dropped. So a small process
//! stays behind, holds the session, and decides what happens next.
//!
//! # The two failures this module is built around
//!
//! **Two launchers at once.** Two copies composing mods over the same tree, or
//! writing the same memory card, corrupts things quietly. The first thing the
//! supervisor does is take a lock, and every operation that could damage shared
//! state is a method on the guard, so it is not reachable without one.
//!
//! **The supervisor dies with the game.** A crash that takes both down leaves a
//! session that was started and never finished. On the next launch that record
//! is still on disk, and it is the only evidence the player has that anything
//! went wrong. It is recovered, not discarded.
//!
//! # Ordering
//!
//! Finishing a session appends to the log *before* clearing the current-session
//! marker. If the process dies between the two, the session is recorded twice
//! and the duplicate is detectable by id. The other order loses it completely,
//! and a lost crash is a crash the player cannot report.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::platform::{DataDir, Error, ExitKind, LockGuard, Platform, Result};
use crate::runtime::{GameBuild, SessionOutcome};

/// Where the wall clock comes from.
///
/// Injectable so tests can assert on ordering and duration without sleeping.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> u64;
}

/// The real clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// A fixed clock that advances only when told to.
#[derive(Debug)]
pub struct FixedClock(std::sync::atomic::AtomicU64);

impl FixedClock {
    pub fn new(start_ms: u64) -> Self {
        Self(std::sync::atomic::AtomicU64::new(start_ms))
    }
    pub fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Which build a session was playing. Mirrors [`GameBuild`] because that type
/// is deliberately not serializable: what goes on disk is a format we own and
/// have to keep readable, and it should not change silently when an enum in the
/// runtime module is refactored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordedBuild {
    Offline,
    Online,
}

impl From<GameBuild> for RecordedBuild {
    fn from(b: GameBuild) -> Self {
        match b {
            GameBuild::Offline => RecordedBuild::Offline,
            GameBuild::Online => RecordedBuild::Online,
        }
    }
}

/// How a session ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionEnd {
    /// The game exited on its own, cleanly or with a non-zero status.
    Exited { code: Option<i32> },
    /// The game crashed. `detail` is what goes on the recovery page.
    Crashed { detail: String },
    /// Something killed the game, most often the player via the task manager.
    Terminated,
    /// The session was found unfinished on a later launch.
    ///
    /// This is not a state anything writes on purpose. It means the launcher
    /// went away without finishing, which almost always means it went down with
    /// the game.
    Interrupted,
}

impl SessionEnd {
    pub fn from_exit(kind: &ExitKind) -> Self {
        match kind {
            ExitKind::Normal => SessionEnd::Exited { code: Some(0) },
            ExitKind::Error { code } => SessionEnd::Exited { code: Some(*code) },
            ExitKind::Terminated => SessionEnd::Terminated,
            ExitKind::Crashed { detail } => SessionEnd::Crashed {
                detail: detail.clone(),
            },
        }
    }

    /// Whether the player should be shown the recovery page.
    ///
    /// An interrupted session counts. From the player's side it is
    /// indistinguishable from a crash, and pretending otherwise means the one
    /// case where the launcher itself fell over is the one case nobody hears
    /// about.
    pub fn needs_recovery(&self) -> bool {
        matches!(
            self,
            SessionEnd::Crashed { .. } | SessionEnd::Interrupted | SessionEnd::Terminated
        )
    }

    /// What the launcher should show once the session is over.
    pub fn outcome(&self) -> SessionOutcome {
        match self {
            SessionEnd::Crashed { detail } => SessionOutcome::ShowRecovery {
                detail: detail.clone(),
            },
            SessionEnd::Interrupted => SessionOutcome::ShowRecovery {
                detail: "The launcher stopped while the game was running. \
                         The session was recovered on the next start."
                    .to_string(),
            },
            SessionEnd::Terminated => SessionOutcome::ShowRecovery {
                detail: "The game was closed by something outside the launcher.".to_string(),
            },
            SessionEnd::Exited { .. } => SessionOutcome::ReturnToLauncher,
        }
    }
}

/// One play session, as it is written to disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    /// Unique within this install. Derived from the start time and the build,
    /// so a duplicated append is detectable.
    pub id: String,
    pub build: RecordedBuild,
    pub started_ms: u64,
    /// Absent while the session is still running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_ms: Option<u64>,
    /// Absent while the session is still running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<SessionEnd>,
}

impl SessionRecord {
    fn new(build: RecordedBuild, started_ms: u64) -> Self {
        let tag = match build {
            RecordedBuild::Offline => "offline",
            RecordedBuild::Online => "online",
        };
        Self {
            id: format!("{started_ms}-{tag}"),
            build,
            started_ms,
            ended_ms: None,
            end: None,
        }
    }

    /// How long the session lasted, if it has finished.
    pub fn duration_ms(&self) -> Option<u64> {
        self.ended_ms.map(|e| e.saturating_sub(self.started_ms))
    }

    pub fn is_finished(&self) -> bool {
        self.end.is_some()
    }
}

/// Filenames under the run directory. Named here so nothing else guesses them.
const LOCK_FILE: &str = "launcher.lock";
const CURRENT_SESSION: &str = "current-session.json";
const SESSION_LOG: &str = "sessions.jsonl";

/// Proof that this process is the only launcher running.
///
/// Everything that writes shared state hangs off this type. That is deliberate:
/// it makes "did we check for a second instance" a thing the compiler tracks
/// rather than a thing somebody has to remember.
#[derive(Debug)]
pub struct Supervisor {
    #[allow(dead_code)]
    lock: LockGuard,
    run_dir: PathBuf,
    clock: Box<dyn Clock>,
}

/// What happened when the launcher tried to start.
#[derive(Debug)]
pub enum Startup {
    /// This process owns the install.
    Owner(Box<Supervisor>),
    /// Another launcher is already running and holds the lock.
    AlreadyRunning { lock_path: PathBuf },
}

impl Supervisor {
    /// Try to become the one live launcher for this install.
    ///
    /// The lock is held for as long as the returned value is alive, and is
    /// released by the operating system if this process dies.
    pub fn acquire(platform: &Platform) -> Result<Startup> {
        Self::acquire_in(platform, SystemClock)
    }

    /// As [`Supervisor::acquire`], with an injected clock.
    pub fn acquire_in<C: Clock + 'static>(platform: &Platform, clock: C) -> Result<Startup> {
        let run_dir = platform.paths.data_root()?.join(DataDir::Cache.relative());
        Self::acquire_at(platform, &run_dir, clock)
    }

    /// As [`Supervisor::acquire`], rooted at an explicit directory.
    ///
    /// Exists so tests hold real locks in a temporary directory rather than
    /// fighting over the developer's own install.
    pub fn acquire_at<C: Clock + 'static>(
        platform: &Platform,
        run_dir: &Path,
        clock: C,
    ) -> Result<Startup> {
        // Refuse to run inside a 2.1 install.
        //
        // The data root used to be exactly that folder, which meant the
        // launcher's `mods/` was the player's JSGME `MODS/`, because Windows
        // paths are case insensitive. `mods/staging` and `mods/active` are
        // classed derived, so a cache purge could have deleted files out of a
        // real mod library. The default path is fixed, but the default is not
        // the only way to get here, so the dangerous case is refused rather
        // than merely avoided.
        let data_root = run_dir.parent().unwrap_or(run_dir);
        if crate::migration::LegacyInstall::detect(data_root).is_some() {
            return Err(Error::PlainIo(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "refusing to use {} for launcher data: it is an existing \
                     NFL Street Reborn 2.1 install. The launcher would write \
                     into the player's live Dolphin tree, and a cache purge \
                     could delete their mods. Migrate from it instead.",
                    data_root.display()
                ),
            )));
        }

        std::fs::create_dir_all(run_dir).map_err(|source| Error::Io {
            path: run_dir.to_path_buf(),
            source,
        })?;
        let lock_path = run_dir.join(LOCK_FILE);
        match platform.files.lock_exclusive(&lock_path)? {
            Some(lock) => Ok(Startup::Owner(Box::new(Supervisor {
                lock,
                run_dir: run_dir.to_path_buf(),
                clock: Box::new(clock),
            }))),
            None => Ok(Startup::AlreadyRunning { lock_path }),
        }
    }

    fn current_path(&self) -> PathBuf {
        self.run_dir.join(CURRENT_SESSION)
    }

    fn log_path(&self) -> PathBuf {
        self.run_dir.join(SESSION_LOG)
    }

    /// Recover a session that was started and never finished.
    ///
    /// Call this once at startup, before beginning anything new. Returns the
    /// recovered record, which is also appended to the log as
    /// [`SessionEnd::Interrupted`].
    ///
    /// This is only reachable through a held lock, which is the point. If a
    /// second launcher could call it, it would "recover" a session that the
    /// first launcher is in the middle of playing.
    pub fn recover_unfinished(&self) -> Result<Option<SessionRecord>> {
        let path = self.current_path();
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;

        let Ok(mut record) = serde_json::from_str::<SessionRecord>(&raw) else {
            // A truncated marker means the process died mid-write. There is
            // nothing to report except that it happened, and leaving the file
            // in place would block every future launch.
            std::fs::remove_file(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            return Ok(None);
        };

        record.ended_ms = Some(self.clock.now_ms());
        record.end = Some(SessionEnd::Interrupted);

        self.append_to_log(&record)?;
        std::fs::remove_file(&path).map_err(|source| Error::Io { path, source })?;
        Ok(Some(record))
    }

    /// Mark a session as started and write the on-disk marker.
    pub fn begin(&self, build: GameBuild) -> Result<SessionRecord> {
        let record = SessionRecord::new(build.into(), self.clock.now_ms());
        self.write_current(&record)?;
        Ok(record)
    }

    /// Mark a session as finished.
    ///
    /// The log append happens first. If this process dies between the append
    /// and the marker removal, the session is recorded twice with the same id,
    /// which is recoverable. The other order would lose it.
    pub fn finish(&self, mut record: SessionRecord, exit: &ExitKind) -> Result<SessionRecord> {
        record.ended_ms = Some(self.clock.now_ms());
        record.end = Some(SessionEnd::from_exit(exit));

        self.append_to_log(&record)?;

        let path = self.current_path();
        if path.exists() {
            std::fs::remove_file(&path).map_err(|source| Error::Io { path, source })?;
        }
        Ok(record)
    }

    /// Every finished session, oldest first.
    ///
    /// Unreadable lines are skipped rather than failing the whole read. A
    /// half-written line from a power cut should not make the play history
    /// unopenable.
    pub fn history(&self) -> Result<Vec<SessionRecord>> {
        let path = self.log_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = std::fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        Ok(raw
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<SessionRecord>(l).ok())
            .collect())
    }

    fn write_current(&self, record: &SessionRecord) -> Result<()> {
        let path = self.current_path();
        let json = serde_json::to_string_pretty(record).map_err(|e| {
            Error::PlainIo(std::io::Error::other(format!(
                "could not encode session record: {e}"
            )))
        })?;
        std::fs::write(&path, json).map_err(|source| Error::Io { path, source })
    }

    fn append_to_log(&self, record: &SessionRecord) -> Result<()> {
        use std::io::Write;
        let path = self.log_path();
        let line = serde_json::to_string(record).map_err(|e| {
            Error::PlainIo(std::io::Error::other(format!(
                "could not encode session record: {e}"
            )))
        })?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
        writeln!(file, "{line}").map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        // Session records are the only evidence a crash happened. Getting them
        // to the disk is worth the flush.
        file.sync_all().map_err(|source| Error::Io { path, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform() -> Platform {
        Platform::detect()
    }

    #[test]
    fn a_second_launcher_is_refused_while_the_first_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let p = platform();

        let first = Supervisor::acquire_at(&p, dir.path(), SystemClock).unwrap();
        assert!(matches!(first, Startup::Owner(_)));

        let second = Supervisor::acquire_at(&p, dir.path(), SystemClock).unwrap();
        assert!(
            matches!(second, Startup::AlreadyRunning { .. }),
            "two launchers must not both own the install"
        );
    }

    /// The whole reason for a lock rather than a recorded process id: when the
    /// holder goes away, the next launcher gets in. No cleanup step runs here,
    /// which is exactly the crash case.
    #[test]
    fn the_lock_is_released_when_the_holder_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let p = platform();

        let first = Supervisor::acquire_at(&p, dir.path(), SystemClock).unwrap();
        assert!(matches!(first, Startup::Owner(_)));
        drop(first);

        let second = Supervisor::acquire_at(&p, dir.path(), SystemClock).unwrap();
        assert!(
            matches!(second, Startup::Owner(_)),
            "a released lock must not keep the next launch out"
        );
    }

    #[test]
    fn a_clean_session_is_logged_and_leaves_no_marker() {
        let dir = tempfile::tempdir().unwrap();
        let p = platform();
        let Startup::Owner(s) =
            Supervisor::acquire_at(&p, dir.path(), FixedClock::new(1000)).unwrap()
        else {
            panic!("expected to own the lock");
        };

        let started = s.begin(GameBuild::Offline).unwrap();
        assert!(dir.path().join(CURRENT_SESSION).exists());

        let done = s.finish(started, &ExitKind::Normal).unwrap();
        assert!(done.is_finished());
        assert!(
            !dir.path().join(CURRENT_SESSION).exists(),
            "a finished session must not leave a marker that looks like a crash"
        );

        let history = s.history().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].end, Some(SessionEnd::Exited { code: Some(0) }));
    }

    /// The case this module exists for. The launcher went down with the game,
    /// so nothing ever called `finish`, and the only trace is the marker.
    #[test]
    fn an_unfinished_session_is_recovered_on_the_next_launch() {
        let dir = tempfile::tempdir().unwrap();
        let p = platform();

        {
            let Startup::Owner(s) =
                Supervisor::acquire_at(&p, dir.path(), FixedClock::new(5_000)).unwrap()
            else {
                panic!("expected to own the lock");
            };
            s.begin(GameBuild::Online).unwrap();
            // No finish. The process dies here.
        }

        let Startup::Owner(next) =
            Supervisor::acquire_at(&p, dir.path(), FixedClock::new(9_000)).unwrap()
        else {
            panic!("expected to own the lock after the previous holder died");
        };

        let recovered = next.recover_unfinished().unwrap().expect("a lost session");
        assert_eq!(recovered.end, Some(SessionEnd::Interrupted));
        assert_eq!(recovered.build, RecordedBuild::Online);
        assert_eq!(recovered.duration_ms(), Some(4_000));
        assert!(
            recovered.end.as_ref().unwrap().needs_recovery(),
            "the player must be told the launcher fell over"
        );

        assert_eq!(next.history().unwrap().len(), 1);
        assert!(
            next.recover_unfinished().unwrap().is_none(),
            "recovery must be idempotent, not report the same loss forever"
        );
    }

    #[test]
    fn a_crash_carries_its_detail_to_the_recovery_page() {
        let dir = tempfile::tempdir().unwrap();
        let p = platform();
        let Startup::Owner(s) = Supervisor::acquire_at(&p, dir.path(), FixedClock::new(0)).unwrap()
        else {
            panic!("expected to own the lock");
        };

        let started = s.begin(GameBuild::Offline).unwrap();
        let done = s
            .finish(
                started,
                &ExitKind::Crashed {
                    detail: "access violation at 0x80005A24".into(),
                },
            )
            .unwrap();

        match done.end.as_ref().unwrap().outcome() {
            SessionOutcome::ShowRecovery { detail } => {
                assert!(detail.contains("0x80005A24"), "detail was lost: {detail}");
            }
            other => panic!("a crash must open recovery, got {other:?}"),
        }
    }

    /// A non-zero exit is a failure, not a crash, and must not throw the player
    /// onto a recovery page for what may be a normal quit path.
    #[test]
    fn a_nonzero_exit_returns_quietly() {
        let end = SessionEnd::from_exit(&ExitKind::Error { code: 3 });
        assert_eq!(end, SessionEnd::Exited { code: Some(3) });
        assert!(!end.needs_recovery());
        assert_eq!(end.outcome(), SessionOutcome::ReturnToLauncher);
    }

    /// A marker truncated by a power cut must not brick every future launch.
    #[test]
    fn a_corrupt_marker_is_discarded_rather_than_blocking_startup() {
        let dir = tempfile::tempdir().unwrap();
        let p = platform();
        std::fs::write(dir.path().join(CURRENT_SESSION), b"{\"id\": \"trunc").unwrap();

        let Startup::Owner(s) = Supervisor::acquire_at(&p, dir.path(), FixedClock::new(1)).unwrap()
        else {
            panic!("expected to own the lock");
        };

        assert!(s.recover_unfinished().unwrap().is_none());
        assert!(
            !dir.path().join(CURRENT_SESSION).exists(),
            "an unreadable marker must be cleared, or it blocks startup forever"
        );
    }

    #[test]
    fn history_survives_a_half_written_line() {
        let dir = tempfile::tempdir().unwrap();
        let p = platform();
        let Startup::Owner(s) =
            Supervisor::acquire_at(&p, dir.path(), FixedClock::new(100)).unwrap()
        else {
            panic!("expected to own the lock");
        };

        let r = s.begin(GameBuild::Offline).unwrap();
        s.finish(r, &ExitKind::Normal).unwrap();
        // A power cut mid-append.
        std::fs::write(
            dir.path().join(SESSION_LOG),
            format!(
                "{}\n{{\"id\": \"half\n",
                serde_json::to_string(&s.history().unwrap()[0]).unwrap()
            ),
        )
        .unwrap();

        let history = s.history().unwrap();
        assert_eq!(history.len(), 1, "one good line must still be readable");
    }

    /// Session ids have to be stable and distinct enough that a duplicated
    /// append (the crash window between log and marker removal) is detectable.
    #[test]
    fn ids_distinguish_build_and_start_time() {
        let a = SessionRecord::new(RecordedBuild::Offline, 1234);
        let b = SessionRecord::new(RecordedBuild::Online, 1234);
        let c = SessionRecord::new(RecordedBuild::Offline, 1235);
        assert_ne!(a.id, b.id);
        assert_ne!(a.id, c.id);
    }

    /// The disk format is ours to keep readable. If a field is renamed, this
    /// fails and whoever renamed it has to think about existing installs.
    #[test]
    fn the_on_disk_shape_is_pinned() {
        let mut r = SessionRecord::new(RecordedBuild::Online, 7);
        let running = serde_json::to_value(&r).unwrap();
        assert_eq!(running["build"], "online");
        assert_eq!(running["started_ms"], 7);
        assert!(
            running.get("ended_ms").is_none(),
            "a running session must not claim an end time"
        );

        r.ended_ms = Some(9);
        r.end = Some(SessionEnd::Crashed {
            detail: "boom".into(),
        });
        let ended = serde_json::to_value(&r).unwrap();
        assert_eq!(ended["end"]["kind"], "crashed");
        assert_eq!(ended["end"]["detail"], "boom");
    }
    /// The launcher must not adopt a 2.1 install as its own data root. Doing so
    /// put `mods/staging` and `mods/active`, both purgeable, inside the
    /// player's JSGME mod library.
    #[test]
    fn a_legacy_install_is_refused_as_a_data_root() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("NFL Street Reborn");
        std::fs::create_dir_all(legacy.join("User").join("GC")).unwrap();
        std::fs::create_dir_all(legacy.join("MODS").join("!INSTLOGS")).unwrap();

        let p = platform();
        let err = Supervisor::acquire_at(&p, &legacy.join("cache"), SystemClock)
            .expect_err("a 2.1 install must not be adopted as launcher data");
        let msg = err.to_string();
        assert!(msg.contains("2.1 install"), "unhelpful message: {msg}");
        assert!(
            msg.contains("Migrate from it"),
            "no way forward given: {msg}"
        );
    }
}
