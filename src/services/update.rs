//! 自动更新：读 GitHub Release、下载安装包、交给 NSIS 安装器覆盖。
//!
//! 更新的是**安装包**而不是运行中的 exe：SAP Auto Logon 是 per-user 安装，
//! 安装目录、开始菜单快捷方式和卸载记录都归安装器管，绕过它直接换文件，
//! 卸载项与文件就会对不上。所以这里只负责"找到、下载、交给安装器"。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::core::paths;

const REPO: &str = "revolveyao/sap-auto-logon";
const USER_AGENT: &str = "sap-auto-logon";
/// 小于这个尺寸的一定不是完整的安装包。
const MIN_SETUP_SIZE: u64 = 1024 * 1024;

/// 发现的新版本。
#[derive(Clone)]
pub struct Release {
    pub version: String,
    pub notes: String,
    setup_url: String,
    /// GitHub 为安装包登记的 SHA-256（Release API 的 digest 字段）。
    /// 走加速代理时靠它证明下载到的包没被替换；发布里没有这个字段时为 None。
    pub sha256: Option<String>,
}

/// 查最新 Release；没有更新的版本时返回 `None`。
pub fn check() -> anyhow::Result<Option<Release>> {
    let text = get(&format!(
        "https://api.github.com/repos/{REPO}/releases/latest"
    ))?;
    let json: serde_json::Value = serde_json::from_str(&text)?;

    let tag = json["tag_name"].as_str().unwrap_or_default();
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    if version.is_empty() || !is_newer(&version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }

    let asset = json["assets"]
        .as_array()
        .and_then(|assets| {
            assets.iter().find(|asset| {
                asset["name"]
                    .as_str()
                    .is_some_and(|name| name.ends_with("-setup.exe"))
            })
        })
        .ok_or_else(|| anyhow::anyhow!("发布里没有安装包"))?;
    let setup_url = asset["browser_download_url"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if setup_url.is_empty() {
        return Err(anyhow::anyhow!("发布里没有安装包地址"));
    }
    let sha256 = asset["digest"]
        .as_str()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| value.len() == 64);

    Ok(Some(Release {
        version,
        notes: json["body"].as_str().unwrap_or_default().trim().to_string(),
        setup_url,
        sha256,
    }))
}

fn updates_dir() -> PathBuf {
    paths::local_dir().join("updates")
}

/// 安装包落点。多个版本各占一份，升级到同版本时直接复用。
pub fn setup_path(version: &str) -> PathBuf {
    updates_dir().join(format!("SAP-Auto-Logon-{version}-setup.exe"))
}

/// 已经下好的安装包。有就不用再下一次——用户可能上次取消过安装。
pub fn cached_setup(release: &Release) -> Option<PathBuf> {
    let path = setup_path(&release.version);
    let size = path.metadata().ok().map(|meta| meta.len()).unwrap_or(0);
    (size >= MIN_SETUP_SIZE).then_some(path)
}

/// 清掉已经不可能再安装的安装包，返回删除个数。
///
/// 版本号不高于当前运行版本的包永远装不上了：升级只会往前走，而 `cached_setup`
/// 只认正在检查的那个版本。留着每个都是几兆到几十兆。比当前版本**新**的包可能是
/// 上次「取消安装」留下的，必须保留——下次检查更新能直接复用，不必重下。
///
/// 顺带清掉下载中断留下的 `.part`：那些文件永远不会被改名成正式包。
pub fn prune_stale_setups(current: &str) -> std::io::Result<usize> {
    let Ok(entries) = std::fs::read_dir(updates_dir()) else {
        // 目录还不存在，说明从没下载过安装包。
        return Ok(0);
    };

    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let stale = if name.starts_with("SAP-Auto-Logon-") && name.ends_with("-setup.exe.part") {
            true
        } else {
            // 只认自己命名的正式包，用户放进这个目录的别的东西不动。
            name.strip_prefix("SAP-Auto-Logon-")
                .and_then(|rest| rest.strip_suffix("-setup.exe"))
                .filter(|version| {
                    !version.is_empty()
                        && version.chars().all(|c| c.is_ascii_digit() || c == '.')
                })
                .is_some_and(|version| !is_newer(version, current))
        };
        if stale && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// 下载安装包，走系统代理（如果系统启用了）。
///
/// `on_progress` 在下载线程上被调用：(已收字节, 总字节)。总长未知时第二项为 `None`
/// ——GitHub 的下载接口一般带 `Content-Length`，但重定向后不一定。
pub fn download(
    release: &Release,
    route: &str,
    proxy: &str,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> anyhow::Result<PathBuf> {
    let path = setup_path(&release.version);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // 先写 `.part` 再改名：中途失败留下的半个文件不会被 `cached_setup` 当成可用安装包。
    let temp = path.with_extension("exe.part");

    let url = effective_url(&release.setup_url, route, proxy);
    log::info!("更新：下载线路 {route}，地址 {url}");
    let response = agent().get(&url).call()?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok());
    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(&temp)?;

    let mut buffer = vec![0u8; 64 * 1024];
    let mut received = 0u64;
    // SHA-256 边下边算：走代理时用它核对 GitHub 登记的 digest。
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    on_progress(0, total);
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        received += read as u64;
        on_progress(received, total);
    }
    file.flush()?;
    drop(file);

    if received < MIN_SETUP_SIZE {
        let _ = std::fs::remove_file(&temp);
        return Err(anyhow::anyhow!("下载的安装包不完整"));
    }

    // 走加速代理时代理是第三方：用 GitHub 登记的 SHA-256 核对一遍，
    // 对不上说明包被换过（或传坏了），直接删掉不装。
    let actual = format!("{:x}", hasher.finalize());
    match release.sha256.as_deref() {
        Some(expected) if expected != actual => {
            let _ = std::fs::remove_file(&temp);
            return Err(anyhow::anyhow!(
                "安装包校验失败：期望 {expected}，实际 {actual}"
            ));
        }
        Some(_) => log::info!("更新：安装包 SHA-256 校验通过"),
        None => log::warn!("更新：发布里没有 digest，跳过校验（实际 {actual}）"),
    }

    std::fs::rename(&temp, &path)?;
    Ok(path)
}

