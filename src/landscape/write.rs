//! 写回 Landscape：只动快捷方式（`Reference`）。
//!
//! 写回是「整份重建 + 原子替换」：解析成可编辑节点树，改动后按 SAP 的紧凑
//! 格式序列化。SAP GUI 自己也是全量重写这个文件，所以不需要保留原始缩进。

use std::path::{Path, PathBuf};

use roxmltree::Node;

use crate::landscape::model::Service;

#[derive(Debug, Clone)]
pub struct XmlNode {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<XmlNode>,
    pub text: Option<String>,
}

impl XmlNode {
    fn parse(text: &str) -> anyhow::Result<Self> {
        let document = roxmltree::Document::parse(text)?;
        Self::from_node(document.root_element(), text)
    }

    /// `source` 是原始文档：属性名要按源码切片取，见下面「限定名」的说明。
    fn from_node(node: Node<'_, '_>, source: &str) -> anyhow::Result<Self> {
        let mut children = Vec::new();
        let mut text = None;
        for child in node.children() {
            if child.is_element() {
                children.push(Self::from_node(child, source)?);
            } else if child.is_text() {
                let value = child.text().unwrap_or_default().trim().to_string();
                if !value.is_empty() {
                    text = Some(value);
                }
            }
        }
        let mut attrs = Vec::new();
        for attr in node.attributes() {
            // 必须写回限定名（含前缀），不能用 attr.name()：它只给本地名，
            // SAP GUI 写的 `xml:space="preserve"` 会塌成 `space="preserve"`，
            // 与该元素上 SAP 自己的 `space="preserve"` 撞成重复属性，
            // 整份 Landscape 变成非法 XML —— SAP GUI 与工具自己都再也读不了。
            let range = attr.range_qname();
            let Some(name) = source.get(range.clone()).filter(|name| !name.is_empty()) else {
                return Err(anyhow::anyhow!(
                    "属性 {} 的限定名无法从源码还原（range={range:?}，源码 {} 字节）",
                    attr.name(),
                    source.len()
                ));
            };
            attrs.push((name.to_string(), attr.value().to_string()));
        }
        Ok(Self {
            tag: node.tag_name().name().to_string(),
            attrs,
            children,
            text,
        })
    }

    fn serialize(&self, out: &mut String, depth: usize) {
        if depth > 0 {
            out.push_str("\r\n");
            for _ in 0..depth {
                out.push('\t');
            }
        }
        out.push('<');
        out.push_str(&self.tag);
        for (name, value) in &self.attrs {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            push_escaped(out, value);
            out.push('"');
        }
        if self.children.is_empty() && self.text.is_none() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        if let Some(text) = &self.text {
            push_escaped(out, text);
        }
        for child in &self.children {
            child.serialize(out, depth + 1);
        }
        out.push_str("\r\n");
        for _ in 0..depth {
            out.push('\t');
        }
        out.push_str("</");
        out.push_str(&self.tag);
        out.push('>');
    }

    /// 整份文档：补回 XML 声明（roxmltree 解析时会丢掉它）。
    fn to_xml(&self) -> String {
        let mut out = String::from("<?xml version=\"1.0\"?>\r\n");
        self.serialize(&mut out, 0);
        out.push_str("\r\n");
        out
    }
    fn child(&self, tag: &str) -> Option<&XmlNode> {
        self.children.iter().find(|child| child.tag == tag)
    }

    fn child_mut(&mut self, tag: &str) -> Option<&mut XmlNode> {
        self.children.iter_mut().find(|child| child.tag == tag)
    }

    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn set_attr(&mut self, name: &str, value: &str) {
        if let Some(entry) = self.attrs.iter_mut().find(|(key, _)| key == name) {
            entry.1 = value.to_string();
        } else {
            self.attrs.push((name.to_string(), value.to_string()));
        }
    }

    fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|(key, _)| key != name);
    }
}

fn push_escaped(out: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(character),
        }
    }
}

fn find_folder_mut<'a>(node: &'a mut XmlNode, uuid: &str) -> Option<&'a mut XmlNode> {
    if node.tag == "Node" && node.attr("uuid") == Some(uuid) {
        return Some(node);
    }
    for child in node.children.iter_mut() {
        if let Some(found) = find_folder_mut(child, uuid) {
            return Some(found);
        }
    }
    None
}

/// 按标签名找第一个节点（顶层插入用：`Workspace` 没有固定 uuid）。
fn find_tag_mut<'a>(node: &'a mut XmlNode, tag: &str) -> Option<&'a mut XmlNode> {
    if node.tag == tag {
        return Some(node);
    }
    for child in node.children.iter_mut() {
        if let Some(found) = find_tag_mut(child, tag) {
            return Some(found);
        }
    }
    None
}

fn find_service_index(root: &XmlNode, uuid: &str) -> Option<usize> {
    let services = root.child("Services")?;
    services
        .children
        .iter()
        .position(|service| service.tag == "Service" && service.attr("uuid") == Some(uuid))
}

fn service_node(service: &Service, text_attrs: &[(&str, &str)]) -> XmlNode {
    let mut node = XmlNode {
        tag: "Service".into(),
        attrs: Vec::new(),
        children: Vec::new(),
        text: None,
    };
    node.set_attr("type", "Reference");
    node.set_attr("shortcut", "1");
    node.set_attr("reuse", if service.reuse { "1" } else { "0" });
    node.set_attr("uuid", &service.uuid);
    node.set_attr("name", &service.name);
    node.set_attr("description", &service.description);
    node.set_attr("systemid", &service.systemid);
    node.set_attr("client", &service.client);
    node.set_attr("user", &service.user);
    node.set_attr("language", &service.language);
    if service.winmax {
        node.set_attr("winmax", "1");
    }
    if !service.work_dir.trim().is_empty() {
        node.set_attr("work_dir", &service.work_dir);
    }
    node.set_attr("link", &service.link);
    for (name, value) in text_attrs {
        node.set_attr(name, value);
    }
    node
}

