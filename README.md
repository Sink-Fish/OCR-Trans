# OCR Trans — 离线截图识字 + 翻译

按快捷键框选屏幕区域 → 识别文字（可一键复制）→ 英文/日文**原位替换**成中文 → 结果钉在屏幕最上层，可缩放、调透明度、切换原文/译文、涂鸦标注。全部离线运行，不上传任何数据。

技术栈：Tauri 2（Rust 核心 + React/TypeScript 界面）。OCR 用 PaddleOCR **PP-OCRv5**（ONNX Runtime），翻译用 **CTranslate2**（opus-mt-en-zh 英→中，M2M100-418M 日→中）。

> 面向最终用户的完整中文说明见 **[docs/USER_GUIDE.md](docs/USER_GUIDE.md)**（安装、权限、所有快捷键与功能、引擎、术语表、模型管理、常见问题、未测试项）。
>
> **0.6.4** 提供 Apple Silicon Mac 与 Windows x64 版本。[获取安装包](https://github.com/Sink-Fish/OCR-Trans/releases) · [问题反馈](https://github.com/Sink-Fish/OCR-Trans/issues)

## 使用

**一个快捷键**：`⌥⇧O` / `Alt+Shift+O`（可改）→ 框选 → 选区旁的操作栏：
**1 翻译 · 2 识字 · 3 钉图 · 4 标注 · ⌘C 复制图片 · ⌘S 保存 · Esc 取消**（字母键 T/O/P/D 也可用），回车 = 默认动作（翻译 / 钉图 / 识字，可设置）。
识字打开一个文字窗口：截图上叠着可选择的文字层（像 macOS 实况文本），旁边是可编辑的全部文字，⌘C 复制选中，“一键复制全部”复制全部；默认不自动写剪贴板（设置里可打开“识字后自动复制全部”）。
打开“截图后自动翻译”则松开鼠标直接原位翻译并钉图。另可选配三个“直接动作”快捷键（默认不设置）。

* 选区：拖动鼠标；`Esc`、右键或“取消”退出；可重新拖动替换选区。
* 钉图窗口：
  * 拖动移动（“移动”工具下），**滚轮缩放**，`⌥/⌘ + 滚轮`（Windows：`Alt + 滚轮`）调**透明度**，触控板双指捏合也是缩放（或点 ◐），双击 / `Esc` 关闭；
  * `T` 或空格、工具栏“翻译/原文”按钮：**原文 ⇄ 译文**切换；
  * 工具：选择（移动 / 删除标注）、画笔、荧光笔、矩形、椭圆、箭头、文字、自动递增序号、马赛克；5 种预设色 + 取色器、3 档线宽，撤销 `⌘Z` / 重做 `⌘⇧Z`；
  * 复制文字（当前显示的是译文就复制译文）、复制图片 `⌘C`（含标注）、保存 PNG `⌘S`。
  * 鼠标停在译文段落上显示**原文**；**双击段落可直接修改译文**（回车保存）。
  * “快/精”按钮：在快速翻译（opus-mt）和高质量翻译（Qwen3 大模型）之间切换重译。
  * 可以同时钉多张。
* 菜单栏托盘图标：截图…、直接翻译 / 识字 / 钉图、截图历史、关闭所有钉图、设置、模型管理、新手引导、退出。
* 截图历史：最近 50 次截图（缩略图 + 文字 + 译文），可重新钉图 / 翻译 / 复制 / 删除 / 清空。
* 设置：快捷键录制、登录时自动启动、默认翻译引擎（快速 / 高质量）、**术语表**（`原文 = 译文`，两种引擎都遵守）、自定义模型文件夹、下载源（huggingface.co / hf-mirror.com 国内镜像）、译文字体、屏幕录制权限状态。
* 首次启动有新手引导窗口：检查屏幕录制权限（可一键打开系统设置、授权后重启）、快捷键、模型状态。
* 译文背景：先把原文从图上“擦掉”（inpainting，渐变背景也自然），纹理/照片背景再加一层半透明底板。
* 模型管理：查看已安装模型及大小，下载（带进度、可取消）、删除。

### 需要用户本人操作的事项（needs the user）

1. **macOS 屏幕录制权限**：第一次截图时系统会弹窗；或到 *系统设置 → 隐私与安全性 → 屏幕与系统音频录制* 勾选 **OCR Trans**，然后再试一次截图。若系统要求重启，按系统提示操作。从 0.5.4 起应用用固定的本机证书签名，**升级后不会再丢失这个权限**；从 0.5.3 及更早版本升级时需要**最后再授权一次**（见下文“签名与安装”）。
2. **从别的电脑拷过来的应用**：本机证书签名的应用在其他 Mac 上不被信任，需右键 → 打开，或执行 `xattr -dr com.apple.quarantine "OCR Trans.app"`。
3. **日文翻译模型和高质量翻译模型（Qwen3-1.7B，1.1 GB）**都不随应用打包：在 *设置 → 模型* 下载，或在 *设置 → 常规 → 自定义模型文件夹* 选择本项目的 `models/` 目录（运行 `scripts/fetch_models.sh --with-m2m --with-llm` 生成）。用 `scripts/package_mac.sh` 安装时，如果这项没设置，会自动设为本项目的 `models/`。
4. 快捷键若与其他软件冲突，会在设置页提示“注册失败”，换一个即可。

## 外观与品牌

设计令牌（薄荷绿色阶 + 带绿意的中性色、浅色 / 深色两套、系统字体、10–16px 圆角与胶囊按钮、柔和阴影、动效曲线）在 `src/theme.css`；所有窗口只用这些变量。`OCRTRANS_THEME=light|dark` 可强制主题（截图用）。
Logo、托盘图标和字形来源见 [品牌素材说明](docs/brand/LICENSES.md)。

## 构建

前置：Rust（rustup）、Node 22.18+（推荐 24 LTS，测试直接运行 TypeScript）与 pnpm、Python 3（模型转换经 `uv` 临时环境，不污染系统）。

```bash
# 1) 下载/转换模型 + ONNX Runtime 动态库到 models/ 和 runtime/
./scripts/fetch_models.sh            # OCR + 英中
./scripts/fetch_models.sh --with-m2m # 另加日中（M2M100，约 470 MB int8）
./scripts/fetch_models.sh --with-llm # 另加高质量翻译（Qwen3-1.7B Q4_K_M GGUF，1.1 GB）

# 安装前端依赖
pnpm install --frozen-lockfile

# 测试（单元 + 用 testdata/ 的端到端测试；模型缺失时自动跳过）
pnpm test && pnpm build
cargo test --release --workspace --locked
python3 -m unittest discover -s scripts/tests -v

# Windows 目标类型检查（不需要 Windows SDK）
./scripts/check_windows.sh

# 2) 开发运行
pnpm tauri dev

# 3) 打包 + 签名 + 安装：release/OCR-Trans_<版本>_arm64.dmg，并安装到 /Applications/OCR Trans.app 后启动
./scripts/package_mac.sh               # 加 --no-install 只打包不安装

# 选区边框回归检查（在 WebKit 里渲染真实的截图界面，1x / 2x，各种选区形状和贴边情况）
pnpm build && ./scripts/check_overlay_frame.sh
# 钉图边框回归检查（真实钉图页面，窗口按页面要求的大小裁剪；白底 / 浅绿底、工具栏在下 / 在上、短行，1x / 2x）
pnpm build && ./scripts/check_pin_frame.sh
# 设置、位图释放和引导的真实 WebKit 行为回归
./scripts/check_frontend_behavior.sh
```

### 签名与安装（macOS）

**为什么**：之前每次构建都是 ad-hoc 签名，代码签名的“指定要求”（designated requirement）是一个随构建变化的 cdhash，系统的隐私权限（TCC）按它识别应用，所以**每次更新后“屏幕录制”权限都会失效**。

**现在**：
- `scripts/setup_signing.sh`（打包时自动调用，只做一次）用 openssl 生成自签名的代码签名证书 **“OCR Trans Local Signing”**（extendedKeyUsage=codeSigning，10 年），放进项目里**专用的钥匙串文件** `.signing/ocrtrans-signing.keychain-db`（随机密码保存在同一目录，`.signing/` 已加入 .gitignore，**不要提交**）。不改动登录钥匙串、系统钥匙串或信任设置，不需要管理员权限，也不会弹密码框。
- `scripts/sign_mac.sh` 用它给应用签名：先签内部的 dylib（ONNX Runtime），再签整个应用；不开启 hardened runtime；签名时临时把专用钥匙串加入搜索列表，结束后恢复。签名后的指定要求是 `identifier "com.sinkfish.ocrtrans" and certificate leaf = H"…"`——只要证书不变，**每次构建都相同**，权限就会一直保留。
- `scripts/install_mac.sh` 把应用装到 **/Applications/OCR Trans.app**（/Applications 不可写时改装到 `~/Applications` 并提示）：先正常退出正在运行的 OCR Trans，用 `ditto` 复制到临时名再 `mv` 替换，注销 `release/`、`target/` 里的旧副本（`lsregister -u`），然后从 /Applications 启动。`release/` 里只保留 .dmg，不再放可运行的 .app。
- 如果“自定义模型文件夹”没设置，安装时会把它设为本项目的 `models/`（以前从 `release/` 运行时会自动找到旁边的 models，装到 /Applications 后就找不到了）。

**用户需要做的（只一次）**：切换到固定签名后，系统会把新应用当成“新的 OCR Trans”。在 *系统设置 → 隐私与安全性 → 屏幕与系统音频录制* 里：如果列表里有旧的 OCR Trans 条目，选中后点 **“−”** 删除；然后重新勾选（或在第一次截图时允许）**OCR Trans**，并重启应用。之后的更新不需要再授权。
**注意**：`.signing/` 里的证书就是应用的“身份”。删掉或换一台电脑重新生成，会得到新证书，需要再授权一次；想保留就备份这个文件夹。

**正式分发的正确做法**：自签名证书只在本机有效。要发给别人用，应使用付费的 **Apple Developer ID**（Developer ID Application 证书）签名，开启 hardened runtime，并提交 Apple **公证（notarization）**；这样在任何 Mac 上都不会被 Gatekeeper 拦截，权限也会按 Team ID 稳定保留。

命令行验证工具（P0）：

```bash
cargo run --release -p ocrtrans-core --bin ocrtrans-cli -- testdata/en_article.png
cargo run --release -p ocrtrans-core --bin ocrtrans-cli -- --capture 0,0,900,400   # 截取屏幕区域
```

输出 JSON：每个段落的四边形坐标、原文、语言、背景/前景色、译文，以及各阶段耗时。

### 开发用环境变量

* `ORT_DYLIB_PATH`：指定 ONNX Runtime 动态库（默认在可执行文件旁、`.app/Contents/Resources/runtime/` 或上层的 `runtime/` 里查找）。
* `OCRTRANS_DATA_DIR`：替换模型下载目录（测试用）。
* `OCRTRANS_TEST_PIN=<png>[:translate]`：启动即用图片打开一个钉图。
* `OCRTRANS_TEST_CAPTURE=<pin|ocr|translate>[@x,y,w,h]`：启动 2 秒后走一遍截图流程，可自动完成选区（不用键盘即可端到端测试）；`@cancel` 则 4 秒后若界面还在就取消（只观察截图前后发生了什么，比如 macOS 的屏幕录制确认窗口，什么也不保留）。
* `OCRTRANS_TEST_OVERLAY_KEYS=1`（配合 `OCRTRANS_TEST_SHOT=<png>`）：选区界面的自检——Esc、截图失败、连按快捷键、选区为空、完成选区、焦点被别的应用拿走、连续 24 轮，每步检查界面已撤下且本进程没有占用 Enter / Esc / 1–4 这些全局按键，结果写进日志后退出。`./scripts/check_overlay_keys.sh` 用已安装的应用跑这一项。
* `OCRTRANS_OPEN_SETTINGS=<general|models|about>`：启动时打开设置。
* `OCRTRANS_TEST_DOWNLOAD=<模型 id>`：启动即下载一个模型（配合 `OCRTRANS_DATA_DIR`）。
* `OCRTRANS_LLM_THREADS=生成[,批处理]`：覆盖高质量翻译的线程数（默认：混合架构只用性能核，否则物理核数减 1～2）；`OCRTRANS_DEBUG_LLM=1`：把模型原始回答打到 stderr。
* `OCRTRANS_FORCE_ONBOARDING=1`：强制显示新手引导；`OCRTRANS_DEBUG=1`：打印截图流程各阶段耗时和原生模型详细日志；`OCRTRANS_ENGINE=quality|fast`：覆盖默认翻译引擎。

## 目录

```
crates/core/        Rust 管线：capture（xcap）、ocr（PP-OCRv5 det/rec）、layout（行→段落、配色）、translate（CTranslate2 + 术语表）、
                    llm（Qwen3 / llama.cpp）、inpaint（擦除原文）
  tests/pipeline.rs         端到端测试
  src/bin/ocrtrans-cli.rs   P0 命令行
src-tauri/          Tauri 应用：托盘、全局快捷键、窗口管理、模型下载、设置
src/                React 界面：Overlay（框选）、Pin（钉图/译文渲染/涂鸦）、TextWindow、Settings
scripts/            fetch_models.sh（下载+转换模型）、make_test_images.py（生成测试图）
models/ runtime/    （git 忽略）模型与 ONNX Runtime
```

## Windows

代码按跨平台写（xcap、Tauri、ONNX Runtime、CTranslate2、llama.cpp 都支持 Windows x64）。

* 类型检查：`scripts/check_windows.sh`（不需要 Windows SDK，不含 C++ 翻译后端）。
* Windows x64 安装包可通过 `scripts/build_windows.sh` 交叉构建，工具链见脚本开头。安装时检查 Microsoft Visual C++ x64 运行库，缺少时可前往[微软官方下载页](https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist)安装，再重新运行本安装器。
* Windows 原生回归使用 `scripts/build_windows_native.ps1` 和 `scripts/verify_windows.ps1`。这些脚本支持专用交互式 Windows x64 runner。

## 许可与商业授权

macOS 和 Windows 官方安装包均可免费下载。SinkFish 的源码向公众开放，非商业用途可以运行、修改和免费分发。企业内部业务、商业服务、销售或其他商业用途均须事先获得单独书面授权。请在 [Issues](https://github.com/Sink-Fish/OCR-Trans/issues) 提出联系请求。具体条件和品牌使用规则见 [项目许可](LICENSE)。

引入的代码、运行库、字体和模型分别适用上游许可。来源见 [第三方清单](docs/LICENSE_INVENTORY.md)，完整声明可在应用的“设置 → 关于”查看。

欢迎通过 Issues 提交问题、功能建议和商业授权联系请求。报告截图或诊断问题时，请先移除截图、模型路径和日志中的个人内容。若计划贡献代码，请先在 Issue 中沟通变更范围和贡献授权。
