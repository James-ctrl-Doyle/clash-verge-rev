#!/usr/bin/env node
/**
 * 组装便携版（免安装）并打包为 zip / 7z。
 *
 * 背景：上游 `tauri build` 只产出安装包（Windows 上是 NSIS），没有"免安装便携版"目标。
 * 本脚本把 Tauri 在构建阶段已经放到 `target/<triple>/release/` 下的产物
 * —— 主程序、sidecar（mihomo 内核）、resources —— 组装成解压即用的目录，再压缩。
 *
 * 用法：
 *   node scripts/portable.mjs [选项]
 *
 * 选项：
 *   --target <triple>    指定目标三元组（默认按当前平台/架构推断）
 *   --formats zip,7z     要产出的压缩格式（默认 zip，若检测到 7-Zip 则再加 7z）
 *   --no-clean           保留已存在的输出目录（默认先清空）
 *
 * 前置条件：先完成一次 release 构建，即
 *   pnpm run prebuild && pnpm build
 * 或仅后端：
 *   cargo build --release --target <triple>
 *
 * 产物（默认输出到 <repo>/release/）：
 *   <ProductName>/                     解压即用的目录（压缩包内不含这一层）
 *   <ProductName>_<version>_Portable.zip
 *   <ProductName>_<version>_Portable.7z   （需系统安装 7-Zip）
 */

import AdmZip from 'adm-zip'
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

/** 按 `平台-架构` 推断 Rust 目标三元组 */
const TRIPLE_BY_PLATFORM = {
  'win32-x64': 'x86_64-pc-windows-msvc',
  'win32-arm64': 'aarch64-pc-windows-msvc',
  'darwin-x64': 'x86_64-apple-darwin',
  'darwin-arm64': 'aarch64-apple-darwin',
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'linux-arm64': 'aarch64-unknown-linux-gnu',
}

const isWindows = process.platform === 'win32'
const EXE = isWindows ? '.exe' : ''

function fail(message) {
  console.error(`错误：${message}`)
  process.exit(1)
}

function parseArgs(argv) {
  const opts = { target: null, formats: null, clean: true }
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i]
    if (arg === '--target') opts.target = argv[++i]
    else if (arg === '--formats') opts.formats = argv[++i].split(',').map((s) => s.trim()).filter(Boolean)
    else if (arg === '--no-clean') opts.clean = false
    else if (arg === '--help' || arg === '-h') {
      console.log(fs.readFileSync(fileURLToPath(import.meta.url), 'utf8').split('*/')[0])
      process.exit(0)
    } else fail(`未知参数 ${arg}`)
  }
  return opts
}

function readJson(relPath) {
  const abs = path.join(ROOT, relPath)
  if (!fs.existsSync(abs)) fail(`找不到 ${relPath}`)
  return JSON.parse(fs.readFileSync(abs, 'utf8'))
}

function copyRecursive(src, dst) {
  const stat = fs.statSync(src)
  if (stat.isDirectory()) {
    fs.mkdirSync(dst, { recursive: true })
    for (const entry of fs.readdirSync(src)) {
      copyRecursive(path.join(src, entry), path.join(dst, entry))
    }
  } else {
    fs.mkdirSync(path.dirname(dst), { recursive: true })
    fs.copyFileSync(src, dst)
  }
}

/** 找可用的 7-Zip 可执行文件；没有返回 null（7z 为可选格式） */
function find7z() {
  const candidates = isWindows
    ? ['7z.exe', 'C:\\Program Files\\7-Zip\\7z.exe', 'C:\\Program Files (x86)\\7-Zip\\7z.exe']
    : ['7z', '7za', '7zr']
  for (const candidate of candidates) {
    const result = spawnSync(candidate, ['i'], { stdio: 'ignore' })
    if (!result.error && result.status === 0) return candidate
  }
  return null
}

function humanSize(bytes) {
  return `${(bytes / 1048576).toFixed(1)} MiB`
}

/** 把目录内容加到 zip 根部（不含目录自身这一层） */
function addFolderFlat(zip, baseDir) {
  const walk = (rel) => {
    const abs = rel ? path.join(baseDir, rel) : baseDir
    for (const entry of fs.readdirSync(abs, { withFileTypes: true })) {
      const relEntry = rel ? path.join(rel, entry.name) : entry.name
      if (entry.isDirectory()) {
        walk(relEntry)
      } else {
        const zipDir = path.dirname(relEntry)
        zip.addLocalFile(
          path.join(baseDir, relEntry),
          zipDir === '.' ? '' : zipDir.replace(/\\/g, '/'),
          entry.name,
        )
      }
    }
  }
  walk('')
}