/// 新增一条快捷方式：`Services` 里加 `Reference`，目标文件夹里加 `Item`。
pub fn add_shortcut(
    source: &str,
    folder_uuid: &str,
    service: &Service,
    item_uuid: &str,
) -> anyhow::Result<String> {
    let mut root = XmlNode::parse(source)?;

    let services = root
        .child_mut("Services")
        .ok_or_else(|| anyhow::anyhow!("Landscape 缺少 Services 节点"))?;
    services.children.push(service_node(service, &[]));

    let target = if folder_uuid.is_empty() {
        // 空 uuid = 工作区顶层：SAP 允许条目直接挂在 `Workspace` 下，
        // 这种布局里一条文件夹都没有，不能再要求用户先建文件夹。
        find_tag_mut(&mut root, "Workspace")
            .ok_or_else(|| anyhow::anyhow!("Landscape 缺少 Workspace 节点"))?
    } else {
        find_folder_mut(&mut root, folder_uuid)
            .ok_or_else(|| anyhow::anyhow!("找不到文件夹（uuid={folder_uuid}）"))?
    };
    target.children.push(XmlNode {
        tag: "Item".into(),
        attrs: vec![
            ("uuid".to_string(), item_uuid.to_string()),
            ("serviceid".to_string(), service.uuid.clone()),
        ],
        children: Vec::new(),
        text: None,
    });

    Ok(root.to_xml())
}

/// 反向同步：删掉 `Service` 与所有指向它的 `Item`。
pub fn remove_shortcut(source: &str, service_uuid: &str) -> anyhow::Result<String> {
    let mut root = XmlNode::parse(source)?;

    if let Some(index) = find_service_index(&root, service_uuid) {
        if let Some(services) = root.child_mut("Services") {
            services.children.remove(index);
        }
    }

    fn remove_items(node: &mut XmlNode, service_uuid: &str) {
        node.children.retain(|child| {
            !(child.tag == "Item" && child.attr("serviceid") == Some(service_uuid))
        });
        for child in node.children.iter_mut() {
            remove_items(child, service_uuid);
        }
    }
    remove_items(&mut root, service_uuid);

    Ok(root.to_xml())
}

/// 编辑快捷方式：只改用户能改的那几个属性，其余原样保留。
pub fn update_shortcut(source: &str, service: &Service) -> anyhow::Result<String> {
    let mut root = XmlNode::parse(source)?;
    let index = find_service_index(&root, &service.uuid)
        .ok_or_else(|| anyhow::anyhow!("找不到快捷方式（uuid={}）", service.uuid))?;
    let services = root
        .child_mut("Services")
        .ok_or_else(|| anyhow::anyhow!("Landscape 缺少 Services 节点"))?;
    let node = &mut services.children[index];

    node.set_attr("name", &service.name);
    node.set_attr("client", &service.client);
    node.set_attr("user", &service.user);
    node.set_attr("language", &service.language);
    if service.reuse {
        node.set_attr("reuse", "1");
    } else {
        node.remove_attr("reuse");
    }
    if service.winmax {
        node.set_attr("winmax", "1");
    } else {
        node.remove_attr("winmax");
    }
    if service.work_dir.trim().is_empty() {
        node.remove_attr("work_dir");
    } else {
        node.set_attr("work_dir", &service.work_dir);
    }
    if service.sncop.trim().is_empty() {
        node.remove_attr("sncop");
    } else {
        node.set_attr("sncop", &service.sncop);
    }

    Ok(root.to_xml())
}

/// SAP GUI 在退出时会整份重写 Landscape，运行中写入会被它覆盖。
pub fn sap_logon_running() -> bool {
    crate::platform::process::is_running("saplogon.exe")
}

/// 写回：先备份，再写临时文件并原子替换。
/// 备份放到应用自己的数据目录，不污染 SAP 的配置目录；只保留最近 5 份。
fn backup_dir() -> PathBuf {
    crate::core::paths::local_dir().join("backups")
}

fn prune_backups(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().map(|ext| ext == "xml").unwrap_or(false) {
                let modified = path.metadata().ok()?.modified().ok()?;
                Some((modified, path))
            } else {
                None
            }
        })
        .collect();
    // 最新的在前，保留前 keep 个，其余删掉。
    files.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, path) in files.into_iter().skip(keep) {
        let _ = std::fs::remove_file(path);
    }
}

pub fn save_atomic(source: &Path, text: &str) -> anyhow::Result<PathBuf> {
    // 写出去的东西必须先能解析回来。序列化只要有一点差错，SAP 唯一的那份
    // Landscape 就会被覆盖成非法 XML（2026-11-16 因属性名塌陷坏过一次），
    // 所以这里宁可报错，也绝不落下这个文件。
    roxmltree::Document::parse(text).map_err(|error| {
        anyhow::anyhow!("生成的 XML 自检失败，已中止写回（原文件未改动）：{error}")
    })?;

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dir = backup_dir();
    std::fs::create_dir_all(&dir)?;
    let backup = dir.join(format!("SAPUILandscape-{stamp}.xml"));
    std::fs::copy(source, &backup)?;
    prune_backups(&dir, 5);

    let temp = source.with_extension("xml.tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, source)?;
    Ok(backup)
}
