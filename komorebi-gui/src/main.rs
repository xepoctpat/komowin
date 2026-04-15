#![warn(clippy::all)]

use eframe::egui;
use eframe::egui::Color32;
use eframe::egui::RichText;
use eframe::egui::ViewportBuilder;
use eframe::egui::viewport::IconData;
use komorebi_client::Container;
use komorebi_client::DefaultLayout;
use komorebi_client::Layout;
use komorebi_client::Monitor;
use komorebi_client::Rect;
use komorebi_client::SocketMessage;
use komorebi_client::State;
use komorebi_client::Window;
use komorebi_client::Workspace;
use komorebi_client::WorkspaceLayer;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

const APP_ID: &str = "komorebi.dashboard";
const APP_TITLE: &str = "komorebi Dashboard";
const DASHBOARD_DESKTOP_COUNT: usize = 2;
const AUTO_REFRESH_INTERVAL: Duration = Duration::from_millis(800);
const VISIBLE_WINDOW_LIMIT: usize = 8;
const DESKTOP_LABELS: [&str; DASHBOARD_DESKTOP_COUNT] = ["Desktop 1", "Desktop 2"];
const LAYOUT_OPTIONS: [DefaultLayout; 7] = [
    DefaultLayout::BSP,
    DefaultLayout::Columns,
    DefaultLayout::Rows,
    DefaultLayout::VerticalStack,
    DefaultLayout::HorizontalStack,
    DefaultLayout::UltrawideVerticalStack,
    DefaultLayout::Grid,
];

fn main() {
    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_app_id(APP_ID)
            .with_icon(Arc::new(dashboard_icon_data()))
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([960.0, 640.0]),
        ..Default::default()
    };

    let _ = eframe::run_native(
        APP_TITLE,
        native_options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(KomorebiGui::new()))
        }),
    );
}

fn dashboard_icon_data() -> IconData {
    const SIZE: u32 = 32;
    let mut rgba = vec![0_u8; (SIZE * SIZE * 4) as usize];

    paint_icon_rect(&mut rgba, SIZE, 0, 0, SIZE, SIZE, [15, 22, 35, 255]);
    paint_icon_rect(&mut rgba, SIZE, 3, 3, SIZE - 6, SIZE - 6, [28, 42, 66, 255]);
    paint_icon_rect(&mut rgba, SIZE, 5, 5, 9, 22, [92, 169, 255, 255]);
    paint_icon_rect(&mut rgba, SIZE, 18, 5, 9, 22, [125, 196, 130, 255]);
    paint_icon_rect(&mut rgba, SIZE, 7, 8, 5, 2, [217, 235, 255, 255]);
    paint_icon_rect(&mut rgba, SIZE, 20, 8, 5, 2, [231, 247, 234, 255]);
    paint_icon_rect(&mut rgba, SIZE, 15, 5, 2, 22, [15, 22, 35, 255]);
    paint_icon_rect(&mut rgba, SIZE, 7, 13, 5, 10, [15, 22, 35, 160]);
    paint_icon_rect(&mut rgba, SIZE, 20, 13, 5, 10, [15, 22, 35, 160]);

    IconData {
        rgba,
        width: SIZE,
        height: SIZE,
    }
}

fn paint_icon_rect(
    rgba: &mut [u8],
    image_width: u32,
    x: u32,
    y: u32,
    rect_width: u32,
    rect_height: u32,
    color: [u8; 4],
) {
    for yy in y..(y + rect_height) {
        for xx in x..(x + rect_width) {
            let pixel_index = ((yy * image_width + xx) * 4) as usize;
            rgba[pixel_index..pixel_index + 4].copy_from_slice(&color);
        }
    }
}

#[derive(Clone, Default)]
struct DashboardState {
    monitors: Vec<DashboardMonitor>,
    focused_monitor_idx: usize,
}

