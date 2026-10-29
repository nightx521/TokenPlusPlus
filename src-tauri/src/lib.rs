mod adapters;
mod agent2api;
mod antigravity;
mod desktop;
mod model;
mod providers;

use model::{Settings, Snapshot, Store};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
mod startup;

#[tauri::command]
fn autostart_enabled() -> Result<bool, String> {
    startup::enabled()
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<bool, String> {
    startup::set(enabled)
}
struct AppState {
    store: Mutex<Store>,
    busy: Mutex<HashSet<String>>,
    schedule: Mutex<HashMap<String, (Instant, u32)>>,
    path: PathBuf,
    client: reqwest::Client,
}

struct RefreshGuard<'a>(&'a Mutex<HashSet<String>>, String);
impl Drop for RefreshGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut busy) = self.0.lock() {
            busy.remove(&self.1);
        }
    }
}

fn persist(state: &AppState, store: &Store) -> Result<(), String> {
    let parent = state.path.parent().ok_or("本地设置路径无效")?;
    std::fs::create_dir_all(parent).map_err(|_| "无法创建本地设置目录")?;
    let temp = state.path.with_extension("tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec_pretty(store).map_err(|_| "设置序列化失败")?,
    )
    .map_err(|_| "无法保存本地设置")?;
    std::fs::rename(temp, &state.path).map_err(|_| "无法更新本地设置文件".into())
}

fn save_geometry(app: &tauri::AppHandle) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let Ok(bounds) = desktop::screen_bounds(&w) else {
        return;
    };
    let state = app.state::<AppState>();
    if let Ok(mut store) = state.store.try_lock() {
        if store.settings.desktop_mode {
            store.settings.desktop_position = Some((bounds.0, bounds.1));
        } else {
            store.settings.panel_bounds = Some(bounds);
        }
        let _ = persist(&state, &store);
    };
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewState {
    settings: Settings,
    snapshots: Vec<Snapshot>,
    configured: HashMap<String, bool>,
    version: &'static str,
}

