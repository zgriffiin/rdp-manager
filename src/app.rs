// Main window: connection list, details, connection editor, and dialogs.

use crate::credentials;
use crate::rdp::Sessions;
use crate::store::{self, Connection, DEFAULT_PORT, Store, ThemeChoice};
use crate::theme::{self, Palette};
use crate::widgets::{self, Kind, button, card, icon_button};
use eframe::egui::{
    self, Align, Color32, ComboBox, CornerRadius, FontId, Frame, Grid, Id, Key, Layout, Margin,
    Modal, RichText, ScrollArea, Sense, Stroke, TextEdit, TextureHandle, ThemePreference, Ui,
    load::SizedTexture, pos2, vec2,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const RESOLUTIONS: [(u32, u32); 5] = [
    (1280, 720),
    (1366, 768),
    (1600, 900),
    (1920, 1080),
    (2560, 1440),
];
const INPUT_WIDTH: f32 = 340.0;

#[derive(Clone, Copy, PartialEq)]
enum Section {
    General,
    Credentials,
    Display,
    Resources,
}

impl Section {
    const ALL: [Section; 4] = [
        Section::General,
        Section::Credentials,
        Section::Display,
        Section::Resources,
    ];

    fn title(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Credentials => "Credentials",
            Section::Display => "Display",
            Section::Resources => "Local resources",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::General => "🖥",
            Section::Credentials => "🔑",
            Section::Display => "🖼",
            Section::Resources => "🔌",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            Section::General => {
                "Name the connection and enter the computer to connect to. The standard Remote Desktop port is 3389."
            }
            Section::Credentials => "The account used to sign in to the remote computer.",
            Section::Display => "How the remote desktop appears on your screen.",
            Section::Resources => "Local devices the remote computer can use.",
        }
    }
}

struct Editor {
    draft: Connection,
    password: String,
    show_password: bool,
    section: Section,
    is_new: bool,
    error: Option<String>,
}

impl Editor {
    fn new(draft: Connection, password: String, is_new: bool) -> Self {
        Self {
            draft,
            password,
            show_password: false,
            section: Section::General,
            is_new,
            error: None,
        }
    }

    /// Cleans up the draft. On failure, returns the section to show and a message.
    fn validate(&mut self) -> Result<(), (Section, String)> {
        let d = &mut self.draft;
        for field in [&mut d.name, &mut d.host, &mut d.group, &mut d.username] {
            *field = field.trim().to_string();
        }
        d.description = d.description.trim_end().to_string();
        let Some((host, port)) = store::parse_address(&d.host) else {
            return Err((
                Section::General,
                "Enter a valid computer name or IP address.".into(),
            ));
        };
        d.host = host;
        if let Some(port) = port {
            d.port = port;
        }
        if d.port == 0 {
            d.port = DEFAULT_PORT;
        }
        if d.name.is_empty() {
            d.name = d.host.clone();
        }
        if !self.password.is_empty() && d.username.is_empty() {
            return Err((
                Section::Credentials,
                "Enter a user name to save the password.".into(),
            ));
        }
        Ok(())
    }
}

enum Action {
    Select(String),
    Connect(String),
    Disconnect(String),
    Edit(String),
    New,
    SaveQuick(String),
    Duplicate(String),
    AskDelete(String),
    Delete(String),
    QuickConnect,
    ToggleTheme,
}

struct Status {
    text: String,
    error: bool,
    at: Instant,
}

