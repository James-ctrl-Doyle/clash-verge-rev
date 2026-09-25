//! 窗口状态持久化。
//!
//! 上游用的是 `tauri-plugin-window-state`，它把状态写到 Tauri 的 `app_config_dir()`
//! （Windows 上即 `%APPDATA%\<identifier>`），且该路径由 Tauri 内部决定、**无法配置**。
//! 这会让便携版仍在系统盘留下 `window_state.json`，破坏"整个目录可以整体搬走"的目标。
//!
//! 因此这里自行实现同样的功能（保存 / 恢复 / 多显示器校验），但存放在
//! [`dirs::app_home_dir()`] 下 —— 便携模式下就是程序同目录的 `config/`。
//! 文件格式与上游插件保持一致（`{"<窗口label>": {...}}`），旧文件可直接沿用。

use crate::{constants::files, utils::dirs};
use clash_verge_logging::{Type, logging};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow, WindowEvent};

/// 上一次落盘的时间戳（毫秒），用于节流 —— 拖动窗口时 Moved 事件极其密集。
static LAST_SAVE_MS: AtomicU64 = AtomicU64::new(0);

/// 两次落盘之间的最小间隔。
const SAVE_INTERVAL_MS: u64 = 500;

fn default_true() -> bool {
    true
}

/// 单个窗口的状态。字段与上游插件一致，保证旧文件可直接读取。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WindowState {
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    #[serde(default)]
    prev_x: i32,
    #[serde(default)]
    prev_y: i32,
    #[serde(default)]
    maximized: bool,
    #[serde(default = "default_true")]
    visible: bool,
    #[serde(default = "default_true")]
    decorated: bool,
    #[serde(default)]
    fullscreen: bool,
}

fn state_file() -> Option<PathBuf> {
    dirs::app_home_dir()
        .ok()
        .map(|dir| dir.join(files::WINDOW_STATE))
}

fn read_all() -> HashMap<String, WindowState> {
    state_file()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// 判断保存的位置是否仍落在某个显示器上 —— 否则窗口会恢复到一个看不见的地方
/// （例如上次使用的显示器已被拔掉）。
fn position_is_visible(window: &WebviewWindow, x: i32, y: i32) -> bool {
    let Ok(monitors) = window.available_monitors() else {
        return true;
    };
    if monitors.is_empty() {
        return true;
    }

    monitors.iter().any(|monitor| {
        let position = monitor.position();
        let size = monitor.size();
        let (left, top) = (position.x, position.y);
        let (width, height) = (size.width as i32, size.height as i32);
        x >= left && x < left + width && y >= top && y < top + height
    })
}

fn apply_saved(window: &WebviewWindow, state: &WindowState) {
    if state.maximized {
        let _ = window.maximize();
        return;
    }

    if state.width > 0 && state.height > 0 {
        let _ = window.set_size(PhysicalSize::new(state.width, state.height));
    }
    if position_is_visible(window, state.x, state.y) {
        let _ = window.set_position(PhysicalPosition::new(state.x, state.y));
    }
    if state.fullscreen {
        let _ = window.set_fullscreen(true);
    }
}

fn capture(window: &WebviewWindow) -> Option<WindowState> {
    let size = window.inner_size().ok()?;
    let position = window.outer_position().ok()?;

    Some(WindowState {
        width: size.width,
        height: size.height,
        x: position.x,
        y: position.y,
        prev_x: 0,
        prev_y: 0,
        maximized: window.is_maximized().unwrap_or(false),
        visible: window.is_visible().unwrap_or(true),
        decorated: true,
        fullscreen: window.is_fullscreen().unwrap_or(false),
    })
}

/// 立即把当前窗口状态写入 `<app_home_dir>/window_state.json`。
pub fn save(window: &WebviewWindow) {
    let Some(state) = capture(window) else {
        return;
    };

    let mut all = read_all();
    all.insert(window.label().to_string(), state);

    let Some(path) = state_file() else {
        return;
    };

    match serde_json::to_string_pretty(&all) {
        Ok(text) => {
            if let Err(error) = fs::write(&path, text) {
                logging!(warn, Type::File, "保存窗口状态失败: {error}");
            }
        }
        Err(error) => logging!(warn, Type::File, "序列化窗口状态失败: {error}"),
    }
}

/// 节流保存：拖动 / 缩放时事件非常密集，没必要每次都落盘。
fn save_throttled(window: &WebviewWindow) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);

    if now.saturating_sub(LAST_SAVE_MS.load(Ordering::Relaxed)) < SAVE_INTERVAL_MS {
        return;
    }
    LAST_SAVE_MS.store(now, Ordering::Relaxed);
    save(window);
}

/// 恢复已保存的状态，并安装事件监听（移动/缩放节流保存，关闭时立即保存）。
pub fn install(window: &WebviewWindow) {
    let label = window.label().to_string();

    if let Some(state) = read_all().get(&label) {
        logging!(debug, Type::Setup, "恢复窗口状态: {label}");
        apply_saved(window, state);
    }

    let watched = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => save_throttled(&watched),
        WindowEvent::CloseRequested { .. } => save(&watched),
        _ => {}
    });
}
