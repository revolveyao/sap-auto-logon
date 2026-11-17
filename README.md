# SAP Auto Logon

把 SAP GUI 的 Landscape 变成一排卡片，双击直接登录。

## 简介

SAP Logon 里登录一次要点开树、选系统、填客户端和密码。这个工具直接读本机的 `SAPUILandscape.xml`，把快捷方式铺成卡片，双击即登录，密码存在 Windows 凭据管理器里；也能反向写回 Landscape 新建、修改、删除快捷方式，写前自动备份。

> 只读本机的 SAP GUI 配置；写回只动快捷方式（`Reference`），不碰基础系统。

## 下载

到 [Releases](../../releases/latest) 下载 `SAP-Auto-Logon-<版本>-setup.exe`。安装是 per-user 的（装到 `%LOCALAPPDATA%\Programs\SAP Auto Logon`），不需要管理员权限。

也可以直接拿单文件 `sap-auto-logon.exe` 跑，纯绿色。

## 功能

| 功能 | 说明 |
|---|---|
| Landscape 定位 | 读注册表 `LandscapeFilesLastUsed` / `LandscapeFilesInUse_<pid>`，回退 `%APPDATA%\SAP\Common` |
| 解析与缓存 | 缓存 `%LOCALAPPDATA%\SapAutoLogon\cache.json`，按源文件 `mtime + size` + 程序版本失效 |
| 卡片网格 | 按文件夹分组、每行四张、最近登录置顶、可固定、可拖动排序 |
| 登录 | 双击卡片；已存密码直接调 `sapshcut.exe`，没存则弹密码框 |
| 密码存储 | Windows 凭据管理器（`CredWriteW`），目标名 `SapAutoLogon/<uuid>` |
| 状态灯 | 灰=没密码 / 蓝=已存密码 / 绿=上次成功 / 红=上次失败 |
| 右键菜单 | 登录、设置密码、编辑参数、设置别名、固定、上移下移、删除 |
| 新建 | 标题栏 `+`：选文件夹（含「顶层」）与基础系统，名称自动拼，客户端校验三位数字 |
| 写回 | 只写 `Reference`；写前备份到 `%LOCALAPPDATA%\SapAutoLogon\backups`（保留 5 份）；SAP Logon 在运行就拒绝 |
| 托盘 | 关闭收进托盘；菜单打开主窗口 / 设置 / 退出 |
| 任务栏图标 | 按系统标识上色、客户端号居中，多开时一眼分清哪个是哪个 |
| 外观 | 跟随系统 / 浅色 / 深色；材质 Mica（Win11 22H2+）/ 亚克力 / 不透明 |
| 主题库 | 主题即文件（%APPDATA%\SapAutoLogon\themes\<id>.json）；命名、切换、新建、删除，导入导出走剪贴板；内置「默认」只读，附四套预置可改可删 |
| 配色编辑 | 客户端 6 组循环色、系统角色 4 色、卡片 7 项、窗口底色，浅深各一套；可按基色一键派生整套（带确定性抖动，同一基色结果稳定） |
| 系统角色 | 系统标识包含 D/Q/P 即为 开发/测试/生产（多命中按 P>Q>D），可右键「设置角色…」按 SID 手动指定；卡片徽标与任务栏图标共用 |
| SAP GUI 特性 | UUID 模式：8.10 及以上默认开启，交给 SAP GUI 自己解析系统 |
| 国际化 | `locales/ui.yml`（en / zh-CN），跟随系统语言 |
| 单实例 | `Local\SapAutoLogon.SingleInstance` |

## 使用

1. 打开程序，它会自动找到 SAP GUI 用着的 Landscape。
2. 双击卡片登录。第一次会问密码，勾选记住后写进凭据管理器。
3. 标题栏 `+` 新建快捷方式，结果会写回 `SAPUILandscape.xml`（SAP GUI 里立即可见）。
4. 点 `✕` 收进托盘；退出走托盘菜单。

## 构建

需要 Rust stable（本机验证 1.99）和 Windows。

```powershell
cargo build            # 调试构建
cargo build --release  # 发布构建
```

图标由 `tools/gen-icon.py` 生成（需要 Pillow），`build.rs` 把它和版本信息嵌进 exe。

## 打包安装包

```powershell
.\scripts\package.ps1
```

需要 [NSIS](https://nsis.sourceforge.io/)（脚本会自动去常见位置找 `makensis.exe`），输出 `dist\SAP-Auto-Logon-<版本>-setup.exe`。

## 目录

```
src/
├── main.rs              启动、窗口选项、i18n 初始化
├── core/                paths / settings / cache / cred
├── landscape/           locate / parse / model / write
├── platform/            tray / window / single_instance / autostart / system
├── services/            logging / shcut
└── ui/                  root（卡片 + 弹框 + 设置）/ card
locales/ui.yml           中英文案
installer/               NSIS 脚本
scripts/package.ps1      本地打包
tools/gen-icon.py        图标生成
```

## 隐私

- 密码只进 Windows 凭据管理器，不落配置文件、不进日志（`sapshcut` 命令行里的 `-pw=` 打码）。
- 不回传任何数据；Landscape 与缓存都只在本机。

## 许可

[MIT](LICENSE)