pub struct App {
    store: Store,
    sessions: Sessions,
    selected: Option<String>,
    search: String,
    quick: String,
    editor: Option<Editor>,
    confirm_delete: Option<String>,
    status: Option<Status>,
    /// Cache of "has a saved password", so the UI does not query Credential Manager every frame.
    has_password: HashMap<String, bool>,
    /// Connection ids in list order, for keyboard navigation.
    visible: Vec<String>,
    scroll_to_selected: bool,
    logo: TextureHandle,
    actions: Vec<Action>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::setup(&cc.egui_ctx);
        let (store, warning) = Store::load();
        cc.egui_ctx.set_theme(match store.data.theme {
            ThemeChoice::System => ThemePreference::System,
            ThemeChoice::Light => ThemePreference::Light,
            ThemeChoice::Dark => ThemePreference::Dark,
        });
        let logo = cc.egui_ctx.load_texture(
            "logo",
            egui::ColorImage::from_rgba_unmultiplied([128, 128], &crate::icon::render_icon(128)),
            egui::TextureOptions::LINEAR,
        );
        let mut app = Self {
            store,
            sessions: Sessions::default(),
            selected: None,
            search: String::new(),
            quick: String::new(),
            editor: None,
            confirm_delete: None,
            status: None,
            has_password: HashMap::new(),
            visible: Vec::new(),
            scroll_to_selected: false,
            logo,
            actions: Vec::new(),
        };
        if let Some(w) = warning {
            app.set_status(w, true);
        }
        app
    }

    fn set_status(&mut self, text: impl Into<String>, error: bool) {
        self.status = Some(Status {
            text: text.into(),
            error,
            at: Instant::now(),
        });
    }

    fn save_store(&mut self) {
        if let Err(e) = self.store.save() {
            self.set_status(e, true);
        }
    }

    fn password_saved(&mut self, id: &str) -> bool {
        *self
            .has_password
            .entry(id.to_string())
            .or_insert_with(|| credentials::load_password(id).is_some())
    }

    fn session_label(&self, id: &str) -> String {
        self.sessions
            .iter()
            .find(|(sid, _)| *sid == id)
            .map(|(_, s)| s.label.clone())
            .unwrap_or_default()
    }

    // ---------------------------------------------------------------- actions

    fn apply(&mut self, ctx: &egui::Context) {
        for action in std::mem::take(&mut self.actions) {
            match action {
                Action::Select(id) => self.selected = Some(id),
                Action::Connect(id) => self.connect(&id),
                Action::Disconnect(id) => {
                    let label = self.session_label(&id);
                    self.sessions.disconnect(&id);
                    self.set_status(format!("Disconnected from {label}."), false);
                }
                Action::Edit(id) => {
                    if let Some(c) = self.store.get(&id) {
                        let password = credentials::load_password(&id).unwrap_or_default();
                        self.editor = Some(Editor::new(c.clone(), password, false));
                    }
                }
                Action::New => {
                    self.editor = Some(Editor::new(Connection::default(), String::new(), true));
                }
                Action::SaveQuick(address) => {
                    if let Some((host, port)) = store::parse_address(&address) {
                        let draft = Connection {
                            name: host.clone(),
                            host,
                            port: port.unwrap_or(DEFAULT_PORT),
                            ..Default::default()
                        };
                        self.editor = Some(Editor::new(draft, String::new(), true));
                    }
                }
                Action::Duplicate(id) => self.duplicate(&id),
                Action::AskDelete(id) => self.confirm_delete = Some(id),
                Action::Delete(id) => {
                    let name = self
                        .store
                        .get(&id)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    self.store.remove(&id);
                    credentials::delete_password(&id);
                    self.has_password.remove(&id);
                    if self.selected.as_deref() == Some(id.as_str()) {
                        self.selected = None;
                    }
                    self.set_status(format!("Deleted {name}."), false);
                    self.save_store();
                }
                Action::QuickConnect => self.quick_connect(),
                Action::ToggleTheme => {
                    let dark = ctx.theme() == egui::Theme::Light;
                    ctx.set_theme(if dark {
                        ThemePreference::Dark
                    } else {
                        ThemePreference::Light
                    });
                    self.store.data.theme = if dark {
                        ThemeChoice::Dark
                    } else {
                        ThemeChoice::Light
                    };
                    self.save_store();
                }
            }
        }
    }

    fn connect(&mut self, id: &str) {
        if self.sessions.is_active(id) {
            return;
        }
        let Some(conn) = self.store.get(id).cloned() else {
            return;
        };
        match self.sessions.connect(&conn) {
            Ok(()) => {
                if let Some(c) = self.store.get_mut(id) {
                    c.last_connected = Some(store::now());
                }
                self.set_status(format!("Opening {}…", conn.name), false);
                self.save_store();
            }
            Err(e) => self.set_status(e, true),
        }
    }

    fn quick_connect(&mut self) {
        let Some((host, port)) = store::parse_address(&self.quick) else {
            self.set_status(
                "Enter a host name or IP address, optionally followed by :port.",
                true,
            );
            return;
        };
        match self
            .sessions
            .quick_connect(&host, port.unwrap_or(DEFAULT_PORT))
        {
            Ok(_) => {
                self.set_status(format!("Opening {}…", self.quick.trim()), false);
                self.quick.clear();
            }
            Err(e) => self.set_status(e, true),
        }
    }

    fn duplicate(&mut self, id: &str) {
        let Some(src) = self.store.get(id).cloned() else {
            return;
        };
        let now = store::now();
        let copy = Connection {
            id: store::new_id(),
            name: format!("{} (copy)", src.name),
            created: now,
            modified: now,
            last_connected: None,
            ..src
        };
        if let Some(pw) = credentials::load_password(id)
            && let Err(e) = credentials::save_password(&copy.id, &copy.username, &pw)
        {
            self.set_status(e, true);
        }
        self.selected = Some(copy.id.clone());
        self.scroll_to_selected = true;
        self.store.upsert(copy);
        self.save_store();
    }

    fn commit(&mut self, mut ed: Editor) {
        ed.draft.modified = store::now();
        let id = ed.draft.id.clone();
        let name = ed.draft.name.clone();
        let result = if ed.password.is_empty() {
            credentials::delete_password(&id);
            Ok(())
        } else {
            credentials::save_password(&id, &ed.draft.username, &ed.password)
        };
        self.has_password
            .insert(id.clone(), !ed.password.is_empty() && result.is_ok());
        match result {
            Ok(()) => self.set_status(format!("Saved {name}."), false),
            Err(e) => self.set_status(e, true),
        }
        self.store.upsert(ed.draft);
        self.selected = Some(id);
        self.scroll_to_selected = true;
        self.save_store();
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.editor.is_some() || self.confirm_delete.is_some() {
            return;
        }
        let typing = ctx.egui_wants_keyboard_input();
        // Outside input_mut: nesting memory_mut inside it would deadlock on the context lock.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::F)) {
            ctx.memory_mut(|m| m.request_focus(Id::unique("search")));
        }
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::COMMAND, Key::N) {
                self.actions.push(Action::New);
            }
            if typing {
                return;
            }
            let Some(sel) = self.selected.clone() else {
                if i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)
                    && let Some(first) = self.visible.first()
                {
                    self.selected = Some(first.clone());
                    self.scroll_to_selected = true;
                }
                return;
            };
            if i.consume_key(egui::Modifiers::NONE, Key::Enter) {
                self.actions.push(Action::Connect(sel.clone()));
            }
            if i.consume_key(egui::Modifiers::NONE, Key::F2) {
                self.actions.push(Action::Edit(sel.clone()));
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Delete) {
                self.actions.push(Action::AskDelete(sel.clone()));
            }
            let pos = self.visible.iter().position(|id| *id == sel);
            let step = if i.consume_key(egui::Modifiers::NONE, Key::ArrowDown) {
                1
            } else if i.consume_key(egui::Modifiers::NONE, Key::ArrowUp) {
                -1
            } else {
                0
            };
            if step != 0 && !self.visible.is_empty() {
                let next = match pos {
                    Some(p) => {
                        (p as isize + step).clamp(0, self.visible.len() as isize - 1) as usize
                    }
                    None => 0,
                };
                self.selected = Some(self.visible[next].clone());
                self.scroll_to_selected = true;
            }
        });
    }

    // ---------------------------------------------------------------- layout

    fn top_bar(&mut self, ui: &mut Ui, p: &Palette) {
        egui::Panel::top("top")
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(SizedTexture::new(
                        self.logo.id(),
                        vec2(28.0, 28.0),
                    )));
                    ui.label(
                        RichText::new("RDP Manager")
                            .font(theme::bold(17.0))
                            .color(p.text),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (icon, tip) = if ui.visuals().dark_mode {
                            ("☀", "Switch to light theme")
                        } else {
                            ("🌙", "Switch to dark theme")
                        };
                        if icon_button(ui, icon, tip).clicked() {
                            self.actions.push(Action::ToggleTheme);
                        }
                        if button(ui, "+  New connection", Kind::Primary)
                            .on_hover_text("Ctrl+N")
                            .clicked()
                        {
                            self.actions.push(Action::New);
                        }
                        ui.add_space(12.0);
                        if button(ui, "Connect", Kind::Secondary).clicked() {
                            self.actions.push(Action::QuickConnect);
                        }
                        let resp = ui.add(
                            TextEdit::singleline(&mut self.quick)
                                .hint_text("Quick connect: host name or IP address")
                                .desired_width(280.0)
                                .margin(Margin::symmetric(10, 7)),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            self.actions.push(Action::QuickConnect);
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut Ui, p: &Palette) {
        if self
            .status
            .as_ref()
            // Errors stay until the next message, so a warning is not missed.
            .is_some_and(|s| !s.error && s.at.elapsed() > Duration::from_secs(8))
        {
            self.status = None;
        }
        egui::Panel::bottom("status")
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .inner_margin(Margin::symmetric(16, 5)),
            )
            .show(ui, |ui| {
                // Counts first, so a long message is shortened instead of covering them.
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let n = self.store.data.connections.len();
                    let m = self.sessions.len();
                    ui.label(
                        RichText::new(format!(
                            "{n} connection{} · {m} active session{}",
                            if n == 1 { "" } else { "s" },
                            if m == 1 { "" } else { "s" }
                        ))
                        .size(12.0)
                        .color(p.text_weak),
                    );
                    ui.add_space(16.0);
                    if let Some(s) = &self.status {
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let (icon, color) = if s.error {
                                ("⚠", p.danger)
                            } else {
                                ("✔", p.success)
                            };
                            ui.label(RichText::new(icon).color(color).size(12.0));
                            // A shortened message shows in full when you point to it.
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&s.text)
                                        .color(if s.error { p.danger } else { p.text })
                                        .size(12.0),
                                )
                                .truncate(),
                            );
                        });
                    }
                });
            });
        if self.status.is_some() {
            ui.ctx().request_repaint_after(Duration::from_secs(1));
        }
    }

    fn sidebar(&mut self, ui: &mut Ui, p: &Palette) {
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(310.0)
            .size_range(250.0..=480.0)
            .frame(Frame::new().fill(p.surface).inner_margin(Margin::same(12)))
            .show(ui, |ui| {
                ui.add(
                    TextEdit::singleline(&mut self.search)
                        .id(Id::unique("search"))
                        .hint_text("🔍  Search connections  (Ctrl+F)")
                        .desired_width(f32::INFINITY)
                        .margin(Margin::symmetric(10, 7)),
                );
                if !self.sessions.is_empty() {
                    self.sessions_panel(ui, p);
                }
                ui.add_space(4.0);
                ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| self.connection_list(ui, p));
            });
    }

    fn connection_list(&mut self, ui: &mut Ui, p: &Palette) {
        let query = self.search.trim().to_string();
        let mut rows: Vec<&Connection> = self
            .store
            .data
            .connections
            .iter()
            .filter(|c| query.is_empty() || c.matches(&query))
            .collect();
        rows.sort_by_cached_key(|c| (c.group.to_lowercase(), c.name.to_lowercase()));
        self.visible = rows.iter().map(|c| c.id.clone()).collect();

        if rows.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                if self.store.data.connections.is_empty() {
                    ui.label(RichText::new("No saved connections yet").color(p.text_weak));
                    ui.add_space(4.0);
                    if button(ui, "Add a connection", Kind::Secondary).clicked() {
                        self.actions.push(Action::New);
                    }
                } else {
                    ui.label(RichText::new("No connections match your search").color(p.text_weak));
                }
            });
            return;
        }

        ui.spacing_mut().item_spacing.y = 2.0;
        let mut group: Option<&str> = None;
        for c in rows {
            if group != Some(c.group.as_str()) {
                group = Some(c.group.as_str());
                ui.add_space(8.0);
                widgets::section_label(
                    ui,
                    p,
                    if c.group.is_empty() {
                        "Connections"
                    } else {
                        &c.group
                    },
                );
                ui.add_space(2.0);
            }
            let selected = self.selected.as_deref() == Some(c.id.as_str());
            let active = self.sessions.is_active(&c.id);
            let resp = connection_row(ui, p, c, selected, active);
            if selected && self.scroll_to_selected {
                resp.scroll_to_me(None);
                self.scroll_to_selected = false;
            }
            if resp.clicked() {
                self.actions.push(Action::Select(c.id.clone()));
            }
            if resp.double_clicked() {
                self.actions.push(Action::Connect(c.id.clone()));
            }
            resp.context_menu(|ui| {
                ui.set_min_width(170.0);
                if active {
                    if ui.button("⏹  Disconnect").clicked() {
                        self.actions.push(Action::Disconnect(c.id.clone()));
                    }
                } else if ui.button("▶  Connect").clicked() {
                    self.actions.push(Action::Connect(c.id.clone()));
                }
                if ui.button("✏  Edit").clicked() {
                    self.actions.push(Action::Edit(c.id.clone()));
                }
                if ui.button("📋  Duplicate").clicked() {
                    self.actions.push(Action::Duplicate(c.id.clone()));
                }
                ui.separator();
                if ui
                    .button(RichText::new("🗑  Delete").color(p.danger))
                    .clicked()
                {
                    self.actions.push(Action::AskDelete(c.id.clone()));
                }
            });
        }
    }

    fn sessions_panel(&mut self, ui: &mut Ui, p: &Palette) {
        ui.add_space(10.0);
        widgets::section_label(ui, p, &format!("Active sessions · {}", self.sessions.len()));
        ui.add_space(2.0);
        for (id, s) in self.sessions.iter() {
            let (rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), p.success.gamma_multiply(0.08));
            ui.painter()
                .circle_filled(pos2(rect.left() + 14.0, rect.center().y), 4.0, p.success);
            let quick = id.starts_with("quick:");
            let text_w = rect.width() - 26.0 - if quick { 70.0 } else { 40.0 };
            let name = widgets::elided(ui, &s.label, theme::bold(13.0), p.text, text_w);
            ui.painter()
                .galley(pos2(rect.left() + 26.0, rect.top() + 4.0), name, p.text);
            let sub = format!("{} · {}", s.address, elapsed(s.started.elapsed()));
            let sub = widgets::elided(ui, &sub, FontId::proportional(11.5), p.text_weak, text_w);
            ui.painter().galley(
                pos2(rect.left() + 26.0, rect.top() + 21.0),
                sub,
                p.text_weak,
            );

            let mut x = rect.right() - 34.0;
            let btn = |x: f32| {
                egui::Rect::from_min_size(pos2(x, rect.center().y - 14.0), vec2(28.0, 28.0))
            };
            let mut cell = ui.new_child(egui::UiBuilder::new().max_rect(btn(x)));
            if icon_button(&mut cell, "✖", "Disconnect").clicked() {
                self.actions.push(Action::Disconnect(id.clone()));
            }
            if quick {
                x -= 30.0;
                let mut cell = ui.new_child(egui::UiBuilder::new().max_rect(btn(x)));
                if icon_button(&mut cell, "💾", "Save as a connection").clicked() {
                    self.actions.push(Action::SaveQuick(s.address.clone()));
                }
            }
        }
        ui.add_space(4.0);
    }

    fn main_area(&mut self, ui: &mut Ui, p: &Palette) {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg).inner_margin(Margin::same(24)))
            .show(ui, |ui| {
                let selected = self
                    .selected
                    .clone()
                    .and_then(|id| self.store.get(&id).cloned());
                match selected {
                    Some(c) => {
                        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                            ui.set_max_width(ui.available_width().min(820.0));
                            self.details(ui, p, &c);
                        });
                    }
                    None => self.welcome(ui, p),
                }
            });
    }

    fn welcome(&mut self, ui: &mut Ui, p: &Palette) {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.22).max(16.0));
            ui.add(egui::Image::new(SizedTexture::new(self.logo.id(), vec2(80.0, 80.0))));
            ui.add_space(12.0);
            ui.label(RichText::new("Welcome to RDP Manager").font(theme::bold(24.0)).color(p.text));
            ui.add_space(2.0);
            ui.label(
                RichText::new(
                    "Select a connection on the left, or type a host name or IP address in Quick connect.",
                )
                .color(p.text_weak),
            );
            ui.add_space(16.0);
            if button(ui, "+  New connection", Kind::Primary).clicked() {
                self.actions.push(Action::New);
            }
            ui.add_space(28.0);
            ui.label(
                RichText::new(
                    "Double-click to connect  ·  Right-click for more  ·  ↑ ↓ to move  ·  Enter to connect  ·  F2 to edit",
                )
                .size(12.0)
                .color(p.text_weak),
            );
        });
    }

    fn details(&mut self, ui: &mut Ui, p: &Palette, c: &Connection) {
        let active = self.sessions.is_active(&c.id);
        let has_password = self.password_saved(&c.id);

        card(ui, p, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                widgets::connection_badge(ui, rect, p, p.accent_soft, active);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&c.name).font(theme::bold(22.0)).color(p.text),
                        )
                        .truncate(),
                    );
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(c.address()).size(15.0).color(p.text_weak));
                        ui.add_space(4.0);
                        if active {
                            widgets::pill(ui, "Connected", p.success);
                        } else {
                            widgets::pill(ui, "Not connected", p.text_weak);
                        }
                    });
                });
            });
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if active {
                    if button(ui, "⏹  Disconnect", Kind::Danger).clicked() {
                        self.actions.push(Action::Disconnect(c.id.clone()));
                    }
                } else if button(ui, "▶  Connect", Kind::Primary)
                    .on_hover_text("Enter")
                    .clicked()
                {
                    self.actions.push(Action::Connect(c.id.clone()));
                }
                if button(ui, "✏  Edit", Kind::Secondary)
                    .on_hover_text("F2")
                    .clicked()
                {
                    self.actions.push(Action::Edit(c.id.clone()));
                }
                if button(ui, "📋  Duplicate", Kind::Secondary).clicked() {
                    self.actions.push(Action::Duplicate(c.id.clone()));
                }
                if button(ui, "🗑  Delete", Kind::DangerOutline)
                    .on_hover_text("Delete key")
                    .clicked()
                {
                    self.actions.push(Action::AskDelete(c.id.clone()));
                }
            });
        });

        ui.add_space(16.0);
        card(ui, p, |ui| {
            card_title(ui, p, "Connection");
            info_grid(ui, "info-connection", p, |row| {
                row("Computer", c.host.clone());
                row("Port", c.port.to_string());
                row(
                    "User name",
                    if c.username.is_empty() {
                        "Not set".into()
                    } else {
                        c.username.clone()
                    },
                );
                row(
                    "Password",
                    if has_password {
                        "Saved in Windows Credential Manager".into()
                    } else {
                        "Not saved (you will be asked when you connect)".into()
                    },
                );
                row("Admin session", yes_no(c.admin_session).into());
                if !c.group.is_empty() {
                    row("Group", c.group.clone());
                }
                if !c.description.is_empty() {
                    row("Description", c.description.clone());
                }
            });
        });

        ui.add_space(16.0);
        card(ui, p, |ui| {
            card_title(ui, p, "Display");
            info_grid(ui, "info-display", p, |row| {
                let display = match (c.fullscreen, c.multi_monitor) {
                    (true, true) => "Full screen on all monitors".into(),
                    (true, false) => "Full screen".into(),
                    (false, _) => format!("Window, {}", resolution_label(c.width, c.height)),
                };
                row("Display", display);
            });
        });

        ui.add_space(12.0);
        let last = c.last_connected.map_or("never".into(), store::ago);
        ui.label(
            RichText::new(format!(
                "Created {}  ·  Modified {}  ·  Last connected {}",
                store::ago(c.created),
                store::ago(c.modified),
                last
            ))
            .size(12.0)
            .color(p.text_weak),
        );
    }

    // ---------------------------------------------------------------- dialogs

    fn editor_dialog(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(ed) = &mut self.editor else { return };
        let mut save = false;
        // Escape in an open dropdown only closes the dropdown.
        let mut cancel =
            !egui::Popup::is_any_open(ctx) && ctx.input(|i| i.key_pressed(Key::Escape));

        Modal::new(Id::unique("editor"))
            .frame(dialog_frame(ctx, p))
            .show(ctx, |ui| {
                ui.set_width(740.0);
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

                Frame::new().inner_margin(Margin::symmetric(24, 18)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
                        widgets::connection_badge(ui, rect, p, p.accent_soft, false);
                        ui.add_space(14.0);
                        ui.vertical(|ui| {
                            let title = if ed.is_new {
                                "New Remote Desktop Connection"
                            } else {
                                "Remote Desktop Connection Settings"
                            };
                            ui.label(RichText::new(title).font(theme::bold(18.0)).color(p.text));
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new("Set where to connect, how to sign in, and how the session looks.")
                                    .color(p.text_weak),
                            );
                        });
                    });
                });
                widgets::divider(ui, p);

                ui.horizontal_top(|ui| {
                    // Frame content inherits the horizontal layout, so each side uses an explicit column.
                    Frame::new().fill(p.bg).inner_margin(Margin::same(10)).show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(170.0);
                            ui.set_min_height(390.0);
                            ui.spacing_mut().item_spacing.y = 2.0;
                            for s in Section::ALL {
                                if nav_item(ui, p, s, ed.section == s).clicked() {
                                    ed.section = s;
                                }
                            }
                        });
                    });
                    Frame::new().inner_margin(Margin::symmetric(28, 22)).show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(ed.section.title()).font(theme::bold(17.0)).color(p.text));
                            ui.add(egui::Label::new(RichText::new(ed.section.blurb()).color(p.text_weak)).wrap());
                            ui.add_space(10.0);
                            section_form(ui, p, ed);
                        });
                    });
                });

                widgets::divider(ui, p);
                Frame::new().inner_margin(Margin::symmetric(24, 14)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if let Some(err) = &ed.error {
                            ui.label(RichText::new(format!("⚠  {err}")).color(p.danger));
                        } else if !ed.is_new {
                            ui.label(
                                RichText::new(format!(
                                    "Created {}  ·  Modified {}",
                                    store::ago(ed.draft.created),
                                    store::ago(ed.draft.modified)
                                ))
                                .size(12.0)
                                .color(p.text_weak),
                            );
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if button(ui, "Save", Kind::Primary).clicked() {
                                save = true;
                            }
                            if button(ui, "Cancel", Kind::Secondary).clicked() {
                                cancel = true;
                            }
                        });
                    });
                });
            });

        if save {
            match ed.validate() {
                Ok(()) => {
                    let ed = self.editor.take().unwrap();
                    self.commit(ed);
                }
                Err((section, msg)) => {
                    ed.section = section;
                    ed.error = Some(msg);
                }
            }
        } else if cancel {
            self.editor = None;
        }
    }

    fn delete_dialog(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(id) = self.confirm_delete.clone() else {
            return;
        };
        let Some(name) = self.store.get(&id).map(|c| c.name.clone()) else {
            self.confirm_delete = None;
            return;
        };
        let mut close = ctx.input(|i| i.key_pressed(Key::Escape));
        Modal::new(Id::unique("confirm-delete"))
            .frame(dialog_frame(ctx, p).inner_margin(Margin::same(24)))
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.label(RichText::new("Delete this connection?").font(theme::bold(18.0)).color(p.text));
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "“{name}” and its saved password will be removed. Open sessions stay open."
                        ))
                        .color(p.text_weak),
                    )
                    .wrap(),
                );
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if button(ui, "Delete", Kind::Danger).clicked() {
                        self.actions.push(Action::Delete(id.clone()));
                        close = true;
                    }
                    if button(ui, "Cancel", Kind::Secondary).clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.confirm_delete = None;
        }
    }
}

