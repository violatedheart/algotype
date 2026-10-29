use crate::model::Profile;
use anyhow::{Context, Result, bail};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

pub struct Store {
    dir: PathBuf,
    _lock: File,
}
impl Store {
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir).context("Cannot create the data directory")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("profile.lock"))?;
        lock.try_lock_exclusive()
            .context("Another Algotype instance is using this profile. Close it first.")?;
        Ok(Self {
            dir: dir.into(),
            _lock: lock,
        })
    }
    pub fn load(&self) -> Result<Profile> {
        let path = self.dir.join("profile.json");
        if !path.exists() {
            return Ok(Profile::default());
        }
        let p:Profile=serde_json::from_slice(&fs::read(&path)?).with_context(||format!("Cannot read {}. Your data has been preserved. Restore profile.backup.json manually or choose --data-dir with a new directory.",path.display()))?;
        if p.schema != 1 {
            bail!(
                "Unsupported profile schema {}; this application supports schema 1 and will not overwrite it",
                p.schema
            );
        }
        if !p.best_wpm.is_finite()
            || p.best_wpm < 0.
            || p.best_by_mode.values().any(|v| !v.is_finite() || *v < 0.)
            || p.skills.values().any(|s| {
                s.errors > s.attempts
                    || !s.latency_ms.is_finite()
                    || s.latency_ms < 0.
                    || !(0.0..=1.0).contains(&s.recent_accuracy)
                    || !s.stability_days.is_finite()
                    || s.stability_days <= 0.
            })
            || p.sessions
                .iter()
                .any(|s| !s.wpm.is_finite() || s.wpm < 0. || !(0.0..=100.0).contains(&s.accuracy))
        {
            bail!("Invalid profile statistics; refusing to overwrite existing data");
        }
        Ok(p)
    }
    pub fn save(&self, profile: &Profile) -> Result<()> {
        let target = self.dir.join("profile.json");
        let temporary = self.dir.join("profile.pending.json");
        let mut f = File::create(&temporary)?;
        serde_json::to_writer(&mut f, profile)?;
        f.sync_all()?;
        if target.exists() {
            // Copy the previous committed state before replacing the primary.
            let backup = self.dir.join("profile.backup.pending");
            fs::copy(&target, &backup)?;
            File::open(&backup)?.sync_all()?;
            fs::rename(backup, self.dir.join("profile.backup.json"))?;
        }
        fs::rename(temporary, target)?;
        #[cfg(unix)]
        File::open(&self.dir)?.sync_all()?;
        Ok(())
    }
}
// Disk work runs after the last keystroke, on a dedicated thread. Shutdown joins it.
pub struct Writer {
    sender: Option<Sender<Profile>>,
    pub outcomes: Receiver<Result<(), String>>,
    thread: Option<JoinHandle<()>>,
}
impl Writer {
    pub fn new(store: Store) -> Self {
        let (tx, rx) = mpsc::channel();
        let (results, outcomes) = mpsc::channel();
        let thread = thread::spawn(move || {
            while let Ok(profile) = rx.recv() {
                let result = store
                    .save(&profile)
                    .map_err(|e| format!("Save failed: {e:#}"));
                if results.send(result).is_err() {
                    break;
                }
            }
        });
        Self {
            sender: Some(tx),
            outcomes,
            thread: Some(thread),
        }
    }
    pub fn save(&self, profile: &Profile) -> Result<()> {
        self.sender
            .as_ref()
            .context("Storage worker stopped")?
            .send(profile.clone())
            .context("Storage worker stopped")
    }
    pub fn finish(&mut self) -> Result<()> {
        self.sender.take();
        if let Some(t) = self.thread.take() {
            t.join()
                .map_err(|_| anyhow::anyhow!("Storage worker panicked"))?;
        }
        for outcome in self.outcomes.try_iter() {
            outcome.map_err(anyhow::Error::msg)?;
        }
        Ok(())
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_or_interrupted_write_preserves_last_commit() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut p = Profile {
            total_sessions: 5,
            ..Profile::default()
        };
        store.save(&p).unwrap();
        fs::create_dir(dir.path().join("profile.pending.json")).unwrap();
        p.total_sessions = 6;
        assert!(store.save(&p).is_err());
        assert_eq!(store.load().unwrap().total_sessions, 5);
        fs::remove_dir(dir.path().join("profile.pending.json")).unwrap();
        fs::write(dir.path().join("profile.pending.json"), "interrupted").unwrap();
        drop(store);
        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(reopened.load().unwrap().total_sessions, 5);
        reopened.save(&p).unwrap();
        assert_eq!(reopened.load().unwrap().total_sessions, 6);
    }
    #[test]
    fn committed_progress_survives_restart_and_has_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut p = store.load().unwrap();
        p.total_sessions = 7;
        store.save(&p).unwrap();
        p.total_sessions = 8;
        store.save(&p).unwrap();
        drop(store);
        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(reopened.load().unwrap().total_sessions, 8);
        let backup: Profile =
            serde_json::from_slice(&fs::read(dir.path().join("profile.backup.json")).unwrap())
                .unwrap();
        assert_eq!(backup.total_sessions, 7);
    }
    #[test]
    fn concurrent_writers_and_corruption_are_rejected_without_data_loss() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert!(Store::open(dir.path()).is_err());
        fs::write(dir.path().join("profile.json"), "broken").unwrap();
        assert!(store.load().is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("profile.json")).unwrap(),
            "broken"
        );
    }
    #[test]
    fn unknown_schema_is_never_silently_downgraded() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        fs::write(dir.path().join("profile.json"), r#"{"schema":99}"#).unwrap();
        assert!(store.load().is_err());
    }
    #[test]
    fn worker_flushes_on_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut w = Writer::new(store);
        let p = Profile {
            total_sessions: 9,
            ..Profile::default()
        };
        w.save(&p).unwrap();
        w.finish().unwrap();
        drop(w);
        assert_eq!(
            Store::open(dir.path())
                .unwrap()
                .load()
                .unwrap()
                .total_sessions,
            9
        );
    }
}
