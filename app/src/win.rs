use crate::pets;
use crate::shared::{lock, panel_height, Ctx, Drag, PANEL_WIDTH};
use nibble_core::config::Anchor;
use serde_json::{json, Value};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{CheckMenuItemBuilder, ContextMenu, Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

const BRIDGE: &str = include_str!("bridge.js");

#[derive(Clone, Copy, Debug)]
pub struct Mon {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub scale: f64,
    pub wx: i32,
    pub wy: i32,
    pub ww: i32,
    pub wh: i32,
}

pub fn monitors(ctx: &Ctx) -> Vec<Mon> {
    ctx.app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let p = m.position();
            let s = m.size();
            let wa = m.work_area();
            Mon {
                x: p.x,
                y: p.y,
                w: s.width as i32,
                h: s.height as i32,
                scale: m.scale_factor(),
                wx: wa.position.x,
                wy: wa.position.y,
                ww: wa.size.width as i32,
                wh: wa.size.height as i32,
            }
        })
        .collect()
}

fn primary(ctx: &Ctx) -> Option<Mon> {
    let all = monitors(ctx);
    let main = ctx.app.primary_monitor().ok().flatten();
    if let Some(m) = main {
        let p = m.position();
        if let Some(found) = all.iter().find(|a| a.x == p.x && a.y == p.y) {
            return Some(*found);
        }
    }
    all.first().copied()
}

fn scale_at(ctx: &Ctx, x: i32, y: i32) -> f64 {
    let all = monitors(ctx);
    all.iter()
        .find(|m| x >= m.x && x < m.x + m.w && y >= m.y && y < m.y + m.h)
        .map(|m| m.scale)
        .or_else(|| primary(ctx).map(|m| m.scale))
        .unwrap_or(1.0)
}

fn monitor_for_rect(ctx: &Ctx, x: i32, y: i32, w: i32, h: i32) -> Option<Mon> {
    let all = monitors(ctx);
    let overlap = |m: &Mon| {
        let ox = (x + w).min(m.x + m.w) - x.max(m.x);
        let oy = (y + h).min(m.y + m.h) - y.max(m.y);
        if ox > 0 && oy > 0 { (ox as i64) * (oy as i64) } else { 0 }
    };
    all.iter().copied().max_by_key(overlap).filter(|m| overlap(m) > 0).or_else(|| primary(ctx))
}

fn default_anchor(ctx: &Ctx) -> Anchor {
    match primary(ctx) {
        Some(m) => Anchor {
            x: m.wx + m.ww - (170.0 * m.scale).round() as i32,
            y: m.wy + m.wh - (6.0 * m.scale).round() as i32,
        },
        None => Anchor { x: 800, y: 600 },
    }
}

pub fn pet_bounds(ctx: &Ctx) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    let layout = ctx.layout();
    let mut anchor = lock(&ctx.config).anchor;
    let all = monitors(ctx);
    let probe = primary(ctx).map(|m| (40.0 * m.scale) as i32).unwrap_or(40);
    let on_screen = anchor.is_some_and(|a| {
        all.iter().any(|m| a.x >= m.x && a.x <= m.x + m.w && a.y - probe >= m.y && a.y - probe <= m.y + m.h)
    });
    if !on_screen {
        anchor = Some(default_anchor(ctx));
        lock(&ctx.config).anchor = anchor;
    }
    let a = anchor.unwrap_or(Anchor { x: 800, y: 600 });
    let scale = scale_at(ctx, a.x, a.y);
    let w = (layout.width * scale).round() as i32;
    let h = (layout.height * scale).round() as i32;
    (PhysicalPosition::new(a.x - (w as f64 / 2.0).round() as i32, a.y - h), PhysicalSize::new(w as u32, h as u32))
}

pub fn resize_pet(ctx: &Ctx) {
    if let Some(win) = ctx.app.get_webview_window("pet") {
        let (pos, size) = pet_bounds(ctx);
        let _ = win.set_size(size);
        let _ = win.set_position(pos);
    }
}

