use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui_image::picker::Picker;
use std::sync::Arc;

use crate::{
    events::AppEvent,
    login::{LoginOutcome, LoginState},
    pollers,
    tabs::{self, Tab},
};

pub struct App {
    pub should_quit: bool,
    pub selected_tab: usize,
    pub tab_hit_rects: Vec<(u16, u16)>,
    pub tabs: Vec<Box<dyn Tab>>,
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
    api: Arc<kw_clients::ApiClient>,
    tx: tokio::sync::mpsc::Sender<AppEvent>,
    auth_enabled: bool,
}

impl App {
    pub fn new(
        picker: Picker,
        api: Arc<kw_clients::ApiClient>,
        info: kw_types::api::InfoResponse,
        tx: tokio::sync::mpsc::Sender<AppEvent>,
    ) -> Self {
        let mut tabs: Vec<Box<dyn Tab>> = vec![];

        // --- Keyboard / mouse input ---
        pollers::spawn_input(tx.clone());

        // --- Screen tab ---
        if !info.blind {
            pollers::spawn_screen_poller(tx.clone(), api.clone());
            tabs.push(Box::new(tabs::ScreenTab::new(
                picker,
                info.allow_screen_commands,
            )));
        }
        // --- Process Trees tab ---
        if !info.pid.is_empty() || info.allow_process_commands {
            pollers::spawn_process_trees_poller(tx.clone(), api.clone());
            tabs.push(Box::new(tabs::ProcessesTab::new(
                info.allow_process_commands,
            )));
        }
        // --- System Resources tab ---
        if info.system_resources {
            pollers::spawn_system_resources_poller(tx.clone(), api.clone());
            tabs.push(Box::new(tabs::SystemResourcesTab::new(
                info.allow_system_resources_commands,
            )));
        }
        // --- Systemd tab ---
        if info.systemd {
            pollers::spawn_systemd_poller(tx.clone(), api.clone());
            tabs.push(Box::new(tabs::SystemdTab::new(info.allow_systemd_commands)));
        }
        // --- Docker tab ---
        if info.docker || info.allow_docker_commands {
            pollers::spawn_docker_poller(tx.clone(), api.clone());
            tabs.push(Box::new(tabs::DockerTab::new(info.allow_docker_commands)));
        }
        // --- Top Processes tab ---
        if info.top_processes {
            let poll_config = Arc::new(std::sync::Mutex::new(
                tabs::TopProcessesPollConfig::default(),
            ));
            pollers::spawn_top_processes_poller(tx.clone(), api.clone(), poll_config.clone());
            tabs.push(Box::new(tabs::TopProcessesTab::new(
                poll_config,
                info.allow_process_commands,
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
            dirty: false,
            login,
            api,
            tx,
            auth_enabled: info.auth_enabled,
            authenticated: false,
        }
    }

    /// Entry point for the event-driven main loop. Dispatches to the
    /// input handler or to the relevant tab(s), and marks `dirty` so the
    /// caller knows a redraw is due.
    pub fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Input(ev) => {
                self.handle_event(ev);
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
        }
    }

    pub fn tab_titles(&self) -> Vec<&'static str> {
        self.tabs.iter().map(|t| t.name()).collect()
    }

    pub fn get_tab_by_name(&mut self, name: &str) -> Option<&mut Box<dyn Tab>> {
        self.tabs.iter_mut().find(|t| t.name() == name)
    }

    pub fn handle_event(&mut self, event: Event) {
        // Login screen, when present, owns all input.
        if let Some(login) = &mut self.login {
            match login.handle_event(&event) {
                LoginOutcome::Submit { username, password } => self.spawn_login(username, password),
                LoginOutcome::Cancel => self.login = None,
                LoginOutcome::None => {}
            }
            return;
        }

        let consumed = self
            .tabs
            .get_mut(self.selected_tab)
            .map(|tab| tab.handle_event(&event))
            .unwrap_or(false);

        if consumed {
            return;
        }

        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.should_quit = true
                }
                KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.login = Some(LoginState::new(true));
                }
                KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.spawn_logout();
                }
                KeyCode::Tab | KeyCode::Right => self.next_tab(),
                KeyCode::BackTab | KeyCode::Left => self.prev_tab(),
                KeyCode::Char(c) if ('1'..='9').contains(&c) => {
                    let idx = (c as usize) - ('1' as usize);
                    if idx < self.tabs.len() {
                        self.selected_tab = idx;
                    }
                }
                _ => {}
            },
            Event::Mouse(mouse) => {
                if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                    if mouse.row < 3 {
                        for (i, &(x0, x1)) in self.tab_hit_rects.iter().enumerate() {
                            if mouse.column >= x0 && mouse.column < x1 {
                                self.selected_tab = i;
                                break;
                            }
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
            if let Err(e) = api.logout().await {
                eprintln!("logout failed: {e}");
            }
            let _ = tx.send(AppEvent::LogoutResult).await;
        });
    }

    fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.selected_tab = (self.selected_tab + 1) % self.tabs.len();
        }
    }

    fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.selected_tab =
                (self.selected_tab + self.tabs.len().saturating_sub(1)) % self.tabs.len();
        }
    }
}
