use anyhow::{Context, Result};
use ratatui::widgets::{Block, Borders};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
    time::Instant,
};
use tui_textarea::TextArea;

use crate::ansible::{
    PlayAction, discover_files, discover_host_vars, list_playbook_tags, parse_host_line,
    parse_inventory_hosts, run_ansible,
};

pub const SPINNER: [char; 8] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧'];

#[derive(Clone, Debug)]
pub struct HostState {
    pub name: String,
    pub status: String,
    pub task: String,
    pub detail: String,
}

#[derive(Debug)]
pub enum AppEvent {
    Output(String),
    Finished(i32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Tasks,
    Inventory,
    HostVars,
    Output,
}

impl Screen {
    pub const ALL: [Screen; 4] = [
        Screen::Tasks,
        Screen::Inventory,
        Screen::HostVars,
        Screen::Output,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Tasks => "Tasks",
            Screen::Inventory => "Inventory",
            Screen::HostVars => "Host vars",
            Screen::Output => "Output",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Screen::Tasks => 0,
            Screen::Inventory => 1,
            Screen::HostVars => 2,
            Screen::Output => 3,
        }
    }
}

/// Whether a run is idle, currently executing, or has just finished (and how).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Idle,
    Running,
    Success,
    Failed,
}

pub struct App {
    pub root: PathBuf,
    /// Path to the playbook (relative to `root`) that tags are read from and run against.
    pub playbook: PathBuf,

    /// Actions discovered by asking Ansible itself (`--list-tags`) - the playbook
    /// is the single source of truth, so this list can never drift out of sync
    /// with it. Each entry also carries the host pattern of its play.
    pub available_tags: Vec<PlayAction>,
    pub selected_tag: usize,
    /// Set when the last attempt to list tags failed (e.g. Ansible missing, playbook
    /// with a syntax error). Kept separate from `status` so the UI can show it prominently.
    pub tags_error: Option<String>,

    pub inventories: Vec<PathBuf>,
    pub selected_inventory: usize,

    pub host_vars: Vec<PathBuf>,
    pub selected_host_vars: usize,

    pub screen: Screen,
    pub hosts: BTreeMap<String, HostState>,
    pub log: Vec<String>,
    pub current_task: String,

    pub run_state: RunState,
    pub run_started: Option<Instant>,
    pub last_duration_secs: Option<f64>,
    rx: Option<Receiver<AppEvent>>,

    pub status: String,
    pub tick: u64,

    pub inventory_editor: Option<TextArea<'static>>,
    pub inventory_dirty: bool,

    /// Lines scrolled up from the bottom of the output log (0 == following the tail).
    pub output_scroll: usize,
}

impl App {
    pub fn new(root: PathBuf, playbook: PathBuf) -> Result<Self> {
        let mut app = Self {
            root,
            playbook,
            available_tags: Vec::new(),
            selected_tag: 0,
            tags_error: None,
            inventories: Vec::new(),
            selected_inventory: 0,
            host_vars: Vec::new(),
            selected_host_vars: 0,
            screen: Screen::Tasks,
            hosts: BTreeMap::new(),
            log: Vec::new(),
            current_task: String::new(),
            run_state: RunState::Idle,
            run_started: None,
            last_duration_secs: None,
            rx: None,
            status: String::from("Ready"),
            tick: 0,
            inventory_editor: None,
            inventory_dirty: false,
            output_scroll: 0,
        };
        app.refresh()?;
        Ok(app)
    }

    pub fn is_running(&self) -> bool {
        matches!(self.run_state, RunState::Running)
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.inventories = discover_files(
            &self.root.join("inventory"),
            &["ini", "yaml", "yml", "json"],
        )?;
        self.host_vars = discover_host_vars(&self.root.join("host_vars"))?;

        if self.selected_inventory >= self.inventories.len() {
            self.selected_inventory = self.inventories.len().saturating_sub(1);
        }
        if self.selected_host_vars >= self.host_vars.len() {
            self.selected_host_vars = self.host_vars.len().saturating_sub(1);
        }

        // Ask Ansible itself which tags exist in the playbook, using whichever
        // inventory is currently selected (some playbooks need one to parse cleanly).
        let inventory = self.selected_inventory_path().map(Path::to_path_buf);
        match list_playbook_tags(&self.root, &self.playbook, inventory.as_deref()) {
            Ok(tags) => {
                self.available_tags = tags;
                self.tags_error = None;
            }
            Err(err) => {
                self.available_tags.clear();
                self.tags_error = Some(format!("{err:#}"));
            }
        }
        if self.selected_tag >= self.available_tags.len() {
            self.selected_tag = self.available_tags.len().saturating_sub(1);
        }

        self.status = self.build_status_message();
        Ok(())
    }

    fn build_status_message(&self) -> String {
        if self.inventories.is_empty() {
            "No inventory found in inventory/".into()
        } else if let Some(err) = &self.tags_error {
            format!(
                "Could not list tags from {}: {err}",
                self.playbook.display()
            )
        } else if self.available_tags.is_empty() {
            format!("No tags found in {}", self.playbook.display())
        } else {
            format!(
                "{} tag(s) in {} · {} inventory file(s) · {} host_vars {}",
                self.available_tags.len(),
                self.playbook.display(),
                self.inventories.len(),
                self.host_vars.len(),
                if self.host_vars.len() == 1 {
                    "entry"
                } else {
                    "entries"
                }
            )
        }
    }