fn base_builder<'a>(ctx: &'a Ctx, label: &'a str, page: &'a str) -> WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle> {
    WebviewWindowBuilder::new(&ctx.app, label, WebviewUrl::App(page.into()))
        .title(nibble_core::NAME)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .minimizable(false)
        .maximizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .initialization_script(BRIDGE)
        .data_directory(ctx.paths.app_data_dir.clone())
}

pub fn create_pet_window(ctx: &Ctx) -> tauri::Result<()> {
    let (pos, size) = pet_bounds(ctx);
    let win = base_builder(ctx, "pet", "pet.html")
        .focusable(false)
        .inner_size(size.width as f64, size.height as f64)
        .build()?;
    win.set_size(size)?;
    win.set_position(pos)?;
    win.set_ignore_cursor_events(true)?;
    win.show()?;
    win.set_skip_taskbar(true)?;
    let c = ctx.clone();
    win.on_window_event(move |event| {
        if let WindowEvent::ScaleFactorChanged { .. } = event {
            resize_pet(&c);
        }
    });
    Ok(())
}

pub fn create_panel_window(ctx: &Ctx) -> tauri::Result<()> {
    let win = base_builder(ctx, "panel", "panel.html")
        .inner_size(PANEL_WIDTH, 520.0)
        .build()?;
    let c = ctx.clone();
    win.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            if !lock(&c.rt).keep_panel_open {
                hide_panel(&c);
            }
        }
    });
    Ok(())
}

fn pet_window(ctx: &Ctx) -> Option<WebviewWindow> {
    ctx.app.get_webview_window("pet")
}

fn panel_window(ctx: &Ctx) -> Option<WebviewWindow> {
    ctx.app.get_webview_window("panel")
}

pub fn position_panel(ctx: &Ctx, view: &str) {
    let (Some(panel), Some(pet)) = (panel_window(ctx), pet_window(ctx)) else { return };
    let height = panel_height(view).unwrap_or(520.0);
    let (Ok(pp), Ok(ps)) = (pet.outer_position(), pet.outer_size()) else { return };
    let Some(mon) = monitor_for_rect(ctx, pp.x, pp.y, ps.width as i32, ps.height as i32) else { return };
    let s = mon.scale;
    let pw = (PANEL_WIDTH * s).round();
    let ph = (height * s).round();
    let gap = (-18.0 * s).round();
    let margin = (8.0 * s).round();
    let (wx, wy, ww, wh) = (mon.wx as f64, mon.wy as f64, mon.ww as f64, mon.wh as f64);
    let petc = pp.x as f64 + ps.width as f64 / 2.0;
    let mut x = if petc > wx + ww / 2.0 { pp.x as f64 - pw - gap } else { pp.x as f64 + ps.width as f64 + gap };
    x = x.max(wx + margin).min(wx + ww - pw - margin);
    let mut y = pp.y as f64 + ps.height as f64 - ph - margin;
    y = y.max(wy + margin).min(wy + wh - ph - margin);
    let _ = panel.set_size(PhysicalSize::new(pw as u32, ph as u32));
    let _ = panel.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

pub fn show_panel(ctx: &Ctx, view: &str, extra: Value) {
    let Some(panel) = panel_window(ctx) else { return };
    lock(&ctx.rt).panel_view = view.to_string();
    position_panel(ctx, view);
    let mut payload = json!({ "name": view });
    if let (Some(base), Some(more)) = (payload.as_object_mut(), extra.as_object()) {
        for (k, v) in more {
            base.insert(k.clone(), v.clone());
        }
    }
    let _ = ctx.app.emit_to("panel", "panel:show", payload);
    let _ = panel.show();
    let _ = panel.set_skip_taskbar(true);
    let _ = panel.set_focus();
}

pub fn hide_panel(ctx: &Ctx) {
    if let Some(panel) = panel_window(ctx) {
        if panel.is_visible().unwrap_or(false) {
            let _ = panel.hide();
        }
    }
}

pub fn toggle_panel(ctx: &Ctx, view: &str, extra: Value) {
    let visible = panel_window(ctx).is_some_and(|p| p.is_visible().unwrap_or(false));
    let same = lock(&ctx.rt).panel_view == view;
    if visible && same {
        hide_panel(ctx);
    } else {
        show_panel(ctx, view, extra);
    }
}

pub fn set_panel_view(ctx: &Ctx, view: &str) {
    if panel_height(view).is_none() {
        return;
    }
    lock(&ctx.rt).panel_view = view.to_string();
    if panel_window(ctx).is_some_and(|p| p.is_visible().unwrap_or(false)) {
        position_panel(ctx, view);
    }
}

pub fn set_ignore(ctx: &Ctx, ignore: bool) {
    let dragging = {
        let mut rt = lock(&ctx.rt);
        rt.pet_ignoring = ignore;
        rt.drag.is_some()
    };
    if !dragging {
        if let Some(win) = pet_window(ctx) {
            let _ = win.set_ignore_cursor_events(ignore);
        }
    }
}

pub fn drag_start(ctx: &Ctx) {
    let Some(win) = pet_window(ctx) else { return };
    if lock(&ctx.rt).drag.is_some() {
        return;
    }
    hide_panel(ctx);
    let (Ok(cursor), Ok(pos)) = (ctx.app.cursor_position(), win.outer_position()) else { return };
    let generation = {
        let mut rt = lock(&ctx.rt);
        rt.drag_generation += 1;
        let generation = rt.drag_generation;
        rt.drag = Some(Drag {
            dx: cursor.x - pos.x as f64,
            dy: cursor.y - pos.y as f64,
            generation,
        });
        generation
    };
    let c = ctx.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(120)).await;
        if lock(&c.rt).drag.as_ref().is_some_and(|d| d.generation == generation) {
            end_drag(&c);
        }
    });
}

