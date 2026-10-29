use crate::{
    adaptive::{self, Plan},
    engine::{MODES, Run},
    model::{Profile, SessionRecord, unix_now},
    store::Writer,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Practice,
    Results,
    Insights,
    Settings,
    Help,
}
pub struct App {
    help_return: Screen,
    pending_saves: usize,
    pub profile: Profile,
    pub screen: Screen,
    pub run: Run,
    pub plan: Plan,
    pub result: Option<SessionRecord>,
    pub new_best: bool,
    pub quit: bool,
    pub notice: Option<String>,
    pub saving: bool,
    pub save_failed: bool,
    pub session_finished_at: f64,
    pub display_wpm: f64,
    pub now: f64,
    pub history_offset: usize,
}
impl App {
    pub fn new(mut profile: Profile) -> Self {
        profile.preferences.mode %= MODES.len();
        profile.preferences.theme %= 3;
        let mode = MODES[profile.preferences.mode];
        let plan = adaptive::plan(
            &profile,
            unix_now(),
            mode.words(),
            matches!(mode, crate::engine::Mode::Focus(_)),
            &mut rand::thread_rng(),
        );
        let run = Run::new(mode, plan.text.clone());
        Self {
            help_return: Screen::Practice,
            pending_saves: 0,
            profile,
            screen: Screen::Practice,
            run,
            plan,
            result: None,
            new_best: false,
            quit: false,
            notice: None,
            saving: false,
            save_failed: false,
            session_finished_at: 0.,
            display_wpm: 0.,
            now: 0.,
            history_offset: 0,
        }
    }
    pub fn restart(&mut self) {
        if !self.save_failed {
            self.notice = None;
        }
        let mode = MODES[self.profile.preferences.mode];
        self.plan = adaptive::plan(
            &self.profile,
            unix_now(),
            mode.words(),
            matches!(mode, crate::engine::Mode::Focus(_)),
            &mut rand::thread_rng(),
        );
        self.run = Run::new(mode, self.plan.text.clone());
        self.screen = Screen::Practice;
        self.display_wpm = 0.;
    }
    pub fn persist(&mut self, writer: &Writer) {
        match writer.save(&self.profile) {
            Ok(()) => {
                self.pending_saves += 1;
                self.saving = true;
            }
            Err(e) => {
                self.notice = Some(e.to_string());
                self.save_failed = true;
            }
        }
    }
    pub fn tick(&mut self, now: f64, dt: f64, writer: &Writer) {
        self.now = now;
        for outcome in writer.outcomes.try_iter() {
            self.pending_saves = self.pending_saves.saturating_sub(1);
            self.saving = self.pending_saves > 0;
            match outcome {
                Ok(()) => {
                    if self.save_failed {
                        self.notice = None;
                    }
                    self.save_failed = false;
                }
                Err(e) => {
                    self.save_failed = true;
                    self.notice = Some(e);
                }
            }
        }
        if self.screen == Screen::Practice {
            self.run.tick(now);
            if self.run.finished_at.is_some() {
                let at = unix_now();
                adaptive::learn(&mut self.profile, &self.run, at);
                let record = self.run.record(now, at, self.plan.targets.clone());
                self.new_best = record.wpm > self.profile.best_for(&record.mode);
                self.profile.best_wpm = self.profile.best().max(record.wpm);
                let previous = self.profile.best_for(&record.mode);
                self.profile
                    .best_by_mode
                    .insert(record.mode.clone(), previous.max(record.wpm));
                self.profile.sessions.push(record.clone());
                self.profile.total_sessions += 1;
                if self.profile.sessions.len() > 1000 {
                    self.profile.sessions.remove(0);
                }
                self.result = Some(record);
                self.screen = Screen::Results;
                self.session_finished_at = now;
                self.persist(writer);
            }
        }
        let goal = if self.screen == Screen::Results {
            self.result.as_ref().map_or(0., |s| s.wpm)
        } else {
            self.run.wpm(now)
        };
        self.display_wpm = if self.profile.preferences.reduced_motion {
            goal
        } else {
            self.display_wpm + (goal - self.display_wpm) * (1. - (-dt * 8.).exp())
        };
    }
    pub fn key(&mut self, key: KeyEvent, now: f64, writer: &Writer) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('c' | 'q')) {
            self.quit = true;
            return;
        }
        if ctrl && key.code == KeyCode::Char('s') {
            self.persist(writer);
            return;
        }
        if key.code == KeyCode::F(1) {
            if self.screen == Screen::Practice && self.run.started_at.is_some() {
                return;
            }
            if self.screen == Screen::Help {
                self.screen = self.help_return;
            } else {
                self.help_return = self.screen;
                self.screen = Screen::Help;
            }
            return;
        }
        match self.screen {
            Screen::Practice => match key.code {
                KeyCode::Esc => {
                    self.restart();
                    if !self.save_failed {
                        self.notice =
                            Some("Session reset · unfinished sessions are not recorded".into());
                    }
                }
                KeyCode::Tab if self.run.started_at.is_none() => {
                    self.profile.preferences.mode =
                        (self.profile.preferences.mode + 1) % MODES.len();
                    self.restart();
                    self.persist(writer);
                }
                KeyCode::F(2) if self.run.started_at.is_none() => self.screen = Screen::Insights,
                KeyCode::F(3) if self.run.started_at.is_none() => self.screen = Screen::Settings,
                KeyCode::Backspace => self.run.backspace(ctrl, now),
                KeyCode::Char('w') if ctrl => self.run.backspace(true, now),
                KeyCode::Char('r') if ctrl => self.restart(),
                KeyCode::Char(c)
                    if !key.modifiers.intersects(
                        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                    ) =>
                {
                    if !self.save_failed {
                        self.notice = None;
                    }
                    self.run.input(c, now)
                }
                _ => {}
            },
            Screen::Results => match key.code {
                KeyCode::Enter | KeyCode::Tab => self.restart(),
                KeyCode::Char('i') | KeyCode::F(2) => self.screen = Screen::Insights,
                KeyCode::Char('s') | KeyCode::F(3) => self.screen = Screen::Settings,
                KeyCode::Char('r') if self.save_failed => self.persist(writer),
                KeyCode::Esc => self.restart(),
                _ => {}
            },
            Screen::Insights => match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::F(2) => self.restart(),
                KeyCode::Down => {
                    self.history_offset =
                        (self.history_offset + 1).min(self.profile.sessions.len().saturating_sub(1))
                }
                KeyCode::Up => self.history_offset = self.history_offset.saturating_sub(1),
                _ => {}
            },
            Screen::Settings => match key.code {
                KeyCode::Char('t') | KeyCode::Right => {
                    self.profile.preferences.theme = (self.profile.preferences.theme + 1) % 3;
                    self.persist(writer);
                }
                KeyCode::Char('m') => {
                    self.profile.preferences.reduced_motion =
                        !self.profile.preferences.reduced_motion;
                    self.persist(writer);
                }
                KeyCode::Tab => {
                    self.profile.preferences.mode =
                        (self.profile.preferences.mode + 1) % MODES.len();
                    self.persist(writer);
                }
                KeyCode::Esc | KeyCode::Enter | KeyCode::F(3) => self.restart(),
                _ => {}
            },
            Screen::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                    self.screen = self.help_return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    #[test]
    fn complete_learn_save_reopen_and_help_never_duplicates_results() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut writer = Writer::new(store);
        let mut app = App::new(Profile::default());
        app.profile.preferences.mode = 2;
        app.restart();
        let target = app.run.target.clone();
        for (i, c) in target.iter().enumerate() {
            let now = i as f64 * 0.12;
            app.key(key(KeyCode::Char(*c)), now, &writer);
            app.tick(now, 0.12, &writer);
        }
        assert!(app.screen == Screen::Results);
        assert_eq!(app.profile.sessions.len(), 1);
        assert_eq!(app.profile.sessions[0].accuracy, 100.);
        assert!(!app.profile.skills.is_empty());
        app.key(key(KeyCode::F(1)), 20., &writer);
        app.key(key(KeyCode::Enter), 21., &writer);
        app.tick(22., 1., &writer);
        assert!(app.screen == Screen::Results);
        assert_eq!(app.profile.sessions.len(), 1);
        writer.finish().unwrap();
        drop(writer);
        let reopened = Store::open(dir.path()).unwrap().load().unwrap();
        assert_eq!(reopened.total_sessions, 1);
        assert_eq!(reopened.preferences.mode, 2);
        assert_eq!(reopened.skills.len(), app.profile.skills.len());
    }
    #[test]
    fn reset_and_mid_session_navigation_do_not_pollute_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut writer = Writer::new(Store::open(dir.path()).unwrap());
        let mut app = App::new(Profile::default());
        app.key(key(KeyCode::Char('x')), 0., &writer);
        app.key(key(KeyCode::F(2)), 1., &writer);
        assert!(app.screen == Screen::Practice);
        app.key(key(KeyCode::Esc), 2., &writer);
        app.tick(100., 1., &writer);
        assert_eq!(app.profile.total_sessions, 0);
        assert!(app.run.started_at.is_none());
        writer.finish().unwrap();
    }
}