#[tauri::command]
fn get_state(state: tauri::State<AppState>) -> Result<ViewState, String> {
    let store = state.store.lock().map_err(|_| "设置暂不可用")?;
    let configured = adapters::ALL
        .into_iter()
        .filter(|a| a.needs_credential())
        .map(|a| {
            (
                a.id().into(),
                if a == adapters::Adapter::Antigravity {
                    antigravity::credential(&store.settings).is_ok()
                } else if a == adapters::Adapter::Agent2Api {
                    agent2api::credential(&store.settings).is_ok()
                } else {
                    providers::credential(a.id()).is_ok()
                },
            )
        })
        .collect();
    Ok(ViewState {
        settings: store.settings.clone(),
        snapshots: store.snapshots.clone(),
        configured,
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[tauri::command]
fn save_settings(
    app: tauri::AppHandle,
    mut settings: Settings,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    antigravity::validate(&settings)?;
    agent2api::validate(&settings)?;
    if !settings.agent2api_url.is_empty() {
        settings.agent2api_url = agent2api::base(&settings)?;
    }
    settings.agent2api_selection.version = 1;
    if !settings.antigravity_url.is_empty() {
        settings.antigravity_url = antigravity::service_base(&settings)?;
    }
    if !(1..=120).contains(&settings.balance_minutes)
        || !(1..=120).contains(&settings.quota_minutes)
    {
        return Err("刷新间隔应为 1–120 分钟".into());
    }
    if !["cn", "intl"].contains(&settings.huawei_site.as_str()) {
        return Err("请选择有效的华为云站点".into());
    }
    if !["frosted", "liquid"].contains(&settings.glass.as_str())
        || !["light", "dark"].contains(&settings.glass_tone.as_str())
        || !(30..=95).contains(&settings.opacity)
        || !(90..=140).contains(&settings.font_scale)
    {
        return Err("显示设置超出有效范围".into());
    }
    settings
        .visible_platforms
        .retain(|p| adapters::Adapter::parse(p).is_some());
    let busy = state.busy.lock().map_err(|_| "刷新状态暂不可用")?;
    let mut store = state.store.lock().map_err(|_| "设置暂不可用")?;
    if (busy.contains("huawei") && settings.huawei_site != store.settings.huawei_site)
        || (busy.contains("codex") && settings.codex_path != store.settings.codex_path)
        || (busy.contains("antigravity")
            && ((settings.antigravity_port != store.settings.antigravity_port
                || settings.antigravity_url != store.settings.antigravity_url)
                || settings.antigravity_account != store.settings.antigravity_account
                || settings.antigravity_groups != store.settings.antigravity_groups))
    {
        return Err("正在查询该平台，请稍后再修改站点或程序路径".into());
    }
    if busy.contains("agent2api")
        && (settings.agent2api_url != store.settings.agent2api_url
            || serde_json::to_value(&settings.agent2api_selection).ok()
                != serde_json::to_value(&store.settings.agent2api_selection).ok())
    {
        return Err("正在查询 Agent2API，请稍后再修改连接或显示项".into());
    }
    let old = store.clone();
    let mut invalidate = HashSet::new();
    if old.settings.agent2api_url != settings.agent2api_url
        || old.settings.agent2api_minutes != settings.agent2api_minutes
        || serde_json::to_value(&old.settings.agent2api_selection).ok()
            != serde_json::to_value(&settings.agent2api_selection).ok()
    {
        invalidate.insert("agent2api");
    }
    if old.settings.agent2api_url != settings.agent2api_url {
        replace(
            &mut store,
            Snapshot::empty("agent2api", "idle", "服务地址已修改，请验证此地址的令牌"),
        );
    }
    if old.settings.balance_minutes != settings.balance_minutes {
        invalidate.extend(["deepseek", "huawei"]);
    }
    if old.settings.huawei_site != settings.huawei_site {
        invalidate.insert("huawei");
    }
    if old.settings.quota_minutes != settings.quota_minutes
        || old.settings.codex_path != settings.codex_path
    {
        invalidate.insert("codex");
    }
    if old.settings.antigravity_port != settings.antigravity_port
        || old.settings.antigravity_url != settings.antigravity_url
        || old.settings.antigravity_account != settings.antigravity_account
        || old.settings.antigravity_groups != settings.antigravity_groups
        || old.settings.antigravity_minutes != settings.antigravity_minutes
    {
        invalidate.insert("antigravity");
    }
    // Geometry is owned by the native window, not stale form state.
    settings.desktop_position = old.settings.desktop_position;
    settings.panel_bounds = old.settings.panel_bounds;
    if let Some(w) = app.get_webview_window("main") {
        if settings.desktop_mode != old.settings.desktop_mode {
            if let Ok(bounds) = desktop::screen_bounds(&w) {
                if old.settings.desktop_mode {
                    settings.desktop_position = Some((bounds.0, bounds.1));
                } else {
                    settings.panel_bounds = Some(bounds);
                }
            }
        }
        desktop::apply(&w, &old.settings, &settings)?;
    }
    if settings.huawei_site != store.settings.huawei_site {
        replace(
            &mut store,
            Snapshot::empty("huawei", "idle", "站点已更新，等待查询"),
        );
    }
    if settings.codex_path != store.settings.codex_path {
        replace(
            &mut store,
            Snapshot::empty("codex", "idle", "路径已更新，等待查询"),
        );
    }
    if (settings.antigravity_port != store.settings.antigravity_port
        || settings.antigravity_url != store.settings.antigravity_url)
        || settings.antigravity_account != store.settings.antigravity_account
        || settings.antigravity_groups != store.settings.antigravity_groups
    {
        let retained = antigravity::retain_view(
            store.snapshots.iter().find(|s| s.provider == "antigravity"),
            &old.settings,
        );
        replace(&mut store, retained);
    }
    store.settings = settings;
    if let Err(e) = persist(&state, &store) {
        if let Some(w) = app.get_webview_window("main") {
            let _ = desktop::apply(&w, &store.settings, &old.settings);
        }
        *store = old;
        return Err(e);
    }
    if !invalidate.is_empty() {
        let mut schedule = state.schedule.lock().map_err(|_| "刷新设置失败")?;
        for provider in invalidate {
            schedule.remove(provider);
        }
    }
    let _ = app.emit("settings-updated", &store.settings);
    Ok(())
}

#[tauri::command]
fn set_display_mode(app: tauri::AppHandle, desktop: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut settings = state
        .store
        .lock()
        .map_err(|_| "设置不可用")?
        .settings
        .clone();
    settings.desktop_mode = desktop;
    save_settings(app.clone(), settings, app.state::<AppState>())?;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        if !desktop {
            let _ = w.set_focus();
        }
    }
    Ok(())
}

#[tauri::command]
fn remember_widget_position(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let w = app.get_webview_window("main").ok_or("窗口不存在")?;
    let (x, y, _, _) = desktop::screen_bounds(&w)?;
    let mut store = state.store.lock().map_err(|_| "设置不可用")?;
    if store.settings.desktop_mode {
        store.settings.desktop_position = Some((x, y));
        persist(&state, &store)?;
    }
    Ok(())
}

#[tauri::command]
fn reset_widget_position(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut s = state
        .store
        .lock()
        .map_err(|_| "设置不可用")?
        .settings
        .clone();
    s.desktop_position = None;
    // Force a fresh attachment to choose an on-screen default.
    let mut old = s.clone();
    old.desktop_mode = false;
    if let Some(w) = app.get_webview_window("main") {
        desktop::apply(&w, &old, &s)?;
    }
    let mut store = state.store.lock().map_err(|_| "设置不可用")?;
    store.settings.desktop_position = None;
    persist(&state, &store)
}

#[tauri::command]
fn desktop_menu(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::menu::{ContextMenu, Menu, MenuItem};
    let labels = [
        ("full", "打开完整面板"),
        ("refresh", "刷新额度"),
        ("compact", "切换精简 / 标准"),
        ("glass", "切换毛玻璃 / 液态玻璃"),
        ("lock", "锁定 / 解锁位置"),
        ("reset", "重置位置"),
        ("hide", "隐藏桌面组件"),
    ];
    let items: Vec<_> = labels
        .iter()
        .map(|(id, label)| {
            MenuItem::with_id(&app, format!("widget-{id}"), *label, true, None::<&str>)
        })
        .collect::<Result<_, _>>()
        .map_err(|_| "创建菜单失败")?;
    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    let menu = Menu::with_items(&app, &refs).map_err(|_| "创建菜单失败")?;
    let w = app.get_webview_window("main").ok_or("窗口不存在")?;
    menu.popup(w.as_ref().window().clone())
        .map_err(|_| "打开菜单失败".into())
}

#[tauri::command]
fn save_credentials(
    provider: String,
    key: String,
    secret: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    if !adapters::Adapter::parse(&provider).is_some_and(|a| a.needs_credential()) {
        return Err("该平台不使用此类凭据".into());
    }
    let busy = state.busy.lock().map_err(|_| "查询状态暂不可用")?;
    if busy.contains(&provider) {
        return Err("该平台正在刷新，请完成后保存凭据".into());
    }
    if key.trim().is_empty() || (provider == "huawei" && secret.trim().is_empty()) {
        return Err("请填写完整凭据".into());
    }
    let mut store = state.store.lock().map_err(|_| "设置暂不可用")?;
    let value = if provider == "huawei" {
        serde_json::json!({"key":key.trim(),"secret":secret.trim()}).to_string()
    } else {
        key.trim().into()
    };
    if provider == "agent2api" {
        agent2api::save_credential(&store.settings, &key)?;
    } else if provider == "antigravity" {
        antigravity::save_credential(&store.settings, &key)?;
    } else {
        keyring::Entry::new("TokenPlusPlus", &provider)
            .map_err(|_| "无法访问系统凭据存储")?
            .set_password(&value)
            .map_err(|_| "凭据保存失败，请检查系统凭据管理器")?;
    }
    let next = if provider == "agent2api" {
        let mut cached = store
            .snapshots
            .iter()
            .find(|s| s.provider == provider)
            .cloned()
            .unwrap_or_else(|| Snapshot::empty(&provider, "idle", "等待查询"));
        cached.status = "stale".into();
        cached.message = "令牌已更新，暂显示上次数据".into();
        cached
    } else if provider == "antigravity" {
        antigravity::retain_view(
            store.snapshots.iter().find(|s| s.provider == provider),
            &store.settings,
        )
    } else {
        Snapshot::empty(&provider, "idle", "凭据已更新，等待查询")
    };
    replace(&mut store, next);
    persist(&state, &store)?;
    state
        .schedule
        .lock()
        .map_err(|_| "刷新设置失败")?
        .remove(&provider);
    Ok(())
}

#[tauri::command]
fn remove_credentials(provider: String, state: tauri::State<AppState>) -> Result<(), String> {
    if !adapters::Adapter::parse(&provider).is_some_and(|a| a.needs_credential()) {
        return Err("无效平台".into());
    }
    let busy = state.busy.lock().map_err(|_| "查询状态暂不可用")?;
    if busy.contains(&provider) {
        return Err("该平台正在刷新，请完成后移除凭据".into());
    }
    let entry =
        keyring::Entry::new("TokenPlusPlus", &provider).map_err(|_| "无法访问系统凭据存储")?;
    match entry.delete_credential() {
        Ok(_) | Err(keyring::Error::NoEntry) => {}
        Err(_) => return Err("移除凭据失败".into()),
    }
    let mut store = state.store.lock().map_err(|_| "设置暂不可用")?;
    replace(
        &mut store,
        Snapshot::empty(&provider, "unconfigured", "请先配置凭据"),
    );
    persist(&state, &store)
}

fn replace(store: &mut Store, snapshot: Snapshot) {
    if let Some(old) = store
        .snapshots
        .iter_mut()
        .find(|s| s.provider == snapshot.provider)
    {
        *old = snapshot;
    } else {
        store.snapshots.push(snapshot);
    }
}

fn failed_snapshot(
    previous: Option<&Snapshot>,
    provider: &str,
    configured: bool,
    message: String,
) -> Snapshot {
    let mut snapshot = previous
        .cloned()
        .unwrap_or_else(|| Snapshot::empty(provider, "error", ""));
    snapshot.status = if snapshot.updated_at.is_some() {
        "stale"
    } else if !configured {
        "unconfigured"
    } else {
        "error"
    }
    .into();
    snapshot.message = message;
    snapshot
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    #[test]
    fn failed_query_keeps_last_success_and_does_not_invent_zero() {
        let mut previous = Snapshot::success("codex");
        previous.metrics.push(model::Metric {
            label: "5小时".into(),
            value: "44".into(),
            unit: "%".into(),
            remaining_percent: Some(44.),
            resets_at: Some(1800000000),
        });
        let failed = failed_snapshot(Some(&previous), "codex", true, "Codex 查询已退出".into());
        assert_eq!(failed.status, "stale");
        assert_eq!(failed.updated_at, previous.updated_at);
        assert_eq!(
            failed_snapshot(Some(&previous), "codex", false, "凭据需要重新配置".into()).status,
            "stale"
        );
        assert_eq!(
            serde_json::to_value(failed.metrics).unwrap(),
            serde_json::to_value(previous.metrics).unwrap()
        );
        let empty = failed_snapshot(None, "codex", true, "Codex 查询已退出".into());
        assert_eq!(empty.status, "error");
        assert!(empty.metrics.is_empty());
    }
}

async fn refresh_one(
    app: tauri::AppHandle,
    provider: String,
    force: bool,
) -> Result<Snapshot, String> {
    let adapter = adapters::Adapter::parse(&provider).ok_or("该平台自动读取尚未接入")?;
    let cooldown = providers::cooldown_seconds(&provider);
    if cooldown > 0 {
        return Err(format!("平台限流，请在 {cooldown} 秒后重试"));
    }
    let state = app.state::<AppState>();
    {
        let mut busy = state.busy.lock().map_err(|_| "刷新状态暂不可用")?;
        if !busy.insert(provider.clone()) {
            return Err("该平台正在刷新，请稍候".into());
        }
    }
    let _guard = RefreshGuard(&state.busy, provider.clone());
    let _ = app.emit("refresh-started", &provider);
    let settings = state
        .store
        .lock()
        .map_err(|_| "设置暂不可用")?
        .settings
        .clone();
    let configured = !adapter.needs_credential()
        || if adapter == adapters::Adapter::Antigravity {
            antigravity::credential(&settings).is_ok()
        } else if adapter == adapters::Adapter::Agent2Api {
            agent2api::credential(&settings).is_ok()
        } else {
            providers::credential(&provider).is_ok()
        };
    let result = if !configured {
        Err("请先在设置中配置凭据".into())
    } else {
        let client = state.client.clone();
        let query_settings = settings.clone();
        tokio::spawn(async move { adapter.query(&client, &query_settings, force).await })
            .await
            .unwrap_or_else(|_| Err("查询进程异常，请稍后重试".into()))
    };
    let succeeded = result.as_ref().is_ok_and(|s| s.status == "ready");
    let snapshot = {
        let mut store = state.store.lock().map_err(|_| "设置暂不可用")?;
        let mut snapshot = match result {
            Ok(s) => s,
            Err(message) => failed_snapshot(
                store.snapshots.iter().find(|s| s.provider == provider),
                &provider,
                configured,
                message,
            ),
        };
        if provider == "agent2api" {
            snapshot = agent2api::retain(
                snapshot,
                store.snapshots.iter().find(|s| s.provider == provider),
            );
        }
        replace(&mut store, snapshot.clone());
        if persist(&state, &store).is_err() {
            snapshot.message = format!("{} · 本地缓存保存失败", snapshot.message);
            replace(&mut store, snapshot.clone());
        }
        snapshot
    };
    {
        let mut schedule = state.schedule.lock().map_err(|_| "刷新计划暂不可用")?;
        let failures = if succeeded || !configured {
            0
        } else {
            schedule
                .get(&provider)
                .map_or(1, |(_, f)| f.saturating_add(1))
        };
        let interval = adapter.interval_minutes(&settings);
        let seconds = if failures == 0 {
            interval * 60
        } else {
            (60u64.saturating_mul(1u64 << failures.min(5)))
                .max(interval * 60)
                .min(3600)
        };
        let wait = Duration::from_secs(seconds.max(providers::cooldown_seconds(&provider)));
        schedule.insert(
            provider.clone(),
            (
                Instant::now()
                    .checked_add(wait)
                    .unwrap_or_else(|| Instant::now() + Duration::from_secs(86400 * 30)),
                failures,
            ),
        );
    }
    state
        .busy
        .lock()
        .map_err(|_| "刷新状态暂不可用")?
        .remove(&provider);
    let _ = app.emit("snapshot-updated", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
async fn refresh_provider(app: tauri::AppHandle, provider: String) -> Result<Snapshot, String> {
    refresh_one(app, provider, true).await
}

#[tauri::command]
async fn list_antigravity_accounts(
    app: tauri::AppHandle,
) -> Result<Vec<antigravity::AccountChoice>, String> {
    let settings = app
        .state::<AppState>()
        .store
        .lock()
        .map_err(|_| "设置暂不可用")?
        .settings
        .clone();
    antigravity::accounts(&settings).await
}

#[tauri::command]
async fn open_agent2api_details(app: tauri::AppHandle, account: String) -> Result<(), String> {
    // WebView2 creation must run outside a synchronous Windows UI command.
    {
        let state = app.state::<AppState>();
        let store = state.store.lock().map_err(|_| "设置暂不可用")?;
        if !store
            .snapshots
            .iter()
            .find(|s| s.provider == "agent2api")
            .and_then(|s| s.gateway.as_ref())
            .is_some_and(|g| g.accounts.iter().any(|a| a.id == account))
        {
            return Err("账号不可用，请先刷新目录".into());
        }
    }
    if let Some(window) = app.get_webview_window("agent2api-details") {
        let _ = window.emit("agent-details-account", &account);
        window.show().map_err(|_| "无法显示详情")?;
        let _ = window.set_focus();
        return Ok(());
    }
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("view", "agent-details")
        .append_pair("account", &account)
        .finish();
    tauri::WebviewWindowBuilder::new(
        &app,
        "agent2api-details",
        tauri::WebviewUrl::App(format!("index.html?{query}").into()),
    )
    .title("Token++ · 额度详情")
    .inner_size(620., 740.)
    .min_inner_size(420., 360.)
    .decorations(true)
    .resizable(true)
    .build()
    .map_err(|_| "无法打开详情窗口")?;
    Ok(())
}

#[tauri::command]
fn open_platform(provider: String) -> Result<(), String> {
    let url = adapters::Adapter::parse(&provider).ok_or("无效平台")?.url();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .arg(url)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|_| "无法打开浏览器")?;
    }
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = set_display_mode(app.clone(), false);
        }))
        .invoke_handler(tauri::generate_handler![
            get_state,
            autostart_enabled,
            set_autostart,
            save_settings,
            save_credentials,
            remove_credentials,
            refresh_provider,
            list_antigravity_accounts,
            open_platform,
            open_agent2api_details,
            set_display_mode,
            desktop_menu,
            remember_widget_position,
            reset_widget_position,
            desktop::resize_widget,
            desktop::desktop_frame,
            desktop::desktop_live_status,
            desktop::desktop_probe_pixels,
            desktop::desktop_drag,
            desktop::desktop_placement,
            desktop::desktop_background
        ])
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            if id == "widget-full" {
                let _ = set_display_mode(app.clone(), false);
            } else if id == "widget-hide" {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            } else if id.starts_with("widget-") {
                let _ = app.emit("widget-action", id);
            }
        })
        .setup(|app| {
            let path = app.path().app_data_dir()?.join("state.json");
            let mut store: Store = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Store>(&b).ok())
                .filter(|s| (1..=2).contains(&s.schema_version))
                .unwrap_or_default();
            store.schema_version = 2;
            store.settings.agent2api_selection.version = 1;
            if !store.snapshots.iter().any(|s| s.provider == "agent2api") {
                store.snapshots.push(Snapshot::empty(
                    "agent2api",
                    "unconfigured",
                    "连接 Agent2API 后查看额度",
                ));
            }
            store.settings.balance_minutes = store.settings.balance_minutes.clamp(1, 120);
            store.settings.quota_minutes = store.settings.quota_minutes.clamp(1, 120);
            store.settings.font_scale = store.settings.font_scale.clamp(90, 140);
            store.settings.opacity = store.settings.opacity.clamp(30, 95);
            if std::env::args().any(|a| a == "--panel") {
                store.settings.desktop_mode = false;
            }
            if std::env::args().any(|a| a == "--desktop") {
                store.settings.desktop_mode = true;
            }
            let legacy_view = antigravity::view(&store.settings);
            for s in &mut store.snapshots {
                if s.provider == "antigravity" && !s.metrics.is_empty() && s.view.is_none() {
                    s.view = Some(legacy_view.clone());
                }
                if s.updated_at.is_some() {
                    s.status = "stale".into();
                    s.message = "上次保存的数据，等待刷新确认".into();
                }
            }
            antigravity::migrate_credential(&store.settings);
            let initial_settings = store.settings.clone();
            app.manage(desktop::DesktopState::default());
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()?;
            app.manage(AppState {
                store: Mutex::new(store),
                busy: Mutex::new(HashSet::new()),
                schedule: Mutex::new(HashMap::new()),
                path,
                client,
            });
            if let Some(w) = app.get_webview_window("main") {
                let mut old = initial_settings.clone();
                old.desktop_mode = !initial_settings.desktop_mode;
                if let Err(message) = desktop::apply(&w, &old, &initial_settings) {
                    let state = app.state::<AppState>();
                    let mut store = state.store.lock().unwrap();
                    store.settings.desktop_mode = false;
                    let _ = desktop::apply(&w, &initial_settings, &store.settings);
                    let _ = persist(&state, &store);
                    let _ = app.emit("desktop-error", message);
                }
                if std::env::args().any(|a| a == "--autostart") && !initial_settings.desktop_mode {
                    let _ = w.hide();
                }
            }
            use tauri::{
                menu::{Menu, MenuItem},
                tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
            };
            let show = MenuItem::with_id(app, "show", "打开 Token++", true, None::<&str>)?;
            let widget = MenuItem::with_id(app, "widget", "显示桌面组件", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &widget, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("Token++ · 额度一览")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        save_geometry(app);
                        app.exit(0)
                    }
                    "show" => {
                        let _ = set_display_mode(app.clone(), false);
                    }
                    "widget" => {
                        if let Err(e) = set_display_mode(app.clone(), true) {
                            let _ = app.emit("desktop-error", e);
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        let _ = set_display_mode(tray.app_handle().clone(), false);
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(2)).await;
                loop {
                    // Rebind after Explorer has recreated its desktop host. Never reopen a deliberately hidden widget.
                    if let Some(w) = handle.get_webview_window("main") {
                        let enabled = handle
                            .state::<AppState>()
                            .store
                            .lock()
                            .map(|s| s.settings.desktop_mode)
                            .unwrap_or(false);
                        if enabled && w.is_visible().unwrap_or(false) && !desktop::is_attached(&w) {
                            let h = handle.clone();
                            let _ = handle.run_on_main_thread(move || {
                                if let Err(e) = set_display_mode(h.clone(), true) {
                                    let _ = set_display_mode(h.clone(), false);
                                    let _ = h.emit("desktop-error", e);
                                }
                            });
                        }
                    }
                    for adapter in adapters::ALL {
                        let provider = adapter.id();
                        let due = handle
                            .state::<AppState>()
                            .schedule
                            .lock()
                            .map(|s| {
                                s.get(provider)
                                    .is_none_or(|(time, _)| Instant::now() >= *time)
                            })
                            .unwrap_or(false);
                        if due {
                            let h = handle.clone();
                            let p = provider.to_string();
                            tauri::async_runtime::spawn(async move {
                                let _ = refresh_one(h, p, false).await;
                            });
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if matches!(event, tauri::WindowEvent::ScaleFactorChanged { .. }) {
                // Let Windows finish applying its suggested DPI rectangle before updating clipping.
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    let target = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        if let Some(w) = target.get_webview_window("main") {
                            if desktop::is_attached(&w) {
                                if let Err(message) = desktop::sync_region(&w) {
                                    let _ = target.emit("desktop-error", message);
                                }
                            }
                        }
                    });
                });
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                save_geometry(window.app_handle());
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("启动 Token++ 失败");
}