#[derive(Clone)]
struct DashboardMonitor {
    index: usize,
    name: String,
    size: Rect,
    focused_workspace_idx: usize,
    workspaces: Vec<DesktopWorkspace>,
}

#[derive(Clone)]
struct DesktopWorkspace {
    index: usize,
    exists: bool,
    name: Option<String>,
    tile: bool,
    layout: DefaultLayout,
    layout_is_custom: bool,
    layer: WorkspaceLayer,
    tiled_container_count: usize,
    tiled_window_count: usize,
    stacked_window_count: usize,
    floating_window_count: usize,
    total_window_count: usize,
    window_summaries: Vec<String>,
}

#[derive(Clone, Default)]
struct NativeWindowsIntegration {
    install_bin_dir: Option<PathBuf>,
}

impl DashboardMonitor {
    fn title(&self) -> String {
        let name = if self.name.trim().is_empty() {
            "Unnamed monitor"
        } else {
            self.name.as_str()
        };

        format!(
            "Monitor {} · {} ({}×{})",
            self.index + 1,
            name,
            self.size.right,
            self.size.bottom
        )
    }
}

impl DesktopWorkspace {
    fn placeholder(index: usize) -> Self {
        Self {
            index,
            exists: false,
            name: None,
            tile: true,
            layout: DefaultLayout::BSP,
            layout_is_custom: false,
            layer: WorkspaceLayer::Tiling,
            tiled_container_count: 0,
            tiled_window_count: 0,
            stacked_window_count: 0,
            floating_window_count: 0,
            total_window_count: 0,
            window_summaries: vec![],
        }
    }

    fn display_name(&self, fallback: &str) -> String {
        self.name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| fallback.to_string())
    }
}

impl NativeWindowsIntegration {
    fn detect() -> Self {
        let install_bin_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()));

        Self { install_bin_dir }
    }

    fn cli_command(&self) -> PathBuf {
        if let Some(bin_dir) = &self.install_bin_dir {
            let no_console = bin_dir.join("komorebic-no-console.exe");
            if no_console.is_file() {
                return no_console;
            }

            let cli = bin_dir.join("komorebic.exe");
            if cli.is_file() {
                return cli;
            }
        }

        PathBuf::from("komorebic-no-console.exe")
    }

    fn install_location_label(&self) -> String {
        self.install_bin_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| String::from("PATH-resolved komorebic executable"))
    }
}

impl From<State> for DashboardState {
    fn from(value: State) -> Self {
        let focused_monitor_idx = value.monitors.focused_idx();
        let monitors = value
            .monitors
            .elements()
            .iter()
            .enumerate()
            .map(|(index, monitor)| dashboard_monitor(index, monitor))
            .collect();

        Self {
            monitors,
            focused_monitor_idx,
        }
    }
}

fn dashboard_monitor(index: usize, monitor: &Monitor) -> DashboardMonitor {
    let workspaces = monitor
        .workspaces()
        .iter()
        .enumerate()
        .map(|(workspace_index, workspace)| dashboard_workspace(workspace_index, workspace))
        .collect();

    DashboardMonitor {
        index,
        name: monitor.name.clone(),
        size: monitor.size,
        focused_workspace_idx: monitor.focused_workspace_idx(),
        workspaces,
    }
}