    pub fn selected_tag(&self) -> Option<&str> {
        self.available_tags
            .get(self.selected_tag)
            .map(|action| action.tag.as_str())
    }

    pub fn selected_inventory_path(&self) -> Option<&Path> {
        self.inventories
            .get(self.selected_inventory)
            .map(PathBuf::as_path)
    }

    pub fn selected_host_vars_path(&self) -> Option<&Path> {
        self.host_vars
            .get(self.selected_host_vars)
            .map(PathBuf::as_path)
    }

    pub fn open_inventory_editor(&mut self) -> Result<()> {
        let Some(path) = self.selected_inventory_path().map(Path::to_path_buf) else {
            return Ok(());
        };
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let mut area = TextArea::from(lines);
        area.set_block(
            Block::default()
                .title(" Inventory - Ctrl-S save, Esc close ")
                .borders(Borders::ALL),
        );
        self.inventory_editor = Some(area);
        self.inventory_dirty = false;
        Ok(())
    }

    pub fn save_inventory(&mut self) -> Result<()> {
        let Some(area) = &self.inventory_editor else {
            return Ok(());
        };
        let Some(path) = self.selected_inventory_path().map(Path::to_path_buf) else {
            return Ok(());
        };
        fs::write(&path, area.lines().join("\n"))
            .with_context(|| format!("cannot write {}", path.display()))?;
        self.inventory_dirty = false;
        self.status = format!("Saved {}", path.display());
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        if self.is_running() {
            return Ok(());
        }
        let Some(tag) = self.selected_tag().map(str::to_string) else {
            self.status = "No tag selected".into();
            return Ok(());
        };
        let Some(inventory) = self.selected_inventory_path().map(PathBuf::from) else {
            self.status = "No inventory selected".into();
            return Ok(());
        };

        self.hosts.clear();
        self.log.clear();
        self.current_task.clear();
        self.output_scroll = 0;

        for host in parse_inventory_hosts(&inventory)? {
            self.hosts.insert(
                host.clone(),
                HostState {
                    name: host,
                    status: "waiting".into(),
                    task: String::new(),
                    detail: String::new(),
                },
            );
        }

        let (tx, rx) = mpsc::channel();
        let root = self.root.clone();
        let playbook = self.playbook.clone();

        self.run_state = RunState::Running;
        self.run_started = Some(Instant::now());
        self.rx = Some(rx);
        self.screen = Screen::Output;
        self.status = format!("Running {} --tags {tag}", playbook.display());

        thread::spawn(move || {
            let result = run_ansible(&root, &playbook, &tag, &inventory, tx.clone());
            if let Err(err) = result {
                let _ = tx.send(AppEvent::Output(format!("ERROR: {err:#}\n")));
                let _ = tx.send(AppEvent::Finished(1));
            }
        });

        Ok(())
    }

    /// Drain any pending events from the running task, updating host/log state.
    /// Call this once per UI tick.
    pub fn poll_process(&mut self) {
        self.tick = self.tick.wrapping_add(1);

        let Some(rx) = &self.rx else {
            return;
        };

        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }

        for ev in events {
            match ev {
                AppEvent::Output(line) => self.consume_output(line),
                AppEvent::Finished(code) => {
                    self.last_duration_secs = self.run_started.map(|t| t.elapsed().as_secs_f64());
                    self.run_state = if code == 0 {
                        RunState::Success
                    } else {
                        RunState::Failed
                    };
                    self.status = match (code, self.last_duration_secs) {
                        (0, Some(d)) => format!("Finished successfully in {d:.1}s"),
                        (0, None) => "Finished successfully".into(),
                        (c, Some(d)) => format!("Ansible exited with code {c} after {d:.1}s"),
                        (c, None) => format!("Ansible exited with code {c}"),
                    };
                    self.rx = None;
                }
            }
        }
    }

    fn consume_output(&mut self, chunk: String) {
        for line in chunk.lines() {
            self.log.push(line.to_string());

            if let Some(task) = line
                .strip_prefix("TASK [")
                .and_then(|s| s.split(']').next())
            {
                self.current_task = task.to_string();
            }

            if let Some((status, host, detail)) = parse_host_line(line) {
                let entry = self.hosts.entry(host.clone()).or_insert(HostState {
                    name: host.clone(),
                    status: "seen".into(),
                    task: String::new(),
                    detail: String::new(),
                });
                entry.status = status.to_string();
                entry.task = self.current_task.clone();
                entry.detail = detail.to_string();
            }
        }

        if self.log.len() > 5000 {
            self.log.drain(..1000);
        }
    }

    pub fn host_summary(&self) -> BTreeMap<&'static str, usize> {
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        for host in self.hosts.values() {
            let key = match host.status.as_str() {
                "ok" => "ok",
                "changed" => "changed",
                "failed" | "unreachable" => "failed",
                "skipped" => "skipped",
                _ => "waiting",
            };
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
    }

    pub fn scroll_output(&mut self, delta: i64) {
        let max = self.log.len().saturating_sub(1);
        let current = self.output_scroll as i64;
        self.output_scroll = (current + delta).clamp(0, max as i64) as usize;
    }

    pub fn scroll_output_to_top(&mut self) {
        self.output_scroll = self.log.len().saturating_sub(1);
    }

    pub fn scroll_output_to_bottom(&mut self) {
        self.output_scroll = 0;
    }
}
