use crate::shared::{lock, Ctx};
use crate::{connect, pets, quick, win};
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::{Emitter, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

fn arg_str(args: &[Value], i: usize) -> String {
    args.get(i).and_then(Value::as_str).unwrap_or("").to_string()
}

fn clip(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tauri::async_runtime::spawn_blocking(f).await.ok()
}

async fn with_dialog<T: Send + 'static>(ctx: &Ctx, open: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    lock(&ctx.rt).keep_panel_open = true;
    let result = blocking(open).await;
    lock(&ctx.rt).keep_panel_open = false;
    if let Some(panel) = ctx.app.get_webview_window("panel") {
        let _ = panel.set_focus();
    }
    result
}

#[tauri::command]
pub async fn ipc(window: WebviewWindow, state: State<'_, Ctx>, channel: String, args: Vec<Value>) -> Result<Value, String> {
    let ctx: Ctx = state.inner().clone();
    let ok = Ok(Value::Null);
    match channel.as_str() {
        "ready" => {
            let c = ctx.clone();
            let snapshot = blocking(move || c.public_state()).await.unwrap_or(Value::Null);
            let _ = ctx.app.emit_to(window.label(), "state", snapshot);
        }
        "pet:ignore" => win::set_ignore(&ctx, args.first().and_then(Value::as_bool).unwrap_or(true)),
        "pet:drag-start" => win::drag_start(&ctx),
        "pet:drag-move" => win::drag_move(&ctx),
        "pet:drag-end" => win::end_drag(&ctx),
        "pet:action" => {
            let action = arg_str(&args, 0);
            let sid = args.get(1).cloned();
            match action.as_str() {
                "chat" => win::toggle_panel(&ctx, "chat", Value::Null),
                "activity" | "pet" => win::toggle_panel(&ctx, "home", Value::Null),
                "menu" => {
                    let c = ctx.clone();
                    blocking(move || win::popup_pet_menu(&c)).await;
                }
                "bubble" => {
                    let wanted = ctx.session_id(sid.as_ref());
                    let top = if wanted.is_empty() { None } else { ctx.store.get(&wanted).map(|s| s.id) }
                        .or_else(|| ctx.store.snapshot().overall.top.map(|t| t.id));
                    match top {
                        Some(id) => win::show_panel(&ctx, "thread", json!({ "sid": id })),
                        None => win::toggle_panel(&ctx, "home", Value::Null),
                    }
                }
                _ => {}
            }
            if !ctx.config().welcomed {
                ctx.set_config(&json!({ "welcomed": true }));
            }
        }
        "panel:view" => win::set_panel_view(&ctx, &arg_str(&args, 0)),
        "panel:close" => win::hide_panel(&ctx),
        "chat:send" => {
            let c = ctx.clone();
            let payload = args.first().cloned().unwrap_or(Value::Null);
            return Ok(blocking(move || c.start_quick_chat(&payload)).await.unwrap_or(Value::Null));
        }
        "chat:reply" => {
            let payload = args.first().cloned().unwrap_or(Value::Null);
            return Ok(ctx.reply_to_chat(&payload));
        }
        "thread:seen" => {
            let sid = ctx.session_id(args.first());
            if !sid.is_empty() {
                ctx.store.record(&sid, json!({ "ev": "Seen" }));
            }
        }
        "thread:dismiss" => {
            let sid = ctx.session_id(args.first());
            if !sid.is_empty() {
                ctx.store.record(&sid, json!({ "ev": "Dismiss" }));
            }
        }
        "threads:clear" => {
            for s in ctx.store.snapshot().sessions {
                use crate::reducer::Status;
                if matches!(s.status, Status::Ready | Status::Blocked) || (s.status == Status::Idle && !s.live) {
                    ctx.store.record(&s.id, json!({ "ev": "Dismiss" }));
                }
            }
        }
        "thread:terminal" => {
            let sid = ctx.session_id(args.first());
            let Some(session) = ctx.store.get(&sid) else { return Ok(json!(false)) };
            win::hide_panel(&ctx);
            return Ok(json!(quick::resume_in_terminal(&session.cwd, &session.id)));
        }
        "thread:folder" => {
            let sid = ctx.session_id(args.first());
            if let Some(session) = ctx.store.get(&sid) {
                if !session.cwd.is_empty() && PathBuf::from(&session.cwd).exists() {
                    let _ = open::that(&session.cwd);
                }
            }
        }
        "claude-app:open" => {
            win::hide_panel(&ctx);
            let _ = open::that("claude://");
        }
        "link:open" => {
            let url = arg_str(&args, 0);
            let lower = url.to_lowercase();
            if lower.starts_with("http://") || lower.starts_with("https://") {
                let _ = open::that(url);
            }
        }
        "config:set" => {
            let patch = args.first().cloned().unwrap_or(Value::Null);
            ctx.set_config(&patch);
        }
        "folder:pick" => {
            let panel = ctx.app.get_webview_window("panel");
            let app = ctx.app.clone();
            let picked = with_dialog(&ctx, move || {
                let mut dialog = app.dialog().file();
                if let Some(p) = &panel {
                    dialog = dialog.set_parent(p);
                }
                dialog.blocking_pick_folder()
            })
            .await
            .flatten();
            return Ok(picked
                .and_then(|p| p.into_path().ok())
                .map(|p| json!(p.to_string_lossy()))
                .unwrap_or(Value::Null));
        }
        "connect" | "disconnect" => {
            let c = ctx.clone();
            let connecting = channel == "connect";
            let result = blocking(move || {
                if connecting { connect::connect(&c.paths) } else { connect::disconnect(&c.paths) }
            })
            .await
            .unwrap_or_else(|| Err("failed".into()));
            let connection = match result {
                Ok(connection) => connection,
                Err(e) => {
                    let mut current = connect::status(&ctx.paths);
                    current.error = Some(e);
                    current
                }
            };
            lock(&ctx.rt).connection = connection.clone();
            ctx.broadcast();
            let mut value = serde_json::to_value(&connection).unwrap_or(Value::Null);
            value["settingsFile"] = json!(crate::shared::tilde_path(&connection.settings_file));
            return Ok(value);
        }
        "pets:refresh" => {
            ctx.reload_pets();
            ctx.broadcast();
        }
        "pets:delete" => {
            let id = arg_str(&args, 0);
            if !id.is_empty() {
                pets::delete_custom(&ctx.paths, &id);
                ctx.reload_pets();
                if ctx.config().pet == id {
                    ctx.set_config(&json!({ "pet": "ember" }));
                }
                win::resize_pet(&ctx);
                ctx.broadcast();
            }
        }
        "pets:hatch" => {
            let description = arg_str(&args, 0);
            let c = ctx.clone();
            return Ok(blocking(move || c.hatch_pet(&description)).await.unwrap_or(Value::Null));
        }
        "pets:import" => {
            let panel = ctx.app.get_webview_window("panel");
            let app = ctx.app.clone();
            let picked = with_dialog(&ctx, move || {
                let mut dialog = app.dialog().file().add_filter("Pet", &["json"]);
                if let Some(p) = &panel {
                    dialog = dialog.set_parent(p);
                }
                dialog.blocking_pick_file()
            })
            .await
            .flatten()
            .and_then(|p| p.into_path().ok());
            let Some(file) = picked else { return Ok(json!({ "ok": false, "cancelled": true })) };
            let outcome = (|| -> Result<String, String> {
                let size = std::fs::metadata(&file).map_err(|e| e.to_string())?.len();
                if size > 200_000 {
                    return Err("file too large".into());
                }
                let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
                let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
                pets::save_custom(&ctx.paths, &value)
            })();
            return Ok(match outcome {
                Ok(id) => {
                    ctx.reload_pets();
                    ctx.set_config(&json!({ "pet": id }));
                    json!({ "ok": true, "id": id })
                }
                Err(e) => json!({ "ok": false, "error": clip(&e, 200) }),
            });
        }
        "pets:export" => {
            let id = arg_str(&args, 0);
            let pet = lock(&ctx.pets).iter().find(|p| p.id == id).cloned();
            let Some(pet) = pet else { return Ok(json!({ "ok": false })) };
            let mut slug = pets::slug(&pet.name, 1000);
            if slug.is_empty() {
                slug = "pet".into();
            }
            let panel = ctx.app.get_webview_window("panel");
            let app = ctx.app.clone();
            let picked = with_dialog(&ctx, move || {
                let mut dialog = app.dialog().file().add_filter("Pet", &["json"]).set_file_name(format!("{slug}.json"));
                if let Some(p) = &panel {
                    dialog = dialog.set_parent(p);
                }
                dialog.blocking_save_file()
            })
            .await
            .flatten()
            .and_then(|p| p.into_path().ok());
            let Some(file) = picked else { return Ok(json!({ "ok": false, "cancelled": true })) };
            let written = serde_json::to_string_pretty(&pets::portable(&pet))
                .map_err(|e| e.to_string())
                .and_then(|text| std::fs::write(&file, text + "\n").map_err(|e| e.to_string()));
            return Ok(match written {
                Ok(()) => json!({ "ok": true }),
                Err(e) => json!({ "ok": false, "error": clip(&e, 200) }),
            });
        }
        "pet:off" => ctx.turn_off(),
        "ask:answer" => {
            let sid = ctx.session_id(args.first());
            if lock(&ctx.rt).asks.contains_key(&sid) {
                let allow = args.get(1).and_then(Value::as_bool).unwrap_or(false);
                ctx.settle_ask(&sid, Some(if allow { "allow" } else { "deny" }));
                ctx.store.record(&sid, json!({ "ev": "Answered" }));
            }
        }
        "history:clear" => {
            ctx.store.clear();
            lock(&ctx.rt).alerted.clear();
        }
        _ => {}
    }
    ok
}