/// 拉起安装器。调用方必须紧接着退出：安装脚本会先结束旧进程再覆盖文件。
pub fn run_installer(path: &Path) -> anyhow::Result<()> {
    // /S = 静默安装：脚本里已经 taskkill 旧进程，装完自动拉起新版本。
    std::process::Command::new(path).arg("/S").spawn()?;
    Ok(())
}

/// 所有出网请求共用一个 agent：代理、超时、UA 只在这里定一次。
fn agent() -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .user_agent(USER_AGENT)
        .timeout_connect(Duration::from_secs(15));

    // 系统开着代理时按它出网：内网解析到的 GitHub 地址通常是假的，直连必失败。
    match crate::platform::system::system_proxy() {
        Some(proxy) => match ureq::Proxy::new(&proxy) {
            Ok(parsed) => {
                log::info!("更新：走系统代理 {proxy}");
                builder = builder.proxy(parsed);
            }
            Err(error) => log::warn!("更新：系统代理不可用（{proxy}）：{error}"),
        },
        None => log::info!("更新：直连（系统未启用代理）"),
    }

    builder.build()
}

fn get(url: &str) -> anyhow::Result<String> {
    let response = agent()
        .get(url)
        .set("Accept", "application/vnd.github+json")
        .call()?;
    Ok(response.into_string()?)
}

/// 按点分数字逐段比较：`0.2.10` 比 `0.2.9` 新，字符串比较会把顺序判反。
fn is_newer(candidate: &str, current: &str) -> bool {
    fn parts(value: &str) -> Vec<u32> {
        value
            .split('.')
            .map(|part| part.trim().parse().unwrap_or(0))
            .collect()
    }
    let mut left = parts(candidate);
    let mut right = parts(current);
    let len = left.len().max(right.len());
    left.resize(len, 0);
    right.resize(len, 0);
    left > right
}

/// 加速代理预设（第三方提供：会知道本机 IP 与所下载的文件）。
/// 安装包由 Windows 签名且签名覆盖版本号，代理改不了内容、也冒充不了旧版本。
pub const PROXY_PRESETS: [&str; 3] = [
    "https://gh-proxy.com/",
    "https://gh-proxy.org/",
    "https://ghfast.top/",
];

/// 按线路把 GitHub 地址换成实际请求地址。
///
/// 代理的用法是「前缀 + 完整原地址」，例如
/// `https://gh-proxy.com/https://github.com/owner/repo/releases/download/...`。
pub fn effective_url(base: &str, route: &str, proxy: &str) -> String {
    if route == "github" || base.is_empty() {
        return base.to_string();
    }
    let mut prefix = proxy.trim();
    if prefix.is_empty() {
        prefix = PROXY_PRESETS[0];
    }
    if prefix.ends_with('/') {
        format!("{prefix}{base}")
    } else {
        format!("{prefix}/{base}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GITHUB: &str = "https://github.com/revolveyao/sap-auto-logon/releases/download/v0.2.15/x-setup.exe";

    #[test]
    fn github_route_keeps_the_original_url() {
        assert_eq!(effective_url(GITHUB, "github", PROXY_PRESETS[0]), GITHUB);
    }

    #[test]
    fn proxy_route_prefixes_the_url() {
        let url = effective_url(GITHUB, "proxy", "https://gh-proxy.com/");
        assert_eq!(url, format!("https://gh-proxy.com/{GITHUB}"));
    }

    #[test]
    fn missing_trailing_slash_is_added() {
        let url = effective_url(GITHUB, "proxy", "https://ghfast.top");
        assert_eq!(url, format!("https://ghfast.top/{GITHUB}"));
    }

    #[test]
    fn empty_proxy_falls_back_to_the_first_preset() {
        let url = effective_url(GITHUB, "proxy", "  ");
        assert_eq!(url, format!("{}{GITHUB}", PROXY_PRESETS[0]));
    }
}