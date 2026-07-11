use std::time::Duration;

pub fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(timeout);
    if let Some(url) = windows_system_proxy() {
        let proxy = reqwest::Proxy::all(&url)
            .map_err(|error| format!("Windows 系统代理配置无效：{error}"))?;
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|error| format!("无法创建网络客户端：{error}"))
}

#[cfg(windows)]
fn windows_system_proxy() -> Option<String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let internet_settings = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    let enabled = internet_settings.get_value::<u32, _>("ProxyEnable").ok()? != 0;
    if !enabled {
        return None;
    }
    let value = internet_settings
        .get_value::<String, _>("ProxyServer")
        .ok()?;
    proxy_url_for_https(&value)
}

#[cfg(not(windows))]
fn windows_system_proxy() -> Option<String> {
    None
}

fn proxy_url_for_https(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let selected = if value.contains('=') {
        let entries = value
            .split(';')
            .filter_map(|entry| entry.split_once('='))
            .map(|(scheme, address)| (scheme.trim().to_ascii_lowercase(), address.trim()))
            .collect::<Vec<_>>();
        entries
            .iter()
            .find(|(scheme, _)| scheme == "https")
            .or_else(|| entries.iter().find(|(scheme, _)| scheme == "http"))
            .map(|(_, address)| *address)?
    } else {
        value
    };
    if selected.is_empty() || selected.to_ascii_lowercase().starts_with("socks") {
        return None;
    }
    if selected.contains("://") {
        Some(selected.to_string())
    } else {
        Some(format!("http://{selected}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_windows_proxy_formats() {
        assert_eq!(
            proxy_url_for_https("127.0.0.1:7890"),
            Some("http://127.0.0.1:7890".into())
        );
        assert_eq!(
            proxy_url_for_https("http=127.0.0.1:8080;https=127.0.0.1:8443"),
            Some("http://127.0.0.1:8443".into())
        );
        assert_eq!(
            proxy_url_for_https("http=http://localhost:7890"),
            Some("http://localhost:7890".into())
        );
        assert_eq!(proxy_url_for_https("socks=127.0.0.1:1080"), None);
    }
}