impl eframe::App for App {
    /// The default near-black clear color shows through sub-pixel gaps between panels.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sessions.poll();
        if !self.sessions.is_empty() {
            // Keep session timers fresh and notice when a Remote Desktop window closes.
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let p = theme::palette(ui);
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        self.top_bar(ui, &p);
        self.status_bar(ui, &p);
        self.sidebar(ui, &p);
        self.main_area(ui, &p);
        self.editor_dialog(&ctx, &p);
        self.delete_dialog(&ctx, &p);
        self.apply(&ctx);
    }
}

// -------------------------------------------------------------------- helpers

fn connection_row(
    ui: &mut Ui,
    p: &Palette,
    c: &Connection,
    selected: bool,
    active: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::ListItem, true, selected, &c.name));
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let painter = ui.painter();
    let fill = if selected {
        p.accent_soft
    } else if resp.hovered() {
        p.surface_hover
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, CornerRadius::same(8), fill);
    if selected {
        let bar = egui::Rect::from_min_size(
            pos2(rect.left(), rect.top() + 14.0),
            vec2(3.0, rect.height() - 28.0),
        );
        painter.rect_filled(bar, CornerRadius::same(2), p.accent);
    }
    if resp.has_focus() {
        painter.rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(1.5, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    let badge = egui::Rect::from_min_size(
        pos2(rect.left() + 10.0, rect.center().y - 18.0),
        vec2(36.0, 36.0),
    );
    let badge_fill = if selected { p.surface } else { p.accent_soft };
    widgets::connection_badge(ui, badge, p, badge_fill, active);

    let x = badge.right() + 12.0;
    let w = rect.right() - x - 10.0;
    let name = widgets::elided(ui, &c.name, theme::bold(14.0), p.text, w);
    let sub_text = if c.username.is_empty() {
        c.address()
    } else {
        format!("{}  ·  {}", c.address(), c.username)
    };
    let sub = widgets::elided(ui, &sub_text, FontId::proportional(12.0), p.text_weak, w);
    let painter = ui.painter();
    painter.galley(pos2(x, rect.top() + 9.0), name, p.text);
    painter.galley(pos2(x, rect.top() + 29.0), sub, p.text_weak);
    resp
}

fn nav_item(ui: &mut Ui, p: &Palette, s: Section, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, selected, s.title()));
    let painter = ui.painter();
    let fill = if selected {
        p.accent_soft
    } else if resp.hovered() {
        p.surface_hover
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, CornerRadius::same(7), fill);
    let color = if selected { p.accent } else { p.text };
    let icon_rect = egui::Rect::from_min_size(rect.min + vec2(8.0, 6.0), vec2(24.0, 24.0));
    widgets::centered_text(
        painter,
        icon_rect,
        s.icon(),
        FontId::proportional(15.0),
        color,
    );
    let font = if selected {
        theme::bold(14.0)
    } else {
        FontId::proportional(14.0)
    };
    painter.text(
        pos2(rect.left() + 40.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        s.title(),
        font,
        color,
    );
    resp
}

