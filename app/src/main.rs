#![cfg_attr(windows, windows_subsystem = "windows")]

mod connect;
mod control;
mod ipc;
mod locales;
mod pets;
mod presence;
mod quick;
mod reducer;
mod shared;
mod store;
mod win;

use nibble_core::paths::Paths;
use serde_json::{json, Value};
use shared::{lock, Ctx, Runtime, Shared};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Manager, RunEvent};
use tauri_plugin_global_shortcut::ShortcutState;

fn main() {
    let paths = Paths::detect();
    if std::env::args().any(|a| a == "--disconnect") {
        let _ = connect::disconnect(&paths);
        return;
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        if let Some(ctx) = app.try_state::<Ctx>() {
                            win::show_panel(ctx.inner(), "chat", Value::Null);
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![ipc::ipc])
        .setup(move |app| {
            let handle = app.handle().clone();
            let _ = std::fs::create_dir_all(&paths.pet_home);
            let mut config = nibble_core::config::Config::load(&paths.config_file);
            config.visible = true;
            config.save(&paths.config_file);
            let store = store::Store::new(paths.events_dir.clone(), paths.registry_dir.clone());
            store.set_show_background(config.show_background);
            let pets = pets::load_all(&paths);
            let mut connection = connect::status(&paths);
            if connection.connected || connection.partial {
                let _ = connect::install_hook_file(&paths);
                connect::install_skills(&paths);
                if connection.outdated {
                    if let Ok(updated) = connect::connect(&paths) {
                        connection = updated;
                    }
                }
            }
            let ctx: Ctx = Arc::new(Shared {
                app: handle,
                paths: paths.clone(),
                config: Mutex::new(config),
                pets: Mutex::new(pets),
                tables: locales::Tables::load(),
                store: store.clone(),
                rt: Mutex::new(Runtime {
                    connection,
                    hatching: false,
                    last_hatch: None,
                    shortcut_ok: true,
                    panel_view: "home".into(),
                    keep_panel_open: false,
                    menu_anchor: None,
                    drag: None,
                    pointer_inside: false,
                    pet_ignoring: true,
                    broadcast_pending: false,
                    asks: HashMap::new(),
                    ask_counter: 0,
                    alerted: HashMap::new(),
                    alerts_armed: false,
                    tray_key: String::new(),
                    drag_generation: 0,
                }),
            });
            app.manage(ctx.clone());

            win::register_app_id();
            ctx.write_app_info();
            start_control(&ctx);
            win::track_pointer(&ctx);
            win::create_pet_window(&ctx)?;
            win::create_panel_window(&ctx)?;
            win::create_menu_window(&ctx)?;
            win::create_tray(&ctx)?;
            win::register_shortcut(&ctx);

            let c = ctx.clone();
            store.set_on_change(move || {
                c.drop_answered_asks();
                c.announce_changes();
                c.broadcast();
            });
            store.start();
            let c = ctx.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(4)).await;
                lock(&c.rt).alerts_armed = true;
            });
            let c = ctx.clone();
            let s = store.clone();
            presence::watch_claude(move || s.live_pids(), move || c.app.exit(0));
            watch_connection(&ctx);
            ctx.broadcast();
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to start Nibble");

    app.run(|handle, event| match event {
        RunEvent::ExitRequested { code, api, .. } => {
            if code.is_none() {
                api.prevent_exit();
            }
        }
        RunEvent::Exit => {
            if let Some(ctx) = handle.try_state::<Ctx>() {
                ctx.store.stop();
            }
        }
        _ => {}
    });
}

fn start_control(ctx: &Ctx) {
    let c = ctx.clone();
    let handler: control::Handler = Arc::new(move |word, payload, closed| {
        let c = c.clone();
        Box::pin(async move { run_command(c, word, payload, closed).await })
    });
    let _ = control::listen(&ctx.paths.pet_home, handler);
}

async fn run_command(ctx: Ctx, word: String, payload: Option<Value>, mut closed: control::Closed) -> Value {
    match word.as_str() {
        "ping" => json!({ "ok": true }),
        "off" => {
            ctx.turn_off();
            json!({ "ok": true })
        }
        "chat" => {
            win::show_panel(&ctx, "chat", Value::Null);
            json!({ "ok": true })
        }
        "ask" => {
            let payload = payload.unwrap_or(Value::Null);
            let sid = nibble_core::sanitize(payload.get("sid").and_then(Value::as_str).unwrap_or(""));
            if sid.is_empty() || !ctx.config().approvals {
                return json!({ "ok": true, "decision": null });
            }
            ctx.store.read_log(&format!("{sid}.jsonl"));
            ctx.settle_ask(&sid, None);
            let clip = |key: &str, n: usize| -> String {
                payload.get(key).and_then(Value::as_str).unwrap_or("").chars().take(n).collect()
            };
            let (id, rx) = ctx.register_ask(&sid, clip("tool", 80), clip("detail", 300));
            tokio::select! {
                decision = rx => json!({ "ok": true, "decision": decision.ok().flatten() }),
                _ = closed.changed() => {
                    ctx.forget_ask(&sid, id);
                    json!({ "ok": true, "decision": null })
                }
            }
        }
        _ => json!({ "ok": false }),
    }
}

fn watch_connection(ctx: &Ctx) {
    let c = ctx.clone();
    tauri::async_runtime::spawn(async move {
        let mut monitors = win::monitors(&c).len();
        loop {
            tokio::time::sleep(Duration::from_secs(15)).await;
            let now = connect::status(&c.paths);
            let changed = {
                let mut rt = lock(&c.rt);
                let changed = now.connected != rt.connection.connected || now.partial != rt.connection.partial;
                if changed {
                    rt.connection = now;
                }
                changed
            };
            if changed {
                c.broadcast();
            }
            let count = win::monitors(&c).len();
            if count != monitors {
                monitors = count;
                win::resize_pet(&c);
            }
        }
    });
}
