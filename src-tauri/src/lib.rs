use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
#[cfg(target_os = "windows")]
use windows::{
    core::{PCSTR, PCWSTR},
    Win32::{
        Foundation::{HWND, POINT},
        System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED},
        UI::{
            Shell::{
                Common::ITEMIDLIST, IContextMenu, IShellFolder, SHBindToParent, SHParseDisplayName,
                CMF_NORMAL, CMINVOKECOMMANDINFO,
            },
            WindowsAndMessaging::{
                CreatePopupMenu, DestroyMenu, GetCursorPos, SetForegroundWindow, TrackPopupMenuEx,
                SW_SHOWNORMAL, TPM_RETURNCMD, TPM_RIGHTBUTTON,
            },
        },
    },
};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DesktopApp {
    name: String,
    target: String,
    source: String,
    icon: String,
    kind: String,
}

#[derive(Clone, Debug, Deserialize)]
struct InteractiveRegion {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

struct InteractiveRegions(Arc<Mutex<Vec<InteractiveRegion>>>);

#[tauri::command]
fn set_interactive_regions(
    state: tauri::State<'_, InteractiveRegions>,
    regions: Vec<InteractiveRegion>,
) {
    if let Ok(mut current) = state.0.lock() {
        *current = regions;
    }
}

#[tauri::command]
fn scan_desktop_apps() -> Vec<DesktopApp> {
    let script = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Drawing
$shell = New-Object -ComObject WScript.Shell
$root = Join-Path $env:LOCALAPPDATA 'Qizhuo\Shortcuts'
$sets = @(
  [pscustomobject]@{ Desktop = [Environment]::GetFolderPath('Desktop'); Store = (Join-Path $root 'User') },
  [pscustomobject]@{ Desktop = [Environment]::GetFolderPath('CommonDesktopDirectory'); Store = (Join-Path $root 'Common') }
)
$items = @()
foreach ($set in $sets) {
  New-Item -ItemType Directory -Force -Path $set.Store | Out-Null
  foreach ($location in @($set.Store, $set.Desktop)) {
    Get-ChildItem -LiteralPath $location -Filter '*.lnk' -File -Force -ErrorAction SilentlyContinue | Select-Object -First 36 | ForEach-Object {
      try {
        $source = $_.FullName
        $managedPath = $source
        $shortcut = $shell.CreateShortcut($source)
        $target = $shortcut.TargetPath
        if ($target -and (Test-Path -LiteralPath $target) -and ([IO.Path]::GetExtension($target) -ieq '.exe')) {
          if ($location -eq $set.Desktop) {
            $destination = Join-Path $set.Store $_.Name
            if (Test-Path -LiteralPath $destination) { return }
            Move-Item -LiteralPath $source -Destination $destination -ErrorAction Stop
            if (-not (Test-Path -LiteralPath $destination)) { return }
            $managedPath = $destination
          }
          $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($target)
          if ($icon) {
            $stream = New-Object IO.MemoryStream
            $bitmap = $icon.ToBitmap()
            $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
            $items += [pscustomobject]@{ name = $_.BaseName; target = $target; source = $managedPath; icon = ('data:image/png;base64,' + [Convert]::ToBase64String($stream.ToArray())); kind = 'app' }
            $bitmap.Dispose(); $icon.Dispose(); $stream.Dispose()
          }
        }
      } catch {}
    }
  }
  if ($set.Store.EndsWith('User')) {
    foreach ($location in @($set.Store, $set.Desktop)) {
      Get-ChildItem -LiteralPath $location -Force -ErrorAction SilentlyContinue | Where-Object { $_.Extension -ne '.lnk' -and $_.Name -ne 'desktop.ini' -and -not ($_.Attributes -band [IO.FileAttributes]::System) } | Select-Object -First 80 | ForEach-Object {
        try {
          $name = $_.Name
          $source = $_.FullName
          $isFolder = $_.PSIsContainer
          $managedPath = $source
          if ($location -eq $set.Desktop) {
            $destination = Join-Path $set.Store $name
            if (Test-Path -LiteralPath $destination) { return }
            Move-Item -LiteralPath $source -Destination $destination -ErrorAction Stop
            if (-not (Test-Path -LiteralPath $destination)) { return }
            $managedPath = $destination
          }
          $iconData = ''
          if (-not $isFolder) {
            $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($managedPath)
            if ($icon) {
              $stream = New-Object IO.MemoryStream
              $bitmap = $icon.ToBitmap()
              $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
              $iconData = 'data:image/png;base64,' + [Convert]::ToBase64String($stream.ToArray())
              $bitmap.Dispose(); $icon.Dispose(); $stream.Dispose()
            }
          }
          $items += [pscustomobject]@{ name = $name; target = $managedPath; source = $managedPath; icon = $iconData; kind = $(if ($isFolder) { 'folder' } else { 'file' }) }
        } catch {}
      }
    }
  }
}
@($items | Sort-Object kind,name -Unique | Select-Object -First 120) | ConvertTo-Json -Compress
"#;
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        script,
    ]);
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    let output = command.output();
    output
        .ok()
        .filter(|result| result.status.success())
        .and_then(|result| serde_json::from_slice::<Vec<DesktopApp>>(&result.stdout).ok())
        .unwrap_or_default()
}