fn dashboard_workspace(index: usize, workspace: &Workspace) -> DesktopWorkspace {
    let (layout, layout_is_custom) = match &workspace.layout {
        Layout::Default(layout) => (*layout, false),
        Layout::Custom(_) => (DefaultLayout::BSP, true),
    };

    let tiled_window_count = workspace
        .containers()
        .iter()
        .map(|container| container.windows().len())
        .sum::<usize>()
        + workspace
            .monocle_container
            .as_ref()
            .map(|container| container.windows().len())
            .unwrap_or(0)
        + usize::from(workspace.maximized_window.is_some());

    let stacked_window_count = workspace
        .containers()
        .iter()
        .map(|container| container.windows().len().saturating_sub(1))
        .sum::<usize>()
        + workspace
            .monocle_container
            .as_ref()
            .map(|container| container.windows().len().saturating_sub(1))
            .unwrap_or(0);

    let floating_window_count = workspace.floating_windows().len();
    let total_window_count = tiled_window_count + floating_window_count;

    DesktopWorkspace {
        index,
        exists: true,
        name: workspace.name.clone(),
        tile: workspace.tile,
        layout,
        layout_is_custom,
        layer: workspace.layer,
        tiled_container_count: workspace.containers().len()
            + usize::from(workspace.monocle_container.is_some()),
        tiled_window_count,
        stacked_window_count,
        floating_window_count,
        total_window_count,
        window_summaries: workspace_window_summaries(workspace),
    }
}

fn workspace_window_summaries(workspace: &Workspace) -> Vec<String> {
    let mut items = Vec::new();

    for container in workspace.containers() {
        push_container_summary(&mut items, None, container);
    }

    if let Some(container) = &workspace.monocle_container {
        push_container_summary(&mut items, Some("Monocle"), container);
    }

    if let Some(window) = &workspace.maximized_window {
        items.push(format!("Maximized · {}", friendly_window_label(window)));
    }

    for window in workspace.floating_windows() {
        items.push(format!("Floating · {}", friendly_window_label(window)));
    }

    if items.len() > VISIBLE_WINDOW_LIMIT {
        let hidden = items.len() - VISIBLE_WINDOW_LIMIT;
        items.truncate(VISIBLE_WINDOW_LIMIT);
        items.push(format!("… plus {hidden} more windows"));
    }

    items
}

fn push_container_summary(items: &mut Vec<String>, prefix: Option<&str>, container: &Container) {
    if let Some(window) = container.focused_window() {
        let mut summary = friendly_window_label(window);
        let stacked = container.windows().len().saturating_sub(1);

        if let Some(prefix) = prefix {
            summary = format!("{prefix} · {summary}");
        }

        if stacked > 0 {
            summary.push_str(&format!(" (+{stacked} stacked)"));
        }

        items.push(summary);
    }
}

fn friendly_window_label(window: &Window) -> String {
    let exe = window.exe().unwrap_or_else(|_| format!("hwnd {}", window.hwnd));
    let title = window.title().unwrap_or_else(|_| String::from("Untitled window"));
    let title = if title.trim().is_empty() {
        String::from("Untitled window")
    } else {
        title
    };

    format!("{exe} — {title}")
}

struct KomorebiGui {
    dashboard: DashboardState,
    native_windows: NativeWindowsIntegration,
    selected_monitor: usize,
    desktop_name_inputs: [String; DASHBOARD_DESKTOP_COUNT],
    name_dirty: [bool; DASHBOARD_DESKTOP_COUNT],
    last_poll_at: Instant,
    last_success_at: Option<Instant>,
    last_error: Option<String>,
}

impl KomorebiGui {
    fn new() -> Self {
        let mut gui = Self {
            dashboard: DashboardState::default(),
            native_windows: NativeWindowsIntegration::detect(),
            selected_monitor: 0,
            desktop_name_inputs: DESKTOP_LABELS.map(|label| label.to_string()),
            name_dirty: [false; DASHBOARD_DESKTOP_COUNT],
            last_poll_at: Instant::now() - AUTO_REFRESH_INTERVAL,
            last_success_at: None,
            last_error: None,
        };

        gui.refresh_dashboard(true);
        gui
    }

