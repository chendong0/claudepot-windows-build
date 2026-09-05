# claudepot-app 安装记录（Windows，本机）

## 结果：成功

最终产物：`target/release/claudepot-tauri.exe`（约 56MB，独立可执行文件，`--no-bundle` 模式，未打包成安装程序）。

## 步骤与实际情况

1. **Rust 工具链**：官方 rustup-init.exe 默认装的是 `x86_64-pc-windows-msvc`，但本机没有 Visual Studio C++ Build Tools（无 `cl.exe`），链接会失败。
   - 第一次尝试：MSVC 工具链 → 失败（缺 MSVC 链接器）。
   - 第二次尝试：切到 `stable-x86_64-pc-windows-gnu` → 编译 `getrandom`/`windows-sys` 时失败，报错 `dlltool.exe 找不到`（缺 MinGW-w64 二进制工具）。
   - 修复：用 `winget install BrechtSanders.WinLibs.POSIX.UCRT` 装了一份自带 `dlltool.exe`/`gcc`/`ld` 的独立 MinGW-w64 发行版，加进 PATH，重新编译成功。
2. **pnpm**：`npm install -g pnpm`，正常。
3. **`cargo build -p claudepot-cli --release`**：成功，耗时约 8 分钟。产物 `target/release/claudepot.exe`，已复制为
   `src-tauri/binaries/claudepot-cli-x86_64-pc-windows-gnu.exe`（GNU host triple，不是 README 里示例的 msvc）。
4. **`pnpm install`**：成功，1 分 28 秒。有一条警告说 `esbuild` 的 build script 被跳过（`pnpm approve-builds` 可手动放行），未影响最终构建。
5. **`pnpm tauri build --no-bundle`**：成功，耗时约 17 分 51 秒。链接阶段有一条无害警告（`.rsrc merge failure: multiple non-default manifests`），不影响产物可用性。

## 已知遗留

- 用的是 GNU 工具链而非 README 默认建议的 MSVC 工具链，属于本机环境的绕行方案；上游若发布 MSVC 专属的行为差异，以此为准排查。
- `--no-bundle` 只生成裸 exe，没有生成安装包/开始菜单快捷方式；如果要正式安装体验（图标、卸载入口），需要另外跑打包步骤或手动创建快捷方式。
- 数据目录：`~/.claudepot/`（可用 `CLAUDEPOT_DATA_DIR` 环境变量改）。