#[tauri::command]
fn open_item(target: String) -> Result<(), String> {
    let path = Path::new(&target);
    if !path.exists() {
        return Err("项目已不存在".into());
    }
    if path.is_file()
        && path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("exe"))
            .unwrap_or(false)
    {
        Command::new(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    } else {
        Command::new("explorer.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn qizhuo_data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Qizhuo")
}

fn command_file() -> PathBuf {
    qizhuo_data_dir().join("desktop-command.txt")
}

pub fn send_command(command: &str) -> bool {
    let directory = qizhuo_data_dir();
    if fs::create_dir_all(&directory).is_err() {
        return false;
    }
    let path = command_file();
    let temporary = directory.join(format!("desktop-command-{}.tmp", std::process::id()));
    if fs::write(&temporary, command.as_bytes()).is_err() {
        return false;
    }
    let _ = fs::remove_file(&path);
    if fs::rename(&temporary, &path).is_err() {
        let _ = fs::remove_file(&temporary);
        return false;
    }
    for _ in 0..12 {
        thread::sleep(Duration::from_millis(70));
        if !path.exists() {
            return true;
        }
    }
    let _ = fs::remove_file(path);
    false
}

#[tauri::command]
fn show_system_context_menu(app: tauri::AppHandle, target: String) -> Result<(), String> {
    let path = PathBuf::from(&target);
    if !path.exists() {
        return Err("项目已不存在".into());
    }

    #[cfg(target_os = "windows")]
    {
        let hwnd = app
            .get_webview_window("main")
            .ok_or_else(|| "找不到主窗口".to_string())?
            .hwnd()
            .map_err(|error| error.to_string())?;
        let hwnd_value = hwnd.0 as isize;
        let (sender, receiver) = mpsc::sync_channel(1);
        app.run_on_main_thread(move || {
            let _ = sender.send(show_windows_shell_menu(hwnd_value, &path));
        })
        .map_err(|error| error.to_string())?;
        return receiver
            .recv()
            .map_err(|_| "系统菜单线程异常".to_string())?;
    }

    #[cfg(not(target_os = "windows"))]
    Err("当前系统不支持 Windows 右键菜单".into())
}

#[cfg(target_os = "windows")]
fn show_windows_shell_menu(hwnd_value: isize, path: &Path) -> Result<(), String> {
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        if !initialized {
            return Err("无法初始化 Windows Shell".into());
        }

        let hwnd = HWND(hwnd_value as *mut _);
        let wide_path = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut absolute_pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        let mut popup_menu = None;

        let result = (|| -> windows::core::Result<()> {
            SHParseDisplayName(
                PCWSTR(wide_path.as_ptr()),
                None,
                &mut absolute_pidl,
                0,
                None,
            )?;

            let mut child_pidl: *mut ITEMIDLIST = std::ptr::null_mut();
            let parent: IShellFolder = SHBindToParent(absolute_pidl, Some(&mut child_pidl))?;
            let context_menu: IContextMenu = parent.GetUIObjectOf(hwnd, &[child_pidl], None)?;
            let menu = CreatePopupMenu()?;
            popup_menu = Some(menu);
            context_menu
                .QueryContextMenu(menu, 0, 1, 0x7fff, CMF_NORMAL)
                .ok()?;

            let mut cursor = POINT::default();
            GetCursorPos(&mut cursor)?;
            let _ = SetForegroundWindow(hwnd);
            let selected = TrackPopupMenuEx(
                menu,
                (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
                cursor.x,
                cursor.y,
                hwnd,
                None,
            );

            if selected.0 > 0 {
                let command = CMINVOKECOMMANDINFO {
                    cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
                    hwnd,
                    lpVerb: PCSTR((selected.0 as usize - 1) as *const u8),
                    nShow: SW_SHOWNORMAL.0,
                    ..Default::default()
                };
                context_menu.InvokeCommand(&command)?;
            }
            Ok(())
        })();

        if let Some(menu) = popup_menu {
            let _ = DestroyMenu(menu);
        }
        if !absolute_pidl.is_null() {
            CoTaskMemFree(Some(absolute_pidl as _));
        }
        CoUninitialize();
        result.map_err(|error| error.to_string())
    }
}

fn managed_store_is_empty() -> bool {
    ["User", "Common"].iter().all(|folder| {
        let path = qizhuo_data_dir().join("Shortcuts").join(folder);
        fs::read_dir(path)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true)
    })
}