    fn refresh_dashboard(&mut self, force_name_sync: bool) {
        self.last_poll_at = Instant::now();

        match query_dashboard() {
            Ok(dashboard) => {
                let previous_monitor = self.selected_monitor;
                self.dashboard = dashboard;

                if self.dashboard.monitors.is_empty() {
                    self.selected_monitor = 0;
                } else if self.selected_monitor >= self.dashboard.monitors.len() {
                    self.selected_monitor = self
                        .dashboard
                        .focused_monitor_idx
                        .min(self.dashboard.monitors.len().saturating_sub(1));
                }

                let monitor_changed = previous_monitor != self.selected_monitor;
                self.sync_name_inputs(force_name_sync || monitor_changed);
                self.last_success_at = Some(Instant::now());
                self.last_error = None;
            }
            Err(error) => {
                self.last_error = Some(error);
            }
        }
    }

    fn sync_name_inputs(&mut self, force: bool) {
        for workspace_idx in 0..DASHBOARD_DESKTOP_COUNT {
            if force || !self.name_dirty[workspace_idx] {
                self.desktop_name_inputs[workspace_idx] = self
                    .current_monitor()
                    .and_then(|monitor| monitor.workspaces.get(workspace_idx))
                    .and_then(|workspace| workspace.name.clone())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| DESKTOP_LABELS[workspace_idx].to_string());

                if force {
                    self.name_dirty[workspace_idx] = false;
                }
            }
        }
    }

    fn current_monitor(&self) -> Option<&DashboardMonitor> {
        self.dashboard.monitors.get(self.selected_monitor)
    }

    fn requested_workspace_name(&self, workspace_idx: usize) -> String {
        self.desktop_name_inputs[workspace_idx]
            .trim()
            .chars()
            .take(64)
            .collect::<String>()
    }

    fn requested_workspace_names(&self) -> Vec<String> {
        (0..DASHBOARD_DESKTOP_COUNT)
            .map(|workspace_idx| {
                let requested = self.requested_workspace_name(workspace_idx);

                if requested.is_empty() {
                    DESKTOP_LABELS[workspace_idx].to_string()
                } else {
                    requested
                }
            })
            .collect()
    }

    fn send_message(&mut self, message: SocketMessage) {
        if let Err(error) = komorebi_client::send_message(&message) {
            self.last_error = Some(format!("command failed: {error}"));
            return;
        }

        self.refresh_dashboard(false);
    }

    fn send_batch(&mut self, messages: Vec<SocketMessage>) {
        if let Err(error) = komorebi_client::send_batch(messages.iter()) {
            self.last_error = Some(format!("command batch failed: {error}"));
            return;
        }

        self.refresh_dashboard(false);
    }

    fn run_native_cli(&mut self, args: &[&str]) -> Result<(), String> {
        let command = self.native_windows.cli_command();
        let output = Command::new(&command)
            .args(args)
            .output()
            .map_err(|error| {
                format!(
                    "could not launch {} with '{}': {error}",
                    command.display(),
                    args.join(" ")
                )
            })?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let details = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("process exited with status {}", output.status)
            };

            Err(format!(
                "{} failed for '{}': {details}",
                command.display(),
                args.join(" ")
            ))
        }
    }

    fn start_manager(&mut self) {
        match self.run_native_cli(&["start"]) {
            Ok(()) => {
                self.last_error = None;
                self.last_success_at = None;
                self.last_poll_at = Instant::now() - AUTO_REFRESH_INTERVAL;
            }
            Err(error) => {
                self.last_error = Some(error);
            }
        }
    }

    fn stop_manager(&mut self) {
        match self.run_native_cli(&["stop"]) {
            Ok(()) => {
                self.dashboard = DashboardState::default();
                self.last_error = None;
                self.last_success_at = None;
                self.last_poll_at = Instant::now() - AUTO_REFRESH_INTERVAL;
            }
            Err(error) => {
                self.last_error = Some(error);
            }
        }
    }

    fn open_install_folder(&mut self) {
        let Some(bin_dir) = self.native_windows.install_bin_dir.clone() else {
            self.last_error = Some(String::from(
                "the install folder could not be resolved for this dashboard instance",
            ));
            return;
        };

        match Command::new("explorer.exe").arg(&bin_dir).spawn() {
            Ok(_) => {
                self.last_error = None;
            }
            Err(error) => {
                self.last_error = Some(format!(
                    "could not open the install folder {}: {error}",
                    bin_dir.display()
                ));
            }
        }
    }

    fn provision_dual_desktops(&mut self) {
        if self.current_monitor().is_none() {
            self.last_error = Some(String::from("komorebi is not exposing any monitors yet"));
            return;
        }

        self.send_batch(vec![SocketMessage::EnsureNamedWorkspaces(
            self.selected_monitor,
            self.requested_workspace_names(),
        )]);

        self.name_dirty = [false; DASHBOARD_DESKTOP_COUNT];
    }

    fn rename_workspace(&mut self, workspace_idx: usize) {
        let name = self.requested_workspace_name(workspace_idx);
        let name = if name.is_empty() {
            DESKTOP_LABELS[workspace_idx].to_string()
        } else {
            name
        };

        self.desktop_name_inputs[workspace_idx] = name.clone();
        self.name_dirty[workspace_idx] = false;
        self.send_message(SocketMessage::WorkspaceName(
            self.selected_monitor,
            workspace_idx,
            name,
        ));
    }

    fn focus_workspace(&mut self, workspace_idx: usize) {
        self.send_message(SocketMessage::FocusMonitorWorkspaceNumber(
            self.selected_monitor,
            workspace_idx,
        ));
    }

    fn move_active_to_workspace(&mut self, workspace_idx: usize, follow: bool) {
        let message = if follow {
            SocketMessage::MoveContainerToMonitorWorkspaceNumber(self.selected_monitor, workspace_idx)
        } else {
            SocketMessage::SendContainerToMonitorWorkspaceNumber(self.selected_monitor, workspace_idx)
        };

        self.send_message(message);
    }

    fn status_message(&self) -> (Color32, String) {
        if let Some(error) = &self.last_error {
            if self.dashboard.monitors.is_empty()
                && error.starts_with("could not query komorebi state:")
            {
                return (
                    Color32::from_rgb(240, 205, 90),
                    String::from("komorebi is not running yet — use Start manager to launch it manually"),
                );
            }

            return (Color32::from_rgb(240, 113, 120), error.clone());
        }

        if let Some(last_success_at) = self.last_success_at {
            let age = last_success_at.elapsed().as_millis();
            return (
                Color32::from_rgb(125, 196, 130),
                format!("synced {age} ms ago"),
            );
        }

        (
            Color32::from_rgb(240, 205, 90),
            String::from("waiting for komorebi state"),
        )
    }

    fn workspace_card(&mut self, ui: &mut egui::Ui, workspace: DesktopWorkspace, is_focused: bool) {
        let fill = if is_focused {
            Color32::from_rgb(36, 56, 88)
        } else {
            ui.visuals().faint_bg_color
        };

        egui::Frame::group(ui.style()).fill(fill).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading(DESKTOP_LABELS[workspace.index]);

                    if is_focused {
                        ui.label(RichText::new("ACTIVE").strong().color(Color32::LIGHT_GREEN));
                    }

                    if !workspace.exists {
                        ui.label(
                            RichText::new("not provisioned yet")
                                .strong()
                                .color(Color32::from_rgb(240, 205, 90)),
                        );
                    }
                });

                ui.label(
                    RichText::new(workspace.display_name(&self.desktop_name_inputs[workspace.index]))
                        .size(20.0),
                );

                ui.small(format!(
                    "{} windows · {} tiled windows · {} floating · {} layer",
                    workspace.total_window_count,
                    workspace.tiled_window_count,
                    workspace.floating_window_count,
                    workspace.layer
                ));

                if workspace.stacked_window_count > 0 {
                    ui.small(format!(
                        "{} stacked windows are hidden behind focused containers",
                        workspace.stacked_window_count
                    ));
                }

                if workspace.layout_is_custom {
                    ui.small("Custom layout is active right now; choosing a preset below will replace it.");
                }

                ui.add_space(6.0);
                ui.label("Custom desktop name");

                let name_response = ui.text_edit_singleline(&mut self.desktop_name_inputs[workspace.index]);

                if name_response.changed() {
                    self.name_dirty[workspace.index] = true;
                }

                if name_response.lost_focus()
                    && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    && workspace.exists
                {
                    self.rename_workspace(workspace.index);
                }

                ui.horizontal_wrapped(|ui| {
                    ui.add_enabled_ui(workspace.exists, |ui| {
                        if ui.button("Rename").clicked() {
                            self.rename_workspace(workspace.index);
                        }
                    });

                    if ui.button("Focus").clicked() {
                        self.focus_workspace(workspace.index);
                    }

                    if ui.button("Move active here").clicked() {
                        self.move_active_to_workspace(workspace.index, true);
                    }

                    if ui.button("Send active here").clicked() {
                        self.move_active_to_workspace(workspace.index, false);
                    }
                });

                ui.add_space(8.0);

                ui.add_enabled_ui(workspace.exists, |ui| {
                    let mut tile = workspace.tile;
                    if ui.checkbox(&mut tile, "Tiling enabled").changed() {
                        self.send_message(SocketMessage::WorkspaceTiling(
                            self.selected_monitor,
                            workspace.index,
                            tile,
                        ));
                    }

                    let mut selected_layout = workspace.layout;
                    egui::ComboBox::from_id_salt(("layout", self.selected_monitor, workspace.index))
                        .selected_text(selected_layout.to_string())
                        .show_ui(ui, |ui| {
                            for option in LAYOUT_OPTIONS {
                                ui.selectable_value(
                                    &mut selected_layout,
                                    option,
                                    option.to_string(),
                                );
                            }
                        });

                    if selected_layout != workspace.layout {
                        self.send_message(SocketMessage::WorkspaceLayout(
                            self.selected_monitor,
                            workspace.index,
                            selected_layout,
                        ));
                    }
                });

                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "{} tiled containers on this desktop",
                        workspace.tiled_container_count
                    ))
                    .strong(),
                );

                if workspace.window_summaries.is_empty() {
                    ui.small("No windows are currently assigned here.");
                } else {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(260.0)
                        .show(ui, |ui| {
                            for summary in &workspace.window_summaries {
                                ui.label(summary);
                            }
                        });
                }
            });
        });
    }
}

