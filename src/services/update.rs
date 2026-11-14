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

    Ok(Some(Release {
        version,
        notes: json["body"].as_str().unwrap_or_default().trim().to_string(),
        setup_url,
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
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> anyhow::Result<PathBuf> {
    let path = setup_path(&release.version);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // 先写 `.part` 再改名：中途失败留下的半个文件不会被 `cached_setup` 当成可用安装包。
    let temp = path.with_extension("exe.part");

    let response = agent().get(&release.setup_url).call()?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok());
    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(&temp)?;

    let mut buffer = vec![0u8; 64 * 1024];
    let mut received = 0u64;
    on_progress(0, total);
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        received += read as u64;
        on_progress(received, total);
    }
    file.flush()?;
    drop(file);

    if received < MIN_SETUP_SIZE {
        let _ = std::fs::remove_file(&temp);
        return Err(anyhow::anyhow!("下载的安装包不完整"));
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