#[cfg(target_os = "windows")]
fn restore_common_shortcuts_elevated() -> bool {
    let directory = qizhuo_data_dir();
    if fs::create_dir_all(&directory).is_err() {
        return false;
    }
    let helper = directory.join("restore-common-desktop.ps1");
    let script = r#"
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:LOCALAPPDATA 'Qizhuo\Shortcuts'
$store = Join-Path $root 'Common'
$desktop = [Environment]::GetFolderPath('CommonDesktopDirectory')
$resolvedRoot = [IO.Path]::GetFullPath($root)
$resolvedStore = [IO.Path]::GetFullPath($store)
$resolvedDesktop = [IO.Path]::GetFullPath($desktop)
if (-not $resolvedStore.StartsWith($resolvedRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unexpected managed store' }
if ($resolvedDesktop -ne [IO.Path]::GetFullPath([Environment]::GetFolderPath('CommonDesktopDirectory'))) { throw 'Unexpected desktop' }
Get-ChildItem -LiteralPath $resolvedStore -Force -ErrorAction SilentlyContinue | ForEach-Object {
  $destination = Join-Path $resolvedDesktop $_.Name
  if (-not (Test-Path -LiteralPath $destination)) {
    Move-Item -LiteralPath $_.FullName -Destination $destination -ErrorAction Stop
  }
}
"#;
    if fs::write(&helper, script).is_err() {
        return false;
    }

    let helper_argument = format!(
        "-NoProfile -ExecutionPolicy Bypass -File \"{}\"",
        helper.display()
    );
    let escaped_argument = helper_argument.replace('\'', "''");
    let launcher = format!(
        "$p=Start-Process -FilePath 'powershell.exe' -Verb RunAs -WindowStyle Hidden -ArgumentList '{}' -Wait -PassThru; exit $p.ExitCode",
        escaped_argument
    );
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &launcher,
    ]);
    command.creation_flags(0x08000000);
    let succeeded = command
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    let _ = fs::remove_file(helper);
    succeeded
}

#[cfg(not(target_os = "windows"))]
fn restore_common_shortcuts_elevated() -> bool {
    false
}