pub fn drag_move(ctx: &Ctx) {
    let Some(win) = pet_window(ctx) else { return };
    let (dx, dy) = match lock(&ctx.rt).drag.as_ref() {
        Some(d) => (d.dx, d.dy),
        None => return,
    };
    if let Ok(p) = ctx.app.cursor_position() {
        let _ = win.set_position(PhysicalPosition::new((p.x - dx).round() as i32, (p.y - dy).round() as i32));
    }
}

pub fn end_drag(ctx: &Ctx) {
    if lock(&ctx.rt).drag.take().is_none() {
        return;
    }
    let Some(win) = pet_window(ctx) else { return };
    if let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) {
        ctx.set_anchor(Some(Anchor { x: pos.x + (size.width as f64 / 2.0).round() as i32, y: pos.y + size.height as i32 }));
        resize_pet(ctx);
        ctx.save_config();
    }
    let ignoring = lock(&ctx.rt).pet_ignoring;
    let _ = win.set_ignore_cursor_events(ignoring);
}

pub fn track_pointer(ctx: &Ctx) {
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("nibble-pointer".into())
        .spawn(move || loop {
            let mut again = 60;
            if let Some(win) = pet_window(&ctx) {
                let dragging = lock(&ctx.rt).drag.is_some();
                if win.is_visible().unwrap_or(false) && !dragging {
                    if let (Ok(p), Ok(pos), Ok(size)) = (ctx.app.cursor_position(), win.outer_position(), win.outer_size()) {
                        let inside = p.x >= pos.x as f64
                            && p.x < pos.x as f64 + size.width as f64
                            && p.y >= pos.y as f64
                            && p.y < pos.y as f64 + size.height as f64;
                        let scale = win.scale_factor().unwrap_or(1.0);
                        let (was_inside, ignoring) = {
                            let mut rt = lock(&ctx.rt);
                            let was = rt.pointer_inside;
                            rt.pointer_inside = inside;
                            (was, rt.pet_ignoring)
                        };
                        if inside {
                            again = 25;
                            if ignoring {
                                let _ = ctx.app.emit_to(
                                    "pet",
                                    "pet:pointer",
                                    json!({ "x": (p.x - pos.x as f64) / scale, "y": (p.y - pos.y as f64) / scale }),
                                );
                            }
                        } else if was_inside {
                            lock(&ctx.rt).pet_ignoring = true;
                            let _ = win.set_ignore_cursor_events(true);
                            let _ = ctx.app.emit_to("pet", "pet:pointer", Value::Null);
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(again));
        })
        .ok();
}

fn menu_label(ctx: &Ctx, key: &str) -> String {
    ctx.t(key)
}

fn build_menu(ctx: &Ctx) -> tauri::Result<Menu<tauri::Wry>> {
    let app = &ctx.app;
    let config = ctx.config();
    let shortcut_ok = lock(&ctx.rt).shortcut_ok;
    let mut chat = MenuItemBuilder::with_id("m:chat", menu_label(ctx, "menu.chat"));
    if shortcut_ok && !config.shortcut.is_empty() {
        chat = chat.accelerator(config.shortcut.clone());
    }
    let chat_item = chat.build(app).or_else(|_| MenuItemBuilder::with_id("m:chat", menu_label(ctx, "menu.chat")).build(app))?;
    let activity = MenuItemBuilder::with_id("m:home", menu_label(ctx, "menu.activity")).build(app)?;
    let settings = MenuItemBuilder::with_id("m:settings", menu_label(ctx, "menu.settings")).build(app)?;
    let off = MenuItemBuilder::with_id("m:off", menu_label(ctx, "menu.off")).build(app)?;
    let mut pet_menu = SubmenuBuilder::new(app, menu_label(ctx, "menu.pet"));
    for pet in lock(&ctx.pets).iter() {
        let item = CheckMenuItemBuilder::with_id(format!("m:pet:{}", pet.id), pet.name.clone())
            .checked(config.pet == pet.id)
            .build(app)?;
        pet_menu = pet_menu.item(&item);
    }
    let mini = CheckMenuItemBuilder::with_id("m:pet:mini", menu_label(ctx, "settings.mini"))
        .checked(config.pet == "mini")
        .build(app)?;
    let pet_menu = pet_menu.item(&mini).build()?;
    MenuBuilder::new(app)
        .item(&chat_item)
        .item(&activity)
        .separator()
        .item(&pet_menu)
        .item(&settings)
        .separator()
        .item(&off)
        .build()
}

pub fn handle_menu(ctx: &Ctx, id: &str) {
    match id {
        "m:chat" => from_menu(ctx, "chat"),
        "m:home" => from_menu(ctx, "home"),
        "m:settings" => from_menu(ctx, "settings"),
        "m:off" => ctx.turn_off(),
        other => {
            if let Some(pet) = other.strip_prefix("m:pet:") {
                ctx.set_config(&json!({ "pet": pet }));
            }
        }
    }
}

fn from_menu(ctx: &Ctx, view: &str) {
    let open = lock(&ctx.rt).menu_open;
    if open {
        lock(&ctx.rt).panel_after_menu = Some(view.to_string());
    } else {
        show_panel(ctx, view, Value::Null);
    }
}

pub fn popup_pet_menu(ctx: &Ctx) {
    let Some(win) = pet_window(ctx) else { return };
    {
        let mut rt = lock(&ctx.rt);
        rt.keep_panel_open = true;
        rt.menu_open = true;
    }
    let _ = win.set_focusable(true);
    if let Ok(menu) = build_menu(ctx) {
        let _ = menu.popup(win.as_ref().window());
    }
    let view = {
        let mut rt = lock(&ctx.rt);
        rt.menu_open = false;
        rt.panel_after_menu.take()
    };
    if let Some(view) = view {
        show_panel(ctx, &view, Value::Null);
    }
    let panel_visible = panel_window(ctx).is_some_and(|p| p.is_visible().unwrap_or(false));
    if panel_visible {
        let _ = win.set_always_on_top(true);
    }
    let _ = win.set_focusable(false);
    let _ = win.set_skip_taskbar(true);
    let c = ctx.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        if let Some(panel) = panel_window(&c) {
            if panel.is_visible().unwrap_or(false) && !panel.is_focused().unwrap_or(false) {
                let _ = panel.set_focus();
            }
        }
        lock(&c.rt).keep_panel_open = false;
    });
}

fn tray_image(ctx: &Ctx) -> Option<Image<'static>> {
    let pet = ctx
        .current_pet()
        .or_else(|| lock(&ctx.pets).iter().find(|p| p.id == "ember").cloned())
        .or_else(|| lock(&ctx.pets).first().cloned())?;
    Some(Image::new_owned(pets::tray_rgba(&pet, 32), 32, 32))
}

