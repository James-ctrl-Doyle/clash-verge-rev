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

具体版本要求见仓库根的 `.tool-versions`。

## 构建

```bash
pnpm install
pnpm run prebuild      # 下载 mihomo 内核、GeoIP/GeoSite、服务端程序
pnpm run web:build     # 构建前端 → dist/
cargo build --release --target x86_64-pc-windows-msvc
pnpm run portable      # 组装便携版并打包 → release/
```

产物都在 `release/`：

```
Clash Verge/                            解压即用的目录
Clash.Verge_<version>_Portable.zip      通用格式
Clash.Verge_<version>_Portable.7z       需系统装有 7-Zip，体积约小一半
```

压缩包内不含多余的顶层目录，解压出来直接就是可执行文件。

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

## 选项

```bash
pnpm run portable -- --formats zip                        # 只打 zip
pnpm run portable -- --target aarch64-pc-windows-msvc     # 指定目标三元组
pnpm run portable -- --no-clean                           # 保留已有输出目录
```

## 配置文件放在哪

构建出的便携版把配置放在**程序同目录的 `config/`**（上游原生是 `%APPDATA%\<APP_ID>`）。
该目录不存在会自动创建；配置文件缺失时用模板生成默认值。所以整个目录可以整体搬走。

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