fn restore_managed_shortcuts() -> bool {
    let script = r#"
$root = Join-Path $env:LOCALAPPDATA 'Qizhuo\Shortcuts'
$sets = @(
  [pscustomobject]@{ Desktop = [Environment]::GetFolderPath('Desktop'); Store = (Join-Path $root 'User') },
  [pscustomobject]@{ Desktop = [Environment]::GetFolderPath('CommonDesktopDirectory'); Store = (Join-Path $root 'Common') }
)
foreach ($set in $sets) {
  Get-ChildItem -LiteralPath $set.Store -Force -ErrorAction SilentlyContinue | ForEach-Object {
    $destination = Join-Path $set.Desktop $_.Name
    if (-not (Test-Path -LiteralPath $destination)) { Move-Item -LiteralPath $_.FullName -Destination $destination -ErrorAction SilentlyContinue }
  }
}
"#;
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        script,
    ]);
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    let _ = command.status();
    if !managed_store_is_empty() {
        let _ = restore_common_shortcuts_elevated();
    }
    managed_store_is_empty()
}

#[cfg(target_os = "windows")]
fn start_restore_watchdog() {
    let pid = std::process::id().to_string();
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$ownerPid = [int]$args[0]
Wait-Process -Id $ownerPid -ErrorAction SilentlyContinue
$root = Join-Path $env:LOCALAPPDATA 'Qizhuo\Shortcuts'
$sets = @(
  [pscustomobject]@{ Desktop = [Environment]::GetFolderPath('Desktop'); Store = (Join-Path $root 'User') },
  [pscustomobject]@{ Desktop = [Environment]::GetFolderPath('CommonDesktopDirectory'); Store = (Join-Path $root 'Common') }
)
foreach ($set in $sets) {
  Get-ChildItem -LiteralPath $set.Store -Force -ErrorAction SilentlyContinue | ForEach-Object {
    $destination = Join-Path $set.Desktop $_.Name
    if (-not (Test-Path -LiteralPath $destination)) {
      Move-Item -LiteralPath $_.FullName -Destination $destination -ErrorAction SilentlyContinue
    }
  }
}
"#;
    let directory = qizhuo_data_dir();
    if fs::create_dir_all(&directory).is_err() {
        return;
    }
    let helper = directory.join("restore-watchdog.ps1");
    if fs::write(&helper, script).is_err() {
        return;
    }
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-WindowStyle",
        "Hidden",
        "-File",
        &helper.to_string_lossy(),
        &pid,
    ]);
    command.creation_flags(0x08000000);
    let _ = command.spawn();
}

#[cfg(not(target_os = "windows"))]
fn start_restore_watchdog() {}

fn start_command_listener(app: tauri::AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(180));
        let path = command_file();
        let Ok(command) = fs::read_to_string(&path) else {
            continue;
        };
        let _ = fs::remove_file(&path);
        match command.trim() {
            "show" => show_main_window(&app),
            "organize" => {
                show_main_window(&app);
                let _ = app.emit("organize-now", ());
            }
            "new-category" => {
                show_main_window(&app);
                let _ = app.emit("add-category-request", ());
            }
            "quit" => {
                if restore_managed_shortcuts() {
                    app.exit(0);
                    break;
                } else {
                    show_main_window(&app);
                    let _ = app.emit("restore-failed", ());
                }
            }
            _ => {}
        }
    });
}

#[cfg(target_os = "windows")]
fn start_interaction_watcher(app: tauri::AppHandle, regions: Arc<Mutex<Vec<InteractiveRegion>>>) {
    thread::spawn(move || {
        let mut ignored = false;
        let mut candidate = false;
        let mut stable_samples = 0_u8;
        loop {
            thread::sleep(Duration::from_millis(32));
            let Some(window) = app.get_webview_window("main") else {
                break;
            };
            let Ok(position) = window.outer_position() else {
                continue;
            };
            let Ok(scale) = window.scale_factor() else {
                continue;
            };
            let mut cursor = POINT::default();
            if unsafe { GetCursorPos(&mut cursor) }.is_err() {
                continue;
            }
            let x = (cursor.x - position.x) as f64 / scale;
            let y = (cursor.y - position.y) as f64 / scale;
            let inside = regions
                .lock()
                .map(|items| {
                    items.iter().any(|region| {
                        const PADDING: f64 = 10.0;
                        x >= region.left - PADDING
                            && x <= region.left + region.width + PADDING
                            && y >= region.top - PADDING
                            && y <= region.top + region.height + PADDING
                    })
                })
                .unwrap_or(false);
            let next_ignored = !inside;
            if next_ignored == candidate {
                stable_samples = stable_samples.saturating_add(1);
            } else {
                candidate = next_ignored;
                stable_samples = 1;
            }
            if stable_samples >= 3 && candidate != ignored {
                let _ = window.set_ignore_cursor_events(candidate);
                ignored = candidate;
            }
        }
    });
}