fn query_dashboard() -> Result<DashboardState, String> {
    let response = komorebi_client::send_query(&SocketMessage::State)
        .map_err(|error| format!("could not query komorebi state: {error}"))?;

    let state: State =
        serde_json::from_str(&response).map_err(|error| format!("could not parse state: {error}"))?;

    Ok(DashboardState::from(state))
}

impl eframe::App for KomorebiGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(AUTO_REFRESH_INTERVAL);

        if self.last_poll_at.elapsed() >= AUTO_REFRESH_INTERVAL {
            self.refresh_dashboard(false);
        }

        let monitor_choices = self.dashboard.monitors.clone();
        let current_monitor_preview = self.current_monitor().cloned();
        let has_live_session = current_monitor_preview.is_some();
        let (status_colour, status_message) = self.status_message();
        let mut selected_monitor_changed = false;

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.heading("Dual Desktop Dashboard");
                ui.label("A human-friendly dashboard for two named komorebi desktops that complements the CLI.");
            });
            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                let selected_text = current_monitor_preview
                    .as_ref()
                    .map(DashboardMonitor::title)
                    .unwrap_or_else(|| String::from("No monitor detected"));

                egui::ComboBox::from_id_salt("monitor-select")
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for monitor in &monitor_choices {
                            if ui
                                .selectable_value(
                                    &mut self.selected_monitor,
                                    monitor.index,
                                    monitor.title(),
                                )
                                .changed()
                            {
                                selected_monitor_changed = true;
                            }
                        }
                    });

                if ui.button("Refresh now").clicked() {
                    self.refresh_dashboard(false);
                }

                if !has_live_session && ui.button("Start manager").clicked() {
                    self.start_manager();
                }

                if has_live_session && ui.button("Stop manager").clicked() {
                    self.stop_manager();
                }

                if ui
                    .add_enabled(
                        self.native_windows.install_bin_dir.is_some(),
                        egui::Button::new("Open install folder"),
                    )
                    .clicked()
                {
                    self.open_install_folder();
                }

                if ui.button("Apply 2-desktop setup").clicked() {
                    self.provision_dual_desktops();
                }

                ui.colored_label(status_colour, status_message);
            });

            if let Some(monitor) = current_monitor_preview.as_ref() {
                ui.small(format!(
                    "Selected monitor focuses desktop {} right now. The dashboard intentionally exposes the first two desktops only.",
                    monitor.focused_workspace_idx + 1
                ));
            } else {
                ui.small("This dashboard can start the installed window manager for you without needing a repo checkout or an auto-start hook.");
            }

            ui.small("Manual launch only: this dashboard does not register anything to start automatically when you sign in.");
            ui.small(format!(
                "CLI bridge: {}",
                self.native_windows.install_location_label()
            ));

            ui.add_space(4.0);
        });

        if selected_monitor_changed {
            self.name_dirty = [false; DASHBOARD_DESKTOP_COUNT];
            self.sync_name_inputs(true);
        }

        let current_monitor = self.current_monitor().cloned();

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(monitor) = current_monitor {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(monitor.title()).strong());

                    if self.dashboard.focused_monitor_idx == monitor.index {
                        ui.label(
                            RichText::new("currently focused monitor")
                                .color(Color32::LIGHT_GREEN)
                                .strong(),
                        );
                    }
                });

                ui.small("Desktop cards below are backed by real komorebi workspaces on the selected monitor.");
                ui.add_space(12.0);

                ui.columns(DASHBOARD_DESKTOP_COUNT, |columns| {
                    for (workspace_idx, column) in columns.iter_mut().enumerate() {
                        let workspace = monitor
                            .workspaces
                            .get(workspace_idx)
                            .cloned()
                            .unwrap_or_else(|| DesktopWorkspace::placeholder(workspace_idx));

                        let is_focused = self.dashboard.focused_monitor_idx == monitor.index
                            && monitor.focused_workspace_idx == workspace_idx;

                        self.workspace_card(column, workspace, is_focused);
                    }
                });

                if monitor.workspaces.len() > DASHBOARD_DESKTOP_COUNT {
                    ui.add_space(10.0);
                    ui.small(format!(
                        "{} additional desktops exist on this monitor, but they are hidden here to keep the UX focused and low-overhead.",
                        monitor.workspaces.len() - DASHBOARD_DESKTOP_COUNT
                    ));
                }
            } else {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.heading("No live komorebi session detected");
                    ui.label("You can use this installed dashboard like a native Windows app — no repo launch required.");
                    ui.label("Use Start manager whenever you want komorebi running in the background. Nothing here adds a sign-in or startup entry behind your back.");
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Start manager").clicked() {
                            self.start_manager();
                        }

                        if ui
                            .add_enabled(
                                self.native_windows.install_bin_dir.is_some(),
                                egui::Button::new("Open install folder"),
                            )
                            .clicked()
                        {
                            self.open_install_folder();
                        }

                        if ui.button("Refresh connection").clicked() {
                            self.refresh_dashboard(false);
                        }
                    });

                    ui.small(format!(
                        "When installed, the Start Menu shortcut launches this dashboard and uses binaries from {}.",
                        self.native_windows.install_location_label()
                    ));
                });
            }
        });
    }
}