fn section_form(ui: &mut Ui, p: &Palette, ed: &mut Editor) {
    let d = &mut ed.draft;
    let grid = |id: &str| {
        Grid::new(id)
            .num_columns(2)
            .spacing([18.0, 12.0])
            .min_col_width(110.0)
    };
    match ed.section {
        Section::General => {
            grid("form-general").show(ui, |ui| {
                form_label(ui, p, "Display name", true);
                text_input(ui, &mut d.name, "e.g. GDE App - Production");
                ui.end_row();
                form_label(ui, p, "Computer name", true);
                text_input(ui, &mut d.host, "Host name or IP address");
                ui.end_row();
                form_label(ui, p, "Port", false);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut d.port)
                            .range(1..=65535)
                            .speed(0.0),
                    );
                    ui.add_space(16.0);
                    ui.checkbox(&mut d.admin_session, "Console / admin session");
                });
                ui.end_row();
                form_label(ui, p, "Group", false);
                text_input(ui, &mut d.group, "Optional, groups connections in the list");
                ui.end_row();
                form_label(ui, p, "Description", false);
                ui.add(
                    TextEdit::multiline(&mut d.description)
                        .hint_text("Optional notes")
                        .desired_rows(3)
                        .desired_width(INPUT_WIDTH)
                        .margin(Margin::symmetric(8, 6)),
                );
                ui.end_row();
            });
        }
        Section::Credentials => {
            grid("form-credentials").show(ui, |ui| {
                form_label(ui, p, "User name", false);
                text_input(ui, &mut d.username, r"DOMAIN\user or user@domain");
                ui.end_row();
                form_label(ui, p, "Password", false);
                ui.horizontal(|ui| {
                    ui.add(
                        TextEdit::singleline(&mut ed.password)
                            .password(!ed.show_password)
                            .hint_text("Leave empty to be asked each time")
                            .desired_width(INPUT_WIDTH - 36.0)
                            .margin(Margin::symmetric(8, 6)),
                    );
                    let (icon, tip) = if ed.show_password {
                        ("🙈", "Hide password")
                    } else {
                        ("👁", "Show password")
                    };
                    if icon_button(ui, icon, tip).clicked() {
                        ed.show_password = !ed.show_password;
                    }
                });
                ui.end_row();
            });
            ui.add_space(14.0);
            note(
                ui,
                p,
                "🔒",
                "Passwords are saved in Windows Credential Manager, encrypted for your Windows account. \
                 They are never written to the connections file.",
            );
        }
        Section::Display => {
            grid("form-display").show(ui, |ui| {
                form_label(ui, p, "Mode", false);
                ui.horizontal(|ui| {
                    ui.radio_value(&mut d.fullscreen, false, "Window");
                    ui.add_space(12.0);
                    ui.radio_value(&mut d.fullscreen, true, "Full screen");
                });
                ui.end_row();
                if d.fullscreen {
                    form_label(ui, p, "Monitors", false);
                    ui.checkbox(&mut d.multi_monitor, "Use all my monitors");
                } else {
                    form_label(ui, p, "Window size", false);
                    ComboBox::from_id_salt("resolution")
                        .width(220.0)
                        .selected_text(resolution_label(d.width, d.height))
                        .show_ui(ui, |ui| {
                            for (w, h) in RESOLUTIONS {
                                let current = d.width == w && d.height == h;
                                if ui
                                    .selectable_label(current, resolution_label(w, h))
                                    .clicked()
                                {
                                    d.width = w;
                                    d.height = h;
                                }
                            }
                        });
                }
                ui.end_row();
            });
            ui.add_space(14.0);
            note(
                ui,
                p,
                "ℹ",
                "Other display options, such as resizing the remote desktop to fit its window, \
                 use the settings in Remote Desktop Connection.",
            );
        }
        Section::Resources => {
            note(
                ui,
                p,
                "ℹ",
                "Clipboard, drives, printers, and audio use the settings in Remote Desktop \
                 Connection. To change them, open Remote Desktop Connection (mstsc), select Show \
                 Options, change the Local Resources tab, then select Save on the General tab.",
            );
        }
    }
}