#[cfg(not(target_os = "windows"))]
fn start_interaction_watcher(_app: tauri::AppHandle, _regions: Arc<Mutex<Vec<InteractiveRegion>>>) {
}

#[cfg(target_os = "windows")]
fn add_registry_value(key: &str, name: Option<&str>, value: &str) {
    let mut command = Command::new("reg.exe");
    command.args(["add", key]);
    if let Some(name) = name {
        command.args(["/v", name]);
    } else {
        command.arg("/ve");
    }
    command.args(["/t", "REG_SZ", "/d", value, "/f"]);
    command.creation_flags(0x08000000);
    let _ = command.status();
}

#[cfg(target_os = "windows")]
fn register_desktop_context_menu() {
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    let executable = executable.to_string_lossy();
    let icon = format!("{},0", executable);
    let root = r"HKCU\Software\Classes\DesktopBackground\Shell\Qizhuo";
    add_registry_value(root, Some("MUIVerb"), "栖桌");
    add_registry_value(root, Some("Icon"), &icon);
    add_registry_value(root, Some("Position"), "Top");
    add_registry_value(root, Some("SubCommands"), "");

    for (order, label, action) in [
        ("01Show", "显示栖桌", "show"),
        ("02Organize", "立即整理桌面", "organize"),
        ("03NewCategory", "新增分类", "new-category"),
        ("04Quit", "退出栖桌", "quit"),
    ] {
        let item_key = format!(r"{}\shell\{}", root, order);
        let command_key = format!(r"{}\command", item_key);
        let command_line = format!(r#""{}" --command {}"#, executable, action);
        add_registry_value(&item_key, Some("MUIVerb"), label);
        add_registry_value(&item_key, Some("Icon"), &icon);
        add_registry_value(&command_key, None, &command_line);
    }
}

#[cfg(not(target_os = "windows"))]
fn register_desktop_context_menu() {}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan_desktop_apps,
            open_item,
            show_system_context_menu,
            set_interactive_regions
        ])
        .setup(|app| {
            let interactive_regions = Arc::new(Mutex::new(Vec::new()));
            app.manage(InteractiveRegions(interactive_regions.clone()));
            start_interaction_watcher(app.handle().clone(), interactive_regions);
            start_restore_watchdog();
            register_desktop_context_menu();
            start_command_listener(app.handle().clone());
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = window.current_monitor() {
                    let monitor_position = monitor.position();
                    let monitor_size = monitor.size();
                    let _ = window.set_size(tauri::PhysicalSize::new(
                        monitor_size.width,
                        monitor_size.height,
                    ));
                    let _ = window.set_position(tauri::PhysicalPosition::new(
                        monitor_position.x,
                        monitor_position.y,
                    ));
                }
            }
            let show = MenuItem::with_id(app, "show", "显示栖桌", true, None::<&str>)?;
            let organize = MenuItem::with_id(app, "organize", "立即整理", true, None::<&str>)?;
            let pause =
                CheckMenuItem::with_id(app, "pause", "暂停自动整理", true, false, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &organize, &pause, &quit])?;

            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().expect("application icon").clone())
                .tooltip("栖桌 · 桌面整理")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "organize" => {
                        show_main_window(app);
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("organize-now", ());
                        }
                    }
                    "quit" => {
                        if restore_managed_shortcuts() {
                            app.exit(0);
                        } else {
                            show_main_window(app);
                            let _ = app.emit("restore-failed", ());
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building 栖桌")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                let _ = restore_managed_shortcuts();
            }
        });
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