pub fn create_tray(ctx: &Ctx) -> tauri::Result<()> {
    let menu = build_menu(ctx)?;
    let mut builder = TrayIconBuilder::with_id("main").menu(&menu).show_menu_on_left_click(false).tooltip(nibble_core::NAME);
    if let Some(image) = tray_image(ctx) {
        builder = builder.icon(image);
    }
    let c = ctx.clone();
    builder
        .on_tray_icon_event(move |_tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle_panel(&c, "home", Value::Null);
            }
        })
        .build(&ctx.app)?;
    Ok(())
}

pub fn update_tray(ctx: &Ctx, state: &Value) {
    let Some(tray) = ctx.app.tray_by_id("main") else { return };
    let config = ctx.config();
    let shortcut_ok = lock(&ctx.rt).shortcut_ok;
    let key = format!("{}|{}|{}|{}|{}", config.pet, ctx.language(), shortcut_ok, config.shortcut, lock(&ctx.pets).len());
    let changed = {
        let mut rt = lock(&ctx.rt);
        if rt.tray_key != key {
            rt.tray_key = key;
            true
        } else {
            false
        }
    };
    if changed {
        if let Some(image) = tray_image(ctx) {
            let _ = tray.set_icon(Some(image));
        }
        if let Ok(menu) = build_menu(ctx) {
            let _ = tray.set_menu(Some(menu));
        }
    }
    let overall = &state["overall"];
    let status = overall["status"].as_str().unwrap_or("idle");
    let tip = if status == "idle" {
        nibble_core::NAME.to_string()
    } else {
        let count = overall["count"].as_u64().unwrap_or(0);
        let extra = if count > 1 { format!(" ({count})") } else { String::new() };
        format!("{} · {}{}", nibble_core::NAME, ctx.t(&format!("status.{status}")), extra)
    };
    let _ = tray.set_tooltip(Some(tip));
}

