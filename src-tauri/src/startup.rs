use std::path::Path;
use windows_registry::{Key, CURRENT_USER};
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const NAME: &str = "Token++";

fn missing(code: i32) -> bool {
    code == 0x80070002u32 as i32
}

fn read(key: &str) -> Result<Option<Key>, String> {
    match CURRENT_USER.open(key) {
        Ok(key) => Ok(Some(key)),
        Err(e) if missing(e.code().0) => Ok(None),
        Err(_) => Err("无法读取 Windows 开机自启设置".into()),
    }
}
pub fn enabled() -> Result<bool, String> {
    let Some(key) = read(RUN)? else {
        return Ok(false);
    };
    match key.get_string(NAME) {
        Ok(_) => {}
        Err(e) if missing(e.code().0) => return Ok(false),
        Err(_) => return Err("无法读取开机启动项".into()),
    }
    if let Some(key) = read(APPROVED)? {
        match key.get_value(NAME) {
            Ok(bytes) if bytes.len() >= 12 => {
                return Ok(bytes.iter().rev().take(8).all(|b| *b == 0))
            }
            Ok(_) => {}
            Err(e) if missing(e.code().0) => {}
            Err(_) => return Err("无法读取 Windows 启动应用状态".into()),
        }
    }
    Ok(true)
}
fn command(path: &Path) -> Result<String, String> {
    if !path.is_absolute() {
        return Err("程序位置无效，无法设置开机自启".into());
    }
    let path = path.to_str().ok_or("程序路径无法识别")?;
    if path.contains('"') {
        return Err("程序路径无效".into());
    }
    Ok(format!("\"{path}\" --autostart"))
}
pub fn set(value: bool) -> Result<bool, String> {
    if value {
        let path = std::env::current_exe().map_err(|_| "无法读取程序安装位置")?;
        let value = command(&path)?;
        CURRENT_USER
            .create(RUN)
            .and_then(|k| k.set_string(NAME, value))
            .map_err(|_| "无法写入当前用户的开机启动项")?;
        // Clear only this application's Task Manager override when explicitly re-enabled.
        match CURRENT_USER.options().write().open(APPROVED) {
            Ok(key) => match key.remove_value(NAME) {
                Ok(()) => {}
                Err(e) if missing(e.code().0) => {}
                Err(_) => {
                    return Err(
                        "无法重置本应用的启动状态，请在任务管理器中允许 Token++ 启动".into(),
                    )
                }
            },
            Err(e) if missing(e.code().0) => {}
            Err(_) => return Err("无法更新本应用的 Windows 启动状态".into()),
        }
    } else {
        match CURRENT_USER.options().write().open(RUN) {
            Ok(key) => match key.remove_value(NAME) {
                Ok(()) => {}
                Err(e) if missing(e.code().0) => {}
                Err(_) => return Err("无法移除本应用的开机启动项".into()),
            },
            Err(e) if missing(e.code().0) => {}
            Err(_) => return Err("无法修改开机自启设置".into()),
        }
    }
    let actual = enabled()?;
    if actual != value {
        return Err("Windows 未能保存开机自启设置，请重试".into());
    }
    Ok(actual)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quotes_program_paths_with_spaces() {
        assert_eq!(
            command(Path::new(r"C:\Users\Test User\Token++\token-plusplus.exe")).unwrap(),
            r#""C:\Users\Test User\Token++\token-plusplus.exe" --autostart"#
        );
        assert!(command(Path::new("relative.exe")).is_err());
    }
}
