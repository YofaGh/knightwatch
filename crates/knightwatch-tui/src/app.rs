use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui_image::picker::Picker;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

use kw_clients::ApiClient;
use kw_utils::conv;

use crate::{
    confirm::{ConfirmOutcome, ConfirmState},
    events::AppEvent,
    login::{LoginOutcome, LoginState},
    pollers::{self, PollControl},
    tabs::{self, Tab},
};

pub struct App {
    pub should_quit: bool,
    pub selected_tab: usize,
    pub tab_hit_rects: Vec<(u16, u16)>,
    pub tabs: Vec<Box<dyn Tab>>,
    pub alarms: Option<kw_types::resources::AlarmSnapshot>,
    /// Set whenever something happened that changed what should be on
    /// screen. The main loop checks this after every event and only calls
    /// `terminal.draw` when it's true — that's what makes this genuinely
    /// event-driven instead of redrawing on a fixed tick regardless of
    /// whether anything changed.
    pub dirty: bool,
    /// `Some` whenever the login screen should be shown full-screen
    /// instead of the tabs — either the mandatory startup login, or one
    /// re-opened later (e.g. via Ctrl+L, or after a 401).
    pub login: Option<LoginState>,
    /// True only once a login has actually succeeded. Starts `false`
    /// unconditionally — even when `auth_enabled` is `false` — because a
    /// tab's command-bool can require login independently of whether the
    /// mandatory startup gate exists.
    pub authenticated: bool,
    api: Arc<ApiClient>,
    tx: Sender<AppEvent>,
    auth_enabled: bool,
    /// `Some` while the shutdown confirmation modal is open.
    pub confirm_shutdown: Option<ConfirmState>,
    /// Whether the server reports its Telegram bot as active. Pure
    /// display info, set once from `/info` and never changes at runtime.
    pub telegram_bot: bool,
    /// Screen-space rect of the shutdown button, recomputed every render
    /// so mouse clicks can be matched against it.
    pub shutdown_hit_rect: Option<Rect>,
}

impl App {
    pub fn new(
        picker: Picker,
        api: Arc<ApiClient>,
        info: &kw_types::api::InfoResponse,
        tx: Sender<AppEvent>,
    ) -> Self {
        let mut tabs: Vec<Box<dyn Tab>> = vec![];

        // --- Keyboard / mouse input ---
        pollers::spawn_input(tx.clone());

        // --- Screen tab ---
        if !info.blind {
            let control = PollControl::new_arc(5000);
            pollers::spawn_screen_poller(tx.clone(), api.clone(), control.clone());
            pollers::spawn_screen_poll_status_poller(tx.clone(), api.clone(), control.clone());
            tabs.push(Box::new(tabs::ScreenTab::new(
                picker,
                info.allow_screen_commands,
                api.clone(),
                tx.clone(),
                control,
            )));
        }
        let processes_control = PollControl::new_arc(2000);
        // --- Process Trees tab ---
        if !info.pid.is_empty() || info.allow_process_commands {
            pollers::spawn_process_trees_poller(tx.clone(), api.clone(), processes_control.clone());
            pollers::spawn_processes_poll_status_poller(
                tx.clone(),
                api.clone(),
                processes_control.clone(),
            );
            tabs.push(Box::new(tabs::ProcessesTab::new(
                info.allow_process_commands,
                api.clone(),
                tx.clone(),
                processes_control.clone(),
            )));
        }
        // --- System Resources tab ---
        if info.system_resources {
            let control = PollControl::new_arc(1000);
            pollers::spawn_system_resources_poller(tx.clone(), api.clone(), control.clone());
            pollers::spawn_system_resources_poll_status_poller(
                tx.clone(),
                api.clone(),
                control.clone(),
            );
            pollers::spawn_system_alarms_poller(tx.clone(), api.clone());
            pollers::spawn_system_resources_thresholds_poller(tx.clone(), api.clone());
            pollers::spawn_system_resources_refresh_mask_poller(tx.clone(), api.clone());
            tabs.push(Box::new(tabs::SystemResourcesTab::new(
                info.allow_system_resources_commands,
                api.clone(),
                tx.clone(),
                control,
            )));
        }
        // --- Systemd tab ---
        if info.systemd {
            let control = PollControl::new_arc(5000);
            pollers::spawn_systemd_poller(tx.clone(), api.clone(), control.clone());
            pollers::spawn_systemd_poll_status_poller(tx.clone(), api.clone(), control.clone());
            tabs.push(Box::new(tabs::SystemdTab::new(
                info.allow_systemd_commands,
                api.clone(),
                tx.clone(),
                control,
            )));
        }
        // --- Docker tab ---
        if info.docker {
            let control = PollControl::new_arc(5000);
            pollers::spawn_docker_poller(tx.clone(), api.clone(), control.clone());
            pollers::spawn_docker_poll_status_poller(tx.clone(), api.clone(), control.clone());
            tabs.push(Box::new(tabs::DockerTab::new(
                info.allow_docker_commands,
                api.clone(),
                tx.clone(),
                control,
            )));
        }
        // --- Top Processes tab ---
        if info.top_processes {
            let poll_config = Arc::new(std::sync::Mutex::new(
                tabs::TopProcessesPollConfig::default(),
            ));
            pollers::spawn_top_processes_poller(
                tx.clone(),
                api.clone(),
                poll_config.clone(),
                processes_control.clone(),
            );
            pollers::spawn_top_processes_poll_status_poller(
                tx.clone(),
                api.clone(),
                processes_control.clone(),
            );
            tabs.push(Box::new(tabs::TopProcessesTab::new(
                poll_config,
                info.allow_process_commands,
                api.clone(),
                tx.clone(),
                processes_control,
            )));
        }

        // Mandatory login gate: /info is assumed to be reachable
        // unauthenticated (it's the thing that tells us auth is needed
        // in the first place). Tabs/pollers are built regardless — they
        // already tolerate request failures and will just start working
        // once the token is set.
        let login = info.auth_enabled.then(|| LoginState::new(false));

        Self {
            should_quit: false,
            selected_tab: 0,
            tab_hit_rects: Vec::new(),
            tabs,
            alarms: None,
            dirty: false,
            login,
            api,
            tx,
            auth_enabled: info.auth_enabled,
            authenticated: false,
            confirm_shutdown: None,
            telegram_bot: info.telegram_bot,
            shutdown_hit_rect: None,
        }
    }

