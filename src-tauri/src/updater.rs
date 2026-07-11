use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Command, time::Duration};

const RELEASE_API: &str =
    "https://api.github.com/repos/Afterlife-lh/codex-quota-bar/releases/latest";
const TRUSTED_DOWNLOAD_PREFIX: &str =
    "https://github.com/Afterlife-lh/codex-quota-bar/releases/download/";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub state: String,
    pub current_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl UpdateStatus {
    pub fn idle(version: impl Into<String>) -> Self {
        Self {
            state: "idle".into(),
            current_version: version.into(),
            available_version: None,
            message: None,
        }
    }

    pub fn with_state(
        version: impl Into<String>,
        state: &str,
        available_version: Option<String>,
        message: Option<String>,
    ) -> Self {
        Self {
            state: state.into(),
            current_version: version.into(),
            available_version,
            message,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpdateRelease {
    pub version: String,
    pub notes: Option<String>,
    pub download_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    body: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

pub async fn check(current_version: &str) -> Result<Option<UpdateRelease>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(RELEASE_API)
        .header("User-Agent", "codex-quota-bar")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| format!("无法连接 GitHub：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("GitHub 返回 HTTP {}", response.status().as_u16()));
    }
    let release: GithubRelease = response
        .json()
        .await
        .map_err(|error| format!("无法解析版本信息：{error}"))?;
    let version = release.tag_name.trim_start_matches(['v', 'V']).to_string();
    if !is_newer_version(current_version, &version) {
        return Ok(None);
    }
    let asset = release
        .assets
        .iter()
        .find(|asset| {
            let name = asset.name.to_ascii_lowercase();
            name.ends_with(".msi") && name.contains("x64") && name.contains("zh-cn")
        })
        .or_else(|| {
            release.assets.iter().find(|asset| {
                let name = asset.name.to_ascii_lowercase();
                name.ends_with(".msi") && name.contains("x64")
            })
        })
        .cloned()
        .ok_or_else(|| "新版本没有可用的 x64 MSI 安装包".to_string())?;
    if !asset
        .browser_download_url
        .starts_with(TRUSTED_DOWNLOAD_PREFIX)
    {
        return Err("更新下载地址不属于受信任仓库".into());
    }
    Ok(Some(UpdateRelease {
        version,
        notes: release.body,
        download_url: asset.browser_download_url,
    }))
}

pub async fn download(release: &UpdateRelease) -> Result<PathBuf, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(&release.download_url)
        .header("User-Agent", "codex-quota-bar")
        .send()
        .await
        .map_err(|error| format!("更新下载失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("更新下载返回 HTTP {}", response.status().as_u16()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("无法读取更新包：{error}"))?;
    let path =
        std::env::temp_dir().join(format!("Codex-Quota-Bar-{}-x64-zh-CN.msi", release.version));
    std::fs::write(&path, bytes).map_err(|error| format!("无法保存更新包：{error}"))?;
    Ok(path)
}

#[cfg(windows)]
pub fn launch_installer(path: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let msi = powershell_literal(&windows_command_path(path));
    let executable = powershell_literal(&windows_command_path(&current_exe));
    let script = format!(
        "$msi='{msi}';$exe='{executable}';Start-Sleep -Seconds 2;& msiexec.exe /i $msi /passive /norestart;$code=$LASTEXITCODE;if($code -eq 0 -or $code -eq 3010){{Remove-Item -LiteralPath $msi -Force -ErrorAction SilentlyContinue}};Start-Process -FilePath $exe"
    );
    Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(|error| format!("无法启动更新安装程序：{error}"))?;
    Ok(())
}

fn windows_command_path(path: &std::path::Path) -> String {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = value.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        value.into_owned()
    }
}

fn powershell_literal(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(not(windows))]
pub fn launch_installer(_: &std::path::Path) -> Result<(), String> {
    Err("自动安装仅支持 Windows".into())
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .trim_start_matches(['v', 'V'])
        .split('.')
        .map(|part| {
            part.split(|character: char| !character.is_ascii_digit())
                .next()
                .and_then(|number| number.parse().ok())
                .unwrap_or(0)
        })
        .collect()
}

pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    let mut current = version_parts(current);
    let mut candidate = version_parts(candidate);
    let length = current.len().max(candidate.len());
    current.resize(length, 0);
    candidate.resize(length, 0);
    candidate > current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_versions_numerically() {
        assert!(is_newer_version("0.4.0", "v0.4.1"));
        assert!(is_newer_version("0.9.9", "1.0.0"));
        assert!(!is_newer_version("0.4.1", "0.4.1"));
        assert!(!is_newer_version("0.5.0", "0.4.9"));
    }

    #[test]
    fn normalizes_extended_windows_paths_for_restart() {
        assert_eq!(
            windows_command_path(std::path::Path::new(r"\\?\C:\Program Files\Codex\app.exe")),
            r"C:\Program Files\Codex\app.exe"
        );
        assert_eq!(
            powershell_literal("C:\\It's\\app.exe"),
            "C:\\It''s\\app.exe"
        );
    }
}