pub fn register_shortcut(ctx: &Ctx) {
    let shortcut = ctx.config().shortcut;
    let gs = ctx.app.global_shortcut();
    let _ = gs.unregister_all();
    let ok = if shortcut.is_empty() { true } else { gs.register(shortcut.as_str()).is_ok() };
    lock(&ctx.rt).shortcut_ok = ok;
}

#[cfg(windows)]
pub fn register_app_id() {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let path = format!("Software\\Classes\\AppUserModelId\\{}", nibble_core::APP_ID);
    if let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(path) {
        let _ = key.set_value("DisplayName", &nibble_core::NAME);
    }
}

#[cfg(not(windows))]
pub fn register_app_id() {}

#[cfg(windows)]
pub fn notify(ctx: &Ctx, title: &str, body: &str, sid: &str) {
    use tauri_winrt_notification::Toast;
    let c = ctx.clone();
    let sid = sid.to_string();
    let _ = Toast::new(nibble_core::APP_ID)
        .title(title)
        .text1(body)
        .sound(None)
        .on_activated(move |_| {
            show_panel(&c, "thread", json!({ "sid": sid }));
            Ok(())
        })
        .show();
}

#[cfg(not(windows))]
pub fn notify(_ctx: &Ctx, _title: &str, _body: &str, _sid: &str) {}