function main() {
  const opts = parseArgs(process.argv.slice(2))

  const tauriConf = readJson('src-tauri/tauri.conf.json')
  const pkg = readJson('package.json')
  const productName = tauriConf.productName || 'Clash Verge'
  const version = tauriConf.version || pkg.version
  const binName = pkg.name // Cargo 侧的 bin 名，与 package.json 同名

  const platformKey = `${process.platform}-${process.arch}`
  const triple = opts.target || TRIPLE_BY_PLATFORM[platformKey]
  if (!triple) fail(`未知平台 ${platformKey}，请用 --target <triple> 指定目标三元组`)

  const releaseDir = path.join(ROOT, 'target', triple, 'release')
  const mainBinary = path.join(releaseDir, binName + EXE)

  console.log('便携版组装')
  console.log(`  版本      : ${version}`)
  console.log(`  目标      : ${triple}`)
  console.log(`  构建产物  : ${path.relative(ROOT, releaseDir)}`)

  if (!fs.existsSync(mainBinary)) {
    fail(
      `未找到主程序 ${path.relative(ROOT, mainBinary)}\n` +
        '       请先完成一次 release 构建：\n' +
        '         pnpm run prebuild   # 下载 mihomo 内核 / 规则库 / 服务等外部依赖\n' +
        '         pnpm build          # 完整构建（内含前端 web:build）\n' +
        '       若前端已构建过、只想编后端，可用：\n' +
        `         cargo build --release --target ${triple}`,
    )
  }

  // 收集要进包的文件：主程序 + sidecar + resources
  const sidecars = fs
    .readdirSync(releaseDir)
    .filter((name) => /^verge-mihomo(-\w+)?\.exe?$/.test(name))
    .sort()

  const resourcesSrc = path.join(releaseDir, 'resources')
  const resources = fs.existsSync(resourcesSrc)
    ? fs.readdirSync(resourcesSrc)
    : []

  if (sidecars.length === 0) {
    console.warn('  警告：release 目录下没有找到 verge-mihomo* sidecar，产物可能无法运行内核')
  }
  if (resources.length === 0) {
    console.warn('  警告：release 目录下没有 resources/，产物可能缺少规则库与服务程序')
  }

  // 组装输出目录
  const outRoot = path.join(ROOT, 'release')
  const packDir = path.join(outRoot, productName)
  if (opts.clean && fs.existsSync(packDir)) fs.rmSync(packDir, { recursive: true, force: true })
  fs.mkdirSync(packDir, { recursive: true })

  const mainDest = path.join(packDir, productName + EXE)
  fs.copyFileSync(mainBinary, mainDest)
  console.log(`\n  主程序   -> ${productName}${EXE}`)

  for (const sidecar of sidecars) {
    fs.copyFileSync(path.join(releaseDir, sidecar), path.join(packDir, sidecar))
    console.log(`  sidecar  -> ${sidecar}`)
  }

  if (resources.length > 0) {
    copyRecursive(resourcesSrc, path.join(packDir, 'resources'))
    console.log(`  resources-> resources/（${resources.length} 个文件）`)
  }

  // 打包
  const formats = opts.formats ?? (find7z() ? ['zip', '7z'] : ['zip'])
  const baseName = `${productName.replace(/\s+/g, '.')}_${version}_Portable`

  console.log('\n压缩包')

  if (formats.includes('zip')) {
    const zipPath = path.join(outRoot, `${baseName}.zip`)
    const zip = new AdmZip()
    addFolderFlat(zip, packDir)
    zip.writeZip(zipPath)
    console.log(`  zip -> ${path.relative(ROOT, zipPath)}  ${humanSize(fs.statSync(zipPath).size)}`)
  }

  if (formats.includes('7z')) {
    const sevenZip = find7z()
    if (!sevenZip) {
      console.warn('  7z  -> 未找到 7-Zip，已跳过（安装 7-Zip 后重跑，或用 --formats zip）')
    } else {
      const zip7Path = path.join(outRoot, `${baseName}.7z`)
      // `*` 交给 7z 自行展开；在 packDir 内执行以保证压缩包内不含顶层目录
      const result = spawnSync(sevenZip, ['a', '-t7z', '-mx=9', zip7Path, '*'], {
        cwd: packDir,
        stdio: 'ignore',
      })
      if (result.status !== 0) fail(`7z 打包失败（退出码 ${result.status}）`)
      console.log(
        `  7z  -> ${path.relative(ROOT, zip7Path)}  ${humanSize(fs.statSync(zip7Path).size)}`,
      )
    }
  }

  console.log(`\n完成。解压目录：${path.relative(ROOT, packDir)}`)
  console.log('压缩包内不含多余的顶层目录，解压出来直接就是可执行文件。')
  console.log('配置保存在程序同目录的 config/ 下（首次运行自动生成）。')
}

main()
