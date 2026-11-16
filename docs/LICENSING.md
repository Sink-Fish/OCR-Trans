# 许可范围与版本边界

## 当前源码修订

© 2026 SinkFish。SinkFish 有权授权的自有源代码、文档和测试材料采用 GNU General Public License 第 3 版，仅限该版本，SPDX 标识为 `GPL-3.0-only`。这不是 `GPL-3.0-or-later`。

GPL 允许商业使用、修改和收费分发。本修订不再以商业用途为由要求额外授权。分发软件或衍生作品须满足 GPL 的适用条件，包括保留许可证与版权声明、标明修改、按要求提供完整对应源码。完整条款见 [LICENSE 原文](../LICENSE)。

## 第三方材料与图稿边界

第三方代码、补丁中的上游内容、模型、运行库、字体及其他第三方材料继续适用各自许可证。保留现有署名、NOTICE、源码获取地址和许可原文。详见 [第三方清单](LICENSE_INVENTORY.md)。

应用与菜单栏图稿中 SinkFish 有权授权的自有设计及生成素材也采用 GPL-3.0-only，不附加非商业限制。上游字体软件仍遵守 SIL OFL 1.1，保留其来源和声明，不重新授权字体软件；名称与标识的商标权、官方背书不随代码或图稿版权许可授予，且商标说明不缩减 GPL 已授予的权利。详见 [图稿说明](brand/LICENSES.md)。

## 旧版 0.6.5

`v0.6.5` 标签及曾公开的源码 ZIP、macOS DMG 和 Windows EXE 是历史发行物，仍携带发布时的 OCR Trans 非商业使用许可 1.0。新版发布后，旧 Release 页面及下载附件已从 GitHub 移除并保留本地备份，历史标签和 Git 记录保留。许可迁移不改写标签、不替换旧包，也不声称旧包已经采用 GPL。

## 已发布的 GPL 二进制与对应源码

[0.6.6 Release](https://github.com/Sink-Fish/OCR-Trans/releases/tag/v0.6.6) 已公开 macOS 和 Windows 安装包，两端均从固定提交 `455f87a3a976826d82533f8f4634a6ad4876ff36` 重新构建，包内包含 GPL 全文与第三方声明。历史标签中的文档保留构建时状态；当前发布状态以该 Release 和 main 文档为准。

发布附件提供固定提交的应用源码、Cargo 依赖源码、前端运行时与上游源码、源码获取说明及 SHA-256 校验清单。ONNX Runtime 的固定源码、递归子模块和外部依赖获取方式见 [SOURCE-SUPPLY.md](https://github.com/Sink-Fish/OCR-Trans/releases/download/v0.6.6/OCR-Trans_0.6.6_SOURCE-SUPPLY.md)。Cargo 源包内的 vendor 配置示例需要将目录改为使用者实际解压路径。

发行者仍须提供与二进制准确对应的完整源码、修改、构建及安装脚本，并持续满足 GPL 第 6 条的源码提供义务；不能只链接一个可能继续变化的 main 分支。依赖获取可能需要联网。后续变更须复核实际使用的 ONNX Runtime、模型及原生运行库版本、许可、源码义务和材料权属。

## 全文来源与维护

`LICENSE` 使用 SPDX 官方 license-list-data 项目的 [GPL-3.0-only 标准全文](https://github.com/spdx/license-list-data/blob/main/text/GPL-3.0-only.txt)，上游原文为 [GNU GPL 第 3 版](https://www.gnu.org/licenses/gpl-3.0.html)。许可证原文不得夹入项目额外限制。本项目选择“仅第 3 版”的适用声明在本文、包元数据与应用声明中明确；标准许可证末尾的使用示例不是本项目的版本选择声明。

`python3 scripts/gen_notices.py` 会将根 LICENSE 与项目声明头部写入随包第三方声明，然后保留各组件自己的许可。变更后应运行许可一致性测试并核对上游许可正文未被意外修改。
