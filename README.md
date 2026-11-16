# OCR Trans

OCR Trans 是适用于 macOS 和 Windows 的离线截图识字与翻译工具。按快捷键框选屏幕，即可提取文字，或将英文、日文原位翻译成中文。结果可以钉在其他窗口上方，切换原文与译文、添加标注或保存为 PNG。

截图、识字和翻译在本机完成。安装包内置 OCR 和英译中快速模型。日译中快速模型和高质量翻译模型可在设置中按需下载。截图历史保存在本机，可以在应用内删除。

## 安装

当前源码版本为 **0.6.6（GPL-3.0-only）**，双端安装包正在构建；下面的 0.6.5 链接是旧许可历史版本。请以对应 Release 的说明和校验清单为准。

[下载 macOS 0.6.5 安装包](https://github.com/Sink-Fish/OCR-Trans/releases/download/v0.6.5/OCR-Trans_0.6.5_arm64.dmg) · [发布说明、源码与 SHA-256 校验文件](https://github.com/Sink-Fish/OCR-Trans/releases/tag/v0.6.5)

已发布的 0.6.5 提供 Apple Silicon Mac 安装包、Windows x64 未签名安装包和对应源码。Windows GUI 安装、启动、截图和钉图流程尚未完成验证，详见发布说明。**0.6.5 现有下载文件仍包含发布时的非商业许可；下文 GPL-3.0-only 适用于本分支许可迁移后的自有材料，不代表旧安装包已被重新打包或替换。**

| 平台 | 安装文件 | 首次使用 |
| --- | --- | --- |
| macOS 13 及以上，Apple 芯片 | `arm64.dmg` | 将 OCR Trans 拖入“应用程序”后打开，并授予屏幕录制权限。当前分发包使用本地签名，未经 Apple 公证。 |
| Windows x64 | [未签名 EXE](https://github.com/Sink-Fish/OCR-Trans/releases/download/v0.6.5/OCR-Trans_0.6.5_native_x64-setup.exe) | Windows 版使用 WebView2 和 Microsoft Visual C++ x64 运行库。 |

## 快速使用

1. 在 Mac 上按 `⌥⇧O`，在 Windows 上按 `Alt+Shift+O`，拖动鼠标框选要处理的区域。快捷键可在设置中修改。
2. 按 `1` 翻译、`2` 识字、`3` 钉图或 `4` 标注。识字窗口可以选取、编辑和复制文字。
3. 在钉图窗口按 `T` 切换原文和译文，滚轮缩放，双击译文段落可以修改文字。

详细操作、模型管理、历史记录和权限排查请参见[使用手册](docs/USER_GUIDE.md)。

## 从源码构建

在 Apple Silicon Mac 上准备 Xcode 命令行工具、Rust 工具链、CMake、Node.js 22.18 或更高版本、pnpm、Python 3 和 uv。在仓库根目录运行：

```sh
pnpm install --frozen-lockfile
./scripts/fetch_models.sh
pnpm tauri dev
```

生成 macOS 安装包可运行 `./scripts/package_mac.sh --no-install`。Windows x64 的原生构建入口为 [build_windows_native.ps1](scripts/build_windows_native.ps1)，需要 x64 MSVC 开发环境以及脚本所列的工具、模型与运行时。项目也提供 [macOS 交叉构建脚本](scripts/build_windows.sh)。

```sh
pnpm test
cargo test --workspace --release --locked
python3 -m unittest discover -s scripts/tests
```

## 使用许可与反馈

© 2026 SinkFish。本分支中 SinkFish 有权授权的自有代码、文档和测试材料采用 **GNU GPL 第 3 版，仅限该版本（SPDX: GPL-3.0-only）**。允许商业使用和收费分发，不再附加“禁止商用”或“商业用途须另行授权”的限制。分发本软件或衍生作品时须遵守 GPL 的许可证、版权声明及对应源码提供等义务；修改版应标明修改。第三方代码、模型、运行库、字体和图稿中的上游材料仍适用各自许可，不因本项目迁移而被重新授权。名称与标识不授予商标权或官方背书。[GPL 全文](LICENSE) · [适用范围与版本边界](docs/LICENSING.md) · [第三方清单](docs/LICENSE_INVENTORY.md)

问题反馈请使用 [GitHub Issues](https://github.com/Sink-Fish/OCR-Trans/issues)。提交问题时请注明应用版本、操作系统和复现步骤，并先遮盖日志或截图中的私人内容。
