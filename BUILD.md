# 构建说明

本仓库发布上游 [Clash Verge Rev](https://github.com/clash-verge-rev/clash-verge-rev) 的
Windows x64 便携版。这里说明如何自己从源码构建出同样的产物。

> 只想要成品？直接到 [Releases](../../releases) 下载，不需要构建。

## 前置条件

| 依赖 | 说明 |
|---|---|
| Rust | 需 MSVC toolchain：`rustup target add x86_64-pc-windows-msvc` |
| Node.js | 20+，并启用 pnpm：`corepack enable` |
| MSVC 生成工具 | Visual Studio Build Tools，含 C++ 工具集 |
| GNU `patch` | Windows 上必需（Git for Windows / MSYS2 自带） |
| 7-Zip | 打包 7z 需要（默认两种格式都产出）。只打 zip 时可省 |

具体版本要求见仓库根的 `.tool-versions`。

## 构建

```bash
pnpm install
pnpm run prebuild      # 下载 mihomo 内核、GeoIP/GeoSite、服务端程序
pnpm run web:build     # 构建前端 → dist/
cargo build --release --target x86_64-pc-windows-msvc
pnpm run portable      # 组装便携版并打包 → _build/release/
```

产物都在 `_build/release/`：

```
Clash Verge/                            解压即用的目录
Clash.Verge_<version>_Portable.zip      通用格式
Clash.Verge_<version>_Portable.7z       需系统装有 7-Zip，体积约小一半
```

压缩包内不含多余的顶层目录，解压出来直接就是可执行文件。**两种格式都会产出**；
缺少 7-Zip 时脚本会直接报错（只打 zip 请用 `--formats zip`）。

> 每次打包**开工第一件事**是删掉输出目录里的 `Clash Verge/config/` —— 那是运行程序时
> 生成的本机配置、日志与浏览器缓存，留着会被一并打进发布包。

## 各步骤在做什么

| 命令 | 作用 |
|---|---|
| `pnpm run prebuild` | 上游的 `scripts/prebuild.mjs`，下载运行所需的外部二进制与规则库 |
| `pnpm run web:build` | `tsc --noEmit && vite build`，生成前端资源 `dist/` |
| `cargo build --release` | 编译 Rust 后端。Tauri 的 build script 会**顺带**把 sidecar（mihomo 内核）与 `resources/` 放进 `target/<triple>/release/` |
| `pnpm run portable` | 收集上述产物 → 组装目录 → 打包 zip / 7z |

## 为什么不需要 `pnpm build`

`pnpm build` 等价于 `tauri build`，会在编译之外**额外执行一次 NSIS 安装包打包**。
便携版用不到安装包，所以按上面的四步走即可，能省掉这一步。

⚠ 但反过来说：`cargo build` **不会**构建前端（那是 `tauri build` 的 `beforeBuildCommand`），
所以 `pnpm run web:build` 不能省。

如果你同时也要安装包，把最后两步换成：

```bash
pnpm build
pnpm run portable
```

## 与上游的差异

相比上游，本仓库多了两处改动 —— 这也是"为什么不能直接用上游的方式构建出便携版"的原因。

**1. 配置目录便携化**（四处改动，细节见上一节）

上游把配置、窗口状态、WebView2 数据、cookie 分散写在 `%APPDATA%` 与 `%LOCALAPPDATA%` 下，
且后三者无法通过配置重定向。本仓库全部改到程序同目录的 `config/` 下。

**2. 便携版组装脚本**（`scripts/portable.mjs` + `pnpm portable`）

上游的 `CONTRIBUTING.md` 里写有 "Portable Version" 一节和 `pnpm portable` 命令，但
**实现并不存在** —— `package.json` 中没有该脚本，也没有任何打包脚本，CI 与 Release
资产里都不含免安装包。本仓库把它补齐了。上游的 `pnpm clean` 同样是"文档里有、实现没有"。

> 也就是说：即使只想要一个"把构建产物压成压缩包"的步骤，上游也没有现成的可以用。

## 选项

```bash
pnpm run portable -- --formats zip                        # 只打 zip（不需要 7-Zip）
pnpm run portable -- --target aarch64-pc-windows-msvc     # 指定目标三元组
pnpm run portable -- --out dist-portable                  # 换输出目录（默认 _build/release）
pnpm run portable -- --no-clean                           # 保留已有输出目录（config/ 仍会先删掉）
```

## 配置文件放在哪

构建出的便携版把**全部**状态都放在程序同目录的 `config/` 下，**不写系统盘**：

| 内容 | 位置 |
|---|---|
| 配置与订阅 | `config/`（`verge.yaml`、`profiles.yaml`、`profiles/` …） |
| 运行日志 | `config/logs/` |
| 窗口位置与大小 | `config/window_state.json` |
| 浏览器数据（Cookie、缓存） | `config/webview/EBWebView/` |

因此整个程序目录可以整体搬走，换机器、换盘符都不会丢配置；删掉目录也不会有残留。

### 为此做的四处改动

上游原生把这些内容分散在 `%APPDATA%\<APP_ID>`、`%LOCALAPPDATA%\<APP_ID>` 与
`%LOCALAPPDATA%\<APP_ID>\EBWebView`，而且其中三处**都绕过了配置目录解析、也无法通过配置重定向**
（Tauri 内部直接调 `dirs::config_dir()` / `dirs::cache_dir()`，不读环境变量）：

1. **`utils/dirs.rs` 的 `app_home_dir()`** —— 配置根目录指向程序同目录的 `config/`
2. **自实现窗口状态持久化**（`utils/window_state.rs`）—— 取代 `tauri-plugin-window-state`
   （它固定写向 Tauri 的 `app_config_dir()`）。文件格式与该插件一致，旧文件可直接沿用
3. **给主窗口指定 `data_directory`**（`utils/resolve/window.rs`）——
   把 WebView2 用户数据目录移进 `config/webview`
4. **关闭 `tauri-plugin-http` 的 `cookies` feature** —— 它固定把 cookie 存到
   `app_cache_dir()`（即 `%LOCALAPPDATA%\<APP_ID>\.cookies`），同样没有配置入口

> 第 4 项的代价：HTTP 层不再持久化 cookie。前端没有直接使用 cookie，
> 影响面仅限于少数依赖登录态的订阅站点可能需要重新登录。

## 故障排查

**`pnpm run prebuild` 报 `spawnSync cmd.exe EBUSY`**
进程创建被环境限制（受限终端 / 沙箱 / 某些容器）。换一个普通终端重试。

**`pnpm install` 时 esbuild、unrs-resolver 的 postinstall 失败**
这两个脚本会校验或下载平台二进制，失败通常是网络原因。确认能访问 npm registry，
国内可切换镜像后重试。

**构建出的程序缺少规则库、或内核起不来**
检查 `pnpm run prebuild` 是否成功 —— sidecar 与 `resources/` 的内容都由它下载。

**构建报 indexmap / schemars 相关的 `E0107`**
本仓库已在 `Cargo.toml` 里显式开启 indexmap 1.9 的 `std` 特性修掉此问题，
请确认代码是最新的。