fn form_label(ui: &mut Ui, p: &Palette, text: &str, required: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.label(RichText::new(text).color(p.text));
        if required {
            ui.label(RichText::new("*").color(p.danger));
        }
    });
}

fn text_input(ui: &mut Ui, value: &mut String, hint: &str) -> egui::Response {
    ui.add(
        TextEdit::singleline(value)
            .hint_text(hint)
            .desired_width(INPUT_WIDTH)
            .margin(Margin::symmetric(8, 6)),
    )
}

fn note(ui: &mut Ui, p: &Palette, icon: &str, text: &str) {
    Frame::new()
        .fill(p.accent_soft)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(icon).color(p.accent));
                ui.add(egui::Label::new(RichText::new(text).color(p.text)).wrap());
            });
        });
}

fn card_title(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(theme::bold(15.0)).color(p.text));
    ui.add_space(6.0);
}

fn info_grid(ui: &mut Ui, id: &str, p: &Palette, rows: impl FnOnce(&mut dyn FnMut(&str, String))) {
    Grid::new(id)
        .num_columns(2)
        .spacing([24.0, 10.0])
        .min_col_width(120.0)
        .show(ui, |ui| {
            rows(&mut |label, value| {
                ui.label(RichText::new(label).color(p.text_weak));
                ui.add(egui::Label::new(RichText::new(value).color(p.text)).wrap());
                ui.end_row();
            });
        });
}

fn dialog_frame(ctx: &egui::Context, p: &Palette) -> Frame {
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(12))
        .shadow(ctx.global_style().visuals.window_shadow)
}

fn resolution_label(w: u32, h: u32) -> String {
    format!("{w} × {h}")
}

fn yes_no(b: bool) -> &'static str {
    if b { "Yes" } else { "No" }
}

fn elapsed(d: Duration) -> String {
    let mins = d.as_secs() / 60;
    match mins {
        0 => "under 1 min".into(),
        1..60 => format!("{mins} min"),
        _ => format!("{} h {} min", mins / 60, mins % 60),
    }
}