    /// Entry point for the event-driven main loop. Dispatches to the
    /// input handler or to the relevant tab(s), and marks `dirty` so the
    /// caller knows a redraw is due.
    pub fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Input(ev) => {
                self.handle_event(&ev);
                self.dirty = true;
            }
            AppEvent::LoginResult(result) => {
                if let Some(login) = &mut self.login {
                    match result {
                        Ok(()) => {
                            self.login = None;
                            self.authenticated = true;
                        }
                        Err(message) => login.fail(message),
                    }
                }
                self.dirty = true;
            }
            AppEvent::LogoutResult => {
                // login screen is already showing (set synchronously in
                // spawn_logout); nothing to do here besides redraw.
                self.dirty = true;
            }
            AppEvent::ShutdownResult(result) => {
                match result {
                    // Server is going down — nothing left to poll or show.
                    Ok(()) => self.should_quit = true,
                    Err(message) => {
                        if let Some(confirm) = &mut self.confirm_shutdown {
                            confirm.fail(message);
                        }
                    }
                }
                self.dirty = true;
            }
            AppEvent::ScreenImages(_) => {
                if let Some(tab) = self.get_tab_by_name("Screen")
                    && tab.handle_app_event(&event)
                {
                    self.dirty = true;
                }
            }
            AppEvent::SystemSnapshot(_) => {
                if let Some(tab) = self.get_tab_by_name("System Resources")
                    && tab.handle_app_event(&event)
                {
                    self.dirty = true;
                }
            }
            AppEvent::AlarmSnapshot(snap) => {
                self.alarms = Some(snap);
                self.dirty = true;
            }
            AppEvent::ThresholdsSynced(_) => {
                if let Some(tab) = self.get_tab_by_name("System Resources")
                    && tab.handle_app_event(&event)
                {
                    self.dirty = true;
                }
            }
            AppEvent::RefreshMaskSynced(_) => {
                if let Some(tab) = self.get_tab_by_name("System Resources")
                    && tab.handle_app_event(&event)
                {
                    self.dirty = true;
                }
            }
            AppEvent::DockerContainers(_) => {
                if let Some(tab) = self.get_tab_by_name("Docker")
                    && tab.handle_app_event(&event)
                {
                    self.dirty = true;
                }
            }
            AppEvent::SystemdSnapshot(_) => {
                if let Some(tab) = self.get_tab_by_name("Systemd")
                    && tab.handle_app_event(&event)
                {
                    self.dirty = true;
                }
            }
            AppEvent::ProcessTrees(_) => {
                if let Some(tab) = self.get_tab_by_name("Processes")
                    && tab.handle_app_event(&event)
                {
                    self.dirty = true;
                }
            }
            AppEvent::TopProcesses(_) => {
                if let Some(tab) = self.get_tab_by_name("Top Processes")
                    && tab.handle_app_event(&event)
                {
                    self.dirty = true;
                }
            }
            AppEvent::CommandResult { tab, .. } => {
                if let Some(t) = self.get_tab_by_name(tab)
                    && t.handle_app_event(&event)
                {
                    self.dirty = true;
                }
            }
            AppEvent::PollStatusSynced { tab: _tab } => {
                self.dirty = true;
            }
        }
    }

    pub fn tab_titles(&self) -> Vec<&'static str> {
        self.tabs.iter().map(|t| t.name()).collect()
    }

    pub fn get_tab_by_name(&mut self, name: &str) -> Option<&mut Box<dyn Tab>> {
        self.tabs.iter_mut().find(|t| t.name() == name)
    }

    /// True whenever commands can actually run: auth is off entirely, or
    /// it's on and login has succeeded.
    pub const fn logged_in(&self) -> bool {
        self.authenticated || !self.auth_enabled
    }

    pub fn handle_event(&mut self, event: &Event) {
        // Login screen, when present, owns all input.
        if let Some(login) = &mut self.login {
            match login.handle_event(event) {
                LoginOutcome::Submit { username, password } => self.spawn_login(username, password),
                LoginOutcome::Cancel => self.login = None,
                LoginOutcome::None => {}
            }
            return;
        }

        // Shutdown confirmation, when present, owns all input.
        if let Some(confirm) = &mut self.confirm_shutdown {
            match confirm.handle_event(event) {
                ConfirmOutcome::Confirm => self.spawn_shutdown(),
                ConfirmOutcome::Cancel => self.confirm_shutdown = None,
                ConfirmOutcome::None => {}
            }
            return;
        }

        let logged_in = self.logged_in();
        let consumed = self
            .tabs
            .get_mut(self.selected_tab)
            .is_some_and(|tab| tab.handle_event(event, logged_in));

        if consumed {
            return;
        }

        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.should_quit = true;
                }
                KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.login = Some(LoginState::new(true));
                }
                KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.spawn_logout();
                }
                KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.open_shutdown_confirm();
                }
                KeyCode::Tab | KeyCode::Right => self.next_tab(),
                KeyCode::BackTab | KeyCode::Left => self.prev_tab(),
                KeyCode::Char(c) if ('1'..='9').contains(&c) => {
                    let idx = conv::char_to_usize_saturating(c)
                        .saturating_sub(conv::char_to_usize_saturating('1'));
                    if idx < self.tabs.len() {
                        self.selected_tab = idx;
                    }
                }
                _ => {}
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                if let Some(rect) = self.shutdown_hit_rect
                    && crate::ui_helpers::mouse_hit(*mouse, rect)
                {
                    self.open_shutdown_confirm();
                    return;
                }
                if mouse.row < 3 {
                    for (i, &(x0, x1)) in self.tab_hit_rects.iter().enumerate() {
                        if mouse.column >= x0 && mouse.column < x1 {
                            self.selected_tab = i;
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn spawn_login(&self, username: String, password: String) {
        let api = self.api.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = api
                .login(username, password)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::LoginResult(result)).await;
        });
    }

    fn spawn_logout(&mut self) {
        let api = self.api.clone();
        let tx = self.tx.clone();
        // Reopen the login screen immediately — it's non-cancellable
        // only if the server actually requires auth; otherwise the user
        // can still Esc out and browse unauthenticated.
        self.login = Some(LoginState::new(!self.auth_enabled));
        self.authenticated = false;
        tokio::spawn(async move {
            if let Err(_e) = api.logout().await {
                // TODO: show logout fail in ui
                //eprintln!("logout failed: {e}");
            }
            let _ = tx.send(AppEvent::LogoutResult).await;
        });
    }

    fn open_shutdown_confirm(&mut self) {
        self.confirm_shutdown = Some(ConfirmState::new(
            "Shutdown Server",
            "Stop the knightwatch server?\nAll tabs will stop working.",
        ));
    }

    fn spawn_shutdown(&self) {
        let api = self.api.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = api.shutdown().await.map(|_| ()).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::ShutdownResult(result)).await;
        });
    }

    fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.selected_tab = (self.selected_tab.saturating_add(1))
                .checked_rem(self.tabs.len())
                .unwrap_or(0);
        }
    }

    fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.selected_tab = (self
                .selected_tab
                .saturating_add(self.tabs.len().saturating_sub(1)))
            .checked_rem(self.tabs.len())
            .unwrap_or(0);
        }
    }
}
